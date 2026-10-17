// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The table trace prints numbers-only veto lines on stderr (#636).
//!
//! The trace writes with `eprintln`, which a test cannot capture in process,
//! so each test below re-runs this binary as a child: the child reads
//! `PDFLUENT_TABLE_TRACE_SELFTEST` and runs one extraction, and the parent
//! reads the child's stderr. The pages are operator-built, through the public
//! path (`extract_blocks`, `table_in_region`): a chart-axis fragment grid
//! under a figure caption (the `find`-path veto removes it, `demote` sweeps
//! what the learned pass rebuilds), and axis ticks over a caption for the
//! region entry.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::{ExtractOptions, PdfDocument};

const BODY_SIZE: f64 = 10.0;
const MARGIN: f64 = 72.0;
const TOP: f64 = 700.0;
/// One row to the next.
const PITCH: f64 = 18.0;
const C2: f64 = 210.0;
const C3: f64 = 320.0;
const C4: f64 = 430.0;

/// One run of glyphs at a point.
struct Piece {
    text: String,
    x: f64,
    y: f64,
}

fn text(body: &str, x: f64, y: f64) -> Piece {
    Piece {
        text: body.to_string(),
        x,
        y,
    }
}

/// One row of fragments: the cells at the column positions given. A `None`
/// cell is a hole, the way a figure's labels leave most of their grid empty.
fn row(cells: &[(Option<&str>, f64)], y: f64) -> Vec<Piece> {
    cells
        .iter()
        .filter_map(|(body, x)| body.map(|body| text(body, *x, y)))
        .collect()
}

fn font(doc: &mut Document) -> Object {
    Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    })))
}

/// A thin ruling: a filled rectangle 0.5 pt across, as producers draw them.
fn ruling(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Operation> {
    let (w, h) = if (y1 - y0).abs() < 1e-9 {
        (x1 - x0, 0.5)
    } else {
        (0.5, y1 - y0)
    };
    vec![
        Operation::new("re", vec![x0.into(), y0.into(), w.into(), h.into()]),
        Operation::new("f", vec![]),
    ]
}

fn stream_of(pieces: &[Piece], rulings: &[(f64, f64, f64, f64)]) -> Vec<u8> {
    let mut operations = Vec::new();
    for &(x0, y0, x1, y1) in rulings {
        operations.extend(ruling(x0, y0, x1, y1));
    }
    for piece in pieces {
        operations.push(Operation::new("BT", vec![]));
        operations.push(Operation::new(
            "Tf",
            vec![Object::Name(b"F1".to_vec()), BODY_SIZE.into()],
        ));
        operations.push(Operation::new("Td", vec![piece.x.into(), piece.y.into()]));
        operations.push(Operation::new(
            "Tj",
            vec![Object::String(
                piece.text.clone().into_bytes(),
                StringFormat::Literal,
            )],
        ));
        operations.push(Operation::new("ET", vec![]));
    }
    Content { operations }.encode().expect("encode content")
}

fn pdf_of(pieces: &[Piece]) -> Vec<u8> {
    pdf_of_ruled(pieces, &[])
}

/// Like [`pdf_of`], with thin rulings drawn under the text.
fn pdf_of_ruled(pieces: &[Piece], rulings: &[(f64, f64, f64, f64)]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let regular = font(&mut doc);
    let content_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        stream_of(pieces, rulings),
    )));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dictionary! {
                "F1" => regular,
            }),
        }),
    }));
    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![Object::Reference(page_id)]),
        "Count" => Object::Integer(1),
    }));
    if let Ok(Object::Dictionary(page)) = doc.get_object_mut(page_id) {
        page.set("Parent", Object::Reference(pages_id));
    }
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));
    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("save synthetic pdf");
    buf
}

/// The drawn lattice round the fragment grid: a box with a line between
/// every row and every column, so the learned pass rebuilds the refused grid
/// from a forced region and the `demote` sweep removes it.
fn extract_rulings() -> Vec<(f64, f64, f64, f64)> {
    let xs = [60.0, 200.0, 310.0, 420.0, 452.0];
    let ys = [709.0, 691.0, 673.0, 655.0, 637.0];
    let mut rulings = Vec::new();
    for &y in &ys {
        rulings.push((xs[0], y, xs[4], y));
    }
    for &x in &xs {
        rulings.push((x, ys[4], x, ys[0]));
    }
    rulings
}

