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

    #[error("unsupported image filter: {0}")]
    UnsupportedImageFilter(String),

    #[error(
        "unsupported /ToUnicode CMap on redacted page (font {font_resource_name}): \
         redaction strip refuses to silently drop /ToUnicode; CMap shape not understood by the \
         conservative pdf-redact parser ({reason})"
    )]
    UnsupportedToUnicodeCMap {
        font_resource_name: String,
        reason: String,
    },

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, RedactError>;
