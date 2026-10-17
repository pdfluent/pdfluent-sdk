//! A decimal dot abutting its digit joins it, on the default path (#655).
//!
//! The glyphs of one run arrive in one span carrying a box per glyph, so a
//! decimal dot is the `.` glyph box right after its digit's box inside the
//! `18.90` span. In a table the leader fold used to read that dot as a dot
//! leader and glue it on with a space, dropping its box and leaving the
//! label's width short: `18.90` came out `18 . 90`, the period without a box
//! of its own, and a gap measured past the stale edge spaced the digit after
//! it (`12. 30`). A dot with no daylight between it and its label is a
//! decimal dot, not a leader across white, so it joins tight, keeps its box,
//! and the label's box grows to cover it. A leader standing off across white
//! still folds with a space (`entries_on_a_dot_leader_are_a_table` pins that
//! side).
//!
//! Through `PdfDocument::extract_blocks` on an operator-built ruled table.
//! Without the abutting join the figures read `18 . 90` and the dot's box is
//! gone, so both tests go red.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::{BlockKind, ExtractOptions, PdfDocument};

const BODY: f64 = 10.0;
const MARGIN: f64 = 72.0;
const TOP: f64 = 680.0;
const PITCH: f64 = 18.0;
const COLUMNS: [f64; 3] = [MARGIN, 220.0, 360.0];

fn row(cells: [&str; 3], y: f64, bold: bool) -> Vec<(Vec<u8>, f64, f64, bool)> {
    COLUMNS
        .iter()
        .zip(cells)
        .map(|(x, body)| (body.as_bytes().to_vec(), *x, y, bold))
        .collect()
}

fn tariff_pdf() -> Vec<u8> {
    let mut pieces = row(["Band", "Units", "Rate"], TOP, true);
    for (index, cells) in [
        ["Standing", "none", "18.90"],
        ["Day", "0 to 500", "24.31"],
        ["Night", "0 to 500", "11.07"],
    ]
    .iter()
    .enumerate()
    {
        pieces.extend(row(
            [cells[0], cells[1], cells[2]],
            TOP - (index + 1) as f64 * PITCH,
            false,
        ));
    }
    let mut doc = Document::with_version("1.7");
    let regular = font(&mut doc, b"Helvetica");
    let strong = font(&mut doc, b"Helvetica-Bold");
    let mut operations = Vec::new();
    for index in 0..5 {
        let y = TOP - index as f64 * PITCH + 11.0;
        operations.push(Operation::new(
            "re",
            vec![
                (MARGIN - 6.0).into(),
                y.into(),
                (430.0 - MARGIN + 6.0).into(),
                0.4.into(),
            ],
        ));
        operations.push(Operation::new("f", vec![]));
    }
    for (bytes, x, y, bold) in &pieces {
        operations.push(Operation::new("BT", vec![]));
        operations.push(Operation::new(
            "Tf",
            vec![
                Object::Name(if *bold {
                    b"F2".to_vec()
                } else {
                    b"F1".to_vec()
                }),
                BODY.into(),
            ],
        ));
        operations.push(Operation::new("Td", vec![(*x).into(), (*y).into()]));
        operations.push(Operation::new(
            "Tj",
            vec![Object::String(bytes.clone(), StringFormat::Literal)],
        ));
        operations.push(Operation::new("ET", vec![]));
    }
    let content = Content { operations }.encode().expect("encode content");
    let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dictionary! {
                "F1" => regular.clone(),
                "F2" => strong.clone(),
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
    doc.save_to(&mut buf).expect("save the tariff");
    buf
}

fn font(doc: &mut Document, base: &[u8]) -> Object {
    Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(base.to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    })))
}

fn cells() -> Vec<(u16, u16, String, Vec<pdf_engine::TextSpan>)> {
    let blocks = PdfDocument::open(tariff_pdf())
        .expect("open the tariff")
        .extract_blocks(&ExtractOptions::default())
        .expect("extract with the default options");
    blocks
        .into_iter()
        .filter_map(|b| match b.kind {
            BlockKind::TableCell { row, column, .. } => Some((row, column, b.text, b.spans)),
            _ => None,
        })
        .collect()
}

/// Decimal figures in a ruled table read joined on the default path.
#[test]
fn decimal_figures_in_a_ruled_table_read_joined() {
    let cells = cells();
    for (row, want) in [(1, "18.90"), (2, "24.31"), (3, "11.07")] {
        let got = cells
            .iter()
            .find(|(r, c, _, _)| *r == row && *c == 2)
            .map(|(_, _, text, _)| text.as_str());
        assert_eq!(got, Some(want), "row {row} rate cell: {cells:?}");
    }
}

/// A folded dot keeps its own box, right after its digit's, and the span's
/// box grows to cover it: the dot is never read on top of its digit again.
#[test]
fn a_folded_dot_keeps_its_own_box_after_its_digit() {
    // The two spellings of one advance (interpreter placement, device
    // width) meet up to float dust; the hair is the fold's own abut
    // epsilon, still far below any real gap.
    const HAIR: f64 = 1e-6;
    let cells = cells();
    let (_, _, _, spans) = cells
        .iter()
        .find(|(r, c, _, _)| *r == 1 && *c == 2)
        .expect("the 18.90 cell");
    let folded = spans
        .iter()
        .find(|s| s.text == "18.90")
        .expect("the 18.90 span");
    assert_eq!(folded.char_bounds.len(), 5, "one box per glyph: {folded:?}");
    let (digit, dot) = (folded.char_bounds[1], folded.char_bounds[2]);
    assert!(
        dot[0] >= digit[2] - HAIR,
        "the dot starts where the digit ends: {digit:?} {dot:?}"
    );
    assert!(dot[2] > dot[0], "the dot has extent: {dot:?}");
    assert!(
        folded.x + folded.width >= dot[2] - HAIR,
        "the span covers the dot: {:?} {dot:?}",
        [folded.x, folded.width]
    );
}
