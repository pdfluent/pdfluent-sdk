//! WASM smoke tests — 12 core scenarios.
//!
//! Run: wasm-pack test --headless --chrome (or --node for non-browser tests)

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use wasm_bindgen_test::*;
use xfa_wasm::{PdfDoc, XfaEngine};

// Embed test fixtures at compile time.
static SAMPLE_PDF: &[u8] = include_bytes!("../../../tests/corpus-mini/simple.pdf");
static MULTI_PDF: &[u8] = include_bytes!("../../../tests/corpus-mini/multi-page.pdf");

// ---------- Scenario 1: Open PDF, count pages ----------

#[wasm_bindgen_test]
fn test_01_open_and_page_count() {
    let doc = PdfDoc::open(SAMPLE_PDF).expect("open sample.pdf");
    assert!(doc.page_count() >= 1);
}

#[wasm_bindgen_test]
fn test_01b_multi_page() {
    let doc = PdfDoc::open(MULTI_PDF).expect("open multi-page.pdf");
    assert!(doc.page_count() > 1);
}

// ---------- Scenario 2: Render page 1 ----------

#[cfg(feature = "render")]
#[wasm_bindgen_test]
fn test_02_render_page() {
    let doc = PdfDoc::open(SAMPLE_PDF).expect("open");
    let data = doc.render_page(0, 1.0).expect("render");
    // Layout: [width:4LE][height:4LE][RGBA pixels...]
    assert!(data.len() > 8);
    let w = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let h = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    assert!(w > 0);
    assert!(h > 0);
    assert_eq!(data.len(), 8 + (w as usize) * (h as usize) * 4);
}

// ---------- Scenario 3: Extract text ----------

#[wasm_bindgen_test]
fn test_03_text_extraction() {
    let doc = PdfDoc::open(SAMPLE_PDF).expect("open");
    let text = doc.text(0);
    assert!(
        text.contains("Test page"),
        "unexpected text extraction: {text}"
    );
}

// ---------- Scenario 4: Read metadata ----------

#[wasm_bindgen_test]
fn test_04_metadata() {
    let doc = PdfDoc::open(SAMPLE_PDF).expect("open");
    let meta_json = doc.metadata();
    let meta: serde_json::Value = serde_json::from_str(&meta_json).expect("valid JSON");
    // Should have all standard keys (values may be null)
    assert!(meta.get("title").is_some());
    assert!(meta.get("author").is_some());
    assert!(meta.get("creator").is_some());
    assert!(meta.get("producer").is_some());
}

// ---------- Scenario 5: Read AcroForm fields ----------

#[wasm_bindgen_test]
fn test_05_form_fields_read() {
    // TODO: PdfDoc does not expose form field reading.
    // Requires extension of the WASM API.
}

// ---------- Scenario 6: Fill text field, save ----------

#[wasm_bindgen_test]
fn test_06_form_field_write() {
    // TODO: PdfDoc does not expose form field writing.
}

// ---------- Scenario 7: Read annotations ----------

#[cfg(feature = "annotate")]
#[wasm_bindgen_test]
fn test_07_annotations_read() {
    let doc = PdfDoc::open(SAMPLE_PDF).expect("open");
    let annots_json = doc.get_annotations(0).expect("get annotations");
    let annots: Vec<serde_json::Value> = serde_json::from_str(&annots_json).expect("valid JSON");
    // simple.pdf may or may not have annotations; just verify it parses
    let _ = annots.len();
}

// ---------- Scenario 8: Add highlight, save ----------

#[cfg(feature = "annotate")]
#[wasm_bindgen_test]
fn test_08_add_highlight() {
    let doc = PdfDoc::open(SAMPLE_PDF).expect("open");
    let new_pdf = doc
        .add_highlight(0, 100.0, 700.0, 200.0, 20.0, Some("#ffeb3b".into()))
        .expect("add highlight");
    assert!(new_pdf.len() > SAMPLE_PDF.len() / 2);
    let reloaded = PdfDoc::open(&new_pdf).expect("reopen");
    assert!(reloaded.page_count() >= 1);
}

// ---------- Scenario 9: Validate PDF/A ----------

#[wasm_bindgen_test]
fn test_09_pdfa_validation() {
    let doc = PdfDoc::open(SAMPLE_PDF).expect("open");
    let report_json = doc.validate_pdfa("pdfa2b").expect("validate");
    let report: serde_json::Value = serde_json::from_str(&report_json).expect("valid JSON");
    assert!(report.get("compliant").is_some());
    assert!(report.get("errors").is_some());
    assert!(report.get("issues").is_some());
}

// ---------- Scenario 10: Merge 2 PDFs ----------

