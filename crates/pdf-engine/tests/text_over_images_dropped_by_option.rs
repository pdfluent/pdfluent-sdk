//! Text over images is dropped when the caller asks (lane L4).
//!
//! A line drawn over a placed image, and one below it. By default both lines
//! are kept -- nothing drops text for standing over an image -- and only
//! `ExtractOptions::drop_text_over_images` drops the over-image line (G90).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::{ExtractOptions, PdfDocument};

/// A line drawn over a placed photograph, and one below it.
fn page_with_text_over_an_image() -> Vec<u8> {
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
    let mut ops = vec![
        Operation::new("q", vec![]),
        Operation::new(
            "cm",
            vec![
                300.into(),
                0.into(),
                0.into(),
                200.into(),
                72.into(),
                460.into(),
            ],
        ),
        Operation::new("Do", vec![Object::Name(b"Im1".to_vec())]),
        Operation::new("Q", vec![]),
    ];
    for (text, y) in [
        ("The line standing over the photograph.", 600.0),
        ("The line standing below it.", 430.0),
    ] {
        ops.push(Operation::new("BT", vec![]));
        ops.push(Operation::new(
            "Tf",
            vec![Object::Name(b"F1".to_vec()), 10.into()],
        ));
        ops.push(Operation::new("Td", vec![72.into(), y.into()]));
        ops.push(Operation::new("Tj", vec![Object::string_literal(text)]));
        ops.push(Operation::new("ET", vec![]));
    }
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
fn text_over_an_image_is_dropped_when_the_caller_asks() {
    let blocks = PdfDocument::open(page_with_text_over_an_image())
        .expect("open")
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    let texts: Vec<&str> = blocks.iter().map(|b| b.text.as_str()).collect();
    assert!(
        texts.iter().any(|t| t.contains("over the photograph")),
        "today the over-image line is kept; this pins that while the drop \
         option has no spelling: {texts:?}"
    );
    // G90: the over-picture line is dropped and the other is kept when the
    // caller asks for text over images to be dropped.
    let mut drop = ExtractOptions::default();
    drop.drop_text_over_images = true;
    let dropped = PdfDocument::open(page_with_text_over_an_image())
        .expect("open")
        .extract_blocks(&drop)
        .expect("blocks");
    let dropped: Vec<&str> = dropped.iter().map(|b| b.text.as_str()).collect();
    assert!(
        !dropped.iter().any(|t| t.contains("over the photograph")),
        "the over-image line survived the drop option: {dropped:?}"
    );
    assert!(
        dropped.iter().any(|t| t.contains("below it")),
        "the line below the image went with the dropped one: {dropped:?}"
    );
}
