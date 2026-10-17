// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A CID is not a glyph index in a face we chose ourselves (#511).
//!
//! `83066e4f6` gives a composite descendant an SFNT substitute, because a bare
//! `/FontFile3 /Subtype /Type1C` on a descendant is not a program veraPDF can
//! read. Under a *predefined* CMap that substitute arrived with
//! `/CIDToGIDMap /Identity`, which claims the Adobe-Japan1 CID the page draws
//! is a glyph index in LiberationSans. It is not: `(57)【要約】` of
//! `pdfjs__test_pdfs_issue13343.pdf` came out as four accent marks, and the
//! seven hanzi of `pdfjs__test_pdfs_issue3521.pdf` as `D A` -- pixels that are
//! not the document's, on two files that conformed no better for them.
//!
//! What relates the two is the character: Adobe publishes `Adobe-Japan1-UCS2`,
//! `pdf-font` carries it, and through it CID 239 is `(` and the substitute's
//! own `cmap` says which glyph draws that. A CID whose character the substitute
//! has no glyph for is mapped to GID 0 and draws `.notdef`, which is #487's
//! answer and not a letter the author never wrote.
//!
//! Both halves are asserted here, and so is the width that follows from the
//! second: §6.2.11.5 compares the declared width of a used glyph against the
//! program, and every CID this map does not name draws `.notdef`, so `/DW` has
//! to be `.notdef`'s advance. `issue13343` failed on eleven glyphs declared 556
//! and drawn 750 before that line existed.

use lopdf::{dictionary, Document, Object, Stream};

/// `(57)【` in Shift-JIS: four one-byte codes and one two-byte code.
const RKSJ_LINE: &[u8] = &[0x28, 0x35, 0x37, 0x29, 0x81, 0x79];

/// `90ms-RKSJ-H` maps `<20>`..`<7d>` to CIDs from 231, so `<28>` is 239.
const CID_OF_PAREN: u16 = 239;

/// `<8179>` falls in `<8140>`..`<817e>` from CID 633, so it is 690 -- `【`,
/// U+3010, which no Latin substitute draws.
const CID_OF_BRACKET: u16 = 690;

/// One page drawing `RKSJ_LINE` through a Type0 over `90ms-RKSJ-H` whose
/// Adobe-Japan1 descendant names a typeface it does not carry.
fn document() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let descriptor = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => "Ryumin-Light",
        "Flags" => Object::Integer(6),
        "ItalicAngle" => Object::Integer(0),
        "StemV" => Object::Integer(69),
        "FontBBox" => Object::Array(vec![
            Object::Integer(-170), Object::Integer(-331),
            Object::Integer(1024), Object::Integer(903),
        ]),
        "Ascent" => Object::Integer(723),
        "Descent" => Object::Integer(-241),
        "CapHeight" => Object::Integer(709),
    }));
    let descendant = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "CIDFontType0",
        "BaseFont" => "Ryumin-Light",
        "CIDSystemInfo" => dictionary! {
            "Registry" => Object::string_literal("Adobe"),
            "Ordering" => Object::string_literal("Japan1"),
            "Supplement" => Object::Integer(2),
        },
        "FontDescriptor" => Object::Reference(descriptor),
        "DW" => Object::Integer(1000),
    }));
    let font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => "Ryumin-Light-90ms-RKSJ-H",
        "Encoding" => Object::Name(b"90ms-RKSJ-H".to_vec()),
        "DescendantFonts" => Object::Array(vec![Object::Reference(descendant)]),
    }));
    let mut content = b"BT /F1 16 Tf 10 20 Td (".to_vec();
    content.extend_from_slice(RKSJ_LINE);
    content.extend_from_slice(b") Tj ET");
    let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference(pages_id),
        "MediaBox" => Object::Array(vec![
            Object::Integer(0), Object::Integer(0),
            Object::Integer(200), Object::Integer(50),
        ]),
        "Contents" => Object::Reference(content_id),
        "Resources" => dictionary! {
            "Font" => dictionary! { "F1" => Object::Reference(font) },
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
    let catalog = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save fixture");
    out
}

fn convert(source: &[u8]) -> Vec<u8> {
    let opts = pdf_manip::pdfa::PdfAConvertOptions {
        conformance: pdf_manip::pdfa_xmp::PdfAConformance::A2b,
        ..Default::default()
    };
    pdf_manip::pdfa::convert_bytes(source, &opts).expect("PDF/A conversion")
}

/// The one CID descendant of a converted fixture: its dictionary, its
/// `/CIDToGIDMap` bytes, and the program that ships.
struct Descendant {
    dict: lopdf::Dictionary,
    map: Vec<u8>,
    program: Vec<u8>,
}

