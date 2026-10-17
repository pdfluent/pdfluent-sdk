//! The notes set under a table are not rows of it (#574).
//!
//! A PubTables-1M page (train split, PMC2438475 page index 1): Table 1, then
//! an asterisked note three lines long and the article's `doi:...t001` line
//! under it, inside the region the table was found in. Read as one grid, the
//! region wrote the note as rows of one cell each, and the annotation -- and a
//! reader -- stops the table at its last full row. Take the trim of the thin
//! bands at the end of a stretch out of `tables::find_whole` and the note comes
//! back as table cells.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_engine::{BlockKind, ExtractOptions, PdfDocument};

#[test]
fn the_notes_under_a_table_are_not_its_rows() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/pubtables/pmc2438475_p1_table_notes.pdf"
    ))
    .expect("fixture");
    let blocks = PdfDocument::open(bytes)
        .expect("open")
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    let in_cells: Vec<&str> = blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::TableCell { .. }))
        .map(|b| b.text.as_str())
        .filter(|t| t.contains("Isolation frequency") || t.contains("doi:"))
        .collect();
    assert!(
        in_cells.is_empty(),
        "the note under the table came back as its rows: {in_cells:?}"
    );
    assert!(
        blocks
            .iter()
            .any(|b| b.text.contains("Isolation frequency from 363 cDNA clones")),
        "the note is gone"
    );
    assert!(
        blocks
            .iter()
            .any(|b| matches!(b.kind, BlockKind::TableCell { .. })),
        "the table itself is gone"
    );
}
