//! web_examples/subset_fonts_rust
//!
//! Source: <https://pdfluent.com/how-to/subset-fonts-rust> (placeholder — page not yet published)
//!
//! Compile-test + runtime coverage for the 3C-2 `subset_fonts()` surface.
//! Auto-extractor replaces this file once the website page ships.

use pdfluent::prelude::*;

/// Run the documented `subset_fonts` flow.
pub fn run(src: &std::path::Path, out: &std::path::Path) -> Result<FontSubsetReport> {
    let mut doc = PdfDocument::open(src)?;
    let report = doc.subset_fonts()?;
    doc.save_with(out, SaveOptions::new().with_overwrite(true))?;
    Ok(report)
}

#[test]
fn subset_fonts_rust_compiles() {
    let _f: fn(&std::path::Path, &std::path::Path) -> Result<FontSubsetReport> = run;
}

#[test]
fn subset_fonts_rust_runs() {
    let out = std::env::temp_dir().join("pdfluent-web-example-subset.pdf");
    let _ = std::fs::remove_file(&out);
    let _ = run(std::path::Path::new("tests/fixtures/sample.pdf"), &out)
        .expect("subset_fonts pipeline");
    assert!(out.exists());
    let _ = std::fs::remove_file(&out);
}
