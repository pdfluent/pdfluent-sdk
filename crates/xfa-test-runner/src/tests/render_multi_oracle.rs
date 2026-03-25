//! Multi-oracle render test: mutool + pdftoppm.
//!
//! Renders page 1 (and up to MAX_PAGES) with three renderers:
//!   - Our engine
//!   - mutool draw (MuPDF)
//!   - pdftoppm (Poppler)
//!
//! Pass/fail logic:
//!   - Pass  : at least ONE oracle SSIM ≥ SSIM_PASS_THRESHOLD
//!   - Fail  : BOTH available oracle SSIMs < SSIM_PASS_THRESHOLD  (our bug)
//!   - Skip  : both oracles unavailable / both fail to render
//!
//! When only one oracle is available, falls back to single-oracle behaviour.
//!
//! Metadata keys: ssim_mutool, ssim_poppler, ssim_mutool_vs_poppler,
//!                verdict ("match_both", "match_mutool_only", "match_poppler_only",
//!                         "mupdf_outlier", "both_fail").

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use super::{PdfTest, TestResult, TestStatus};
use crate::oracles::ssim;

const MAX_PAGES: usize = 2;
const RENDER_DPI: f64 = 150.0;
const SSIM_PASS_THRESHOLD: f64 = 0.75;

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct RenderMultiOracleTest;

