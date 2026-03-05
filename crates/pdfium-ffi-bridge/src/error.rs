//! PDFium bridge error types.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PdfError {
    #[error("PDFium library not found: {0}")]
    LibraryNotFound(String),

    #[error("Failed to load PDF: {0}")]
    LoadFailed(String),

    #[error("XFA packet not found: {0}")]
    XfaPacketNotFound(String),

    #[error("Render error: {0}")]
    RenderError(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("XML parse error: {0}")]
    XmlParse(String),

    #[error("Font error: {0}")]
    FontError(String),
}

pub type Result<T> = std::result::Result<T, PdfError>;
