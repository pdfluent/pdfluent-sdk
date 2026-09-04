//! Error types for PDF → DOCX conversion.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use thiserror::Error;

/// Errors returned by `pdf-docx` while converting a PDF to a DOCX
/// (Office Open XML WordprocessingML) document.
///
/// All variants except [`Other`](Self::Other) wrap an underlying error
/// from a more specific layer (PDF parsing, text extraction, XML, ZIP, I/O).
/// Surface the inner cause when reporting the failure.
#[derive(Debug, Error)]
pub enum DocxError {
    /// The source PDF could not be parsed before conversion could begin.
    #[error("PDF error: {0}")]
    Pdf(#[from] lopdf::Error),

    /// Text or layout extraction from the source PDF failed.
    #[error("extraction error: {0}")]
    Extract(#[from] pdf_extract::ExtractError),

    /// Building the DOCX `document.xml` body failed (malformed text run,
    /// invalid attribute, encoding error in the XML serializer).
    #[error("XML error: {0}")]
    Xml(#[from] quick_xml::Error),

    /// Packaging the DOCX archive failed (ZIP central directory error,
    /// invalid entry name, compression failure).
    #[error("ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),

    /// An I/O error while reading the source PDF or writing the DOCX
    /// archive.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// A non-categorised conversion failure. Reserved for cases the more
    /// specific variants do not cover; the message describes the situation.
    #[error("{0}")]
    Other(String),
}

/// Convenience `Result` alias for fallible `pdf-docx` operations.
pub type Result<T> = std::result::Result<T, DocxError>;
