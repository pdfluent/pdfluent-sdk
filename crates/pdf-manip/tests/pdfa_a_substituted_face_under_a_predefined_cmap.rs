// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A face we put there ourselves does not get to be 410 kB (#503).
//!
//! `pdfjs__test_pdfs_issue3521.pdf` is 4,486 bytes: one page, seven Chinese
//! characters, and four Type0 fonts over `/GBKp-EUC-H` whose descendants carry
//! no program at all. PDF/A wants a program, so the converter supplies one, and
//! since `83066e4f6` ("a predefined CMap outlives the font program it named")
//! that program is an SFNT rather than a bare CFF -- for a good reason, because
//! a `/FontFile3 /Subtype /Type1C` on a descendant is not a program veraPDF can
//! read and all eight Acrobat-OCR documents of #486 were failing 6.2.11.4.1
//! over it.
//!
//! The subsetter then declined to open any of the four, because it required
//! every parent of a composite font to be `Identity-H`/`Identity-V`: with a
//! predefined CMap the drawn bytes are neither CIDs nor code points, and
//! reading them as CIDs is the defect `pdfa_fonts` documents at length. So the
//! output carried four complete Liberation faces -- `/Length1` 410,712,
//! 414,456, 415,816 and 408,996 -- and the document came out at 876,199 bytes,
//! 197 times its input where the ratchet baseline is 15.66.
//!
//! What this file pins is not "the file got smaller". It is the pair of claims
//! that make the reduction legal:
//!
//!   * the glyphs the CMap can actually reach are still there. A subsetter
//!     that kept `.notdef` and called it a day would pass a size assertion and
//!     change the page. What "reach" means moved with #511: the CID is not a
//!     glyph index in a face we chose, it is a character -- `<41>` is CID 34,
//!     which Adobe-GB1 says is `A`, and the glyph that has to survive is the
//!     one the substitute draws `A` with. The line below carries one so that
//!     this file still guards a glyph the page really draws; it used to name
//!     glyph 1531 for `<B6BC>`, and that number was the defect.
//!
//!   * a CMap this build cannot read still stops the subsetter dead. That is
//!     the rule the fix relaxes, and it is only safe while "cannot read" means
//!     "keep the face whole" rather than "guess".

use lopdf::{dictionary, Document, Object, Stream};

/// The four faces are 410 kB whole. Anything in this order of magnitude is a
/// subset; anything above it is a face that was not opened.
const KILOBYTES: i64 = 100_000;

/// The seven GBK codes of `issue3521`'s only line of text, "我们都是黑体字",
/// and one `A` -- a character a Latin substitute can actually draw.
const GBK_LINE: &[u8] = &[
    0xCE, 0xD2, 0xC3, 0xC7, 0xB6, 0xBC, 0xCA, 0xC7, 0xBA, 0xDA, 0xCC, 0xE5, 0xD7, 0xD6, 0x41,
];

/// `GBKp-EUC-H` maps `<20>`..`<7e>` from CID 1, so `<41>` is CID 34 -- `A` in
/// Adobe-GB1, and the one character of that line the substitute has.
const REACHABLE_CID: u16 = 34;

/// One page drawing `GBK_LINE` through a Type0 over `cmap` whose CIDFontType0
/// descendant names a typeface it does not carry.
fn document_over(cmap: &str) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let descriptor = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => "SimHei",
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
        "BaseFont" => "SimHei",
        "CIDSystemInfo" => dictionary! {
            "Registry" => Object::string_literal("Adobe"),
            "Ordering" => Object::string_literal("GB1"),
            "Supplement" => Object::Integer(2),
        },
        "FontDescriptor" => Object::Reference(descriptor),
        "DW" => Object::Integer(1000),
    }));
    let font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => "SimHei",
        "Encoding" => Object::Name(cmap.as_bytes().to_vec()),
        "DescendantFonts" => Object::Array(vec![Object::Reference(descendant)]),
    }));
    let mut content = b"BT /F1 20 Tf 28 793 Td (".to_vec();
    content.extend_from_slice(GBK_LINE);
    content.extend_from_slice(b") Tj ET");
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

