//! Choice fields read as their display labels (lane L3).
//!
//! A choice option pairs an export value with the label the reader sees,
//! and the field's value names the export: `/Opt [[(a) (Alpha)] [(b)
//! (Beta)]]` with `/V (b)` means Beta. Today the label half never decodes,
//! so the field reads as its export `b` -- a code, not what the form shows.
//! The rebuilt engine resolves the selected option to its display label.
//!
//! Unspecified; our behaviour: the front door maps each form field to its
//! value with its pages (G194), and our all-text path carries field values
//! the same way; the value of a choice is the label the reader chose.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::PdfDocument;

/// A one-line page carrying a two-option choice field selecting `(b)`.
fn choice_field_pdf() -> Vec<u8> {
    use lopdf::content::{Content, Operation};

    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let ops = vec![
        Operation::new("BT", vec![]),
        Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), 10.into()]),
        Operation::new("Td", vec![72.into(), 700.into()]),
        Operation::new("Tj", vec![Object::string_literal("Form:")]),
        Operation::new("ET", vec![]),
    ];
    let content_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        Content { operations: ops }.encode().expect("encode"),
    )));
    let option = |export: &[u8], display: &[u8]| {
        Object::Array(vec![
            Object::String(export.to_vec(), StringFormat::Literal),
            Object::String(display.to_vec(), StringFormat::Literal),
        ])
    };
    let widget_id = doc.new_object_id();
    let pages_id = doc.new_object_id();
    let page_id = doc.new_object_id();
    let acroform_id = doc.new_object_id();
    doc.objects.insert(
        widget_id,
        Object::Dictionary(dictionary! {
            "Type" => "Annot",
            "Subtype" => "Widget",
            "FT" => "Ch",
            "T" => Object::String(b"country".to_vec(), StringFormat::Literal),
            "Opt" => Object::Array(vec![
                option(b"a", b"Alpha"),
                option(b"b", b"Beta"),
            ]),
            "V" => Object::String(b"b".to_vec(), StringFormat::Literal),
            "Rect" => Object::Array(vec![100.into(), 700.into(), 260.into(), 730.into()]),
            "P" => Object::Reference(page_id),
        }),
    );
    doc.objects.insert(
        page_id,
        Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
            "Annots" => Object::Array(vec![Object::Reference(widget_id)]),
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(dictionary! {
                "Font" => Object::Dictionary(dictionary! { "F1" => Object::Reference(font) }),
            }),
        }),
    );
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
            "Count" => Object::Integer(1),
        }),
    );
    doc.objects.insert(
        acroform_id,
        Object::Dictionary(dictionary! {
            "Fields" => Object::Array(vec![Object::Reference(widget_id)]),
        }),
    );
    let catalog_id = doc.new_object_id();
    doc.objects.insert(
        catalog_id,
        Object::Dictionary(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
            "AcroForm" => Object::Reference(acroform_id),
        }),
    );
    doc.trailer.set("Root", Object::Reference(catalog_id));
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save pdf");
    out
}

#[test]
fn a_selected_choice_reads_as_its_display_label() {
    let text = PdfDocument::open(choice_field_pdf())
        .expect("open")
        .extract_all_text();
    assert!(
        text.contains("Beta"),
        "the choice reads as its export value, not its label: {text:?}"
    );
}
