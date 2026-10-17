//! A wrapped cell's continuation joins its row -- with geometry and a cap (#574).
//!
//! End to end, on synthesized ruled tables: the page draws a full grid, so the
//! lattice numbers one row per drawn line and the wrap's continuation starts
//! life as a row of its own. Through `PdfDocument::extract_blocks`, which
//! ships the join on:
//!
//! * a 3-column table whose middle cells wrap onto a second line starting
//!   uppercase reads one row per item (the lexical run-on would not fire: the
//!   continuation starts uppercase and its head ends on no run-on word);
//! * a table with blank stubs whose rows sit at row spacing keeps its rows:
//!   the candidate pair satisfies every clause but the geometry guard, which
//!   is what holds it apart;
//! * a chain of 4 continuation lines under one stub body row joins at most 2:
//!   the cap, not the geometry, stops the third;
//! * a stub-empty row holding only a figure stays a row of its own;
//! * a continuation-looking row directly under the grid's first row stands:
//!   the first row holds the header by position;
//! * grouped stub rows holding complete values under balanced heads stay rows
//!   of their own.
//!
//! The join test and the cap test are red with the rule taken out (drawn rows
//! stay split); the guard test is red with the guard taken out (the wide pair
//! joins); the cap test is red with the cap taken out (all four join); the
//! figure test is red with the figure clause taken out (the figure joins);
//! the first-row test is red with the header-by-position rule taken out; the
//! grouped-rows test is red with the complete-row rule taken out.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::{to_markdown, Block, BlockKind, ExtractOptions, PdfDocument};

/// A thin line: a filled rectangle 0.5 pt across, as producers draw rulings.
fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Operation> {
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

fn text(body: &str, x: f64, y: f64) -> Vec<Operation> {
    vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 9.into()]),
        Operation::new("Td", vec![x.into(), y.into()]),
        Operation::new(
            "Tj",
            vec![Object::String(
                body.as_bytes().to_vec(),
                StringFormat::Literal,
            )],
        ),
        Operation::new("ET", vec![]),
    ]
}

/// A ruled table: `xs` the vertical edges, `ys` the horizontal edges from the
/// top, `rows` the cell texts (`""` leaves the cell empty). Each text sits on
/// a baseline 5 pt above its row's bottom edge.
fn pdf_of(xs: &[f64], ys: &[f64], rows: &[Vec<&str>]) -> Vec<u8> {
    assert_eq!(ys.len(), rows.len() + 1);
    let mut ops = Vec::new();
    for &y in ys {
        ops.extend(line(xs[0], y, xs[xs.len() - 1], y));
    }
    for &x in xs {
        ops.extend(line(x, ys[ys.len() - 1], x, ys[0]));
    }
    for (r, row) in rows.iter().enumerate() {
        let y = ys[r + 1] + 5.0;
        for (c, body) in row.iter().enumerate() {
            if !body.is_empty() {
                ops.extend(text(body, xs[c] + 4.0, y));
            }
        }
    }
    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    }));
    let content = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        Content { operations: ops }.encode().expect("encode"),
    )));
    let page = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => Object::Reference(content),
        "Resources" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dictionary! { "F1" => Object::Reference(font) }),
        }),
    }));
    let pages = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![Object::Reference(page)]),
        "Count" => Object::Integer(1),
    }));
    if let Ok(Object::Dictionary(p)) = doc.get_object_mut(page) {
        p.set("Parent", Object::Reference(pages));
    }
    let catalog = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages),
    }));
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save");
    out
}

/// `(row, column, text)` of every cell, in block order.
fn grid_of(blocks: &[Block]) -> Vec<(u16, u16, String)> {
    blocks
        .iter()
        .filter_map(|b| match b.kind {
            BlockKind::TableCell { row, column, .. } => Some((row, column, b.text.clone())),
            _ => None,
        })
        .collect()
}

fn blocks_of(pdf: &[u8]) -> Vec<Block> {
    PdfDocument::open(pdf.to_vec())
        .expect("open")
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks")
}

const XS: [f64; 4] = [72.0, 190.0, 390.0, 470.0];

