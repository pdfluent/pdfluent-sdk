//! XFA flatten test.
//!
//! For each XFA PDF, strips the AcroForm / XFA layer using lopdf (the same
//! approach as `xfa-cli`'s `flatten` command), then re-parses the result
//! with `pdf_syntax` and checks that the output is a structurally valid PDF
//! with at least one page.
//!
//! When the iText 5 oracle script is present at `/opt/itext/itext-xfa-oracle.sh`,
//! the test also compares our page count against iText's flatten output and
//! fails if they differ.
//!
//! SSIM visual regression: when `mutool` is available, renders page 1 of the
//! flattened output (via pdf-engine) and page 1 of the original (via mutool),
//! then computes SSIM between them. Fails if SSIM < 0.70.  The threshold is
//! intentionally lenient: XFA flattening strips dynamic form content, so the
//! appearance is expected to change, but shouldn't produce garbage.
//!
//! Skip policy:
//! - No /XFA key in AcroForm → Skip (not an XFA form)
//! - lopdf cannot load the PDF  → Skip (corrupt; defer to parse test)
//!
//! Fail conditions:
//! - lopdf cannot re-serialise the mutated document
//! - pdf_syntax cannot re-parse the saved bytes
//! - Re-parsed PDF has zero pages
//! - Page count differs from iText oracle (when oracle is available)
//! - SSIM of page 1 < 0.70 (when mutool available and rendering succeeds)

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use super::{PdfTest, TestResult, TestStatus};
use crate::oracles::itext::ITextOracle;
use crate::oracles::ssim;

const SSIM_PASS_THRESHOLD: f64 = 0.70;
const RENDER_DPI: f64 = 150.0;

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct XfaFlattenTest;

impl PdfTest for XfaFlattenTest {
    fn name(&self) -> &str {
        "xfa_flatten"
    }

    fn run(&self, pdf_data: &[u8], path: &Path) -> TestResult {
        let start = std::time::Instant::now();

        let mut doc = match lopdf::Document::load_mem(pdf_data) {
            Ok(d) => d,
            Err(_) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some("lopdf could not load PDF".into()),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                }
            }
        };

        if !has_xfa(&doc) {
            return TestResult {
                status: TestStatus::Skip,
                error_message: None,
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        // Query the iText oracle with the original PDF before we mutate it.
        // Stored as Option<(itext_page_count, flatten_success)>.
        let itext_result = ITextOracle::new().and_then(|oracle| oracle.call(path));

        // Strip the AcroForm (which contains the /XFA key) from the catalog.
        // This is the minimum "flatten" step: the page content streams remain
        // untouched so the PDF stays renderable.
        remove_acroform(&mut doc);

        let mut buf = Vec::new();
        if let Err(e) = doc.save_to(&mut buf) {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!("re-serialisation failed: {e}")),
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        match pdf_syntax::Pdf::new(buf.clone()) {
            Ok(reparsed) => {
                let pages = reparsed.pages().len();
                let mut metadata = HashMap::new();
                metadata.insert("page_count".to_string(), pages.to_string());

                // Structural check: at least one page.
                if pages == 0 {
                    return TestResult {
                        status: TestStatus::Fail,
                        error_message: Some("flattened PDF has no pages".into()),
                        duration_ms: start.elapsed().as_millis() as u64,
                        oracle_score: None,
                        metadata,
                    };
                }

                // iText oracle comparison: fail if page count differs.
                if let Some(ref r) = itext_result {
                    metadata.insert("itext_has_xfa".to_string(), r.has_xfa.to_string());
                    metadata.insert("itext_page_count".to_string(), r.page_count.to_string());
                    metadata.insert(
                        "itext_flatten_success".to_string(),
                        r.flatten_success.to_string(),
                    );
                    if !r.errors.is_empty() {
                        metadata.insert("itext_errors".to_string(), r.errors.join("; "));
                    }
                    if r.has_xfa
                        && r.flatten_success
                        && r.page_count > 0
                        && r.page_count as usize != pages
                    {
                        return TestResult {
                            status: TestStatus::Fail,
                            error_message: Some(format!(
                                "page count mismatch: ours={pages}, iText={}",
                                r.page_count
                            )),
                            duration_ms: start.elapsed().as_millis() as u64,
                            oracle_score: None,
                            metadata,
                        };
                    }
                }

                // SSIM visual regression: compare our flatten output (page 1 via
                // pdf-engine) against the original PDF (page 1 via mutool). Skips
                // the comparison if mutool is unavailable or if rendering fails.
                let ssim_result = compute_ssim_comparison(pdf_data, &buf);
                match ssim_result {
                    SsimResult::Score(score) => {
                        metadata.insert("ssim".to_string(), format!("{score:.4}"));
                        if score < SSIM_PASS_THRESHOLD {
                            return TestResult {
                                status: TestStatus::Fail,
                                error_message: Some(format!(
                                    "SSIM {score:.4} below threshold {SSIM_PASS_THRESHOLD} (flatten visual regression)"
                                )),
                                duration_ms: start.elapsed().as_millis() as u64,
                                oracle_score: Some(score),
                                metadata,
                            };
                        }
                        TestResult {
                            status: TestStatus::Pass,
                            error_message: None,
                            duration_ms: start.elapsed().as_millis() as u64,
                            oracle_score: Some(score),
                            metadata,
                        }
                    }
                    SsimResult::Skipped(reason) => {
                        metadata.insert("ssim_skip".to_string(), reason);
                        TestResult {
                            status: TestStatus::Pass,
                            error_message: None,
                            duration_ms: start.elapsed().as_millis() as u64,
                            oracle_score: None,
                            metadata,
                        }
                    }
                }
            }
            Err(e) => TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!("re-parse of flattened PDF failed: {e:?}")),
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata: HashMap::new(),
            },
        }
    }
}

