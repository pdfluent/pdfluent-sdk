//! A text column in the middle does not split the table (#574).
//!
//! `Item | Amount | Description | Amount2`, the gap before the middle
//! column the narrowest by far: the column kinds repeat across the middle
//! (`[text, figure]` twice), but the seam stands out against no neighbour,
//! so the side split stays shut with the switch on. Today's single table,
//! through `refine_page` and through `PdfDocument::extract_blocks`.
//!
//! Red with the gap check taken out (every cut stands out): the on-shape
//! reads two tables, checked once by disabling it.

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
/// Column starts set so the gap before the middle column is the narrowest by
/// far: the kinds repeat across it, but the seam stands out nowhere.
const COLS: [f64; 4] = [72.0, 220.0, 300.0, 450.0];

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

fn ledger() -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut y = 700.0;
    for (x, head) in COLS
        .iter()
        .zip(["Item", "Amount", "Description", "Amount2"])
    {
        pieces.push(at(head, *x, y));
    }
    y -= PITCH;
    for (item, amount, note, total) in [
        ("Alpha", "100", "first lot", "110"),
        ("Bravo", "200", "second lot", "220"),
        ("Charlie", "300", "third lot", "330"),
        ("Delta", "400", "fourth lot", "440"),
        ("Echo", "500", "fifth lot", "550"),
        ("Foxtrot", "600", "sixth lot", "660"),
    ] {
        for (x, text) in COLS.iter().zip([item, amount, note, total]) {
            pieces.push(at(text, *x, y));
        }
        y -= PITCH;
    }
    pieces
}

#[test]
fn a_text_column_in_the_middle_does_not_split() {
    let doc = PdfDocument::open(pdf_of(&ledger())).expect("open synthetic pdf");
    let all = doc.extract_blocks_before_learning().expect("blocks");
    let facts = document_facts(&all);
    let run = |side: bool| {
        refine_page(
            page_blocks(&all, 0),
            (792.0, 792.0),
            &facts,
            Switches {
                table_split_side_by_side: side,
                ..SWITCHES
            },
        )
    };
    let (off, on) = (run(false), run(true));
    let (off_md, on_md) = (to_markdown(&off), to_markdown(&on));
    assert_eq!(tables_in(&off), 1, "with the side split off:\n{off_md}");
    assert_eq!(tables_in(&on), 1, "with the side split on:\n{on_md}");
    // One table of four columns, not two tables and not prose: the veto is
    // the gap's, on a live cut candidate.
    assert!(
        on.iter()
            .any(|b| matches!(b.kind, BlockKind::TableCell { column: 3, .. })),
        "the table is not four columns wide:\n{on_md}"
    );
    // The shipped path reads today's shape: one table.
    let shipped = doc
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    let shipped_md = to_markdown(&shipped);
    assert_eq!(tables_in(&shipped), 1, "on the shipped path:\n{shipped_md}");
}