/// The `(declared uncompressed length, program bytes, /CIDToGIDMap bytes)` of
/// every CID descendant program in a converted document.
///
/// An empty map means `/Identity`, which is what a descendant whose CIDs this
/// build cannot name still carries.
fn descendant_programs(pdf: &[u8]) -> Vec<(i64, Vec<u8>, Vec<u8>)> {
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
        let map = font
            .get(b"CIDToGIDMap")
            .ok()
            .and_then(|o| doc.dereference(o).ok())
            .and_then(|(_, o)| o.as_stream().ok())
            .and_then(|s| s.decompressed_content().ok())
            .unwrap_or_default();
        found.push((
            length1,
            program.decompressed_content().unwrap_or_else(|_| {
                panic!("the embedded program has to decode to be a program at all")
            }),
            map,
        ));
    }
    assert!(!found.is_empty(), "no CID descendant program was embedded");
    found
}

/// Whether the glyph `cid` reaches draws anything -- an outline with bytes in
/// it, as opposed to the empty `loca` span a discarded glyph leaves behind.
///
/// Through the `/CIDToGIDMap` this descendant carries (#511), not through the
/// CID: the two were the same number only while the substitute was being
/// indexed by a CID that means nothing in it.
fn cid_draws_an_outline(program: &[u8], map: &[u8], cid: u16) -> bool {
    let face = ttf_parser::Face::parse(program, 0).expect("the subset is still a TrueType face");
    let at = usize::from(cid) * 2;
    let gid = map
        .get(at..at + 2)
        .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
        .unwrap_or(0);
    assert!(
        gid != 0 && gid < face.number_of_glyphs(),
        "CID {cid} reaches no glyph of the substitute at all"
    );
    let mut bounds = None;
    face.outline_glyph(ttf_parser::GlyphId(gid), &mut Sink(&mut bounds));
    bounds.is_some()
}

/// Both outline readers report through a builder; nothing here needs the path,
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
impl cff_parser::OutlineBuilder for Sink<'_> {
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

#[test]
fn a_substituted_face_under_a_predefined_cmap_is_not_shipped_whole() {
    let converted = convert(&document_over("GBKp-EUC-H"));
    for (length1, program, map) in descendant_programs(&converted) {
        assert!(
            length1 < KILOBYTES,
            "a substitute for a font the document never carried was embedded whole: \
             /Length1 {length1}"
        );
        // The size above is only allowed because the reachable glyph survived.
        assert!(
            cid_draws_an_outline(&program, &map, REACHABLE_CID),
            "CID {REACHABLE_CID} is `A` and this face draws `A`; dropping that \
             glyph changes the page"
        );
    }
}

#[test]
fn a_cmap_this_build_cannot_read_still_stops_the_subsetter() {
    // Big5 for Adobe-CNS1. Nothing in `cmap_codes` carries its table, and a
    // code under it means nothing this crate can compute.
    let converted = convert(&document_over("ETen-B5-H"));
    for (length1, _, _) in descendant_programs(&converted) {
        assert!(
            length1 > KILOBYTES,
            "a CMap with no table cannot bound what the document draws, so the \
             face has to stay whole: /Length1 {length1}"
        );
    }
}

/// The document `issue9534_reduced.pdf` has: a Type0 whose `/Encoding` is a
/// CMap it embeds, and a `CIDFontType0` descendant whose `/FontFile3
/// /Subtype /CIDFontType0C` is **not CID-keyed** -- 494 glyphs and not one
/// with a CID of its own. ISO 32000-1 9.7.4.2 says the CID is then the glyph
/// index, and a subsetter that asks the charset instead is told nothing about
/// every glyph and keeps only `.notdef`.
///
/// Nothing reached that path before #503, because an embedded CMap was not a
/// CMap this module would read. It is now, and the first measured run of the
/// change came back with "Cina, il Grande" rendered as an empty page.
mod a_cff_that_is_not_cid_keyed {
    use super::{convert, Sink};
    use lopdf::{dictionary, Document, Object, Stream};

