//! A block of figures is not split where its own white jumps (#574).
//!
//! Eight fourteen-point figure lines on a sixteen-point pitch, the first
//! joined across a twenty-four-point gap the way the m228 page joins its
//! pair -- while the paragraph still has one line the leading in force is
//! the run's, and the twelve wide tail lines below keep that leading high.
//! The step-split pass would cut the merged block at the gap (24 is 1.5
//! times the block's median 16), but the number guard refuses: the block
//! reads as figures, so splitting it is the table pass's business.
//! `extract_blocks` returns the eight lines as one paragraph. Take the
//! number guard out of the pass and this goes red with the figures cut
//! apart.

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
fn a_number_block_survives_the_step_split() {
    let rows = [
        "1,234.5 678 12.3% 4,567.8 901 23.4% $5,432.10 12 98.7%",
        "2,345.6 789 23.4% 5,678.9 012 34.5% $6,543.21 23 87.6%",
        "3,456.7 890 34.5% 6,789.0 123 45.6% $7,654.32 34 76.5%",
    ];
    let figures: Vec<String> = (0..8).map(|row| pad(rows[row % 3])).collect();
    let tails = [
        "Tail line one keeps the run leading high above the pitch",
        "Tail line two keeps the run leading high above the pitch",
        "Tail line three keeps the run leading high above the pitch",
    ];
    let tail: Vec<String> = (0..12).map(|row| pad(tails[row % 3])).collect();
    // Steps below each line: the 24-point gap onto the figures' second row,
    // the 16-point pitch, the 24-point break onto the tail, the tail's own
    // 24-point pitch.
    let mut lines: Vec<(&str, f64, f64)> = Vec::new();
    let mut y = 700.0;
    for (i, line) in figures.iter().chain(tail.iter()).enumerate() {
        lines.push((line, 72.0, y));
        y -= match i {
            0 | 7 => 24.0,
            _ if i >= 8 => 24.0,
            _ => 16.0,
        };
    }
    let pdf = pdf_of(&lines, 14.0);
    let doc = PdfDocument::open(pdf).expect("open");
    let blocks = doc
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    let held: Vec<_> = blocks
        .iter()
        .filter(|b| b.text.contains("1,234.5"))
        .collect();
    assert_eq!(
        held.len(),
        1,
        "the figures stand as one block: {:?}",
        blocks.iter().map(|b| &b.text).collect::<Vec<_>>()
    );
    assert!(
        matches!(held[0].kind, BlockKind::Paragraph),
        "a paragraph the rule would cut, not a table or list it skips: {:?}",
        held[0].kind
    );
    assert_eq!(held[0].spans.len(), 8);
}
