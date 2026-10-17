//! Text from glyph names when a simple font has no usable `/ToUnicode` value
//! (spec `specs/text/glyph-names.md`, C2+C3).
//!
//! Two rules, held here end to end through `extract_text`:
//!
//! - A `/ToUnicode` value made only of control characters (U+0000-U+001F,
//!   U+007F-U+009F) is no text. The glyph name the font's `/Encoding` gives
//!   the code is asked instead; any other value still wins.
//! - A name the Adobe Glyph List does not know is looked up in the small AMS
//!   math-font table (`squaredot`, `square`, `dblarrowup`, `dblarrowdwn`,
//!   `triangle`, since C4 `circlecopyrt`, since C5a `negationslash`, since
//!   C5c `angbracketleft` and `angbracketright`) --
//!   after every AGL rule, never before.
//!
//! A name neither table knows leaves the code exactly as it was.
//!
//! Every fixture is built here: a non-embedded `Type1` font standing on
//! Helvetica, or a `Type3` font with drawn glyphs, each with an
//! `/Encoding /Differences` array. Text is compared with whitespace removed:
//! these tests are about which character a code becomes, not about where
//! the extractor puts word gaps.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, Stream};
use pdf_engine::PdfDocument;

/// `<codes> Tj` inside `BT`/`ET`, one byte per code.
fn content(codes: &[u8]) -> Vec<u8> {
    let ops = vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 24.into()]),
        Operation::new("Td", vec![72.into(), 700.into()]),
        Operation::new(
            "Tj",
            vec![Object::String(
                codes.to_vec(),
                lopdf::StringFormat::Hexadecimal,
            )],
        ),
        Operation::new("ET", vec![]),
    ];
    Content { operations: ops }
        .encode()
        .expect("encode content")
}

/// A one-page document drawing `codes` in `font`, saved to bytes.
fn page_with_font(mut doc: Document, font: Dictionary, codes: &[u8]) -> Vec<u8> {
    let font_id = doc.add_object(Object::Dictionary(font));
    let stream_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content(codes))));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
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

/// The page's text with every whitespace character removed.
fn text_of(bytes: Vec<u8>) -> String {
    PdfDocument::open(bytes)
        .expect("open pdf")
        .extract_text(0)
        .expect("extract text")
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

/// `/Differences [first /name1 /name2 ...]`.
fn differences(first: i64, names: &[&str]) -> Object {
    let mut array = vec![Object::Integer(first)];
    array.extend(names.iter().map(|n| Object::Name(n.as_bytes().to_vec())));
    Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Encoding".to_vec()),
        "Differences" => Object::Array(array),
    })
}

/// A one-byte `/ToUnicode` CMap with one `bfchar` line per `(code, value)`.
fn to_unicode(pairs: &[(u8, u32)]) -> Vec<u8> {
    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin\n\
         12 dict begin\nbegincmap\n/CMapType 2 def\n\
         1 begincodespacerange\n<00> <FF>\nendcodespacerange\n",
    );
    cmap.push_str(&format!("{} beginbfchar\n", pairs.len()));
    for (code, value) in pairs {
        cmap.push_str(&format!("<{code:02X}> <{value:04X}>\n"));
    }
    cmap.push_str("endbfchar\nendcmap\nend end\n");
    cmap.into_bytes()
}

/// A non-embedded `Type1` font on Helvetica whose `/Differences` names the
/// codes from `first` on, with a `/ToUnicode` map when `pairs` is not empty.
fn type1_font(doc: &mut Document, first: i64, names: &[&str], pairs: &[(u8, u32)]) -> Dictionary {
    let mut font = dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        "FirstChar" => Object::Integer(0),
        "LastChar" => Object::Integer(255),
        "Widths" => Object::Array(vec![Object::Integer(600); 256]),
        "Encoding" => differences(first, names),
    };
    if !pairs.is_empty() {
        let id = doc.add_object(Object::Stream(Stream::new(
            dictionary! {},
            to_unicode(pairs),
        )));
        font.set("ToUnicode", Object::Reference(id));
    }
    font
}

