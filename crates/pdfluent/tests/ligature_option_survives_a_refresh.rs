//! `keep_ligatures(true)` still holds after an operation that re-parses the
//! document (#437, second review finding).
//!
//! Every in-place mutation on the facade -- a redaction, a rotation, a
//! watermark, an explicit `sync_engine()` -- writes the lopdf side out and
//! builds a fresh engine from the bytes. A fresh engine folds, so a caller who
//! had asked for the file's own characters got them until the first such call
//! and folded text after it, with nothing in between to say why.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdfluent::PdfDocument;

/// The word `oﬃce hours`, written the way a PDF text string carries anything
/// outside Latin-1: UTF-16BE behind a byte-order mark.
fn utf16be_with_bom(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xFE, 0xFF];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    bytes
}

/// A one-page document whose only text is a push button's caption. Small on
/// purpose: the fold itself is tested against drawn glyphs in
/// `pdf-engine/tests/ligatures_fold_to_their_letters.rs`, and what this file
/// needs is a document the facade will happily re-parse.
fn document_whose_only_text_is(caption: &str) -> Vec<u8> {
    let mut doc = Document::with_version("1.4");

    let catalog_id = doc.new_object_id();
    let pages_id = doc.new_object_id();
    let page_id = doc.new_object_id();
    let acroform_id = doc.new_object_id();
    let content_id = doc.new_object_id();
    let widget_id = doc.new_object_id();

    doc.objects.insert(
        content_id,
        Object::Stream(Stream::new(dictionary! {}, Vec::new())),
    );
    doc.objects.insert(
        widget_id,
        Object::Dictionary(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "FT" => "Btn",
            "Ff" => Object::Integer(1 << 16),
            "T" => Object::String(b"button".to_vec(), StringFormat::Literal),
            "MK" => dictionary! {
                "CA" => Object::String(utf16be_with_bom(caption), StringFormat::Literal),
            },
            "Rect" => Object::Array(vec![100.into(), 700.into(), 260.into(), 730.into()]),
            "P" => Object::Reference(page_id),
        }),
    );
    doc.objects.insert(
        page_id,
        Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
            "Annots" => Object::Array(vec![Object::Reference(widget_id)]),
            "Contents" => Object::Reference(content_id),
        }),
    );
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
            "Count" => Object::Integer(1),
        }),
    );
    doc.objects.insert(
        acroform_id,
        Object::Dictionary(dictionary! {
            "Fields" => Object::Array(vec![Object::Reference(widget_id)]),
        }),
    );
    doc.objects.insert(
        catalog_id,
        Object::Dictionary(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
            "AcroForm" => Object::Reference(acroform_id),
        }),
    );
    doc.trailer.set("Root", Object::Reference(catalog_id));

    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save the fixture");
    bytes
}

const LIGATURE: &str = "o\u{FB03}ce hours";
const FOLDED: &str = "office hours";

#[test]
fn the_default_folds_and_survives_a_refresh() {
    let mut doc = PdfDocument::from_bytes(&document_whose_only_text_is(LIGATURE)).expect("open");
    assert!(doc.text().expect("text").contains(FOLDED));

    doc.sync_engine().expect("refresh the engine");
    assert!(
        doc.text().expect("text").contains(FOLDED),
        "the default stopped folding after a refresh"
    );
}

#[test]
fn keep_ligatures_survives_a_refresh() {
    let mut doc = PdfDocument::from_bytes(&document_whose_only_text_is(LIGATURE)).expect("open");
    doc.keep_ligatures(true);
    assert!(doc.text().expect("text").contains(LIGATURE));

    doc.sync_engine().expect("refresh the engine");
    assert!(
        doc.text().expect("text").contains(LIGATURE),
        "the refresh reset keep_ligatures and folded behind the caller's back"
    );
}
