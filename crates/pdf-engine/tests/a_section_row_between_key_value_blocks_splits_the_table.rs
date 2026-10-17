//! A section row between two key-value blocks splits the table (#638).
//!
//! A PubTables-1M page (train split, PMC2959354 page index 3): an Acta Cryst E
//! crystal-data block of `label = value` cells, a row holding only `Data
//! collection`, then the second key-value block, all read as one two-column
//! table. In the source they are separate tables, and the truth annotates them
//! as such. Take the split out of `learned::tables::refine_judged` and the page
//! comes back as one table with `Data collection` as a cell of it.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_engine::{Block, BlockKind, ExtractOptions, PdfDocument};

/// Tables as the serializer reads them: maximal runs of adjacent cells whose
/// `(row, column)` climbs with every block.
fn tables(blocks: &[Block]) -> Vec<&[Block]> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < blocks.len() {
        let start = at;
        let mut last: Option<(u16, u16)> = None;
        while at < blocks.len() {
            match blocks[at].kind {
                BlockKind::TableCell { row, column, .. }
                    if last.is_none_or(|l| (row, column) > l) =>
                {
                    last = Some((row, column));
                    at += 1;
                }
                _ => break,
            }
        }
        if at > start {
            out.push(&blocks[start..at]);
        } else {
            at += 1;
        }
    }
    out
}

#[test]
fn a_section_row_between_key_value_blocks_splits_the_table() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/pubtables/pmc2959354_p3_crystal_data.pdf"
    ))
    .expect("fixture");
    let blocks = PdfDocument::open(bytes)
        .expect("open")
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    let found = tables(&blocks);
    assert_eq!(
        found.len(),
        2,
        "the crystal page came back as {} tables, not two",
        found.len()
    );
    for (n, table) in found.iter().enumerate() {
        assert!(
            matches!(table[0].kind, BlockKind::TableCell { row: 0, .. }),
            "table {n} does not open at row 0"
        );
    }
    let section: Vec<&Block> = blocks
        .iter()
        .filter(|b| b.text == "Data collection")
        .collect();
    assert_eq!(
        section.len(),
        1,
        "the section row is gone or came back twice: {} blocks",
        section.len()
    );
    assert!(
        !matches!(section[0].kind, BlockKind::TableCell { .. }),
        "the section row is still a table cell"
    );
    assert!(
        matches!(
            section[0].kind,
            BlockKind::Heading { .. } | BlockKind::Paragraph
        ),
        "the section row is a {:?}, not a heading or prose",
        section[0].kind
    );
}
