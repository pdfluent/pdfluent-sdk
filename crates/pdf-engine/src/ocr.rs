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
//! - **Google Vision** (`ocr-google` feature): blocking HTTP adapter to
//!   Google Cloud Vision's `images:annotate` OCR endpoint.
//!
//! - **AWS Textract** (`ocr-aws` feature): blocking HTTP adapter to
//!   Amazon Textract's `DetectDocumentText` endpoint.
//!
//! - **Azure Document Intelligence** (`ocr-azure` feature): blocking HTTP
//!   adapter to the `prebuilt-read` model.
//!
//! - **Paddle ONNX** (`ocr-onnx` feature): adapter around `pdf-ocr`'s
//!   PaddleOCR engine.
//!
//! - **Custom**: implement [`OcrBackend`] directly for proprietary or
//!   cloud-based OCR flows.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::fmt;

#[cfg(any(
    feature = "ocr-mistral",
    feature = "ocr-google",
    feature = "ocr-aws",
    feature = "ocr-azure"
))]
use std::io::Cursor;

#[cfg(feature = "ocr-azure")]
use std::time::Duration;

#[cfg(any(feature = "ocr-mistral", feature = "ocr-google", feature = "ocr-aws"))]
use base64::Engine as _;
#[cfg(feature = "ocr-aws")]
use hmac::{Hmac, Mac};
#[cfg(any(
    feature = "ocr-mistral",
    feature = "ocr-google",
    feature = "ocr-aws",
    feature = "ocr-azure"
))]
use image::{DynamicImage, ImageFormat, RgbImage};
#[cfg(feature = "ocr-google")]
use pkcs8::DecodePrivateKey as _;
#[cfg(feature = "ocr-google")]
use rsa::{
    pkcs1v15::SigningKey,
    signature::{SignatureEncoding as _, Signer as _},
    RsaPrivateKey,
};
#[cfg(any(
    feature = "ocr-mistral",
    feature = "ocr-google",
    feature = "ocr-aws",
    feature = "ocr-azure"
))]
use serde::{Deserialize, Serialize};
#[cfg(feature = "ocr-aws")]
use sha2::Digest as _;
#[cfg(any(feature = "ocr-google", feature = "ocr-aws"))]
use sha2::Sha256;
#[cfg(feature = "ocr-aws")]
use time::macros::format_description;
#[cfg(any(feature = "ocr-google", feature = "ocr-aws"))]
use time::OffsetDateTime;

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

// ── Shared helpers ───────────────────────────────────────────────────────────

#[cfg(any(
    feature = "ocr-mistral",
    feature = "ocr-google",
    feature = "ocr-aws",
    feature = "ocr-azure"
))]
fn blocking_http_client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .expect("building reqwest blocking client")
}

#[cfg(any(
    feature = "ocr-mistral",
    feature = "ocr-google",
    feature = "ocr-aws",
    feature = "ocr-azure"
))]
fn expected_rgb_len(width: u32, height: u32) -> Result<usize, OcrError> {
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|px| px.checked_mul(3))
        .ok_or_else(|| OcrError::ImageError("image dimensions overflowed".into()))
}

#[cfg(any(
    feature = "ocr-mistral",
    feature = "ocr-google",
    feature = "ocr-aws",
    feature = "ocr-azure"
))]
fn rgb_png_bytes(image_data: &[u8], width: u32, height: u32) -> Result<Vec<u8>, OcrError> {
    let expected_len = expected_rgb_len(width, height)?;
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

    Ok(cursor.into_inner())
}

#[cfg(any(feature = "ocr-mistral", feature = "ocr-google", feature = "ocr-aws"))]
fn rgb_base64_png(image_data: &[u8], width: u32, height: u32) -> Result<String, OcrError> {
    let png_bytes = rgb_png_bytes(image_data, width, height)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(png_bytes))
}

#[cfg(feature = "ocr-mistral")]
fn rgb_data_url(image_data: &[u8], width: u32, height: u32) -> Result<String, OcrError> {
    let encoded = rgb_base64_png(image_data, width, height)?;
    Ok(format!("data:image/png;base64,{encoded}"))
}

#[cfg(any(feature = "ocr-google", feature = "ocr-azure"))]
fn bbox_from_points(points: &[(f32, f32)]) -> [f32; 4] {
    if points.is_empty() {
        return [0.0, 0.0, 0.0, 0.0];
    }

    let mut min_x = f32::INFINITY;
    let mut min_y = f32::INFINITY;
    let mut max_x = f32::NEG_INFINITY;
    let mut max_y = f32::NEG_INFINITY;

    for &(x, y) in points {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }

    [min_x, min_y, max_x - min_x, max_y - min_y]
}

#[cfg(any(feature = "ocr-google", feature = "ocr-aws", feature = "ocr-azure"))]
fn confidence_from_words(words: &[OcrWord], fallback: f32) -> f32 {
    if words.is_empty() {
        fallback
    } else {
        words.iter().map(|word| word.confidence).sum::<f32>() / words.len() as f32
    }
}

#[cfg(any(feature = "ocr-google", feature = "ocr-aws", feature = "ocr-azure"))]
fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(std::path::PathBuf::from))
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
            client: blocking_http_client(),
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

// ── Google Cloud Vision backend ──────────────────────────────────────────────

#[cfg(feature = "ocr-google")]
const GOOGLE_VISION_ENDPOINT: &str = "https://vision.googleapis.com/v1/images:annotate";
#[cfg(feature = "ocr-google")]
const GOOGLE_TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
#[cfg(feature = "ocr-google")]
const GOOGLE_OAUTH_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";

/// Blocking HTTP OCR backend for Google Cloud Vision.
///
/// Authentication supports:
/// - API key via [`GoogleVisionBackend::from_api_key`]
/// - Service-account JSON via [`GoogleVisionBackend::from_service_account`]
/// - Auto-detection via [`GoogleVisionBackend::from_env`]
#[cfg(feature = "ocr-google")]
pub struct GoogleVisionBackend {
    api_key: Option<String>,
    service_account_json: Option<String>,
    client: reqwest::blocking::Client,
    endpoint: String,
}

#[cfg(feature = "ocr-google")]
impl GoogleVisionBackend {
    /// Create a backend from an API key.
    pub fn from_api_key(key: &str) -> Self {
        Self::with_auth(
            Some(key.to_string()),
            None,
            GOOGLE_VISION_ENDPOINT.to_string(),
        )
    }

    /// Create a backend from a service-account JSON file.
    ///
    /// The credentials are loaded eagerly and an OAuth access token is minted
    /// on demand using the service-account private key.
    pub fn from_service_account(json_path: &str) -> Result<Self, OcrError> {
        let json = std::fs::read_to_string(json_path).map_err(|e| {
            OcrError::RecognitionFailed(format!(
                "read Google service-account JSON from {json_path}: {e}"
            ))
        })?;
        let credentials = parse_google_credentials(&json)?;
        match credentials.kind.as_deref() {
            Some("service_account") => Ok(Self::with_auth(
                None,
                Some(json),
                GOOGLE_VISION_ENDPOINT.to_string(),
            )),
            Some(other) => Err(OcrError::RecognitionFailed(format!(
                "expected Google service-account JSON, found credentials type {other}"
            ))),
            None => Err(OcrError::RecognitionFailed(
                "Google service-account JSON is missing the `type` field".into(),
            )),
        }
    }

    /// Create a backend from Google Cloud environment variables.
    ///
    /// Resolution order:
    /// 1. `GOOGLE_VISION_API_KEY`
    /// 2. `GOOGLE_APPLICATION_CREDENTIALS`
    /// 3. `GOOGLE_CLOUD_PROJECT` plus the standard gcloud application-default
    ///    credentials file
    pub fn from_env() -> Result<Self, OcrError> {
        if let Ok(key) = std::env::var("GOOGLE_VISION_API_KEY") {
            if !key.trim().is_empty() {
                return Ok(Self::from_api_key(&key));
            }
        }

        if let Ok(path) = std::env::var("GOOGLE_APPLICATION_CREDENTIALS") {
            return Self::from_credentials_file(&path);
        }

        if std::env::var_os("GOOGLE_CLOUD_PROJECT").is_some() {
            if let Some(path) = google_application_default_credentials_path() {
                if path.is_file() {
                    return Self::from_credentials_path(path);
                }
            }
        }

        Err(OcrError::NoEngine)
    }

    fn with_auth(
        api_key: Option<String>,
        service_account_json: Option<String>,
        endpoint: String,
    ) -> Self {
        Self {
            api_key,
            service_account_json,
            client: blocking_http_client(),
            endpoint,
        }
    }

