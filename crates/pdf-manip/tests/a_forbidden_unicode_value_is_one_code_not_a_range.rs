// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A bfrange destination is a base that increments, not one mapping.
//!
//! ISO 19005-2 §6.2.11.7.2 forbids a /ToUnicode destination of U+0000, the
//! byte order mark or a non-character, and `fix_tounicode_forbidden_values`
//! rewrites such a destination to U+FFFD. In a `beginbfrange` entry the third
//! token is not the destination of one code but the base of (hi - lo + 1) of
//! them, so writing U+FFFD there moves the whole range: mPDF and TCPDF declare
//! their fonts as `<0000> <FFFF> <0000>` (identity, CID = Unicode) and WRTC
//! writes `<00> <7F> <0000>` for ASCII, and after the repair code 0x62
//! extracted as U+FFFD + 0x62 = U+1005F. Every character of those documents
//! became a Linear B codepoint: 22 documents on the tuning 1000, 10 of them at
//! word *and* character retention 0.000, reproduced identically by mutool and
//! pdftotext. (#484.)
//!
//! What is forbidden is the mapping of one code, so the range is split around
//! that code and it keeps no mapping at all. An absent entry violates nothing,
//! and the codes involved -- U+0000, the surrogate block, U+FEFF, U+FFFE,
//! U+FFFF -- are not the ones a page draws.
//!
//! The second half of the same jump is #485: the identity range is also where
//! mPDF declares its space, so a document only reads correctly once both this
//! repair leaves the range intact *and* the word-separator reader sees it.
//! `the_space_of_an_identity_range_is_still_a_space` states that interlock.

use lopdf::{dictionary, Document, Object, Stream};

/// A font dictionary with nothing but the /ToUnicode under test.
///
/// `fix_tounicode_forbidden_values` collects every dictionary that has a
/// /Subtype and a /ToUnicode, so this is the whole fixture the pass needs.
fn document_with_tounicode(cmap_sections: &str) -> (Document, lopdf::ObjectId) {
    let mut doc = Document::with_version("1.7");
    let cmap = format!(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CMapName /Test def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
         {cmap_sections}endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n"
    );
    let stream_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        cmap.into_bytes(),
    )));
    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => "MPDFAA+DejaVuSerifCondensed",
        "Encoding" => Object::Name(b"Identity-H".to_vec()),
        "ToUnicode" => Object::Reference(stream_id),
    }));
    (doc, font_id)
}

/// The /ToUnicode of `font_id`, as text.
fn tounicode_of(doc: &Document, font_id: lopdf::ObjectId) -> String {
    let Ok(Object::Reference(id)) = doc.get_dictionary(font_id).unwrap().get(b"ToUnicode") else {
        panic!("the fixture lost its /ToUnicode");
    };
    let Some(Object::Stream(stream)) = doc.objects.get(id) else {
        panic!("the /ToUnicode is not a stream");
    };
    let mut stream = stream.clone();
    let _ = stream.decompress();
    String::from_utf8_lossy(&stream.content).into_owned()
}

/// What a reader gets for `code` out of a CMap: bfchar pairs and bfrange
/// entries in both destination forms, resolved the way the specification says.
///
/// Deliberately not the pipeline's own reader: this has to be what a reader
/// outside this repository sees, which is what mutool and pdftotext showed.
fn resolve(cmap: &str, code: u32) -> Option<String> {
    let mut tokens: Vec<String> = Vec::new();
    let mut rest = cmap;
    while let Some(open) = rest.find('<') {
        for word in rest[..open].split_whitespace() {
            for part in word.split_inclusive(['[', ']']) {
                tokens.push(part.to_string());
            }
        }
        let after = &rest[open + 1..];
        let Some(close) = after.find('>') else { break };
        tokens.push(format!(
            "<{}>",
            after[..close]
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<String>()
        ));
        rest = &after[close + 1..];
    }

    let hex =
        |t: &str| -> Option<String> { t.strip_prefix('<')?.strip_suffix('>').map(str::to_string) };
    let text = |h: &str| -> Option<String> {
        let units: Vec<u16> = h
            .as_bytes()
            .chunks(4)
            .map(|c| u16::from_str_radix(std::str::from_utf8(c).ok()?, 16).ok())
            .collect::<Option<_>>()?;
        String::from_utf16(&units).ok()
    };

    let mut section = "";
    let mut i = 0;
    while i < tokens.len() {
        let token = tokens[i].as_str();
        if token.contains("beginbfchar") || token.contains("beginbfrange") {
            section = if token.contains("bfchar") {
                "bfchar"
            } else {
                "bfrange"
            };
            i += 1;
            continue;
        }
        if token.contains("endbf") {
            section = "";
            i += 1;
            continue;
        }
        let Some(source) = hex(token) else {
            i += 1;
            continue;
        };
        let Ok(lo) = u32::from_str_radix(&source, 16) else {
            i += 1;
            continue;
        };
        if section == "bfchar" {
            let destination = tokens.get(i + 1).and_then(|t| hex(t));
            if lo == code {
                return destination.and_then(|d| text(&d));
            }
            i += 2;
            continue;
        }
        if section == "bfrange" {
            let Some(hi) = tokens.get(i + 1).and_then(|t| hex(t)) else {
                break;
            };
            let Ok(hi) = u32::from_str_radix(&hi, 16) else {
                break;
            };
            match tokens.get(i + 2).map(String::as_str) {
                Some("[") => {
                    let mut j = i + 3;
                    let mut offset = 0;
                    while let Some(destination) = tokens.get(j).and_then(|t| hex(t)) {
                        if lo + offset == code {
                            return text(&destination);
                        }
                        offset += 1;
                        j += 1;
                    }
                    i = j + 1;
                }
                Some(base) => {
                    let base = hex(base).unwrap_or_default();
                    if (lo..=hi).contains(&code) {
                        // The destination increments per code, which is the
                        // whole point of the entry -- and the defect: with a
                        // base of U+FFFD, code 0x41 resolves to U+1003E.
                        let mut base = text(&base)?;
                        let last = base.pop()?;
                        base.push(char::from_u32(last as u32 + (code - lo))?);
                        return Some(base);
                    }
                    i += 3;
                }
                None => break,
            }
            continue;
        }
        i += 1;
    }
    None
}

