//! The caller chooses how tables render (lane L5).
//!
//! A headed rectangular table beside a headerless grid: the automatic
//! choice renders the headed one as pipes and the headerless one as HTML,
//! and the caller can ask for either form for both.
//!
//! The front door's own table switch accepts only Markdown and refuses the
//! rest (G103); its headerless grids render as pipe tables with the first
//! row above the divider (G99), and its ruled tables as pipe tables with a
//! bolded header row (G104).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::{ExtractOptions, PdfDocument, TableRenderMode};

/// A two-column grid at `top`, one row per element, set solid.
fn grid(ops: &mut Vec<Operation>, rows: &[(&str, &str)], top: f64) {
    for (i, (left, right)) in rows.iter().enumerate() {
        let y = top - i as f64 * 18.0;
        for (text, x) in [(*left, 72.0), (*right, 300.0)] {
            ops.push(Operation::new("BT", vec![]));
            ops.push(Operation::new(
                "Tf",
                vec![Object::Name(b"F1".to_vec()), 10.into()],
            ));
            ops.push(Operation::new("Td", vec![x.into(), y.into()]));
            ops.push(Operation::new("Tj", vec![Object::string_literal(text)]));
            ops.push(Operation::new("ET", vec![]));
        }
    }
}

/// One headed table over one headerless grid.
fn page_with_two_tables() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let mut ops = Vec::new();
    grid(
        &mut ops,
        &[("Item", "Price"), ("Widget", "12.00"), ("Gadget", "8.50")],
        740.0,
    );
    grid(
        &mut ops,
        &[("North", "12"), ("South", "34"), ("East", "56")],
        600.0,
    );
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
fn a_table_renders_in_the_mode_the_caller_asks_for() {
    let markdown = PdfDocument::open(page_with_two_tables())
        .expect("open")
        .extract_markdown(&ExtractOptions::default())
        .expect("markdown");
    assert!(!markdown.is_empty(), "the tables page reads as nothing");
    // G104: a table renders as pipes with its header row above the
    // divider. G99: a headerless grid renders the same way, first row up --
    // where the automatic choice writes HTML for it instead.
    assert!(
        markdown.contains("| Item | Price |"),
        "the headed table renders as pipes: {markdown:?}"
    );
    assert_eq!(
        markdown.matches("<table>").count(),
        1,
        "the headerless grid renders as HTML today: {markdown:?}"
    );
    let rendered = |mode: TableRenderMode| {
        let mut options = ExtractOptions::default();
        options.table_render_mode = mode;
        PdfDocument::open(page_with_two_tables())
            .expect("open")
            .extract_markdown(&options)
            .expect("markdown")
    };
    let pipes = rendered(TableRenderMode::Markdown);
    assert!(
        !pipes.contains("<table>"),
        "no table renders as HTML on request: {pipes:?}"
    );
    assert!(
        pipes.contains("| North | 12 |"),
        "the headerless grid renders as pipes on request: {pipes:?}"
    );
    assert!(
        pipes.contains("| --- | --- |"),
        "the divider stands after the first row: {pipes:?}"
    );
    let html = rendered(TableRenderMode::Html);
    assert_eq!(
        html.matches("<table>").count(),
        2,
        "both tables render as HTML on request: {html:?}"
    );
    assert!(
        !html.contains("| --- |"),
        "no pipe divider survives the HTML request: {html:?}"
    );
}
