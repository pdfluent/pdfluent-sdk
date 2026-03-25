// Replace glibc malloc with jemalloc to prevent heap fragmentation OOM (#461).
// lopdf creates thousands of small allocations per PDF; glibc retains freed
// pages in its sbrk free-list, growing anon-rss ~10 MB per PDF. jemalloc
// returns unused pages to the OS via madvise(MADV_FREE), keeping RSS bounded.
#[global_allocator]
static ALLOC: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

mod classifier;
#[allow(dead_code)]
mod clustering;
mod config;
#[allow(dead_code)]
mod dashboard;
mod db;
#[allow(dead_code)]
mod github_issues;
mod oracle_db;
mod oracles;
mod pool;
mod retest;
mod runner;
mod tests;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use std::sync::Arc;

use config::{Config, TestTier};
use db::Database;
use oracles::verapdf::VeraPdfOracle;
use runner::{run_single_pdf, Runner};

/// Corpus test runner for XFA-Native-Rust SDK
#[derive(Parser)]
#[command(
    name = "xfa-test-runner",
    about = "Run PDF corpus tests against the XFA SDK"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Execute corpus tests
    Run {
        /// Directory containing PDF files to test
        #[arg(short, long)]
        corpus: PathBuf,

        /// SQLite database path for results
        #[arg(short, long, default_value = "results.sqlite")]
        db: PathBuf,

        /// Number of parallel workers ("auto" for nproc-2)
        #[arg(short = 'j', long, default_value = "auto")]
        workers: String,

        /// Timeout per PDF in seconds
        #[arg(short, long, default_value_t = 30)]
        timeout: u64,

        /// Only run specific tests (comma-separated)
        #[arg(long)]
        tests: Option<String>,

        /// Resume from last incomplete run
        #[arg(long)]
        resume: bool,

        /// Rerun only PDFs that failed/crashed/timed-out in the previous run
        #[arg(long)]
        rerun_failures: bool,

        /// Rerun only PDFs where this test failed (e.g. "compliance")
        #[arg(long)]
        affected_by: Option<String>,

        /// Run ID (auto-generated if not provided)
        #[arg(long)]
        run_id: Option<String>,

        /// Code version for incremental testing (skip unchanged PDFs)
        #[arg(long)]
        code_version: Option<String>,

        /// Disable veraPDF oracle
        #[arg(long)]
        no_verapdf: bool,

        /// Path to veraPDF binary
        #[arg(long, default_value = "/usr/local/bin/verapdf")]
        verapdf_path: PathBuf,

        /// Test tier: fast, standard, full, oracle
        #[arg(long, default_value = "full")]
        tier: String,

        /// Maximum number of PDF files to process
        #[arg(long)]
        limit: Option<usize>,
    },

    /// Generate summary report from results
    Report {
        /// SQLite database path
        #[arg(short, long, default_value = "results.sqlite")]
        db: PathBuf,

        /// Run ID (latest if not provided)
        #[arg(long)]
        run_id: Option<String>,
    },

    /// Show error clusters
    Clusters {
        /// SQLite database path
        #[arg(short, long, default_value = "results.sqlite")]
        db: PathBuf,

        /// Run ID (latest if not provided)
        #[arg(long)]
        run_id: Option<String>,
    },

    /// Compare two runs
    Compare {
        /// SQLite database path
        #[arg(short, long, default_value = "results.sqlite")]
        db: PathBuf,

        /// First run ID
        #[arg(long)]
        run_a: String,

        /// Second run ID
        #[arg(long)]
        run_b: String,
    },

    /// Download example PDFs from a cluster for regression test fixtures
    DownloadExamples {
        /// SQLite database path
        #[arg(short, long, default_value = "results.sqlite")]
        db: PathBuf,

        /// Run ID (latest if not provided)
        #[arg(long)]
        run_id: Option<String>,

        /// Test name (e.g. "parse", "text_extract")
        #[arg(long)]
        test: String,

        /// Error category (e.g. "invalid_xref", "missing_font")
        #[arg(long)]
        category: String,

        /// Output directory for fixtures
        #[arg(short, long, default_value = "tests/regression/fixtures")]
        output: PathBuf,

        /// Maximum number of examples to download
        #[arg(long, default_value_t = 5)]
        limit: usize,

        /// Maximum file size in KB per fixture
        #[arg(long, default_value_t = 100)]
        max_size_kb: usize,
    },

    /// Export error cluster as GitHub Issue markdown
    Export {
        /// SQLite database path
        #[arg(short, long, default_value = "results.sqlite")]
        db: PathBuf,

        /// Run ID (latest if not provided)
        #[arg(long)]
        run_id: Option<String>,

        /// Error category to export
        #[arg(long)]
        category: String,
    },

    /// Generate GitHub Issue markdown for clusters
    Issues {
        /// SQLite database path
        #[arg(short, long, default_value = "results.sqlite")]
        db: PathBuf,

        /// Run ID (latest if not provided)
        #[arg(long)]
        run_id: Option<String>,

        /// Only show top N clusters
        #[arg(long, default_value_t = 20)]
        top: usize,
    },

    /// Generate HTML dashboard
    Dashboard {
        /// SQLite database path
        #[arg(short, long, default_value = "results.sqlite")]
        db: PathBuf,

        /// Run ID (latest if not provided)
        #[arg(long)]
        run_id: Option<String>,

        /// Output directory for HTML files
        #[arg(short, long, default_value = "dashboard")]
        output: PathBuf,
    },

    /// Clean up stale/abandoned runs
    CleanStale {
        /// SQLite database path
        #[arg(short, long, default_value = "results.sqlite")]
        db: PathBuf,

        /// Keep this run_id active (don't mark as stale)
        #[arg(long)]
        keep: Option<String>,
    },

    /// Merge results from another database
    MergeDb {
        /// Target SQLite database path
        #[arg(short, long, default_value = "results.sqlite")]
        db: PathBuf,

        /// Source database to merge from
        #[arg(long)]
        source: PathBuf,
    },

    /// Show pass rate trend across runs
    Trend {
        /// SQLite database path
        #[arg(short, long, default_value = "results.sqlite")]
        db: PathBuf,
    },

    /// Process a single PDF and write results as JSON to stdout (for process-per-PDF orchestration)
    SinglePdf {
        /// Path to the PDF file to process
        #[arg(value_name = "PATH")]
        path: PathBuf,

        /// Timeout per test in seconds
        #[arg(short, long, default_value_t = 30)]
        timeout: u64,

        /// Only run specific tests (comma-separated)
        #[arg(long)]
        tests: Option<String>,

        /// Test tier: fast, standard, full, oracle
        #[arg(long, default_value = "full")]
        tier: String,

        /// Disable veraPDF oracle
        #[arg(long)]
        no_verapdf: bool,

        /// Path to veraPDF binary
        #[arg(long, default_value = "/usr/local/bin/verapdf")]
        verapdf_path: PathBuf,

        /// Path to pre-generated oracle database (uses cached results instead of live veraPDF)
        #[arg(long)]
        oracle_db: Option<PathBuf>,
    },

    /// Run corpus using a pool of N child processes (one process per PDF).
    ///
    /// Each child gets RLIMIT_AS=4 GB (Linux) and a hard 120 s kill timeout.
    /// Crashed / OOM / timed-out children are recorded as `skip` rows.
    Pool {
        /// Directory containing PDF files (recursive, mutually exclusive with --pdf-list)
        #[arg(long, group = "input")]
        corpus: Option<PathBuf>,

        /// File with one PDF path per line (mutually exclusive with --corpus)
        #[arg(long, group = "input")]
        pdf_list: Option<PathBuf>,

        /// SQLite database path for results
        #[arg(short, long, default_value = "results.sqlite")]
        db: PathBuf,

        /// Number of parallel child processes
        #[arg(short = 'j', long, default_value_t = 6)]
        workers: usize,

        /// Per-test timeout passed to each child (seconds)
        #[arg(short, long, default_value_t = 30)]
        timeout: u64,

        /// Only run specific tests (comma-separated)
        #[arg(long)]
        tests: Option<String>,

        /// Test tier: fast, standard, full, oracle
        #[arg(long, default_value = "full")]
        tier: String,

        /// Disable veraPDF oracle
        #[arg(long)]
        no_verapdf: bool,

        /// Path to veraPDF binary
        #[arg(long, default_value = "/usr/local/bin/verapdf")]
        verapdf_path: PathBuf,

        /// Run ID (auto-generated if not provided)
        #[arg(long)]
        run_id: Option<String>,

        /// Path to pre-generated oracle database (passed to each child single-pdf process)
        #[arg(long)]
        oracle_db: Option<PathBuf>,
    },

    /// Render one page of a PDF to a PNG file (for render comparison scripts)
    RenderPage {
        /// Input PDF path
        #[arg(value_name = "PDF")]
        pdf: PathBuf,

        /// Page number to render (1-based)
        #[arg(long, default_value_t = 1)]
        page: u32,

        /// Output PNG path
        #[arg(short, long)]
        output: PathBuf,

        /// Resolution in DPI
        #[arg(long, default_value_t = 150.0_f64)]
        dpi: f64,
    },

    /// Pre-generate oracle results (veraPDF) for a corpus.
    ///
    /// Populates a standalone oracle database with veraPDF results for each PDF.
    /// Subsequent test runs can read from this DB instead of re-running veraPDF.
    OracleGenerate {
        /// Directory containing PDF files
        #[arg(long)]
        corpus: PathBuf,

        /// Path to the oracle SQLite database
        #[arg(long)]
        oracle_db: PathBuf,

        /// Path to veraPDF binary
        #[arg(long, default_value = "/usr/local/bin/verapdf")]
        verapdf_path: PathBuf,

        /// Number of parallel workers
        #[arg(short = 'j', long, default_value_t = 6)]
        workers: usize,

        /// Skip PDFs already in the oracle DB
        #[arg(long)]
        skip_existing: bool,
    },

    /// Retest compliance failures from a previous run with the current binary.
    ///
    /// Reads failing PDF paths from the source DB and re-runs the compliance
    /// test, reporting what changed (fixed vs still failing, FN/FP breakdown).
    RetestFailures {
        /// Source database with previous run results
        #[arg(long)]
        source_db: PathBuf,

        /// Run ID in the source database
        #[arg(long)]
        source_run: String,

        /// Path to veraPDF binary
        #[arg(long, default_value = "/usr/local/bin/verapdf")]
        verapdf_path: PathBuf,

        /// Path to pre-generated oracle database
        #[arg(long)]
        oracle_db: Option<PathBuf>,

        /// Only run specific tests (comma-separated, default: compliance)
        #[arg(long)]
        tests: Option<String>,

        /// Timeout per PDF in seconds
        #[arg(short, long, default_value_t = 120)]
        timeout: u64,

        /// Number of parallel workers
        #[arg(short = 'j', long, default_value_t = 6)]
        workers: usize,
    },

    /// Internal: convert a single PDF to PDF/A and write to output path.
    ///
    /// Used by oracle-generate to isolate crashes — each PDF runs in its own
    /// subprocess so a SIGSEGV or abort() does not kill the parent.
    #[command(hide = true)]
    OracleConvertOne {
        /// Input PDF path
        #[arg(long)]
        input: PathBuf,

        /// Output path for the converted PDF bytes
        #[arg(long)]
        output: PathBuf,
    },

    /// Check for regression between two runs (exit code 1 = regression)
    CheckRegression {
        /// SQLite database path
        #[arg(short, long, default_value = "results.sqlite")]
        db: PathBuf,

        /// First run ID (baseline)
        #[arg(long)]
        run_a: String,

        /// Second run ID (current)
        #[arg(long)]
        run_b: String,
    },
}

