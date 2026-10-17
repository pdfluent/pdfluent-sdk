// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Which outlines a conversion is allowed to drop, and which it is not.
//!
//! Round 3 of #379 is the font axis: embedding is what PDF/A requires, and
//! +12,565,268 bytes of it over the 300-document tuning set is a substitute
//! face embedded for a name the document never embedded itself. Every byte of
//! that growth is in the 226 documents that needed an embedding
//! (`benchmarks/pdfa/size/ROUND3.md`); the 74 that did not came out 64,498
//! bytes smaller.
//!
//! Two rules decided how much of a face survived, and both were drawn wider
//! than the thing they protect:
//!
//!   * **an interactive form froze the whole document.** A viewer can regenerate
//!     a field's appearance with text nobody has typed yet, so no census of the
//!     content that exists can bound what a form draws. True -- of the fonts
//!     the form would draw it with, which `/DA` resolves in the form's default
//!     resources `/DR` (PDF 32000-1 12.7.3.3), and not of every program in the
//!     file. 48 of the 300 have an `/AcroForm`, and they were paying
//!     +4,116,642 bytes of fonts between them.
//!
//!   * **a code in use pinned the glyph at that index.** A reader that cannot
//!     map a code through the character map or the encoding's glyph names reads
//!     the code as a glyph index, so the glyph at that index has to stay. True
//!     -- for a code nothing else resolved. Applied to every code in use it
//!     keeps up to 256 outlines chosen by their position in the file rather
//!     than by anything the document draws, which on a substitute face whose
//!     cmap maps everything is pure storage: 1,768,463 bytes over the 300.
//!
//! #187 is why neither is loosened without a fixture. Careless subsetting there
//! was green on three axes -- smaller, text intact, pages pixel-identical --
//! while veraPDF went from 17 conformant to 0. A saving is not a saving until
//! the axis nobody was watching says so, and a rule this file gets wrong drops
//! a glyph a reader would have drawn.

use lopdf::{dictionary, Document, Object, Stream};

/// Three glyphs carry a byte sequence that appears nowhere else in the
/// program, so "this glyph survived" is a substring test on the font a reader
/// would receive rather than an inference from a size. Each answers a different
/// question:
///
///   * `AT_A_MAPPED_CODE` sits at the index of the code the page draws, which
///     the character map resolves elsewhere. Only the old blanket glyph-index
///     rule keeps it.
///   * `AT_AN_UNMAPPED_CODE` sits at the index of a code the character map does
///     not resolve. The full simple-font repertoire keeps it; a resolved census
///     of a page that never draws that code does not.
///   * `BEYOND_EVERY_CODE` sits past the last index a byte can address. Nothing
///     that reduces a face at all keeps it -- only a program left untouched.
const AT_A_MAPPED_CODE: u16 = b'A' as u16;
const AT_AN_UNMAPPED_CODE: u16 = b'B' as u16;
const BEYOND_EVERY_CODE: u16 = 300;
const MARK_MAPPED: [u8; 8] = [0x5A, 0x7B, 0x11, 0x33, 0x5A, 0x7B, 0x11, 0x33];
const MARK_UNMAPPED: [u8; 8] = [0x6C, 0x2D, 0x44, 0x91, 0x6C, 0x2D, 0x44, 0x91];
const MARK_BEYOND: [u8; 8] = [0x3E, 0xB5, 0x08, 0x77, 0x3E, 0xB5, 0x08, 0x77];

/// A TrueType face built byte by byte: 400 glyphs, of which glyph 1 is what the
/// character map answers for `A`, and glyphs 65, 66 and 300 are the three the
/// tests watch.
///
/// Generated rather than committed, like the one in `embed_font.rs`: this
/// repository ships no font files.
mod synthetic {
    use super::{
        AT_AN_UNMAPPED_CODE, AT_A_MAPPED_CODE, BEYOND_EVERY_CODE, MARK_BEYOND, MARK_MAPPED,
        MARK_UNMAPPED,
    };

    fn be16(o: &mut Vec<u8>, v: u16) {
        o.extend_from_slice(&v.to_be_bytes());
    }
    fn be32(o: &mut Vec<u8>, v: u32) {
        o.extend_from_slice(&v.to_be_bytes());
    }

