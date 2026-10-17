// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A note between two separate tables joins neither (#636).
//!
//! End to end: the note stays a paragraph and the two grids keep their own
//! row numbering, so they still read as two tables rather than one bridged
//! by the note. On a clean grid the earlier passes absorb a between-rows
//! line into a neighbouring row themselves -- the note arrives at the
//! edge-row reclaim already a cell, which the table-edge peel hands back --
//! so this test pins the composition rather than one guard: the reclaim
//! never sees the note, and must join neither table through it.
//!
//! The sixteen-point gaps are load-bearing: closer in, the line reads as a
//! row of one table or the other and never reaches the paragraph state this
//! test asserts.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::{Block, BlockKind, ExtractOptions, PdfDocument};

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

/// How many tables the serializer writes, read the way every consumer reads
/// a run of cells: a new table wherever `(row, column)` stops climbing.
fn tables_in(blocks: &[Block]) -> usize {
    let mut tables = 0;
    let mut last: Option<(u16, u16)> = None;
    for block in blocks {
        match block.kind {
            BlockKind::TableCell { row, column, .. } => {
                if last.is_none_or(|seen| (row, column) <= seen) {
                    tables += 1;
                }
                last = Some((row, column));
            }
            _ => last = None,
        }
    }
    tables
}

fn grid(
    text: &mut Vec<(String, f64, f64)>,
    lines: &mut Vec<(f64, f64, f64, f64)>,
    top: f64,
    labels: &[&str],
    base: usize,
) -> f64 {
    for (i, label) in labels.iter().enumerate() {
        let y = top - i as f64 * 16.0;
        text.push(((i + 1).to_string(), 72.0, y));
        text.push((label.to_string(), 110.0, y));
        text.push((format!("{}", base + 100 * i), 480.0, y));
    }
    let rows = labels.len();
    let bottom = top - (rows as f64 - 1.0) * 16.0;
    for row in 0..=rows {
        lines.push((60.0, top + 12.0 - row as f64 * 16.0, 460.0, 0.5));
    }
    for x in [60.0, 100.0, 450.0, 520.0] {
        lines.push((x, bottom - 4.0, 0.5, rows as f64 * 16.0 + 16.0));
    }
    bottom
}

/// Two ruled grids with a prose note between them, through the public path
/// with the default options.
fn page_blocks() -> Vec<Block> {
    let mut text: Vec<(String, f64, f64)> = Vec::new();
    let mut lines: Vec<(f64, f64, f64, f64)> = Vec::new();
    let first_bottom = grid(
        &mut text,
        &mut lines,
        740.0,
        &["Apples", "Pears", "Plums", "Figs"],
        100,
    );
    let note = first_bottom - 16.0 - 10.0;
    text.push((
        "The notes below explain the figures in full detail.".to_string(),
        72.0,
        note,
    ));
    grid(
        &mut text,
        &mut lines,
        note - 10.0 - 16.0,
        &["Figs", "Dates", "Quince", "Lemons"],
        600,
    );
    let document = PdfDocument::open(pdf_of(&text, &lines)).expect("open synthetic pdf");
    document
        .extract_blocks(&ExtractOptions::default())
        .expect("extract with the default options")
}

/// The note between the tables stays a paragraph: absorbed into a
/// neighbouring row upstream, the table-edge peel hands it back, and no
/// step joins it to either table.
#[test]
fn the_note_between_the_tables_stays_a_paragraph() {
    let blocks = page_blocks();
    let note = blocks
        .iter()
        .find(|block| block.text.contains("notes below"))
        .expect("the note reads");
    assert!(
        matches!(note.kind, BlockKind::Paragraph),
        "the note stays a paragraph, got {:?}: {}",
        note.kind,
        note.text
    );
}

/// The note never bridges the two grids into one table.
#[test]
fn the_two_tables_stay_two() {
    let blocks = page_blocks();
    assert_eq!(tables_in(&blocks), 2, "two grids, two tables");
}
