//! Native PDF I/O — XFA packet extraction and PDF manipulation.
//!
//! Pure Rust implementation using `lopdf` for PDF structure parsing.
//! PDFium is available as an optional feature for visual comparison.

pub mod appearance;
pub mod appearance_cache;
pub mod colorspace;
pub mod dataset_sync;
pub mod docmdp;
pub mod error;
pub mod events;
pub mod fieldmdp;
pub mod flatten;
pub mod font;
pub mod native_renderer;
pub mod pdf_reader;
pub mod pdfa_sanitize;
pub mod pipeline;
pub mod template_parser;
pub mod ur3;
pub mod xfa_extract;
pub mod xmp;

#[cfg(feature = "pdfium")]
pub mod renderer;
