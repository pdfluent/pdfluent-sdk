//! Prose the builder reads as a grid is read back as prose (#574).
//!
//! An arXiv page (DocLayNet validation, `1001.0809.pdf` page 3) with no table
//! on it: the builder reads a run of its lines as a table with the evidence it
//! has, and the table judge scores that table 0.0002. A table the judge
//! rejects outright (`judge::REJECT`) is prose whatever the builder's record
//! says -- the same shape as ParseBench's `vidsa2`, three columns of running
//! text read as one grid (0.004). Set `REJECT` to zero and this page comes back
//! with a table of sentence fragments.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_engine::{BlockKind, ExtractOptions, PdfDocument};

#[test]
fn a_prose_page_the_builder_reads_as_a_grid_is_prose() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/doclaynet/arxiv_1001_0809_prose.pdf"
    ))
    .expect("fixture");
    let blocks = PdfDocument::open(bytes)
        .expect("open")
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    let cells: Vec<&str> = blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::TableCell { .. }))
        .map(|b| b.text.as_str())
        .collect();
    assert!(
        cells.is_empty(),
        "a page of prose came back with a table: {cells:?}"
    );
}
