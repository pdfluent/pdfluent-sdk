//! Error types for PDF to PPTX conversion.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PptxError {
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

pub type Result<T> = std::result::Result<T, PptxError>;