/// A `Type3` font whose `/Differences` names the codes from `first` on, each
/// glyph a filled square, with a `/ToUnicode` map when `pairs` is not empty.
fn type3_font(doc: &mut Document, first: i64, names: &[&str], pairs: &[(u8, u32)]) -> Dictionary {
    let mut procs = Dictionary::new();
    for name in names {
        let id = doc.add_object(Object::Stream(Stream::new(
            dictionary! {},
            b"1000 0 d0 0 0 750 750 re f".to_vec(),
        )));
        procs.set(name.as_bytes().to_vec(), Object::Reference(id));
    }
    let mut font = dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type3".to_vec()),
        "FontBBox" => Object::Array(vec![0.into(), 0.into(), 750.into(), 750.into()]),
        "FontMatrix" => Object::Array(vec![
            Object::Real(0.001), 0.into(), 0.into(), Object::Real(0.001), 0.into(), 0.into(),
        ]),
        "CharProcs" => Object::Dictionary(procs),
        "Encoding" => differences(first, names),
        "FirstChar" => Object::Integer(first),
        "LastChar" => Object::Integer(first + names.len() as i64 - 1),
        "Widths" => Object::Array(vec![Object::Integer(1000); names.len()]),
        "Resources" => Object::Dictionary(dictionary! {}),
    };
    if !pairs.is_empty() {
        let id = doc.add_object(Object::Stream(Stream::new(
            dictionary! {},
            to_unicode(pairs),
        )));
        font.set("ToUnicode", Object::Reference(id));
    }
    font
}

/// A `Type1` font whose `/ToUnicode` sends code 3 to U+0003: the value is a
/// control character, so the `/Differences` name `square` speaks instead.
/// Without the rule the control character is the text.
#[test]
fn a_control_character_tounicode_value_falls_back_to_the_name() {
    let mut doc = Document::with_version("1.7");
    let font = type1_font(&mut doc, 3, &["square"], &[(3, 0x0003)]);
    assert_eq!(text_of(page_with_font(doc, font, &[3])), "\u{25A1}");
}

/// The same font, but the value is `X`: an ordinary value wins over any
/// name, exactly as before.
#[test]
fn an_ordinary_tounicode_value_still_wins() {
    let mut doc = Document::with_version("1.7");
    let font = type1_font(&mut doc, 3, &["square"], &[(3, 0x0058)]);
    assert_eq!(text_of(page_with_font(doc, font, &[3])), "X");
}

/// Each of the five phase-1 AMS rows, through a `Type1` font with no
/// `/ToUnicode`.
#[test]
fn every_ams_name_reads_as_its_character() {
    let rows = [
        ("squaredot", '\u{22A1}'),
        ("square", '\u{25A1}'),
        ("dblarrowup", '\u{21C8}'),
        ("dblarrowdwn", '\u{21CA}'),
        ("triangle", '\u{25B3}'),
    ];
    for (name, expected) in rows {
        let mut doc = Document::with_version("1.7");
        let font = type1_font(&mut doc, 65, &[name], &[]);
        assert_eq!(
            text_of(page_with_font(doc, font, &[65])),
            expected.to_string(),
            "/{name}"
        );
    }
}

/// The phase-2 row `circlecopyrt` (`circlecopyrt.md`, C4) reads as U+20DD
/// only after a glyph it encloses (`enclosing_circle_follows_its_glyph.rs`,
/// which also holds the test that fails when the row is removed). Alone it is
/// never a lone mark: the code reads as it did before the row, exactly like a
/// name no table knows (rule 3).
#[test]
fn a_lone_circlecopyrt_reads_as_before_its_row() {
    let read = |name: &str| {
        let mut doc = Document::with_version("1.7");
        let font = type1_font(&mut doc, 65, &[name], &[]);
        text_of(page_with_font(doc, font, &[65]))
    };
    let text = read("circlecopyrt");
    assert!(!text.contains('\u{20DD}'), "{text:?}");
    assert_eq!(text, read("zzunlisted"));
}

