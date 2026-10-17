// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A substitute is chosen by what the page draws, not only by the name on it.
//!
//! The gap (#502, left open by #487). `pdf-substitute-fonts` carried twelve
//! Liberation faces and nothing else, and Liberation has Latin, Greek and
//! Cyrillic. `2033091` references three Arabic typefaces it does not embed --
//! `TraditionalArabic`, `Lotus`, `Lotus,Bold` -- and none of the three is in
//! any Standard-14 table, so the name heuristic answered with a Latin serif
//! that has no glyph for one of the 171 characters those fonts declare.
//! Before #487 the codes were deleted from the page; after it they survived
//! and drew .notdef. A wall of boxes is not the document either.
//!
//! The document says which characters it draws: its /ToUnicode, the same table
//! `point_substituted_cid_font_at_its_own_glyphs` aims through. So when the
//! name's answer covers almost none of them, the face that covers them is the
//! one to embed -- and only then, because a face chosen over the name is a face
//! with different metrics, and metric compatibility is why the name is asked
//! first.
//!
//! What this test does not check is how it looks. That is the four-axis
//! measurement on the ticket. What it checks is that the choice is made at all,
//! and that a Latin document does not get dragged into it.

use lopdf::{dictionary, Document, Object, Stream};

/// One page drawing `codes` through an unembedded Identity-H CID font named
/// `base_font`, whose /ToUnicode maps each code to the matching character.
fn document_drawing(base_font: &str, codes: &[u16], chars: &[u16]) -> Document {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();

    let fd_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => Object::Name(base_font.as_bytes().to_vec()),
        "Flags" => Object::Integer(4),
        "ItalicAngle" => Object::Integer(0),
        "StemV" => Object::Integer(80),
        "FontBBox" => Object::Array(vec![
            Object::Integer(-200), Object::Integer(-300),
            Object::Integer(1200), Object::Integer(1000),
        ]),
        "Ascent" => Object::Integer(800),
        "Descent" => Object::Integer(-200),
        "CapHeight" => Object::Integer(700),
    }));
    let descendant_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "CIDFontType2",
        "BaseFont" => Object::Name(base_font.as_bytes().to_vec()),
        "CIDSystemInfo" => dictionary! {
            "Registry" => Object::string_literal("Adobe"),
            "Ordering" => Object::string_literal("Identity"),
            "Supplement" => Object::Integer(0),
        },
        "FontDescriptor" => Object::Reference(fd_id),
        "DW" => Object::Integer(1000),
    }));

    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo\n<<\n/Registry (T9+0)\n/Ordering (UCS)\n/Supplement 0\n>> def\n\
         /CMapName /T9 def\n/CMapType 2 def\n1 begincodespacerange\n\
         <0000> <FFFF>\nendcodespacerange\n",
    );
    cmap.push_str(&format!("{} beginbfchar\n", codes.len()));
    for (code, ch) in codes.iter().zip(chars) {
        cmap.push_str(&format!("<{code:04X}> <{ch:04X}>\n"));
    }
    cmap.push_str("endbfchar\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    let tounicode = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        cmap.into_bytes(),
    )));

    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => Object::Name(base_font.as_bytes().to_vec()),
        "Encoding" => Object::Name(b"Identity-H".to_vec()),
        "DescendantFonts" => Object::Array(vec![Object::Reference(descendant_id)]),
        "ToUnicode" => Object::Reference(tounicode),
    }));

    let mut content = b"BT /F1 12 Tf 72 700 Td <".to_vec();
    for code in codes {
        content.extend_from_slice(format!("{code:04X}").as_bytes());
    }
    content.extend_from_slice(b"> Tj ET");
    let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));

    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference(pages_id),
        "MediaBox" => Object::Array(vec![
            Object::Integer(0), Object::Integer(0),
            Object::Integer(612), Object::Integer(792),
        ]),
        "Contents" => Object::Reference(content_id),
        "Resources" => dictionary! {
            "Font" => dictionary! { "F1" => Object::Reference(font_id) },
        },
    }));
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(1),
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
        }),
    );
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));
    doc
}

/// The descendant CIDFont of the document's one Type0 font.
fn descendant_of(doc: &Document) -> lopdf::ObjectId {
    for obj in doc.objects.values() {
        let Object::Dictionary(dict) = obj else {
            continue;
        };
        if dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) != Some(b"Type0") {
            continue;
        }
        if let Ok(Object::Array(arr)) = dict.get(b"DescendantFonts") {
            if let Some(Object::Reference(id)) = arr.first() {
                return *id;
            }
        }
    }
    panic!("no Type0 font left in the document");
}

/// The glyph a code draws, through the /CIDToGIDMap the conversion wrote.
fn glyph_for(doc: &Document, cid_id: lopdf::ObjectId, code: u16) -> u16 {
    let Some(Object::Dictionary(cid)) = doc.objects.get(&cid_id) else {
        panic!("the CIDFont dictionary is gone");
    };
    let stream = match cid.get(b"CIDToGIDMap").ok() {
        Some(Object::Reference(id)) => match doc.objects.get(id) {
            Some(Object::Stream(s)) => s.clone(),
            _ => return code,
        },
        Some(Object::Stream(s)) => s.clone(),
        _ => return code,
    };
    let mut stream = stream;
    let _ = stream.decompress();
    let at = usize::from(code) * 2;
    match stream.content.get(at..at + 2) {
        Some(pair) => u16::from_be_bytes([pair[0], pair[1]]),
        None => 0,
    }
}

