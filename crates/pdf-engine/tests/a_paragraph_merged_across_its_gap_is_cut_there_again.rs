//! A paragraph the builder merged across its own gap is cut there again (#574).
//!
//! Fourteen-point type on a sixteen-point pitch, a one-line paragraph, a
//! twenty-four-point gap, and a four-line paragraph: the builder joins the
//! first line across the gap because, while the paragraph still has one line,
//! the leading in force is the RUN's -- and the run is set wide, so 24 clears
//! 1.35 times its leading and the 1.8 em rail. The step-split pass cuts the
//! merged block where its own white jumps (24 is 1.5 times the block's median
//! 16) and `extract_blocks` returns the two paragraphs. Take the pass out of
//! the document path and this goes red with one six-line block.
//!
//! The three evenly spaced lines under the pair are scaffolding, not prose:
//! without wide steps further down the run its leading would be the 16-point
//! pitch and the builder would break at the gap like it should. A literal
//! blank line -- twice the pitch -- can never exercise this: it always clears
//! the 1.8 em rail, whatever the leading says.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::{BlockKind, ExtractOptions, PdfDocument};

fn text(operations: &mut Vec<Operation>, body: &str, x: f64, y: f64, size: f64) {
    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F1".to_vec()), size.into()],
    ));
    operations.push(Operation::new("Td", vec![x.into(), y.into()]));
    operations.push(Operation::new(
        "Tj",
        vec![Object::String(
            body.as_bytes().to_vec(),
            StringFormat::Literal,
        )],
    ));
    operations.push(Operation::new("ET", vec![]));
}

fn pdf_of(lines: &[(&str, f64, f64)], size: f64) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    // Courier: every glyph advances the same, so equal-length lines share one
    // right edge and no short-line rail fires anywhere on the page.
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Courier",
    });
    let mut operations = Vec::new();
    for (body, x, y) in lines {
        text(&mut operations, body, *x, *y, size);
    }
    let content = Content { operations }.encode().expect("encode content");
    let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page", "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
    });
    let pages_id = doc.add_object(dictionary! {
        "Type" => "Pages", "Kids" => vec![Object::Reference(page_id)], "Count" => 1,
    });
    if let Object::Dictionary(d) = doc.objects.get_mut(&page_id).expect("page") {
        d.set("Parent", pages_id);
    }
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save pdf");
    out
}

/// Every line the same width, so each runs the full measure.
fn pad(line: &str) -> String {
    format!("{line:60.60}")
}

#[test]
fn a_paragraph_merged_across_its_gap_is_cut_there_again() {
    let first = pad("The opening line states the finding plainly and runs on");
    let second = [
        pad("The second paragraph opens here after the wider gap and"),
        pad("it continues across its own second line of fourteen point"),
        pad("type, and a third line holds the point it opened with"),
        pad("while a fourth line closes the second paragraph at last"),
    ];
    let tail = [
        pad("Tail line one keeps the run leading high above the pitch"),
        pad("Tail line two keeps the run leading high above the pitch"),
        pad("Tail line three keeps the run leading high above the pitch"),
    ];
    // Steps below each line: the 24-point paragraph gap, the 16-point pitch,
    // the 24-point break onto the tail, the tail's own 24-point pitch.
    let steps = [24.0, 16.0, 16.0, 16.0, 24.0, 24.0, 24.0];
    let mut lines: Vec<(&str, f64, f64)> = Vec::new();
    let mut y = 700.0;
    for (i, line) in [&first]
        .into_iter()
        .chain(second.iter())
        .chain(tail.iter())
        .enumerate()
    {
        lines.push((line, 72.0, y));
        if let Some(step) = steps.get(i) {
            y -= step;
        }
    }
    let pdf = pdf_of(&lines, 14.0);
    let doc = PdfDocument::open(pdf).expect("open");
    let blocks = doc
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    assert!(
        blocks
            .iter()
            .all(|block| matches!(block.kind, BlockKind::Paragraph)),
        "three paragraphs, no headings or items: {:?}",
        blocks
            .iter()
            .map(|block| (block.kind, &block.text))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        blocks.len(),
        3,
        "the pair cut, the tail untouched: {:?}",
        blocks.iter().map(|block| &block.text).collect::<Vec<_>>()
    );
    assert_eq!(blocks[0].text, first.trim_end());
    assert_eq!(
        blocks[1].text,
        second
            .iter()
            .map(|line| line.trim_end())
            .collect::<Vec<_>>()
            .join(" ")
    );
    assert_eq!(blocks[0].spans.len(), 1);
    assert_eq!(blocks[1].spans.len(), 4);
}
