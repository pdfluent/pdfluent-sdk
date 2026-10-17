//! A glyph the document never names still has the name its font program gave
//! it, and that name spells the character (#636).
//!
//! The #550 chain ends at the embedded program: a TrueType `post` table, a
//! CFF charset, or Type1 CharStrings keys. But the CID leg of that chain
//! never asked it -- a `CIDFontType2` without `/ToUnicode` and without a
//! Unicode `cmap` dropped every glyph, so subset fonts whose programs carry
//! names like `three.tnum_lnum` lost their digits from the extracted text.
//! And the name reader itself dropped the `uniXXXX…` sequences of AGLFN §3,
//! which spell more code points than one `char` can hold.
//!
//! The tests below are the two halves of that fix:
//!
//! - `post_names_with_aglfn_spellings_read_as_text` -- an embedded
//!   `CIDFontType2` with no `/ToUnicode` and no Unicode `cmap`, whose
//!   glyphs are named `three.tnum_lnum`, `f_i` and `uni20AC`, reads as
//!   `3fi€`: the variant suffix stripped, the ligature split, the `uni`
//!   name decoded.
//! - `an_unresolvable_post_name_stays_missing` -- a name no rule reads
//!   contributes nothing, beside a glyph that does.
//! - `a_tounicode_entry_still_wins_over_the_post_name` -- the document's
//!   own mapping outranks the program's, per glyph: the mapped code reads
//!   as mapped, the unmapped one as named.
//! - `a_uni_sequence_name_reads_as_its_code_points` -- `uni00410042` is
//!   `AB`, on the simple-font leg where the name reader is the only step
//!   in question.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::PdfDocument;

/// `<codes> Tj` inside `BT`/`ET`, one byte per code for simple fonts and two
/// per CID for `Identity-H`.
fn content(raw_codes: &[u8]) -> Vec<u8> {
    let ops = vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 24.into()]),
        Operation::new(
            "Tm",
            vec![
                1.into(),
                0.into(),
                0.into(),
                1.into(),
                72.into(),
                770.into(),
            ],
        ),
        Operation::new(
            "Tj",
            vec![Object::String(
                raw_codes.to_vec(),
                lopdf::StringFormat::Hexadecimal,
            )],
        ),
        Operation::new("ET", vec![]),
    ];
    Content { operations: ops }
        .encode()
        .expect("encode content")
}

/// Wrap one font dictionary in a one-page document that draws `raw_codes`.
fn page_with_font(doc: &mut Document, font: Object, raw_codes: &[u8]) -> Vec<u8> {
    let font_id = doc.add_object(font);
    let stream_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        content(raw_codes),
    )));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![
            Object::Integer(0), Object::Integer(0),
            Object::Integer(612), Object::Integer(792),
        ]),
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

fn text_of(bytes: Vec<u8>) -> String {
    PdfDocument::open(bytes)
        .expect("open pdf")
        .extract_text(0)
        .expect("extract text")
        .trim()
        .to_string()
}

/// A `/ToUnicode` CMap mapping each 2-byte `(code, unicode)` pair with
/// `bfchar`, for the `Type0` font of
/// [`a_tounicode_entry_still_wins_over_the_post_name`].
fn to_unicode_cmap_2byte(pairs: &[(u16, u32)]) -> Vec<u8> {
    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin\n\
         12 dict begin\nbegincmap\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    cmap.push_str(&format!("{} beginbfchar\n", pairs.len()));
    for (code, unicode) in pairs {
        cmap.push_str(&format!("<{code:04X}> <{unicode:04X}>\n"));
    }
    cmap.push_str("endbfchar\nendcmap\nend end\n");
    cmap.into_bytes()
}

