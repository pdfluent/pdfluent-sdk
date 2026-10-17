// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The table judge here is the table judge the model was trained with (#574).
//!
//! `train_table_judge.py` builds the proposals and their features and trains
//! the model; `judge.rs` builds them again at serve time. This holds the two
//! together on real DocLayNet validation pages written by the Python: which
//! lines a detector's hull holds, which proposals exist and from which kind,
//! all 35 features to 1e-9, the exported model's probability, and the tables
//! chosen at the shipped floor.

use pdf_engine::learned::judge::{self, Line, Proposal, FEATURES, KINDS};
use pdf_engine::learned::{alignment, splits};

fn number(v: &serde_json::Value) -> f64 {
    v.as_f64().unwrap_or(f64::NAN)
}

fn indexes(v: &serde_json::Value) -> Vec<usize> {
    v.as_array()
        .expect("list")
        .iter()
        .map(|i| i.as_u64().expect("index") as usize)
        .collect()
}

fn kind(name: &str) -> &'static str {
    match name {
        "table_cell" => "table_cell",
        "paragraph" => "paragraph",
        "heading" => "heading",
        "list_item" => "list_item",
        "header" => "header",
        "footer" => "footer",
        _ => "other",
    }
}

fn close(a: f64, b: f64) -> bool {
    (a.is_nan() && b.is_nan()) || (a - b).abs() <= 1e-9
}

#[test]
fn the_judge_proposes_measures_and_chooses_as_the_python_does() {
    let raw = include_str!("fixtures/learned/judge_parity.json");
    let fixture: serde_json::Value = serde_json::from_str(raw).expect("fixture");
    let names = fixture["names"].as_array().expect("names");
    assert_eq!(names.len(), FEATURES, "the feature count moved");
    let floor = number(&fixture["floor"]);
    assert_eq!(
        floor,
        judge::FLOOR,
        "the fixture was written at another floor"
    );
    let pages = fixture["pages"].as_array().expect("pages");
    assert!(pages.len() >= 8, "too few pages to hold the judge");

    let mut kinds_seen = std::collections::BTreeSet::new();
    let mut compared = 0usize;
    for (n, page) in pages.iter().enumerate() {
        let lines: Vec<Line> = page["lines"]
            .as_array()
            .expect("lines")
            .iter()
            .map(|l| {
                let b = l["bbox"].as_array().expect("bbox");
                Line {
                    bbox: [number(&b[0]), number(&b[1]), number(&b[2]), number(&b[3])],
                    text: l["text"].as_str().expect("text").to_string(),
                    words: l["words"].as_u64().expect("words") as usize,
                    digits: l["digits"].as_u64().expect("digits") as usize,
                    kind: kind(l["kind"].as_str().expect("kind")),
                    table: l["table"].as_u64().map(|t| t as u32),
                    grid_fill: number(&l["grid_fill"]),
                    grid_columns: number(&l["grid_columns"]),
                    block: 0,
                    list: number(&l["list"]),
                }
            })
            .collect();

        // THE HULL RULE: the lines each detector's region holds.
        let mut drawn: Vec<(&'static str, Vec<usize>, bool)> = Vec::new();
        for hull in page["hulls"].as_array().expect("hulls") {
            let h = hull["hull"].as_array().expect("hull");
            let got = judge::held_by(
                [number(&h[0]), number(&h[1]), number(&h[2]), number(&h[3])],
                &lines,
            );
            assert_eq!(
                got,
                indexes(&hull["lines"]),
                "page {n}: a hull holds other lines"
            );
            let detector = hull["detector"].as_str().expect("detector");
            let name = KINDS
                .iter()
                .copied()
                .find(|k| *k == detector)
                .unwrap_or("aligned");
            drawn.push((name, got, hull["accepted"].as_bool().unwrap_or(true)));
        }
        // AND THE ALIGNMENT PORT, after the regions, as the dump orders them.
        let boxes: Vec<[f64; 4]> = lines.iter().map(|l| l.bbox).collect();
        for set in alignment::proposals(&boxes) {
            drawn.push(("alignment_lines", set, true));
        }
        // AND THE PIECES (`splits::pieces`, T2 #2016), held to the dump's.
        let sets: Vec<Vec<usize>> = drawn.iter().map(|(_, s, _)| s.clone()).collect();
        let pieces = splits::pieces(&sets, &boxes);
        let want: Vec<Vec<usize>> = page["pieces"]
            .as_array()
            .expect("pieces")
            .iter()
            .map(indexes)
            .collect();
        assert_eq!(pieces, want, "page {n}: different pieces");
        drawn.extend(pieces.into_iter().map(|s| ("split", s, true)));

        let all = judge::proposals(&lines, &drawn);
        let want: Vec<Proposal> = page["proposals"]
            .as_array()
            .expect("proposals")
            .iter()
            .map(|p| Proposal {
                lines: indexes(&p["lines"]),
                kind: KINDS
                    .iter()
                    .copied()
                    .find(|k| *k == p["kind"].as_str().expect("kind"))
                    .expect("a known kind"),
                accepted: p["accepted"].as_bool().expect("accepted"),
            })
            .collect();
        assert_eq!(all, want, "page {n}: different proposals");

        let theirs = page["features"].as_array().expect("features");
        let mut probabilities = Vec::new();
        for (i, candidate) in all.iter().enumerate() {
            kinds_seen.insert(candidate.kind);
            let x = judge::features(
                (number(&page["size"][0]), number(&page["size"][1])),
                &lines,
                candidate,
                &all,
            );
            let want = theirs[i].as_array().expect("x");
            for (k, name) in names.iter().enumerate() {
                assert!(
                    close(x[k], number(&want[k])),
                    "page {n}, proposal {i}, {}: Rust {} against Python {}",
                    name.as_str().unwrap_or("?"),
                    x[k],
                    number(&want[k])
                );
                compared += 1;
            }
            let p = judge::probability(&x);
            let python = number(&page["probabilities"][i]);
            assert!(
                (p - python).abs() <= 1e-6,
                "page {n}, proposal {i}: probability {p} against LightGBM's {python}"
            );
            probabilities.push(p);
        }
        let chosen = judge::choose(&all, &probabilities, floor);
        let want: Vec<Vec<usize>> = page["chosen"]
            .as_array()
            .expect("chosen")
            .iter()
            .map(indexes)
            .collect();
        assert_eq!(chosen, want, "page {n}: different tables chosen");
    }
    assert_eq!(
        kinds_seen.len(),
        KINDS.len(),
        "not every kind of proposal is held: {kinds_seen:?}"
    );
    assert!(compared > 1000, "only {compared} features compared");
}
