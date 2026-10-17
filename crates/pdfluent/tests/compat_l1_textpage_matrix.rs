//! Compat L1: a text page matrix narrows the extraction window (#643).
//!
//! The recordings pin the rule: a text page built under a 2x scale reads
//! only the quarter of the page the window keeps (the clip, or the full
//! page when the page has none, seen through the inverse matrix), while
//! `rect` still reports the untransformed page. An identity or singular
//! matrix leaves the clip alone.
//!
//! The fixture is built in-memory via `lopdf` so the test stays
//! self-contained: `Left` and `Right` on one line, far enough apart that a
//! 2x window keeps only the left word.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::{dictionary, Document, Object, Stream};
use pdfluent::compat::{Matrix, Page};
use pdfluent::prelude::*;

/// One page with `Left` at x 72 and `Right` at x 400, Helvetica 12.
fn two_column_pdf() -> Vec<u8> {
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
    let content =
        b"BT\n/F1 12 Tf\n72 700 Td\n(Left) Tj\nET\nBT\n/F1 12 Tf\n400 700 Td\n(Right) Tj\nET\n"
            .to_vec();
    let content_id = doc.add_object(Stream::new(dictionary! {}, content));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => resources_id,
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("in-memory fixture saves");
    bytes
}

fn scale(factor: f64) -> Matrix {
    Matrix {
        a: factor,
        b: 0.0,
        c: 0.0,
        d: factor,
        e: 0.0,
        f: 0.0,
    }
}

#[test]
fn matrix_narrows_the_window() {
    let doc = PdfDocument::from_bytes(&two_column_pdf()).expect("fixture opens");
    let page = Page::new(&doc, 0);
    let scaled = page
        .get_textpage(None, 0, Some(scale(2.0)))
        .expect("text page builds");
    // The gap filler between the runs overlaps the window, so it reads
    // along: the window keeps characters, not words.
    assert_eq!(
        scaled.extract_text(false).expect("text extracts"),
        "Left \n"
    );
    let plain = page
        .get_textpage(None, 0, Some(scale(1.0)))
        .expect("text page builds");
    let full = plain.extract_text(false).expect("text extracts");
    assert!(full.contains("Left"), "identity keeps all: {full:?}");
    assert!(full.contains("Right"), "identity keeps all: {full:?}");
    let flat = page
        .get_textpage(None, 0, Some(scale(0.0)))
        .expect("text page builds");
    let kept = flat.extract_text(false).expect("text extracts");
    assert!(kept.contains("Left"), "singular keeps all: {kept:?}");
    assert!(kept.contains("Right"), "singular keeps all: {kept:?}");
}

#[test]
fn clip_without_matrix_still_clips() {
    use pdfluent::compat::Rect;
    let doc = PdfDocument::from_bytes(&two_column_pdf()).expect("fixture opens");
    let page = Page::new(&doc, 0);
    let clip = Rect {
        x0: 0.0,
        y0: 0.0,
        x1: 1.0,
        y1: 1.0,
    };
    let empty = page
        .get_textpage(Some(clip), 0, None)
        .expect("text page builds");
    assert_eq!(empty.extract_text(false).expect("text extracts"), "");
}
