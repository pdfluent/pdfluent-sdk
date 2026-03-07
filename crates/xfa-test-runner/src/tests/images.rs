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
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some(format!("{e}")),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        match pdf_extract::extract_page_images(&doc, 1) {
            Ok(images) => {
                let mut metadata = HashMap::new();
                metadata.insert("image_count".to_string(), images.len().to_string());

                TestResult {
                    status: TestStatus::Pass,
                    error_message: None,
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata,
                }
            }
            Err(e) => TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!("{e}")),
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata: HashMap::new(),
            },
        }
    }
}
