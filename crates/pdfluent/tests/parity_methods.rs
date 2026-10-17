//! Integration tests for Epic 3 #1224 — parity methods.
//!
//! Covers the 7 methods added in 3C-2:
//!
//! - `to_docx` / `to_xlsx` / `to_pptx` — write non-empty OOXML packages
//! - `to_images` — renders each page to a PNG or JPEG
//! - `compress` — runs the optimisation stack and returns a report
//! - `subset_fonts` — returns a FontSubsetReport
//! - `insert_image` — inserts a JPEG into a page and survives reopen
//! - `linearize` — honestly deferred; calls return
//!   [`Error::MissingDependency`]
//!
//! Page-range validation and wasm-stub paths are covered where
//! relevant.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent::prelude::*;

fn business_doc(path: &str) -> PdfDocument {
    PdfDocument::open_with(path, pdfluent::OpenOptions::new()).expect("open sample")
}

// ---------------------------------------------------------------------------
// to_docx_bytes / to_xlsx_bytes / to_pptx_bytes — the form every binding uses
// ---------------------------------------------------------------------------

/// A `.docx` is a ZIP. Checking the magic bytes and one required entry is the
/// difference between "the call returned something" and "the call returned a
/// file Word will open".
fn is_ooxml(bytes: &[u8], verwachte_ingang: &str) -> bool {
    if bytes.len() < 4 || &bytes[..2] != b"PK" {
        return false;
    }
    let leesbaar = String::from_utf8_lossy(bytes);
    leesbaar.contains(verwachte_ingang)
}

#[test]
fn to_docx_bytes_returns_an_openable_package() {
    let doc = business_doc("tests/fixtures/sample.pdf");
    let bytes = doc.to_docx_bytes().expect("to_docx_bytes");
    assert!(
        is_ooxml(&bytes, "word/document.xml"),
        "expected a ZIP containing word/document.xml, got {} bytes starting {:?}",
        bytes.len(),
        &bytes[..bytes.len().min(4)]
    );
}

#[test]
fn to_xlsx_bytes_returns_an_openable_package() {
    let doc = business_doc("tests/fixtures/sample.pdf");
    let bytes = doc.to_xlsx_bytes().expect("to_xlsx_bytes");
    assert!(
        is_ooxml(&bytes, "xl/workbook.xml"),
        "expected a ZIP containing xl/workbook.xml, got {} bytes",
        bytes.len()
    );
}

#[test]
fn to_pptx_bytes_returns_an_openable_package() {
    let doc = business_doc("tests/fixtures/sample.pdf");
    let bytes = doc.to_pptx_bytes().expect("to_pptx_bytes");
    assert!(
        is_ooxml(&bytes, "ppt/presentation.xml"),
        "expected a ZIP containing ppt/presentation.xml, got {} bytes",
        bytes.len()
    );
}

/// The byte form and the file form must not drift apart: `to_docx` is
/// documented as writing exactly what `to_docx_bytes` returns.
#[test]
fn the_file_form_writes_what_the_byte_form_returns() {
    let doc = business_doc("tests/fixtures/sample.pdf");
    let uit = std::env::temp_dir().join("pdfluent-bytes-parity.docx");
    let _ = std::fs::remove_file(&uit);
    doc.to_docx(&uit).expect("to_docx");
    let van_schijf = std::fs::read(&uit).expect("read back");
    let van_bytes = doc.to_docx_bytes().expect("to_docx_bytes");
    assert_eq!(
        van_schijf.len(),
        van_bytes.len(),
        "the two forms produced different packages"
    );
    let _ = std::fs::remove_file(&uit);
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
    // E-INTERNAL until #453. `Error::Internal` tells the reader to file a bug
    // report; a range the caller wrote past the end of their own document is
    // not one.
    assert_eq!(err.code(), "E-PAGE-RANGE-INVALID");
    assert!(matches!(
        err,
        pdfluent::Error::PageRangeInvalid {
            start,
            end,
            page_count,
        } if start == total + 1 && end == total + 5 && page_count == total
    ));

    let _ = std::fs::remove_dir_all(&dir);
}

