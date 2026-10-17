// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A substituted face CAN be fitted to the widths the author wrote.
//!
//! Off by default since the corpus measured it (run 34521732841): the older
//! way, a horizontal scale, renders closer to source on 29 documents against 9
//! and produces 5% less output. These fixtures therefore ask for the mechanism
//! explicitly. They stay because the mechanism stays, and because what each of
//! them reproduces is a real defect in it that review found.
//!
//! The defect (#380, cause 2). PDF/A wants every font embedded and many
//! documents only name one, so a converter supplies a face. The Liberation
//! faces we carry are metric-compatible with Helvetica, Times and Courier --
//! for those three, substituting changes nothing. Nothing else in the world is
//! metric-compatible with them, and that is where the text moves.
//!
//! Measured on govdocs `001_001672`, which draws five variants of AGaramond and
//! embeds none: after conversion, 67 of the 84 codes with a real width had a
//! different one. `A` went from 623 to 722 -- the Times advance -- and `D` from
//! 780 to 722. Nearly 16 % on a single letter, so every line re-spaces. Its
//! 0.0568 mean render difference is not lost text; it is text that moved.
//!
//! It stays conformant while doing it, and that is the trap. veraPDF
//! §6.2.11.5 asks that `/Widths` agree with the embedded program, and it does
//! -- because the widths were rewritten towards the substitute. The rule is
//! satisfied by moving the wrong side.
//!
//! This asks for the other side to move: the declared widths are the author's
//! and are kept, and the embedded program is re-metricked to them.

use lopdf::{dictionary, Document, Object, Stream};
use pdf_manip::pdfa::{convert_bytes, PdfAConvertOptions};

/// Adobe Garamond's real advances for the codes this fixture draws, which are
/// nothing like Times's. Taken from the source font of `001_001672`.
const CODES: &[(u8, i64)] = &[
    (b'A', 623),
    (b'B', 605),
    (b'D', 780),
    (b'H', 806),
    (b'e', 444),
    (b'n', 500),
];

/// A page that names a typeface it does not carry, and says how wide it is.
fn document() -> Vec<u8> {
    let mut doc = Document::with_version("1.4");
    let pages_id = doc.new_object_id();

    let first = CODES[0].0;
    let last = CODES[CODES.len() - 1].0;
    let widths: Vec<Object> = (first..=last)
        .map(|code| {
            Object::Integer(
                CODES
                    .iter()
                    .find(|(c, _)| *c == code)
                    .map_or(500, |(_, w)| *w),
            )
        })
        .collect();

    let font = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "AGaramond-Regular",
        "Encoding" => "WinAnsiEncoding",
        "FirstChar" => i64::from(first),
        "LastChar" => i64::from(last),
        "Widths" => Object::Array(widths),
    });

    let text: String = CODES.iter().map(|(c, _)| *c as char).collect();
    let content = format!("BT /F1 12 Tf 72 700 Td ({text}) Tj ET");
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        "Contents" => content_id,
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).unwrap();
    out
}

/// The `/Widths` entry for one code of the AGaramond font.
fn width_of(doc: &Document, code: u8) -> Option<i64> {
    width_of_font(doc, "AGaramond", code)
}

/// Convert with the re-metricking mechanism asked for.
///
/// It is off by default since the corpus chose the scaled fit, so every fixture
/// here has to ask. The lock is not decoration: these tests share one process
/// and one environment, and one of them setting the variable while another
/// converts is the defect #495 was.
fn convert_asking_for_remetric(raw: &[u8]) -> Vec<u8> {
    use std::sync::{Mutex, OnceLock};
    static ENV: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = ENV
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    std::env::set_var("PDFA_REMETRIC_SUBSTITUTES", "1");
    let out = convert_bytes(raw, &PdfAConvertOptions::default());
    std::env::remove_var("PDFA_REMETRIC_SUBSTITUTES");
    out.expect("the fixture converts")
}

/// The `/Widths` entry for one code, as declared -- fraction and all.
///
/// `width_of` rounds, which is fine for the whole numbers most fixtures use and
/// useless for the one thing a fractional width has to prove.
fn exact_width_of(doc: &Document, code: u8) -> Option<f64> {
    raw_width_of(doc, "AGaramond", code).and_then(|o| match o {
        Object::Integer(i) => Some(i as f64),
        Object::Real(r) => Some(f64::from(r)),
        _ => None,
    })
}

/// The `/Widths` entry for one code of the font whose BaseFont contains `name`.
fn width_of_font(doc: &Document, name: &str, code: u8) -> Option<i64> {
    raw_width_of(doc, name, code).and_then(|o| match o {
        Object::Integer(i) => Some(i),
        Object::Real(r) => Some(r as i64),
        _ => None,
    })
}

