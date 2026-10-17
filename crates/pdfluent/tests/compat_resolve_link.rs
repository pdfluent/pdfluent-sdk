// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Named destinations through `Document.resolve_link` (#643).
//!
//! Page fragments and unknown targets keep their recorded shapes (pinned
//! in `compat_no_silent_noops.rs`); names resolve through the catalog
//! `/Dests` and the `/Names` tree, with the destination point in the
//! coordinate slots and nulls where the fit mode names no coordinate.

use lopdf::{dictionary, Document, Object, Stream};
use pdfluent::compat::Value;
use pdfluent::prelude::*;

fn dest_fixture() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let content_id = doc.add_object(Stream::new(dictionary! {}, b"".to_vec()));
    let page_id = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => content_id,
        "Resources" => dictionary! {},
    });
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        }),
    );
    // Catalog /Dests with an XYZ array plus a /Names tree with a FitH entry.
    let xyz = Object::Array(vec![
        Object::Reference(page_id),
        Object::Name(b"XYZ".to_vec()),
        Object::Integer(100),
        Object::Integer(200),
        Object::Null,
    ]);
    let fith = Object::Array(vec![
        Object::Reference(page_id),
        Object::Name(b"FitH".to_vec()),
        Object::Integer(300),
    ]);
    let names_id = doc.add_object(dictionary! {
        "Dests" => dictionary! {
            "Names" => vec![
                Object::String(b"ch2".to_vec(), lopdf::StringFormat::Literal),
                fith,
            ],
        },
    });
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
        "Names" => names_id,
        "Dests" => dictionary! { "ch1" => xyz },
    });
    doc.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("fixture saves");
    bytes
}

fn open_dest() -> PdfDocument {
    PdfDocument::from_bytes(&dest_fixture()).expect("fixture opens")
}

fn floats(page: i64, x: Option<f64>, y: Option<f64>) -> Value {
    let coord = |v: Option<f64>| v.map(Value::Float).unwrap_or(Value::Null);
    Value::List(vec![Value::Int(page), coord(x), coord(y)])
}

#[test]
fn catalog_dest_resolves_with_its_point() {
    let doc = open_dest();
    // XYZ (100, 200) in y-up points reads y-down against the 792 top.
    let want = floats(0, Some(100.0), Some(592.0));
    assert_eq!(
        doc.resolve_link(Some("#ch1"), 0)
            .expect("fragment resolves"),
        want
    );
    assert_eq!(
        doc.resolve_link(Some("ch1"), 0).expect("bare resolves"),
        want
    );
}

#[test]
fn name_tree_dest_reads_unknown_x_as_null() {
    let doc = open_dest();
    assert_eq!(
        doc.resolve_link(Some("#ch2"), 0).expect("tree resolves"),
        floats(0, None, Some(492.0))
    );
}

#[test]
fn unknown_name_keeps_the_recorded_shape() {
    let doc = open_dest();
    assert_eq!(
        doc.resolve_link(Some("#nope"), 0)
            .expect("unknown resolves"),
        Value::List(vec![Value::Int(-1), Value::Null, Value::Null])
    );
    assert_eq!(
        doc.resolve_link(Some("#ch1"), 1).expect("chapters resolve"),
        Value::List(vec![
            Value::List(vec![Value::Int(0), Value::Int(0)]),
            Value::Float(100.0),
            Value::Float(592.0),
        ])
    );
}
