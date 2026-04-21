//! Integration tests for Epic 2 #1242 — Document lifecycle & I/O wiring.
//!
//! These tests exercise the real wiring against `lopdf` and `pdf-engine`.
//! They complement the `web_examples` bootstrap suite: the bootstrap tests
//! validate website-verbatim snippets; these tests validate the wiring
//! contract (constructors, save round-trip, version parsing, iterator).

use std::io::Cursor;

use pdfluent::prelude::*;

const FIXTURE_PATH: &str = "tests/fixtures/sample.pdf";

// ---------------------------------------------------------------------------
// Constructors
// ---------------------------------------------------------------------------

#[test]
fn open_from_path_parses_fixture() {
    let doc = PdfDocument::open(FIXTURE_PATH).expect("open path");
    assert_eq!(doc.page_count(), 1, "fixture has 1 page");
}

#[test]
fn from_bytes_parses_same_content_as_open() {
    let bytes = std::fs::read(FIXTURE_PATH).expect("read fixture");
    let doc = PdfDocument::from_bytes(&bytes).expect("from_bytes");
    assert_eq!(doc.page_count(), 1);
}

#[test]
fn from_reader_reads_stream() {
    let bytes = std::fs::read(FIXTURE_PATH).expect("read fixture");
    let cursor = Cursor::new(bytes);
    let doc = PdfDocument::from_reader(cursor).expect("from_reader");
    assert_eq!(doc.page_count(), 1);
}

#[test]
fn create_produces_minimal_parseable_doc() {
    let doc = PdfDocument::create();
    assert_eq!(doc.page_count(), 1, "create() seeds a single blank page");
}

#[test]
fn open_missing_file_returns_file_not_found() {
    let err = PdfDocument::open("/nonexistent/path/to/file.pdf").unwrap_err();
    assert!(
        matches!(err, pdfluent::Error::FileNotFound { .. }),
        "expected FileNotFound, got {err:?}",
    );
}

#[test]
fn from_bytes_with_memory_limit_rejects_large_input() {
    let bytes = std::fs::read(FIXTURE_PATH).expect("read fixture");
    let err =
        PdfDocument::from_bytes_with(&bytes, pdfluent::OpenOptions::new().strict_memory_limit(16))
            .unwrap_err();
    assert!(
        matches!(err, pdfluent::Error::MemoryBudgetExceeded { .. }),
        "expected MemoryBudgetExceeded, got {err:?}",
    );
}

// ---------------------------------------------------------------------------
// Read-only content
// ---------------------------------------------------------------------------

#[test]
fn version_reads_header() {
    let doc = PdfDocument::open(FIXTURE_PATH).expect("open");
    let v = doc.version();
    // fixture is PDF-1.7 (regenerated via lopdf in gen_fixture)
    assert_eq!(v.major, 1);
    assert_eq!(v.minor, 7);
    assert_eq!(format!("{v}"), "1.7");
}

#[test]
fn text_extracts_fixture_content() {
    let doc = PdfDocument::open(FIXTURE_PATH).expect("open");
    let text = doc.text().expect("text");
    assert!(
        text.contains("PDFluent test fixture"),
        "expected fixture text; got {text:?}",
    );
}

#[test]
fn text_with_layout_produces_blocks() {
    let doc = PdfDocument::open(FIXTURE_PATH).expect("open");
    let blocks = doc.text_with_layout().expect("text_with_layout");
    // Minimal fixture has exactly one text block on page 1.
    assert!(
        !blocks.is_empty(),
        "expected at least one text block for fixture with content",
    );
    for block in &blocks {
        assert_eq!(block.page, 1);
        assert_eq!(block.bbox.len(), 4);
    }
}

#[test]
fn page_by_index_returns_borrowed_handle() {
    let doc = PdfDocument::open(FIXTURE_PATH).expect("open");
    let page = doc.page(1).expect("page 1");
    assert_eq!(page.number(), 1);
}

#[test]
fn page_out_of_range_errors() {
    let doc = PdfDocument::open(FIXTURE_PATH).expect("open");
    assert!(doc.page(0).is_err(), "page 0 is invalid (1-based)");
    assert!(doc.page(999).is_err(), "page 999 is beyond doc length");
}

#[test]
fn pages_iterator_yields_exactly_page_count() {
    let doc = PdfDocument::open(FIXTURE_PATH).expect("open");
    let count: usize = doc.pages().count();
    assert_eq!(count, doc.page_count());
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

#[test]
fn save_roundtrip_via_path() {
    let tmp = std::env::temp_dir().join("pdfluent-test-save-roundtrip.pdf");
    let _ = std::fs::remove_file(&tmp);

    let doc = PdfDocument::open(FIXTURE_PATH).expect("open");
    doc.save(&tmp).expect("save");

    // Re-open the saved file and verify content survives the round-trip.
    let reloaded = PdfDocument::open(&tmp).expect("re-open saved");
    assert_eq!(reloaded.page_count(), doc.page_count());

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn to_bytes_returns_parseable_output() {
    let doc = PdfDocument::open(FIXTURE_PATH).expect("open");
    let bytes = doc.to_bytes().expect("to_bytes");
    assert!(
        bytes.starts_with(b"%PDF-"),
        "serialised output must begin with %PDF- header",
    );
    // Round-trip: re-parse the bytes.
    let reloaded = PdfDocument::from_bytes(&bytes).expect("re-parse serialised bytes");
    assert_eq!(reloaded.page_count(), doc.page_count());
}

#[test]
fn write_to_forwards_full_bytes() {
    let doc = PdfDocument::open(FIXTURE_PATH).expect("open");
    let mut buf: Vec<u8> = Vec::new();
    doc.write_to(&mut buf).expect("write_to");
    assert!(buf.starts_with(b"%PDF-"));
    let reloaded = PdfDocument::from_bytes(&buf).expect("re-parse write_to output");
    assert_eq!(reloaded.page_count(), doc.page_count());
}

#[test]
fn save_with_linearize_is_noop_in_1_0() {
    // Per RFC §14 v1.3 + SaveOptions::with_linearize rustdoc: linearize is
    // accepted but a no-op in 1.0. Assert this behaviour is stable so
    // users can opt in for forward-compat without runtime surprises.
    let tmp = std::env::temp_dir().join("pdfluent-test-linearize-noop.pdf");
    let _ = std::fs::remove_file(&tmp);
    let doc = PdfDocument::open(FIXTURE_PATH).expect("open");
    doc.save_with(&tmp, pdfluent::SaveOptions::new().with_linearize(true))
        .expect("save_with linearize=true must not error in 1.0");
    let _ = std::fs::remove_file(&tmp);
}
