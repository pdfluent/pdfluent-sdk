//! web_examples/extract_text_pdf_rust
//!
//! Source: <https://pdfluent.com/how-to/extract-text-pdf-rust> (fetched 2026-04-21)
//!
//! Validates `PdfDocument::text()` from RFC 0001 §9.1.

use pdfluent::prelude::*;

/// Extract all text from a PDF and print it.
pub fn run() -> Result<String> {
    let doc = PdfDocument::open("tests/fixtures/sample.pdf")?;
    let text = doc.text()?;
    Ok(text)
}

#[test]
#[ignore = "blocked on Epic 2 #1242 (Document lifecycle wiring)"]
fn extract_text_rust_runs() {
    let out = run().expect("text extraction");
    assert!(!out.is_empty(), "expected non-empty text output");
}

#[test]
fn extract_text_rust_compiles() {
    let _f: fn() -> Result<String> = run;
}
