//! web_examples/extract_text_pdf_rust
//!
//! Source: <https://pdfluent.com/how-to/extract-text-pdf-rust> (fetched 2026-04-22)
//!
//! Auto-extracted by `tools/pdfluent-snippet-extract` (#1236).
//! Do not edit by hand — re-run the extractor instead.

use pdfluent::PdfDocument;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = PdfDocument::open("document.pdf")?;

    for (i, page) in doc.pages().enumerate() {
        let text = page.extract_text()?;
        println!("--- Page {} ---", i + 1);
        println!("{}", text);
    }
    Ok(())
}
