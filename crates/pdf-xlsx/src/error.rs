//! Error types for PDF to XLSX conversion.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum XlsxError {
    #[error("PDF error: {0}")]
    Pdf(#[from] lopdf::Error),

    #[error("extraction error: {0}")]
    Extract(#[from] pdf_extract::ExtractError),

    #[error("XLSX error: {0}")]
    Xlsx(#[from] rust_xlsxwriter::XlsxError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, XlsxError>;
