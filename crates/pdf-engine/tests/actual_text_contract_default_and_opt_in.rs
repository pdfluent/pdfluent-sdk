// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The `/ActualText` contract (D35-C): replacement text is opt-in.
//!
//! Two shapes that also occur in the public extraction gate set, rebuilt here
//! from scratch: a period whose ToUnicode entry says space but whose
//! `/ActualText` says period, and one invisible (mode 3) glyph whose ToUnicode
//! entry is a private-use code point while its `/ActualText` spells a word.
//! By default the page reads as its ToUnicode map says; with
//! [`ExtractOptions::actual_text`] it reads as the replacement. Without any
//! `/ActualText` both modes read the same, which is the negative control.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::{ExtractOptions, PdfDocument};

/// One code-to-Unicode pair per entry, as a ToUnicode CMap stream body.
fn to_unicode(pairs: &[(u8, &str)]) -> Vec<u8> {
    let mut body = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
         /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n\
         1 begincodespacerange\n<00> <FF>\nendcodespacerange\n",
    );
    body.push_str(&format!("{} beginbfchar\n", pairs.len()));
    for (code, hex) in pairs {
        body.push_str(&format!("<{code:02X}> <{hex}>\n"));
    }
    body.push_str("endbfchar\nendcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    body.into_bytes()
}

/// A page showing `runs` in order with one Helvetica font carrying `cmap`.
/// A run with `Some(text)` sits inside `/Span <</ActualText text>> BDC ... EMC`.
fn page(cmap: &[(u8, &str)], render_mode: i64, runs: &[(&[u8], Option<&str>)]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let cmap_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        to_unicode(cmap),
    )));
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        "Encoding" => "WinAnsiEncoding", "ToUnicode" => cmap_id,
    });
    let mut ops = vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec!["F1".into(), 24.into()]),
        Operation::new("Tr", vec![render_mode.into()]),
        Operation::new("Td", vec![72.into(), 700.into()]),
    ];
    for (bytes, actual) in runs {
        if let Some(text) = actual {
            ops.push(Operation::new(
                "BDC",
                vec![
                    Object::Name(b"Span".to_vec()),
                    Object::Dictionary(
                        dictionary! { "ActualText" => Object::string_literal(*text) },
                    ),
                ],
            ));
        }
        ops.push(Operation::new(
            "Tj",
            vec![Object::String(bytes.to_vec(), lopdf::StringFormat::Literal)],
        ));
        if actual.is_some() {
            ops.push(Operation::new("EMC", vec![]));
        }
    }
    ops.push(Operation::new("ET", vec![]));
    let content = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        Content { operations: ops }.encode().expect("encode"),
    )));
    let pages_id = doc.new_object_id();
    let page = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => pages_id, "Contents" => content,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(
            dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 },
        ),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save");
    out
}

fn text(pdf: &[u8], actual_text: bool) -> String {
    // The default mode is `ExtractOptions::default()` untouched: the contract is about the default.
    let mut options = ExtractOptions::default();
    if actual_text {
        options.actual_text = true;
    }
    PdfDocument::open(pdf.to_vec())
        .expect("open")
        .extract_blocks(&options)
        .expect("extract")
        .iter()
        .map(|block| block.text.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

const DIGITS_AND_PERIOD: &[(u8, &str)] = &[(0x30, "0030"), (0x31, "0031"), (0x2E, "0020")];
const PRIVATE_USE: &[(u8, &str)] = &[(0x01, "E020")];

#[test]
fn a_period_mapped_to_space_reads_as_its_map_by_default() {
    let pdf = page(
        DIGITS_AND_PERIOD,
        0,
        &[(b"0", None), (b".", Some(".")), (b"1", None)],
    );
    assert_eq!(text(&pdf, false), "0 1");
}

#[test]
fn a_period_mapped_to_space_reads_as_its_replacement_when_opted_in() {
    let pdf = page(
        DIGITS_AND_PERIOD,
        0,
        &[(b"0", None), (b".", Some(".")), (b"1", None)],
    );
    assert_eq!(text(&pdf, true), "0.1");
}

#[test]
fn an_invisible_private_use_glyph_reads_as_its_map_by_default() {
    let pdf = page(PRIVATE_USE, 3, &[(b"\x01", Some("ABC"))]);
    assert_eq!(text(&pdf, false), "\u{E020}");
}

#[test]
fn an_invisible_private_use_glyph_reads_as_its_replacement_when_opted_in() {
    let pdf = page(PRIVATE_USE, 3, &[(b"\x01", Some("ABC"))]);
    assert_eq!(text(&pdf, true), "ABC");
}

#[test]
fn without_actual_text_both_modes_read_the_same() {
    let pdf = page(
        DIGITS_AND_PERIOD,
        0,
        &[(b"0", None), (b".", None), (b"1", None)],
    );
    assert_eq!(text(&pdf, false), "0 1");
    assert_eq!(text(&pdf, true), "0 1");
    let pdf = page(PRIVATE_USE, 3, &[(b"\x01", None)]);
    assert_eq!(text(&pdf, false), "\u{E020}");
    assert_eq!(text(&pdf, true), "\u{E020}");
}
