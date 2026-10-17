//! A table the page rules into rows is read row by row, not column by column
//! (#545, class O-C of #389).
//!
//! Two columns of text with a gutter between them is the shape of a table and
//! the shape of an academic paper, and from the text alone the two are the same
//! picture. Read as columns, the Isartor manual's appendix A comes out as every
//! test file's name and only then every message -- 204 names before the first
//! message -- where both mutool and pdftotext read it row by row.
//!
//! The discriminator tried first was slice width (the appendix gives its file
//! names 108 pt and their messages 363 pt, a factor of 3.4). It fixed that page
//! and cost `tracemonkey` p2 and p13 and the same pages of `highlights`, which
//! are the canonical two-column academic pages -- so a width heuristic is not a
//! table detector, it is the same guess with a threshold on it.
//!
//! What the appendix has and the paper does not is a ruling line between every
//! pair of rows: 28 of them, each 0.18 pt tall and running the whole width of
//! the table. That is the document itself saying "rows", and it is what these
//! fixtures draw and withhold.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::PdfDocument;

const FONT_SIZE: f64 = 10.0;

/// The appendix's shape: a narrow left column of identifiers and a wide right
/// column of sentences, on a pitch of 18 pt.
const NAME_X: f64 = 60.0;
const MESSAGE_X: f64 = 200.0;
const FIRST_ROW_Y: f64 = 700.0;
const ROW_PITCH: f64 = 18.0;

/// The left and right edge of the ruling lines, wider than the text they
/// separate, as a table's rules are.
const RULE_LEFT: f64 = 55.0;
const RULE_RIGHT: f64 = 540.0;
/// The Isartor appendix draws its rules 0.18 pt tall.
const RULE_THICKNESS: f64 = 0.18;
/// How far above a row's baseline its rule sits: between this row and the one
/// before it, as a rule between two rows does.
const RULE_ABOVE_BASELINE: f64 = 11.0;

const ROWS: [(&str, &str); 6] = [
    (
        "id-6-1-2-t01",
        "Document does not start with a percent character",
    ),
    (
        "id-6-1-3-t01",
        "The trailer dictionary does not contain an ID",
    ),
    (
        "id-6-1-3-t02",
        "The trailer dictionary contains an Encrypt entry",
    ),
    (
        "id-6-1-7-t01",
        "The stream token is not followed by a single newline",
    ),
    (
        "id-6-1-8-t03",
        "The object number is not preceded by an end of line",
    ),
    (
        "id-6-1-10-t01",
        "Compression by LZW is used for an image XObject",
    ),
];

fn courier(doc: &mut Document) -> Object {
    Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Courier".to_vec()),
    })))
}

fn cell(operations: &mut Vec<Operation>, text: &str, x: f64, y: f64) {
    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F1".to_vec()), FONT_SIZE.into()],
    ));
    operations.push(Operation::new("Td", vec![x.into(), y.into()]));
    operations.push(Operation::new("Tj", vec![Object::string_literal(text)]));
    operations.push(Operation::new("ET", vec![]));
}

/// A one-page PDF of the table above, with a ruling line above every row when
/// `ruled`, and nothing but the text when not.
fn table_page(ruled: bool) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = courier(&mut doc);

    let mut operations = Vec::new();
    for (row, (name, message)) in ROWS.iter().enumerate() {
        let y = FIRST_ROW_Y - row as f64 * ROW_PITCH;
        if ruled {
            operations.push(Operation::new(
                "re",
                vec![
                    RULE_LEFT.into(),
                    (y + RULE_ABOVE_BASELINE).into(),
                    (RULE_RIGHT - RULE_LEFT).into(),
                    RULE_THICKNESS.into(),
                ],
            ));
            operations.push(Operation::new("f", vec![]));
        }
        cell(&mut operations, name, NAME_X, y);
        cell(&mut operations, message, MESSAGE_X, y);
    }

    let content = Content { operations }.encode().expect("encode content");
    let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![
            0.into(), 0.into(), 612.into(), 792.into(),
        ]),
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dictionary! { "F1" => font }),
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

fn text_of(bytes: Vec<u8>) -> String {
    PdfDocument::open(bytes)
        .expect("open pdf")
        .extract_text(0)
        .expect("extract text")
}

fn position_of(text: &str, needle: &str) -> usize {
    text.find(needle)
        .unwrap_or_else(|| panic!("{needle:?} is missing from the extracted text: {text:?}"))
}

#[test]
fn a_ruled_two_column_table_is_read_row_by_row() {
    let text = text_of(table_page(true));

    for (name, message) in ROWS {
        assert!(
            position_of(&text, name) < position_of(&text, message),
            "{name:?} is not read before its own message: {text:?}"
        );
    }
    for pair in ROWS.windows(2) {
        assert!(
            position_of(&text, pair[0].1) < position_of(&text, pair[1].0),
            "row {:?} is read before the message of the row above it: {text:?}",
            pair[1].0
        );
    }
}

/// The same table with its rules withheld is still read column by column, so
/// this is a change the page's own graphics ask for and not a new rule about
/// every two-column region. Widening it to the geometry alone fails here as
/// loudly as removing it fails above.
#[test]
fn the_same_table_without_rules_is_still_read_column_by_column() {
    let text = text_of(table_page(false));

    let last_name = position_of(&text, ROWS[ROWS.len() - 1].0);
    let first_message = position_of(&text, ROWS[0].1);
    assert!(
        last_name < first_message,
        "the unruled fixture no longer exercises the column cut, so the test \
         above proves nothing: {text:?}"
    );
}
