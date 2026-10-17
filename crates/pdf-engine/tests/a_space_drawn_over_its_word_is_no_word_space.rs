//! A space drawn over the letter before it is no word space (#574).
//!
//! The page draws the word as two runs, and the second run starts with a
//! SPACE character placed LEFT of where the first run ends: after a ligature
//! glyph the next run starts ` cally` 0.3 em back, after an initial capital
//! ` ransition` 0.6 em back. A real word space never starts left of the
//! letter before it, so the join carries no space and the word reads whole.
//!
//! What this test refuses:
//!
//! - `an_overlapping_second_run_joins_its_word` -- the page below draws `ﬁ`
//!   and then ` nancial` starting 0.3 em before the ligature ends. Removing
//!   the join puts the space back and the page reads `fi nancial`.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::PdfDocument;

/// A one-page PDF drawing `ﬁ` and then ` nancial`, each glyph through its own
/// text matrix, through a CID font whose `ToUnicode` map names each glyph's
/// character.
///
/// The runs are positioned the way the failing pages position them: the
/// ligature 12 points wide at a 20-point size, the second run starting 6
/// points (0.3 em) before the ligature ends. That overlap is past the span
/// merger's seam allowance, so the two runs reach the line as two spans.
fn page_drawing_overlapping_runs() -> Vec<u8> {
    let font_size = 20.0_f64;
    let advance = 12.0_f64;
    let first_x = 72.0_f64;
    let second_x = first_x + advance - 0.3 * font_size;

    let mut placed: Vec<(char, f64)> = vec![('ﬁ', first_x)];
    let mut x = second_x;
    for ch in " nancial".chars() {
        placed.push((ch, x));
        x += advance;
    }

    let mut ops = vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 20.into()]),
    ];
    let mut widths: Vec<Object> = Vec::new();
    let mut to_unicode = String::new();
    for (index, (ch, x)) in placed.iter().enumerate() {
        let cid = (index + 1) as u16;
        ops.push(Operation::new(
            "Tm",
            vec![
                1.into(),
                0.into(),
                0.into(),
                1.into(),
                (*x).into(),
                700.into(),
            ],
        ));
        ops.push(Operation::new(
            "Tj",
            vec![Object::String(
                cid.to_be_bytes().to_vec(),
                lopdf::StringFormat::Hexadecimal,
            )],
        ));
        widths.push(Object::Integer(i64::from(cid)));
        widths.push(Object::Array(vec![Object::Integer(
            (advance / font_size * 1000.0) as i64,
        )]));
        to_unicode.push_str(&format!(
            "<{:04X}> <{}>\n",
            cid,
            ch.encode_utf16(&mut [0u16; 2])
                .iter()
                .map(|unit| format!("{unit:04X}"))
                .collect::<String>()
        ));
    }
    ops.push(Operation::new("ET", vec![]));
    let content = Content { operations: ops }
        .encode()
        .expect("encode content");

    let cmap = format!(
        "/CIDInit /ProcSet findresource begin\n\
         12 dict begin\nbegincmap\n/CMapName /Custom def\n/CMapType 2 def\n\
         1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
         {} beginbfchar\n{}endbfchar\nendcmap\n\
         CMapName currentdict /CMap defineresource pop\nend\nend",
        placed.len(),
        to_unicode
    );

    let mut doc = Document::with_version("1.7");
    let to_unicode_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        cmap.into_bytes(),
    )));
    let descriptor_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"FontDescriptor".to_vec()),
        "FontName" => Object::Name(b"NotoSerif".to_vec()),
        "Flags" => Object::Integer(4),
        "FontBBox" => Object::Array(vec![
            Object::Integer(-568), Object::Integer(-307),
            Object::Integer(2000), Object::Integer(1007),
        ]),
        "ItalicAngle" => Object::Integer(0),
        "Ascent" => Object::Integer(891),
        "Descent" => Object::Integer(-216),
        "CapHeight" => Object::Integer(693),
        "StemV" => Object::Integer(82),
    }));
    let descendant_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"CIDFontType2".to_vec()),
        "BaseFont" => Object::Name(b"NotoSerif".to_vec()),
        "CIDSystemInfo" => Object::Dictionary(dictionary! {
            "Registry" => Object::string_literal("Adobe"),
            "Ordering" => Object::string_literal("Identity"),
            "Supplement" => Object::Integer(0),
        }),
        "FontDescriptor" => Object::Reference(descriptor_id),
        "DW" => Object::Integer(1000),
        "W" => Object::Array(widths),
    }));
    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type0".to_vec()),
        "BaseFont" => Object::Name(b"NotoSerif".to_vec()),
        "Encoding" => Object::Name(b"Identity-H".to_vec()),
        "DescendantFonts" => Object::Array(vec![Object::Reference(descendant_id)]),
        "ToUnicode" => Object::Reference(to_unicode_id),
    }));
    let stream_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
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

/// The word the page draws in two overlapping runs reads as one word.
#[test]
fn an_overlapping_second_run_joins_its_word() {
    let doc = PdfDocument::open(page_drawing_overlapping_runs()).expect("open pdf");
    let text = doc
        .extract_text(0)
        .expect("extract text")
        .trim()
        .to_string();
    assert_eq!(text, "financial", "the word split in two: {text:?}");
}
