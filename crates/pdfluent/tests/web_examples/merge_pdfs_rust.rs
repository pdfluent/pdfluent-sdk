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

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent::prelude::*;
use std::path::PathBuf;

fn out_path() -> PathBuf {
    std::env::temp_dir().join("pdfluent-bootstrap-merge-combined.pdf")
}

/// Merge two PDFs into a single combined document.
///
/// This is parametrised over the target path so the test can keep
/// things cross-platform (website snippet uses a hard-coded `/tmp/...`
/// which is Unix-only).
pub fn run_to(out: &std::path::Path) -> Result<()> {
    let merged = PdfMerger::new()
        .add(PdfDocument::open("tests/fixtures/sample.pdf")?)
        .add(PdfDocument::open("tests/fixtures/sample.pdf")?)
        .with_bookmarks(BookmarkMergeStrategy::Concat)
        .build()?;

    merged.save(out)?;
    Ok(())
}

/// Website-snippet entry-point kept for the `_compiles` test. In the
/// published how-to this targets `/tmp/combined.pdf`; here we route to a
/// cross-platform temp path inside the test so CI works on Windows.
pub fn run() -> Result<()> {
    run_to(&out_path())
}

#[test]
fn merge_pdfs_rust_compiles_and_runs() {
    // Enabled by Epic 2 #1243 wiring.
    let path = out_path();
    let _ = std::fs::remove_file(&path);
    run_to(&path).expect("merge flow");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn merge_pdfs_rust_compiles() {
    // Just importing the example is enough to verify the API surface;
    // actual execution depends on Epic 2 wiring.
    let _f: fn() -> Result<()> = run;
}
