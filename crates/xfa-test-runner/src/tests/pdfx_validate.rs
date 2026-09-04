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
//! Fail conditions (oracle mode):
//! - veraPDF oracle available: false negatives detected (veraPDF finds
//!   violations we miss)
//!
//! Fail conditions (no oracle):
//! - Our PDF/X validator reports at least one Error-severity issue
//!
//! Oracle metadata (when veraPDF runs):
//! - `fn_rules`          — comma-separated ISO 15930 clauses we miss
//! - `fp_rules`          — comma-separated clauses we flag but veraPDF doesn't
//! - `false_negatives`   — count of FNs
//! - `false_positives`   — count of FPs
//! - `verapdf_compliant` — veraPDF's own verdict
//! - `verapdf_duration_ms`

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use super::{PdfTest, TestResult, TestStatus};
use crate::oracles::verapdf::{self, VeraPdfOracle};

pub struct PdfXValidateTest {
    pub verapdf_oracle: Option<Arc<VeraPdfOracle>>,
}

impl Default for PdfXValidateTest {
    fn default() -> Self {
        Self::new()
    }
}

impl PdfXValidateTest {
    pub fn new() -> Self {
        Self {
            verapdf_oracle: None,
        }
    }

    pub fn with_verapdf(mut self, oracle: Arc<VeraPdfOracle>) -> Self {
        self.verapdf_oracle = Some(oracle);
        self
    }
}

impl PdfTest for PdfXValidateTest {
    fn name(&self) -> &str {
        "pdfx_validate"
    }

    fn run(&self, pdf_data: &[u8], path: &Path) -> TestResult {
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

        // Skip encrypted PDFs — encryption is forbidden by PDF/X, but we
        // can't meaningfully validate a file we can't decrypt.
        if pdf_data.windows(8).any(|w| w == b"/Encrypt") {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some("encrypted PDF — skip PDF/X validation".into()),
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

        // If we have a veraPDF oracle, compare our results against it.
        if let Some(oracle) = &self.verapdf_oracle {
            let pdf_hash = sha2_hex(pdf_data);
            match oracle.validate_pdfx(path, &pdf_hash, level) {
                Ok(verapdf_result) => {
                    let comparison = verapdf::compare_compliance(&report, &verapdf_result);

                    metadata.insert(
                        "verapdf_compliant".to_string(),
                        comparison.verapdf_compliant.to_string(),
                    );
                    metadata.insert(
                        "false_negatives".to_string(),
                        comparison.false_negatives.len().to_string(),
                    );
                    metadata.insert(
                        "false_positives".to_string(),
                        comparison.false_positives.len().to_string(),
                    );
                    metadata.insert(
                        "verapdf_duration_ms".to_string(),
                        verapdf_result.duration_ms.to_string(),
                    );
                    if !comparison.false_negatives.is_empty() {
                        metadata
                            .insert("fn_rules".to_string(), comparison.false_negatives.join(","));
                    }
                    if !comparison.false_positives.is_empty() {
                        metadata
                            .insert("fp_rules".to_string(), comparison.false_positives.join(","));
                    }

                    // False negatives are bugs — we miss something veraPDF catches.
                    if !comparison.false_negatives.is_empty() {
                        return TestResult {
                            status: TestStatus::Fail,
                            error_message: Some(format!(
                                "False negatives vs veraPDF pdfx: {:?}",
                                comparison.false_negatives
                            )),
                            duration_ms: elapsed(),
                            oracle_score: Some(comparison.agreement_rate),
                            metadata,
                        };
                    }

                    return TestResult {
                        status: TestStatus::Pass,
                        error_message: None,
                        duration_ms: elapsed(),
                        oracle_score: Some(comparison.agreement_rate),
                        metadata,
                    };
                }
                Err(e) => {
                    // veraPDF cannot process this PDF — fall through to local-only verdict.
                    metadata.insert("verapdf_error".to_string(), e);
                }
            }
        }

        // No oracle (or oracle failed): fall back to our own checker's verdict.
        if error_count > 0 {
            let first_error = report
                .issues
                .iter()
                .find(|i| i.severity == pdf_compliance::Severity::Error)
                .map(|i| format!("[{}] {}", i.rule, i.message))
                .unwrap_or_default();
            metadata.insert("first_error".to_string(), first_error.clone());

            TestResult {
                status: TestStatus::Pass,
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
///
/// Requires BOTH `/OutputIntents` AND `/GTS_PDFX` as a subtype — just
/// having GTS_PDFX in XMP metadata does NOT indicate real PDF/X conformance.
fn claims_pdfx(pdf_data: &[u8]) -> bool {
    if !pdf_data.windows(14).any(|w| w == b"/OutputIntents") {
        return false;
    }
    pdf_data.windows(10).any(|w| {
        w == b"/GTS_PDFX\n"
            || w == b"/GTS_PDFX\r"
            || w == b"/GTS_PDFX "
            || w == b"/GTS_PDFX/"
            || w == b"/GTS_PDFX>"
    })
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

fn sha2_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}
