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
            Err(e) => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some(format!("{e}")),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        let page_count = doc.page_count();
        let pages_to_extract = page_count.min(5);
        let mut total_chars = 0usize;

        for i in 0..pages_to_extract {
            match doc.extract_text(i) {
                Ok(text) => {
                    total_chars += text.len();
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
        metadata.insert("pages_extracted".to_string(), pages_to_extract.to_string());

        TestResult {
            status: TestStatus::Pass,
            error_message: None,
            duration_ms: start.elapsed().as_millis() as u64,
            oracle_score: None,
            metadata,
        }
    }
}
