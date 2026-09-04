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

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

mod encrypt_pdf_rust;
mod extract_text_pdf_rust;
mod fill_pdf_form_rust;
mod merge_pdfs_rust;
mod render_pdf_to_png_rust;

// ---------------------------------------------------------------------------
// Epic 5 #1237 — expanded compile + runtime coverage for the 3C-2
// parity methods. These files are placeholders until the corresponding
// how-to pages land on pdfluent.com; at that point the extractor
// (#1236) takes over and rewrites them in-place.
// ---------------------------------------------------------------------------

mod compress_pdf_rust;
mod convert_pdf_to_docx_rust;
mod insert_image_pdf_rust;
mod render_pdf_to_jpeg_rust;
mod subset_fonts_rust;
