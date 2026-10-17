//! Clip margins keep the rectangle they leave, through the public path.
//!
//! `ExtractOptions.clip_margins` reads each page through the rectangle the
//! margins leave: content whose centre falls outside is dropped before
//! segmentation. The default (`None`) reads the whole page, exactly as
//! without the option.
//!
//! Courier throughout: every glyph advances 600/1000 of the font size, so
//! the line centres are what the arithmetic says they are.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::{ClipMargins, ExtractOptions, PdfDocument};

/// A one-page PDF whose content stream is `content`, with Courier as `/F1`.
fn page(content: &str) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Courier".to_vec()),
    }));
    let contents = doc.add_object(Stream::new(dictionary! {}, content.as_bytes().to_vec()));
    let pages = doc.new_object_id();
    let page = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => contents,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
    });
    doc.objects.insert(
        pages,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page.into()],
            "Count" => 1,
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
    doc.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save");
    bytes
}

fn markdown_of(content: &str, clip: Option<ClipMargins>) -> String {
    let doc = PdfDocument::open(page(content)).expect("open");
    let mut options = ExtractOptions::default();
    options.clip_margins = clip;
    doc.extract_markdown(&options).expect("markdown")
}

#[test]
fn the_default_reads_the_whole_page() {
    let markdown = markdown_of(
        "BT /F1 12 Tf 72 700 Td (TOP LINE) Tj ET BT /F1 12 Tf 72 350 Td (MIDDLE LINE) Tj ET BT /F1 12 Tf 72 72 Td (BOTTOM LINE) Tj ET",
        None,
    );
    assert!(markdown.contains("TOP LINE"), "got: {markdown}");
    assert!(markdown.contains("MIDDLE LINE"), "got: {markdown}");
    assert!(markdown.contains("BOTTOM LINE"), "got: {markdown}");
}

#[test]
fn a_top_margin_drops_what_it_covers() {
    let content = "BT /F1 12 Tf 72 700 Td (TOP LINE) Tj ET BT /F1 12 Tf 72 350 Td (MIDDLE LINE) Tj ET BT /F1 12 Tf 72 72 Td (BOTTOM LINE) Tj ET";
    let markdown = markdown_of(
        content,
        Some(ClipMargins::new(0.0, 300.0, 0.0, 0.0).expect("valid")),
    );
    assert!(!markdown.contains("TOP LINE"), "got: {markdown}");
    assert!(markdown.contains("MIDDLE LINE"), "got: {markdown}");
    assert!(markdown.contains("BOTTOM LINE"), "got: {markdown}");
}

#[test]
fn a_left_margin_drops_only_the_left() {
    let markdown = markdown_of(
        "BT /F1 12 Tf 72 400 Td (LEFT EDGE) Tj ET BT /F1 12 Tf 400 400 Td (RIGHT SIDE) Tj ET",
        Some(ClipMargins::new(300.0, 0.0, 0.0, 0.0).expect("valid")),
    );
    assert!(!markdown.contains("LEFT EDGE"), "got: {markdown}");
    assert!(markdown.contains("RIGHT SIDE"), "got: {markdown}");
}
