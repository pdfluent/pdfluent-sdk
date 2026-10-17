//! The double-spaced join runs after heading classification (#636).
//!
//! Each line of a double-spaced passage clears the paragraph-break threshold,
//! so each arrives as a block of its own; the joining pass puts the run back
//! together. Stacked headings arrive on the same wide pitch, and German
//! section lines too -- joining before they are marked turns two headings, or
//! two sections, into one paragraph. So the join runs after
//! `headings::classify`, over `BlockKind::Paragraph` only, and a line opening
//! with `§` or `Art.` never joins the one above it.
//!
//! Take the join out of either extract path and the fourth test goes red with
//! the paths disagreeing; move it before the classifier and the first goes red
//! with one heading where it asks for two; take the section marker out of the
//! guard and the second goes red with one block where it asks for two.

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
const HEADING_SIZE: f64 = 14.0;

/// One run of glyphs at a point: the bytes the page shows, not a string, so
/// `§` can be written as the WinAnsi code a real document writes it as.
struct Piece {
    bytes: Vec<u8>,
    bold: bool,
    mono: bool,
    x: f64,
    y: f64,
    size: f64,
}

/// A run of ordinary text in the body's face.
fn text(body: &str, x: f64, y: f64) -> Piece {
    Piece {
        bytes: body.as_bytes().to_vec(),
        bold: false,
        mono: false,
        x,
        y,
        size: BODY_SIZE,
    }
}

/// A run set in Courier: every glyph advances the same, so lines of the same
/// length end at the same right edge and the join reads them as filling the
/// measure.
fn mono(body: &str, x: f64, y: f64) -> Piece {
    Piece {
        bytes: body.as_bytes().to_vec(),
        bold: false,
        mono: true,
        x,
        y,
        size: BODY_SIZE,
    }
}

/// A run in WinAnsi, the encoding the fixture's font declares: `§` is one
/// byte there and two in UTF-8.
fn latin(body: &str, x: f64, y: f64, size: f64, bold: bool) -> Piece {
    Piece {
        bytes: body.chars().map(|c| c as u32 as u8).collect(),
        bold,
        mono: false,
        x,
        y,
        size,
    }
}

