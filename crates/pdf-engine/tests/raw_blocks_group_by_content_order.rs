// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Blocks group consecutive draws by content order (E2b).
//!
//! Under raw order a block is a run of consecutively drawn lines: one
//! marked-content segment never parts, a turn of the reading direction
//! always does, and between them an overlapping row joins unless it reads
//! forward into a partial overlap, while a gapped row joins when it reads
//! back on (same type or larger, a short line, a small gap) or opens larger
//! type. The form reads as 55 blocks against 64 recorded: 9 boundaries the
//! recordings split carry no local signal here -- seven every-third-row
//! splits in the numbered rows plus two overlapping rows the adjacent joins
//! match feature for feature -- and are pinned as such below.

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

fn first_holding(blocks: &[Block], landmark: &str) -> usize {
    blocks
        .iter()
        .position(|b| b.text.contains(landmark) || b.text.replace('\n', "").contains(landmark))
        .unwrap_or_else(|| panic!("no raw block holds {landmark:?}"))
}

/// The form reads as 56 blocks: the 64 recorded minus the 8 boundaries no
/// local signal parts.
#[test]
fn the_form_reads_as_fifty_five_blocks() {
    let blocks = raw_blocks_on_first_page("corpus/f1040.pdf");
    assert_eq!(blocks.len(), 55);
}

/// The first two checkboxes share their block across the 2.67pt gap: a
/// short line reads on.
#[test]
fn the_first_checkboxes_share_a_block() {
    let blocks = raw_blocks_on_first_page("corpus/f1040.pdf");
    assert_eq!(
        first_holding(&blocks, "Single"),
        first_holding(&blocks, "Married filing jointly"),
        "Single parted from the row below it"
    );
}

/// The second checkbox row opens its own block: a full line does not read on.
#[test]
fn the_third_checkbox_opens_its_own_block() {
    let blocks = raw_blocks_on_first_page("corpus/f1040.pdf");
    assert!(
        first_holding(&blocks, "Married filing jointly")
            < first_holding(&blocks, "Married filing separately"),
        "the filing rows joined into one block"
    );
}

/// The first income rows share their block: short lines read on.
#[test]
fn the_first_income_rows_share_a_block() {
    let blocks = raw_blocks_on_first_page("corpus/f1040.pdf");
    assert_eq!(
        first_holding(&blocks, "Total amount from Form"),
        first_holding(&blocks, "Household employee wages"),
        "the 1a row parted from the 1b row"
    );
}

/// KNOWN MISMATCH (1-7 of 9): the recordings split every third numbered row
/// (`1b` from `1c`, `1e` from `1f`, `1h` from `1i`, `2b` from `3a`, `4b`
/// from `c`, `3` from `6a`, `8` from `9`), each geometrically identical to
/// the joins around it (same gap, size and motion), and no local signal
/// parts them either. The first stands for all seven.
#[test]
fn the_second_and_third_income_rows_stay_together() {
    let blocks = raw_blocks_on_first_page("corpus/f1040.pdf");
    assert_eq!(
        first_holding(&blocks, "Household employee wages"),
        first_holding(&blocks, "Tip income"),
        "the 1b row parted from the 1c row"
    );
}

/// KNOWN MISMATCH (8 of 9): the recordings open a block at the section head
/// after a wrapped line, feature for feature like the overlap joins around
/// it (the matching join sits at the very next boundary).
#[test]
fn the_wrapped_line_reads_on_into_the_section_head() {
    let blocks = raw_blocks_on_first_page("corpus/f1040.pdf");
    assert_eq!(
        first_holding(&blocks, "more than half of 2025"),
        first_holding(&blocks, "Presidential Election Campaign"),
        "the wrapped line parted from the section head"
    );
}

/// KNOWN MISMATCH (9 of 9): the recordings open a block at `You` after the
/// wrapped paragraph, feature for feature like the overlap joins around it.
#[test]
fn the_paragraph_tail_reads_on_into_you() {
    let blocks = raw_blocks_on_first_page("corpus/f1040.pdf");
    assert_eq!(
        first_holding(&blocks, "will not change your tax"),
        first_holding(&blocks, "You "),
        "the paragraph tail parted from You"
    );
}

/// The checkbox grid's staggered rows part: forward into a partial overlap
/// opens a block.
#[test]
fn the_staggered_checkbox_rows_part() {
    let blocks = raw_blocks_on_first_page("corpus/f1040.pdf");
    assert!(
        first_holding(&blocks, "Full-time student") < first_holding(&blocks, "and totally"),
        "the staggered rows joined into one block"
    );
}

/// The structured page reads as its two blocks: the title, then the bullets.
#[test]
fn the_structured_page_reads_as_two_blocks() {
    let blocks = raw_blocks_on_first_page("fixtures/structured.pdf");
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[0].text, "Safety rules");
}
