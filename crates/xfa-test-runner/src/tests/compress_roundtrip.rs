// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

/// Corpus-level roundtrip test for PDF stream compression.
///
/// Runs `pdf_manip::optimize::compress_streams` on the document, saves to
/// bytes, reloads with lopdf, and verifies that:
/// - The output is parseable.
/// - The page count is unchanged.
///
/// The test does NOT require that compression actually shrinks the file
/// (already-compressed PDFs may not benefit), only that the output remains
/// a valid, parseable PDF.
pub struct CompressRoundtripTest;

impl PdfTest for CompressRoundtripTest {
    fn name(&self) -> &str {
        "compress_roundtrip"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let pdf_owned = pdf_data.to_vec();
        let (tx, rx) = std::sync::mpsc::channel();
        if std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || {
                let r =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_inner(pdf_owned)));
                let _ = tx.send(r);
            })
            .is_err()
        {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some("thread spawn failed (resource limit)".into()),
                duration_ms: 0,
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
        match rx.recv_timeout(std::time::Duration::from_secs(25)) {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => TestResult {
                status: TestStatus::Crash,
                error_message: Some("panic in compress_roundtrip".into()),
                duration_ms: 0,
                oracle_score: None,
                metadata: HashMap::new(),
            },
            Err(_) => TestResult {
                status: TestStatus::Timeout,
                error_message: Some("compress_roundtrip timed out (>25s)".into()),
                duration_ms: 30_000,
                oracle_score: None,
                metadata: HashMap::new(),
            },
        }
    }
}

fn run_inner(pdf: Vec<u8>) -> TestResult {
    use pdf_manip::optimize::compress_streams;

    let start = std::time::Instant::now();
    let elapsed = || start.elapsed().as_millis() as u64;

    let mut doc = match lopdf::Document::load_mem(&pdf) {
        Ok(d) => d,
        Err(e) => {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some(format!("lopdf load failed: {e}")),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
    };

    let original_pages = doc.get_pages().len();
    if original_pages == 0 {
        return TestResult {
            status: TestStatus::Skip,
            error_message: Some("0 pages".into()),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    // Run stream compression.
    let streams_compressed =
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| compress_streams(&mut doc)))
        {
            Ok(Ok(n)) => n,
            Ok(Err(e)) => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some(format!("compress_streams failed: {e}")),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
            Err(_) => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some("compress_streams panicked".into()),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

    // Save to bytes.
    let saved = match (|| -> Result<Vec<u8>, lopdf::Error> {
        let mut buf = Vec::new();
        doc.save_to(&mut buf)?;
        Ok(buf)
    })() {
        Ok(b) => b,
        Err(e) => {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!("save failed: {e}")),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
    };

    // Reload and verify page count.
    let reopened = match lopdf::Document::load_mem(&saved) {
        Ok(d) => d,
        Err(e) => {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!("roundtrip: lopdf reopen failed: {e}")),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
    };

    let reopened_pages = reopened.get_pages().len();
    if reopened_pages != original_pages {
        return TestResult {
            status: TestStatus::Fail,
            error_message: Some(format!(
                "page count changed: {original_pages} → {reopened_pages}"
            )),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    let mut metadata = HashMap::new();
    metadata.insert("pages".to_string(), original_pages.to_string());
    metadata.insert(
        "streams_compressed".to_string(),
        streams_compressed.to_string(),
    );
    metadata.insert("saved_bytes".to_string(), saved.len().to_string());

    TestResult {
        status: TestStatus::Pass,
        error_message: None,
        duration_ms: elapsed(),
        oracle_score: None,
        metadata,
    }
}