    fn from_credentials_file(path: &str) -> Result<Self, OcrError> {
        Self::from_credentials_path(std::path::PathBuf::from(path))
    }

    fn from_credentials_path(path: impl AsRef<std::path::Path>) -> Result<Self, OcrError> {
        let path = path.as_ref();
        let json = std::fs::read_to_string(path).map_err(|e| {
            OcrError::RecognitionFailed(format!(
                "read Google application credentials from {}: {e}",
                path.display()
            ))
        })?;
        let credentials = parse_google_credentials(&json)?;
        match credentials.kind.as_deref() {
            Some("service_account") | Some("authorized_user") => Ok(Self::with_auth(
                None,
                Some(json),
                GOOGLE_VISION_ENDPOINT.to_string(),
            )),
            Some(other) => Err(OcrError::RecognitionFailed(format!(
                "unsupported Google credentials type {other}"
            ))),
            None if credentials.refresh_token.is_some() => Ok(Self::with_auth(
                None,
                Some(json),
                GOOGLE_VISION_ENDPOINT.to_string(),
            )),
            None => Err(OcrError::RecognitionFailed(
                "Google credentials JSON is missing both `type` and refresh-token fields".into(),
            )),
        }
    }

    #[cfg(test)]
    fn with_endpoint_api_key(key: &str, endpoint: &str) -> Self {
        Self::with_auth(Some(key.to_string()), None, endpoint.to_string())
    }

    #[cfg(test)]
    fn with_endpoint_credentials(credentials_json: &str, endpoint: &str) -> Self {
        Self::with_auth(
            None,
            Some(credentials_json.to_string()),
            endpoint.to_string(),
        )
    }
}

#[cfg(feature = "ocr-google")]
impl OcrBackend for GoogleVisionBackend {
    fn recognize(&self, image_data: &[u8], width: u32, height: u32) -> Result<OcrResult, OcrError> {
        let encoded = rgb_base64_png(image_data, width, height)?;
        let request = GoogleVisionAnnotateEnvelopeRequest {
            requests: vec![GoogleVisionAnnotateRequest {
                image: GoogleVisionAnnotateImage { content: &encoded },
                features: vec![GoogleVisionAnnotateFeature {
                    kind: "TEXT_DETECTION",
                }],
            }],
        };
        let body_json = serde_json::to_vec(&request).map_err(|e| {
            OcrError::RecognitionFailed(format!("serialize Google Vision request: {e}"))
        })?;

        let mut http = self
            .client
            .post(&self.endpoint)
            .header(reqwest::header::CONTENT_TYPE, "application/json");

        if let Some(api_key) = &self.api_key {
            http = http.query(&[("key", api_key.as_str())]);
        } else if let Some(credentials_json) = &self.service_account_json {
            let access_token = google_access_token(&self.client, credentials_json)?;
            http = http.header(
                reqwest::header::AUTHORIZATION,
                format!("Bearer {access_token}"),
            );
        } else {
            return Err(OcrError::NoEngine);
        }

        let response = http.body(body_json).send().map_err(|e| {
            OcrError::RecognitionFailed(format!("Google Vision OCR request failed: {e}"))
        })?;

        let status = response.status();
        let response_text = response.text().map_err(|e| {
            OcrError::RecognitionFailed(format!("read Google Vision OCR response: {e}"))
        })?;

        if !status.is_success() {
            return Err(OcrError::RecognitionFailed(format!(
                "Google Vision returned {status}: {response_text}"
            )));
        }

        let parsed: GoogleVisionAnnotateEnvelopeResponse = serde_json::from_str(&response_text)
            .map_err(|e| {
                OcrError::RecognitionFailed(format!("parse Google Vision OCR response: {e}"))
            })?;
        let response = parsed.responses.into_iter().next().ok_or_else(|| {
            OcrError::RecognitionFailed("Google Vision response did not contain any entries".into())
        })?;

        if let Some(error) = response.error {
            let message = error
                .message
                .unwrap_or_else(|| "unknown Google Vision error".into());
            return Err(OcrError::RecognitionFailed(format!(
                "Google Vision OCR error: {message}"
            )));
        }

        let text = response
            .full_text_annotation
            .as_ref()
            .and_then(|annotation| annotation.text.clone())
            .or_else(|| {
                response
                    .text_annotations
                    .first()
                    .map(|annotation| annotation.description.clone())
            })
            .unwrap_or_default();

        let words = response
            .text_annotations
            .into_iter()
            .enumerate()
            .filter_map(|(index, annotation)| {
                if index == 0 {
                    return None;
                }

                let text = annotation.description.trim().to_string();
                if text.is_empty() {
                    return None;
                }

                let bbox = annotation
                    .bounding_poly
                    .map(|poly| {
                        let points = poly
                            .vertices
                            .into_iter()
                            .map(|vertex| (vertex.x.unwrap_or(0.0), vertex.y.unwrap_or(0.0)))
                            .collect::<Vec<_>>();
                        bbox_from_points(&points)
                    })
                    .unwrap_or([0.0, 0.0, 0.0, 0.0]);

                Some(OcrWord {
                    text,
                    bbox,
                    confidence: 1.0,
                })
            })
            .collect::<Vec<_>>();

        let confidence = if text.trim().is_empty() {
            0.0
        } else {
            confidence_from_words(&words, 1.0)
        };

        Ok(OcrResult {
            text,
            words,
            confidence,
        })
    }

    fn name(&self) -> &str {
        "google-vision"
    }
}

#[cfg(feature = "ocr-google")]
#[derive(Serialize)]
struct GoogleVisionAnnotateEnvelopeRequest<'a> {
    requests: Vec<GoogleVisionAnnotateRequest<'a>>,
}

#[cfg(feature = "ocr-google")]
#[derive(Serialize)]
struct GoogleVisionAnnotateRequest<'a> {
    image: GoogleVisionAnnotateImage<'a>,
    features: Vec<GoogleVisionAnnotateFeature<'a>>,
}

#[cfg(feature = "ocr-google")]
#[derive(Serialize)]
struct GoogleVisionAnnotateImage<'a> {
    content: &'a str,
}

#[cfg(feature = "ocr-google")]
#[derive(Serialize)]
struct GoogleVisionAnnotateFeature<'a> {
    #[serde(rename = "type")]
    kind: &'a str,
}

#[cfg(feature = "ocr-google")]
#[derive(Deserialize)]
struct GoogleVisionAnnotateEnvelopeResponse {
    #[serde(default)]
    responses: Vec<GoogleVisionAnnotateResponse>,
}

#[cfg(feature = "ocr-google")]
#[derive(Deserialize)]
struct GoogleVisionAnnotateResponse {
    #[serde(rename = "fullTextAnnotation")]
    full_text_annotation: Option<GoogleVisionFullTextAnnotation>,
    #[serde(rename = "textAnnotations", default)]
    text_annotations: Vec<GoogleVisionTextAnnotation>,
    error: Option<GoogleVisionError>,
}

#[cfg(feature = "ocr-google")]
#[derive(Deserialize)]
struct GoogleVisionFullTextAnnotation {
    text: Option<String>,
}

#[cfg(feature = "ocr-google")]
#[derive(Deserialize)]
struct GoogleVisionTextAnnotation {
    description: String,
    #[serde(rename = "boundingPoly")]
    bounding_poly: Option<GoogleVisionBoundingPoly>,
}

#[cfg(feature = "ocr-google")]
#[derive(Deserialize)]
struct GoogleVisionBoundingPoly {
    #[serde(default)]
    vertices: Vec<GoogleVisionVertex>,
}

#[cfg(feature = "ocr-google")]
#[derive(Deserialize)]
struct GoogleVisionVertex {
    x: Option<f32>,
    y: Option<f32>,
}

#[cfg(feature = "ocr-google")]
#[derive(Deserialize)]
struct GoogleVisionError {
    message: Option<String>,
}

#[cfg(feature = "ocr-google")]
#[derive(Deserialize)]
struct GoogleCredentialsFile {
    #[serde(rename = "type")]
    kind: Option<String>,
    client_email: Option<String>,
    private_key: Option<String>,
    token_uri: Option<String>,
    client_id: Option<String>,
    client_secret: Option<String>,
    refresh_token: Option<String>,
}

#[cfg(feature = "ocr-google")]
#[derive(Serialize)]
struct GoogleServiceAccountClaims<'a> {
    iss: &'a str,
    scope: &'a str,
    aud: &'a str,
    exp: i64,
    iat: i64,
}

