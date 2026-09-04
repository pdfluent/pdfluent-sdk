// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Character codes below 32 are only control characters when the font says so.
//!
//! ISO 19005-2 §6.2.11.8 forbids .notdef references, and codes below 32 in a
//! font with no encoding entry resolve to .notdef — so the PDF/A conversion
//! blanks them to 0x20. A TeX subset encoding, however, starts its
//! /Differences at code 1: on a Computer Modern subset, codes 1..31 are the
//! document's ordinary letters. Blanking those erases the page and leaves a
//! *conforming* file behind, which is why validation alone never caught it.
//!
//! Measured on govdocs 170_170298.pdf (a pdfTeX physics problem set): 277 of
//! its 368 character codes sit below 32, and stripping them dropped word
//! retention from 100% to 6% while veraPDF reported the output compliant.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, Stream, StringFormat};

/// A Type1 subset whose /Differences puts real letters on codes 1..5,
/// the way pdfTeX writes Computer Modern.
fn tex_subset_font() -> Dictionary {
    dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "BEGJOF+CMR10",
        "FirstChar" => 1_i64,
        "LastChar" => 5_i64,
        "Encoding" => dictionary! {
            "Type" => "Encoding",
            "Differences" => vec![
                1.into(),
                Object::Name(b"h".to_vec()),
                Object::Name(b"e".to_vec()),
                Object::Name(b"l".to_vec()),
                Object::Name(b"o".to_vec()),
                Object::Name(b"w".to_vec()),
            ],
        },
    }
}

/// The same font with no /Encoding at all: codes below 32 have nothing
/// mapping them to a glyph, so they really do resolve to .notdef.
fn unencoded_font() -> Dictionary {
    dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "BEGJOF+CMR10",
        "FirstChar" => 1_i64,
        "LastChar" => 5_i64,
    }
}

fn doc_with(font: Dictionary, text: &[u8]) -> (Document, (u32, u16)) {
    let mut doc = Document::with_version("1.7");
    let font_id = doc.add_object(Object::Dictionary(font));
    let mut fonts = Dictionary::new();
    fonts.set("F1", Object::Reference(font_id));

    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 12.into()]),
            Operation::new(
                "Tj",
                vec![Object::String(text.to_vec(), StringFormat::Literal)],
            ),
            Operation::new("ET", vec![]),
        ],
    };
    let stream_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        content.encode().unwrap(),
    )));

    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Page",
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => Object::Reference(stream_id),
        "Resources" => dictionary! { "Font" => Object::Dictionary(fonts) },
    }));
    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Pages",
        "Kids" => vec![Object::Reference(page_id)],
        "Count" => 1_i64,
    }));
    if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
        d.set("Parent", Object::Reference(pages_id));
    }
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));
    (doc, stream_id)
}

/// The bytes of the single Tj operand, after whatever the pass did to it.
fn shown_bytes(doc: &Document, stream_id: (u32, u16)) -> Vec<u8> {
    let Some(Object::Stream(s)) = doc.objects.get(&stream_id) else {
        panic!("content stream vanished");
    };
    let content = Content::decode(&s.content).expect("decode content");
    for op in &content.operations {
        if op.operator == "Tj" {
            if let Some(Object::String(bytes, _)) = op.operands.first() {
                return bytes.clone();
            }
        }
    }
    panic!("no Tj in content stream");
}

#[test]
fn differences_mapped_control_codes_survive_the_strip() {
    let text = b"\x01\x02\x03\x03\x04"; // "hello" in this subset's encoding
    let (mut doc, stream_id) = doc_with(tex_subset_font(), text);

    pdf_manip::pdfa_fonts::strip_control_chars_from_streams(&mut doc);

    assert_eq!(
        shown_bytes(&doc, stream_id),
        text.to_vec(),
        "codes 1..5 are letters here — /Differences names them — so the pass \
         must leave them alone; blanking them to 0x20 erases the page"
    );
}

#[test]
fn unmapped_control_codes_are_still_stripped() {
    // Without this the fix would be indistinguishable from deleting the pass.
    let text = b"\x01\x02\x03";
    let (mut doc, stream_id) = doc_with(unencoded_font(), text);

    pdf_manip::pdfa_fonts::strip_control_chars_from_streams(&mut doc);

    assert_eq!(
        shown_bytes(&doc, stream_id),
        b"   ".to_vec(),
        "no /Differences means these codes resolve to .notdef, which \
         ISO 19005-2 6.2.11.8 forbids — they must become spaces"
    );
}

#[test]
fn tab_and_newline_are_never_touched() {
    let text = b"\x09\x0A\x0D";
    let (mut doc, stream_id) = doc_with(unencoded_font(), text);

    pdf_manip::pdfa_fonts::strip_control_chars_from_streams(&mut doc);

    assert_eq!(shown_bytes(&doc, stream_id), text.to_vec());
}
