// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A code the embedded font can draw keeps its byte on the page.
//!
//! Two passes decide that a character code resolves to nothing and write 0x20
//! over it. Both ask a source that does not know:
//!
//! * `fix_simple_font_streams` strips every byte below 32 unless a
//!   `/Differences` array names it. pdfTeX's CM fonts carry their encoding
//!   inside the program (`dup 0 /minus put`) and write no `/Differences` at
//!   all, so `minus`, `periodcentered`, `tau` and `lessequal` are replaced by
//!   spaces -- and by a space the subset has no glyph for, which is how the
//!   repair also *breaks* ISO 19005-2 6.2.11.4.1 on a file that passed.
//!   Measured on govdocs 1707302 (1230 codes), 2852917 and 5473488.
//! * `fix_type1_subset_missing_glyphs` asks `ttf_parser` for the glyph, and
//!   `ttf_parser` answers a format 4 character map by binary search over the
//!   endCode array -- sound only while the segments are sorted. VSPDF writes a
//!   spurious 0-0 segment before the 0xFFFF terminator, the search walks past
//!   every segment above it, and `q` to `z` all answer "no glyph" for a font
//!   that draws them. Measured on govdocs 841_841792: `r`, `s` and `t` became
//!   spaces 230 times and the document kept 64.7 % of its words.
//!
//! The rule both tests hold to: a glyph the font itself can draw is not a
//! missing glyph, whichever table has to be read to see it.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};

// ---------------------------------------------------------------------------
// A Type 1 font program whose encoding lives in the program
// ---------------------------------------------------------------------------

/// Encrypt a Type 1 section (Adobe Type 1 Font Format §7): `r` is 55665 for
/// eexec and 4330 for a charstring.
fn type1_encrypt(plain: &[u8], mut r: u16, leading: usize) -> Vec<u8> {
    const C1: u16 = 52845;
    const C2: u16 = 22719;
    let mut out = Vec::with_capacity(plain.len() + leading);
    let padded: Vec<u8> = std::iter::repeat_n(0x55u8, leading)
        .chain(plain.iter().copied())
        .collect();
    for &p in &padded {
        let c = p ^ (r >> 8) as u8;
        r = (c as u16).wrapping_add(r).wrapping_mul(C1).wrapping_add(C2);
        out.push(c);
    }
    out
}

/// A charstring that only declares an advance: `0 <width> hsbw endchar`.
fn charstring(width: i32) -> Vec<u8> {
    let mut v = vec![139u8]; // 0
    if (108..=1131).contains(&width) {
        let d = width - 108;
        v.push((d / 256) as u8 + 247);
        v.push((d % 256) as u8);
    } else {
        v.push(width as u8 + 139);
    }
    v.push(13); // hsbw
    v.push(14); // endchar
    type1_encrypt(&v, 4330, 4)
}

/// A Type 1 program with a built-in `dup <code> /<name> put` encoding and a
/// charstring for every name it reaches.
fn type1_program(font_name: &str, encoding: &[(u8, &str)]) -> Vec<u8> {
    let mut cleartext = format!(
        "%!FontType1-1.0: {font_name} 001.001\n\
         /FontName /{font_name} def\n\
         /PaintType 0 def\n\
         /FontType 1 def\n\
         /FontMatrix [0.001 0 0 0.001 0 0] readonly def\n\
         /FontBBox {{0 0 1000 1000}} readonly def\n\
         /Encoding 256 array\n\
         0 1 255 {{1 index exch /.notdef put}} for\n"
    );
    for (code, name) in encoding {
        cleartext.push_str(&format!("dup {code} /{name} put\n"));
    }
    cleartext.push_str(
        "readonly def\n\
         currentdict end\n\
         currentfile eexec\n",
    );

    let mut private = String::from(
        "dup /Private 8 dict dup begin\n\
         /RD {string currentfile exch readstring pop} executeonly def\n\
         /ND {noaccess def} executeonly def\n\
         /NP {noaccess put} executeonly def\n\
         /lenIV 4 def\n\
         /BlueValues [] ND\n\
         /Subrs 0 array\n\
         ND\n\
         end\n",
    );
    private.push_str(&format!(
        "/CharStrings {} dict dup begin\n",
        encoding.len() + 1
    ));
    let mut private_bytes = private.into_bytes();
    for (name, width) in
        std::iter::once((".notdef", 0)).chain(encoding.iter().map(|(_, n)| (*n, 500)))
    {
        let cs = charstring(width);
        private_bytes.extend_from_slice(format!("/{name} {} RD ", cs.len()).as_bytes());
        private_bytes.extend_from_slice(&cs);
        private_bytes.extend_from_slice(b" ND\n");
    }
    private_bytes.extend_from_slice(b"end\nend\nmark currentfile closefile\n");

    let mut out = cleartext.into_bytes();
    out.extend_from_slice(&type1_encrypt(&private_bytes, 55665, 4));
    out.extend_from_slice(b"\n");
    out.extend_from_slice(&vec![b'0'; 512]);
    out.extend_from_slice(b"\ncleartomark\n");
    out
}

