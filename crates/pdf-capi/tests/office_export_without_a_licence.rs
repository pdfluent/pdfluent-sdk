// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Office export works with no licence, and the environment cannot change it.
//!
//! This file used to assert the opposite: that the three exports refused below
//! Business tier. There are no tiers and no keys any more (#199, #226), so the
//! property worth guarding is the inverse one -- an unlicensed caller gets the
//! package, and a `PDFLUENT_LICENSE_KEY` in the environment is inert.
//!
//! A separate test binary on purpose, as the refusal test was: it must be the
//! only thing in its process that touches the environment, so no other test's
//! setup can decide the answer.

use std::ptr;

use pdf_capi::{
    pdf_bytes_free, pdf_document_free, pdf_document_open_from_bytes, pdf_document_to_docx,
    pdf_document_to_pptx, pdf_document_to_xlsx, PdfDocument, PdfStatus,
};

const SAMPLE_PDF: &[u8] = include_bytes!("../../../fixtures/sample.pdf");

type Export = unsafe extern "C" fn(*const PdfDocument, *mut *mut u8, *mut usize) -> PdfStatus;

fn open_sample() -> *mut PdfDocument {
    let mut doc: *mut PdfDocument = ptr::null_mut();
    let rc =
        unsafe { pdf_document_open_from_bytes(SAMPLE_PDF.as_ptr(), SAMPLE_PDF.len(), &mut doc) };
    assert_eq!(rc, PdfStatus::Ok);
    doc
}

/// Export all three formats and return their byte lengths.
fn export_all(doc: *mut PdfDocument) -> Vec<(&'static str, usize)> {
    let mut out = Vec::new();
    for (name, f) in [
        ("docx", pdf_document_to_docx as Export),
        ("xlsx", pdf_document_to_xlsx),
        ("pptx", pdf_document_to_pptx),
    ] {
        let mut data: *mut u8 = ptr::null_mut();
        let mut len: usize = 0;
        let rc = unsafe { f(doc, &mut data, &mut len) };
        assert_eq!(
            rc,
            PdfStatus::Ok,
            "{name}: an unlicensed caller was refused with {rc:?} -- \
             a licence check is back in the C ABI"
        );
        assert!(!data.is_null(), "{name}: no buffer on a successful export");
        assert!(len > 0, "{name}: empty package");
        unsafe { pdf_bytes_free(data, len) };
        out.push((name, len));
    }
    out
}

#[test]
fn office_export_needs_no_licence_and_ignores_the_environment() {
    // This test binary holds one test, so nothing else in the process reads
    // the environment while it is being changed.
    std::env::remove_var("PDFLUENT_LICENSE_KEY");
    let doc = open_sample();
    let without = export_all(doc);

    std::env::set_var("PDFLUENT_LICENSE_KEY", "tier:enterprise");
    let with_a_key = export_all(doc);
    std::env::remove_var("PDFLUENT_LICENSE_KEY");

    assert_eq!(
        without, with_a_key,
        "a key in the environment changed what the exports produced"
    );
    unsafe { pdf_document_free(doc) };
}
