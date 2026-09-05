// Copyright (c) 2026 Innovation Trigger B.V. All rights reserved.
//
// This software is proprietary. The PDFluent application is free to use,
// including for commercial purposes. Redistribution, or extraction or reuse
// of its components (including the embedded PDF engine), requires a licence.
// See https://pdfluent.com/license for terms.

//! C ABI Office-export contract test.
//!
//! Until 23-08-2026 none of the five bindings could convert a PDF to Word,
//! Excel or PowerPoint, while the feature page sold it. The Rust crates
//! existed and stopped at the language boundary. These three exports are what
//! .NET, Java and Node reach Office through, so a test that only checks the
//! status code would miss the thing that matters: whether the bytes that come
//! back are a package an Office application opens.

use std::ptr;

use pdf_capi::{
    pdf_bytes_free, pdf_document_free, pdf_document_open_from_bytes, pdf_document_to_docx,
    pdf_document_to_pptx, pdf_document_to_xlsx, PdfDocument, PdfStatus,
};

const SAMPLE_PDF: &[u8] = include_bytes!("../../../fixtures/sample.pdf");

fn open_sample() -> *mut PdfDocument {
    let mut doc: *mut PdfDocument = ptr::null_mut();
    let rc =
        unsafe { pdf_document_open_from_bytes(SAMPLE_PDF.as_ptr(), SAMPLE_PDF.len(), &mut doc) };
    assert_eq!(rc, PdfStatus::Ok, "open_from_bytes(sample.pdf) failed");
    doc
}

/// An OOXML package is a ZIP with a known entry. Checking both is the
/// difference between "bytes came back" and "Word will open this".
fn assert_ooxml(data: *mut u8, len: usize, entry: &str, what: &str) {
    assert!(!data.is_null(), "{what}: null buffer");
    assert!(len > 0, "{what}: zero length");
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    assert_eq!(&bytes[..2], b"PK", "{what}: not a ZIP");
    let readable = String::from_utf8_lossy(bytes);
    assert!(
        readable.contains(entry),
        "{what}: {len} bytes but no {entry} inside"
    );
}

fn licensed() {
    // Office export is Business and up. Without this the exports correctly
    // refuse, which is itself covered below.
    let _ = pdfluent::set_license_key("tier:business");
}

#[test]
fn docx_export_returns_a_package_word_opens() {
    licensed();
    let doc = open_sample();
    let mut data: *mut u8 = ptr::null_mut();
    let mut len: usize = 0;
    let rc = unsafe { pdf_document_to_docx(doc, &mut data, &mut len) };
    assert_eq!(rc, PdfStatus::Ok, "to_docx returned {rc:?}");
    assert_ooxml(data, len, "word/document.xml", "docx");
    unsafe {
        pdf_bytes_free(data, len);
        pdf_document_free(doc);
    }
}

#[test]
fn xlsx_export_returns_a_package_excel_opens() {
    licensed();
    let doc = open_sample();
    let mut data: *mut u8 = ptr::null_mut();
    let mut len: usize = 0;
    let rc = unsafe { pdf_document_to_xlsx(doc, &mut data, &mut len) };
    assert_eq!(rc, PdfStatus::Ok, "to_xlsx returned {rc:?}");
    assert_ooxml(data, len, "xl/workbook.xml", "xlsx");
    unsafe {
        pdf_bytes_free(data, len);
        pdf_document_free(doc);
    }
}

#[test]
fn pptx_export_returns_a_package_powerpoint_opens() {
    licensed();
    let doc = open_sample();
    let mut data: *mut u8 = ptr::null_mut();
    let mut len: usize = 0;
    let rc = unsafe { pdf_document_to_pptx(doc, &mut data, &mut len) };
    assert_eq!(rc, PdfStatus::Ok, "to_pptx returned {rc:?}");
    assert_ooxml(data, len, "ppt/presentation.xml", "pptx");
    unsafe {
        pdf_bytes_free(data, len);
        pdf_document_free(doc);
    }
}

/// Every out-argument must be untouched when the call refuses, or a caller
/// that checks the pointer instead of the status frees a wild address.
#[test]
fn a_null_document_is_refused_without_touching_the_out_arguments() {
    let mut data: *mut u8 = ptr::null_mut();
    let mut len: usize = 999;
    let rc = unsafe { pdf_document_to_docx(ptr::null(), &mut data, &mut len) };
    assert_eq!(rc, PdfStatus::ErrorInvalidArgument);
    assert!(data.is_null());
    assert_eq!(len, 999, "len must not be written on a rejected call");
}

#[test]
fn a_null_out_pointer_is_refused() {
    let doc = open_sample();
    let mut len: usize = 0;
    let rc = unsafe { pdf_document_to_docx(doc, ptr::null_mut(), &mut len) };
    assert_eq!(rc, PdfStatus::ErrorInvalidArgument);
    unsafe { pdf_document_free(doc) };
}
