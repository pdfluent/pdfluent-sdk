//! OCR backend trait and implementations.
//!
//! ## OCR Backend Extension
//!
//! The [`OcrBackend`] trait is public. Callers can implement their own backends
//! for specific use cases:
//!
//! - **ocrs** (this crate, `ocr` feature): pure-Rust Tesseract-style engine.
//!   WASM-compatible. Requires external model files (`.rten` format).
//!
//! - **Mistral OCR** (`ocr-mistral` feature): blocking HTTP adapter to
//!   Mistral's cloud OCR API.
//!
//! - **Paddle ONNX** (`ocr-onnx` feature): adapter around `pdf-ocr`'s
//!   PaddleOCR engine.
//!
//! - **Custom**: implement [`OcrBackend`] directly for proprietary or
//!   cloud-based OCR (Google Vision, AWS Textract, etc.).

use std::fmt;

#[cfg(feature = "ocr-mistral")]
use std::io::Cursor;

#[cfg(feature = "ocr-mistral")]
use base64::Engine as _;
#[cfg(feature = "ocr-mistral")]
use image::{DynamicImage, ImageFormat, RgbImage};
#[cfg(feature = "ocr-mistral")]
use serde::{Deserialize, Serialize};

// ── Public types ─────────────────────────────────────────────────────────────

/// Result of recognizing text in a single image.
#[derive(Debug, Clone)]
pub struct OcrResult {
    /// Full recognized text, words joined with spaces.
    pub text: String,
    /// Individual recognized words with positions and confidence.
    pub words: Vec<OcrWord>,
    /// Overall recognition confidence (0.0 – 1.0).
    pub confidence: f32,
}

/// A single recognized word.
#[derive(Debug, Clone)]
pub struct OcrWord {
    /// The recognized text of the word.
    pub text: String,
    /// Bounding box in image coordinates: `[x, y, width, height]` in pixels.
    pub bbox: [f32; 4],
    /// Word-level confidence (0.0 – 1.0).
    pub confidence: f32,
}

/// Errors returned by an [`OcrBackend`].
#[derive(Debug)]
pub enum OcrError {
    /// No OCR engine is available (models missing, feature not compiled, etc.).
    NoEngine,
    /// The input image data is invalid or cannot be decoded.
    ImageError(String),
    /// Recognition ran but produced an error.
    RecognitionFailed(String),
}

impl fmt::Display for OcrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OcrError::NoEngine => write!(f, "no OCR engine available"),
            OcrError::ImageError(e) => write!(f, "image error: {e}"),
            OcrError::RecognitionFailed(e) => write!(f, "recognition failed: {e}"),
        }
    }
}

impl std::error::Error for OcrError {}

/// Pluggable OCR backend.
///
/// Implementations receive raw RGB pixel data and return recognized text.
/// Implementations must be `Send + Sync` so they can be held in a `static`
/// or shared across threads.
pub trait OcrBackend: Send + Sync {
    /// Recognize text in a rasterized page image.
    ///
    /// # Arguments
    /// * `image_data` — raw RGB pixels, row-major, 3 bytes per pixel.
    /// * `width`  — image width in pixels.
    /// * `height` — image height in pixels.
    fn recognize(&self, image_data: &[u8], width: u32, height: u32) -> Result<OcrResult, OcrError>;

    /// Short identifier shown in test metadata (e.g. `"ocrs"`, `"mistral"`).
    fn name(&self) -> &str;
}

// ── Mistral OCR backend ──────────────────────────────────────────────────────

#[cfg(feature = "ocr-mistral")]
const MISTRAL_OCR_ENDPOINT: &str = "https://api.mistral.ai/v1/ocr";
#[cfg(feature = "ocr-mistral")]
const MISTRAL_OCR_MODEL: &str = "mistral-ocr-latest";

/// Blocking HTTP OCR backend for the Mistral OCR API.
#[cfg(feature = "ocr-mistral")]
pub struct MistralOcrBackend {
    api_key: String,
    client: reqwest::blocking::Client,
}

#[cfg(feature = "ocr-mistral")]
impl MistralOcrBackend {
    /// Create a backend from an API key.
    pub fn new(api_key: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            client: reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .expect("building reqwest blocking client"),
        }
    }

    /// Create a backend from the `MISTRAL_API_KEY` environment variable.
    pub fn from_env() -> Result<Self, OcrError> {
        let key = std::env::var("MISTRAL_API_KEY").map_err(|_| OcrError::NoEngine)?;
        Ok(Self::new(&key))
    }
}

