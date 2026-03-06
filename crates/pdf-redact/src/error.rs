//! Error types for pdf-redact operations.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum RedactError {
    #[error("PDF error: {0}")]
    Pdf(#[from] lopdf::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("page {0} out of range (document has {1} pages)")]
    PageOutOfRange(u32, u32),

    #[error("no redaction areas specified")]
    NoAreas,

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, RedactError>;
