//! web_examples/encrypt_pdf_rust
//!
//! Source: <https://pdfluent.com/how-to/encrypt-pdf-rust> (fetched 2026-04-22)
//!
//! Auto-extracted by `tools/pdfluent-snippet-extract` (#1236).
//! Do not edit by hand — re-run the extractor instead.

use pdfluent::{EncryptOptions, PdfDocument, PdfPermissions};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = PdfDocument::open("report.pdf")?;

    let opts = EncryptOptions::aes256()
        .user_password("open123")
        .owner_password("owner_secret")
        .permissions(PdfPermissions::all_except_edit());

    doc.encrypt(&opts)?;
    doc.save("report_protected.pdf")?;
    Ok(())
}
