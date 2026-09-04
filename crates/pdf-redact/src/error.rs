//! Error types for `pdf-redact` operations.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use thiserror::Error;

/// Errors returned by the `pdf-redact` crate while marking or applying
/// content redaction.
///
/// Redaction is high-stakes (compliance, legal disclosure) so each variant
/// describes a *refusal to proceed* rather than a best-effort fallback. If
/// you receive any variant, the input is left untouched.
#[derive(Debug, Error)]
pub enum RedactError {
    /// The underlying PDF byte stream could not be parsed.
    ///
    /// Typically a malformed cross-reference table, a truncated stream, or
    /// an encrypted PDF that was not opened with the right password.
    #[error("PDF error: {0}")]
    Pdf(#[from] lopdf::Error),

    /// An I/O error occurred while reading the source PDF or writing the
    /// redacted output.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// The caller asked to redact a page index that does not exist in the
    /// document.
    ///
    /// Fields: `(requested_page, total_pages)`. Page numbers are 1-based.
    #[error("page {0} out of range (document has {1} pages)")]
    PageOutOfRange(u32, u32),

    /// The redaction call had no regions and no search query — nothing to
    /// remove. Returned eagerly so callers do not silently produce
    /// no-op output.
    #[error("no redaction areas specified")]
    NoAreas,

    /// An image embedded inside a redaction region uses a PDF stream filter
    /// that the redactor cannot rewrite (for example JBIG2 or some JPEG2000
    /// profiles). The redaction is aborted because partially-stripped
    /// images would leak content. The caller can either rasterise the page
    /// before redacting or skip the affected page.
    #[error("unsupported image filter: {0}")]
    UnsupportedImageFilter(String),

    /// A page touched by the redaction has a `/ToUnicode` CMap whose shape
    /// the conservative `pdf-redact` parser cannot rewrite safely.
    ///
    /// The redactor refuses to silently drop the CMap because that would
    /// break text-extraction tools downstream. The caller's options are
    /// to rasterise the page first or to redact a different page range
    /// that does not include this font.
    #[error(
        "unsupported /ToUnicode CMap on redacted page (font {font_resource_name}): \
         redaction strip refuses to silently drop /ToUnicode; CMap shape not understood by the \
         conservative pdf-redact parser ({reason})"
    )]
    UnsupportedToUnicodeCMap {
        /// The PDF resource name of the font whose CMap is unsupported,
        /// e.g. `"F1"`.
        font_resource_name: String,
        /// A human-readable reason for the parser's refusal, e.g.
        /// `"surrogate pair without continuation"`.
        reason: String,
    },

    /// A non-categorised redaction failure. Reserved for cases the more
    /// specific variants do not cover; the message describes the situation.
    #[error("{0}")]
    Other(String),
}

/// Convenience `Result` alias for fallible `pdf-redact` operations.
pub type Result<T> = std::result::Result<T, RedactError>;