/// Render one page with mutool draw into a PNG file.
fn render_mutool_to_file(pdf_path: &Path, page_1indexed: usize, png_path: &Path) -> bool {
    let dpi_s = format!("{}", RENDER_DPI as u32);
    Command::new("mutool")
        .args([
            "draw",
            "-q",
            "-r",
            &dpi_s,
            "-o",
            png_path.to_str().unwrap_or(""),
            pdf_path.to_str().unwrap_or(""),
            &format!("{page_1indexed}"),
        ])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Render one page with pdftoppm into a PNG file.
/// Uses -singlefile so the output is exactly `{prefix}.png`.
fn render_poppler_to_file(pdf_path: &Path, page_1indexed: usize, out_png: &Path) -> bool {
    let dpi_s = format!("{}", RENDER_DPI as u32);
    let page_s = format!("{page_1indexed}");
    // pdftoppm with -singlefile writes to `{prefix}.png` — strip the extension.
    let prefix = out_png.to_str().unwrap_or("").trim_end_matches(".png");
    let ok = Command::new("pdftoppm")
        .args([
            "-png",
            "-r",
            &dpi_s,
            "-f",
            &page_s,
            "-l",
            &page_s,
            "-singlefile",
            pdf_path.to_str().unwrap_or(""),
            prefix,
        ])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    ok && out_png.exists()
}

/// Load a PNG from disk as RGBA8 and clean up the file.
fn load_and_remove(path: &Path) -> Option<image::RgbaImage> {
    let img = image::open(path).ok().map(|i| i.into_rgba8());
    let _ = std::fs::remove_file(path);
    img
}

impl PdfTest for RenderMultiOracleTest {
    fn name(&self) -> &str {
        "render_multi_oracle"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let start = std::time::Instant::now();

        let has_mutool = Command::new("mutool").arg("-v").output().is_ok();
        let has_poppler = Command::new("pdftoppm").arg("-v").output().is_ok();

        if !has_mutool && !has_poppler {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some("neither mutool nor pdftoppm found".into()),
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        // Open with our engine.
        let doc = match pdf_engine::PdfDocument::open(pdf_data.to_vec()) {
            Ok(d) => d,
            Err(e) => {
                let msg = format!("engine open: {e}");
                let skip = msg.contains("Decryption(")
                    || msg.contains("PasswordProtected")
                    || msg.contains("UnsupportedAlgorithm")
                    || msg.contains("invalid PDF");
                return TestResult {
                    status: if skip {
                        TestStatus::Skip
                    } else {
                        TestStatus::Fail
                    },
                    error_message: Some(msg),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        let uid = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let tmp = std::env::temp_dir();
        let pdf_tmp = tmp.join(format!("xfa-multi-{pid}-{uid}.pdf"));
        if let Err(e) = std::fs::write(&pdf_tmp, pdf_data) {
            return TestResult {
                status: TestStatus::Fail,
                error_message: Some(format!("write temp PDF: {e}")),
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        let page_count = doc.page_count().min(MAX_PAGES);
        let options = pdf_engine::RenderOptions {
            dpi: RENDER_DPI,
            ..Default::default()
        };

        let mut min_ssim_mutool: Option<f64> = None;
        let mut min_ssim_poppler: Option<f64> = None;
        let mut min_ssim_mu_vs_pop: Option<f64> = None;
        let mut worst_page = 0usize;
        let mut mutool_failed = false;
        let mut poppler_failed = false;
        let mut pages_computed = 0usize;

        for i in 0..page_count {
            let our_render = match doc.render_page(i, &options) {
                Ok(r) => r,
                Err(e) => {
                    let _ = std::fs::remove_file(&pdf_tmp);
                    return TestResult {
                        status: TestStatus::Fail,
                        error_message: Some(format!("our render page {i}: {e}")),
                        duration_ms: start.elapsed().as_millis() as u64,
                        oracle_score: None,
                        metadata: HashMap::new(),
                    };
                }
            };

            // Render oracles once each and cache the image in memory.
            let mu_img: Option<image::RgbaImage> = if has_mutool && !mutool_failed {
                let png = tmp.join(format!("xfa-multi-{pid}-{uid}-mu-p{i}.png"));
                if render_mutool_to_file(&pdf_tmp, i + 1, &png) {
                    match load_and_remove(&png) {
                        Some(img) => Some(img),
                        None => {
                            mutool_failed = true;
                            None
                        }
                    }
                } else {
                    mutool_failed = true;
                    None
                }
            } else {
                None
            };

            let pop_img: Option<image::RgbaImage> = if has_poppler && !poppler_failed {
                let png = tmp.join(format!("xfa-multi-{pid}-{uid}-pop-p{i}.png"));
                if render_poppler_to_file(&pdf_tmp, i + 1, &png) {
                    match load_and_remove(&png) {
                        Some(img) => Some(img),
                        None => {
                            poppler_failed = true;
                            None
                        }
                    }
                } else {
                    poppler_failed = true;
                    None
                }
            } else {
                None
            };

            // Compute SSIMs using cached images (no re-rendering).
            let ssim_mu = mu_img.as_ref().map(|img| {
                ssim::compute_ssim(
                    &our_render.pixels,
                    our_render.width,
                    our_render.height,
                    img.as_raw(),
                    img.width(),
                    img.height(),
                )
            });

            let ssim_pop = pop_img.as_ref().map(|img| {
                ssim::compute_ssim(
                    &our_render.pixels,
                    our_render.width,
                    our_render.height,
                    img.as_raw(),
                    img.width(),
                    img.height(),
                )
            });

            let ssim_mu_pop = match (&mu_img, &pop_img) {
                (Some(mu), Some(pop)) => Some(ssim::compute_ssim(
                    mu.as_raw(),
                    mu.width(),
                    mu.height(),
                    pop.as_raw(),
                    pop.width(),
                    pop.height(),
                )),
                _ => None,
            };

            // Track per-page worst.
            if ssim_mu.is_some() || ssim_pop.is_some() {
                let page_worst = ssim_mu.into_iter().chain(ssim_pop).fold(1.0_f64, f64::min);
                let prev_worst = min_ssim_mutool
                    .into_iter()
                    .chain(min_ssim_poppler)
                    .fold(1.0_f64, f64::min);
                if pages_computed == 0 || page_worst < prev_worst {
                    worst_page = i;
                }
                pages_computed += 1;
            }

            min_ssim_mutool = match (min_ssim_mutool, ssim_mu) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (None, b) => b,
                (a, None) => a,
            };
            min_ssim_poppler = match (min_ssim_poppler, ssim_pop) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (None, b) => b,
                (a, None) => a,
            };
            min_ssim_mu_vs_pop = match (min_ssim_mu_vs_pop, ssim_mu_pop) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (None, b) => b,
                (a, None) => a,
            };
        }

        let _ = std::fs::remove_file(&pdf_tmp);

        if pages_computed == 0 {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some("all oracles failed to render".into()),
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        // Pass/fail: pass if at least ONE oracle agrees (SSIM ≥ threshold).
        // Fail only when BOTH available oracles disagree with us.
        let mu_passes = min_ssim_mutool.is_some_and(|s| s >= SSIM_PASS_THRESHOLD);
        let pop_passes = min_ssim_poppler.is_some_and(|s| s >= SSIM_PASS_THRESHOLD);
        let mu_available = min_ssim_mutool.is_some();
        let pop_available = min_ssim_poppler.is_some();

        let any_passes = mu_passes || pop_passes;

        let (status, error_message) = if any_passes {
            (TestStatus::Pass, None)
        } else {
            let ssim_desc = match (min_ssim_mutool, min_ssim_poppler) {
                (Some(mu), Some(pop)) => format!("mutool={mu:.4} poppler={pop:.4}"),
                (Some(mu), None) => format!("mutool={mu:.4}"),
                (None, Some(pop)) => format!("poppler={pop:.4}"),
                (None, None) => "no ssim".into(),
            };
            (
                TestStatus::Fail,
                Some(format!(
                    "SSIM below {SSIM_PASS_THRESHOLD} (both oracles): {ssim_desc} (worst: page {worst_page})"
                )),
            )
        };

        let verdict = match (mu_passes, pop_passes, mu_available, pop_available) {
            (true, true, _, _) => "match_both",
            (true, false, _, true) => "match_mutool_only",
            // poppler agrees with us but mutool doesn't → MuPDF-specific divergence
            (false, true, true, _) => "mupdf_outlier",
            (false, false, true, true) => "both_fail",
            (true, _, _, false) => "match_mutool_only",
            (_, true, false, _) => "match_poppler_only",
            _ => "unknown",
        };

        let oracle_score = match (min_ssim_mutool, min_ssim_poppler) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, None) => a,
            (None, b) => b,
        };

        let mut metadata = HashMap::new();
        if let Some(s) = min_ssim_mutool {
            metadata.insert("ssim_mutool".to_string(), format!("{s:.4}"));
        }
        if let Some(s) = min_ssim_poppler {
            metadata.insert("ssim_poppler".to_string(), format!("{s:.4}"));
        }
        if let Some(s) = min_ssim_mu_vs_pop {
            metadata.insert("ssim_mutool_vs_poppler".to_string(), format!("{s:.4}"));
        }
        metadata.insert("verdict".to_string(), verdict.to_string());
        metadata.insert("worst_page".to_string(), worst_page.to_string());
        metadata.insert("pages_compared".to_string(), pages_computed.to_string());
        metadata.insert("dpi".to_string(), RENDER_DPI.to_string());

        TestResult {
            status,
            error_message,
            duration_ms: start.elapsed().as_millis() as u64,
            oracle_score,
            metadata,
        }
    }
}
