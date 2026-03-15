//! Creates fixtures/maxLenInheritanceTest.pdf — a minimal PDF/1.4 form where
//! /MaxLen sits on the parent field node rather than the terminal widget.
//!
//! This is the canonical test fixture for #459: the form_write test must
//! correctly identify the widget as a Text field via effective_field_type()
//! (inherited /FT /Tx from parent), write a value, and round-trip it back.
//!
//! Run with:
//!   cargo run -p xfa-test-runner --example create_maxlen_fixture

use std::path::PathBuf;

use lopdf::{Dictionary, Document, Object};

fn main() {
    let out_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/maxLenInheritanceTest.pdf");

    let bytes = build_pdf();
    std::fs::write(&out_path, &bytes).expect("write fixture");
    println!("Wrote {} bytes to {}", bytes.len(), out_path.display());
}

/// Constructs the PDF bytes.
///
/// Structure:
///   AcroForm.Fields → [parent_field]
///   parent_field: /FT /Tx  /T (name)  /MaxLen 20  /Kids [widget]
///   widget:       /T (field)  /Subtype /Widget  /Rect [100 700 300 720]
///                 (no /FT, no /MaxLen — both inherited from parent)
fn build_pdf() -> Vec<u8> {
    let mut doc = Document::with_version("1.4");

    // ------------------------------------------------------------------
    // Allocate object IDs in advance.
    // ------------------------------------------------------------------
    let catalog_id = doc.new_object_id();
    let pages_id = doc.new_object_id();
    let page_id = doc.new_object_id();
    let acroform_id = doc.new_object_id();
    let parent_field_id = doc.new_object_id();
    let widget_id = doc.new_object_id();

    // ------------------------------------------------------------------
    // Widget annotation (terminal field node, no /FT, no /MaxLen).
    // ------------------------------------------------------------------
    let mut widget = Dictionary::new();
    widget.set("Type", Object::Name(b"Annot".to_vec()));
    widget.set("Subtype", Object::Name(b"Widget".to_vec()));
    widget.set("Parent", Object::Reference(parent_field_id));
    widget.set(
        "T",
        Object::String(b"field".to_vec(), lopdf::StringFormat::Literal),
    );
    widget.set(
        "V",
        Object::String(b"".to_vec(), lopdf::StringFormat::Literal),
    );
    widget.set(
        "DA",
        Object::String(b"/Helv 12 Tf 0 g".to_vec(), lopdf::StringFormat::Literal),
    );
    widget.set(
        "Rect",
        Object::Array(vec![
            Object::Real(100.0),
            Object::Real(700.0),
            Object::Real(400.0),
            Object::Real(720.0),
        ]),
    );
    widget.set("P", Object::Reference(page_id));
    doc.objects.insert(widget_id, Object::Dictionary(widget));

    // ------------------------------------------------------------------
    // Parent field node (intermediate, carries /FT and /MaxLen).
    // /MaxLen 20 is intentionally larger than the test value (22 chars)
    // is NOT larger, so we can test that MaxLen is found via inheritance
    // without also truncating the round-trip value.
    // ------------------------------------------------------------------
    let mut parent_field = Dictionary::new();
    parent_field.set("FT", Object::Name(b"Tx".to_vec()));
    parent_field.set(
        "T",
        Object::String(b"name".to_vec(), lopdf::StringFormat::Literal),
    );
    // MaxLen 20: inherited by the child widget.  Smaller than the test
    // value "__xfa_roundtrip_test__" (22 chars) so that set_text_value
    // *would* truncate — but form_write bypasses it via set_field_value_lopdf,
    // exercising that the write is not blocked by inherited MaxLen.
    parent_field.set("MaxLen", Object::Integer(20));
    parent_field.set("Kids", Object::Array(vec![Object::Reference(widget_id)]));
    doc.objects
        .insert(parent_field_id, Object::Dictionary(parent_field));

    // ------------------------------------------------------------------
    // AcroForm dictionary.
    // ------------------------------------------------------------------
    let mut acroform = Dictionary::new();
    acroform.set(
        "Fields",
        Object::Array(vec![Object::Reference(parent_field_id)]),
    );
    acroform.set(
        "DA",
        Object::String(b"/Helv 12 Tf 0 g".to_vec(), lopdf::StringFormat::Literal),
    );
    doc.objects
        .insert(acroform_id, Object::Dictionary(acroform));

    // ------------------------------------------------------------------
    // Page tree.
    // ------------------------------------------------------------------
    let mut page = Dictionary::new();
    page.set("Type", Object::Name(b"Page".to_vec()));
    page.set("Parent", Object::Reference(pages_id));
    page.set(
        "MediaBox",
        Object::Array(vec![
            Object::Integer(0),
            Object::Integer(0),
            Object::Integer(612),
            Object::Integer(792),
        ]),
    );
    page.set("Annots", Object::Array(vec![Object::Reference(widget_id)]));
    doc.objects.insert(page_id, Object::Dictionary(page));

    let mut pages = Dictionary::new();
    pages.set("Type", Object::Name(b"Pages".to_vec()));
    pages.set("Kids", Object::Array(vec![Object::Reference(page_id)]));
    pages.set("Count", Object::Integer(1));
    doc.objects.insert(pages_id, Object::Dictionary(pages));

    // ------------------------------------------------------------------
    // Catalog.
    // ------------------------------------------------------------------
    let mut catalog = Dictionary::new();
    catalog.set("Type", Object::Name(b"Catalog".to_vec()));
    catalog.set("Pages", Object::Reference(pages_id));
    catalog.set("AcroForm", Object::Reference(acroform_id));
    doc.objects.insert(catalog_id, Object::Dictionary(catalog));

    // lopdf needs a trailer root.
    doc.trailer.set("Root", Object::Reference(catalog_id));

    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("lopdf save");
    buf
}
