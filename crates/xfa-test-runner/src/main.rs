mod classifier;
#[allow(dead_code)]
mod clustering;
mod config;
#[allow(dead_code)]
mod dashboard;
mod db;
#[allow(dead_code)]
mod github_issues;
mod oracles;
mod runner;
mod tests;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use std::sync::Arc;

use config::{Config, TestTier};
use db::Database;
use oracles::verapdf::VeraPdfOracle;
use runner::Runner;

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
