//! Error types for EPUB content extraction.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use thiserror::Error;

/// Errors returned by `epub-extract` while reading an EPUB archive.
#[derive(Debug, Error)]
pub enum EpubError {
    /// The EPUB archive itself could not be opened or read (not a ZIP,
    /// corrupt central directory, unsupported compression).
    #[error("ZIP error: {0}")]
    Zip(#[from] zip::result::ZipError),

    /// An XML document inside the archive (`container.xml`, the package
    /// document, or a chapter) could not be parsed. The wrapped error names
    /// the position.
    #[error("XML error: {0}")]
    Xml(#[from] quick_xml::Error),

    /// An I/O error while reading an archive entry, including entries that
    /// are not valid UTF-8.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// A file the EPUB format requires is absent from the archive. Carries
    /// the archive path that was looked up (`META-INF/container.xml`, the
    /// package document, or a spine chapter).
    #[error("missing file in EPUB: {0}")]
    MissingFile(String),

    /// The archive is a readable ZIP but not a readable EPUB: no rootfile in
    /// `container.xml`, an empty spine, a spine item without a manifest
    /// entry, or an entry past the size limit.
    #[error("invalid EPUB: {0}")]
    InvalidFormat(String),

    /// A non-categorised extraction failure. Reserved for cases the more
    /// specific variants do not cover; the message describes the situation.
    #[error("{0}")]
    Other(String),
}

/// Convenience `Result` alias for fallible `epub-extract` operations.
pub type Result<T> = std::result::Result<T, EpubError>;
