//! Node.js bindings for the PDF engine (via napi-rs).
//!
//! Exposes `PdfDocument`, `PdfPage`, and rendering APIs
//! as async (Promise-based) and sync Node.js classes.

mod annotation;
mod document;
mod error;
mod form;
mod page;
