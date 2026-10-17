//! A heading made heavy or set wide still reads as its words (#623).
//!
//! Two ways a display line is drawn that the text layer read wrongly, both
//! costing heading matches on the DocLayNet validation pages:
//!
//! - Render mode 2 fills each glyph and then strokes it -- a heavier heading
//!   without a bold face. The interpreter hands the device both, and the
//!   extraction read `1.0 BACKGROUND` as `BBAACCKKGGRROOUUNNDD`.
//! - Letter-spaced capitals with one pair kerned in the TJ, or with their word
//!   spaces drawn as space glyphs, broke the run's regularity and came out
//!   `CORPORAT I O N` or `ITALL`.
//!
//! Courier throughout: every glyph advances 600/1000 of the font size, so the
//! gaps are what the arithmetic says they are.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::PdfDocument;

/// A one-page PDF whose content stream is `content`, with Courier as `/F1`.
fn page(content: &str) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Courier".to_vec()),
    }));
    let contents = doc.add_object(Stream::new(dictionary! {}, content.as_bytes().to_vec()));
    let pages = doc.new_object_id();
    let page = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => contents,
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
    });
    doc.objects.insert(
        pages,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page.into()],
            "Count" => 1,
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
    doc.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save");
    bytes
}

fn text_of(content: &str) -> String {
    let doc = PdfDocument::open(page(content)).expect("open");
    doc.extract_text_blocks(0)
        .expect("blocks")
        .iter()
        .flat_map(|block| block.spans.iter().map(|span| span.text.clone()))
        .collect::<Vec<_>>()
        .join("|")
}

#[test]
fn a_heading_filled_and_stroked_reads_once() {
    let text = text_of("BT /F1 14 Tf 2 Tr 72 700 Td (1.0 BACKGROUND) Tj ET");
    assert_eq!(text, "1.0 BACKGROUND");
}

#[test]
fn fill_then_stroke_with_clipping_reads_once_too() {
    let text = text_of("BT /F1 14 Tf 6 Tr 72 700 Td (MONITORING) Tj ET");
    assert_eq!(text, "MONITORING");
}

#[test]
fn a_pair_kern_inside_a_tracked_word_does_not_break_it() {
    // Three points of tracking on twelve-point capitals, over the word-space
    // threshold, and one pair tightened by 74 in the TJ: NYSE_PVA_2004.
    let text = text_of("BT /F1 12 Tf 3 Tc 72 700 Td [(CORPORA) 74 (TION)] TJ ET");
    assert_eq!(text, "CORPORATION");
}

#[test]
fn a_tracked_line_keeps_the_spaces_it_draws() {
    // Word spaces drawn as glyphs in a letter-spaced line: NYSE_WAL_2014.
    let text = text_of("BT /F1 12 Tf 3 Tc 72 700 Td (IT ALL ADDS UP) Tj ET");
    assert_eq!(text, "IT ALL ADDS UP");
}

#[test]
fn prose_with_word_spaces_as_gaps_keeps_them() {
    // No tracking: the words touch inside and part by a TJ word space.
    let text = text_of("BT /F1 12 Tf 72 700 Td [(suppose) -300 (that) -300 (the)] TJ ET");
    assert_eq!(text, "suppose that the");
}
