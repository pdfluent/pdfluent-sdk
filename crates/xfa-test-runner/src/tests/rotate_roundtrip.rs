use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

/// Corpus-level roundtrip test for page rotation.
///
/// Rotates every page of the input PDF by 90° (accumulating on any existing
/// /Rotate value), saves the result, reopens it, and verifies that each
/// page's /Rotate value increased by exactly 90° (mod 360).
pub struct RotateRoundtripTest;

impl PdfTest for RotateRoundtripTest {
    fn name(&self) -> &str {
        "rotate_roundtrip"
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
                error_message: Some("panic in rotate_roundtrip".into()),
                duration_ms: 0,
                oracle_score: None,
                metadata: HashMap::new(),
            },
            Err(_) => TestResult {
                status: TestStatus::Timeout,
                error_message: Some("rotate_roundtrip timed out (>25s)".into()),
                duration_ms: 30_000,
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
        Err(e) => {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some(format!("lopdf load failed: {e}")),
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
            error_message: Some("0 pages".into()),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    // Capture the /Rotate value for each page before rotation.
    let before: Vec<i64> = (1..=page_count as u32)
        .map(|p| get_page_rotate(&doc, p))
        .collect();

    // Rotate every page by 90°.
    for page_num in 1..=page_count as u32 {
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            pdf_manip::pages::rotate_page(&mut doc, page_num, 90)
        }));
        match r {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some(format!("rotate_page({page_num}) failed: {e}")),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
            Err(_) => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some(format!("rotate_page({page_num}) panicked")),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        }
    }

    // Verify /Rotate values after rotation (before serialization).
    for (i, &pre) in before.iter().enumerate() {
        let page_num = (i + 1) as u32;
        let post = get_page_rotate(&doc, page_num);
        let expected = (pre + 90) % 360;
        if post != expected {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!(
                    "page {page_num}: /Rotate before={pre} after={post} expected={expected}"
                )),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
    }

    // Save to bytes and reload — full roundtrip.
    let saved = match save_doc(&mut doc) {
        Ok(b) => b,
        Err(e) => {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!("save failed: {e}")),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
    };

    let reopened = match lopdf::Document::load_mem(&saved) {
        Ok(d) => d,
        Err(e) => {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!("roundtrip: lopdf reopen failed: {e}")),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
    };

    // Verify page count and /Rotate values are preserved after reload.
    let reopened_count = reopened.get_pages().len();
    if reopened_count != page_count {
        return TestResult {
            status: TestStatus::Fail,
            error_message: Some(format!(
                "roundtrip: page count changed {page_count} → {reopened_count}"
            )),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    for (i, &pre) in before.iter().enumerate() {
        let page_num = (i + 1) as u32;
        let post = get_page_rotate(&reopened, page_num);
        let expected = (pre + 90) % 360;
        if post != expected {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!(
                    "roundtrip page {page_num}: /Rotate expected={expected} got={post}"
                )),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
    }

    let mut metadata = HashMap::new();
    metadata.insert("pages".to_string(), page_count.to_string());
    metadata.insert("saved_bytes".to_string(), saved.len().to_string());

    TestResult {
        status: TestStatus::Pass,
        error_message: None,
        duration_ms: elapsed(),
        oracle_score: None,
        metadata,
    }
}

/// Read the /Rotate value for a page (0 if absent or not an integer).
fn get_page_rotate(doc: &lopdf::Document, page_num: u32) -> i64 {
    let pages = doc.get_pages();
    let page_id = match pages.get(&page_num) {
        Some(&id) => id,
        None => return 0,
    };
    if let Some(lopdf::Object::Dictionary(dict)) = doc.objects.get(&page_id) {
        if let Ok(lopdf::Object::Integer(n)) = dict.get(b"Rotate") {
            return *n;
        }
    }
    0
}

fn save_doc(doc: &mut lopdf::Document) -> Result<Vec<u8>, lopdf::Error> {
    let mut buf = Vec::new();
    doc.save_to(&mut buf)?;
    Ok(buf)
}
