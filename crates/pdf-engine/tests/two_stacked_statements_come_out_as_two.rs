//! Two stacked statements come out as two tables (#574).
//!
//! Figure rows, then a year row (`At December 31: 2018 2017`), then figure
//! rows: one judged region holding two statements, the tuning cluster's
//! stacked shape. With `table_split_stacked` on, the grid parts before the
//! year row; with it set off explicitly -- through `refine_page` -- the
//! single table. The switch ships on, so the shipped path reads two. The
//! side switch alone does not move a stacked page.
//!
//! Red with the stacked cut taken out (`stacked_cut` always `None`): the
//! on-shape reads one table, checked once by disabling it.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::learned::{document_facts, page_blocks, refine_page, Switches, SWITCHES};
use pdf_engine::{to_markdown, Block, BlockKind, ExtractOptions, PdfDocument};

const SIZE: f64 = 9.0;
const PITCH: f64 = 18.0;
const STUB: f64 = 72.0;
const FIGS: [f64; 2] = [360.0, 430.0];

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

/// How many tables the blocks hold, read the way every consumer reads a run
/// of cells: a new table wherever `(row, column)` stops climbing.
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

fn statements() -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut y = 700.0;
    pieces.push(at("For the year ended", STUB, y));
    for (x, year) in FIGS.iter().zip(["2018", "2017"]) {
        pieces.push(at(year, *x, y));
    }
    y -= PITCH;
    for (label, figs) in [
        ("Revenue", ["79,591", "79,139"]),
        ("Net income", ["8,728", "5,753"]),
        ("Cash dividends", ["5,666", "5,506"]),
    ] {
        pieces.push(at(label, STUB, y));
        for (x, fig) in FIGS.iter().zip(figs) {
            pieces.push(at(fig, *x, y));
        }
        y -= PITCH;
    }
    pieces.push(at("At December 31:", STUB, y));
    for (x, year) in FIGS.iter().zip(["2018", "2017"]) {
        pieces.push(at(year, *x, y));
    }
    y -= PITCH;
    for (label, figs) in [
        ("Total assets", ["123,382", "125,356"]),
        ("Total debt", ["45,812", "46,824"]),
    ] {
        pieces.push(at(label, STUB, y));
        for (x, fig) in FIGS.iter().zip(figs) {
            pieces.push(at(fig, *x, y));
        }
        y -= PITCH;
    }
    pieces
}

fn refined(side: bool, stacked: bool) -> Vec<Block> {
    let doc = PdfDocument::open(pdf_of(&statements())).expect("open synthetic pdf");
    let all = doc.extract_blocks_before_learning().expect("blocks");
    let facts = document_facts(&all);
    refine_page(
        page_blocks(&all, 0),
        (792.0, 792.0),
        &facts,
        Switches {
            table_split_side_by_side: side,
            table_split_stacked: stacked,
            ..SWITCHES
        },
    )
}

#[test]
fn two_stacked_statements_come_out_as_two() {
    let on = refined(false, true);
    let on_md = to_markdown(&on);
    assert_eq!(tables_in(&on), 2, "with the stacked split on:\n{on_md}");
    // The year row opens the lower table.
    let lower = on
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::TableCell { .. }))
        .skip_while(|b| {
            !matches!(
                b.kind,
                BlockKind::TableCell {
                    row: 0,
                    column: 0,
                    ..
                }
            )
        })
        .skip(1)
        .find(|b| {
            matches!(
                b.kind,
                BlockKind::TableCell {
                    row: 0,
                    column: 0,
                    ..
                }
            )
        })
        .expect("the lower table opens");
    assert!(
        lower.text.contains("At December 31:"),
        "the lower table does not open at the year row:\n{on_md}"
    );
    for needle in ["Revenue", "Total assets", "Total debt"] {
        assert!(
            on.iter()
                .any(|b| matches!(b.kind, BlockKind::TableCell { .. }) && b.text.contains(needle)),
            "{needle:?} is not a cell:\n{on_md}"
        );
    }
}

#[test]
fn two_stacked_statements_stay_one_with_stacked_off() {
    let off = refined(false, false);
    let off_md = to_markdown(&off);
    assert_eq!(tables_in(&off), 1, "with the stacked split off:\n{off_md}");
    // The side switch alone does not move a stacked page.
    let side = refined(true, false);
    let side_md = to_markdown(&side);
    assert_eq!(
        tables_in(&side),
        1,
        "with only the side split on:\n{side_md}"
    );
    // The stacked split ships on, so the shipped path reads two tables.
    let doc = PdfDocument::open(pdf_of(&statements())).expect("open synthetic pdf");
    let shipped = doc
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    let shipped_md = to_markdown(&shipped);
    assert_eq!(tables_in(&shipped), 2, "on the shipped path:\n{shipped_md}");
}
