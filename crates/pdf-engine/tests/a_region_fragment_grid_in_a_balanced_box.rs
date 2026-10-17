//! A balanced "table" box over figure fragments assembles no table (#636).
//!
//! The region entry's veto through the page assembly: a layout "table" box
//! over structure shards or axis ticks reads as paragraphs, while a numeric
//! table in the same box still grids. Operator-built pages, no models.

#![cfg(feature = "ort-layout")]

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::balanced::{balanced_page_markdown, BalancedOptions};
use pdf_engine::layout::LayoutBox;
use pdf_engine::PdfDocument;

const BODY_SIZE: f64 = 10.0;
const MARGIN: f64 = 72.0;
const TOP: f64 = 700.0;
const PITCH: f64 = 18.0;

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

fn row(cells: &[(Option<&str>, f64)], y: f64) -> Vec<Piece> {
    cells
        .iter()
        .filter_map(|(body, x)| body.map(|body| text(body, *x, y)))
        .collect()
}

fn pdf_of(pieces: &[Piece]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let regular = Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    })));
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
    let content_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        Content { operations }.encode().expect("encode content"),
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

/// A balanced "table" box over the content rows: render pixels at 96 dpi on
/// a 1:1 render, which is PDF points [40, 615, 560, 725] y up.
fn balanced_of(pieces: &[Piece]) -> String {
    let doc = PdfDocument::open(pdf_of(pieces)).expect("open pdf");
    let boxes = vec![LayoutBox {
        label: "table".to_string(),
        score: 0.9,
        bbox_px: [40.0, 60.0, 560.0, 185.0],
        render_w: 612,
        render_h: 792,
    }];
    balanced_page_markdown(&doc, 0, &boxes, &BalancedOptions::default()).expect("balanced runs")
}

const C2: f64 = 210.0;
const C3: f64 = 320.0;
const C4: f64 = 430.0;

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

#[test]
fn a_continued_structure_panel_is_no_table_in_a_balanced_box() {
    let markdown = balanced_of(&continued_structure_panels());
    assert!(
        !markdown.contains("<table"),
        "structure shards assembled as a table: {markdown}"
    );
    assert!(
        markdown.contains("continued"),
        "the veto must leave the labels as paragraphs: {markdown}"
    );
}

#[test]
fn chart_ticks_are_no_table_in_a_balanced_box() {
    let markdown = balanced_of(&chart_ticks_over_figure_caption());
    assert!(
        !markdown.contains("<table"),
        "axis ticks assembled as a table: {markdown}"
    );
    assert!(
        markdown.contains("2.5"),
        "the veto must leave the labels as paragraphs: {markdown}"
    );
}

#[test]
fn a_numeric_table_still_assembles_in_a_balanced_box() {
    let markdown = balanced_of(&numeric_financial_table());
    assert!(
        markdown.contains("<table"),
        "a numeric table must assemble as a table: {markdown}"
    );
}
