//! A paragraph set double-spaced comes out as one paragraph (#574).
//!
//! Each of its lines clears the paragraph-break threshold, so each arrives as
//! a block of its own; the joining pass puts the run back together. Take the
//! join out of the extract path and this goes red with six blocks.
//!
//! Page one holds single-spaced body, page two the double-spaced paragraph.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::{BlockKind, ExtractOptions, PdfDocument};

fn text(operations: &mut Vec<Operation>, body: &str, x: f64, y: f64) {
    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F1".to_vec()), 10.into()],
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

fn pdf_of(pages: Vec<Vec<Operation>>) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    // Courier: every glyph advances the same, so lines of the same length end
    // at the same right edge and the join reads them as filling the measure.
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Courier",
    });
    let pages_id = doc.new_object_id();
    let mut kids: Vec<Object> = Vec::new();
    for operations in pages {
        let content = Content { operations }.encode().expect("encode content");
        let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "Contents" => content_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        });
        kids.push(page_id.into());
    }
    let count = kids.len() as i64;
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => count }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save pdf");
    out
}

const RUN: [&str; 6] = [
    "The double-spaced abstract is six full lines of ten point type on a wide pitch",
    "and each line comes to the paragraph stage as a block of its own, on the pitch",
    "as the break is judged upon the single-spaced body set elsewhere upon the page",
    "so every line clears the threshold alone, and reads as six paragraphs, not one",
    "until the joining pass puts the single lines back as one paragraph now at last",
    "with the guards keeping lists apart.",
];

fn two_pages() -> Vec<u8> {
    let mut first = Vec::new();
    let mut y = 700.0;
    for body in [
        "The committee met in ordinary session on Tuesday morning and heard",
        "the report of the working group on procedure before adjourning for",
        "the midday recess without further business.",
    ] {
        text(&mut first, body, 72.0, y);
        y -= 12.0;
    }
    let mut second = Vec::new();
    let mut y = 700.0;
    for body in RUN {
        text(&mut second, body, 72.0, y);
        y -= 20.0;
    }
    pdf_of(vec![first, second])
}

#[test]
fn a_double_spaced_paragraph_is_one_paragraph() {
    let doc = PdfDocument::open(two_pages()).expect("open");
    // A test of the heuristic join, not of the learned pass after it (#599).
    let options = ExtractOptions::default().without_the_learned_pass();
    let blocks = doc.extract_blocks(&options).expect("blocks");
    let page_two: Vec<_> = blocks
        .iter()
        .filter(|block| block.page == 1 && matches!(block.kind, BlockKind::Paragraph))
        .collect();
    assert_eq!(
        page_two.len(),
        1,
        "the run joined: {:?}",
        page_two.iter().map(|block| &block.text).collect::<Vec<_>>()
    );
    assert_eq!(page_two[0].text, RUN.join(" "));
}
