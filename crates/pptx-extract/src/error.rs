//! Error types for PPTX to block extraction.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use thiserror::Error;

/// Errors returned while reading a PPTX package into [`pdf_engine::Block`]s.
#[derive(Debug, Error)]
pub enum PptxExtractError {
    /// The file is not a readable ZIP archive.
    #[error("ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),

    /// A package part is not well-formed XML.
    #[error("XML error: {0}")]
    Xml(#[from] quick_xml::Error),

    /// The archive could not be read.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The archive is a valid ZIP but not a usable presentation
    /// (no slides, unresolvable slide references, malformed values).
    #[error("invalid PPTX: {0}")]
    Invalid(String),
}

/// The result of a PPTX extraction step.
pub type Result<T> = std::result::Result<T, PptxExtractError>;
