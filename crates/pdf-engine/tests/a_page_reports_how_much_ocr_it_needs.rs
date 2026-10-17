//! A page reports how much OCR it needs: none, a full pass, or its regions.
//!
//! [`OcrNeed`](pdf_engine::OcrNeed) is page shape, measured before any
//! recognizer runs: born-digital text reads as it is, a scan wants the whole
//! page recognized, and a mix wants only its image regions. The fixtures are
//! built rather than shipped -- a 2x2 image drawn over the page is what a
//! scanner produces, and a line of Helvetica is what a typesetter does.
//!
//! The edge cases pin what does NOT count as a text layer: an invisible layer
//! (render mode 3) may be a stale OCR pass rather than the page's words, and
//! glyphs with no usable mapping are not words at all. Both still report Full
//! under a page-sized image. And a text page with a small figure reports None,
//! because a logo is not a region.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::{OcrNeed, PdfDocument};

/// One page of `ops` with `resources`, in a document that already holds whatever
/// objects those resources point at.
fn finish(mut doc: Document, ops: Vec<Operation>, resources: lopdf::Dictionary) -> Vec<u8> {
    let content = Content { operations: ops }
        .encode()
        .expect("encode content");
    let stream_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![
            Object::Integer(0), Object::Integer(0),
            Object::Integer(612), Object::Integer(792),
        ]),
        "Contents" => Object::Reference(stream_id),
        "Resources" => Object::Dictionary(resources),
    }));
    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![Object::Reference(page_id)]),
        "Count" => Object::Integer(1),
    }));
    if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
        d.set("Parent", Object::Reference(pages_id));
    }
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save");
    out
}

/// A 2x2 greyscale image: what matters is that the page draws one, not what is
/// in it.
fn image_object(doc: &mut Document) -> Object {
    let img = doc.add_object(Object::Stream(Stream::new(
        dictionary! {
            "Type" => Object::Name(b"XObject".to_vec()),
            "Subtype" => Object::Name(b"Image".to_vec()),
            "Width" => Object::Integer(2),
            "Height" => Object::Integer(2),
            "ColorSpace" => Object::Name(b"DeviceGray".to_vec()),
            "BitsPerComponent" => Object::Integer(8),
        },
        vec![0x00, 0x40, 0x80, 0xff],
    )));
    Object::Reference(img)
}

/// A non-embedded Helvetica.
fn font_object(doc: &mut Document) -> Object {
    let fid = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    }));
    Object::Reference(fid)
}

/// `q a b c d e f cm /Im0 Do Q`: the image over one rectangle.
fn draw_image(a: i64, b: i64, c: i64, d: i64, e: i64, f: i64) -> Vec<Operation> {
    vec![
        Operation::new("q", vec![]),
        Operation::new(
            "cm",
            vec![a.into(), b.into(), c.into(), d.into(), e.into(), f.into()],
        ),
        Operation::new("Do", vec![Object::Name(b"Im0".to_vec())]),
        Operation::new("Q", vec![]),
    ]
}

/// One line of text at the top of the page, in 12pt Helvetica.
fn show_line(text: Vec<u8>) -> Vec<Operation> {
    vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 12.into()]),
        Operation::new("Td", vec![72.into(), 700.into()]),
        Operation::new(
            "Tj",
            vec![Object::String(text, lopdf::StringFormat::Literal)],
        ),
        Operation::new("ET", vec![]),
    ]
}

const PROSE: &[u8] = b"The quick brown fox jumps over the lazy dog and keeps going for a while.";

/// Born digital: a paragraph of real text, no image.
fn born_digital_page() -> Vec<u8> {
    let mut built = Document::with_version("1.7");
    let font = font_object(&mut built);
    finish(
        built,
        show_line(PROSE.to_vec()),
        dictionary! {
            "Font" => Object::Dictionary(dictionary! { "F1" => font }),
        },
    )
}

/// A scan: one image over the whole page, no text operator at all.
fn scanned_page() -> Vec<u8> {
    let mut built = Document::with_version("1.7");
    let img = image_object(&mut built);
    finish(
        built,
        draw_image(612, 0, 0, 792, 0, 0),
        dictionary! {
            "XObject" => Object::Dictionary(dictionary! { "Im0" => img }),
        },
    )
}

