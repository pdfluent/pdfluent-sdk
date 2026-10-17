//! One-line blocks back into the narrow column they were sliced out of (#574).
//!
//! A paragraph set in a narrow column -- a sidebar, a caption, an address
//! block -- arrives as one block per line: the XY-cut puts each line in a
//! region of its own and regions never merge back. The builder joins them
//! again where the pair reads as two lines of one column.
//!
//! Every case here is a page built from operators, through the public path.
//! Take the join out and the first test goes red with three blocks where it
//! asks for one; weaken any guard and one of the others does.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::{BlockKind, ExtractOptions, PdfDocument};

const BODY_SIZE: f64 = 10.0;
/// Solid setting for ten-point type.
const LEADING: f64 = 12.0;

/// One run of glyphs at a point, in either face.
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
    })))
}

fn page_of(pieces: &[Piece]) -> Vec<u8> {
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

/// THE HEURISTIC AND NOT THE MODEL. These pages assert what the builder's
/// narrow-column join does, and the learned pass is a stage after it with a
/// switch of its own; a test of the first that silently measures the second
/// tells nothing about either (#599).
fn blocks_of(bytes: Vec<u8>) -> Vec<pdf_engine::Block> {
    PdfDocument::open(bytes)
        .expect("open pdf")
        .extract_blocks(&ExtractOptions::default().without_the_learned_pass())
        .expect("extract blocks")
}

fn at(text: &'static str, x: f64, y: f64) -> Piece {
    Piece {
        text,
        bold: false,
        x,
        y,
        size: BODY_SIZE,
    }
}

/// A paragraph set in a narrow sidebar comes back as one block, not one per
/// line.
///
/// The page around it is what slices it: a body column overlapping the
/// sidebar in x refuses the vertical cut, the offset baselines drag the cut's
/// quartile down until every line gap qualifies, and the sidebar's own steps
/// (12 then 13) give the leading floor no repetition to arm on -- so each of
/// the three lines lands in a region of its own, adjacent in line order. Take
/// the join out and this goes red with three blocks.
#[test]
fn a_sliced_narrow_column_comes_back_as_one_paragraph() {
    const SIDEBAR: [&str; 3] = [
        "The Northwest Shelf is a",
        "world scale offshore oil and",
        "gas exploration province.",
    ];
    let mut pieces = Vec::new();
    for (i, text) in [
        "Body line number one here",
        "Body line number two here",
        "Body line number three",
        "Body line number four",
        "Body line number five",
    ]
    .iter()
    .enumerate()
    {
        pieces.push(at(text, 190.0, 748.0 - i as f64 * LEADING));
    }
    for (i, text) in [
        "Margin note number one",
        "Margin note number two",
        "Margin note number three",
        "Margin note number four",
    ]
    .iter()
    .enumerate()
    {
        pieces.push(at(text, 72.0, 742.5 - i as f64 * LEADING));
    }
    for (i, text) in SIDEBAR.iter().enumerate() {
        let y = [694.5, 682.5, 669.5][i];
        pieces.push(at(text, 72.0, y));
    }
    for (i, text) in ["Body line number six here", "Body line number seven"]
        .iter()
        .enumerate()
    {
        pieces.push(at(text, 190.0, 652.0 - i as f64 * LEADING));
    }

    let bytes = page_of(&pieces);
    let blocks = blocks_of(bytes.clone());
    let holding: Vec<&pdf_engine::Block> = blocks
        .iter()
        .filter(|b| b.text.contains("Northwest"))
        .collect();
    assert_eq!(
        holding.len(),
        1,
        "the sidebar split over {} blocks: {:?}",
        holding.len(),
        blocks.iter().map(|b| &b.text).collect::<Vec<_>>()
    );
    assert_eq!(holding[0].text, SIDEBAR.join(" "));
    assert_eq!(holding[0].kind, BlockKind::Paragraph);
    // The two body lines below the sidebar were sliced the same way, and join
    // the same way; the interleaved singles above stay single, one column
    // apart in line order.
    assert!(
        blocks
            .iter()
            .any(|b| b.text == "Body line number six here Body line number seven"),
        "the sliced body lines did not join: {:?}",
        blocks.iter().map(|b| &b.text).collect::<Vec<_>>()
    );
    assert_eq!(
        blocks.len(),
        11,
        "{:?}",
        blocks.iter().map(|b| &b.text).collect::<Vec<_>>()
    );
    // And through the markdown: one paragraph, not three lines.
    let markdown = PdfDocument::open(bytes)
        .expect("open pdf")
        .extract_markdown(&ExtractOptions::default().without_the_learned_pass())
        .expect("extract markdown");
    assert!(
        markdown.contains(&SIDEBAR.join(" ")),
        "the markdown kept the sidebar split:\n{markdown}"
    );
}

/// A heading line over body text does not join it: bold over regular is a
/// change of type, whatever the geometry says.
#[test]
fn a_bold_heading_line_does_not_join_the_body_below_it() {
    let pieces = [
        Piece {
            text: "Directors Report",
            bold: true,
            x: 72.0,
            y: 700.0,
            size: BODY_SIZE,
        },
        at("The year was good.", 72.0, 688.0),
        at("All divisions grew strongly this year.", 72.0, 676.0),
    ];
    let blocks = blocks_of(page_of(&pieces));
    assert_eq!(
        blocks.len(),
        2,
        "{:?}",
        blocks.iter().map(|b| &b.text).collect::<Vec<_>>()
    );
    assert_eq!(blocks[0].text, "Directors Report");
    // The two regular lines are a sentence boundary, which joins; the heading
    // stands apart from both. The full stop keeps the heading-subtitle join
    // from reading the body as a subtitle, so what this pins is the weight
    // guard of the narrow-column join.
    assert_eq!(
        blocks[1].text,
        "The year was good. All divisions grew strongly this year."
    );
}

/// List items with markers do not join, neither onto each other nor onto the
/// plain line above them: each starts its own block.
#[test]
fn two_list_items_with_markers_do_not_join() {
    let pieces = [
        at("The list opens here.", 72.0, 700.0),
        at("1. First item here with more words", 72.0, 688.0),
        at("2. Second item here", 72.0, 676.0),
    ];
    let blocks = blocks_of(page_of(&pieces));
    assert_eq!(
        blocks.len(),
        3,
        "{:?}",
        blocks.iter().map(|b| &b.text).collect::<Vec<_>>()
    );
    assert_eq!(blocks[0].text, "The list opens here.");
    assert!(
        matches!(blocks[1].kind, BlockKind::ListItem { .. }),
        "first item is {:?}",
        blocks[1].kind
    );
    assert!(
        matches!(blocks[2].kind, BlockKind::ListItem { .. }),
        "second item is {:?}",
        blocks[2].kind
    );
}

/// Two wide one-line paragraphs with a real blank line between them stay two
/// blocks: a second line of white is a break, however alike the lines are.
/// The full stop and the capital keep the double-spaced join from reading the
/// pair as a double-spaced paragraph, so what this pins is the leading guard
/// of the narrow-column join.
#[test]
fn a_blank_line_between_two_paragraphs_keeps_them_apart() {
    let pieces = [
        at(
            "Wide paragraph one with many words set across the page.",
            72.0,
            700.0,
        ),
        at(
            "Wide paragraph two with many words set across the page",
            72.0,
            700.0 - 2.0 * LEADING,
        ),
    ];
    let blocks = blocks_of(page_of(&pieces));
    assert_eq!(
        blocks.len(),
        2,
        "{:?}",
        blocks.iter().map(|b| &b.text).collect::<Vec<_>>()
    );
    assert_eq!(blocks[0].kind, BlockKind::Paragraph);
    assert_eq!(blocks[1].kind, BlockKind::Paragraph);
}
