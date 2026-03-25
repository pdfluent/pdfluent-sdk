//! OCR backend trait and implementations.
//!
//! ## OCR Backend Extension
//!
//! The [`OcrBackend`] trait is public. Callers can implement their own backends
//! for specific use cases:
//!
//! - **ocrs** (this crate, `ocr` feature): pure-Rust Tesseract-style engine.
//!   WASM-compatible. Requires external model files (`.rten` format). This
//!   backend is currently beta: internal scanned-form benchmarks are still
//!   below the 80% similarity target, so treat results as best-effort.
//!
//! - **Tesseract** (`ocr-tesseract` feature, planned): supports 100+ languages
//!   including CJK and Arabic. Requires system Tesseract + leptonica.
//!
//! - **PaddleOCR ONNX** (`ocr-onnx` feature): higher-accuracy backend for
//!   scanned documents. Uses the workspace `pdf-ocr` crate and ONNX Runtime.
//!
//! - **Custom**: implement [`OcrBackend`] directly for proprietary or
//!   cloud-based OCR (Google Vision, AWS Textract, etc.).

use std::fmt;

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

    /// Short identifier shown in test metadata (e.g. `"ocrs"`, `"tesseract"`).
    fn name(&self) -> &str;
}

// ── ocrs backend ─────────────────────────────────────────────────────────────

/// Pure-Rust OCR backend backed by the [`ocrs`](https://crates.io/crates/ocrs)
/// engine (which uses ONNX models via `rten`).
///
/// This backend is currently beta. It works on clean Latin-script scans, but
/// scanned-form accuracy remains below the project target in internal testing.
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

// ── PaddleOCR ONNX backend ───────────────────────────────────────────────────

/// PaddleOCR backend backed by the workspace `pdf-ocr` crate.
///
/// Models are downloaded on first use into `~/.cache/xfa/ocr-models/`.
/// Runtime inference requires a loadable ONNX Runtime shared library; set
/// `ORT_DYLIB_PATH` when your host does not expose it via the default loader.
#[cfg(feature = "ocr-onnx")]
pub struct PaddleOnnxBackend {
    engine: pdf_ocr::PaddleOcrEngine,
}

#[cfg(feature = "ocr-onnx")]
impl PaddleOnnxBackend {
    /// Create a backend with the default PaddleOCR ONNX configuration.
    pub fn new() -> Result<Self, OcrError> {
        let engine = pdf_ocr::PaddleOcrEngine::new()
            .map_err(|e| OcrError::RecognitionFailed(e.to_string()))?;
        Ok(Self { engine })
    }

    /// Create a backend with a custom PaddleOCR ONNX configuration.
    pub fn with_config(config: pdf_ocr::paddle::PaddleOcrConfig) -> Result<Self, OcrError> {
        let engine = pdf_ocr::PaddleOcrEngine::with_config(config)
            .map_err(|e| OcrError::RecognitionFailed(e.to_string()))?;
        Ok(Self { engine })
    }
}

#[cfg(feature = "ocr-onnx")]
impl OcrBackend for PaddleOnnxBackend {
    fn recognize(&self, image_data: &[u8], width: u32, height: u32) -> Result<OcrResult, OcrError> {
        use pdf_ocr::OcrEngine as _;

        let result = self
            .engine
            .recognize(image_data, width, height, 300)
            .map_err(|e| OcrError::RecognitionFailed(e.to_string()))?;

        let confidence = result.confidence;
        let words = result
            .words
            .into_iter()
            .map(|word| {
                let [x0, y0, x1, y1] = word.bbox_px;
                OcrWord {
                    text: word.text,
                    bbox: [
                        x0 as f32,
                        y0 as f32,
                        x1.saturating_sub(x0) as f32,
                        y1.saturating_sub(y0) as f32,
                    ],
                    confidence: word.confidence,
                }
            })
            .collect::<Vec<_>>();
        let text = words
            .iter()
            .map(|word| word.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        Ok(OcrResult {
            text,
            words,
            confidence,
        })
    }

    fn name(&self) -> &str {
        "paddle-onnx"
    }
}

// ── Convenience function ──────────────────────────────────────────────────────

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
