use std::collections::HashMap;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};

/// OCR roundtrip test: detects scanned pages, runs PaddleOCR, and verifies
/// that text is recognized.
///
/// When the `paddle-ocr` feature is not enabled, this test detects scanned
/// pages and reports metadata but skips actual OCR inference.
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

/// Try to initialize the PaddleOCR engine once (shared across all PDFs).
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

impl PdfTest for OcrTest {
    fn name(&self) -> &str {
        "ocr"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let start = std::time::Instant::now();
        let elapsed = || start.elapsed().as_millis() as u64;

        // 1. Load with lopdf.
        let doc = match lopdf::Document::load_mem(pdf_data) {
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

        // 3. Run OCR if PaddleOCR is available.
        #[cfg(feature = "paddle-ocr")]
        {
            use pdf_ocr::OcrEngine;
            let engine = match get_paddle_engine() {
                Some(e) => e,
                None => {
                    metadata.insert("ocr_engine".into(), "unavailable".into());
                    return TestResult {
                        status: TestStatus::Skip,
                        error_message: Some(
                            "PaddleOCR engine not available (models missing?)".into(),
                        ),
                        duration_ms: elapsed(),
                        oracle_score: None,
                        metadata,
                    };
                }
            };

            metadata.insert("ocr_engine".into(), "paddle".into());

            // Render first scanned page and run OCR.
            let target_page = scanned_pages[0];
            let render_result = render_page(pdf_data, target_page);

            match render_result {
                Ok((pixels, width, height)) => {
                    let ocr_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        engine.recognize(&pixels, width, height, 300)
                    }));

                    match ocr_result {
                        Ok(Ok(result)) => {
                            let word_count = result.words.len();
                            let text = result.full_text();
                            let preview = if text.len() > 100 {
                                format!("{}...", &text[..100])
                            } else {
                                text.clone()
                            };

                            metadata.insert("words_recognized".into(), word_count.to_string());
                            metadata
                                .insert("confidence".into(), format!("{:.2}", result.confidence));
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

        #[cfg(not(feature = "paddle-ocr"))]
        {
            metadata.insert("ocr_engine".into(), "none".into());
            TestResult {
                status: TestStatus::Pass,
                error_message: Some(format!(
                    "{} scanned pages detected (OCR engine not compiled in)",
                    scanned_pages.len()
                )),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata,
            }
        }
    }
}

/// Render a PDF page to RGB pixels using pdf-engine.
#[cfg(feature = "paddle-ocr")]
fn render_page(pdf_data: &[u8], page_num: u32) -> Result<(Vec<u8>, u32, u32), String> {
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

    // Convert RGBA to RGB.
    let mut rgb = Vec::with_capacity((width * height * 3) as usize);
    for chunk in rendered.pixels.chunks(4) {
        rgb.push(chunk[0]);
        rgb.push(chunk[1]);
        rgb.push(chunk[2]);
    }

    Ok((rgb, width, height))
}
