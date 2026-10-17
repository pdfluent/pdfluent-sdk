// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Layer interface rows through the facade (#643).
//!
//! Groups added with an order entry (`add_ocg`) read back as checkbox
//! rows; groups without one (`add_layer`) have no row, as the recording
//! pins. `set_layer_ui_config` shows by default and toggles on action 1.

use pdfluent::compat::Value;
use pdfluent::prelude::*;

const SAMPLE_PDF: &[u8] = include_bytes!("../../../fixtures/sample.pdf");

fn open_sample() -> PdfDocument {
    PdfDocument::from_bytes(SAMPLE_PDF).expect("sample opens")
}

fn field<'a>(row: &'a Value, key: &str) -> &'a Value {
    match row {
        Value::Dict(map) => map.get(key).unwrap_or(&Value::Null),
        _ => &Value::Null,
    }
}

fn row_shape(row: &Value) -> (i64, i64, i64, i64, &str, &str) {
    let int = |key: &str| match field(row, key) {
        Value::Int(n) => *n,
        other => panic!("{key} is not an int: {other:?}"),
    };
    let text = |key: &str| match field(row, key) {
        Value::Str(s) => s.as_str(),
        other => panic!("{key} is not text: {other:?}"),
    };
    (
        int("depth"),
        int("locked"),
        int("number"),
        int("on"),
        text("text"),
        text("type"),
    )
}

#[test]
fn plain_and_layer_only_docs_have_no_rows() {
    let doc = open_sample();
    assert!(doc.layer_ui_configs().expect("read rows").is_empty());

    let mut doc = open_sample();
    doc.add_layer("L1", None, None).expect("add layer");
    assert!(doc.layer_ui_configs().expect("read rows").is_empty());
}

#[test]
fn ocg_reads_back_as_a_checkbox_row() {
    let mut doc = open_sample();
    doc.add_ocg("hide", -1, true, None, None).expect("add ocg");
    let rows = doc.layer_ui_configs().expect("read rows");
    assert_eq!(rows.len(), 1);
    assert_eq!(row_shape(&rows[0]), (0, 0, 0, 1, "hide", "checkbox"));
}

#[test]
fn ui_config_shows_by_default_and_toggles() {
    let mut doc = open_sample();
    doc.add_ocg("hide", -1, true, None, None).expect("add ocg");

    doc.set_layer_ui_config(0, 0).expect("default shows");
    let rows = doc.layer_ui_configs().expect("read rows");
    assert_eq!(row_shape(&rows[0]).3, 1);

    doc.set_layer_ui_config(0, 1).expect("action toggles");
    let rows = doc.layer_ui_configs().expect("read rows");
    assert_eq!(row_shape(&rows[0]).3, 0);
}

#[test]
fn unknown_interface_number_is_a_bad_value() {
    let mut doc = open_sample();
    doc.add_ocg("hide", -1, true, None, None).expect("add ocg");
    let err = doc
        .set_layer_ui_config(99, 0)
        .expect_err("missing interface fails");
    assert!(err.to_string().contains("no layer interface 99"), "{err}");
}
