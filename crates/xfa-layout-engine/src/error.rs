//! Layout engine error types.

use thiserror::Error;

#[derive(Debug, Error)]
/// Layout error.
pub enum LayoutError {
    #[error("No matching page area for layout state")]
    /// No matching page area.
    NoMatchingPageArea,

    #[error("Content area overflow: cannot place content")]
    /// Content area overflow.
    ContentAreaOverflow,

    #[error("Invalid measurement: {0}")]
    /// Invalid measurement.
    InvalidMeasurement(String),

    #[error("Layout error: {0}")]
    /// General error.
    General(String),
}

/// Result type alias.
pub type Result<T> = std::result::Result<T, LayoutError>;
