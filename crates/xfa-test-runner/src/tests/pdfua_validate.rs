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

        // Only validate PDF/UA-1 (ISO 14289-1). PDF/UA-2 (ISO 14289-2, 2024)
        // has different rules; running our PDF/UA-1 checker against it produces
        // noise failures.  Skip until we support PDF/UA-2 validation.
        if pdfua_part_number(pdf_data) != Some(1) {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some("PDF/UA-2 not supported by this checker".into()),
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
    pdf_data.windows(12).any(|w| w == b"pdfuaid:part")
}

/// Parse the numeric PDF/UA part from `pdfuaid:part` in the raw XMP bytes.
///
/// Handles both attribute (`pdfuaid:part="2"`) and element
/// (`<pdfuaid:part>2</pdfuaid:part>`) forms.  Returns `None` if the claim is
/// absent or unparseable.
fn pdfua_part_number(pdf_data: &[u8]) -> Option<u8> {
    // Fast scan: find "pdfuaid:part" then read the digit(s) following it.
    let needle = b"pdfuaid:part";
    let pos = pdf_data.windows(needle.len()).position(|w| w == needle)?;
    let after = &pdf_data[pos + needle.len()..];
    // Skip whitespace, '=', '"', '>'
    let digit_start = after
        .iter()
        .position(|&b| b.is_ascii_digit())?;
    let rest = &after[digit_start..];
    let digit_end = rest
        .iter()
        .position(|&b| !b.is_ascii_digit())
        .unwrap_or(rest.len());
    std::str::from_utf8(&rest[..digit_end])
        .ok()?
        .parse::<u8>()
        .ok()
}
