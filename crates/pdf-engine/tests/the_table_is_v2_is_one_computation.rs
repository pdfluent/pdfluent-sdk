// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The final-table features here are the final-table features Python computes
//! (#574, #638).
//!
//! The fixture (`make_table_is_v2_parity.py`) holds synthetic tables only --
//! no rig pages -- with their cells, heading probabilities, judge probability,
//! proposer origin and rulings flag, and what Python makes of them: all 29
//! features in `TABLE_IS_V2_NAMES` order. Here every one is recomputed from
//! the cells alone. A feature that drifts between the two turns this red, and
//! Python recomputes nowhere else: the trainer reads the dump's vectors.

use pdf_engine::learned::tables::{
    table_is_v2_features, CellHint, TABLE_IS_V2_FEATURES, TABLE_IS_V2_NAMES,
};
use pdf_engine::{Block, BlockKind, TextSpan};

fn number(v: &serde_json::Value) -> f64 {
    v.as_f64().unwrap_or(f64::NAN)
}

fn close(a: f64, b: f64) -> bool {
    (a.is_nan() && b.is_nan()) || (a - b).abs() <= 1e-9 * b.abs().max(1.0)
}

#[test]
fn the_final_table_features_read_as_the_python_does() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/learned/table_is_v2_parity.json"
    ))
    .expect("fixture");
    let fixture: serde_json::Value = serde_json::from_str(&text).expect("json");
    let names = fixture["names"].as_array().expect("names");
    assert_eq!(names.len(), TABLE_IS_V2_FEATURES, "the feature count moved");
    for (i, name) in names.iter().enumerate() {
        assert_eq!(
            name.as_str().expect("name"),
            TABLE_IS_V2_NAMES[i],
            "feature {i} is named differently"
        );
    }
    let tables = fixture["tables"].as_array().expect("tables");
    assert!(tables.len() >= 8, "too few tables to hold the features");
    let mut compared = 0usize;
    for (n, table) in tables.iter().enumerate() {
        let cells = table["cells"].as_array().expect("cells");
        let blocks: Vec<Block> = cells
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let b = c["bbox"].as_array().expect("bbox");
                let spans: Vec<TextSpan> = c["spans"]
                    .as_array()
                    .map(|s| {
                        s.iter()
                            .map(|q| {
                                let q = q.as_array().expect("span");
                                TextSpan {
                                    x: number(&q[0]),
                                    y: number(&q[1]),
                                    width: number(&q[2]),
                                    height: number(&q[3]),
                                    ..Default::default()
                                }
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let mut block = Block::new(
                    BlockKind::TableCell {
                        row: c["row"].as_u64().expect("row") as u16,
                        column: c["column"].as_u64().expect("column") as u16,
                        row_span: c["row_span"].as_u64().unwrap_or(1) as u16,
                        column_span: c["column_span"].as_u64().unwrap_or(1) as u16,
                        header: false,
                    },
                    0,
                    [number(&b[0]), number(&b[1]), number(&b[2]), number(&b[3])],
                    c["text"].as_str().unwrap_or_default().to_string(),
                    i,
                );
                block.spans = spans;
                block
            })
            .collect();
        let hints: Vec<CellHint> = cells
            .iter()
            .map(|c| CellHint {
                heading: false,
                p_heading: c["p"].as_f64(),
            })
            .collect();
        let x = table_is_v2_features(
            &blocks,
            &hints,
            number(&table["height"]),
            number(&table["judge_p"]),
            table["from_builder"].as_bool().expect("from_builder"),
            table["has_rulings"].as_bool().expect("has_rulings"),
        );
        let want = table["features"].as_array().expect("x");
        assert_eq!(want.len(), TABLE_IS_V2_FEATURES, "table {n}: short vector");
        for (f, v) in x.iter().enumerate() {
            let python = number(&want[f]);
            assert!(
                close(*v, python),
                "table {n}: feature {f} ({}) is {v} here and {python} in Python",
                TABLE_IS_V2_NAMES[f]
            );
            compared += 1;
        }
    }
    assert!(
        compared >= 8 * TABLE_IS_V2_FEATURES,
        "only {compared} compared"
    );
}
