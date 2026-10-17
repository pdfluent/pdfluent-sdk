//! A table set over a page break is one table (#574).
//!
//! The halves keep their own pages -- the relation is `continues_previous` on
//! the first cell of the second half, as for a paragraph the page broke -- and
//! the markdown writes them as one table, the head the second half repeats
//! written once. Take `blocks::tables_across_pages` out of `extract_blocks`
//! and the first test goes red.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::{to_markdown, BlockKind, ExtractOptions, PdfDocument};

const COLUMNS: [f64; 3] = [72.0, 300.0, 420.0];

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

fn row(operations: &mut Vec<Operation>, cells: [&str; 3], y: f64) {
    for (x, body) in COLUMNS.iter().zip(cells) {
        text(operations, body, *x, y);
    }
}

fn pdf_of(pages: Vec<Vec<Operation>>) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
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

/// A statement that fills the foot of page one and runs on at the head of page
/// two, the head repeated there; `second_columns` moves page two's columns.
fn two_pages(second_columns: [f64; 3]) -> Vec<u8> {
    let mut first = Vec::new();
    text(
        &mut first,
        "Notes to the financial statements, continued.",
        72.0,
        700.0,
    );
    row(&mut first, ["Item", "2014", "2013"], 200.0);
    let mut y = 184.0;
    for (item, a, b) in [
        ("Revenue", "13,110", "12,166"),
        ("Cost of sales", "(8,406)", "(7,737)"),
        ("Gross profit", "4,704", "4,429"),
        ("Selling expenses", "(3,145)", "(2,963)"),
    ] {
        row(&mut first, [item, a, b], y);
        y -= 16.0;
    }
    let mut second = Vec::new();
    let shifted = |cells: [&str; 3], y: f64, ops: &mut Vec<Operation>| {
        for (x, body) in second_columns.iter().zip(cells) {
            text(ops, body, *x, y);
        }
    };
    shifted(["Item", "2014", "2013"], 740.0, &mut second);
    let mut y = 724.0;
    for (item, a, b) in [
        ("Operating profit", "1,559", "1,466"),
        ("Interest", "(161)", "(162)"),
        ("Earnings before tax", "1,398", "1,304"),
    ] {
        shifted([item, a, b], y, &mut second);
        y -= 16.0;
    }
    text(
        &mut second,
        "The notes are an integral part of these statements.",
        72.0,
        600.0,
    );
    pdf_of(vec![first, second])
}

fn markdown_of(bytes: Vec<u8>) -> (String, Vec<pdf_engine::Block>) {
    let doc = PdfDocument::open(bytes).expect("open");
    let blocks = doc
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    (to_markdown(&blocks), blocks)
}

#[test]
fn a_table_set_over_a_page_break_is_one_table() {
    let (markdown, blocks) = markdown_of(two_pages(COLUMNS));
    let opened = blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::TableCell { .. }) && b.continues_previous)
        .count();
    assert_eq!(opened, 1, "the second half is marked once:\n{markdown}");
    assert_eq!(
        markdown.lines().filter(|l| l.starts_with("| ---")).count()
            + markdown.matches("<table>").count(),
        1,
        "one table:\n{markdown}"
    );
    assert_eq!(
        markdown.matches("2014").count(),
        1,
        "the head once:\n{markdown}"
    );
    assert!(
        markdown.contains("Selling expenses") && markdown.contains("Operating profit"),
        "{markdown}"
    );
    let selling = markdown.find("Selling expenses").expect("row");
    let operating = markdown.find("Operating profit").expect("row");
    assert!(
        selling < operating,
        "page one's rows come first:\n{markdown}"
    );
}

#[test]
fn a_different_table_on_the_next_page_is_another_table() {
    let (markdown, blocks) = markdown_of(two_pages([72.0, 240.0, 500.0]));
    assert!(
        !blocks
            .iter()
            .any(|b| b.continues_previous && matches!(b.kind, BlockKind::TableCell { .. })),
        "{markdown}"
    );
}
