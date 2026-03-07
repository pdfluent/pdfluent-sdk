use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

pub struct ImageExtractTest;

impl PdfTest for ImageExtractTest {
    fn name(&self) -> &str {
        "images"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let start = std::time::Instant::now();

        let doc = match lopdf::Document::load_mem(pdf_data) {
            Ok(d) => d,
            Err(e) => {
                let status = if pdf_syntax::Pdf::new(pdf_data.to_vec()).is_ok() {
                    TestStatus::Skip
                } else {
                    TestStatus::Fail
                };
                return TestResult {
                    status,
                    error_message: Some(format!("{e}")),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        let pages = doc.get_pages();
        if pages.is_empty() {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some("document has 0 pages".to_string()),
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        let mut total_images = 0usize;
        let page_count = pages.len() as u32;
        let pages_to_check = page_count.min(5);

        for page_num in 1..=pages_to_check {
            match pdf_extract::extract_page_images(&doc, page_num) {
                Ok(images) => {
                    total_images += images.len();
                }
                Err(e) => {
                    return TestResult {
                        status: TestStatus::Fail,
                        error_message: Some(format!("page {page_num}: {e}")),
                        duration_ms: start.elapsed().as_millis() as u64,
                        oracle_score: None,
                        metadata: HashMap::new(),
                    };
                }
            }
        }

        let mut metadata = HashMap::new();
        metadata.insert("image_count".to_string(), total_images.to_string());
        metadata.insert("pages_checked".to_string(), pages_to_check.to_string());

        TestResult {
            status: TestStatus::Pass,
            error_message: None,
            duration_ms: start.elapsed().as_millis() as u64,
            oracle_score: None,
            metadata,
        }
    }
}
