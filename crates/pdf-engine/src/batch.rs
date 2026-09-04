//! Batch helpers for processing many PDFs with a bounded worker pool.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use crate::{PdfDocument, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc};
use std::thread;
use std::time::{Duration, Instant};

/// How batch processing should classify per-file failures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ErrorStrategy {
    /// Collect failures in [`BatchResult::failures`] and continue (default).
    #[default]
    Collect,
    /// Record failures in [`BatchResult::skipped`] and continue.
    Skip,
    /// Stop scheduling new work after the first failure.
    FailFast,
}

/// Configuration for [`process_batch`] and [`PdfBatch`] helpers.
pub struct BatchConfig {
    /// Number of worker threads to use.
    pub workers: usize,
    /// Maximum time allowed per input PDF.
    pub timeout: Duration,
    /// How per-file failures should be recorded.
    pub on_error: ErrorStrategy,
    /// Optional progress callback invoked as `(done, total)`.
    pub on_progress: Option<Box<dyn Fn(usize, usize) + Send + Sync + 'static>>,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            workers: thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1),
            timeout: Duration::from_secs(30),
            on_error: ErrorStrategy::Collect,
            on_progress: None,
        }
    }
}

/// Aggregated outcome of a batch run.
#[derive(Debug)]
pub struct BatchResult<T> {
    /// Successfully processed PDFs and their outputs.
    pub successes: Vec<(PathBuf, T)>,
    /// PDFs that failed to process.
    pub failures: Vec<(PathBuf, String)>,
    /// PDFs skipped due to [`ErrorStrategy::Skip`].
    pub skipped: Vec<(PathBuf, String)>,
    /// End-to-end runtime for the batch.
    pub duration: Duration,
}

impl<T> Default for BatchResult<T> {
    fn default() -> Self {
        Self {
            successes: Vec::new(),
            failures: Vec::new(),
            skipped: Vec::new(),
            duration: Duration::default(),
        }
    }
}

impl<T> BatchResult<T> {
    /// Total number of completed inputs.
    pub fn total(&self) -> usize {
        self.successes.len() + self.failures.len() + self.skipped.len()
    }
}

/// Convenience helpers built on top of [`process_batch`].
#[derive(Debug, Default, Clone, Copy)]
pub struct PdfBatch;

impl PdfBatch {
    /// Recursively collect `.pdf` files from a directory tree.
    pub fn collect_pdfs(root: impl AsRef<Path>) -> std::io::Result<Vec<PathBuf>> {
        let mut paths = Vec::new();
        collect_pdfs_recursive(root.as_ref(), &mut paths)?;
        paths.sort();
        Ok(paths)
    }

    /// Extract combined text from every PDF under `root`.
    pub fn extract_text(root: impl AsRef<Path>, config: BatchConfig) -> BatchResult<String> {
        let root = root.as_ref().to_path_buf();
        match Self::collect_pdfs(&root) {
            Ok(paths) => process_batch(&paths, config, |doc| Ok(doc.extract_all_text())),
            Err(err) => {
                let mut result = BatchResult::default();
                result.failures.push((root, err.to_string()));
                result
            }
        }
    }

    /// Count pages for every PDF under `root`.
    pub fn page_counts(root: impl AsRef<Path>, config: BatchConfig) -> BatchResult<usize> {
        let root = root.as_ref().to_path_buf();
        match Self::collect_pdfs(&root) {
            Ok(paths) => process_batch(&paths, config, |doc| Ok(doc.page_count())),
            Err(err) => {
                let mut result = BatchResult::default();
                result.failures.push((root, err.to_string()));
                result
            }
        }
    }
}

/// Process a list of PDF paths with a fixed-size worker pool.
pub fn process_batch<P, F, T>(paths: &[P], config: BatchConfig, processor: F) -> BatchResult<T>
where
    P: AsRef<Path>,
    F: Fn(&PdfDocument) -> Result<T> + Send + Sync + 'static,
    T: Send + 'static,
{
    let started = Instant::now();
    let input_paths = paths
        .iter()
        .map(|path| path.as_ref().to_path_buf())
        .collect::<Vec<_>>();
    let total = input_paths.len();

    if total == 0 {
        return BatchResult {
            duration: started.elapsed(),
            ..Default::default()
        };
    }

    let workers = config.workers.max(1).min(total);
    let next_index = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let input_paths = Arc::new(input_paths);
    let processor = Arc::new(processor);
    let (tx, rx) = mpsc::channel();
    let mut handles = Vec::with_capacity(workers);

    for _ in 0..workers {
        let tx = tx.clone();
        let next_index = Arc::clone(&next_index);
        let stop = Arc::clone(&stop);
        let input_paths = Arc::clone(&input_paths);
        let processor = Arc::clone(&processor);
        let timeout = config.timeout;

        handles.push(thread::spawn(move || loop {
            if stop.load(Ordering::Relaxed) {
                break;
            }

            let index = next_index.fetch_add(1, Ordering::Relaxed);
            if index >= input_paths.len() {
                break;
            }

            let path = input_paths[index].clone();
            let outcome = process_one(path, timeout, Arc::clone(&processor));
            if tx.send(outcome).is_err() {
                break;
            }
        }));
    }
    drop(tx);

    let mut result = BatchResult::default();
    let mut done = 0usize;

    while done < total {
        let Ok(outcome) = rx.recv() else {
            break;
        };
        done += 1;

        match outcome {
            BatchOutcome::Success(path, value) => result.successes.push((path, value)),
            BatchOutcome::Failure(path, err) => match config.on_error {
                ErrorStrategy::Collect => result.failures.push((path, err)),
                ErrorStrategy::Skip => result.skipped.push((path, err)),
                ErrorStrategy::FailFast => {
                    result.failures.push((path, err));
                    stop.store(true, Ordering::Relaxed);
                }
            },
        }

        if let Some(callback) = config.on_progress.as_ref() {
            callback(done, total);
        }

        if config.on_error == ErrorStrategy::FailFast && !result.failures.is_empty() {
            break;
        }
    }

    stop.store(true, Ordering::Relaxed);
    for handle in handles {
        let _ = handle.join();
    }

    result.duration = started.elapsed();
    result
}

