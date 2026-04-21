//! web_examples/encrypt_pdf_rust
//!
//! Source: <https://pdfluent.com/how-to/encrypt-pdf-rust> (fetched 2026-04-21)
//!
//! Validates `PdfDocument::encrypt()` + `EncryptOptions::aes256()` +
//! `Permissions::print_only()` from RFC 0001 §3.1.

use pdfluent::prelude::*;
use std::path::PathBuf;

fn out_path() -> PathBuf {
    std::env::temp_dir().join("pdfluent-bootstrap-encrypted.pdf")
}

/// Encrypt a PDF with AES-256 and print-only permissions.
///
/// # Website-drift note
///
/// The published snippet writes to `/tmp/encrypted.pdf` without cleanup
/// and without `save_with(overwrite=true)`. Because our save-contract
/// refuses to clobber by default (RFC §1.2), this test uses a
/// cross-platform temp path and removes-before-run. Tracked for
/// Epic 6 #1237 content audit.
pub fn run_to(out: &std::path::Path) -> Result<()> {
    let mut doc = PdfDocument::open("tests/fixtures/sample.pdf")?;

    doc.encrypt(
        EncryptOptions::aes256()
            .with_user_password("user-secret")
            .with_owner_password("owner-secret")
            .with_permissions(Permissions::print_only()),
    )?;

    doc.save(out)?;
    Ok(())
}

/// Website-snippet entry point kept for the `_compiles` test.
pub fn run() -> Result<()> {
    run_to(&out_path())
}

#[test]
fn encrypt_pdf_rust_runs() {
    // Enabled by Epic 2 #1244 wiring.
    let path = out_path();
    let _ = std::fs::remove_file(&path);
    run_to(&path).expect("encrypt flow");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn encrypt_pdf_rust_compiles() {
    let _f: fn() -> Result<()> = run;
}
