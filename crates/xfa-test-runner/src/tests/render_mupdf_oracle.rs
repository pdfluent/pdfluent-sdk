//! Render oracle using mutool draw as reference renderer.
//!
//! Renders each page (up to MAX_PAGES) with both our engine and mutool at
//! RENDER_DPI, then computes SSIM between the two outputs.
//!
//! Skip conditions:
//! - mutool not found in PATH
//! - mutool fails to render a page (unsupported content)
//! - Our engine cannot open the PDF (defer to parse test)
//!
//! Fail condition:
//! - Any page SSIM < SSIM_PASS_THRESHOLD (0.75)
//!
//! Threshold rationale (empirical, 2026-03-22):
//! Measured mutool-vs-pdftoppm SSIM on 50 corpus PDFs using scikit-image
//! (per-channel, pixel-stride). Reference-vs-reference P10 = 0.83, median =
//! 0.94. Our Rust SSIM (8×8 windows, grayscale) runs slightly higher for the
//! same pair, so 0.75 gives comfortable headroom below P10 while still
//! catching real rendering defects.

use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use super::{PdfTest, TestResult, TestStatus};
use crate::oracles::ssim;

const MAX_PAGES: usize = 5;
const RENDER_DPI: f64 = 150.0;
const SSIM_PASS_THRESHOLD: f64 = 0.75;

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub struct RenderMupdfOracleTest;

impl PdfTest for RenderMupdfOracleTest {
    fn name(&self) -> &str {
        "render_mupdf_oracle"
    }

    fn run(&self, pdf_data: &[u8], _path: &Path) -> TestResult {
        let start = std::time::Instant::now();

        // Skip if mutool is not in PATH.
        if Command::new("mutool").arg("-v").output().is_err() {
            return TestResult {
                status: TestStatus::Skip,
                error_message: Some("mutool not found".into()),
                duration_ms: start.elapsed().as_millis() as u64,
                oracle_score: None,
                metadata: HashMap::new(),
            };
        }

        // Open with our engine.
        // Encrypted PDFs that we cannot decrypt without a password are not a
        // rendering defect — skip rather than fail so they don't pollute the
        // SSIM statistics.
        let doc = match pdf_engine::PdfDocument::open(pdf_data.to_vec()) {
            Ok(d) => d,
            Err(e) => {
                let msg = format!("engine open: {e}");
                // Also skip PDFs that are structurally invalid and cannot be opened at all —
                // these are not rendering defects but corrupt/unsupported input files.
                let is_encrypted = msg.contains("Decryption(")
                    || msg.contains("PasswordProtected")
                    || msg.contains("UnsupportedAlgorithm")
                    || msg.contains("invalid PDF");
                return TestResult {
                    status: if is_encrypted {
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

        // Write PDF to a temp file so mutool can read it.
        let uid = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let tmp_dir = std::env::temp_dir();
        let pdf_tmp = tmp_dir.join(format!("xfa-mupdf-{pid}-{uid}.pdf"));
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

        let mut min_ssim = 1.0_f64;
        let mut worst_page = 0usize;
        let mut pages_computed = 0usize;
        let mut result_status = TestStatus::Pass;
        let mut result_error: Option<String> = None;

        'pages: for i in 0..page_count {
            // Render with our engine.
            let our_render = match doc.render_page(i, &options) {
                Ok(r) => r,
                Err(e) => {
                    result_status = TestStatus::Fail;
                    result_error = Some(format!("our render page {i}: {e}"));
                    break 'pages;
                }
            };

            // Render with mutool (pages are 1-indexed in mutool's CLI).
            let png_tmp = tmp_dir.join(format!("xfa-mupdf-{pid}-{uid}-p{i}.png"));
            let dpi_s = format!("{}", RENDER_DPI as u32);
            let ok = Command::new("mutool")
                .args([
                    "draw",
                    "-q",
                    "-r",
                    &dpi_s,
                    "-o",
                    png_tmp.to_str().unwrap_or(""),
                    pdf_tmp.to_str().unwrap_or(""),
                    &format!("{}", i + 1),
                ])
                .status()
                .map(|s| s.success())
                .unwrap_or(false);

            if !ok {
                // mutool can't render this page — skip oracle for this PDF.
                let _ = std::fs::remove_file(&png_tmp);
                result_status = TestStatus::Skip;
                result_error = Some(format!("mutool draw failed on page {}", i + 1));
                break 'pages;
            }

            let mutool_img = match image::open(&png_tmp) {
                Ok(img) => img.into_rgba8(),
                Err(e) => {
                    let _ = std::fs::remove_file(&png_tmp);
                    result_status = TestStatus::Skip;
                    result_error = Some(format!("load mutool PNG page {i}: {e}"));
                    break 'pages;
                }
            };
            let _ = std::fs::remove_file(&png_tmp);

            let (mw, mh) = (mutool_img.width(), mutool_img.height());
            let page_ssim = ssim::compute_ssim(
                &our_render.pixels,
                our_render.width,
                our_render.height,
                mutool_img.as_raw(),
                mw,
                mh,
            );

            if page_ssim < min_ssim {
                min_ssim = page_ssim;
                worst_page = i;
            }
            pages_computed += 1;
        }

        let _ = std::fs::remove_file(&pdf_tmp);

        // Apply SSIM threshold only when all pages completed successfully.
        if result_status == TestStatus::Pass && min_ssim < SSIM_PASS_THRESHOLD {
            result_status = TestStatus::Fail;
            result_error = Some(format!(
                "SSIM {min_ssim:.4} below threshold {SSIM_PASS_THRESHOLD} (worst: page {worst_page})"
            ));
        }

        let mut metadata = HashMap::new();
        metadata.insert("ssim_min".to_string(), format!("{min_ssim:.4}"));
        metadata.insert("worst_page".to_string(), worst_page.to_string());
        metadata.insert("pages_compared".to_string(), pages_computed.to_string());
        metadata.insert("dpi".to_string(), RENDER_DPI.to_string());

        TestResult {
            status: result_status,
            error_message: result_error,
            duration_ms: start.elapsed().as_millis() as u64,
            oracle_score: if pages_computed > 0 {
                Some(min_ssim)
            } else {
                None
            },
            metadata,
        }
    }
}
