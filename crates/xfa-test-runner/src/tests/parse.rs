use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

pub struct ParseTest;

impl PdfTest for ParseTest {
    fn name(&self) -> &str {
        "parse"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let start = std::time::Instant::now();

        match pdf_syntax::Pdf::new(pdf_data.to_vec()) {
            Ok(pdf) => {
                let page_count = pdf.pages().len();
                let mut metadata = HashMap::new();
                metadata.insert("page_count".to_string(), page_count.to_string());
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
                error_message: Some(format!("{e:?}")),
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata: HashMap::new(),
            },
        }
    }
}
