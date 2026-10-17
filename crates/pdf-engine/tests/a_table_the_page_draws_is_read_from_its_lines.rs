//! A table the page draws as a grid is read from the grid (#574).
//!
//! The page below draws a table the way a gazette or a timetable does: a line
//! between every row and every column, a box round the lot, and a head that
//! spans all three columns because its inner lines stop under it. The text in
//! its cells is long and wraps, which is the shape the text alone reads worst.
//! Take the lattice out (`learned::lattice`, or the rulings the text device
//! records) and the table comes back as paragraphs or as a grid that splits
//! each wrapped cell into rows.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::{BlockKind, ExtractOptions, PdfDocument};

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

fn pdf() -> Vec<u8> {
    // Columns at 72 | 172 | 372 | 540; rows at 700 | 680 | 640 | 600 | 560.
    let xs = [72.0, 172.0, 372.0, 540.0];
    let ys = [700.0, 680.0, 640.0, 600.0, 560.0];
    let mut ops = Vec::new();
    for &y in &ys {
        ops.extend(line(xs[0], y, xs[3], y));
    }
    ops.extend(line(xs[0], ys[4], xs[0], ys[0]));
    ops.extend(line(xs[3], ys[4], xs[3], ys[0]));
    // The inner columns stop under the head: it spans all three.
    ops.extend(line(xs[1], ys[4], xs[1], ys[1]));
    ops.extend(line(xs[2], ys[4], xs[2], ys[1]));
    ops.extend(text("Feed materials of plant origin", 76.0, 686.0));
    let rows = [
        (
            "1.1.1",
            "Barley",
            "Grains of Hordeum vulgare L.",
            "It may be",
        ),
        (
            "1.1.2",
            "Barley, puffed",
            "Product obtained from milled",
            "or broken barley",
        ),
        (
            "1.1.3",
            "Barley, toasted",
            "Product of barley roasting",
            "partially burnt",
        ),
    ];
    for (k, (a, b, c, d)) in rows.iter().enumerate() {
        let top = ys[k + 1] - 14.0;
        ops.extend(text(a, 76.0, top));
        ops.extend(text(b, 76.0, top - 12.0));
        ops.extend(text(c, 176.0, top));
        ops.extend(text(d, 376.0, top));
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

#[test]
fn a_drawn_grid_is_a_table_with_the_cells_its_lines_make() {
    let blocks = PdfDocument::open(pdf())
        .expect("open")
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    let cells: Vec<(u16, u16, u16, u16, String)> = blocks
        .iter()
        .filter_map(|b| match b.kind {
            BlockKind::TableCell {
                row,
                column,
                row_span,
                column_span,
                ..
            } => Some((row, column, row_span, column_span, b.text.clone())),
            _ => None,
        })
        .collect();
    assert!(
        cells.contains(&(0, 0, 1, 3, "Feed materials of plant origin".to_string())),
        "the head is not one cell across the three columns: {cells:?}"
    );
    assert!(
        cells.contains(&(1, 0, 1, 1, "1.1.1 Barley".to_string())),
        "a wrapped cell came apart: {cells:?}"
    );
    assert!(
        cells.contains(&(3, 2, 1, 1, "partially burnt".to_string())),
        "the last cell is not where the lines put it: {cells:?}"
    );
}
