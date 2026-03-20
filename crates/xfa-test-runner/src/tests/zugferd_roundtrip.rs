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
const ZUGFERD_V1_NS_PREFIX: &[u8] = b"urn:ferd:pdfa:CrossIndustryDocument:invoice:";

pub struct ZugferdRoundtripTest;

impl PdfTest for ZugferdRoundtripTest {
    fn name(&self) -> &str {
        "zugferd_roundtrip"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let pdf_owned = pdf_data.to_vec();
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(move || {
                let r =
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_inner(pdf_owned)));
                let _ = tx.send(r);
            })
            .expect("thread spawn");
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
fn verify_xml(xml_bytes: Vec<u8>, filename: &str, duration_ms: u64) -> TestResult {
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
