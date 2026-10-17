// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Lines are x-runs under raw order (E2a).
//!
//! Within a raw block, consecutive runs share a line while they advance
//! along it with at most a word space between them; an x-gap past sixtenths
//! of an em either way opens the next line. The threshold sits between the
//! 2.65pt same-line gap at 7pt type (0.38em, `Deceased` to `MM`) and the
//! 6.5pt line split at 8pt type (0.81em, the dot leaders) the recordings
//! pin; a step back within it joins too (`1a` reads back 0.28em into its
//! own line).

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
        .position(|b| b.text.contains(landmark))
        .unwrap_or_else(|| panic!("no raw block holds {landmark:?}"))
}

/// Each bullet stands alone on its line and the text follows on the next:
/// the 10.5pt gap at 10pt type parts them.
#[test]
fn bullets_part_from_their_text() {
    let blocks = raw_blocks_on_first_page("fixtures/structured.pdf");
    let bullets = &blocks[first_holding(&blocks, "gangways")];
    assert_eq!(
        bullets.text,
        "•\nWear the issued boots on the floor.\n•\nKeep the gangways clear of stock.\n•\nReport a spill before you clean it."
    );
}

/// One run is one line.
#[test]
fn the_title_is_one_line() {
    let blocks = raw_blocks_on_first_page("fixtures/structured.pdf");
    let title = &blocks[first_holding(&blocks, "Safety rules")];
    assert_eq!(title.text, "Safety rules");
}

/// The title block reads as its three x-runs.
#[test]
fn the_title_block_is_three_lines() {
    let blocks = raw_blocks_on_first_page("corpus/f1040.pdf");
    let title = &blocks[first_holding(&blocks, "1040")];
    assert_eq!(
        title.text,
        "1040 \n2025\nU.S. Individual Income Tax Return "
    );
}

/// The header row reads as its two x-runs.
#[test]
fn the_header_row_is_two_lines() {
    let blocks = raw_blocks_on_first_page("corpus/f1040.pdf");
    let header = &blocks[first_holding(&blocks, "OMB No.")];
    assert_eq!(
        header.text,
        "OMB No. 1545-0074\nIRS Use Only—Do not write or staple in this space. "
    );
}

/// The tax-year row reads as its four x-runs.
#[test]
fn the_tax_year_row_is_four_lines() {
    let blocks = raw_blocks_on_first_page("corpus/f1040.pdf");
    let row = &blocks[first_holding(&blocks, "For the year")];
    assert_eq!(
        row.text,
        "For the year Jan. 1–Dec. 31, 2025, or other tax year beginning \n, 2025, ending \n, 20 \nSee separate instructions."
    );
}

/// The `1a` line number reads back into its own line: a step back within
/// sixtenths of an em joins.
#[test]
fn the_line_number_reads_back_into_its_line() {
    let blocks = raw_blocks_on_first_page("corpus/f1040.pdf");
    let row = &blocks[first_holding(&blocks, "Total amount from Form")];
    assert!(
        row.text.split('\n').any(|line| line == "1 a"),
        "the 1a line number parted: {:?}",
        row.text
    );
}

/// KNOWN MISMATCH: the recordings keep the first dot of a short leader on
/// the text's line (`Qualified dividends .`), while the long leaders part
/// before every dot. No local signal separates the two -- the short gap is
/// 77pt, the long one 209pt, both far past the line gap -- so the first dot
/// opens its own line here.
#[test]
fn the_first_dot_of_a_short_leader_opens_its_own_line() {
    let blocks = raw_blocks_on_first_page("corpus/f1040.pdf");
    let row = &blocks[first_holding(&blocks, "Qualified dividends")];
    assert!(
        row.text.contains("Qualified dividends \n."),
        "the first dot joined the text line: {:?}",
        row.text
    );
}

/// The social-security row's runs share one line across their 2.65pt gap.
#[test]
fn the_social_security_row_is_one_line() {
    let blocks = raw_blocks_on_first_page("corpus/f1040.pdf");
    let row = &blocks[first_holding(&blocks, "Deceased")];
    let line = row
        .text
        .split('\n')
        .find(|line| line.contains("Deceased"))
        .expect("a line holding Deceased");
    for run in ["MM", "DD", "YYYY"] {
        assert!(
            line.contains(run),
            "the social-security row parted: {line:?}"
        );
    }
}

/// No dot leader ever shares its line: past their 6.5pt gaps they stand one
/// per line, however the block rule groups the rows around them.
#[test]
fn dot_leaders_stand_one_per_line() {
    let blocks = raw_blocks_on_first_page("corpus/f1040.pdf");
    let row = &blocks[first_holding(&blocks, "Household employee")];
    let mut dots = 0;
    for line in row.text.split('\n') {
        if line.contains('.') && line.trim_matches('.').is_empty() {
            assert_eq!(line, ".", "dot leaders joined: {line:?}");
            dots += 1;
        }
    }
    assert!(
        dots >= 12,
        "dot leaders lost: {dots} single-dot lines in {:?}",
        row.text
    );
}
