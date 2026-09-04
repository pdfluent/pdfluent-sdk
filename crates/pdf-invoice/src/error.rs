//! Error types for the pdf-invoice crate.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

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
