//! Errors for the XFA DOM resolver.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use thiserror::Error;

/// Error type for XFA DOM operations.
#[derive(Debug, Error)]
pub enum XfaDomError {
    /// XML parse error.
    #[error("XML parse error: {0}")]
    XmlParse(#[from] roxmltree::Error),

    /// XFA packet not found.
    #[error("XFA packet not found: {0}")]
    PacketNotFound(String),

    /// SOM path resolution failed.
    #[error("SOM path resolution failed: {path}")]
    SomResolutionFailed {
        /// The SOM path that could not be resolved.
        path: String,
    },

    /// SOM parse error.
    #[error("SOM parse error at position {pos}: {message}")]
    SomParseError {
        /// Position in the input where parsing failed.
        pos: usize,
        /// Parse error message.
        message: String,
    },

    /// Invalid node type encountered.
    #[error("Invalid node type: expected {expected}, got {got}")]
    InvalidNodeType {
        /// Expected node type.
        expected: &'static str,
        /// Actual node type received.
        got: String,
    },

    /// Node not found.
    #[error("Node not found: {0}")]
    NodeNotFound(String),

    /// Index out of bounds.
    #[error("Index out of bounds: {index} (max {max})")]
    IndexOutOfBounds {
        /// The out-of-bounds index.
        index: usize,
        /// Maximum valid index.
        max: usize,
    },
}

/// Convenience type alias for `Result<T, XfaDomError>`.
pub type Result<T> = std::result::Result<T, XfaDomError>;