/// The name of the face the conversion embedded, as `pdffonts` would read it.
fn embedded_face(doc: &Document, cid_id: lopdf::ObjectId) -> String {
    let Some(Object::Dictionary(cid)) = doc.objects.get(&cid_id) else {
        panic!("the CIDFont dictionary is gone");
    };
    let Ok(Object::Reference(fd_id)) = cid.get(b"FontDescriptor") else {
        panic!("the descendant has no FontDescriptor");
    };
    let Some(Object::Dictionary(fd)) = doc.objects.get(fd_id) else {
        panic!("the FontDescriptor is gone");
    };
    let Ok(Object::Reference(file_id)) = fd.get(b"FontFile2") else {
        return String::from("<no FontFile2>");
    };
    let Some(Object::Stream(stream)) = doc.objects.get(file_id) else {
        return String::from("<no program>");
    };
    let mut stream = stream.clone();
    let _ = stream.decompress();
    let Ok(face) = ttf_parser::Face::parse(&stream.content, 0) else {
        return String::from("<unparseable>");
    };
    face.names()
        .into_iter()
        .find(|n| n.name_id == ttf_parser::name_id::FAMILY)
        .and_then(|n| n.to_string())
        .unwrap_or_default()
}

/// Arabic gets a face with Arabic in it.
///
/// `2033091`'s shape, three characters wide: an Arabic typeface nobody's table
/// knows, drawing Arabic presentation forms. On master the name heuristic picks
/// a Liberation serif, every code maps to glyph 0, and the page draws boxes.
#[test]
fn an_arabic_document_is_given_a_face_that_has_arabic_in_it() {
    // U+FE81, U+FE76, U+FB58: presentation forms 2033091 actually draws.
    let mut doc = document_drawing(
        "TraditionalArabic",
        &[0x0159, 0x015A, 0x015B],
        &[0xFE81, 0xFE76, 0xFB58],
    );

    pdf_manip::pdfa_fonts::embed_fonts(&mut doc).expect("embedding failed");

    let cid_id = descendant_of(&doc);
    let family = embedded_face(&doc, cid_id);
    assert!(
        family.to_ascii_lowercase().contains("arabic"),
        "an Arabic document was given `{family}`, which has no Arabic in it",
    );
    for code in [0x0159u16, 0x015A, 0x015B] {
        assert_ne!(
            glyph_for(&doc, cid_id, code),
            0,
            "code {code:#06X} still draws .notdef, so the page is still boxes",
        );
    }
}

/// A bold Arabic name gets the bold face, not the regular one.
#[test]
fn a_bold_arabic_typeface_is_given_the_bold_face() {
    let mut doc = document_drawing("Lotus,Bold", &[0x0159], &[0xFE81]);

    pdf_manip::pdfa_fonts::embed_fonts(&mut doc).expect("embedding failed");

    let cid_id = descendant_of(&doc);
    let Some(Object::Dictionary(cid)) = doc.objects.get(&cid_id) else {
        panic!("the CIDFont dictionary is gone");
    };
    let Ok(Object::Reference(fd_id)) = cid.get(b"FontDescriptor") else {
        panic!("no FontDescriptor");
    };
    let Some(Object::Dictionary(fd)) = doc.objects.get(fd_id) else {
        panic!("no FontDescriptor");
    };
    let Ok(Object::Reference(file_id)) = fd.get(b"FontFile2") else {
        panic!("no FontFile2");
    };
    let Some(Object::Stream(stream)) = doc.objects.get(file_id) else {
        panic!("no program");
    };
    let mut stream = stream.clone();
    let _ = stream.decompress();
    let face = ttf_parser::Face::parse(&stream.content, 0).expect("the substitute parses");
    // Arabic first: a bold Latin face is bold and still cannot draw the page,
    // which is what master answers here.
    assert!(
        face.glyph_index('\u{FE81}').is_some_and(|gid| gid.0 != 0),
        "a bold Arabic typeface was given a face with no Arabic in it",
    );
    assert!(
        face.is_bold()
            || face.names().into_iter().any(|n| {
                n.name_id == ttf_parser::name_id::SUBFAMILY
                    && n.to_string().is_some_and(|s| s.contains("Bold"))
            }),
        "a bold Arabic typeface was given a regular face",
    );
}

/// A Latin document is not dragged into any of this.
///
/// The floor under the rule: the coverage test only fires where the name's
/// answer covers almost nothing, and a Latin page served by a Latin substitute
/// covers everything. If this goes red, the threshold has swallowed the normal
/// case and every Latin document just changed typeface.
#[test]
fn a_latin_document_keeps_the_face_its_name_asked_for() {
    let mut doc = document_drawing(
        "TimesNewRoman",
        &[0x0100, 0x0101, 0x0102],
        &[0x0041, 0x0042, 0x0043],
    );

    pdf_manip::pdfa_fonts::embed_fonts(&mut doc).expect("embedding failed");

    let cid_id = descendant_of(&doc);
    let family = embedded_face(&doc, cid_id);
    assert!(
        !family.to_ascii_lowercase().contains("arabic"),
        "a Latin document was given `{family}`",
    );
    // Read through the map rather than through the name: what matters is that
    // the face draws A, B and C, and a name table is not evidence of a glyph.
    for code in [0x0100u16, 0x0101, 0x0102] {
        assert_ne!(
            glyph_for(&doc, cid_id, code),
            0,
            "code {code:#06X} of a Latin document draws .notdef; the face it was given \
             cannot draw its own alphabet",
        );
    }
}
