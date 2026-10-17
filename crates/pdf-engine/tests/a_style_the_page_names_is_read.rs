//! A weight or slant the page names is read, when the font stands on a
//! standard face (#623).
//!
//! NYSE_SSL_2003 page 59 (DocLayNet validation) sets its text in the Bliss
//! family, embedded as CFF: `Bliss-LightItalic` for the body and
//! `Bliss-BoldItalic` for a head. The renderer draws them on a standard face,
//! and the text layer used to see only that face's name, so nothing on the
//! page read as italic or bold. The font dictionary's /BaseFont says what the
//! page asked for.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_engine::PdfDocument;

#[test]
fn italic_and_bold_named_by_the_page_are_read() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/doclaynet/nyse_ssl_2003_p59.pdf"
    ))
    .expect("fixture");
    let doc = PdfDocument::open(bytes).expect("open");
    let spans: Vec<_> = doc
        .extract_text_blocks(0)
        .expect("blocks")
        .into_iter()
        .flat_map(|block| block.spans)
        .collect();
    let body = spans
        .iter()
        .find(|s| s.text.contains("audit committee meets"))
        .expect("the body line");
    assert!(body.is_italic, "Bliss-LightItalic is italic: {body:?}");
    let head = spans
        .iter()
        .find(|s| s.text.to_lowercase().contains("corporate governance"))
        .expect("the head");
    assert!(head.is_bold && head.is_italic, "Bliss-BoldItalic: {head:?}");
}

/// An embedded Type1 whose program names no face takes the page's /BaseFont
/// (#574, parity audit). Every span of this page had `font_name: None`, and so
/// did every block of five of the rig-like proxy's 36 documents: the heading
/// and list models' `font_is_the_body_face` could say nothing there.
#[test]
fn a_face_the_program_does_not_name_is_the_page_s() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/doclaynet/nyse_ssl_2003_p59.pdf"
    ))
    .expect("fixture");
    let doc = PdfDocument::open(bytes).expect("open");
    let spans: Vec<_> = doc
        .extract_text_blocks(0)
        .expect("blocks")
        .into_iter()
        .flat_map(|block| block.spans)
        .filter(|s| !s.text.trim().is_empty())
        .collect();
    assert!(!spans.is_empty());
    let unnamed: Vec<_> = spans.iter().filter(|s| s.font_name.is_none()).collect();
    assert!(unnamed.is_empty(), "spans with no face: {unnamed:?}");
    let body = spans
        .iter()
        .find(|s| s.text.contains("audit committee meets"))
        .expect("the body line");
    assert_eq!(
        body.font_name.as_deref(),
        Some("Bliss-LightItalic"),
        "the subset prefix is stripped and the name is the page's"
    );
}

/// And the name does not move a span boundary: spans merge on the face the
/// program names, as they did (#574). Merged on the page's name instead, the
/// math faces of an arXiv page (`CMMI12` beside `CMR12`) split every formula
/// from its sentence, and the word space at the split was lost:
/// `whereµmeasures a deviation from a perfe<sub>c</sub>t nesting`.
#[test]
fn a_page_s_name_does_not_split_a_sentence_from_its_formula() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/doclaynet/arxiv_1001_2528_p1.pdf"
    ))
    .expect("fixture");
    let doc = PdfDocument::open(bytes).expect("open");
    let md = doc
        .extract_markdown(&pdf_engine::ExtractOptions::default())
        .expect("markdown");
    assert!(
        md.contains("where µ measures a deviation from a perfect nesting"),
        "the sentence around the formula lost its spaces: {md}"
    );
}
