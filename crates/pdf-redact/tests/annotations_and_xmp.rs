//! Annotation field stripping + XMP metadata cleanup tests — #1294 (M5-REDACT-02).
//!
//! Validates that:
//! - `/Contents` and `/T` are stripped from annotations that overlap a
//!   redacted region.
//! - The catalog `/Metadata` XMP stream is replaced with a minimal stub that
//!   only preserves `pdf:Producer`.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::{dictionary, Document, Object, ObjectId, Stream};
use pdf_redact::{RedactionArea, Redactor};

/// Build a document with one highlight annotation at `annot_rect` containing
/// the given `/Contents` text.  Returns `(doc, annot_id)`.
fn make_doc_with_annotation(annot_rect: [f32; 4], contents: &str) -> (Document, ObjectId) {
    let mut doc = Document::with_version("1.7");

    let content_str = format!(
        "BT /F1 12 Tf {} {} Td (Secret) Tj ET",
        annot_rect[0], annot_rect[1]
    );
    let content_stream = Stream::new(dictionary! {}, content_str.into_bytes());
    let content_id = doc.add_object(Object::Stream(content_stream));

    let annot_dict = dictionary! {
        "Type"     => "Annot",
        "Subtype"  => "Highlight",
        "Rect"     => vec![
            Object::Real(annot_rect[0]),
            Object::Real(annot_rect[1]),
            Object::Real(annot_rect[2]),
            Object::Real(annot_rect[3]),
        ],
        "Contents" => Object::String(
            contents.as_bytes().to_vec(),
            lopdf::StringFormat::Literal,
        ),
        "T" => Object::String(b"Author".to_vec(), lopdf::StringFormat::Literal),
    };
    let annot_id = doc.add_object(Object::Dictionary(annot_dict));

    let page_dict = dictionary! {
        "Type"     => "Page",
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => Object::Reference(content_id),
        "Annots"   => vec![Object::Reference(annot_id)],
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

    (doc, annot_id)
}

/// Build a document with an XMP /Metadata stream in the catalog.
fn make_doc_with_xmp(xmp_bytes: &[u8]) -> Document {
    let mut doc = Document::with_version("1.7");

    let content_stream = Stream::new(dictionary! {}, b"BT (Hello) Tj ET".to_vec());
    let content_id = doc.add_object(Object::Stream(content_stream));

    let page_dict = dictionary! {
        "Type"     => "Page",
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => Object::Reference(content_id),
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

    let xmp_stream = Stream::new(
        dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
        xmp_bytes.to_vec(),
    );
    let xmp_id = doc.add_object(Object::Stream(xmp_stream));

    let catalog = dictionary! {
        "Type"     => "Catalog",
        "Pages"    => Object::Reference(pages_id),
        "Metadata" => Object::Reference(xmp_id),
    };
    let catalog_id = doc.add_object(Object::Dictionary(catalog));
    doc.trailer.set("Root", Object::Reference(catalog_id));

    doc
}

/// An annotation that overlaps the redaction area must have /Contents and /T
/// stripped.
#[test]
fn annotations_and_xmp_annotation_contents_stripped() {
    let (mut doc, annot_id) =
        make_doc_with_annotation([90.0, 690.0, 200.0, 720.0], "Sensitive note");
    let mut redactor = Redactor::new();
    redactor.mark(RedactionArea::new(1, [80.0, 680.0, 210.0, 730.0]));

    redactor.apply(&mut doc).expect("apply should succeed");

    let annot_dict = match doc.get_object(annot_id) {
        Ok(Object::Dictionary(ref d)) => d.clone(),
        _ => panic!("annotation should still exist as a dict"),
    };
    assert!(
        annot_dict.get(b"Contents").is_err(),
        "/Contents must be removed from an overlapping annotation"
    );
    assert!(
        annot_dict.get(b"T").is_err(),
        "/T must be removed from an overlapping annotation"
    );
}

/// An annotation that does NOT overlap the redaction area must keep its fields.
#[test]
fn annotations_and_xmp_annotation_outside_area_intact() {
    // Annotation at [400, 700, 500, 750]; redaction at [0, 0, 100, 100].
    let (mut doc, annot_id) = make_doc_with_annotation([400.0, 700.0, 500.0, 750.0], "Safe note");
    let mut redactor = Redactor::new();
    redactor.mark(RedactionArea::new(1, [0.0, 0.0, 100.0, 100.0]));

    redactor.apply(&mut doc).expect("apply should succeed");

    let annot_dict = match doc.get_object(annot_id) {
        Ok(Object::Dictionary(ref d)) => d.clone(),
        _ => panic!("annotation should still exist as a dict"),
    };
    assert!(
        annot_dict.get(b"Contents").is_ok(),
        "/Contents of an annotation outside the redaction area must be preserved"
    );
}

/// After redaction the catalog /Metadata stream must contain only
/// `pdf:Producer` and no PII fields (dc:title, dc:creator, etc.).
#[test]
fn annotations_and_xmp_xmp_replaced_with_minimal() {
    let full_xmp = b"<?xpacket begin=\"\" id=\"W\"?>\
        <x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\
          <rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\
            <rdf:Description xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\
              <dc:title>Top Secret Document</dc:title>\
              <dc:creator>John Doe</dc:creator>\
            </rdf:Description>\
          </rdf:RDF>\
        </x:xmpmeta>\
        <?xpacket end=\"w\"?>";

    let mut doc = make_doc_with_xmp(full_xmp);
    let mut redactor = Redactor::new();
    redactor.mark(RedactionArea::new(1, [0.0, 0.0, 100.0, 100.0]));
    redactor.apply(&mut doc).expect("apply should succeed");

    // Resolve the /Metadata stream from the catalog.
    let root_id = match doc.trailer.get(b"Root") {
        Ok(Object::Reference(id)) => *id,
        _ => panic!("trailer must have /Root"),
    };
    let catalog = match doc.get_object(root_id) {
        Ok(Object::Dictionary(ref d)) => d.clone(),
        _ => panic!("catalog must be a dict"),
    };
    let meta_id = match catalog.get(b"Metadata") {
        Ok(Object::Reference(id)) => *id,
        _ => panic!("/Metadata reference must exist in catalog"),
    };
    let meta_stream = match doc.get_object(meta_id) {
        Ok(Object::Stream(s)) => s.clone(),
        _ => panic!("/Metadata must be a stream"),
    };

    let content = std::str::from_utf8(&meta_stream.content).expect("XMP must be UTF-8");
    assert!(
        content.contains("pdf:Producer"),
        "minimal XMP must contain pdf:Producer"
    );
    assert!(
        !content.contains("dc:title"),
        "dc:title must be removed from XMP"
    );
    assert!(
        !content.contains("dc:creator"),
        "dc:creator must be removed from XMP"
    );
}
