//! Thumbnail generation options.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

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
