//! Native PDF I/O — XFA packet extraction and PDF manipulation.
//!
//! Pure Rust implementation using `lopdf` for PDF structure parsing.
//! PDFium is available as an optional feature for visual comparison.

pub mod dataset_sync;
pub mod error;
pub mod events;
pub mod native_renderer;
pub mod pdf_reader;
pub mod pipeline;
pub mod xfa_extract;

#[cfg(feature = "pdfium")]
pub mod renderer;
