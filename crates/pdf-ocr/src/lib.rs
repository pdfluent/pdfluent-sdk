//! OCR integration for scanned PDFs with pluggable engine support.

pub mod engine;
pub mod error;
pub mod pipeline;

pub use engine::{NoOpEngine, OcrEngine, OcrPageResult, OcrWord};
pub use error::{OcrError, Result};
pub use pipeline::{make_searchable, OcrConfig, OcrPageReport, OcrReport};
