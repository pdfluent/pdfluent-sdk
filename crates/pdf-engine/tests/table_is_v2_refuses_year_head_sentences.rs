//! v2 refuses year-head sentences, with a margin (#638).
//!
//! Six sentence rows under a year row -- no stub labels, no figures -- grid
//! as one table with v2 off and are refused with it on, by the shipped
//! refreshed model at p <= 0.15 (measured 0.077 against a 0.22 threshold).
//! The split on or off makes no difference: a single grid never divides.
//! The stacked-split structure -- a refused grid stays prose, a kept one
//! splits with its verdict on both parts -- is held by the crate unit test
//! `a_refused_grid_stays_whole_and_a_kept_one_splits_in_two`, with hand-made
//! verdicts and no model read.
//!
//! Red when the model keeps the grid: the 0-table assertions and the p bound
//! fail together. The m196 pin flipped exactly this way (shipped 0.217
//! refuse, refreshed 0.346 keep); this fixture is refused by both models.

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
const FIGS: [f64; 2] = [300.0, 450.0];

/// The refusal bar this pin holds: well under the model's threshold, with
/// room for retrains to move.
const REFUSAL_PIN: f64 = 0.15;

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

/// A year row over six rows of sentences, one sentence per cell: no stub
/// labels, no figures. Grids as one table; v2 refuses it.
fn year_head_sentences() -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut y = 700.0;
    pieces.push(at("For the year ended", STUB, y));
    for (x, year) in FIGS.iter().zip(["2023", "2022"]) {
        pieces.push(at(year, *x, y));
    }
    y -= PITCH;
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
        pieces.push(at(row[0], STUB, y));
        for (x, sentence) in FIGS.iter().zip(&row[1..]) {
            pieces.push(at(sentence, *x, y));
        }
        y -= PITCH;
    }
    pieces
}

fn refined(stacked: bool, v2: bool) -> Vec<Block> {
    let doc = PdfDocument::open(pdf_of(&year_head_sentences())).expect("open synthetic pdf");
    let all = doc.extract_blocks_before_learning().expect("blocks");
    let facts = document_facts(&all);
    refine_page(
        page_blocks(&all, 0),
        (792.0, 792.0),
        &facts,
        Switches {
            table_split_stacked: stacked,
            table_is_v2: v2,
            ..SWITCHES
        },
    )
}

/// The page's table signals with the split as given, so the whole's refused
/// probability is read where the refusal read it.
fn signals(stacked: bool) -> Vec<Option<pdf_engine::learned::tables::PageTableSignals>> {
    let doc = PdfDocument::open(pdf_of(&year_head_sentences())).expect("open synthetic pdf");
    let (blocks, rules) = doc
        .extract_blocks_and_rules_before_learning()
        .expect("blocks");
    let size = doc
        .page_geometry(0)
        .map(|g| g.effective_dimensions())
        .unwrap_or((0.0, 0.0));
    let rulings = vec![doc.page_rulings(0).expect("rulings")];
    let images = doc.extract_image_boxes().expect("images");
    table_signals_with(
        &blocks,
        &[size],
        &rules,
        &rulings,
        &images,
        Switches {
            table_split_stacked: stacked,
            ..SWITCHES
        },
    )
}

#[test]
fn table_is_v2_refuses_year_head_sentences() {
    const {
        assert!(
            REFUSAL_PIN < TABLE_IS_V2_THRESHOLD,
            "the pin must sit under the threshold it holds"
        );
    }
    // Precondition: with both switches off the page is one table.
    let grid = refined(false, false);
    let grid_md = to_markdown(&grid);
    assert_eq!(tables_in(&grid), 1, "not one table:\n{grid_md}");

    // Refused with the split off, and still refused with it on: no table
    // either way.
    let off = refined(false, true);
    let off_md = to_markdown(&off);
    assert_eq!(tables_in(&off), 0, "the grid is not refused:\n{off_md}");
    let on = refined(true, true);
    let on_md = to_markdown(&on);
    assert_eq!(
        tables_in(&on),
        0,
        "a refused grid split into a table:\n{on_md}"
    );

    // The refusal's number, read with the split off so v2 sees the whole:
    // under the pin, hence under the threshold, and v2's.
    let judged = signals(false);
    assert_eq!(judged.len(), 1, "one page, one record");
    let whole = judged[0].as_ref().expect("the judge ran, so Some");
    assert_eq!(whole.tables.len(), 1, "one grid, one signal: {whole:?}");
    let p = whole.tables[0].p_is_table.unwrap_or_else(|| {
        panic!("v2 judged the grid, so its probability is on the signal: {whole:?}")
    });
    assert_eq!(
        whole.tables[0].p_is_table_model,
        Some(IsTableModel::V2),
        "the refused number is not v2's: {whole:?}"
    );
    assert!(
        p <= REFUSAL_PIN,
        "the refusal lost its margin: {p} > {REFUSAL_PIN}: {whole:?}"
    );

    // And the shipped path reads prose, with the sentences standing.
    let doc = PdfDocument::open(pdf_of(&year_head_sentences())).expect("open synthetic pdf");
    let shipped = doc
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    let shipped_md = to_markdown(&shipped);
    assert_eq!(tables_in(&shipped), 0, "on the shipped path:\n{shipped_md}");
    assert!(
        shipped
            .iter()
            .any(|b| b.text.contains("Sales recovered quickly")),
        "the prose is gone:\n{shipped_md}"
    );
}
