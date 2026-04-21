//! web_examples/encrypt_pdf_rust
//!
//! Source: <https://pdfluent.com/how-to/encrypt-pdf-rust> (fetched 2026-04-21)
//!
//! Validates `PdfDocument::encrypt()` + `EncryptOptions::aes256()` +
//! `Permissions::print_only()` from RFC 0001 §3.1.

use pdfluent::prelude::*;

/// Encrypt a PDF with AES-256 and print-only permissions.
pub fn run() -> Result<()> {
    let mut doc = PdfDocument::open("tests/fixtures/sample.pdf")?;

    doc.encrypt(
        EncryptOptions::aes256()
            .with_user_password("user-secret")
            .with_owner_password("owner-secret")
            .with_permissions(Permissions::print_only()),
    )?;

    doc.save("/tmp/encrypted.pdf")?;
    Ok(())
}

#[test]
#[ignore = "blocked on Epic 2 #1244 (Security & encryption wiring)"]
fn encrypt_pdf_rust_runs() {
    run().expect("encrypt flow");
}

#[test]
fn encrypt_pdf_rust_compiles() {
    let _f: fn() -> Result<()> = run;
}
