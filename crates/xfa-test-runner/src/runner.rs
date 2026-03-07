use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::classifier::{classify_error, ErrorCategory};
use crate::config::Config;
use crate::db::{Database, RunSummary, TestResultRow};
use crate::tests::{PdfTest, TestStatus};

/// Maximum number of spawned test threads allowed in-flight at once.
/// Prevents unbounded thread accumulation when tests repeatedly time out.
const MAX_IN_FLIGHT_THREADS: usize = 256;

pub struct Runner {
    config: Config,
    tests: Vec<Arc<dyn PdfTest>>,
    db: Arc<Database>,
    in_flight: Arc<AtomicUsize>,
}

impl Runner {
    pub fn new(config: Config, tests: Vec<Box<dyn PdfTest>>, db: Database) -> Self {
        Self {
            config,
            tests: tests.into_iter().map(Arc::from).collect(),
            db: Arc::new(db),
            in_flight: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn run_corpus(&self) -> RunSummary {
        let pdfs = self.enumerate_pdfs();
        let total = pdfs.len();
        let test_count = self.tests.len();

        eprintln!("Found {} PDF files in {:?}", total, self.config.corpus_dir);

        self.db
            .start_run(
                &self.config.run_id,
                &self.config.corpus_dir.to_string_lossy(),
                total,
            )
            .expect("Failed to start run in database");

        let progress = ProgressBar::new(total as u64);
        progress.set_style(
            ProgressStyle::default_bar()
                .template("[{pos}/{len}] {percent}% | {bar:40} | Pass: {msg} | ETA: {eta}")
                .unwrap()
                .progress_chars("=> "),
        );

        let pass_count = Arc::new(AtomicUsize::new(0));
        let fail_count = Arc::new(AtomicUsize::new(0));
        let crash_count = Arc::new(AtomicUsize::new(0));
        let timeout_count = Arc::new(AtomicUsize::new(0));

        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(self.config.workers)
            .stack_size(64 * 1024 * 1024) // 8 MB — deep PDF object graphs can overflow default
            .build()
            .expect("Failed to create thread pool");

        pool.install(|| {
            pdfs.par_iter().for_each(|pdf_path| {
                let path_str = pdf_path.to_string_lossy().to_string();

                // Resume: skip only if ALL tests are already completed for this PDF
                if self.config.resume {
                    let completed = self
                        .db
                        .tests_completed_for_pdf(&self.config.run_id, &path_str);
                    if completed >= test_count {
                        progress.inc(1);
                        return;
                    }
                }

                let pdf_data = match std::fs::read(pdf_path) {
                    Ok(data) => data,
                    Err(e) => {
                        eprintln!("Failed to read {}: {}", path_str, e);
                        // Record read failure for every test so it shows up in results
                        let err_msg = format!("IO error: {e}");
                        for test in &self.tests {
                            let category = classify_error(test.name(), &err_msg);
                            let row = TestResultRow::from_test_result(
                                &self.config.run_id,
                                &path_str,
                                "",
                                0,
                                test.name(),
                                &TestStatus::Fail,
                                Some(&err_msg),
                                Some(&category),
                                0,
                            );
                            let _ = self.db.insert_result(&row);
                        }
                        fail_count.fetch_add(self.tests.len(), Ordering::Relaxed);
                        progress.inc(1);
                        return;
                    }
                };

                let pdf_hash = hex_sha256(&pdf_data);
                let pdf_size = pdf_data.len() as i64;

                // Pre-check: detect encrypted PDFs and skip all tests.
                if let Some(skip_reason) = detect_encrypted(&pdf_data) {
                    let cat = ErrorCategory::Encrypted;
                    for test in &self.tests {
                        let row = TestResultRow::from_test_result(
                            &self.config.run_id,
                            &path_str,
                            &pdf_hash,
                            pdf_size,
                            test.name(),
                            &TestStatus::Skip,
                            Some(&skip_reason),
                            Some(&cat),
                            0,
                        );
                        let _ = self.db.insert_result(&row);
                    }
                    progress.inc(1);
                    return;
                }

                for test in &self.tests {
                    if let Some(filter) = &self.config.test_filter {
                        if !filter.iter().any(|f| f == test.name()) {
                            continue;
                        }
                    }

                    let result = self.run_single_test(Arc::clone(test), &pdf_data, pdf_path);

                    match result.status {
                        TestStatus::Pass => {
                            pass_count.fetch_add(1, Ordering::Relaxed);
                        }
                        TestStatus::Fail => {
                            fail_count.fetch_add(1, Ordering::Relaxed);
                        }
                        TestStatus::Crash => {
                            crash_count.fetch_add(1, Ordering::Relaxed);
                        }
                        TestStatus::Timeout => {
                            timeout_count.fetch_add(1, Ordering::Relaxed);
                        }
                        TestStatus::Skip => {}
                    }

                    let error_category = result
                        .error_message
                        .as_deref()
                        .map(|msg| classify_error(test.name(), msg));

                    let mut row = TestResultRow::from_test_result(
                        &self.config.run_id,
                        &path_str,
                        &pdf_hash,
                        pdf_size,
                        test.name(),
                        &result.status,
                        result.error_message.as_deref(),
                        error_category.as_ref(),
                        result.duration_ms,
                    );
                    row.oracle_score = result.oracle_score;
                    if !result.metadata.is_empty() {
                        row.metadata_json = serde_json::to_string(&result.metadata).ok();
                    }

                    if let Err(e) = self.db.insert_result(&row) {
                        eprintln!("DB write error: {}", e);
                    }
                }

                let p = pass_count.load(Ordering::Relaxed);
                let f = fail_count.load(Ordering::Relaxed);
                let c = crash_count.load(Ordering::Relaxed);
                let t = timeout_count.load(Ordering::Relaxed);
                progress.set_message(format!(
                    "{} | Fail: {} | Crash: {} | Timeout: {}",
                    p, f, c, t
                ));
                progress.inc(1);
            });
        });

        progress.finish_with_message("done");

        self.db
            .finish_run(&self.config.run_id)
            .expect("Failed to finish run in database");

        self.db.summary(&self.config.run_id)
    }

    fn run_single_test(
        &self,
        test: Arc<dyn PdfTest>,
        pdf_data: &[u8],
        path: &Path,
    ) -> crate::tests::TestResult {
        let timeout = self.config.timeout;
        let test_name = test.name().to_string();

        // Backpressure: wait if too many timed-out threads are still running.
        // Prevents unbounded thread accumulation on corpora with many hangs.
        while self.in_flight.load(Ordering::Relaxed) >= MAX_IN_FLIGHT_THREADS {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        self.in_flight.fetch_add(1, Ordering::Relaxed);

        let data = pdf_data.to_vec();
        let path_buf = path.to_path_buf();
        let in_flight = Arc::clone(&self.in_flight);

        // Spawn in a separate thread with preemptive timeout via recv_timeout.
        let (tx, rx) = std::sync::mpsc::channel();

        std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || {
                // Drop guard: decrement in_flight even on double-panic.
                let _guard = InFlightGuard(in_flight);

                let start = Instant::now();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    test.run(&data, &path_buf)
                }));
                let elapsed = start.elapsed();

                let test_result = match result {
                    Ok(r) => r,
                    Err(panic_info) => {
                        let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                            s.to_string()
                        } else if let Some(s) = panic_info.downcast_ref::<String>() {
                            s.clone()
                        } else {
                            "Unknown panic".to_string()
                        };
                        crate::tests::TestResult {
                            status: TestStatus::Crash,
                            error_message: Some(msg),
                            duration_ms: elapsed.as_millis() as u64,
                            oracle_score: None,
                            metadata: Default::default(),
                        }
                    }
                };
                let _ = tx.send(test_result);
            })
            .expect("failed to spawn test thread");

        match rx.recv_timeout(timeout) {
            Ok(result) => result,
            Err(_) => crate::tests::TestResult {
                status: TestStatus::Timeout,
                error_message: Some(format!(
                    "Test '{test_name}' exceeded timeout of {timeout:?}"
                )),
                duration_ms: timeout.as_millis() as u64,
                oracle_score: None,
                metadata: Default::default(),
            },
        }
    }

    fn enumerate_pdfs(&self) -> Vec<PathBuf> {
        let skip_set = self.load_skip_list();
        WalkDir::new(&self.config.corpus_dir)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
            })
            .map(|e| e.into_path())
            .filter(|p| !skip_set.contains(&p.to_string_lossy().to_string()))
            .collect()
    }

    fn load_skip_list(&self) -> std::collections::HashSet<String> {
        let skip_path = self.config.corpus_dir.join("skip.txt");
        let mut set = std::collections::HashSet::new();
        if let Ok(contents) = std::fs::read_to_string(&skip_path) {
            for line in contents.lines() {
                let line = line.trim();
                if !line.is_empty() && !line.starts_with('#') {
                    set.insert(line.to_string());
                }
            }
            if !set.is_empty() {
                eprintln!("Skipping {} PDFs from {}", set.len(), skip_path.display());
            }
        }
        set
    }
}

/// RAII guard that decrements the in-flight counter on drop,
/// ensuring cleanup even if the thread panics through catch_unwind.
struct InFlightGuard(Arc<AtomicUsize>);

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::Relaxed);
    }
}

/// Detect encrypted PDFs that cannot be processed without a password.
/// Returns `Some(reason)` if the PDF is encrypted, `None` otherwise.
fn detect_encrypted(pdf_data: &[u8]) -> Option<String> {
    match pdf_syntax::Pdf::new(pdf_data.to_vec()) {
        Err(pdf_syntax::LoadPdfError::Decryption(e)) => Some(format!("Decryption({e:?})")),
        _ => None,
    }
}

fn hex_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}
