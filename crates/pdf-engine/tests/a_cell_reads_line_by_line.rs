//! A rebuilt cell reads top line first, left to right within a line (#574).
//!
//! End to end, on synthesized statements shaped like corp-q1-2025 p13: two
//! stacked header lines whose extents touch, so the decoder holds them in one
//! cell, and a wrapped row label the same way. The decoder's within-cell
//! order merges touching lines and sorts by x, so the header cell joins
//! "2025 March 31,"; with `table_cell_text_by_line` on it reads
//! "March 31, 2025", and the wrapped label reads its first line before its
//! wrap. A single-line cell reads exactly as before.
//!
//! All three run with shipped defaults plus `table_stitch_header` and
//! `table_cell_text_by_line` on, through `refine_page` on an operator-built
//! PDF. The first two are red with the line-by-line join removed: the header
//! cell joins "2025 March 31," and the wrapped label interleaves by x. The
//! third pins the no-op: one line has one order.

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
const WIDE_PITCH: f64 = 18.0;
const STUB: f64 = 72.0;
const FIGS: [f64; 3] = [360.0, 430.0, 500.0];

struct Piece {
    text: String,
    x: f64,
    y: f64,
    bold: bool,
}

fn at(text: &str, x: f64, y: f64) -> Piece {
    Piece {
        text: text.to_string(),
        x,
        y,
        bold: false,
    }
}

