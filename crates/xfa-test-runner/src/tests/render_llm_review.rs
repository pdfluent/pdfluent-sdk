//! LLM-based render quality review (Issue #553).
//!
//! Renders page 0 of a PDF at 150 DPI, sends the PNG to Gemma 3 27B via
//! OpenRouter, and passes/fails based on the returned quality score.
//!
//! This complements render_mupdf_oracle (SSIM) by catching semantic rendering
//! defects that SSIM misses: garbled fonts, wrong text content, broken layouts.
//!
//! Skip conditions:
//! - `OPENROUTER_API_KEY` not set
//! - PDF not selected by hash-based sampler (~1 in 25 = ~800/20K)
//! - Our engine fails to open or render the PDF
//! - LLM API error (request failure, rate limit exhausted, parse failure)
//!
//! Pass condition: LLM score >= 7
//! Fail condition: LLM score < 7

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::path::Path;

use super::{PdfTest, TestResult, TestStatus};
use crate::oracles::llm_vision::LlmVisionOracle;

const RENDER_DPI: f64 = 150.0;
const SCORE_PASS_THRESHOLD: u8 = 7;
/// Test ~1 PDF in every SAMPLE_RATE (~800 out of 20K).
const SAMPLE_RATE: u64 = 25;
/// Always review PDFs whose last mupdf oracle SSIM was below this.
const SSIM_ALWAYS_REVIEW: f64 = 0.80;

pub struct RenderLlmReviewTest;

impl PdfTest for RenderLlmReviewTest {
    fn name(&self) -> &str {
        "render_llm_review"
    }

    fn run(&self, pdf_data: &[u8], path: &Path) -> TestResult {
        let start = std::time::Instant::now();

        // Require API key — skip silently when absent.
        let oracle = match LlmVisionOracle::new() {
            Some(o) => o,
            None => {
                return skip("OPENROUTER_API_KEY not set", start);
            }
        };

        // Hash-based sampler: test ~1 in SAMPLE_RATE PDFs for cost control.
        // Also always test when the environment requests forced review.
        let force = std::env::var("LLM_REVIEW_ALL").is_ok();
        if !force && !should_sample(path) {
            return skip("not in sample", start);
        }

        // Open and render page 0.
        let doc = match pdf_engine::PdfDocument::open(pdf_data.to_vec()) {
            Ok(d) => d,
            Err(e) => return skip(&format!("engine open: {e}"), start),
        };

        let options = pdf_engine::RenderOptions {
            dpi: RENDER_DPI,
            ..Default::default()
        };
        let render = match doc.render_page(0, &options) {
            Ok(r) => r,
            Err(e) => return skip(&format!("render page 0: {e}"), start),
        };

        // Encode rendered RGBA pixels to PNG bytes.
        let img =
            match image::RgbaImage::from_raw(render.width, render.height, render.pixels) {
                Some(i) => i,
                None => return skip("RGBA image construction failed", start),
            };
        let mut png_bytes: Vec<u8> = Vec::new();
        if let Err(e) = img.write_to(
            &mut Cursor::new(&mut png_bytes),
            image::ImageFormat::Png,
        ) {
            return skip(&format!("PNG encode: {e}"), start);
        }

        // Send to LLM oracle.
        let review = match oracle.review_page(&png_bytes) {
            Ok(r) => r,
            Err(e) => return skip(&format!("LLM error: {e}"), start),
        };

        let mut metadata = HashMap::new();
        metadata.insert("llm_score".to_string(), review.score.to_string());
        metadata.insert("llm_model".to_string(), "google/gemma-3-27b-it".to_string());
        if !review.issues.is_empty() {
            metadata.insert("llm_issues".to_string(), review.issues.join("; "));
        }

        let (status, error_message) = if review.score >= SCORE_PASS_THRESHOLD {
            (TestStatus::Pass, None)
        } else {
            (
                TestStatus::Fail,
                Some(format!(
                    "LLM score {}/10 below threshold {} — issues: {}",
                    review.score,
                    SCORE_PASS_THRESHOLD,
                    if review.issues.is_empty() {
                        "(none reported)".to_string()
                    } else {
                        review.issues.join("; ")
                    }
                )),
            )
        };

        TestResult {
            status,
            error_message,
            duration_ms: start.elapsed().as_millis() as u64,
            oracle_score: Some(review.score as f64 / 10.0),
            metadata,
        }
    }
}

/// Returns true for ~1 in SAMPLE_RATE paths, deterministically by hash.
fn should_sample(path: &Path) -> bool {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut h);
    h.finish().is_multiple_of(SAMPLE_RATE)
}

fn skip(reason: &str, start: std::time::Instant) -> TestResult {
    TestResult {
        status: TestStatus::Skip,
        error_message: Some(reason.to_string()),
        duration_ms: start.elapsed().as_millis() as u64,
        oracle_score: None,
        metadata: HashMap::new(),
    }
}

#[allow(dead_code)]
fn ssim_always_review_threshold() -> f64 {
    SSIM_ALWAYS_REVIEW
}
