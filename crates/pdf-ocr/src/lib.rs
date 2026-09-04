#![warn(missing_docs)]
//! OCR integration for scanned PDFs with pluggable engine support.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

pub mod engine;
pub mod error;
/// Integrity checking for anything fetched over the network. Compiled always,
/// feature flags or not, so its tests cannot be skipped out of CI.
pub mod integrity;
pub mod pipeline;

#[cfg(feature = "tesseract")]
pub mod tesseract;

#[cfg(feature = "paddle")]
pub mod paddle;

pub use engine::{NoOpEngine, OcrEngine, OcrPageResult, OcrWord};
pub use error::{OcrError, Result};
pub use pipeline::{make_searchable, OcrConfig, OcrPageReport, OcrReport};

#[cfg(feature = "tesseract")]
pub use tesseract::TesseractEngine;

#[cfg(feature = "paddle")]
pub use paddle::PaddleOcrEngine;
