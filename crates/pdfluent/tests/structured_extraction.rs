//! The structured-extraction API on the facade (#573).
//!
//! `pdf-engine` is where the block model is built and pinned; this is the
//! surface a customer of the `pdfluent` crate actually calls, and it is a
//! separate assertion because a method can be right in the engine and missing,
//! misnamed or unreachable one crate up -- which is the whole failure this
//! ticket's five bindings are also about.
//!
//! The document is `fixtures/structured.pdf`, built and pinned by
//! `crates/pdf-engine/tests/the_api_over_the_block_model.rs`: a heading and a
//! bulleted list on page 0, a ruled table on page 1.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent::{BlockKind, ExtractOptions, PdfDocument};
use std::path::PathBuf;

fn document() -> PdfDocument {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("fixtures/structured.pdf");
    PdfDocument::open(&path).expect("open the structured fixture")
}

#[test]
fn the_markdown_carries_a_heading_a_list_item_and_a_table_cell() {
    let markdown = document()
        .extract_markdown(&ExtractOptions::default())
        .expect("extract markdown");

    assert!(markdown.contains("# Safety rules"), "{markdown}");
    assert!(
        markdown.contains("- Wear the issued boots on the floor."),
        "{markdown}"
    );
    assert!(markdown.contains("| Band | Units | Rate |"), "{markdown}");
}

#[test]
fn the_blocks_carry_a_heading_a_list_item_and_a_table_cell() {
    let blocks = document()
        .extract_blocks(&ExtractOptions::default())
        .expect("extract blocks");

    assert!(blocks
        .iter()
        .any(|b| matches!(b.kind, BlockKind::Heading { level: 1 }) && b.text == "Safety rules"));
    assert!(blocks
        .iter()
        .any(|b| matches!(b.kind, BlockKind::ListItem { .. })
            && b.text == "Wear the issued boots on the floor."));
    assert!(blocks
        .iter()
        .any(|b| matches!(b.kind, BlockKind::TableCell { header: true, .. }) && b.text == "Band"));
}

/// And both serialisations are reachable from the facade too, because a caller
/// that has the blocks and no way to write them out has half an API.
#[test]
fn both_serialisations_are_on_the_facade() {
    let blocks = document()
        .extract_blocks(&ExtractOptions::default())
        .expect("extract blocks");

    assert!(pdfluent::to_markdown(&blocks).contains("# Safety rules"));
    assert!(pdfluent::to_json(&blocks).contains("\"kind\": \"heading\""));
}

/// And the Docling form (#629), from the document and from the blocks.
#[test]
fn the_docling_document_is_on_the_facade() {
    let document = document();
    let json = document
        .extract_docling("structured", &ExtractOptions::default())
        .expect("extract docling");

    assert!(
        json.contains("\"schema_name\": \"DoclingDocument\""),
        "{json}"
    );
    assert!(json.contains("\"label\": \"section_header\""), "{json}");
    assert!(json.contains("\"label\": \"table\""), "{json}");

    let blocks = document
        .extract_blocks(&ExtractOptions::default())
        .expect("extract blocks");
    let page = pdfluent::DoclingPage {
        page: 0,
        width: 612.0,
        height: 792.0,
    };
    assert!(pdfluent::to_docling_json("structured", &[page], &blocks).contains("\"list_item\""));
}

#[test]
fn a_window_holds_the_pages_it_asks_for() {
    let document = document();
    let second = document
        .extract_blocks(&ExtractOptions::window(1, 1))
        .expect("the second page");

    assert!(!second.is_empty());
    assert!(second.iter().all(|b| b.page == 1));
    assert!(document
        .extract_markdown(&ExtractOptions::window(1, 1))
        .expect("markdown over a window")
        .contains("| Band | Units | Rate |"));
}
