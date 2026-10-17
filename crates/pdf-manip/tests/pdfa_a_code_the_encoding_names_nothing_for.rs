// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A code the encoding names nothing for still has to resolve to a glyph.
//!
//! ISO 19005-2 6.2.11.8:1 forbids a text-showing operator from reaching the
//! `.notdef` glyph. A producer that emits `\177` -- DEL, which no ordinary text
//! encoding names -- on a MacRoman-encoded face hands us exactly that: the base
//! encoding names nothing for the code, so nothing can be drawn for it, so
//! veraPDF reads `.notdef`.
//!
//! Neither repair written for that rule reaches it. `fix_symbolic_font_notdef_streams`
//! wants a symbolic face and `fix_type1_subset_missing_glyphs` wants a subset;
//! this is neither. The one pass that would have -- `fix_notdef_glyph_refs` --
//! skipped its whole second phase for any non-subset CFF declaring
//! MacRomanEncoding, a guard written against the MacRoman *high* bytes, where a
//! fresh `/Differences` entry replaces a working internal CFF mapping with a
//! false space and then a width mismatch. The codes MacRoman leaves unnamed are
//! the opposite case: there is no mapping to lose.
//!
//! Measured on govdocs 000_000153, page 8, resource font `NewYork`, code 127,
//! one operator on one page -- the one document behind every `299 of 300` in
//! the PDF/A series (#489, parent #481).

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, Stream, StringFormat};

/// An unembedded, non-symbolic MacRoman TrueType face, drawing `text`.
///
/// `NewYork` is a Mac serif nobody carries: it goes through the ordinary
/// substitute path and comes back as a bundled Standard 14 face, which is a
/// CFF program in a `/Type1` dictionary -- the shape the guard covered.
fn doc_drawing(text: &[u8]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");

    let descriptor_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => Object::Name(b"NewYork".to_vec()),
        // Serif, non-symbolic, ForceBold -- the flags 000_000153 carries.
        "Flags" => 32802_i64,
        "ItalicAngle" => 0_i64,
        "Ascent" => 750_i64,
        "Descent" => -250_i64,
        "CapHeight" => 700_i64,
        "StemV" => 80_i64,
        "FontBBox" => vec![(-100).into(), (-250).into(), 1000.into(), 900.into()],
    }));
    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "TrueType",
        "BaseFont" => Object::Name(b"NewYork".to_vec()),
        "FirstChar" => 0_i64,
        "LastChar" => 255_i64,
        "Widths" => Object::Array(vec![Object::Integer(500); 256]),
        "Encoding" => Object::Name(b"MacRomanEncoding".to_vec()),
        "FontDescriptor" => Object::Reference(descriptor_id),
    }));

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

fn the_substituted_face(doc: &Document) -> Dictionary {
    doc.objects
        .values()
        .filter_map(|o| o.as_dict().ok())
        .find(|d| {
            d.get(b"Type").and_then(|o| o.as_name()).ok() == Some(b"Font")
                && d.get(b"BaseFont")
                    .and_then(|o| o.as_name())
                    .map(|n| n.ends_with(b"NewYork"))
                    .unwrap_or(false)
        })
        .expect("no NewYork font dictionary in the output")
        .clone()
}

