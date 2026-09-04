//! Image XObject redaction tests — #1293 (M5-REDACT-01).
//!
//! Validates that Image XObjects whose page-space bounding box overlaps a
//! redaction area are replaced with all-black FlateDecode pixel data, and
//! that unsupported filters (JBIG2Decode) return UnsupportedImageFilter.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::{dictionary, Document, Object, ObjectId, Stream};
use pdf_redact::{RedactError, RedactionArea, Redactor};

/// Build a document with a single Image XObject placed at `(x, y)` with
/// pixel dimensions `w × h`.  Returns `(doc, img_id)`.
fn make_doc_with_image(x: f64, y: f64, w: f64, h: f64, filter: &str) -> (Document, ObjectId) {
    let mut doc = Document::with_version("1.7");

    let img_stream = Stream::new(
        dictionary! {
            "Type"             => "XObject",
            "Subtype"          => "Image",
            "Width"            => 4_i64,
            "Height"           => 4_i64,
            "ColorSpace"       => "DeviceRGB",
            "BitsPerComponent" => 8_i64,
            "Filter"           => Object::Name(filter.as_bytes().to_vec()),
        },
        vec![0xFFu8; 48], // fake pixel bytes (4×4×3)
    );
    let img_id = doc.add_object(Object::Stream(img_stream));

    // `w 0 0 h x y cm /Im0 Do` places the image at (x, y) with size w×h.
    let content = format!("{w} 0 0 {h} {x} {y} cm /Im0 Do");
    let content_stream = Stream::new(dictionary! {}, content.into_bytes());
    let content_id = doc.add_object(Object::Stream(content_stream));

    let resources = dictionary! {
        "XObject" => dictionary! {
            "Im0" => Object::Reference(img_id),
        },
    };
    let page_dict = dictionary! {
        "Type"      => "Page",
        "MediaBox"  => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents"  => Object::Reference(content_id),
        "Resources" => resources,
    };
    let page_id = doc.add_object(Object::Dictionary(page_dict));

    let pages_dict = dictionary! {
        "Type"  => "Pages",
        "Kids"  => vec![Object::Reference(page_id)],
        "Count" => 1_i64,
    };
    let pages_id = doc.add_object(Object::Dictionary(pages_dict));

    if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
        d.set("Parent", Object::Reference(pages_id));
    }

    let catalog = dictionary! {
        "Type"  => "Catalog",
        "Pages" => Object::Reference(pages_id),
    };
    let catalog_id = doc.add_object(Object::Dictionary(catalog));
    doc.trailer.set("Root", Object::Reference(catalog_id));

    (doc, img_id)
}

/// An image whose bounding box overlaps the redaction area must be replaced
/// with a FlateDecode all-black image of the same dimensions.
#[test]
fn redact_image_replaces_with_black() {
    // Image at (50, 400) with size 100×150 points; redaction area covers it.
    let (mut doc, img_id) = make_doc_with_image(50.0, 400.0, 100.0, 150.0, "DCTDecode");
    let mut redactor = Redactor::new();
    redactor.mark(RedactionArea::new(1, [40.0, 390.0, 160.0, 560.0]));

    redactor.apply(&mut doc).expect("apply should succeed");

    let stream = match doc.get_object(img_id) {
        Ok(Object::Stream(s)) => s.clone(),
        _ => panic!("image XObject should still be a stream"),
    };
    assert_eq!(
        stream.dict.get(b"Filter").unwrap(),
        &Object::Name(b"FlateDecode".to_vec()),
        "filter should be FlateDecode after blackout"
    );
    assert!(
        !stream.content.is_empty(),
        "replacement content must be non-empty"
    );
}

/// An image whose bounding box does NOT overlap the redaction area must be
/// left unchanged.
#[test]
fn redact_image_outside_area_is_unchanged() {
    // Image at (50, 400); redaction area is far away at (300, 300)-(400, 400).
    let (mut doc, img_id) = make_doc_with_image(50.0, 400.0, 100.0, 150.0, "DCTDecode");
    let original_content: Vec<u8> = match doc.get_object(img_id) {
        Ok(Object::Stream(s)) => s.content.clone(),
        _ => panic!("expected stream"),
    };

    let mut redactor = Redactor::new();
    redactor.mark(RedactionArea::new(1, [300.0, 300.0, 400.0, 400.0]));
    redactor.apply(&mut doc).expect("apply should succeed");

    let after_content: Vec<u8> = match doc.get_object(img_id) {
        Ok(Object::Stream(s)) => s.content.clone(),
        _ => panic!("expected stream"),
    };
    assert_eq!(
        original_content, after_content,
        "image outside redaction area must be unchanged"
    );
}

/// A JBIG2Decode image inside a redaction area must return UnsupportedImageFilter.
#[test]
fn redact_image_jbig2_returns_unsupported_error() {
    let (mut doc, _) = make_doc_with_image(50.0, 400.0, 100.0, 150.0, "JBIG2Decode");
    let mut redactor = Redactor::new();
    redactor.mark(RedactionArea::new(1, [40.0, 390.0, 160.0, 560.0]));

    let result = redactor.apply(&mut doc);
    assert!(
        matches!(result, Err(RedactError::UnsupportedImageFilter(_))),
        "JBIG2Decode image in redaction area should return UnsupportedImageFilter, got: {result:?}"
    );
}
