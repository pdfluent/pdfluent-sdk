//! The exported models equal LightGBM's own predictions (#598).
//!
//! `parity.json` holds feature rows and the raw margins LightGBM computed for
//! them when the model was trained. The generated Rust must reproduce each
//! margin within 1e-6 and the same class. A changed tree, threshold, leaf or
//! missing-value rule in the generated file fails here by row; a `model.json`
//! regenerated without re-exporting fails the hash check.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_learned::models::toy_heading;
use sha2::{Digest, Sha256};

const MODEL_DIR: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../scripts/extraction/learned/models/toy_heading"
);

fn read(name: &str) -> Vec<u8> {
    std::fs::read(format!("{MODEL_DIR}/{name}")).unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn the_generated_code_is_of_the_checked_in_dump() {
    let digest: String = Sha256::digest(read("model.json"))
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(
        toy_heading::MODEL_SHA256,
        digest,
        "model.json changed without export_rust.py being run again"
    );
}

#[test]
fn the_generated_code_reproduces_lightgbm() {
    let parity: serde_json::Value = serde_json::from_slice(&read("parity.json")).expect("json");
    let names: Vec<&str> = parity["features"]
        .as_array()
        .expect("features")
        .iter()
        .map(|v| v.as_str().expect("name"))
        .collect();
    assert_eq!(names, toy_heading::FEATURES, "the feature order moved");
    let rows = parity["rows"].as_array().expect("rows");
    let margins = parity["margins"].as_array().expect("margins");
    assert_eq!(rows.len(), margins.len());
    assert!(!rows.is_empty());
    for (i, (row, want)) in rows.iter().zip(margins).enumerate() {
        let mut x = [0.0f64; 24];
        for (slot, v) in x.iter_mut().zip(row.as_array().expect("row")) {
            *slot = v.as_f64().unwrap_or(f64::NAN);
        }
        let got = toy_heading::predict_margin(&x);
        let want = want.as_f64().expect("margin");
        assert!(
            (got - want).abs() <= 1e-6,
            "row {i}: Rust {got} vs LightGBM {want}"
        );
        assert_eq!(got > 0.0, want > 0.0, "row {i}: the class differs");
    }
}
