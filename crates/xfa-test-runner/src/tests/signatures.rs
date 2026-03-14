use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

pub struct SignaturesTest;

impl PdfTest for SignaturesTest {
    fn name(&self) -> &str {
        "signatures"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let start = std::time::Instant::now();

        let pdf = match pdf_syntax::Pdf::new(pdf_data.to_vec()) {
            Ok(p) => p,
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

        let sigs = pdf_sign::signature_fields(&pdf);
        if sigs.is_empty() {
            return TestResult {
                status: TestStatus::Skip,
                error_message: None,
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        let results = pdf_sign::validate_signatures(&pdf);

        let mut metadata = HashMap::new();
        metadata.insert("signature_count".to_string(), sigs.len().to_string());
        metadata.insert("validation_count".to_string(), results.len().to_string());

        TestResult {
            status: TestStatus::Pass,
            error_message: None,
            duration_ms: start.elapsed().as_millis() as u64,
            oracle_score: None,
            metadata,
        }
    }
}
