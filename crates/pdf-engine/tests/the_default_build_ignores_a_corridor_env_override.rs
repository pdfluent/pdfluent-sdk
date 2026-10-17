//! The default build ignores `PDFLUENT_CORRIDOR` (#574).
//!
//! The corridor guard override is a grid instrument behind the
//! `corridor-grid` feature. Without it the decoder serves the shipped guard
//! config from the table switch: a stray variable in the host process can
//! neither change the output nor abort it. This pins both halves through the
//! public path -- the markdown of a joined stripe a corridor splits -- with
//! the variable set to garbage.
//!
//! ONE TEST IN ITS OWN BINARY, because the variable is process-global: on a
//! build that honours it, the first decode reads the garbage and fails loud,
//! which is the red this exists for. Under `corridor-grid` the loud failure
//! is the grid's own semantics, so this file is default builds only.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

#![cfg(not(feature = "corridor-grid"))]

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::{to_markdown, Block, ExtractOptions, PdfDocument};

const TOP: f64 = 700.0;
const PITCH: f64 = 18.0;

struct Piece {
    text: String,
    x: f64,
    y: f64,
}

fn pdf_of(pieces: &[Piece]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let regular = Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    })));
    let pages_id = doc.new_object_id();
    let mut operations = Vec::new();
    for piece in pieces {
        operations.push(Operation::new("BT", vec![]));
        operations.push(Operation::new(
            "Tf",
            vec![Object::Name(b"F1".to_vec()), 10.0.into()],
        ));
        operations.push(Operation::new("Td", vec![piece.x.into(), piece.y.into()]));
        operations.push(Operation::new(
            "Tj",
            vec![Object::string_literal(piece.text.as_str())],
        ));
        operations.push(Operation::new("ET", vec![]));
    }
    let content = doc.add_object(Stream::new(
        dictionary! {},
        Content { operations }.encode().expect("encode"),
    ));
    let page = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "Parent" => Object::Reference(pages_id),
        "Contents" => Object::Reference(content),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Resources" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dictionary! {
                "F1" => regular.clone(),
            }),
        }),
    }));
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => Object::Array(vec![Object::Reference(page)]),
            "Count" => 1,
        }),
    );
    let catalogue = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(catalogue));
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save pdf");
    out
}

fn blocks_of(pieces: &[Piece]) -> Vec<Block> {
    PdfDocument::open(pdf_of(pieces))
        .expect("open pdf")
        .extract_blocks(&ExtractOptions::default())
        .expect("extract blocks")
}

/// Six body rows over stub, figure and word, a long label crossing the
/// stub-figure gutter, a commit row: the corridor splits the joined stripe
/// in three under the shipped guards.
fn layout_label_crossing() -> Vec<Piece> {
    let stubs = ["Item A", "Item B", "Item C", "Item D", "Item E", "Item F"];
    let words = [
        "Accepted form",
        "Rejected form",
        "Approved form",
        "Declined form",
        "Resolved form",
        "Reopened case",
    ];
    let mut pieces = Vec::new();
    for (r, (stub, word)) in stubs.iter().zip(words.iter()).enumerate() {
        let y = TOP - r as f64 * PITCH;
        pieces.push(Piece {
            text: stub.to_string(),
            x: 40.0,
            y,
        });
        pieces.push(Piece {
            text: format!("1{r}.3"),
            x: 110.0,
            y,
        });
        pieces.push(Piece {
            text: word.to_string(),
            x: 185.0,
            y,
        });
    }
    pieces.push(Piece {
        text: "Telecommunications".to_string(),
        x: 40.0,
        y: TOP - 6.0 * PITCH,
    });
    pieces.push(Piece {
        text: "Verified case".to_string(),
        x: 185.0,
        y: TOP - 6.0 * PITCH,
    });
    pieces.push(Piece {
        text: "Item H".to_string(),
        x: 40.0,
        y: TOP - 7.0 * PITCH,
    });
    pieces.push(Piece {
        text: "Filed form".to_string(),
        x: 185.0,
        y: TOP - 7.0 * PITCH,
    });
    pieces
}

/// The default markdown of [`layout_label_crossing`], captured with the
/// variable unset: the garbage run below must read byte for byte the same.
const GOLDEN: &str = "<table>\n<tr><td>Item A</td><td>10.3</td><td>Accepted form</td></tr>\n<tr><td>Item B</td><td>11.3</td><td>Rejected form</td></tr>\n<tr><td>Item C</td><td>12.3</td><td>Approved form</td></tr>\n<tr><td>Item D</td><td>13.3</td><td>Declined form</td></tr>\n<tr><td>Item E</td><td>14.3</td><td>Resolved form</td></tr>\n<tr><td>Item F</td><td>15.3</td><td>Reopened case</td></tr>\n<tr><td colspan=\"2\">Telecommunications</td><td>Verified case</td></tr>\n<tr><td>Item H</td><td></td><td>Filed form</td></tr>\n</table>\n";

#[test]
fn a_garbage_corridor_env_leaves_the_default_output_untouched() {
    std::env::set_var("PDFLUENT_CORRIDOR", "garbage");
    let markdown = to_markdown(&blocks_of(&layout_label_crossing()));
    std::env::remove_var("PDFLUENT_CORRIDOR");
    assert_eq!(markdown, GOLDEN, "the default build read PDFLUENT_CORRIDOR");
}
