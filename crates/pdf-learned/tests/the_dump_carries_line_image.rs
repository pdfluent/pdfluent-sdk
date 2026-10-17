//! The dump carries the line network's image inputs beside its vector ones (#574).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_learned::dump_pdf;

fn line(operations: &mut Vec<Operation>, text: &str, x: f64, y: f64) {
    operations.push(Operation::new("BT", vec![]));
    operations.push(Operation::new(
        "Tf",
        vec![Object::Name(b"F1".to_vec()), 10.into()],
    ));
    operations.push(Operation::new("Td", vec![x.into(), y.into()]));
    operations.push(Operation::new("Tj", vec![Object::string_literal(text)]));
    operations.push(Operation::new("ET", vec![]));
}

/// One PDF, one page per list of operations.
fn pdf_of(pages: Vec<Vec<Operation>>) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
    });
    let pages_id = doc.new_object_id();
    let mut kids: Vec<Object> = Vec::new();
    for operations in pages {
        let content = Content { operations }.encode().expect("encode content");
        let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages_id, "Contents" => content_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        });
        kids.push(page_id.into());
    }
    let count = kids.len() as i64;
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => kids, "Count" => count,
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save pdf");
    out
}

fn records(pdf: Vec<u8>) -> Vec<serde_json::Value> {
    dump_pdf(pdf, None)
        .expect("dump")
        .lines()
        .map(|line| serde_json::from_str(line).expect("a record per line"))
        .collect()
}

#[test]
fn every_line_has_an_image_row_in_range() {
    let mut first = Vec::new();
    line(&mut first, "A heading set in heavier type", 72.0, 700.0);
    line(
        &mut first,
        "and the paragraph running under it",
        72.0,
        686.0,
    );
    let mut last = Vec::new();
    line(&mut last, "A third page keeps its own rows", 72.0, 700.0);
    // THREE PAGES, THE MIDDLE ONE WITH NO TEXT: the length check below runs
    // on every page the dump writes, so a drift past the first page -- or a
    // blank page read as a failed render -- goes red here (#574).
    let records = records(pdf_of(vec![first, Vec::new(), last]));
    assert_eq!(records.len(), 3, "one record per page");
    let mut lines = 0;
    for (page, record) in records.iter().enumerate() {
        let vector = record["line_vector"].as_array().expect("line_vector");
        let image = record["line_image"]
            .as_array()
            .expect("line_image: rendered, so present, not null");
        assert_eq!(
            image.len(),
            vector.len(),
            "page {page}: one image row per vector row, in the same order"
        );
        for row in image {
            let row = row.as_array().expect("a row of six");
            assert_eq!(row.len(), 6, "IMAGE_FEATURES is six wide");
            let num = |i: usize| row[i].as_f64().expect("a number");
            for i in [0, 1, 2, 5] {
                assert!(
                    (0.0..=1.0).contains(&num(i)),
                    "page {page}, column {i} is a fraction: {row:?}"
                );
            }
            for i in [3, 4] {
                assert!(
                    num(i) == 0.0 || num(i) == 1.0,
                    "page {page}, column {i} is a verdict: {row:?}"
                );
            }
            lines += 1;
        }
    }
    assert!(lines > 0, "the text pages have lines");
    // The blank page carries empty rows, not null: null means the page did
    // not render, and an empty page renders blank.
    assert!(
        records[1]["line_vector"]
            .as_array()
            .is_some_and(Vec::is_empty),
        "the blank page has no vector rows"
    );
    assert_eq!(
        records[1]["line_image"],
        serde_json::json!([]),
        "and no image rows either -- not null"
    );
}
