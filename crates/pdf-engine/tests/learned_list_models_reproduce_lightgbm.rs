// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The generated list model equals LightGBM's own predictions (#599).
//!
//! `parity.json` beside each model's dump holds feature rows and the raw
//! margins LightGBM computed for them when the model was trained -- missing
//! values included, which is where a port goes wrong first: the first and last
//! lines of a block have no neighbour and reach the model as NaN. The generated
//! Rust must reproduce every margin within 1e-6 and the same side of the
//! decision threshold. That the Rust is the export of the checked-in dump, byte
//! for byte, is `scripts/ci/test_learned_export_is_reproducible.py`'s job.

const MODELS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../scripts/extraction/learned/models"
);

fn parity(name: &str) -> (Vec<String>, Vec<Vec<f64>>, Vec<f64>) {
    let path = format!("{MODELS}/{name}/parity.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let value: serde_json::Value = serde_json::from_str(&text).expect("parity.json is json");
    let names = value["features"]
        .as_array()
        .expect("features")
        .iter()
        .map(|v| v.as_str().expect("name").to_string())
        .collect();
    let rows = value["rows"]
        .as_array()
        .expect("rows")
        .iter()
        .map(|row| {
            row.as_array()
                .expect("row")
                .iter()
                .map(|v| v.as_f64().unwrap_or(f64::NAN))
                .collect()
        })
        .collect();
    let margins = value["margins"]
        .as_array()
        .expect("margins")
        .iter()
        .map(|v| v.as_f64().expect("margin"))
        .collect();
    (names, rows, margins)
}

fn check<const N: usize>(
    name: &str,
    features: &[&str; N],
    threshold: f64,
    predict: impl Fn(&[f64; N]) -> f64,
) {
    check_with(name, features, threshold, predict, true)
}

/// The same, for a model whose features CANNOT be missing.
///
/// `needs_nan` is not a convenience. Every model that can be handed a missing
/// value must prove its NaN routing here, because a feature that is absent at
/// serve time and never absent in a fixture is the untested path a model takes
/// on a real page. The separation models are the exception and it is a
/// property of their features rather than of their fixture: a gap is a
/// subtraction of two numbers that exist, every ratio's denominator is guarded
/// away from zero in `separation_features._scale`, and every count is a length.
/// There is no input that can be missing, so demanding one would mean writing
/// a fixture row that the engine cannot produce (#574).
fn check_with<const N: usize>(
    name: &str,
    features: &[&str; N],
    threshold: f64,
    predict: impl Fn(&[f64; N]) -> f64,
    needs_nan: bool,
) {
    let (names, rows, margins) = parity(name);
    assert_eq!(names, features.to_vec(), "{name}: the feature order moved");
    assert!(!rows.is_empty(), "{name}: no parity rows");
    let cut = (threshold / (1.0 - threshold)).ln();
    let mut with_nan = 0;
    let mut worst = 0f64;
    for (i, (row, want)) in rows.iter().zip(&margins).enumerate() {
        let x: [f64; N] = row.as_slice().try_into().expect("row width");
        with_nan += usize::from(x.iter().any(|v| v.is_nan()));
        let got = predict(&x);
        worst = worst.max((got - want).abs());
        assert!(
            (got - want).abs() <= 1e-6,
            "{name} row {i}: Rust {got} vs LightGBM {want}"
        );
        assert_eq!(
            got >= cut,
            *want >= cut,
            "{name} row {i}: the decision differs"
        );
    }
    // The largest difference, for the parity audit's table (#574).
    eprintln!(
        "{name}: {} rows, largest margin difference {worst:e}",
        rows.len()
    );
    if needs_nan {
        assert!(
            with_nan > 0,
            "{name}: no parity row carries a missing value, so the NaN routing is untested"
        );
    } else {
        assert_eq!(
            with_nan, 0,
            "{name} is declared unable to have a missing feature, and a parity \
             row has one: either the features changed or the declaration is wrong"
        );
    }
}

#[test]
fn the_block_model_reproduces_lightgbm() {
    use pdf_engine::learned::models::list_item;
    check(
        "list_item",
        &list_item::FEATURES,
        list_item::THRESHOLD,
        list_item::predict_margin,
    );
}

#[test]
fn the_line_model_reproduces_lightgbm() {
    use pdf_engine::learned::lines::LINE_FEATURES;
    use pdf_engine::learned::models::list_opens;
    assert_eq!(
        list_opens::FEATURES,
        LINE_FEATURES,
        "model and features disagree"
    );
    check(
        "list_opens",
        &list_opens::FEATURES,
        list_opens::THRESHOLD,
        list_opens::predict_margin,
    );
}

#[test]
fn the_table_region_model_reproduces_lightgbm() {
    use pdf_engine::learned::models::table_region;
    use pdf_engine::learned::region;
    assert_eq!(
        table_region::FEATURES.to_vec(),
        region::names(),
        "the model and the features it is fed disagree"
    );
    check(
        "table_region",
        &table_region::FEATURES,
        table_region::THRESHOLD,
        table_region::predict_margin,
    );
}

/// THE TWO BOUNDARY MODELS REPRODUCE LIGHTGBM (#574).
///
/// These replaced the pair model whose multiclass parity used to be held here.
/// That check existed because LightGBM writes a multiclass model as ONE flat
/// list of trees in which tree `i` scores class `i % num_class`, and an
/// exporter that sums the list returns a plausible float that is no class's
/// margin. These two are binary, so the trap is gone with the model -- but the
/// reason it was worth a paragraph is not: an exported model that is subtly
/// wrong returns confident numbers, and the grids it builds read as a weak
/// model rather than as a broken export.
#[test]
fn the_separation_models_reproduce_lightgbm() {
    use pdf_engine::learned::models::{
        table_column_boundary, table_column_bridge, table_row_boundary,
    };
    check_with(
        "table_row_boundary",
        &table_row_boundary::FEATURES,
        table_row_boundary::THRESHOLD,
        table_row_boundary::predict_margin,
        false,
    );
    check_with(
        "table_column_boundary",
        &table_column_boundary::FEATURES,
        table_column_boundary::THRESHOLD,
        table_column_boundary::predict_margin,
        false,
    );
    // The third decision of the decoder: which words are left out of the
    // stripes because they bridge two columns.
    check_with(
        "table_column_bridge",
        &table_column_bridge::FEATURES,
        table_column_bridge::THRESHOLD,
        table_column_bridge::predict_margin,
        false,
    );
}

/// THE TABLE JUDGE REPRODUCES LIGHTGBM (#574): the model that chooses which
/// proposals on a page are tables.
#[test]
fn the_table_judge_reproduces_lightgbm() {
    use pdf_engine::learned::models::table_judge;
    check_with(
        "table_judge",
        &table_judge::FEATURES,
        table_judge::THRESHOLD,
        table_judge::predict_margin,
        true,
    );
}

/// THE LINE CLASS MODEL REPRODUCES LIGHTGBM, one margin per class (#574):
/// four sets of trees in one file, and a tree added to the wrong class would
/// pass a check of any single number.
#[test]
fn the_line_class_model_reproduces_lightgbm() {
    use pdf_engine::learned::heading_lines::{heading_line_features, LINE_CLASSES};
    use pdf_engine::learned::models::line_classes;
    assert_eq!(
        line_classes::FEATURES.to_vec(),
        heading_line_features(),
        "the line model and the row the engine builds for it disagree: a model \
         reads its features by INDEX (#574)"
    );
    assert_eq!(line_classes::CLASSES, LINE_CLASSES.len());
    let path = format!("{MODELS}/line_classes/parity.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let value: serde_json::Value = serde_json::from_str(&text).expect("parity.json is json");
    let rows = value["rows"].as_array().expect("rows");
    let margins = value["margins"].as_array().expect("margins");
    assert!(!rows.is_empty());
    let mut with_nan = 0;
    let mut worst = 0f64;
    for (i, (row, want)) in rows.iter().zip(margins).enumerate() {
        let x: Vec<f64> = row
            .as_array()
            .expect("row")
            .iter()
            .map(|v| v.as_f64().unwrap_or(f64::NAN))
            .collect();
        with_nan += usize::from(x.iter().any(|v| v.is_nan()));
        let x: [f64; 105] = x.as_slice().try_into().expect("row width");
        let got = line_classes::predict_margins(&x);
        for (c, want) in want
            .as_array()
            .expect("a margin per class")
            .iter()
            .enumerate()
        {
            let want = want.as_f64().expect("margin");
            worst = worst.max((got[c] - want).abs());
            assert!(
                (got[c] - want).abs() <= 1e-6,
                "row {i} class {c}: Rust {} vs LightGBM {want}",
                got[c]
            );
        }
    }
    assert!(with_nan > 0, "no parity row carries a missing value");
    eprintln!(
        "line_classes: {} rows, largest margin difference {worst:e}",
        rows.len()
    );
    // AND THE THRESHOLD IN THE ENGINE IS THE MANIFEST'S.
    let manifest: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(format!("{MODELS}/line_classes/manifest.json")).expect("manifest"),
    )
    .expect("manifest is json");
    assert_eq!(
        manifest["heading_threshold"].as_f64(),
        Some(pdf_engine::learned::heading_lines::HEADING_THRESHOLD),
        "the engine's heading threshold is not the one chosen on validation"
    );
}
#[test]
fn the_heading_line_model_reproduces_lightgbm() {
    use pdf_engine::learned::heading_lines::heading_line_features;
    use pdf_engine::learned::models::heading_lines;
    assert_eq!(
        heading_lines::FEATURES.to_vec(),
        heading_line_features(),
        "the heading line model and the row the engine builds for it disagree: \
         a model reads its features by INDEX (#574)"
    );
    check(
        "heading_lines",
        &heading_lines::FEATURES,
        heading_lines::THRESHOLD,
        heading_lines::predict_margin,
    );
}

#[test]
fn the_caption_line_model_reproduces_lightgbm() {
    use pdf_engine::learned::captions::caption_line_features;
    use pdf_engine::learned::models::caption_lines;
    assert_eq!(
        caption_lines::FEATURES.to_vec(),
        caption_line_features(),
        "the caption line model and the row the engine builds for it disagree: \
         a model reads its features by INDEX (#636)"
    );
    check(
        "caption_lines",
        &caption_lines::FEATURES,
        caption_lines::THRESHOLD,
        caption_lines::predict_margin,
    );
}

/// THE LINE GRAPH NETWORK REPRODUCES PYTORCH (#638): five validation pages'
/// graphs, raw inputs in, and every class logit and link logit within 1e-3 of
/// what PyTorch computed -- the normalisation, the six edge directions, the
/// three rounds and both heads.
#[test]
fn the_line_graph_network_reproduces_pytorch() {
    graph_network_reproduces_pytorch("line_gnn", &pdf_engine::learned::line_gnn::V1);
}

/// And the retrained network (#574), the same way: its own parity pages, the
/// rows of its own input groups.
#[test]
fn the_retrained_line_graph_network_reproduces_pytorch() {
    graph_network_reproduces_pytorch("line_gnn_v2", &pdf_engine::learned::line_gnn::V2);
}

fn graph_network_reproduces_pytorch(name: &str, net: &pdf_engine::learned::line_gnn::Net) {
    use pdf_engine::learned::line_gnn::{forward, PageGraph};
    let path = format!("{MODELS}/{name}/parity.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let value: serde_json::Value = serde_json::from_str(&text).expect("json");
    let floats = |v: &serde_json::Value| -> Vec<f32> {
        v.as_array()
            .expect("array")
            .iter()
            .map(|x| x.as_f64().expect("number") as f32)
            .collect()
    };
    let mut lines = 0;
    let mut worst = 0f32;
    for page in value["pages"].as_array().expect("pages") {
        let graph = PageGraph {
            nodes: page["nodes"]
                .as_array()
                .expect("nodes")
                .iter()
                .map(floats)
                .collect(),
            neighbours: page["neighbours"]
                .as_array()
                .expect("neighbours")
                .iter()
                .map(|r| {
                    let v: Vec<i64> = r
                        .as_array()
                        .expect("row")
                        .iter()
                        .map(|x| x.as_i64().expect("index"))
                        .collect();
                    v.try_into().expect("six directions")
                })
                .collect(),
            edges: page["edges"]
                .as_array()
                .expect("edges")
                .iter()
                .map(floats)
                .collect(),
        };
        let (classes, link) = forward(&graph, net);
        let want_classes: Vec<Vec<f32>> = page["classes"]
            .as_array()
            .expect("classes")
            .iter()
            .map(floats)
            .collect();
        let want_link = floats(&page["link"]);
        for (i, (got, want)) in classes.iter().zip(&want_classes).enumerate() {
            worst = worst.max((link[i] - want_link[i]).abs());
            for (g, w) in got.iter().zip(want) {
                worst = worst.max((g - w).abs());
                assert!(
                    (g - w).abs() <= 1e-3,
                    "line {i}: class logit {g} vs PyTorch {w}"
                );
            }
            assert!(
                (link[i] - want_link[i]).abs() <= 1e-3,
                "line {i}: link {} vs {}",
                link[i],
                want_link[i]
            );
        }
        lines += classes.len();
    }
    eprintln!("line_gnn: {lines} lines, largest logit difference {worst:e}");
    assert!(
        lines > 50,
        "the fixture holds too few lines to test anything: {lines}"
    );
}
