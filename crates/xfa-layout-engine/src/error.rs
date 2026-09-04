//! Layout engine error types.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

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
