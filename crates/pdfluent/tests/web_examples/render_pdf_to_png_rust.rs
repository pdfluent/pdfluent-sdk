//! web_examples/render_pdf_to_png_rust
//!
//! Source: <https://pdfluent.com/how-to/render-pdf-to-png-rust> (fetched 2026-04-22)
//!
//! Auto-extracted by `tools/pdfluent-snippet-extract` (#1236).
//! Do not edit by hand — re-run the extractor instead.

use pdfluent::{PdfDocument, RenderOptions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let doc = PdfDocument::open("document.pdf")?;
    let page = doc.page(0)?;

    let opts = RenderOptions::default().dpi(150);
    let image = page.render(&opts)?;
    image.save_png("page_1.png")?;

    println!("{}x{} pixels", image.width, image.height);
    Ok(())
}
