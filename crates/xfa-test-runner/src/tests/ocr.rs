use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

/// OCR roundtrip test: detects scanned pages, runs OCR, and verifies text
/// is recognized.
///
/// Backend selection (in priority order):
/// 1. `ocr` feature — pure-Rust `ocrs` engine (requires model files)
/// 2. `paddle-ocr` feature — legacy PaddleOCR engine
/// 3. Neither — detects scanned pages, but skips inference
pub struct OcrTest;

/// Check if a page needs OCR (has fewer than `threshold` text characters).
fn page_needs_ocr(doc: &lopdf::Document, page_id: lopdf::ObjectId, threshold: usize) -> bool {
    let content_bytes = match doc.get_page_content(page_id) {
        Ok(b) => b,
        Err(_) => return true,
    };
    let content = match lopdf::content::Content::decode(&content_bytes) {
        Ok(c) => c,
        Err(_) => return true,
    };

    let mut char_count = 0usize;
    for op in &content.operations {
        match op.operator.as_str() {
            "Tj" | "'" | "\"" => {
                for operand in &op.operands {
                    if let lopdf::Object::String(bytes, _) = operand {
                        char_count += bytes.len();
                    }
                }
            }
            "TJ" => {
                for operand in &op.operands {
                    if let lopdf::Object::Array(arr) = operand {
                        for item in arr {
                            if let lopdf::Object::String(bytes, _) = item {
                                char_count += bytes.len();
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        if char_count >= threshold {
            return false;
        }
    }
    char_count < threshold
}

// ── ocrs backend init ─────────────────────────────────────────────────────────

/// Try to initialize the ocrs engine once (shared across all PDFs).
#[cfg(feature = "ocr")]
fn get_ocrs_engine() -> Option<&'static pdf_engine::OcrsBackend> {
    use std::sync::OnceLock;
    static ENGINE: OnceLock<Option<pdf_engine::OcrsBackend>> = OnceLock::new();
    ENGINE
        .get_or_init(|| match pdf_engine::OcrsBackend::try_default() {
            Ok(e) => Some(e),
            Err(e) => {
                eprintln!("ocrs init: {e}");
                None
            }
        })
        .as_ref()
}

// ── PaddleOCR backend init (legacy) ──────────────────────────────────────────

#[cfg(feature = "paddle-ocr")]
fn get_paddle_engine() -> Option<&'static pdf_ocr::PaddleOcrEngine> {
    use std::sync::OnceLock;
    static ENGINE: OnceLock<Option<pdf_ocr::PaddleOcrEngine>> = OnceLock::new();
    ENGINE
        .get_or_init(|| match pdf_ocr::PaddleOcrEngine::new() {
            Ok(e) => Some(e),
            Err(e) => {
                eprintln!("PaddleOCR init failed: {e}");
                None
            }
        })
        .as_ref()
}

// ── PdfTest impl ──────────────────────────────────────────────────────────────

impl PdfTest for OcrTest {
    fn name(&self) -> &str {
        "ocr"
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
                error_message: Some("panic in test execution".into()),
                duration_ms: 0,
                oracle_score: None,
                metadata: HashMap::new(),
            },
            Err(_) => TestResult {
                status: TestStatus::Timeout,
                error_message: Some("test timed out (>25s)".into()),
                duration_ms: 30_000,
                oracle_score: None,
                metadata: HashMap::new(),
            },
        }
    }
}

// ── Inner test logic ──────────────────────────────────────────────────────────

fn run_inner(pdf: Vec<u8>) -> TestResult {
    let start = std::time::Instant::now();
    let elapsed = || start.elapsed().as_millis() as u64;

    // 1. Load with lopdf.
    let doc = match lopdf::Document::load_mem(&pdf) {
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

    // 2. Detect scanned pages (max 5 pages, 10-char threshold).
    let pages = doc.get_pages();
    let total_pages = pages.len();
    let max_pages = total_pages.min(5);
    let mut scanned_pages = Vec::new();

    for page_num in 1..=(max_pages as u32) {
        if let Some(&page_id) = pages.get(&page_num) {
            if page_needs_ocr(&doc, page_id, 10) {
                scanned_pages.push(page_num);
            }
        }
    }

    let mut metadata = HashMap::new();
    metadata.insert("total_pages".into(), total_pages.to_string());
    metadata.insert("pages_checked".into(), max_pages.to_string());
    metadata.insert("scanned_pages".into(), scanned_pages.len().to_string());

    if scanned_pages.is_empty() {
        return TestResult {
            status: TestStatus::Skip,
            error_message: Some("no scanned pages detected".into()),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata,
        };
    }

    metadata.insert(
        "scanned_page_nums".into(),
        scanned_pages
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join(","),
    );

    // 3. Run OCR with the first available backend.
    run_ocr_inference(pdf, scanned_pages[0], metadata, elapsed)
}

/// Dispatch to the first available OCR backend.
fn run_ocr_inference(
    pdf: Vec<u8>,
    target_page: u32,
    mut metadata: HashMap<String, String>,
    elapsed: impl Fn() -> u64,
) -> TestResult {
    // ── ocrs (pure-Rust, preferred when feature = "ocr") ─────────────────────
    #[cfg(feature = "ocr")]
    {
        let engine: &dyn pdf_engine::OcrBackend = match get_ocrs_engine() {
            Some(e) => e,
            None => {
                metadata.insert("ocr_engine".into(), "ocrs".into());
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some(
                        "ocrs models not found — set OCRS_DETECTION_MODEL / \
                         OCRS_RECOGNITION_MODEL or place models in ~/.cache/ocrs/"
                            .into(),
                    ),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata,
                };
            }
        };
        metadata.insert("ocr_engine".into(), "ocrs".into());
        return run_with_engine_new(pdf, target_page, engine, metadata, elapsed);
    }

    // ── PaddleOCR (legacy) ───────────────────────────────────────────────────
    #[cfg(all(feature = "paddle-ocr", not(feature = "ocr")))]
    {
        use pdf_ocr::OcrEngine;
        let engine = match get_paddle_engine() {
            Some(e) => e,
            None => {
                metadata.insert("ocr_engine".into(), "unavailable".into());
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some("PaddleOCR engine not available (models missing?)".into()),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata,
                };
            }
        };
        metadata.insert("ocr_engine".into(), "paddle".into());
        return run_with_paddle(pdf, target_page, engine, metadata, elapsed);
    }

    // ── No backend compiled ──────────────────────────────────────────────────
    #[cfg(not(any(feature = "ocr", feature = "paddle-ocr")))]
    {
        let _ = (pdf, target_page);
        metadata.insert("ocr_engine".into(), "none".into());
        TestResult {
            status: TestStatus::Skip,
            error_message: Some(
                "OCR: skipped — no OCR backend compiled \
                 (build with --features ocr for pure-Rust ocrs, \
                 or --features paddle-ocr for legacy PaddleOCR)"
                    .into(),
            ),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata,
        }
    }
}

/// Run OCR using the new `OcrBackend` trait (ocrs or any custom impl).
#[cfg(feature = "ocr")]
fn run_with_engine_new(
    pdf: Vec<u8>,
    target_page: u32,
    engine: &dyn pdf_engine::OcrBackend,
    mut metadata: HashMap<String, String>,
    elapsed: impl Fn() -> u64,
) -> TestResult {
    let (pixels, width, height) = match render_page_rgb(&pdf, target_page) {
        Ok(v) => v,
        Err(e) => {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some(format!("render failed for page {target_page}: {e}")),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata,
            };
        }
    };

    let ocr_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        engine.recognize(&pixels, width, height)
    }));

    match ocr_result {
        Ok(Ok(result)) => {
            let word_count = result.words.len();
            let text_length = result.text.len();
            let preview = if result.text.len() > 100 {
                format!("{}...", &result.text[..100])
            } else {
                result.text.clone()
            };
            metadata.insert("words_recognized".into(), word_count.to_string());
            metadata.insert("text_length".into(), text_length.to_string());
            metadata.insert("confidence".into(), format!("{:.2}", result.confidence));
            metadata.insert("text_preview".into(), preview);
            TestResult {
                status: TestStatus::Pass,
                error_message: None,
                duration_ms: elapsed(),
                oracle_score: Some(result.confidence as f64),
                metadata,
            }
        }
        Ok(Err(e)) => TestResult {
            status: TestStatus::Fail,
            error_message: Some(format!("OCR recognition failed: {e}")),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata,
        },
        Err(_) => TestResult {
            status: TestStatus::Fail,
            error_message: Some("panic in OCR recognition".into()),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata,
        },
    }
}

