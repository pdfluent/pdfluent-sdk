use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

pub struct MetadataTest;

impl PdfTest for MetadataTest {
    fn name(&self) -> &str {
        "metadata"
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
        if page_count == 0 {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some("page_count is 0".to_string()),
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        let info = doc.info();
        let mut metadata = HashMap::new();
        metadata.insert("page_count".to_string(), page_count.to_string());
        if let Some(t) = &info.title {
            metadata.insert("title".to_string(), t.clone());
        }
        if let Some(a) = &info.author {
            metadata.insert("author".to_string(), a.clone());
        }
        if let Some(p) = &info.producer {
            metadata.insert("producer".to_string(), p.clone());
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
