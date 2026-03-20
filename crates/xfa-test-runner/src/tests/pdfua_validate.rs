//! PDF/UA corpus validation test.
//!
//! Only runs on PDFs that claim PDF/UA conformance (via `pdfuaid:part` in XMP
//! or `/MarkInfo /Marked true` in the catalog).  All other PDFs are skipped.
//!
//! Skip policy:
//! - PDF does not claim PDF/UA → Skip
//! - pdf-syntax cannot parse the PDF → Skip
//!
//! Fail conditions:
//! - Our PDF/UA validator reports at least one Error-severity issue

use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

pub struct PdfUaValidateTest;

impl PdfTest for PdfUaValidateTest {
    fn name(&self) -> &str {
        "pdfua_validate"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let start = std::time::Instant::now();
        let elapsed = || start.elapsed().as_millis() as u64;

        if !claims_pdfua(pdf_data) {
            return TestResult {
                status: TestStatus::Skip,
                error_message: None,
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        let pdf = match pdf_syntax::Pdf::new(pdf_data.to_vec()) {
            Ok(p) => p,
            Err(_) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some("pdf-syntax could not parse PDF".into()),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        let report = pdf_compliance::validate_pdfua(&pdf);

        let error_count = report.error_count();
        let warning_count = report.warning_count();

        let mut metadata = HashMap::new();
        metadata.insert("error_count".to_string(), error_count.to_string());
        metadata.insert("warning_count".to_string(), warning_count.to_string());
        metadata.insert("compliant".to_string(), report.is_compliant().to_string());

        if error_count > 0 {
            // Include the first error message for easy debugging.
            let first_error = report
                .issues
                .iter()
                .find(|i| i.severity == pdf_compliance::Severity::Error)
                .map(|i| format!("[{}] {}", i.rule, i.message))
                .unwrap_or_default();
            metadata.insert("first_error".to_string(), first_error.clone());

            TestResult {
                status: TestStatus::Fail,
                error_message: Some(first_error),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata,
            }
        } else {
            TestResult {
                status: TestStatus::Pass,
                error_message: None,
                duration_ms: elapsed(),
                oracle_score: None,
                metadata,
            }
        }
    }
}

/// Returns `true` if the PDF claims PDF/UA conformance.
///
/// Checks for `pdfuaid:part` in XMP (the canonical marker) as a fast
/// byte-level scan.
fn claims_pdfua(pdf_data: &[u8]) -> bool {
    pdf_data
        .windows(12)
        .any(|w| w == b"pdfuaid:part")
}
