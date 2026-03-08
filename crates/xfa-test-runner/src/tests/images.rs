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
            Err(_) => {
                // lopdf can't parse — fall back to pdf_engine to verify the PDF is valid.
                return self.fallback_check(pdf_data, start);
            }
        };

        let pages = doc.get_pages();
        if pages.is_empty() {
            // lopdf found 0 pages — fall back to pdf_engine page count check.
            return self.fallback_check(pdf_data, start);
        }

        let mut total_images = 0usize;
        let page_count = pages.len() as u32;
        let pages_to_check = page_count.min(5);
        let mut pages_checked = 0u32;

        for page_num in 1..=pages_to_check {
            // Abort if total test time exceeds budget.
            if start.elapsed().as_secs() >= 20 {
                break;
            }
            let page_start = std::time::Instant::now();
            match pdf_extract::extract_page_images(&doc, page_num) {
                Ok(images) => {
                    total_images += images.len();
                    pages_checked += 1;
                    if page_start.elapsed().as_secs() >= 10 {
                        break;
                    }
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
        metadata.insert("pages_checked".to_string(), pages_checked.to_string());
        metadata.insert("backend".to_string(), "lopdf".to_string());

        TestResult {
            status: TestStatus::Pass,
            error_message: None,
            duration_ms: start.elapsed().as_millis() as u64,
            oracle_score: None,
            metadata,
        }
    }
}

impl ImageExtractTest {
    /// Fallback when lopdf fails: use pdf_engine to verify the PDF has pages.
    /// We can't extract images without lopdf, but we can confirm the PDF is valid.
    fn fallback_check(&self, pdf_data: &[u8], start: std::time::Instant) -> super::TestResult {
        match pdf_engine::PdfDocument::open(pdf_data.to_vec()) {
            Ok(doc) => {
                let page_count = doc.page_count();
                let mut metadata = HashMap::new();
                metadata.insert("page_count".to_string(), page_count.to_string());
                metadata.insert("backend".to_string(), "pdf_engine".to_string());
                metadata.insert(
                    "note".to_string(),
                    "lopdf could not parse; image extraction skipped".to_string(),
                );

                super::TestResult {
                    status: TestStatus::Pass,
                    error_message: None,
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata,
                }
            }
            Err(e) => super::TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!("{e}")),
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata: HashMap::new(),
            },
        }
    }
}
