// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! No pass writes a space over a character code that a later pass would have
//! resolved.
//!
//! The font dictionary of a substituted Symbol or Zapf Dingbats face comes out
//! right -- Annex D encoding, real widths, a `/ToUnicode` that says `pi` --
//! and the page is still blank, because the byte that asked for the glyph is
//! no longer in the content stream. `fix_symbolic_font_notdef_streams`
//! resolves every code through the `/Encoding` as it stands, and it used to
//! stand before `fix_classic_symbolic_base14_encoding` had restated it: every
//! code of the face still measured as `.notdef`, so `(\256)Tj` was written out
//! as `( )Tj` and the encoding repair arrived at a page with nothing left to
//! draw. A content rewrite that removes what it cannot yet resolve is data
//! loss no validator ever sees -- veraPDF reads a conformant file either way.
//!
//! Measured on govdocs 002_002183 (right arrow x5, `pi tau plusminus
//! approxequal`), 000_000153 (a dingbat 309 times) and 001_001799 (micro).

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, Stream, StringFormat};

/// An unembedded classic symbolic face, drawing `text`.
///
/// `indirect_differences` writes the `/Differences` array as its own object,
/// which is what the producer of 000_000153 did.
fn doc_with(
    base_font: &str,
    differences: Option<Vec<Object>>,
    indirect_differences: bool,
    text: &[u8],
) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");

    let mut font = dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => Object::Name(base_font.as_bytes().to_vec()),
    };
    if let Some(differences) = differences {
        let diffs = if indirect_differences {
            Object::Reference(doc.add_object(Object::Array(differences)))
        } else {
            Object::Array(differences)
        };
        let enc_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Encoding",
            "Differences" => diffs,
        }));
        font.set("Encoding", Object::Reference(enc_id));
    }
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
    let diffs = match enc.get(b"Differences") {
        Ok(Object::Array(a)) => a.clone(),
        Ok(Object::Reference(r)) => doc
            .get_object(*r)
            .and_then(|o| o.as_array())
            .expect("/Differences is not an array")
            .clone(),
        _ => panic!("the output's /Encoding has no /Differences"),
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

/// govdocs 002_002183: `(\256)Tj` in, `( )Tj` out, on a font whose dictionary
/// the conversion had by then made entirely right.
#[test]
fn the_byte_that_asks_for_a_glyph_stays_on_the_page() {
    let doc = convert(&doc_with(
        "Symbol",
        Some(vec![Object::Integer(240), Object::Name(b"apple".to_vec())]),
        false,
        b"\xAE\xB1\x70\x74\xBB",
    ));

    assert_eq!(
        drawn_bytes(&doc),
        b"\xAE\xB1\x70\x74\xBB",
        "the content stream must still draw the codes the source drew; \
         0x20 in their place is the .notdef repair having run against an \
         encoding that had not been restated yet"
    );

    // The half that already worked, asserted here so a regression cannot be
    // mistaken for this one: the encoding really does resolve those codes.
    let font = the_font(&doc);
    assert_eq!(
        encoded_names(&doc, &font).get(&0xAE).map(String::as_str),
        Some("arrowright")
    );
}

/// govdocs 000_000153: `/Encoding 403 0 R` -> `/Differences 404 0 R`.
///
/// An encoding dictionary may hold its array as an indirect object. Reading
/// only the direct form made the restatement take the "the author says nothing
/// about this face" exit, so the face kept the substitute's StandardEncoding,
/// every width stayed at `.notdef`'s 278, and the content pass then blanked
/// the codes -- the whole defect, on a document whose only difference from the
/// one above is where the array was written.
#[test]
fn an_encoding_array_written_as_its_own_object_is_still_read() {
    let doc = convert(&doc_with(
        "ZapfDingbats",
        Some(vec![Object::Integer(127), Object::Name(b"space".to_vec())]),
        true,
        b"\x31\x6E",
    ));
    let font = the_font(&doc);

    assert_eq!(
        encoded_names(&doc, &font).get(&0x31).map(String::as_str),
        Some("a17"),
        "Annex D.6 puts `a17` on code 0x31; an indirect /Differences is not a \
         reason to leave the face on StandardEncoding"
    );
    assert_ne!(
        width_for(&doc, &font, 0x31),
        278,
        "278 is `.notdef`'s advance in the bundled Zapf Dingbats program"
    );
    assert_eq!(
        drawn_bytes(&doc),
        b"\x31\x6E",
        "the codes must survive to the page"
    );
    assert!(
        maps(&to_unicode(&doc, &font), 0x31, 0x2711),
        "code 0x31 draws `a17`, which the glyph list calls U+2711"
    );
}

/// govdocs 001_001799: `/BaseFont /Symbol,Bold`.
///
/// Neither Symbol nor Zapf Dingbats was ever cut bold or italic, so the style
/// heuristic that answers Helvetica-Bold for anything containing `bold` was
/// answering with a different alphabet: code 0x6D draws `mu` and came out as
/// `u`, on Arial's widths. Approximating a style is what the heuristic is for;
/// approximating an alphabet is not.
#[test]
fn a_styled_symbolic_face_keeps_its_alphabet() {
    let doc = convert(&doc_with("Symbol,Bold", None, false, b"\x6D\x70"));
    let font = the_font(&doc);

    let cmap = to_unicode(&doc, &font);
    // The glyph list calls Symbol's `mu` U+00B5 (micro sign); U+03BC is the
    // Greek letter and reads the same. Either is the character the page
    // draws. `u` is not.
    assert!(
        maps(&cmap, 0x6D, 0x00B5) || maps(&cmap, 0x6D, 0x03BC),
        "code 0x6D draws `mu` and must extract as one, not as `u`:\n{cmap}"
    );
    assert!(
        maps(&cmap, 0x70, 0x03C0),
        "code 0x70 draws `pi` and must extract as U+03C0:\n{cmap}"
    );
    assert_eq!(
        width_for(&doc, &font, 0x6D),
        576,
        "`mu` is 576 in Symbol; 556 is Helvetica-Bold's `m`, and a line set on \
         the wrong widths reflows"
    );
}

/// The guard against fixing the one above by matching `symbol` anywhere.
/// A typeface whose name merely contains the word is a Latin text face and
/// must keep going through the style heuristic.
#[test]
fn a_latin_face_named_after_a_symbol_is_not_a_symbolic_face() {
    let doc = convert(&doc_with("SymbolicLogoSans,Bold", None, false, b"AB"));
    let font = the_font(&doc);
    // Helvetica-Bold's advances for `A` and `B`. Symbol has no `A`, so a
    // Symbol substitute here would leave both codes unresolved.
    assert_eq!(width_for(&doc, &font, 0x41), 722);
    assert_eq!(width_for(&doc, &font, 0x42), 722);
}
