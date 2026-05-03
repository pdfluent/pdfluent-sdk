//! Error types for `pdf-extract` operations.

use thiserror::Error;

/// Errors returned by the `pdf-extract` crate while reading text or images
/// from a PDF.
///
/// Customers usually see these wrapped inside `pdfluent::Error` after they
/// bubble up through the facade. The inner variant tells the caller what
/// kind of input prevented extraction.
#[derive(Debug, Error)]
pub enum ExtractError {
    /// The underlying PDF byte stream could not be parsed.
    ///
    /// Typically a malformed cross-reference table, a truncated stream, or
    /// an unsupported PDF construct. The wrapped `lopdf::Error` has the
    /// detail; surface it to the user when reporting the problem.
    #[error("PDF error: {0}")]
    Pdf(#[from] lopdf::Error),

    /// An I/O error occurred while reading the document or writing extracted
    /// output (for image extraction targets).
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// The caller asked for a page index that does not exist in the
    /// document.
    ///
    /// Fields: `(requested_page, total_pages)`. Page numbers are 1-based at
    /// the public API; this variant carries the same convention.
    #[error("page {0} out of range (document has {1} pages)")]
    PageOutOfRange(u32, u32),

    /// The image embedded on the page uses a PDF stream filter that this
    /// crate cannot decode (for example uncommon JPX profiles or unknown
    /// custom filters). The wrapped string identifies the filter by its
    /// PDF name.
    #[error("unsupported image filter: {0}")]
    UnsupportedFilter(String),

    /// Image bytes were extracted but could not be decoded by the image
    /// backend (corrupt JPEG/PNG/JPX payload, invalid colorspace, etc.).
    #[error("image decode error: {0}")]
    ImageDecode(String),

    /// A non-categorised extraction failure. Reserved for cases the more
    /// specific variants do not cover; the message describes the situation.
    #[error("{0}")]
    Other(String),
}

/// Convenience `Result` alias for fallible `pdf-extract` operations.
pub type Result<T> = std::result::Result<T, ExtractError>;