fn raw_width_of(doc: &Document, name: &str, code: u8) -> Option<Object> {
    for (_, obj) in doc.objects.iter() {
        let Ok(dict) = obj.as_dict() else { continue };
        if dict.get(b"Type").and_then(Object::as_name).ok() != Some(b"Font") {
            continue;
        }
        let Ok(base) = dict.get(b"BaseFont").and_then(Object::as_name) else {
            continue;
        };
        if !String::from_utf8_lossy(base).contains(name) {
            continue;
        }
        let first = dict.get(b"FirstChar").and_then(Object::as_i64).ok()?;
        let widths = dict
            .get(b"Widths")
            .and_then(|o| doc.dereference(o).map(|(_, o)| o))
            .ok()?
            .as_array()
            .ok()?
            .clone();
        let index = usize::try_from(i64::from(code) - first).ok()?;
        return widths.get(index).cloned();
    }
    None
}

/// A font that names no encoding is read through the program's own.
///
/// Found by review on #1880, and it is the difference between fitting the glyph
/// the page draws and fitting a different one. With no `/Encoding`, a reader
/// takes the embedded program's built-in encoding; the generic ASCII/WinAnsi
/// fallback disagrees with it for codes like 39 and 96 and most of the upper
/// byte. Re-metricking the wrong glyph is worse than not re-metricking at all:
/// the width this code draws is still corrected away from the author's value,
/// AND a glyph nobody asked about now carries an advance it did not have.
#[test]
fn a_font_with_no_encoding_is_read_through_the_programs_own() {
    // 39 is `quoteright` in the built-in encoding and `quotesingle` in the
    // generic fallback: two different glyphs with two different advances, which
    // is exactly why this code was chosen.
    const CODE: u8 = 39;
    const DECLARED: i64 = 611;

    let mut doc = Document::with_version("1.4");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "AGaramond-Regular",
        // No /Encoding at all. That is the point of the fixture.
        "FirstChar" => i64::from(CODE),
        "LastChar" => i64::from(CODE),
        "Widths" => Object::Array(vec![Object::Integer(DECLARED)]),
    });
    let content = format!("BT /F1 12 Tf 72 700 Td ({}) Tj ET", CODE as char);
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        "Contents" => content_id,
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut raw = Vec::new();
    doc.save_to(&mut raw).unwrap();

    let output = convert_asking_for_remetric(&raw);
    let converted = Document::load_mem(&output).expect("the output parses");
    let now = width_of(&converted, CODE).expect("the font survives the conversion");
    assert_eq!(
        now, DECLARED,
        "code {CODE} was declared {DECLARED} and came back {now}: with no /Encoding the \
         glyph has to be resolved through the program's built-in encoding, and this \
         resolved a different one"
    );
}

/// A symbolic face is fitted against the encoding it ends up with.
///
/// Found by review on #1880. An unembedded `Symbol` or `ZapfDingbats` gets its
/// Annex D built-in encoding written in as explicit `/Differences` during the
/// conversion, so a fit computed before that ran picked glyphs for codes that
/// no longer mean the same thing: the author's widths were lost anyway, and an
/// unrelated glyph was left carrying an advance it never had. The fit is now
/// after the encoding is settled, and this holds it there.
#[test]
fn a_symbolic_font_is_fitted_after_its_encoding_is_settled() {
    // Codes drawn from Symbol's own encoding, with advances that are the
    // author's rather than the face's.
    const CODES: &[(u8, i64)] = &[(0x61, 700), (0x62, 690), (0x67, 660)];

    let mut doc = Document::with_version("1.4");
    let pages_id = doc.new_object_id();
    let first = CODES[0].0;
    let last = CODES[CODES.len() - 1].0;
    let widths: Vec<Object> = (first..=last)
        .map(|code| {
            Object::Integer(
                CODES
                    .iter()
                    .find(|(c, _)| *c == code)
                    .map_or(600, |(_, w)| *w),
            )
        })
        .collect();
    let font = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Symbol",
        "FirstChar" => i64::from(first),
        "LastChar" => i64::from(last),
        "Widths" => Object::Array(widths),
    });
    let text: String = CODES.iter().map(|(c, _)| *c as char).collect();
    let content = format!("BT /F1 12 Tf 72 700 Td ({text}) Tj ET");
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        "Contents" => content_id,
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut raw = Vec::new();
    doc.save_to(&mut raw).unwrap();

    let output = convert_asking_for_remetric(&raw);
    let converted = Document::load_mem(&output).expect("the output parses");

    let moved: Vec<_> = CODES
        .iter()
        .filter_map(|(code, declared)| {
            let now = width_of_font(&converted, "Symbol", *code)?;
            (now != *declared).then_some((*code, *declared, now))
        })
        .collect();
    assert!(
        moved.is_empty(),
        "a symbolic substitution moved the author's widths: {moved:?} (code, declared, \
         after). The fit has to run against the encoding the document ends up with, not \
         the one it arrived with."
    );
}

