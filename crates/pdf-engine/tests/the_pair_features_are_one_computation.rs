// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The pair features here are the pair features the model was trained on (#600).
//!
//! The table model is trained in Python on what `feature_dump` writes and run
//! in Rust on what the engine computes. `features.rs` states the rule those
//! two obey -- one computation, in the engine -- and this holds the port that
//! moved it there: every value, on fixtures generated from the Python the
//! model was trained with, to 1e-9.
//!
//! WHY THAT MATTERS MORE THAN IT LOOKS. A feature that differs in the sixth
//! decimal is not a small error. It is the model reading input it was never
//! trained on, and it does not announce itself: the grids come out slightly
//! worse and it reads as a weak model, so it gets answered with more data or a
//! bigger tree instead of with a fix.

use pdf_engine::learned::pairs::{candidates, features, PairWord, NAMES};

fn word(value: &serde_json::Value) -> PairWord {
    let box_of = value["bbox"].as_array().expect("bbox");
    PairWord {
        text: value["text"].as_str().expect("text").to_string(),
        bbox: [
            box_of[0].as_f64().unwrap(),
            box_of[1].as_f64().unwrap(),
            box_of[2].as_f64().unwrap(),
            box_of[3].as_f64().unwrap(),
        ],
        table: value
            .get("table")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32),
        row: value.get("row").and_then(|v| v.as_u64()).map(|v| v as u16),
        column: value
            .get("column")
            .and_then(|v| v.as_u64())
            .map(|v| v as u16),
    }
}

#[test]
fn every_feature_matches_the_python_the_model_was_trained_with() {
    let raw = include_str!("fixtures/learned/pair_features_parity.json");
    let fixtures: serde_json::Value = serde_json::from_str(raw).expect("fixtures");
    let names = fixtures["names"].as_array().expect("names");
    assert_eq!(
        names.len(),
        NAMES.len(),
        "the two sides do not even agree on how many features there are"
    );
    for (index, name) in names.iter().enumerate() {
        assert_eq!(
            name.as_str().unwrap(),
            NAMES[index],
            "feature {index} is named differently on the two sides, so a \
             value that matches would be matching the wrong thing"
        );
    }

    let cases = fixtures["cases"].as_array().expect("cases");
    assert!(
        cases.len() >= 60,
        "too few cases to catch a per-branch slip"
    );
    for (n, case) in cases.iter().enumerate() {
        let a = word(&case["a"]);
        let b = word(&case["b"]);
        let got = features(&a, &b);
        let want = case["want"].as_array().expect("want");
        for (index, expected) in want.iter().enumerate() {
            let expected = expected.as_f64().expect("number");
            let difference = (got[index] - expected).abs();
            assert!(
                difference < 1e-9,
                "case {n}, feature `{}`: Rust {} against Python {} \
                 (difference {difference:e}) -- {:?} / {:?}",
                NAMES[index],
                got[index],
                expected,
                a.text,
                b.text
            );
        }
    }
}

#[test]
fn a_pair_is_described_one_way_round() {
    let left = PairWord {
        text: "Region".into(),
        bbox: [10.0, 90.0, 60.0, 100.0],
        table: None,
        row: None,
        column: None,
    };
    let right = PairWord {
        text: "12,345".into(),
        bbox: [70.0, 90.0, 130.0, 100.0],
        table: None,
        row: None,
        column: None,
    };
    assert_eq!(
        features(&left, &right),
        features(&right, &left),
        "the pair has one description and not two mirror images; the model \
         should be handed the symmetry rather than learn it from examples"
    );
}

