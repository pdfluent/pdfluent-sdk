//! A cross-reference table we cannot read is not a document we cannot read
//! (#433, class R-F).
//!
//! Every document opens twice here: `pdf-syntax` rebuilds a broken table by
//! scanning for objects, and the lopdf side returned the parse error, so the
//! open failed on the half that had given up. Eight of the twenty documents we
//! could not parse on the gate set failed exactly this way -- among them
//! `data-000001.pdf`, which mutool, pdftotext, PyMuPDF, pypdf, pdfminer.six and
//! Docling all read (#450, #390). The document was not damaged beyond reading;
//! only one of our two readers had said so.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent::diagnostics::Diagnostic;
use pdfluent::prelude::*;

const SAMPLE: &str = "tests/fixtures/sample.pdf";
const SAMPLE_TEXT: &str = "PDFluent test fixture.";

/// The sample document with its `startxref` offset pointed into open water.
///
/// The digits are overwritten in place, so nothing else about the file moves:
/// the objects, the cross-reference stream and the trailer all stay exactly
/// where they were, and only the one number that says where to start is wrong.
fn with_a_broken_startxref(bytes: &[u8]) -> Vec<u8> {
    let mut out = bytes.to_vec();
    let keyword = out
        .windows(b"startxref".len())
        .rposition(|window| window == b"startxref")
        .expect("the sample carries a startxref");

    let digits_start = keyword + b"startxref\n".len();
    let digits_end = digits_start
        + out[digits_start..]
            .iter()
            .position(|byte| !byte.is_ascii_digit())
            .expect("digits, then a line end");

    let mut wrong = b"9999999".to_vec();
    wrong.truncate(digits_end - digits_start);
    wrong.resize(digits_end - digits_start, b'9');
    out.splice(digits_start..digits_end, wrong);

    out
}

#[test]
fn a_startxref_pointing_nowhere_still_yields_the_text() {
    let bytes = std::fs::read(SAMPLE).expect("the sample fixture");
    let broken = with_a_broken_startxref(&bytes);
    assert_ne!(broken, bytes, "the fixture must actually be damaged");

    let doc =
        PdfDocument::from_bytes(&broken).expect("a document whose objects are all present opens");

    assert_eq!(doc.page_count(), 1);
    let text = doc.extract_text().expect("text comes out");
    assert!(
        text.contains(SAMPLE_TEXT),
        "the page text must survive the repair, got {text:?}"
    );
}

#[test]
fn the_repair_is_reported_rather_than_hidden() {
    let bytes = std::fs::read(SAMPLE).expect("the sample fixture");
    let broken = with_a_broken_startxref(&bytes);

    let doc = PdfDocument::from_bytes(&broken).expect("the damaged document opens");
    let codes: Vec<&str> = doc.diagnostics().iter().map(|d| d.code).collect();

    assert!(
        codes.contains(&Diagnostic::CODE_XREF_REBUILT),
        "a rebuilt table is a fact about the document the caller may want to \
         act on; it must not be repaired silently. Diagnostics: {codes:?}"
    );
}

/// The intact document must not take the repair path.
///
/// A recovery that fires on healthy files would hide a real regression: every
/// document would open, and the axis that measures whether we read the table
/// would be measuring the scanner instead.
#[test]
fn an_intact_document_is_not_rebuilt() {
    let bytes = std::fs::read(SAMPLE).expect("the sample fixture");

    let doc = PdfDocument::from_bytes(&bytes).expect("the sample opens");
    let codes: Vec<&str> = doc.diagnostics().iter().map(|d| d.code).collect();

    assert!(
        !codes.contains(&Diagnostic::CODE_XREF_REBUILT),
        "nothing is wrong with this document's table. Diagnostics: {codes:?}"
    );
}

/// Bytes that are not a PDF at all still fail, and say so.
#[test]
fn a_file_with_no_objects_in_it_is_still_refused() {
    let not_a_pdf = b"%PDF-1.7\nthis file carries no indirect object at all\n%%EOF";

    PdfDocument::from_bytes(not_a_pdf)
        .expect_err("scanning for objects must not turn rubbish into a document");
}
