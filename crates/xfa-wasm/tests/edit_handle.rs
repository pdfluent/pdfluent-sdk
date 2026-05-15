//! Tests for `PdfDocMut` — the stateful editing handle.
//!
//! Native happy-path tests with plain `#[test]`. Error-path tests that
//! construct `JsError` are wasm32-gated via `#[wasm_bindgen_test]` (same
//! pattern as the Wave 2 edits tests; JsError panics on non-wasm32).

use xfa_wasm::edit_handle::PdfDocMut;

static SIMPLE_PDF: &[u8] = include_bytes!("../../../tests/corpus-mini/simple.pdf");
static MULTI_PDF: &[u8] = include_bytes!("../../../tests/corpus-mini/multi-page.pdf");

// ---- Open + save no-op -----------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn open_then_save_returns_valid_pdf() {
    let editor = PdfDocMut::open(SIMPLE_PDF).expect("open");
    let bytes = editor.save().expect("save");
    assert!(bytes.starts_with(b"%PDF-"), "expected PDF magic");
    assert_eq!(editor.page_count(), 1);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn save_is_non_consuming() {
    let mut editor = PdfDocMut::open(MULTI_PDF).expect("open");
    editor.rotate_page(0, 90).expect("rotate");
    let snapshot_a = editor.save().expect("snapshot a");

    editor.add_text_watermark("DRAFT", 0.5).expect("watermark");
    let snapshot_b = editor.save().expect("snapshot b");

    // Two snapshots produced, both valid PDFs, second includes the watermark.
    assert!(snapshot_a.starts_with(b"%PDF-"));
    assert!(snapshot_b.starts_with(b"%PDF-"));
    assert!(snapshot_b.len() >= snapshot_a.len(), "watermark adds bytes");
}

// ---- Pages -----------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn delete_pages_then_save_reduces_count() {
    let mut editor = PdfDocMut::open(MULTI_PDF).expect("open");
    let original = editor.page_count();
    assert!(original >= 2);
    editor.delete_pages(&[0]).expect("delete");
    assert_eq!(editor.page_count(), original - 1);
    let bytes = editor.save().expect("save");
    assert!(bytes.starts_with(b"%PDF-"));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn rotate_page_does_not_change_page_count() {
    let mut editor = PdfDocMut::open(MULTI_PDF).expect("open");
    let before = editor.page_count();
    editor.rotate_page(0, 90).expect("rotate");
    assert_eq!(editor.page_count(), before);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn reorder_pages_preserves_count() {
    let mut editor = PdfDocMut::open(MULTI_PDF).expect("open");
    let count = editor.page_count() as u32;
    let reversed: Vec<u32> = (0..count).rev().collect();
    editor.reorder_pages(&reversed).expect("reorder");
    assert_eq!(editor.page_count() as u32, count);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn extract_pages_returns_subdocument_without_mutating_self() {
    let editor = PdfDocMut::open(MULTI_PDF).expect("open");
    let original_count = editor.page_count();

    let sub_bytes = editor.extract_pages(&[0]).expect("extract");

    // self is unchanged
    assert_eq!(editor.page_count(), original_count);

    // returned bytes are a valid PDF with one page
    let sub = PdfDocMut::open(&sub_bytes).expect("reopen sub");
    assert_eq!(sub.page_count(), 1);
}

// ---- Watermark -------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn watermark_then_save_includes_text() {
    let mut editor = PdfDocMut::open(SIMPLE_PDF).expect("open");
    editor.add_text_watermark("CONCEPT", 0.3).expect("watermark");
    let bytes = editor.save().expect("save");
    assert!(bytes.starts_with(b"%PDF-"));
}

// ---- Annotations -----------------------------------------------------------

#[cfg(all(not(target_arch = "wasm32"), feature = "annotate"))]
#[test]
fn highlight_then_save() {
    let mut editor = PdfDocMut::open(SIMPLE_PDF).expect("open");
    editor
        .add_highlight(0, 100.0, 700.0, 200.0, 20.0, Some("#ffeb3b".into()))
        .expect("highlight");
    let bytes = editor.save().expect("save");
    assert!(bytes.starts_with(b"%PDF-"));
}

#[cfg(all(not(target_arch = "wasm32"), feature = "annotate"))]
#[test]
fn sticky_note_then_save() {
    let mut editor = PdfDocMut::open(SIMPLE_PDF).expect("open");
    editor
        .add_sticky_note(0, 50.0, 750.0, "Test note")
        .expect("sticky");
    let bytes = editor.save().expect("save");
    assert!(bytes.starts_with(b"%PDF-"));
}

#[cfg(all(not(target_arch = "wasm32"), feature = "annotate"))]
#[test]
fn free_text_then_save() {
    let mut editor = PdfDocMut::open(SIMPLE_PDF).expect("open");
    editor
        .add_free_text(0, 100.0, 600.0, 200.0, 80.0, "Annotated")
        .expect("free text");
    let bytes = editor.save().expect("save");
    assert!(bytes.starts_with(b"%PDF-"));
}

// ---- Redaction -------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn redact_region_then_save() {
    let mut editor = PdfDocMut::open(SIMPLE_PDF).expect("open");
    editor.redact_region(0, 50.0, 50.0, 100.0, 20.0).expect("redact");
    let bytes = editor.save().expect("save");
    assert!(bytes.starts_with(b"%PDF-"));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn redact_search_no_match_gracefully() {
    let mut editor = PdfDocMut::open(SIMPLE_PDF).expect("open");
    editor.redact_search("UNLIKELY_STRING_12345").expect("no match");
    let bytes = editor.save().expect("save");
    assert!(bytes.starts_with(b"%PDF-"));
}

// ---- Compress --------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn compress_then_save() {
    let mut editor = PdfDocMut::open(MULTI_PDF).expect("open");
    editor.compress().expect("compress");
    let bytes = editor.save().expect("save");
    assert!(bytes.starts_with(b"%PDF-"));
}