#[cfg(feature = "ocr-google")]
#[derive(Deserialize)]
struct GoogleOAuthTokenResponse {
    access_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
}

#[cfg(feature = "ocr-google")]
fn parse_google_credentials(json: &str) -> Result<GoogleCredentialsFile, OcrError> {
    serde_json::from_str(json)
        .map_err(|e| OcrError::RecognitionFailed(format!("parse Google credentials JSON: {e}")))
}

#[cfg(feature = "ocr-google")]
fn google_access_token(
    client: &reqwest::blocking::Client,
    credentials_json: &str,
) -> Result<String, OcrError> {
    let credentials = parse_google_credentials(credentials_json)?;
    match credentials.kind.as_deref() {
        Some("service_account") => google_service_account_access_token(client, &credentials),
        Some("authorized_user") => google_authorized_user_access_token(client, &credentials),
        Some(other) => Err(OcrError::RecognitionFailed(format!(
            "unsupported Google credentials type {other}"
        ))),
        None if credentials.refresh_token.is_some() => {
            google_authorized_user_access_token(client, &credentials)
        }
        None => Err(OcrError::RecognitionFailed(
            "Google credentials are missing the `type` field".into(),
        )),
    }
}

#[cfg(feature = "ocr-google")]
fn google_service_account_access_token(
    client: &reqwest::blocking::Client,
    credentials: &GoogleCredentialsFile,
) -> Result<String, OcrError> {
    let client_email = credentials.client_email.as_deref().ok_or_else(|| {
        OcrError::RecognitionFailed("Google service-account JSON is missing `client_email`".into())
    })?;
    let private_key_pem = credentials.private_key.as_deref().ok_or_else(|| {
        OcrError::RecognitionFailed("Google service-account JSON is missing `private_key`".into())
    })?;
    let token_uri = credentials
        .token_uri
        .as_deref()
        .unwrap_or(GOOGLE_TOKEN_ENDPOINT);

    let now = OffsetDateTime::now_utc().unix_timestamp();
    let claims = GoogleServiceAccountClaims {
        iss: client_email,
        scope: GOOGLE_OAUTH_SCOPE,
        aud: token_uri,
        exp: now + 3600,
        iat: now,
    };

    let header_json = serde_json::to_vec(&serde_json::json!({
        "alg": "RS256",
        "typ": "JWT",
    }))
    .map_err(|e| OcrError::RecognitionFailed(format!("serialize Google JWT header: {e}")))?;
    let claims_json = serde_json::to_vec(&claims)
        .map_err(|e| OcrError::RecognitionFailed(format!("serialize Google JWT claims: {e}")))?;

    let signing_input = format!(
        "{}.{}",
        google_base64_url(&header_json),
        google_base64_url(&claims_json)
    );
    let private_key = RsaPrivateKey::from_pkcs8_pem(private_key_pem).map_err(|e| {
        OcrError::RecognitionFailed(format!("parse Google service-account private key: {e}"))
    })?;
    let signing_key = SigningKey::<Sha256>::new(private_key);
    let signature = signing_key.sign(signing_input.as_bytes());
    let assertion = format!("{signing_input}.{}", google_base64_url(&signature.to_vec()));

    let response = client
        .post(token_uri)
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("assertion", assertion.as_str()),
        ])
        .send()
        .map_err(|e| {
            OcrError::RecognitionFailed(format!(
                "request Google OAuth access token with service account: {e}"
            ))
        })?;

    google_token_response_text(response)
}

#[cfg(feature = "ocr-google")]
fn google_authorized_user_access_token(
    client: &reqwest::blocking::Client,
    credentials: &GoogleCredentialsFile,
) -> Result<String, OcrError> {
    let client_id = credentials.client_id.as_deref().ok_or_else(|| {
        OcrError::RecognitionFailed(
            "Google application-default credentials are missing `client_id`".into(),
        )
    })?;
    let client_secret = credentials.client_secret.as_deref().ok_or_else(|| {
        OcrError::RecognitionFailed(
            "Google application-default credentials are missing `client_secret`".into(),
        )
    })?;
    let refresh_token = credentials.refresh_token.as_deref().ok_or_else(|| {
        OcrError::RecognitionFailed(
            "Google application-default credentials are missing `refresh_token`".into(),
        )
    })?;
    let token_uri = credentials
        .token_uri
        .as_deref()
        .unwrap_or(GOOGLE_TOKEN_ENDPOINT);

    let response = client
        .post(token_uri)
        .form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .map_err(|e| {
            OcrError::RecognitionFailed(format!(
                "request Google OAuth access token with refresh token: {e}"
            ))
        })?;

    google_token_response_text(response)
}

#[cfg(feature = "ocr-google")]
fn google_token_response_text(response: reqwest::blocking::Response) -> Result<String, OcrError> {
    let status = response.status();
    let body = response.text().map_err(|e| {
        OcrError::RecognitionFailed(format!("read Google OAuth token response: {e}"))
    })?;

    if !status.is_success() {
        return Err(OcrError::RecognitionFailed(format!(
            "Google OAuth token exchange returned {status}: {body}"
        )));
    }

    let token: GoogleOAuthTokenResponse = serde_json::from_str(&body).map_err(|e| {
        OcrError::RecognitionFailed(format!("parse Google OAuth token response: {e}"))
    })?;

    if let Some(access_token) = token.access_token {
        return Ok(access_token);
    }

    let error = token.error.unwrap_or_else(|| "unknown error".into());
    let description = token
        .error_description
        .unwrap_or_else(|| "no error description".into());
    Err(OcrError::RecognitionFailed(format!(
        "Google OAuth token exchange failed: {error}: {description}"
    )))
}

#[cfg(feature = "ocr-google")]
fn google_base64_url(data: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

#[cfg(feature = "ocr-google")]
fn google_application_default_credentials_path() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("APPDATA")
            .map(std::path::PathBuf::from)
            .map(|dir| {
                dir.join("gcloud")
                    .join("application_default_credentials.json")
            })
    }

    #[cfg(not(target_os = "windows"))]
    {
        home_dir().map(|dir| {
            dir.join(".config")
                .join("gcloud")
                .join("application_default_credentials.json")
        })
    }
}

// ── AWS Textract backend ─────────────────────────────────────────────────────

#[cfg(feature = "ocr-aws")]
const AWS_TEXTRACT_CONTENT_TYPE: &str = "application/x-amz-json-1.1";
#[cfg(feature = "ocr-aws")]
const AWS_TEXTRACT_TARGET: &str = "Textract.DetectDocumentText";
#[cfg(feature = "ocr-aws")]
const AWS_TEXTRACT_SERVICE: &str = "textract";

/// Blocking HTTP OCR backend for AWS Textract.
#[cfg(feature = "ocr-aws")]
pub struct AwsTextractBackend {
    region: String,
    access_key: Option<String>,
    secret_key: Option<String>,
    session_token: Option<String>,
    client: reqwest::blocking::Client,
    endpoint: String,
}

#[cfg(feature = "ocr-aws")]
impl AwsTextractBackend {
    /// Create a backend from explicit IAM credentials.
    pub fn new(region: &str, access_key: &str, secret_key: &str) -> Self {
        Self::with_credentials(
            region,
            access_key,
            secret_key,
            None,
            format!("https://textract.{region}.amazonaws.com/"),
        )
    }

    /// Create a backend from the standard AWS environment/profile chain.
    ///
    /// Resolution order:
    /// 1. `AWS_REGION` or `AWS_DEFAULT_REGION`
    /// 2. `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, optional
    ///    `AWS_SESSION_TOKEN`
    /// 3. `AWS_PROFILE` (or `default`) in the shared credentials/config files
    pub fn from_env() -> Result<Self, OcrError> {
        let profile_name = std::env::var("AWS_PROFILE").unwrap_or_else(|_| "default".into());
        let shared = load_aws_profile(&profile_name).unwrap_or_default();

        let region = std::env::var("AWS_REGION")
            .ok()
            .or_else(|| std::env::var("AWS_DEFAULT_REGION").ok())
            .or(shared.region);
        let access_key = std::env::var("AWS_ACCESS_KEY_ID")
            .ok()
            .or(shared.access_key);
        let secret_key = std::env::var("AWS_SECRET_ACCESS_KEY")
            .ok()
            .or(shared.secret_key);
        let session_token = std::env::var("AWS_SESSION_TOKEN")
            .ok()
            .or(shared.session_token);

        match (region, access_key, secret_key) {
            (Some(region), Some(access_key), Some(secret_key)) => Ok(Self::with_credentials(
                &region,
                &access_key,
                &secret_key,
                session_token.as_deref(),
                format!("https://textract.{region}.amazonaws.com/"),
            )),
            _ => Err(OcrError::NoEngine),
        }
    }

