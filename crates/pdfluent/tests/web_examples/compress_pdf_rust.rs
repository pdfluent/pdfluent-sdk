//! web_examples/compress_pdf_rust
//!
//! Source: <https://pdfluent.com/how-to/compress-pdf-rust> (placeholder — page not yet published)
//!
//! Compile-test + runtime coverage for the 3C-2 `compress()` surface.
//! This file will be replaced by the auto-extractor (#1236/#1275) once
//! the corresponding how-to page ships. The shape here locks the
//! expected snippet form in advance so the drift-guard can flag any
//! website deviation.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent::prelude::*;

/// Run the documented `compress` flow.
pub fn run(src: &std::path::Path, out: &std::path::Path) -> Result<CompressReport> {
    let mut doc = PdfDocument::open(src)?;
    let report = doc.compress(CompressOptions::strict())?;
    doc.save_with(out, SaveOptions::new().with_overwrite(true))?;
    Ok(report)
}

#[test]
fn compress_pdf_rust_compiles() {
    let _f: fn(&std::path::Path, &std::path::Path) -> Result<CompressReport> = run;
}

#[test]
fn compress_pdf_rust_runs() {
    let out = std::env::temp_dir().join("pdfluent-web-example-compressed.pdf");
    let _ = std::fs::remove_file(&out);
    let report =
        run(std::path::Path::new("tests/fixtures/sample.pdf"), &out).expect("compress pipeline");
    // Structural sanity — strict() preset enables all passes.
    assert!(report.font_subset.is_some());
    assert!(out.exists());
    let _ = std::fs::remove_file(&out);
}
