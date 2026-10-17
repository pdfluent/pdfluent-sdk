// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A figure's fragments in a balanced table region are not a table (#636).
//!
//! The earlier veto lives on the `find`/`find_whole` path, but a balanced
//! "table" box never reaches it: it goes through `table_in_region` into the
//! region rebuild, which grids the same shards the veto refuses elsewhere.
//! Every page here is built from operators, so what is tested is the whole
//! region path: interpreter, spans, region lines, grid, serializer.
//!
//! Cases 1-3 mimic the false tables: OCR'd structure panels headed
//! "-continued", polymer drawing shards, and chart-axis ticks above a
//! `Figure 20` caption. Each goes red with the region veto disabled. The
//! keep-cases guard the other direction: a numeric table, a word table with
//! holes, and a word table under a `Table 1` caption beside a figure caption
//! all keep their tables with the veto in place.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::{to_markdown, Block, BlockKind, ExtractOptions, PdfDocument};

const BODY_SIZE: f64 = 10.0;
const MARGIN: f64 = 72.0;
const TOP: f64 = 700.0;
/// One row to the next. Well over the type size, which is what a table is set
/// on and what keeps the rows apart.
const PITCH: f64 = 18.0;

/// One run of glyphs at a point.
struct Piece {
    text: String,
    x: f64,
    y: f64,
}

fn text(body: &str, x: f64, y: f64) -> Piece {
    Piece {
        text: body.to_string(),
        x,
        y,
    }
}

/// One row of fragments: the cells at the column positions given. A `None`
/// cell is a hole: the row draws nothing there, the way a figure's labels
/// leave most of their grid empty.
fn row(cells: &[(Option<&str>, f64)], y: f64) -> Vec<Piece> {
    cells
        .iter()
        .filter_map(|(body, x)| body.map(|body| text(body, *x, y)))
        .collect()
}

fn font(doc: &mut Document) -> Object {
    Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    })))
}