fn descendant(pdf: &[u8]) -> Descendant {
    let doc = Document::load_mem(pdf).expect("load converted output");
    let mut found = Vec::new();
    for object in doc.objects.values() {
        let Ok(font) = object.as_dict() else { continue };
        if font.get(b"Subtype").and_then(Object::as_name).ok() != Some(b"CIDFontType2") {
            continue;
        }
        let Some(program) = font
            .get(b"FontDescriptor")
            .ok()
            .and_then(|o| doc.dereference(o).ok())
            .and_then(|(_, o)| o.as_dict().ok())
            .and_then(|fd| fd.get(b"FontFile2").ok())
            .and_then(|o| doc.dereference(o).ok())
            .and_then(|(_, o)| o.as_stream().ok())
        else {
            continue;
        };
        // The point of the ticket: `/Identity` here is the claim that an
        // Adobe-Japan1 CID indexes a face we chose, and it is not true.
        let map = match font.get(b"CIDToGIDMap") {
            Ok(Object::Reference(_)) | Ok(Object::Stream(_)) => doc
                .dereference(font.get(b"CIDToGIDMap").expect("read back"))
                .ok()
                .and_then(|(_, o)| o.as_stream().ok())
                .and_then(|s| s.decompressed_content().ok())
                .expect("the /CIDToGIDMap stream has to decode"),
            other => panic!(
                "the descendant of a substituted CJK font still says \
                 /CIDToGIDMap {other:?}: its CIDs are being read as glyph \
                 indices in a face the document never named"
            ),
        };
        found.push(Descendant {
            dict: font.clone(),
            map,
            program: program
                .decompressed_content()
                .expect("the embedded program has to decode to be a program at all"),
        });
    }
    assert_eq!(
        found.len(),
        1,
        "the fixture has exactly one CID descendant to embed"
    );
    found.pop().expect("checked above")
}

fn mapped_glyph(map: &[u8], cid: u16) -> u16 {
    let at = usize::from(cid) * 2;
    map.get(at..at + 2)
        .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
        .unwrap_or(0)
}

/// Whether the face draws anything for `gid`.
fn glyph_has_an_outline(face: &ttf_parser::Face, gid: u16) -> bool {
    let mut drew = None;
    face.outline_glyph(ttf_parser::GlyphId(gid), &mut Sink(&mut drew));
    drew.is_some()
}

struct Sink<'a>(&'a mut Option<()>);
impl ttf_parser::OutlineBuilder for Sink<'_> {
    fn move_to(&mut self, _: f32, _: f32) {
        *self.0 = Some(());
    }
    fn line_to(&mut self, _: f32, _: f32) {
        *self.0 = Some(());
    }
    fn quad_to(&mut self, _: f32, _: f32, _: f32, _: f32) {
        *self.0 = Some(());
    }
    fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32) {
        *self.0 = Some(());
    }
    fn close(&mut self) {}
}

/// The width the descendant declares for one CID: its `/W` entry, or `/DW`.
fn declared_width(dict: &lopdf::Dictionary, cid: u16) -> i64 {
    let default = dict.get(b"DW").and_then(Object::as_i64).unwrap_or(1000);
    let Ok(Object::Array(w)) = dict.get(b"W") else {
        return default;
    };
    let mut at = 0;
    while at < w.len() {
        let Some(first) = w[at].as_i64().ok() else {
            break;
        };
        match w.get(at + 1) {
            Some(Object::Array(widths)) => {
                for (offset, width) in widths.iter().enumerate() {
                    if first + offset as i64 == i64::from(cid) {
                        return width.as_i64().unwrap_or(default);
                    }
                }
                at += 2;
            }
            Some(last) => {
                let last = last.as_i64().unwrap_or(first);
                let width = w
                    .get(at + 2)
                    .and_then(|o| o.as_i64().ok())
                    .unwrap_or(default);
                if (first..=last).contains(&i64::from(cid)) {
                    return width;
                }
                at += 3;
            }
            None => break,
        }
    }
    default
}

#[test]
fn a_substituted_cid_font_draws_the_character_its_cid_names() {
    let converted = convert(&document());
    let descendant = descendant(&converted);
    let face = ttf_parser::Face::parse(&descendant.program, 0)
        .expect("the substitute is still a TrueType face");

    // `(` is a character the substitute has, so the CID that means `(` draws
    // it. On `/Identity` this CID drew glyph 239 of LiberationSans instead.
    let paren = mapped_glyph(&descendant.map, CID_OF_PAREN);
    assert_eq!(
        Some(ttf_parser::GlyphId(paren)),
        face.glyph_index('('),
        "CID {CID_OF_PAREN} is `(` in Adobe-Japan1 and the substitute has a \
         glyph for it, so that is the glyph it has to draw"
    );
    assert!(
        glyph_has_an_outline(&face, paren),
        "the glyph the CID reaches was dropped from the subset: the page lost \
         a character it could draw"
    );

    // `【` is a character the substitute does not have. Drawing whichever
    // glyph sits at index 690 is the defect; .notdef is the answer.
    assert_eq!(
        mapped_glyph(&descendant.map, CID_OF_BRACKET),
        0,
        "nothing relates CID {CID_OF_BRACKET} to a glyph of a Latin face, and \
         a glyph chosen by index is a character the author never wrote"
    );

    // §6.2.11.5: the width declared for a used glyph is the program's.
    let upem = f64::from(face.units_per_em());
    let notdef = face
        .glyph_hor_advance(ttf_parser::GlyphId(0))
        .expect("the substitute declares an advance for .notdef");
    assert_eq!(
        declared_width(&descendant.dict, CID_OF_BRACKET),
        (f64::from(notdef) * 1000.0 / upem).round() as i64,
        "CID {CID_OF_BRACKET} draws .notdef, so the width the file declares \
         for it is .notdef's"
    );
}
