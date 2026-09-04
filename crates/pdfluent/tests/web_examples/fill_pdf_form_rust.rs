//! web_examples/fill_pdf_form_rust
//!
//! Source: <https://pdfluent.com/how-to/fill-pdf-form-rust> (fetched 2026-04-21)
//!
//! Validates the `PdfDocument::form_mut()` accessor and `PdfFormMut::set_*`
//! chain from RFC 0001 §2, §3.2.
//!
//! # Website-drift notes
//!
//! - The published snippet had `doc.form_mut()?` returning `Result`. RFC
//!   0001 validation pass 01 changed `form_mut()` to return `PdfFormMut`
//!   directly (no `Result`), for consistency with `metadata_mut()`.
//! - The snippet writes to `/tmp/form_filled.pdf` without cleanup and
//!   relies on `/tmp`. Because our save-contract refuses to clobber by
//!   default (RFC §1.2) and `/tmp` is not cross-platform, this test uses
//!   `std::env::temp_dir()` and removes-before-run.
//!
//! Both points are tracked for Epic 6 #1237 content audit.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent::prelude::*;
use std::path::PathBuf;

fn out_path() -> PathBuf {
    std::env::temp_dir().join("pdfluent-bootstrap-form_filled.pdf")
}

/// Fill three AcroForm fields and save the result.
pub fn run_to(out: &std::path::Path) -> Result<()> {
    let mut doc = PdfDocument::open_with(
        "tests/fixtures/form.pdf",
        OpenOptions::new().with_license_key("tier:developer"),
    )?;

    {
        let mut form = doc.form_mut();
        form.set_text("first_name", "Jane")?
            .set_text("last_name", "Smith")?
            .set_checkbox("agree_terms", true)?;
    }

    doc.save(out)?;
    Ok(())
}

/// Website-snippet entry point kept for the `_compiles` test.
pub fn run() -> Result<()> {
    run_to(&out_path())
}

#[test]
fn fill_pdf_form_rust_runs() {
    // Unblocked by Epic 3 #1245 (form-mutation wiring).
    let path = out_path();
    let _ = std::fs::remove_file(&path);
    run_to(&path).expect("fill-form flow");

    // Verify the round-trip: reopen the saved file and read the values
    // back through the public `form_fields()` API.
    let reopened =
        PdfDocument::open_with(&path, OpenOptions::new().with_license_key("tier:developer"))
            .expect("reopen filled form");
    let by_name: std::collections::HashMap<String, String> = reopened
        .form_fields()
        .expect("form_fields")
        .into_iter()
        .map(|f| (f.name, f.value))
        .collect();
    assert_eq!(by_name.get("first_name").map(String::as_str), Some("Jane"));
    assert_eq!(by_name.get("last_name").map(String::as_str), Some("Smith"));
    assert_eq!(by_name.get("agree_terms").map(String::as_str), Some("Yes"));

    let _ = std::fs::remove_file(&path);
}

#[test]
fn fill_pdf_form_rust_compiles() {
    let _f: fn() -> Result<()> = run;
}
