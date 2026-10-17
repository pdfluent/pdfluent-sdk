//! A non-embedded CID font with an `Identity` encoding and no `ToUnicode` map
//! still has to yield the text a reader shows (#430, class R-B of #389).
//!
//! Such a font hands the extractor glyph indices into a font the file does not
//! carry. There are two ways to learn what an index means, and this file holds
//! one document for each:
//!
//! - `standard_order_cids_read_as_text` — the face is a Latin one whose glyph
//!   order is the standard Macintosh ordering, so the index names a glyph and
//!   the glyph name names a character. Without that step the substitute font's
//!   own glyph order answers instead and every letter comes out shifted, which
//!   is what issue6127.pdf used to do (`eqpfkvkqpu` for `conditions`).
//! - `japan1_cids_read_as_text` — the descendant declares Adobe-Japan1, so the
//!   registry's UCS2 CMap answers. `Identity-H` says nothing about the
//!   collection, and reading only the encoding we called these CIDs
//!   Adobe-Identity and emitted each one as a code point.
//!
//! `an_unnamed_face_is_left_alone` is the other half of the first test: the
//! ordering is a property of a named face, so a face the table does not name
//! must not be given a plausible wrong reading.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::PdfDocument;

/// A one-page PDF drawing `cids` through a non-embedded `CIDFontType2` with an
/// `Identity-H` encoding and no `ToUnicode` map.
///
/// The font descriptor carries no `FontFile2` and the flags say symbolic,
/// which is what Word writes for exactly these fonts — the reader has nothing
/// to go on but `/BaseFont` and `/CIDSystemInfo`.
fn identity_h_pdf(base_font: &str, ordering: &str, cids: &[u16]) -> Vec<u8> {
    identity_h_pdf_with_cid_to_gid(base_font, ordering, cids, None)
}

