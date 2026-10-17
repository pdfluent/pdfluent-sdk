//! The split tables read the same in markdown as in blocks (#574).
//!
//! The separation splits run inside the table pass, before the blocks are
//! emitted, so every consumer reads the same tables. With both switches on,
//! the side-by-side operator page reads as two tables in the blocks and as
//! two tables in the markdown; on the shipped path it reads as one in both.
//! (There is no m168 final-tables test on this branch yet -- it has not
//! committed -- so this is the agreement pinned here.)
//!
//! Red with the side cut taken out: blocks and markdown read one table,
//! checked once by disabling it.

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

/// How many pipe tables the markdown holds: groups of consecutive lines
/// opening with a pipe.
fn tables_in_markdown(markdown: &str) -> usize {
    let mut tables = 0;
    let mut open = false;
    for line in markdown.lines() {
        if line.starts_with('|') {
            if !open {
                tables += 1;
                open = true;
            }
        } else {
            open = false;
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

#[test]
fn the_split_tables_read_the_same_in_markdown() {
    let doc = PdfDocument::open(pdf_of(&statements())).expect("open synthetic pdf");
    let all = doc.extract_blocks_before_learning().expect("blocks");
    let facts = document_facts(&all);
    let blocks = refine_page(
        page_blocks(&all, 0),
        (792.0, 792.0),
        &facts,
        Switches {
            table_split_side_by_side: true,
            table_split_stacked: true,
            ..SWITCHES
        },
    );
    let markdown = to_markdown(&blocks);
    assert_eq!(tables_in(&blocks), 2, "in blocks:\n{markdown}");
    assert_eq!(tables_in_markdown(&markdown), 2, "in markdown:\n{markdown}");
    // The shipped path agrees with itself on one table.
    let shipped = doc
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    let shipped_md = to_markdown(&shipped);
    assert_eq!(tables_in(&shipped), 1, "in shipped blocks:\n{shipped_md}");
    assert_eq!(
        tables_in_markdown(&shipped_md),
        1,
        "in shipped markdown:\n{shipped_md}"
    );
}