#[cfg(feature = "ocr-mistral")]
impl OcrBackend for MistralOcrBackend {
    fn recognize(&self, image_data: &[u8], width: u32, height: u32) -> Result<OcrResult, OcrError> {
        let image_url = rgb_data_url(image_data, width, height)?;
        let model =
            std::env::var("MISTRAL_OCR_MODEL").unwrap_or_else(|_| MISTRAL_OCR_MODEL.to_string());
        let body = MistralOcrRequest {
            model: &model,
            document: MistralOcrDocument {
                kind: "image_url",
                image_url: &image_url,
            },
        };
        let body_json = serde_json::to_vec(&body)
            .map_err(|e| OcrError::RecognitionFailed(format!("serialize request: {e}")))?;

        let response = self
            .client
            .post(MISTRAL_OCR_ENDPOINT)
            .header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {}", self.api_key),
            )
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body_json)
            .send()
            .map_err(|e| OcrError::RecognitionFailed(format!("Mistral OCR request failed: {e}")))?;

        let status = response.status();
        let response_text = response
            .text()
            .map_err(|e| OcrError::RecognitionFailed(format!("read Mistral OCR response: {e}")))?;

        if !status.is_success() {
            return Err(OcrError::RecognitionFailed(format!(
                "Mistral OCR returned {status}: {response_text}"
            )));
        }

        let parsed: MistralOcrResponse = serde_json::from_str(&response_text)
            .map_err(|e| OcrError::RecognitionFailed(format!("parse Mistral OCR response: {e}")))?;
        let text = mistral_markdown_text(&parsed);
        let confidence = if text.trim().is_empty() { 0.0 } else { 0.95 };

        Ok(OcrResult {
            text,
            words: Vec::new(),
            confidence,
        })
    }

    fn name(&self) -> &str {
        "mistral"
    }
}

#[cfg(feature = "ocr-mistral")]
#[derive(Serialize)]
struct MistralOcrRequest<'a> {
    model: &'a str,
    document: MistralOcrDocument<'a>,
}

#[cfg(feature = "ocr-mistral")]
#[derive(Serialize)]
struct MistralOcrDocument<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
    image_url: &'a str,
}

#[cfg(feature = "ocr-mistral")]
#[derive(Deserialize)]
struct MistralOcrResponse {
    #[serde(default)]
    pages: Vec<MistralOcrPage>,
}

#[cfg(feature = "ocr-mistral")]
#[derive(Deserialize)]
struct MistralOcrPage {
    markdown: Option<String>,
}

#[cfg(feature = "ocr-mistral")]
fn rgb_data_url(image_data: &[u8], width: u32, height: u32) -> Result<String, OcrError> {
    let expected_len = (width as usize)
        .checked_mul(height as usize)
        .and_then(|px| px.checked_mul(3))
        .ok_or_else(|| OcrError::ImageError("image dimensions overflowed".into()))?;

    if image_data.len() != expected_len {
        return Err(OcrError::ImageError(format!(
            "expected {expected_len} RGB bytes, got {}",
            image_data.len()
        )));
    }

    let image = RgbImage::from_raw(width, height, image_data.to_vec()).ok_or_else(|| {
        OcrError::ImageError("failed to create RGB image from raw OCR buffer".into())
    })?;

    let mut cursor = Cursor::new(Vec::new());
    DynamicImage::ImageRgb8(image)
        .write_to(&mut cursor, ImageFormat::Png)
        .map_err(|e| OcrError::ImageError(format!("encode PNG for OCR request: {e}")))?;

    let encoded = base64::engine::general_purpose::STANDARD.encode(cursor.into_inner());
    Ok(format!("data:image/png;base64,{encoded}"))
}

