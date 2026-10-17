// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! `Page.get_bboxlog` through the facade (#646).
//!
//! One `(kind, rect)` entry per painted item in content-stream order, with
//! an optional layer element. Boxes are the engine's line-metric boxes,
//! which the known-differences contract calls out against glyph-ink boxes.

use lopdf::{dictionary, Document, Object, Stream};
use pdfluent::compat::{Page, Value};
use pdfluent::prelude::*;

const SAMPLE_PDF: &[u8] = include_bytes!("../../../fixtures/sample.pdf");
const TABLE_PDF: &[u8] = include_bytes!(
    "../../../crates/pdf-engine/tests/fixtures/pubtables/pmc2438475_p1_table_notes.pdf"
);
const SCAN_PDF: &[u8] = include_bytes!("../../../fixtures/scanned/scan_01_sample.pdf");

fn entries_of(value: &Value) -> Vec<(String, [f64; 4], Option<String>)> {
    let Value::List(entries) = value else {
        panic!("box log is not a list: {value:?}");
    };
    entries
        .iter()
        .map(|entry| {
            let Value::List(parts) = entry else {
                panic!("entry is not a list: {entry:?}");
            };
            assert!(
                parts.len() == 2 || parts.len() == 3,
                "entry has {} elements: {entry:?}",
                parts.len()
            );
            let Value::Str(kind) = &parts[0] else {
                panic!("kind is not a string: {entry:?}");
            };
            let Value::List(rect) = &parts[1] else {
                panic!("rect is not a list: {entry:?}");
            };
            let coords: Vec<f64> = rect
                .iter()
                .map(|n| match n {
                    Value::Float(f) => *f,
                    Value::Int(i) => *i as f64,
                    other => panic!("rect entry is not a number: {other:?}"),
                })
                .collect();
            assert_eq!(coords.len(), 4, "rect is not four numbers: {entry:?}");
            let layer = if parts.len() == 3 {
                let Value::Str(name) = &parts[2] else {
                    panic!("layer is not a string: {entry:?}");
                };
                Some(name.clone())
            } else {
                None
            };
            (
                kind.clone(),
                [coords[0], coords[1], coords[2], coords[3]],
                layer,
            )
        })
        .collect()
}

fn log_of(
    doc: &PdfDocument,
    page: usize,
    layers: Option<&Value>,
) -> Vec<(String, [f64; 4], Option<String>)> {
    let value = Page::new(doc, page)
        .get_bboxlog(layers)
        .expect("box log reads");
    entries_of(&value)
}

#[test]
fn bboxlog_lists_text_spans_in_order() {
    let doc = PdfDocument::from_bytes(SAMPLE_PDF).expect("open sample.pdf");
    let entries = log_of(&doc, 0, None);
    assert_eq!(entries.len(), 1);
    let (kind, rect, layer) = &entries[0];
    assert_eq!(kind, "fill-text");
    assert_eq!(layer, &None);
    // The engine's line-metric box for "Test page", not the glyph-ink box
    // the recording pins ([99.25, 82.25, 153.84, 95.61]).
    for (got, want) in rect.iter().zip([100.0, 79.1, 153.36, 95.59]) {
        assert!((got - want).abs() < 0.01, "rect drifted: {rect:?}");
    }
}

#[test]
fn bboxlog_layers_add_the_layer_element() {
    let doc = PdfDocument::from_bytes(SAMPLE_PDF).expect("open sample.pdf");
    let plain = log_of(&doc, 0, None);
    assert!(plain.iter().all(|(_, _, layer)| layer.is_none()));

    let empty = Value::List(Vec::new());
    let unfiltered = log_of(&doc, 0, Some(&empty));
    assert_eq!(unfiltered.len(), plain.len());
    assert!(unfiltered.iter().all(|(_, _, layer)| layer.is_none()));

    let with_layers = log_of(&doc, 0, Some(&Value::Bool(true)));
    assert_eq!(with_layers.len(), plain.len());
    assert_eq!(with_layers[0].2.as_deref(), Some(""));
}

#[test]
fn bboxlog_logs_vector_paths() {
    let doc = PdfDocument::from_bytes(TABLE_PDF).expect("open table pdf");
    let entries = log_of(&doc, 0, None);
    assert!(
        entries
            .iter()
            .any(|(kind, _, _)| kind == "fill-path" || kind == "stroke-path"),
        "no path entry in {} entries",
        entries.len()
    );
    for (kind, rect, _) in &entries {
        assert!(
            rect[2] >= rect[0] && rect[3] >= rect[1],
            "{kind} box is inverted: {rect:?}"
        );
    }
}

#[test]
fn bboxlog_logs_scanned_images() {
    let doc = PdfDocument::from_bytes(SCAN_PDF).expect("open scan pdf");
    let entries = log_of(&doc, 0, None);
    assert!(
        entries.iter().any(|(kind, _, _)| kind == "fill-image"),
        "no image entry in {} entries: {:?}",
        entries.len(),
        entries.iter().map(|(k, _, _)| k).collect::<Vec<_>>()
    );
}

#[test]
fn bboxlog_empty_page_is_empty() {
    let bytes = blank_page_pdf();
    let doc = PdfDocument::from_bytes(&bytes).expect("open operator PDF");
    assert!(log_of(&doc, 0, None).is_empty());
}

#[test]
fn bboxlog_out_of_range_page_fails() {
    let doc = PdfDocument::from_bytes(SAMPLE_PDF).expect("open sample.pdf");
    assert!(Page::new(&doc, 99).get_bboxlog(None).is_err());
}

/// One blank page, no marks at all.
fn blank_page_pdf() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();
    let content_id = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
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
    let catalog_id = doc.add_object(dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    });
    doc.trailer.set("Root", catalog_id);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).expect("fixture saves");
    bytes
}
