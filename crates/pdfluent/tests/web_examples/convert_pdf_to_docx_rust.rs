//! web_examples/convert_pdf_to_docx_rust
//!
//! Source: <https://pdfluent.com/how-to/convert-pdf-to-docx-rust> (placeholder — page not yet published)
//!
//! Compile-test + runtime coverage for the 3C-2 `to_docx()` surface.
//! Auto-extractor replaces this file once the website page ships.
//!
//! # wasm note
//!
//! `to_docx` is a native-only method (see `WASM_SUPPORT.md` §2.5). On
//! wasm32 it returns `Error::UnsupportedOnWasm`. The `_runs` test is
//! therefore native-only; the `_compiles` test wraps both signatures
//! so the file builds on any target.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent::prelude::*;

/// Run the documented `to_docx` flow.
pub fn run(src: &std::path::Path, out: &std::path::Path) -> Result<()> {
    let doc = PdfDocument::open_with(src, pdfluent::OpenOptions::new())?;
    doc.to_docx(out)
}

#[test]
fn convert_pdf_to_docx_rust_compiles() {
    let _f: fn(&std::path::Path, &std::path::Path) -> Result<()> = run;
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn convert_pdf_to_docx_rust_runs() {
    let out = std::env::temp_dir().join("pdfluent-web-example-convert.docx");
    let _ = std::fs::remove_file(&out);
    run(std::path::Path::new("tests/fixtures/sample.pdf"), &out).expect("to_docx");
    let bytes = std::fs::read(&out).expect("docx on disk");
    assert_eq!(&bytes[..2], b"PK", "docx = ZIP (PK signature)");
    let _ = std::fs::remove_file(&out);
}
