use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use walkdir::WalkDir;

use crate::classifier::classify_error;
use crate::config::Config;
use crate::db::{Database, RunSummary, TestResultRow};
use crate::tests::{PdfTest, TestStatus};

pub struct Runner {
    config: Config,
    tests: Vec<Box<dyn PdfTest>>,
    db: Arc<Database>,
}

impl Runner {
    pub fn new(config: Config, tests: Vec<Box<dyn PdfTest>>, db: Database) -> Self {
        Self {
            config,
            tests,
            db: Arc::new(db),
        }
    }

    pub fn run_corpus(&self) -> RunSummary {
        let pdfs = self.enumerate_pdfs();
        let total = pdfs.len();
        let test_count = self.tests.len();

        eprintln!(
            "Found {} PDF files in {:?}",
            total, self.config.corpus_dir
        );

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
            .build()
            .expect("Failed to create thread pool");

        pool.install(|| {
            pdfs.par_iter().for_each(|pdf_path| {
                let path_str = pdf_path.to_string_lossy().to_string();

                // Resume: skip only if ALL tests are already completed for this PDF
                if self.config.resume {
                    let completed = self.db.tests_completed_for_pdf(&self.config.run_id, &path_str);
                    if completed >= test_count {
                        progress.inc(1);
                        return;
                    }
                }

                let pdf_data = match std::fs::read(pdf_path) {
                    Ok(data) => data,
                    Err(e) => {
                        eprintln!("Failed to read {}: {}", path_str, e);
                        progress.inc(1);
                        return;
                    }
                };

                let pdf_hash = hex_sha256(&pdf_data);
                let pdf_size = pdf_data.len() as i64;

                for test in &self.tests {
                    if let Some(filter) = &self.config.test_filter {
                        if !filter.iter().any(|f| f == test.name()) {
                            continue;
                        }
                    }

                    let result = self.run_single_test(test.as_ref(), &pdf_data, pdf_path);

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

                    let row = TestResultRow::from_test_result(
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
        test: &dyn PdfTest,
        pdf_data: &[u8],
        path: &Path,
    ) -> crate::tests::TestResult {
        let timeout = self.config.timeout;
        let test_name = test.name().to_string();

        let data = pdf_data.to_vec();
        let path_buf = path.to_path_buf();

        // catch_unwind for panic safety; post-hoc timeout check
        let start = Instant::now();

        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| test.run(&data, &path_buf)));

        let elapsed = start.elapsed();

        if elapsed > timeout {
            return crate::tests::TestResult {
                status: TestStatus::Timeout,
                error_message: Some(format!(
                    "Test '{test_name}' exceeded timeout of {timeout:?}"
                )),
                duration_ms: elapsed.as_millis() as u64,
                oracle_score: None,
                metadata: Default::default(),
            };
        }

        match result {
            Ok(test_result) => test_result,
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
        }
    }

    fn enumerate_pdfs(&self) -> Vec<PathBuf> {
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
            .collect()
    }
}

fn hex_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}
