// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The wired final-table model equals LightGBM's own probabilities (#638).
//!
//! `parity.json` beside the model's dump holds feature rows and the raw
//! margins LightGBM computed for them when the model was trained -- a missing
//! judge probability included, which is where a port goes wrong first: a
//! table from no judged region reaches the model as NaN. Rust's
//! `table_is_v2_probability` must equal the sigmoid of every margin within
//! 1e-9. That the Rust is the export of the checked-in dump, byte for byte,
//! is `scripts/ci/test_learned_export_is_reproducible.py`'s job.

use pdf_engine::learned::tables::{
    table_is_v2_probability, TABLE_IS_V2_FEATURES, TABLE_IS_V2_NAMES,
};

fn sigmoid(margin: f64) -> f64 {
    1.0 / (1.0 + (-margin).exp())
}

#[test]
fn the_final_table_model_reproduces_lightgbm() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../scripts/extraction/learned/models/table_is_v2/parity.json"
    ))
    .expect("parity.json");
    let parity: serde_json::Value = serde_json::from_str(&text).expect("json");
    let names: Vec<&str> = parity["features"]
        .as_array()
        .expect("features")
        .iter()
        .map(|v| v.as_str().expect("name"))
        .collect();
    assert_eq!(
        names,
        TABLE_IS_V2_NAMES.to_vec(),
        "the feature order moved: a model reads its features by INDEX"
    );
    let rows = parity["rows"].as_array().expect("rows");
    let margins = parity["margins"].as_array().expect("margins");
    assert_eq!(rows.len(), margins.len(), "a margin per row");
    assert!(!rows.is_empty(), "no parity rows");
    let mut with_nan = 0usize;
    let mut worst = 0f64;
    for (i, (row, want)) in rows.iter().zip(margins).enumerate() {
        let x: Vec<f64> = row
            .as_array()
            .expect("row")
            .iter()
            .map(|v| v.as_f64().unwrap_or(f64::NAN))
            .collect();
        let x: [f64; TABLE_IS_V2_FEATURES] = x.as_slice().try_into().expect("row width");
        with_nan += usize::from(x.iter().any(|v| v.is_nan()));
        let got = table_is_v2_probability(&x);
        let want = sigmoid(want.as_f64().expect("margin"));
        worst = worst.max((got - want).abs());
        assert!(
            (got - want).abs() <= 1e-9,
            "row {i}: Rust {got} vs LightGBM {want}"
        );
    }
    eprintln!(
        "table_is_v2: {} rows, largest probability difference {worst:e}",
        rows.len()
    );
    assert!(
        with_nan > 0,
        "no parity row carries a missing value, so the NaN routing is untested"
    );
}
