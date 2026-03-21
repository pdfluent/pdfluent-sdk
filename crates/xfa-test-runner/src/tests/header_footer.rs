//! Header/footer addition corpus test.
//!
//! Adds a header and footer to each corpus PDF and verifies the result is
//! loadable and has the same page count.
//!
//! Skip policy:
//! - lopdf cannot load the PDF → Skip
//! - PDF has 0 pages → Skip
//!
//! Fail conditions:
//! - add_header or add_footer returns an error
//! - Saved result cannot be reloaded
//! - Page count changed after modification

use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

pub struct HeaderFooterTest;

impl PdfTest for HeaderFooterTest {
    fn name(&self) -> &str {
        "header_footer"
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
                error_message: Some("panic in header/footer test".into()),
                duration_ms: 0,
                oracle_score: None,
                metadata: HashMap::new(),
            },
            Err(_) => TestResult {
                status: TestStatus::Timeout,
                error_message: Some("header/footer test timed out (>25s)".into()),
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

    let mut doc = match lopdf::Document::load_mem(&pdf) {
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

    let page_count = doc.get_pages().len();
    if page_count == 0 {
        return TestResult {
            status: TestStatus::Skip,
            error_message: Some("PDF has 0 pages".into()),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    let selection = pdf_manip::watermark::PageSelection::All;

    // Add header
    let header = pdf_manip::header_footer::HeaderFooter {
        center: Some("Header Test".to_string()),
        ..Default::default()
    };
    if let Err(e) = pdf_manip::header_footer::add_header(&mut doc, &header, &selection) {
        return TestResult {
            status: TestStatus::Fail,
            error_message: Some(format!("add_header failed: {e}")),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    // Add footer
    let footer = pdf_manip::header_footer::HeaderFooter {
        left: Some("Page {{page}}".to_string()),
        right: Some("{{total}} pages".to_string()),
        ..Default::default()
    };
    if let Err(e) = pdf_manip::header_footer::add_footer(&mut doc, &footer, &selection) {
        return TestResult {
            status: TestStatus::Fail,
            error_message: Some(format!("add_footer failed: {e}")),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    // Save and reload to verify structural integrity
    let mut out = Vec::new();
    if let Err(e) = doc.save_to(&mut out) {
        return TestResult {
            status: TestStatus::Fail,
            error_message: Some(format!("save after header/footer failed: {e}")),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    let reloaded = match lopdf::Document::load_mem(&out) {
        Ok(d) => d,
        Err(e) => {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!("reload after header/footer failed: {e}")),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
    };

    let new_count = reloaded.get_pages().len();
    if new_count != page_count {
        return TestResult {
            status: TestStatus::Fail,
            error_message: Some(format!("page count changed: {page_count} → {new_count}")),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    let mut metadata = HashMap::new();
    metadata.insert("pages".to_string(), page_count.to_string());
    metadata.insert("output_size".to_string(), out.len().to_string());

    TestResult {
        status: TestStatus::Pass,
        error_message: None,
        duration_ms: elapsed(),
        oracle_score: None,
        metadata,
    }
}