#[wasm_bindgen_test]
fn test_10_merge_pdfs() {
    // TODO: PdfDoc does not expose PDF merge.
    // Requires extension with pdf_manip merge in WASM.
}

// ---------- Scenario 11: Verify signature ----------

#[wasm_bindgen_test]
fn test_11_verify_signatures() {
    let doc = PdfDoc::open(SAMPLE_PDF).expect("open");
    let result_json = doc.verify_signatures();
    let results: Vec<serde_json::Value> = serde_json::from_str(&result_json).expect("valid JSON");
    // simple.pdf likely has no signatures
    assert!(results.is_empty() || results[0].get("field_name").is_some());
}

// ---------- Scenario 12: Extract images ----------

#[wasm_bindgen_test]
fn test_12_extract_images() {
    // TODO: PdfDoc does not expose image extraction.
}

// ---------- Extra: page geometry ----------

#[wasm_bindgen_test]
fn test_page_geometry() {
    let doc = PdfDoc::open(SAMPLE_PDF).expect("open");
    let w = doc.page_width(0);
    let h = doc.page_height(0);
    assert!(w > 0.0);
    assert!(h > 0.0);
}

// ---------- Extra: thumbnail rendering ----------

#[cfg(feature = "render")]
#[wasm_bindgen_test]
fn test_render_thumbnail() {
    let doc = PdfDoc::open(SAMPLE_PDF).expect("open");
    let data = doc.render_thumbnail(0, 200).expect("thumbnail");
    assert!(data.len() > 8);
    let w = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let h = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    assert!(w > 0 && w <= 200);
    assert!(h > 0 && h <= 200);
}

// ---------- Extra: sticky note annotation ----------

#[cfg(feature = "annotate")]
#[wasm_bindgen_test]
fn test_add_sticky_note() {
    let doc = PdfDoc::open(SAMPLE_PDF).expect("open");
    let new_pdf = doc
        .add_sticky_note(0, 50.0, 750.0, "Test note")
        .expect("add sticky note");
    assert!(new_pdf.len() > SAMPLE_PDF.len() / 2);
    let reloaded = PdfDoc::open(&new_pdf).expect("reopen");
    assert!(reloaded.page_count() >= 1);
}

// ---------- Extra: DSS info ----------

#[wasm_bindgen_test]
fn test_dss_info() {
    let doc = PdfDoc::open(SAMPLE_PDF).expect("open");
    // simple.pdf likely has no DSS
    let dss = doc.dss_info();
    if let Some(json) = dss {
        let _: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
    }
}

// ---------- Extra: has_signatures ----------

#[wasm_bindgen_test]
fn test_has_signatures() {
    let doc = PdfDoc::open(SAMPLE_PDF).expect("open");
    // Just verify it doesn't crash
    let _ = doc.has_signatures();
}

// ---------- Extra: XfaEngine ----------

#[wasm_bindgen_test]
fn test_xfa_engine_basic() {
    let fields = r#"[
        {"name": "Name", "value": "Alice"},
        {"name": "Total", "value": "", "calculate": "10 + 20"}
    ]"#;
    let mut engine = XfaEngine::from_fields(fields).expect("from_fields");
    assert_eq!(engine.node_count(), 3);

    // Building the tree and reading plain values works here; evaluating
    // FormCalc does not. QuickJS cannot target wasm32-unknown-unknown, so the
    // scripting module is compiled out of this build (see 26e91f0e4).
    //
    // This test asserts the CONTRACT, not the wish. It used to expect "30" and
    // pass by never being run: nothing in CI executed the WASM suite, while the
    // method quietly returned Ok(()) and left the field empty. Any browser
    // caller was told the calculation had succeeded and got an empty field.
    //
    // So: the call must fail, it must explain why, and it must leave the field
    // as it was rather than half-written.
    let err = engine
        .run_calculations()
        .expect_err("FormCalc must report that it cannot run here, not claim success");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("not available in the WebAssembly build"),
        "error should say why FormCalc is unavailable, got: {msg}"
    );

    assert_eq!(
        engine.get_field_value("form1.Total"),
        Some(String::new()),
        "the calculated field must be untouched, not partially written"
    );
    assert_eq!(
        engine.get_field_value("form1.Name"),
        Some("Alice".to_string()),
        "plain field values must still be readable"
    );
}

// ---------- Extra: error handling ----------

#[wasm_bindgen_test]
fn test_invalid_pdf() {
    let result = PdfDoc::open(b"not a pdf");
    assert!(result.is_err());
}

#[cfg(feature = "render")]
#[wasm_bindgen_test]
fn test_render_out_of_range() {
    let doc = PdfDoc::open(SAMPLE_PDF).expect("open");
    let result = doc.render_page(999, 1.0);
    assert!(result.is_err());
}