#[test]
fn a_wrapped_middle_cell_starting_uppercase_joins_its_row() {
    // Five drawn rows: a header and two items whose middle cell wraps onto a
    // second line starting uppercase. The lexical run-on does not fire on
    // either continuation (`Steel Screws`, `Brass Fittings`), so without the
    // wrapped-row join this reads five rows.
    let ys = [700.0, 684.0, 668.0, 652.0, 636.0, 620.0];
    let pdf = pdf_of(
        &XS,
        &ys,
        &[
            vec!["Item", "Description", "Qty"],
            vec!["Widget A", "Stainless steel", "12"],
            vec!["", "Screws", ""],
            vec!["Widget B", "Brass", "8"],
            vec!["", "Fittings", ""],
        ],
    );
    let blocks = blocks_of(&pdf);
    let grid = grid_of(&blocks);
    let markdown = to_markdown(&blocks);
    assert_eq!(
        grid,
        vec![
            (0, 0, "Item".to_string()),
            (0, 1, "Description".to_string()),
            (0, 2, "Qty".to_string()),
            (1, 0, "Widget A".to_string()),
            (1, 1, "Stainless steel Screws".to_string()),
            (1, 2, "12".to_string()),
            (2, 0, "Widget B".to_string()),
            (2, 1, "Brass Fittings".to_string()),
            (2, 2, "8".to_string()),
        ],
        "one row per item:\n{markdown}"
    );
}

#[test]
fn a_blank_stub_row_at_row_spacing_stays_a_row_of_its_own() {
    // Row 2 is stub-empty under the stubbed body row 1, holds no figure, and
    // row 1 has taken no continuation: every clause but the geometry guard
    // passes. Its gap (a 30 pt step) stands past 1.2x the table's pitch,
    // which the four tight stubbed rows below set at 14 pt -- so it stays
    // separate, and with the guard taken out it joins. It sits under row 1,
    // not row 0, so the header-by-position rule is not what holds it apart.
    let ys = [
        700.0, 684.0, 654.0, 624.0, 594.0, 580.0, 566.0, 552.0, 538.0,
    ];
    let pdf = pdf_of(
        &XS,
        &ys,
        &[
            vec!["Table", "Note", "-"],
            vec!["Group A", "Note", "-"],
            vec!["", "Alpha reading", ""],
            vec!["Item one", "First value", "11"],
            vec!["Item two", "Second value", "22"],
            vec!["Item three", "Third value", "33"],
            vec!["Item four", "Fourth value", "44"],
            vec!["Item five", "Fifth value", "55"],
        ],
    );
    let blocks = blocks_of(&pdf);
    let grid = grid_of(&blocks);
    let markdown = to_markdown(&blocks);
    let rows = grid.iter().map(|(r, _, _)| *r).max().unwrap_or(0) + 1;
    assert_eq!(rows, 8, "no row joins:\n{markdown}");
    assert!(
        grid.contains(&(1, 0, "Group A".to_string())),
        "row 1 kept its stub:\n{markdown}"
    );
    assert!(
        grid.contains(&(2, 1, "Alpha reading".to_string())),
        "row 2 stayed a row of its own:\n{markdown}"
    );
}

#[test]
fn a_continuation_looking_row_directly_under_the_first_row_stands() {
    // Row 1 is stub-empty under a stubbed row 0, holds no figure, sits at
    // the line's own leading, and row 0 holds one line: everything but the
    // header-by-position rule passes, so it stays separate -- and with that
    // rule taken out it joins. Row 0 reads as data (`9` in the last cell),
    // so no header flag is what holds it apart.
    let ys = [700.0, 684.0, 668.0, 652.0];
    let pdf = pdf_of(
        &XS,
        &ys,
        &[
            vec!["Gadget X", "Premium line", "9"],
            vec!["", "Extension", ""],
            vec!["Widget Z", "Standard", "5"],
        ],
    );
    let blocks = blocks_of(&pdf);
    let grid = grid_of(&blocks);
    let markdown = to_markdown(&blocks);
    let rows = grid.iter().map(|(r, _, _)| *r).max().unwrap_or(0) + 1;
    assert_eq!(rows, 3, "the first-row pair stays separate:\n{markdown}");
    assert!(
        grid.contains(&(1, 1, "Extension".to_string())),
        "row 1 stayed a row of its own:\n{markdown}"
    );
}

