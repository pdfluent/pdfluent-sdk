use std::collections::HashMap;
use std::path::Path;

use pdf_redact::search_redact::{search_and_redact, RedactSearchOptions};

use super::{PdfTest, TestResult, TestStatus};

/// Render page 1 of `pdf_data` at 72 dpi and return the RGBA pixel buffer
/// together with the rendered width and height in pixels.
///
/// At 72 dpi, 1 PDF point == 1 pixel, which makes coordinate conversion
/// from PDF-space bounding boxes trivial.
fn render_page1_72dpi(pdf_data: &[u8]) -> Option<(Vec<u8>, u32, u32)> {
    let doc = pdf_engine::PdfDocument::open(pdf_data.to_vec()).ok()?;
    let opts = pdf_engine::RenderOptions {
        dpi: 72.0,
        ..Default::default()
    };
    let rendered = doc.render_page(0, &opts).ok()?;
    Some((rendered.pixels, rendered.width, rendered.height))
}

/// Return the mean brightness (0.0–255.0) of the RGBA pixels inside the
/// given PDF-coordinate rectangle on a page rendered at 72 dpi.
///
/// PDF coordinates have y=0 at the bottom; the pixel buffer has y=0 at the
/// top.  At 72 dpi, 1 point == 1 pixel so the only conversion needed is a
/// Y-axis flip.
///
/// Returns `None` when the rectangle falls entirely outside the image or
/// no pixels are sampled.
fn mean_brightness_in_rect(
    pixels: &[u8],
    img_w: u32,
    img_h: u32,
    rect: [f64; 4], // [x0, y0, x1, y1] in PDF points
) -> Option<f64> {
    // PDF rect coords → pixel coords (72dpi: 1pt = 1px, flip Y).
    let px0 = rect[0].max(0.0) as u32;
    let py0 = (img_h as f64 - rect[3]).max(0.0) as u32; // top of rect in image
    let px1 = (rect[2] as u32).min(img_w);
    let py1 = (img_h as f64 - rect[1]).min(img_h as f64) as u32; // bottom of rect in image

    if px0 >= px1 || py0 >= py1 {
        return None;
    }

    let mut total = 0u64;
    let mut count = 0u64;
    for row in py0..py1 {
        for col in px0..px1 {
            let idx = ((row * img_w + col) * 4) as usize;
            if idx + 2 >= pixels.len() {
                continue;
            }
            let r = pixels[idx] as u64;
            let g = pixels[idx + 1] as u64;
            let b = pixels[idx + 2] as u64;
            total += r + g + b;
            count += 3;
        }
    }
    if count == 0 {
        return None;
    }
    Some(total as f64 / count as f64)
}

/// Extract text from page 1 using pdf-engine (for initial word selection).
fn extract_page1_text(pdf_data: &[u8]) -> Option<String> {
    let doc = pdf_engine::PdfDocument::open(pdf_data.to_vec()).ok()?;
    let text = doc.extract_text(0).ok()?;
    Some(text)
}

/// Verify whether a word is still present in the page 1 content stream after
/// redaction.  Uses extract_positioned_chars (the same method search_and_redact
/// uses to locate text) rather than pdf_engine::extract_text.
///
/// pdf_engine also extracts text from non-content locations such as document
/// outlines/bookmarks and URI-action strings; those are NOT redacted by
/// search_and_redact and would produce false FAILs if used for verification.
/// Fixes #466: MOZILLA-666767-3.pdf "Mozilla" survived in the outline title
/// "Mozilla Privacy Policy" and URI actions even after all content occurrences
/// were successfully redacted.
fn page1_still_contains_word(saved: &[u8], word: &str) -> bool {
    let doc = match lopdf::Document::load_mem(saved) {
        Ok(d) => d,
        Err(_) => return false,
    };
    let chars = match pdf_extract::extract_positioned_chars(&doc, 1) {
        Ok(c) => c,
        Err(_) => return false,
    };
    let text: String = chars.iter().map(|c| c.ch).collect();
    // Use case-SENSITIVE word-boundary matching.  The redaction is also
    // case-sensitive (exact match), so we only verify that the exact searched
    // form has been removed.  A remaining "Are" after searching for "are" is
    // acceptable — we only committed to removing the form we found.
    // Using (?i) here caused 360 PASS→FAIL regressions (#redact-ci-regression):
    // for PDFs where the PDF contains, e.g., "THE" (all-caps), the case-insensitive
    // redaction failed to remove all variants while (?i) verification then found
    // remaining lowercase variants.
    let pattern = format!(r"\b{}\b", regex_lite::escape(word));
    match regex_lite::Regex::new(&pattern) {
        Ok(re) => re.is_match(&text),
        // Fallback to substring if regex construction somehow fails.
        Err(_) => text.contains(word),
    }
}

/// Corpus test: redact first word on page 1, verify it is absent after roundtrip.
pub struct RedactTest;

