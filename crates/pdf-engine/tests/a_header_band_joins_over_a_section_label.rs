//! A header band joins over a section label, a bare title stays out (#574).
//!
//! End to end, on synthesized statements shaped like corp-q1-2025 p13: date
//! headers over a bold stub-only section label over figure rows and a bold
//! total row. The upward walk holds that one stub-only line until the header
//! rows join above and commit it: the table's first rows hold the header
//! texts and the section label. Where no header row stands above -- a title
//! over a section label over figure rows -- both lines stay outside the table
//! and its top edge is the first figure row.
//!
//! Both pages carry a caption between the label and the figure rows. That is
//! what cuts the page for the test: the region detector spans any single
//! statement (heads only break it where they repeat) and the verifier prefers
//! the whole statement over a crop, so without the caption the judge takes
//! everything itself and the test stays green with the rule removed. The
//! caption breaks the region pre-learn, the judge chooses the figure rows,
//! and the walk skips the caption on its way up; the caption stays prose.
//!
//! Both tests run with shipped defaults plus `table_stitch_header` on. The
//! first is red with the switch off and red where the walk stops at the
//! label instead of holding it; the second is red where a held line is
//! claimed uncommitted instead of released at the walk's end.

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

fn blocks_with_header(pieces: &[Piece], header: bool) -> Vec<Block> {
    let doc = PdfDocument::open(pdf_of(pieces)).expect("open synthetic pdf");
    let all = doc.extract_blocks_before_learning().expect("blocks");
    let facts = document_facts(&all);
    refine_page(
        page_blocks(&all, 0),
        (792.0, 792.0),
        &facts,
        Switches {
            table_stitch_header: header,
            ..SWITCHES
        },
    )
}

/// The corp-q1 shape: a title, two header rows, a bold stub-only section
/// label, a caption cutting the region, figure rows at a pitch of 18, a bold
/// total row, and a note under it all. The held label stands a row past the
/// set's edge (the skipped caption between), inside the commit pitch at 18.
fn statement_with_header_band() -> Vec<Piece> {
    let mut pieces = vec![at("Consolidated Balance Sheets", STUB, 752.0)];
    // First header row: one group per column, a piece break between them.
    pieces.push(at("March", 382.0, 740.0));
    pieces.push(at("31,", 410.0, 740.0));
    pieces.push(at("December", 445.0, 740.0));
    pieces.push(at("31,", 489.5, 740.0));
    // Second header row: the unit note at the stub, years, and one label.
    pieces.push(at("(in millions)", STUB, 728.0));
    pieces.push(at("2025", FIGS[0], 728.0));
    pieces.push(at("2024", FIGS[1], 728.0));
    pieces.push(at("Change", 490.0, 728.0));
    pieces.push(at_bold("Liabilities", STUB, 716.0));
    pieces.push(at("Table 2 Net sales", STUB, 704.0));
    let mut y = 692.0;
    for (label, figs) in [
        ("Deposits", ["9,898", "2,240", "2,598"]),
        ("Borrowed funds", ["1,111", "2,222", "3,333"]),
        ("Accounts payable", ["4,444", "5,555", "6,666"]),
    ] {
        pieces.push(at(label, STUB, y));
        for (x, fig) in FIGS.iter().zip(figs) {
            pieces.push(at(fig, *x, y));
        }
        y -= WIDE_PITCH;
    }
    pieces.push(at_bold("Total liabilities", STUB, y));
    for (x, fig) in FIGS.iter().zip(["7,777", "8,888", "9,999"]) {
        pieces.push(at_bold(fig, *x, y));
    }
    pieces.push(at(
        "The accompanying notes are an integral part of these statements.",
        STUB,
        y - WIDE_PITCH,
    ));
    pieces
}