/// WinAnsi codes 160 and 173, and a declared zero.
///
/// Both found by review on #1880.
///
/// The Type 1 encoding calls code 160 `space` and code 173 `hyphen`; the
/// Unicode round-trip asks for `nbspace` and `softhyphen`, which the bundled
/// faces do not carry. The raw fallback then picked whatever StandardEncoding
/// put at that code -- `guilsinglright` for 173 -- so an unrelated glyph was
/// re-metricked while the real one kept the substitute's advance. It is also
/// the table veraPDF judges our widths against.
///
/// And a declared zero is a declaration: an overprinting mark draws and moves
/// nothing. Skipped, it came back as the substitute glyph's positive advance
/// and shifted every glyph after it.
#[test]
fn the_exact_winansi_names_and_a_declared_zero_survive() {
    const CODES: &[(u8, i64)] = &[
        (160, 611), // space in the Type 1 encoding, nbspace in the AGL
        (173, 588), // hyphen there, softhyphen in the AGL
        (174, 0),   // a deliberate zero advance
    ];

    let mut doc = Document::with_version("1.4");
    let pages_id = doc.new_object_id();
    let first = CODES[0].0;
    let last = CODES[CODES.len() - 1].0;
    let widths: Vec<Object> = (first..=last)
        .map(|code| {
            Object::Integer(
                CODES
                    .iter()
                    .find(|(c, _)| *c == code)
                    .map_or(500, |(_, w)| *w),
            )
        })
        .collect();
    let font = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "AGaramond-Regular",
        "Encoding" => "WinAnsiEncoding",
        "FirstChar" => i64::from(first),
        "LastChar" => i64::from(last),
        "Widths" => Object::Array(widths),
    });
    let drawn: String = CODES.iter().map(|(c, _)| *c as char).collect();
    let content = format!(
        "BT /F1 12 Tf 72 700 Td <{}> Tj ET",
        drawn
            .chars()
            .map(|c| format!("{:02x}", c as u32))
            .collect::<String>()
    );
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        "Contents" => content_id,
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut raw = Vec::new();
    doc.save_to(&mut raw).unwrap();

    let output = convert_asking_for_remetric(&raw);
    let converted = Document::load_mem(&output).expect("the output parses");

    let moved: Vec<_> = CODES
        .iter()
        .filter_map(|(code, declared)| {
            let now = width_of(&converted, *code)?;
            (now != *declared).then_some((*code, *declared, now))
        })
        .collect();
    assert!(
        moved.is_empty(),
        "these declarations did not survive: {moved:?} (code, declared, after). 160 and \
         173 are `space` and `hyphen` in the Type 1 encoding, and a declared zero is a \
         declaration."
    );
}

/// MacRoman's code 202 is the Type 1 `space` too.
///
/// The same disagreement as WinAnsi's 160, reached through another encoding:
/// the round-trip calls it `nbspace`, the bundled faces do not carry that name,
/// and the raw fallback then picks StandardEncoding's `ring`. Found by review on
/// #1880 after the WinAnsi form of it, which is why the alias is asked for by
/// name rather than per encoding.
#[test]
fn a_macroman_alias_resolves_to_the_glyph_the_face_carries() {
    const CODE: u8 = 202;
    const DECLARED: i64 = 611;

    let mut doc = Document::with_version("1.4");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "AGaramond-Regular",
        "Encoding" => "MacRomanEncoding",
        "FirstChar" => i64::from(CODE),
        "LastChar" => i64::from(CODE),
        "Widths" => Object::Array(vec![Object::Integer(DECLARED)]),
    });
    let content = format!("BT /F1 12 Tf 72 700 Td <{CODE:02x}> Tj ET");
    let content_id = doc.add_object(Stream::new(dictionary! {}, content.into_bytes()));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        "Contents" => content_id,
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut raw = Vec::new();
    doc.save_to(&mut raw).unwrap();

    let output = convert_asking_for_remetric(&raw);
    let converted = Document::load_mem(&output).expect("the output parses");
    let now = width_of(&converted, CODE).expect("the font survives the conversion");
    assert_eq!(
        now, DECLARED,
        "MacRoman code {CODE} was declared {DECLARED} and came back {now}: it is the \
         Type 1 `space`, and the AGL name for it is not in the face"
    );
}

