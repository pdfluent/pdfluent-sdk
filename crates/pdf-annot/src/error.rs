//! Error types for annotation building.

/// Errors that can occur when building annotations.
#[derive(Debug, thiserror::Error)]
pub enum AnnotBuildError {
    /// Page number is out of range.
    #[error("page {0} out of range (document has {1} pages)")]
    PageOutOfRange(u32, usize),

    /// Failed to encode appearance stream content.
    #[error("failed to encode appearance stream: {0}")]
    AppearanceEncode(String),

    /// The annotation rectangle is invalid (zero area).
    #[error("invalid annotation rectangle: width or height is zero")]
    InvalidRect,
}
