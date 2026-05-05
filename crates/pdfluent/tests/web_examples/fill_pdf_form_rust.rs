//! web_examples/fill_pdf_form_rust
//!
//! Source: <https://pdfluent.com/how-to/fill-pdf-form-rust> (fetched 2026-04-22)
//!
//! Auto-extracted by `tools/pdfluent-snippet-extract` (#1236).
//! Do not edit by hand — re-run the extractor instead.

use pdfluent::PdfDocument;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = PdfDocument::open("application.pdf")?;
    let mut form = doc.form_mut()?;

    form.set_text("first_name", "Jane")?;
    form.set_text("last_name", "Smith")?;
    form.set_checkbox("agree_terms", true)?;
    form.set_dropdown("country", "Netherlands")?;

    doc.save("application_filled.pdf")?;
    Ok(())
}
