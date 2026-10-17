//! `circlecopyrt` reads as U+20DD and `negationslash` as U+0338, each after
//! the glyph it marks (spec `specs/text/circlecopyrt.md`, C4 and C5a).
//!
//! TeX's `\copyright` is a `c` from the text font with a large circle from
//! `cmsy10` drawn over it, the circle's box starting left of the `c`; `\not=`
//! is `cmsy10`'s slash drawn before the `=` it strikes. The AMS name table
//! reads the circle as U+20DD COMBINING ENCLOSING CIRCLE and the slash as
//! U+0338 COMBINING LONG SOLIDUS OVERLAY, and the interpreter puts each mark
//! directly after the glyph drawn next to it whose centre its box holds --
//! `c⃝` and `≠` (`=` U+0338), never `⃝c` or `̸=` -- in whichever order the page
//! drew the two.
//!
//! Every fixture is built here: Helvetica (not embedded) for the text, and a
//! second non-embedded `Type1` font whose `/Differences` names code `A`, one
//! em wide, so the mark's box is a 10 pt square at 10 pt.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, Stream};
use pdf_engine::PdfDocument;

const CIRCLE: char = '\u{20DD}';
const SLASH: char = '\u{0338}';

/// One thing the page draws: a string in a font at a point, size 10.
enum Draw<'a> {
    /// Text in Helvetica (`/F1`).
    Text(f64, &'a str),
    /// Code `A` of the named-glyph font (`/F2`).
    Named(f64),
    /// Text in Helvetica inside a marked-content sequence whose
    /// `/ActualText` is the third field.
    Replaced(f64, &'a str, &'a str),
    /// Code `A` of a font whose `/ToUnicode` gives it U+FFFF (`/F3`) or
    /// U+FFFF U+20DD (`/F4`), as [`Odd`] says.
    Odd(f64, Odd),
    /// Text in Helvetica drawn invisibly (render mode 3).
    Hidden(f64, &'a str),
    /// A marked-content sequence with this `/ActualText` and no glyph in it,
    /// only a small filled square far right, so the replacement has
    /// somewhere to read.
    Glyphless(&'a str),
}

/// Which noncharacter font [`Draw::Odd`] draws in.
#[derive(Clone, Copy)]
enum Odd {
    /// `/ToUnicode` gives U+FFFF.
    Nonchar,
    /// `/ToUnicode` gives U+FFFF followed by U+20DD.
    NoncharCircle,
}

/// The content stream drawing `draws` on the baseline y = 700, in order.
fn content(draws: &[Draw<'_>]) -> Vec<u8> {
    let mut ops = vec![Operation::new("BT", vec![])];
    for draw in draws {
        if let Draw::Glyphless(actual) = draw {
            ops.push(Operation::new(
                "BDC",
                vec![
                    Object::Name(b"Span".to_vec()),
                    Object::Dictionary(dictionary! {
                        "ActualText" => Object::String(
                            actual.as_bytes().to_vec(),
                            lopdf::StringFormat::Literal,
                        ),
                    }),
                ],
            ));
            ops.push(Operation::new(
                "re",
                vec![400.into(), 700.into(), 4.into(), 4.into()],
            ));
            ops.push(Operation::new("f", vec![]));
            ops.push(Operation::new("EMC", vec![]));
            continue;
        }
        if let Draw::Hidden(..) = draw {
            ops.push(Operation::new("Tr", vec![3.into()]));
        }
        let (font, x, bytes) = match draw {
            Draw::Text(x, text) | Draw::Replaced(x, text, _) => {
                ("F1", *x, text.as_bytes().to_vec())
            }
            Draw::Named(x) => ("F2", *x, b"A".to_vec()),
            Draw::Odd(x, Odd::Nonchar) => ("F3", *x, b"A".to_vec()),
            Draw::Odd(x, Odd::NoncharCircle) => ("F4", *x, b"A".to_vec()),
            Draw::Hidden(x, text) => ("F1", *x, text.as_bytes().to_vec()),
            Draw::Glyphless(_) => unreachable!("handled above"),
        };
        if let Draw::Replaced(_, _, actual) = draw {
            ops.push(Operation::new(
                "BDC",
                vec![
                    Object::Name(b"Span".to_vec()),
                    Object::Dictionary(dictionary! {
                        "ActualText" => Object::String(
                            actual.as_bytes().to_vec(),
                            lopdf::StringFormat::Literal,
                        ),
                    }),
                ],
            ));
        }
        ops.push(Operation::new(
            "Tf",
            vec![Object::Name(font.as_bytes().to_vec()), 10.into()],
        ));
        ops.push(Operation::new(
            "Tm",
            vec![
                1.into(),
                0.into(),
                0.into(),
                1.into(),
                Object::Real(x as f32),
                700.into(),
            ],
        ));
        ops.push(Operation::new(
            "Tj",
            vec![Object::String(bytes, lopdf::StringFormat::Literal)],
        ));
        if let Draw::Replaced(..) = draw {
            ops.push(Operation::new("EMC", vec![]));
        }
        if let Draw::Hidden(..) = draw {
            ops.push(Operation::new("Tr", vec![0.into()]));
        }
    }
    ops.push(Operation::new("ET", vec![]));
    Content { operations: ops }
        .encode()
        .expect("encode content")
}

/// A one-byte `/ToUnicode` CMap sending `code` to `value`: one UTF-16 unit,
/// or two when `value` is over U+FFFF (the high one first).
fn to_unicode(code: u8, value: u32) -> Vec<u8> {
    format!(
        "/CIDInit /ProcSet findresource begin\n\
         12 dict begin\nbegincmap\n/CMapType 2 def\n\
         1 begincodespacerange\n<00> <FF>\nendcodespacerange\n\
         1 beginbfchar\n<{code:02X}> <{value:04X}>\nendbfchar\n\
         endcmap\nend end\n"
    )
    .into_bytes()
}

/// A non-embedded `Type1` font whose `/Differences` names code `A` `name`,
/// one em wide, with a `/ToUnicode` value for it when `unicode` is given.
fn named_font(doc: &mut Document, name: &str, unicode: Option<u32>) -> Dictionary {
    let mut font = dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"CMSY10".to_vec()),
        "FirstChar" => Object::Integer(65),
        "LastChar" => Object::Integer(65),
        "Widths" => Object::Array(vec![Object::Integer(1000)]),
        "Encoding" => Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Encoding".to_vec()),
            "Differences" => Object::Array(vec![
                Object::Integer(65),
                Object::Name(name.as_bytes().to_vec()),
            ]),
        }),
    };
    if let Some(value) = unicode {
        let id = doc.add_object(Object::Stream(Stream::new(
            dictionary! {},
            to_unicode(b'A', value),
        )));
        font.set("ToUnicode", Object::Reference(id));
    }
    font
}

