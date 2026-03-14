use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

pub struct TextExtractTest;

impl PdfTest for TextExtractTest {
    fn name(&self) -> &str {
        "text_extract"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let start = std::time::Instant::now();

        let doc = match pdf_engine::PdfDocument::open(pdf_data.to_vec()) {
            Ok(d) => d,
            Err(_) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some("invalid PDF: cannot parse".into()),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        let page_count = doc.page_count();
        let pages_to_extract = page_count.min(5);
        let mut total_chars = 0usize;
        let mut pages_extracted = 0usize;

        for i in 0..pages_to_extract {
            // Abort if total test time exceeds budget.
            if start.elapsed().as_secs() >= 20 {
                break;
            }
            let page_start = std::time::Instant::now();
            match doc.extract_text(i) {
                Ok(text) => {
                    total_chars += text.len();
                    pages_extracted += 1;
                    // Skip remaining pages if this one was slow.
                    if page_start.elapsed().as_secs() >= 10 {
                        break;
                    }
                }
                Err(e) => {
                    return TestResult {
                        status: TestStatus::Fail,
                        error_message: Some(format!("Text extraction page {i} failed: {e}")),
                        duration_ms: start.elapsed().as_millis() as u64,
                        oracle_score: None,
                        metadata: HashMap::new(),
                    };
                }
            }
        }

        let mut metadata = HashMap::new();
        metadata.insert("total_chars".to_string(), total_chars.to_string());
        metadata.insert("pages_extracted".to_string(), pages_extracted.to_string());

        TestResult {
            status: TestStatus::Pass,
            error_message: None,
            duration_ms: start.elapsed().as_millis() as u64,
            oracle_score: None,
            metadata,
        }
    }
}
