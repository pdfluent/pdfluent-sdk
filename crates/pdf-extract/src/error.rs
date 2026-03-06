//! Error types for pdf-extract operations.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExtractError {
    #[error("PDF error: {0}")]
    Pdf(#[from] lopdf::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("page {0} out of range (document has {1} pages)")]
    PageOutOfRange(u32, u32),

    #[error("unsupported image filter: {0}")]
    UnsupportedFilter(String),

    #[error("image decode error: {0}")]
    ImageDecode(String),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, ExtractError>;