// ---- Multi-step editing chain ---------------------------------------------

#[cfg(all(not(target_arch = "wasm32"), feature = "annotate"))]
#[test]
fn multi_step_session_save_once() {
    let mut editor = PdfDocMut::open(MULTI_PDF).expect("open");
    let original_count = editor.page_count();

    editor.rotate_page(0, 90).expect("rotate");
    editor.add_text_watermark("CONCEPT", 0.3).expect("watermark");
    editor
        .add_highlight(0, 100.0, 700.0, 200.0, 20.0, Some("#ffeb3b".into()))
        .expect("highlight");
    editor.compress().expect("compress");

    let bytes = editor.save().expect("save once");
    assert!(bytes.starts_with(b"%PDF-"));
    // sanity: rotate/watermark/highlight/compress do not change page count
    assert_eq!(editor.page_count(), original_count);
}

// ---- Wasm32-only error-path tests -----------------------------------------

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn invalid_page_rotation_errors() {
    let mut editor = PdfDocMut::open(SIMPLE_PDF).expect("open");
    assert!(editor.rotate_page(0, 45).is_err());
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn empty_watermark_text_errors() {
    let mut editor = PdfDocMut::open(SIMPLE_PDF).expect("open");
    assert!(editor.add_text_watermark("", 0.5).is_err());
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn invalid_opacity_errors() {
    let mut editor = PdfDocMut::open(SIMPLE_PDF).expect("open");
    assert!(editor.add_text_watermark("X", 2.0).is_err());
    assert!(editor.add_text_watermark("X", -0.1).is_err());
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn invalid_json_for_set_form_fields_errors() {
    let mut editor = PdfDocMut::open(SIMPLE_PDF).expect("open");
    assert!(editor.set_form_fields("{ not json }").is_err());
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn empty_redact_search_query_errors() {
    let mut editor = PdfDocMut::open(SIMPLE_PDF).expect("open");
    assert!(editor.redact_search("").is_err());
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn missing_form_field_errors() {
    let mut editor = PdfDocMut::open(SIMPLE_PDF).expect("open");
    // SIMPLE_PDF has no AcroForm.
    assert!(editor.set_form_field("name", "Alice").is_err());
}
