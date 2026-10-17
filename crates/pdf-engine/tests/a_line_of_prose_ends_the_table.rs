//! A line of running text under a table ends it (#638).
//!
//! A DocLayNet validation page (ASX Limited's 2003 annual report, page 56): a
//! dividends table, then the paragraph and the two-item list that explain it,
//! all inside the region the table was found in. Read as one grid, the region
//! wrote each line of that text as a table row of one cell -- the commonest
//! thing our tables swallow on DocLayNet. Take the prose cut out of
//! `tables::find_whole` and the paragraph comes back as table cells.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_engine::{BlockKind, ExtractOptions, PdfDocument};

#[test]
fn a_line_of_prose_ends_the_table() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/doclaynet/asx_2003_p56_prose_under_a_table.pdf"
    ))
    .expect("fixture");
    let blocks = PdfDocument::open(bytes)
        .expect("open")
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    let prose_in_cells: Vec<&str> = blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::TableCell { .. }))
        .map(|b| b.text.as_str())
        .filter(|t| {
            t.contains("franking credits is dependent") || t.contains("available amounts are based")
        })
        .collect();
    assert!(
        prose_in_cells.is_empty(),
        "the text under the table came back as its rows: {prose_in_cells:?}"
    );
    assert!(
        blocks
            .iter()
            .any(|b| !matches!(b.kind, BlockKind::TableCell { .. })
                && b.text.contains("available amounts are based")),
        "the paragraph is gone"
    );
    assert!(
        blocks
            .iter()
            .any(|b| matches!(b.kind, BlockKind::TableCell { .. }) && b.text.contains("2003")),
        "the table itself is gone"
    );
}
