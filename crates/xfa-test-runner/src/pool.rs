//! Process-pool orchestrator for corpus runs.
//!
//! Spawns N child `xfa-test-runner single-pdf <path>` processes in parallel.
//! Each child is given a hard wall-clock timeout of 120 s; on Linux its virtual
//! address space is capped at 4 GB via `RLIMIT_AS` so a runaway PDF can't OOM the
//! whole machine — it just causes the child to crash instead.
//!
//! The orchestrator collects child JSON output and writes `TestResultRow`s into
//! the same SQLite schema as the thread-based `Runner`.  Crashed / OOM / timed-out
//! children are recorded as `skip` rows with an explanatory error message.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};

use indicatif::{ProgressBar, ProgressStyle};
use sha2::{Digest, Sha256};

use crate::classifier::classify_error;
use crate::db::{Database, TestResultRow};
use crate::runner::SinglePdfOutput;

/// Hard wall-clock kill timeout per child process (seconds).
const CHILD_KILL_TIMEOUT_SECS: u64 = 120;

/// Virtual address space limit per child process (4 GB).
/// On Linux this is applied via RLIMIT_AS before exec so a runaway PDF can't
/// cause a system-wide OOM — the child's allocator fails instead.
#[cfg(target_os = "linux")]
const CHILD_RLIMIT_AS_BYTES: u64 = 4 * 1024 * 1024 * 1024;

// ── PDF collection ──────────────────────────────────────────────────────────

/// Collect all `.pdf` files from `corpus_dir` (recursive).
pub fn collect_pdfs_from_dir(corpus_dir: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(corpus_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_type().is_file()
                && e.path()
                    .extension()
                    .map(|x| x.eq_ignore_ascii_case("pdf"))
                    .unwrap_or(false)
        })
        .map(|e| e.path().to_path_buf())
        .collect()
}

/// Read a newline-separated list of PDF paths from `list_file`.
pub fn collect_pdfs_from_list(list_file: &Path) -> std::io::Result<Vec<PathBuf>> {
    let content = std::fs::read_to_string(list_file)?;
    Ok(content
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(PathBuf::from)
        .collect())
}

// ── Orchestrator RSS ─────────────────────────────────────────────────────────

/// Return the orchestrator's current RSS in MB (Linux only; 0 elsewhere).
fn orchestrator_rss_mb() -> u64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
            for line in status.lines() {
                if let Some(rest) = line.strip_prefix("VmRSS:") {
                    let kb_str = rest.trim().trim_end_matches("kB").trim();
                    if let Ok(kb) = kb_str.parse::<u64>() {
                        return kb / 1024;
                    }
                }
            }
        }
    }
    0
}

// ── Public entry point ───────────────────────────────────────────────────────

