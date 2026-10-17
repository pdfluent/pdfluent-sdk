// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! An unembedded Identity-H CID font keeps the text it draws.
//!
//! The defect (#487, class F of #381). `2033091` (Arabic) and `6501188`
//! (Cyrillic) reference CID TrueType fonts they do not carry, with Identity-H:
//! the codes on the page are the *source* font's glyph ids. PDF/A wants a font
//! embedded, so we put one there -- and the ids mean nothing in it. Every code
//! resolved to .notdef, `fix_cid_font_notdef` removed it from the page, and
//! 14,648 codes left `2033091` (34,348 shown bytes down to 5,008, word
//! retention 4.7 %) and 2,270 left `6501188` (18 %). The Cyrillic and Arabic
//! were gone from the render and from the extraction; only Latin punctuation
//! survived.
//!
//! Both documents say what their codes mean -- they carry a /ToUnicode. That is
//! the bridge: code -> Unicode -> the substitute's own glyph, written into
//! /CIDToGIDMap, which is exactly the indirection a CIDFontType2 has for this.
//! The content stream is not touched and the /ToUnicode stays true; only the
//! map between them is filled in.
//!
//! Two things had to change together. The substitute has to be a TrueType face
//! -- the Standard 14 we compile in are bare CFF, and a CFF on a CID font is
//! read by glyph index, which is the very problem -- and the widths have to
//! follow the map rather than collapse onto one /DW.
//!
//! What no fix reaches: a character the substitute has no glyph for. The
//! bundled faces are Liberation, which carries Latin, Greek and Cyrillic and no
//! Arabic, so `2033091` stays unrenderable. The rule this test holds is that
//! such a code is left unmapped -- never pointed at some other glyph, which
//! would put a different character on the page.

use lopdf::{dictionary, Document, Object, Stream};

