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
    let mut sections = format!("{} beginbfchar\n", bfchar.len());
    for (cid, unicode) in bfchar {
        sections.push_str(&format!("<{cid:04X}> <{unicode:04X}>\n"));
    }
    sections.push_str("endbfchar\n");
    document_declaring(cids, &sections)
}

/// The same page, with the /ToUnicode sections given verbatim.
///
/// A producer may declare the very same mapping as a bfchar pair, as a
/// one-code bfrange, as a bfrange with an array of destinations, or as an
/// identity bfrange over the whole plane -- and the reader has to see the
/// space in all four. The tests below hand over the bytes their producer
/// writes, which is the only way to state that case.
fn document_declaring(cids: &[u16], tounicode_sections: &str) -> Document {
    document_declaring_under(cids, tounicode_sections, None)
}

/// The same page again, read through a CMap of the document's own: `cmap_name`
/// names an embedded /Encoding stream, and None keeps Identity-H.
///
/// An embedded CMap is the shape that matters here -- it is what pdf.js
/// `bug1019475_1` carries, and unlike a predefined name there is no table of
/// ranges the pipeline can fall back on.
fn document_declaring_under(
    cids: &[u16],
    tounicode_sections: &str,
    cmap_name: Option<&str>,
) -> Document {
    let mut doc = Document::with_version("1.7");
    let encoding = match cmap_name {
        None => Object::Name(b"Identity-H".to_vec()),
        Some(name) => {
            let cmap = format!(
                "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
                 /CMapName /{name} def\n/CMapType 1 def\n1 begincodespacerange\n\
                 <0000> <FFFF>\nendcodespacerange\n1 begincidrange\n<0000> <FFFF> 0\n\
                 endcidrange\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n"
            );
            Object::Reference(doc.add_object(Object::Stream(Stream::new(
                dictionary! {
                    "Type" => "CMap",
                    "CMapName" => Object::Name(name.as_bytes().to_vec()),
                },
                cmap.into_bytes(),
            ))))
        }
    };
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
        // The two widths the CFF above actually carries, so a pass that
        // compares the two finds them agreeing.
        "W" => Object::Array(vec![
            Object::Integer(0),
            Object::Array(vec![Object::Integer(500), Object::Integer(650)]),
        ]),
    }));

    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin\n\
         12 dict begin\nbegincmap\n/CMapName /Test def\n1 begincodespacerange\n\
         <0000> <FFFF>\nendcodespacerange\n",
    );
    cmap.push_str(tounicode_sections);
    cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    let tounicode = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        cmap.into_bytes(),
    )));

    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => "AAAAAA+TestCID",
        "Encoding" => encoding,
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

/// Word, Excel and PowerPoint declare the space in an array of destinations.
///
/// `ABCDEE+Calibri` of tuning document 0031969 writes exactly this line, and
/// its CID 3 is drawn 743 times. The reader knew `beginbfchar` alone, so it
/// answered "this font declares no whitespace", the empty outline of the space
/// read as a glyph the subsetter had stripped, and every one of those 743
/// codes was deleted from the page -- words glued together and, the advance
/// going with the code, the lines reflowed. (#485.)
#[test]
fn a_space_declared_in_an_array_of_destinations_survives() {
    let shape = "1 beginbfrange\n<0000> <0001> [<0020> <0041>]\nendbfrange\n";
    let mapping = [(0u16, 0x0020u16), (1, 0x0041)];
    let drawn = [1u16, 0, 1];
    let mut doc = document_declaring(&drawn, shape);

    assert_eq!(
        text_of(&cids_drawn(&doc), &mapping),
        "A A",
        "the fixture does not draw what it claims to"
    );

    pdf_manip::pdfa_fonts::fix_cid_font_notdef(&mut doc);

    let after = cids_drawn(&doc);
    assert_eq!(
        text_of(&after, &mapping),
        "A A",
        "the space declared in an array was read as a missing glyph: {after:?}"
    );
}

/// iText, Publisher, Soda PDF and PDFium declare it as a one-code range.
///
/// `NECPHF+IPAexGothic` of tuning document 3098921 writes `<0003><0003><0020>`
/// with no space between the tokens; its CID 3 is drawn 445 times.
#[test]
fn a_space_declared_as_a_one_code_range_survives() {
    let shape = "2 beginbfrange\n<0000><0000><0020>\n<0001><0001><0041>\nendbfrange\n";
    let mapping = [(0u16, 0x0020u16), (1, 0x0041)];
    let drawn = [1u16, 0, 1];
    let mut doc = document_declaring(&drawn, shape);

    pdf_manip::pdfa_fonts::fix_cid_font_notdef(&mut doc);

    let after = cids_drawn(&doc);
    assert_eq!(
        text_of(&after, &mapping),
        "A A",
        "the space declared as a one-code range was read as a missing glyph: {after:?}"
    );
}

