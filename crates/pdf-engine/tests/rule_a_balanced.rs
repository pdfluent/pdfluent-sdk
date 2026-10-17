// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Rule A2 (`drawing_dense`) through the balanced tier's page assembly (#636).
//!
//! A layout "table" box over three or more drawn clusters assembles no
//! table (owner decision D37: the only part of rule A in the product); the
//! same box over a forced low-judge wordless grid, a ruled word table, a
//! financial table or a high-judge forced table still does. Operator-built pages, no models.

#![cfg(feature = "ort-layout")]

mod rule_a_fixtures;

use pdf_engine::balanced::{balanced_page_markdown, BalancedOptions};
use pdf_engine::layout::LayoutBox;
use pdf_engine::PdfDocument;
use rule_a_fixtures::*;

/// A balanced "table" box over `[top_y, bottom_y]` (PDF points, y up) of a
/// 612 x 792 page rendered 1:1.
fn balanced_over(bytes: Vec<u8>, top_y: f64, bottom_y: f64) -> String {
    let doc = PdfDocument::open(bytes).expect("open pdf");
    let boxes = vec![LayoutBox {
        label: "table".to_string(),
        score: 0.9,
        bbox_px: [
            40.0,
            (792.0 - top_y) as f32,
            560.0,
            (792.0 - bottom_y) as f32,
        ],
        render_w: 612,
        render_h: 792,
    }];
    balanced_page_markdown(&doc, 0, &boxes, &BalancedOptions::default()).expect("balanced runs")
}

/// The text of every `<table>` in the markdown.
fn tables_in(markdown: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = markdown;
    while let Some(start) = rest.find("<table") {
        let after = &rest[start..];
        let end = after.find("</table>").map_or(after.len(), |e| e + 8);
        out.push(&after[..end]);
        rest = &after[end..];
    }
    out
}

#[test]
fn a_forced_low_judge_wordless_grid_is_still_a_table_in_a_balanced_box() {
    let markdown = balanced_over(pdf_of(&forced_low_judge_wordless(), vec![]), 720.0, 640.0);
    assert!(
        !tables_in(&markdown).is_empty(),
        "the low-judge wordless grid was withdrawn: {markdown}"
    );
}

#[test]
fn a_table_box_over_three_drawn_clusters_is_no_table_in_a_balanced_box() {
    // The box holds the lower rows, which stand over the drawing.
    let markdown = balanced_over(word_table_over(3), 660.0, 600.0);
    for word in ["Plums", "Grapes", "Lemons", "4.25", "0.95"] {
        assert!(markdown.contains(word), "text {word:?} lost: {markdown}");
    }
    assert!(
        !tables_in(&markdown)
            .iter()
            .any(|table| table.contains("Grapes")),
        "the rows over the drawing came back as a table: {markdown}"
    );
}

#[test]
fn fewer_than_three_drawn_clusters_keep_the_table_in_a_balanced_box() {
    for n in [1, 2] {
        let markdown = balanced_over(word_table_over(n), 660.0, 600.0);
        assert!(
            tables_in(&markdown)
                .iter()
                .any(|table| table.contains("Grapes")),
            "{n} drawn cluster(s) cost the table: {markdown}"
        );
    }
}

#[test]
fn a_ruled_word_table_is_a_table_in_a_balanced_box() {
    let markdown = balanced_over(ruled_word_table(), 720.0, 630.0);
    assert!(
        tables_in(&markdown)
            .iter()
            .any(|table| table.contains("Plums")),
        "the ruled word table must assemble: {markdown}"
    );
}

#[test]
fn a_numeric_financial_table_is_a_table_in_a_balanced_box() {
    let markdown = balanced_over(pdf_of(&financial_table(), vec![]), 720.0, 600.0);
    assert!(
        tables_in(&markdown)
            .iter()
            .any(|table| table.contains("1,234.50")),
        "the financial table must assemble: {markdown}"
    );
}

#[test]
fn a_forced_high_judge_table_is_a_table_in_a_balanced_box() {
    let markdown = balanced_over(pdf_of(&forced_high_judge(), vec![]), 720.0, 600.0);
    assert!(
        tables_in(&markdown)
            .iter()
            .any(|table| table.contains("CH")),
        "the high-judge forced table must assemble: {markdown}"
    );
}

#[test]
fn a_fully_ruled_table_with_shaded_cells_is_a_table_in_a_balanced_box() {
    // Three drawn clusters stand inside the grid, but its cells are bounded
    // by rulings (ruled fraction above the A2 limit): the shading is the
    // table's own, and the grid assembles.
    let markdown = balanced_over(ruled_shaded_table(), 720.0, 630.0);
    assert!(
        tables_in(&markdown)
            .iter()
            .any(|table| table.contains("Apples")),
        "the ruled shaded table must assemble: {markdown}"
    );
}
