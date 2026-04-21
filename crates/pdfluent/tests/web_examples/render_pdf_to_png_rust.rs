//! web_examples/render_pdf_to_png_rust
//!
//! Source: <https://pdfluent.com/how-to/render-pdf-to-png-rust> (fetched 2026-04-21)
//!
//! Validates rasterisation. `to_images` is part of the "remaining parity"
//! set (#1224) — this example is `#[ignore]` until that wiring lands.

use pdfluent::prelude::*;

#[allow(dead_code)]
/// Render each page of a PDF to a PNG file.
///
/// Uses `PdfDocument::to_images` (pending in Epic 2 #1224).
pub fn run() -> Result<()> {
    let _doc = PdfDocument::open("tests/fixtures/sample.pdf")?;
    // Placeholder: `to_images` is not on the scaffolded PdfDocument yet; it
    // lives in #1224 (remaining parity methods). When #1224 lands, this
    // example enables via:
    //
    //     _doc.to_images("/tmp/page-{}.png", ImageFormat::Png)?;
    Ok(())
}

#[test]
#[ignore = "blocked on Epic 2 #1224 (to_images wiring in remaining-parity umbrella)"]
fn render_png_rust_runs() {
    run().expect("render flow");
}

#[test]
fn render_png_rust_compiles() {
    let _f: fn() -> Result<()> = run;
}