fn truncate_utf8(s: &str, max_chars: usize) -> String {
    let truncated: String = s.chars().take(max_chars).collect();
    if truncated.len() < s.len() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

/// Try to acquire a lockfile. Returns Ok(lockfile_path) or Err with message.
fn acquire_lock() -> Result<PathBuf, String> {
    let lock_path = std::env::temp_dir().join("xfa-runner.lock");
    if lock_path.exists() {
        if let Ok(pid_str) = std::fs::read_to_string(&lock_path) {
            let pid = pid_str.trim();
            // Check if process is still alive
            if !pid.is_empty() {
                if let Ok(status) = std::process::Command::new("kill")
                    .args(["-0", pid])
                    .status()
                {
                    if status.success() {
                        return Err(format!(
                            "Another runner is active (PID {pid}). Use --force or kill it first."
                        ));
                    }
                }
            }
        }
        // Stale lock — remove it
        let _ = std::fs::remove_file(&lock_path);
    }
    std::fs::write(&lock_path, std::process::id().to_string())
        .map_err(|e| format!("Failed to create lockfile: {e}"))?;
    Ok(lock_path)
}

fn release_lock(lock_path: &PathBuf) {
    let _ = std::fs::remove_file(lock_path);
}

fn main() {
    // Fix #461: lopdf uses rayon internally (reader.rs par_iter, object_stream.rs par_chunks)
    // via the *global* rayon thread pool, which defaults to 8 MB stacks. Our custom per-run
    // pool has 64 MB stacks but those threads are spawned outside that pool's scope, so lopdf's
    // internal par_iter falls back to the global pool. Deep recursion on pathological PDFs
    // overflows the 8 MB stack at ~2800 PDFs. Configure the global pool first.
    rayon::ThreadPoolBuilder::new()
        .stack_size(64 * 1024 * 1024) // 64 MB — matches our custom pool and per-test spawns
        .build_global()
        .expect("Failed to configure global rayon thread pool");

    let cli = Cli::parse();

    match cli.command {
        Command::Run {
            corpus,
            db,
            workers,
            timeout,
            tests: test_filter,
            resume,
            rerun_failures,
            affected_by,
            run_id,
            code_version,
            no_verapdf,
            verapdf_path,
            tier,
            limit,
        } => {
            // Lockfile: prevent concurrent runners
            let lock_path = match acquire_lock() {
                Ok(p) => p,
                Err(e) => {
                    eprintln!("ERROR: {e}");
                    std::process::exit(1);
                }
            };

            let resolved_workers = config::resolve_workers(&workers);
            let tier: TestTier = tier.parse().unwrap_or_else(|e| {
                eprintln!("WARNING: {e}, defaulting to 'full'");
                TestTier::Full
            });

            let database = Database::open(&db).expect("Failed to open database");
            let config = Config::new(
                corpus,
                db,
                resolved_workers,
                timeout,
                test_filter,
                resume,
                rerun_failures,
                affected_by,
                run_id,
                Some(&database),
                tier,
                limit,
                code_version,
            );

            // Set up veraPDF oracle if available and not disabled
            let verapdf_oracle = if no_verapdf {
                None
            } else {
                let oracle = VeraPdfOracle::new(verapdf_path);
                if oracle.is_available() {
                    eprintln!("veraPDF oracle: enabled");
                    Some(Arc::new(oracle))
                } else {
                    eprintln!(
                        "veraPDF oracle: not available (use --verapdf-path or install veraPDF)"
                    );
                    None
                }
            };

            // Enable cache if oracle is present — re-open db as Arc for sharing
            let verapdf_oracle = verapdf_oracle.map(|o| {
                let db_arc =
                    Arc::new(Database::open(&config.db_path).expect("Failed to open cache db"));
                Arc::new(
                    Arc::try_unwrap(o)
                        .expect("single reference")
                        .with_cache(db_arc),
                )
            });

            let test_config = tests::TestConfig {
                verapdf_oracle,
                #[cfg(feature = "pdfium-oracle")]
                diff_dir: std::env::var("XFA_DIFF_DIR").ok().map(PathBuf::from),
            };
            let mut available_tests = tests::all_tests(test_config);

            // Apply tier filter
            available_tests.retain(|t| config.tier.includes(t.name()));

            // Apply explicit test filter on top
            if let Some(filter) = &config.test_filter {
                available_tests.retain(|t| filter.iter().any(|f| f == t.name()));
            }

            eprintln!(
                "Starting run '{}' with {} workers, {} tests (tier: {})",
                config.run_id,
                config.workers,
                available_tests.len(),
                config.tier,
            );

            let runner = Runner::new(config, available_tests, database);
            let summary = runner.run_corpus();
            eprintln!("\n{summary}");

            release_lock(&lock_path);
        }

        Command::Report { db, run_id } => {
            let database = Database::open(&db).expect("Failed to open database");
            let run_id = run_id
                .or_else(|| database.latest_run_id())
                .expect("No runs found");
            let summary = database.summary(&run_id);
            println!("Run: {run_id}");
            println!("{summary}");
        }

        Command::Clusters { db, run_id } => {
            let database = Database::open(&db).expect("Failed to open database");
            let run_id = run_id
                .or_else(|| database.latest_run_id())
                .expect("No runs found");
            let clusters = database.clusters(&run_id);

            if clusters.is_empty() {
                println!("No error clusters found for run '{run_id}'");
                return;
            }

            println!("Error clusters for run '{run_id}':\n");
            println!("{:<30} {:<25} {:>8}  Pattern", "Test", "Category", "Count");
            println!("{}", "-".repeat(90));
            for c in &clusters {
                let pattern = truncate_utf8(&c.error_pattern, 40);
                println!(
                    "{:<30} {:<25} {:>8}  {}",
                    c.test_name, c.error_category, c.pdf_count, pattern
                );
            }
        }

        Command::Compare { db, run_a, run_b } => {
            let database = Database::open(&db).expect("Failed to open database");
            let result = database.compare_runs_detailed(&run_a, &run_b);
            println!("{result}");

            if result.verdict == db::Verdict::Regression {
                std::process::exit(1);
            }
        }

        Command::DownloadExamples {
            db,
            run_id,
            test,
            category,
            output,
            limit,
            max_size_kb,
        } => {
            let database = Database::open(&db).expect("Failed to open database");
            let run_id = run_id
                .or_else(|| database.latest_run_id())
                .expect("No runs found");

            let examples = database.cluster_examples(&run_id, &test, &category, limit);

            if examples.is_empty() {
                eprintln!(
                    "No examples found for test='{}' category='{}' in run '{}'",
                    test, category, run_id
                );
                return;
            }

            std::fs::create_dir_all(&output).expect("Failed to create output directory");

            let mut copied = 0usize;
            for ex in &examples {
                if ex.pdf_size > (max_size_kb as i64 * 1024) {
                    eprintln!(
                        "  skip {} ({}KB > {}KB limit)",
                        ex.pdf_path,
                        ex.pdf_size / 1024,
                        max_size_kb
                    );
                    continue;
                }

                let src = std::path::Path::new(&ex.pdf_path);
                if !src.exists() {
                    eprintln!("  skip {} (file not found)", ex.pdf_path);
                    continue;
                }

                let stem = src.file_stem().unwrap_or_default().to_string_lossy();
                let hash_prefix = &ex.pdf_hash[..8.min(ex.pdf_hash.len())];
                let dest_name = format!("{}_{}.pdf", stem, hash_prefix);
                let dest = output.join(&dest_name);

                std::fs::copy(src, &dest).expect("Failed to copy PDF");
                copied += 1;
                println!(
                    "  {} -> {} ({}KB)",
                    ex.pdf_path,
                    dest.display(),
                    ex.pdf_size / 1024
                );
            }

            println!("\nCopied {} fixture(s) to {}", copied, output.display());
        }

        Command::Export {
            db,
            run_id,
            category,
        } => {
            let database = Database::open(&db).expect("Failed to open database");
            let run_id = run_id
                .or_else(|| database.latest_run_id())
                .expect("No runs found");
            let clusters = database.clusters(&run_id);
            let matching: Vec<_> = clusters
                .iter()
                .filter(|c| c.error_category == category)
                .collect();

            if matching.is_empty() {
                println!("No clusters found for category '{category}'");
                return;
            }

            println!("## Error Cluster: {category}\n");
            println!("**Run:** {run_id}\n");
            for c in &matching {
                println!("### {} — {} PDFs\n", c.test_name, c.pdf_count);
                println!("**Pattern:** `{}`\n", c.error_pattern);
            }
            let total: i64 = matching.iter().map(|c| c.pdf_count).sum();
            println!("**Total affected PDFs:** {total}");
        }

        Command::Issues { db, run_id, top } => {
            let database = Database::open(&db).expect("Failed to open database");
            let run_id = run_id
                .or_else(|| database.latest_run_id())
                .expect("No runs found");

            let clusters = clustering::compute_clusters(&database, &run_id);

            if clusters.is_empty() {
                println!("No clusters found for run '{run_id}'");
                return;
            }

            println!("{}", clustering::format_cluster_table(&clusters));

            println!("\n--- GitHub Issue Markdown ---\n");
            for cluster in clusters.iter().take(top) {
                let title = github_issues::generate_issue_title(cluster);
                let body = github_issues::generate_issue_body(cluster, &run_id);
                println!("## {title}\n");
                println!("{body}\n");
                println!("{}\n", "-".repeat(80));
            }
        }

        Command::Dashboard { db, run_id, output } => {
            let database = Database::open(&db).expect("Failed to open database");
            let run_id = run_id
                .or_else(|| database.latest_run_id())
                .expect("No runs found");

            let clusters = clustering::compute_clusters(&database, &run_id);
            let data = dashboard::collect_dashboard_data(&database, &run_id, clusters);
            dashboard::generate_dashboard(&data, &output).expect("Failed to write dashboard");
            println!(
                "Dashboard generated in {} for run '{}'",
                output.display(),
                run_id
            );
        }

        Command::CleanStale { db, keep } => {
            let database = Database::open(&db).expect("Failed to open database");
            let cleaned = database.clean_stale_runs(keep.as_deref());
            println!("Marked {cleaned} stale run(s) as abandoned");
        }

        Command::MergeDb { db, source } => {
            let database = Database::open(&db).expect("Failed to open database");
            match database.merge_from(&source) {
                Ok((runs, results)) => {
                    println!(
                        "Merged {} run(s) and {} result(s) from {}",
                        runs,
                        results,
                        source.display()
                    );
                }
                Err(e) => {
                    eprintln!("Merge failed: {e}");
                    std::process::exit(1);
                }
            }
        }

        Command::Trend { db } => {
            let database = Database::open(&db).expect("Failed to open database");
            let trend = database.run_trend();

            if trend.is_empty() {
                println!("No completed runs found");
                return;
            }

            println!(
                "{:<35} {:>10} {:>10} {:>12}",
                "Run ID", "Pass Rate", "Total", "Avg Oracle"
            );
            println!("{}", "-".repeat(70));
            for entry in &trend {
                let oracle = entry
                    .avg_oracle_score
                    .map(|s| format!("{s:.3}"))
                    .unwrap_or_else(|| "n/a".to_string());
                println!(
                    "{:<35} {:>9.1}% {:>10} {:>12}",
                    entry.run_id, entry.pass_rate, entry.total, oracle
                );
            }
        }

        Command::SinglePdf {
            path,
            timeout,
            tests: test_filter,
            tier,
            no_verapdf,
            verapdf_path,
            oracle_db: oracle_db_path,
        } => {
            // Set up veraPDF oracle.
            // If --oracle-db is provided, wrap oracle with the pre-generated DB.
            let verapdf_oracle = if no_verapdf {
                None
            } else {
                let mut oracle = VeraPdfOracle::new(verapdf_path);
                // Attach oracle DB for cache lookups
                if let Some(ref odb_path) = oracle_db_path {
                    if let Ok(odb) = oracle_db::OracleDb::open(odb_path) {
                        oracle = oracle.with_oracle_db(Arc::new(std::sync::Mutex::new(odb)));
                    }
                }
                if oracle.is_available() || oracle_db_path.is_some() {
                    Some(std::sync::Arc::new(oracle))
                } else {
                    None
                }
            };

            let test_config = tests::TestConfig {
                verapdf_oracle,
                #[cfg(feature = "pdfium-oracle")]
                diff_dir: std::env::var("XFA_DIFF_DIR").ok().map(PathBuf::from),
            };
            let mut available_tests = tests::all_tests(test_config);

            // Apply tier filter.
            let tier: TestTier = tier.parse().unwrap_or(TestTier::Full);
            available_tests.retain(|t| tier.includes(t.name()));

            // Apply explicit test filter on top.
            if let Some(filter) = &test_filter {
                let names: Vec<&str> = filter.split(',').map(str::trim).collect();
                available_tests.retain(|t| names.iter().any(|f| *f == t.name()));
            }

            match run_single_pdf(available_tests, &path, timeout) {
                Ok(output) => {
                    // All tests ran — check if any failed.
                    let json = serde_json::to_string(&output).expect("JSON serialization failed");
                    println!("{json}");
                    let any_failed = output
                        .results
                        .iter()
                        .any(|r| matches!(r.status.as_str(), "fail" | "crash" | "timeout"));
                    std::process::exit(if any_failed { 1 } else { 0 });
                }
                Err(e) => {
                    // Pre-flight error (I/O, not a PDF).
                    let output = serde_json::json!({
                        "pdf_path": path.to_string_lossy(),
                        "error": e,
                        "results": []
                    });
                    println!("{output}");
                    std::process::exit(2);
                }
            }
        }

        Command::Pool {
            corpus,
            pdf_list,
            db,
            workers,
            timeout,
            tests: test_filter,
            tier,
            no_verapdf,
            verapdf_path,
            run_id,
            oracle_db: oracle_db_path,
        } => {
            // Collect PDF list.
            let pdfs = match (corpus, pdf_list) {
                (Some(dir), None) => pool::collect_pdfs_from_dir(&dir),
                (None, Some(list)) => {
                    pool::collect_pdfs_from_list(&list).expect("failed to read --pdf-list file")
                }
                _ => {
                    eprintln!("error: provide exactly one of --corpus or --pdf-list");
                    std::process::exit(1);
                }
            };
            if pdfs.is_empty() {
                eprintln!("No PDF files found.");
                std::process::exit(0);
            }

            let run_id = run_id
                .unwrap_or_else(|| format!("pool-{}", chrono::Utc::now().format("%Y%m%d-%H%M%S")));

            // Build the test name list (same logic as SinglePdf / Run).
            // No veraPDF oracle in the orchestrator — the child handles that.
            let test_config = tests::TestConfig {
                verapdf_oracle: None,
                #[cfg(feature = "pdfium-oracle")]
                diff_dir: std::env::var("XFA_DIFF_DIR").ok().map(PathBuf::from),
            };
            let mut available_tests = tests::all_tests(test_config);
            let tier_parsed: TestTier = tier.parse().unwrap_or(TestTier::Full);
            available_tests.retain(|t| tier_parsed.includes(t.name()));
            if let Some(filter) = &test_filter {
                let names: Vec<&str> = filter.split(',').map(str::trim).collect();
                available_tests.retain(|t| names.iter().any(|f| *f == t.name()));
            }
            let test_names: Vec<String> = available_tests
                .iter()
                .map(|t| t.name().to_string())
                .collect();

            let database = Arc::new(Database::open(&db).expect("Failed to open database"));
            database
                .start_run(&run_id, "process-pool", pdfs.len())
                .expect("Failed to start run in database");

            // Build extra args to pass verbatim to every child.
            let mut extra: Vec<String> = vec![
                "--timeout".to_string(),
                timeout.to_string(),
                "--tier".to_string(),
                tier,
            ];
            if let Some(filter) = test_filter {
                extra.push("--tests".to_string());
                extra.push(filter);
            }
            if no_verapdf {
                extra.push("--no-verapdf".to_string());
            } else {
                extra.push("--verapdf-path".to_string());
                extra.push(verapdf_path.to_string_lossy().to_string());
            }
            if let Some(ref odb_path) = oracle_db_path {
                extra.push("--oracle-db".to_string());
                extra.push(odb_path.to_string_lossy().to_string());
            }

            // Path to this binary (used to spawn children).
            let exe = std::env::current_exe().expect("cannot determine own executable path");

            pool::run_pool(
                &exe,
                pdfs,
                database.clone(),
                &run_id,
                workers,
                test_names,
                extra,
            );
            database.finish_run(&run_id).expect("Failed to finish run");

            // Print summary.
            let summary = database.summary(&run_id);
            eprintln!("Run: {run_id}");
            eprintln!("{summary}");
        }

        Command::RenderPage {
            pdf,
            page,
            output,
            dpi,
        } => {
            let data = match std::fs::read(&pdf) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("render-page: cannot read {}: {e}", pdf.display());
                    std::process::exit(1);
                }
            };
            let doc = match pdf_engine::PdfDocument::open(data) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("render-page: cannot parse {}: {e:?}", pdf.display());
                    std::process::exit(1);
                }
            };
            let page_idx = page.saturating_sub(1) as usize; // convert 1-based to 0-based
            if page_idx >= doc.page_count() {
                eprintln!(
                    "render-page: page {page} out of range (document has {} pages)",
                    doc.page_count()
                );
                std::process::exit(1);
            }
            let opts = pdf_engine::RenderOptions {
                dpi,
                ..Default::default()
            };
            let rendered = match doc.render_page(page_idx, &opts) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("render-page: render failed for page {page}: {e:?}");
                    std::process::exit(1);
                }
            };
            let img = image::RgbaImage::from_raw(rendered.width, rendered.height, rendered.pixels)
                .expect("invalid pixel dimensions");
            if let Some(parent) = output.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).ok();
                }
            }
            if let Err(e) = img.save(&output) {
                eprintln!("render-page: cannot save {}: {e}", output.display());
                std::process::exit(1);
            }
        }

        Command::OracleGenerate {
            corpus,
            oracle_db,
            verapdf_path,
            workers,
            skip_existing,
        } => {
            use oracle_db::OracleDb;
            use sha2::{Digest, Sha256};
            use std::sync::atomic::{AtomicUsize, Ordering};

            let odb = OracleDb::open(&oracle_db).expect("Failed to open oracle DB");

            // Detect veraPDF version
            let ver_output = std::process::Command::new(&verapdf_path)
                .arg("--version")
                .output()
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_else(|_| "unknown".to_string());
            let verapdf_version = ver_output
                .lines()
                .find(|l| l.contains("veraPDF") || l.chars().any(|c| c.is_ascii_digit()))
                .unwrap_or(&ver_output)
                .trim()
                .to_string();
            eprintln!("veraPDF version: {verapdf_version}");

            // Collect PDFs
            let mut pdfs: Vec<PathBuf> = Vec::new();
            for entry in walkdir::WalkDir::new(&corpus)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                if entry.path().extension().is_some_and(|e| e == "pdf") {
                    pdfs.push(entry.into_path());
                }
            }
            let total = pdfs.len();
            eprintln!(
                "[oracle-generate] {total} PDFs, {workers} workers, skip_existing={skip_existing}"
            );
            eprintln!(
                "Existing cache entries: {}",
                odb.count_for("verapdf-parsed", "any")
            );

            // Shared state
            let done = Arc::new(AtomicUsize::new(0));
            let skipped = Arc::new(AtomicUsize::new(0));
            let failed = Arc::new(AtomicUsize::new(0));
            let odb = Arc::new(std::sync::Mutex::new(odb));

            // Thread pool — 64 MB stack matches per-test spawns; prevents stack
            // overflows on deeply recursive PDFs processed by verapdf parsing. (#oracle-gen-crash)
            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(workers)
                .stack_size(64 * 1024 * 1024)
                .build()
                .expect("Failed to build thread pool");

            pool.scope(|s| {
                for pdf_path in &pdfs {
                    let done = Arc::clone(&done);
                    let skipped = Arc::clone(&skipped);
                    let failed = Arc::clone(&failed);
                    let odb = Arc::clone(&odb);
                    let verapdf_path = verapdf_path.clone();

                    s.spawn(move |_| {
                        // Read input PDF.
                        let input_data = match std::fs::read(pdf_path) {
                            Ok(d) => d,
                            Err(_) => {
                                failed.fetch_add(1, Ordering::Relaxed);
                                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                                if n.is_multiple_of(100) {
                                    eprintln!(
                                        "[{n}/{total}] skipped={} failed={}",
                                        skipped.load(Ordering::Relaxed),
                                        failed.load(Ordering::Relaxed)
                                    );
                                }
                                return;
                            }
                        };

                        // Run our full PDF/A conversion pipeline in a subprocess.
                        // Hash the CONVERTED bytes — must match what PdfAConvertTest::run()
                        // hashes when it calls verapdf.validate(). (Bug 1 + Bug 2 fix)
                        //
                        // subprocess isolation: a thread-based approach (even with 256 MB
                        // stack + catch_unwind) cannot survive SIGSEGV or abort() in
                        // pathological PDFs.  Running each PDF in a child process ensures
                        // crashes are contained: the parent skips the PDF and continues.
                        // (#oracle-gen-subprocess)
                        eprintln!("converting: {}", pdf_path.display());
                        let current_exe = std::env::current_exe()
                            .unwrap_or_else(|_| PathBuf::from("xfa-test-runner"));
                        let tmp_conv = {
                            use sha2::{Digest, Sha256};
                            let mut h = Sha256::new();
                            h.update(&input_data);
                            let hex = format!("{:x}", h.finalize());
                            std::env::temp_dir().join(format!("{}_oracle_conv.pdf", &hex[..16]))
                        };
                        let (tx, rx) = std::sync::mpsc::channel::<Option<Vec<u8>>>();
                        let exe_c = current_exe.clone();
                        let path_c = pdf_path.clone();
                        let tmp_c = tmp_conv.clone();
                        std::thread::spawn(move || {
                            let status = std::process::Command::new(&exe_c)
                                .args([
                                    std::ffi::OsStr::new("oracle-convert-one"),
                                    std::ffi::OsStr::new("--input"),
                                    path_c.as_os_str(),
                                    std::ffi::OsStr::new("--output"),
                                    tmp_c.as_os_str(),
                                ])
                                .stdout(std::process::Stdio::null())
                                .stderr(std::process::Stdio::null())
                                .status();
                            let result = match status {
                                Ok(s) if s.success() => std::fs::read(&tmp_c).ok(),
                                _ => None,
                            };
                            let _ = std::fs::remove_file(&tmp_c);
                            let _ = tx.send(result);
                        });
                        let converted = match rx
                            .recv_timeout(std::time::Duration::from_secs(90))
                            .unwrap_or(None)
                        {
                            Some(c) => c,
                            None => {
                                let _ = std::fs::remove_file(&tmp_conv);
                                // Not a PDF, already PDF/A, conversion failed, timeout, or crash.
                                skipped.fetch_add(1, Ordering::Relaxed);
                                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                                if n.is_multiple_of(100) {
                                    eprintln!(
                                        "[{n}/{total}] skipped={} failed={}",
                                        skipped.load(Ordering::Relaxed),
                                        failed.load(Ordering::Relaxed)
                                    );
                                }
                                return;
                            }
                        };

                        let hash = {
                            let mut hasher = Sha256::new();
                            hasher.update(&converted);
                            format!("{:x}", hasher.finalize())
                        };

                        // Skip if already cached.
                        // Bug 3 fix: key is ("verapdf-parsed", "any"), matching the lookup
                        // in VeraPdfOracle::validate().
                        if skip_existing {
                            let db = odb.lock().unwrap();
                            if db.lookup(&hash, "verapdf-parsed", "any").is_some() {
                                skipped.fetch_add(1, Ordering::Relaxed);
                                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                                if n.is_multiple_of(100) {
                                    eprintln!(
                                        "[{n}/{total}] (skipped: {})",
                                        skipped.load(Ordering::Relaxed)
                                    );
                                }
                                return;
                            }
                        }

                        // Write converted PDF to a temp file for veraPDF.
                        let hash_prefix = &hash[..16];
                        let tmp_path =
                            std::env::temp_dir().join(format!("{hash_prefix}_oracle_gen.pdf"));
                        if std::fs::write(&tmp_path, &converted).is_err() {
                            failed.fetch_add(1, Ordering::Relaxed);
                            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                            if n.is_multiple_of(100) {
                                eprintln!(
                                    "[{n}/{total}] skipped={} failed={}",
                                    skipped.load(Ordering::Relaxed),
                                    failed.load(Ordering::Relaxed)
                                );
                            }
                            return;
                        }

                        // Run veraPDF on the converted PDF.
                        let mut cmd = std::process::Command::new(&verapdf_path);
                        cmd.args(["--format", "json", "--flavour", "0"]);
                        cmd.arg(&tmp_path);
                        if std::env::var("JAVA_HOME").is_err() {
                            for java_dir in [
                                "/usr/lib/jvm/java-21-openjdk-amd64",
                                "/usr/lib/jvm/java-17-openjdk-amd64",
                            ] {
                                if std::path::Path::new(java_dir).is_dir() {
                                    cmd.env("JAVA_HOME", java_dir);
                                    break;
                                }
                            }
                        }
                        let verapdf_output = cmd.output();
                        let _ = std::fs::remove_file(&tmp_path);

                        match verapdf_output {
                            Ok(output) if output.status.success() || !output.stdout.is_empty() => {
                                let duration_ms = 0u64; // not meaningful for pre-generated cache
                                                        // Bug 4 fix: parse raw JSON into VeraPdfResult, then serialize
                                                        // to the same format used by the run-local cache.
                                match oracles::verapdf::parse_verapdf_json_output(
                                    &output.stdout,
                                    duration_ms,
                                ) {
                                    Ok(result) => {
                                        if let Ok(json) = serde_json::to_string(&result) {
                                            let db = odb.lock().unwrap();
                                            db.store(&hash, "verapdf-parsed", "any", None, &json);
                                        } else {
                                            failed.fetch_add(1, Ordering::Relaxed);
                                        }
                                    }
                                    Err(_) => {
                                        failed.fetch_add(1, Ordering::Relaxed);
                                    }
                                }
                            }
                            _ => {
                                failed.fetch_add(1, Ordering::Relaxed);
                            }
                        }

                        let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                        if n.is_multiple_of(100) {
                            eprintln!(
                                "[{n}/{total}] skipped={} failed={}",
                                skipped.load(Ordering::Relaxed),
                                failed.load(Ordering::Relaxed)
                            );
                        }
                    });
                }
            });

            let final_count = odb.lock().unwrap().count_for("verapdf-parsed", "any");
            eprintln!(
                "Done: {} processed, {} skipped, {} failed. Oracle DB has {} entries (verapdf-parsed/any)",
                done.load(Ordering::Relaxed),
                skipped.load(Ordering::Relaxed),
                failed.load(Ordering::Relaxed),
                final_count,
            );
        }

        Command::OracleConvertOne { input, output } => {
            let data = match std::fs::read(&input) {
                Ok(d) => d,
                Err(_) => std::process::exit(1),
            };
            match tests::pdfa_convert::convert_to_pdfa_bytes(&data, &input) {
                Some(converted) => {
                    if std::fs::write(&output, &converted).is_err() {
                        std::process::exit(1);
                    }
                }
                None => std::process::exit(1),
            }
        }

        Command::RetestFailures {
            source_db,
            source_run,
            verapdf_path,
            oracle_db: oracle_db_path,
            tests: test_filter,
            timeout,
            workers,
        } => {
            retest::retest_failures(
                &source_db,
                &source_run,
                &verapdf_path,
                oracle_db_path.as_deref(),
                test_filter.as_deref(),
                timeout,
                workers,
            );
        }

        Command::CheckRegression { db, run_a, run_b } => {
            let database = Database::open(&db).expect("Failed to open database");
            let result = database.compare_runs_detailed(&run_a, &run_b);
            println!("{result}");

            match result.verdict {
                db::Verdict::Regression => {
                    eprintln!("REGRESSION DETECTED");
                    std::process::exit(1);
                }
                db::Verdict::NetImprovement => {
                    eprintln!("Improvement confirmed");
                    std::process::exit(0);
                }
                db::Verdict::Neutral => {
                    eprintln!("No significant changes");
                    std::process::exit(0);
                }
            }
        }
    }
}
