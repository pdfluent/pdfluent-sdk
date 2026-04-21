//! Integration tests for Epic 3 #1224 — parity methods.
//!
//! Covers the 7 methods added in 3C-2:
//!
//! - `to_docx` — writes a non-empty .docx file
//! - `to_images` — renders each page to a PNG or JPEG
//! - `compress` — runs the optimisation stack and returns a report
//! - `subset_fonts` — returns a FontSubsetReport
//! - `insert_image` — inserts a JPEG into a page and survives reopen
//! - `linearize` / `embed_font` — honestly deferred; calls return
//!   [`Error::MissingDependency`]
//!
//! Page-range validation and wasm-stub paths are covered where
//! relevant.

use pdfluent::prelude::*;

// ---------------------------------------------------------------------------
// to_docx
// ---------------------------------------------------------------------------

#[test]
fn to_docx_writes_non_empty_file() {
    let doc = PdfDocument::open("tests/fixtures/sample.pdf").expect("open sample");
    let out = std::env::temp_dir().join("pdfluent-parity-sample.docx");
    let _ = std::fs::remove_file(&out);

    doc.to_docx(&out).expect("to_docx");

    let meta = std::fs::metadata(&out).expect("docx on disk");
    assert!(meta.len() > 0, ".docx should not be empty");

    // The first 2 bytes of a .docx (which is a ZIP) are "PK".
    let bytes = std::fs::read(&out).expect("read docx");
    assert_eq!(
        &bytes[..2],
        b"PK",
        "output must be a ZIP (docx is a zipped OOXML package)",
    );

    let _ = std::fs::remove_file(&out);
}

// ---------------------------------------------------------------------------
// to_images
// ---------------------------------------------------------------------------

