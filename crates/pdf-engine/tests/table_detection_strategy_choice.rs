//! The caller chooses how tables are found (lane L5).
//!
//! A ruled table and a borderless one, both found by the combined
//! detection: the lines strategy keeps the ruled one, the text strategy
//! reads every aligned grid, and both is the two together.
//!
//! The front door steers its own detection the same way: a lines-strict, a
//! lines and a text strategy, where the text strategy alone finds unruled
//! tables (G97, G98), while its default engine finds ruled, unruled and
//! headerless tables alike (G104).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::{BlockKind, ExtractOptions, PdfDocument, TableStrategy};

/// One line of text at a place.
fn show(ops: &mut Vec<Operation>, text: &str, x: f64, y: f64) {
    ops.push(Operation::new("BT", vec![]));
    ops.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F1".to_vec()), 10.into()],
    ));
    ops.push(Operation::new("Td", vec![x.into(), y.into()]));
    ops.push(Operation::new("Tj", vec![Object::string_literal(text)]));
    ops.push(Operation::new("ET", vec![]));
}

/// A hairline from corner to corner.
fn rule(ops: &mut Vec<Operation>, x: f64, y: f64, w: f64, h: f64) {
    ops.push(Operation::new(
        "re",
        vec![x.into(), y.into(), w.into(), h.into()],
    ));
    ops.push(Operation::new("f", vec![]));
}

/// A ruled two-by-two table over a borderless two-column grid.
fn page_with_two_tables() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let mut ops = Vec::new();
    for (i, (left, right)) in [("Item", "Price"), ("Widget", "12.00")].iter().enumerate() {
        let y = 740.0 - i as f64 * 18.0;
        show(&mut ops, left, 72.0, y);
        show(&mut ops, right, 300.0, y);
        rule(&mut ops, 70.0, y - 4.0, 300.0, 0.5);
    }
    rule(&mut ops, 70.0, 740.0 + 10.0, 300.0, 0.5);
    for (i, (left, right)) in [("North", "12"), ("South", "34")].iter().enumerate() {
        let y = 600.0 - i as f64 * 18.0;
        show(&mut ops, left, 72.0, y);
        show(&mut ops, right, 300.0, y);
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
fn each_strategy_finds_what_it_should_on_a_ruled_and_a_plain_table() {
    let blocks = PdfDocument::open(page_with_two_tables())
        .expect("open")
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    assert!(
        !blocks.is_empty(),
        "the tables page reads as nothing at all"
    );
    // G97: the ruled table is found under every strategy. G98: the
    // borderless grid is found by the text strategy, not by lines.
    let cell_words = |blocks: &[pdf_engine::Block]| {
        let mut words: Vec<String> = blocks
            .iter()
            .filter(|b| matches!(b.kind, BlockKind::TableCell { .. }))
            .flat_map(|b| b.text.split_whitespace().map(str::to_string))
            .collect();
        words.sort();
        words
    };
    let both = cell_words(&blocks);
    for word in [
        "Item", "Price", "Widget", "12.00", "North", "12", "South", "34",
    ] {
        assert!(
            both.contains(&word.to_string()),
            "the combined detection reads {word} as a cell: {both:?}"
        );
    }
    let words_with = |strategy: TableStrategy| {
        let mut options = ExtractOptions::default();
        options.table_strategy = strategy;
        let blocks = PdfDocument::open(page_with_two_tables())
            .expect("open")
            .extract_blocks(&options)
            .expect("blocks");
        cell_words(&blocks)
    };
    let lines = words_with(TableStrategy::Lines);
    for word in ["Item", "Price", "Widget", "12.00"] {
        assert!(
            lines.contains(&word.to_string()),
            "the lines strategy reads the ruled table: {lines:?}"
        );
    }
    for word in ["North", "South", "12", "34"] {
        assert!(
            !lines.contains(&word.to_string()),
            "the lines strategy leaves the borderless grid to prose: {lines:?}"
        );
    }
    let text = words_with(TableStrategy::Text);
    for word in [
        "Item", "Price", "Widget", "12.00", "North", "12", "South", "34",
    ] {
        assert!(
            text.contains(&word.to_string()),
            "the text strategy reads the aligned words as cells: {text:?}"
        );
    }
}
