//! web_examples/merge_pdfs_rust
//!
//! Source: <https://pdfluent.com/how-to/merge-pdfs-rust> (fetched 2026-04-21)
//!
//! Validates the `PdfMerger` builder API defined in RFC 0001 §9.2.
//!
//! This example is `#[ignore]` until Epic 2 split #1243 (Merge & combine)
//! wires `PdfMerger::build()` to `pdf_manip::pages::merge_docs`.

use pdfluent::prelude::*;

/// Merge three PDFs into a single combined document.
pub fn run() -> Result<()> {
    let merged = PdfMerger::new()
        .add(PdfDocument::open("tests/fixtures/sample.pdf")?)
        .add(PdfDocument::open("tests/fixtures/sample.pdf")?)
        .with_bookmarks(BookmarkMergeStrategy::Concat)
        .build()?;

    merged.save("/tmp/combined.pdf")?;
    Ok(())
}

#[test]
#[ignore = "blocked on Epic 2 #1243 (Merge & combine methods wiring)"]
fn merge_pdfs_rust_compiles_and_runs() {
    run().expect("merge flow");
}

#[test]
fn merge_pdfs_rust_compiles() {
    // Just importing the example is enough to verify the API surface;
    // actual execution depends on Epic 2 wiring.
    let _f: fn() -> Result<()> = run;
}
