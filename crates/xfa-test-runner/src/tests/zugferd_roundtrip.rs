//! ZUGFeRD / Factur-X roundtrip corpus test.
//!
//! Attempts to extract a ZUGFeRD / Factur-X XML attachment from each corpus PDF
//! using the known attachment filenames (ZUGFeRD 1.0, ZUGFeRD 2.x / Factur-X,
//! XRechnung).  If an attachment is found, the XML structure is validated:
//!
//! - Non-empty bytes
//! - Starts with an XML declaration or CII root element
//! - Contains the UN/CEFACT CII namespace (ZUGFeRD 2.x / Factur-X) or the
//!   legacy FERD namespace (ZUGFeRD 1.0)
//!
//! Skip policy:
//! - lopdf cannot load the PDF → Skip
//! - No ZUGFeRD attachment under any known filename → Skip
//!
//! Fail conditions:
//! - Embedded file is empty
//! - Content is not recognisable as XML
//! - Neither CII nor ZUGFeRD v1 namespace found (not ZUGFeRD-conformant)

use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

/// Known attachment filenames for ZUGFeRD / Factur-X / XRechnung.
const ZUGFERD_FILENAMES: &[&str] = &[
    "factur-x.xml",        // ZUGFeRD 2.x / Factur-X 1.0
    "ZUGFeRD-invoice.xml", // ZUGFeRD 1.0
    "xrechnung.xml",       // XRechnung (DE national profile)
];

/// CII namespace present in ZUGFeRD 2.x / Factur-X.
const CII_NS_PREFIX: &[u8] = b"urn:un:unece:uncefact:data:standard:CrossIndustryInvoice";
/// ZUGFeRD 1.0 used a different namespace (FERD schema, pre-CII alignment).
/// The XMP extension schema uses urn:ferd:pdfa:..., but the embedded XML data
/// uses urn:ferd:CrossIndustryDocument:invoice:1p0 (no "pdfa" segment). (#507)
const ZUGFERD_V1_NS_PREFIX: &[u8] = b"urn:ferd:CrossIndustryDocument:invoice:";

pub struct ZugferdRoundtripTest;

impl PdfTest for ZugferdRoundtripTest {
    fn name(&self) -> &str {
        "zugferd_roundtrip"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let pdf_owned = pdf_data.to_vec();
        let (tx, rx) = std::sync::mpsc::channel();
        // Spawn may fail transiently (EAGAIN) under high thread load — treat as skip.
        if std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || {
                let r =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_inner(pdf_owned)));
                let _ = tx.send(r);
            })
            .is_err()
        {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some("thread spawn failed (resource limit)".into()),
                duration_ms: 0,
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
        match rx.recv_timeout(std::time::Duration::from_secs(25)) {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => TestResult {
                status: TestStatus::Crash,
                error_message: Some("panic in ZUGFeRD extraction".into()),
                duration_ms: 0,
                oracle_score: None,
                metadata: HashMap::new(),
            },
            Err(_) => TestResult {
                status: TestStatus::Timeout,
                error_message: Some("ZUGFeRD extraction timed out (>25s)".into()),
                duration_ms: 25_000,
                oracle_score: None,
                metadata: HashMap::new(),
            },
        }
    }
}

fn run_inner(pdf: Vec<u8>) -> TestResult {
    let start = std::time::Instant::now();
    let elapsed = || start.elapsed().as_millis() as u64;

    let doc = match lopdf::Document::load_mem(&pdf) {
        Ok(d) => d,
        Err(_) => {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some("lopdf could not load PDF".into()),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            }
        }
    };

    // Try each known ZUGFeRD filename in priority order.
    for &filename in ZUGFERD_FILENAMES {
        match pdf_invoice::embed::extract_xml_attachment(&doc, filename) {
            Ok(Some(xml_bytes)) => {
                return verify_xml(xml_bytes, filename, elapsed());
            }
            Ok(None) => continue, // Not found under this name — try the next.
            Err(_) => continue,   // Extraction error (malformed structure) — try next.
        }
    }

    // No ZUGFeRD attachment found — skip.
    TestResult {
        status: TestStatus::Skip,
        error_message: None,
        duration_ms: elapsed(),
        oracle_score: None,
        metadata: HashMap::new(),
    }
}

