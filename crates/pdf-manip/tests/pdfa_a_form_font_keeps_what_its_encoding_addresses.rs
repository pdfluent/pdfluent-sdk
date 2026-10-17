// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A composite font an interactive form could draw with keeps a repertoire,
//! not the whole face (#513).
//!
//! A program a variable-text field can be drawn with has no census bound: the
//! appearance is regenerated from `/DA` out of text nobody has typed yet, so no
//! count of the content that exists covers it. For a SIMPLE font the answer
//! since #379 round 3 is "keep the full repertoire the encoding can address" --
//! every code 0..255 -- and that is a real bound rather than the whole face.
//!
//! For a composite font `subset_fonts` used to have no such answer. It required
//! `!form_programs.contains(&program)`, so a `/DR` font with a descendant was
//! not subsettable at all, and the face went out whole however its parent was
//! encoded. That is right for `Identity-H`, where the two-byte code IS the CID
//! and a field can therefore select every glyph in the file. It is drawn too
//! wide for a parent whose CMap this build can read: `UniJIS-UTF16-H` produces
//! the CIDs of Adobe-Japan1 and nothing else, and on the Latin substitute of
//! `pdfjs__test_pdfs_issue15262.pdf` that is 2,268 of the face's 2,620 glyphs.
//! The other 352 are glyphs no text a viewer regenerates can reach.
//!
//! The two cases below are the two halves of that sentence, and they fail in
//! opposite directions: take the narrowing out and the first one keeps a whole
//! face, widen it past a readable CMap and the second one subsets a font whose
//! encoding reaches everything.

use lopdf::{dictionary, Document, Object, Stream};

/// The substitute face is 410 kB whole. A fixture that embedded nothing of the
/// kind would make every size assertion below vacuous.
const KILOBYTES: i64 = 100_000;

/// `<3042>`, HIRAGANA LETTER A, is CID 843 under `UniJIS-UTF16-H`, and 843 is
/// inside the substitute face.
///
/// Nothing in the fixture draws it. That is the point: the page never uses this
/// font at all, so the census says "no code at all" and the only reason to keep
/// any glyph is the appearance a viewer has not regenerated yet.
const REACHABLE_GLYPH: u16 = 843;

/// CID 115 is one of the 352 CIDs below the face's glyph count that
/// `UniJIS-UTF16-H` does not produce: no UTF-16 code point selects it, so no
/// appearance a viewer regenerates can draw it. It is asserted in both
/// directions below -- present in the whole face, gone from the subset -- so
/// that a face which merely failed to embed cannot pass for a narrowing.
const UNREACHABLE_GLYPH: u16 = 115;

/// One page, and an `/AcroForm` whose default resources name a Type0 font over
/// `cmap` whose descendant names a typeface it does not carry.
///
/// The form is what puts the program in `form_regenerable_programs`: a field
/// could be drawn with `/DR /Font /F1` through its `/DA`, so the converter may
/// not bound the program by the text the page happens to draw.
fn form_document_over(cmap: &str) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let descriptor = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => "DejimaMincho",
        "Flags" => Object::Integer(6),
        "ItalicAngle" => Object::Integer(0),
        "StemV" => Object::Integer(50),
        "FontBBox" => Object::Array(vec![
            Object::Integer(0), Object::Integer(-200),
            Object::Integer(1000), Object::Integer(900),
        ]),
        "Ascent" => Object::Integer(800),
        "Descent" => Object::Integer(-200),
        "CapHeight" => Object::Integer(800),
    }));
    let descendant = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "CIDFontType0",
        "BaseFont" => "DejimaMincho",
        "CIDSystemInfo" => dictionary! {
            "Registry" => Object::string_literal("Adobe"),
            "Ordering" => Object::string_literal("Japan1"),
            "Supplement" => Object::Integer(6),
        },
        "FontDescriptor" => Object::Reference(descriptor),
        "DW" => Object::Integer(1000),
    }));
    let font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => "DejimaMincho",
        "Encoding" => Object::Name(cmap.as_bytes().to_vec()),
        "DescendantFonts" => Object::Array(vec![Object::Reference(descendant)]),
    }));
    // The page draws with a Standard 14 font and never with `font`: an
    // appearance regenerated from `/DA` is the ONLY thing that can reach it.
    // Drawing with it here would put its codes in the census and hide which of
    // the two bounds the subsetter used.
    let helvetica = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding",
    }));
    let content_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        b"BT /Helv 20 Tf 28 793 Td (typed later) Tj ET".to_vec(),
    )));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference(pages_id),
        "MediaBox" => Object::Array(vec![
            Object::Integer(0), Object::Integer(0),
            Object::Integer(612), Object::Integer(792),
        ]),
        "Contents" => Object::Reference(content_id),
        "Resources" => dictionary! {
            "Font" => dictionary! { "Helv" => Object::Reference(helvetica) },
        },
    }));
    let field = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Tx",
        "T" => Object::string_literal("typed later"),
        "DA" => Object::string_literal("/F1 12 Tf 0 g"),
        "Rect" => Object::Array(vec![
            Object::Integer(72), Object::Integer(600),
            Object::Integer(300), Object::Integer(620),
        ]),
        "P" => Object::Reference(page_id),
    }));
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(1),
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
        }),
    );
    let form = doc.add_object(Object::Dictionary(dictionary! {
        "Fields" => Object::Array(vec![Object::Reference(field)]),
        "DA" => Object::string_literal("/F1 0 Tf 0 g"),
        "DR" => dictionary! {
            "Font" => dictionary! { "F1" => Object::Reference(font) },
        },
    }));
    let catalog = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
        "AcroForm" => Object::Reference(form),
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

