#![warn(missing_docs)]
//! OCR integration for scanned PDFs with pluggable engine support.

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
