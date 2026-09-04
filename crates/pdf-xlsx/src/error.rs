//! Error types for PDF to XLSX conversion.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

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
