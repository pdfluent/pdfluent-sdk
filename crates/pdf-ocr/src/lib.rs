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

#[cfg(feature = "tesseract-cli")]
pub mod tesseract_cli;

#[cfg(feature = "paddle")]
pub mod paddle;

/// The ONE shared ONNX Runtime layer for the opt-in model tiers: resolution,
/// checksums, one-time init, version check, session cache. Pin: ort
/// =2.0.0-rc.13, api-23, ONNX Runtime 1.23.2, loaded dynamically.
///
/// Always compiled: the specs, the failure types and the path resolution are
/// plain Rust, so a build without the tier still reports a missing model with
/// the typed [`ModelError`](runtime::ModelError) rather than a string. Only
/// the session half (`init`, `builder`, `session`, `SharedSession`) needs the
/// `onnx-runtime` feature and the `ort` crate.
pub mod runtime;

pub use engine::{NoOpEngine, OcrEngine, OcrPageResult, OcrWord};
pub use error::{OcrError, Result};
pub use pipeline::{make_searchable, OcrConfig, OcrPageReport, OcrReport};

#[cfg(feature = "tesseract")]
pub use tesseract::TesseractEngine;

#[cfg(feature = "tesseract-cli")]
pub use tesseract_cli::{TesseractCliEngine, TesseractCliError};

#[cfg(feature = "paddle")]
pub use paddle::PaddleOcrEngine;
