//! A title keeps the subtitle line set tight under it (#636).
//!
//! `Statements of cash flows` in eighteen point over `for the year ended 30
//! June 2003` in nine and a half, on the same left edge and nearly touching,
//! is annotated as ONE heading. The two lines arrive as blocks of their own --
//! the sizes differ, so the paragraph merge will not touch them -- and the
//! subtitle join puts them back together after the table passes, where the
//! kinds are final. Take the join out of the document path and this goes red
//! with a heading plus a paragraph where it asks for one heading.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::{Block, BlockKind, ExtractOptions, PdfDocument};

const BODY_SIZE: f64 = 10.0;
/// Solid setting for ten-point type.
const LEADING: f64 = 12.0;
const MARGIN: f64 = 72.0;
const TOP: f64 = 700.0;
const TITLE_SIZE: f64 = 18.0;
const SUBTITLE_SIZE: f64 = 9.5;

/// One run of glyphs at a point.
struct Piece {
    text: &'static str,
    bold: bool,
    x: f64,
    y: f64,
    size: f64,
}

fn font(doc: &mut Document, base: &[u8]) -> Object {
    Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(base.to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    })))
}

fn pdf_of(pieces: &[Piece]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let regular = font(&mut doc, b"Helvetica");
    let bold = font(&mut doc, b"Helvetica-Bold");

    let mut operations = Vec::new();
    for piece in pieces {
        let face: &[u8] = if piece.bold { b"F2" } else { b"F1" };
        operations.push(Operation::new("BT", vec![]));
        operations.push(Operation::new(
            "Tf",
            vec![Object::Name(face.to_vec()), piece.size.into()],
        ));
        operations.push(Operation::new("Td", vec![piece.x.into(), piece.y.into()]));
        operations.push(Operation::new(
            "Tj",
            vec![Object::String(
                piece.text.as_bytes().to_vec(),
                StringFormat::Literal,
            )],
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
                "F1" => regular,
                "F2" => bold,
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

/// THE HEURISTIC AND NOT THE MODEL. This page asserts what the subtitle join
/// does with what `headings::classify` hands it, and the learned pass is a
/// stage before it with a switch of its own.
fn blocks_of(pieces: &[Piece]) -> Vec<Block> {
    PdfDocument::open(pdf_of(pieces))
        .expect("open pdf")
        .extract_blocks(&ExtractOptions::default().without_the_learned_pass())
        .expect("extract blocks")
}

fn shape(blocks: &[Block]) -> Vec<(BlockKind, &str)> {
    blocks.iter().map(|b| (b.kind, b.text.as_str())).collect()
}

/// Five lines of body text at the margin, set solid from `y`: the document's
/// own body, which is what reads the title above it as a heading.
fn prose(y: f64) -> Vec<Piece> {
    [
        "Compiler design is the study of how a program written in one",
        "language becomes a program written in another, and of what may",
        "safely be assumed about the two along the way. The subject is",
        "older than the machines that made it necessary, and the proofs",
        "it rests on are older still than that.",
    ]
    .iter()
    .enumerate()
    .map(|(i, text)| Piece {
        text,
        bold: false,
        x: MARGIN,
        y: y - i as f64 * LEADING,
        size: BODY_SIZE,
    })
    .collect()
}

#[test]
fn a_title_keeps_its_subtitle_on_the_page() {
    // The title in eighteen-point bold, the subtitle in nine and a half on
    // the same left edge, its baseline ten points below the title's: a gap of
    // about a point and a half, well inside the two-tenths-of-an-em the join
    // allows an eighteen-point heading.
    let mut pieces = vec![
        Piece {
            text: "Statements of cash flows",
            bold: true,
            x: MARGIN,
            y: TOP,
            size: TITLE_SIZE,
        },
        Piece {
            text: "for the year ended 30 June 2003",
            bold: false,
            x: MARGIN,
            y: TOP - 10.0,
            size: SUBTITLE_SIZE,
        },
    ];
    pieces.extend(prose(TOP - 60.0));

    let blocks = blocks_of(&pieces);
    let title: Vec<&Block> = blocks
        .iter()
        .filter(|b| b.text.starts_with("Statements of cash flows"))
        .collect();
    assert_eq!(
        title.len(),
        1,
        "the title came out in pieces: {:?}",
        shape(&blocks)
    );
    assert_eq!(
        title[0].kind,
        BlockKind::Heading { level: 1 },
        "the joined title is not a heading: {:?}",
        shape(&blocks)
    );
    assert_eq!(
        title[0].text,
        "Statements of cash flows for the year ended 30 June 2003"
    );
}