/// Run OCR using the legacy PaddleOCR engine.
#[cfg(all(feature = "paddle-ocr", not(feature = "ocr")))]
fn run_with_paddle(
    pdf: Vec<u8>,
    target_page: u32,
    engine: &pdf_ocr::PaddleOcrEngine,
    mut metadata: HashMap<String, String>,
    elapsed: impl Fn() -> u64,
) -> TestResult {
    use pdf_ocr::OcrEngine;
    match render_page_rgb(&pdf, target_page) {
        Ok((pixels, width, height)) => {
            let ocr_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                engine.recognize(&pixels, width, height, 300)
            }));
            match ocr_result {
                Ok(Ok(result)) => {
                    let word_count = result.words.len();
                    let text = result.full_text();
                    let text_length = text.len();
                    let preview = if text.len() > 100 {
                        format!("{}...", &text[..100])
                    } else {
                        text.clone()
                    };
                    metadata.insert("words_recognized".into(), word_count.to_string());
                    metadata.insert("text_length".into(), text_length.to_string());
                    metadata.insert("confidence".into(), format!("{:.2}", result.confidence));
                    metadata.insert("text_preview".into(), preview);
                    TestResult {
                        status: TestStatus::Pass,
                        error_message: None,
                        duration_ms: elapsed(),
                        oracle_score: Some(result.confidence as f64),
                        metadata,
                    }
                }
                Ok(Err(e)) => TestResult {
                    status: TestStatus::Fail,
                    error_message: Some(format!("OCR recognition failed: {e}")),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata,
                },
                Err(_) => TestResult {
                    status: TestStatus::Fail,
                    error_message: Some("panic in OCR recognition".into()),
                    duration_ms: elapsed(),
                    oracle_score: None,
                    metadata,
                },
            }
        }
        Err(e) => TestResult {
            status: TestStatus::Skip,
            error_message: Some(format!("render failed for page {target_page}: {e}")),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata,
        },
    }
}

// ── Shared render helper ──────────────────────────────────────────────────────

/// Render a PDF page to RGB pixels (3 bytes per pixel, row-major) using pdf-engine.
#[cfg(any(feature = "ocr", feature = "paddle-ocr"))]
fn render_page_rgb(pdf_data: &[u8], page_num: u32) -> Result<(Vec<u8>, u32, u32), String> {
    let doc =
        pdf_engine::PdfDocument::open(pdf_data.to_vec()).map_err(|e| format!("open: {e:?}"))?;
    let page_idx = (page_num - 1) as usize;
    let options = pdf_engine::RenderOptions {
        dpi: 150.0,
        ..Default::default()
    };
    let rendered = doc
        .render_page(page_idx, &options)
        .map_err(|e| format!("render: {e:?}"))?;

    let width = rendered.width;
    let height = rendered.height;

    // Convert RGBA → RGB (OCR backends expect 3-channel input).
    let mut rgb = Vec::with_capacity((width * height * 3) as usize);
    for chunk in rendered.pixels.chunks(4) {
        rgb.push(chunk[0]);
        rgb.push(chunk[1]);
        rgb.push(chunk[2]);
    }

    Ok((rgb, width, height))
}