    pub const UNITS_PER_EM: u16 = 1000;
    pub const NUM_GLYPHS: u16 = 400;
    /// Glyph 1 is what `A` maps to through the character map.
    pub const MAPPED_GLYPH: u16 = 1;

    fn head() -> Vec<u8> {
        let mut t = Vec::new();
        be16(&mut t, 1);
        be16(&mut t, 0);
        be32(&mut t, 0x0001_0000);
        be32(&mut t, 0);
        be32(&mut t, 0x5F0F_3CF5);
        be16(&mut t, 0);
        be16(&mut t, UNITS_PER_EM);
        t.extend_from_slice(&0i64.to_be_bytes());
        t.extend_from_slice(&0i64.to_be_bytes());
        // xMin, yMin, xMax, yMax, macStyle, lowestRecPPEM, fontDirectionHint,
        // indexToLocFormat (1 = long), glyphDataFormat.
        for v in [0i16, 0, 1000, 1000, 0, 8, 2, 1, 0] {
            t.extend_from_slice(&v.to_be_bytes());
        }
        debug_assert_eq!(t.len(), 54);
        t
    }

    fn hhea() -> Vec<u8> {
        let mut t = Vec::new();
        be16(&mut t, 1);
        be16(&mut t, 0);
        for v in [800i16, -200, 0] {
            t.extend_from_slice(&v.to_be_bytes());
        }
        be16(&mut t, 500);
        for v in [0i16, 0, 1000, 1, 0, 0, 0, 0, 0, 0, 0] {
            t.extend_from_slice(&v.to_be_bytes());
        }
        be16(&mut t, NUM_GLYPHS);
        debug_assert_eq!(t.len(), 36);
        t
    }

    fn maxp() -> Vec<u8> {
        let mut t = Vec::new();
        be32(&mut t, 0x0000_5000);
        be16(&mut t, NUM_GLYPHS);
        t
    }

    fn hmtx() -> Vec<u8> {
        let mut t = Vec::new();
        for _ in 0..NUM_GLYPHS {
            be16(&mut t, 500);
            be16(&mut t, 0);
        }
        t
    }

    /// A simple closed contour. `filler` decides how many extra points it
    /// carries, which is how the watched glyph is made big enough that keeping
    /// it is visible in the stored size as well as in the byte test.
    fn glyph(filler: usize, mark: Option<&[u8]>) -> Vec<u8> {
        let points = 4 + filler;
        let mut g = Vec::new();
        be16(&mut g, 1); // numberOfContours
        for v in [0i16, 0, 500, 700] {
            g.extend_from_slice(&v.to_be_bytes());
        }
        be16(&mut g, points as u16 - 1); // endPtsOfContours[0]
        be16(&mut g, 0); // instructionLength
                         // Flags: on-curve, with x and y each a signed 16-bit delta.
        g.extend(std::iter::repeat_n(0x01u8, points));
        // x coordinates, then y coordinates: two bytes each.
        for axis in 0..2 {
            for i in 0..points {
                let v = if axis == 0 {
                    (i % 7) as i16
                } else {
                    (i % 5) as i16
                };
                g.extend_from_slice(&v.to_be_bytes());
            }
        }
        if let Some(mark) = mark {
            // Inside the glyph's own bytes, past every field a subsetter reads:
            // padding a glyph is legal and this one is never rendered by the
            // test, which only asks whether these bytes travelled.
            g.extend_from_slice(mark);
        }
        while g.len() % 4 != 0 {
            g.push(0);
        }
        g
    }

    /// A format 4 cmap mapping `A` to glyph 1 and nothing else.
    fn cmap() -> Vec<u8> {
        let segs: [(u16, u16, i32); 2] = [
            (
                u16::from(b'A'),
                u16::from(b'A'),
                i32::from(MAPPED_GLYPH) - i32::from(b'A'),
            ),
            (0xFFFF, 0xFFFF, 1),
        ];
        let seg_count = segs.len() as u16;
        let mut sub = Vec::new();
        be16(&mut sub, 4);
        be16(&mut sub, 0);
        be16(&mut sub, 0);
        be16(&mut sub, seg_count * 2);
        let entry_selector = (u16::BITS - 1 - seg_count.leading_zeros()) as u16;
        let search_range = (1u16 << entry_selector) * 2;
        be16(&mut sub, search_range);
        be16(&mut sub, entry_selector);
        be16(&mut sub, seg_count * 2 - search_range);
        for (_, end, _) in segs {
            be16(&mut sub, end);
        }
        be16(&mut sub, 0);
        for (start, _, _) in segs {
            be16(&mut sub, start);
        }
        for (_, _, delta) in segs {
            be16(&mut sub, delta as u16);
        }
        for _ in segs {
            be16(&mut sub, 0);
        }
        let len = sub.len() as u16;
        sub[2..4].copy_from_slice(&len.to_be_bytes());

        let mut t = Vec::new();
        be16(&mut t, 0);
        be16(&mut t, 1);
        be16(&mut t, 3);
        be16(&mut t, 1);
        be32(&mut t, 12);
        t.extend_from_slice(&sub);
        t
    }

