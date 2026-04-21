//! web_examples/fill_pdf_form_rust
//!
//! Source: <https://pdfluent.com/how-to/fill-pdf-form-rust> (fetched 2026-04-21)
//!
//! Validates the `PdfDocument::form_mut()` accessor and `PdfFormMut::set_*`
//! chain from RFC 0001 §2, §3.2.

use pdfluent::prelude::*;

/// Fill three AcroForm fields and save the result.
pub fn run() -> Result<()> {
    let mut doc = PdfDocument::open("tests/fixtures/form.pdf")?;

    {
        let mut form = doc.form_mut()?;
        form.set_text("first_name", "Jane")?
            .set_text("last_name", "Smith")?
            .set_checkbox("agree_terms", true)?;
    }

    doc.save("/tmp/form_filled.pdf")?;
    Ok(())
}

#[test]
#[ignore = "blocked on Epic 2 #1245 (Metadata & form_fields wiring)"]
fn fill_pdf_form_rust_runs() {
    run().expect("fill-form flow");
}

#[test]
fn fill_pdf_form_rust_compiles() {
    let _f: fn() -> Result<()> = run;
}
