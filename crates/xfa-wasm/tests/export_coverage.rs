//! Coverage for every exported method the other WASM suites never call.
//!
//! WHY THIS FILE EXISTS
//!
//! `convertToPdfa` was broken in the WebAssembly build for three months. Every
//! native test stayed green the whole time, because the fault was not in the
//! logic but in the binding: a `SystemTime::now()` that compiles for wasm32 and
//! then traps at runtime. It surfaced only when a gate test on the website hit
//! it (see crates/pdf-manip/src/clock.rs).
//!
//! A smoke suite for the binding already existed and would have caught it on day
//! one — except that nothing ever ran it, and it did not call this method
//! anyway. Of the 25 exported methods, 16 were covered.
//!
//! So the point here is not depth. The native suites test the logic. This file
//! calls each remaining export once, on a real PDF, purely to prove the binding
//! survives the crossing. That is the cheapest test that exists and it is the
//! one that was missing.
//!
//! Run: wasm-pack test --node crates/xfa-wasm --no-default-features --features wasm

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use wasm_bindgen_test::*;
use xfa_wasm::{PdfDoc, XfaEngine};

static SAMPLE_PDF: &[u8] = include_bytes!("../../../tests/corpus-mini/simple.pdf");
static MULTI_PDF: &[u8] = include_bytes!("../../../tests/corpus-mini/multi-page.pdf");

// ---------- Office export: the capability the browser SDK never had ----------

/// An OOXML package is a ZIP with a known entry. The binding could return
/// bytes and still be useless; checking the entry is what separates the two.
fn assert_ooxml(bytes: &[u8], entry: &str, what: &str) {
    assert!(!bytes.is_empty(), "{what}: empty");
    assert_eq!(&bytes[..2], b"PK", "{what}: not a ZIP");
    let readable = String::from_utf8_lossy(bytes);
    assert!(readable.contains(entry), "{what}: no {entry} inside");
}

#[wasm_bindgen_test]
fn to_docx_runs_in_wasm() {
    let doc = PdfDoc::open(SAMPLE_PDF).expect("open sample.pdf");
    let out = doc
        .to_docx()
        .expect("toDocx must not trap in the wasm build");
    assert_ooxml(&out, "word/document.xml", "docx");
}

#[wasm_bindgen_test]
fn to_xlsx_runs_in_wasm() {
    let doc = PdfDoc::open(SAMPLE_PDF).expect("open sample.pdf");
    let out = doc
        .to_xlsx()
        .expect("toXlsx must not trap in the wasm build");
    assert_ooxml(&out, "xl/workbook.xml", "xlsx");
}

#[wasm_bindgen_test]
fn to_pptx_runs_in_wasm() {
    let doc = PdfDoc::open(MULTI_PDF).expect("open multi-page.pdf");
    let out = doc
        .to_pptx()
        .expect("toPptx must not trap in the wasm build");
    assert_ooxml(&out, "ppt/presentation.xml", "pptx");
}

// ---------- The regression that started all this ----------

#[wasm_bindgen_test]
fn convert_to_pdfa_runs_in_wasm() {
    let doc = PdfDoc::open(SAMPLE_PDF).expect("open sample.pdf");

    // The bug was not a wrong result — it was a trap. Reaching this line at all
    // is most of what the test is for.
    let out = doc
        .convert_to_pdfa("pdfa2b")
        .expect("convertToPdfa must not trap in the WebAssembly build");

    assert!(!out.is_empty(), "PDF/A output must not be empty");
    assert!(
        out.starts_with(b"%PDF-"),
        "PDF/A output must still be a PDF"
    );
    // The converted file must be openable again; a trap-free call that emits
    // rubbish would otherwise pass.
    let reopened = PdfDoc::open(&out).expect("converted PDF/A must reopen");
    assert!(reopened.page_count() >= 1);
}

#[wasm_bindgen_test]
fn convert_to_pdfa_rejects_an_unknown_level() {
    let doc = PdfDoc::open(SAMPLE_PDF).expect("open sample.pdf");
    assert!(
        doc.convert_to_pdfa("not-a-level").is_err(),
        "an unknown conformance level must be an error, not a silent default"
    );
}

// ---------- Document assembly ----------

#[wasm_bindgen_test]
fn merge_combines_page_counts() {
    let a = PdfDoc::open(SAMPLE_PDF).expect("open sample.pdf");
    let b_pages = PdfDoc::open(MULTI_PDF)
        .expect("open multi-page.pdf")
        .page_count();
    let a_pages = a.page_count();

    let merged_bytes = a.merge(MULTI_PDF).expect("merge must work in wasm");
    let merged = PdfDoc::open(&merged_bytes).expect("merged output must reopen");

    assert_eq!(
        merged.page_count(),
        a_pages + b_pages,
        "merged page count must be the sum of its inputs"
    );
}

// ---------- Form data round-trip ----------

#[wasm_bindgen_test]
fn form_json_round_trip_survives_the_binding() {
    let fields = r#"[
        {"name": "Name", "value": "Alice"},
        {"name": "City", "value": "Amsterdam"}
    ]"#;
    let engine = XfaEngine::from_fields(fields).expect("from_fields");

    let exported = engine.export_json().expect("exportJson");
    assert!(
        exported.contains("Alice"),
        "exported JSON should carry the field values, got: {exported}"
    );

    let schema = engine.export_schema().expect("exportSchema");
    assert!(
        schema.contains("Name"),
        "exported schema should name the fields, got: {schema}"
    );

    // fromJson must accept what exportJson produced. A binding that can write a
    // shape it cannot read back is broken in a way no native test would notice,
    // because natively both sides use the same in-process types.
    let mut rebuilt = XfaEngine::from_json(&exported).expect("fromJson on our own exportJson");

    assert!(
        rebuilt.set_field_value("form1.City", "Rotterdam"),
        "setFieldValue should report success for a field that exists"
    );
    assert_eq!(
        rebuilt.get_field_value("form1.City"),
        Some("Rotterdam".to_string()),
        "setFieldValue must actually change the value"
    );
    assert!(
        !rebuilt.set_field_value("form1.DoesNotExist", "x"),
        "setFieldValue must report failure for a field that does not exist"
    );

    rebuilt.import_json(&exported).expect("importJson");
    assert_eq!(
        rebuilt.get_field_value("form1.City"),
        Some("Amsterdam".to_string()),
        "importJson must overwrite with the imported values"
    );
}
