//! The dump on a real PDF: every block the engine returns is a row, and the
//! word boxes of a span cover exactly its glyph boxes (#598).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_engine::{ExtractOptions, PdfDocument};
use pdf_learned::{page_record, words_of, SCHEMA};

fn fixture() -> PdfDocument {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/structured.pdf");
    PdfDocument::open(std::fs::read(path).expect("fixture")).expect("open")
}

#[test]
fn every_block_of_every_page_is_a_row() {
    let doc = fixture();
    let blocks = doc
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    assert!(!blocks.is_empty());
    let mut rows = 0;
    for page in 0..doc.page_count() {
        let on_page: Vec<_> = blocks.iter().filter(|b| b.page == page).cloned().collect();
        let r = page_record(&on_page, &[], page, 612.0, 792.0, "id", "sha");
        assert_eq!(r["schema"], SCHEMA);
        let got = r["blocks"].as_array().expect("blocks array");
        assert_eq!(
            got.len(),
            on_page.len(),
            "page {page}: a block went missing"
        );
        rows += got.len();
    }
    assert_eq!(rows, blocks.len());
}

#[test]
fn word_boxes_cover_the_glyph_boxes_of_their_span() {
    let doc = fixture();
    let blocks = doc
        .extract_blocks(&ExtractOptions::default())
        .expect("blocks");
    let mut checked = 0;
    for span in blocks.iter().flat_map(|b| b.spans.iter()) {
        let chars: Vec<char> = span.text.chars().collect();
        if span.char_bounds.len() != chars.len()
            || chars.first().is_some_and(|c| c.is_whitespace())
            || chars.last().is_some_and(|c| c.is_whitespace())
        {
            continue;
        }
        let words = words_of(span);
        let cover = words
            .iter()
            .fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |a, w| {
                [
                    a[0].min(w.bbox[0]),
                    a[1].min(w.bbox[1]),
                    a[2].max(w.bbox[2]),
                    a[3].max(w.bbox[3]),
                ]
            });
        let glyphs =
            span.char_bounds
                .iter()
                .fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |a, g| {
                    [
                        a[0].min(g[0]),
                        a[1].min(g[1]),
                        a[2].max(g[2]),
                        a[3].max(g[3]),
                    ]
                });
        assert_eq!(cover, glyphs, "span {:?}", span.text);
        assert_eq!(
            words
                .iter()
                .map(|w| w.text.as_str())
                .collect::<Vec<_>>()
                .join(" "),
            span.text.split_whitespace().collect::<Vec<_>>().join(" ")
        );
        checked += 1;
    }
    assert!(checked > 5, "only {checked} spans checked");
}
