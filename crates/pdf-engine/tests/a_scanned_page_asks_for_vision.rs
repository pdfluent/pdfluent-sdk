//! A page that draws ink and yields no text asks for a vision model (#624).
//!
//! The flag exists so an agent can route the pages we cannot read to something
//! that can, and the whole value of it is that it is honest: it must fire on a
//! scan and stay quiet on a page we read perfectly well. The three cases here are
//! the three that decide that.
//!
//! WHY A PAGE THAT DRAWS NOTHING IS THE THIRD CASE. An empty page yields no text
//! either, so a flag that asked only about text would send every blank page to a
//! vision model -- a call that costs money and returns nothing. It is the
//! combination, silence AND ink, that means "there is something here we cannot
//! read".
//!
//! The scan is built rather than shipped: an image XObject drawn over the page
//! with no text operator anywhere, which is what a scanner produces. On the real
//! corpus the class is 25 of the 200 pages of the DocLayNet round of 24-09-2026,
//! and on those pages neither we nor Docling extract anything at all.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::PdfDocument;

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

/// A scan: one image over the whole page, no text operator at all.
fn scanned_page() -> Vec<u8> {
    let mut built = Document::with_version("1.7");
    // A 2x2 greyscale image is enough: what matters is that the page DRAWS one.
    let img = built.add_object(Object::Stream(Stream::new(
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
    let ops = vec![
        Operation::new("q", vec![]),
        Operation::new(
            "cm",
            vec![
                612.into(),
                0.into(),
                0.into(),
                792.into(),
                0.into(),
                0.into(),
            ],
        ),
        Operation::new("Do", vec![Object::Name(b"Im0".to_vec())]),
        Operation::new("Q", vec![]),
    ];
    finish(
        built,
        ops,
        dictionary! {
            "XObject" => Object::Dictionary(dictionary! { "Im0" => Object::Reference(img) }),
        },
    )
}

/// Born digital: a paragraph of real text, no image.
fn born_digital_page() -> Vec<u8> {
    let mut built = Document::with_version("1.7");
    let fid = built.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
    }));
    let ops = vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 12.into()]),
        Operation::new("Td", vec![72.into(), 700.into()]),
        Operation::new(
            "Tj",
            vec![Object::String(
                b"The quick brown fox jumps over the lazy dog and keeps going for a while."
                    .to_vec(),
                lopdf::StringFormat::Literal,
            )],
        ),
        Operation::new("ET", vec![]),
    ];
    finish(
        built,
        ops,
        dictionary! {
            "Font" => Object::Dictionary(dictionary! { "F1" => Object::Reference(fid) }),
        },
    )
}

#[test]
fn a_scanned_page_asks_for_vision() {
    let doc = PdfDocument::open(scanned_page()).expect("open the scan");
    let advice = doc.vision_advice().expect("advice");
    assert_eq!(advice.len(), 1, "one page, one verdict");
    assert!(
        advice[0].no_text_layer,
        "a page that draws an image and yields no text was not flagged: {:?}",
        advice[0]
    );
    assert!(advice[0].needs_vision(), "{:?}", advice[0]);
}

#[test]
fn a_born_digital_page_does_not() {
    let doc = PdfDocument::open(born_digital_page()).expect("open the page");
    let advice = doc.vision_advice().expect("advice");
    assert_eq!(advice.len(), 1);
    assert!(
        !advice[0].no_text_layer,
        "a page of plain text was sent to a vision model: {:?}",
        advice[0]
    );
    assert!(!advice[0].needs_vision(), "{:?}", advice[0]);
}

#[test]
fn a_blank_page_does_not_either() {
    let blank = finish(Document::with_version("1.7"), Vec::new(), dictionary! {});
    let doc = PdfDocument::open(blank).expect("open the blank");
    let advice = doc.vision_advice().expect("advice");
    assert_eq!(advice.len(), 1);
    assert!(
        !advice[0].needs_vision(),
        "an empty page was sent to a vision model, which costs a call and \
         returns nothing: {:?}",
        advice[0]
    );
}

/// The model's own number reaches the caller, and nothing invents one (#624).
///
/// `confidence` is the whole of what distinguishes us from a parser that only
/// guesses: we say where we are unsure. That is worth nothing if the number is
/// dropped on the way out, and worse than nothing if a block no model considered
/// carries a confidence somebody made up. So this asks for both halves on a real
/// page: at least one block carries a probability, and every probability that is
/// there is one the model could have produced.
///
/// Red if `apply_traced` stops carrying `p_item`: no block has a confidence at
/// all, and the flag that tells a caller where to look twice is silently gone.
#[test]
fn a_block_the_model_judged_carries_its_probability() {
    const PAGE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/doclaynet/nasdaq_wwd_2002_p7.pdf"
    );
    let doc = PdfDocument::open(std::fs::read(PAGE).expect("fixture")).expect("open");
    let blocks = doc
        .extract_blocks(&pdf_engine::ExtractOptions::default())
        .expect("blocks");
    let judged: Vec<f64> = blocks.iter().filter_map(|b| b.confidence).collect();
    assert!(
        !judged.is_empty(),
        "no block on a page the list model acts on carried a confidence; \
         {} block(s) came back",
        blocks.len()
    );
    for p in &judged {
        assert!(
            (0.0..=1.0).contains(p),
            "a confidence outside 0..=1 reached the caller: {p}"
        );
    }
    // And the blocks nobody judged say so rather than reading zero.
    assert!(
        blocks.iter().any(|b| b.confidence.is_none()),
        "every block carried a confidence, which on this page means something \
         invented one: only the list model speaks here"
    );
}