#[test]
fn a_continuation_holding_only_a_figure_stays_a_row_of_its_own() {
    // Row 2 is stub-empty under a stubbed body row and sits at the line's own
    // leading, but it holds a figure (`12%`): figures are no continuation, so
    // it stays separate, and with that clause taken out it joins.
    let ys = [700.0, 684.0, 668.0, 652.0];
    let pdf = pdf_of(
        &XS,
        &ys,
        &[
            vec!["Item", "Description", "Qty"],
            vec!["Widget C", "Copper", "15"],
            vec!["", "12%", ""],
        ],
    );
    let blocks = blocks_of(&pdf);
    let grid = grid_of(&blocks);
    let markdown = to_markdown(&blocks);
    let rows = grid.iter().map(|(r, _, _)| *r).max().unwrap_or(0) + 1;
    assert_eq!(rows, 3, "the figure row stays separate:\n{markdown}");
    assert!(
        grid.contains(&(2, 1, "12%".to_string())),
        "the figure stayed a row of its own:\n{markdown}"
    );
}

#[test]
fn grouped_stub_rows_with_complete_values_stay_rows_of_their_own() {
    // Rows 2 and 3 read like continuations -- stub-empty under a stubbed body
    // row, tight, no figures -- but each holds a complete row of independent
    // values under balanced heads, so the complete-row rule holds them apart,
    // and with that rule taken out they join. Row 4's filled stub is not
    // what stops anything above it.
    let ys = [700.0, 684.0, 668.0, 652.0, 636.0, 620.0];
    let pdf = pdf_of(
        &XS,
        &ys,
        &[
            vec!["Variable", "Category", "Count"],
            vec!["1Age", "0-2years", "30(43)"],
            vec!["", "2-5years", "19(28)"],
            vec!["", "5 above", "20(29)"],
            vec!["2Sex", "Female", "37(54)"],
        ],
    );
    let blocks = blocks_of(&pdf);
    let grid = grid_of(&blocks);
    let markdown = to_markdown(&blocks);
    let rows = grid.iter().map(|(r, _, _)| *r).max().unwrap_or(0) + 1;
    assert_eq!(rows, 5, "no grouped row joins:\n{markdown}");
    assert!(
        grid.contains(&(2, 1, "2-5years".to_string())),
        "row 2 stayed a row of its own:\n{markdown}"
    );
    assert!(
        grid.contains(&(3, 1, "5 above".to_string())),
        "row 3 stayed a row of its own:\n{markdown}"
    );
}

#[test]
fn a_chain_of_four_continuations_joins_at_most_two() {
    // A stub body row and four tight continuation lines, each satisfying
    // every clause. The stub row takes the first two and the cap stops the
    // third; the last two stand as rows of their own (stub-empty under
    // stub-empty). Without the join this reads six rows; without the cap it
    // reads two. The stub sits at row 1 so the header-by-position rule is
    // not what stops anything.
    let ys = [700.0, 684.0, 668.0, 652.0, 636.0, 620.0, 604.0];
    let pdf = pdf_of(
        &XS,
        &ys,
        &[
            vec!["Item", "Description", "Qty"],
            vec!["Gadget B", "Premium", "7"],
            vec!["", "Extension One", ""],
            vec!["", "Extension Two", ""],
            vec!["", "Extension Three", ""],
            vec!["", "Extension Four", ""],
        ],
    );
    let blocks = blocks_of(&pdf);
    let grid = grid_of(&blocks);
    let markdown = to_markdown(&blocks);
    let rows = grid.iter().map(|(r, _, _)| *r).max().unwrap_or(0) + 1;
    assert_eq!(rows, 4, "two continuations join, two stand:\n{markdown}");
    assert!(
        grid.contains(&(1, 1, "Premium Extension One Extension Two".to_string())),
        "the stub row took exactly two:\n{markdown}"
    );
    assert!(
        grid.contains(&(2, 1, "Extension Three".to_string())),
        "the third continuation stands:\n{markdown}"
    );
    assert!(
        grid.contains(&(3, 1, "Extension Four".to_string())),
        "the fourth continuation stands:\n{markdown}"
    );
}