/// The `(declared uncompressed length, program bytes)` of every CID descendant
/// program in a converted document.
fn descendant_programs(pdf: &[u8]) -> Vec<(i64, Vec<u8>)> {
    let doc = Document::load_mem(pdf).expect("load converted output");
    let mut found = Vec::new();
    for object in doc.objects.values() {
        let Ok(font) = object.as_dict() else { continue };
        if font.get(b"Subtype").and_then(Object::as_name).ok() != Some(b"CIDFontType2") {
            continue;
        }
        let Some(descriptor) = font
            .get(b"FontDescriptor")
            .ok()
            .and_then(|o| doc.dereference(o).ok())
            .and_then(|(_, o)| o.as_dict().ok())
        else {
            continue;
        };
        let Some(program) = descriptor
            .get(b"FontFile2")
            .ok()
            .and_then(|o| doc.dereference(o).ok())
            .and_then(|(_, o)| o.as_stream().ok())
        else {
            continue;
        };
        let length1 = program
            .dict
            .get(b"Length1")
            .and_then(Object::as_i64)
            .unwrap_or_default();
        found.push((
            length1,
            program.decompressed_content().unwrap_or_else(|_| {
                panic!("the embedded program has to decode to be a program at all")
            }),
        ));
    }
    assert!(!found.is_empty(), "no CID descendant program was embedded");
    found
}

/// Whether the face draws anything for `gid` -- an outline with bytes in it, as
/// opposed to the empty `loca` span a discarded glyph leaves behind.
fn glyph_has_an_outline(program: &[u8], gid: u16) -> bool {
    let face = ttf_parser::Face::parse(program, 0).expect("the subset is still a TrueType face");
    assert!(
        gid < face.number_of_glyphs(),
        "glyph {gid} is outside the substitute face"
    );
    let mut drew = None;
    face.outline_glyph(ttf_parser::GlyphId(gid), &mut Sink(&mut drew));
    drew.is_some()
}

/// The outline reader reports through a builder; nothing here needs the path,
/// only whether there was one.
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

/// The one program of a converted fixture.
fn the_program(pdf: &[u8]) -> (i64, Vec<u8>) {
    let mut programs = descendant_programs(pdf);
    assert_eq!(programs.len(), 1, "the fixture has one descendant font");
    programs.remove(0)
}

#[test]
fn a_readable_cmap_bounds_a_form_font_even_though_the_census_does_not() {
    let (bounded, subset) = the_program(&convert(&form_document_over("UniJIS-UTF16-H")));
    let (whole, face) = the_program(&convert(&form_document_over("Identity-H")));
    assert!(
        whole > KILOBYTES,
        "the fixture has to embed a face worth bounding: /Length1 {whole}"
    );
    assert!(
        bounded < whole,
        "a field can only ask this font for the CIDs /UniJIS-UTF16-H produces, \
         so the face is not the bound: /Length1 {bounded} against {whole}"
    );
    // The size above is only allowed because the repertoire survived.
    assert!(
        glyph_has_an_outline(&subset, REACHABLE_GLYPH),
        "glyph {REACHABLE_GLYPH} is what <3042> reaches through this CMap; \
         dropping it changes every appearance a viewer regenerates"
    );
    // And it is only a narrowing if what went is what the encoding cannot
    // reach. The whole face carries this glyph; the bounded one must not.
    assert!(
        glyph_has_an_outline(&face, UNREACHABLE_GLYPH),
        "glyph {UNREACHABLE_GLYPH} draws in the substitute face, or this \
         fixture asserts nothing below"
    );
    assert!(
        !glyph_has_an_outline(&subset, UNREACHABLE_GLYPH),
        "no UTF-16 code point selects CID {UNREACHABLE_GLYPH}, so no appearance \
         a viewer regenerates can draw it and it is payload"
    );
}

#[test]
fn an_identity_encoded_form_font_still_goes_out_whole() {
    // Under `Identity-H` the two-byte code is the CID and `/CIDToGIDMap` is the
    // default identity, so a field's `/DA` can select every glyph in the face.
    // There is no repertoire short of the program, which is why this half of
    // the rule stays where #379 round 3 put it.
    let (length1, _) = the_program(&convert(&form_document_over("Identity-H")));
    assert!(
        length1 > KILOBYTES,
        "nothing bounds what a field can draw with an Identity-encoded \
         composite font: /Length1 {length1}"
    );
}
