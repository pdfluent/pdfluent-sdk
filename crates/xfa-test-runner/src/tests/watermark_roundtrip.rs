use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

/// Corpus-level roundtrip test for text watermarking.
///
/// Applies a "DRAFT" text watermark to all pages, saves the result, reopens
/// it, and verifies that:
/// - The page count is unchanged.
/// - At least one page content stream references the watermark font "F_WM"
///   (written by `apply_text_watermark`).
pub struct WatermarkRoundtripTest;

impl PdfTest for WatermarkRoundtripTest {
    fn name(&self) -> &str {
        "watermark_roundtrip"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let pdf_owned = pdf_data.to_vec();
        let (tx, rx) = std::sync::mpsc::channel();
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
                status: TestStatus::Fail,
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
                error_message: Some("panic in watermark_roundtrip".into()),
                duration_ms: 0,
                oracle_score: None,
                metadata: HashMap::new(),
            },
            Err(_) => TestResult {
                status: TestStatus::Timeout,
                error_message: Some("watermark_roundtrip timed out (>25s)".into()),
                duration_ms: 30_000,
                oracle_score: None,
                metadata: HashMap::new(),
            },
        }
    }
}

fn run_inner(pdf: Vec<u8>) -> TestResult {
    use pdf_manip::watermark::{
        apply_text_watermark, Layer, PageSelection, Position, TextWatermark,
    };

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

    if super::has_malformed_page_tree(&pdf) {
        return TestResult {
            status: TestStatus::Skip,
            error_message: Some("malformed page tree (duplicate/looping refs)".into()),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    let original_pages = doc.get_pages().len();
    if original_pages == 0 {
        return TestResult {
            status: TestStatus::Skip,
            error_message: Some("0 pages".into()),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    let wm = TextWatermark {
        text: "DRAFT".into(),
        font_size: 60.0,
        rotation: 45.0,
        opacity: 0.3,
        color: pdf_manip::watermark::Color::Gray(0.5),
        position: Position::Center,
        layer: Layer::Foreground,
    };

    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        apply_text_watermark(&mut doc, &wm, &PageSelection::All)
    }));

    match r {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!("apply_text_watermark failed: {e}")),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
        Err(_) => {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some("apply_text_watermark panicked".into()),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
    }

    // Save to bytes.
    let saved = match (|| -> Result<Vec<u8>, lopdf::Error> {
        let mut buf = Vec::new();
        doc.save_to(&mut buf)?;
        Ok(buf)
    })() {
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

    // Reload and verify page count.
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

    let reopened_pages = reopened.get_pages().len();
    if reopened_pages != original_pages {
        return TestResult {
            status: TestStatus::Fail,
            error_message: Some(format!(
                "page count changed: {original_pages} → {reopened_pages}"
            )),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    // Verify the watermark font name "F_WM" appears in the saved bytes.
    // apply_text_watermark always writes a Font resource named "F_WM".
    let watermark_present = saved.windows(4).any(|w| w == b"F_WM");
    if !watermark_present {
        return TestResult {
            status: TestStatus::Fail,
            error_message: Some("watermark font 'F_WM' not found in saved PDF bytes".into()),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    let mut metadata = HashMap::new();
    metadata.insert("pages".to_string(), original_pages.to_string());
    metadata.insert("saved_bytes".to_string(), saved.len().to_string());

    TestResult {
        status: TestStatus::Pass,
        error_message: None,
        duration_ms: elapsed(),
        oracle_score: None,
        metadata,
    }
}
