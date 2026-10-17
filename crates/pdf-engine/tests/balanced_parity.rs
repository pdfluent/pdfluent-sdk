//! The balanced assembly against the Python adapter, page for page (#641).
//!
//! Some twenty pages under `PDFLUENT_BALANCED_PARITY_DIR` (`pdfs/` with the
//! PDFs, `boxes/` with the cached detector-A boxes as `<pdf>.A.json`, `ref/`
//! with the adapter's markdown as `<stem>.md`), all copied from the parity
//! run the job reports. The gate is byte-identical markdown on every page,
//! from the same cached boxes. Runs only when the directory is set; skipped
//! aloud otherwise, which is the CI path (the pages live outside the repo).

#![cfg(feature = "ort-layout")]

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::path::{Path, PathBuf};

use pdf_engine::balanced::{balanced_page_markdown, BalancedOptions};
use pdf_engine::layout::LayoutBox;
use pdf_engine::PdfDocument;

/// Cached detector-A boxes for one page: the render size plus the
/// post-threshold boxes, as the adapter's `--box-cache` holds them.
struct CachedBoxes {
    /// Render width and height the boxes are addressed against.
    render: (u32, u32),
    /// The boxes.
    boxes: Vec<LayoutBox>,
}

/// Read `<stem>.pdf.A.json`: `{"size": [w, h], "boxes": [...]}`.
///
/// Scores and coordinates parse as f64 and narrow to f32, which recovers the
/// detector's own f32 values bit for bit (the JSON round-trips them).
fn read_cached_boxes(path: &Path) -> CachedBoxes {
    let text = std::fs::read_to_string(path).expect("a cached boxes file");
    let v: serde_json::Value = serde_json::from_str(&text).expect("json parses");
    let size = v["size"].as_array().expect("size is [w, h]");
    let render = (
        size[0].as_u64().expect("w") as u32,
        size[1].as_u64().expect("h") as u32,
    );
    let boxes = v["boxes"]
        .as_array()
        .expect("boxes is a list")
        .iter()
        .map(|b| {
            let bb = b["bbox"].as_array().expect("bbox is [x1, y1, x2, y2]");
            let coord = |i: usize| bb[i].as_f64().expect("a coordinate") as f32;
            LayoutBox {
                label: b["label"].as_str().expect("a label").to_string(),
                score: b["score"].as_f64().expect("a score") as f32,
                bbox_px: [coord(0), coord(1), coord(2), coord(3)],
                render_w: render.0,
                render_h: render.1,
            }
        })
        .collect();
    CachedBoxes { render, boxes }
}

/// Ten lines around the first differing line: five of context, then the two
/// sides' next five lines.
fn ten_line_diff(want: &str, got: &str) -> String {
    let want: Vec<&str> = want.lines().collect();
    let got: Vec<&str> = got.lines().collect();
    let first = (0..want.len().max(got.len()))
        .find(|&i| want.get(i) != got.get(i))
        .unwrap_or(0);
    let from = first.saturating_sub(5);
    let mut out = format!("first difference at line {}:\n", first + 1);
    for line in want.iter().take(first + 5).skip(from) {
        out.push_str(&format!("< {line}\n"));
    }
    for line in got.iter().take(first + 5).skip(from) {
        out.push_str(&format!("> {line}\n"));
    }
    out
}

#[test]
fn balanced_parity_with_the_python_adapter() {
    let parity = std::env::var("PDFLUENT_BALANCED_PARITY_DIR")
        .ok()
        .filter(|s| !s.is_empty());
    let Some(parity) = parity else {
        eprintln!("skipped: PDFLUENT_BALANCED_PARITY_DIR must be set");
        return;
    };
    let parity = PathBuf::from(parity);
    let mut pdfs: Vec<PathBuf> = std::fs::read_dir(parity.join("pdfs"))
        .expect("pdfs/ reads")
        .map(|e| e.expect("a dir entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "pdf"))
        .collect();
    pdfs.sort();
    assert!(!pdfs.is_empty(), "pdfs/ holds the parity pages");

    let opts = BalancedOptions::default();
    let mut identical = 0usize;
    let mut different = Vec::new();
    for pdf in &pdfs {
        let name = pdf.file_name().expect("a file name");
        let stem = pdf.file_stem().expect("a stem");
        let cached = read_cached_boxes(
            &parity
                .join("boxes")
                .join(format!("{}.A.json", name.to_string_lossy())),
        );
        let _ = cached.render;
        let bytes = std::fs::read(pdf).expect("the pdf reads");
        let doc = PdfDocument::open(bytes).expect("the pdf opens");
        let got = balanced_page_markdown(&doc, 0, &cached.boxes, &opts).expect("balanced runs");
        let want = std::fs::read_to_string(
            parity
                .join("ref")
                .join(format!("{}.md", stem.to_string_lossy())),
        )
        .expect("the reference reads");
        if got == want {
            identical += 1;
        } else {
            different.push((
                name.to_string_lossy().to_string(),
                ten_line_diff(&want, &got),
            ));
        }
    }
    println!(
        "balanced parity: {identical} identical, {} different",
        different.len()
    );
    for (page, diff) in &different {
        println!("--- {page}\n{diff}");
    }
    assert!(
        different.is_empty(),
        "{} of {} pages differ",
        different.len(),
        pdfs.len()
    );
}
