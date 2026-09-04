//! Render all XFA PDFs in the corpus to PNG images.
//!
//! This is used by the AVRT (Automated Visual Regression Testing) pipeline
//! to generate engine renders that are compared against Adobe gold masters.
//!
//! Usage:
//!   cargo run --release --bin corpus-render -- --corpus corpus/ --output renders/

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

// TODO: migrate from pdfium-ffi-bridge (#622)
// This binary depended on pdfium-ffi-bridge for:
//   - xfa_extract::scan_pdf_for_xfa
//   - template_parser::parse_template
//   - pipeline::render_form_tree
//   - pipeline::save_pages_as_png
//   - RenderConfig
// These need native Rust equivalents before this binary can be re-enabled.

fn main() {
    eprintln!("corpus-render is currently disabled — see TODO in source (#622)");
    std::process::exit(1);
}