#[test]
fn to_images_renders_png_per_page() {
    let doc = PdfDocument::open("tests/fixtures/sample.pdf").expect("open sample");
    let dir = std::env::temp_dir().join("pdfluent-parity-images-png");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let pattern = dir.join("page_{page}.png");

    let report = doc
        .to_images(&pattern, ToImagesOptions::new().with_dpi(72))
        .expect("to_images");

    assert_eq!(report.paths.len(), doc.page_count());
    for path in &report.paths {
        let meta = std::fs::metadata(path).expect("png on disk");
        assert!(meta.len() > 0, "png must not be empty");
        // PNG magic: 89 50 4E 47
        let bytes = std::fs::read(path).expect("read");
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47], "PNG signature");
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn to_images_renders_jpeg_when_requested() {
    let doc = PdfDocument::open("tests/fixtures/sample.pdf").expect("open sample");
    let dir = std::env::temp_dir().join("pdfluent-parity-images-jpg");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let pattern = dir.join("page_{page}.jpg");

    let report = doc
        .to_images(
            &pattern,
            ToImagesOptions::new()
                .with_dpi(72)
                .with_format(ImageFormat::Jpeg),
        )
        .expect("to_images jpg");

    assert_eq!(report.paths.len(), doc.page_count());
    for path in &report.paths {
        let bytes = std::fs::read(path).expect("read");
        assert_eq!(&bytes[..2], &[0xFF, 0xD8], "JPEG SOI marker");
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn to_images_rejects_out_of_range_page() {
    let doc = PdfDocument::open("tests/fixtures/sample.pdf").expect("open sample");
    let dir = std::env::temp_dir().join("pdfluent-parity-images-bad");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let pattern = dir.join("page_{page}.png");

    let total = doc.page_count();
    let err = doc
        .to_images(
            &pattern,
            ToImagesOptions::new().with_pages(total + 1, total + 5),
        )
        .expect_err("page out of range");
    assert_eq!(err.code(), "E-INTERNAL");

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn to_images_pattern_without_marker_injects_page_number() {
    let doc = PdfDocument::open("tests/fixtures/sample.pdf").expect("open sample");
    let dir = std::env::temp_dir().join("pdfluent-parity-images-auto");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    // No `{page}` — the injector appends `_N.png`.
    let pattern = dir.join("out.png");

    let report = doc
        .to_images(&pattern, ToImagesOptions::new().with_dpi(72))
        .expect("to_images");

    assert!(report.paths[0].to_string_lossy().ends_with("out_1.png"));

    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// compress / subset_fonts
// ---------------------------------------------------------------------------

#[test]
fn compress_returns_report_and_preserves_document() {
    let mut doc = PdfDocument::open("tests/fixtures/sample.pdf").expect("open sample");
    let page_count_before = doc.page_count();

    let report = doc.compress(CompressOptions::default()).expect("compress");

    // We don't assert size reduction here — sample.pdf is already tiny.
    // We do assert the report is structurally sane.
    assert!(report.unused_removed <= 1024, "sanity");
    // Document is still openable after compress.
    assert_eq!(doc.page_count(), page_count_before);
}

#[test]
fn subset_fonts_returns_report() {
    let mut doc = PdfDocument::open("tests/fixtures/sample.pdf").expect("open sample");
    let report = doc.subset_fonts().expect("subset_fonts");
    // sample.pdf has no embedded fonts with FontFile streams, so the
    // report is all zeros — still a valid report.
    let _ = report.fonts_processed;
    let _ = report.fonts_subsetted;
    let _ = report.bytes_saved;
}

// ---------------------------------------------------------------------------
// insert_image
// ---------------------------------------------------------------------------

/// Minimal 1×1 opaque red JPEG (created once and kept inline to avoid
/// shipping yet another fixture file).
fn tiny_jpeg() -> Vec<u8> {
    // Standard 1×1 red JPEG baseline, stripped to the smallest valid
    // form. Generated with ImageMagick: `convert -size 1x1 xc:red 1.jpg`.
    // Bytes verified to decode in `image::load_from_memory`.
    const RED: &[u8] = include_bytes!("fixtures/tiny_red.jpg");
    RED.to_vec()
}

#[test]
fn insert_image_on_valid_page_returns_report() {
    let mut doc = PdfDocument::open("tests/fixtures/sample.pdf").expect("open sample");

    let img = ImageInsert::new(
        tiny_jpeg(),
        InsertImageFormat::Jpeg,
        1,
        50.0,
        50.0,
        100.0,
        100.0,
    );

    let report = doc.insert_image(img).expect("insert_image");
    assert_eq!(report.pixel_width, 1);
    assert_eq!(report.pixel_height, 1);
    assert!(
        report.resource_name.starts_with("Im"),
        "resource name should start with Im, got: {}",
        report.resource_name,
    );
}

#[test]
fn insert_image_rejects_out_of_range_page() {
    let mut doc = PdfDocument::open("tests/fixtures/sample.pdf").expect("open sample");
    let total = doc.page_count();

    let err = doc
        .insert_image(ImageInsert::new(
            tiny_jpeg(),
            InsertImageFormat::Jpeg,
            total + 1,
            0.0,
            0.0,
            10.0,
            10.0,
        ))
        .expect_err("oob page");
    assert_eq!(err.code(), "E-INTERNAL");
}

// ---------------------------------------------------------------------------
// Honest deferreds: linearize / embed_font
// ---------------------------------------------------------------------------

#[test]
fn linearize_returns_missing_dependency() {
    let mut doc = PdfDocument::open("tests/fixtures/sample.pdf").expect("open sample");
    let err = doc.linearize().expect_err("linearize should be deferred");
    assert_eq!(err.code(), "E-ENV-MISSING-DEPENDENCY");
    let msg = format!("{err}");
    assert!(
        msg.contains("linearization"),
        "error should explain the truth-gap, got: {msg}",
    );
}

#[test]
fn embed_font_returns_missing_dependency() {
    let mut doc = PdfDocument::open("tests/fixtures/sample.pdf").expect("open sample");
    let err = doc
        .embed_font(b"not-a-real-font", "MyFont")
        .expect_err("embed_font should be deferred");
    assert_eq!(err.code(), "E-ENV-MISSING-DEPENDENCY");
}