/// The phase-3 row `negationslash` (C5a) likewise: U+0338 only after the
/// glyph it strikes, and alone the code reads as before the row.
#[test]
fn a_lone_negationslash_reads_as_before_its_row() {
    let read = |name: &str| {
        let mut doc = Document::with_version("1.7");
        let font = type1_font(&mut doc, 65, &[name], &[]);
        text_of(page_with_font(doc, font, &[65]))
    };
    let text = read("negationslash");
    assert!(!text.contains('\u{0338}'), "{text:?}");
    assert_eq!(text, read("zzunlisted"));
}

/// The phase-4 rows (C5c) are plain characters: `angbracketleft` and
/// `angbracketright` read as U+27E8 and U+27E9 where they stand, alone and
/// between other glyphs. A `/ToUnicode` value still wins over the row.
#[test]
fn the_angle_bracket_rows_read_as_the_mathematical_brackets() {
    let read = |codes: &[u8], pairs: &[(u8, u32)]| {
        let mut doc = Document::with_version("1.7");
        let font = type1_font(&mut doc, 104, &["angbracketleft", "angbracketright"], pairs);
        text_of(page_with_font(doc, font, codes))
    };
    assert_eq!(read(&[104], &[]), "\u{27E8}");
    assert_eq!(read(&[105], &[]), "\u{27E9}");
    assert_eq!(read(&[104, 65, 105], &[]), "\u{27E8}A\u{27E9}");
    assert_eq!(read(&[104], &[(104, 0x003C)]), "<");
}

/// The phase-5 row (C5b) is a plain character: `visiblespace`, as `cmtt10`
/// names it, reads as U+2423 where it stands, alone and between other
/// glyphs. A `/ToUnicode` value still wins over the row.
#[test]
fn the_visiblespace_row_reads_as_the_open_box() {
    let read = |codes: &[u8], pairs: &[(u8, u32)]| {
        let mut doc = Document::with_version("1.7");
        let font = type1_font(&mut doc, 104, &["visiblespace"], pairs);
        text_of(page_with_font(doc, font, codes))
    };
    assert_eq!(read(&[104], &[]), "\u{2423}");
    assert_eq!(read(&[65, 104, 66], &[]), "A\u{2423}B");
    assert_eq!(read(&[104], &[(104, 0x005F)]), "_");
}

/// An AGL name still reads through the AGL, here behind a control-character
/// `/ToUnicode` value: `Aring` is `Å`.
#[test]
fn an_agl_name_reads_through_the_agl() {
    let mut doc = Document::with_version("1.7");
    let font = type1_font(&mut doc, 3, &["Aring"], &[(3, 0x0003)]);
    assert_eq!(text_of(page_with_font(doc, font, &[3])), "\u{00C5}");
}

/// A name neither table knows leaves the code as it was: code `A` mapped to
/// U+0001 by `/ToUnicode` and named `striangle` keeps the U+0001 value. Had
/// the value been dropped, the standard-encoding guess would read the code
/// as `A`.
#[test]
fn an_unknown_name_leaves_the_code_unchanged() {
    let mut doc = Document::with_version("1.7");
    let font = type1_font(&mut doc, 65, &["striangle"], &[(65, 0x0001)]);
    let text = text_of(page_with_font(doc, font, &[65]));
    assert!(
        !text.contains('A'),
        "an unknown name was guessed at: {text:?}"
    );
}

/// The `Type3` case of the gate files: `/Differences [97 /rect /triangle]`
/// with no `/ToUnicode`. `triangle` reads as `△`; `rect`, which no table
/// knows, keeps its old reading `a`.
#[test]
fn a_type3_font_reads_an_ams_name() {
    let mut doc = Document::with_version("1.7");
    let font = type3_font(&mut doc, 97, &["rect", "triangle"], &[]);
    assert_eq!(text_of(page_with_font(doc, font, &[97, 98])), "a\u{25B3}");
}

/// A `Type3` font whose `/ToUnicode` gives a control character: the name
/// `dblarrowup` speaks instead.
#[test]
fn a_type3_control_character_value_falls_back_to_the_name() {
    let mut doc = Document::with_version("1.7");
    let font = type3_font(&mut doc, 20, &["dblarrowup"], &[(20, 0x0014)]);
    assert_eq!(text_of(page_with_font(doc, font, &[20])), "\u{21C8}");
}