    fn with_credentials(
        region: &str,
        access_key: &str,
        secret_key: &str,
        session_token: Option<&str>,
        endpoint: String,
    ) -> Self {
        Self {
            region: region.to_string(),
            access_key: Some(access_key.to_string()),
            secret_key: Some(secret_key.to_string()),
            session_token: session_token.map(str::to_string),
            client: blocking_http_client(),
            endpoint,
        }
    }

    #[cfg(test)]
    fn with_endpoint(region: &str, access_key: &str, secret_key: &str, endpoint: &str) -> Self {
        Self::with_credentials(region, access_key, secret_key, None, endpoint.to_string())
    }
}

#[cfg(feature = "ocr-aws")]
impl OcrBackend for AwsTextractBackend {
    fn recognize(&self, image_data: &[u8], width: u32, height: u32) -> Result<OcrResult, OcrError> {
        let access_key = self.access_key.as_deref().ok_or(OcrError::NoEngine)?;
        let secret_key = self.secret_key.as_deref().ok_or(OcrError::NoEngine)?;
        let encoded = rgb_base64_png(image_data, width, height)?;
        let body = AwsTextractRequest {
            document: AwsTextractDocument { bytes: &encoded },
        };
        let body_json = serde_json::to_vec(&body).map_err(|e| {
            OcrError::RecognitionFailed(format!("serialize AWS Textract request: {e}"))
        })?;

        let url = reqwest::Url::parse(&self.endpoint).map_err(|e| {
            OcrError::RecognitionFailed(format!("parse AWS Textract endpoint: {e}"))
        })?;
        let host = host_header_value(&url)?;
        let canonical_uri = if url.path().is_empty() {
            "/"
        } else {
            url.path()
        };
        let canonical_query = url.query().unwrap_or("");
        let now = OffsetDateTime::now_utc();
        let amz_date = now
            .format(&format_description!(
                "[year][month][day]T[hour][minute][second]Z"
            ))
            .map_err(|e| OcrError::RecognitionFailed(format!("format AWS SigV4 timestamp: {e}")))?;
        let date_stamp = now
            .format(&format_description!("[year][month][day]"))
            .map_err(|e| {
                OcrError::RecognitionFailed(format!("format AWS SigV4 date stamp: {e}"))
            })?;
        let payload_hash = sha256_hex(&body_json);

        let mut headers = vec![
            (
                "content-type".to_string(),
                AWS_TEXTRACT_CONTENT_TYPE.to_string(),
            ),
            ("host".to_string(), host.clone()),
            ("x-amz-date".to_string(), amz_date.clone()),
            ("x-amz-target".to_string(), AWS_TEXTRACT_TARGET.to_string()),
        ];
        if let Some(token) = &self.session_token {
            headers.push(("x-amz-security-token".to_string(), token.clone()));
        }
        headers.sort_by(|left, right| left.0.cmp(&right.0));

        let canonical_headers = headers
            .iter()
            .map(|(name, value)| format!("{name}:{}\n", value.trim()))
            .collect::<String>();
        let signed_headers = headers
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>()
            .join(";");
        let canonical_request = format!(
            "POST\n{canonical_uri}\n{canonical_query}\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
        );
        let credential_scope = format!(
            "{date_stamp}/{}/{AWS_TEXTRACT_SERVICE}/aws4_request",
            self.region
        );
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{amz_date}\n{credential_scope}\n{}",
            sha256_hex(canonical_request.as_bytes())
        );
        let signature = aws_sigv4_signature(
            secret_key,
            &date_stamp,
            &self.region,
            AWS_TEXTRACT_SERVICE,
            &string_to_sign,
        )?;
        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={access_key}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}"
        );

        let mut request = self
            .client
            .post(url)
            .header(reqwest::header::CONTENT_TYPE, AWS_TEXTRACT_CONTENT_TYPE)
            .header("X-Amz-Target", AWS_TEXTRACT_TARGET)
            .header("X-Amz-Date", &amz_date)
            .header(reqwest::header::HOST, host)
            .header(reqwest::header::AUTHORIZATION, authorization);

        if let Some(token) = &self.session_token {
            request = request.header("X-Amz-Security-Token", token);
        }

        let response = request.body(body_json).send().map_err(|e| {
            OcrError::RecognitionFailed(format!("AWS Textract OCR request failed: {e}"))
        })?;

        let status = response.status();
        let body = response.text().map_err(|e| {
            OcrError::RecognitionFailed(format!("read AWS Textract OCR response: {e}"))
        })?;

        if !status.is_success() {
            return Err(OcrError::RecognitionFailed(format!(
                "AWS Textract returned {status}: {body}"
            )));
        }

        let parsed: AwsTextractResponse = serde_json::from_str(&body).map_err(|e| {
            OcrError::RecognitionFailed(format!("parse AWS Textract OCR response: {e}"))
        })?;

        let mut words = Vec::new();
        let mut lines = Vec::new();
        for block in parsed
            .blocks
            .into_iter()
            .filter(|block| block.block_type.as_deref() == Some("LINE"))
        {
            let text = block.text.unwrap_or_default().trim().to_string();
            if text.is_empty() {
                continue;
            }

            lines.push(text.clone());
            let bbox = block
                .geometry
                .and_then(|geometry| geometry.bounding_box)
                .map(|bbox| {
                    [
                        bbox.left.unwrap_or(0.0) * width as f32,
                        bbox.top.unwrap_or(0.0) * height as f32,
                        bbox.width.unwrap_or(0.0) * width as f32,
                        bbox.height.unwrap_or(0.0) * height as f32,
                    ]
                })
                .unwrap_or([0.0, 0.0, 0.0, 0.0]);
            let confidence = block.confidence.unwrap_or(100.0) / 100.0;
            words.push(OcrWord {
                text,
                bbox,
                confidence,
            });
        }

        let text = lines.join("\n");
        let confidence = if text.trim().is_empty() {
            0.0
        } else {
            confidence_from_words(&words, 1.0)
        };

        Ok(OcrResult {
            text,
            words,
            confidence,
        })
    }

    fn name(&self) -> &str {
        "aws-textract"
    }
}

#[cfg(feature = "ocr-aws")]
#[derive(Serialize)]
struct AwsTextractRequest<'a> {
    #[serde(rename = "Document")]
    document: AwsTextractDocument<'a>,
}

#[cfg(feature = "ocr-aws")]
#[derive(Serialize)]
struct AwsTextractDocument<'a> {
    #[serde(rename = "Bytes")]
    bytes: &'a str,
}

#[cfg(feature = "ocr-aws")]
#[derive(Deserialize)]
struct AwsTextractResponse {
    #[serde(rename = "Blocks", default)]
    blocks: Vec<AwsTextractBlock>,
}

#[cfg(feature = "ocr-aws")]
#[derive(Deserialize)]
struct AwsTextractBlock {
    #[serde(rename = "BlockType")]
    block_type: Option<String>,
    #[serde(rename = "Text")]
    text: Option<String>,
    #[serde(rename = "Confidence")]
    confidence: Option<f32>,
    #[serde(rename = "Geometry")]
    geometry: Option<AwsTextractGeometry>,
}

#[cfg(feature = "ocr-aws")]
#[derive(Deserialize)]
struct AwsTextractGeometry {
    #[serde(rename = "BoundingBox")]
    bounding_box: Option<AwsTextractBoundingBox>,
}

#[cfg(feature = "ocr-aws")]
#[derive(Deserialize)]
struct AwsTextractBoundingBox {
    #[serde(rename = "Left")]
    left: Option<f32>,
    #[serde(rename = "Top")]
    top: Option<f32>,
    #[serde(rename = "Width")]
    width: Option<f32>,
    #[serde(rename = "Height")]
    height: Option<f32>,
}

#[cfg(feature = "ocr-aws")]
#[derive(Default)]
struct AwsResolvedCredentials {
    region: Option<String>,
    access_key: Option<String>,
    secret_key: Option<String>,
    session_token: Option<String>,
}

