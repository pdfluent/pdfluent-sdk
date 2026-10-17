//! The line network's image inputs, Rust against Python (#574).
//!
//! `scripts/extraction/learned/raster.py --write-parity` writes 100 small
//! synthetic rasters with a line each and what the Python reads of it;
//! `learned::raster::line_image` has to read the same, value for value, or the
//! network is trained on one thing and run on another.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use base64::engine::general_purpose::STANDARD as Base64;
use base64::Engine as _;
use pdf_engine::learned::raster::{line_image, GrayPage};

#[test]
fn the_raster_around_a_line_reads_as_the_python_reads_it() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/learned/raster_parity.json"
    );
    let cases: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json");
    let num = |v: &serde_json::Value| v.as_f64().expect("a number");
    let quad = |v: &serde_json::Value| -> [f64; 4] {
        let a = v.as_array().expect("four numbers");
        [num(&a[0]), num(&a[1]), num(&a[2]), num(&a[3])]
    };
    let cases = cases.as_array().expect("cases");
    assert_eq!(cases.len(), 100);
    for (k, case) in cases.iter().enumerate() {
        let width = case["width"].as_u64().expect("width") as u32;
        let height = case["height"].as_u64().expect("height") as u32;
        let data = Base64
            .decode(case["raster"].as_str().expect("raster"))
            .expect("base64");
        assert_eq!(data.len(), width as usize * height as usize, "case {k}");
        let page = GrayPage {
            width,
            height,
            data: &data,
        };
        let got = line_image(
            quad(&case["bbox"]),
            num(&case["size"]),
            &page,
            f64::from(height),
        );
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