    /// A CMap that embeds its own table: two-byte codes, one `cidrange`
    /// mapping `<0041>` to the CID under test.
    fn cmap_stream(cid: u16) -> Vec<u8> {
        format!(
            "/CIDInit /ProcSet findresource begin\n\
             12 dict begin\n begincmap\n/CMapName /T7Test def\n/CMapType 1 def\n\
             1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
             1 begincidrange\n<0041> <0041> {cid}\nendcidrange\nendcmap\nend\nend\n"
        )
        .into_bytes()
    }

    /// The first glyph of the compiled-in Helvetica CFF that draws anything.
    /// Read rather than written down, so the fixture cannot go stale against a
    /// different build of the font program.
    fn a_glyph_that_draws(program: &[u8]) -> u16 {
        let cff = cff_parser::Table::parse(program).expect("the Standard 14 CFF parses");
        assert!(
            (0..cff.number_of_glyphs())
                .all(|gid| cff.glyph_cid(cff_parser::GlyphId(gid)).is_none()),
            "this fixture is only about a CFF that is not CID-keyed"
        );
        for gid in 1..cff.number_of_glyphs() {
            let mut drew = None;
            let _ = cff.outline(cff_parser::GlyphId(gid), &mut Sink(&mut drew));
            if drew.is_some() {
                return gid;
            }
        }
        panic!("the Standard 14 Helvetica draws nothing at all");
    }

    fn document_drawing(cid: u16, program: &[u8]) -> Vec<u8> {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();
        let font_file = doc.add_object(Object::Stream(Stream::new(
            dictionary! { "Subtype" => "CIDFontType0C" },
            program.to_vec(),
        )));
        let descriptor = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "FontDescriptor",
            "FontName" => "T7SerifCID",
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
            "FontFile3" => Object::Reference(font_file),
        }));
        let descendant = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Font",
            "Subtype" => "CIDFontType0",
            "BaseFont" => "T7SerifCID",
            "CIDSystemInfo" => dictionary! {
                "Registry" => Object::string_literal("Adobe"),
                "Ordering" => Object::string_literal("Identity"),
                "Supplement" => Object::Integer(0),
            },
            "FontDescriptor" => Object::Reference(descriptor),
            "DW" => Object::Integer(1000),
        }));
        let encoding = doc.add_object(Object::Stream(Stream::new(
            dictionary! { "Type" => "CMap", "CMapName" => "T7Test" },
            cmap_stream(cid),
        )));
        let font = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type0",
            "BaseFont" => "T7SerifCID",
            "Encoding" => Object::Reference(encoding),
            "DescendantFonts" => Object::Array(vec![Object::Reference(descendant)]),
        }));
        let content_id = doc.add_object(Object::Stream(Stream::new(
            dictionary! {},
            b"BT /F1 24 Tf 72 700 Td <0041> Tj ET".to_vec(),
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

    /// The CFF programs of every CID descendant in a converted document.
    fn descendant_cff(pdf: &[u8]) -> Vec<Vec<u8>> {
        let doc = Document::load_mem(pdf).expect("load converted output");
        let mut found = Vec::new();
        for object in doc.objects.values() {
            let Ok(stream) = object.as_stream() else {
                continue;
            };
            if stream
                .dict
                .get(b"Subtype")
                .and_then(Object::as_name)
                .ok()
                .is_none_or(|s| s != b"CIDFontType0C")
            {
                continue;
            }
            found.push(
                stream
                    .decompressed_content()
                    .unwrap_or_else(|_| stream.content.clone()),
            );
        }
        assert_eq!(found.len(), 1, "one descendant program was embedded");
        found
    }

    #[test]
    fn keeps_the_glyph_its_cid_indexes() {
        let program = pdf_standard_fonts::StandardFont::Helvetica.data();
        let cid = a_glyph_that_draws(program);
        let converted = convert(&document_drawing(cid, program));
        for subset in descendant_cff(&converted) {
            let cff = cff_parser::Table::parse(&subset).expect("the subset is still a CFF");
            let mut drew = None;
            let _ = cff.outline(cff_parser::GlyphId(cid), &mut Sink(&mut drew));
            assert!(
                drew.is_some(),
                "the page draws CID {cid}, and in a CFF with no CIDs of its own \
                 that is glyph {cid}: dropping it empties the page"
            );
        }
    }
}