/// mPDF and TCPDF: identity over the whole plane, base U+0000.
///
/// The repair may not touch the codes the page actually draws. On master the
/// entry became `<0000> <FFFF> <FFFD>` and `A` came out as U+1003C.
#[test]
fn an_identity_range_keeps_every_code_it_maps() {
    let (mut doc, font) =
        document_with_tounicode("1 beginbfrange\n<0000> <FFFF> <0000>\nendbfrange\n");

    pdf_manip::pdfa_fonts::fix_tounicode_forbidden_values(&mut doc);

    let cmap = tounicode_of(&doc, font);
    assert_eq!(
        resolve(&cmap, 0x0041).as_deref(),
        Some("A"),
        "code 0x41 no longer extracts as 'A':\n{cmap}"
    );
    assert_eq!(
        resolve(&cmap, 0x00E9).as_deref(),
        Some("é"),
        "code 0xE9 no longer extracts as 'é':\n{cmap}"
    );
    assert_eq!(
        resolve(&cmap, 0x0000),
        None,
        "U+0000 is still written down; it may not be:\n{cmap}"
    );
}

/// WRTC: the ASCII half, base U+0000.
///
/// One forbidden code at the very start of the range, and the other 127 have
/// to survive it.
#[test]
fn an_ascii_range_loses_only_its_forbidden_code() {
    let (mut doc, font) = document_with_tounicode("1 beginbfrange\n<00> <7F> <0000>\nendbfrange\n");

    pdf_manip::pdfa_fonts::fix_tounicode_forbidden_values(&mut doc);

    let cmap = tounicode_of(&doc, font);
    for (code, expected) in [(0x20u32, " "), (0x41, "A"), (0x7F, "\u{007F}")] {
        assert_eq!(
            resolve(&cmap, code).as_deref(),
            Some(expected),
            "code {code:#04X} lost its mapping:\n{cmap}"
        );
    }
    assert_eq!(resolve(&cmap, 0x0000), None, "U+0000 survived:\n{cmap}");
}

/// The repair the pass exists for still happens, one code at a time.
///
/// A bfchar destination of U+0000 is a single mapping and becomes U+FFFD, and
/// a surrogate range keeps that treatment as well -- what changed is only that
/// a range is split rather than moved.
#[test]
fn a_single_forbidden_destination_still_becomes_the_replacement_character() {
    let (mut doc, font) = document_with_tounicode(
        "2 beginbfchar\n<0003> <0000>\n<0004> <D800>\nendbfchar\n\
         1 beginbfrange\n<0010> <0011> <FEFF>\nendbfrange\n",
    );

    pdf_manip::pdfa_fonts::fix_tounicode_forbidden_values(&mut doc);

    let cmap = tounicode_of(&doc, font);
    assert_eq!(
        resolve(&cmap, 0x0003).as_deref(),
        Some("\u{FFFD}"),
        "a bfchar destination of U+0000 was left alone:\n{cmap}"
    );
    assert_eq!(
        resolve(&cmap, 0x0004).as_deref(),
        Some("\u{FFFD}"),
        "a lone surrogate was left alone:\n{cmap}"
    );
    assert_eq!(
        resolve(&cmap, 0x0010),
        None,
        "a byte order mark is still written down:\n{cmap}"
    );
    assert_eq!(
        resolve(&cmap, 0x0011).as_deref(),
        Some("\u{FF00}"),
        "the code after the byte order mark lost the mapping of its own:\n{cmap}"
    );
}

/// The interlock with #485: mPDF's space lives in that same identity range.
///
/// `fix_tounicode_forbidden_values` runs before every `.notdef` pass, so what
/// it leaves behind is what the word-separator reader gets to see. Rewriting
/// the base moved the space out of reach: code 0x20 read as U+FFFD + 0x20, not
/// whitespace, and `fix_cid_font_notdef` then deleted every space on the page.
#[test]
fn the_space_of_an_identity_range_is_still_a_space() {
    let (mut doc, font) =
        document_with_tounicode("1 beginbfrange\n<0000> <FFFF> <0000>\nendbfrange\n");

    pdf_manip::pdfa_fonts::fix_tounicode_forbidden_values(&mut doc);

    let cmap = tounicode_of(&doc, font);
    assert_eq!(
        resolve(&cmap, 0x0020).as_deref(),
        Some(" "),
        "the word separator of an mPDF document is no longer declared:\n{cmap}"
    );
}