/// Chart-axis ticks under a `FIG.` caption: the `find`-path veto removes the
/// grid, and the learned pass rebuilds it for the `demote` sweep.
fn extract_pieces() -> Vec<Piece> {
    let mut pieces = Vec::new();
    pieces.extend(row(
        &[
            (Some("14"), MARGIN),
            (Some("2.5"), C2),
            (Some("12"), C3),
            (Some("8"), C4),
        ],
        TOP,
    ));
    pieces.extend(row(
        &[
            (Some("0"), MARGIN),
            (None, C2),
            (Some("-2"), C3),
            (None, C4),
        ],
        TOP - PITCH,
    ));
    pieces.extend(row(
        &[
            (None, MARGIN),
            (Some("-4"), C2),
            (None, C3),
            (Some("-6"), C4),
        ],
        TOP - 2.0 * PITCH,
    ));
    pieces.extend(row(
        &[
            (Some("-10"), MARGIN),
            (None, C2),
            (None, C3),
            (Some("-12"), C4),
        ],
        TOP - 3.0 * PITCH,
    ));
    pieces.push(text(
        "FIG. 3. Dopant salt response.",
        MARGIN,
        TOP - 4.0 * PITCH,
    ));
    pieces
}

/// Axis ticks over a `Figure` caption for the region entry.
fn region_pieces() -> Vec<Piece> {
    let mut pieces = Vec::new();
    pieces.extend(row(
        &[
            (Some("14"), MARGIN),
            (Some("2.5"), C2),
            (Some("12"), C3),
            (Some("8"), C4),
        ],
        TOP,
    ));
    pieces.extend(row(
        &[
            (Some("0"), MARGIN),
            (None, C2),
            (Some("-2"), C3),
            (None, C4),
        ],
        TOP - PITCH,
    ));
    pieces.extend(row(
        &[
            (None, MARGIN),
            (Some("-4"), C2),
            (None, C3),
            (Some("-6"), C4),
        ],
        TOP - 2.0 * PITCH,
    ));
    pieces.extend(row(
        &[
            (Some("-10"), MARGIN),
            (None, C2),
            (None, C3),
            (Some("-12"), C4),
        ],
        TOP - 3.0 * PITCH,
    ));
    pieces.push(text("Figure 20. Tick marks.", MARGIN, TOP - 4.0 * PITCH));
    pieces
}

/// The child: one extraction on stderr, steered by
/// `PDFLUENT_TABLE_TRACE_SELFTEST`.
#[test]
fn table_trace_selftest_child() {
    match std::env::var("PDFLUENT_TABLE_TRACE_SELFTEST").as_deref() {
        Ok("extract") => {
            let doc = PdfDocument::open(pdf_of_ruled(&extract_pieces(), &extract_rulings()))
                .expect("open pdf");
            let _ = doc
                .extract_blocks(&ExtractOptions::default())
                .expect("extract blocks");
        }
        Ok("region") => {
            let doc = PdfDocument::open(pdf_of(&region_pieces())).expect("open pdf");
            let _ = doc
                .table_in_region(0, [40.0, 615.0, 560.0, 725.0])
                .expect("region entry runs");
        }
        _ => {}
    }
}

