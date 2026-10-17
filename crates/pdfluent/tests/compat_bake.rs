// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! `Document.bake` through the facade (#648).
//!
//! Annotations flatten into page content (gaining a generated appearance
//! first when they have none), widgets flatten through the form pipeline,
//! and both flags off changes nothing. A field the flattener skips fails
//! the bake by name instead of silently staying editable.

use lopdf::{dictionary, Document, Object, Stream};
use pdfluent::compat::document::BakeOptions;
use pdfluent::compat::PageMut;
use pdfluent::prelude::*;

const ANNOTATIONS_PDF: &[u8] =
    include_bytes!("../../../crates/visual-regression/fixtures/annotations.pdf");
const ACROFORM_PDF: &[u8] = include_bytes!("../../../fixtures/acroform.pdf");

fn bake_all() -> BakeOptions {
    BakeOptions::default()
}

fn bake_none() -> BakeOptions {
    let mut options = BakeOptions::default();
    options.annots = false;
    options.widgets = false;
    options
}

fn annot_count(doc: &mut PdfDocument, page: usize) -> usize {
    PageMut::new(doc, page)
        .annots(None)
        .expect("list annots")
        .len()
}

fn widget_count(doc: &mut PdfDocument, page: usize) -> usize {
    PageMut::new(doc, page)
        .widgets(None)
        .expect("list widgets")
        .len()
}

#[test]
fn annots_list_excludes_widgets() {
    let mut doc = PdfDocument::from_bytes(ACROFORM_PDF).expect("open acroform.pdf");
    assert_eq!(annot_count(&mut doc, 0), 0);
    assert_eq!(widget_count(&mut doc, 0), 2);
}

#[test]
fn bake_flattens_annots_and_leaves_a_valid_document() {
    let mut doc = PdfDocument::from_bytes(ANNOTATIONS_PDF).expect("open annotations.pdf");
    assert_eq!(annot_count(&mut doc, 0), 3);

    doc.bake(&bake_all()).expect("bake");

    assert_eq!(annot_count(&mut doc, 0), 0);
    let after = doc.to_bytes().expect("serialise after bake");
    assert!(
        !contains(&after, b"/Annots"),
        "an /Annots array survived the bake"
    );
    let reopened = PdfDocument::from_bytes(&after).expect("baked bytes re-open");
    assert_eq!(reopened.page_count(), 1);
}

#[test]
fn bake_backfills_a_missing_appearance_first() {
    let bytes = pdf_with_bare_square();
    let mut doc = PdfDocument::from_bytes(&bytes).expect("open operator PDF");
    assert_eq!(annot_count(&mut doc, 0), 1);

    doc.bake(&bake_all()).expect("bake");

    // Without the backfill the annot has no /AP /N, flatten skips it, and
    // this count stays 1.
    assert_eq!(annot_count(&mut doc, 0), 0);
}

#[test]
fn bake_flattens_widgets_and_drops_the_form() {
    let mut doc = PdfDocument::from_bytes(ACROFORM_PDF).expect("open acroform.pdf");
    assert_eq!(widget_count(&mut doc, 0), 2);

    doc.bake(&bake_all()).expect("bake");

    assert_eq!(widget_count(&mut doc, 0), 0);
    assert_eq!(annot_count(&mut doc, 0), 0);
    let after = doc.to_bytes().expect("serialise after bake");
    assert!(
        !contains(&after, b"/AcroForm"),
        "the AcroForm dictionary survived the bake"
    );
    let reopened = PdfDocument::from_bytes(&after).expect("baked bytes re-open");
    assert_eq!(reopened.page_count(), 1);
}

#[test]
fn bake_with_both_flags_off_changes_nothing() {
    let mut doc = PdfDocument::from_bytes(ANNOTATIONS_PDF).expect("open annotations.pdf");
    doc.bake(&bake_none()).expect("bake with flags off");
    assert_eq!(annot_count(&mut doc, 0), 3);

    let mut doc = PdfDocument::from_bytes(ACROFORM_PDF).expect("open acroform.pdf");
    doc.bake(&bake_none()).expect("bake with flags off");
    assert_eq!(widget_count(&mut doc, 0), 2);
}

#[test]
fn bake_annots_only_keeps_widgets() {
    let mut doc = PdfDocument::from_bytes(ACROFORM_PDF).expect("open acroform.pdf");
    let mut options = BakeOptions::default();
    options.widgets = false;
    doc.bake(&options).expect("bake annots only");
    assert_eq!(widget_count(&mut doc, 0), 2);
}

#[test]
fn bake_widgets_only_keeps_annots() {
    let mut doc = PdfDocument::from_bytes(ANNOTATIONS_PDF).expect("open annotations.pdf");
    let mut options = BakeOptions::default();
    options.annots = false;
    doc.bake(&options).expect("bake widgets only");
    assert_eq!(annot_count(&mut doc, 0), 3);
}

#[test]
fn bake_names_a_field_the_flattener_skips() {
    let bytes = pdf_with_rectless_field();
    let mut doc = PdfDocument::from_bytes(&bytes).expect("open operator PDF");
    let err = doc
        .bake(&bake_all())
        .expect_err("a skipped field must fail the bake");
    assert!(
        err.to_string().contains("NoRect"),
        "error names no field: {err}"
    );
}

/// One page with one Square annotation and deliberately no `/AP`.
fn pdf_with_bare_square() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let content_id = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
    let annot_id = doc.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Square",
        "Rect" => vec![10.into(), 10.into(), 110.into(), 60.into()],
        "C" => vec![1.into(), 0.into(), 0.into()],
    });
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => dictionary! {},
        "Annots" => vec![Object::Reference(annot_id)],
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

/// One page with a text field whose widget has no `/Rect`, so the form
/// flattener reports it as skipped.
fn pdf_with_rectless_field() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let content_id = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
    let widget_id = doc.add_object(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Widget",
        "FT" => "Tx",
        "T" => Object::String(b"NoRect".to_vec(), lopdf::StringFormat::Literal),
    });
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => dictionary! {},
        "Annots" => vec![Object::Reference(widget_id)],
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        }),
    );
    let acroform_id = doc.add_object(dictionary! {
        "Fields" => vec![Object::Reference(widget_id)],
    });
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
        "AcroForm" => acroform_id,
    });
    doc.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("fixture saves");
    bytes
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}
