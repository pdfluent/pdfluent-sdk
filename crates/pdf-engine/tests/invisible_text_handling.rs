//! Hidden text stays out of the extraction (lane L1).
//!
//! Text the page hides -- render mode 3, which paints no glyphs -- is read
//! today exactly as the visible text is: there is no handling choice, so
//! nothing tells the two apart, and the hidden line lands in the blocks and
//! the Markdown beside what the reader sees. The rebuilt engine drops the
//! hidden line from both, the way the front door's own default engine drops
//! hidden lines from its Markdown, text and JSON (G165).
//!
//! This is about text the author hid. The invisible layer the engine itself
//! adds when it recognizes a scan is read, as the OCR tests pin.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::{ExtractOptions, PdfDocument};

/// A visible line over a hidden one, far enough apart to stay two blocks.
fn page_with_a_hidden_line() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let mut ops = Vec::new();
    for (text, y, mode) in [
        ("The line the reader sees.", 700.0, 0),
        ("The line the author hid.", 550.0, 3),
    ] {
        ops.push(Operation::new("Tr", vec![Object::Integer(mode)]));
        ops.push(Operation::new("BT", vec![]));
        ops.push(Operation::new(
            "Tf",
            vec![Object::Name(b"F1".to_vec()), 10.into()],
        ));
        ops.push(Operation::new("Td", vec![72.into(), y.into()]));
        ops.push(Operation::new("Tj", vec![Object::string_literal(text)]));
        ops.push(Operation::new("ET", vec![]));
    }
    ops.push(Operation::new("Tr", vec![Object::Integer(0)]));
    let content = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        Content { operations: ops }.encode().expect("encode"),
    )));
    let pages_id = doc.new_object_id();
    let page = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "Contents" => content,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
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

/// A visible line drawn as two runs, far enough apart to stay one block with
/// two spans: the shape that caught the drop rebuilding every block's text.
fn page_with_two_runs_on_one_line() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let bold = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica-Bold",
    });
    let mut ops = Vec::new();
    for (face, text, x) in [("F1", "Hello ", 72.0), ("F2", "world", 99.0)] {
        ops.push(Operation::new("BT", vec![]));
        ops.push(Operation::new(
            "Tf",
            vec![Object::Name(face.as_bytes().to_vec()), 10.into()],
        ));
        ops.push(Operation::new("Td", vec![x.into(), 700.into()]));
        ops.push(Operation::new("Tj", vec![Object::string_literal(text)]));
        ops.push(Operation::new("ET", vec![]));
    }
    let content = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        Content { operations: ops }.encode().expect("encode"),
    )));
    let pages_id = doc.new_object_id();
    let page = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "Contents" => content,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font, "F2" => bold } },
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

/// The drop touches nothing where nothing is hidden: a page of visible text
/// reads byte-identical with the default options and with the drop switched
/// off, and the two runs on its line read as one spaced line.
#[test]
fn a_page_with_no_hidden_text_reads_the_same_with_the_drop_on_or_off() {
    let bytes = page_with_two_runs_on_one_line();
    let defaulted = PdfDocument::open(bytes.clone())
        .expect("open")
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    let mut kept = ExtractOptions::default();
    kept.include_hidden = true;
    let unfiltered = PdfDocument::open(bytes)
        .expect("open")
        .extract_blocks(&kept)
        .expect("blocks");
    let default_texts: Vec<&str> = defaulted.iter().map(|b| b.text.as_str()).collect();
    let unfiltered_texts: Vec<&str> = unfiltered.iter().map(|b| b.text.as_str()).collect();
    assert_eq!(
        default_texts, unfiltered_texts,
        "the drop rewrote visible text: {default_texts:?} vs {unfiltered_texts:?}"
    );
    assert_eq!(default_texts, vec!["Hello world"]);
}

#[test]
fn a_hidden_line_reaches_neither_the_blocks_nor_the_markdown() {
    let bytes = page_with_a_hidden_line();
    let blocks = PdfDocument::open(bytes.clone())
        .expect("open")
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    assert!(
        blocks.iter().all(|b| !b.text.contains("hid")),
        "the hidden line is in the blocks: {:?}",
        blocks.iter().map(|b| &b.text).collect::<Vec<_>>()
    );
    let markdown = PdfDocument::open(bytes)
        .expect("open")
        .extract_markdown(&ExtractOptions::default())
        .expect("markdown");
    assert!(
        !markdown.contains("hid"),
        "the hidden line is in the markdown: {markdown:?}"
    );
}
