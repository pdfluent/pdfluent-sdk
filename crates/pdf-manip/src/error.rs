//! Error types for pdf-manip operations.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ManipError {
    #[error("PDF error: {0}")]
    Pdf(#[from] lopdf::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("page index {0} out of range (document has {1} pages)")]
    PageOutOfRange(usize, usize),

    #[error("empty page range")]
    EmptyPageRange,

    #[error("encryption error: {0}")]
    Encryption(String),

    #[error("decryption failed: wrong password or unsupported algorithm")]
    DecryptionFailed,

    #[error("unsupported encryption: {0}")]
    UnsupportedEncryption(String),

    #[error("invalid bookmark: {0}")]
    InvalidBookmark(String),

    #[error("watermark error: {0}")]
    Watermark(String),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, ManipError>;