    /// `with_cmap` false is the symbolic subset shape: no character map at all,
    /// so a code has nowhere to resolve except as a glyph index.
    ///
    /// `variant` only changes how much filler the ordinary glyphs carry, so
    /// that two faces in one document are two programs. `compact_storage`
    /// shares payloads by content, and two fixtures built from identical bytes
    /// arrive at the subsetter as a single program -- which is correct, and
    /// would make "the form's face was kept and the drawn one was not"
    /// unaskable.
    pub fn font(with_cmap: bool, variant: usize) -> Vec<u8> {
        let mut glyf = Vec::new();
        let mut loca = Vec::new();
        for gid in 0..NUM_GLYPHS {
            be32(&mut loca, glyf.len() as u32);
            let body = match gid {
                0 => Vec::new(),
                AT_A_MAPPED_CODE => glyph(400, Some(&MARK_MAPPED)),
                AT_AN_UNMAPPED_CODE => glyph(400, Some(&MARK_UNMAPPED)),
                BEYOND_EVERY_CODE => glyph(400, Some(&MARK_BEYOND)),
                MAPPED_GLYPH => glyph(8 + variant, None),
                _ => glyph(1 + variant, None),
            };
            glyf.extend_from_slice(&body);
        }
        be32(&mut loca, glyf.len() as u32);

        let mut tables: Vec<(&[u8; 4], Vec<u8>)> = vec![
            (b"glyf", glyf),
            (b"head", head()),
            (b"hhea", hhea()),
            (b"hmtx", hmtx()),
            (b"loca", loca),
            (b"maxp", maxp()),
        ];
        if with_cmap {
            tables.push((b"cmap", cmap()));
        }
        // The table directory is ordered by tag, which is what a reader and
        // every checksum tool expects.
        tables.sort_by_key(|(tag, _)| **tag);

        let n = tables.len() as u16;
        let mut out = Vec::new();
        be32(&mut out, 0x0001_0000);
        be16(&mut out, n);
        let entry_selector = (u16::BITS - 1 - n.leading_zeros()) as u16;
        let search_range = (1u16 << entry_selector) * 16;
        be16(&mut out, search_range);
        be16(&mut out, entry_selector);
        be16(&mut out, n * 16 - search_range);

        let mut offset = 12 + 16 * tables.len() as u32;
        let mut body = Vec::new();
        for (tag, data) in &tables {
            out.extend_from_slice(*tag);
            be32(&mut out, 0);
            be32(&mut out, offset);
            be32(&mut out, data.len() as u32);
            offset += data.len() as u32;
            body.extend_from_slice(data);
        }
        out.extend_from_slice(&body);
        out
    }
}

/// One page that shows `A`, plus whatever the caller adds to the catalogue.
///
/// `extra_fonts` are font dictionaries that the page does NOT draw with; a form
/// puts them in its default resources instead, which is the whole question.
struct Fixture {
    doc: Document,
}

fn font_with_program(doc: &mut Document, name: &str, program: Vec<u8>) -> lopdf::ObjectId {
    let length1 = program.len() as i64;
    let program_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! { "Length1" => length1 },
        program,
    )));
    let descriptor_id = doc.add_object(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => Object::Name(name.as_bytes().to_vec()),
        // Non-symbolic, serif: the flags a text face carries.
        "Flags" => 34_i64,
        "FontBBox" => vec![0.into(), 0.into(), 1000.into(), 1000.into()],
        "ItalicAngle" => 0_i64,
        "Ascent" => 800_i64,
        "Descent" => (-200_i64),
        "CapHeight" => 700_i64,
        "StemV" => 80_i64,
        "FontFile2" => Object::Reference(program_id),
    });
    let font_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "TrueType",
        "BaseFont" => Object::Name(name.as_bytes().to_vec()),
        "FirstChar" => 65_i64,
        "LastChar" => 65_i64,
        "Widths" => vec![Object::Integer(500)],
        "Encoding" => "WinAnsiEncoding",
        "FontDescriptor" => Object::Reference(descriptor_id),
    });
    font_id
}

