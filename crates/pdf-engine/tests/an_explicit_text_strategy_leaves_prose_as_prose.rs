// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! An explicit text strategy still leaves prose as prose (#659).
//!
//! The twins of the prose guards: where SPEC G98 keeps a grid of short
//! aligned labels with an explicit strategy, two columns of running text
//! and a page of year-head sentences are still no table under it. Their
//! cells read as sentences -- the fragments run three words and more, the
//! year-head rows end them -- so the clean-columns exemption does not fire
//! and the judge and the final-table model refuse as on the default path.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::{Block, BlockKind, ExtractOptions, PdfDocument, TableStrategy};

fn with_text_strategy() -> ExtractOptions {
    let mut options = ExtractOptions::default();
    options.table_strategy = TableStrategy::Text;
    options
}

/// Two run-on prose columns the builder tables and the models refuse, the
/// refused-release guard's page: the left column's fragments are short
/// enough to table, the right column's run on, both columns break between
/// the third and fourth rows, and the judge and the final-table model send
/// both back to prose -- on the default path and under an explicit text
/// strategy alike.
fn run_on_columns() -> Vec<Piece> {
    const LEFT: [&str; 5] = [
        "Anna Berg joined",
        "the board in late",
        "spring and now",
        "chairs the audit",
        "group each year",
    ];
    const RIGHT: [&str; 5] = [
        "she previously served as chief",
        "financial officer for a decade",
        "across three continents with",
        "patience through every season",
        "and retired last spring.",
    ];
    let mut pieces = Vec::new();
    for r in 0..5 {
        let y = 700.0 - 18.0 * r as f64 - if r >= 3 { 18.0 } else { 0.0 };
        pieces.push(Piece {
            text: LEFT[r].to_string(),
            x: 72.0,
            y,
        });
        pieces.push(Piece {
            text: RIGHT[r].to_string(),
            x: 220.0,
            y,
        });
    }
    pieces
}

#[test]
fn an_explicit_text_strategy_leaves_run_on_columns_as_prose() {
    let bytes = pdf_of(&run_on_columns(), 10.0, 612);
    // Precondition: the builder tables the page, so a prose answer means a
    // refusal and not a page no grid was ever read from.
    let before = PdfDocument::open(bytes.clone())
        .expect("open synthetic pdf")
        .extract_blocks_before_learning()
        .expect("blocks");
    assert!(
        before
            .iter()
            .any(|b| matches!(b.kind, BlockKind::TableCell { .. })),
        "the builder tabled nothing, so this test says nothing"
    );
    let blocks = PdfDocument::open(bytes)
        .expect("open synthetic pdf")
        .extract_blocks(&with_text_strategy())
        .expect("blocks");
    let markdown = pdf_engine::to_markdown(&blocks);
    assert_eq!(
        tables_in(&blocks),
        0,
        "run-on prose columns read as a table under the text strategy:\n{markdown}"
    );
    assert!(
        blocks.iter().any(|b| b.text.contains("Anna Berg joined")),
        "the prose is gone:\n{markdown}"
    );
}

struct Piece {
    text: String,
    x: f64,
    y: f64,
}

fn pdf_of(pieces: &[Piece], font_size: f64, media: i64) -> Vec<u8> {
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
            vec![Object::Name(b"F1".to_vec()), font_size.into()],
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
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), media.into(), 792.into()]),
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

#[test]
fn an_explicit_text_strategy_leaves_year_head_sentences_as_prose() {
    const STUB: f64 = 72.0;
    const FIGS: [f64; 2] = [300.0, 450.0];
    let mut pieces = Vec::new();
    let mut y = 700.0;
    pieces.push(Piece {
        text: "For the year ended".to_string(),
        x: STUB,
        y,
    });
    for (x, year) in FIGS.iter().zip(["2023", "2022"]) {
        pieces.push(Piece {
            text: year.to_string(),
            x: *x,
            y,
        });
    }
    y -= 18.0;
    for row in [
        [
            "Sales recovered quickly.",
            "Margins held through the winter.",
            "Orders rose again in spring.",
        ],
        [
            "The backlog cleared by autumn.",
            "Prices stayed firm all year.",
            "New clients signed in May.",
        ],
        [
            "Exports doubled to Asia.",
            "The order book stands full.",
            "Hiring resumed in July.",
        ],
        [
            "Debt fell below target.",
            "Cash reserves grew strongly.",
            "The dividend rose again.",
        ],
        [
            "Two rivals left the market.",
            "Our share passed thirty percent.",
            "Growth beat the plan.",
        ],
        [
            "The outlook stays bright.",
            "Spring trading beat records.",
            "We guide higher again.",
        ],
    ] {
        pieces.push(Piece {
            text: row[0].to_string(),
            x: STUB,
            y,
        });
        for (x, sentence) in FIGS.iter().zip(&row[1..]) {
            pieces.push(Piece {
                text: sentence.to_string(),
                x: *x,
                y,
            });
        }
        y -= 18.0;
    }
    let blocks = PdfDocument::open(pdf_of(&pieces, 9.0, 792))
        .expect("open synthetic pdf")
        .extract_blocks(&with_text_strategy())
        .expect("blocks");
    let markdown = pdf_engine::to_markdown(&blocks);
    assert_eq!(
        tables_in(&blocks),
        0,
        "year-head sentences read as a table under the text strategy:\n{markdown}"
    );
    assert!(
        blocks
            .iter()
            .any(|b| b.text.contains("Sales recovered quickly")),
        "the prose is gone:\n{markdown}"
    );
}
