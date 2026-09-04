//! Error types for annotation building.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

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

    /// Failed to write the annotation to the page dictionary (e.g. ObjStm page
    /// that lopdf cannot mutate in-place).  Fixes #470.
    #[error("failed to write annotation to page dictionary")]
    PageMutationFailed,
}
