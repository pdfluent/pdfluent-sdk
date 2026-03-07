mod classifier;
mod config;
mod db;
mod runner;
mod tests;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use config::Config;
use db::Database;
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

            let mut available_tests = tests::all_tests();
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
            let result = database.compare_runs(&run_a, &run_b);
            println!("{result}");
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
    }
}
