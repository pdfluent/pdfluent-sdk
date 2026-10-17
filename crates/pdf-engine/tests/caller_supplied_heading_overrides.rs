//! The caller steers what counts as a heading (lane L1).
//!
//! PLACEHOLDER: the overrides have no spelling yet, so nothing here can name
//! them. What is pinned is the input they will govern: a large line over
//! body text, read as a heading today purely from the document's own
//! statistics. The lane restores the caller-supplied heading overrides and
//! replaces the panic below with the steered reading of this page -- for
//! example promoting the second line, or demoting the first to prose.
//!
//! The front door steers its own headings the same way: a detector object
//! the caller builds and hands in, defaulted when absent (G17), where a
//! stricter detector reads no headings at all on sizes the default promotes
//! (G24, G25).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::{ExtractOptions, PdfDocument};

/// A large line over two lines of body text.
fn page_with_a_heading() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let mut ops = Vec::new();
    for (text, y, size) in [
        ("The running of the plant", 740.0, 24),
        (
            "The plant treats the water of the whole valley before it is",
            700.0,
            10,
        ),
        (
            "returned to the river, and runs through the night.",
            688.0,
            10,
        ),
    ] {
        ops.push(Operation::new("BT", vec![]));
        ops.push(Operation::new(
            "Tf",
            vec![Object::Name(b"F1".to_vec()), size.into()],
        ));
        ops.push(Operation::new("Td", vec![72.into(), y.into()]));
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
fn a_heading_override_promotes_or_demotes_a_line() {
    let blocks = PdfDocument::open(page_with_a_heading())
        .expect("open")
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    assert!(
        !blocks.is_empty(),
        "the heading page reads as nothing at all"
    );
    // SPEC G24: a stricter detector reads no headings on sizes the default
    // promotes. The default reading marks the large line; the steered
    // reading demotes it to prose, so no `#` markers appear at all.
    assert!(
        matches!(
            blocks.first().map(|b| &b.kind),
            Some(pdf_engine::BlockKind::Heading { .. })
        ),
        "the default reading marks no heading: {:?}",
        blocks
            .iter()
            .map(|b| format!("{:?}: {}", b.kind, b.text))
            .collect::<Vec<_>>()
    );
    let mut steered_options = ExtractOptions::default();
    steered_options.heading_overrides = vec![pdf_engine::blocks::HeadingOverride {
        text: "The running of the plant".to_string(),
        level: None,
    }];
    let steered = PdfDocument::open(page_with_a_heading())
        .expect("open")
        .extract_blocks(&steered_options)
        .expect("blocks");
    assert!(
        steered
            .iter()
            .all(|b| !matches!(b.kind, pdf_engine::BlockKind::Heading { .. })),
        "the demoted line still reads as a heading: {:?}",
        steered
            .iter()
            .map(|b| format!("{:?}: {}", b.kind, b.text))
            .collect::<Vec<_>>()
    );
    let markdown = PdfDocument::open(page_with_a_heading())
        .expect("open")
        .extract_markdown(&steered_options)
        .expect("markdown");
    assert!(
        !markdown.contains('#'),
        "no `#` markers appear at all (SPEC G24): {markdown:?}"
    );
}
