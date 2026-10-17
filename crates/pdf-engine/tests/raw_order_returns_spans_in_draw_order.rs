// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Raw order returns spans in content-stream order, with no reading sort (E1).
//!
//! With `ExtractOptions::raw_order`, `extract_blocks` hands back the runs the
//! page draws in the order it draws them: blocks, lines and spans follow the
//! content stream, and the reading-order sort never runs. The landmarks below
//! are runs whose relative order and block membership the later E2 commits do
//! not move, so this suite stays green while the grouping under the option is
//! refined.

use pdf_engine::{Block, ExtractOptions, PdfDocument};
use std::path::PathBuf;

fn workspace_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

fn raw_blocks_on_first_page(relative: &str) -> Vec<Block> {
    let bytes = std::fs::read(workspace_path(relative)).expect("read the fixture");
    let document = PdfDocument::open(bytes).expect("open the fixture");
    let mut options = ExtractOptions::window(0, 1);
    options.raw_order = true;
    document
        .extract_blocks(&options)
        .expect("extract in raw order")
}

/// The index of the first block whose text holds the landmark, line breaks
/// ignored: the rotated caption reads across four lines until its runs join.
fn first_holding(blocks: &[Block], landmark: &str) -> usize {
    blocks
        .iter()
        .position(|b| b.text.contains(landmark) || b.text.replace('\n', "").contains(landmark))
        .unwrap_or_else(|| panic!("no raw block holds {landmark:?}"))
}

/// The rotated `Form` caption the stream draws first is the first block: the
/// reading order would sort it elsewhere, the raw order keeps it where the
/// page put it.
#[test]
fn the_first_draw_is_the_first_block() {
    let blocks = raw_blocks_on_first_page("corpus/f1040.pdf");
    assert!(
        !blocks.is_empty(),
        "the raw path returns no blocks on f1040 page 0"
    );
    let first: String = blocks[0]
        .text
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    assert!(
        first.starts_with("Form"),
        "the first raw block is not the Form caption: {:?}",
        blocks[0].text
    );
    assert_eq!(
        first_holding(&blocks, "Form"),
        0,
        "Form is drawn first and stays first"
    );
}

/// Successive landmarks of the form's head arrive in draw order.
#[test]
fn the_form_head_reads_in_draw_order() {
    let blocks = raw_blocks_on_first_page("corpus/f1040.pdf");
    let landmarks = [
        "Form",
        "1040",
        "U.S. Individual Income Tax Return",
        "Department of the Treasury",
        "OMB No.",
        "IRS Use Only",
        "For the year",
        "Filed",
        "Combat zone",
        "Deceased",
        "Spouse",
        "Other",
        "Your first name",
    ];
    let positions: Vec<usize> = landmarks
        .iter()
        .map(|m| first_holding(&blocks, m))
        .collect();
    for pair in positions.windows(2) {
        assert!(
            pair[0] <= pair[1],
            "landmarks out of draw order: {landmarks:?} at {positions:?}"
        );
    }
    // Splits the grouping never revisits: the title leaves the caption, the
    // caption leaves the header row, the head leaves the first field row.
    assert!(
        first_holding(&blocks, "U.S. Individual Income Tax Return")
            < first_holding(&blocks, "Department of the Treasury"),
        "the title and its caption share a raw block"
    );
    assert!(
        first_holding(&blocks, "Department of the Treasury") < first_holding(&blocks, "OMB No."),
        "the caption and the header row share a raw block"
    );
    assert!(
        first_holding(&blocks, "Spouse") < first_holding(&blocks, "Other"),
        "the social-security row and Other share a raw block"
    );
    // The two halves of the social-security row never part.
    assert_eq!(
        first_holding(&blocks, "Deceased"),
        first_holding(&blocks, "Spouse"),
        "the social-security row is split across raw blocks"
    );
}

/// Within the kept-together title block the runs stay in draw order.
#[test]
fn the_title_runs_keep_their_draw_order() {
    let blocks = raw_blocks_on_first_page("corpus/f1040.pdf");
    let title = &blocks[first_holding(&blocks, "1040")];
    let texts: Vec<&str> = title.spans.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(
        &texts[..4],
        &["1040 ", "20", "25", "U.S. Individual Income Tax Return "],
        "title runs out of draw order: {texts:?}"
    );
}

/// The structured page reads top to bottom: the title, then the bullets.
#[test]
fn the_structured_page_reads_in_draw_order() {
    let blocks = raw_blocks_on_first_page("fixtures/structured.pdf");
    assert!(
        blocks[0].text.contains("Safety rules"),
        "the title is not the first raw block: {:?}",
        blocks[0].text
    );
    let landmarks = [
        "Safety rules",
        "Wear the issued boots",
        "Keep the gangways",
        "Report a spill",
    ];
    let positions: Vec<usize> = landmarks
        .iter()
        .map(|m| first_holding(&blocks, m))
        .collect();
    for pair in positions.windows(2) {
        assert!(
            pair[0] <= pair[1],
            "landmarks out of draw order: {landmarks:?} at {positions:?}"
        );
    }
    assert_eq!(
        first_holding(&blocks, "Wear the issued boots"),
        first_holding(&blocks, "Keep the gangways"),
        "the bullet rows parted across raw blocks"
    );
}

/// One run on the page is one block.
#[test]
fn a_single_run_is_a_single_block() {
    let blocks = raw_blocks_on_first_page("tests/corpus-mini/simple.pdf");
    assert_eq!(blocks.len(), 1, "one run should be one block");
    assert_eq!(blocks[0].text, "Test page");
}
