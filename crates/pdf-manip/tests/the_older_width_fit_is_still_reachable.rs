// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A plain conversion corrects the widths and scales, which is the default.
//!
//! Both ways keep the next glyph in place and they do not look the same: a
//! horizontal scale stretches the letterform to the declared width,
//! re-metricking keeps the letterform and gives it the room. The corpus decides
//! which is better, and it can only decide if one binary produces either (#380).
//!
//! ITS OWN TEST BINARY, and that is the point of the file rather than an
//! accident of layout. The variable is read at conversion time and every test
//! in a binary shares one environment: run beside the test that asserts the
//! DEFAULT behaviour, this one sets the variable out from under it, and the two
//! fail each other at random. That is the same defect #495 was, found here
//! before it was pushed rather than after a wave went red.

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

/// The `/Widths` entry for one code, in the converted document.
fn width_of(doc: &Document, code: u8) -> Option<i64> {
    for (_, obj) in doc.objects.iter() {
        let Ok(dict) = obj.as_dict() else { continue };
        if dict.get(b"Type").and_then(Object::as_name).ok() != Some(b"Font") {
            continue;
        }
        let Ok(base) = dict.get(b"BaseFont").and_then(Object::as_name) else {
            continue;
        };
        if !String::from_utf8_lossy(base).contains("AGaramond") {
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
        return widths.get(index).and_then(|o| match o {
            Object::Integer(i) => Some(*i),
            Object::Real(r) => Some(*r as i64),
            _ => None,
        });
    }
    None
}

#[test]
fn the_scaled_fit_is_what_a_plain_conversion_does() {
    let output =
        convert_bytes(&document(), &PdfAConvertOptions::default()).expect("the fixture converts");

    let converted = Document::load_mem(&output).expect("the output parses");

    let moved: Vec<_> = CODES
        .iter()
        .filter_map(|(code, source)| {
            let now = width_of(&converted, *code)?;
            (now != *source).then_some((*code as char, *source, now))
        })
        .collect();
    assert!(
        !moved.is_empty(),
        "a plain conversion corrects the widths towards the face, which the \
         horizontal-scale compensation then undoes. Nothing moved, so the substitution \
         is not running at all."
    );
}