/// A one-page document with `/F1` Helvetica and `/F2` naming code `A`
/// `name`, drawing `draws`; its extracted text.
fn text_of(name: &str, unicode: Option<u32>, draws: &[Draw<'_>]) -> String {
    document(name, unicode, draws)
        .extract_text(0)
        .expect("extract text")
        .trim()
        .to_string()
}

/// The document [`text_of`] reads.
fn document(name: &str, unicode: Option<u32>, draws: &[Draw<'_>]) -> PdfDocument {
    let mut doc = Document::with_version("1.7");
    let helvetica = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    }));
    let named = named_font(&mut doc, name, unicode);
    let named = doc.add_object(Object::Dictionary(named));
    let nonchar = named_font(&mut doc, UNKNOWN_NAME, Some(0xFFFF));
    let nonchar = doc.add_object(Object::Dictionary(nonchar));
    let nonchar_circle = named_font(&mut doc, UNKNOWN_NAME, Some(0xFFFF_20DD));
    let nonchar_circle = doc.add_object(Object::Dictionary(nonchar_circle));
    let stream_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content(draws))));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => Object::Reference(stream_id),
        "Resources" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dictionary! {
                "F1" => Object::Reference(helvetica),
                "F2" => Object::Reference(named),
                "F3" => Object::Reference(nonchar),
                "F4" => Object::Reference(nonchar_circle),
            }),
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
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("save synthetic pdf");
    PdfDocument::open(bytes).expect("open pdf")
}