#[cfg(feature = "ocr-mistral")]
fn mistral_markdown_text(response: &MistralOcrResponse) -> String {
    response
        .pages
        .iter()
        .filter_map(|page| page.markdown.as_deref())
        .map(str::trim)
        .filter(|markdown| !markdown.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

// ── Paddle ONNX backend ──────────────────────────────────────────────────────

/// OCR backend backed by `pdf-ocr`'s PaddleOCR ONNX pipeline.
#[cfg(feature = "ocr-onnx")]
pub struct PaddleOnnxBackend {
    engine: pdf_ocr::PaddleOcrEngine,
}

#[cfg(feature = "ocr-onnx")]
impl PaddleOnnxBackend {
    /// Create the backend using the default PaddleOCR model cache/download flow.
    pub fn new() -> Result<Self, OcrError> {
        let engine = pdf_ocr::PaddleOcrEngine::new()
            .map_err(|e| OcrError::RecognitionFailed(format!("init PaddleOCR: {e}")))?;
        Ok(Self { engine })
    }

    /// Create the backend from the environment.
    ///
    /// The underlying PaddleOCR engine resolves its own model cache and
    /// download locations, so this is currently equivalent to [`Self::new`].
    pub fn from_env() -> Result<Self, OcrError> {
        Self::new()
    }
}

#[cfg(feature = "ocr-onnx")]
impl OcrBackend for PaddleOnnxBackend {
    fn recognize(&self, image_data: &[u8], width: u32, height: u32) -> Result<OcrResult, OcrError> {
        use pdf_ocr::OcrEngine;

        let result = self
            .engine
            .recognize(image_data, width, height, 300)
            .map_err(|e| OcrError::RecognitionFailed(format!("PaddleOCR recognize: {e}")))?;
        let text = result.full_text();
        let words = result
            .words
            .into_iter()
            .map(|word| OcrWord {
                text: word.text,
                bbox: [
                    word.bbox_px[0] as f32,
                    word.bbox_px[1] as f32,
                    word.bbox_px[2].saturating_sub(word.bbox_px[0]) as f32,
                    word.bbox_px[3].saturating_sub(word.bbox_px[1]) as f32,
                ],
                confidence: word.confidence,
            })
            .collect();

        Ok(OcrResult {
            text,
            words,
            confidence: result.confidence,
        })
    }

    fn name(&self) -> &str {
        "paddle-onnx"
    }
}

// ── ocrs backend ─────────────────────────────────────────────────────────────

/// Pure-Rust OCR backend backed by the [`ocrs`](https://crates.io/crates/ocrs)
/// engine (which uses ONNX models via `rten`).
///
/// # Model files
///
/// `ocrs` requires two pre-trained model files in `.rten` format:
///
/// | Model | Default env var | Default path |
/// |---|---|---|
/// | Text detection | `OCRS_DETECTION_MODEL` | `~/.cache/ocrs/text-detection.rten` |
/// | Text recognition | `OCRS_RECOGNITION_MODEL` | `~/.cache/ocrs/text-recognition.rten` |
///
/// Download links:
/// - <https://ocrs-models.s3-accelerate.amazonaws.com/text-detection.rten>
/// - <https://ocrs-models.s3-accelerate.amazonaws.com/text-recognition.rten>
///
/// # WASM compatibility
///
/// This backend is WASM-compatible. On WASM use [`OcrsBackend::from_bytes`]
/// since filesystem access is unavailable.
#[cfg(feature = "ocr")]
pub struct OcrsBackend {
    engine: ocrs::OcrEngine,
}

#[cfg(feature = "ocr")]
impl OcrsBackend {
    /// Create from pre-loaded model bytes.
    ///
    /// Suitable for WASM where filesystem access is unavailable.
    /// Pass the raw `.rten` file bytes for each model.
    pub fn from_bytes(detection: &[u8], recognition: &[u8]) -> Result<Self, OcrError> {
        let det = rten::Model::load(detection.to_vec()).map_err(|_| OcrError::NoEngine)?;
        let rec = rten::Model::load(recognition.to_vec()).map_err(|_| OcrError::NoEngine)?;
        Self::build(det, rec)
    }

    /// Create from filesystem paths to the `.rten` model files.
    ///
    /// Not available on WASM (use [`OcrsBackend::from_bytes`] instead).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn from_files(
        detection_path: impl AsRef<std::path::Path>,
        recognition_path: impl AsRef<std::path::Path>,
    ) -> Result<Self, OcrError> {
        let det = rten::Model::load_file(detection_path).map_err(|_| OcrError::NoEngine)?;
        let rec = rten::Model::load_file(recognition_path).map_err(|_| OcrError::NoEngine)?;
        Self::build(det, rec)
    }

    /// Try to initialize from standard locations.
    ///
    /// Checks `OCRS_DETECTION_MODEL` / `OCRS_RECOGNITION_MODEL` env vars first,
    /// then falls back to `~/.cache/ocrs/{text-detection,text-recognition}.rten`.
    ///
    /// Returns `Err(OcrError::NoEngine)` when no models are found.
    /// Not available on WASM (use [`OcrsBackend::from_bytes`] instead).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn try_default() -> Result<Self, OcrError> {
        let det_path: std::path::PathBuf = std::env::var("OCRS_DETECTION_MODEL")
            .ok()
            .map(std::path::PathBuf::from)
            .or_else(|| default_model_path("text-detection.rten"))
            .ok_or(OcrError::NoEngine)?;
        let rec_path: std::path::PathBuf = std::env::var("OCRS_RECOGNITION_MODEL")
            .ok()
            .map(std::path::PathBuf::from)
            .or_else(|| default_model_path("text-recognition.rten"))
            .ok_or(OcrError::NoEngine)?;
        if !det_path.exists() || !rec_path.exists() {
            return Err(OcrError::NoEngine);
        }
        Self::from_files(det_path, rec_path)
    }

    fn build(detection: rten::Model, recognition: rten::Model) -> Result<Self, OcrError> {
        let engine = ocrs::OcrEngine::new(ocrs::OcrEngineParams {
            detection_model: Some(detection),
            recognition_model: Some(recognition),
            ..Default::default()
        })
        .map_err(|_| OcrError::NoEngine)?;
        Ok(Self { engine })
    }
}

