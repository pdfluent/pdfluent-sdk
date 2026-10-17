// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Rule A2 (`drawing_dense`) withdraws false tables from the default output (#636).
//!
//! Operator-built pages through the public path (`extract_blocks`): a table
//! box over three or more drawn clusters goes back to text with every
//! character kept; a forced low-judge wordless grid, a ruled word table, a numeric financial table, a table with
//! one small icon and a high-judge forced table stay tables.

mod rule_a_fixtures;

use pdf_engine::Block;
use rule_a_fixtures::*;

fn cell_texts(blocks: &[Block]) -> Vec<String> {
    cells_of(blocks).iter().map(|b| b.text.clone()).collect()
}

#[test]
fn a_forced_low_judge_wordless_grid_is_not_withdrawn() {
    // Owner decision D37: the judge number and the missing word decide
    // nothing in the product. The grid stays a table with all its cells.
    let blocks = blocks_of(pdf_of(&forced_low_judge_wordless(), vec![]));
    assert_eq!(
        cell_texts(&blocks),
        vec!["-4", "5", "-10", "-2", "12", "-10", "Si"],
        "the low-judge wordless grid was withdrawn: {:?}",
        texts_of(&blocks)
    );
}

#[test]
fn a_table_box_over_three_drawn_clusters_is_withdrawn() {
    let blocks = blocks_of(word_table_over(3));
    let rows: Vec<&str> = texts_of(&blocks);
    // The upper rows stay a table beside it; the lower rows, over the
    // drawing, are text again.
    assert_eq!(
        cell_texts(&blocks).len(),
        9,
        "only the upper half stays a table: {rows:?}"
    );
    for word in ["Grapes", "Lemons", "4.25", "0.95", "15", "9"] {
        assert!(
            blocks.iter().any(|b| b.text.contains(word)),
            "text {word:?} lost: {rows:?}"
        );
    }
    assert!(
        !cell_texts(&blocks).iter().any(|t| t == "Grapes"),
        "the lower rows are still cells: {rows:?}"
    );
}

#[test]
fn fewer_than_three_drawn_clusters_keep_the_table() {
    for n in [1, 2] {
        let blocks = blocks_of(word_table_over(n));
        assert_eq!(
            cell_texts(&blocks).len(),
            18,
            "{n} drawn cluster(s) cost the table its cells"
        );
        assert!(
            cell_texts(&blocks).iter().any(|t| t == "Grapes"),
            "{n} drawn cluster(s): the lower rows are not cells"
        );
    }
}

#[test]
fn a_ruled_word_table_is_kept() {
    let blocks = blocks_of(ruled_word_table());
    assert_eq!(cell_texts(&blocks).len(), 12);
}

#[test]
fn a_numeric_financial_table_is_kept() {
    let blocks = blocks_of(pdf_of(&financial_table(), vec![]));
    let cells = cell_texts(&blocks);
    assert_eq!(cells.len(), 18, "financial table lost cells: {cells:?}");
    assert!(cells.iter().any(|t| t == "1,234.50"));
}

#[test]
fn a_forced_high_judge_table_is_kept() {
    let blocks = blocks_of(pdf_of(&forced_high_judge(), vec![]));
    assert!(
        cells_of(&blocks).len() >= 10,
        "the high-judge forced table was withdrawn: {:?}",
        texts_of(&blocks)
    );
}

#[test]
fn a_fully_ruled_table_with_shaded_cells_is_kept() {
    // Nine filled rectangles in three drawn clusters stand inside the box (drawing_dense), but every
    // cell is bounded by rulings: the drawings are the table's own shading.
    let blocks = blocks_of(ruled_shaded_table());
    assert_eq!(
        cell_texts(&blocks).len(),
        12,
        "the ruled shaded table lost cells: {:?}",
        texts_of(&blocks)
    );
}
