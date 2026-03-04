//! PDFium FFI Bridge — PDF rendering and XFA packet extraction.
//!
//! Provides Rust bindings to PDFium via pdfium-render,
//! handling XFA packet extraction, rendering, and UI events.

pub mod error;
pub mod renderer;
pub mod xfa_extract;
