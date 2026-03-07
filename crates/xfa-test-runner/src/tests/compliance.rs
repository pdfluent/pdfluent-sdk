use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

pub struct ComplianceTest;

impl PdfTest for ComplianceTest {
    fn name(&self) -> &str {
        "compliance"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let start = std::time::Instant::now();

        let pdf = match pdf_syntax::Pdf::new(pdf_data.to_vec()) {
            Ok(p) => p,
            Err(e) => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some(format!("{e:?}")),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        let report = pdf_compliance::validate_pdfa(&pdf, pdf_compliance::PdfALevel::A1b);

        let mut metadata = HashMap::new();
        metadata.insert("compliant".to_string(), report.compliant.to_string());
        metadata.insert("error_count".to_string(), report.error_count().to_string());
        metadata.insert(
            "warning_count".to_string(),
            report.warning_count().to_string(),
        );

        TestResult {
            status: TestStatus::Pass,
            error_message: None,
            duration_ms: start.elapsed().as_millis() as u64,
            oracle_score: None,
            metadata,
        }
    }
}
