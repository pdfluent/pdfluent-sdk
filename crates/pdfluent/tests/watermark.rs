// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Watermarking through the facade.
//!
//! Until 23-08-2026 `add_decoration` returned Error::MissingDependency saying
//! the watermark runtime would land with a later epic. It had landed long
//! before: the C ABI has been calling `pdf_manip::watermark` the whole time.
//! Only the facade was still refusing, and the note explaining why had gone
//! stale without anything noticing -- there was no test to notice with.

use pdfluent::watermark::{Position, WatermarkOptions};
use pdfluent::PdfDocument;

const SAMPLE_PDF: &[u8] = include_bytes!("../../../tests/corpus-mini/simple.pdf");

/// The text corpus-mini/simple.pdf actually draws.
const NEEDLE: &str = "Test page";

fn licensed() {}

fn text_of(doc: &PdfDocument) -> String {
    doc.extract_text().expect("extract_text")
}

#[test]
fn a_watermark_is_added_and_the_page_content_is_kept() {
    licensed();
    let mut doc = PdfDocument::from_bytes(SAMPLE_PDF).expect("open sample.pdf");
    doc.add_watermark("CONCEPT", WatermarkOptions::centered())
        .expect("add_watermark");

    let text = text_of(&doc);
    assert!(
        text.contains("CONCEPT"),
        "the watermark text is not in the document"
    );
    // Both halves. A watermark that replaces the page is not a watermark, and
    // a page that is unchanged did not get one.
    assert!(
        text.contains(NEEDLE),
        "watermarking removed the page content"
    );
}

#[test]
fn an_exact_position_is_carried_through_rather_than_centred() {
    licensed();

    let mut centred = PdfDocument::from_bytes(SAMPLE_PDF).expect("open");
    centred
        .add_watermark("CONCEPT", WatermarkOptions::centered())
        .expect("centred watermark");

    let mut placed = PdfDocument::from_bytes(SAMPLE_PDF).expect("open");
    placed
        .add_watermark(
            "CONCEPT",
            WatermarkOptions::centered().at(Position::Exact(10.0, 10.0)),
        )
        .expect("placed watermark");

    // The two documents must differ. Mapping every option to a default would
    // produce a watermark that works and ignores what the caller asked for --
    // the failure mode that a "did it return Ok" test cannot see.
    assert_ne!(
        centred.to_bytes().expect("serialise centred"),
        placed.to_bytes().expect("serialise placed"),
        "position had no effect on the output"
    );
}

#[test]
fn watermarking_twice_leaves_both_marks() {
    licensed();
    let mut doc = PdfDocument::from_bytes(SAMPLE_PDF).expect("open sample.pdf");
    doc.add_watermark("EERSTE", WatermarkOptions::centered())
        .expect("first");
    doc.add_watermark("TWEEDE", WatermarkOptions::centered())
        .expect("second");

    let text = text_of(&doc);
    assert!(
        text.contains("EERSTE"),
        "the first watermark was overwritten"
    );
    assert!(text.contains("TWEEDE"), "the second watermark is missing");
}
