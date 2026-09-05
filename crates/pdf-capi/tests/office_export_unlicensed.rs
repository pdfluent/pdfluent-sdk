// Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
//
// This software is proprietary. The PDFluent application is free to use,
// including for commercial purposes. Redistribution, or extraction or reuse
// of its components (including the embedded PDF engine), requires a licence.
// See https://pdfluent.com/license for terms.

//! Office export must refuse below Business tier.
//!
//! A separate test binary on purpose. The licence is process-wide state, and
//! `office_export.rs` sets a Business key to test the happy path — so a
//! refusal test living beside it would pass whether or not the check exists.
//! Verified: deleting the capability check in `office_export` leaves
//! `office_export.rs` fully green and fails only this file.

use std::ptr;

use pdf_capi::{
    pdf_document_free, pdf_document_open_from_bytes, pdf_document_to_docx, pdf_document_to_pptx,
    pdf_document_to_xlsx, PdfDocument, PdfStatus,
};

const SAMPLE_PDF: &[u8] = include_bytes!("../../../fixtures/sample.pdf");

fn open_sample() -> *mut PdfDocument {
    let mut doc: *mut PdfDocument = ptr::null_mut();
    let rc =
        unsafe { pdf_document_open_from_bytes(SAMPLE_PDF.as_ptr(), SAMPLE_PDF.len(), &mut doc) };
    assert_eq!(rc, PdfStatus::Ok);
    doc
}

#[test]
fn office_export_refuses_below_business_tier() {
    // A key in the environment would make this test assert the opposite of
    // what it is for. Saying so beats a green tick that means nothing.
    if pdfluent::require_capability(pdfluent::Capability::DocxExport).is_ok() {
        eprintln!(
            "SKIPPED (not a pass): the ambient licence already grants DocxExport \
             (PDFLUENT_LICENSE_KEY set?), so a refusal cannot be observed"
        );
        return;
    }

    let doc = open_sample();
    for (naam, f) in [
        (
            "docx",
            pdf_document_to_docx as unsafe extern "C" fn(_, _, _) -> PdfStatus,
        ),
        ("xlsx", pdf_document_to_xlsx),
        ("pptx", pdf_document_to_pptx),
    ] {
        let mut data: *mut u8 = ptr::null_mut();
        let mut len: usize = 0;
        let rc = unsafe { f(doc, &mut data, &mut len) };
        assert_eq!(
            rc,
            PdfStatus::ErrorCapabilityNotLicensed,
            "{naam}: expected a licence refusal, got {rc:?} — an unlicensed \
             caller would receive a package they have not paid for"
        );
        assert!(data.is_null(), "{naam}: refused calls must not allocate");
    }
    unsafe { pdf_document_free(doc) };
}