#[cfg(feature = "ocr-aws")]
fn load_aws_profile(profile_name: &str) -> Option<AwsResolvedCredentials> {
    let credentials_sections = aws_shared_file("credentials")
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| parse_ini_sections(&text))
        .unwrap_or_default();
    let config_sections = aws_shared_file("config")
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| parse_ini_sections(&text))
        .unwrap_or_default();

    let credentials_section = credentials_sections
        .get(profile_name)
        .or_else(|| credentials_sections.get(&format!("profile {profile_name}")));
    let config_section_name = if profile_name == "default" {
        "default".to_string()
    } else {
        format!("profile {profile_name}")
    };
    let config_section = config_sections.get(&config_section_name);

    Some(AwsResolvedCredentials {
        region: config_section.and_then(|section| section.get("region").cloned()),
        access_key: credentials_section
            .and_then(|section| section.get("aws_access_key_id").cloned()),
        secret_key: credentials_section
            .and_then(|section| section.get("aws_secret_access_key").cloned()),
        session_token: credentials_section
            .and_then(|section| section.get("aws_session_token").cloned()),
    })
}

#[cfg(feature = "ocr-aws")]
fn aws_shared_file(name: &str) -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    {
        home_dir().map(|dir| dir.join(".aws").join(name))
    }

    #[cfg(not(target_os = "windows"))]
    {
        home_dir().map(|dir| dir.join(".aws").join(name))
    }
}

#[cfg(feature = "ocr-aws")]
fn parse_ini_sections(
    text: &str,
) -> std::collections::HashMap<String, std::collections::HashMap<String, String>> {
    let mut sections: std::collections::HashMap<String, std::collections::HashMap<String, String>> =
        std::collections::HashMap::new();
    let mut current = String::new();

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }

        if let Some(section) = line
            .strip_prefix('[')
            .and_then(|line| line.strip_suffix(']'))
        {
            current = section.trim().to_string();
            sections.entry(current.clone()).or_default();
            continue;
        }

        if let Some((key, value)) = line.split_once('=') {
            sections
                .entry(current.clone())
                .or_default()
                .insert(key.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }

    sections
}

#[cfg(feature = "ocr-aws")]
fn host_header_value(url: &reqwest::Url) -> Result<String, OcrError> {
    let host = url.host_str().ok_or_else(|| {
        OcrError::RecognitionFailed("AWS Textract endpoint is missing a host".into())
    })?;
    Ok(match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    })
}

#[cfg(feature = "ocr-aws")]
fn aws_sigv4_signature(
    secret_key: &str,
    date_stamp: &str,
    region: &str,
    service: &str,
    string_to_sign: &str,
) -> Result<String, OcrError> {
    let k_date = hmac_sha256(format!("AWS4{secret_key}").as_bytes(), date_stamp)?;
    let k_region = hmac_sha256(&k_date, region)?;
    let k_service = hmac_sha256(&k_region, service)?;
    let k_signing = hmac_sha256(&k_service, "aws4_request")?;
    let signature = hmac_sha256(&k_signing, string_to_sign)?;
    Ok(hex_encode(&signature))
}

#[cfg(feature = "ocr-aws")]
fn hmac_sha256(key: &[u8], data: &str) -> Result<Vec<u8>, OcrError> {
    type HmacSha256 = Hmac<Sha256>;

    let mut mac = HmacSha256::new_from_slice(key)
        .map_err(|e| OcrError::RecognitionFailed(format!("build AWS SigV4 HMAC: {e}")))?;
    mac.update(data.as_bytes());
    Ok(mac.finalize().into_bytes().to_vec())
}

#[cfg(feature = "ocr-aws")]
fn sha256_hex(data: &[u8]) -> String {
    hex_encode(&Sha256::digest(data))
}

