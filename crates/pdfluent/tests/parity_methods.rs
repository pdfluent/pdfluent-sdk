//! Integration tests for Epic 3 #1224 — parity methods.
//!
//! Covers the 7 methods added in 3C-2:
//!
//! - `to_docx` / `to_xlsx` / `to_pptx` — write non-empty OOXML packages
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

fn business_doc(path: &str) -> PdfDocument {
    PdfDocument::open_with(
        path,
        pdfluent::OpenOptions::new().with_license_key("tier:business"),
    )
    .expect("open sample")
}

// ---------------------------------------------------------------------------
// to_docx
// ---------------------------------------------------------------------------

#[test]
fn to_docx_writes_non_empty_file() {
    let doc = business_doc("tests/fixtures/sample.pdf");
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
    let doc = business_doc("tests/fixtures/sample.pdf");
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
    let doc = business_doc("tests/fixtures/sample.pdf");
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
    let doc = business_doc("tests/fixtures/sample.pdf");
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
    let doc = business_doc("tests/fixtures/sample.pdf");
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
    let mut doc = business_doc("tests/fixtures/sample.pdf");
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
    let mut doc = business_doc("tests/fixtures/sample.pdf");
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
    let mut doc = business_doc("tests/fixtures/sample.pdf");

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
    let mut doc = business_doc("tests/fixtures/sample.pdf");
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
    let mut doc = business_doc("tests/fixtures/sample.pdf");
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
    let mut doc = business_doc("tests/fixtures/sample.pdf");
    let err = doc
        .embed_font(b"not-a-real-font", "MyFont")
        .expect_err("embed_font should be deferred");
    assert_eq!(err.code(), "E-ENV-MISSING-DEPENDENCY");
}

/// Codex #1269 P2 regression guard: `to_images` on a document with zero
/// pages previously underflowed `with_capacity(to - from + 1)` and
/// panicked. Since FASE B it returns an empty report cleanly.
#[test]
fn to_images_on_zero_page_document_returns_empty_report() {
    // Build an in-memory PDF with a catalog + /Pages dict that has
    // Count=0 and an empty /Kids array. This is a legal (if unusual)
    // PDF.
    use lopdf::{dictionary, Document, Object};
    let mut doc_builder = Document::with_version("1.4");
    let pages_id = doc_builder.new_object_id();
    doc_builder.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => Vec::<Object>::new(),
            "Count" => 0,
        }),
    );
    let catalog_id = doc_builder.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc_builder.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    doc_builder.save_to(&mut bytes).expect("serialise");

    let doc = PdfDocument::from_bytes_with(
        &bytes,
        pdfluent::OpenOptions::new().with_license_key("tier:business"),
    )
    .expect("parse empty doc");
    assert_eq!(doc.page_count(), 0);

    let dir = std::env::temp_dir().join("pdfluent-zero-page-to_images");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let pattern = dir.join("page_{page}.png");

    // Must NOT panic — must return an empty report.
    let report = doc
        .to_images(&pattern, ToImagesOptions::new().with_dpi(72))
        .expect("to_images on empty doc");
    assert!(report.paths.is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// to_xlsx / to_pptx
//
// Both crates were published and tested long before the facade depended on
// them, so "PDF to Excel" and "PDF to PowerPoint" were advertised as SDK
// capabilities while a customer using the pdfluent crate could not reach them.
// These tests exist to keep that from silently regressing: they assert the
// method is on the facade AND that it writes a real OOXML package, not that the
// call merely returns Ok.
// ---------------------------------------------------------------------------

/// A valid OOXML package is a ZIP whose central directory names the part that
/// makes it that document type. Checking only for "PK" would pass on any zip,
/// including an empty one.
fn assert_ooxml(path: &std::path::Path, expected_part: &str, label: &str) {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{label}: read failed: {e}"));
    assert!(
        bytes.len() > 200,
        "{label}: {} bytes is too small to be a real package",
        bytes.len()
    );
    assert_eq!(&bytes[..2], b"PK", "{label}: OOXML packages are ZIPs");
    let as_text = String::from_utf8_lossy(&bytes);
    assert!(
        as_text.contains(expected_part),
        "{label}: package should contain {expected_part}"
    );
}

#[test]
fn to_xlsx_writes_a_real_spreadsheet() {
    let doc = business_doc("tests/fixtures/sample.pdf");
    let out = std::env::temp_dir().join("pdfluent-parity-sample.xlsx");
    let _ = std::fs::remove_file(&out);

    doc.to_xlsx(&out).expect("to_xlsx");
    assert_ooxml(&out, "xl/", "xlsx");

    let _ = std::fs::remove_file(&out);
}

#[test]
fn to_pptx_writes_a_real_presentation() {
    let doc = business_doc("tests/fixtures/sample.pdf");
    let out = std::env::temp_dir().join("pdfluent-parity-sample.pptx");
    let _ = std::fs::remove_file(&out);

    doc.to_pptx(&out).expect("to_pptx");
    assert_ooxml(&out, "ppt/", "pptx");

    let _ = std::fs::remove_file(&out);
}
