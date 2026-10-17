// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The region features the engine computes are the ones the model was trained
//! on (#600).
//!
//! The same rule and the same reason as `the_pair_features_are_one_computation`:
//! the model is trained on what Python writes and run on what the engine
//! computes, and two implementations of one formula drift. A drift here does
//! not crash; it reads as a weak model, which is the worst kind of bug,
//! because you do not notice it and you explain it wrongly.
//!
//! The fixtures are 359 blocks of six real pages, with the 35 values
//! `scripts/extraction/learned/table_region.py` computed for each. Rust must
//! reproduce every one.

use pdf_engine::learned::region;
use pdf_engine::{Block, BlockKind};

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/learned/region_features_parity.json"
);

fn kind_of(name: &str) -> BlockKind {
    match name {
        "table_cell" => BlockKind::TableCell {
            row: 0,
            column: 0,
            row_span: 1,
            column_span: 1,
            header: false,
        },
        "heading" => BlockKind::Heading { level: 1 },
        "list_item" => BlockKind::ListItem {
            level: 0,
            ordinal: None,
        },
        "footer" => BlockKind::Footer,
        "header" => BlockKind::Header,
        _ => BlockKind::Paragraph,
    }
}

#[test]
fn every_region_feature_matches_the_python_the_model_was_trained_with() {
    let text = std::fs::read_to_string(FIXTURE).expect("the fixtures are checked in");
    let fixture: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    let names: Vec<String> = fixture["names"]
        .as_array()
        .expect("names")
        .iter()
        .map(|v| v.as_str().expect("name").to_string())
        .collect();
    assert_eq!(names, region::names(), "the feature order moved");

    let pages = fixture["pages"].as_array().expect("pages");
    assert!(pages.len() >= 4, "enough pages to be worth the name");
    let mut checked = 0usize;
    let mut leaders = 0usize;
    for page in pages {
        let rows = page["blocks"].as_array().expect("blocks");
        let blocks: Vec<Block> = rows
            .iter()
            .map(|b| {
                let bb = b["bbox"].as_array().expect("bbox");
                Block::new(
                    kind_of(b["kind"].as_str().expect("kind")),
                    0,
                    [
                        bb[0].as_f64().expect("x0"),
                        bb[1].as_f64().expect("y0"),
                        bb[2].as_f64().expect("x1"),
                        bb[3].as_f64().expect("y1"),
                    ],
                    b["text"].as_str().unwrap_or("").to_string(),
                    0,
                )
            })
            .collect();
        let refs: Vec<&Block> = blocks.iter().collect();
        let kinds: Vec<&str> = rows
            .iter()
            .map(|b| b["kind"].as_str().expect("kind"))
            .collect();

        for (i, b) in rows.iter().enumerate() {
            let head: [f64; 24] = b["head"]
                .as_array()
                .expect("head")
                .iter()
                .map(|v| v.as_f64().unwrap_or(f64::NAN))
                .collect::<Vec<f64>>()
                .as_slice()
                .try_into()
                .expect("24 block features");
            let gaps = (
                b["gap_above_em"].as_f64().unwrap_or(f64::NAN),
                b["gap_below_em"].as_f64().unwrap_or(f64::NAN),
            );
            let size = b["size"].as_f64().unwrap_or(0.0);
            let ours = region::vector_of(head, &kinds, size, gaps, &refs, i);
            let theirs: Vec<f64> = page["vectors"].as_array().expect("vectors")[i]
                .as_array()
                .expect("vector")
                .iter()
                .map(|v| v.as_f64().unwrap_or(f64::NAN))
                .collect();
            assert_eq!(theirs.len(), 35);
            if ours[region::names()
                .iter()
                .position(|n| *n == "leader_run")
                .unwrap()]
                == 1.0
            {
                leaders += 1;
            }
            for (k, want) in theirs.iter().enumerate() {
                let got = ours[k];
                let same = (got.is_nan() && want.is_nan()) || (got - want).abs() <= 1e-9;
                assert!(
                    same,
                    "page {} block {i}, {}: Rust {got} vs Python {want}",
                    page["page"], names[k]
                );
            }
            checked += 1;
        }
    }
    assert!(checked > 200, "only {checked} blocks checked");
    // WITHOUT THIS THE LEADER SET IS UNTESTED. Every other feature is
    // arithmetic that any page exercises; `leader_run` fires only where a page
    // actually draws a run of dots, and a fixture set without one would pass
    // with `is_leader` returning false for everything.
    assert!(
        leaders > 0,
        "no fixture block carries a leader run, so that feature is untested"
    );
}