/// `text` with every whitespace character removed.
fn bare(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

fn count(text: &str, wanted: char) -> usize {
    text.chars().filter(|&c| c == wanted).count()
}

/// The gate's order: `c` at 102.8, then the circle at 100 -- its box starts
/// left of the `c` and holds the `c`'s centre. Without the placement rule the
/// x order reads `⃝ c`.
#[test]
fn a_circle_drawn_over_a_c_after_it_follows_the_c() {
    let text = text_of(
        "circlecopyrt",
        None,
        &[
            Draw::Text(50.0, "copyright"),
            Draw::Text(102.8, "c"),
            Draw::Named(100.0),
            Draw::Text(115.0, "1994"),
        ],
    );
    assert_eq!(text, "copyright c\u{20DD} 1994");
}

/// The circle drawn first and the `c` over it: still `c⃝`.
#[test]
fn a_circle_drawn_before_its_c_still_follows_the_c() {
    let text = text_of(
        "circlecopyrt",
        None,
        &[
            Draw::Text(50.0, "copyright"),
            Draw::Named(100.0),
            Draw::Text(102.8, "c"),
            Draw::Text(115.0, "1994"),
        ],
    );
    assert_eq!(text, "copyright c\u{20DD} 1994");
}

/// A glyph name in no table at all: the code reads as `circlecopyrt` did
/// before the C4 row, so it stands in for "the row removed".
const UNKNOWN_NAME: &str = "zzunlisted";

/// Pages where the mark has nothing inside it: alone on its line, between two
/// glyphs it does not reach, and at the end of a line after glyphs it does
/// not reach.
const NOTHING_INSIDE: [&[Draw<'static>]; 3] = [
    &[Draw::Named(100.0)],
    &[
        Draw::Text(50.0, "a"),
        Draw::Named(70.0),
        Draw::Text(90.0, "b"),
    ],
    &[Draw::Text(50.0, "ab"), Draw::Named(80.0)],
];

/// Negative control (rule 3, review fix 3): a circle with nothing inside it
/// gives no U+20DD at all, and the line equals the line the same page gives
/// without the C4 row -- text, spacing and all, so the pen moved exactly as
/// before.
#[test]
fn a_circle_enclosing_nothing_reads_as_without_the_row() {
    for draws in NOTHING_INSIDE {
        let text = text_of("circlecopyrt", None, draws);
        assert_eq!(count(&text, CIRCLE), 0, "{text:?}");
        assert_eq!(text, text_of(UNKNOWN_NAME, None, draws));
    }
}

/// Two `\copyright`s on one line: each circle after its own `c`, each once.
#[test]
fn two_pairs_on_one_line_each_read_once() {
    let text = text_of(
        "circlecopyrt",
        None,
        &[
            Draw::Text(50.0, "copyright"),
            Draw::Text(102.8, "c"),
            Draw::Named(100.0),
            Draw::Text(115.0, "and"),
            Draw::Text(152.8, "c"),
            Draw::Named(150.0),
            Draw::Text(165.0, "1994"),
        ],
    );
    assert_eq!(text, "copyright c\u{20DD} and c\u{20DD} 1994");
    assert_eq!(count(&text, CIRCLE), 2);
}

/// The AGL name `copyright` is not the AMS circle: it reads `©`, once.
#[test]
fn the_agl_copyright_name_still_reads_as_the_sign() {
    let text = text_of("copyright", None, &[Draw::Named(100.0)]);
    assert_eq!(text, "\u{00A9}");
}

/// A `/ToUnicode` value on the circle's code wins over the name, and then no
/// enclosing mark is there to place: the line keeps its x order.
#[test]
fn a_tounicode_value_on_the_circle_code_wins() {
    let text = text_of(
        "circlecopyrt",
        Some(0x004F),
        &[Draw::Text(102.8, "c"), Draw::Named(100.0)],
    );
    assert_eq!(count(&text, CIRCLE), 0, "{text:?}");
    let bare: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    assert_eq!(bare, "Oc");
}

/// Rules 2 and 3 are only for the `circlecopyrt` row (rule 6). A U+20DD that
/// `/ToUnicode` gives keeps today's content and order: alone it stays, and
/// over a `c` drawn before it it keeps the x order, ahead of the `c`. The
/// glyph name is one no table knows, so the value can only be the map's.
#[test]
fn a_tounicode_circle_keeps_todays_text() {
    let lone = text_of(UNKNOWN_NAME, Some(0x20DD), &[Draw::Named(100.0)]);
    assert_eq!(lone, "\u{20DD}");

    let enclosing = text_of(
        UNKNOWN_NAME,
        Some(0x20DD),
        &[
            Draw::Text(50.0, "copyright"),
            Draw::Text(102.8, "c"),
            Draw::Named(100.0),
            Draw::Text(115.0, "1994"),
        ],
    );
    assert_eq!(count(&enclosing, CIRCLE), 1, "{enclosing:?}");
    let bare: String = enclosing.chars().filter(|c| !c.is_whitespace()).collect();
    assert_eq!(bare, "copyright\u{20DD}c1994");
}

/// The same, with the glyph named `circlecopyrt`: the `/ToUnicode` value still
/// wins (rule 1) and is still not placed or dropped (rule 6).
#[test]
fn a_tounicode_circle_on_the_circlecopyrt_code_keeps_todays_text() {
    let lone = text_of("circlecopyrt", Some(0x20DD), &[Draw::Named(100.0)]);
    assert_eq!(lone, "\u{20DD}");

    let draws = [
        Draw::Text(50.0, "copyright"),
        Draw::Text(102.8, "c"),
        Draw::Named(100.0),
        Draw::Text(115.0, "1994"),
    ];
    let enclosing = text_of("circlecopyrt", Some(0x20DD), &draws);
    let bare: String = enclosing.chars().filter(|c| !c.is_whitespace()).collect();
    assert_eq!(bare, "copyright\u{20DD}c1994");
    assert_eq!(enclosing, text_of(UNKNOWN_NAME, Some(0x20DD), &draws));
}

/// C5a: the slash drawn before the `=` it strikes, its box starting left of
/// the `=`: the `=` reads first and the mark after it, `≠`. What the slash's
/// code read as before its row (here the standard-encoding guess for code
/// `A`) is gone, not left in front.
#[test]
fn a_slash_drawn_before_its_equals_sign_follows_it() {
    let draws = [
        Draw::Text(50.0, "a"),
        Draw::Named(100.0),
        Draw::Text(102.8, "="),
        Draw::Text(115.0, "b"),
    ];
    let text = text_of("negationslash", None, &draws);
    assert_eq!(bare(&text), "a=\u{0338}b", "{text:?}");
    assert!(!text.contains("\u{0338}="), "{text:?}");
    assert_eq!(count(&text, SLASH), 1);
}

/// The slash drawn after the `=`: still `≠`.
#[test]
fn a_slash_drawn_after_its_equals_sign_follows_it() {
    let draws = [
        Draw::Text(50.0, "a"),
        Draw::Text(102.8, "="),
        Draw::Named(100.0),
        Draw::Text(115.0, "b"),
    ];
    let text = text_of("negationslash", None, &draws);
    assert_eq!(bare(&text), "a=\u{0338}b", "{text:?}");
    assert_eq!(count(&text, SLASH), 1);
}

/// C5a, negative control: a slash over nothing gives no U+0338 at all, and
/// the line equals the line the same page gives without the row, spacing
/// included: alone, between two glyphs, at the end of a line.
#[test]
fn a_slash_over_nothing_reads_as_without_the_row() {
    for draws in NOTHING_INSIDE {
        let text = text_of("negationslash", None, draws);
        assert_eq!(count(&text, SLASH), 0, "{text:?}");
        assert_eq!(text, text_of(UNKNOWN_NAME, None, draws));
    }
}

/// C5a: a U+0338 that `/ToUnicode` gives keeps today's content and order:
/// alone it stays, and drawn before the `=` it strikes it stays in front, in
/// x order -- with the glyph named `negationslash` or not.
#[test]
fn a_tounicode_slash_keeps_todays_text() {
    for name in [UNKNOWN_NAME, "negationslash"] {
        let lone = text_of(name, Some(0x0338), &[Draw::Named(100.0)]);
        assert_eq!(lone, "\u{0338}", "/{name}");

        let draws = [
            Draw::Text(50.0, "a"),
            Draw::Named(100.0),
            Draw::Text(102.8, "="),
            Draw::Text(115.0, "b"),
        ];
        let striking = text_of(name, Some(0x0338), &draws);
        assert_eq!(bare(&striking), "a\u{0338}=b", "/{name}: {striking:?}");
        assert_eq!(striking, text_of(UNKNOWN_NAME, Some(0x0338), &draws));
    }
}

/// The two marks the AMS table reads as combining marks, with the glyph each
/// is drawn over in the gate file.
const MARK_ROWS: [(&str, char, &str); 2] =
    [("circlecopyrt", CIRCLE, "c"), ("negationslash", SLASH, "=")];

/// Review fix 5: each mark row's own test, which fails when that row is taken
/// out of the table -- the mark is then nowhere in the text, drawn before its
/// glyph or after it.
#[test]
fn each_mark_row_puts_its_mark_after_its_glyph() {
    for (name, mark, base) in MARK_ROWS {
        let expected = format!("{base}{mark}");
        let after = [Draw::Text(102.8, base), Draw::Named(100.0)];
        let before = [Draw::Named(100.0), Draw::Text(102.8, base)];
        for draws in [&after[..], &before[..]] {
            let text = text_of(name, None, draws);
            assert_eq!(bare(&text), expected, "/{name}: {text:?}");
        }
    }
}

/// The circle drawn over a `c` twice, one after the other: the first follows
/// the `c`; the second is drawn directly after a mark, not after the `c`, so
/// it reads as its code did before the row (review 14-10, rule 2 narrowed).
#[test]
fn of_two_circles_over_one_c_only_the_first_follows_it() {
    let text = text_of(
        "circlecopyrt",
        None,
        &[
            Draw::Text(102.8, "c"),
            Draw::Named(100.0),
            Draw::Named(100.0),
        ],
    );
    assert_eq!(count(&text, CIRCLE), 1, "{text:?}");
    assert!(text.contains("c\u{20DD}"), "{text:?}");
    let without_row = text_of(
        UNKNOWN_NAME,
        None,
        &[Draw::Text(102.8, "c"), Draw::Named(100.0)],
    );
    assert_eq!(bare(&text).replace(CIRCLE, ""), bare(&without_row));
}

/// A mark is placed only on the glyph drawn directly before or after it. A
/// circle drawn after a whole run whose first glyph it encloses reads as its
/// code did before its row, like any circle over nothing: no lone U+20DD.
#[test]
fn a_circle_drawn_after_a_run_reads_as_without_the_row() {
    let draws = [Draw::Text(102.8, "c 1994"), Draw::Named(100.0)];
    let text = text_of("circlecopyrt", None, &draws);
    assert_eq!(count(&text, CIRCLE), 0, "{text:?}");
    assert_eq!(text, text_of(UNKNOWN_NAME, None, &draws));
}

/// Review fix 6: the words and the Markdown of a page carry the marks exactly
/// where its text does, for both rows and both drawing orders.
#[test]
fn words_and_markdown_read_the_marks_as_the_text_does() {
    let options = pdf_engine::ExtractOptions::default();
    for (name, mark, base) in MARK_ROWS {
        let after = [
            Draw::Text(50.0, "x"),
            Draw::Text(102.8, base),
            Draw::Named(100.0),
            Draw::Text(115.0, "y"),
        ];
        let before = [
            Draw::Text(50.0, "x"),
            Draw::Named(100.0),
            Draw::Text(102.8, base),
            Draw::Text(115.0, "y"),
        ];
        for draws in [&after[..], &before[..]] {
            let doc = document(name, None, draws);
            let text = bare(&doc.extract_text(0).expect("text"));
            assert_eq!(text, format!("x{base}{mark}y"), "/{name}");
            let words: String = doc
                .extract_words(&options)
                .expect("words")
                .iter()
                .map(|word| bare(&word.text))
                .collect();
            assert_eq!(words, text, "/{name}: words");
            let markdown = bare(&doc.extract_markdown(&options).expect("markdown"));
            assert_eq!(markdown, text, "/{name}: markdown");
        }
    }
}

/// The page's text with `/ActualText` replacements read.
fn replaced_text_of(name: &str, draws: &[Draw<'_>]) -> String {
    let mut options = pdf_engine::ExtractOptions::default();
    options.actual_text = true;
    bare(
        &document(name, None, draws)
            .extract_markdown(&options)
            .expect("markdown"),
    )
}

/// Review (b): a base whose text an `/ActualText` replacement stands in for
/// cannot take the mark, so the mark's glyph reads exactly as before its row,
/// whichever of the two was drawn first, for both rows.
#[test]
fn a_mark_whose_base_reads_as_its_actual_text_reads_as_without_the_row() {
    for (name, mark, base) in MARK_ROWS {
        let after = [
            Draw::Text(50.0, "x"),
            Draw::Replaced(102.8, base, "Q"),
            Draw::Named(100.0),
            Draw::Text(115.0, "y"),
        ];
        let before = [
            Draw::Text(50.0, "x"),
            Draw::Named(100.0),
            Draw::Replaced(102.8, base, "Q"),
            Draw::Text(115.0, "y"),
        ];
        for draws in [&after[..], &before[..]] {
            let text = replaced_text_of(name, draws);
            assert_eq!(count(&text, mark), 0, "/{name}: {text:?}");
            assert!(text.contains('Q'), "/{name}: {text:?}");
            assert_eq!(text, replaced_text_of(UNKNOWN_NAME, draws), "/{name}");
        }
    }
}

/// Review (c): a `/ToUnicode` U+FFFF drawn directly before or after a row
/// mark that does not reach it reads as today, and so does the mark.
#[test]
fn a_tounicode_nonchar_next_to_a_row_mark_reads_as_today() {
    for (name, _, _) in MARK_ROWS {
        for odd in [Odd::Nonchar, Odd::NoncharCircle] {
            let pages: [&[Draw<'_>]; 2] = [
                &[Draw::Odd(80.0, odd), Draw::Named(100.0)],
                &[Draw::Named(100.0), Draw::Odd(120.0, odd)],
            ];
            for draws in pages {
                let text = text_of(name, None, draws);
                assert_eq!(text, text_of(UNKNOWN_NAME, None, draws), "/{name}");
                assert!(text.contains('\u{FFFF}'), "/{name}: {text:?}");
            }
        }
    }
}

/// Review (c): a `/ToUnicode` U+FFFF alone reads as its value, and so does
/// U+FFFF followed by U+20DD -- never taken for a row's mark and dropped.
#[test]
fn a_tounicode_nonchar_alone_reads_as_today() {
    let text = text_of(UNKNOWN_NAME, None, &[Draw::Odd(100.0, Odd::Nonchar)]);
    assert_eq!(text, "\u{FFFF}");
    let text = text_of(UNKNOWN_NAME, None, &[Draw::Odd(100.0, Odd::NoncharCircle)]);
    assert_eq!(text, "\u{FFFF}\u{20DD}");
}

/// Review 14-10 02:00 (ii): a base the device may drop -- text the page
/// hides, which the output can leave out -- is not read again, so the mark
/// drawn over it reads exactly as before its row, for both rows and both
/// drawing orders.
#[test]
fn a_mark_over_a_hidden_base_reads_as_without_the_row() {
    for (name, mark, base) in MARK_ROWS {
        let after = [
            Draw::Text(50.0, "x"),
            Draw::Hidden(102.8, base),
            Draw::Named(100.0),
            Draw::Text(115.0, "y"),
        ];
        let before = [
            Draw::Text(50.0, "x"),
            Draw::Named(100.0),
            Draw::Hidden(102.8, base),
            Draw::Text(115.0, "y"),
        ];
        for draws in [&after[..], &before[..]] {
            let text = text_of(name, None, draws);
            assert_eq!(count(&text, mark), 0, "/{name}: {text:?}");
            assert_eq!(text, text_of(UNKNOWN_NAME, None, draws), "/{name}");
        }
    }
}

/// Review 14-10 02:00 (ii): when the device writes something after the base
/// before the mark arrives -- here a glyphless `/ActualText` replacement --
/// the base's text no longer stands last and cannot be read again, so the
/// mark reads exactly as before its row.
#[test]
fn a_mark_after_a_base_the_device_wrote_past_reads_as_without_the_row() {
    for (name, mark, base) in MARK_ROWS {
        let draws = [
            Draw::Text(50.0, "x"),
            Draw::Text(102.8, base),
            Draw::Glyphless("Z"),
            Draw::Named(100.0),
            Draw::Text(115.0, "y"),
        ];
        let text = replaced_text_of(name, &draws);
        assert_eq!(count(&text, mark), 0, "/{name}: {text:?}");
        assert!(text.contains('Z'), "/{name}: {text:?}");
        assert_eq!(text, replaced_text_of(UNKNOWN_NAME, &draws), "/{name}");
    }
}
