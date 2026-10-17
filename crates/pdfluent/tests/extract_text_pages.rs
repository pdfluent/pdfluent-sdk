//! Page boundaries survive extraction (#449).
//!
//! `extract_text()` and `text()` both hand back one run of text: the first
//! joins pages with a blank line, the second with a form feed, and neither
//! tells a caller which characters belong to which page. A pipeline that
//! chunks or cites by page ("see page 7") had to open the document a second
//! time with somebody else's library to find out -- the one thing an
//! extraction SDK exists to prevent. Measured in #390: our output carried 0
//! form feeds on a 14-page document where pdftotext, mutool, PyMuPDF and
//! pdfminer.six each carried 14.
//!
//! The fixture is built in-memory via `lopdf` so the test file stays
//! self-contained and the two pages carry text that tells them apart -- a
//! fixture whose pages read alike cannot catch a reader that hands back page
//! one twice.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::{dictionary, Document, Object, Stream};
use pdfluent::prelude::*;

const PAGE_ONE_TEXT: &str = "Alpha on page one";
const PAGE_TWO_TEXT: &str = "Beta on page two";

/// A two-page PDF whose pages carry different, known text.
fn build_two_page_pdf() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();

    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
    });
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! { "F1" => font_id },
    });

    let mut page_ids = Vec::new();
    for text in [PAGE_ONE_TEXT, PAGE_TWO_TEXT] {
        let content = format!("BT\n/F1 12 Tf\n72 720 Td\n({text}) Tj\nET\n");
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => resources_id,
        });
        page_ids.push(page_id);
    }

    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => page_ids.iter().map(|id| Object::Reference(*id)).collect::<Vec<_>>(),
            "Count" => page_ids.len() as i64,
        }),
    );

    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);

    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("serialise fixture");
    buf
}

fn open_fixture(bytes: &[u8]) -> PdfDocument {
    PdfDocument::from_bytes_with(bytes, OpenOptions::new()).expect("parse fixture")
}

#[test]
fn every_page_comes_back_separately() {
    let doc = open_fixture(&build_two_page_pdf());
    let pages = doc.extract_text_pages().expect("extract pages");

    assert_eq!(pages.len(), 2, "a two-page document has two pages of text");
    assert!(
        pages[0].contains(PAGE_ONE_TEXT),
        "page 1 carries its own text, got: {:?}",
        pages[0]
    );
    assert!(
        pages[1].contains(PAGE_TWO_TEXT),
        "page 2 carries its own text, got: {:?}",
        pages[1]
    );
    assert!(
        !pages[0].contains(PAGE_TWO_TEXT),
        "page 1 does not carry page 2's text"
    );
}

#[test]
fn the_flat_form_carries_one_separator_per_page() {
    let doc = open_fixture(&build_two_page_pdf());
    let pages = doc.extract_text_pages().expect("extract pages");
    let flat = pdfluent::join_page_texts(pages.iter().map(String::as_str));

    // After every page, not between them: n pages, n form feeds. That is what
    // pdftotext, mutool, PyMuPDF and pdfminer.six each emit, and matching them
    // is what makes our text substitutable for theirs.
    assert_eq!(
        flat.matches('\u{000C}').count(),
        2,
        "two pages carry two form feeds, got: {flat:?}"
    );
    let mut split: Vec<&str> = flat.split('\u{000C}').collect();
    assert_eq!(split.pop().map(str::trim), Some(""));
    assert_eq!(split.len(), 2);
    assert!(split[0].contains(PAGE_ONE_TEXT));
    assert!(split[1].contains(PAGE_TWO_TEXT));
}

#[test]
fn the_joined_form_still_joins() {
    // `extract_text()` keeps its documented blank-line join: the page-aware
    // form is additive, not a silent change to what callers already parse.
    let doc = open_fixture(&build_two_page_pdf());
    let joined = doc.extract_text().expect("extract text");
    assert!(
        !joined.contains('\u{000C}'),
        "extract_text keeps its documented join"
    );
    assert!(joined.contains(PAGE_ONE_TEXT) && joined.contains(PAGE_TWO_TEXT));
}
