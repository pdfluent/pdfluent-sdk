//! Two columns of text stay two columns, and a list never crosses the gutter
//! between them (#636).
//!
//! Every page here is built from operators, so what is tested is the whole
//! path: interpreter, spans, cut, gutter split, paragraph merge, list rule,
//! serializer. Take the split out of `crate::blocks` and the two-column cases
//! go red the same way -- the rows come back as fused paragraphs holding a
//! left item and a right item each, and no item is found at all, because a
//! fused line opens with the left column's prose and not with any marker.
//!
//! The cut is defeated the way the tuning pages defeat it: a row or two run
//! across the gutter, so no x is free of every span and the columns are never
//! parted. The gutter is still there inside every other row, a wide gap at
//! the same x row after row, which is what the split reads.

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
const LEFT_MARGIN: f64 = 72.0;
const RIGHT_BULLET: f64 = 340.0;
const RIGHT_TEXT: f64 = 354.0;
const TOP: f64 = 700.0;
/// Where an item's own text begins.
const TEXT_INDENT: f64 = 14.0;

/// One run of glyphs at a point.
struct Piece {
    bytes: Vec<u8>,
    x: f64,
    y: f64,
}

fn text(body: &str, x: f64, y: f64) -> Piece {
    Piece {
        bytes: body.as_bytes().to_vec(),
        x,
        y,
    }
}

/// A bullet item in the left column: the WinAnsi bullet in its own run, the
/// text hanging off it. Short, so the row's gutter gapes.
fn left_item(body: &str, y: f64) -> Vec<Piece> {
    vec![
        Piece {
            bytes: vec![0x95],
            x: LEFT_MARGIN,
            y,
        },
        text(body, LEFT_MARGIN + TEXT_INDENT, y),
    ]
}

/// A bullet item in the right column, on the same baseline as its left
/// neighbour: one fused row across the gutter.
fn right_item(body: &str, y: f64) -> Vec<Piece> {
    vec![
        Piece {
            bytes: vec![0x95],
            x: RIGHT_BULLET,
            y,
        },
        text(body, RIGHT_TEXT, y),
    ]
}

/// A line of prose running the full measure, gutter and all: the row that
/// keeps the cut from parting the columns.
fn bridging(body: &str, y: f64) -> Piece {
    text(body, LEFT_MARGIN, y)
}

fn pdf_of(pieces: &[Piece]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    }));

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
                "F1" => Object::Reference(font_id),
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

fn shape(blocks: &[Block]) -> Vec<(BlockKind, &str)> {
    blocks.iter().map(|b| (b.kind, b.text.as_str())).collect()
}

fn items(blocks: &[Block]) -> Vec<&Block> {
    blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::ListItem { .. }))
        .collect()
}

#[test]
fn two_column_lists_stay_in_their_own_columns() {
    // Four rows, a list of four in each column on shared baselines, with two
    // full-measure rows keeping the cut from parting them. Without the gutter
    // split every row is one fused line, tab-marked and markerless, and the
    // page comes back as prose with the bullets still inside it.
    // Three fused rows, no more: the band grouper's own gutter support wants
    // four fifths of the bands gapped, and three of six leaves it silent, so
    // every row reaches the merge fused. Ragged left ends, so the rows are no
    // grid either: a table is never found, on either arm, and what is tested
    // is the gutter split and nothing else.
    let left = [
        "apples and crabs",
        "quinces for keeping over winter",
        "medlars and sorbs",
    ];
    let right = ["pears and perry", "plums and gages", "cherries and merries"];
    // Long, so no x in the gutter is free of every span: the cut never parts
    // the columns.
    let mut pieces = vec![
        bridging(
            "The committee considered the proposals put before it in turn at length seriatim",
            TOP,
        ),
        bridging(
            "The officers were asked to report before the annual meeting ended promptly",
            TOP - LEADING,
        ),
    ];
    for (i, (l, r)) in left.iter().zip(right.iter()).enumerate() {
        let y = TOP - (i + 2) as f64 * LEADING;
        pieces.extend(left_item(l, y));
        pieces.extend(right_item(r, y));
    }
    pieces.push(bridging(
        "The members resolved to meet again before the year was out entirely",
        TOP - 5.0 * LEADING,
    ));

    // Without the learned pass: the table the rows suggest is refused and
    // released by the models on the small page, which hides the fused lines
    // the gutter split is for. The split lives in the heuristics, and that is
    // where it is pinned.
    let mut options = ExtractOptions::default();
    options.learned = false;
    let blocks = PdfDocument::open(pdf_of(&pieces))
        .expect("open pdf")
        .extract_blocks(&options)
        .expect("extract blocks");
    assert!(
        blocks
            .iter()
            .all(|b| !matches!(b.kind, BlockKind::TableCell { .. })),
        "the test page was read as a table, which is not what is tested: {:?}",
        shape(&blocks)
    );
    assert_eq!(
        items(&blocks).len(),
        6,
        "the two columns did not come back as six items: {:?}",
        shape(&blocks)
    );
    for body in left.iter().chain(right.iter()) {
        assert!(
            items(&blocks).iter().any(|b| b.text == **body),
            "{body:?} is not an item of its own: {:?}",
            shape(&blocks)
        );
    }
    // And no block holds text from both sides of the gutter.
    for block in &blocks {
        let has_left = left.iter().any(|l| block.text.contains(l));
        let has_right = right.iter().any(|r| block.text.contains(r));
        assert!(
            !(has_left && has_right),
            "a block crosses the gutter: {:?}",
            block.text
        );
    }
}

