//! Bootstrap web_examples test suite.
//!
//! Scope: issue #1240. Each file corresponds to a how-to guide on
//! <https://pdfluent.com/how-to>. Tests here validate that the public
//! `pdfluent` API accepts the snippets verbatim (import paths, method
//! names, signatures). Runtime execution is gated on Epic 2 wiring: tests
//! that require an unimplemented method are marked `#[ignore]` with a link
//! to the blocking issue.
//!
//! Any time Epic 2 wires an additional method, the corresponding
//! `#[ignore]` attribute is removed in the same PR.

mod encrypt_pdf_rust;
mod extract_text_pdf_rust;
mod fill_pdf_form_rust;
mod merge_pdfs_rust;
mod render_pdf_to_png_rust;