/// A document that draws `A` with `/F0` and carries `extra` further fonts that
/// nothing on the page draws with.
fn fixture(with_cmap: bool, extra: usize) -> Fixture {
    let mut doc = Document::with_version("1.7");
    let drawn_font = font_with_program(&mut doc, "DrawnFace", synthetic::font(with_cmap, 0));
    let mut fonts = dictionary! { "F0" => Object::Reference(drawn_font) };
    for i in 0..extra {
        let font = font_with_program(
            &mut doc,
            &format!("SpareFace{i}"),
            synthetic::font(with_cmap, i + 1),
        );
        fonts.set(format!("S{i}"), Object::Reference(font));
    }

    let content_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        b"BT /F0 12 Tf 72 720 Td (A) Tj ET\n".to_vec(),
    )));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => Object::Reference(content_id),
        "Resources" => dictionary! { "Font" => fonts },
    });
    let pages_id = doc.add_object(dictionary! {
        "Type" => "Pages",
        "Kids" => vec![Object::Reference(page_id)],
        "Count" => 1_i64,
    });
    if let Ok(Object::Dictionary(page)) = doc.get_object_mut(page_id) {
        page.set("Parent", Object::Reference(pages_id));
    }
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
    });
    doc.trailer.set("Root", Object::Reference(catalog_id));
    Fixture { doc }
}

/// Put an `/AcroForm` on the catalogue. `dr_fonts` are the font dictionaries
/// its default resources name; `extra` is merged into the form dictionary.
fn add_form(doc: &mut Document, dr_fonts: &[(&str, lopdf::ObjectId)], extra: lopdf::Dictionary) {
    let field_id = doc.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Tx",
        "T" => Object::string_literal("field"),
        "Rect" => vec![0.into(), 0.into(), 100.into(), 20.into()],
        "DA" => Object::string_literal("/D0 12 Tf 0 g"),
    });
    let mut fonts = dictionary! {};
    for (name, id) in dr_fonts {
        fonts.set(*name, Object::Reference(*id));
    }
    let mut form = dictionary! {
        "Fields" => vec![Object::Reference(field_id)],
        "DA" => Object::string_literal("/D0 12 Tf 0 g"),
        "DR" => dictionary! { "Font" => fonts },
    };
    for (key, value) in extra.iter() {
        form.set(String::from_utf8_lossy(key).to_string(), value.clone());
    }
    let form_id = doc.add_object(form);
    let root = doc
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .expect("the fixture has a catalogue");
    if let Ok(Object::Dictionary(catalog)) = doc.get_object_mut(root) {
        catalog.set("AcroForm", Object::Reference(form_id));
    }
}

/// Convert through the shipping pipeline and hand back every embedded TrueType
/// program in the output, decompressed.
fn converted_font_programs(doc: &Document) -> Vec<Vec<u8>> {
    convert(doc).1
}

/// The converted bytes, and every embedded TrueType program in them,
/// decompressed.
fn convert(doc: &Document) -> (Vec<u8>, Vec<Vec<u8>>) {
    let mut source = Vec::new();
    let mut doc = doc.clone();
    doc.save_to(&mut source)
        .expect("the fixture must serialise");
    let opts = pdf_manip::pdfa::PdfAConvertOptions::default();
    let converted = pdf_manip::pdfa::convert_bytes(&source, &opts).expect("conversion failed");
    let out = Document::load_mem(&converted).expect("the converted document must load");
    let mut programs = Vec::new();
    for object in out.objects.values() {
        let Object::Stream(stream) = object else {
            continue;
        };
        if !stream.dict.has(b"Length1") {
            continue;
        }
        programs.push(
            stream
                .decompressed_content()
                .unwrap_or_else(|_| stream.content.clone()),
        );
    }
    (converted, programs)
}

fn programs_keeping(programs: &[Vec<u8>], mark: &[u8]) -> usize {
    programs
        .iter()
        .filter(|p| p.windows(mark.len()).any(|w| w == mark))
        .count()
}

