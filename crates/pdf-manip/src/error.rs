//! Error types for `pdf-manip` operations.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use thiserror::Error;

/// Errors returned by the `pdf-manip` crate while performing page-level
/// operations (merge, split, extract, rotate, watermark, encrypt, etc.).
///
/// These bubble up through the `pdfluent` facade as `Error::*` variants —
/// customers will normally see them indirectly. Variants with explicit
/// limit fields (image size, decompression cap) describe safety guards
/// that refused to proceed; the source document is not modified.
#[derive(Debug, Error)]
pub enum ManipError {
    /// The underlying PDF byte stream could not be parsed (malformed
    /// xref, truncated stream, unsupported construct). The wrapped
    /// `lopdf::Error` carries the detail.
    #[error("PDF error: {0}")]
    Pdf(#[from] lopdf::Error),

    /// An I/O error while reading the source or writing the result.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// A page index passed to a manipulation routine does not exist in
    /// the document. Fields: `(requested_index, total_pages)`. Page
    /// indexes at this layer are 0-based; the public facade translates
    /// to 1-based.
    #[error("page index {0} out of range (document has {1} pages)")]
    PageOutOfRange(usize, usize),

    /// A range-based operation (extract, split) received an empty range.
    /// Returned eagerly to surface the no-op rather than silently produce
    /// an empty document.
    #[error("empty page range")]
    EmptyPageRange,

    /// Setting up an encryption pipeline failed (key derivation, cipher
    /// initialization, or invalid permission flag combination). The
    /// wrapped string identifies the failing step.
    #[error("encryption error: {0}")]
    Encryption(String),

    /// Decryption failed. Either the supplied password is wrong, the
    /// document uses an algorithm this build does not support (RC4 in a
    /// no-RC4 build), or the encryption dictionary is malformed.
    #[error("decryption failed: wrong password or unsupported algorithm")]
    DecryptionFailed,

    /// The document advertises an encryption algorithm that the SDK
    /// cannot read. The wrapped string identifies the algorithm
    /// (e.g. `"V=4 RC4 with custom CFM"`).
    #[error("unsupported encryption: {0}")]
    UnsupportedEncryption(String),

    /// A bookmark/outline operation received an invalid bookmark
    /// description (missing title, invalid destination, malformed tree).
    #[error("invalid bookmark: {0}")]
    InvalidBookmark(String),

    /// A watermark operation could not be applied (font/image resolution
    /// failure, invalid coordinates, malformed text).
    #[error("watermark error: {0}")]
    Watermark(String),

    /// An image operation (insertion, replacement, watermark image)
    /// failed before the page was modified.
    #[error("image error: {0}")]
    Image(String),

    /// An image operation refused to proceed because the requested image
    /// would exceed the configured allocation safety limits.
    ///
    /// Allocation = `width * height * bytes_per_pixel`. The guards exist
    /// to prevent denial-of-service via crafted PDFs that trigger huge
    /// pixel-buffer allocations. If you genuinely need a larger image,
    /// raise the limit in the operation's options or pre-resize the
    /// image before insertion.
    #[error(
        "image {width}x{height} exceeds pdf-manip safety limits (max dimension {max_dimension}, max allocation {max_allocation_bytes} bytes at {bytes_per_pixel} bytes/pixel)"
    )]
    ImageTooLarge {
        /// Image width in pixels as decoded from the source.
        width: u32,
        /// Image height in pixels as decoded from the source.
        height: u32,
        /// Pixel-format depth (e.g. 4 for RGBA8) used to compute the
        /// allocation estimate.
        bytes_per_pixel: u32,
        /// The dimension cap (applied to both width and height) that the
        /// image violated.
        max_dimension: u32,
        /// The total-allocation cap in bytes that the image would have
        /// exceeded.
        max_allocation_bytes: usize,
    },

    /// A FlateDecode-compressed stream expanded past the configured
    /// decompression limit. Like [`ImageTooLarge`](Self::ImageTooLarge)
    /// this is a denial-of-service guard. The `u64` is the byte limit
    /// that was hit.
    #[error("decompressed FlateDecode stream exceeds {0}-byte limit")]
    DecompressionLimitExceeded(u64),

    /// The requested bold or italic variant font is not embedded in the document.
    ///
    /// `set_text_run_style` only swaps to fonts that are already present in the
    /// xref. Synthetic bold (stroke-and-fill) and system-font injection are
    /// explicitly forbidden. The caller must embed the variant before styling.
    ///
    /// `available_variants` lists the BaseFont names of all variants of the same
    /// family that *are* embedded, so the caller can surface actionable guidance.
    #[error(
        "font variant not embedded: cannot apply {requested:?} (current font {current:?}); \
         available embedded variants: {available_variants:?}"
    )]
    FontVariantNotEmbedded {
        /// BaseFont name of the current (pre-swap) font.
        current: String,
        /// BaseFont name of the desired variant that was not found.
        requested: String,
        /// All embedded font variants belonging to the same family.
        available_variants: Vec<String>,
    },

    /// A non-categorised manipulation failure. Reserved for cases the
    /// more specific variants do not cover; the message describes the
    /// situation.
    #[error("{0}")]
    Other(String),
}

/// Convenience `Result` alias for fallible `pdf-manip` operations.
pub type Result<T> = std::result::Result<T, ManipError>;
