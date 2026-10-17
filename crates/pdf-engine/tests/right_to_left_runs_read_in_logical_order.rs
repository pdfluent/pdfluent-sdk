//! A right-to-left run comes back in the order it is read, not the order it is
//! drawn (#436, class R-H of #389).
//!
//! Every document here draws its glyphs the way a writer lays out Arabic or
//! Hebrew: one at a time, each further to the LEFT than the one before, so a
//! left-to-right scan of the page hands the letters back in reverse. That is
//! what the extractor used to emit -- `العربية` came out `ةيبرعلا`, which no
//! search will ever match -- and what these tests refuse.
//!
//! The three shapes are the three the ticket names, and each fails differently
//! if the reordering goes:
//!
//! - `an_arabic_word_between_two_latin_words` -- mixed on one line. Removing the
//!   reordering reverses the Arabic word; reordering the whole line instead
//!   would reverse `Hello` and `World.` with it.
//! - `arabic_indic_digits_keep_their_own_direction` -- a number inside a
//!   right-to-left line is laid out left to right, so it is the one run on the
//!   line that must NOT turn around when the line does.
//! - `hebrew_points_follow_the_letter_they_belong_to` -- a point is drawn on top
//!   of its letter and must still follow it afterwards; reversing characters
//!   rather than clusters puts every point in front of its base.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::PdfDocument;

/// One glyph of the page: the character it stands for, where its origin sits,
/// and how wide it is. Width zero is a mark, which is drawn on its base and
/// advances nothing.
struct Drawn {
    ch: char,
    x: f64,
    width: f64,
}

/// A left-to-right word starting at `x`: every glyph one `advance` further
/// right than the last.
fn left_to_right(word: &str, mut x: f64, advance: f64) -> Vec<Drawn> {
    word.chars()
        .map(|ch| {
            let drawn = Drawn {
                ch,
                x,
                width: advance,
            };
            x += advance;
            drawn
        })
        .collect()
}

/// A right-to-left word whose RIGHT edge is at `x`: the first letter of the
/// word sits rightmost, each following one further left. This is the layout the
/// whole ticket is about.
fn right_to_left(word: &str, mut x: f64, advance: f64) -> Vec<Drawn> {
    word.chars()
        .map(|ch| {
            x -= advance;
            Drawn {
                ch,
                x,
                width: advance,
            }
        })
        .collect()
}

/// A mark: no advance of its own, drawn on the glyph before it.
fn mark(ch: char, x: f64) -> Drawn {
    Drawn { ch, x, width: 0.0 }
}

/// A one-page PDF that draws `glyphs` where they say, through a CID font whose
/// `ToUnicode` map names the character each code stands for.
///
/// Every glyph gets its own `Tm`, which is how a writer positions a
/// right-to-left run and the only way to place a mark on top of its base.
fn page_of(glyphs: &[Drawn]) -> Vec<u8> {
    let mut ops = vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 20.into()]),
    ];
    let mut widths: Vec<Object> = Vec::new();
    let mut to_unicode = String::new();
    for (index, glyph) in glyphs.iter().enumerate() {
        let cid = (index + 1) as u16;
        ops.push(Operation::new(
            "Tm",
            vec![
                1.into(),
                0.into(),
                0.into(),
                1.into(),
                glyph.x.into(),
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
            (glyph.width / 20.0 * 1000.0) as i64,
        )]));
        to_unicode.push_str(&format!(
            "<{:04X}> <{}>\n",
            cid,
            glyph
                .ch
                .encode_utf16(&mut [0u16; 2])
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
        glyphs.len(),
        to_unicode
    );

    let mut doc = Document::with_version("1.7");
    let to_unicode_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        cmap.into_bytes(),
    )));
    let descriptor_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"FontDescriptor".to_vec()),
        "FontName" => Object::Name(b"NotoSans".to_vec()),
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
        "BaseFont" => Object::Name(b"NotoSans".to_vec()),
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
        "BaseFont" => Object::Name(b"NotoSans".to_vec()),
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