fn heading(body: &str, y: f64) -> Piece {
    Piece {
        bytes: body.as_bytes().to_vec(),
        bold: true,
        mono: false,
        x: MARGIN,
        y,
        size: HEADING_SIZE,
    }
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
    let mono = font(&mut doc, b"Courier");

    let mut operations = Vec::new();
    for piece in pieces {
        let face: &[u8] = if piece.mono {
            b"F3"
        } else if piece.bold {
            b"F2"
        } else {
            b"F1"
        };
        operations.push(Operation::new("BT", vec![]));
        operations.push(Operation::new(
            "Tf",
            vec![Object::Name(face.to_vec()), piece.size.into()],
        ));
        operations.push(Operation::new("Td", vec![piece.x.into(), piece.y.into()]));
        operations.push(Operation::new(
            "Tj",
            vec![Object::String(piece.bytes.clone(), StringFormat::Literal)],
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
                "F3" => mono,
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

/// THE HEURISTIC AND NOT THE MODEL. These pages assert what the joining pass
/// does with what `headings::classify` hands it, and the learned pass of #599
/// is a stage after it with a switch of its own.
fn blocks_of(pieces: &[Piece]) -> Vec<Block> {
    PdfDocument::open(pdf_of(pieces))
        .expect("open pdf")
        .extract_blocks(&ExtractOptions::default().without_the_learned_pass())
        .expect("extract blocks")
}

fn shape(blocks: &[Block]) -> Vec<(BlockKind, &str)> {
    blocks.iter().map(|b| (b.kind, b.text.as_str())).collect()
}

/// Five lines of body text at the margin, set solid from `y`.
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
    .map(|(i, body)| text(body, MARGIN, y - i as f64 * LEADING))
    .collect()
}

#[test]
fn stacked_headings_on_wide_leading_stay_two_headings() {
    // Two headings on a double-spaced pitch: the same size, the same left
    // edge, a gap that clears the paragraph break. Joining before the
    // classifier turns them into one paragraph; joining after it leaves two
    // headings. The upper shouts and the lower does not, so the wrapped-title
    // rule reads two headings and not one title over two lines.
    let mut pieces = vec![
        heading("RESULTS OF OPERATIONS", TOP),
        heading("Revenue and Gross Profit", TOP - 30.0),
    ];
    pieces.extend(prose(TOP - 80.0));

    let blocks = blocks_of(&pieces);
    let headings: Vec<&Block> = blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::Heading { .. }))
        .collect();
    assert_eq!(
        headings.len(),
        2,
        "stacked headings joined: {:?}",
        shape(&blocks)
    );
    assert_eq!(headings[0].text, "RESULTS OF OPERATIONS");
    assert_eq!(headings[1].text, "Revenue and Gross Profit");
}

#[test]
fn section_lines_on_wide_leading_stay_apart() {
    // Two section lines on a double-spaced pitch: the statute's sections read
    // as list items, the articles as paragraphs, and neither pair joins.
    let mut pieces = prose(TOP);
    pieces.push(latin(
        "§ 2 Begriffe und Definitionen der in diesem Vertrag verwendeten Terminologie",
        MARGIN,
        TOP - 90.0,
        BODY_SIZE,
        false,
    ));
    pieces.push(latin(
        "§ 3 Anwendungsbereich und Geltung dieser Bestimmungen im gesamten Inland",
        MARGIN,
        TOP - 114.0,
        BODY_SIZE,
        false,
    ));
    pieces.push(text(
        "Art. 2 Begriffe und Definitionen der in diesem Vertrag verwendeten Terminologie und allen Anlagen",
        MARGIN,
        TOP - 150.0,
    ));
    pieces.push(text(
        "Art. 3 Anwendungsbereich und Geltung dieser Bestimmungen im gesamten Inland und allen Anlagen",
        MARGIN,
        TOP - 174.0,
    ));

    let blocks = blocks_of(&pieces);
    let sections: Vec<&Block> = blocks.iter().filter(|b| b.text.starts_with('§')).collect();
    assert_eq!(
        sections.len(),
        2,
        "section lines joined: {:?}",
        shape(&blocks)
    );
    let articles: Vec<&Block> = blocks
        .iter()
        .filter(|b| b.text.starts_with("Art."))
        .collect();
    assert_eq!(
        articles.len(),
        2,
        "article lines joined: {:?}",
        shape(&blocks)
    );
    assert!(
        articles
            .iter()
            .all(|b| matches!(b.kind, BlockKind::Paragraph)),
        "the guard keeps paragraphs apart: {:?}",
        shape(&blocks)
    );
}

#[test]
fn both_extract_paths_agree_on_a_double_spaced_page() {
    // Headings that stay two, a run that joins to one, sections that stay
    // two: the page path and the document path answer alike on all of them.
    let mut pieces = vec![
        heading("RESULTS OF OPERATIONS", TOP),
        heading("Revenue and Gross Profit", TOP - 30.0),
    ];
    pieces.extend(prose(TOP - 80.0));
    // Four lines of 78 Courier glyphs on a 20 point pitch: the same measure
    // the one-paragraph test joins, set beside the lines that must not.
    for (i, body) in [
        "The double-spaced abstract is six full lines of ten point type on a wide pitch",
        "and each line comes to the paragraph stage as a block of its own, on the pitch",
        "as the break is judged upon the single-spaced body set elsewhere upon the page",
        "so every line clears the threshold alone, and reads as six paragraphs, not one",
    ]
    .iter()
    .enumerate()
    {
        pieces.push(mono(body, MARGIN, TOP - 170.0 - i as f64 * 20.0));
    }
    pieces.push(latin(
        "§ 2 Begriffe und Definitionen der in diesem Vertrag verwendeten Terminologie",
        MARGIN,
        TOP - 270.0,
        BODY_SIZE,
        false,
    ));
    pieces.push(latin(
        "§ 3 Anwendungsbereich und Geltung dieser Bestimmungen im gesamten Inland",
        MARGIN,
        TOP - 294.0,
        BODY_SIZE,
        false,
    ));
    pieces.push(text(
        "Art. 2 Begriffe und Definitionen der in diesem Vertrag verwendeten Terminologie und allen Anlagen",
        MARGIN,
        TOP - 330.0,
    ));
    pieces.push(text(
        "Art. 3 Anwendungsbereich und Geltung dieser Bestimmungen im gesamten Inland und allen Anlagen",
        MARGIN,
        TOP - 354.0,
    ));

    let bytes = pdf_of(&pieces);
    let doc = PdfDocument::open(bytes).expect("open pdf");
    // A test of the heuristic join, not of the learned pass after it (#599):
    // the page path never runs the model, so the document path must not
    // either where the two are compared.
    let whole = doc
        .extract_blocks(&ExtractOptions::default().without_the_learned_pass())
        .expect("extract blocks");
    let page = doc.extract_page_blocks(0, 0).expect("extract page blocks");
    let kinds_and_texts = |blocks: &[Block]| -> Vec<(BlockKind, String)> {
        blocks.iter().map(|b| (b.kind, b.text.clone())).collect()
    };
    assert_eq!(
        kinds_and_texts(&page),
        kinds_and_texts(&whole),
        "page path: {:?}\ndocument path: {:?}",
        shape(&page),
        shape(&whole)
    );
    // And the page says what the first three tests say it says: two
    // headings, one joined run, two sections.
    assert_eq!(
        whole
            .iter()
            .filter(|b| matches!(b.kind, BlockKind::Heading { .. }))
            .count(),
        2,
        "{:?}",
        shape(&whole)
    );
    assert_eq!(
        whole.iter().filter(|b| b.text.starts_with('§')).count(),
        2,
        "{:?}",
        shape(&whole)
    );
    assert!(
        whole.iter().any(
            |b| b.text.starts_with("The double-spaced abstract") && b.text.ends_with("not one")
        ),
        "the run did not join: {:?}",
        shape(&whole)
    );
}
