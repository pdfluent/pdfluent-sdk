//! ZUGFeRD / Factur-X EN 16931 validation corpus test.
//!
//! For PDFs that contain a ZUGFeRD XML attachment, parses the invoice and
//! validates it against EN 16931 business rules using `pdf_invoice::validate_invoice`.
//!
//! Skip policy:
//! - lopdf cannot load the PDF → Skip
//! - No ZUGFeRD attachment found → Skip
//! - XML parsing fails (malformed invoice) → Skip (tested by zugferd_roundtrip)
//!
//! Fail conditions:
//! - EN 16931 validation returns errors (Severity::Error)
//!
//! Pass with metadata:
//! - Validation passes (0 errors), reports warning count and profile

use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

const ZUGFERD_FILENAMES: &[&str] = &["factur-x.xml", "ZUGFeRD-invoice.xml", "xrechnung.xml"];

pub struct ZugferdValidateTest;

impl PdfTest for ZugferdValidateTest {
    fn name(&self) -> &str {
        "zugferd_validate"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let start = std::time::Instant::now();
        let elapsed = || start.elapsed().as_millis() as u64;

        let doc = match lopdf::Document::load_mem(pdf_data) {
            Ok(d) => d,
            Err(_) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some("lopdf could not load PDF".into()),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        // Try to find and extract a ZUGFeRD XML attachment.
        let mut xml_bytes = None;
        let mut found_filename = "";
        for &filename in ZUGFERD_FILENAMES {
            match pdf_invoice::embed::extract_xml_attachment(&doc, filename) {
                Ok(Some(bytes)) if !bytes.is_empty() => {
                    xml_bytes = Some(bytes);
                    found_filename = filename;
                    break;
                }
                _ => continue,
            }
        }

        let Some(xml) = xml_bytes else {
            return TestResult {
                status: TestStatus::Skip,
                error_message: None,
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        };

        // Parse the XML into a ZugferdInvoice.
        let xml_str = match std::str::from_utf8(&xml) {
            Ok(s) => s,
            Err(_) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some("ZUGFeRD XML is not valid UTF-8".into()),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        let invoice = match pdf_invoice::zugferd::ZugferdInvoice::from_xml(xml_str) {
            Ok(inv) => inv,
            Err(_) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some("Could not parse ZUGFeRD XML into invoice model".into()),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        // Validate against EN 16931 business rules.
        let report = pdf_invoice::validate_invoice(&invoice);

        let mut metadata = HashMap::new();
        metadata.insert("filename".to_string(), found_filename.to_string());
        metadata.insert("errors".to_string(), report.error_count().to_string());
        metadata.insert("warnings".to_string(), report.warning_count().to_string());
        metadata.insert("profile".to_string(), format!("{:?}", invoice.profile));

        if report.is_valid() {
            TestResult {
                status: TestStatus::Pass,
                error_message: None,
                duration_ms: elapsed(),
                oracle_score: None,
                metadata,
            }
        } else {
            let error_rules: Vec<String> = report
                .issues
                .iter()
                .filter(|i| i.severity == pdf_invoice::Severity::Error)
                .map(|i| format!("{}: {}", i.rule, i.message))
                .collect();
            TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!(
                    "{} EN 16931 errors: {}",
                    report.error_count(),
                    error_rules.join("; ")
                )),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata,
            }
        }
    }
}
