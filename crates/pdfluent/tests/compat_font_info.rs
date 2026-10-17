// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! `module.repair_mono_font` and `module.UpdateFontInfo` (#643).
//!
//! A ragged Courier normalises to its common width while proportional
//! faces pass through untouched; font info records validate against the
//! live fonts and store their base names back.

use lopdf::{dictionary, Document, Object, Stream};
use pdfluent::compat::{repair_mono_font, update_font_info, Page, PageMut, Value};
use pdfluent::prelude::*;

const SAMPLE_PDF: &[u8] = include_bytes!("../../../fixtures/sample.pdf");

fn font_fixture() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let content_id = doc.add_object(Stream::new(
        dictionary! {},
        b"BT /F1 12 Tf 72 720 Td (Hi) Tj ET".to_vec(),
    ));
    let courier_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Courier",
        "FirstChar" => 32_i64,
        "LastChar" => 34_i64,
        "Widths" => vec![Object::Integer(600), Object::Integer(600), Object::Integer(300)],
    });
    let helv_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type1",
        "BaseFont" => "Helvetica",
        "FirstChar" => 32_i64,
        "LastChar" => 33_i64,
        "Widths" => vec![Object::Integer(500), Object::Integer(600)],
    });
    let cid_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "CIDFontType2",
        "BaseFont" => "ABCDEF+NotoSans",
        "CIDSystemInfo" => dictionary! {
            "Registry" => "Adobe",
            "Ordering" => "Identity",
            "Supplement" => 0_i64,
        },
        "DW" => 1000_i64,
    });
    let composite_id = doc.add_object(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => "ABCDEF+NotoSans",
        "Encoding" => "Identity-H",
        "DescendantFonts" => vec![Object::Reference(cid_id)],
    });
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => dictionary! {
            "Font" => dictionary! {
                "F1" => courier_id,
                "F2" => helv_id,
                "F3" => composite_id,
            },
        },
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        }),
    );
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("fixture saves");
    bytes
}

fn widths_of(bytes: &[u8], xref: i32) -> Vec<i64> {
    let doc = Document::load_mem(bytes).expect("bytes parse");
    let dict = doc.get_dictionary((xref as u32, 0)).expect("font resolves");
    let raw = dict.get(b"Widths").expect("widths present").clone();
    let items = match raw {
        Object::Array(items) => items,
        Object::Reference(id) => doc
            .get_object(id)
            .expect("widths resolve")
            .as_array()
            .expect("widths are an array")
            .clone(),
        _ => panic!("unexpected widths slot"),
    };
    items
        .into_iter()
        .map(|o| o.as_i64().expect("widths are integers"))
        .collect()
}

#[test]
fn ragged_courier_normalises_proportional_passes() {
    let mut doc = PdfDocument::from_bytes(&font_fixture()).expect("fixture opens");
    let fonts = Page::new(&doc, 0).get_fonts(false).expect("fonts list");
    let courier = fonts
        .iter()
        .find(|f| f.name == "Courier")
        .expect("courier lists");
    let helv = fonts
        .iter()
        .find(|f| f.name == "Helvetica")
        .expect("helvetica lists");
    let (courier_xref, helv_xref) = (courier.xref, helv.xref);

    let mut view = PageMut::new(&mut doc, 0);
    repair_mono_font(&mut view, courier).expect("courier repairs");
    repair_mono_font(&mut view, helv).expect("helvetica passes");

    let bytes = doc.to_bytes().expect("save works");
    assert_eq!(widths_of(&bytes, courier_xref), vec![600, 600, 600]);
    assert_eq!(widths_of(&bytes, helv_xref), vec![500, 600]);
}

#[test]
fn sample_helvetica_needs_no_repair() {
    let mut doc = PdfDocument::from_bytes(SAMPLE_PDF).expect("sample opens");
    let fonts = Page::new(&doc, 0).get_fonts(false).expect("fonts list");
    let helv = fonts
        .iter()
        .find(|f| f.name == "Helvetica")
        .expect("sample carries helvetica");
    let mut view = PageMut::new(&mut doc, 0);
    repair_mono_font(&mut view, helv).expect("proportional passes");
}

#[test]
fn composite_and_missing_fonts_keep_the_raise() {
    let mut doc = PdfDocument::from_bytes(&font_fixture()).expect("fixture opens");
    let fonts = Page::new(&doc, 0).get_fonts(false).expect("fonts list");
    let composite = fonts
        .iter()
        .find(|f| f.subtype == "Type0")
        .expect("fixture carries a composite font");
    let outcome = {
        let mut view = PageMut::new(&mut doc, 0);
        repair_mono_font(&mut view, composite)
    };
    match outcome {
        Err(pdfluent::Error::Unsupported(reason)) => assert!(
            reason.contains("module.repair_mono_font"),
            "loud raise: {reason}"
        ),
        Err(other) => panic!("wrong error class: {other:?}"),
        Ok(()) => panic!("silent success, expected Unsupported"),
    }
}

fn record(xref: i32, name: &str) -> Value {
    Value::List(vec![
        Value::Int(i64::from(xref)),
        Value::Str("n/a".to_string()),
        Value::Str("Type1".to_string()),
        Value::Str(name.to_string()),
        Value::Str("F1".to_string()),
        Value::Str("WinAnsi".to_string()),
        Value::Int(0),
    ])
}

#[test]
fn info_records_validate_and_store() {
    let mut doc = PdfDocument::from_bytes(&font_fixture()).expect("fixture opens");
    let fonts = Page::new(&doc, 0).get_fonts(false).expect("fonts list");
    let courier = fonts
        .iter()
        .find(|f| f.name == "Courier")
        .expect("courier lists");

    update_font_info(&mut doc, &[record(courier.xref, "Courier")]).expect("matching stores");
    update_font_info(&mut doc, &[]).expect("empty stores nothing");

    update_font_info(&mut doc, &[record(courier.xref, "Courier-Bold")]).expect("rename stores");
    let fonts = Page::new(&doc, 0).get_fonts(false).expect("fonts list");
    assert!(
        fonts.iter().any(|f| f.name == "Courier-Bold"),
        "renamed face lists: {fonts:?}"
    );

    let err =
        update_font_info(&mut doc, &[record(9999, "Courier")]).expect_err("unknown xref fails");
    assert!(err.to_string().contains("names no font"), "{err}");

    let err = update_font_info(&mut doc, &[Value::Int(7)]).expect_err("short record fails");
    assert_eq!(
        err.compat_kind(),
        Some(&pdfluent::compat::ErrorKind::WrongType),
        "{err:?}"
    );
}
