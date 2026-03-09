use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestTier {
    Fast,     // parse, metadata, geometry, bookmarks, signatures, form_fields, annotations
    Standard, // Fast + render, text_extract, compliance, search
    Full,     // Standard + text_oracle, manipulation, images, metadata_oracle
    Oracle,   // only text_oracle, metadata_oracle
}

impl TestTier {
    pub fn includes(&self, test_name: &str) -> bool {
        match self {
            Self::Fast => matches!(
                test_name,
                "parse"
                    | "metadata"
                    | "geometry"
                    | "bookmarks"
                    | "signatures"
                    | "form_fields"
                    | "annotations"
            ),
            Self::Standard => {
                Self::Fast.includes(test_name)
                    || matches!(
                        test_name,
                        "render" | "text_extract" | "compliance" | "search"
                    )
            }
            Self::Full => true,
            Self::Oracle => matches!(test_name, "text_oracle" | "metadata_oracle"),
        }
    }
}

impl std::str::FromStr for TestTier {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "fast" => Ok(Self::Fast),
            "standard" => Ok(Self::Standard),
            "full" => Ok(Self::Full),
            "oracle" => Ok(Self::Oracle),
            _ => Err(format!(
                "unknown tier '{s}', expected: fast, standard, full, oracle"
            )),
        }
    }
}

impl std::fmt::Display for TestTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Fast => write!(f, "fast"),
            Self::Standard => write!(f, "standard"),
            Self::Full => write!(f, "full"),
            Self::Oracle => write!(f, "oracle"),
        }
    }
}

pub struct Config {
    pub corpus_dir: PathBuf,
    #[allow(dead_code)]
    pub db_path: PathBuf,
    pub workers: usize,
    pub timeout: Duration,
    pub test_filter: Option<Vec<String>>,
    pub resume: bool,
    pub run_id: String,
    pub tier: TestTier,
    pub limit: Option<usize>,
}

impl Config {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        corpus_dir: PathBuf,
        db_path: PathBuf,
        workers: usize,
        timeout_secs: u64,
        tests: Option<String>,
        resume: bool,
        run_id: Option<String>,
        db: Option<&crate::db::Database>,
        tier: TestTier,
        limit: Option<usize>,
    ) -> Self {
        let run_id = run_id.unwrap_or_else(|| {
            if resume {
                // When resuming, find the latest unfinished run for this corpus
                if let Some(db) = db {
                    let corpus_str = corpus_dir.to_string_lossy();
                    if let Some(latest) = db.latest_run_id_for_corpus(&corpus_str) {
                        return latest;
                    }
                }
            }
            let now = chrono::Utc::now();
            format!("run-{}", now.format("%Y%m%d-%H%M%S%.3f"))
        });
        let test_filter = tests.map(|t| t.split(',').map(|s| s.trim().to_string()).collect());
        Self {
            corpus_dir,
            db_path,
            workers,
            timeout: Duration::from_secs(timeout_secs),
            test_filter,
            resume,
            run_id,
            tier,
            limit,
        }
    }
}

/// Resolve worker count: "auto" → nproc - 2, otherwise parse as number.
pub fn resolve_workers(input: &str) -> usize {
    if input == "auto" {
        let cpus = num_cpus::get();
        cpus.saturating_sub(2).max(1)
    } else {
        input.parse::<usize>().unwrap_or_else(|_| {
            eprintln!("Invalid --workers value '{input}', using auto");
            num_cpus::get().saturating_sub(2).max(1)
        })
    }
}
