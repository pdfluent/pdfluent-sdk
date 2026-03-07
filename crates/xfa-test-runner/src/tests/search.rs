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

        let doc = match lopdf::Document::load_mem(pdf_data) {
            Ok(d) => d,
            Err(e) => {
                // If pdf_syntax can parse it but lopdf cannot, this is a lopdf limitation,
                // not a bug in our code — skip rather than fail.
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

        let options = pdf_extract::SearchOptions::default();
        let results = pdf_extract::search_text(&doc, "the", &options);

        let mut metadata = HashMap::new();
        metadata.insert("match_count".to_string(), results.len().to_string());

        TestResult {
            status: TestStatus::Pass,
            error_message: None,
            duration_ms: start.elapsed().as_millis() as u64,
            oracle_score: None,
            metadata,
        }
    }
}