/// The same document, with an explicit `/CIDToGIDMap` stream: entry `i` of
/// `cid_to_gid` is the glyph index CID `i` stands for.
fn identity_h_pdf_with_cid_to_gid(
    base_font: &str,
    ordering: &str,
    cids: &[u16],
    cid_to_gid: Option<&[u16]>,
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(cids.len() * 2);
    for cid in cids {
        bytes.extend_from_slice(&cid.to_be_bytes());
    }

    let ops = vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 24.into()]),
        Operation::new(
            "Tm",
            vec![
                1.into(),
                0.into(),
                0.into(),
                1.into(),
                72.into(),
                770.into(),
            ],
        ),
        Operation::new(
            "Tj",
            vec![Object::String(bytes, lopdf::StringFormat::Hexadecimal)],
        ),
        Operation::new("ET", vec![]),
    ];
    let content = Content { operations: ops }
        .encode()
        .expect("encode content");

    let mut doc = Document::with_version("1.7");
    let descriptor_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"FontDescriptor".to_vec()),
        "FontName" => Object::Name(base_font.as_bytes().to_vec()),
        "Flags" => Object::Integer(6),
        "FontBBox" => Object::Array(vec![
            Object::Integer(-568), Object::Integer(-307),
            Object::Integer(2000), Object::Integer(1007),
        ]),
        "ItalicAngle" => Object::Integer(0),
        "Ascent" => Object::Integer(891),
        "Descent" => Object::Integer(-216),
        "CapHeight" => Object::Integer(693),
        "StemV" => Object::Integer(82),
    }));
    let descendant_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"CIDFontType2".to_vec()),
        "BaseFont" => Object::Name(base_font.as_bytes().to_vec()),
        "CIDSystemInfo" => Object::Dictionary(dictionary! {
            "Registry" => Object::string_literal("Adobe"),
            "Ordering" => Object::string_literal(ordering),
            "Supplement" => Object::Integer(0),
        }),
        "FontDescriptor" => Object::Reference(descriptor_id),
        "DW" => Object::Integer(1000),
    }));
    if let Some(table) = cid_to_gid {
        let mut bytes = Vec::with_capacity(table.len() * 2);
        for gid in table {
            bytes.extend_from_slice(&gid.to_be_bytes());
        }
        let map_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, bytes)));
        if let Ok(Object::Dictionary(d)) = doc.get_object_mut(descendant_id) {
            d.set("CIDToGIDMap", Object::Reference(map_id));
        }
    }
    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type0".to_vec()),
        "BaseFont" => Object::Name(base_font.as_bytes().to_vec()),
        "Encoding" => Object::Name(b"Identity-H".to_vec()),
        "DescendantFonts" => Object::Array(vec![Object::Reference(descendant_id)]),
    }));
    let stream_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![
            Object::Integer(0), Object::Integer(0),
            Object::Integer(612), Object::Integer(792),
        ]),
        "Contents" => Object::Reference(stream_id),
        "Resources" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dictionary! { "F1" => Object::Reference(font_id) }),
        }),
    }));
    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![Object::Reference(page_id)]),
        "Count" => Object::Integer(1),
    }));
    if let Ok(Object::Dictionary(d)) = doc.get_object_mut(page_id) {
        d.set("Parent", Object::Reference(pages_id));
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

fn text_of(bytes: Vec<u8>) -> String {
    PdfDocument::open(bytes)
        .expect("open pdf")
        .extract_text(0)
        .expect("extract text")
        .trim()
        .to_string()
}

/// The CIDs are issue6127.pdf's, which draw `dessus` through its non-embedded
/// `TimesNewRoman`: index 71 is the 72nd name of the standard Macintosh
/// ordering, `d`, and printable ASCII runs on from there.
#[test]
fn standard_order_cids_read_as_text() {
    let text = text_of(identity_h_pdf(
        "TimesNewRoman",
        "Identity",
        &[0x47, 0x48, 0x56, 0x56, 0x58, 0x56],
    ));
    assert_eq!(text, "dessus", "standard-order CIDs did not read as text");
}

/// The same document under a face nobody can name. The mapping must not fire:
/// the standard ordering is a property of the named face, and applying it to an
/// unknown one turns an obviously broken extraction into a plausible wrong one.
#[test]
fn an_unnamed_face_is_left_alone() {
    let text = text_of(identity_h_pdf(
        "NoSuchFaceAnywhere",
        "Identity",
        &[0x47, 0x48, 0x56, 0x56, 0x58, 0x56],
    ));
    assert_ne!(
        text, "dessus",
        "an unknown face was read through the standard glyph ordering"
    );
}

/// noembed-identity-2.pdf's own CIDs and its own answer. `Identity-H` names no
/// character collection; the descendant's `/CIDSystemInfo` does, and the
/// Adobe-Japan1 UCS2 CMap turns those CIDs into the text a reader shows.
#[test]
fn japan1_cids_read_as_text() {
    let text = text_of(identity_h_pdf(
        "MS-PGothic",
        "Japan1",
        &[
            0x034B, 0x0022, 0x0043, 0x0054, 0x0055, 0x0053, 0x0042, 0x0044, 0x0055, 0x00F7, 0x00F8,
            0x00F9,
        ],
    ));
    assert_eq!(
        text, "あAbstract012",
        "Adobe-Japan1 CIDs behind Identity-H did not read as text"
    );
}

/// A `/CIDToGIDMap` stream says the CID is not itself the glyph index, and the
/// standard ordering speaks about glyph indices. The same six letters, drawn
/// as CIDs 1 to 6 through a map that sends them to the glyph indices of
/// `dessus`, have to read the same.
#[test]
fn a_cid_to_gid_map_is_followed() {
    let mut table = vec![0u16; 7];
    table[1..].copy_from_slice(&[0x47, 0x48, 0x56, 0x56, 0x58, 0x56]);
    let text = text_of(identity_h_pdf_with_cid_to_gid(
        "TimesNewRoman",
        "Identity",
        &[1, 2, 3, 4, 5, 6],
        Some(&table),
    ));
    assert_eq!(text, "dessus", "the CID to GID map was read past");
}
