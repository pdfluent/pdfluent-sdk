//! Vector paths are read as figures (lane L4).
//!
//! A drawing made of paths -- a chart, a diagram, a logo -- is invisible to
//! extraction today: the paths are recorded, clustered, and then read by
//! nothing on the extraction path, so a page of a paragraph and a drawing
//! comes back as the paragraph alone. The rebuilt engine lists the drawing
//! as a figure beside the text, the way it lists a placed image.
//!
//! Only the count is pinned: one text block and one figure block.
//! Unspecified; our behaviour: the front door never surfaces vector
//! graphics as Markdown either (G85, G86), but our block model lists
//! figures, and a drawing is one.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::{ExtractOptions, PdfDocument};

/// Three lines of body text over five filled squares in a row.
fn page_with_a_drawing() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let mut ops = Vec::new();
    let mut y = 740.0;
    for line in [
        "The plant treats the water of the whole valley before it is returned",
        "to the river, and runs through the night when the demand is lowest.",
        "Its capacity was doubled in the last year of the programme.",
    ] {
        ops.push(Operation::new("BT", vec![]));
        ops.push(Operation::new(
            "Tf",
            vec![Object::Name(b"F1".to_vec()), 10.into()],
        ));
        ops.push(Operation::new("Td", vec![72.into(), y.into()]));
        ops.push(Operation::new("Tj", vec![Object::string_literal(line)]));
        ops.push(Operation::new("ET", vec![]));
        y -= 12.0;
    }
    for i in 0..5 {
        let x = 72.0 + i as f64 * 32.0;
        ops.push(Operation::new(
            "re",
            vec![x.into(), 460.into(), 30.into(), 30.into()],
        ));
        ops.push(Operation::new("f", vec![]));
    }
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

#[test]
fn a_drawing_beside_a_paragraph_is_listed_as_a_block() {
    let blocks = PdfDocument::open(page_with_a_drawing())
        .expect("open")
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    assert_eq!(
        blocks.len(),
        2,
        "a paragraph and a drawing came back as {} blocks: {:?}",
        blocks.len(),
        blocks.iter().map(|b| &b.text).collect::<Vec<_>>()
    );
}
