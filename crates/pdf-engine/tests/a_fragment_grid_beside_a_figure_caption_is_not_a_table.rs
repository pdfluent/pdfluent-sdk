// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A figure's labels are not a table, even when they line up like one (#636).
//!
//! Repaired word boxes let chemistry-figure fragments and chart axes, legends
//! and ticks cross the column-alignment threshold, so the grid builder proposes
//! a table where the page holds a figure. The acceptance step vetoes such a
//! grid, but only when both halves hold: a figure-caption marker nearby AND
//! fragmentary cells. Every page here is built from operators, so what is
//! tested is the whole path: interpreter, spans, cut, row detection, grid,
//! serializer.
//!
//! The first two cases go red with the veto removed: the rows come back as a
//! grid of cells. The last two guard the other direction: a real data table
//! under a caption, and fragments with no figure nearby, still read as
//! before.

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

/// The fragment columns both grids below are set at. Four of them: with six
/// holes in sixteen positions the grid is mostly holes, the way a figure's
/// labels leave most of their grid empty, while still clearing the fill floor.
const C2: f64 = 210.0;
const C3: f64 = 320.0;
const C4: f64 = 430.0;

/// Chart-axis ticks: short shards at steady x's, most of the grid holes, with
/// the caption set under them. Without the veto the steady x's agree and the
/// run reads as a grid; with it the rows stay paragraphs.
fn chart_axis_grid() -> Vec<Piece> {
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
    pieces
}

/// OCR shards of a chemistry figure: one- and two-character cells, most of
/// the grid holes, with the structure's marker set over them.
fn chemistry_fragments() -> Vec<Piece> {
    let mut pieces = Vec::new();
    pieces.extend(row(
        &[
            (Some("CH"), MARGIN),
            (Some("b"), C2),
            (Some("Si"), C3),
            (Some("O"), C4),
        ],
        TOP,
    ));
    pieces.extend(row(
        &[(Some("I"), MARGIN), (None, C2), (Some("R"), C3), (None, C4)],
        TOP - PITCH,
    ));
    pieces.extend(row(
        &[
            (None, MARGIN),
            (Some("Rf"), C2),
            (None, C3),
            (Some("Re"), C4),
        ],
        TOP - 2.0 * PITCH,
    ));
    pieces.extend(row(
        &[
            (Some("y"), MARGIN),
            (None, C2),
            (None, C3),
            (Some("Sn"), C4),
        ],
        TOP - 3.0 * PITCH,
    ));
    pieces
}

#[test]
fn a_chart_axis_grid_beside_a_fig_caption_is_not_a_table() {
    let mut pieces = chart_axis_grid();
    pieces.push(text(
        "FIG. 3. Dopant salt response.",
        MARGIN,
        TOP - 4.0 * PITCH,
    ));
    let blocks = blocks_of(&pieces);
    assert!(
        cells(&blocks).is_empty(),
        "axis ticks beside a caption read as a grid: {:?}",
        cells(&blocks)
    );
    let markdown = to_markdown(&blocks);
    assert!(
        !markdown.contains('|'),
        "axis ticks written as a table: {markdown}"
    );
    assert!(
        markdown.contains("2.5"),
        "the veto must leave the labels as paragraphs: {markdown}"
    );
}

#[test]
fn an_ocr_fragment_grid_beside_a_chemical_formula_caption_is_not_a_table() {
    let mut pieces = vec![text("[Chemical Formula 4]", MARGIN, TOP + PITCH)];
    pieces.extend(chemistry_fragments());
    let blocks = blocks_of(&pieces);
    assert!(
        cells(&blocks).is_empty(),
        "figure fragments beside their marker read as a grid: {:?}",
        cells(&blocks)
    );
    let markdown = to_markdown(&blocks);
    assert!(
        !markdown.contains('|'),
        "figure fragments written as a table: {markdown}"
    );
    assert!(
        markdown.contains("Chemical Formula"),
        "the veto must leave the fragments as paragraphs: {markdown}"
    );
}

#[test]
fn a_data_table_under_a_figure_caption_still_reads_as_a_table() {
    let mut pieces = vec![text("Figure 2. Results by region.", MARGIN, TOP + PITCH)];
    for (at, (region, amount, share)) in [
        ("Region", "Amount", "Share"),
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
                (Some(*amount), 240.0),
                (Some(*share), 360.0),
            ],
            TOP - at as f64 * PITCH,
        ));
    }
    let blocks = blocks_of(&pieces);
    assert_eq!(
        cells(&blocks).len(),
        12,
        "a word-cell table under a caption must stand: {:?}",
        cells(&blocks)
    );
    assert!(
        to_markdown(&blocks).contains("| Region | Amount | Share |"),
        "{}",
        to_markdown(&blocks)
    );
}

#[test]
fn a_fragment_grid_with_no_figure_nearby_still_reads_as_a_table() {
    let blocks = blocks_of(&chart_axis_grid());
    assert!(
        !cells(&blocks).is_empty(),
        "fragments with no caption nearby must still grid: {}",
        to_markdown(&blocks)
    );
}
