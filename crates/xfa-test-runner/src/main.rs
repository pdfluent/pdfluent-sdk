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

use config::Config;
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

        /// Number of parallel workers
        #[arg(short = 'j', long, default_value_t = num_cpus::get())]
        workers: usize,

        /// Timeout per PDF in seconds
        #[arg(short, long, default_value_t = 30)]
        timeout: u64,

        /// Only run specific tests (comma-separated)
        #[arg(long)]
        tests: Option<String>,

        /// Resume from last incomplete run
        #[arg(long)]
        resume: bool,

        /// Run ID (auto-generated if not provided)
        #[arg(long)]
        run_id: Option<String>,

        /// Disable veraPDF oracle
        #[arg(long)]
        no_verapdf: bool,

        /// Path to veraPDF binary
        #[arg(long, default_value = "/usr/local/bin/verapdf")]
        verapdf_path: PathBuf,
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
            run_id,
            no_verapdf,
            verapdf_path,
        } => {
            let database = Database::open(&db).expect("Failed to open database");
            let config = Config::new(
                corpus,
                db,
                workers,
                timeout,
                test_filter,
                resume,
                run_id,
                Some(&database),
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
            if let Some(filter) = &config.test_filter {
                available_tests.retain(|t| filter.iter().any(|f| f == t.name()));
            }

            eprintln!(
                "Starting run '{}' with {} workers, {} tests",
                config.run_id,
                config.workers,
                available_tests.len()
            );

            let runner = Runner::new(config, available_tests, database);
            let summary = runner.run_corpus();
            eprintln!("\n{summary}");
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
                let pattern = if c.error_pattern.len() > 40 {
                    format!("{}...", &c.error_pattern[..40])
                } else {
                    c.error_pattern.clone()
                };
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
    }
}
