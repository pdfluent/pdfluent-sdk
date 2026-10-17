// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! `Document.convert_to_pdf` through the facade (#646).
//!
//! A page span becomes the bytes of a new PDF, each page rotated as asked,
//! while the source handle is unchanged. Bad rotations, reversed spans and
//! out-of-range pages fail loudly.

use pdfluent::prelude::*;

const SAMPLE_PDF: &[u8] = include_bytes!("../../../fixtures/sample.pdf");
const MULTI_PDF: &[u8] = include_bytes!("../../../fixtures/multi-page.pdf");

fn page_count_of(bytes: &[u8]) -> usize {
    PdfDocument::from_bytes(bytes)
        .expect("converted bytes re-open")
        .page_count()
}

fn text_of(bytes: &[u8]) -> String {
    let engine =
        pdf_engine::PdfDocument::open(bytes.to_vec()).expect("engine opens converted bytes");
    let blocks = engine
        .extract_blocks(&pdf_engine::ExtractOptions::default())
        .expect("extract converted text");
    blocks
        .iter()
        .map(|block| block.text.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn convert_defaults_return_the_whole_document() {
    let doc = PdfDocument::from_bytes(SAMPLE_PDF).expect("open sample.pdf");
    let out = doc.convert_to_pdf(0, -1, 0).expect("convert defaults");
    assert_eq!(page_count_of(&out), 1);
    let text = text_of(&out);
    let words: Vec<&str> = text.split_whitespace().collect();
    assert_eq!(words, ["Test", "page"], "converted page lost its text");
}

#[test]
fn convert_span_selects_pages() {
    let doc = PdfDocument::from_bytes(MULTI_PDF).expect("open multi-page.pdf");
    let total = doc.page_count();
    assert!(total >= 3, "fixture needs 3+ pages, has {total}");

    let two = doc.convert_to_pdf(1, 2, 0).expect("convert 1..=2");
    assert_eq!(page_count_of(&two), 2);

    let one = doc.convert_to_pdf(0, 0, 0).expect("convert 0..=0");
    assert_eq!(page_count_of(&one), 1);

    let tail = doc.convert_to_pdf(-1, -1, 0).expect("convert last page");
    assert_eq!(page_count_of(&tail), 1);
}

#[test]
fn convert_rotate_sets_rotation_and_leaves_the_source() {
    let doc = PdfDocument::from_bytes(SAMPLE_PDF).expect("open sample.pdf");
    let before = doc.to_bytes().expect("serialise source");

    let out = doc.convert_to_pdf(0, -1, 90).expect("convert rotated");
    assert_eq!(page_count_of(&out), 1);
    assert!(
        contains(&out, b"/Rotate") && contains(&out, b"90"),
        "rotated output carries no /Rotate 90"
    );

    let after = doc.to_bytes().expect("serialise source again");
    assert_eq!(before, after, "convert_to_pdf changed the source handle");
}

#[test]
fn convert_rejects_a_rotation_off_the_quarter_turn() {
    let doc = PdfDocument::from_bytes(SAMPLE_PDF).expect("open sample.pdf");
    let err = doc
        .convert_to_pdf(0, -1, 45)
        .expect_err("rotate=45 must fail");
    assert!(
        err.to_string().contains("multiple of 90"),
        "wrong error: {err}"
    );
}

#[test]
fn convert_rejects_a_reversed_span() {
    let doc = PdfDocument::from_bytes(MULTI_PDF).expect("open multi-page.pdf");
    let err = doc
        .convert_to_pdf(1, 0, 0)
        .expect_err("to < from must fail");
    assert!(err.to_string().contains("before"), "wrong error: {err}");
}

#[test]
fn convert_rejects_out_of_range_pages() {
    let doc = PdfDocument::from_bytes(SAMPLE_PDF).expect("open sample.pdf");
    assert!(
        doc.convert_to_pdf(0, 99, 0).is_err(),
        "to_page past the end must fail"
    );
    assert!(
        doc.convert_to_pdf(-99, -1, 0).is_err(),
        "from_page before the start must fail"
    );
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}