#[test]
fn a_code_the_character_map_resolves_does_not_pin_its_glyph_index() {
    // `A` maps to glyph 1 through the cmap. The glyph at index 65 is reachable
    // only by reading the code as an index, which no reader does for a font
    // whose character map answered -- so its outline is storage and nothing
    // else.
    let fixture = fixture(true, 0);
    let programs = converted_font_programs(&fixture.doc);
    assert_eq!(programs.len(), 1, "one embedded program was expected");
    assert_eq!(
        programs_keeping(&programs, &MARK_MAPPED),
        0,
        "the glyph at index {AT_A_MAPPED_CODE} survived a conversion of a \
         document that draws one character, whose character map maps that \
         character to glyph {}. Keeping the glyph AT THE CODE's INDEX for \
         every code in use is what round 3 of #379 narrowed: it keeps up to \
         256 outlines chosen by their position in the file, 1.7 MB over the \
         300-document tuning set (benchmarks/pdfa/size/ROUND3.md).",
        synthetic::MAPPED_GLYPH,
    );
}

#[test]
fn a_code_nothing_resolves_still_pins_its_glyph_index() {
    // The same document with the character map taken out: `A` now resolves
    // nowhere, and reading the code as a glyph index is the only mapping a
    // reader has left. Drop this and a symbolic subset loses the glyph it draws.
    let fixture = fixture(false, 0);
    let programs = converted_font_programs(&fixture.doc);
    assert_eq!(programs.len(), 1, "one embedded program was expected");
    assert_eq!(
        programs_keeping(&programs, &MARK_MAPPED),
        1,
        "the glyph at index {AT_A_MAPPED_CODE} was dropped from a font with no \
         character map, where a reader resolves code {AT_A_MAPPED_CODE} by \
         reading it as a glyph index and has nothing else to try. That is a \
         blank where the page drew a character -- the failure #187 records, \
         green on size and on text and wrong on the page."
    );
}

/// The document every form test uses: a page that draws `A` with its own face,
/// and a second face that only the form's default resources name.
fn a_form_over_two_faces(extra: lopdf::Dictionary) -> Fixture {
    let mut fixture = fixture(true, 1);
    let spare = spare_font_id(&fixture.doc, "SpareFace0");
    add_form(&mut fixture.doc, &[("D0", spare)], extra);
    fixture
}

#[test]
fn a_form_default_resource_font_keeps_its_repertoire() {
    // A field's /DA could draw any character with the /DR face, and the census
    // of the content that exists says nothing about that -- so every code the
    // encoding can address keeps its glyph. Code `B` is one nothing maps, and
    // the page never draws it.
    let fixture = a_form_over_two_faces(dictionary! {});
    let programs = converted_font_programs(&fixture.doc);
    assert_eq!(
        programs_keeping(&programs, &MARK_UNMAPPED),
        1,
        "the font a form field would be drawn with came back without the glyph \
         for code {AT_AN_UNMAPPED_CODE}. Its /DA resolves in the form's default \
         resources (PDF 32000-1 12.7.3.3) and the text it will draw is not in \
         the document yet, so no census of the content bounds it."
    );
}

#[test]
fn a_form_default_resource_font_is_still_not_left_whole() {
    // The other half, and the one the gate corpus taught: protecting a /DR face
    // by leaving it alone costs more than the census saves. A byte-addressed
    // simple font reaches 256 glyphs; a substitute for Arial Unicode carries
    // fifty thousand. Blocking the program outright grew seven gate-corpus
    // documents, one from 28.9% of its input to 62.0%.
    let fixture = a_form_over_two_faces(dictionary! {});
    let programs = converted_font_programs(&fixture.doc);
    assert_eq!(
        programs_keeping(&programs, &MARK_BEYOND),
        0,
        "a glyph past the last index a byte can address survived in a face a \
         form field can use. No code in any simple encoding reaches it, so no \
         appearance a viewer regenerates can draw it: this face was left whole \
         rather than reduced to the repertoire."
    );
}

#[test]
fn a_font_only_page_content_draws_is_reduced_even_in_a_form() {
    // The drawn face is not one a field could use, so an interactive form is no
    // reason to keep the codes the page never draws.
    let fixture = a_form_over_two_faces(dictionary! {});
    let programs = converted_font_programs(&fixture.doc);
    assert_eq!(
        programs_keeping(&programs, &MARK_UNMAPPED),
        1,
        "both faces kept the glyph for a code the page never draws, which is \
         what an /AcroForm used to do to every program in the document -- \
         +4,116,642 bytes of fonts across the 48 forms in the tuning set. Only \
         the face in /DR should have it."
    );
}

