use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

pub struct SearchTest;

impl PdfTest for SearchTest {
    fn name(&self) -> &str {
        "search"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let start = std::time::Instant::now();

        // Try lopdf first for full search capability.
        match lopdf::Document::load_mem(pdf_data) {
            Ok(doc) => {
                let options = pdf_extract::SearchOptions::default();
                let results = pdf_extract::search_text(&doc, "the", &options);

                let mut metadata = HashMap::new();
                metadata.insert("match_count".to_string(), results.len().to_string());
                metadata.insert("backend".to_string(), "lopdf".to_string());

                TestResult {
                    status: TestStatus::Pass,
                    error_message: None,
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata,
                }
            }
            Err(_) => {
                // Fallback: use pdf_engine text extraction for search validation.
                match pdf_engine::PdfDocument::open(pdf_data.to_vec()) {
                    Ok(doc) => {
                        let page_count = doc.page_count();
                        let mut match_count = 0usize;
                        let pages_to_search = page_count.min(5);

                        for i in 0..pages_to_search {
                            if let Ok(text) = doc.extract_text(i) {
                                match_count += text.to_lowercase().matches("the").count();
                            }
                        }

                        let mut metadata = HashMap::new();
                        metadata.insert("match_count".to_string(), match_count.to_string());
                        metadata.insert("backend".to_string(), "pdf_engine".to_string());

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
    }
}
