//! A table is stitched across its wrapped labels and header rows (#574).
//!
//! End to end, on a synthesized financial statement: a year header over three
//! figure columns, figure rows, a row label wrapped over three lines with the
//! figures on its last, a section label row without figures, and a note
//! paragraph under it all. The statement reads as one table with the year
//! header as its first row; the title above it and the notes below stay prose.
//!
//! The absorb switches have no end-to-end operator test on purpose: on a clean
//! synthetic page the judge takes the whole statement itself, so the year
//! header and the wrap join whatever the switches say. Their operator tests
//! live beside the pass (`learned::stitch`), where chosen sets arrive already
//! cut and every absorb test is red with its switch off. What this page pins
//! is the end-to-end shape: with the absorb on, the year header is the
//! table's first row and the title above it stays prose.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::learned::{document_facts, page_blocks, refine_page, Switches, SWITCHES};
use pdf_engine::{to_markdown, Block, BlockKind, PdfDocument};

const SIZE: f64 = 9.0;
const PITCH: f64 = 12.0;
const STUB: f64 = 72.0;
const FIGS: [f64; 3] = [360.0, 430.0, 500.0];

struct Piece {
    text: String,
    x: f64,
    y: f64,
}

fn at(text: &str, x: f64, y: f64) -> Piece {
    Piece {
        text: text.to_string(),
        x,
        y,
    }
}

fn pdf_of(pieces: &[Piece]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    }));
    let mut operations = Vec::new();
    for piece in pieces {
        operations.push(Operation::new("BT", vec![]));
        operations.push(Operation::new(
            "Tf",
            vec![Object::Name(b"F1".to_vec()), SIZE.into()],
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
    let content = Content { operations }.encode().expect("encode content");
    let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 792.into(), 792.into()]),
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

/// How many tables the serializer writes, read the way every consumer reads a
/// run of cells: a new table wherever `(row, column)` stops climbing.
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

fn statement() -> Vec<Piece> {
    let mut pieces = vec![at("Consolidated Statements of Income", STUB, 740.0)];
    let mut y = 716.0;
    pieces.push(at("Fiscal Year", STUB, y));
    for (x, year) in FIGS.iter().zip(["2003", "2002", "2001"]) {
        pieces.push(at(year, *x, y));
    }
    y -= PITCH;
    for (label, figs) in [
        ("Net sales", ["9,898", "2,240", "2,598"]),
        ("Cost of sales", ["1,111", "2,222", "3,333"]),
        ("Gross profit", ["4,444", "5,555", "6,666"]),
    ] {
        pieces.push(at(label, STUB, y));
        for (x, fig) in FIGS.iter().zip(figs) {
            pieces.push(at(fig, *x, y));
        }
        y -= PITCH;
    }
    // A row label wrapped over three lines, figures on the last.
    for label in [
        "Add: stock-based compensation",
        "expense included in reported",
        "net income, net of tax",
    ] {
        pieces.push(at(label, STUB, y));
        y -= PITCH;
    }
    y += PITCH;
    for (x, fig) in FIGS.iter().zip(["7,777", "8,888", "9,999"]) {
        pieces.push(at(fig, *x, y));
    }
    y -= PITCH;
    pieces.push(at("Deferred income taxes:", STUB, y));
    y -= PITCH;
    for (label, figs) in [
        ("Current", ["1,010", "2,020", "3,030"]),
        ("Deferred", ["4,040", "5,050", "6,060"]),
    ] {
        pieces.push(at(label, STUB, y));
        for (x, fig) in FIGS.iter().zip(figs) {
            pieces.push(at(fig, *x, y));
        }
        y -= PITCH;
    }
    y -= PITCH;
    pieces.push(at(
        "The accompanying notes are an integral part of these statements.",
        STUB,
        y,
    ));
    pieces.push(at(
        "All amounts are in thousands of dollars.",
        STUB,
        y - PITCH,
    ));
    pieces
}

/// With both absorb switches on (both ship on): the year header is the
/// table's first row, the title stays outside it.
#[test]
fn the_year_header_joins_but_the_title_does_not() {
    let doc = PdfDocument::open(pdf_of(&statement())).expect("open synthetic pdf");
    let all = doc.extract_blocks_before_learning().expect("blocks");
    let facts = document_facts(&all);
    let blocks = refine_page(
        page_blocks(&all, 0),
        (792.0, 792.0),
        &facts,
        Switches {
            table_stitch_header: true,
            table_stitch_rows: true,
            ..SWITCHES
        },
    );
    let markdown = to_markdown(&blocks);
    assert_eq!(
        tables_in(&blocks),
        1,
        "one statement, one table:\n{markdown}"
    );
    // The wrap, the section row, and the year header are cells of it.
    for needle in [
        "Add: stock-based compensation",
        "expense included in reported",
        "net income, net of tax",
        "Deferred income taxes:",
        "Fiscal Year",
        "2003",
    ] {
        assert!(
            blocks
                .iter()
                .any(|b| matches!(b.kind, BlockKind::TableCell { .. }) && b.text.contains(needle)),
            "{needle:?} is not a cell of the table:\n{markdown}"
        );
    }
    // The year header is the table's first row; the title above it is prose.
    // It stands in the body's own type at the stub, which the old label rule
    // could not tell from a section label -- upward the pass takes header
    // rows only now, and a stub-only line caps the table.
    let first = blocks
        .iter()
        .find(|b| matches!(b.kind, BlockKind::TableCell { .. }))
        .expect("a table is found");
    assert!(
        first.text.contains("Fiscal Year") || first.text.contains("2003"),
        "the table does not start at the year header:\n{markdown}"
    );
    let title = blocks
        .iter()
        .find(|b| b.text.contains("Consolidated Statements of Income"))
        .expect("the title is read");
    assert!(
        !matches!(title.kind, BlockKind::TableCell { .. }),
        "the title joined the table:\n{markdown}"
    );
    // The notes under the table stay prose.
    let note = blocks
        .iter()
        .find(|b| b.text.contains("accompanying notes"))
        .expect("the note is read");
    assert!(matches!(note.kind, BlockKind::Paragraph), "{markdown}");
}