/// Run the process pool.
///
/// * `exe`              — path to this binary (spawned as child)
/// * `pdfs`             — list of PDF files to process
/// * `db`               — opened SQLite database
/// * `run_id`           — run identifier written to every result row
/// * `workers`          — number of parallel child processes
/// * `test_names`       — names of active tests (used for skip rows on crash)
/// * `extra_child_args` — passthrough args: `--timeout`, `--tier`, `--tests`,
///   `--no-verapdf`, `--verapdf-path`
pub fn run_pool(
    exe: &Path,
    pdfs: Vec<PathBuf>,
    db: Arc<Database>,
    run_id: &str,
    workers: usize,
    test_names: Vec<String>,
    extra_child_args: Vec<String>,
) {
    let total = pdfs.len();
    eprintln!(
        "[pool] {total} PDFs | {workers} workers | timeout={}s per child",
        CHILD_KILL_TIMEOUT_SECS,
    );

    let progress = ProgressBar::new(total as u64);
    progress.set_style(
        ProgressStyle::default_bar()
            .template("[{pos}/{len}] {percent}% | {bar:40} | {msg} | ETA: {eta}")
            .unwrap()
            .progress_chars("=> "),
    );

    let fail_count = Arc::new(AtomicUsize::new(0));
    let timeout_count = Arc::new(AtomicUsize::new(0));

    // Shared work queue: all workers pull from this.
    let queue: Arc<Mutex<std::collections::VecDeque<PathBuf>>> =
        Arc::new(Mutex::new(pdfs.into_iter().collect()));

    let mut handles = Vec::with_capacity(workers);
    for _ in 0..workers {
        let queue = Arc::clone(&queue);
        let db = Arc::clone(&db);
        let exe = exe.to_path_buf();
        let run_id = run_id.to_string();
        let test_names = test_names.clone();
        let extra_child_args = extra_child_args.clone();
        let fail_count = Arc::clone(&fail_count);
        let timeout_count = Arc::clone(&timeout_count);
        let progress = progress.clone();

        handles.push(std::thread::spawn(move || loop {
            let pdf_path = {
                let mut q = queue.lock().unwrap();
                q.pop_front()
            };
            let pdf_path = match pdf_path {
                Some(p) => p,
                None => break,
            };

            process_one(
                &exe,
                &pdf_path,
                &db,
                &run_id,
                &test_names,
                &extra_child_args,
                &fail_count,
                &timeout_count,
            );

            let f = fail_count.load(Ordering::Relaxed);
            let t = timeout_count.load(Ordering::Relaxed);
            let rss = orchestrator_rss_mb();
            progress.set_message(format!("fail={f} timeout={t} rss={rss}MB"));
            progress.inc(1);
        }));
    }

    for h in handles {
        h.join().expect("pool worker thread panicked");
    }

    let f = fail_count.load(Ordering::Relaxed);
    let t = timeout_count.load(Ordering::Relaxed);
    let rss = orchestrator_rss_mb();
    progress.finish_with_message(format!("done fail={f} timeout={t} rss={rss}MB"));
    eprintln!("[pool] finished: {total} PDFs | fail={f} timeout={t} | orchestrator rss={rss}MB");
}

