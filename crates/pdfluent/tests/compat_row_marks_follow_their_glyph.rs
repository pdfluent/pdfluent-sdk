//! The character output carries the AMS combining marks where the text does
//! (spec `specs/text/circlecopyrt.md`, C4 and C5a, review fix 6).
//!
//! `circlecopyrt` reads as U+20DD and `negationslash` as U+0338, each
//! directly after the glyph it is drawn over. The raw dict's characters are
//! one more output path over the same spans; read in order they must spell
//! what the page's text spells, with no lone mark and no mark in front.
//!
//! The fixture is built in-memory via `lopdf`: Helvetica for the text, and a
//! non-embedded `Type1` font whose `/Differences` names code `A`.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::{dictionary, Document, Object, Stream};
use pdfluent::compat::{DictBlock, Page};
use pdfluent::prelude::*;

/// One page drawing `x`, then the mark glyph named `name` at 100 and `base`
/// at 102.8 in the order `mark_first` says, then `y`, all at 10 pt.
fn page(name: &str, base: &str, mark_first: bool) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let helvetica = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    });
    let named = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "CMSY10",
        "FirstChar" => 65_i64,
        "LastChar" => 65_i64,
        "Widths" => vec![Object::Integer(1000)],
        "Encoding" => dictionary! {
            "Type" => "Encoding",
            "Differences" => vec![Object::Integer(65), Object::Name(name.as_bytes().to_vec())],
        },
    });
    let resources_id = doc.add_object(dictionary! {
        "Font" => dictionary! { "F1" => helvetica, "F2" => named },
    });
    let mark = "/F2 10 Tf 1 0 0 1 100 700 Tm (A) Tj\n".to_string();
    let glyph = format!("/F1 10 Tf 1 0 0 1 102.8 700 Tm ({base}) Tj\n");
    let (first, second) = if mark_first {
        (mark, glyph)
    } else {
        (glyph, mark)
    };
    let content = format!(
        "BT\n/F1 10 Tf 1 0 0 1 50 700 Tm (x) Tj\n{first}{second}/F1 10 Tf 1 0 0 1 115 700 Tm (y) Tj\nET\n"
    );
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
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

fn bare(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

#[test]
fn the_raw_characters_spell_the_marks_where_the_text_does() {
    for (name, mark, base) in [
        ("circlecopyrt", '\u{20DD}', "c"),
        ("negationslash", '\u{0338}', "="),
    ] {
        for mark_first in [false, true] {
            let doc =
                PdfDocument::from_bytes(&page(name, base, mark_first)).expect("fixture opens");
            let page = Page::new(&doc, 0);
            let textpage = page.get_textpage(None, 0, None).expect("text page builds");
            let text = bare(&textpage.extract_text(false).expect("text extracts"));
            assert_eq!(
                text,
                format!("x{base}{mark}y"),
                "/{name}, mark first: {mark_first}"
            );
            let raw = textpage.extract_rawdict(false).expect("raw dict extracts");
            let mut chars = String::new();
            for block in &raw.blocks {
                if let DictBlock::Text { lines, .. } = block {
                    for line in lines {
                        for span in &line.spans {
                            chars.extend(span.chars.iter().map(|c| c.ch));
                        }
                    }
                }
            }
            assert_eq!(bare(&chars), text, "/{name}, mark first: {mark_first}");
        }
    }
}
