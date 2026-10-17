// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A word halfway between two lines of a cell keeps the cut's order (#636).
//!
//! The final by-line cell text lets glyph overlap win where it disagrees with
//! the half-height cut, but only past half the word's height. A word sitting
//! exactly halfway between two lines overlaps both equally, so neither side
//! wins and the cut's own line stands: with the middle word set between the
//! outer words' x positions, joining it upwards would read `AA MID BB CC`
//! and joining it downwards `AA BB CC MID`.

use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::{BlockKind, ExtractOptions, PdfDocument};

/// Baselines a half step apart: the middle word overlaps the outer lines
/// equally by construction (same font, same size, symmetric placement).
fn page() -> Vec<u8> {
    let mut content = String::new();
    // A 2 x 2 grid, ruled, 50..550 by 400..750.
    for x in [50.0, 300.0, 550.0] {
        content += &format!("{x} 400 m {x} 750 l S\n");
    }
    for y in [400.0, 575.0, 750.0] {
        content += &format!("50 {y} m 550 {y} l S\n");
    }
    // Three lines in the top-left cell; the middle word sits between the
    // outer words' x positions so any join would reorder the text.
    for (text, x, y) in [
        ("AA", 60.0, 706.0),
        ("BB", 150.0, 706.0),
        ("MID", 100.0, 698.0),
        ("CC", 60.0, 690.0),
    ] {
        content += &format!("BT /F1 10 Tf {x} {y} Td ({text}) Tj ET\n");
    }
    // One word in each other cell, so the grid holds its words.
    for (x, y) in [(400.0, 650.0), (150.0, 480.0), (400.0, 480.0)] {
        content += &format!("BT /F1 10 Tf {x} {y} Td (cell) Tj ET\n");
    }
    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Courier".to_vec()),
    }));
    let contents = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
    let pages = doc.new_object_id();
    let page = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => contents,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
    });
    doc.objects.insert(
        pages,
        Object::Dictionary(
            dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 },
        ),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
    doc.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save");
    bytes
}

#[test]
fn a_word_halfway_between_two_lines_keeps_the_cut_order() {
    let bytes = page();
    let document = PdfDocument::open(bytes).expect("open the probe");
    let blocks = document
        .extract_blocks(&ExtractOptions::default())
        .expect("extract with the default options");
    let cell = blocks
        .iter()
        .find(|b| b.text.contains("AA"))
        .expect("the probe cell reads");
    assert!(
        matches!(cell.kind, BlockKind::TableCell { .. }),
        "the probe words stay one cell, got {:?}: {}",
        cell.kind,
        cell.text
    );
    assert_eq!(
        cell.text, "AA BB MID CC",
        "the halfway word keeps its own line",
    );
}
