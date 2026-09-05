// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A PDF/A conversion may not put a different character where a space was.
//!
//! `conversion_keeps_the_word_separator.rs` asks the same question of the
//! Type1 path and needs a corpus to do it. This one asks it of the Type0/CID
//! path and needs nothing: the shape fits in a two-glyph CID-keyed CFF, so it
//! runs on every machine and in CI.
//!
//! The defect it pins down (#182, #210). `fix_cid_font_notdef` decides which
//! CIDs the embedded program can draw and rewrites the rest. Two rules in that
//! decision were wrong together:
//!
//!   * CID 0 was .notdef by assumption. It is .notdef by convention, and a
//!     subset whose /ToUnicode says CID 0 is U+0020 draws a space there.
//!   * When no glyph called "space" was found, the substitute became
//!     `valid_cids.iter().min()` -- the lowest surviving CID of a subset,
//!     which is whatever glyph the subsetter happened to put first.
//!
//! So every space became a printing letter. On govdocs 000_000338 that read
//! `CHRISTINE ACHBERGER,$Institutionen$för$Geovetenskaper`; on 120_120186 it
//! was `NOTE(Graham`. Nothing we measured moved: the file validates, it
//! renders, and character retention goes *up*, because what used to be a space
//! now counts as a character. Only word retention fell, and it fell so far
//! (1,3% -- 8,7%) that it read as total text loss rather than as this.
//!
//! Which is why this test asserts the text that comes back out, not that the
//! output conforms. veraPDF is happy either way.

use lopdf::{dictionary, Document, Object, Stream};

/// A CID-keyed CFF with exactly two glyphs, taken from `cff-parser`'s own
/// fixture so the parser and this test agree about what it contains:
/// GID 0 -> CID 0, width 500 (the Private DICT default), and
/// GID 1 -> CID 1, width 650. No glyph names -- CID-keyed fonts have none,
/// which is exactly why the "is there a glyph called space" question cannot be
/// answered here and the substitute has to come from /ToUnicode.
#[rustfmt::skip]
const TWO_GLYPH_CID_CFF: &[u8] = &[
    0x01, 0x00, 0x04, 0x01,
    0x00, 0x01, 0x01, 0x01, 0x02, 0x46,
    0x00, 0x01, 0x01, 0x01, 0x11,
    0xCD, 0xF7, 0x78, 0x8B, 0x0C, 0x1E,
    0xAE, 0x0F,
    0xB1, 0x0C, 0x25,
    0xBE, 0x0C, 0x24,
    0xB4, 0x11,
    0x00, 0x00,
    0x00, 0x00,
    0x00, 0x00, 0x01,
    0x00, 0x00, 0x00,
    0x00, 0x02, 0x01, 0x01, 0x02, 0x05,
    0x0E,
    0xF9, 0x1E, 0x0E,
    0x00, 0x01, 0x01, 0x01, 0x04,
    0x90, 0xC6, 0x12,
    0xF8, 0x88, 0x14,
    0x8B, 0x15,
];

