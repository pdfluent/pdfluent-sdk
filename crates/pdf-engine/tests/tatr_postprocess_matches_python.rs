//! TATR in Rust against the vendored Python, and against the model itself.
//!
//! The parity test feeds every committed fixture's raw ONNX outputs through
//! `cells_from_raw_outputs` and requires the Python-decoded cells: same
//! grid, same header flags, same boxes, same filled texts, same kept count,
//! same confidence. No model, no dylib. The synthetic fixtures must match
//! all; the terminal-run real crops (30: 15 PT-val, 15 FTN-clean) must match
//! at least 29, with every diff listed.
//!
//! The end-to-end test runs real inference through the shared session on the
//! fixtures carrying pixels: the Rust bilinear against the resized PNG to an
//! approximate bar, then the exact resized pixels through the model within
//! 1e-3 of the fixture logits. It runs only where `PDFLUENT_ORT_TEST_DYLIB`
//! names a real ONNX Runtime 1.23.2, `PDFLUENT_TATR_TEST_MODEL_DIR` holds the
//! pinned package, and the e2e fixtures are committed -- and skips loudly
//! otherwise, like the shared layer's own dylib tests.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

#![cfg(feature = "ort-tables")]

use std::path::PathBuf;

use lopdf::dictionary;
use pdf_engine::learned::tables_tatr::{
    cells_from_raw_outputs, crop_px_to_pdf, fill_cells, infer_nchw, max_resize_dims,
    normalize_nchw, resize_bilinear_rgb, TatrWord, TATR_DPI, TATR_MAX_SIZE, TATR_SHA256, TATR_SPEC,
};

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tatr")
}

fn num(v: &serde_json::Value) -> f64 {
    v.as_f64().expect("a number")
}

fn quad(v: &serde_json::Value) -> [f64; 4] {
    let a = v.as_array().expect("four numbers");
    [num(&a[0]), num(&a[1]), num(&a[2]), num(&a[3])]
}