/// Code → glyph name, as the output's `/Encoding` states it.
fn encoded_names(doc: &Document, font: &Dictionary) -> std::collections::BTreeMap<u8, String> {
    let enc = match font.get(b"Encoding") {
        Ok(Object::Dictionary(d)) => d.clone(),
        Ok(Object::Reference(r)) => match doc.get_object(*r) {
            Ok(Object::Dictionary(d)) => d.clone(),
            _ => return std::collections::BTreeMap::new(),
        },
        // A bare name carries no per-code statement at all.
        _ => return std::collections::BTreeMap::new(),
    };
    let diffs = match enc.get(b"Differences") {
        Ok(Object::Array(a)) => a.clone(),
        Ok(Object::Reference(r)) => doc
            .get_object(*r)
            .and_then(|o| o.as_array())
            .expect("/Differences is not an array")
            .clone(),
        _ => return std::collections::BTreeMap::new(),
    };
    let mut out = std::collections::BTreeMap::new();
    let mut code: u8 = 0;
    for item in &diffs {
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

/// The glyph names the embedded program actually carries.
fn embedded_glyph_names(doc: &Document, font: &Dictionary) -> Vec<String> {
    let Ok(Object::Reference(fd_id)) = font.get(b"FontDescriptor") else {
        panic!("the substituted face has no /FontDescriptor");
    };
    let fd = doc
        .get_object(*fd_id)
        .and_then(|o| o.as_dict())
        .expect("/FontDescriptor is not a dictionary");
    let program_id = ["FontFile3", "FontFile2", "FontFile"]
        .iter()
        .find_map(|key| match fd.get(key.as_bytes()) {
            Ok(Object::Reference(r)) => Some(*r),
            _ => None,
        })
        .expect("the substituted face embeds no program");
    let Some(Object::Stream(stream)) = doc.objects.get(&program_id) else {
        panic!("the embedded program is not a stream");
    };
    let mut stream = stream.clone();
    let _ = stream.decompress();
    let table = cff_parser::Table::parse(&stream.content).expect("embedded program is not CFF");
    (0..table.number_of_glyphs())
        .filter_map(|gid| {
            table
                .glyph_name(cff_parser::GlyphId(gid))
                .map(|n| n.to_string())
        })
        .collect()
}

/// The bytes of every `Tj` string on the page, concatenated.
fn drawn_bytes(doc: &Document) -> Vec<u8> {
    let mut out = Vec::new();
    for page_id in doc.get_pages().values() {
        for cs_id in pdf_manip::content_editor::get_content_stream_ids(doc, *page_id) {
            let Some(Object::Stream(s)) = doc.objects.get(&cs_id) else {
                continue;
            };
            let mut s = s.clone();
            let _ = s.decompress();
            let editor = pdf_manip::content_editor::ContentEditor::from_stream(&s.content)
                .expect("parse the converted content stream");
            for op in editor.operations() {
                if op.operator != "Tj" {
                    continue;
                }
                if let Some(Object::String(bytes, _)) = op.operands.first() {
                    out.extend_from_slice(bytes);
                }
            }
        }
    }
    out
}

/// The defect. Without the fix code 127 carries no statement at all, and
/// veraPDF answers 6.2.11.8:1 on the operator that draws it.
#[test]
fn a_drawn_code_the_encoding_names_nothing_for_resolves_to_a_glyph_the_program_has() {
    let doc = convert(&doc_drawing(b"Chapter\x7f one"));
    let font = the_substituted_face(&doc);
    let names = encoded_names(&doc, &font);

    let name = names.get(&127).unwrap_or_else(|| {
        panic!("code 127 is drawn and the encoding still names nothing for it: {names:?}")
    });
    assert!(
        embedded_glyph_names(&doc, &font).contains(name),
        "code 127 was pointed at /{name}, which the embedded program does not carry"
    );
}

/// The text around it is untouched: this is an encoding statement, not a
/// content rewrite. #481 is the ticket about what a content rewrite costs.
#[test]
fn the_page_still_draws_every_byte_it_drew_before() {
    let doc = convert(&doc_drawing(b"Chapter\x7f one"));
    assert_eq!(
        drawn_bytes(&doc),
        b"Chapter\x7f one".to_vec(),
        "the repair rewrote the page instead of the encoding"
    );
}

/// And it stays narrow. A code the encoding names, whose glyph the substitute
/// carries, draws that glyph -- it is not swept into the same repair.
#[test]
fn a_code_that_resolves_keeps_its_own_glyph() {
    let doc = convert(&doc_drawing(b"Chapter\x7f one"));
    let font = the_substituted_face(&doc);
    let names = encoded_names(&doc, &font);
    for (code, expected) in [(b'C', "C"), (b'h', "h"), (b' ', "space")] {
        if let Some(name) = names.get(&code) {
            assert_eq!(
                name, expected,
                "code {code} resolves to /{expected}; the output says /{name}"
            );
        }
    }
}
