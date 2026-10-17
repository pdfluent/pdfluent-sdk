// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A caption under a picture is a caption, and only with the switch on (#636).
//!
//! The operator test for `captions`, RED with the switch off and red without
//! the picture boxes: the caption model reads where a line stands against the
//! figures the page places, and a page the engine reads without them has no
//! caption at all.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::learned::{refine_with_images, Switches, SWITCHES};
use pdf_engine::{BlockKind, PdfDocument};

const CAPTION: &str = "Figure 1. The treatment plant at night, seen from the river bank.";

fn text(ops: &mut Vec<Operation>, line: &str, x: f64, y: f64) {
    ops.push(Operation::new("BT", vec![]));
    ops.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F1".to_vec()), 10.into()],
    ));
    ops.push(Operation::new("Td", vec![x.into(), y.into()]));
    ops.push(Operation::new("Tj", vec![Object::string_literal(line)]));
    ops.push(Operation::new("ET", vec![]));
}

/// A page of body text with a photograph in it and its caption under it.
fn page() -> Vec<u8> {
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
        text(&mut ops, line, 72.0, y);
        y -= 14.0;
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
    text(&mut ops, CAPTION, 72.0, 445.0);
    y = 410.0;
    for line in [
        "The second stage filters what the first has settled, and the third",
        "treats it with light before it leaves the plant for the river again.",
    ] {
        text(&mut ops, line, 72.0, y);
        y -= 14.0;
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

fn kind_of_caption(captions: bool, with_pictures: bool) -> Option<BlockKind> {
    let doc = PdfDocument::open(page()).expect("open");
    let (blocks, rules) = doc
        .extract_blocks_and_rules_before_learning()
        .expect("blocks");
    let images = if with_pictures {
        doc.extract_image_boxes().expect("images")
    } else {
        Vec::new()
    };
    let switches = Switches {
        captions,
        ..SWITCHES
    };
    let out = refine_with_images(blocks, &[(612.0, 792.0)], &rules, &[], &images, switches);
    out.iter()
        .find(|b| b.text.contains("treatment plant at night"))
        .map(|b| b.kind)
}

#[test]
fn a_caption_under_a_picture_is_a_caption() {
    assert_eq!(
        kind_of_caption(true, true),
        Some(BlockKind::Caption),
        "the line under the photograph should be read as its caption"
    );
}

#[test]
fn and_only_with_the_switch_on() {
    assert_ne!(
        kind_of_caption(false, true),
        Some(BlockKind::Caption),
        "with the switch off nothing may become a caption"
    );
}

#[test]
fn the_markdown_sets_it_apart() {
    let doc = PdfDocument::open(page()).expect("open");
    let (blocks, rules) = doc
        .extract_blocks_and_rules_before_learning()
        .expect("blocks");
    let images = doc.extract_image_boxes().expect("images");
    let switches = Switches {
        captions: true,
        ..SWITCHES
    };
    let out = refine_with_images(blocks, &[(612.0, 792.0)], &rules, &[], &images, switches);
    let md = pdf_engine::to_markdown(&out);
    assert!(md.contains(&format!("*{CAPTION}*")), "{md}");
}
