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

/// Export all three formats and return their bytes.
///
/// Full bytes, not lengths or digests. A length is nearly blind here: the thing
/// that varied between two exports was a timestamp inside `docProps/core.xml`,
/// and one timestamp is as long as another, so the deflated package was usually
/// the same size while its contents differed. Wave 34410536849 caught it only
/// because one pair of seconds happened to compress to 4936 against 4935.
/// Bytes see what a length cannot.
fn export_all(doc: *mut PdfDocument) -> Vec<(&'static str, Vec<u8>)> {
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
        let bytes = unsafe { std::slice::from_raw_parts(data, len) }.to_vec();
        unsafe { pdf_bytes_free(data, len) };
        out.push((name, bytes));
    }
    out
}

/// Two export runs must agree byte for byte, naming only the format on failure.
///
/// An `assert_eq!` on the byte vectors would print kilobytes of numbers; the
/// lengths locate the break just as well, since a mismatch here is never a
/// length coincidence.
fn assert_same_export(left: &[(&str, Vec<u8>)], right: &[(&str, Vec<u8>)], why: &str) {
    assert_eq!(left.len(), right.len(), "export count changed: {why}");
    for ((left_name, left_bytes), (right_name, right_bytes)) in left.iter().zip(right.iter()) {
        assert_eq!(left_name, right_name);
        assert!(
            left_bytes == right_bytes,
            "{left_name}: {why} ({} bytes against {})",
            left_bytes.len(),
            right_bytes.len()
        );
    }
}

#[test]
fn office_export_needs_no_licence_and_ignores_the_environment() {
    // This test binary holds one test, so nothing else in the process reads
    // the environment while it is being changed.
    std::env::remove_var("PDFLUENT_LICENSE_KEY");
    std::env::remove_var("SOURCE_DATE_EPOCH");
    let doc = open_sample();
    let without = export_all(doc);

    std::env::set_var("PDFLUENT_LICENSE_KEY", "tier:enterprise");
    let with_a_key = export_all(doc);
    std::env::remove_var("PDFLUENT_LICENSE_KEY");

    assert_same_export(
        &without,
        &with_a_key,
        "a key in the environment changed what the exports produced",
    );

    // `SOURCE_DATE_EPOCH` used to pin the xlsx clock; the stamp now comes from
    // the document's `/CreationDate`, or a fixed epoch when there is none, so
    // the variable must be inert like the licence key.
    std::env::set_var("SOURCE_DATE_EPOCH", "1000000000");
    let with_epoch = export_all(doc);
    std::env::remove_var("SOURCE_DATE_EPOCH");
    assert_same_export(
        &without,
        &with_epoch,
        "SOURCE_DATE_EPOCH changed what the exports produced",
    );

    // And the determinism the comparisons above lean on has to hold on its own:
    // two exports deliberately taken apart in time, with no key either time.
    // `pdf_to_xlsx` used to stamp the wall clock into the workbook, so two
    // exports a moment apart differed by one byte after deflate. On this Mac
    // the two calls below land inside the same second and a length comparison
    // passes; on the wave's Linux runner they crossed a boundary and it failed,
    // reporting "a key in the environment changed what the exports produced"
    // about a clock (wave 34410536849, #477). Measured here before believing
    // it: with no key involved at all, twelve consecutive xlsx exports gave two
    // distinct lengths and eleven of eleven differed from the first byte for
    // byte, while docx and pptx were identical every time. A sleep past a
    // second boundary must now not move a byte.
    let early = export_all(doc);
    std::thread::sleep(std::time::Duration::from_millis(1100));
    let late = export_all(doc);
    assert_same_export(
        &early,
        &late,
        "two exports a second apart differ, so the clock is back in the export \
         and the comparisons above prove nothing",
    );

    unsafe { pdf_document_free(doc) };
}
