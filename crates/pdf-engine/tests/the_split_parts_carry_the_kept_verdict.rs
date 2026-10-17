//! The split parts carry the kept verdict (#574).
//!
//! The stacked split runs after the final-table judge, on the tables v2
//! kept: a kept glued grid still comes out as two tables, and both parts'
//! signals carry the kept verdict's probability -- the whole's number, one
//! verdict per part, no second judgement. The off run judges the same whole
//! and its signal holds the same number, which is what the parts are read
//! against; their block ranges tile the whole's.
//!
//! Red with the split back before the judge: each part judges on its own
//! features then, so the two probabilities differ from each other and from
//! the whole's, checked once by moving the call back.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::learned::tables::{IsTableModel, TABLE_IS_V2_THRESHOLD};
use pdf_engine::learned::{
    document_facts, page_blocks, refine_page, table_signals_with, Switches, SWITCHES,
};
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

/// The stacked pair from `two_stacked_statements_come_out_as_two`: one
/// judged region holding two statements, kept by v2 as a whole.
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

fn refined(stacked: bool) -> Vec<Block> {
    let doc = PdfDocument::open(pdf_of(&statements())).expect("open synthetic pdf");
    let all = doc.extract_blocks_before_learning().expect("blocks");
    let facts = document_facts(&all);
    refine_page(
        page_blocks(&all, 0),
        (792.0, 792.0),
        &facts,
        Switches {
            table_split_stacked: stacked,
            ..SWITCHES
        },
    )
}

#[test]
fn the_split_parts_carry_the_kept_verdict() {
    let doc = PdfDocument::open(pdf_of(&statements())).expect("open synthetic pdf");
    let (blocks, rules) = doc
        .extract_blocks_and_rules_before_learning()
        .expect("blocks");
    let size = doc
        .page_geometry(0)
        .map(|g| g.effective_dimensions())
        .unwrap_or((0.0, 0.0));
    let rulings = vec![doc.page_rulings(0).expect("rulings")];
    let images = doc.extract_image_boxes().expect("images");
    let sizes = vec![size];
    let signals = |stacked: bool| {
        table_signals_with(
            &blocks,
            &sizes,
            &rules,
            &rulings,
            &images,
            Switches {
                table_split_stacked: stacked,
                ..SWITCHES
            },
        )
    };

    // The whole, judged with the split off: one table, kept.
    let off = signals(false);
    assert_eq!(off.len(), 1, "one page, one record");
    let whole = off[0].as_ref().expect("the judge ran, so Some");
    assert_eq!(whole.tables.len(), 1, "whole table: {whole:?}");
    let kept = whole.tables[0].p_is_table.unwrap_or_else(|| {
        panic!("v2 judged the whole, so its probability is on the signal: {whole:?}")
    });
    assert_eq!(
        whole.tables[0].p_is_table_model,
        Some(IsTableModel::V2),
        "the whole's number is v2's: {whole:?}"
    );
    assert!(
        kept >= TABLE_IS_V2_THRESHOLD,
        "the whole is kept: {kept} < {TABLE_IS_V2_THRESHOLD}"
    );

    // The parts, with the split on: two tables, the kept number on both.
    let on = signals(true);
    assert_eq!(on.len(), 1, "one page, one record");
    let parts = on[0].as_ref().expect("the judge ran, so Some");
    assert_eq!(parts.tables.len(), 2, "split table: {parts:?}");
    for (n, part) in parts.tables.iter().enumerate() {
        let p = part
            .p_is_table
            .unwrap_or_else(|| panic!("part {n} carries no v2 probability: {parts:?}"));
        assert_eq!(
            p, kept,
            "part {n} reads {p}, the kept verdict {kept}: {parts:?}"
        );
        assert_eq!(
            part.p_is_table_model,
            Some(IsTableModel::V2),
            "part {n}'s number is v2's: {parts:?}"
        );
    }
    // The ranges tile the whole's: the split renumbers cells, never moves
    // them.
    let (first, second) = (&parts.tables[0], &parts.tables[1]);
    assert_eq!(
        first.block_range.start, whole.tables[0].block_range.start,
        "the first part opens where the whole opened: {parts:?}"
    );
    assert_eq!(
        second.block_range.end, whole.tables[0].block_range.end,
        "the second part closes where the whole closed: {parts:?}"
    );
    assert_eq!(
        first.block_range.end, second.block_range.start,
        "the parts meet: {parts:?}"
    );

    // And the shipped path still reads two tables, with no block lost or
    // gained in the split.
    let shipped = doc
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    let shipped_md = to_markdown(&shipped);
    assert_eq!(tables_in(&shipped), 2, "on the shipped path:\n{shipped_md}");
    assert_eq!(
        refined(true).len(),
        refined(false).len(),
        "the split changes the block count"
    );
}
