//! Heron-ft ep03 through [`layout_boxes`](pdf_engine::layout::layout_boxes),
//! against the HF reference (#641).
//!
//! Twenty proxy pages under `PDFLUENT_LAYOUT_PARITY_DIR` (`pdfs/` plus per
//! page `ref/<pdf>.json` with the render size, the preprocessed tensor's
//! shape/sum/mean/sha256 and the HF post-processing at 0.05, written by
//! `make_ref.py`), with `PDFLUENT_MODEL_DIR` holding
//! `layout-heron-ep03/heron_ep03.onnx` and `ORT_DYLIB_PATH` naming ONNX
//! Runtime 1.23.2. Runs only when both directories are set; skipped aloud
//! otherwise, which is the CI path (no CI machine carries the model).
//!
//! Gates: the render size equals the reference on every page; boxes match by
//! label + IoU >= 0.9, and then per class the p95 |score delta| is below
//! 0.01; no unmatched box scores >= 0.1 on either side. The preprocessed
//! tensor's sum/mean against the reference is reported, not gated -- resize
//! filters can differ by ~1e-3 -- and on a box failure the report shows the
//! tensor comparison beside the box comparison, so a preprocess drift reads
//! as a tensor diff and a postprocess bug as boxes differing from an
//! identical tensor. Median detector time (preprocess + run + post, load
//! excluded) and whole-page time are reported at 4 intra threads.

#![cfg(feature = "ort-layout")]

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;

use pdf_engine::layout::{
    layout_boxes, preprocess_to_nchw, render_page_rgb, LayoutBox, RuntimeConfig,
};
use pdf_engine::PdfDocument;

/// Box match threshold: same label and at least this IoU.
const MATCH_IOU: f64 = 0.9;

/// Per-class p95 |score delta| ceiling on matched boxes.
const SCORE_P95: f64 = 0.01;

/// An unmatched box at or above this score fails the gate on either side.
const UNMATCHED_SCORE: f64 = 0.1;

struct RefBox {
    label: String,
    score: f64,
    bbox: [f64; 4],
}

struct PageRef {
    width: u32,
    height: u32,
    sum: f64,
    mean: f64,
    boxes: Vec<RefBox>,
}

fn read_page_ref(path: &std::path::Path) -> PageRef {
    let text = std::fs::read_to_string(path).expect("a reference json");
    let v: serde_json::Value = serde_json::from_str(&text).expect("json parses");
    let size = v["size"].as_array().expect("size is [w, h]");
    let tensor = &v["tensor"];
    let boxes = v["boxes"]
        .as_array()
        .expect("boxes is a list")
        .iter()
        .map(|b| {
            let bb = b["bbox"].as_array().expect("bbox is [x1, y1, x2, y2]");
            RefBox {
                label: b["label"].as_str().expect("a label").to_string(),
                score: b["score"].as_f64().expect("a score"),
                bbox: [
                    bb[0].as_f64().expect("x1"),
                    bb[1].as_f64().expect("y1"),
                    bb[2].as_f64().expect("x2"),
                    bb[3].as_f64().expect("y2"),
                ],
            }
        })
        .collect();
    PageRef {
        width: size[0].as_u64().expect("w") as u32,
        height: size[1].as_u64().expect("h") as u32,
        sum: tensor["sum"].as_f64().expect("sum"),
        mean: tensor["mean"].as_f64().expect("mean"),
        boxes,
    }
}

fn iou(a: &[f32; 4], b: &[f64; 4]) -> f64 {
    let (ax1, ay1, ax2, ay2) = (
        f64::from(a[0]),
        f64::from(a[1]),
        f64::from(a[2]),
        f64::from(a[3]),
    );
    let ix1 = ax1.max(b[0]);
    let iy1 = ay1.max(b[1]);
    let ix2 = ax2.min(b[2]);
    let iy2 = ay2.min(b[3]);
    let inter = (ix2 - ix1).max(0.0) * (iy2 - iy1).max(0.0);
    let area_a = (ax2 - ax1).max(0.0) * (ay2 - ay1).max(0.0);
    let area_b = (b[2] - b[0]).max(0.0) * (b[3] - b[1]).max(0.0);
    let union = area_a + area_b - inter;
    if union <= 0.0 {
        1.0
    } else {
        inter / union
    }
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(|a, b| a.total_cmp(b));
    let n = values.len();
    if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    }
}