fn at_bold(text: &str, x: f64, y: f64) -> Piece {
    Piece {
        text: text.to_string(),
        x,
        y,
        bold: true,
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
    let bold = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica-Bold".to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    }));
    let mut operations = Vec::new();
    for piece in pieces {
        operations.push(Operation::new("BT", vec![]));
        operations.push(Operation::new(
            "Tf",
            vec![
                Object::Name(if piece.bold {
                    b"F2".to_vec()
                } else {
                    b"F1".to_vec()
                }),
                SIZE.into(),
            ],
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
                "F2" => Object::Reference(bold),
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

fn blocks_with_lines(pieces: &[Piece]) -> Vec<Block> {
    let doc = PdfDocument::open(pdf_of(pieces)).expect("open synthetic pdf");
    let all = doc.extract_blocks_before_learning().expect("blocks");
    let facts = document_facts(&all);
    refine_page(
        page_blocks(&all, 0),
        (792.0, 792.0),
        &facts,
        Switches {
            table_stitch_header: true,
            table_cell_text_by_line: true,
            ..SWITCHES
        },
    )
}

fn figure_rows(pieces: &mut Vec<Piece>, mut y: f64, rows: &[(&str, [&str; 3])]) -> f64 {
    for (label, figs) in rows {
        pieces.push(at(label, STUB, y));
        for (x, fig) in FIGS.iter().zip(figs.iter()) {
            pieces.push(at(fig, *x, y));
        }
        y -= WIDE_PITCH;
    }
    y
}

/// The corp-q1 shape, on the header-band page: "March 31," over "2025", the
/// two lines' extents touching and "2025" left of both upper words, so x
/// order puts it first. The caption cuts the builder's table so the judge
/// chooses the figure rows, the walk absorbs the band, and the straddling
/// region is rebuilt: the decoder holds all three words in one header cell.
fn statement_with_stacked_header() -> Vec<Piece> {
    let mut pieces = vec![at("Consolidated Balance Sheets", STUB, 752.0)];
    pieces.push(at("March", 382.0, 740.0));
    pieces.push(at("31,", 386.0, 740.0));
    pieces.push(at("December", 445.0, 740.0));
    pieces.push(at("31,", 489.5, 740.0));
    pieces.push(at("(in millions)", STUB, 731.0));
    pieces.push(at("2025", FIGS[0], 731.0));
    pieces.push(at("2024", FIGS[1], 731.0));
    pieces.push(at("Change", 490.0, 731.0));
    pieces.push(at_bold("Liabilities", STUB, 716.0));
    pieces.push(at("Table 2 Net sales", STUB, 704.0));
    let mut y = figure_rows(
        &mut pieces,
        692.0,
        &[
            ("Deposits", ["9,898", "2,240", "2,598"]),
            ("Borrowed funds", ["1,111", "2,222", "3,333"]),
            ("Accounts payable", ["4,444", "5,555", "6,666"]),
        ],
    );
    pieces.push(at_bold("Total liabilities", STUB, y));
    for (x, fig) in FIGS.iter().zip(["7,777", "8,888", "9,999"]) {
        pieces.push(at_bold(fig, *x, y));
    }
    y -= WIDE_PITCH;
    pieces.push(at(
        "The accompanying notes are an integral part of these statements.",
        STUB,
        y,
    ));
    pieces
}

/// The header-band page with a wrapped first body row: the wrap touches the
/// label line and sits under its middle, so x order interleaves the two
/// lines. The body below steps down an extra line so the wrap touches
/// nothing else.
fn statement_with_wrapped_label() -> Vec<Piece> {
    let mut pieces = vec![at("Consolidated Balance Sheets", STUB, 752.0)];
    pieces.push(at("March", 382.0, 740.0));
    pieces.push(at("31,", 410.0, 740.0));
    pieces.push(at("December", 445.0, 740.0));
    pieces.push(at("31,", 489.5, 740.0));
    pieces.push(at("(in millions)", STUB, 728.0));
    pieces.push(at("2025", FIGS[0], 728.0));
    pieces.push(at("2024", FIGS[1], 728.0));
    pieces.push(at("Change", 490.0, 728.0));
    pieces.push(at_bold("Liabilities", STUB, 716.0));
    pieces.push(at("Table 2 Net sales", STUB, 704.0));
    pieces.push(at("Federal funds purchased and", STUB, 692.0));
    for (x, fig) in FIGS.iter().zip(["9,898", "2,240", "2,598"]) {
        pieces.push(at(fig, *x, 692.0));
    }
    pieces.push(at("securities loaned", 120.0, 683.0));
    let mut y = figure_rows(
        &mut pieces,
        665.0,
        &[
            ("Deposits", ["1,111", "2,222", "3,333"]),
            ("Borrowed funds", ["4,444", "5,555", "6,666"]),
        ],
    );
    pieces.push(at_bold("Total liabilities", STUB, y));
    for (x, fig) in FIGS.iter().zip(["7,777", "8,888", "9,999"]) {
        pieces.push(at_bold(fig, *x, y));
    }
    y -= WIDE_PITCH;
    pieces.push(at(
        "The accompanying notes are an integral part of these statements.",
        STUB,
        y,
    ));
    pieces
}

fn cell_texts(blocks: &[Block]) -> Vec<&str> {
    blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::TableCell { .. }))
        .map(|b| b.text.as_str())
        .collect()
}

#[test]
fn a_stacked_header_cell_reads_top_line_first() {
    let blocks = blocks_with_lines(&statement_with_stacked_header());
    let markdown = to_markdown(&blocks);
    assert!(
        cell_texts(&blocks).contains(&"March 31, 2025"),
        "no cell reads the stacked header line by line: {cell_texts:?}:\n{markdown}",
        cell_texts = cell_texts(&blocks),
    );
    assert!(
        markdown.contains("March 31, 2025"),
        "the markdown does not read the stacked header line by line:\n{markdown}"
    );
}

#[test]
fn a_wrapped_row_label_reads_in_order() {
    let blocks = blocks_with_lines(&statement_with_wrapped_label());
    let markdown = to_markdown(&blocks);
    assert!(
        cell_texts(&blocks).contains(&"Federal funds purchased and securities loaned"),
        "no cell reads the wrapped label in order: {cell_texts:?}:\n{markdown}",
        cell_texts = cell_texts(&blocks),
    );
}

#[test]
fn a_single_line_cell_is_unchanged() {
    for pieces in [
        statement_with_stacked_header(),
        statement_with_wrapped_label(),
    ] {
        let blocks = blocks_with_lines(&pieces);
        let markdown = to_markdown(&blocks);
        assert!(
            cell_texts(&blocks).contains(&"9,898"),
            "a single-line figure cell changed: {cell_texts:?}:\n{markdown}",
            cell_texts = cell_texts(&blocks),
        );
    }
}