#[test]
fn a_wrapped_item_beside_another_column_stays_one_item() {
    // The left column holds two items, the second wrapping to a second line;
    // the right column holds prose on the same baselines. The split must not
    // cut the item at its own hang, and the prose must not join it. Three
    // gapped rows of six bands, as above, so every row reaches the merge
    // fused -- and without the learned pass, which would release the rows.
    let mut pieces = vec![
        bridging(
            "The committee considered the proposals put before it in turn at length seriatim",
            TOP,
        ),
        bridging(
            "The chair declared the meeting open and welcomed those present here",
            TOP - LEADING,
        ),
    ];
    pieces.extend(left_item("apples and pears", TOP - 2.0 * LEADING));
    pieces.push(text(
        "The officers were asked to report in due course here",
        RIGHT_TEXT,
        TOP - 2.0 * LEADING,
    ));
    pieces.extend(left_item(
        "quinces, medlars and several others",
        TOP - 3.0 * LEADING,
    ));
    pieces.push(text(
        "The treasurer laid the accounts before them all",
        RIGHT_TEXT,
        TOP - 3.0 * LEADING,
    ));
    pieces.push(text(
        "that will not ripen on the tree at all",
        LEFT_MARGIN + TEXT_INDENT,
        TOP - 4.0 * LEADING,
    ));
    pieces.push(text(
        "The members resolved to meet again soon after",
        RIGHT_TEXT,
        TOP - 4.0 * LEADING,
    ));
    pieces.push(bridging(
        "The minutes were taken as read aloud and confirmed nemine contradicente",
        TOP - 5.0 * LEADING,
    ));

    let mut options = ExtractOptions::default();
    options.learned = false;
    let blocks = PdfDocument::open(pdf_of(&pieces))
        .expect("open pdf")
        .extract_blocks(&options)
        .expect("extract blocks");
    assert_eq!(
        items(&blocks).len(),
        2,
        "the left column is not two items: {:?}",
        shape(&blocks)
    );
    let found = items(&blocks);
    let wrapped = found
        .iter()
        .find(|b| b.text.starts_with("quinces"))
        .unwrap_or_else(|| panic!("no item opens with the wrapped one: {:?}", shape(&blocks)));
    assert_eq!(
        wrapped.text,
        "quinces, medlars and several others that will not ripen on the tree at all",
        "{:?}",
        shape(&blocks)
    );
    assert!(
        blocks
            .iter()
            .any(|b| b.kind == BlockKind::Paragraph && b.text.contains("The officers were asked")),
        "the right column prose went missing: {:?}",
        shape(&blocks)
    );
}

/// Page 55 of the Orica 2012 annual report: two columns read as one, the
/// right column's list fused row by row into the left column's prose, its
/// bullets a Wingdings `y` the cut hangs into the gutter. Without the gutter
/// split the page comes back fused -- the approval sentence holding half a
/// list item -- and none of the left column's own items is found.
const ORICA: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/doclaynet/asx_ori_2012_p55.pdf"
);

#[test]
fn a_real_two_column_page_keeps_both_columns_lists_separate() {
    let blocks = PdfDocument::open(std::fs::read(ORICA).expect("fixture"))
        .expect("open pdf")
        .extract_blocks(&ExtractOptions::default())
        .expect("extract blocks");
    let found = items(&blocks);
    for body in ["AASB 2011-4 Amendments", "AASB 13 Fair Value Measurement"] {
        assert!(
            found.iter().any(|b| b.text.starts_with(body)),
            "no item opens with {body:?}: {:?}",
            shape(&blocks)
        );
    }
    assert!(
        !blocks
            .iter()
            .any(|b| b.text.contains("approved by the Board of")
                && b.text.contains("Disclosure Requirements")),
        "a block still crosses the gutter: {:?}",
        shape(&blocks)
    );
    assert!(
        blocks
            .iter()
            .any(|b| matches!(b.kind, BlockKind::Heading { .. })
                && b.text.contains("Accounting policies")),
        "the section heading did not survive the split: {:?}",
        shape(&blocks)
    );
}