/// A title over a section label over figure rows, with no header row above.
/// The caption cuts the region as in the first page; the walk holds the
/// label, stops at the title, and releases the label at its end.
fn statement_with_bare_title() -> Vec<Piece> {
    let mut pieces = vec![at("Regional Results", STUB, 728.0)];
    pieces.push(at_bold("Liabilities", STUB, 716.0));
    pieces.push(at("Table 2 Net sales", STUB, 704.0));
    let mut y = 692.0;
    for (label, figs) in [
        ("Deposits", ["9,898", "2,240", "2,598"]),
        ("Borrowed funds", ["1,111", "2,222", "3,333"]),
        ("Accounts payable", ["4,444", "5,555", "6,666"]),
    ] {
        pieces.push(at(label, STUB, y));
        for (x, fig) in FIGS.iter().zip(figs) {
            pieces.push(at(fig, *x, y));
        }
        y -= PITCH;
    }
    // Short on purpose: the longer note of the first page reaches under the
    // first figure column here and the grid reads the page as two tables.
    pieces.push(at(
        "The accompanying notes are part of these statements.",
        STUB,
        y - 2.0 * PITCH,
    ));
    pieces
}

#[test]
fn header_rows_over_a_section_label_join_the_table() {
    let blocks = blocks_with_header(&statement_with_header_band(), true);
    let markdown = to_markdown(&blocks);
    assert_eq!(
        tables_in(&blocks),
        1,
        "one statement, one table:\n{markdown}"
    );
    // The header texts and the section label are cells of it.
    for needle in [
        "March",
        "December",
        "(in millions)",
        "2025",
        "2024",
        "Change",
        "Liabilities",
        "Total liabilities",
    ] {
        assert!(
            blocks
                .iter()
                .any(|b| matches!(b.kind, BlockKind::TableCell { .. }) && b.text.contains(needle)),
            "{needle:?} is not a cell of the table:\n{markdown}"
        );
    }
    // The table starts at the header band, and no paragraph above it carries
    // the header texts or the section label.
    let first = blocks
        .iter()
        .find(|b| matches!(b.kind, BlockKind::TableCell { .. }))
        .expect("a table is found");
    assert!(
        first.text.contains("March"),
        "the table does not start at the header band:\n{markdown}"
    );
    for needle in [
        "March",
        "December",
        "(in millions)",
        "2025",
        "2024",
        "Change",
        "Liabilities",
    ] {
        assert!(
            !blocks
                .iter()
                .any(|b| matches!(b.kind, BlockKind::Paragraph) && b.text.contains(needle)),
            "{needle:?} stayed a paragraph above the table:\n{markdown}"
        );
    }
    // The title above the band, the caption below the label, and the note
    // below the table stay prose.
    for needle in [
        "Consolidated Balance Sheets",
        "Table 2 Net sales",
        "accompanying notes",
    ] {
        let prose = blocks
            .iter()
            .find(|b| b.text.contains(needle))
            .expect("the prose is read");
        assert!(
            matches!(prose.kind, BlockKind::Paragraph),
            "{needle:?} joined the table:\n{markdown}"
        );
    }
    // With the switch off the band stays out: the test moves the page.
    let off = blocks_with_header(&statement_with_header_band(), false);
    let off_markdown = to_markdown(&off);
    for needle in ["March", "(in millions)", "2025", "Liabilities"] {
        assert!(
            !off.iter()
                .any(|b| matches!(b.kind, BlockKind::TableCell { .. }) && b.text.contains(needle)),
            "{needle:?} joined with the switch off:\n{off_markdown}"
        );
    }
}

#[test]
fn a_title_over_a_section_label_stays_out() {
    let blocks = blocks_with_header(&statement_with_bare_title(), true);
    let markdown = to_markdown(&blocks);
    assert_eq!(
        tables_in(&blocks),
        1,
        "one statement, one table:\n{markdown}"
    );
    // Both lines stay outside the table, as paragraphs; no cell holds them.
    for needle in ["Regional Results", "Liabilities", "Table 2 Net sales"] {
        assert!(
            blocks
                .iter()
                .any(|b| matches!(b.kind, BlockKind::Paragraph) && b.text.contains(needle)),
            "{needle:?} is not a paragraph:\n{markdown}"
        );
        assert!(
            !blocks
                .iter()
                .any(|b| matches!(b.kind, BlockKind::TableCell { .. }) && b.text.contains(needle)),
            "{needle:?} joined the table:\n{markdown}"
        );
    }
    // The table's top edge is the first figure row.
    let first = blocks
        .iter()
        .find(|b| matches!(b.kind, BlockKind::TableCell { .. }))
        .expect("a table is found");
    assert!(
        first.text.contains("Deposits"),
        "the table does not start at the first figure row:\n{markdown}"
    );
}
