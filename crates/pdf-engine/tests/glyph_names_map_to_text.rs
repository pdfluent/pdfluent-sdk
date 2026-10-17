// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Glyph names from a simple font without `/ToUnicode` read as text (#662).
//!
//! When a simple font carries no usable `/ToUnicode` map, extraction falls
//! back to the names in its `/Encoding /Differences`: each name is looked up
//! in the Adobe Glyph List data, decoded as a `uni`/`u` spelling, split into
//! underscore-joined components, or reduced to the base before a `.suffix`.
//! A name none of those rules reads contributes nothing.
//!
//! Each test below builds a one-page PDF whose Type1 font names its glyphs
//! only through `/Differences`, then reads the page back through
//! [`PdfDocument::extract_blocks`]:
//!
//! - `plain_agl_names_map_to_text` -- `/A` and `/eacute` read as `Aé`.
//! - `uni_and_u_names_map_to_text` -- `/uni00E9` and `/u1F600` read as
//!   `é` and `😀`.
//! - `underscore_ligature_name_maps_to_text` -- `/f_i` reads as `fi`.
//! - `suffixed_name_maps_through_its_base` -- `/a.sc` reads as `a`.
//! - `unknown_name_falls_back_unchanged` -- `/zzzUnknown` contributes
//!   nothing beside the `B` that does.
//!
//! The codes stay below printable ASCII on purpose: the final-resort
//! identity guess answers for ASCII codes even when every name rule misses,
//! which would mask a removed mapping. Down here a missing rule reads as
//! the empty string, so every test above fails when its rule is removed.
//!

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::{to_markdown, ExtractOptions, PdfDocument};

/// A one-page document whose single Type1 font names `codes` through
/// `/Differences`, with no `/ToUnicode` anywhere.
fn pdf_with_names(names: &[&str]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");

    let mut differences = vec![Object::Integer(1)];
    differences.extend(
        names
            .iter()
            .map(|name| Object::Name(name.as_bytes().to_vec())),
    );
    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        "Encoding" => Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Encoding".to_vec()),
            "BaseEncoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
            "Differences" => Object::Array(differences),
        }),
    }));

    let shown: Vec<u8> = (1..=names.len() as u8).collect();
    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new(
                "Tf",
                vec![Object::Name(b"F1".to_vec()), Object::Integer(12)],
            ),
            Operation::new("Td", vec![72.into(), 700.into()]),
            Operation::new("Tj", vec![Object::String(shown, StringFormat::Hexadecimal)]),
            Operation::new("ET", vec![]),
        ],
    }
    .encode()
    .expect("encode content");
    let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));

    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dictionary! {
                "F1" => Object::Reference(font_id),
            }),
        }),
    }));
    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![Object::Reference(page_id)]),
        "Count" => Object::Integer(1),
    }));
    if let Ok(Object::Dictionary(page)) = doc.get_object_mut(page_id) {
        page.set("Parent", Object::Reference(pages_id));
    }
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));

    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("save synthetic pdf");
    buf
}

/// The document's markdown through the public block pipeline.
fn markdown_of(names: &[&str]) -> String {
    let blocks = PdfDocument::open(pdf_with_names(names))
        .expect("open pdf")
        .extract_blocks(&ExtractOptions::default())
        .expect("extract blocks");
    to_markdown(&blocks)
}

#[test]
fn plain_agl_names_map_to_text() {
    let md = markdown_of(&["A", "eacute"]);
    assert!(
        md.contains("Aé"),
        "plain AGL names did not reach the text; got:\n{md}"
    );
}

#[test]
fn uni_and_u_names_map_to_text() {
    let md = markdown_of(&["uni00E9", "u1F600"]);
    assert!(
        md.contains("é"),
        "a uniXXXX name did not reach the text; got:\n{md}"
    );
    assert!(
        md.contains("😀"),
        "a uXXXXX name did not reach the text; got:\n{md}"
    );
}

#[test]
fn underscore_ligature_name_maps_to_text() {
    let md = markdown_of(&["f_i"]);
    assert!(
        md.contains("fi"),
        "an underscore ligature name did not read as its components; got:\n{md}"
    );
}

#[test]
fn suffixed_name_maps_through_its_base() {
    let md = markdown_of(&["a.sc"]);
    assert!(
        md.contains('a'),
        "a suffixed name did not read as its base; got:\n{md}"
    );
}

#[test]
fn unknown_name_falls_back_unchanged() {
    let md = markdown_of(&["zzzUnknown", "B"]);
    assert!(
        md.contains('B'),
        "a resolvable name beside an unknown one did not reach the text; got:\n{md}"
    );
    assert!(
        !md.contains('\u{FFFD}'),
        "an unknown name must contribute nothing, not a replacement character; got:\n{md}"
    );
}
