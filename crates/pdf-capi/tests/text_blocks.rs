//! C ABI text-block extraction contract test.
//!
//! Covers the full `pdf_page_extract_text_blocks` + `pdf_text_blocks_free`
//! surface from the Rust callable side. The companion C smoke
//! (`tests/test_text_blocks.c`) exercises the same scenarios through the
//! C type system after `cargo build -p pdf-capi --release`.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::ffi::CStr;
use std::ptr;

use pdf_capi::{
    pdf_document_free, pdf_document_open_from_bytes, pdf_page_extract_text_blocks,
    pdf_text_blocks_free, PdfDocument, PdfStatus, PdfTextBlock,
};

const SAMPLE_PDF: &[u8] = include_bytes!("../../../fixtures/sample.pdf");

/// Open the shared fixture or fail the test with a descriptive panic.
fn open_sample() -> *mut PdfDocument {
    let mut doc: *mut PdfDocument = ptr::null_mut();
    let rc =
        unsafe { pdf_document_open_from_bytes(SAMPLE_PDF.as_ptr(), SAMPLE_PDF.len(), &mut doc) };
    assert_eq!(rc, PdfStatus::Ok, "open_from_bytes(sample.pdf) failed");
    assert!(!doc.is_null());
    doc
}

#[test]
fn null_doc_returns_invalid_argument() {
    let mut blocks: *mut PdfTextBlock = ptr::null_mut();
    let mut count: usize = 999;
    let rc = unsafe { pdf_page_extract_text_blocks(ptr::null(), 0, &mut blocks, &mut count) };
    assert_eq!(rc, PdfStatus::ErrorInvalidArgument);
    // Out args must always be reset on failure.
    assert!(blocks.is_null());
    assert_eq!(count, 0);
}

#[test]
fn null_out_blocks_returns_invalid_argument() {
    let doc = open_sample();
    let mut count: usize = 999;
    let rc = unsafe { pdf_page_extract_text_blocks(doc, 0, ptr::null_mut(), &mut count) };
    assert_eq!(rc, PdfStatus::ErrorInvalidArgument);
    assert_eq!(count, 0);
    unsafe { pdf_document_free(doc) };
}

#[test]
fn null_out_count_returns_invalid_argument() {
    let doc = open_sample();
    let mut blocks: *mut PdfTextBlock = ptr::null_mut();
    let rc = unsafe { pdf_page_extract_text_blocks(doc, 0, &mut blocks, ptr::null_mut()) };
    assert_eq!(rc, PdfStatus::ErrorInvalidArgument);
    assert!(blocks.is_null());
    unsafe { pdf_document_free(doc) };
}

#[test]
fn negative_page_index_returns_invalid_argument() {
    let doc = open_sample();
    let mut blocks: *mut PdfTextBlock = ptr::null_mut();
    let mut count: usize = 999;
    let rc = unsafe { pdf_page_extract_text_blocks(doc, -1, &mut blocks, &mut count) };
    assert_eq!(rc, PdfStatus::ErrorInvalidArgument);
    assert!(blocks.is_null());
    assert_eq!(count, 0);
    unsafe { pdf_document_free(doc) };
}

#[test]
fn out_of_range_page_index_returns_page_range() {
    let doc = open_sample();
    let mut blocks: *mut PdfTextBlock = ptr::null_mut();
    let mut count: usize = 999;
    let rc = unsafe { pdf_page_extract_text_blocks(doc, 9_999, &mut blocks, &mut count) };
    assert_eq!(rc, PdfStatus::ErrorPageRange);
    assert!(blocks.is_null());
    assert_eq!(count, 0);
    unsafe { pdf_document_free(doc) };
}

#[test]
fn valid_page_returns_blocks_with_documented_shape() {
    let doc = open_sample();
    let mut blocks: *mut PdfTextBlock = ptr::null_mut();
    let mut count: usize = 0;
    let rc = unsafe { pdf_page_extract_text_blocks(doc, 0, &mut blocks, &mut count) };
    assert_eq!(rc, PdfStatus::Ok);
    if count == 0 {
        // Sample fixture has at least some text — fail loudly if not.
        panic!("sample.pdf produced zero text blocks; fixture may be wrong");
    }
    assert!(!blocks.is_null());

    // Walk the array.
    let slice = unsafe { std::slice::from_raw_parts(blocks, count) };
    for (i, b) in slice.iter().enumerate() {
        assert!(
            b.width >= 0.0,
            "block {i} width must be >= 0, got {}",
            b.width
        );
        assert!(
            b.height >= 0.0,
            "block {i} height must be >= 0, got {}",
            b.height
        );
        assert!(b.x.is_finite(), "block {i} x must be finite");
        assert!(b.y.is_finite(), "block {i} y must be finite");
        assert!(!b.text.is_null(), "block {i} text must not be null");
        let text = unsafe { CStr::from_ptr(b.text) }
            .to_str()
            .expect("text must be UTF-8");
        let _ = text; // shape-only check; we don't pin content
    }

    unsafe { pdf_text_blocks_free(blocks, count) };
    unsafe { pdf_document_free(doc) };
}

#[test]
fn double_extract_then_free_is_safe() {
    let doc = open_sample();
    for _ in 0..3 {
        let mut blocks: *mut PdfTextBlock = ptr::null_mut();
        let mut count: usize = 0;
        let rc = unsafe { pdf_page_extract_text_blocks(doc, 0, &mut blocks, &mut count) };
        assert_eq!(rc, PdfStatus::Ok);
        unsafe { pdf_text_blocks_free(blocks, count) };
    }
    unsafe { pdf_document_free(doc) };
}

#[test]
fn free_null_is_noop() {
    // Must not crash. Pattern used by C consumers that defensively free
    // even when extraction failed.
    unsafe { pdf_text_blocks_free(ptr::null_mut(), 0) };
    unsafe { pdf_text_blocks_free(ptr::null_mut(), 42) };
}

#[test]
fn free_unregistered_pointer_is_safe_noop() {
    // The implementation tolerates pointers it doesn't own (e.g. caller
    // bug). This is safer than aborting.
    let mut fake = PdfTextBlock {
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
        text: ptr::null(),
    };
    unsafe { pdf_text_blocks_free(&mut fake as *mut _, 1) };
}