/// Verify that the extracted bytes are valid ZUGFeRD XML.
pub(crate) fn verify_xml(xml_bytes: Vec<u8>, filename: &str, duration_ms: u64) -> TestResult {
    if xml_bytes.is_empty() {
        return TestResult {
            status: TestStatus::Fail,
            error_message: Some(format!("{filename}: embedded file is empty")),
            duration_ms,
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    // Strip leading whitespace / BOM before checking the signature.
    let trimmed = xml_bytes
        .iter()
        .position(|&b| !b.is_ascii_whitespace() && b != 0xEF && b != 0xBB && b != 0xBF)
        .map(|i| &xml_bytes[i..])
        .unwrap_or(&xml_bytes);

    let is_xml = trimmed.starts_with(b"<?xml")
        || trimmed.starts_with(b"<rsm:")
        || trimmed.starts_with(b"<CrossIndustryInvoice");

    if !is_xml {
        return TestResult {
            status: TestStatus::Fail,
            error_message: Some(format!("{filename}: embedded content is not XML")),
            duration_ms,
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    // ZUGFeRD 2.x / Factur-X uses the UN/CEFACT CII namespace.
    // ZUGFeRD 1.0 used urn:ferd:pdfa:CrossIndustryDocument:invoice:... (pre-CII alignment).
    // Both are valid ZUGFeRD XML; accept either namespace.
    let has_known_ns = xml_bytes
        .windows(CII_NS_PREFIX.len())
        .any(|w| w == CII_NS_PREFIX)
        || xml_bytes
            .windows(ZUGFERD_V1_NS_PREFIX.len())
            .any(|w| w == ZUGFERD_V1_NS_PREFIX);

    if !has_known_ns {
        return TestResult {
            status: TestStatus::Fail,
            error_message: Some(format!(
                "{filename}: CII namespace absent — not ZUGFeRD-conformant"
            )),
            duration_ms,
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    let mut metadata = HashMap::new();
    metadata.insert("filename".to_string(), filename.to_string());
    metadata.insert("xml_size_bytes".to_string(), xml_bytes.len().to_string());

    TestResult {
        status: TestStatus::Pass,
        error_message: None,
        duration_ms,
        oracle_score: None,
        metadata,
    }
}

// ── Test helpers shared with zugferd_validate tests ──────────────────────────

/// Build a minimal valid lopdf Document (no pages, just Catalog + Pages node).
#[cfg(test)]
pub(crate) fn make_minimal_lopdf_pdf() -> lopdf::Document {
    use lopdf::{Dictionary, Object};
    let mut doc = lopdf::Document::with_version("1.7");
    let pages_id = doc.add_object(Dictionary::from_iter(vec![
        ("Type", Object::Name(b"Pages".to_vec())),
        ("Kids", Object::Array(vec![])),
        ("Count", Object::Integer(0)),
    ]));
    let catalog_id = doc.add_object(Dictionary::from_iter(vec![
        ("Type", Object::Name(b"Catalog".to_vec())),
        ("Pages", Object::Reference(pages_id)),
    ]));
    doc.trailer.set("Root", Object::Reference(catalog_id));
    doc
}

/// Build a minimal valid ZugferdInvoice for the given profile.
///
/// Totals are intentionally small (100 + 21% VAT = 121) and consistent so
/// that EN 16931 arithmetic rules (BR-CO-10/11/13/15) pass.
#[cfg(test)]
pub(crate) fn make_invoice_for_profile(
    profile: pdf_invoice::zugferd::ZugferdProfile,
) -> pdf_invoice::zugferd::ZugferdInvoice {
    use chrono::NaiveDate;
    use pdf_invoice::zugferd::*;

    let needs_line_items = profile.requires_line_items();
    let needs_tax_id = matches!(profile, ZugferdProfile::EN16931 | ZugferdProfile::Extended);
    let issue_date = NaiveDate::from_ymd_opt(2026, 1, 15).unwrap();

    let line_items = if needs_line_items {
        vec![LineItem {
            id: "1".into(),
            description: "Test service".into(),
            quantity: 1.0,
            unit_code: "C62".into(),
            unit_price: 100.0,
            line_total: 100.0,
            tax_rate: 21.0,
            tax_category: TaxCategory::Standard,
        }]
    } else {
        vec![]
    };

    ZugferdInvoice {
        profile,
        invoice_number: format!("TEST-{profile:?}"),
        type_code: "380".into(),
        issue_date,
        seller: TradeParty {
            name: "Test Seller B.V.".into(),
            address: Address {
                street: Some("Teststraat 1".into()),
                city: Some("Amsterdam".into()),
                postal_code: Some("1000 AA".into()),
                country_code: "NL".into(),
            },
            tax_id: if needs_tax_id {
                Some("NL123456789B01".into())
            } else {
                None
            },
            registration_id: None,
            email: None,
        },
        buyer: TradeParty {
            name: "Test Buyer GmbH".into(),
            address: Address {
                street: None,
                city: Some("Berlin".into()),
                postal_code: None,
                country_code: "DE".into(),
            },
            tax_id: None,
            registration_id: None,
            email: None,
        },
        line_items,
        currency: "EUR".into(),
        tax_basis_total: 100.0,
        tax_total: 21.0,
        grand_total: 121.0,
        due_payable: 121.0,
        payment_terms: None,
        buyer_reference: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    macro_rules! roundtrip_test {
        ($name:ident, $profile:expr) => {
            #[test]
            fn $name() {
                let pdf = pdf_bytes_with_invoice($profile);
                let doc = lopdf::Document::load_mem(&pdf).unwrap();
                let xml = pdf_invoice::embed::extract_xml_attachment(&doc, "factur-x.xml")
                    .expect("extract")
                    .expect("should have attachment");
                let result = verify_xml(xml, "factur-x.xml", 0);
                assert_eq!(
                    result.status,
                    TestStatus::Pass,
                    "profile {:?}: {:?}",
                    $profile,
                    result.error_message
                );
            }
        };
    }

    roundtrip_test!(
        roundtrip_minimum,
        pdf_invoice::zugferd::ZugferdProfile::Minimum
    );
    roundtrip_test!(
        roundtrip_basicwl,
        pdf_invoice::zugferd::ZugferdProfile::BasicWL
    );
    roundtrip_test!(roundtrip_basic, pdf_invoice::zugferd::ZugferdProfile::Basic);
    roundtrip_test!(
        roundtrip_en16931,
        pdf_invoice::zugferd::ZugferdProfile::EN16931
    );
    roundtrip_test!(
        roundtrip_extended,
        pdf_invoice::zugferd::ZugferdProfile::Extended
    );
}
