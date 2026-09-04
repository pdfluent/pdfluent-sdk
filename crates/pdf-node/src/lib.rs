//! Node.js bindings for the PDF engine (via napi-rs).
//!
//! Exposes `PdfDocument`, `PdfPage`, and rendering APIs
//! as async (Promise-based) and sync Node.js classes.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

mod annotation;
mod document;
mod error;
mod form;
mod functions;
mod license;
mod page;
mod text_edit;