// ── Per-PDF processing ───────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn process_one(
    exe: &Path,
    pdf_path: &Path,
    db: &Arc<Database>,
    run_id: &str,
    test_names: &[String],
    extra_child_args: &[String],
    fail_count: &Arc<AtomicUsize>,
    timeout_count: &Arc<AtomicUsize>,
) {
    // Hash and size — read by the orchestrator so skip rows have correct metadata.
    let (pdf_hash, pdf_size) = match std::fs::read(pdf_path) {
        Ok(data) => {
            let hash = format!("{:x}", Sha256::digest(&data));
            let size = data.len() as i64;
            (hash, size)
        }
        Err(e) => {
            insert_skip_rows(
                db,
                run_id,
                pdf_path,
                0,
                "",
                &format!("orchestrator IO error: {e}"),
                test_names,
                fail_count,
            );
            return;
        }
    };

    // ── Spawn child ─────────────────────────────────────────────────────────
    let mut cmd = Command::new(exe);
    cmd.arg("single-pdf")
        .arg(pdf_path)
        .args(extra_child_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Linux: cap child virtual address space to 4 GB before exec.
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        // SAFETY: setrlimit is async-signal-safe and only modifies the child's
        // resource limits — no heap allocations, no locks held.
        unsafe {
            cmd.pre_exec(|| {
                let limit = libc::rlimit {
                    rlim_cur: CHILD_RLIMIT_AS_BYTES,
                    rlim_max: CHILD_RLIMIT_AS_BYTES,
                };
                // Ignore errors: the child still runs, just without the cap.
                let _ = libc::setrlimit(libc::RLIMIT_AS, &limit);
                Ok(())
            });
        }
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            insert_skip_rows(
                db,
                run_id,
                pdf_path,
                pdf_size,
                &pdf_hash,
                &format!("child spawn error: {e}"),
                test_names,
                fail_count,
            );
            return;
        }
    };

    // Drain stdout + stderr in background threads to prevent pipe-buffer deadlock.
    let stdout_handle = {
        let mut pipe = child.stdout.take().expect("stdout piped");
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            buf
        })
    };
    let stderr_handle = {
        let mut pipe = child.stderr.take().expect("stderr piped");
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = pipe.read_to_end(&mut buf);
            buf
        })
    };

    // ── Wait with hard kill timeout ──────────────────────────────────────────
    let deadline = Instant::now() + Duration::from_secs(CHILD_KILL_TIMEOUT_SECS);
    let exit_status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break Some(s),
            Ok(None) => {
                if Instant::now() >= deadline {
                    // Kill -9 on timeout.
                    let _ = child.kill();
                    let _ = child.wait(); // reap zombie
                    break None;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                eprintln!("[pool] try_wait error for {}: {e}", pdf_path.display());
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };

    let stdout_bytes = stdout_handle.join().unwrap_or_default();
    let stderr_bytes = stderr_handle.join().unwrap_or_default();

    // ── Timeout ──────────────────────────────────────────────────────────────
    let Some(exit_status) = exit_status else {
        timeout_count.fetch_add(1, Ordering::Relaxed);
        insert_skip_rows(
            db,
            run_id,
            pdf_path,
            pdf_size,
            &pdf_hash,
            &format!("process killed after {}s timeout", CHILD_KILL_TIMEOUT_SECS),
            test_names,
            fail_count,
        );
        return;
    };

    let exit_code = exit_status.code().unwrap_or(-1);

    // ── Parse JSON from stdout ───────────────────────────────────────────────
    // Exit code 0 = all pass, 1 = some test failures, 2 = pre-flight error.
    // Any other exit code = OOM / signal / panic.
    let output: Option<SinglePdfOutput> = if stdout_bytes.is_empty() {
        None
    } else {
        serde_json::from_slice(&stdout_bytes).ok()
    };

    let Some(output) = output else {
        // Crash, OOM, or pre-flight error.
        let stderr_snippet: String = String::from_utf8_lossy(&stderr_bytes)
            .chars()
            .take(200)
            .collect();
        let reason = if exit_code == 2 {
            format!("pre-flight error (exit=2): {stderr_snippet}")
        } else {
            format!("child crash (exit={exit_code}): {stderr_snippet}")
        };
        insert_skip_rows(
            db, run_id, pdf_path, pdf_size, &pdf_hash, &reason, test_names, fail_count,
        );
        return;
    };

    // ── Write results to SQLite ──────────────────────────────────────────────
    for result in &output.results {
        if matches!(result.status.as_str(), "fail" | "crash" | "timeout") {
            fail_count.fetch_add(1, Ordering::Relaxed);
        }
        let error_cat = result
            .error_message
            .as_deref()
            .map(|msg| classify_error(&result.test_name, msg).to_string());

        let row = TestResultRow {
            run_id: run_id.to_string(),
            pdf_path: pdf_path.to_string_lossy().to_string(),
            pdf_hash: pdf_hash.clone(),
            pdf_size,
            test_name: result.test_name.clone(),
            status: result.status.clone(),
            error_message: result.error_message.clone(),
            error_category: error_cat,
            duration_ms: result.duration_ms as i64,
            oracle_score: None,
            metadata_json: result.metadata_json.clone(),
        };
        if let Err(e) = db.insert_result(&row) {
            eprintln!("[pool] DB insert failed for {}: {e}", pdf_path.display());
        }
    }
}

// ── Skip-row helper ──────────────────────────────────────────────────────────

/// Insert a `skip` row for every test in `test_names` for this PDF.
/// Used when the child crashes, OOMs, or times out.
#[allow(clippy::too_many_arguments)]
fn insert_skip_rows(
    db: &Arc<Database>,
    run_id: &str,
    pdf_path: &Path,
    pdf_size: i64,
    pdf_hash: &str,
    reason: &str,
    test_names: &[String],
    _fail_count: &Arc<AtomicUsize>,
) {
    let pdf_path_str = pdf_path.to_string_lossy().to_string();
    for test_name in test_names {
        let row = TestResultRow {
            run_id: run_id.to_string(),
            pdf_path: pdf_path_str.clone(),
            pdf_hash: pdf_hash.to_string(),
            pdf_size,
            test_name: test_name.clone(),
            status: "skip".to_string(),
            error_message: Some(reason.to_string()),
            // Use a dedicated category so pool-skips are easily filtered.
            error_category: Some("pool_skip".to_string()),
            duration_ms: 0,
            oracle_score: None,
            metadata_json: None,
        };
        if let Err(e) = db.insert_result(&row) {
            eprintln!(
                "[pool] DB insert (skip) failed for {}: {e}",
                pdf_path.display()
            );
        }
    }
}