/// Two codes on one glyph, declaring nearly the same width, is still two.
///
/// A charstring carries one advance, so when two codes resolve to the same
/// glyph and declare different widths, neither may be fitted -- one of them
/// would get an advance its document never declared. "Different" was measured
/// to half a unit, left over from when the advance was rounded anyway, so
/// 500.0 and 500.4 counted as the same and the second silently replaced the
/// first. Found by review on #1880.
#[test]
fn two_codes_wanting_different_widths_from_one_glyph_are_left_alone() {
    // Both names map to the same glyph in the substitute, with widths that
    // differ by less than the old half-unit tolerance.
    let mut doc = Document::with_version("1.4");
    let pages_id = doc.new_object_id();
    let differences = Object::Array(vec![
        Object::Integer(65),
        Object::Name(b"A".to_vec()),
        Object::Integer(66),
        Object::Name(b"A".to_vec()),
    ]);
    let font = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "AGaramond-Regular",
        "Encoding" => dictionary! {
            "Type" => "Encoding",
            "BaseEncoding" => "WinAnsiEncoding",
            "Differences" => differences,
        },
        "FirstChar" => 65,
        "LastChar" => 66,
        "Widths" => Object::Array(vec![Object::Real(500.0), Object::Real(500.4)]),
    });
    let content_id = doc.add_object(Stream::new(
        dictionary! {},
        b"BT /F1 12 Tf 72 700 Td (AB) Tj ET".to_vec(),
    ));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        "Contents" => content_id,
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut raw = Vec::new();
    doc.save_to(&mut raw).unwrap();

    let output = convert_asking_for_remetric(&raw);
    let converted = Document::load_mem(&output).expect("the output parses");
    let a = width_of(&converted, 65).expect("code 65 survives");
    let b = width_of(&converted, 66).expect("code 66 survives");
    // Both take the substitute's advance, because the glyph was left alone.
    // Equal is not the test -- they are equal either way, since one glyph has
    // one advance. The test is that NEITHER declared width was written into it:
    // fitting the glyph to 500 gives code 66 an advance its document never
    // declared, and fitting it to 500.4 does the same to code 65.
    assert_eq!(a, b, "one glyph, one advance");
    assert!(
        !(499..=501).contains(&a),
        "codes 65 and 66 declare 500.0 and 500.4 for the SAME glyph, so neither may be \
         fitted -- and the advance came back as {a}, which is one of them. A charstring \
         carries one width; giving it either code's is giving the other one a width its \
         document never declared."
    );
}

/// A fractional declared width survives into the `/Widths` array, not just the
/// program.
///
/// The first version of this checked the rewritten CFF and stopped there, so it
/// passed while `update_metrics_from_cff` rebuilt the array from that program
/// and rounded 500.4 back to 500 -- the preservation undone one step after it
/// happened, by the pass that was supposed to find nothing to correct. Found by
/// review on #1880. A test that watches one half of a round trip is a test of
/// half of it.
#[test]
fn a_fractional_width_survives_into_the_widths_array() {
    const CODE: u8 = b'A';
    const DECLARED: f64 = 500.4;

    let mut doc = Document::with_version("1.4");
    let pages_id = doc.new_object_id();
    let font = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "AGaramond-Regular",
        "Encoding" => "WinAnsiEncoding",
        "FirstChar" => i64::from(CODE),
        "LastChar" => i64::from(CODE),
        "Widths" => Object::Array(vec![Object::Real(DECLARED as f32)]),
    });
    let content_id = doc.add_object(Stream::new(
        dictionary! {},
        b"BT /F1 12 Tf 72 700 Td (A) Tj ET".to_vec(),
    ));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        "Contents" => content_id,
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut raw = Vec::new();
    doc.save_to(&mut raw).unwrap();

    let output = convert_asking_for_remetric(&raw);
    let converted = Document::load_mem(&output).expect("the output parses");
    let now = exact_width_of(&converted, CODE).expect("the font survives the conversion");
    assert!(
        (now - DECLARED).abs() < 0.01,
        "code A was declared {DECLARED} and the array came back with {now}: the fraction \
         reached the font program and was then rounded out of the dictionary beside it"
    );
}

#[test]
fn a_substitution_keeps_the_widths_the_author_declared() {
    let output = convert_asking_for_remetric(&document());
    let converted = Document::load_mem(&output).expect("the output parses");

    let mut moved = Vec::new();
    for (code, source) in CODES {
        let now = width_of(&converted, *code)
            .unwrap_or_else(|| panic!("no width for {:?} after conversion", *code as char));
        if now != *source {
            moved.push((*code as char, *source, now));
        }
    }

    assert!(
        moved.is_empty(),
        "the substitution moved the author's widths instead of fitting the face to them: \
         {moved:?} (code, declared, after). Every line of this typeface re-spaces by that \
         much, which is #380 cause 2 -- and it passes PDF/A while doing it, because \
         §6.2.11.5 was satisfied by moving the widths rather than the program."
    );
}
