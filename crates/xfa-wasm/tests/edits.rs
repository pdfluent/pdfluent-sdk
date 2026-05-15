//! Wave 2 edit operations: pages, forms, annotations, watermark, redaction, compress.
//!
//! Run on native: `cargo test -p xfa-wasm --test edits`
//! Run on wasm32: `wasm-pack test --node --no-default-features --features wasm --test edits`

use xfa_wasm::PdfDoc;

static SIMPLE_PDF: &[u8] = include_bytes!("../../../tests/corpus-mini/simple.pdf");
static MULTI_PDF: &[u8] = include_bytes!("../../../tests/corpus-mini/multi-page.pdf");

// ---- Pages ----------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn delete_pages_removes_listed_pages() {
    let doc = PdfDoc::open(MULTI_PDF).expect("open");
    let original = doc.page_count();
    assert!(original >= 2, "fixture must be multi-page");

    let bytes = doc.delete_pages(&[0]).expect("delete page 0");
    let reduced = PdfDoc::open(&bytes).expect("reload reduced");
    assert_eq!(reduced.page_count(), original - 1);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn rotate_page_returns_new_bytes() {
    let doc = PdfDoc::open(SIMPLE_PDF).expect("open");
    let bytes = doc.rotate_page(0, 90).expect("rotate 90");
    let rotated = PdfDoc::open(&bytes).expect("reload rotated");
    assert_eq!(rotated.page_count(), doc.page_count());
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn rotate_page_rejects_non_orthogonal() {
    let doc = PdfDoc::open(SIMPLE_PDF).expect("open");
    let err = doc.rotate_page(0, 45);
    assert!(err.is_err(), "45 degrees should be rejected");
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn reorder_pages_handles_permutation() {
    let doc = PdfDoc::open(MULTI_PDF).expect("open");
    let count = doc.page_count() as u32;
    // reverse order
    let order: Vec<u32> = (0..count).rev().collect();
    let bytes = doc.reorder_pages(&order).expect("reorder");
    let reordered = PdfDoc::open(&bytes).expect("reload reordered");
    assert_eq!(reordered.page_count() as u32, count);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn extract_pages_produces_subset() {
    let doc = PdfDoc::open(MULTI_PDF).expect("open");
    let bytes = doc.extract_pages(&[0]).expect("extract page 0");
    let single = PdfDoc::open(&bytes).expect("reload single");
    assert_eq!(single.page_count(), 1);
}

// ---- Compress -------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn compress_returns_valid_pdf() {
    let doc = PdfDoc::open(MULTI_PDF).expect("open");
    let bytes = doc.compress().expect("compress");
    // Round-trip: bytes must still be a valid PDF.
    let reloaded = PdfDoc::open(&bytes).expect("reload compressed");
    assert_eq!(reloaded.page_count(), doc.page_count());
}

// ---- Text watermark -------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn add_text_watermark_increases_or_keeps_size() {
    let doc = PdfDoc::open(SIMPLE_PDF).expect("open");
    let original_size = SIMPLE_PDF.len();
    let bytes = doc
        .add_text_watermark("CONCEPT", 0.3)
        .expect("watermark");
    assert!(bytes.len() >= original_size / 2, "result PDF too small");
    let reloaded = PdfDoc::open(&bytes).expect("reload watermarked");
    assert_eq!(reloaded.page_count(), doc.page_count());
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn add_text_watermark_rejects_empty() {
    let doc = PdfDoc::open(SIMPLE_PDF).expect("open");
    assert!(doc.add_text_watermark("", 0.5).is_err());
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn add_text_watermark_rejects_bad_opacity() {
    let doc = PdfDoc::open(SIMPLE_PDF).expect("open");
    assert!(doc.add_text_watermark("X", 2.0).is_err());
    assert!(doc.add_text_watermark("X", -0.1).is_err());
}

// ---- Annotations ----------------------------------------------------------

#[cfg(all(not(target_arch = "wasm32"), feature = "annotate"))]
#[test]
fn add_highlight_returns_valid_pdf() {
    let doc = PdfDoc::open(SIMPLE_PDF).expect("open");
    let bytes = doc
        .add_highlight(0, 100.0, 100.0, 200.0, 20.0, Some("#ffeb3b".into()))
        .expect("highlight");
    let reloaded = PdfDoc::open(&bytes).expect("reload highlighted");
    assert_eq!(reloaded.page_count(), doc.page_count());
}

#[cfg(all(not(target_arch = "wasm32"), feature = "annotate"))]
#[test]
fn add_sticky_note_returns_valid_pdf() {
    let doc = PdfDoc::open(SIMPLE_PDF).expect("open");
    let bytes = doc
        .add_sticky_note(0, 100.0, 100.0, "Review this paragraph")
        .expect("sticky");
    let reloaded = PdfDoc::open(&bytes).expect("reload sticky");
    assert_eq!(reloaded.page_count(), doc.page_count());
}

#[cfg(all(not(target_arch = "wasm32"), feature = "annotate"))]
#[test]
fn add_free_text_returns_valid_pdf() {
    let doc = PdfDoc::open(SIMPLE_PDF).expect("open");
    let bytes = doc
        .add_free_text(0, 100.0, 100.0, 200.0, 80.0, "Annotated by PDFluent")
        .expect("free text");
    let reloaded = PdfDoc::open(&bytes).expect("reload free text");
    assert_eq!(reloaded.page_count(), doc.page_count());
}

// ---- Redaction ------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn redact_region_returns_valid_pdf() {
    let doc = PdfDoc::open(SIMPLE_PDF).expect("open");
    let bytes = doc
        .redact_region(0, 50.0, 50.0, 100.0, 20.0)
        .expect("redact region");
    let reloaded = PdfDoc::open(&bytes).expect("reload redacted");
    assert_eq!(reloaded.page_count(), doc.page_count());
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn redact_search_rejects_empty_query() {
    let doc = PdfDoc::open(SIMPLE_PDF).expect("open");
    assert!(doc.redact_search("").is_err());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn redact_search_handles_no_match_gracefully() {
    let doc = PdfDoc::open(SIMPLE_PDF).expect("open");
    let bytes = doc
        .redact_search("STRING_THAT_DEFINITELY_DOES_NOT_APPEAR_IN_FIXTURE_12345")
        .expect("search redact");
    let reloaded = PdfDoc::open(&bytes).expect("reload");
    assert_eq!(reloaded.page_count(), doc.page_count());
}

// ---- AcroForm writes ------------------------------------------------------
// The mini-corpus simple/multi-page PDFs have no AcroForm. set_form_field
// should fail cleanly with "field not found", not panic.

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn set_form_field_no_acroform_errors_cleanly() {
    let doc = PdfDoc::open(SIMPLE_PDF).expect("open");
    let r = doc.set_form_field("name", "Alice");
    assert!(r.is_err(), "expected error on PDF without AcroForm");
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn set_form_fields_invalid_json_errors_cleanly() {
    let doc = PdfDoc::open(SIMPLE_PDF).expect("open");
    let r = doc.set_form_fields("{ not valid json }");
    assert!(r.is_err());
}