/// This binary as a child, with the trace variable set or removed.
fn child_stderr(mode: &str, trace_on: bool) -> String {
    let exe = std::env::current_exe().expect("current test binary");
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("table_trace_selftest_child")
        .arg("--exact")
        .arg("--nocapture")
        .env("PDFLUENT_TABLE_TRACE_SELFTEST", mode);
    if trace_on {
        cmd.env("PDFLUENT_TABLE_TRACE", "1");
    } else {
        cmd.env_remove("PDFLUENT_TABLE_TRACE");
    }
    let output = cmd.output().expect("spawn self as child");
    assert!(
        output.status.success(),
        "child failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stderr).expect("stderr is text")
}

/// Every `TABLE` line of the child's stderr.
fn table_lines(stderr: &str) -> Vec<&str> {
    stderr
        .lines()
        .filter(|line| line.starts_with("TABLE "))
        .collect()
}

/// The trace vocabulary: every word a `TABLE` line may carry besides its
/// numbers. Anything else on the line is leaked cell or page text.
const TRACE_WORDS: &[&str] = &[
    "TABLE",
    "page",
    "bbox",
    "rows",
    "cols",
    "cells",
    "origin",
    "median_alnum",
    "max_alpha_run",
    "empty_frac",
    "continu_row0",
    "continu_rows",
    "caption_marker",
    "fig_veto",
    "region_veto",
    "demote",
    "fired",
    "none",
    "no",
    "find",
    "find_whole",
    "learned-refine",
    "learned-forced",
    "region",
    "engine-grid-match",
    "tatr",
    "other",
    "no_cells",
    "no_columns",
    "dense",
    "holes<=third",
    "median>3",
    "no_caption_marker",
    "not_sideways",
    "no_caption_within_12em",
    "continu<2",
    "continu_not_in_row0",
    "dense_and_long",
    "not_sparse",
    "alpha_run>=4",
    "cells_not_short",
    "no_continu_lead_nearby",
    "no_grid_box",
    "judge_p",
    "forced",
    "lattice",
    "judged-not-one-table",
    "rules_h",
    "rules_v",
    "ruled_frac",
    "ocr_text",
    "fonts",
    "num_frac",
    "word_frac",
    "cols_aligned",
    "page_tables",
    "drawing_ops",
    "cand_forced_noruling",
    "cand_forced_lowjudge",
    "cand_ocr_drawings",
    "cand_unaligned_cols",
    "cand_drawing_dense",
    "cand_single_strip",
    "cand_wordless",
    "cand_ocr_only",
    "rule_a",
    "withdrawn",
    "kept",
    "no-trigger",
];

/// No cell or page text in the trace: strip the vocabulary and the numbers,
/// and nothing may remain.
fn assert_numbers_only(lines: &[&str]) {
    for line in lines {
        let mut rest = (*line).to_string();
        let mut words = TRACE_WORDS.to_vec();
        // Longest first, so `find_whole` strips before `find`.
        words.sort_by_key(|word| std::cmp::Reverse(word.len()));
        for word in words {
            rest = rest.replace(word, "");
        }
        assert!(
            rest.chars()
                .all(|c| c.is_ascii_digit() || ".,:;[] =+-".contains(c)),
            "trace line carries text beyond numbers: {line}"
        );
    }
    let banned = [
        "FIG",
        "Figure",
        "Dopant",
        "response",
        "Tick",
        "continued",
        "Polymer",
    ];
    for line in lines {
        for word in banned {
            assert!(
                !line.contains(word),
                "trace line leaks page text {word:?}: {line}"
            );
        }
    }
}

#[test]
fn the_trace_is_off_by_default() {
    for mode in ["extract", "region"] {
        let stderr = child_stderr(mode, false);
        assert!(
            table_lines(&stderr).is_empty(),
            "trace on with the variable unset: {stderr}"
        );
    }
}

#[test]
fn the_trace_reports_the_fired_vetoes() {
    let stderr = child_stderr("extract", true);
    let lines = table_lines(&stderr);
    assert!(
        lines.iter().any(|line| line.contains("fig_veto=fired")),
        "no fired figure veto in the trace: {stderr}"
    );
    assert!(
        lines.iter().any(|line| line.contains("demote=fired")),
        "no fired demote sweep in the trace: {stderr}"
    );
    // The sweep removed a learned rebuild, not just scored a candidate: only
    // the learned pass stamps `learned-forced`.
    assert!(
        lines
            .iter()
            .any(|line| line.contains("origin=learned-forced") && line.contains("demote=fired")),
        "demote removed no learned rebuild in the trace: {stderr}"
    );
    assert_numbers_only(&lines);
}

#[test]
fn the_trace_reports_the_region_veto() {
    let stderr = child_stderr("region", true);
    let lines = table_lines(&stderr);
    assert!(
        lines.iter().any(|line| line.contains("region_veto=fired")),
        "no fired region veto in the trace: {stderr}"
    );
    assert_numbers_only(&lines);
}
