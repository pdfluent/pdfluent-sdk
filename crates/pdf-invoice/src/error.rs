//! Error types for the pdf-invoice crate.

/// Errors that can occur during form data exchange or invoice operations.
#[derive(Debug, thiserror::Error)]
pub enum InvoiceError {
    /// PDF object model error.
    #[error("PDF error: {0}")]
    Pdf(#[from] lopdf::Error),
    /// I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// XML parsing or generation error.
    #[error("XML error: {0}")]
    Xml(String),
    /// A required field is missing from the input.
    #[error("missing required field: {0}")]
    MissingRequired(String),
    /// Generic parse error for FDF/XFDF/CII data.
    #[error("parse error: {0}")]
    Parse(String),
    /// ZUGFeRD profile validation failure.
    #[error("profile validation: {0}")]
    ProfileValidation(String),
}

/// Result type alias for this crate.
pub type Result<T> = std::result::Result<T, InvoiceError>;
