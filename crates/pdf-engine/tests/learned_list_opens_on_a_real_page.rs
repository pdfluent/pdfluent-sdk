// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The list-splitting model on the page it was shipped for (#599).
//!
//! Page 7 of Woodward Governor's 2002 annual report, from DocLayNet v1.2
//! train (registered in `scripts/ci/corpus_herkomst.py`): the markets the
//! company serves, as four short bulleted lists side by side. The heuristics
//! read each column as ONE paragraph -- `• Power Plants • Distributed Power •
//! Back-Up Power` -- and no page built from operators reproduces that: every
//! constructed version came out as a table or as confirmed items. So the
//! operator page of this switch is the real one.
//!
//! With `list_opens` on, each bullet is an item of its own; off, the column
//! stays one block -- the block model may promote it to an item, but one item
//! holding three bullets is the same defect.

use pdf_engine::learned::{refine_with, Switches};
use pdf_engine::{Block, BlockKind, ExtractOptions, PdfDocument};

const PAGE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/doclaynet/nasdaq_wwd_2002_p7.pdf"
);

fn blocks(list_opens: bool) -> Vec<Block> {
    let doc = PdfDocument::open(std::fs::read(PAGE).expect("fixture")).expect("open");
    let sizes: Vec<(f64, f64)> = (0..doc.page_count())
        .map(|p| {
            doc.page_geometry(p)
                .map(|g| g.effective_dimensions())
                .unwrap_or((0.0, 0.0))
        })
        .collect();
    let before = doc.extract_blocks_before_learning().expect("blocks");
    refine_with(
        before,
        &sizes,
        Switches {
            list_item: true,
            list_opens,
            // This page is about the list boundary; the two heading
            // switches are measured by their own tests (#599, #602).
            heading_lines: false,
            captions: false,
            line_gnn: false,
            line_image: false,
            line_gnn_v2: false,
            demote_per_cell: false,
            table_region: false,
            table_structure: false,
            table_judge: false,
            table_lattice: false,
            table_stitch_header: false,
            table_stitch_rows: false,
            table_stitch_merge: false,
            table_is_v2: false,
            table_drop_empty_columns: false,
            table_cell_text_by_line: false,
            table_v2_release_rejoin: false,
            table_v2_release_column_order: false,
            table_split_side_by_side: false,
            table_split_stacked: false,
            table_runs_on_figures: false,
            markdown_header_block_figures: false,
            table_corridor_split: false,
            markdown_header_block_rowspan_crossing: false,
            markdown_header_block_word_table: false,
            markdown_header_block_mixed_body: false,
            markdown_headerless_html: false,
            markdown_header_block_stub_years: false,
        },
    )
}

fn shape(blocks: &[Block]) -> Vec<(String, String)> {
    blocks
        .iter()
        .map(|b| (format!("{:?}", b.kind), b.text.chars().take(40).collect()))
        .collect()
}

#[test]
fn a_column_of_bullets_read_as_one_paragraph_is_split_into_its_items() {
    let on = blocks(true);
    // Power Generation: the block model already calls the column an item, so
    // this is the cut between lines. Transportation and Aerospace it leaves a
    // paragraph, so there the first line has to open an item of its own.
    for item in [
        "Power Plants",
        "Distributed Power",
        "Back-Up Power",
        "Marine Propulsion",
        "Locomotive",
        "Commercial",
        "Military",
    ] {
        let found = on
            .iter()
            .find(|b| b.text == item)
            .unwrap_or_else(|| panic!("no block reads {item:?}: {:?}", shape(&on)));
        assert!(
            matches!(found.kind, BlockKind::ListItem { ordinal: None, .. }),
            "{item:?} is not a bulleted item: {:?}",
            shape(&on)
        );
    }

    // Process Industries: the bullets are a block of their own, their words
    // another. Cut at every line they would be items with no text.
    assert!(
        on.iter()
            .all(|b| !matches!(b.kind, BlockKind::ListItem { .. }) || !b.text.trim().is_empty()),
        "an item with no text: {:?}",
        shape(&on)
    );

    // The shipped switch, not only the operator: the default path splits too.
    let doc = PdfDocument::open(std::fs::read(PAGE).expect("fixture")).expect("open");
    let shipped = doc
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    assert!(
        shipped.iter().any(|b| b.text == "Distributed Power"),
        "extract_blocks does not split the column -- is list_opens switched off in SWITCHES? {:?}",
        shape(&shipped)
    );

    // The page no longer shows the switch on its own (#623): with the bold its
    // fonts name read right, the heuristics already split the column without
    // the line model. The assertions above still hold the output; a page that
    // needs the switch to split is T5's to find (#574).
}