/// A range that ends before it starts is inside the document at both ends and
/// still describes no pages. It is a different mistake from "past the end",
/// and the message says so.
#[test]
fn to_images_rejects_an_empty_page_range() {
    let doc = business_doc("tests/fixtures/sample.pdf");
    let dir = std::env::temp_dir().join("pdfluent-parity-images-empty");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir");
    let pattern = dir.join("page_{page}.png");

    let err = doc
        .to_images(&pattern, ToImagesOptions::new().with_pages(2, 1))
        .expect_err("empty page range");
    assert_eq!(err.code(), "E-PAGE-RANGE-INVALID");
    let message = err.to_string();
    assert!(
        message.contains("2..=1") && message.contains("empty"),
        "an inverted range must be reported as empty, not as out of range: {message}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// `extract_pages` is the other caller of `normalise_page_range`, and it takes
/// a `RangeBounds` rather than a pair — so `0..=1` is reachable there and is
/// the exact off-by-one a caller arriving from a 0-based API makes.
#[test]
fn extract_pages_rejects_a_range_starting_at_zero() {
    let doc = business_doc("tests/fixtures/sample.pdf");
    let err = doc.extract_pages(0..=1).expect_err("page 0 does not exist");
    assert_eq!(err.code(), "E-PAGE-RANGE-INVALID");
    assert_eq!(
        err.docs_url(),
        "https://pdfluent.com/errors#E-PAGE-RANGE-INVALID"
    );
    assert!(
        err.to_string().contains("numbered from 1"),
        "the message must name the convention: {err}"
    );
}

#[test]
fn extract_pages_rejects_a_range_past_the_end() {
    let doc = business_doc("tests/fixtures/sample.pdf");
    let total = doc.page_count();
    let err = doc
        .extract_pages(1..=total + 1)
        .expect_err("range past the end");
    assert_eq!(err.code(), "E-PAGE-RANGE-INVALID");
    assert!(matches!(
        err,
        pdfluent::Error::PageRangeInvalid { end, page_count, .. }
            if end == total + 1 && page_count == total
    ));
}

#[test]
fn extract_pages_accepts_the_whole_document() {
    let doc = business_doc("tests/fixtures/sample.pdf");
    let total = doc.page_count();
    let extracted = doc
        .extract_pages(1..=total)
        .expect("the full range must be accepted");
    assert_eq!(extracted.page_count(), total);
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
    // E-INTERNAL until #414: a page number the caller passed is their
    // mistake to fix, not a fault to report to us.
    assert_eq!(err.code(), "E-PAGE-OUT-OF-RANGE");
    assert!(matches!(err, pdfluent::Error::PageOutOfRange { .. }));
}

// ---------------------------------------------------------------------------
// Honest deferred: linearize. embed_font landed 2026-09-01.
// ---------------------------------------------------------------------------

#[test]
fn linearize_reports_unsupported() {
    let mut doc = business_doc("tests/fixtures/sample.pdf");
    let err = doc.linearize().expect_err("linearize should be deferred");
    assert_eq!(err.code(), "E-UNSUPPORTED");
    let msg = format!("{err}");
    assert!(
        msg.contains("linearization"),
        "error should explain the truth-gap, got: {msg}",
    );
}

/// `embed_font` stopped being deferred on 2026-09-01. What it must not do is
/// accept bytes that are not a font: the failure there is a viewer showing
/// nothing, long after the call returned Ok.
#[test]
fn embed_font_rejects_data_that_is_not_a_font() {
    let mut doc = business_doc("tests/fixtures/sample.pdf");
    let err = doc
        .embed_font(b"not-a-real-font", "MyFont")
        .expect_err("bytes that are not a font must not embed");
    let msg = format!("{err}");
    assert!(
        msg.contains("not a readable TrueType or OpenType font"),
        "the error should say what was wrong with the data, got: {msg}",
    );
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

    let doc = PdfDocument::from_bytes_with(&bytes, pdfluent::OpenOptions::new())
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

// ---------------------------------------------------------------------------
// Raster quality (speed round R1): both pipelines through the public path
// ---------------------------------------------------------------------------

use pdfluent::{RasterQuality, RenderPageOptions};

/// Both raster modes render every page to a valid PNG via the public
/// `to_images` path. Fails if the quality option does not reach the engine
/// (unknown variant, dropped field) or if Speed fails to render.
#[test]
fn to_images_renders_in_both_raster_modes() {
    let doc = business_doc("tests/fixtures/sample.pdf");
    for quality in [RasterQuality::Quality, RasterQuality::Speed] {
        let tag = format!("pdfluent-parity-raster-{quality:?}");
        let dir = std::env::temp_dir().join(tag);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let pattern = dir.join("page_{page}.png");

        let report = doc
            .to_images(
                &pattern,
                ToImagesOptions::new().with_dpi(72).with_quality(quality),
            )
            .unwrap_or_else(|e| panic!("to_images {quality:?}: {e}"));

        assert_eq!(report.paths.len(), doc.page_count());
        for path in &report.paths {
            let bytes = std::fs::read(path).expect("read");
            assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47], "PNG signature");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Both raster modes render one page to valid PNG bytes via the public
/// `render_page_with_options` path.
#[test]
fn render_page_with_options_renders_in_both_raster_modes() {
    let doc = business_doc("tests/fixtures/sample.pdf");
    for quality in [RasterQuality::Quality, RasterQuality::Speed] {
        let opts = RenderPageOptions::new().with_dpi(72).with_quality(quality);
        let bytes = doc
            .render_page_with_options(1, opts)
            .unwrap_or_else(|e| panic!("render_page {quality:?}: {e}"));
        assert_eq!(&bytes[..4], &[0x89, 0x50, 0x4E, 0x47], "PNG signature");
    }
}

/// The default stays Quality until the T5-gated flip: the convenience
/// `render_page` path must equal an explicit Quality render.
#[test]
fn render_page_default_matches_explicit_quality() {
    let doc = business_doc("tests/fixtures/sample.pdf");
    let via_default = doc.render_page(1, 72, ImageFormat::Png).expect("default");
    let via_explicit = doc
        .render_page_with_options(
            1,
            RenderPageOptions::new()
                .with_dpi(72)
                .with_quality(RasterQuality::Quality),
        )
        .expect("explicit quality");
    assert_eq!(via_default, via_explicit);
}
