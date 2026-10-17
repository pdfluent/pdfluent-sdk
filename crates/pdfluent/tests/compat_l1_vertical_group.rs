//! Compat L1: quarter-turned spans group along the reading direction (#643).
//!
//! The engine hands the lane one span per drawn glyph; a word set up the
//! page arrives as one span per letter. The recordings read it as one line
//! in reading order: the f1040's turned `Form` is a single line, direction
//! (0, -1), not four lines reading `m`, `r`, `o`, `F` down the page.
//!
//! The fixture is built in-memory via `lopdf` so the test stays
//! self-contained: `Form` in Helvetica 7 under a rotation-only matrix.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::{dictionary, Document, Object, Stream};
use pdfluent::compat::{Page, TextFormat, TextOptions, TextOutput, WordOptions};
use pdfluent::prelude::*;

/// One page drawing `Form` up the page: size from Tf, rotation-only matrix.
fn turned_pdf() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id },
    });
    let content = b"BT\n/F1 7 Tf\n0 1 -1 0 200 700 Tm\n(Form) Tj\nET\n".to_vec();
    let content_id = doc.add_object(Stream::new(dictionary! {}, content));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => resources_id,
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("in-memory fixture saves");
    bytes
}

#[test]
fn turned_glyphs_read_in_order() {
    let doc = PdfDocument::from_bytes(&turned_pdf()).expect("fixture opens");
    let page = Page::new(&doc, 0);
    let options = TextOptions::default().with_format(TextFormat::Text);
    let output = page.get_text(&options).expect("text extracts");
    assert_eq!(output, TextOutput::Text("Form\n".to_string()));
}

#[test]
fn turned_glyphs_join_into_words() {
    let doc = PdfDocument::from_bytes(&turned_pdf()).expect("fixture opens");
    let page = Page::new(&doc, 0);
    let words = page
        .get_text_words(&WordOptions::default())
        .expect("words extract");
    let texts: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
    assert_eq!(texts, vec!["Form"]);
}