/// Returns `~/.cache/ocrs/<filename>` as a `Some(PathBuf)` if the home dir is
/// resolvable, `None` otherwise.
#[cfg(all(feature = "ocr", not(target_arch = "wasm32")))]
fn default_model_path(filename: &str) -> Option<std::path::PathBuf> {
    dirs_sys::home_dir().map(|h: std::path::PathBuf| h.join(".cache").join("ocrs").join(filename))
}

#[cfg(feature = "ocr")]
impl OcrBackend for OcrsBackend {
    fn recognize(&self, image_data: &[u8], width: u32, height: u32) -> Result<OcrResult, OcrError> {
        let image_source = ocrs::ImageSource::from_bytes(image_data, (width, height))
            .map_err(|e| OcrError::ImageError(e.to_string()))?;

        let input = self
            .engine
            .prepare_input(image_source)
            .map_err(|e| OcrError::RecognitionFailed(e.to_string()))?;

        let text = self
            .engine
            .get_text(&input)
            .map_err(|e| OcrError::RecognitionFailed(e.to_string()))?;

        let words = text
            .split_whitespace()
            .map(|w| OcrWord {
                text: w.to_string(),
                bbox: [0.0, 0.0, 0.0, 0.0],
                confidence: 1.0,
            })
            .collect::<Vec<_>>();

        let confidence = if text.is_empty() { 0.0 } else { 1.0 };

        Ok(OcrResult {
            text,
            words,
            confidence,
        })
    }

    fn name(&self) -> &str {
        "ocrs"
    }
}

// ── Convenience functions ────────────────────────────────────────────────────

/// Return the first available OCR backend in priority order.
///
/// The current order is:
/// 1. `ocr-mistral` with `MISTRAL_API_KEY`
/// 2. `ocr-onnx` via the Paddle ONNX adapter
/// 3. `ocr` via local `ocrs` model files
#[cfg(not(target_arch = "wasm32"))]
pub fn best_available_backend() -> Result<Box<dyn OcrBackend>, OcrError> {
    #[cfg(feature = "ocr-mistral")]
    if let Ok(backend) = MistralOcrBackend::from_env() {
        return Ok(Box::new(backend));
    }

    #[cfg(feature = "ocr-onnx")]
    if let Ok(backend) = PaddleOnnxBackend::from_env() {
        return Ok(Box::new(backend));
    }

    #[cfg(feature = "ocr")]
    if let Ok(backend) = OcrsBackend::try_default() {
        return Ok(Box::new(backend));
    }

    Err(OcrError::NoEngine)
}

/// Run OCR on a rendered page image using the default `ocrs` backend.
///
/// Initialises `OcrsBackend` from the standard model paths on every call.
/// For repeated use, create an `OcrsBackend` once and call
/// `ocr_page` with a reference to it.
#[cfg(all(feature = "ocr", not(target_arch = "wasm32")))]
pub fn ocr_page_default(image_data: &[u8], width: u32, height: u32) -> Result<OcrResult, OcrError> {
    let backend = OcrsBackend::try_default()?;
    backend.recognize(image_data, width, height)
}