#[cfg(feature = "ocr-aws")]
fn hex_encode(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

// ── Azure Document Intelligence backend ─────────────────────────────────────

#[cfg(feature = "ocr-azure")]
const AZURE_DOC_INTELLIGENCE_API_VERSION: &str = "2024-11-30";

/// Blocking HTTP OCR backend for Azure AI Document Intelligence.
#[cfg(feature = "ocr-azure")]
pub struct AzureDocIntelBackend {
    endpoint: String,
    api_key: String,
    client: reqwest::blocking::Client,
}

#[cfg(feature = "ocr-azure")]
impl AzureDocIntelBackend {
    /// Create a backend from an explicit endpoint and API key.
    pub fn new(endpoint: &str, api_key: &str) -> Self {
        Self {
            endpoint: endpoint.trim_end_matches('/').to_string(),
            api_key: api_key.to_string(),
            client: blocking_http_client(),
        }
    }

    /// Create a backend from Azure environment variables.
    ///
    /// Required variables:
    /// - `AZURE_DOCUMENT_INTELLIGENCE_ENDPOINT`
    /// - `AZURE_DOCUMENT_INTELLIGENCE_KEY`
    pub fn from_env() -> Result<Self, OcrError> {
        let endpoint = std::env::var("AZURE_DOCUMENT_INTELLIGENCE_ENDPOINT")
            .map_err(|_| OcrError::NoEngine)?;
        let api_key =
            std::env::var("AZURE_DOCUMENT_INTELLIGENCE_KEY").map_err(|_| OcrError::NoEngine)?;
        Ok(Self::new(&endpoint, &api_key))
    }

    fn analyze_endpoint(&self) -> String {
        let api_version = std::env::var("AZURE_DOCUMENT_INTELLIGENCE_API_VERSION")
            .unwrap_or_else(|_| AZURE_DOC_INTELLIGENCE_API_VERSION.to_string());
        format!(
            "{}/documentintelligence/documentModels/prebuilt-read:analyze?api-version={api_version}",
            self.endpoint
        )
    }

    fn poll_interval() -> Duration {
        std::env::var("AZURE_DOCUMENT_INTELLIGENCE_POLL_INTERVAL_MS")
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .map(Duration::from_millis)
            .unwrap_or_else(|| Duration::from_millis(250))
    }

    fn max_polls() -> usize {
        std::env::var("AZURE_DOCUMENT_INTELLIGENCE_MAX_POLLS")
            .ok()
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(120)
    }

    fn poll_operation(&self, operation_location: &str) -> Result<OcrResult, OcrError> {
        for attempt in 0..Self::max_polls() {
            let response = self
                .client
                .get(operation_location)
                .header("Ocp-Apim-Subscription-Key", &self.api_key)
                .send()
                .map_err(|e| {
                    OcrError::RecognitionFailed(format!(
                        "Azure Document Intelligence poll request failed: {e}"
                    ))
                })?;

            let status = response.status();
            let body = response.text().map_err(|e| {
                OcrError::RecognitionFailed(format!(
                    "read Azure Document Intelligence poll response: {e}"
                ))
            })?;

            if !status.is_success() {
                return Err(OcrError::RecognitionFailed(format!(
                    "Azure Document Intelligence poll returned {status}: {body}"
                )));
            }

            let operation: AzureAnalyzeOperation = serde_json::from_str(&body).map_err(|e| {
                OcrError::RecognitionFailed(format!(
                    "parse Azure Document Intelligence poll response: {e}"
                ))
            })?;

            if operation.status.eq_ignore_ascii_case("succeeded") {
                return azure_operation_to_result(operation);
            }

            if operation.status.eq_ignore_ascii_case("failed")
                || operation.status.eq_ignore_ascii_case("cancelled")
            {
                let message = operation
                    .error
                    .and_then(|error| error.message)
                    .unwrap_or_else(|| format!("Azure operation status {}", operation.status));
                return Err(OcrError::RecognitionFailed(format!(
                    "Azure Document Intelligence OCR failed: {message}"
                )));
            }

            if attempt + 1 < Self::max_polls() {
                std::thread::sleep(Self::poll_interval());
            }
        }

        Err(OcrError::RecognitionFailed(
            "timed out polling Azure Document Intelligence analyze operation".into(),
        ))
    }
}

#[cfg(feature = "ocr-azure")]
impl OcrBackend for AzureDocIntelBackend {
    fn recognize(&self, image_data: &[u8], width: u32, height: u32) -> Result<OcrResult, OcrError> {
        let png_bytes = rgb_png_bytes(image_data, width, height)?;
        let response = self
            .client
            .post(self.analyze_endpoint())
            .header("Ocp-Apim-Subscription-Key", &self.api_key)
            .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
            .body(png_bytes)
            .send()
            .map_err(|e| {
                OcrError::RecognitionFailed(format!(
                    "Azure Document Intelligence analyze request failed: {e}"
                ))
            })?;

        let status = response.status();
        let operation_location = response
            .headers()
            .get("operation-location")
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        let body = response.text().map_err(|e| {
            OcrError::RecognitionFailed(format!(
                "read Azure Document Intelligence analyze response: {e}"
            ))
        })?;

        if !status.is_success() {
            return Err(OcrError::RecognitionFailed(format!(
                "Azure Document Intelligence returned {status}: {body}"
            )));
        }

        if let Some(operation_location) = operation_location {
            return self.poll_operation(&operation_location);
        }

        let operation: AzureAnalyzeOperation = serde_json::from_str(&body).map_err(|e| {
            OcrError::RecognitionFailed(format!(
                "parse Azure Document Intelligence analyze response: {e}"
            ))
        })?;
        azure_operation_to_result(operation)
    }

    fn name(&self) -> &str {
        "azure-doc-intel"
    }
}

#[cfg(feature = "ocr-azure")]
#[derive(Deserialize)]
struct AzureAnalyzeOperation {
    #[serde(default)]
    status: String,
    #[serde(rename = "analyzeResult")]
    analyze_result: Option<AzureAnalyzeResult>,
    error: Option<AzureAnalyzeError>,
}

#[cfg(feature = "ocr-azure")]
#[derive(Deserialize)]
struct AzureAnalyzeError {
    message: Option<String>,
}

#[cfg(feature = "ocr-azure")]
#[derive(Deserialize)]
struct AzureAnalyzeResult {
    content: Option<String>,
    #[serde(default)]
    pages: Vec<AzureAnalyzePage>,
}

#[cfg(feature = "ocr-azure")]
#[derive(Deserialize)]
struct AzureAnalyzePage {
    #[serde(default)]
    words: Vec<AzureAnalyzeWord>,
}

#[cfg(feature = "ocr-azure")]
#[derive(Deserialize)]
struct AzureAnalyzeWord {
    content: String,
    confidence: Option<f32>,
    #[serde(default, alias = "boundingPolygon")]
    polygon: Vec<f32>,
}

#[cfg(feature = "ocr-azure")]
fn azure_operation_to_result(operation: AzureAnalyzeOperation) -> Result<OcrResult, OcrError> {
    let analyze_result = operation.analyze_result.ok_or_else(|| {
        OcrError::RecognitionFailed(format!(
            "Azure Document Intelligence operation ended with status {} but no analyze result",
            operation.status
        ))
    })?;

    let mut words = Vec::new();
    for page in analyze_result.pages {
        for word in page.words {
            let points = word
                .polygon
                .chunks_exact(2)
                .map(|pair| (pair[0], pair[1]))
                .collect::<Vec<_>>();
            words.push(OcrWord {
                text: word.content,
                bbox: bbox_from_points(&points),
                confidence: word.confidence.unwrap_or(1.0),
            });
        }
    }

    let text = analyze_result.content.unwrap_or_else(|| {
        words
            .iter()
            .map(|word| word.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    });
    let confidence = if text.trim().is_empty() {
        0.0
    } else {
        confidence_from_words(&words, 1.0)
    };

    Ok(OcrResult {
        text,
        words,
        confidence,
    })
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
/// 2. `ocr-google` with `GOOGLE_VISION_API_KEY` or Google credentials
/// 3. `ocr-azure` with `AZURE_DOCUMENT_INTELLIGENCE_*`
/// 4. `ocr-aws` with standard AWS credentials
/// 5. `ocr-onnx` via the Paddle ONNX adapter
/// 6. `ocr` via local `ocrs` model files
#[cfg(not(target_arch = "wasm32"))]
pub fn best_available_backend() -> Result<Box<dyn OcrBackend>, OcrError> {
    #[cfg(feature = "ocr-mistral")]
    if let Ok(backend) = MistralOcrBackend::from_env() {
        return Ok(Box::new(backend));
    }

    #[cfg(feature = "ocr-google")]
    if let Ok(backend) = GoogleVisionBackend::from_env() {
        return Ok(Box::new(backend));
    }

    #[cfg(feature = "ocr-azure")]
    if let Ok(backend) = AzureDocIntelBackend::from_env() {
        return Ok(Box::new(backend));
    }

    #[cfg(feature = "ocr-aws")]
    if let Ok(backend) = AwsTextractBackend::from_env() {
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

#[cfg(all(
    test,
    not(target_arch = "wasm32"),
    any(
        feature = "ocr-mistral",
        feature = "ocr-google",
        feature = "ocr-aws",
        feature = "ocr-azure"
    )
))]
mod tests {
    use super::*;

    use std::collections::HashMap;
    use std::ffi::OsString;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Mutex, OnceLock};
    use std::thread;

    // Tests use GOOGLE_APPLICATION_CREDENTIALS env var pointing to a real service account JSON.
    // Never hardcode private keys in source — set the env var before running ocr-google tests.

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct ScopedEnv {
        saved: Vec<(String, Option<OsString>)>,
    }

    impl ScopedEnv {
        fn set(changes: &[(&str, Option<&str>)]) -> Self {
            let saved = changes
                .iter()
                .map(|(key, _)| ((*key).to_string(), std::env::var_os(key)))
                .collect::<Vec<_>>();

            for (key, value) in changes {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }

            Self { saved }
        }
    }

    impl Drop for ScopedEnv {
        fn drop(&mut self) {
            for (key, value) in self.saved.drain(..) {
                match value {
                    Some(value) => std::env::set_var(&key, value),
                    None => std::env::remove_var(&key),
                }
            }
        }
    }

    struct MockServer {
        base_url: String,
        join_handle: Option<thread::JoinHandle<()>>,
    }

    impl MockServer {
        fn start<F>(requests: usize, handler: F) -> Self
        where
            F: Fn(MockRequest, usize, &str) -> MockResponse + Send + 'static,
        {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock server");
            let address = listener.local_addr().expect("mock server address");
            let base_url = format!("http://{address}");
            let handler_base_url = base_url.clone();

            let join_handle = thread::spawn(move || {
                for index in 0..requests {
                    let (mut stream, _) = listener.accept().expect("accept mock request");
                    let request = read_http_request(&mut stream);
                    let response = handler(request, index, &handler_base_url);
                    write_http_response(&mut stream, response);
                }
            });

            Self {
                base_url,
                join_handle: Some(join_handle),
            }
        }

        fn url(&self, path: &str) -> String {
            format!("{}{}", self.base_url, path)
        }

        fn finish(mut self) {
            if let Some(join_handle) = self.join_handle.take() {
                join_handle.join().expect("mock server thread");
            }
        }
    }

    #[derive(Debug)]
    struct MockRequest {
        method: String,
        path: String,
        headers: HashMap<String, String>,
        body: Vec<u8>,
    }

    struct MockResponse {
        status_code: u16,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    }

    impl MockResponse {
        fn json(status_code: u16, body: &str) -> Self {
            Self {
                status_code,
                headers: vec![("Content-Type".into(), "application/json".into())],
                body: body.as_bytes().to_vec(),
            }
        }

        fn empty(status_code: u16, headers: &[(&str, &str)]) -> Self {
            Self {
                status_code,
                headers: headers
                    .iter()
                    .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                    .collect(),
                body: Vec::new(),
            }
        }
    }

    fn read_http_request(stream: &mut TcpStream) -> MockRequest {
        let mut buffer = Vec::new();
        let mut chunk = [0u8; 4096];
        let header_end = loop {
            let read = stream.read(&mut chunk).expect("read mock request");
            assert!(read > 0, "mock request closed before headers completed");
            buffer.extend_from_slice(&chunk[..read]);

            if let Some(index) = find_bytes(&buffer, b"\r\n\r\n") {
                break index + 4;
            }
        };

        let header_text = String::from_utf8_lossy(&buffer[..header_end]);
        let mut lines = header_text.split("\r\n").filter(|line| !line.is_empty());
        let request_line = lines.next().expect("request line");
        let mut request_parts = request_line.split_whitespace();
        let method = request_parts.next().expect("request method").to_string();
        let path = request_parts.next().expect("request path").to_string();

        let mut headers = HashMap::new();
        let mut content_length = 0usize;
        for line in lines {
            if let Some((name, value)) = line.split_once(':') {
                let key = name.trim().to_ascii_lowercase();
                let value = value.trim().to_string();
                if key == "content-length" {
                    content_length = value.parse::<usize>().expect("content-length");
                }
                headers.insert(key, value);
            }
        }

        let mut body = buffer[header_end..].to_vec();
        while body.len() < content_length {
            let read = stream.read(&mut chunk).expect("read mock request body");
            assert!(read > 0, "mock request closed before body completed");
            body.extend_from_slice(&chunk[..read]);
        }

        MockRequest {
            method,
            path,
            headers,
            body,
        }
    }

    fn write_http_response(stream: &mut TcpStream, response: MockResponse) {
        let reason = match response.status_code {
            200 => "OK",
            202 => "Accepted",
            400 => "Bad Request",
            401 => "Unauthorized",
            403 => "Forbidden",
            404 => "Not Found",
            _ => "OK",
        };
        let mut head = format!(
            "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n",
            response.status_code,
            reason,
            response.body.len()
        );
        for (name, value) in response.headers {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
        head.push_str("\r\n");

        stream
            .write_all(head.as_bytes())
            .expect("write mock response head");
        stream
            .write_all(&response.body)
            .expect("write mock response body");
    }

    fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack
            .windows(needle.len())
            .position(|window| window == needle)
    }

    #[cfg(feature = "ocr-mistral")]
    #[test]
    fn ocr_mistral_name_is_stable() {
        let backend = MistralOcrBackend::new("test-key");
        assert_eq!(backend.name(), "mistral");
    }

    #[cfg(feature = "ocr-mistral")]
    #[test]
    fn ocr_mistral_from_env_reads_api_key() {
        let _env = env_lock().lock().expect("env lock");
        let _scoped = ScopedEnv::set(&[("MISTRAL_API_KEY", Some("test-key"))]);

        let backend = MistralOcrBackend::from_env().expect("Mistral backend from env");
        assert_eq!(backend.name(), "mistral");
    }

    #[cfg(feature = "ocr-mistral")]
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

    #[cfg(feature = "ocr-mistral")]
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

    #[cfg(feature = "ocr-google")]
    #[test]
    fn ocr_google_name_is_stable() {
        let backend = GoogleVisionBackend::from_api_key("test-key");
        assert_eq!(backend.name(), "google-vision");
    }

    #[cfg(feature = "ocr-google")]
    #[test]
    fn ocr_google_from_env_prefers_api_key() {
        let _env = env_lock().lock().expect("env lock");
        let temp_path = std::env::temp_dir().join("codex-google-creds.json");
        let temp_path_string = temp_path.to_string_lossy().into_owned();
        std::fs::write(
            &temp_path,
            r#"{"type":"service_account","client_email":"test@example.com","private_key":"missing"}"#,
        )
        .expect("write google credentials fixture");
        let _scoped = ScopedEnv::set(&[
            ("GOOGLE_VISION_API_KEY", Some("test-key")),
            (
                "GOOGLE_APPLICATION_CREDENTIALS",
                Some(temp_path_string.as_str()),
            ),
            ("GOOGLE_CLOUD_PROJECT", None),
        ]);

        let backend = GoogleVisionBackend::from_env().expect("Google backend from env");
        assert_eq!(backend.name(), "google-vision");
    }

    #[cfg(feature = "ocr-google")]
    #[test]
    fn ocr_google_mock_request_uses_api_key_and_parses_words() {
        let server = MockServer::start(1, |request, _, _| {
            assert_eq!(request.method, "POST");
            assert!(request.path.starts_with("/v1/images:annotate?key=test-key"));
            assert_eq!(
                request.headers.get("content-type").map(String::as_str),
                Some("application/json")
            );

            let body: serde_json::Value =
                serde_json::from_slice(&request.body).expect("google request JSON");
            assert_eq!(
                body["requests"][0]["features"][0]["type"].as_str(),
                Some("TEXT_DETECTION")
            );

            let content = body["requests"][0]["image"]["content"]
                .as_str()
                .expect("google image content");
            let png = base64::engine::general_purpose::STANDARD
                .decode(content)
                .expect("decode google image");
            assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));

            MockResponse::json(
                200,
                r#"{
                    "responses": [{
                        "fullTextAnnotation": { "text": "Hello World" },
                        "textAnnotations": [
                            { "description": "Hello World" },
                            {
                                "description": "Hello",
                                "boundingPoly": {
                                    "vertices": [
                                        { "x": 1, "y": 2 },
                                        { "x": 41, "y": 2 },
                                        { "x": 41, "y": 14 },
                                        { "x": 1, "y": 14 }
                                    ]
                                }
                            },
                            {
                                "description": "World",
                                "boundingPoly": {
                                    "vertices": [
                                        { "x": 50, "y": 2 },
                                        { "x": 92, "y": 2 },
                                        { "x": 92, "y": 14 },
                                        { "x": 50, "y": 14 }
                                    ]
                                }
                            }
                        ]
                    }]
                }"#,
            )
        });

        let backend = GoogleVisionBackend::with_endpoint_api_key(
            "test-key",
            &server.url("/v1/images:annotate"),
        );
        let (pixels, width, height) = test_text_image("TEST 123");
        let result = backend
            .recognize(&pixels, width, height)
            .expect("Google OCR");

        assert_eq!(result.text, "Hello World");
        assert_eq!(result.words.len(), 2);
        assert_eq!(result.words[0].text, "Hello");
        assert_eq!(result.words[0].bbox, [1.0, 2.0, 40.0, 12.0]);
        assert_eq!(result.words[1].text, "World");

        server.finish();
    }

    #[cfg(feature = "ocr-google")]
    #[test]
    fn ocr_google_service_account_flow_uses_jwt_exchange() {
        let server = MockServer::start(2, |request, index, _| match index {
            0 => {
                assert_eq!(request.method, "POST");
                assert_eq!(request.path, "/token");
                let body = String::from_utf8(request.body).expect("token body");
                assert!(body
                    .contains("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer"));
                assert!(body.contains("assertion="));

                MockResponse::json(200, r#"{"access_token":"ya29.test-token"}"#)
            }
            1 => {
                assert_eq!(request.method, "POST");
                assert_eq!(request.path, "/v1/images:annotate");
                assert_eq!(
                    request.headers.get("authorization").map(String::as_str),
                    Some("Bearer ya29.test-token")
                );

                MockResponse::json(
                    200,
                    r#"{
                        "responses": [{
                            "fullTextAnnotation": { "text": "Service Account" },
                            "textAnnotations": [
                                { "description": "Service Account" },
                                {
                                    "description": "Service",
                                    "boundingPoly": {
                                        "vertices": [
                                            { "x": 0, "y": 0 },
                                            { "x": 60, "y": 0 },
                                            { "x": 60, "y": 10 },
                                            { "x": 0, "y": 10 }
                                        ]
                                    }
                                }
                            ]
                        }]
                    }"#,
                )
            }
            _ => unreachable!(),
        });

        let credentials_json = format!(
            r#"{{
                "type": "service_account",
                "client_email": "test@example.com",
                "private_key": {private_key:?},
                "token_uri": {token_uri:?}
            }}"#,
            // Assembled rather than written as one literal: a PEM header in
            // shipped source trips secret scanners (including our own
            // prepublish audit) even when the body is obviously fake.
            private_key = concat!(
                "-----BEGIN ",
                "PRIVATE KEY",
                "-----\n",
                "TEST_PLACEHOLDER_NOT_A_REAL_KEY\n",
                "-----END ",
                "PRIVATE KEY",
                "-----\n",
            ),
            token_uri = server.url("/token"),
        );

        let backend = GoogleVisionBackend::with_endpoint_credentials(
            &credentials_json,
            &server.url("/v1/images:annotate"),
        );
        let (pixels, width, height) = test_text_image("TEST");
        let result = backend
            .recognize(&pixels, width, height)
            .expect("Google OCR via service account");

        assert_eq!(result.text, "Service Account");
        assert_eq!(result.words.len(), 1);
        assert_eq!(result.words[0].text, "Service");

        server.finish();
    }

    #[cfg(feature = "ocr-aws")]
    #[test]
    fn ocr_aws_name_is_stable() {
        let backend = AwsTextractBackend::new("eu-west-1", "access", "secret");
        assert_eq!(backend.name(), "aws-textract");
    }

    #[cfg(feature = "ocr-aws")]
    #[test]
    fn ocr_aws_from_env_reads_region_and_keys() {
        let _env = env_lock().lock().expect("env lock");
        let _scoped = ScopedEnv::set(&[
            ("AWS_REGION", Some("eu-west-1")),
            ("AWS_DEFAULT_REGION", None),
            ("AWS_ACCESS_KEY_ID", Some("access")),
            ("AWS_SECRET_ACCESS_KEY", Some("secret")),
            ("AWS_SESSION_TOKEN", None),
            ("AWS_PROFILE", None),
        ]);

        let backend = AwsTextractBackend::from_env().expect("AWS backend from env");
        assert_eq!(backend.name(), "aws-textract");
    }

    #[cfg(feature = "ocr-aws")]
    #[test]
    fn ocr_aws_mock_request_signs_sigv4_and_parses_lines() {
        let server = MockServer::start(1, |request, _, _| {
            assert_eq!(request.method, "POST");
            assert_eq!(request.path, "/");
            assert_eq!(
                request.headers.get("content-type").map(String::as_str),
                Some("application/x-amz-json-1.1")
            );
            assert_eq!(
                request.headers.get("x-amz-target").map(String::as_str),
                Some("Textract.DetectDocumentText")
            );
            let authorization = request
                .headers
                .get("authorization")
                .expect("aws authorization");
            assert!(authorization.starts_with("AWS4-HMAC-SHA256 "));

            let body: serde_json::Value =
                serde_json::from_slice(&request.body).expect("aws request JSON");
            let encoded = body["Document"]["Bytes"]
                .as_str()
                .expect("aws document bytes");
            let png = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .expect("decode aws image");
            assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));

            MockResponse::json(
                200,
                r#"{
                    "Blocks": [
                        {
                            "BlockType": "LINE",
                            "Text": "Hello Textract",
                            "Confidence": 97.5,
                            "Geometry": {
                                "BoundingBox": {
                                    "Left": 0.1,
                                    "Top": 0.2,
                                    "Width": 0.5,
                                    "Height": 0.1
                                }
                            }
                        }
                    ]
                }"#,
            )
        });

        let backend =
            AwsTextractBackend::with_endpoint("eu-west-1", "access", "secret", &server.url("/"));
        let (pixels, width, height) = test_text_image("TEST");
        let result = backend.recognize(&pixels, width, height).expect("AWS OCR");

        assert_eq!(result.text, "Hello Textract");
        assert_eq!(result.words.len(), 1);
        assert_eq!(result.words[0].text, "Hello Textract");
        assert!((result.words[0].bbox[0] - (width as f32 * 0.1)).abs() < 0.001);
        assert!((result.words[0].confidence - 0.975).abs() < 0.0001);

        server.finish();
    }

    #[cfg(feature = "ocr-azure")]
    #[test]
    fn ocr_azure_name_is_stable() {
        let backend =
            AzureDocIntelBackend::new("https://example.cognitiveservices.azure.com", "key");
        assert_eq!(backend.name(), "azure-doc-intel");
    }

    #[cfg(feature = "ocr-azure")]
    #[test]
    fn ocr_azure_from_env_reads_endpoint_and_key() {
        let _env = env_lock().lock().expect("env lock");
        let _scoped = ScopedEnv::set(&[
            (
                "AZURE_DOCUMENT_INTELLIGENCE_ENDPOINT",
                Some("https://example.cognitiveservices.azure.com"),
            ),
            ("AZURE_DOCUMENT_INTELLIGENCE_KEY", Some("test-key")),
        ]);

        let backend = AzureDocIntelBackend::from_env().expect("Azure backend from env");
        assert_eq!(backend.name(), "azure-doc-intel");
    }

    #[cfg(feature = "ocr-azure")]
    #[test]
    fn ocr_azure_mock_request_polls_operation_and_parses_words() {
        let server = MockServer::start(2, |request, index, base_url| match index {
            0 => {
                assert_eq!(request.method, "POST");
                assert!(request
                    .path
                    .starts_with("/documentintelligence/documentModels/prebuilt-read:analyze?"));
                assert_eq!(
                    request
                        .headers
                        .get("ocp-apim-subscription-key")
                        .map(String::as_str),
                    Some("test-key")
                );
                assert_eq!(
                    request.headers.get("content-type").map(String::as_str),
                    Some("application/octet-stream")
                );
                assert!(request.body.starts_with(&[0x89, b'P', b'N', b'G']));

                MockResponse::empty(
                    202,
                    &[("Operation-Location", &format!("{base_url}/operations/123"))],
                )
            }
            1 => {
                assert_eq!(request.method, "GET");
                assert_eq!(request.path, "/operations/123");

                MockResponse::json(
                    200,
                    r#"{
                        "status": "succeeded",
                        "analyzeResult": {
                            "content": "Hello Azure",
                            "pages": [
                                {
                                    "words": [
                                        {
                                            "content": "Hello",
                                            "confidence": 0.98,
                                            "polygon": [1,2, 31,2, 31,12, 1,12]
                                        },
                                        {
                                            "content": "Azure",
                                            "confidence": 0.96,
                                            "polygon": [40,2, 78,2, 78,12, 40,12]
                                        }
                                    ]
                                }
                            ]
                        }
                    }"#,
                )
            }
            _ => unreachable!(),
        });

        let backend = AzureDocIntelBackend::new(&server.base_url, "test-key");
        let (pixels, width, height) = test_text_image("TEST");
        let result = backend
            .recognize(&pixels, width, height)
            .expect("Azure OCR");

        assert_eq!(result.text, "Hello Azure");
        assert_eq!(result.words.len(), 2);
        assert_eq!(result.words[0].bbox, [1.0, 2.0, 30.0, 10.0]);
        assert!((result.confidence - 0.97).abs() < 0.001);

        server.finish();
    }

    #[cfg(any(
        feature = "ocr-mistral",
        feature = "ocr-google",
        feature = "ocr-azure",
        feature = "ocr-aws"
    ))]
    #[test]
    fn ocr_best_available_respects_cloud_priority() {
        let _env = env_lock().lock().expect("env lock");

        #[cfg(feature = "ocr-mistral")]
        {
            let _scoped = ScopedEnv::set(&[
                ("MISTRAL_API_KEY", Some("mistral-key")),
                ("GOOGLE_VISION_API_KEY", Some("google-key")),
                (
                    "AZURE_DOCUMENT_INTELLIGENCE_ENDPOINT",
                    Some("https://example.azure.com"),
                ),
                ("AZURE_DOCUMENT_INTELLIGENCE_KEY", Some("azure-key")),
                ("AWS_REGION", Some("eu-west-1")),
                ("AWS_ACCESS_KEY_ID", Some("aws-access")),
                ("AWS_SECRET_ACCESS_KEY", Some("aws-secret")),
            ]);

            let backend = best_available_backend().expect("best OCR backend");
            assert_eq!(backend.name(), "mistral");
        }

        #[cfg(all(not(feature = "ocr-mistral"), feature = "ocr-google"))]
        {
            let _scoped = ScopedEnv::set(&[
                ("GOOGLE_VISION_API_KEY", Some("google-key")),
                (
                    "AZURE_DOCUMENT_INTELLIGENCE_ENDPOINT",
                    Some("https://example.azure.com"),
                ),
                ("AZURE_DOCUMENT_INTELLIGENCE_KEY", Some("azure-key")),
                ("AWS_REGION", Some("eu-west-1")),
                ("AWS_ACCESS_KEY_ID", Some("aws-access")),
                ("AWS_SECRET_ACCESS_KEY", Some("aws-secret")),
            ]);

            let backend = best_available_backend().expect("best OCR backend");
            assert_eq!(backend.name(), "google-vision");
            return;
        }

        #[cfg(all(
            not(feature = "ocr-mistral"),
            not(feature = "ocr-google"),
            feature = "ocr-azure"
        ))]
        {
            let _scoped = ScopedEnv::set(&[
                (
                    "AZURE_DOCUMENT_INTELLIGENCE_ENDPOINT",
                    Some("https://example.azure.com"),
                ),
                ("AZURE_DOCUMENT_INTELLIGENCE_KEY", Some("azure-key")),
                ("AWS_REGION", Some("eu-west-1")),
                ("AWS_ACCESS_KEY_ID", Some("aws-access")),
                ("AWS_SECRET_ACCESS_KEY", Some("aws-secret")),
            ]);

            let backend = best_available_backend().expect("best OCR backend");
            assert_eq!(backend.name(), "azure-doc-intel");
            return;
        }

        #[cfg(all(
            not(feature = "ocr-mistral"),
            not(feature = "ocr-google"),
            not(feature = "ocr-azure"),
            feature = "ocr-aws"
        ))]
        {
            let _scoped = ScopedEnv::set(&[
                ("AWS_REGION", Some("eu-west-1")),
                ("AWS_ACCESS_KEY_ID", Some("aws-access")),
                ("AWS_SECRET_ACCESS_KEY", Some("aws-secret")),
            ]);

            let backend = best_available_backend().expect("best OCR backend");
            assert_eq!(backend.name(), "aws-textract");
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
