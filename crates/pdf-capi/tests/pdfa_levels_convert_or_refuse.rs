//! What the C ABI does with a PDF/A level it does not convert.
//!
//! `pdf_document_convert_pdfa` accepted eleven levels and mapped the A, U and
//! part-4 values onto the B level of their part. A caller asking for 2U got
//! `PDF_STATUS_OK` and a file whose `pdfaid:conformance` said `B`, with nothing
//! in the header, the return value or the error message saying so — and Node,
//! Python, Java and .NET all convert through this function, so the same held
//! for four bindings. An accepted level that is quietly something else is worse
//! than a refusal: the caller archives it. (#447)

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::ffi::CStr;
use std::ptr;

use pdf_capi::{
    pdf_clear_error, pdf_compliance_report_free, pdf_document_convert_pdfa, pdf_document_free,
    pdf_document_open_from_bytes, pdf_document_validate_pdfa, PdfALevel, PdfComplianceReport,
    PdfDocument, PdfStatus,
};

const SAMPLE_PDF: &[u8] = include_bytes!("../../../fixtures/sample.pdf");

/// The three levels the conversion delivers, and the eight it does not.
const DELIVERED: &[PdfALevel] = &[PdfALevel::A1b, PdfALevel::A2b, PdfALevel::A3b];
const REFUSED: &[PdfALevel] = &[
    PdfALevel::A1a,
    PdfALevel::A2u,
    PdfALevel::A2a,
    PdfALevel::A3u,
    PdfALevel::A3a,
    PdfALevel::A4,
    PdfALevel::A4f,
    PdfALevel::A4e,
];

fn open_sample() -> *mut PdfDocument {
    let mut doc: *mut PdfDocument = ptr::null_mut();
    let rc =
        unsafe { pdf_document_open_from_bytes(SAMPLE_PDF.as_ptr(), SAMPLE_PDF.len(), &mut doc) };
    assert_eq!(rc, PdfStatus::Ok, "open_from_bytes(sample.pdf) failed");
    doc
}

fn last_error() -> String {
    let msg = pdf_capi::pdf_get_last_error();
    assert!(!msg.is_null(), "a refusal left no message behind");
    unsafe { CStr::from_ptr(msg) }
        .to_string_lossy()
        .into_owned()
}

#[test]
fn a_level_the_conversion_does_not_deliver_is_refused() {
    let doc = open_sample();
    for &level in REFUSED {
        pdf_clear_error();
        let mut out: *mut PdfDocument = ptr::null_mut();
        let rc = unsafe { pdf_document_convert_pdfa(doc, level, &mut out) };
        assert_eq!(
            rc,
            PdfStatus::ErrorInvalidArgument,
            "{level:?} was accepted; it used to come back as its part's B level"
        );
        assert!(
            out.is_null(),
            "{level:?} was refused and still wrote a document"
        );

        // And the message says what the caller can ask for instead. A refusal
        // that does not is a caller reading the source to find out.
        let msg = last_error();
        for name in PdfALevel::DELIVERED_FOR_CONVERSION {
            assert!(
                msg.contains(name),
                "the refusal for {level:?} does not name {name}: {msg:?}"
            );
        }
    }
    unsafe { pdf_document_free(doc) };
}

#[test]
fn the_three_delivered_levels_still_convert() {
    // The other half: a fix that refused everything would pass the test above.
    let doc = open_sample();
    for &level in DELIVERED {
        let mut out: *mut PdfDocument = ptr::null_mut();
        let rc = unsafe { pdf_document_convert_pdfa(doc, level, &mut out) };
        assert_eq!(
            rc,
            PdfStatus::Ok,
            "{level:?} is delivered and did not convert"
        );
        assert!(!out.is_null(), "{level:?} returned OK without a document");
        unsafe { pdf_document_free(out) };
    }
    unsafe { pdf_document_free(doc) };
}

#[test]
fn validation_still_covers_every_level() {
    // Conversion and validation are different questions and the refusal above
    // answers only the first. veraPDF-shaped callers validate at 2U against
    // files they did not produce, and that path must stay open.
    let doc = open_sample();
    for &level in REFUSED.iter().chain(DELIVERED) {
        let mut report: *mut PdfComplianceReport = ptr::null_mut();
        let rc = unsafe { pdf_document_validate_pdfa(doc, level, &mut report) };
        assert_eq!(rc, PdfStatus::Ok, "validation at {level:?} did not run");
        assert!(!report.is_null());
        unsafe { pdf_compliance_report_free(report) };
    }
    unsafe { pdf_document_free(doc) };
}