/// A `Type0` font with an `Identity-H` encoding whose descendant
/// `CIDFontType2` embeds [`named_truetype`], and no `/ToUnicode` map unless
/// `to_unicode` carries one.
///
/// The CIDs are glyph indices through the default identity `CIDToGIDMap`,
/// and the embedded program's only `cmap` is a Macintosh one, so neither a
/// reverse `cmap` nor -- for these small CIDs -- the Identity guess answers.
/// Whatever the text says about a glyph comes from its `post` name.
fn type0_font(doc: &mut Document, glyph_names: &[&str], to_unicode: Option<Vec<u8>>) -> Object {
    let file_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! { "Length1" => Object::Integer(0) },
        named_truetype(glyph_names),
    )));
    let descriptor_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"FontDescriptor".to_vec()),
        "FontName" => Object::Name(b"Synthetic".to_vec()),
        "Flags" => Object::Integer(4),
        "FontBBox" => Object::Array(vec![
            Object::Integer(0), Object::Integer(0),
            Object::Integer(1000), Object::Integer(1000),
        ]),
        "ItalicAngle" => Object::Integer(0),
        "Ascent" => Object::Integer(800),
        "Descent" => Object::Integer(-200),
        "CapHeight" => Object::Integer(700),
        "StemV" => Object::Integer(80),
        "FontFile2" => Object::Reference(file_id),
    }));
    let descendant_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"CIDFontType2".to_vec()),
        "BaseFont" => Object::Name(b"Synthetic".to_vec()),
        "CIDSystemInfo" => Object::Dictionary(dictionary! {
            "Registry" => Object::string_literal("Adobe"),
            "Ordering" => Object::string_literal("Identity"),
            "Supplement" => Object::Integer(0),
        }),
        "FontDescriptor" => Object::Reference(descriptor_id),
        "DW" => Object::Integer(1000),
    }));
    let mut font = dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type0".to_vec()),
        "BaseFont" => Object::Name(b"Synthetic".to_vec()),
        "Encoding" => Object::Name(b"Identity-H".to_vec()),
        "DescendantFonts" => Object::Array(vec![Object::Reference(descendant_id)]),
    };
    if let Some(cmap) = to_unicode {
        let to_unicode_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, cmap)));
        font.set("ToUnicode", Object::Reference(to_unicode_id));
    }
    Object::Dictionary(font)
}

/// A symbolic simple `TrueType` font with no `/Encoding` and no `/ToUnicode`,
/// embedding [`named_truetype`]: the only thing naming a glyph is the
/// program's `post` table.
fn simple_font(doc: &mut Document, glyph_names: &[&str]) -> Object {
    let file_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! { "Length1" => Object::Integer(0) },
        named_truetype(glyph_names),
    )));
    let descriptor_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"FontDescriptor".to_vec()),
        "FontName" => Object::Name(b"Synthetic".to_vec()),
        "Flags" => Object::Integer(4),
        "FontBBox" => Object::Array(vec![
            Object::Integer(0), Object::Integer(0),
            Object::Integer(1000), Object::Integer(1000),
        ]),
        "ItalicAngle" => Object::Integer(0),
        "Ascent" => Object::Integer(800),
        "Descent" => Object::Integer(-200),
        "CapHeight" => Object::Integer(700),
        "StemV" => Object::Integer(80),
        "FontFile2" => Object::Reference(file_id),
    }));
    let last = glyph_names.len() as i64;
    let widths = vec![Object::Integer(600); glyph_names.len()];
    Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"TrueType".to_vec()),
        "BaseFont" => Object::Name(b"Synthetic".to_vec()),
        "FirstChar" => Object::Integer(1),
        "LastChar" => Object::Integer(last),
        "Widths" => Object::Array(widths),
        "FontDescriptor" => Object::Reference(descriptor_id),
    })
}

/// The three AGLFN spellings of the financial-reports fonts, drawn as CIDs 1
/// to 3 through an embedded `CIDFontType2` that maps none of them: no
/// `/ToUnicode`, no Unicode `cmap`, and CIDs the Identity guess reads as
/// control characters. Before #636 every one of them came out as nothing.
#[test]
fn post_names_with_aglfn_spellings_read_as_text() {
    let mut doc = Document::with_version("1.7");
    let font = type0_font(&mut doc, &["three.tnum_lnum", "f_i", "uni20AC"], None);

    let mut codes = Vec::new();
    for cid in [1u16, 2, 3] {
        codes.extend_from_slice(&cid.to_be_bytes());
    }
    let text = text_of(page_with_font(&mut doc, font, &codes));
    assert_eq!(
        text, "3fi€",
        "the embedded post names did not reach the text"
    );
}

/// A name no AGLFN rule reads is not a character, and must not become one:
/// `zzzUnknown` contributes nothing beside the `3` that does. Without the
/// fallback both glyphs come out as nothing, so this reads `""` and not
/// `"3"`.
#[test]
fn an_unresolvable_post_name_stays_missing() {
    let mut doc = Document::with_version("1.7");
    let font = type0_font(&mut doc, &["three.tnum_lnum", "zzzUnknown"], None);

    let mut codes = Vec::new();
    for cid in [1u16, 2] {
        codes.extend_from_slice(&cid.to_be_bytes());
    }
    let text = text_of(page_with_font(&mut doc, font, &codes));
    assert_eq!(
        text, "3",
        "an unresolvable post name reached the text, or a resolvable one did not"
    );
}

