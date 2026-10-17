//! What a line is set in against the page's body, Rust against Python (#574).
//!
//! `scripts/extraction/learned/style.py --write-parity` writes 120 random
//! pages -- faces with and without a subset prefix or a weight in the name,
//! colours, sizes, the bold flag -- and what the Python reads of every line;
//! `learned::style::page_style` has to read the same, value for value, or the
//! line models are trained on one thing and run on another.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_engine::learned::features::PageWord;
use pdf_engine::learned::style::page_style;

#[test]
fn what_a_line_is_set_in_reads_as_the_python_reads_it() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/learned/style_parity.json"
    );
    let cases: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json");
    let cases = cases.as_array().expect("cases");
    assert_eq!(cases.len(), 120);
    let num = |v: &serde_json::Value| v.as_f64().expect("a number");
    let mut lines = 0;
    for (k, case) in cases.iter().enumerate() {
        let words: Vec<PageWord> = case["words"]
            .as_array()
            .expect("words")
            .iter()
            .map(|w| {
                let b = w["bbox"].as_array().expect("bbox");
                PageWord {
                    text: w["text"].as_str().expect("text").to_string(),
                    bbox: [num(&b[0]), num(&b[1]), num(&b[2]), num(&b[3])],
                    block: w["block"].as_u64().expect("block") as usize,
                    span: 0,
                    size: num(&w["size"]),
                    bold: w["bold"].as_bool().expect("bold"),
                    italic: false,
                    font: w["font"].as_str().map(str::to_string),
                    color: w["color"].as_array().map(|c| {
                        [
                            c[0].as_u64().expect("r") as u8,
                            c[1].as_u64().expect("g") as u8,
                            c[2].as_u64().expect("b") as u8,
                        ]
                    }),
                }
            })
            .collect();
        let n_blocks = case["n_blocks"].as_u64().expect("n_blocks") as usize;
        let got: Vec<[f64; 5]> = page_style(n_blocks, &words).into_iter().flatten().collect();
        let want = case["want"].as_array().expect("want");
        assert_eq!(got.len(), want.len(), "page {k}: lines");
        for (line, (g, w)) in got.iter().zip(want).enumerate() {
            let w: Vec<f64> = w.as_array().expect("row").iter().map(num).collect();
            for (a, b) in g.iter().zip(&w) {
                assert!(
                    (a - b).abs() < 1e-9,
                    "page {k} line {line}: {g:?} against {w:?}"
                );
            }
            lines += 1;
        }
    }
    assert!(lines > 500, "the fixture reads {lines} lines");
}