// ---------------------------------------------------------------------------
// A TrueType program whose character map segments are out of order
// ---------------------------------------------------------------------------

mod truetype {
    fn be16(o: &mut Vec<u8>, v: u16) {
        o.extend_from_slice(&v.to_be_bytes());
    }
    fn be32(o: &mut Vec<u8>, v: u32) {
        o.extend_from_slice(&v.to_be_bytes());
    }

    pub const UNITS_PER_EM: u16 = 1000;
    pub const NUM_GLYPHS: u16 = 300;

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

    /// One closed contour, so the glyph has outline data of its own.
    fn glyph() -> Vec<u8> {
        let points = 4;
        let mut g = Vec::new();
        be16(&mut g, 1); // numberOfContours
        for v in [0i16, 0, 500, 700] {
            g.extend_from_slice(&v.to_be_bytes());
        }
        be16(&mut g, points as u16 - 1); // endPtsOfContours[0]
        be16(&mut g, 0); // instructionLength
        g.extend(std::iter::repeat_n(0x01u8, points));
        for axis in 0..2 {
            for i in 0..points {
                let v = if axis == 0 { i as i16 } else { (i % 3) as i16 };
                g.extend_from_slice(&v.to_be_bytes());
            }
        }
        while g.len() % 4 != 0 {
            g.push(0);
        }
        g
    }

