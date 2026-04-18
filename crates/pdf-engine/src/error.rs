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

    /// The PDF is encrypted and requires a password to open.
    #[error("PDF is encrypted: {0}")]
    Encrypted(String),

    /// The page has invalid or degenerate dimensions.
    #[error("invalid page geometry (w={width:.2}pt, h={height:.2}pt): {reason}")]
    InvalidPageGeometry {
        /// Page width in points.
        width: f32,
        /// Page height in points.
        height: f32,
        /// Human-readable reason for the failure.
        reason: String,
    },

    /// XFA flattening failed.
    #[error("XFA flatten failed: {0}")]
    XfaFlattenFailed(String),
}

impl PdfError for EngineError {
    fn code(&self) -> &str {
        match self {
            EngineError::InvalidPdf(_) => "INVALID_PDF",
            EngineError::PageOutOfRange { .. } => "PAGE_OUT_OF_RANGE",
            EngineError::RenderError(_) => "RENDER_ERROR",
            EngineError::Io(_) => "IO_ERROR",
            EngineError::Encrypted(_) => "ENCRYPTED",
            EngineError::InvalidPageGeometry { .. } => "INVALID_PAGE_GEOMETRY",
            EngineError::XfaFlattenFailed(_) => "XFA_FLATTEN_FAILED",
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
            EngineError::Encrypted(_) => {
                Some("The PDF is encrypted. Provide the correct password to open it.".to_string())
            }
            EngineError::InvalidPageGeometry { .. } => {
                Some("The page has invalid or degenerate dimensions and cannot be rendered.".to_string())
            }
            EngineError::XfaFlattenFailed(_) => {
                Some("XFA flattening produced an invalid PDF. The document may use unsupported XFA features.".to_string())
            }
        }
    }
}

/// Convenience type alias.
pub type Result<T> = std::result::Result<T, EngineError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypted_error_code() {
        let e = EngineError::Encrypted("bad password".into());
        assert_eq!(e.code(), "ENCRYPTED");
    }

    #[test]
    fn invalid_page_geometry_error_code() {
        let e = EngineError::InvalidPageGeometry {
            width: 0.5,
            height: 0.5,
            reason: "test".into(),
        };
        assert_eq!(e.code(), "INVALID_PAGE_GEOMETRY");
    }

    #[test]
    fn xfa_flatten_failed_error_code() {
        let e = EngineError::XfaFlattenFailed("corrupt output".into());
        assert_eq!(e.code(), "XFA_FLATTEN_FAILED");
    }
}
