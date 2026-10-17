//! Two statements side by side come out as two tables (#574).
//!
//! The tuning record-gap cluster this job answers: two 3-column statements
//! next to each other glued into one wide grid. The operator page sets them
//! 35pt apart with 15pt figure gaps inside each, the motivating pages' shape
//! (ragged stubs, narrow gutter): with `table_split_side_by_side` on, the
//! grid parts at the gutter into two tables of 3 columns; with it off --
//! through `refine_page` and through `PdfDocument::extract_blocks` --
//! today's single wide table. `table_split_stacked` alone does not move it.
//!
//! Red with the side cut taken out (`side_cut` always `None`): the on-shape
//! reads one table, checked once by disabling it.

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
const FIGS: [f64; 2] = [240.0, 280.0];
const STUB_RIGHT: f64 = 340.0;
const FIGS_RIGHT: [f64; 2] = [510.0, 550.0];

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
    pieces.push(at("Holdings", STUB, y));
    for (x, head) in FIGS.iter().zip(["Shares", "Value"]) {
        pieces.push(at(head, *x, y));
    }
    pieces.push(at("Holdings", STUB_RIGHT, y));
    for (x, head) in FIGS_RIGHT.iter().zip(["Shares", "Value"]) {
        pieces.push(at(head, *x, y));
    }
    y -= PITCH;
    let left = [
        ("Alpha Corp", ["1,000", "2,000"]),
        ("Bravo Inc", ["4,000", "5,000"]),
        ("Charlie Ltd", ["7,000", "8,000"]),
        ("Delta Corp", ["1,100", "2,200"]),
        ("Echo Inc", ["4,400", "5,500"]),
        ("Foxtrot Ltd", ["7,700", "8,800"]),
    ];
    let right = [
        ("Gulf Corp", ["1,010", "2,020"]),
        ("Hotel Inc", ["4,040", "5,050"]),
        ("India Ltd", ["7,070", "8,080"]),
        ("Juliet Corp", ["1,110", "2,220"]),
        ("Kilo Inc", ["4,440", "5,550"]),
        ("Lima Ltd", ["7,770", "8,880"]),
    ];
    for ((label, figs), (label_right, figs_right)) in left.into_iter().zip(right) {
        pieces.push(at(label, STUB, y));
        for (x, fig) in FIGS.iter().zip(figs) {
            pieces.push(at(fig, *x, y));
        }
        pieces.push(at(label_right, STUB_RIGHT, y));
        for (x, fig) in FIGS_RIGHT.iter().zip(figs_right) {
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
fn two_tables_side_by_side_come_out_as_two() {
    let on = refined(true, false);
    let on_md = to_markdown(&on);
    assert_eq!(tables_in(&on), 2, "with the side split on:\n{on_md}");
    // Two tables of 3 columns: no cell stands past column 2.
    assert!(
        on.iter()
            .all(|b| !matches!(b.kind, BlockKind::TableCell { column: 3.., .. })),
        "a cell stands past column 2:\n{on_md}"
    );
    // Both statements' labels are cells, west part first.
    let first = on
        .iter()
        .find(|b| matches!(b.kind, BlockKind::TableCell { .. }))
        .expect("a table is found");
    assert!(
        first.text.contains("Holdings") || first.text.contains("Alpha Corp"),
        "the first table is not the west statement:\n{on_md}"
    );
    for needle in ["Alpha Corp", "Foxtrot Ltd", "Gulf Corp", "Lima Ltd"] {
        assert!(
            on.iter()
                .any(|b| matches!(b.kind, BlockKind::TableCell { .. }) && b.text.contains(needle)),
            "{needle:?} is not a cell:\n{on_md}"
        );
    }
}

#[test]
fn two_tables_side_by_side_stay_one_with_the_switch_off() {
    let off = refined(false, false);
    let off_md = to_markdown(&off);
    assert_eq!(tables_in(&off), 1, "with the side split off:\n{off_md}");
    // The stacked switch alone does not move a side-by-side page.
    let stacked = refined(false, true);
    let stacked_md = to_markdown(&stacked);
    assert_eq!(
        tables_in(&stacked),
        1,
        "with only the stacked split on:\n{stacked_md}"
    );
    // The shipped path reads today's shape: one wide table.
    let doc = PdfDocument::open(pdf_of(&statements())).expect("open synthetic pdf");
    let shipped = doc
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    let shipped_md = to_markdown(&shipped);
    assert_eq!(tables_in(&shipped), 1, "on the shipped path:\n{shipped_md}");
}
