//! The table verifier computes what `table_verifier.py` computes (#638).
//!
//! The fixture (`make_verifier_parity.py`) holds real pages -- PubTables-train
//! and DocLayNet-train, none a rig document -- with the judge's proposals and
//! probabilities, and what Python made of them: the candidates, each one's 18
//! features, the ranker's score, and the tables chosen. Here every one of
//! those is recomputed from the lines, words, proposals and probabilities
//! alone. A feature that drifts between the two, a ranker exported wrong, or a
//! choice that breaks a tie the other way turns this red.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_engine::learned::features::PageWord;
use pdf_engine::learned::judge::{Line, Proposal, KINDS};
use pdf_engine::learned::verifier;

fn number(v: &serde_json::Value) -> f64 {
    v.as_f64().unwrap_or(f64::NAN)
}

fn bbox(v: &serde_json::Value) -> [f64; 4] {
    let b = v.as_array().expect("bbox");
    [number(&b[0]), number(&b[1]), number(&b[2]), number(&b[3])]
}

fn indexes(v: &serde_json::Value) -> Vec<usize> {
    v.as_array()
        .expect("list")
        .iter()
        .map(|i| i.as_u64().expect("index") as usize)
        .collect()
}

#[test]
fn the_verifier_ranks_and_chooses_as_the_python_does() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/learned/verifier_parity.json"
    ))
    .expect("fixture");
    let fixture: serde_json::Value = serde_json::from_str(&text).expect("json");
    assert_eq!(
        fixture["names"].as_array().expect("names").len(),
        verifier::FEATURES,
        "the feature count moved"
    );
    let floor = number(&fixture["floor"]);
    let pages = fixture["pages"].as_array().expect("pages");
    assert!(pages.len() >= 8, "too few pages to hold the verifier");
    let mut compared = 0usize;
    for (n, page) in pages.iter().enumerate() {
        let size = (number(&page["size"][0]), number(&page["size"][1]));
        let lines: Vec<Line> = page["lines"]
            .as_array()
            .expect("lines")
            .iter()
            .enumerate()
            .map(|(i, l)| Line {
                bbox: bbox(&l["bbox"]),
                text: l["text"].as_str().expect("text").to_string(),
                words: 0,
                digits: 0,
                kind: "other",
                table: None,
                grid_fill: f64::NAN,
                grid_columns: f64::NAN,
                block: i,
                list: f64::NAN,
            })
            .collect();
        let words: Vec<PageWord> = page["words"]
            .as_array()
            .expect("words")
            .iter()
            .map(|w| PageWord {
                text: w["text"].as_str().expect("text").to_string(),
                bbox: bbox(&w["bbox"]),
                block: 0,
                span: 0,
                size: 0.0,
                bold: false,
                italic: false,
                font: None,
                color: None,
            })
            .collect();
        let all: Vec<Proposal> = page["proposals"]
            .as_array()
            .expect("proposals")
            .iter()
            .map(|p| {
                let name = p["kind"].as_str().expect("kind");
                Proposal {
                    lines: indexes(&p["lines"]),
                    kind: KINDS
                        .iter()
                        .copied()
                        .find(|k| *k == name)
                        .expect("a known kind"),
                    accepted: true,
                }
            })
            .collect();
        let probabilities: Vec<f64> = page["probabilities"]
            .as_array()
            .expect("probabilities")
            .iter()
            .map(number)
            .collect();

        let picked = verifier::candidates(&all, &probabilities);
        assert_eq!(
            picked,
            indexes(&page["candidates"]),
            "page {n}: different candidates"
        );
        let hulls: Vec<[f64; 4]> = picked
            .iter()
            .map(|&i| verifier::hull_of(&lines, &all[i].lines))
            .collect();
        for (k, &i) in picked.iter().enumerate() {
            let inside = verifier::inside_of(&words, &hulls[k]);
            let mine: Vec<&PageWord> = inside.iter().map(|&j| &words[j]).collect();
            let cells = verifier::grid(&mine).map(|g| g.cells).unwrap_or_default();
            let x = verifier::features(
                size,
                &lines,
                &words,
                &hulls[k],
                &hulls,
                k,
                probabilities[i],
                all[i].kind == "builder",
                &cells,
            );
            let want = page["features"][k].as_array().expect("x");
            for (f, v) in x.iter().enumerate() {
                let python = number(&want[f]);
                assert!(
                    (v - python).abs() <= 1e-9 * python.abs().max(1.0),
                    "page {n}, candidate {k}: feature {f} ({}) is {v} here and {python} in Python",
                    fixture["names"][f]
                );
                compared += 1;
            }
            let score = verifier::score(&x);
            let python = number(&page["scores"][k]);
            assert!(
                (score - python).abs() <= 1e-6,
                "page {n}, candidate {k}: score {score} against LightGBM's {python}"
            );
        }
        let chosen = verifier::choose(size, &lines, &words, &all, &probabilities, floor);
        let want: Vec<Vec<usize>> = page["chosen"]
            .as_array()
            .expect("chosen")
            .iter()
            .map(indexes)
            .collect();
        assert_eq!(chosen, want, "page {n}: different tables chosen");
    }
    assert!(compared > 500, "only {compared} features compared");
}
