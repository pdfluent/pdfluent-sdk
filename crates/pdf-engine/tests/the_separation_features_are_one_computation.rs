// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The gap features here are the gap features the models were trained on (#574).
//!
//! The two boundary models are trained in Python on what
//! `separation_features.py` computes and run in Rust on what
//! `separations.rs` computes. This holds the two together on fixtures written
//! by the Python the models were trained with, to 1e-9.
//!
//! WHY IT IS WORTH A FILE. A feature that differs in the sixth decimal is the
//! model reading input it never saw, and it does not announce itself: the
//! grids come out a little worse, which reads as a weak model and gets
//! answered with more trees rather than with a fix. This pair of tickets has
//! paid for that nine times over, most expensively when a harness scored a
//! decoder against a truth of empty cells -- TEDS 0.2013, which looked merely
//! disappointing and was in fact nothing at all.

use pdf_engine::learned::separations::{
    bands, bridge_features, bridging, cells_placed, column_features, place, row_features, Ruling,
    Word, BRIDGE_FLOOR,
};

fn close(a: f64, b: f64) -> bool {
    (a.is_nan() && b.is_nan()) || (a - b).abs() <= 1e-9
}

#[test]
fn every_separation_feature_matches_the_python_the_models_were_trained_with() {
    let raw = include_str!("fixtures/learned/separation_parity.json");
    let fixture: serde_json::Value = serde_json::from_str(raw).expect("fixture");
    let row_names = fixture["row_names"].as_array().expect("row names");
    let column_names = fixture["column_names"].as_array().expect("column names");
    let tables = fixture["tables"].as_array().expect("tables");
    assert!(!tables.is_empty(), "the fixture holds no tables");

    let mut checked = 0usize;
    for table in tables {
        let words: Vec<Word> = table["words"]
            .as_array()
            .expect("words")
            .iter()
            .map(|w| Word {
                bbox: {
                    let b = w["bbox"].as_array().expect("bbox");
                    [
                        b[0].as_f64().unwrap(),
                        b[1].as_f64().unwrap(),
                        b[2].as_f64().unwrap(),
                        b[3].as_f64().unwrap(),
                    ]
                },
                // DERIVED HERE FROM THE TEXT, not read from the fixture.
                // Reading the flag made this test blind to the one thing it
                // most needed to hold: the two sides disagreed on what a digit
                // is -- Python's `str.isdigit()` against Rust's
                // `is_ascii_digit` -- and every value in the fixture still
                // matched (codex, #574). The fixture now carries the word and
                // the assertion below holds the definition.
                numeric: w["text"]
                    .as_str()
                    .expect("the fixture carries the word's text")
                    .chars()
                    .any(|c| c.is_ascii_digit()),
            })
            .collect();
        let rules: Vec<Ruling> = table["rules"]
            .as_array()
            .expect("rules")
            .iter()
            .map(|r| Ruling {
                y: r["y"].as_f64().unwrap(),
            })
            .collect();
        let text_height = table["text_height"].as_f64().expect("text height");
        let word_width = table["word_width"].as_f64().expect("word width");
        // AND THE DEFINITION ITSELF IS HELD, so a change on either side fails
        // here rather than drifting into the model's input.
        for (w, said) in table["words"].as_array().expect("words").iter().zip(&words) {
            assert_eq!(
                said.numeric,
                w["numeric"].as_bool().unwrap_or(false),
                "what counts as a digit differs between Rust and the Python the \
                 models were trained with, on {:?}",
                w["text"].as_str().unwrap_or("?")
            );
        }
        let boxes: Vec<[f64; 4]> = words.iter().map(|w| w.bbox).collect();

        // THE BANDS ARE REBUILT HERE rather than read from the fixture, so
        // that the grouping is checked too: Python's bands and ours have to
        // be the same bands before their features can be the same numbers.
        let mut down: Vec<usize> = (0..words.len()).collect();
        down.sort_by(|&a, &b| boxes[b][3].partial_cmp(&boxes[a][3]).unwrap());
        let lines = bands(down, &boxes, 1, 3);
        let mut across: Vec<usize> = (0..words.len()).collect();
        across.sort_by(|&a, &b| boxes[a][0].partial_cmp(&boxes[b][0]).unwrap());
        let stripes = bands(across, &boxes, 0, 2);

        let rows = table["rows"].as_array().expect("rows");
        assert_eq!(
            lines.len().saturating_sub(1),
            rows.len(),
            "Python found {} row gaps and we found {}: the bands differ before \
             the features do",
            rows.len(),
            lines.len().saturating_sub(1)
        );
        let columns = table["columns"].as_array().expect("columns");
        assert_eq!(
            stripes.len().saturating_sub(1),
            columns.len(),
            "Python found {} column gaps and we found {}",
            columns.len(),
            stripes.len().saturating_sub(1)
        );

        let row_gaps: Vec<f64> = lines
            .windows(2)
            .map(|p| {
                p[0].iter()
                    .map(|&i| boxes[i][1])
                    .fold(f64::INFINITY, f64::min)
                    - p[1]
                        .iter()
                        .map(|&i| boxes[i][3])
                        .fold(f64::NEG_INFINITY, f64::max)
            })
            .collect();
        let (row_median, row_tight) = pdf_engine::learned::separations::scale(&row_gaps);
        for (n, want) in rows.iter().enumerate() {
            let got = row_features(
                &lines[n],
                &lines[n + 1],
                &words,
                row_gaps[n],
                &row_gaps,
                row_median,
                row_tight,
                text_height,
                &rules,
            );
            let theirs = want["x"].as_array().expect("x");
            for (k, name) in row_names.iter().enumerate() {
                let mine = got[k];
                let python = theirs[k].as_f64().unwrap_or(f64::NAN);
                assert!(
                    close(mine, python),
                    "row gap {n}, {}: Rust {mine} against Python {python}",
                    name.as_str().unwrap_or("?")
                );
                checked += 1;
            }
        }

        let col_gaps: Vec<f64> = stripes
            .windows(2)
            .map(|p| {
                p[1].iter()
                    .map(|&i| boxes[i][0])
                    .fold(f64::INFINITY, f64::min)
                    - p[0]
                        .iter()
                        .map(|&i| boxes[i][2])
                        .fold(f64::NEG_INFINITY, f64::max)
            })
            .collect();
        let (col_median, col_tight) = pdf_engine::learned::separations::scale(&col_gaps);
        for (n, want) in columns.iter().enumerate() {
            let got = column_features(
                &stripes[n],
                &stripes[n + 1],
                &words,
                col_gaps[n],
                &col_gaps,
                col_median,
                col_tight,
                word_width,
            );
            let theirs = want["x"].as_array().expect("x");
            for (k, name) in column_names.iter().enumerate() {
                let mine = got[k];
                let python = theirs[k].as_f64().unwrap_or(f64::NAN);
                assert!(
                    close(mine, python),
                    "column gap {n}, {}: Rust {mine} against Python {python}",
                    name.as_str().unwrap_or("?")
                );
                checked += 1;
            }
        }

        // THE BRIDGE FEATURES, one vector per word in the words' own order.
        let bridge_names = fixture["bridge_names"].as_array().expect("bridge names");
        let theirs = table["bridges"].as_array().expect("bridges");
        let ours = bridge_features(&words, word_width);
        assert_eq!(theirs.len(), ours.len(), "one bridge vector per word");
        for (n, (want, got)) in theirs.iter().zip(&ours).enumerate() {
            let want = want.as_array().expect("x");
            for (k, name) in bridge_names.iter().enumerate() {
                let python = want[k].as_f64().unwrap_or(f64::NAN);
                assert!(
                    close(got[k], python),
                    "word {n}, {}: Rust {} against Python {python}",
                    name.as_str().unwrap_or("?"),
                    got[k]
                );
                checked += 1;
            }
        }
    }
    assert!(
        checked > 2000,
        "only {checked} values were compared, which is too few to hold a port"
    );
}

