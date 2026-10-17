//! Compat L1: word boxes use font-metric extents on recorded baselines (#643).
//!
//! The recordings pin boxes built from the baseline plus the font's
//! ascender above it and descender below: `Test page` in Helvetica 12 at
//! baseline 92 spans y 79.1-95.588, not the 80-92 advance slab the lane
//! used to report. A quarter-turned word spans the same metrics across its
//! baseline. Expected numbers come from the committed
//! `tests/compat/expected/` recordings for the same bytes.
//!
//! Fixtures are built in-memory via `lopdf` so the test stays self-contained.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::{dictionary, Document, Object, Stream};
use pdfluent::compat::{Page, WordOptions};
use pdfluent::prelude::*;

/// One page drawing `content` in Helvetica at the given media size.
fn one_page_pdf(content: &[u8]) -> Vec<u8> {
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
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.to_vec()));
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

/// The harness's relative float rule: within 1e-6 of the larger magnitude.
fn assert_close(actual: f64, expected: f64) {
    let allowed = 1e-6 * expected.abs().max(actual.abs());
    assert!(
        (actual - expected).abs() <= allowed,
        "{actual} != {expected} (allows {allowed})"
    );
}

#[test]
fn word_boxes_span_baseline_metrics() {
    let pdf = one_page_pdf(b"BT\n/F1 12 Tf\n100 700 Td\n(Test page) Tj\nET\n");
    let doc = PdfDocument::from_bytes(&pdf).expect("fixture opens");
    let words = Page::new(&doc, 0)
        .get_text_words(&WordOptions::default())
        .expect("words extract");
    assert_eq!(words.len(), 2);
    // Committed recording for the same bytes.
    let expected = [
        [
            100.0,
            79.0999984741211,
            123.33999633789062,
            95.58799743652344,
        ],
        [
            126.67599487304688,
            79.0999984741211,
            153.36398315429688,
            95.58799743652344,
        ],
    ];
    for (word, want) in words.iter().zip(expected) {
        let got = [word.rect.x0, word.rect.y0, word.rect.x1, word.rect.y1];
        for (a, b) in got.iter().zip(want) {
            assert_close(*a, b);
        }
    }
}

#[test]
fn turned_words_span_metrics_across_the_baseline() {
    // `Form` up the page: size from Tf, rotation-only text matrix.
    let pdf = one_page_pdf(b"BT\n/F1 7 Tf\n0 1 -1 0 200 700 Tm\n(Form) Tj\nET\n");
    let doc = PdfDocument::from_bytes(&pdf).expect("fixture opens");
    let words = Page::new(&doc, 0)
        .get_text_words(&WordOptions::default())
        .expect("words extract");
    assert!(!words.is_empty());
    // Helvetica's recorded vertical metrics, 1.075 above and 0.299 below,
    // read across the baseline at size 7: every glyph box is that wide.
    let width = (1.075f64 + 0.299f64) * 7.0;
    for word in &words {
        assert_close(word.rect.x1 - word.rect.x0, width);
    }
}