#[derive(Debug)]
enum BatchOutcome<T> {
    Success(PathBuf, T),
    Failure(PathBuf, String),
}

fn process_one<F, T>(path: PathBuf, timeout: Duration, processor: Arc<F>) -> BatchOutcome<T>
where
    F: Fn(&PdfDocument) -> Result<T> + Send + Sync + 'static,
    T: Send + 'static,
{
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(err) => return BatchOutcome::Failure(path, err.to_string()),
    };

    let (tx, rx) = mpsc::sync_channel(1);
    thread::spawn(move || {
        let outcome = (|| {
            let doc = PdfDocument::open(bytes).map_err(|err| err.to_string())?;
            processor(&doc).map_err(|err| err.to_string())
        })();
        let _ = tx.send(outcome);
    });

    match rx.recv_timeout(timeout) {
        Ok(Ok(value)) => BatchOutcome::Success(path, value),
        Ok(Err(err)) => BatchOutcome::Failure(path, err),
        Err(mpsc::RecvTimeoutError::Timeout) => {
            BatchOutcome::Failure(path, format!("timed out after {}s", timeout.as_secs_f64()))
        }
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            BatchOutcome::Failure(path, "worker disconnected".into())
        }
    }
}

fn collect_pdfs_recursive(root: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_pdfs_recursive(&path, out)?;
            continue;
        }

        if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("pdf"))
        {
            out.push(path);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{process_batch, BatchConfig, ErrorStrategy, PdfBatch};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[test]
    fn batch_process_counts_pages() {
        let dir = make_temp_dir("counts");
        let first = dir.join("first.pdf");
        let second = dir.join("second.pdf");
        fs::write(&first, minimal_pdf_bytes()).expect("write fixture");
        fs::write(&second, minimal_pdf_bytes()).expect("write fixture");

        let result = process_batch(
            &[first.clone(), second.clone()],
            BatchConfig {
                workers: 2,
                timeout: Duration::from_secs(5),
                on_error: ErrorStrategy::Collect,
                on_progress: None,
            },
            |doc| Ok(doc.page_count()),
        );

        assert_eq!(result.successes.len(), 2);
        assert!(result.failures.is_empty());
        assert!(result.skipped.is_empty());
        assert!(result.successes.iter().all(|(_, count)| *count == 1));

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn batch_process_skip_strategy_records_skips() {
        let dir = make_temp_dir("skip");
        let good = dir.join("good.pdf");
        let bad = dir.join("bad.pdf");
        fs::write(&good, minimal_pdf_bytes()).expect("write fixture");
        fs::write(&bad, b"not a pdf").expect("write invalid fixture");

        let result = process_batch(
            &[good, bad],
            BatchConfig {
                workers: 1,
                timeout: Duration::from_secs(5),
                on_error: ErrorStrategy::Skip,
                on_progress: None,
            },
            |doc| Ok(doc.page_count()),
        );

        assert_eq!(result.successes.len(), 1);
        assert!(result.failures.is_empty());
        assert_eq!(result.skipped.len(), 1);

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn pdf_batch_collects_nested_pdfs() {
        let dir = make_temp_dir("walk");
        let nested = dir.join("nested");
        fs::create_dir_all(&nested).expect("create nested dir");
        fs::write(dir.join("root.pdf"), minimal_pdf_bytes()).expect("write root pdf");
        fs::write(nested.join("child.pdf"), minimal_pdf_bytes()).expect("write child pdf");
        fs::write(nested.join("ignore.txt"), b"hello").expect("write non-pdf");

        let pdfs = PdfBatch::collect_pdfs(&dir).expect("collect pdfs");
        assert_eq!(pdfs.len(), 2);

        fs::remove_dir_all(dir).ok();
    }

    fn make_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "pdf-engine-batch-{label}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn minimal_pdf_bytes() -> Vec<u8> {
        br#"%PDF-1.4
1 0 obj
<< /Type /Catalog /Pages 2 0 R >>
endobj
2 0 obj
<< /Type /Pages /Kids [3 0 R] /Count 1 >>
endobj
3 0 obj
<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >>
endobj
xref
0 4
0000000000 65535 f 
0000000009 00000 n 
0000000058 00000 n 
0000000115 00000 n 
trailer
<< /Size 4 /Root 1 0 R >>
startxref
186
%%EOF
"#
        .to_vec()
    }
}