/// One page drawing `codes` through an unembedded Identity-H CID font.
///
/// `bfchar` is the /ToUnicode the document carries. The font names a real
/// typeface it does not embed, which is the whole shape of the defect.
fn document_drawing(codes: &[u16], bfchar: &[(u16, u16)]) -> Document {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();

    let fd_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => "TimesNewRoman",
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
        "BaseFont" => "TimesNewRoman",
        "CIDSystemInfo" => dictionary! {
            "Registry" => Object::string_literal("Adobe"),
            "Ordering" => Object::string_literal("Identity"),
            "Supplement" => Object::Integer(0),
        },
        "FontDescriptor" => Object::Reference(fd_id),
        "DW" => Object::Integer(250),
    }));

    // The /CIDSystemInfo dictionary and its literal strings are in the fixture
    // on purpose: they are what a real CMap carries, and a reader that takes
    // `<<` for a hex string swallows the dictionary and then cannot get past
    // the `>` of its `>>`.
    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo\n<<\n/Registry (F1+0)\n/Ordering (UCS)\n\
         /Supplement 0\n>> def\n\
         /CMapName /Test def\n/CMapType 2 def\n1 begincodespacerange\n\
         <0000> <FFFF>\nendcodespacerange\n",
    );
    cmap.push_str(&format!("{} beginbfchar\n", bfchar.len()));
    for (code, unicode) in bfchar {
        cmap.push_str(&format!("<{code:04X}> <{unicode:04X}>\n"));
    }
    cmap.push_str("endbfchar\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    let tounicode = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        cmap.into_bytes(),
    )));

    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => "TimesNewRoman",
        "Encoding" => Object::Name(b"Identity-H".to_vec()),
        "DescendantFonts" => Object::Array(vec![Object::Reference(descendant_id)]),
        "ToUnicode" => Object::Reference(tounicode),
    }));

    let mut content = b"BT /F1 12 Tf 72 700 Td <".to_vec();
    for code in codes {
        content.extend_from_slice(format!("{:04X}", code).as_bytes());
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

/// Every code the page draws, in order, read out of the content stream.
fn codes_drawn(doc: &Document) -> Vec<u16> {
    let page_id = *doc.get_pages().values().next().expect("one page");
    let Some(Object::Dictionary(page)) = doc.objects.get(&page_id) else {
        panic!("page is not a dictionary");
    };
    let Ok(Object::Reference(content_id)) = page.get(b"Contents") else {
        panic!("page has no /Contents reference");
    };
    let Some(Object::Stream(stream)) = doc.objects.get(content_id) else {
        panic!("the content stream is gone");
    };
    let mut stream = stream.clone();
    let _ = stream.decompress();
    let ops = lopdf::content::Content::decode(&stream.content).expect("decode content");
    let mut drawn = Vec::new();
    for op in ops.operations {
        if op.operator != "Tj" {
            continue;
        }
        if let Some(Object::String(bytes, _)) = op.operands.first() {
            for pair in bytes.chunks_exact(2) {
                drawn.push(u16::from_be_bytes([pair[0], pair[1]]));
            }
        }
    }
    drawn
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

/// The glyph the conversion says a code draws, through /CIDToGIDMap.
fn glyph_for(doc: &Document, cid_id: lopdf::ObjectId, code: u16) -> u16 {
    let Some(Object::Dictionary(cid)) = doc.objects.get(&cid_id) else {
        panic!("the CIDFont dictionary is gone");
    };
    let stream = match cid.get(b"CIDToGIDMap").ok() {
        Some(Object::Reference(id)) => match doc.objects.get(id) {
            Some(Object::Stream(s)) => s.clone(),
            _ => return code, // /Identity: the code is the glyph id.
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

/// The width the dictionary states for a code.
fn width_for(doc: &Document, cid_id: lopdf::ObjectId, code: u16) -> i64 {
    let Some(Object::Dictionary(cid)) = doc.objects.get(&cid_id) else {
        panic!("the CIDFont dictionary is gone");
    };
    let dw = match cid.get(b"DW") {
        Ok(Object::Integer(v)) => *v,
        _ => 1000,
    };
    let Ok(Object::Array(w)) = cid.get(b"W") else {
        return dw;
    };
    let mut i = 0;
    while i < w.len() {
        let Ok(first) = w[i].as_i64() else { break };
        match w.get(i + 1) {
            Some(Object::Array(entries)) => {
                for (j, entry) in entries.iter().enumerate() {
                    if first + j as i64 == i64::from(code) {
                        if let Ok(v) = entry.as_i64() {
                            return v;
                        }
                    }
                }
                i += 2;
            }
            Some(_) => {
                let (Ok(last), Some(Ok(width))) =
                    (w[i + 1].as_i64(), w.get(i + 2).map(|o| o.as_i64()))
                else {
                    break;
                };
                if (first..=last).contains(&i64::from(code)) {
                    return width;
                }
                i += 3;
            }
            None => break,
        }
    }
    dw
}

/// Cyrillic that the substitute can draw survives the conversion, aimed.
///
/// This is `6501188`: codes 0x023A.. are the source TimesNewRoman's glyph ids
/// for А, Б, В. Before the fix the substitute was a bare CFF read by glyph
/// index, every code came out .notdef, and all three were deleted from the page.
#[test]
fn a_code_the_substitute_can_draw_is_pointed_at_the_right_glyph() {
    // А, Б, В — Liberation Serif carries all three.
    let bfchar = [(0x023Au16, 0x0410u16), (0x023B, 0x0411), (0x023C, 0x0412)];
    let drawn = [0x023Au16, 0x023B, 0x023C];
    let mut doc = document_drawing(&drawn, &bfchar);

    pdf_manip::pdfa_fonts::embed_fonts(&mut doc).expect("embedding failed");
    pdf_manip::pdfa_fonts::fix_cid_font_notdef(&mut doc);

    assert_eq!(
        codes_drawn(&doc),
        drawn,
        "the conversion removed Cyrillic codes from the page instead of aiming them",
    );

    let cid_id = descendant_of(&doc);
    for code in drawn {
        assert_ne!(
            glyph_for(&doc, cid_id, code),
            0,
            "code {code:#06X} still draws .notdef, so its character is not on the page",
        );
        assert!(
            width_for(&doc, cid_id, code) > 0,
            "code {code:#06X} has no width of its own, so the line reflows",
        );
    }
}

/// Three different letters do not become one advance.
///
/// The old width step wiped /W and set /DW to .notdef's advance, so every code
/// in a substituted CID font was the same width whatever it drew.
#[test]
fn the_widths_follow_the_glyphs_the_codes_were_aimed_at() {
    // i, m, W — three widths no proportional face agrees on.
    let bfchar = [(0x0100u16, 0x0069u16), (0x0101, 0x006D), (0x0102, 0x0057)];
    let drawn = [0x0100u16, 0x0101, 0x0102];
    let mut doc = document_drawing(&drawn, &bfchar);

    pdf_manip::pdfa_fonts::embed_fonts(&mut doc).expect("embedding failed");

    let cid_id = descendant_of(&doc);
    let widths: Vec<i64> = drawn.iter().map(|c| width_for(&doc, cid_id, *c)).collect();
    assert!(
        widths[0] < widths[1] && widths[1] < widths[2],
        "i, m and W came out {widths:?}: the widths do not follow the glyphs",
    );
}

/// A character no bundled face carries is left unmapped, not redirected.
///
/// Thai, since #502: Arabic used to be this case and is not any more, and a
/// test whose premise a later change removes is a test that passes for the
/// wrong reason. Not Hebrew either -- Liberation carries that, which is worth
/// knowing before anyone bundles a face for it. What must not happen is the code being pointed at some glyph
/// anyway: an unrenderable code is a known loss, a wrong glyph is a different
/// document.
#[test]
fn a_character_the_substitute_lacks_is_never_pointed_at_another_glyph() {
    // U+0E01 THAI CHARACTER KO KAI, alongside a Latin 'A' that must still work.
    let bfchar = [(0x0159u16, 0x0E01u16), (0x015A, 0x0041)];
    let drawn = [0x0159u16, 0x015A];
    let mut doc = document_drawing(&drawn, &bfchar);

    pdf_manip::pdfa_fonts::embed_fonts(&mut doc).expect("embedding failed");

    let cid_id = descendant_of(&doc);
    assert_eq!(
        glyph_for(&doc, cid_id, 0x0159),
        0,
        "the Thai code was aimed at a glyph the substitute has for something else",
    );
    assert_ne!(
        glyph_for(&doc, cid_id, 0x015A),
        0,
        "the Latin code in the same font was not aimed at all",
    );
}

/// A font the substitute cannot serve at all still gets a map.
///
/// `2033091`'s Arabic faces were this case until #502 bundled a face that can
/// draw them; Thai is the case now, and the rule is the same one. Leaving
/// /CIDToGIDMap at Identity here would let every code draw whichever glyph sits
/// at that index in a 2,500-glyph Latin face -- the page would read as text and
/// say something else. So the map is written even when it is all zeros, and the
/// codes stay .notdef. Such a document needs a face for its script, not a
/// different map; the counts per script are on #502.
#[test]
fn a_font_the_substitute_cannot_serve_is_not_left_on_identity() {
    // Thai: no bundled face has it, Noto Naskh Arabic included. (Liberation
    // does carry Hebrew, Greek and Cyrillic, so none of those would do here.)
    let bfchar = [(0x0159u16, 0x0E01u16), (0x015A, 0x0E02), (0x015B, 0x0E03)];
    let drawn = [0x0159u16, 0x015A, 0x015B];
    let mut doc = document_drawing(&drawn, &bfchar);

    pdf_manip::pdfa_fonts::embed_fonts(&mut doc).expect("embedding failed");

    let cid_id = descendant_of(&doc);
    for code in drawn {
        assert_eq!(
            glyph_for(&doc, cid_id, code),
            0,
            "code {code:#06X} draws a glyph the substitute has for another character",
        );
    }
}