/// The document's own mapping outranks the program's, per glyph: CID 1 is
/// `X` by `/ToUnicode` although its program name says `three`, and CID 2 --
/// which the map does not cover -- reads as its program name `f_i` says.
/// Without the fallback the second glyph comes out as nothing.
#[test]
fn a_tounicode_entry_still_wins_over_the_post_name() {
    let mut doc = Document::with_version("1.7");
    let cmap = to_unicode_cmap_2byte(&[(0x0001, 0x0058)]);
    let font = type0_font(&mut doc, &["three.tnum_lnum", "f_i"], Some(cmap));

    let mut codes = Vec::new();
    for cid in [1u16, 2] {
        codes.extend_from_slice(&cid.to_be_bytes());
    }
    let text = text_of(page_with_font(&mut doc, font, &codes));
    assert_eq!(
        text, "Xfi",
        "a /ToUnicode entry did not win over the post name, or the fallback did not fire beside it"
    );
}

/// `uni00410042` spells two code points, which is one more than the
/// single-character name reader can hold -- before #636 it answered with
/// nothing at all. The simple-font leg, where the name reader is the only
/// step in question; `uni20AC` beside it pins the single-group form.
#[test]
fn a_uni_sequence_name_reads_as_its_code_points() {
    let mut doc = Document::with_version("1.7");
    let font = simple_font(&mut doc, &["uni00410042", "uni20AC"]);

    let text = text_of(page_with_font(&mut doc, font, &[0x01, 0x02]));
    assert_eq!(
        text, "AB€",
        "a uni sequence name did not read as the code points it spells"
    );
}

// ---------------------------------------------------------------------------
// A TrueType font program, built here rather than checked in, so the test says
// in Rust what the font contains: glyph 0 is `.notdef`, glyphs 1..=N carry
// the given `post` names, and a Macintosh `cmap` sends codes 1..=N to glyphs
// 1..=N -- a mapping that says nothing about Unicode, which is the point.
// ---------------------------------------------------------------------------

/// A TrueType whose `post` table (version 2.0) names glyphs 1..=N.
fn named_truetype(glyph_names: &[&str]) -> Vec<u8> {
    let num_glyphs = glyph_names.len() + 1;
    let mut post = Vec::new();
    post.extend_from_slice(&0x0002_0000u32.to_be_bytes()); // version 2.0
    post.extend_from_slice(&[0u8; 28]); // the metrics nobody reads here
    post.extend_from_slice(&(num_glyphs as u16).to_be_bytes());
    post.extend_from_slice(&0u16.to_be_bytes()); // glyph 0 -> ".notdef"
    for i in 0..glyph_names.len() {
        post.extend_from_slice(&(258u16 + i as u16).to_be_bytes());
    }
    for name in glyph_names {
        post.push(name.len() as u8);
        post.extend_from_slice(name.as_bytes());
    }
    assemble_truetype(post, num_glyphs as u16)
}

