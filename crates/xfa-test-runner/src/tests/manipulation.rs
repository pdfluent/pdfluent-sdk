use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

/// Tests PDF manipulation operations (split, merge) via pdf-manip.
///
/// Stap 3 van issue #433: the merge result is serialized to bytes and
/// reopened via both lopdf and pdf-syntax, verifying the full I/O roundtrip
/// (lopdf writer → lopdf reader + pdf-syntax reader).
pub struct ManipulationTest;

impl PdfTest for ManipulationTest {
    fn name(&self) -> &str {
        "manipulation"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        // Wrap entire execution in a thread to guard against lopdf hangs on
        // corrupt PDFs (page-tree loops, infinite decompression, etc.). #452
        let pdf_owned = pdf_data.to_vec();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_inner(pdf_owned)));
            let _ = tx.send(r);
        });
        match rx.recv_timeout(std::time::Duration::from_secs(30)) {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => TestResult {
                status: TestStatus::Crash,
                error_message: Some("panic in test execution".into()),
                duration_ms: 0,
                oracle_score: None,
                metadata: HashMap::new(),
            },
            Err(_) => TestResult {
                status: TestStatus::Timeout,
                error_message: Some("test timed out (>30s)".into()),
                duration_ms: 30_000,
                oracle_score: None,
                metadata: HashMap::new(),
            },
        }
    }
}

fn run_inner(pdf: Vec<u8>) -> TestResult {
    let start = std::time::Instant::now();
    let elapsed = || start.elapsed().as_millis() as u64;

    let doc = match lopdf::Document::load_mem(&pdf) {
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
            error_message: Some("0 pages".to_string()),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    let mut metadata = HashMap::new();
    metadata.insert("original_pages".to_string(), original_pages.to_string());

    // Test 1: Split first page (catch panics from lopdf)
    let split_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pdf_manip::pages::split_by_ranges(&doc, &[(1, 1)])
    }));
    match split_result {
        Ok(Ok(parts)) => {
            if parts.len() != 1 {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some(format!(
                        "split returned {} parts, expected 1",
                        parts.len()
                    )),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata,
                };
            }
            let split_pages = parts[0].get_pages().len();
            metadata.insert("split_pages".to_string(), split_pages.to_string());
            if split_pages != 1 {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some(format!("split page count {split_pages}, expected 1")),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata,
                };
            }
        }
        Ok(Err(e)) => {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!("split failed: {e}")),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata,
            };
        }
        Err(panic_info) => {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!("split panic: {}", panic_message(&panic_info))),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata,
            };
        }
    }

    // Test 2: Merge document with itself (catch panics from lopdf)
    let expected_merged = original_pages * 2;
    let doc_clone = doc.clone();
    let merge_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        pdf_manip::pages::merge_documents(&[doc_clone, doc])
    }));

    let mut merged = match merge_result {
        Ok(Ok(merged)) => {
            let merged_pages = merged.get_pages().len();
            metadata.insert("merged_pages".to_string(), merged_pages.to_string());
            if merged_pages != expected_merged {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some(format!(
                        "merge page count {merged_pages}, expected {expected_merged}"
                    )),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata,
                };
            }
            merged
        }
        Ok(Err(e)) => {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!("merge failed: {e}")),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata,
            };
        }
        Err(panic_info) => {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!("merge panic: {}", panic_message(&panic_info))),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata,
            };
        }
    };

    // Test 3: Serialize merged doc and reopen — verifies the full I/O roundtrip:
    //   lopdf merge → lopdf writer → bytes → lopdf reader + pdf-syntax reader
    let save_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut buf = Vec::new();
        merged.save_to(&mut buf).map(|_| buf)
    }));
    let saved_bytes = match save_result {
        Ok(Ok(b)) => b,
        Ok(Err(e)) => {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!("save merged doc failed: {e}")),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata,
            };
        }
        Err(_) => {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some("panic in save merged doc".into()),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata,
            };
        }
    };
    metadata.insert("saved_bytes".to_string(), saved_bytes.len().to_string());

    // Reopen with lopdf — tests lopdf write → lopdf read consistency
    match lopdf::Document::load_mem(&saved_bytes) {
        Ok(reopened) => {
            let reopened_pages = reopened.get_pages().len();
            if reopened_pages != expected_merged {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some(format!(
                        "roundtrip: reopened page count {reopened_pages}, expected {expected_merged}"
                    )),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata,
                };
            }
        }
        Err(e) => {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!("roundtrip: lopdf reopen failed: {e}")),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata,
            };
        }
    }

    // Reopen with pdf-syntax — tests lopdf write → pdf-syntax read compatibility
    if let Err(e) = pdf_syntax::Pdf::new(saved_bytes) {
        return TestResult {
            status: TestStatus::Fail,
            error_message: Some(format!("roundtrip: pdf-syntax reopen failed: {e:?}")),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata,
        };
    }

    TestResult {
        status: TestStatus::Pass,
        error_message: None,
        duration_ms: elapsed(),
        oracle_score: None,
        metadata,
    }
}

fn panic_message(info: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = info.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = info.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
    }
}
