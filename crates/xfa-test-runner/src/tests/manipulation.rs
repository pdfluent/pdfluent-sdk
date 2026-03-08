use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

/// Tests PDF manipulation operations (split, merge) via pdf-manip.
pub struct ManipulationTest;

impl PdfTest for ManipulationTest {
    fn name(&self) -> &str {
        "manipulation"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let start = std::time::Instant::now();

        let doc = match lopdf::Document::load_mem(pdf_data) {
            Ok(d) => d,
            Err(e) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some(format!("lopdf load failed: {e}")),
                    duration_ms: start.elapsed().as_millis() as u64,
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
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        let mut metadata = HashMap::new();
        metadata.insert("original_pages".to_string(), original_pages.to_string());

        // Test 1: Split first page
        match pdf_manip::pages::split_by_ranges(&doc, &[(1, 1)]) {
            Ok(parts) => {
                if parts.len() != 1 {
                    return TestResult {
                        status: TestStatus::Fail,
                        error_message: Some(format!(
                            "split returned {} parts, expected 1",
                            parts.len()
                        )),
                        duration_ms: start.elapsed().as_millis() as u64,
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
                        duration_ms: start.elapsed().as_millis() as u64,
                        oracle_score: None,
                        metadata,
                    };
                }
            }
            Err(e) => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some(format!("split failed: {e}")),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata,
                };
            }
        }

        // Test 2: Merge document with itself
        match pdf_manip::pages::merge_documents(&[doc.clone(), doc]) {
            Ok(merged) => {
                let merged_pages = merged.get_pages().len();
                metadata.insert("merged_pages".to_string(), merged_pages.to_string());
                let expected = original_pages * 2;
                if merged_pages != expected {
                    return TestResult {
                        status: TestStatus::Fail,
                        error_message: Some(format!(
                            "merge page count {merged_pages}, expected {expected}"
                        )),
                        duration_ms: start.elapsed().as_millis() as u64,
                        oracle_score: None,
                        metadata,
                    };
                }
            }
            Err(e) => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some(format!("merge failed: {e}")),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata,
                };
            }
        }

        TestResult {
            status: TestStatus::Pass,
            error_message: None,
            duration_ms: start.elapsed().as_millis() as u64,
            oracle_score: None,
            metadata,
        }
    }
}
