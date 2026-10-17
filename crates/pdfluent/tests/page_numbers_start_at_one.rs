//! Asking for page 0 is a caller mistake, and it is told so (#414).
//!
//! Copyright (c) 2026 Innovation Trigger B.V.
//!
//! PDFluent is available under two licences, at your option: the GNU AGPLv3, or
//! the PDFluent Commercial Licence. See the LICENSE file in this repository --
//! that file travels with the copy you received, which a URL does not.
//!
//! WHAT IS BEING GUARDED
//!
//! `render_page` and its neighbours are 1-based. Page `0` is therefore the
//! first thing a caller arriving from a 0-based API tries, and until now what
//! came back was:
//!
//! ```text
//! Internal error (please report): page index 0 out of range (document has 1 pages)
//! ```
//!
//! Two failures in one sentence. It is `Error::Internal`, whose text asks the
//! reader to file a bug report -- so a mistake the caller can fix in one
//! keystroke sends them to our issue tracker. And "out of range" describes 0
//! as merely outside some bounds, which invites the reading that the range
//! starts lower still, when the fact that would fix the call is that pages
//! are numbered from 1.
//!
//! So two things are asserted, and they are different things: the VARIANT,
//! because that is what a program branches on, and the CONVENTION IN THE
//! MESSAGE, because that is what a person reads. A fix that corrected only
//! the wording would leave every `match` arm still treating a typo as an
//! engine fault; one that corrected only the variant would leave the reader
//! no wiser about why 0 failed.

use pdfluent::{Error, ImageFormat, PdfDocument};

fn fixture() -> PdfDocument {
    PdfDocument::open("tests/fixtures/sample.pdf").expect("the sample fixture opens")
}

/// The four entry points that take a page number, so the fix cannot be
/// applied to one and forgotten in the others. `render_page` is the one the
/// report came in about; the rest had the identical construction.
#[test]
fn page_zero_is_a_caller_error_everywhere_a_page_number_is_taken() {
    let doc = fixture();
    let total = doc.page_count();

    let mut checked = 0;
    for error in [
        doc.page(0).err(),
        doc.render_page(0, 72, ImageFormat::Png).err(),
        doc.page(total + 1).err(),
        doc.render_page(total + 1, 72, ImageFormat::Png).err(),
    ]
    .into_iter()
    .flatten()
    {
        checked += 1;
        assert!(
            matches!(error, Error::PageOutOfRange { .. }),
            "a page number outside the document must be a caller error, not an \
             engine fault; got {error:?}"
        );
        assert!(
            !matches!(error, Error::Internal { .. }),
            "Error::Internal asks the reader to file a bug report; passing the \
             wrong page number is not a bug in this crate"
        );
    }
    assert_eq!(
        checked, 4,
        "every entry point that takes a page number must reject an impossible one"
    );
}

/// The message has to carry the convention, because the convention is the
/// whole of what the caller got wrong.
#[test]
fn the_message_says_pages_are_numbered_from_one() {
    let doc = fixture();
    let error = doc
        .render_page(0, 72, ImageFormat::Png)
        .expect_err("page 0 never exists");
    let text = error.to_string();

    assert!(
        text.contains("numbered from 1"),
        "the message must name the convention that would fix the call; got: {text}"
    );
    assert!(
        !text.contains("please report"),
        "the caller must not be sent to the issue tracker for their own typo; got: {text}"
    );
    assert_eq!(error.code(), "E-PAGE-OUT-OF-RANGE");
}

/// A document with one page says "1 page", not "1 pages". Small, and the
/// reason it is here is that the old message got it wrong in exactly the
/// place a first-time reader is looking.
#[test]
fn the_count_is_not_written_as_one_pages() {
    let doc = fixture();
    let text = doc
        .render_page(0, 72, ImageFormat::Png)
        .expect_err("page 0 never exists")
        .to_string();

    if doc.page_count() == 1 {
        assert!(
            text.contains("1 page.") || text.contains("1 page\n") || text.contains("1 page "),
            "a one-page document should read '1 page'; got: {text}"
        );
        assert!(
            !text.contains("1 pages"),
            "'1 pages' is the wording the report singled out; got: {text}"
        );
    }
}

/// Page 1 of a document that has one page is not an error. The bounds check
/// is what produces the error above, so a test that only proved rejection
/// would pass just as well on a function that rejected everything.
#[test]
fn the_first_page_is_page_one() {
    let doc = fixture();
    assert!(
        doc.page(1).is_ok(),
        "page 1 must be the first page, or the convention in the message is a lie"
    );
    assert!(
        doc.render_page(1, 72, ImageFormat::Png).is_ok(),
        "rendering page 1 must work"
    );
}
