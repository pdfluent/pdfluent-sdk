// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The table-trace feature fields and candidate flags (#636).
//!
//! Through the public path (`extract_blocks`) on an operator-built PDF: a
//! real ruled word table prints every `cand_*` flag as 0, with the ruling,
//! font and alignment features populated. Like `table_trace.rs` this file
//! re-runs its own binary as a child, because the trace writes with
//! `eprintln`, which a test cannot capture in process.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::{ExtractOptions, PdfDocument};

const BODY_SIZE: f64 = 10.0;

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

fn stream_of(pieces: &[Piece], rulings: &[(f64, f64, f64, f64)], invisible: bool) -> Vec<u8> {
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
        if invisible {
            operations.push(Operation::new("Tr", vec![3.into()]));
        }
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

/// A real ruled word table: dense, aligned columns, full lattice, no
/// caption anywhere near it, so every veto leaves it alone.
fn real_table_pdf(invisible: bool) -> Vec<u8> {
    let xs = [60.0, 200.0, 300.0, 340.0];
    let ys = [709.0, 691.0, 673.0, 655.0, 637.0];
    let rows = [
        (["Product", "Weight", "Price"], 700.0),
        (["Apples", "12", "2.50"], 682.0),
        (["Pears", "8", "1.75"], 664.0),
        (["Plums", "20", "3.00"], 646.0),
    ];
    let mut pieces = Vec::new();
    for (cells, y) in rows {
        for (cell, x) in cells.into_iter().zip([xs[0], xs[1], xs[2]]) {
            pieces.push(text(cell, x, y));
        }
    }
    let mut rulings = Vec::new();
    for &y in &ys {
        rulings.push((xs[0], y, xs[3], y));
    }
    for &x in &xs {
        rulings.push((x, ys[4], x, ys[0]));
    }
    let mut doc = Document::with_version("1.7");
    let regular = font(&mut doc);
    let content_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        stream_of(&pieces, &rulings, invisible),
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

/// The child: one extraction on stderr, steered by
/// `PDFLUENT_TABLE_TRACE_FEATURES_SELFTEST`.
#[test]
fn table_trace_features_selftest_child() {
    let invisible = match std::env::var("PDFLUENT_TABLE_TRACE_FEATURES_SELFTEST").as_deref() {
        Ok("real") => Some(false),
        Ok("ocr") => Some(true),
        _ => None,
    };
    if let Some(invisible) = invisible {
        let doc = PdfDocument::open(real_table_pdf(invisible)).expect("open pdf");
        let _ = doc
            .extract_blocks(&ExtractOptions::default())
            .expect("extract blocks");
    }
}

/// This binary as a child, with the trace variable set or removed.
fn child_stderr(trace_on: bool, mode: &str) -> String {
    let exe = std::env::current_exe().expect("current test binary");
    let mut cmd = std::process::Command::new(exe);
    cmd.arg("table_trace_features_selftest_child")
        .arg("--exact")
        .arg("--nocapture")
        .env("PDFLUENT_TABLE_TRACE_FEATURES_SELFTEST", mode);
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

/// One `key=value` field of a `TABLE` line, by key.
fn field<'a>(line: &'a str, key: &str) -> &'a str {
    let needle = format!("{key}=");
    let at = line
        .find(&needle)
        .unwrap_or_else(|| panic!("no {key} in: {line}"));
    let rest = &line[at + needle.len()..];
    rest.split(' ').next().expect("field value")
}

/// All eight candidate flags, in line order.
const CANDIDATES: &[&str] = &[
    "cand_forced_noruling",
    "cand_forced_lowjudge",
    "cand_ocr_drawings",
    "cand_unaligned_cols",
    "cand_drawing_dense",
    "cand_single_strip",
    "cand_wordless",
    "cand_ocr_only",
];

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
    let banned = ["Product", "Weight", "Apples", "Pears", "Plums"];
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
fn the_feature_trace_is_off_by_default() {
    let stderr = child_stderr(false, "real");
    assert!(
        table_lines(&stderr).is_empty(),
        "trace on with the variable unset: {stderr}"
    );
}

#[test]
fn a_real_table_trips_no_candidate_rule() {
    let stderr = child_stderr(true, "real");
    let lines = table_lines(&stderr);
    assert_eq!(
        lines.len(),
        1,
        "one real table reads as one TABLE line: {stderr}"
    );
    let line = lines[0];
    // Every candidate rule reads 0 on the real table.
    for flag in CANDIDATES {
        assert_eq!(field(line, flag), "0", "real table trips {flag}: {line}");
    }
    // And the features behind them are populated, not `-`.
    assert!(
        field(line, "rules_h").parse::<usize>().unwrap_or(0) >= 2,
        "no horizontal rulings behind the real table: {line}"
    );
    assert!(
        field(line, "rules_v").parse::<usize>().unwrap_or(0) >= 2,
        "no vertical rulings behind the real table: {line}"
    );
    assert!(
        field(line, "ruled_frac").parse::<f64>().unwrap_or(0.0) > 0.5,
        "real table reads unruled: {line}"
    );
    assert_eq!(
        field(line, "ocr_text"),
        "0.00",
        "visible text reads as OCR: {line}"
    );
    assert!(
        field(line, "fonts").parse::<usize>().unwrap_or(0) >= 1,
        "no font behind the real table: {line}"
    );
    assert!(
        field(line, "num_frac").parse::<f64>().unwrap_or(0.0) > 0.0,
        "no numeric cell in the real table: {line}"
    );
    assert!(
        field(line, "word_frac").parse::<f64>().unwrap_or(0.0) > 0.0,
        "no word cell in the real table: {line}"
    );
    assert!(
        field(line, "cols_aligned").parse::<f64>().unwrap_or(0.0) >= 0.5,
        "aligned columns read unaligned: {line}"
    );
    assert_eq!(field(line, "page_tables"), "1", "page table count: {line}");
    assert_eq!(
        field(line, "drawing_ops"),
        "0",
        "drawings behind the real table: {line}"
    );
    assert_numbers_only(&lines);
}

#[test]
fn invisible_text_reads_as_ocr_text() {
    let stderr = child_stderr(true, "ocr");
    let lines = table_lines(&stderr);
    assert_eq!(lines.len(), 1, "one table line expected: {stderr}");
    let line = lines[0];
    assert_eq!(
        field(line, "ocr_text"),
        "1.00",
        "render mode 3 text not read as OCR: {line}"
    );
    assert_eq!(field(line, "cand_ocr_only"), "1", "{line}");
}