fn assemble_truetype(post: Vec<u8>, num_glyphs: u16) -> Vec<u8> {
    let mut head = Vec::new();
    head.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // version
    head.extend_from_slice(&0u32.to_be_bytes()); // fontRevision
    head.extend_from_slice(&0u32.to_be_bytes()); // checkSumAdjustment
    head.extend_from_slice(&0x5F0F_3CF5u32.to_be_bytes()); // magicNumber
    head.extend_from_slice(&0u16.to_be_bytes()); // flags
    head.extend_from_slice(&1000u16.to_be_bytes()); // unitsPerEm
    head.extend_from_slice(&[0u8; 16]); // created, modified
    head.extend_from_slice(&0i16.to_be_bytes()); // xMin
    head.extend_from_slice(&0i16.to_be_bytes()); // yMin
    head.extend_from_slice(&1000i16.to_be_bytes()); // xMax
    head.extend_from_slice(&1000i16.to_be_bytes()); // yMax
    head.extend_from_slice(&0u16.to_be_bytes()); // macStyle
    head.extend_from_slice(&8u16.to_be_bytes()); // lowestRecPPEM
    head.extend_from_slice(&2i16.to_be_bytes()); // fontDirectionHint
    head.extend_from_slice(&0i16.to_be_bytes()); // indexToLocFormat: short
    head.extend_from_slice(&0i16.to_be_bytes()); // glyphDataFormat

    let mut hhea = Vec::new();
    hhea.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    hhea.extend_from_slice(&800i16.to_be_bytes()); // ascender
    hhea.extend_from_slice(&(-200i16).to_be_bytes()); // descender
    hhea.extend_from_slice(&0i16.to_be_bytes()); // lineGap
    hhea.extend_from_slice(&600u16.to_be_bytes()); // advanceWidthMax
    hhea.extend_from_slice(&[0u8; 14]); // bearings, extent, caret
    hhea.extend_from_slice(&[0u8; 8]); // reserved
    hhea.extend_from_slice(&0i16.to_be_bytes()); // metricDataFormat
    hhea.extend_from_slice(&num_glyphs.to_be_bytes()); // numberOfHMetrics

    let mut maxp = Vec::new();
    maxp.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    maxp.extend_from_slice(&num_glyphs.to_be_bytes());
    maxp.extend_from_slice(&[0u8; 26]);

    let mut hmtx = Vec::new();
    for _ in 0..num_glyphs {
        hmtx.extend_from_slice(&600u16.to_be_bytes());
        hmtx.extend_from_slice(&0i16.to_be_bytes());
    }

    // Every glyph empty: `loca` has numGlyphs + 1 zeroes, so each one is a
    // zero-length entry in `glyf`. The test is about names, not outlines.
    let loca = vec![0u8; (num_glyphs as usize + 1) * 2];
    let glyf = vec![0u8; 4];

    // A format 0 Macintosh subtable: byte code -> glyph index, directly. It is
    // the subtable a symbolic TrueType is read through, and it says nothing
    // about Unicode, which is the point.
    let mut cmap = Vec::new();
    cmap.extend_from_slice(&0u16.to_be_bytes()); // version
    cmap.extend_from_slice(&1u16.to_be_bytes()); // numTables
    cmap.extend_from_slice(&1u16.to_be_bytes()); // platformID: Macintosh
    cmap.extend_from_slice(&0u16.to_be_bytes()); // encodingID: Roman
    cmap.extend_from_slice(&12u32.to_be_bytes()); // offset to the subtable
    cmap.extend_from_slice(&0u16.to_be_bytes()); // format 0
    cmap.extend_from_slice(&262u16.to_be_bytes()); // length
    cmap.extend_from_slice(&0u16.to_be_bytes()); // language
    let mut glyph_id_array = [0u8; 256];
    for code in 1..num_glyphs {
        glyph_id_array[code as usize] = code as u8;
    }
    cmap.extend_from_slice(&glyph_id_array);

    // Table records are read by tag, but the directory must be sorted by tag
    // for a reader that binary-searches it.
    let tables: Vec<(&[u8; 4], Vec<u8>)> = vec![
        (b"cmap", cmap),
        (b"glyf", glyf),
        (b"head", head),
        (b"hhea", hhea),
        (b"hmtx", hmtx),
        (b"loca", loca),
        (b"maxp", maxp),
        (b"post", post),
    ];

    let num_tables = tables.len() as u16;
    let mut font = Vec::new();
    font.extend_from_slice(&0x0001_0000u32.to_be_bytes()); // sfntVersion
    font.extend_from_slice(&num_tables.to_be_bytes());
    let entry_selector = (15 - num_tables.leading_zeros()) as u16;
    font.extend_from_slice(&(16u16 << entry_selector).to_be_bytes()); // searchRange
    font.extend_from_slice(&entry_selector.to_be_bytes());
    font.extend_from_slice(&(num_tables * 16 - (16 << entry_selector)).to_be_bytes());

    let mut offset = 12 + tables.len() * 16;
    let mut records = Vec::new();
    for (tag, data) in &tables {
        records.extend_from_slice(*tag);
        records.extend_from_slice(&0u32.to_be_bytes()); // checkSum, unverified
        records.extend_from_slice(&(offset as u32).to_be_bytes());
        records.extend_from_slice(&(data.len() as u32).to_be_bytes());
        offset += data.len().next_multiple_of(4);
    }
    font.extend_from_slice(&records);
    for (_, data) in &tables {
        font.extend_from_slice(data);
        font.resize(font.len().next_multiple_of(4), 0);
    }
    font
}
