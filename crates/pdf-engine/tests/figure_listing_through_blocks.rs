//! Figures are listed through the block model (lane L4).
//!
//! The figure-listing calls are gone, and with them every listing: a page of
//! a paragraph and a photograph comes back as the paragraph alone. The
//! rebuilt engine lists the figure beside the text, as its own block, so a
//! caller chunking the document meets the figure where the reader does.
//!
//! Only the count is pinned: one text block and one figure block. The front
//! door lists one picture box per kept image with its place in the text
//! (G57, G58); our block form is our own.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::{ExtractOptions, PdfDocument};

/// Three lines of body text over a placed photograph.
fn page_with_a_picture() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let image = doc.add_object(Object::Stream(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image", "Width" => 40, "Height" => 30,
            "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8,
        },
        vec![128u8; 40 * 30],
    )));
    let mut ops = Vec::new();
    let mut y = 740.0;
    for line in [
        "The plant treats the water of the whole valley before it is returned",
        "to the river, and runs through the night when the demand is lowest.",
        "Its capacity was doubled in the last year of the programme.",
    ] {
        ops.push(Operation::new("BT", vec![]));
        ops.push(Operation::new(
            "Tf",
            vec![Object::Name(b"F1".to_vec()), 10.into()],
        ));
        ops.push(Operation::new("Td", vec![72.into(), y.into()]));
        ops.push(Operation::new("Tj", vec![Object::string_literal(line)]));
        ops.push(Operation::new("ET", vec![]));
        y -= 12.0;
    }
    ops.push(Operation::new("q", vec![]));
    ops.push(Operation::new(
        "cm",
        vec![
            300.into(),
            0.into(),
            0.into(),
            200.into(),
            72.into(),
            460.into(),
        ],
    ));
    ops.push(Operation::new("Do", vec![Object::Name(b"Im1".to_vec())]));
    ops.push(Operation::new("Q", vec![]));
    let content = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        Content { operations: ops }.encode().expect("encode"),
    )));
    let pages_id = doc.new_object_id();
    let page = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "Contents" => content,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => dictionary! {
            "Font" => dictionary! { "F1" => font },
            "XObject" => dictionary! { "Im1" => image },
        },
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1,
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save pdf");
    out
}

#[test]
fn a_photograph_beside_a_paragraph_is_listed_as_a_block() {
    let blocks = PdfDocument::open(page_with_a_picture())
        .expect("open")
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    assert_eq!(
        blocks.len(),
        2,
        "a paragraph and a photograph came back as {} blocks: {:?}",
        blocks.len(),
        blocks.iter().map(|b| &b.text).collect::<Vec<_>>()
    );
}
