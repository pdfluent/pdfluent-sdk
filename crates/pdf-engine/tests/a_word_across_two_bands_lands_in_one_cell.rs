// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A word across two line bands lands in exactly one cell (#636).
//!
//! A label line drawn as one span, decoded into two cells with the span's
//! centre in the word gap between them, used to fall back to every span of
//! its source block in BOTH cells; the stacked-row split then re-emitted each
//! line band's words in every cell of the row (PMC5139084-p0004: 740 cell
//! words from a 162-word region). Now the fallback clips, per word of the
//! cell, the span under the word box's centre, so each occurrence lands once.
//!
//! End to end through `extract_blocks` on an operator-built table: a stub and
//! eight figure columns with a mid-table label line over the first figure
//! column. Red without the fix (the label line reads in both cells of its row
//! and the data lines repeat beside it); green with it.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::{Block, BlockKind, ExtractOptions, PdfDocument};
use std::collections::HashMap;

const BODY_SIZE: f64 = 10.0;
const TOP: f64 = 700.0;
const PITCH: f64 = 18.0;
const LABEL: &str = "Concentric knee flexor";

/// Stub plus eight figure columns, narrow at the stub so the heuristic builder
/// stands down and the judged region forces the decoder path.
const COLUMNS: [f64; 9] = [40.0, 120.0, 175.0, 230.0, 285.0, 340.0, 395.0, 450.0, 505.0];
const STUBS: [&str; 4] = ["Strain A", "Strain B", "Strain C", "Strain D"];
const FIGS: [&str; 8] = [
    "12.3", "45.6", "78.9", "10.2", "33.4", "55.6", "77.8", "99.0",
];

fn font(doc: &mut Document, base: &[u8]) -> Object {
    Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(base.to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    })))
}

fn pdf_of(pieces: &[(String, bool, f64, f64)]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let regular = font(&mut doc, b"Helvetica");
    let heavy = font(&mut doc, b"Helvetica-Bold");
    let mut ops = Vec::new();
    for (text, bold, x, y) in pieces {
        ops.push(Operation::new("BT", vec![]));
        ops.push(Operation::new(
            "Tf",
            vec![
                Object::Name(if *bold { b"F2" } else { b"F1" }.to_vec()),
                BODY_SIZE.into(),
            ],
        ));
        ops.push(Operation::new("Td", vec![(*x).into(), (*y).into()]));
        ops.push(Operation::new(
            "Tj",
            vec![Object::String(
                text.clone().into_bytes(),
                StringFormat::Literal,
            )],
        ));
        ops.push(Operation::new("ET", vec![]));
    }
    let content_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        Content { operations: ops }
            .encode()
            .expect("encode content"),
    )));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dictionary! {
                "F1" => regular.clone(),
                "F2" => heavy.clone(),
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

fn blocks_of(pieces: &[(String, bool, f64, f64)]) -> Vec<Block> {
    PdfDocument::open(pdf_of(pieces))
        .expect("open pdf")
        .extract_blocks(&ExtractOptions::default())
        .expect("extract blocks")
}

/// (word, span box) occurrences per cell: by box identity, not text.
type Occurrence = (String, [i64; 4]);

fn occurrences(blocks: &[Block]) -> HashMap<Occurrence, Vec<(u16, u16)>> {
    let mut out: HashMap<Occurrence, Vec<(u16, u16)>> = HashMap::new();
    for b in blocks {
        let BlockKind::TableCell { row, column, .. } = b.kind else {
            continue;
        };
        for s in &b.spans {
            let bbox = [
                (s.x * 100.0).round() as i64,
                (s.y * 100.0).round() as i64,
                ((s.x + s.width) * 100.0).round() as i64,
                ((s.y + s.height) * 100.0).round() as i64,
            ];
            for w in s.text.split_whitespace() {
                out.entry((w.to_string(), bbox))
                    .or_default()
                    .push((row, column));
            }
        }
    }
    out
}

fn table_pieces() -> Vec<(String, bool, f64, f64)> {
    let mut pieces = Vec::new();
    let mut y = TOP;
    for (r, stub) in STUBS.iter().enumerate() {
        y -= PITCH;
        if r == 2 {
            pieces.push((LABEL.to_string(), false, 120.0, y));
            y -= PITCH;
        }
        pieces.push((stub.to_string(), false, COLUMNS[0], y));
        for (c, fig) in FIGS.iter().enumerate() {
            pieces.push((format!("{fig}{r}"), false, COLUMNS[c + 1], y));
        }
    }
    pieces
}

/// The label's words, and every data word, land in exactly one cell each.
#[test]
fn a_word_across_two_bands_lands_in_one_cell() {
    let blocks = blocks_of(&table_pieces());
    let mut dups: Vec<(String, Vec<(u16, u16)>)> = Vec::new();
    for (key, cells) in occurrences(&blocks) {
        let mut unique = cells.clone();
        unique.sort();
        unique.dedup();
        if unique.len() > 1 {
            dups.push((key.0.clone(), unique));
        }
    }
    dups.sort();
    assert!(
        dups.is_empty(),
        "word occurrences in more than one cell: {dups:?}"
    );
    for word in ["Concentric", "knee", "flexor"] {
        let holders: Vec<(u16, u16)> = blocks
            .iter()
            .filter_map(|b| match b.kind {
                BlockKind::TableCell { row, column, .. }
                    if b.text.split_whitespace().any(|w| w == word) =>
                {
                    Some((row, column))
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            holders.len(),
            1,
            "{word} reads in more than one cell: {holders:?}"
        );
    }
}
