//! web_examples/render_pdf_to_jpeg_rust
//!
//! Source: <https://pdfluent.com/how-to/render-pdf-to-jpeg-rust> (placeholder — page not yet published)
//!
//! Compile-test + runtime coverage for the 3C-2 `to_images()` surface
//! with the JPEG format. Auto-extractor replaces this file once the
//! website page ships. Companion of `render_pdf_to_png_rust.rs`.
//!
//! # wasm note
//!
//! `to_images` is native-only (see `WASM_SUPPORT.md` §2.5). The
//! `_runs` test is gated accordingly.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent::prelude::*;

/// Run the documented `to_images` flow for JPEG output.
pub fn run(src: &std::path::Path, pattern: &std::path::Path) -> Result<ToImagesReport> {
    let doc = PdfDocument::open_with(
        src,
        pdfluent::OpenOptions::new().with_license_key("tier:team"),
    )?;
    doc.to_images(
        pattern,
        ToImagesOptions::new()
            .with_dpi(150)
            .with_format(ImageFormat::Jpeg),
    )
}

#[test]
fn render_pdf_to_jpeg_rust_compiles() {
    let _f: fn(&std::path::Path, &std::path::Path) -> Result<ToImagesReport> = run;
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn render_pdf_to_jpeg_rust_runs() {
    let dir = std::env::temp_dir().join("pdfluent-web-example-render_jpeg");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let pattern = dir.join("page_{page}.jpg");
    let report =
        run(std::path::Path::new("tests/fixtures/sample.pdf"), &pattern).expect("to_images jpeg");
    assert!(!report.paths.is_empty());
    // JPEG SOI magic.
    let bytes = std::fs::read(&report.paths[0]).expect("read");
    assert_eq!(&bytes[..2], &[0xFF, 0xD8]);
    let _ = std::fs::remove_dir_all(&dir);
}