/// ABBYY FineReader ends its CMap lines with a bare CR.
///
/// `BXTCFK+TimesNewRomanPS-BoldItalicMT` of tuning document 0260774 declares
/// 47 bfchar pairs, the first of them `<0001> <0020>`, separated by CR alone.
/// `str::lines` does not split on that, so the whole CMap arrived as one line
/// whose only content was the word `beginbfchar` and not a single pair was
/// read -- the shape the reader did know, in a file it could not see. Its
/// spaces were rewritten to CID 0x4B, which extracts as `%`.
#[test]
fn a_space_declared_on_a_carriage_return_line_survives() {
    let shape = "2 beginbfchar\r<0000> <0020>\r<0001> <0041>\rendbfchar\r";
    let mapping = [(0u16, 0x0020u16), (1, 0x0041)];
    let drawn = [1u16, 0, 1];
    let mut doc = document_declaring(&drawn, shape);

    pdf_manip::pdfa_fonts::fix_cid_font_notdef(&mut doc);

    let after = cids_drawn(&doc);
    assert_eq!(
        text_of(&after, &mapping),
        "A A",
        "a CMap written with CR line endings was not read at all: {after:?}"
    );
}

/// A /ToUnicode is keyed by character code, and only Identity makes that a CID.
///
/// This pass reads the keys of a /ToUnicode as CIDs, which they are under
/// Identity-H and are not under anything else: the encoding CMap sits between
/// the two and maps one to the other. Reading them as CIDs under a CMap of the
/// document's own keeps whatever that CMap resolves them to -- on pdf.js
/// `bug1019475_1`, an embedded `Prince-ArialMT-H` whose /ToUnicode declares
/// `<01> <04> <0020>`, that is .notdef, 148 times on a page that used to be
/// empty, and §6.2.11.8 fails.
///
/// So the declaration is read where its keys mean what this pass takes them
/// for, and nowhere else. CID 0 is `.notdef` by convention and survives only
/// because the document calls it a space: under Identity that claim is about
/// this CID, and under any other CMap it is about a code that the encoding
/// resolves to some other one.
#[test]
fn a_blank_declared_under_a_cmap_of_the_documents_own_is_not_a_cid() {
    let shape = "2 beginbfchar\n<0000> <0020>\n<0001> <0041>\nendbfchar\n";
    let drawn = [1u16, 0, 1];

    let mut identity = document_declaring(&drawn, shape);
    pdf_manip::pdfa_fonts::fix_cid_font_notdef(&mut identity);
    assert!(
        cids_drawn(&identity).contains(&0),
        "under Identity-H the /ToUnicode keys are CIDs, and CID 0 is declared blank"
    );

    let mut through_a_cmap = document_declaring_under(&drawn, shape, Some("Prince-Test-H"));
    pdf_manip::pdfa_fonts::fix_cid_font_notdef(&mut through_a_cmap);
    assert!(
        !cids_drawn(&through_a_cmap).contains(&0),
        "the code was kept as a CID through a CMap that does not make it one: {:?}",
        cids_drawn(&through_a_cmap)
    );
}

/// A declared blank the program has no glyph for is still a .notdef reference.
///
/// `HiddenHorzOCR`, the stub face an OCR layer draws its invisible text with,
/// is a handful of glyphs and a /ToUnicode that calls code 32 a space. Keeping
/// that code because the document calls it blank puts a glyph on the page that
/// the program cannot draw -- §6.2.11.8, 16 times on tuning document 0627369
/// and 100 times on 2569641, both of which had their OCR layer cleared before.
///
/// An empty outline is a space. An absent glyph is not, whatever the document
/// says about it.
#[test]
fn a_blank_the_program_does_not_carry_is_not_admitted() {
    // CID 7 is past the end of the two-glyph program, and declared a space.
    let shape = "3 beginbfchar\n<0000> <0020>\n<0001> <0041>\n<0007> <0020>\nendbfchar\n";
    let drawn = [1u16, 7, 1];
    let mut doc = document_declaring(&drawn, shape);

    pdf_manip::pdfa_fonts::fix_cid_font_notdef(&mut doc);

    assert!(
        !cids_drawn(&doc).contains(&7),
        "a code with no glyph was kept because the document called it a space: {:?}",
        cids_drawn(&doc)
    );
}
