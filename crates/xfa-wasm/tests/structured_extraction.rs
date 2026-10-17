//! The structured-extraction API through the WASM binding (#573).
//!
//! Two methods, one document. `fixtures/structured.pdf` is built and pinned by
//! `crates/pdf-engine/tests/the_api_over_the_block_model.rs`: a heading and a
//! bulleted list on page 0, a ruled table on page 1. Every binding suite reads
//! that same file and asserts the same three things, so a fault in one
//! crossing stands next to five that still work.
//!
//! Run: `wasm-pack test --node crates/xfa-wasm --no-default-features --features wasm`

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use wasm_bindgen_test::*;
use xfa_wasm::PdfDoc;

static STRUCTURED_PDF: &[u8] = include_bytes!("../../../fixtures/structured.pdf");

fn document() -> PdfDoc {
    PdfDoc::open(STRUCTURED_PDF).expect("open the structured fixture")
}

#[wasm_bindgen_test]
fn the_markdown_carries_a_heading_a_list_item_and_a_table_cell() {
    let markdown = document()
        .extract_markdown(None, None)
        .expect("extractMarkdown");

    assert!(markdown.contains("# Safety rules"), "{markdown}");
    assert!(
        markdown.contains("- Wear the issued boots on the floor."),
        "{markdown}"
    );
    assert!(markdown.contains("| Band | Units | Rate |"), "{markdown}");
}

#[wasm_bindgen_test]
fn the_blocks_carry_a_heading_a_list_item_and_a_table_cell() {
    let json = document()
        .extract_blocks(None, None)
        .expect("extractBlocks");

    // The JSON is the contract `docs/STRUCTURED_EXTRACTION.md` writes down, so
    // the assertions are on the strings a caller in JavaScript reads off it.
    assert!(
        json.contains("\"kind\": \"heading\"") && json.contains("\"text\": \"Safety rules\""),
        "{json}"
    );
    assert!(
        json.contains("\"kind\": \"listItem\"")
            && json.contains("\"text\": \"Wear the issued boots on the floor.\""),
        "{json}"
    );
    assert!(
        json.contains("\"kind\": \"tableCell\"") && json.contains("\"text\": \"Band\""),
        "{json}"
    );
}

#[wasm_bindgen_test]
fn the_docling_document_carries_a_heading_a_list_and_a_table() {
    let document = document();
    let json = document
        .extract_docling(None, None, Some("structured".to_string()))
        .expect("extractDocling");

    assert!(
        json.contains("\"schema_name\": \"DoclingDocument\""),
        "{json}"
    );
    assert!(json.contains("\"name\": \"structured\""), "{json}");
    assert!(json.contains("\"label\": \"section_header\""), "{json}");
    assert!(json.contains("\"label\": \"list_item\""), "{json}");
    assert!(json.contains("\"label\": \"table\""), "{json}");
    assert!(document
        .extract_docling(None, None, None)
        .expect("extractDocling without a name")
        .contains("\"name\": \"document\""));
}

#[wasm_bindgen_test]
fn a_window_holds_the_pages_it_asks_for() {
    let document = document();
    let second = document
        .extract_blocks(Some(1), Some(1))
        .expect("extractBlocks over a window");

    assert!(second.contains("\"text\": \"Band\""), "{second}");
    assert!(!second.contains("Safety rules"), "{second}");
    assert!(document
        .extract_markdown(Some(1), Some(1))
        .expect("extractMarkdown over a window")
        .contains("| Band | Units | Rate |"));
}

#[wasm_bindgen_test]
fn a_window_past_the_end_is_empty_rather_than_an_error() {
    let document = document();

    assert_eq!(
        document
            .extract_markdown(Some(9), Some(4))
            .expect("markdown past the end"),
        ""
    );
    assert!(document
        .extract_blocks(Some(9), Some(4))
        .expect("blocks past the end")
        .contains("\"blocks\": []"));
}
