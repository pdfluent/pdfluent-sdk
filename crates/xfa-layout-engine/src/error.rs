//! Layout engine error types.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum LayoutError {
    #[error("No matching page area for layout state")]
    NoMatchingPageArea,

    #[error("Content area overflow: cannot place content")]
    ContentAreaOverflow,

    #[error("Invalid measurement: {0}")]
    InvalidMeasurement(String),

    #[error("Layout error: {0}")]
    General(String),
}

pub type Result<T> = std::result::Result<T, LayoutError>;
