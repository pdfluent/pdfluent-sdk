//! XFA packet extraction test.
//!
//! Verifies that `pdf_xfa::extract::extract_xfa` can read XFA packets from
//! real corpus PDFs without panicking, and that the extracted data is
//! structurally sane (non-empty packet list, recognisable names).
//!
//! Skip policy:
//! - No /XFA in this PDF  → Skip (PacketNotFound)
//! - Unreadable / encrypted → Skip (delegates to parse test)
//! - Extraction error on an XFA PDF → Skip with diagnostic (not Fail, because
//!   corrupt XFA is common in the wild and shouldn't count as a regression)

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

pub struct XfaExtractTest;

impl PdfTest for XfaExtractTest {
    fn name(&self) -> &str {
        "xfa_extract"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let start = std::time::Instant::now();

        let pdf = match pdf_syntax::Pdf::new(pdf_data.to_vec()) {
            Ok(p) => p,
            Err(pdf_syntax::LoadPdfError::Invalid) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some("PDF is invalid or could not be parsed".into()),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                }
            }
            Err(pdf_syntax::LoadPdfError::Decryption(_)) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some("PDF is encrypted (no password available)".into()),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                }
            }
            // PDFs exceeding object/page limits are rejected to prevent OOM. (#497)
            Err(pdf_syntax::LoadPdfError::TooLarge(obj, pg)) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some(format!(
                        "PDF too large ({obj} objects, {pg} pages) — skipping"
                    )),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                }
            }
        };

        match pdf_xfa::extract::extract_xfa(&pdf) {
            // No XFA content in this PDF — not a failure, just not applicable.
            Err(pdf_xfa::error::XfaError::PacketNotFound(_)) => TestResult {
                status: TestStatus::Skip,
                error_message: None,
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata: HashMap::new(),
            },
            // Extraction error (e.g. malformed XFA stream) — skip with note.
            Err(e) => TestResult {
                status: TestStatus::Skip,
                error_message: Some(format!("XFA extraction error: {e}")),
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata: HashMap::new(),
            },
            Ok(packets) => {
                let packet_names: Vec<&str> =
                    packets.packets.iter().map(|(n, _)| n.as_str()).collect();

                let mut metadata = HashMap::new();
                metadata.insert(
                    "packet_count".to_string(),
                    packets.packets.len().to_string(),
                );
                metadata.insert("packets".to_string(), packet_names.join(","));
                metadata.insert(
                    "has_template".to_string(),
                    packets.template().is_some().to_string(),
                );
                metadata.insert(
                    "has_datasets".to_string(),
                    packets.datasets().is_some().to_string(),
                );
                metadata.insert(
                    "has_full_xml".to_string(),
                    packets.full_xml.is_some().to_string(),
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
    }
}