fn fixtures() -> Vec<(String, serde_json::Value)> {
    let mut out: Vec<(String, serde_json::Value)> = std::fs::read_dir(fixture_dir())
        .expect("the tatr fixture dir")
        .map(|entry| entry.expect("a fixture entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .map(|path| {
            let name = path
                .file_stem()
                .expect("a stem")
                .to_string_lossy()
                .into_owned();
            let text = std::fs::read_to_string(&path).expect("fixture text");
            let json: serde_json::Value = serde_json::from_str(&text).expect("fixture json");
            (name, json)
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// The Rust decode beside the fixture's Python cells: empty means identical.
fn diff_fixture(name: &str, fixture: &serde_json::Value) -> Vec<String> {
    let mut diffs = Vec::new();
    let report = |diffs: &mut Vec<String>, what: String| diffs.push(format!("{name}: {what}"));
    if fixture.get("schema").and_then(serde_json::Value::as_u64) != Some(1) {
        report(
            &mut diffs,
            format!("schema is {:?}, want 1", fixture.get("schema")),
        );
        return diffs;
    }
    let crop = fixture.get("crop").expect("a crop");
    if crop.get("dpi").and_then(serde_json::Value::as_u64) != Some(u64::from(TATR_DPI)) {
        report(
            &mut diffs,
            format!("dpi is {:?}, want {}", crop.get("dpi"), TATR_DPI),
        );
        return diffs;
    }
    let crop_box = quad(crop.get("crop_box_pt").expect("a crop box"));
    let width_px = num(crop.get("width_px").expect("a width"));
    let height_px = num(crop.get("height_px").expect("a height"));
    let logits: Vec<f32> = fixture
        .get("logits")
        .expect("logits")
        .as_array()
        .expect("a logits array")
        .iter()
        .map(|v| num(v) as f32)
        .collect();
    let boxes: Vec<f32> = fixture
        .get("pred_boxes")
        .expect("boxes")
        .as_array()
        .expect("a boxes array")
        .iter()
        .map(|v| num(v) as f32)
        .collect();
    let words: Vec<TatrWord> = fixture
        .get("words")
        .expect("words")
        .as_array()
        .expect("a words array")
        .iter()
        .map(|w| {
            TatrWord::new(
                w.get("text")
                    .expect("word text")
                    .as_str()
                    .expect("a string")
                    .to_string(),
                quad(w.get("bbox").expect("a word box")),
            )
        })
        .collect();
    let decode = cells_from_raw_outputs(&logits, &boxes, width_px, height_px, &words)
        .expect("shapes the generator validated");
    let details = fixture.get("details").expect("details");
    if details.get("kept").and_then(serde_json::Value::as_u64) != Some(decode.kept_objects as u64) {
        report(
            &mut diffs,
            format!(
                "kept {} objects, python {}",
                decode.kept_objects,
                details.get("kept").expect("a kept count")
            ),
        );
    }
    let confidence = num(details.get("confidence").expect("a confidence"));
    if (decode.confidence - confidence).abs() > 1e-9 {
        report(
            &mut diffs,
            format!("confidence {}, python {confidence}", decode.confidence),
        );
    }
    let want_cells = fixture
        .get("cells")
        .expect("cells")
        .as_array()
        .expect("a cells array");
    if decode.cells.len() != want_cells.len() {
        report(
            &mut diffs,
            format!("{} cells, python {}", decode.cells.len(), want_cells.len()),
        );
        return diffs;
    }
    // Texts fill in the generator's own frame: both sides through the crop
    // box, the same call the generator makes.
    let word_pdf: Vec<[f64; 4]> = words
        .iter()
        .map(|w| crop_px_to_pdf(w.bbox, crop_box))
        .collect();
    let cell_pdf: Vec<[f64; 4]> = decode
        .cells
        .iter()
        .map(|c| crop_px_to_pdf(c.bbox, crop_box))
        .collect();
    let claims = fill_cells(&word_pdf, &cell_pdf);
    for (at, (got, want)) in decode.cells.iter().zip(want_cells.iter()).enumerate() {
        let want_row = want
            .get("row")
            .and_then(serde_json::Value::as_u64)
            .expect("a row");
        let want_col = want
            .get("col")
            .and_then(serde_json::Value::as_u64)
            .expect("a col");
        let want_row_span = want
            .get("row_span")
            .and_then(serde_json::Value::as_u64)
            .expect("a row span");
        let want_col_span = want
            .get("col_span")
            .and_then(serde_json::Value::as_u64)
            .expect("a col span");
        let want_header = want
            .get("header")
            .and_then(serde_json::Value::as_bool)
            .expect("a header");
        if got.row as u64 != want_row
            || got.col as u64 != want_col
            || got.row_span as u64 != want_row_span
            || got.col_span as u64 != want_col_span
            || got.header != want_header
        {
            report(
                &mut diffs,
                format!(
                    "cell {at}: r{} c{} {}x{} h={} -- python r{want_row} c{want_col} \
                     {want_row_span}x{want_col_span} h={want_header}",
                    got.row, got.col, got.row_span, got.col_span, got.header,
                ),
            );
        }
        let want_box = quad(want.get("bbox").expect("a cell box"));
        let off = got
            .bbox
            .iter()
            .zip(want_box)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0, f64::max);
        if off > 1e-9 {
            report(
                &mut diffs,
                format!("cell {at}: box {:?} -- python {want_box:?}", got.bbox),
            );
        }
        let text = claims[at]
            .iter()
            .map(|&w| words[w].text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let want_text = want
            .get("text")
            .and_then(serde_json::Value::as_str)
            .expect("a text");
        if text != want_text {
            report(
                &mut diffs,
                format!("cell {at}: text {text:?} -- python {want_text:?}"),
            );
        }
    }
    diffs
}

#[test]
fn decoded_cells_match_the_python_cells() {
    let all = fixtures();
    assert!(
        !all.is_empty(),
        "no tatr fixtures: the parity test proves nothing"
    );
    let mut synthetic_diffs = Vec::new();
    let mut real_diffs: Vec<(String, Vec<String>)> = Vec::new();
    let mut real_count: usize = 0;
    for (name, fixture) in &all {
        let diffs = diff_fixture(name, fixture);
        if name.starts_with("syn_") {
            synthetic_diffs.extend(diffs);
        } else {
            real_count += 1;
            if !diffs.is_empty() {
                real_diffs.push((name.clone(), diffs));
            }
        }
    }
    for line in &synthetic_diffs {
        eprintln!("synthetic diff: {line}");
    }
    for (name, diffs) in &real_diffs {
        for line in diffs {
            eprintln!("real diff ({name}): {line}");
        }
    }
    assert!(
        synthetic_diffs.is_empty(),
        "{} synthetic diff(s): the port disagrees with the vendored path",
        synthetic_diffs.len()
    );
    if real_count == 0 {
        eprintln!("note: no terminal-run fixtures yet (T5's run lands 30); synthetic only");
        return;
    }
    // The P2 bar: identical on at least 29 of 30.
    let allowed = real_count.saturating_sub(29);
    let differing = real_diffs.len();
    assert!(
        differing <= allowed,
        "{differing} of {real_count} real fixtures differ (allowed {allowed}); see above"
    );
    eprintln!(
        "parity: {} of {real_count} real fixtures identical",
        real_count - differing
    );
}

fn test_dylib() -> Option<PathBuf> {
    match std::env::var("PDFLUENT_ORT_TEST_DYLIB") {
        Ok(path) if !path.is_empty() => Some(PathBuf::from(path)),
        _ => None,
    }
}

fn is_placeholder_sha() -> bool {
    TATR_SHA256 == "0000000000000000000000000000000000000000000000000000000000000000"
}

fn decode_png(payload: &str, what: &str) -> (u32, u32, Vec<u8>) {
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .expect("base64 pixels");
    let image = image::load_from_memory(&bytes).expect("a png").to_rgb8();
    let (width, height) = (image.width(), image.height());
    eprintln!("{what}: {width}x{height}");
    (width, height, image.into_raw())
}

fn blank_page_pdf() -> Vec<u8> {
    let mut doc = lopdf::Document::with_version("1.7");
    let content = lopdf::content::Content {
        operations: Vec::new(),
    }
    .encode()
    .expect("encode");
    let content_id = doc.add_object(lopdf::Object::Stream(lopdf::Stream::new(
        lopdf::dictionary! {},
        content,
    )));
    let page_id = doc.add_object(lopdf::dictionary! {
        "Type" => "Page", "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
    });
    let pages_id = doc.add_object(lopdf::dictionary! {
        "Type" => "Pages", "Kids" => vec![lopdf::Object::Reference(page_id)], "Count" => 1,
    });
    if let lopdf::Object::Dictionary(d) = doc.objects.get_mut(&page_id).expect("page") {
        d.set("Parent", pages_id);
    }
    let catalog = doc.add_object(lopdf::dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save pdf");
    out
}

#[test]
fn tatr_cells_without_a_model_is_not_available() {
    use pdf_engine::learned::tables_tatr::tatr_cells;

    // Through render, crop and preprocess to the shared layer's refusal --
    // no dylib involved, the model file is checked before the library.
    let pdf = blank_page_pdf();
    let document = pdf_engine::PdfDocument::open(pdf).expect("open");
    let dir = std::env::temp_dir().join("tatr-no-such-model-dir");
    let config = pdf_ocr::runtime::RuntimeConfig::new()
        .with_model_dir(dir.clone())
        .with_ort_dylib(dir.join("no-such-lib"));
    let err = tatr_cells(
        &document,
        0,
        [100.0, 100.0, 300.0, 200.0],
        &[],
        &[],
        &[],
        &[],
        &config,
    )
    .expect_err("a missing model must fail");
    let back = pdf_ocr::runtime::model_error(&err).expect("the chain carries the ModelError");
    assert_eq!(
        back.kind,
        pdf_ocr::runtime::ModelErrorKind::ModelNotAvailable
    );
    assert_eq!(back.model.as_deref(), Some("tatr-v1.1-all"));
    assert!(
        err.to_string().contains("tatr-v1.1-all.onnx"),
        "the display names the file: {err}"
    );
}

#[test]
fn tatr_cells_refuses_a_missing_page_and_a_nan_region() {
    use pdf_engine::learned::tables_tatr::tatr_cells;

    let pdf = blank_page_pdf();
    let document = pdf_engine::PdfDocument::open(pdf).expect("open");
    let config = pdf_ocr::runtime::RuntimeConfig::new()
        .with_model_dir("tatr-test-no-such-dir")
        .with_ort_dylib("tatr-test-no-such-lib");
    let err = tatr_cells(
        &document,
        99,
        [100.0, 100.0, 300.0, 200.0],
        &[],
        &[],
        &[],
        &[],
        &config,
    )
    .expect_err("a missing page must fail");
    assert!(
        pdf_ocr::runtime::model_error(&err).is_none(),
        "a render failure, not a model one: {err}"
    );
    let err = tatr_cells(&document, 0, [f64::NAN; 4], &[], &[], &[], &[], &config)
        .expect_err("a NaN region must fail");
    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
}

#[test]
fn real_inference_matches_the_fixture_logits() {
    let Some(dylib) = test_dylib() else {
        eprintln!("skipped: PDFLUENT_ORT_TEST_DYLIB is not set");
        return;
    };
    let model_dir = match std::env::var("PDFLUENT_TATR_TEST_MODEL_DIR") {
        Ok(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => {
            eprintln!("skipped: PDFLUENT_TATR_TEST_MODEL_DIR is not set");
            return;
        }
    };
    if is_placeholder_sha() {
        eprintln!("skipped: TATR_SHA256 is still the placeholder, no model verifies");
        return;
    }
    let e2e: Vec<(String, serde_json::Value)> = fixtures()
        .into_iter()
        .filter(|(_, f)| {
            f.get("crop_png_base64").is_some() && f.get("resized_png_base64").is_some()
        })
        .collect();
    if e2e.is_empty() {
        eprintln!("skipped: no fixtures carry e2e pixels yet (T5's run lands 3)");
        return;
    }
    assert!(
        e2e.len() >= 3,
        "want the 3 e2e crops, {} carry pixels: a partial set proves nothing",
        e2e.len()
    );
    let config = pdf_ocr::runtime::RuntimeConfig::new()
        .with_model_dir(model_dir)
        .with_ort_dylib(dylib);
    let shared = pdf_ocr::runtime::session(&TATR_SPEC, &config).expect("the pinned model loads");
    for (name, fixture) in &e2e {
        let crop = fixture.get("crop").expect("a crop");
        let (crop_w, crop_h, crop_rgb) = decode_png(
            fixture
                .get("crop_png_base64")
                .expect("crop pixels")
                .as_str()
                .expect("a string"),
            &format!("{name} crop"),
        );
        assert_eq!(
            (crop_w as f64, crop_h as f64),
            (
                num(crop.get("width_px").expect("a width")),
                num(crop.get("height_px").expect("a height"))
            ),
            "{name}: the crop pixels are not the recorded size"
        );
        let (resized_w, resized_h, resized_rgb) = decode_png(
            fixture
                .get("resized_png_base64")
                .expect("resized pixels")
                .as_str()
                .expect("a string"),
            &format!("{name} resized"),
        );
        // The resize geometry is exact; its values approximate (PIL's
        // fixed-point passes against ours).
        assert_eq!(
            max_resize_dims(crop_w, crop_h),
            (resized_w, resized_h),
            "{name}: MaxResize dims drifted from PIL's"
        );
        assert_eq!(
            resized_w.max(resized_h) as f64,
            TATR_MAX_SIZE,
            "{name}: longest side"
        );
        let ours = resize_bilinear_rgb(&crop_rgb, crop_w, crop_h, resized_w, resized_h);
        assert_eq!(ours.len(), resized_rgb.len(), "{name}: resized length");
        let mut total = 0u64;
        let mut max = 0u8;
        for (a, b) in ours.iter().zip(resized_rgb.iter()) {
            let gap = a.abs_diff(*b);
            total += u64::from(gap);
            max = max.max(gap);
        }
        let mean = total as f64 / ours.len() as f64;
        eprintln!("{name}: resize mean abs diff {mean:.3} lsb, max {max} lsb");
        assert!(
            mean <= 2.0,
            "{name}: resize mean {mean:.3} lsb, want <= 2.0"
        );
        assert!(max <= 32, "{name}: resize max {max} lsb, want <= 32");
        // The strict bar runs on the exact resized pixels.
        let nchw = normalize_nchw(&resized_rgb, resized_w, resized_h);
        let (logits, boxes) =
            infer_nchw(&shared, &nchw, resized_w, resized_h).expect("inference runs");
        let want_logits: Vec<f32> = fixture
            .get("logits")
            .expect("logits")
            .as_array()
            .expect("an array")
            .iter()
            .map(|v| num(v) as f32)
            .collect();
        let want_boxes: Vec<f32> = fixture
            .get("pred_boxes")
            .expect("boxes")
            .as_array()
            .expect("an array")
            .iter()
            .map(|v| num(v) as f32)
            .collect();
        let max_gap = |a: &[f32], b: &[f32]| {
            assert_eq!(a.len(), b.len(), "{name}: output length");
            a.iter()
                .zip(b.iter())
                .map(|(x, y)| (x - y).abs())
                .fold(0.0, f32::max)
        };
        let logits_gap = max_gap(&logits, &want_logits);
        let boxes_gap = max_gap(&boxes, &want_boxes);
        eprintln!("{name}: max logit diff {logits_gap:.2e}, max box diff {boxes_gap:.2e}");
        assert!(
            logits_gap <= 1e-3,
            "{name}: max logit diff {logits_gap:.2e}, want <= 1e-3"
        );
        assert!(
            boxes_gap <= 1e-3,
            "{name}: max box diff {boxes_gap:.2e}, want <= 1e-3"
        );
    }
}
