//! web_examples/merge_pdfs_rust
//!
//! Source: <https://pdfluent.com/how-to/merge-pdfs-rust> (fetched 2026-04-21)
//!
//! Validates the `PdfMerger` builder API defined in RFC 0001 §9.2.
//!
//! # Website-drift note
//!
//! The published snippet writes to `/tmp/combined.pdf` without cleaning up.
//! Because `save()` refuses to clobber existing files per RFC §1.2, a second
//! run of the unmodified snippet would fail. This test removes the target
//! before running so CI is idempotent; the website snippet will be updated
//! as part of the #1237 content-audit pass.

use pdfluent::prelude::*;

const OUT: &str = "/tmp/pdfluent-bootstrap-merge-combined.pdf";

/// Merge two PDFs into a single combined document.
pub fn run() -> Result<()> {
    let merged = PdfMerger::new()
        .add(PdfDocument::open("tests/fixtures/sample.pdf")?)
        .add(PdfDocument::open("tests/fixtures/sample.pdf")?)
        .with_bookmarks(BookmarkMergeStrategy::Concat)
        .build()?;

    merged.save(OUT)?;
    Ok(())
}

#[test]
fn merge_pdfs_rust_compiles_and_runs() {
    // Enabled by Epic 2 #1243 wiring.
    let _ = std::fs::remove_file(OUT);
    run().expect("merge flow");
    let _ = std::fs::remove_file(OUT);
}

#[test]
fn merge_pdfs_rust_compiles() {
    // Just importing the example is enough to verify the API surface;
    // actual execution depends on Epic 2 wiring.
    let _f: fn() -> Result<()> = run;
}
