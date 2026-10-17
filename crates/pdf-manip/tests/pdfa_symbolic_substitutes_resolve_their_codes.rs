// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A substituted Symbol or Zapf Dingbats face resolves its codes through
//! Annex D, whatever small `/Differences` the author wrote on top.
//!
//! These two faces carry their own encoding: ISO 32000-1 Annex D.5 and D.6 fix
//! what every code draws, and the bundled substitute's CFF program declares
//! StandardEncoding, under which none of those codes names a glyph it has.
//! An `/Encoding` dictionary whose whole content is `[240 /apple]` used to be
//! read as "the author has this covered", so the other 255 codes stayed on
//! StandardEncoding, resolved to nothing, and `/Widths` came out as the
//! `.notdef` advance for all of them -- 250 for Symbol, 278 for Zapf Dingbats.
//! Readers then drew nothing and extracted nothing.
//!
//! Measured on govdocs: 002_002183 lost the right arrow five times and
//! `pi tau plusminus approxequal` entirely; 000_000153 lost a dingbat 309
//! times (page 46 render MAE 0.0128, dark pixels 2366 -> 2036); 001_001799
//! turned micro into `u`.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, Stream, StringFormat};
use pdf_standard_fonts::StandardFont;

/// An unembedded classic symbolic face with an author `/Differences` that
/// speaks for one code and says nothing about the rest -- the shape of
/// govdocs 002_002183 and 000_000153.
fn symbolic_font(base_font: &str, differences: Vec<Object>) -> (Dictionary, Dictionary) {
    (
        dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => Object::Name(base_font.as_bytes().to_vec()),
        },
        dictionary! {
            "Type" => "Encoding",
            "Differences" => differences,
        },
    )
}

fn doc_with(font: Dictionary, encoding: Dictionary, text: &[u8]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let enc_id = doc.add_object(Object::Dictionary(encoding));
    let mut font = font;
    // An indirect encoding dictionary, which is what producers write and what
    // the pass used to walk straight past.
    font.set("Encoding", Object::Reference(enc_id));
    let font_id = doc.add_object(Object::Dictionary(font));

    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Td", vec![72.into(), 700.into()]),
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
        "Resources" => dictionary! {
            "Font" => dictionary! { "F1" => Object::Reference(font_id) },
        },
    }));
    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Pages",
        "Kids" => vec![Object::Reference(page_id)],
        "Count" => 1_i64,
    }));
    if let Ok(Object::Dictionary(d)) = doc.get_object_mut(page_id) {
        d.set("Parent", Object::Reference(pages_id));
    }
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));

    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save fixture");
    out
}

fn convert(source: &[u8]) -> Document {
    let opts = pdf_manip::pdfa::PdfAConvertOptions {
        conformance: pdf_manip::pdfa_xmp::PdfAConformance::A2b,
        ..Default::default()
    };
    let out = pdf_manip::pdfa::convert_bytes(source, &opts).expect("PDF/A conversion");
    Document::load_mem(&out).expect("load converted output")
}

fn the_font(doc: &Document) -> Dictionary {
    doc.objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .find(|d| d.get(b"Type").and_then(|o| o.as_name()).ok() == Some(b"Font"))
        .expect("no font dictionary in the output")
        .clone()
}

fn to_unicode(doc: &Document, font: &Dictionary) -> String {
    let Ok(Object::Reference(r)) = font.get(b"ToUnicode") else {
        panic!("font has no /ToUnicode; extraction has nothing to read");
    };
    let Some(Object::Stream(s)) = doc.objects.get(r) else {
        panic!("/ToUnicode is not a stream");
    };
    let mut s = s.clone();
    let _ = s.decompress();
    String::from_utf8_lossy(&s.content).into_owned()
}

fn maps(cmap: &str, code: u8, unicode: u16) -> bool {
    let cmap = cmap.to_uppercase();
    cmap.contains(&format!("<{code:02X}> <{unicode:04X}>"))
        || cmap.contains(&format!("<{code:02X}><{unicode:04X}>"))
}

fn width_for(doc: &Document, font: &Dictionary, code: usize) -> i64 {
    let first = font
        .get(b"FirstChar")
        .and_then(|o| o.as_i64())
        .expect("no /FirstChar") as usize;
    let widths = match font.get(b"Widths") {
        Ok(Object::Array(a)) => a.clone(),
        Ok(Object::Reference(r)) => doc
            .get_object(*r)
            .and_then(|o| o.as_array())
            .expect("/Widths is not an array")
            .clone(),
        _ => panic!("font has no /Widths"),
    };
    widths
        .get(code - first)
        .and_then(|o| o.as_i64().ok())
        .unwrap_or_else(|| panic!("no width for code {code}"))
}

/// The glyph names the output's `/Encoding /Differences` assigns.
fn encoded_names(doc: &Document, font: &Dictionary) -> std::collections::BTreeMap<u8, String> {
    let enc = match font.get(b"Encoding") {
        Ok(Object::Dictionary(d)) => d.clone(),
        Ok(Object::Reference(r)) => doc
            .get_object(*r)
            .and_then(|o| o.as_dict())
            .expect("/Encoding is not a dictionary")
            .clone(),
        _ => panic!("the output declares no /Encoding for a symbolic face"),
    };
    let Ok(Object::Array(diffs)) = enc.get(b"Differences") else {
        panic!("the output's /Encoding has no /Differences");
    };
    let mut out = std::collections::BTreeMap::new();
    let mut code: u8 = 0;
    for item in diffs {
        match item {
            Object::Integer(n) => code = (*n).clamp(0, 255) as u8,
            Object::Name(n) => {
                out.insert(code, String::from_utf8_lossy(n).into_owned());
                code = code.wrapping_add(1);
            }
            _ => {}
        }
    }
    out
}