/// One page, one Identity-H Type0 font over the two-glyph CFF above.
///
/// `bfchar` is the (CID, Unicode) list the document declares. `cids` is what
/// the page draws. Both are given by the caller so a test can state its case
/// in the two things that matter and nothing else.
fn document_drawing(cids: &[u16], bfchar: &[(u16, u16)]) -> Document {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();

    let font_file = doc.add_object(Object::Stream(Stream::new(
        dictionary! { "Subtype" => "CIDFontType0C" },
        TWO_GLYPH_CID_CFF.to_vec(),
    )));
    let fd_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => "AAAAAA+TestCID",
        "Flags" => Object::Integer(4),
        "FontFile3" => Object::Reference(font_file),
    }));
    let descendant = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "CIDFontType0",
        "BaseFont" => "AAAAAA+TestCID",
        "CIDSystemInfo" => dictionary! {
            "Registry" => Object::string_literal("Adobe"),
            "Ordering" => Object::string_literal("Identity"),
            "Supplement" => Object::Integer(0),
        },
        "FontDescriptor" => Object::Reference(fd_id),
        "DW" => Object::Integer(500),
    }));

    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin\n\
         12 dict begin\nbegincmap\n/CMapName /Test def\n1 begincodespacerange\n\
         <0000> <FFFF>\nendcodespacerange\n",
    );
    cmap.push_str(&format!("{} beginbfchar\n", bfchar.len()));
    for (cid, unicode) in bfchar {
        cmap.push_str(&format!("<{cid:04X}> <{unicode:04X}>\n"));
    }
    cmap.push_str("endbfchar\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    let tounicode = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        cmap.into_bytes(),
    )));

    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => "AAAAAA+TestCID",
        "Encoding" => Object::Name(b"Identity-H".to_vec()),
        "DescendantFonts" => Object::Array(vec![Object::Reference(descendant)]),
        "ToUnicode" => Object::Reference(tounicode),
    }));

    let mut drawn = Vec::with_capacity(cids.len() * 2);
    for cid in cids {
        drawn.extend_from_slice(&cid.to_be_bytes());
    }
    let mut content = b"BT /F1 12 Tf 72 700 Td <".to_vec();
    for byte in &drawn {
        content.extend_from_slice(format!("{byte:02X}").as_bytes());
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

/// Every CID the page draws, in order.
///
/// Read out of the content stream rather than out of a text extractor: this
/// has to be the bytes the conversion wrote, not a reader's opinion of them.
fn cids_drawn(doc: &Document) -> Vec<u16> {
    let page_id = *doc.get_pages().values().next().expect("one page");
    let content_ids = match doc.objects.get(&page_id) {
        Some(Object::Dictionary(d)) => match d.get(b"Contents") {
            Ok(Object::Reference(id)) => vec![*id],
            _ => panic!("page has no /Contents reference"),
        },
        _ => panic!("page is not a dictionary"),
    };
    let mut uit = Vec::new();
    for id in content_ids {
        let Some(Object::Stream(stream)) = doc.objects.get(&id) else {
            continue;
        };
        let mut stream = stream.clone();
        let _ = stream.decompress();
        let ops = lopdf::content::Content::decode(&stream.content).expect("decode content");
        for op in ops.operations {
            if op.operator != "Tj" {
                continue;
            }
            let Some(Object::String(bytes, _)) = op.operands.first() else {
                continue;
            };
            for pair in bytes.chunks_exact(2) {
                uit.push(u16::from_be_bytes([pair[0], pair[1]]));
            }
        }
    }
    uit
}

/// The text the page yields, by the document's own /ToUnicode.
fn text_of(cids: &[u16], bfchar: &[(u16, u16)]) -> String {
    cids.iter()
        .map(|cid| {
            bfchar
                .iter()
                .find(|(c, _)| c == cid)
                .and_then(|(_, u)| char::from_u32(*u as u32))
                .unwrap_or('\u{FFFD}')
        })
        .collect()
}

/// CID 0 is the space here, and it has to still be the space afterwards.
///
/// The old code rejected value 0 outright and then substituted
/// `min(valid_cids)` = CID 1 = 'A', so `A A` came back as `AAA`: the same
/// three characters, one of them wrong, and no measurement we had moved.
#[test]
fn a_space_parked_at_cid_zero_is_not_replaced_by_a_letter() {
    // CID 0 -> U+0020, CID 1 -> 'A'. The subset put the space at CID 0, which
    // every rule of thumb reads as .notdef.
    let bfchar = [(0u16, 0x0020u16), (1, 0x0041)];
    let drawn = [1u16, 0, 1];
    let mut doc = document_drawing(&drawn, &bfchar);

    let before = cids_drawn(&doc);
    assert_eq!(
        text_of(&before, &bfchar),
        "A A",
        "the fixture does not draw what it claims to"
    );

    pdf_manip::pdfa_fonts::fix_cid_font_notdef(&mut doc);

    let after = cids_drawn(&doc);
    assert_eq!(
        text_of(&after, &bfchar),
        "A A",
        "the conversion changed the text: {before:?} -> {after:?}"
    );
}

/// With no space to substitute, the code is dropped -- never rewritten.
///
/// CID 7 has no glyph and the document declares no whitespace anywhere, so
/// there is nothing that could stand in for it. Master's rule for that case is
/// `the_rewriter_still_drops_cid_zero_when_it_is_not_valid`: a genuine .notdef
/// reference is removed, because ISO 19005-2 6.2.11.8 forbids it on the page.
/// What must not happen is the third option the old code took -- inventing a
/// letter. Dropping loses one character; substituting the lowest surviving CID
/// changes what the page says.
#[test]
fn a_code_with_no_substitute_is_dropped_rather_than_rewritten() {
    let bfchar = [(1u16, 0x0041u16), (7, 0x0042)];
    let drawn = [1u16, 7, 1];
    let mut doc = document_drawing(&drawn, &bfchar);

    pdf_manip::pdfa_fonts::fix_cid_font_notdef(&mut doc);

    let after = cids_drawn(&doc);
    assert_eq!(
        text_of(&after, &bfchar),
        "AA",
        "the unrenderable code was rewritten into a glyph instead of removed: {after:?}"
    );
}

/// A substitute is a space or it is nothing. It is never the first glyph left.
///
/// CID 3 is the space by /ToUnicode and CID 9 is drawn without a glyph. The
/// page draws two `A`s; whatever the conversion does with CID 9, it must still
/// draw two. The old fallback made it three, and three `A`s is what
/// `AFFILIATIONS((alphabetical(by(author)(` is on a real document.
#[test]
fn an_unrenderable_code_never_becomes_the_lowest_surviving_glyph() {
    let bfchar = [(1u16, 0x0041u16), (3, 0x0020), (9, 0x0042)];
    let drawn = [1u16, 9, 1];
    let mut doc = document_drawing(&drawn, &bfchar);

    let letters_before = drawn.iter().filter(|cid| **cid == 1).count();
    pdf_manip::pdfa_fonts::fix_cid_font_notdef(&mut doc);
    let after = cids_drawn(&doc);
    let letters_after = after.iter().filter(|cid| **cid == 1).count();

    assert_eq!(
        letters_after, letters_before,
        "a code was replaced by the lowest surviving CID, which is a letter: \
         {drawn:?} -> {after:?}"
    );
}