#[cfg(all(test, feature = "ocr-mistral", not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    fn ocr_mistral_name_is_stable() {
        let backend = MistralOcrBackend::new("test-key");
        assert_eq!(backend.name(), "mistral");
    }

    #[test]
    fn ocr_mistral_from_env_reads_api_key() {
        let original = std::env::var("MISTRAL_API_KEY").ok();
        std::env::set_var("MISTRAL_API_KEY", "test-key");

        let backend = MistralOcrBackend::from_env().expect("Mistral backend from env");
        assert_eq!(backend.name(), "mistral");

        if let Some(value) = original {
            std::env::set_var("MISTRAL_API_KEY", value);
        } else {
            std::env::remove_var("MISTRAL_API_KEY");
        }
    }

    #[test]
    fn ocr_mistral_best_available_prefers_mistral() {
        let original = std::env::var("MISTRAL_API_KEY").ok();
        std::env::set_var("MISTRAL_API_KEY", "test-key");

        let backend = best_available_backend().expect("best OCR backend");
        assert_eq!(backend.name(), "mistral");

        if let Some(value) = original {
            std::env::set_var("MISTRAL_API_KEY", value);
        } else {
            std::env::remove_var("MISTRAL_API_KEY");
        }
    }

    #[test]
    fn ocr_mistral_encodes_rgb_image_as_png_data_url() {
        let rgb = vec![
            255, 255, 255, 0, 0, 0, //
            0, 0, 0, 255, 255, 255,
        ];
        let data_url = rgb_data_url(&rgb, 2, 2).expect("PNG data URL");
        assert!(data_url.starts_with("data:image/png;base64,"));
        assert!(data_url.len() > "data:image/png;base64,".len());
    }

    #[test]
    fn ocr_mistral_collects_markdown_pages() {
        let response = MistralOcrResponse {
            pages: vec![
                MistralOcrPage {
                    markdown: Some("  Hello  ".into()),
                },
                MistralOcrPage {
                    markdown: Some("World".into()),
                },
                MistralOcrPage { markdown: None },
            ],
        };

        assert_eq!(mistral_markdown_text(&response), "Hello\n\nWorld");
    }

    #[test]
    fn ocr_mistral_live_smoke() {
        if std::env::var_os("MISTRAL_API_KEY").is_none() {
            return;
        }

        let backend = MistralOcrBackend::from_env().expect("Mistral backend from env");
        let (pixels, width, height) = test_text_image("TEST 123");
        match backend.recognize(&pixels, width, height) {
            Ok(result) => assert!(result.confidence >= 0.0),
            Err(OcrError::RecognitionFailed(message))
                if message.contains("401 Unauthorized") || message.contains("403 Forbidden") =>
            {
                return;
            }
            Err(error) => panic!("live Mistral OCR request: {error}"),
        }
    }

    fn test_text_image(text: &str) -> (Vec<u8>, u32, u32) {
        let scale = 8usize;
        let glyph_width = 5usize;
        let glyph_height = 7usize;
        let spacing = 2usize;
        let margin = 12usize;
        let width = margin * 2
            + text
                .chars()
                .map(|ch| match ch {
                    ' ' => scale * 3,
                    _ => glyph_width * scale + spacing * scale,
                })
                .sum::<usize>();
        let height = margin * 2 + glyph_height * scale;
        let mut pixels = vec![255u8; width * height * 3];
        let mut cursor_x = margin;

        for ch in text.chars() {
            if ch == ' ' {
                cursor_x += scale * 3;
                continue;
            }

            draw_glyph(
                &mut pixels,
                width,
                cursor_x,
                margin,
                scale,
                glyph_pattern(ch),
            );
            cursor_x += glyph_width * scale + spacing * scale;
        }

        (pixels, width as u32, height as u32)
    }

    fn draw_glyph(
        pixels: &mut [u8],
        image_width: usize,
        offset_x: usize,
        offset_y: usize,
        scale: usize,
        glyph: [&str; 7],
    ) {
        for (row, pattern) in glyph.into_iter().enumerate() {
            for (col, bit) in pattern.bytes().enumerate() {
                if bit != b'#' {
                    continue;
                }

                for dy in 0..scale {
                    for dx in 0..scale {
                        let x = offset_x + col * scale + dx;
                        let y = offset_y + row * scale + dy;
                        let idx = (y * image_width + x) * 3;
                        pixels[idx] = 0;
                        pixels[idx + 1] = 0;
                        pixels[idx + 2] = 0;
                    }
                }
            }
        }
    }

    fn glyph_pattern(ch: char) -> [&'static str; 7] {
        match ch {
            '1' => [
                "..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###.",
            ],
            '2' => [
                ".###.", "#...#", "....#", "...#.", "..#..", ".#...", "#####",
            ],
            '3' => [
                ".###.", "#...#", "....#", "..##.", "....#", "#...#", ".###.",
            ],
            'E' => [
                "#####", "#....", "#....", "####.", "#....", "#....", "#####",
            ],
            'S' => [
                ".####", "#....", "#....", ".###.", "....#", "....#", "####.",
            ],
            'T' => [
                "#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#..",
            ],
            _ => panic!("unsupported glyph: {ch}"),
        }
    }
}
