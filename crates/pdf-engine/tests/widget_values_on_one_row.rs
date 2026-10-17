//! Three filled text fields on one row of a form all reach the extracted text
//! (#432, class R-E of #389).
//!
//! A text field's appearance stream draws its value with the advance the font
//! asks for, which is regularly wider than the field box: the reader clips the
//! glyphs, but the advance is what the extractor measures. Three such fields
//! side by side therefore produce spans that overlap, and when they hold the
//! same value -- a repeated date, a column of `N/A`, or this document's
//! `asdfASDF1234xyz` -- each one read as the previous span's last word said
//! again and was dropped by the join that removes a word drawn twice at the
//! seam between two runs. Two of the three values disappeared, which on a form
//! is the loss of entered data.
//!
//! The other half of the rule -- that a word really drawn twice at a seam is
//! still reported once -- is `text::tests::overlapping_fake_bold_spans_collapse_to_single_copy`,
//! which carries 0105.pdf's own spans through the same join.
//!
//! The document below uses Courier, whose every glyph advances 600/1000 of the
//! font size, so the positions are exact rather than nearly right.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::PdfDocument;

/// The value every field on the row holds, and its advance in Courier.
const VALUE: &str = "asdfASDF1234xyz";
const FIELD_FONT_SIZE: f64 = 22.0;
const COURIER_ADVANCE: f64 = 0.6;

/// Field box of Acroform-PDFBOX-2333's widest row, and the pitch between its
/// three columns. The value's advance (198 pt) is wider than the box, so each
/// field's text reaches 35 pt into the next field's column.
const FIELD_WIDTH: f64 = 141.7;
const FIELD_HEIGHT: f64 = 22.3;
const COLUMN_PITCH: f64 = 163.0;
const FIRST_COLUMN_X: f64 = 56.0;
const ROW_Y: f64 = 565.0;

/// Where the value starts inside the field box, as the document writes it.
const TEXT_INSET_X: f64 = 2.0;
const TEXT_INSET_Y: f64 = 6.9;

fn courier(doc: &mut Document) -> Object {
    Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Courier".to_vec()),
    })))
}

/// A one-page PDF with `count` text-field widgets on one row, each holding
/// `VALUE` in its `/AP /N` appearance stream. The page itself draws nothing:
/// the values are the page's only text, as they are on a filled form.
fn row_of_filled_fields(count: usize) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = courier(&mut doc);

    let mut widgets = Vec::new();
    for index in 0..count {
        let left = FIRST_COLUMN_X + COLUMN_PITCH * index as f64;
        let appearance = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new(
                    "Tf",
                    vec![Object::Name(b"F1".to_vec()), FIELD_FONT_SIZE.into()],
                ),
                Operation::new("Td", vec![TEXT_INSET_X.into(), TEXT_INSET_Y.into()]),
                Operation::new("Tj", vec![Object::string_literal(VALUE)]),
                Operation::new("ET", vec![]),
            ],
        }
        .encode()
        .expect("encode appearance");

        // BBox and Rect are the same size, so the appearance is placed without
        // a scale and the glyph positions are the ones written above.
        let appearance_id = doc.add_object(Object::Stream(Stream::new(
            dictionary! {
                "Type" => Object::Name(b"XObject".to_vec()),
                "Subtype" => Object::Name(b"Form".to_vec()),
                "BBox" => Object::Array(vec![
                    0.into(), 0.into(), FIELD_WIDTH.into(), FIELD_HEIGHT.into(),
                ]),
                "Resources" => Object::Dictionary(dictionary! {
                    "Font" => Object::Dictionary(dictionary! { "F1" => font.clone() }),
                }),
            },
            appearance,
        )));

        widgets.push(Object::Reference(doc.add_object(Object::Dictionary(
            dictionary! {
                "Type" => Object::Name(b"Annot".to_vec()),
                "Subtype" => Object::Name(b"Widget".to_vec()),
                "FT" => Object::Name(b"Tx".to_vec()),
                "T" => Object::string_literal(format!("field{index}")),
                "V" => Object::string_literal(VALUE),
                "Rect" => Object::Array(vec![
                    left.into(),
                    ROW_Y.into(),
                    (left + FIELD_WIDTH).into(),
                    (ROW_Y + FIELD_HEIGHT).into(),
                ]),
                "AP" => Object::Dictionary(dictionary! { "N" => Object::Reference(appearance_id) }),
            },
        ))));
    }

    let empty = doc.add_object(Object::Stream(Stream::new(dictionary! {}, Vec::new())));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![
            0.into(), 0.into(), 612.into(), 792.into(),
        ]),
        "Contents" => Object::Reference(empty),
        "Resources" => Object::Dictionary(dictionary! {}),
        "Annots" => Object::Array(widgets),
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

#[test]
fn three_fields_on_one_row_all_reach_the_text() {
    // The advance is wider than the field box, which is what makes the spans
    // touch; a fixture whose text fits its box would prove nothing.
    let advance = VALUE.chars().count() as f64 * FIELD_FONT_SIZE * COURIER_ADVANCE;
    assert!(
        advance > COLUMN_PITCH,
        "fixture no longer overruns its column: {advance} pt in a {COLUMN_PITCH} pt pitch"
    );

    let text = text_of(row_of_filled_fields(3));
    assert_eq!(
        text.matches(VALUE).count(),
        3,
        "a filled field's value was dropped from the row: {text:?}"
    );
}
