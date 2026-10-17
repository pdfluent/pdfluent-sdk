// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A `/ActualText` sequence that closes without a glyph to carry it.
//!
//! The replacement is emitted once at the sequence's position with
//! [`ExtractOptions::actual_text`]: the box the sequence drew, or nothing
//! when it drew nothing at all. Without the option the page reads as drawn,
//! which here is nothing. A nested replacement inside a replacement is one
//! reading of one set of marks: the innermost answers, the outer stays quiet.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::{ExtractOptions, PdfDocument};

/// A page drawing a filled rectangle inside `/ActualText` sequences and no
/// glyph at all: `texts` around the box, each one a replacement.
fn page_with_glyphless_actual_text(texts: &[&str]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let mut ops = Vec::new();
    for text in texts {
        ops.push(Operation::new(
            "BDC",
            vec![
                Object::Name(b"Span".to_vec()),
                Object::Dictionary(dictionary! {
                    "ActualText" => Object::string_literal(*text),
                }),
            ],
        ));
    }
    ops.push(Operation::new(
        "re",
        vec![72.into(), 700.into(), 100.into(), 12.into()],
    ));
    ops.push(Operation::new("f", vec![]));
    for _ in texts {
        ops.push(Operation::new("EMC", vec![]));
    }
    let content = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        Content { operations: ops }.encode().expect("encode"),
    )));
    let pages_id = doc.new_object_id();
    let page = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "Contents" => content,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => dictionary! {},
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
    doc.save_to(&mut out).expect("save");
    out
}

fn options_actual_text() -> ExtractOptions {
    let mut options = ExtractOptions::default();
    options.actual_text = true;
    options
}

fn text_of(pdf: &[u8], options: &ExtractOptions) -> String {
    PdfDocument::open(pdf.to_vec())
        .expect("open")
        .extract_blocks(options)
        .expect("extract")
        .iter()
        .map(|block| block.text.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Without the option the glyphless sequence reads as what was drawn, which
/// is no text; with it the replacement reads once, in the box it drew.
#[test]
fn a_glyphless_replacement_reads_once_with_the_option() {
    let pdf = page_with_glyphless_actual_text(&["REPLACEMENT"]);
    assert!(
        !text_of(&pdf, &ExtractOptions::default()).contains("REPLACEMENT"),
        "a glyphless replacement read without the option"
    );
    let on = text_of(&pdf, &options_actual_text());
    assert_eq!(
        on.matches("REPLACEMENT").count(),
        1,
        "a glyphless replacement did not read exactly once: {on:?}"
    );

    let document = PdfDocument::open(pdf).expect("open");
    let words = document
        .extract_words(&options_actual_text())
        .expect("words");
    assert_eq!(
        words
            .iter()
            .map(|word| word.text.as_str())
            .collect::<Vec<_>>(),
        vec!["REPLACEMENT"],
        "the replacement did not come back as one word"
    );
    let [x0, y0, x1, y1] = words[0].bbox;
    assert!(
        (x0 - 72.0).abs() < 1e-4
            && (y0 - 700.0).abs() < 1e-4
            && (x1 - 172.0).abs() < 1e-4
            && (y1 - 712.0).abs() < 1e-4,
        "the replacement is not in the box its sequence drew: {:?}",
        words[0].bbox
    );
}

/// A glyphless replacement inside a glyphless replacement is one reading of
/// one set of marks: the innermost answers, and the outer stays quiet.
#[test]
fn a_nested_glyphless_replacement_reads_once() {
    let pdf = page_with_glyphless_actual_text(&["outer_value", "inner_value"]);
    let on = text_of(&pdf, &options_actual_text());
    assert!(
        on.contains("inner_value"),
        "the innermost replacement never reached the text: {on:?}"
    );
    assert!(
        !on.contains("outer_value"),
        "an enclosing replacement was read as well as the one inside it: {on:?}"
    );
    assert!(
        !text_of(&pdf, &ExtractOptions::default()).contains("inner_value"),
        "a nested glyphless replacement read without the option"
    );
}

/// `/ActualText ()` stands for no characters: a glyphless empty replacement
/// adds no text with the option on either.
#[test]
fn a_glyphless_empty_replacement_stays_empty() {
    let pdf = page_with_glyphless_actual_text(&[""]);
    assert!(
        text_of(&pdf, &options_actual_text()).trim().is_empty(),
        "an empty glyphless replacement produced text"
    );
}