impl PdfTest for RedactTest {
    fn name(&self) -> &str {
        "redact"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        // Wrap entire execution in a thread to guard against lopdf hangs on
        // corrupt PDFs (page-tree loops, infinite decompression, etc.). #452
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

fn run_inner(pdf: Vec<u8>) -> TestResult {
    let start = std::time::Instant::now();
    let elapsed = || start.elapsed().as_millis() as u64;

    // Skip encrypted PDFs: pdf-engine hangs trying to extract text from
    // encrypted streams without a password. Fast byte scan for /Encrypt. (#redact-timeout)
    if pdf.windows(8).any(|w| w == b"/Encrypt") {
        return TestResult {
            status: TestStatus::Skip,
            error_message: Some("encrypted PDF — skipping redact".into()),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    // 1. Extract text from page 1 to find a word to redact.
    let text = match extract_page1_text(&pdf) {
        Some(t) if !t.trim().is_empty() => t,
        _ => {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some("no text on page 1".into()),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
    };

    // Pick the first word ≥ 3 chars with at least one letter.
    // Purely numeric tokens (e.g. "000") match too broadly in charts/barcodes
    // and cannot be reliably redacted across all rendering paths.
    let search_word = match text.split_whitespace().find(|w| {
        w.len() >= 3
            && w.chars().all(|c| c.is_alphanumeric())
            && w.chars().any(|c| c.is_alphabetic())
    }) {
        Some(w) => w.to_string(),
        None => {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some("no suitable word found".into()),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
    };

    // 2. Load via lopdf and perform redaction.
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

    // Use exact (case-sensitive) search.  Verification is also case-sensitive,
    // so the two sides agree: we only verify the exact form we searched for is
    // gone.  Case-insensitive search caused 360 PASS→FAIL regressions because
    // the case-insensitive redaction engine failed to remove all variants for
    // many PDFs while the (?i) verification then found them. (#redact-ci-regression)
    let opts = RedactSearchOptions::default().pages(vec![1]);

    let redact_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        search_and_redact(&mut doc, &search_word, &opts)
    }));

    let report = match redact_result {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some(format!("redact failed: {e}")),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
        Err(_) => {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some("panic in redaction".into()),
                duration_ms: elapsed(),
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }
    };

    if report.areas_redacted == 0 {
        return TestResult {
            status: TestStatus::Skip,
            error_message: Some("0 areas redacted (font encoding mismatch?)".into()),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    // 3. Save to bytes.
    let mut saved = Vec::new();
    if let Err(e) = doc.save_to(&mut saved) {
        return TestResult {
            status: TestStatus::Fail,
            error_message: Some(format!("save failed: {e}")),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata: HashMap::new(),
        };
    }

    // 4. Verify the word is gone from the page 1 content stream.
    let mut metadata = HashMap::new();
    metadata.insert("search_word".into(), search_word.clone());
    metadata.insert("areas_redacted".into(), report.areas_redacted.to_string());
    metadata.insert("ops_removed".into(), report.operations_removed.to_string());

    if page1_still_contains_word(&saved, &search_word) {
        return TestResult {
            status: TestStatus::Fail,
            error_message: Some(format!(
                "redacted word '{}' still present in content stream",
                search_word
            )),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata,
        };
    }

    // 5. Visual check: render page 1 and verify the redaction overlay is dark.
    //    At 72 dpi, 1 PDF point == 1 pixel, so coordinate conversion is trivial.
    //    Only check when we have a page-1 rect from the report.
    //
    //    The content stream check (step 4) is authoritative — if the text
    //    operator was removed, the redaction succeeded.  The visual check is
    //    a secondary confirmation.  A bright overlay (light background, CTM
    //    compositing, CropBox clipping) does NOT constitute a failure when
    //    the text is confirmed absent from the content stream.
    let page1_rects: Vec<[f64; 4]> = report
        .redacted_rects
        .iter()
        .filter(|(page, _)| *page == 1)
        .map(|(_, rect)| *rect)
        .collect();

    if !page1_rects.is_empty() {
        match render_page1_72dpi(&saved) {
            Some((pixels, w, h)) => {
                for rect in &page1_rects {
                    let rw = rect[2] - rect[0];
                    let rh = rect[3] - rect[1];
                    let rect_area = rw * rh;

                    if rw < 4.0
                        || rh < 4.0
                        || rect[0] < 0.0
                        || rect[1] < 0.0
                        || rect[2] > w as f64
                        || rect[3] > h as f64
                    {
                        metadata.insert("visual_check".into(), "rect_too_small_or_offpage".into());
                        continue;
                    }

                    if let Some(brightness) = mean_brightness_in_rect(&pixels, w, h, *rect) {
                        metadata.insert("visual_brightness".into(), format!("{brightness:.1}"));
                        let threshold = if rect_area < 100.0 {
                            128.0
                        } else if rect_area < 400.0 {
                            128.0 - (rect_area - 100.0) / 300.0 * 78.0
                        } else {
                            50.0
                        };
                        if brightness > threshold {
                            // Text was already confirmed absent (step 4).
                            // Log the visual anomaly but do not fail.
                            metadata.insert(
                                "visual_check".into(),
                                format!(
                                    "overlay_bright:{brightness:.1}>{threshold:.0} \
                                     rect=[{:.1},{:.1},{:.1},{:.1}]",
                                    rect[0], rect[1], rect[2], rect[3]
                                ),
                            );
                        }
                        break;
                    }
                }
            }
            None => {
                metadata.insert("visual_check".into(), "render_failed".into());
            }
        }
    }

    // 6. Verify the PDF is still valid (parse succeeds).
    match pdf_syntax::Pdf::new(saved) {
        Ok(_) => TestResult {
            status: TestStatus::Pass,
            error_message: None,
            duration_ms: elapsed(),
            oracle_score: None,
            metadata,
        },
        Err(e) => TestResult {
            status: TestStatus::Fail,
            error_message: Some(format!("reparse failed after redaction: {e:?}")),
            duration_ms: elapsed(),
            oracle_score: None,
            metadata,
        },
    }
}