fn percentile_95(sorted_ascending: &[f64]) -> f64 {
    let n = sorted_ascending.len();
    sorted_ascending[(0.95 * n as f64).ceil() as usize - 1]
}

#[test]
fn layout_parity_with_the_hf_reference() {
    let parity = std::env::var("PDFLUENT_LAYOUT_PARITY_DIR")
        .ok()
        .filter(|s| !s.is_empty());
    let models = std::env::var("PDFLUENT_MODEL_DIR")
        .ok()
        .filter(|s| !s.is_empty());
    let (Some(parity), Some(models)) = (parity, models) else {
        eprintln!("skipped: PDFLUENT_LAYOUT_PARITY_DIR and PDFLUENT_MODEL_DIR must both be set");
        return;
    };
    let parity = PathBuf::from(parity);
    let mut pdfs: Vec<PathBuf> = std::fs::read_dir(parity.join("pdfs"))
        .expect("pdfs/ reads")
        .map(|e| e.expect("a dir entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "pdf"))
        .collect();
    pdfs.sort();
    assert!(!pdfs.is_empty(), "pdfs/ holds the twenty proxy pages");

    let config = RuntimeConfig::new()
        .with_model_dir(models)
        .with_intra_threads(4);

    // Warmup: the checksum, the library load and the first inference happen
    // once, outside the clock. Every page below then times steady state.
    let first = std::fs::read(&pdfs[0]).expect("the first pdf reads");
    let doc = PdfDocument::open(first).expect("the first pdf opens");
    layout_boxes(&doc, 0, &config).expect("the warmup page runs");

    let mut failures: Vec<String> = Vec::new();
    let mut whole_ms: Vec<f64> = Vec::new();
    let mut detector_ms: Vec<f64> = Vec::new();
    let mut report: Vec<String> = Vec::new();

    for pdf in &pdfs {
        let name = pdf
            .file_name()
            .expect("a file name")
            .to_string_lossy()
            .into_owned();
        let bytes = std::fs::read(pdf).expect("a pdf reads");
        let doc = PdfDocument::open(bytes).expect("a pdf opens");
        let reference = read_page_ref(&parity.join("ref").join(format!("{name}.json")));

        let render_start = Instant::now();
        let rgb = render_page_rgb(&doc, 0).expect("the page renders");
        let render_ms = render_start.elapsed().as_secs_f64() * 1000.0;

        let whole_start = Instant::now();
        let got = layout_boxes(&doc, 0, &config).expect("the page detects");
        let whole = whole_start.elapsed().as_secs_f64() * 1000.0;
        whole_ms.push(whole);
        detector_ms.push((whole - render_ms).max(0.0));

        if (rgb.width, rgb.height) != (reference.width, reference.height) {
            failures.push(format!(
                "{name}: render size {}x{} is not the reference {}x{}",
                rgb.width, rgb.height, reference.width, reference.height
            ));
            continue;
        }
        for b in &got {
            assert_eq!(
                (b.render_w, b.render_h),
                (rgb.width, rgb.height),
                "{name}: every box is addressed against the render"
            );
        }

        // The tensor comparison: reported, not gated. A box failure beside a
        // matching sum/mean is a postprocess bug; beside a drifted one, the
        // resize filter.
        let nchw = preprocess_to_nchw(&rgb);
        assert_eq!(nchw.len(), 3 * 640 * 640, "{name}: NCHW 1x3x640x640");
        let sum: f64 = nchw.iter().map(|&v| f64::from(v)).sum();
        let mean = sum / nchw.len() as f64;
        let sum_rel = (sum - reference.sum).abs() / reference.sum.abs().max(1e-9);
        let mean_rel = (mean - reference.mean).abs() / reference.mean.abs().max(1e-9);

        // Greedy match by descending reference score: same label, best IoU,
        // each candidate box used once.
        let mut order: Vec<usize> = (0..reference.boxes.len()).collect();
        order.sort_by(|&a, &b| {
            reference.boxes[b]
                .score
                .total_cmp(&reference.boxes[a].score)
        });
        let mut used = vec![false; got.len()];
        let mut matched: Vec<(usize, usize, f64)> = Vec::new();
        let mut unmatched_ref: Vec<usize> = Vec::new();
        for &r in &order {
            let want = &reference.boxes[r];
            let mut best: Option<(usize, f64)> = None;
            for (c, cand) in got.iter().enumerate() {
                if used[c] || cand.label != want.label {
                    continue;
                }
                let overlap = iou(&cand.bbox_px, &want.bbox);
                if overlap >= MATCH_IOU && best.is_none_or(|(_, b)| overlap > b) {
                    best = Some((c, overlap));
                }
            }
            match best {
                Some((c, overlap)) => {
                    used[c] = true;
                    matched.push((r, c, overlap));
                }
                None => unmatched_ref.push(r),
            }
        }
        let unmatched_got: Vec<usize> = used
            .iter()
            .enumerate()
            .filter(|(_, u)| !**u)
            .map(|(c, _)| c)
            .collect();

        // Per class p95 |score delta| on the matches.
        let mut per_class: HashMap<&str, Vec<(f64, f64, f64, f64)>> = HashMap::new();
        for &(r, c, overlap) in &matched {
            let delta = (f64::from(got[c].score) - reference.boxes[r].score).abs();
            per_class.entry(got[c].label.as_str()).or_default().push((
                delta,
                reference.boxes[r].score,
                f64::from(got[c].score),
                overlap,
            ));
        }
        let mut worst_p95 = 0.0f64;
        let mut classes = per_class.keys().copied().collect::<Vec<_>>();
        classes.sort();
        for class in classes {
            let mut pairs = per_class[class].clone();
            pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
            let deltas: Vec<f64> = pairs.iter().map(|p| p.0).collect();
            let p95 = percentile_95(&deltas);
            worst_p95 = worst_p95.max(p95);
            if p95 >= SCORE_P95 {
                failures.push(format!(
                    "{name}: class {class} p95 |score delta| {p95:.4} over {} matches is not < {SCORE_P95}",
                    deltas.len()
                ));
                for (delta, ref_score, got_score, overlap) in pairs.iter().rev().take(5) {
                    failures.push(format!(
                        "{name}: {class} delta {delta:.4} ref {ref_score:.4} got {got_score:.4} iou {overlap:.4}"
                    ));
                }
            }
        }

        let loud_ref: Vec<&RefBox> = unmatched_ref
            .iter()
            .map(|&r| &reference.boxes[r])
            .filter(|b| b.score >= UNMATCHED_SCORE)
            .collect();
        let loud_got: Vec<&LayoutBox> = unmatched_got
            .iter()
            .map(|&c| &got[c])
            .filter(|b| f64::from(b.score) >= UNMATCHED_SCORE)
            .collect();
        if !loud_ref.is_empty() || !loud_got.is_empty() {
            failures.push(format!(
                "{name}: {} unmatched reference boxes and {} unmatched candidate boxes at score >= {UNMATCHED_SCORE} ({} ref, {} got, {} matched)",
                loud_ref.len(),
                loud_got.len(),
                reference.boxes.len(),
                got.len(),
                matched.len()
            ));
            for b in loud_ref.iter().take(5) {
                failures.push(format!(
                    "{name}: unmatched ref {} score {:.3} bbox {:?}",
                    b.label, b.score, b.bbox
                ));
            }
            for b in loud_got.iter().take(5) {
                failures.push(format!(
                    "{name}: unmatched got {} score {:.3} bbox {:?}",
                    b.label, b.score, b.bbox_px
                ));
            }
        }

        report.push(format!(
            "{name}: {}x{} ok, tensor sum {sum:.1} vs {:.1} (rel {sum_rel:.2e}), mean {mean:.6} vs {:.6} (rel {mean_rel:.2e}), {} ref / {} got / {} matched, worst class p95 {worst_p95:.4}",
            rgb.width,
            rgb.height,
            reference.sum,
            reference.mean,
            reference.boxes.len(),
            got.len(),
            matched.len(),
        ));
    }

    let whole_p50 = median(&mut whole_ms.clone());
    let detector_p50 = median(&mut detector_ms.clone());
    eprintln!("layout parity: {} pages at 4 intra threads", pdfs.len());
    for line in &report {
        eprintln!("  {line}");
    }
    eprintln!("  median whole-page ms: {whole_p50:.0}");
    eprintln!("  median detector ms (load excluded): {detector_p50:.0}");

    assert!(
        failures.is_empty(),
        "layout parity failed:\n  {}",
        failures.join("\n  ")
    );
}