/// govdocs 002_002183: the right arrow five times, and four symbols gone.
#[test]
fn symbol_codes_reach_the_glyphs_annex_d_names() {
    let (font, encoding) = symbolic_font(
        "Symbol",
        vec![Object::Integer(240), Object::Name(b"apple".to_vec())],
    );
    // arrowright, plusminus, pi, tau, approxequal.
    let doc = convert(&doc_with(font, encoding, b"\xAE\xB1\x70\x74\xBB"));
    let font = the_font(&doc);

    let names = encoded_names(&doc, &font);
    assert_eq!(
        names.get(&0xAE).map(String::as_str),
        Some("arrowright"),
        "Annex D.5 puts `arrowright` on code 174; the encoding says {names:?}"
    );

    // 6.2.11.5 compares /Widths against the advance of the glyph the encoding
    // names. `.notdef` is 250 in this program, and a reader draws the width it
    // is given, so an unresolved code both draws nothing and takes no space.
    for (code, name) in [(0xAEusize, "arrowright"), (0xB1, "plusminus"), (0x70, "pi")] {
        let width = width_for(&doc, &font, code);
        assert_ne!(
            width, 250,
            "code {code:#04X} draws `{name}`; a width of 250 is `.notdef`'s, \
             which is what every one of the 256 codes used to get"
        );
    }
    assert_eq!(
        width_for(&doc, &font, 0xAE),
        987,
        "`arrowright` is 987 in the bundled Symbol program"
    );

    let cmap = to_unicode(&doc, &font);
    for (code, unicode, glyph) in [
        (0xAEu8, 0x2192u16, "arrowright"),
        (0xB1, 0x00B1, "plusminus"),
        (0x70, 0x03C0, "pi"),
        (0x74, 0x03C4, "tau"),
        (0xBB, 0x2248, "approxequal"),
    ] {
        assert!(
            maps(&cmap, code, unicode),
            "code {code:#04X} draws `{glyph}` and must extract as one:\n{cmap}"
        );
    }
}

/// govdocs 000_000153: a dingbat drawn 309 times, gone from page and text.
#[test]
fn dingbat_codes_reach_the_glyphs_annex_d_names() {
    let (font, encoding) = symbolic_font(
        "ZapfDingbats",
        vec![Object::Integer(127), Object::Name(b"space".to_vec())],
    );
    // a17 (the pencil, U+2711) and a73 (the filled square).
    let doc = convert(&doc_with(font, encoding, b"\x31\x6E"));
    let font = the_font(&doc);

    let names = encoded_names(&doc, &font);
    assert_eq!(names.get(&0x31).map(String::as_str), Some("a17"));

    for code in [0x31usize, 0x6E] {
        assert_ne!(
            width_for(&doc, &font, code),
            278,
            "278 is `.notdef`'s advance in the bundled Zapf Dingbats program"
        );
    }

    let cmap = to_unicode(&doc, &font);
    assert!(
        maps(&cmap, 0x31, 0x2711),
        "code 0x31 draws `a17`, which the glyph list calls U+2711:\n{cmap}"
    );
    assert!(
        maps(&cmap, 0x6E, 0x25A0),
        "code 0x6E draws `a73`, which the glyph list calls U+25A0:\n{cmap}"
    );
}

/// The author still has the last word. `/Differences` is them writing about
/// this face by hand, so it survives the restatement rather than being
/// overwritten by Annex D.
#[test]
fn the_authors_own_differences_survive() {
    let (font, encoding) = symbolic_font(
        "Symbol",
        vec![Object::Integer(240), Object::Name(b"apple".to_vec())],
    );
    let doc = convert(&doc_with(font, encoding, b"\xF0"));
    let font = the_font(&doc);

    assert_eq!(
        encoded_names(&doc, &font).get(&0xF0).map(String::as_str),
        Some("apple"),
        "code 240 is undefined in Annex D.5 and the author named it"
    );
    assert!(
        maps(&to_unicode(&doc, &font), 0xF0, 0xF8FF),
        "`apple` is U+F8FF in the glyph list, not the byte 0xF0"
    );
}

/// The guard against fixing this by resolving `aNN` names everywhere. Zapf
/// Dingbats calls its glyphs `a1` and `a12`; a Latin font uses those names for
/// variants of `a`, and turning one into a pair of scissors is the same class
/// of error as the one being fixed.
#[test]
fn dingbat_names_stay_on_the_dingbat_face() {
    assert_eq!(
        StandardFont::ZapfDingbats.glyph_name_to_char("a1"),
        Some('\u{2701}')
    );
    assert_eq!(StandardFont::Helvetica.glyph_name_to_char("a1"), None);
    assert_eq!(StandardFont::Symbol.glyph_name_to_char("a1"), None);
}
