// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The rules a page drew, beside the blocks it holds (#600).
//!
//! A cell boundary in a ruled table is a drawn line, and the learned table
//! structure of #600 reads those lines. They were `pub(crate)` and so could
//! not leave the engine; `extract_blocks_and_rules` hands them out beside the
//! blocks, in ONE interpretation of the page, because two passes over one page
//! are two chances to disagree.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::{ExtractOptions, PdfDocument};

/// A hairline from `left` to `right` at `y`, as a table rules a row off.
fn rule(operations: &mut Vec<Operation>, left: f64, right: f64, y: f64) {
    operations.push(Operation::new(
        "re",
        vec![left.into(), y.into(), (right - left).into(), 0.18.into()],
    ));
    operations.push(Operation::new("f", vec![]));
}

fn line(operations: &mut Vec<Operation>, text: &str, x: f64, y: f64) {
    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F1".to_vec()), 10.into()],
    ));
    operations.push(Operation::new("Td", vec![x.into(), y.into()]));
    operations.push(Operation::new("Tj", vec![Object::string_literal(text)]));
    operations.push(Operation::new("ET", vec![]));
}

fn page_of(operations: Vec<Operation>) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let content = Content { operations }.encode().expect("encode content");
    let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
    let pages_id = doc.new_object_id();
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "Contents" => content_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save pdf");
    out
}

#[test]
fn a_ruled_table_hands_out_the_lines_it_drew() {
    let mut operations = Vec::new();
    for (i, row) in ["Region Revenue", "North 12,345", "South 23,456"]
        .iter()
        .enumerate()
    {
        let y = 700.0 - i as f64 * 20.0;
        line(&mut operations, row, 72.0, y);
        rule(&mut operations, 70.0, 300.0, y + 13.0);
    }
    let pdf = page_of(operations);

    let doc = PdfDocument::open(pdf.clone()).expect("open");
    let (blocks, ruled) = doc
        .extract_blocks_and_rules(&ExtractOptions::default())
        .expect("blocks and rules");

    assert_eq!(ruled.len(), 1, "one page, one list of rules");
    assert_eq!(
        ruled[0].len(),
        3,
        "the page drew three rules and the engine kept them: {:?}",
        ruled[0].iter().map(|r| r.y).collect::<Vec<_>>()
    );
    for drawn in &ruled[0] {
        assert!(
            drawn.left <= 70.5 && drawn.right >= 299.5,
            "a rule came back the wrong width: {:?}",
            (drawn.left, drawn.right, drawn.y)
        );
    }

    // THE BLOCKS ARE THE SAME BLOCKS. `extract_blocks` is this call with the
    // rules dropped, so a reader and a model see one reading of the page --
    // the thing two passes could not promise.
    let plain = PdfDocument::open(pdf)
        .expect("open")
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    assert_eq!(
        plain.iter().map(|b| b.text.as_str()).collect::<Vec<_>>(),
        blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>(),
        "the two entry points disagreed about the page"
    );
}

#[test]
fn a_page_that_drew_nothing_reports_no_rules() {
    let mut operations = Vec::new();
    line(&mut operations, "The committee met on Tuesday", 72.0, 700.0);
    let (_blocks, ruled) = PdfDocument::open(page_of(operations))
        .expect("open")
        .extract_blocks_and_rules(&ExtractOptions::default())
        .expect("blocks and rules");
    assert_eq!(ruled.len(), 1, "the page is still a page");
    assert!(
        ruled[0].is_empty(),
        "prose drew rules it never drew: {:?}",
        ruled[0].iter().map(|r| r.y).collect::<Vec<_>>()
    );
}
