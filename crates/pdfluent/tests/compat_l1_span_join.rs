//! Compat L1: per-glyph engine spans join into content runs (#643).
//!
//! The engine hands the compat lane one span per drawn glyph, while the
//! recorded lane reads one span per content run: `Test page` is a single
//! span, two words, one line of text. Without the join the same page reads
//! as nine one-glyph spans and eight one-character words.
//!
//! The fixture is built in-memory via `lopdf` so the test stays
//! self-contained: one Helvetica line, the same bytes the committed
//! `tests/corpus-mini/simple.pdf` carries for its text.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::{dictionary, Document, Object, Stream};
use pdfluent::compat::{Page, TextFormat, TextOptions, TextOutput, WordOptions};
use pdfluent::prelude::*;

/// One page carrying `Test page` in Helvetica 12 at the simple fixture's spot.
fn two_word_pdf() -> Vec<u8> {
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
    let content = b"BT\n/F1 12 Tf\n100 700 Td\n(Test page) Tj\nET\n".to_vec();
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
fn glyph_spans_join_into_words() {
    let doc = PdfDocument::from_bytes(&two_word_pdf()).expect("fixture opens");
    let page = Page::new(&doc, 0);
    let words = page
        .get_text_words(&WordOptions::default())
        .expect("words extract");
    let texts: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
    assert_eq!(texts, vec!["Test", "page"]);
    assert_eq!(words[0].word_no, 0);
    assert_eq!(words[1].word_no, 1);
}

#[test]
fn joining_keeps_plain_text() {
    let doc = PdfDocument::from_bytes(&two_word_pdf()).expect("fixture opens");
    let page = Page::new(&doc, 0);
    let options = TextOptions::default().with_format(TextFormat::Text);
    let output = page.get_text(&options).expect("text extracts");
    assert_eq!(output, TextOutput::Text("Test page\n".to_string()));
}
