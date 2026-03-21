//! PDF/X corpus validation test.
//!
//! Only runs on PDFs that claim PDF/X conformance (detected by the presence of
//! a `GTS_PDFX` OutputIntent in the raw byte stream).  All other PDFs are skipped.
//!
//! The PDF/X level is auto-detected from XMP (`pdfxid:GTS_PDFXVersion`) with a
//! fallback to PDF/X-4 (the most permissive level).
//!
//! Skip policy:
//! - PDF does not contain `GTS_PDFX` marker → Skip
//! - pdf-syntax cannot parse the PDF → Skip
//!
//! Fail conditions:
//! - Our PDF/X validator reports at least one Error-severity issue

use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

pub struct PdfXValidateTest;

impl PdfTest for PdfXValidateTest {
    fn name(&self) -> &str {
        "pdfx_validate"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let start = std::time::Instant::now();
        let elapsed = || start.elapsed().as_millis() as u64;

        if !claims_pdfx(pdf_data) {
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

        let level = detect_pdfx_level(pdf_data);

        let report = pdf_compliance::validate_pdfx(&pdf, level);

        let error_count = report.error_count();
        let warning_count = report.warning_count();

        let mut metadata = HashMap::new();
        metadata.insert("level".to_string(), format!("{:?}", level));
        metadata.insert("error_count".to_string(), error_count.to_string());
        metadata.insert("warning_count".to_string(), warning_count.to_string());
        metadata.insert("compliant".to_string(), report.is_compliant().to_string());

        if error_count > 0 {
            let first_error = report
                .issues
                .iter()
                .find(|i| i.severity == pdf_compliance::Severity::Error)
                .map(|i| format!("[{}] {}", i.rule, i.message))
                .unwrap_or_default();
            metadata.insert("first_error".to_string(), first_error.clone());

            TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!("{}: {}", level.version_string(), first_error)),
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

/// Returns `true` if the PDF contains a `GTS_PDFX` OutputIntent marker.
fn claims_pdfx(pdf_data: &[u8]) -> bool {
    pdf_data.windows(8).any(|w| w == b"GTS_PDFX")
}

/// Detect PDF/X level from XMP `pdfxid:GTS_PDFXVersion` or raw bytes.
///
/// Falls back to `X4` (most permissive) when the version cannot be determined.
fn detect_pdfx_level(pdf_data: &[u8]) -> pdf_compliance::PdfXLevel {
    // Fast byte scan for common version strings.
    let data = pdf_data;

    if data
        .windows(14)
        .any(|w| w == b"PDF/X-1a:2003\"" || w == b"PDF/X-1a:2003 ")
        || data.windows(13).any(|w| w == b"PDF/X-1a:2003")
    {
        return pdf_compliance::PdfXLevel::X1a2003;
    }
    if data
        .windows(13)
        .any(|w| w == b"PDF/X-3:2003\"" || w == b"PDF/X-3:2003 " || w == b"PDF/X-3:2003\n")
        || data.windows(12).any(|w| w == b"PDF/X-3:2003")
    {
        return pdf_compliance::PdfXLevel::X32003;
    }
    // Default: PDF/X-4
    pdf_compliance::PdfXLevel::X4
}