#[test]
fn the_grid_finds_the_neighbours_a_double_loop_would() {
    // The grid is an optimisation, so it has to answer what the plain search
    // answers. A page of three lines of four words, and every pair the naive
    // rules find must be found here too.
    let mut words = Vec::new();
    for line in 0..3 {
        for column in 0..4 {
            let x = 10.0 + column as f64 * 40.0;
            let y = 100.0 - line as f64 * 20.0;
            words.push(PairWord {
                text: format!("w{line}{column}"),
                bbox: [x, y, x + 30.0, y + 10.0],
                table: None,
                row: None,
                column: None,
            });
        }
    }
    let found = candidates(&words, 6);
    assert!(
        !found.is_empty(),
        "a page of twelve words found no neighbours"
    );
    for (i, j) in &found {
        assert!(i < j, "a pair must be ordered, got ({i}, {j})");
    }
    // the word to the right on the same line, and the one below, are in
    for line in 0..3 {
        for column in 0..3 {
            let here = line * 4 + column;
            assert!(
                found.contains(&(here, here + 1)),
                "the next word along line {line} is not a candidate"
            );
        }
    }
    for column in 0..4 {
        assert!(
            found.contains(&(column, column + 4)),
            "the word below column {column} is not a candidate"
        );
    }
}

/// WHICH PAIRS, not just what they hold. The feature parity above says the two
/// implementations agree about a pair; it says nothing about them agreeing on
/// WHICH pairs to ask about, and that is where the damage was: the grid search
/// used a fixed window of two cells, missed 159 of the 544 pairs Python finds
/// on a real region, and the ones it missed were the long edges that join one
/// side of a table to the other. The decoder closes rows and columns
/// transitively, so those are the edges that hold a table together -- TEDS
/// 0.0876 against 0.7377 with the same model, the same words and the same
/// floor (#600). A function with a parity test beside a function without one.
#[test]
fn the_pairs_we_ask_about_are_the_pairs_python_asks_about() {
    use pdf_engine::learned::pairs::{candidates, PairWord};

    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/learned/candidates_parity.json"
    ))
    .expect("the fixtures are checked in");
    let regions: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    let regions = regions.as_array().expect("a list of regions");
    assert!(regions.len() >= 4, "enough regions to be worth the name");

    let mut total = 0usize;
    for region in regions {
        let words: Vec<PairWord> = region["words"]
            .as_array()
            .expect("words")
            .iter()
            .map(|w| {
                let b = w["bbox"].as_array().expect("bbox");
                PairWord {
                    text: w["text"].as_str().expect("text").to_string(),
                    bbox: [
                        b[0].as_f64().expect("x0"),
                        b[1].as_f64().expect("y0"),
                        b[2].as_f64().expect("x1"),
                        b[3].as_f64().expect("y1"),
                    ],
                    table: None,
                    row: None,
                    column: None,
                }
            })
            .collect();
        let nearest = region["nearest"].as_u64().expect("nearest") as usize;
        let theirs: Vec<(usize, usize)> = region["pairs"]
            .as_array()
            .expect("pairs")
            .iter()
            .map(|p| {
                let p = p.as_array().expect("a pair");
                (
                    p[0].as_u64().expect("i") as usize,
                    p[1].as_u64().expect("j") as usize,
                )
            })
            .collect();
        let ours = candidates(&words, nearest);
        total += theirs.len();
        if ours != theirs {
            let mine: std::collections::HashSet<_> = ours.iter().copied().collect();
            let yours: std::collections::HashSet<_> = theirs.iter().copied().collect();
            let say = |set: &std::collections::HashSet<(usize, usize)>| {
                let mut v: Vec<String> = set
                    .iter()
                    .take(8)
                    .map(|(i, j)| format!("{:?}/{:?}", words[*i].text, words[*j].text))
                    .collect();
                v.sort();
                v.join(", ")
            };
            panic!(
                "page {} of {}: ours {} theirs {}\n  ours only: {}\n  theirs only: {}",
                region["page"],
                region["page_id"],
                ours.len(),
                theirs.len(),
                say(&mine.difference(&yours).copied().collect()),
                say(&yours.difference(&mine).copied().collect()),
            );
        }
    }
    assert!(total > 2000, "the fixtures are not a toy: {total} pairs");
}
