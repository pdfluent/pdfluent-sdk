//! Thumbnail generation options.

/// Options for thumbnail generation.
#[derive(Debug, Clone)]
pub struct ThumbnailOptions {
    /// Maximum pixel dimension (longest side). Default: 256.
    pub max_dimension: u32,
}

impl Default for ThumbnailOptions {
    fn default() -> Self {
        Self { max_dimension: 256 }
    }
}