    /// The segment layout govdocs 841_841792 carries, glyph index equal to the
    /// code so that every mapping is trivially checkable.
    ///
    /// The 0-0 segment in front of the 0xFFFF terminator is what breaks a
    /// binary search: looking for 0x72 the search reads endCode[4] = 0x71,
    /// moves right, reads the 0-0 segment at endCode[7], moves right again and
    /// arrives at the terminator, which starts above every code. Segment 5,
    /// which holds 0x72, is never looked at.
    fn cmap() -> Vec<u8> {
        let segs: [(u16, u16); 9] = [
            (0x01, 0x2B),
            (0x2C, 0x3A),
            (0x3B, 0x60),
            (0x61, 0x70),
            (0x71, 0x71),
            (0x72, 0x7A),
            (0x7B, 0xFF),
            (0x00, 0x00),
            (0xFFFF, 0xFFFF),
        ];
        let seg_count = segs.len() as u16;
        let mut sub = Vec::new();
        be16(&mut sub, 4);
        be16(&mut sub, 0); // length, filled in below
        be16(&mut sub, 0);
        be16(&mut sub, seg_count * 2);
        let entry_selector = (u16::BITS - 1 - seg_count.leading_zeros()) as u16;
        let search_range = (1u16 << entry_selector) * 2;
        be16(&mut sub, search_range);
        be16(&mut sub, entry_selector);
        be16(&mut sub, seg_count * 2 - search_range);
        for (_, end) in segs {
            be16(&mut sub, end);
        }
        be16(&mut sub, 0); // reservedPad
        for (start, _) in segs {
            be16(&mut sub, start);
        }
        for (start, _) in segs {
            // The terminator maps 0xFFFF to glyph 0; every real segment maps a
            // code to the glyph of the same index.
            be16(&mut sub, if start == 0xFFFF { 1 } else { 0 });
        }
        for _ in segs {
            be16(&mut sub, 0); // idRangeOffset
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

    pub fn font() -> Vec<u8> {
        let mut glyf = Vec::new();
        let mut loca = Vec::new();
        for gid in 0..NUM_GLYPHS {
            be32(&mut loca, glyf.len() as u32);
            if gid != 0 {
                glyf.extend_from_slice(&glyph());
            }
        }
        be32(&mut loca, glyf.len() as u32);

        let mut tables: Vec<(&[u8; 4], Vec<u8>)> = vec![
            (b"cmap", cmap()),
            (b"glyf", glyf),
            (b"head", head()),
            (b"hhea", hhea()),
            (b"hmtx", hmtx()),
            (b"loca", loca),
            (b"maxp", maxp()),
        ];
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

// ---------------------------------------------------------------------------
// Fixture assembly
// ---------------------------------------------------------------------------

/// One page drawing `text` with one embedded font, which is the whole document.
fn doc_with(font: lopdf::Dictionary, program: Vec<u8>, is_type1: bool, text: &[u8]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let program_len = program.len() as i64;
    let (key, stream_dict) = if is_type1 {
        let cleartext_len = program
            .windows(5)
            .position(|w| w == b"eexec")
            .expect("the program has an eexec section")
            + 6;
        (
            "FontFile",
            dictionary! {
                "Length" => program_len,
                "Length1" => cleartext_len as i64,
                "Length2" => program_len - cleartext_len as i64,
                "Length3" => 0_i64,
            },
        )
    } else {
        (
            "FontFile2",
            dictionary! {
                "Length" => program_len,
                "Length1" => program_len,
            },
        )
    };
    let file_id = doc.add_object(Object::Stream(Stream::new(stream_dict, program)));

    let mut font = font;
    let base_font = font
        .get(b"BaseFont")
        .and_then(|o| o.as_name())
        .expect("the fixture font names itself")
        .to_vec();
    let mut descriptor = dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => Object::Name(base_font),
        // Non-symbolic: these are text faces with an unusual code layout, and
        // nothing in either test depends on the flag.
        "Flags" => 32_i64,
        "FontBBox" => vec![0.into(), 0.into(), 1000.into(), 1000.into()],
        "ItalicAngle" => 0_i64,
        "Ascent" => 800_i64,
        "Descent" => (-200_i64),
        "CapHeight" => 700_i64,
        "StemV" => 80_i64,
    };
    descriptor.set(key, Object::Reference(file_id));
    let fd_id = doc.add_object(Object::Dictionary(descriptor));
    font.set("FontDescriptor", Object::Reference(fd_id));
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

/// A pdfTeX-shaped document: the encoding is in the program, the font
/// dictionary says nothing about it, and the page draws control codes.
fn a_program_encoded_type1(text: &[u8]) -> Vec<u8> {
    let encoding: &[(u8, &str)] = &[(1, "minus"), (3, "tau"), (5, "space")];
    let program = type1_program("PDFLNT+CMSY10", encoding);
    let font = dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => Object::Name(b"PDFLNT+CMSY10".to_vec()),
        "FirstChar" => 0_i64,
        "LastChar" => 255_i64,
        "Widths" => (0..=255).map(|_| Object::Integer(500)).collect::<Vec<_>>(),
    };
    doc_with(font, program, true, text)
}

/// A VSPDF-shaped document: a subset TrueType whose character map segments are
/// out of order, drawn through WinAnsi.
fn a_font_with_unsorted_segments(text: &[u8]) -> Vec<u8> {
    let font = dictionary! {
        "Type" => "Font",
        "Subtype" => "TrueType",
        "BaseFont" => Object::Name(b"PDFLNT+BookAntiqua".to_vec()),
        "FirstChar" => 0_i64,
        "LastChar" => 255_i64,
        "Widths" => (0..=255).map(|_| Object::Integer(500)).collect::<Vec<_>>(),
        "Encoding" => "WinAnsiEncoding",
    };
    doc_with(font, truetype::font(), false, text)
}

fn convert(source: &[u8]) -> Document {
    let opts = pdf_manip::pdfa::PdfAConvertOptions {
        conformance: pdf_manip::pdfa_xmp::PdfAConformance::A2b,
        ..Default::default()
    };
    let out = pdf_manip::pdfa::convert_bytes(source, &opts).expect("PDF/A conversion");
    Document::load_mem(&out).expect("load converted output")
}

/// The bytes of every shown string on the page, concatenated.
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
                match op.operator.as_str() {
                    "Tj" | "'" => {
                        if let Some(Object::String(bytes, _)) = op.operands.first() {
                            out.extend_from_slice(bytes);
                        }
                    }
                    "TJ" => {
                        if let Some(Object::Array(items)) = op.operands.first() {
                            for item in items {
                                if let Object::String(bytes, _) = item {
                                    out.extend_from_slice(bytes);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// The tests
// ---------------------------------------------------------------------------

#[test]
fn a_control_code_the_program_draws_keeps_its_byte() {
    let converted = convert(&a_program_encoded_type1(b"\x01\x03"));
    assert_eq!(
        drawn_bytes(&converted),
        b"\x01\x03",
        "the program draws `minus` on code 1 and `tau` on code 3; writing 0x20 \
         over them takes the characters off the page and points at a glyph the \
         subset does not have"
    );
}

#[test]
fn a_control_code_the_program_cannot_draw_is_still_normalised() {
    // The other half of the rule, so that the fix cannot be "keep every byte
    // below 32": code 7 is `.notdef` in this program, nothing draws for it,
    // and normalising it to the space is what keeps the page's advances and
    // 6.2.11.4.1 both intact.
    let converted = convert(&a_program_encoded_type1(b"\x07"));
    assert_eq!(
        drawn_bytes(&converted),
        b" ",
        "a control code the program has no glyph for is still normalised"
    );
}

#[test]
fn a_code_the_fonts_own_character_map_resolves_keeps_its_byte() {
    let converted = convert(&a_font_with_unsorted_segments(b"Ar"));
    assert_eq!(
        drawn_bytes(&converted),
        b"Ar",
        "`A` and `r` are both in the font's character map and both have \
         outlines; only the segment `r` sits in is one a binary search over an \
         unsorted endCode array skips"
    );
}
