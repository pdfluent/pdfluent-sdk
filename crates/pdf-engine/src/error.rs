//! Error types for the rendering engine.

/// Errors that can occur in the rendering engine.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    /// The PDF data is invalid or could not be parsed.
    #[error("invalid PDF: {0}")]
    InvalidPdf(String),

    /// A page index is out of range.
    #[error("page {index} out of range (document has {count} pages)")]
    PageOutOfRange {
        /// The requested page index.
        index: usize,
        /// Total number of pages.
        count: usize,
    },

    /// A rendering error occurred.
    #[error("render error: {0}")]
    RenderError(String),

    /// An I/O error occurred.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Convenience type alias.
pub type Result<T> = std::result::Result<T, EngineError>;
