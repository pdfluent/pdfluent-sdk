//! Error types for PDF to DOCX conversion.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum DocxError {
    #[error("PDF error: {0}")]
    Pdf(#[from] lopdf::Error),

    #[error("extraction error: {0}")]
    Extract(#[from] pdf_extract::ExtractError),

    #[error("XML error: {0}")]
    Xml(#[from] quick_xml::Error),

    #[error("ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, DocxError>;