fn words_of(table: &serde_json::Value) -> Vec<Word> {
    table["words"]
        .as_array()
        .expect("words")
        .iter()
        .map(|w| {
            let b = w["bbox"].as_array().expect("bbox");
            Word {
                bbox: [
                    b[0].as_f64().unwrap(),
                    b[1].as_f64().unwrap(),
                    b[2].as_f64().unwrap(),
                    b[3].as_f64().unwrap(),
                ],
                numeric: w["text"]
                    .as_str()
                    .expect("text")
                    .chars()
                    .any(|c| c.is_ascii_digit()),
            }
        })
        .collect()
}

fn indexes(value: &serde_json::Value) -> Vec<usize> {
    value
        .as_array()
        .expect("list")
        .iter()
        .map(|v| v.as_u64().expect("index") as usize)
        .collect()
}

/// A WHOLE TABLE IS PLACED AS PYTHON PLACES IT (#574).
///
/// The features above can all agree and the grid still differ: in which
/// words are left out of the stripes, in which column a bridge lands, in
/// where a span stops. This holds the three exported models, the features
/// and the decoder together on real held-out PubTables tables that each hold
/// at least one bridging word -- if any of them drifts, some word in some
/// table moves, and this says which.
#[test]
fn every_word_is_placed_where_the_python_decoder_places_it() {
    let raw = include_str!("fixtures/learned/separation_parity.json");
    let fixture: serde_json::Value = serde_json::from_str(raw).expect("fixture");
    let floor = fixture["separation_floor"]
        .as_f64()
        .expect("separation floor");
    let bridge_floor = fixture["bridge_floor"].as_f64().expect("bridge floor");
    assert_eq!(
        bridge_floor, BRIDGE_FLOOR,
        "the fixture was written at another bridge floor than the one that ships"
    );
    let decodes = fixture["decodes"].as_array().expect("decodes");
    assert!(
        decodes.len() >= 4,
        "too few whole decodes to hold the decoder"
    );
    let mut spans = 0usize;
    for (t, table) in decodes.iter().enumerate() {
        let words = words_of(table);
        let want: Vec<bool> = table["bridge"]
            .as_array()
            .expect("bridge")
            .iter()
            .map(|b| b.as_bool().expect("bool"))
            .collect();
        let bridge = bridging(&words, BRIDGE_FLOOR);
        assert_eq!(bridge, want, "table {t}: a different set of words bridges");
        let placed = place(&words, &[], floor, &bridge).expect("placed");
        assert_eq!(placed.row_of, indexes(&table["row_of"]), "table {t}: rows");
        assert_eq!(
            placed.column_of,
            indexes(&table["column_of"]),
            "table {t}: columns"
        );
        let last = indexes(&table["cell_last_column"]);
        let bottom = indexes(&table["cell_last_row"]);
        let grid = cells_placed(&words, &placed).expect("cells");
        assert_eq!(
            grid.header_rows,
            table["header_rows"].as_u64().expect("header rows") as usize,
            "table {t}: header rows"
        );
        for cell in &grid.cells {
            let reach = *cell.columns.last().expect("a column");
            let low = *cell.rows.last().expect("a row");
            spans += usize::from(cell.columns.len() > 1);
            for &w in &cell.words {
                assert_eq!(
                    reach, last[w],
                    "table {t}: word {w}'s cell ends in column {reach}, Python's in {}",
                    last[w]
                );
                assert_eq!(
                    low, bottom[w],
                    "table {t}: word {w}'s cell ends in row {low}, Python's in {}",
                    bottom[w]
                );
            }
        }
    }
    assert!(spans > 0, "no decode in the fixture spans a column");
}
