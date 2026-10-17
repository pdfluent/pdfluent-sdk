// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Operator-built pages shared by the rule A tests (#636): the default path
//! (`rule_a_false_tables.rs`) and the balanced tier (`rule_a_balanced.rs`).

#![allow(dead_code)]

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::{Block, BlockKind, ExtractOptions, PdfDocument};

const BODY_SIZE: f64 = 10.0;

/// One run of glyphs at a point.
pub struct Piece {
    pub text: String,
    pub x: f64,
    pub y: f64,
}

pub fn text(body: &str, x: f64, y: f64) -> Piece {
    Piece {
        text: body.to_string(),
        x,
        y,
    }
}

/// A filled blob of three overlapping triangles, 24 points each way: one
/// drawn cluster (three paths that touch), the way a structure drawing or a
/// chart mark reads. Not a ruling: every path is thick both ways.
pub fn blob(x: f64, y: f64) -> Vec<Operation> {
    let mut ops = Vec::new();
    for dx in [0.0, 6.0, 12.0] {
        ops.push(Operation::new("m", vec![(x + dx).into(), y.into()]));
        ops.push(Operation::new("l", vec![(x + dx + 12.0).into(), y.into()]));
        ops.push(Operation::new(
            "l",
            vec![(x + dx + 6.0).into(), (y + 24.0).into()],
        ));
        ops.push(Operation::new("h", vec![]));
        ops.push(Operation::new("f", vec![]));
    }
    ops
}

/// A thin ruling: a filled rectangle 0.5 pt across, as producers draw them.
pub fn ruling(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Operation> {
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

pub fn pdf_of(pieces: &[Piece], drawing: Vec<Operation>) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let regular = Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    })));
    let mut operations = drawing;
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
    let content_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        Content { operations }.encode().expect("encode content"),
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

pub fn blocks_of(bytes: Vec<u8>) -> Vec<Block> {
    PdfDocument::open(bytes)
        .expect("open pdf")
        .extract_blocks(&ExtractOptions::default())
        .expect("extract blocks")
}

pub fn cells_of(blocks: &[Block]) -> Vec<&Block> {
    blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::TableCell { .. }))
        .collect()
}

/// Block texts in the order the blocks stand.
pub fn texts_of(blocks: &[Block]) -> Vec<&str> {
    blocks.iter().map(|b| b.text.as_str()).collect()
}

/// A grid the learned pass forces on a low judge number: seven digit and
/// symbol cells in two columns, no caption, no ruling, no word. The judge
/// reads it at 0.32 and the pass forces the region because it is not one of
/// the builder's tables.
pub fn forced_low_judge_wordless() -> Vec<Piece> {
    vec![
        text("-4", 73.0, 700.0),
        text("5", 73.0, 684.0),
        text("-10", 171.0, 684.0),
        text("-2", 73.0, 668.0),
        text("12", 171.0, 668.0),
        text("-10", 73.0, 652.0),
        text("Si", 171.0, 652.0),
    ]
}

/// The same kind of grid, forced on a judge number of 0.86.
pub fn forced_high_judge() -> Vec<Piece> {
    vec![
        text("0", 81.0, 700.0),
        text("8", 263.0, 700.0),
        text("-4", 263.0, 684.0),
        text("I", 81.0, 668.0),
        text("-10", 81.0, 652.0),
        text("CH", 263.0, 652.0),
        text("-6", 81.0, 636.0),
        text("OH", 163.0, 636.0),
        text("I", 263.0, 636.0),
        text("14", 81.0, 620.0),
        text("OH", 163.0, 620.0),
        text("-4", 263.0, 620.0),
    ]
}

/// A word table with a header, three columns and a wide white gutter between
/// the first and the second.
pub fn word_table() -> Vec<Piece> {
    let rows = [
        ["Product", "Weight", "Price"],
        ["Apples", "12", "2.50"],
        ["Pears", "8", "1.75"],
        ["Plums", "20", "3.00"],
        ["Grapes", "15", "4.25"],
        ["Lemons", "9", "0.95"],
    ];
    let mut pieces = Vec::new();
    for (r, row) in rows.iter().enumerate() {
        for (cell, x) in row.iter().zip([60.0, 300.0, 400.0]) {
            pieces.push(text(cell, x, 700.0 - 18.0 * r as f64));
        }
    }
    pieces
}

/// The word table with `n` drawn clusters standing in its gutter, beside the
/// lower rows.
pub fn word_table_over(n: usize) -> Vec<u8> {
    let mut drawing = Vec::new();
    for k in 0..n {
        drawing.extend(blob(120.0 + 40.0 * k as f64, 640.0));
    }
    pdf_of(&word_table(), drawing)
}

/// A financial table: a word header over amounts, nothing drawn.
pub fn financial_table() -> Vec<Piece> {
    let rows = [
        ["Item", "2023", "2024"],
        ["Revenue", "1,234.50", "1,456.75"],
        ["Costs", "(890.00)", "(912.30)"],
        ["Profit", "344.50", "544.45"],
        ["Taxes", "80.10", "120.40"],
        ["Net income", "264.40", "424.05"],
    ];
    let mut pieces = Vec::new();
    for (r, row) in rows.iter().enumerate() {
        for (cell, x) in row.iter().zip([72.0, 300.0, 400.0]) {
            pieces.push(text(cell, x, 700.0 - 18.0 * r as f64));
        }
    }
    pieces
}

/// A ruled word table: full lattice, aligned columns.
pub fn ruled_word_table() -> Vec<u8> {
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
    let mut drawing = Vec::new();
    for &y in &ys {
        drawing.extend(ruling(xs[0], y, xs[3], y));
    }
    for &x in &xs {
        drawing.extend(ruling(x, ys[4], x, ys[0]));
    }
    pdf_of(&pieces, drawing)
}

/// The ruled word table with three light-grey shaded areas, each inset three
/// points inside the lattice so they never touch: columns 0 and 2 of the two
/// upper rows, and column 1 of the two lower rows. Each area is three
/// overlapping filled rectangles, one drawn cluster at least 20 points each
/// way: three clusters inside the box (`drawing_dense`), and yet every cell
/// stays bounded by rulings.
pub fn ruled_shaded_table() -> Vec<u8> {
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
    let mut drawing = vec![Operation::new("g", vec![0.85.into()])];
    for (col, top, bottom) in [(0usize, 0usize, 2usize), (2, 0, 2), (1, 2, 4)] {
        let (x0, x1) = (xs[col] + 3.0, xs[col + 1] - 3.0);
        let (y0, y1) = (ys[bottom] + 3.0, ys[top] - 3.0);
        for k in 0..3 {
            let shrink = k as f64;
            drawing.push(Operation::new(
                "re",
                vec![
                    (x0 + shrink).into(),
                    (y0 + shrink).into(),
                    (x1 - x0 - 2.0 * shrink).into(),
                    (y1 - y0 - 2.0 * shrink).into(),
                ],
            ));
            drawing.push(Operation::new("f", vec![]));
        }
    }
    drawing.push(Operation::new("g", vec![0.0.into()]));
    for &y in &ys {
        drawing.extend(ruling(xs[0], y, xs[3], y));
    }
    for &x in &xs {
        drawing.extend(ruling(x, ys[4], x, ys[0]));
    }
    pdf_of(&pieces, drawing)
}