#[test]
fn an_xfa_form_is_gone_before_the_fonts_are_read() {
    // The XFA branch in `form_regenerable_programs` is unreachable from a
    // conversion and that is worth pinning rather than assuming: PDF/A forbids
    // an /XFA entry (ISO 19005-2 6.4.2:1) and `pdfa_cleanup` removes it well
    // before the font passes run, so what reaches the subsetter is an ordinary
    // AcroForm and its /DR decides. The guard stays for the caller that runs
    // this pass on a document nothing has cleaned; the unit test in
    // `pdfa_subset.rs` is where that path is exercised.
    let fixture = a_form_over_two_faces(dictionary! { "XFA" => Object::string_literal("<xdp/>") });
    let (converted, programs) = convert(&fixture.doc);
    assert!(
        !converted.windows(4).any(|w| w == b"/XFA"),
        "the conversion left an /XFA entry, which 19005-2 6.4.2:1 forbids"
    );
    assert_eq!(
        programs_keeping(&programs, &MARK_UNMAPPED),
        1,
        "with the /XFA entry removed this is an ordinary form: its default \
         resource font keeps its repertoire and the drawn one does not."
    );
}

#[test]
fn a_form_that_names_no_font_stops_every_reduction() {
    // Fields, but no default resources to resolve /DA in. What a viewer would
    // draw the field with is then not a question this module can answer, so
    // every face in the document keeps the whole repertoire -- and none of them
    // keeps more than that.
    let mut fixture = fixture(true, 1);
    add_form(&mut fixture.doc, &[], dictionary! {});
    // add_form always writes a /DR; take the /Font out of it, which is the
    // shape a form with nothing to name has.
    strip_default_resource_fonts(&mut fixture.doc);
    let programs = converted_font_programs(&fixture.doc);
    assert_eq!(
        programs_keeping(&programs, &MARK_UNMAPPED),
        2,
        "a form whose default resources name no font had a face reduced to the \
         codes the page draws. There is nothing to say which font a field \
         would be drawn with, so every one of them keeps the repertoire."
    );
    assert_eq!(
        programs_keeping(&programs, &MARK_BEYOND),
        0,
        "an unreadable form is not a reason to leave a face whole: no simple \
         encoding reaches a glyph past index 255."
    );
}

#[test]
fn an_acroform_with_no_fields_does_not_stop_reduction() {
    // A leftover /AcroForm dictionary with an empty /Fields is not a form: no
    // field means nothing to fill in and nothing to regenerate.
    let fixture = {
        let mut fixture = a_form_over_two_faces(dictionary! {});
        empty_the_fields(&mut fixture.doc);
        fixture
    };
    let programs = converted_font_programs(&fixture.doc);
    assert_eq!(
        programs_keeping(&programs, &MARK_UNMAPPED),
        0,
        "an /AcroForm with no fields still froze the document's fonts. There \
         is no field to fill in, so no appearance to regenerate."
    );
}

fn spare_font_id(doc: &Document, base: &str) -> lopdf::ObjectId {
    for (id, object) in &doc.objects {
        if let Ok(dict) = object.as_dict() {
            if dict.get(b"BaseFont").and_then(Object::as_name).ok() == Some(base.as_bytes()) {
                return *id;
            }
        }
    }
    panic!("the fixture must carry {base}");
}

fn form_id(doc: &Document) -> lopdf::ObjectId {
    let root = doc
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .expect("catalogue");
    doc.get_dictionary(root)
        .expect("catalogue")
        .get(b"AcroForm")
        .and_then(Object::as_reference)
        .expect("the fixture must carry a form")
}

fn strip_default_resource_fonts(doc: &mut Document) {
    let id = form_id(doc);
    if let Ok(form) = doc.get_dictionary_mut(id) {
        form.set("DR", Object::Dictionary(dictionary! {}));
    }
}

fn empty_the_fields(doc: &mut Document) {
    let id = form_id(doc);
    if let Ok(form) = doc.get_dictionary_mut(id) {
        form.set("Fields", Object::Array(Vec::new()));
    }
}
