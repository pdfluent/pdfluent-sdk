//! Error types for `pdf-ocr` operations.

use thiserror::Error;

/// Errors returned by the `pdf-ocr` crate during optical character
/// recognition.
///
/// Wrapping engines (Tesseract, PaddleOCR) bubble their own errors up via
/// the [`OcrError::Engine`] variant — this crate does not classify
/// engine-specific failures.
#[derive(Debug, Error)]
pub enum OcrError {
    /// The underlying PDF byte stream could not be parsed before OCR could
    /// be attempted (malformed cross-reference table, truncated stream,
    /// encrypted document opened without a password, etc.).
    #[error("PDF error: {0}")]
    Pdf(#[from] lopdf::Error),

    /// An I/O error occurred while reading the source PDF or writing OCR
    /// output (e.g. searchable PDF or hOCR sidecar).
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// The caller asked to OCR a page index that does not exist in the
    /// document.
    ///
    /// Fields: `(requested_page, total_pages)`. Page numbers are 1-based.
    #[error("page {0} out of range (document has {1} pages)")]
    PageOutOfRange(u32, u32),

    /// The configured OCR engine returned a non-recoverable error
    /// (Tesseract initialisation failure, missing language data, model
    /// load failure, etc.). The wrapped string is the engine's verbatim
    /// message — surface it to the user.
    #[error("OCR engine error: {0}")]
    Engine(String),

    /// Rasterising the page for OCR failed before the engine could see
    /// any pixels (font resolution failure, unsupported colorspace,
    /// page geometry error, etc.).
    #[error("render error: {0}")]
    Render(String),

    /// A non-categorised OCR failure. Reserved for cases the more specific
    /// variants do not cover; the message describes the situation.
    #[error("{0}")]
    Other(String),
}

/// Convenience `Result` alias for fallible `pdf-ocr` operations.
pub type Result<T> = std::result::Result<T, OcrError>;
