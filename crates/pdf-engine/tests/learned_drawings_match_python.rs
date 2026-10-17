//! The line network's vector inputs, Rust against Python (#574).
//!
//! `scripts/extraction/learned/drawings.py --write-parity` writes 200 random
//! lines with rulings and fills around them and what the Python reads of each;
//! `learned::drawings::line_drawings` has to read the same, value for value, or
//! the network is trained on one thing and run on another.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_engine::learned::drawings::line_drawings;
use pdf_engine::text::Ruling;

#[test]
fn the_drawings_around_a_line_read_as_the_python_reads_them() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/learned/drawings_parity.json"
    );
    let cases: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json");
    let num = |v: &serde_json::Value| v.as_f64().expect("a number");
    let quad = |v: &serde_json::Value| -> [f64; 4] {
        let a = v.as_array().expect("four numbers");
        [num(&a[0]), num(&a[1]), num(&a[2]), num(&a[3])]
    };
    let cases = cases.as_array().expect("cases");
    assert_eq!(cases.len(), 200);
    for (k, case) in cases.iter().enumerate() {
        let rulings: Vec<Ruling> = case["rulings"]
            .as_array()
            .expect("rulings")
            .iter()
            .map(|r| {
                let [x0, y0, x1, y1] = quad(r);
                Ruling { x0, y0, x1, y1 }
            })
            .collect();
        let fills: Vec<[f64; 4]> = case["fills"]
            .as_array()
            .expect("fills")
            .iter()
            .map(quad)
            .collect();
        let got = line_drawings(quad(&case["bbox"]), num(&case["size"]), &rulings, &fills);
        let want: Vec<f64> = case["want"]
            .as_array()
            .expect("want")
            .iter()
            .map(num)
            .collect();
        for (g, w) in got.iter().zip(&want) {
            assert!((g - w).abs() < 1e-9, "case {k}: {got:?} against {want:?}");
        }
    }
}
