//! web_examples/insert_image_pdf_rust
//!
//! Source: <https://pdfluent.com/how-to/insert-image-pdf-rust> (placeholder — page not yet published)
//!
//! Compile-test + runtime coverage for the 3C-2 `insert_image()` surface.
//! Auto-extractor replaces this file once the website page ships.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent::prelude::*;

/// Fixture: minimal 1×1 JPEG shipped with the SDK tests. The website
/// example will supply its own; the runtime test here reuses the
/// existing parity fixture so we don't drag a second binary fixture
/// into the repo.
fn tiny_jpeg_bytes() -> Vec<u8> {
    std::fs::read("tests/fixtures/tiny_red.jpg").expect("fixture read")
}

/// Run the documented `insert_image` flow.
pub fn run(src: &std::path::Path, out: &std::path::Path) -> Result<ImageInsertReport> {
    let mut doc = PdfDocument::open(src)?;
    let img = ImageInsert::new(
        tiny_jpeg_bytes(),
        InsertImageFormat::Jpeg,
        1,
        50.0,
        50.0,
        100.0,
        100.0,
    );
    let report = doc.insert_image(img)?;
    doc.save_with(out, SaveOptions::new().with_overwrite(true))?;
    Ok(report)
}

#[test]
fn insert_image_pdf_rust_compiles() {
    let _f: fn(&std::path::Path, &std::path::Path) -> Result<ImageInsertReport> = run;
}

#[test]
fn insert_image_pdf_rust_runs() {
    let out = std::env::temp_dir().join("pdfluent-web-example-insert_image.pdf");
    let _ = std::fs::remove_file(&out);
    let report = run(std::path::Path::new("tests/fixtures/sample.pdf"), &out)
        .expect("insert_image pipeline");
    assert_eq!(report.pixel_width, 1);
    assert_eq!(report.pixel_height, 1);
    assert!(out.exists());
    let _ = std::fs::remove_file(&out);
}
