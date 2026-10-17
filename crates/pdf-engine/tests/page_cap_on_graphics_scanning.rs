//! Graphics scanning stops after the first pages (lane L4).
//!
//! Every page is scanned for figures today: the page cap is gone, so a long
//! document pays the graphics pass on page two hundred as on page one. The
//! rebuilt engine caps the scan, and pages past the cap report no figures.
//!
//! Only the shape is pinned: on a two-hundred-page document of one figure
//! per page, fewer than all pages report one. The cap itself is ours to
//! choose. Unspecified; our behaviour: the front door's own graphics limit
//! changes nothing at any setting (G87, P17), but ours is a real cap.
//! (G87 is still the reason the figure on an early page must be found: the
//! cap truncates the scan, it never empties it.)

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream};
use pdf_engine::PdfDocument;

/// How many figure pages the document holds: past any reasonable cap.
const PAGES: usize = 200;

/// Two hundred pages, each placing the same photograph once.
fn many_figure_pages() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let image = doc.add_object(Object::Stream(Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image", "Width" => 40, "Height" => 30,
            "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8,
        },
        vec![128u8; 40 * 30],
    )));
    let pages_id = doc.new_object_id();
    let mut kids = Vec::new();
    for _ in 0..PAGES {
        let ops = vec![
            Operation::new("q", vec![]),
            Operation::new(
                "cm",
                vec![
                    300.into(),
                    0.into(),
                    0.into(),
                    200.into(),
                    72.into(),
                    460.into(),
                ],
            ),
            Operation::new("Do", vec![Object::Name(b"Im1".to_vec())]),
            Operation::new("Q", vec![]),
        ];
        let content = doc.add_object(Object::Stream(Stream::new(
            dictionary! {},
            Content { operations: ops }.encode().expect("encode"),
        )));
        kids.push(
            doc.add_object(dictionary! {
                "Type" => "Page", "Parent" => pages_id, "Contents" => content,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                "Resources" => dictionary! {
                    "XObject" => dictionary! { "Im1" => image },
                },
            })
            .into(),
        );
    }
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => Object::Array(kids), "Count" => PAGES as u32,
        }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
    doc.trailer.set("Root", catalog);
    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save pdf");
    out
}

#[test]
fn pages_past_the_cap_report_no_figures() {
    let boxes = PdfDocument::open(many_figure_pages())
        .expect("open")
        .extract_image_boxes()
        .expect("figure boxes");
    assert_eq!(boxes.len(), PAGES, "one entry per page");
    assert!(
        !boxes[0].is_empty(),
        "the cap truncates the scan, it never empties it"
    );
    let scanned = boxes.iter().filter(|b| !b.is_empty()).count();
    assert!(
        scanned < PAGES,
        "all {PAGES} pages were scanned for figures; the cap is gone"
    );
}
