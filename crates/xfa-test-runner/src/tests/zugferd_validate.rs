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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::zugferd_roundtrip::{make_invoice_for_profile, make_minimal_lopdf_pdf};

    fn pdf_bytes_with_invoice(profile: pdf_invoice::zugferd::ZugferdProfile) -> Vec<u8> {
        let invoice = make_invoice_for_profile(profile);
        let xml = invoice.to_xml().expect("to_xml");
        let mut doc = make_minimal_lopdf_pdf();
        pdf_invoice::embed::embed_xml_attachment(
            &mut doc,
            "factur-x.xml",
            xml.as_bytes(),
            pdf_invoice::embed::AfRelationship::Data,
        )
        .expect("embed");
        let mut out = Vec::new();
        doc.save_to(&mut out).expect("save");
        out
    }

    macro_rules! validate_test {
        ($name:ident, $profile:expr) => {
            #[test]
            fn $name() {
                let pdf = pdf_bytes_with_invoice($profile);
                let test = ZugferdValidateTest;
                let result = PdfTest::run(&test, &pdf, std::path::Path::new("synthetic"));
                // Minimum and BasicWL are not subject to full EN 16931 arithmetic
                // rules, so validation reports no errors; other profiles need
                // correct totals which we provide.
                assert!(
                    result.status == TestStatus::Pass || result.status == TestStatus::Skip,
                    "profile {:?}: expected Pass or Skip, got {:?}: {:?}",
                    $profile,
                    result.status,
                    result.error_message
                );
            }
        };
    }

    validate_test!(
        validate_minimum,
        pdf_invoice::zugferd::ZugferdProfile::Minimum
    );
    validate_test!(
        validate_basicwl,
        pdf_invoice::zugferd::ZugferdProfile::BasicWL
    );
    validate_test!(validate_basic, pdf_invoice::zugferd::ZugferdProfile::Basic);
    validate_test!(
        validate_en16931,
        pdf_invoice::zugferd::ZugferdProfile::EN16931
    );
    validate_test!(
        validate_extended,
        pdf_invoice::zugferd::ZugferdProfile::Extended
    );
}