fn text_of(glyphs: Vec<Drawn>) -> String {
    PdfDocument::open(page_of(&glyphs))
        .expect("open pdf")
        .extract_text(0)
        .expect("extract text")
        .trim()
        .to_string()
}

/// `العربية`, and the letters in the order the page draws them.
const ARABIC: &str = "\u{627}\u{644}\u{639}\u{631}\u{628}\u{64a}\u{629}";

/// hello3.pdf's line, in the shape the extractor sees it: `Hello`, then an
/// Arabic word laid out right to left, then `World.`.
#[test]
fn an_arabic_word_between_two_latin_words() {
    let mut glyphs = left_to_right("Hello ", 72.0, 12.0);
    // The Arabic word occupies the next 7 advances; its first letter is at the
    // right edge of that space, which is where the writer starts it.
    let right_edge = 72.0 + 6.0 * 12.0 + 7.0 * 12.0;
    glyphs.extend(right_to_left(ARABIC, right_edge, 12.0));
    glyphs.extend(left_to_right(" World.", right_edge, 12.0));

    let text = text_of(glyphs);
    assert_eq!(
        text,
        format!("Hello {ARABIC} World."),
        "the Arabic run did not come back in logical order"
    );
    assert!(
        text.contains(ARABIC),
        "the Arabic word is not searchable: {text:?}"
    );
}

/// BidiSample.pdf's date line, `١٤ إبريل ١٤٣٤`: a right-to-left line whose two
/// numbers are laid out left to right inside it.
#[test]
fn arabic_indic_digits_keep_their_own_direction() {
    let month = "\u{625}\u{628}\u{631}\u{64a}\u{644}"; // إبريل
    let day = "\u{661}\u{664}"; // ١٤
    let year = "\u{661}\u{664}\u{663}\u{664}"; // ١٤٣٤

    // Right to left across the line: the day first (rightmost), then a space,
    // the month, a space, and the year at the left. Each number is itself
    // written left to right, so its glyphs run the other way.
    let mut glyphs = Vec::new();
    let day_left = 300.0 - 2.0 * 12.0;
    glyphs.extend(left_to_right(day, day_left, 12.0));
    let month_right = day_left - 12.0;
    glyphs.extend(right_to_left(month, month_right, 12.0));
    let year_left = month_right - 5.0 * 12.0 - 12.0 - 4.0 * 12.0;
    glyphs.extend(left_to_right(year, year_left, 12.0));

    assert_eq!(
        text_of(glyphs),
        format!("{day} {month} {year}"),
        "the digits turned around with the line they sit in"
    );
}

/// `שָׁלוֹם`: three of its letters carry points, which are drawn on top of the
/// letter and have to stay behind it when the run is turned back around.
#[test]
fn hebrew_points_follow_the_letter_they_belong_to() {
    let word = "\u{5e9}\u{5b8}\u{5c1}\u{5dc}\u{5d5}\u{5b9}\u{5dd}";

    let mut glyphs = Vec::new();
    let mut x = 300.0;
    for ch in word.chars() {
        // A point has no advance: it is drawn at the origin of the letter
        // before it, which by then is the last position used.
        if matches!(ch, '\u{5b8}' | '\u{5c1}' | '\u{5b9}') {
            glyphs.push(mark(ch, x));
            continue;
        }
        x -= 12.0;
        glyphs.push(Drawn { ch, x, width: 12.0 });
    }

    assert_eq!(
        text_of(glyphs),
        word,
        "the Hebrew points did not stay behind their letters"
    );
}

/// The other half of the fix: a page with no right-to-left character is not
/// touched by any of this. Same builder, same line assembly, plain Latin.
#[test]
fn a_latin_line_is_unaffected() {
    let glyphs = left_to_right("Total 1234 units", 72.0, 12.0);
    assert_eq!(text_of(glyphs), "Total 1234 units");
}
