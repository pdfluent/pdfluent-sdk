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
//! - **Tesseract** (`ocr-tesseract` feature, planned): supports 100+ languages
//!   including CJK and Arabic. Requires system Tesseract + leptonica.
//!
//! - **PaddleOCR ONNX** (`ocr-onnx` feature): native ONNX Runtime-backed
//!   PaddleOCR loading pre-converted models from env vars or
//!   `~/.cache/paddle-ocr/`.
//!
//! - **PaddleOCR** (`ocr-paddle` feature, legacy): highest accuracy on complex
//!   documents. Requires PaddlePaddle C++ runtime.
//!
//! - **Custom**: implement [`OcrBackend`] directly for proprietary or
//!   cloud-based OCR (Google Vision, AWS Textract, etc.).

use std::fmt;
#[cfg(all(feature = "ocr-onnx", not(target_arch = "wasm32")))]
use std::{path::PathBuf, sync::Mutex};

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
    cache_root().map(|root| root.join("ocrs").join(filename))
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

/// PaddleOCR backend powered by ONNX Runtime.
///
/// This type is intentionally minimal in the feature-flag branch: it wires the
/// model-loading surface and feature gating needed for modular builds, while
/// keeping the heavy native inference path isolated behind `ocr-onnx`.
#[cfg(all(feature = "ocr-onnx", not(target_arch = "wasm32")))]
pub struct PaddleOnnxBackend {
    det_session: Mutex<ort::session::Session>,
    rec_session: Mutex<ort::session::Session>,
    dictionary: Vec<String>,
}

#[cfg(all(feature = "ocr-onnx", not(target_arch = "wasm32")))]
impl PaddleOnnxBackend {
    /// Load the ONNX backend from explicit model and dictionary paths.
    pub fn new(det_model: &str, rec_model: &str, dict: &str) -> Result<Self, OcrError> {
        let det_session = build_onnx_session(det_model)?;
        let rec_session = build_onnx_session(rec_model)?;
        let dictionary = std::fs::read_to_string(dict)
            .map_err(|e| OcrError::RecognitionFailed(e.to_string()))?
            .lines()
            .filter(|line| !line.is_empty())
            .map(|line| line.to_string())
            .collect::<Vec<_>>();

        Ok(Self {
            det_session: Mutex::new(det_session),
            rec_session: Mutex::new(rec_session),
            dictionary,
        })
    }

    /// Load model paths from env vars or `~/.cache/paddle-ocr/`.
    ///
    /// Supported env vars:
    /// - `PADDLE_DET_MODEL`
    /// - `PADDLE_REC_MODEL`
    /// - `PADDLE_DICT`
    pub fn from_env() -> Result<Self, OcrError> {
        let cache = cache_root()
            .map(|root| root.join("paddle-ocr"))
            .ok_or(OcrError::NoEngine)?;
        let det = std::env::var_os("PADDLE_DET_MODEL")
            .map(PathBuf::from)
            .unwrap_or_else(|| cache.join("ch_PP-OCRv4_det.onnx"));
        let rec = std::env::var_os("PADDLE_REC_MODEL")
            .map(PathBuf::from)
            .unwrap_or_else(|| cache.join("ch_PP-OCRv4_rec.onnx"));
        let dict = std::env::var_os("PADDLE_DICT")
            .map(PathBuf::from)
            .unwrap_or_else(|| cache.join("ppocr_keys_v1.txt"));

        if !det.exists() || !rec.exists() || !dict.exists() {
            return Err(OcrError::NoEngine);
        }

        let det = det.to_string_lossy();
        let rec = rec.to_string_lossy();
        let dict = dict.to_string_lossy();
        Self::new(det.as_ref(), rec.as_ref(), dict.as_ref())
    }
}

#[cfg(all(feature = "ocr-onnx", not(target_arch = "wasm32")))]
impl OcrBackend for PaddleOnnxBackend {
    fn recognize(&self, image_data: &[u8], width: u32, height: u32) -> Result<OcrResult, OcrError> {
        let expected_len = width as usize * height as usize * 3;
        if image_data.len() != expected_len {
            return Err(OcrError::ImageError(format!(
                "expected {expected_len} RGB bytes for {width}x{height}, got {}",
                image_data.len()
            )));
        }
        if self.dictionary.is_empty() {
            return Err(OcrError::RecognitionFailed(
                "recognition dictionary is empty".to_string(),
            ));
        }

        let _det_session = self
            .det_session
            .lock()
            .map_err(|_| OcrError::RecognitionFailed("detection session lock poisoned".into()))?;
        let _rec_session = self
            .rec_session
            .lock()
            .map_err(|_| OcrError::RecognitionFailed("recognition session lock poisoned".into()))?;

        Err(OcrError::RecognitionFailed(
            "PaddleOCR ONNX runtime is feature-gated but inference is not wired in this branch"
                .to_string(),
        ))
    }

    fn name(&self) -> &str {
        "paddle-onnx"
    }
}

#[cfg(all(feature = "ocr-onnx", not(target_arch = "wasm32")))]
fn build_onnx_session(model_path: &str) -> Result<ort::session::Session, OcrError> {
    ort::session::Session::builder()
        .map_err(|e| OcrError::RecognitionFailed(e.to_string()))?
        .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)
        .map_err(|e| OcrError::RecognitionFailed(e.to_string()))?
        .with_intra_threads(4)
        .map_err(|e| OcrError::RecognitionFailed(e.to_string()))?
        .commit_from_file(model_path)
        .map_err(|e| OcrError::RecognitionFailed(format!("{model_path}: {e}")))
}

#[cfg(any(
    all(feature = "ocr", not(target_arch = "wasm32")),
    all(feature = "ocr-onnx", not(target_arch = "wasm32"))
))]
fn cache_root() -> Option<std::path::PathBuf> {
    if let Some(dir) = std::env::var_os("XDG_CACHE_HOME") {
        return Some(std::path::PathBuf::from(dir));
    }
    if let Some(dir) = std::env::var_os("LOCALAPPDATA") {
        return Some(std::path::PathBuf::from(dir));
    }
    std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".cache"))
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