fn stream_of(pieces: &[Piece]) -> Vec<u8> {
    let mut operations = Vec::new();
    for piece in pieces {
        operations.push(Operation::new("BT", vec![]));
        operations.push(Operation::new(
            "Tf",
            vec![Object::Name(b"F1".to_vec()), BODY_SIZE.into()],
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
    Content { operations }.encode().expect("encode content")
}

fn pdf_of(pieces: &[Piece]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let regular = font(&mut doc);
    let content_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        stream_of(pieces),
    )));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dictionary! {
                "F1" => regular,
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

fn blocks_of(pieces: &[Piece]) -> Vec<Block> {
    PdfDocument::open(pdf_of(pieces))
        .expect("open pdf")
        .extract_blocks(&ExtractOptions::default())
        .expect("extract blocks")
}

/// The region entry over the content rows: what a balanced "table" box over
/// the fragments rebuilds. The box is in PDF points, y up.
fn region_of(pieces: &[Piece]) -> Vec<Block> {
    let doc = PdfDocument::open(pdf_of(pieces)).expect("open pdf");
    doc.table_in_region(0, [40.0, 615.0, 560.0, 725.0])
        .expect("region entry runs")
}

/// Every table cell of the blocks: the whole of what the grid claims.
fn cells(blocks: &[Block]) -> Vec<&str> {
    blocks
        .iter()
        .filter_map(|block| match block.kind {
            BlockKind::TableCell { .. } => Some(block.text.as_str()),
            _ => None,
        })
        .collect()
}

/// The fragment columns every grid below is set at.
const C2: f64 = 210.0;
const C3: f64 = 320.0;
const C4: f64 = 430.0;

/// Case 1: OCR'd chemical structure panels headed "-continued". Short shards
/// at steady x's, most of the grid holes, under panel headers with no figure
/// caption anywhere on the page.
fn continued_structure_panels() -> Vec<Piece> {
    let mut pieces = Vec::new();
    pieces.extend(row(
        &[
            (Some("-continued I"), MARGIN),
            (Some("I"), C2),
            (Some("-continued II"), C3),
            (None, C4),
        ],
        TOP,
    ));
    pieces.extend(row(
        &[
            (Some("CH"), MARGIN),
            (None, C2),
            (Some("Si"), C3),
            (Some("O"), C4),
        ],
        TOP - PITCH,
    ));
    pieces.extend(row(
        &[
            (Some("I"), MARGIN),
            (Some("HO"), C2),
            (None, C3),
            (None, C4),
        ],
        TOP - 2.0 * PITCH,
    ));
    pieces.extend(row(
        &[
            (None, MARGIN),
            (Some("b"), C2),
            (Some("Rf"), C3),
            (Some("Re"), C4),
        ],
        TOP - 3.0 * PITCH,
    ));
    pieces.extend(row(
        &[
            (Some("y"), MARGIN),
            (None, C2),
            (None, C3),
            (Some("Sn"), C4),
        ],
        TOP - 4.0 * PITCH,
    ));
    pieces
}

/// Case 2: polymer structure drawing shards under "-continued" panel headers.
/// Mostly holes, mostly one- and two-character cells, no figure caption.
fn continued_polymer_panels() -> Vec<Piece> {
    let mut pieces = Vec::new();
    pieces.extend(row(
        &[
            (None, MARGIN),
            (Some("-continued"), C2),
            (None, C3),
            (Some("-continued"), C4),
        ],
        TOP,
    ));
    pieces.extend(row(
        &[
            (None, MARGIN),
            (None, C2),
            (None, C3),
            (Some("Polymer4"), C4),
        ],
        TOP - PITCH,
    ));
    pieces.extend(row(
        &[
            (Some("r 0 0"), MARGIN),
            (Some("Mw 8"), C2),
            (Some("P2"), C3),
            (Some("r 0 0"), C4),
        ],
        TOP - 2.0 * PITCH,
    ));
    pieces.extend(row(
        &[
            (Some("r 0 0"), MARGIN),
            (None, C2),
            (Some("r 0 0"), C3),
            (None, C4),
        ],
        TOP - 3.0 * PITCH,
    ));
    pieces
}

/// Case 3: chart-axis ticks directly above a `Figure 20` caption. Short
/// shards, most of the grid holes, the caption below the region box.
fn chart_ticks_over_figure_caption() -> Vec<Piece> {
    let mut pieces = Vec::new();
    pieces.extend(row(
        &[
            (Some("14"), MARGIN),
            (Some("2.5"), C2),
            (Some("12"), C3),
            (Some("8"), C4),
        ],
        TOP,
    ));
    pieces.extend(row(
        &[
            (Some("0"), MARGIN),
            (None, C2),
            (Some("-2"), C3),
            (None, C4),
        ],
        TOP - PITCH,
    ));
    pieces.extend(row(
        &[
            (None, MARGIN),
            (Some("-4"), C2),
            (None, C3),
            (Some("-6"), C4),
        ],
        TOP - 2.0 * PITCH,
    ));
    pieces.extend(row(
        &[
            (Some("-10"), MARGIN),
            (None, C2),
            (None, C3),
            (Some("-12"), C4),
        ],
        TOP - 3.0 * PITCH,
    ));
    pieces.push(text(
        "Figure 20. Response against dopant salt.",
        MARGIN,
        590.0,
    ));
    pieces
}

/// Keep 1: a 4x3 numeric financial table. Dense, no holes: never fragmentary.
fn numeric_financial_table() -> Vec<Piece> {
    let mut pieces = Vec::new();
    for (at, (item, a, b)) in [
        ("Item", "2024", "2023"),
        ("Revenue", "13,110", "12,166"),
        ("Cost of sales", "8,406", "7,737"),
        ("Net income", "1,559", "1,466"),
    ]
    .iter()
    .enumerate()
    {
        pieces.extend(row(
            &[(Some(*item), MARGIN), (Some(*a), 250.0), (Some(*b), 380.0)],
            TOP - at as f64 * PITCH,
        ));
    }
    pieces
}

/// Keep 2: a header-plus-data word table with empty cells. Holes, but word
/// cells throughout.
fn header_data_table_with_holes() -> Vec<Piece> {
    let mut pieces = Vec::new();
    for (at, (a, b, c)) in [
        (Some("Region"), Some("Amount"), Some("Share")),
        (Some("North"), Some("120"), Some("41%")),
        (Some("South"), None, Some("32%")),
        (Some("East"), Some("78"), None),
    ]
    .iter()
    .enumerate()
    {
        pieces.extend(row(
            &[(*a, MARGIN), (*b, 250.0), (*c, 380.0)],
            TOP - at as f64 * PITCH,
        ));
    }
    pieces
}

/// Keep 3: a word table directly under a `Table 1` caption, with a figure
/// caption further down the page. Caption neighbourhood alone never vetoes a
/// word-cell grid.
fn word_table_under_table_caption() -> Vec<Piece> {
    let mut pieces = vec![text("Table 1. Results by region.", MARGIN, 745.0)];
    for (at, (region, amount, share)) in [
        ("North", "120", "41%"),
        ("South", "95", "32%"),
        ("East", "78", "27%"),
    ]
    .iter()
    .enumerate()
    {
        pieces.extend(row(
            &[
                (Some(*region), MARGIN),
                (Some(*amount), 250.0),
                (Some(*share), 380.0),
            ],
            TOP - at as f64 * PITCH,
        ));
    }
    pieces.push(text("Figure 1. Apparatus schematic.", MARGIN, 560.0));
    pieces
}

#[test]
fn a_continued_structure_panel_is_no_region_table() {
    let region = region_of(&continued_structure_panels());
    assert!(
        cells(&region).is_empty(),
        "structure shards rebuilt as a grid: {:?}",
        cells(&region)
    );
}

#[test]
fn a_continued_polymer_panel_is_no_region_table() {
    let region = region_of(&continued_polymer_panels());
    assert!(
        cells(&region).is_empty(),
        "polymer shards rebuilt as a grid: {:?}",
        cells(&region)
    );
}

#[test]
fn chart_ticks_over_a_figure_caption_are_no_region_table() {
    let region = region_of(&chart_ticks_over_figure_caption());
    assert!(
        cells(&region).is_empty(),
        "axis ticks rebuilt as a grid: {:?}",
        cells(&region)
    );
}

#[test]
fn a_continued_structure_panel_is_no_table_through_extract_blocks() {
    let blocks = blocks_of(&continued_structure_panels());
    assert!(
        cells(&blocks).is_empty(),
        "structure shards read as a grid: {:?}",
        cells(&blocks)
    );
    assert!(
        !to_markdown(&blocks).contains('|'),
        "structure shards written as a table"
    );
}

#[test]
fn a_continued_polymer_panel_is_no_table_through_extract_blocks() {
    let blocks = blocks_of(&continued_polymer_panels());
    assert!(
        cells(&blocks).is_empty(),
        "polymer shards read as a grid: {:?}",
        cells(&blocks)
    );
    assert!(
        !to_markdown(&blocks).contains('|'),
        "polymer shards written as a table"
    );
}

#[test]
fn chart_ticks_over_a_figure_caption_are_no_table_through_extract_blocks() {
    let blocks = blocks_of(&chart_ticks_over_figure_caption());
    assert!(
        cells(&blocks).is_empty(),
        "axis ticks read as a grid: {:?}",
        cells(&blocks)
    );
    assert!(
        !to_markdown(&blocks).contains('|'),
        "axis ticks written as a table"
    );
}

#[test]
fn a_numeric_table_still_rebuilds_in_its_region() {
    let region = region_of(&numeric_financial_table());
    assert_eq!(
        cells(&region).len(),
        12,
        "a dense numeric table must stand: {:?}",
        cells(&region)
    );
}

#[test]
fn a_word_table_with_holes_still_rebuilds_in_its_region() {
    let region = region_of(&header_data_table_with_holes());
    assert!(
        cells(&region).len() >= 10,
        "a word table with holes must stand: {:?}",
        cells(&region)
    );
    assert!(
        to_markdown(&region).contains("Region"),
        "the header must survive: {}",
        to_markdown(&region)
    );
}

#[test]
fn a_word_table_under_a_table_caption_still_rebuilds_in_its_region() {
    let region = region_of(&word_table_under_table_caption());
    assert!(
        cells(&region).len() >= 9,
        "a word table under its caption must stand: {:?}",
        cells(&region)
    );
}

#[test]
fn keep_tables_still_read_through_extract_blocks() {
    for (name, pieces) in [
        ("numeric", numeric_financial_table()),
        ("holes", header_data_table_with_holes()),
        ("caption", word_table_under_table_caption()),
    ] {
        let blocks = blocks_of(&pieces);
        assert!(
            !cells(&blocks).is_empty(),
            "{name}: a true table must grid: {}",
            to_markdown(&blocks)
        );
    }
}

#[test]
fn a_word_table_under_a_table_caption_still_reads_through_extract_blocks() {
    let blocks = blocks_of(&word_table_under_table_caption());
    assert!(
        !cells(&blocks).is_empty(),
        "a word table under its caption must grid: {}",
        to_markdown(&blocks)
    );
}
