// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A prose line carrying numbers under a real table stays a paragraph (#636).
//!
//! The edge-row reclaim adopts a paragraph only where every visual line
//! opens with a marker in the table's marker column: this line opens with
//! prose, so the amount and checkbox paths never even run, and its `42`
//! stands mid-page, nowhere near the table's amount column. A line-leading
//! marker would not reach this test on a clean grid -- the earlier passes
//! absorb a marker-led line under an accepted table themselves -- so the
//! operator page carries its numbers embedded in prose, which is the shape
//! that does reach the reclaim and must still stay out.
//!
//! The line is a fragment on purpose: a trailing period would read as a
//! sentence to the table-edge peel, which demotes lone edge rows back to
//! prose. That backstop stays as defence in depth, but the test pins the
//! reclaim's own guards, so it uses a shape the peel keeps.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::{BlockKind, ExtractOptions, PdfDocument};

fn pdf_of(text: &[(String, f64, f64)], lines: &[(f64, f64, f64, f64)]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    }));
    let mut operations = Vec::new();
    for (piece, x, y) in text {
        operations.push(Operation::new("BT", vec![]));
        operations.push(Operation::new(
            "Tf",
            vec![Object::Name(b"F1".to_vec()), 10.0.into()],
        ));
        operations.push(Operation::new("Td", vec![(*x).into(), (*y).into()]));
        operations.push(Operation::new(
            "Tj",
            vec![Object::String(
                piece.clone().into_bytes(),
                StringFormat::Literal,
            )],
        ));
        operations.push(Operation::new("ET", vec![]));
    }
    for (x, y, w, h) in lines {
        operations.push(Operation::new(
            "re",
            vec![(*x).into(), (*y).into(), (*w).into(), (*h).into()],
        ));
        operations.push(Operation::new("S", vec![]));
    }
    let content = Content { operations }.encode().expect("encode content");
    let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dictionary! {
                "F1" => Object::Reference(font),
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

/// Five ruled rows over a prose line carrying numbers, through the public
/// path with the default options.
fn page_blocks() -> Vec<pdf_engine::Block> {
    let mut text: Vec<(String, f64, f64)> = Vec::new();
    let top = 740.0;
    for (i, label) in ["Apples", "Pears", "Plums", "Figs", "Dates"]
        .iter()
        .enumerate()
    {
        let y = top - i as f64 * 16.0;
        text.push(((i + 1).to_string(), 72.0, y));
        text.push((label.to_string(), 110.0, y));
        text.push((format!("{}", 100 * (i + 1)), 480.0, y));
    }
    let bottom = top - 4.0 * 16.0;
    let note = bottom - 7.0 - 10.0;
    text.push((
        "See note 9 for the adjustment, total 42".to_string(),
        72.0,
        note,
    ));
    let mut lines: Vec<(f64, f64, f64, f64)> = Vec::new();
    for row in 0..=5 {
        lines.push((60.0, top + 12.0 - row as f64 * 16.0, 460.0, 0.5));
    }
    for x in [60.0, 100.0, 450.0, 520.0] {
        lines.push((x, bottom - 4.0, 0.5, 5.0 * 16.0 + 16.0));
    }
    let document = PdfDocument::open(pdf_of(&text, &lines)).expect("open synthetic pdf");
    document
        .extract_blocks(&ExtractOptions::default())
        .expect("extract with the default options")
}

/// The table above is accepted, so the reclaim had a table to join: five
/// rows of marker, stub and amount cells.
#[test]
fn the_table_above_is_accepted() {
    let blocks = page_blocks();
    let last = blocks
        .iter()
        .filter(|block| {
            matches!(block.kind, BlockKind::TableCell { row: 4, .. }) && block.text == "500"
        })
        .count();
    assert_eq!(last, 1, "the fifth amount cell reads as a cell");
}

/// The numbered prose line under the table is not pulled in: its amount
/// stands nowhere near the amount column, and no checkbox ruling stands on
/// its band.
#[test]
fn the_numbered_line_under_the_table_stays_a_paragraph() {
    let blocks = page_blocks();
    let line = blocks
        .iter()
        .find(|block| block.text.contains("total 42"))
        .expect("the numbered line reads");
    assert!(
        matches!(line.kind, BlockKind::Paragraph),
        "the line stays a paragraph, got {:?}: {}",
        line.kind,
        line.text
    );
}
