//! Error types for the rendering engine.

use crate::api_error::PdfError;

/// Errors that can occur in the rendering engine.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    /// The PDF data is invalid or could not be parsed.
    #[error("invalid PDF: {0}")]
    InvalidPdf(String),

    /// A page index is out of range.
    #[error("page {index} out of range (document has {count} pages)")]
    PageOutOfRange {
        /// The requested page index.
        index: usize,
        /// Total number of pages.
        count: usize,
    },

    /// A rendering error occurred.
    #[error("render error: {0}")]
    RenderError(String),

    /// An I/O error occurred.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl PdfError for EngineError {
    fn code(&self) -> &str {
        match self {
            EngineError::InvalidPdf(_) => "INVALID_PDF",
            EngineError::PageOutOfRange { .. } => "PAGE_OUT_OF_RANGE",
            EngineError::RenderError(_) => "RENDER_ERROR",
            EngineError::Io(_) => "IO_ERROR",
        }
    }

    fn help(&self) -> Option<String> {
        match self {
            EngineError::InvalidPdf(_) => {
                Some("The PDF file is corrupt or invalid. Try using a PDF repair tool or re-save the document.".to_string())
            }
            EngineError::PageOutOfRange { index, count } => {
                Some(format!("Page {} does not exist. This document has {} pages (0-{}).", index, count, count - 1))
            }
            EngineError::RenderError(_) => {
                Some("Rendering failed. Check that the page exists and the document is not corrupted.".to_string())
            }
            EngineError::Io(_) => {
                Some("An I/O error occurred. Check file permissions and disk space.".to_string())
            }
        }
    }
}

/// Convenience type alias.
pub type Result<T> = std::result::Result<T, EngineError>;
