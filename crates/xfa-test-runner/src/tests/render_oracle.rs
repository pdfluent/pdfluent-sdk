use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::{PdfTest, TestResult, TestStatus};
use crate::oracles::ssim;

const MAX_PAGES: usize = 5;
const RENDER_DPI: f64 = 150.0;
const SSIM_PASS_THRESHOLD: f64 = 0.95;

pub struct RenderOracleTest {
    pub diff_dir: Option<PathBuf>,
}

impl PdfTest for RenderOracleTest {
    fn name(&self) -> &str {
        "render_oracle"
    }

    fn run(&self, pdf_data: &[u8], path: &Path) -> TestResult {
        let start = std::time::Instant::now();

        // 1. Open with our engine
        let doc = match pdf_engine::PdfDocument::open(pdf_data.to_vec()) {
            Ok(d) => d,
            Err(e) => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some(format!("Our engine: {e}")),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        // 2. Create PDFium renderer (per-call: safe for multi-threaded use)
        let pdfium = match pdfium_ffi_bridge::renderer::PdfRenderer::new() {
            Ok(p) => p,
            Err(e) => {
                return TestResult {
                    status: TestStatus::Skip,
                    error_message: Some(format!("PDFium unavailable: {e}")),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        let pdfium_doc = match pdfium.load_document(pdf_data) {
            Ok(d) => d,
            Err(e) => {
                return TestResult {
                    status: TestStatus::Fail,
                    error_message: Some(format!("PDFium load: {e}")),
                    duration_ms: start.elapsed().as_millis() as u64,
                    oracle_score: None,
                    metadata: HashMap::new(),
                };
            }
        };

        let page_count = doc.page_count().min(MAX_PAGES);
        let mut min_ssim = 1.0_f64;
        let mut worst_page = 0usize;
        let options = pdf_engine::RenderOptions {
            dpi: RENDER_DPI,
            ..Default::default()
        };

        for i in 0..page_count {
            // Render with our engine
            let our_render = match doc.render_page(i, &options) {
                Ok(r) => r,
                Err(e) => {
                    return TestResult {
                        status: TestStatus::Fail,
                        error_message: Some(format!("Our render page {i}: {e}")),
                        duration_ms: start.elapsed().as_millis() as u64,
                        oracle_score: None,
                        metadata: HashMap::new(),
                    };
                }
            };

            // Render with PDFium
            let pdfium_img = match pdfium.render_page(&pdfium_doc, i as u16, RENDER_DPI as f32) {
                Ok(r) => r,
                Err(e) => {
                    return TestResult {
                        status: TestStatus::Fail,
                        error_message: Some(format!("PDFium render page {i}: {e}")),
                        duration_ms: start.elapsed().as_millis() as u64,
                        oracle_score: None,
                        metadata: HashMap::new(),
                    };
                }
            };

            let pdfium_rgba = pdfium_img.into_rgba8();
            let (pw, ph) = (pdfium_rgba.width(), pdfium_rgba.height());

            // Compute SSIM
            let page_ssim = ssim::compute_ssim(
                &our_render.pixels,
                our_render.width,
                our_render.height,
                pdfium_rgba.as_raw(),
                pw,
                ph,
            );

            if page_ssim < min_ssim {
                min_ssim = page_ssim;
                worst_page = i;
            }

            // Save diff image when below threshold
            if page_ssim < SSIM_PASS_THRESHOLD {
                if let Some(ref dir) = self.diff_dir {
                    let w = our_render.width.min(pw);
                    let h = our_render.height.min(ph);
                    let diff_pixels = ssim::generate_diff(
                        &our_render.pixels,
                        our_render.width,
                        pdfium_rgba.as_raw(),
                        pw,
                        w,
                        h,
                    );
                    let _ = std::fs::create_dir_all(dir);
                    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
                    let filename = format!("{stem}_page{i}.png");
                    if let Some(img) = image::RgbaImage::from_raw(w, h, diff_pixels) {
                        let _ = img.save(dir.join(filename));
                    }
                }
            }
        }

        let mut metadata = HashMap::new();
        metadata.insert("ssim_min".to_string(), format!("{min_ssim:.4}"));
        metadata.insert("worst_page".to_string(), worst_page.to_string());
        metadata.insert("pages_compared".to_string(), page_count.to_string());
        metadata.insert("dpi".to_string(), RENDER_DPI.to_string());

        let status = if min_ssim >= SSIM_PASS_THRESHOLD {
            TestStatus::Pass
        } else {
            TestStatus::Fail
        };

        let error_message = if min_ssim < SSIM_PASS_THRESHOLD {
            Some(format!(
                "SSIM {min_ssim:.4} below threshold {SSIM_PASS_THRESHOLD} (worst: page {worst_page})"
            ))
        } else {
            None
        };

        TestResult {
            status,
            error_message,
            duration_ms: start.elapsed().as_millis() as u64,
            oracle_score: Some(min_ssim),
            metadata,
        }
    }
}
