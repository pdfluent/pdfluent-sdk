//! Error types for XLSX workbook extraction.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use thiserror::Error;

/// Errors from reading an XLSX workbook.
#[derive(Debug, Error)]
pub enum XlsxExtractError {
    /// The input is not a readable zip archive (every XLSX file is one).
    #[error("invalid zip archive: {0}")]
    Zip(#[from] zip::result::ZipError),

    /// An XML part of the workbook failed to parse.
    #[error("invalid workbook XML: {0}")]
    Xml(#[from] quick_xml::Error),

    /// I/O while reading the archive.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The archive is valid zip but not a workbook we understand
    /// (missing workbook.xml, no sheets, corrupt cell references).
    #[error("not an XLSX workbook: {0}")]
    Invalid(String),
}

/// Shorthand for extraction results.
pub type Result<T> = std::result::Result<T, XlsxExtractError>;