/// Text over the top half, an image over the bottom half, nothing shared.
fn mixed_page() -> Vec<u8> {
    let mut built = Document::with_version("1.7");
    let img = image_object(&mut built);
    let font = font_object(&mut built);
    let mut ops = show_line(PROSE.to_vec());
    ops.extend(draw_image(612, 0, 0, 396, 0, 0));
    finish(
        built,
        ops,
        dictionary! {
            "Font" => Object::Dictionary(dictionary! { "F1" => font }),
            "XObject" => Object::Dictionary(dictionary! { "Im0" => img }),
        },
    )
}

/// A scan with a full invisible text layer over it: searchable, but nothing
/// the eye can read.
fn invisible_layer_page() -> Vec<u8> {
    let mut built = Document::with_version("1.7");
    let img = image_object(&mut built);
    let font = font_object(&mut built);
    let mut ops = draw_image(612, 0, 0, 792, 0, 0);
    ops.push(Operation::new("BT", vec![]));
    ops.push(Operation::new("Tr", vec![Object::Integer(3)]));
    ops.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F1".to_vec()), 12.into()],
    ));
    ops.push(Operation::new("Td", vec![72.into(), 700.into()]));
    ops.push(Operation::new(
        "Tj",
        vec![Object::String(PROSE.to_vec(), lopdf::StringFormat::Literal)],
    ));
    ops.push(Operation::new("ET", vec![]));
    finish(
        built,
        ops,
        dictionary! {
            "Font" => Object::Dictionary(dictionary! { "F1" => font }),
            "XObject" => Object::Dictionary(dictionary! { "Im0" => img }),
        },
    )
}

/// A scan with seventy glyphs no encoding maps: control codes through
/// Helvetica, which reach the device as `.notdef` without a Unicode mapping.
fn garbage_glyphs_page() -> Vec<u8> {
    let mut built = Document::with_version("1.7");
    let img = image_object(&mut built);
    let font = font_object(&mut built);
    let mut ops = draw_image(612, 0, 0, 792, 0, 0);
    let garbage: Vec<u8> = (0..70u8).map(|i| 1 + (i % 8)).collect();
    ops.extend(show_line(garbage));
    finish(
        built,
        ops,
        dictionary! {
            "Font" => Object::Dictionary(dictionary! { "F1" => font }),
            "XObject" => Object::Dictionary(dictionary! { "Im0" => img }),
        },
    )
}

/// Text with a small figure: a 100-by-100 image is decoration, not a region.
fn text_with_small_figure_page() -> Vec<u8> {
    let mut built = Document::with_version("1.7");
    let img = image_object(&mut built);
    let font = font_object(&mut built);
    let mut ops = show_line(PROSE.to_vec());
    ops.extend(draw_image(100, 0, 0, 100, 500, 50));
    finish(
        built,
        ops,
        dictionary! {
            "Font" => Object::Dictionary(dictionary! { "F1" => font }),
            "XObject" => Object::Dictionary(dictionary! { "Im0" => img }),
        },
    )
}

fn need_of(pdf: &[u8]) -> OcrNeed {
    let doc = PdfDocument::open(pdf.to_vec()).expect("open");
    assert_eq!(doc.page_count(), 1);
    // The method and the free function are one verdict.
    let via_method = doc.page_needs_ocr(0);
    assert_eq!(via_method, pdf_engine::page_needs_ocr(&doc, 0));
    via_method
}

#[test]
fn a_text_page_needs_no_ocr() {
    assert_eq!(need_of(&born_digital_page()), OcrNeed::None);
}

#[test]
fn a_full_page_image_with_no_text_needs_full_ocr() {
    assert_eq!(need_of(&scanned_page()), OcrNeed::Full);
}

#[test]
fn text_beside_a_large_bare_image_needs_partial_ocr() {
    assert_eq!(need_of(&mixed_page()), OcrNeed::Partial);
}

#[test]
fn a_blank_page_needs_no_ocr() {
    let blank = finish(Document::with_version("1.7"), Vec::new(), dictionary! {});
    assert_eq!(need_of(&blank), OcrNeed::None);
}

#[test]
fn only_invisible_text_is_no_text_layer() {
    assert_eq!(need_of(&invisible_layer_page()), OcrNeed::Full);
}

#[test]
fn only_garbage_glyphs_are_no_text_layer() {
    assert_eq!(need_of(&garbage_glyphs_page()), OcrNeed::Full);
}

#[test]
fn a_small_figure_is_not_a_region() {
    assert_eq!(need_of(&text_with_small_figure_page()), OcrNeed::None);
}

#[test]
fn a_page_that_is_not_there_needs_no_ocr() {
    let doc = PdfDocument::open(born_digital_page()).expect("open");
    assert_eq!(doc.page_needs_ocr(7), OcrNeed::None);
}
