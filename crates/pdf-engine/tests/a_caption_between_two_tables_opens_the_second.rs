//! A caption between two tables ends the first and opens the second (#574).
//!
//! A PubTables-1M page (train split, PMC1064095 page index 3) sets Table 1 and
//! Table 2 one under the other with nothing between them but the second one's
//! caption. The page's table region holds both, and read as one grid it wrote
//! them, and the `Table 2` row, as one table. On PubTables' round of 29-09 that
//! was the largest share of the gap to Docling. Take the caption cut out of
//! `tables::find_whole` and this page comes back with one table holding
//! `Table 2` as a cell.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_engine::{BlockKind, ExtractOptions, PdfDocument};

#[test]
fn a_caption_between_two_tables_opens_the_second() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/pubtables/pmc1064095_p3_two_tables.pdf"
    ))
    .expect("fixture");
    let blocks = PdfDocument::open(bytes)
        .expect("open")
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    let captions_in_cells: Vec<&str> = blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::TableCell { .. }))
        .map(|b| b.text.as_str())
        .filter(|t| t.starts_with("Table 2"))
        .collect();
    assert!(
        captions_in_cells.is_empty(),
        "the caption of the second table is a cell of the first: {captions_in_cells:?}"
    );
    let tables = blocks
        .iter()
        .filter(|b| {
            matches!(
                b.kind,
                BlockKind::TableCell {
                    row: 0,
                    column: 0,
                    ..
                }
            )
        })
        .count();
    assert_eq!(tables, 2, "the two tables are not two");
    assert!(
        blocks.iter().any(
            |b| !matches!(b.kind, BlockKind::TableCell { .. }) && b.text.starts_with("Table 2")
        ),
        "the caption of the second table is gone"
    );
}
