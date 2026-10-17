// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The dump says where each page draws its images (/9, #636).
//!
//! A caption sits beside a picture, and the text device used to drop every
//! image it was handed. Measured as an upper bound on DocLayNet's validation
//! pages, a line model that knows where the pictures are reads captions at
//! F1 0.812 against 0.681 without. So the device records the box each image
//! XObject is drawn into, and the dump carries it.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_learned::dump_pdf;
use serde_json::Value;

/// One page: a caption line of text and a 40x30-pixel grey image drawn into the box
/// `x 72..272, y 500..600` by `cm`.
fn page_with_an_image() -> Vec<u8> {
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
    let operations = vec![
        Operation::new("q", vec![]),
        Operation::new(
            "cm",
            vec![
                200.into(),
                0.into(),
                0.into(),
                100.into(),
                72.into(),
                500.into(),
            ],
        ),
        Operation::new("Do", vec![Object::Name(b"Im1".to_vec())]),
        Operation::new("Q", vec![]),
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 10.into()]),
        Operation::new("Td", vec![72.into(), 485.into()]),
        Operation::new(
            "Tj",
            vec![Object::string_literal("Figure 1. The plant at night.")],
        ),
        Operation::new("ET", vec![]),
    ];
    let content = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        Content { operations }.encode().expect("encode"),
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
        Object::Dictionary(
            dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 },
        ),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save pdf");
    out
}

#[test]
fn a_page_says_where_its_image_is() {
    let record: Value = serde_json::from_str(
        dump_pdf(page_with_an_image(), None)
            .expect("dump")
            .lines()
            .next()
            .expect("one record"),
    )
    .expect("a record is JSON");
    let images = record["images"].as_array().expect("images is an array");
    assert_eq!(images.len(), 1, "one image on the page: {images:?}");
    let got: Vec<f64> = images[0]
        .as_array()
        .expect("a box")
        .iter()
        .map(|v| v.as_f64().expect("a number"))
        .collect();
    // In the frame of the blocks: PDF user space, y up. MORE THAN ONE PIXEL,
    // because the interpreter's transform maps the image's pixel grid and not
    // the unit square: a 1x1 image hid that, and the first dump recorded every
    // photograph as a one-point dot.
    for (value, want) in got.iter().zip([72.0, 500.0, 272.0, 600.0]) {
        assert!(
            (value - want).abs() < 0.5,
            "the image's box is {got:?}, drawn at [72, 500, 272, 600]"
        );
    }
    // And the caption is below it, in the same frame, which is the whole use.
    let caption_top = record["blocks"][0]["bbox"][3].as_f64().expect("a block");
    assert!(caption_top <= 500.0 + 0.5, "caption top {caption_top}");
}