enum SsimResult {
    Score(f64),
    Skipped(String),
}

/// Compute SSIM between page 1 of our flatten output and the original PDF.
///
/// - Our output is rendered via pdf-engine.
/// - The original is rendered via mutool (as visual reference).
/// - Returns `Skipped` if mutool is unavailable or if either render fails.
fn compute_ssim_comparison(original_data: &[u8], flattened_data: &[u8]) -> SsimResult {
    // Require mutool.
    if Command::new("mutool").arg("-v").output().is_err() {
        return SsimResult::Skipped("mutool not found".into());
    }

    // Render our flatten output page 1 via pdf-engine.
    let flatten_doc = match pdf_engine::PdfDocument::open(flattened_data.to_vec()) {
        Ok(d) => d,
        Err(e) => return SsimResult::Skipped(format!("engine open flatten: {e}")),
    };
    if flatten_doc.page_count() == 0 {
        return SsimResult::Skipped("flatten has no pages".into());
    }
    let opts = pdf_engine::RenderOptions {
        dpi: RENDER_DPI,
        ..Default::default()
    };
    let our_render = match flatten_doc.render_page(0, &opts) {
        Ok(r) => r,
        Err(e) => return SsimResult::Skipped(format!("engine render page 0: {e}")),
    };

    // Render the original PDF page 1 via mutool.
    let uid = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let tmp_dir = std::env::temp_dir();
    let orig_tmp = tmp_dir.join(format!("xfa-flatten-orig-{pid}-{uid}.pdf"));
    let png_tmp = tmp_dir.join(format!("xfa-flatten-orig-{pid}-{uid}.png"));

    if std::fs::write(&orig_tmp, original_data).is_err() {
        return SsimResult::Skipped("could not write temp PDF".into());
    }

    let dpi_s = format!("{}", RENDER_DPI as u32);
    let ok = Command::new("mutool")
        .args([
            "draw",
            "-q",
            "-r",
            &dpi_s,
            "-o",
            png_tmp.to_str().unwrap_or(""),
            orig_tmp.to_str().unwrap_or(""),
            "1",
        ])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    let _ = std::fs::remove_file(&orig_tmp);

    if !ok {
        let _ = std::fs::remove_file(&png_tmp);
        return SsimResult::Skipped("mutool draw failed on original".into());
    }

    let orig_img = match image::open(&png_tmp) {
        Ok(img) => img.into_rgba8(),
        Err(e) => {
            let _ = std::fs::remove_file(&png_tmp);
            return SsimResult::Skipped(format!("load mutool PNG: {e}"));
        }
    };
    let _ = std::fs::remove_file(&png_tmp);

    let (ow, oh) = (orig_img.width(), orig_img.height());
    let score = ssim::compute_ssim(
        &our_render.pixels,
        our_render.width,
        our_render.height,
        orig_img.as_raw(),
        ow,
        oh,
    );
    SsimResult::Score(score)
}

/// Returns `true` when the PDF catalog has an AcroForm that contains a /XFA key.
fn has_xfa(doc: &lopdf::Document) -> bool {
    let root_id = match doc.trailer.get(b"Root") {
        Ok(lopdf::Object::Reference(id)) => *id,
        _ => return false,
    };

    // Resolve the AcroForm entry — it may be an indirect reference or inline dict.
    let acro_obj = {
        let catalog = match doc.get_dictionary(root_id) {
            Ok(d) => d,
            Err(_) => return false,
        };
        match catalog.get(b"AcroForm") {
            Ok(lopdf::Object::Reference(r)) => lopdf::Object::Reference(*r),
            Ok(lopdf::Object::Dictionary(d)) => {
                // Inline dict: check directly without a second lookup.
                return d.get(b"XFA").is_ok();
            }
            _ => return false,
        }
    };

    match acro_obj {
        lopdf::Object::Reference(r) => doc
            .get_dictionary(r)
            .map(|d| d.get(b"XFA").is_ok())
            .unwrap_or(false),
        _ => false,
    }
}

/// Remove the /AcroForm entry from the document catalog.
fn remove_acroform(doc: &mut lopdf::Document) {
    let root_id = match doc.trailer.get(b"Root") {
        Ok(lopdf::Object::Reference(id)) => *id,
        _ => return,
    };
    if let Ok(lopdf::Object::Dictionary(ref mut dict)) = doc.get_object_mut(root_id) {
        dict.remove(b"AcroForm");
    }
}
