// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A page-level /Resources dict shadows the /Pages dict wholesale (#256).
//!
//! Upstream LaurenzV/hayro#1387: inheritable means the nearest /Resources
//! wins, not that ancestor and child entries merge. A page with its own
//! /Resources that lacks /F1 must NOT resolve /F1 from the /Pages node
//! (it falls back to Helvetica, like other viewers); a page without own
//! /Resources still inherits it.
//!
//! The /Pages /F1 maps S to X via /Differences, so the leak is visible:
//! page 1 (own /Resources, no /F1) extracts "Shadowed" through the
//! fallback, page 2 (no own /Resources) inherits /F1 and extracts
//! "Inherited" untouched. Without the fix the ancestor font leaks into
//! page 1 and "Shadowed" comes back "Xhadowed".

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Document, Object, Stream, StringFormat};
use pdf_engine::{to_markdown, ExtractOptions, PdfDocument};

fn font(doc: &mut Document, base: &[u8]) -> Object {
    Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(base.to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    })))
}

fn text_content(text: &str, font: &[u8]) -> Vec<u8> {
    Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec![Object::Name(font.to_vec()), Object::Integer(12)]),
            Operation::new("Td", vec![72.into(), 700.into()]),
            Operation::new(
                "Tj",
                vec![Object::String(
                    text.as_bytes().to_vec(),
                    StringFormat::Literal,
                )],
            ),
            Operation::new("ET", vec![]),
        ],
    }
    .encode()
    .expect("encode content")
}

fn pdf_of() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    // /F1 maps S (83) to X, so resolving it is observable in the text.
    let f1 = Object::Reference(doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        "Encoding" => Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Encoding".to_vec()),
            "BaseEncoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
            "Differences" => Object::Array(vec![
                Object::Integer(83),
                Object::Name(b"X".to_vec()),
            ]),
        }),
    })));
    let f2 = font(&mut doc, b"Helvetica-Bold");

    let content1 = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        text_content("Shadowed", b"F1"),
    )));
    let content2 = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        text_content("Inherited", b"F1"),
    )));

    // Page 1: own /Resources with only /F2. /F1 lives on the /Pages node
    // and must NOT leak in.
    let page1 = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => Object::Reference(content1),
        "Resources" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dictionary! {
                "F2" => f2.clone(),
            }),
        }),
    }));
    // Page 2: no own /Resources; inherits /F1 from the /Pages node.
    let page2 = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Page".to_vec()),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Contents" => Object::Reference(content2),
    }));
    let pages = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![Object::Reference(page1), Object::Reference(page2)]),
        "Count" => Object::Integer(2),
        "Resources" => Object::Dictionary(dictionary! {
            "Font" => Object::Dictionary(dictionary! {
                "F1" => f1.clone(),
            }),
        }),
    }));
    for page in [page1, page2] {
        if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(page) {
            dict.set("Parent", Object::Reference(pages));
        }
    }
    let catalog = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages),
    }));
    doc.trailer.set("Root", Object::Reference(catalog));
    let mut buf = Vec::new();
    doc.save_to(&mut buf).expect("save synthetic pdf");
    buf
}

#[test]
fn page_resources_shadow_pages_resources() {
    let md = to_markdown(
        &PdfDocument::open(pdf_of())
            .expect("open pdf")
            .extract_blocks(&ExtractOptions::default())
            .expect("extract blocks"),
    );
    assert!(
        md.contains("Inherited"),
        "a page without own /Resources must inherit /F1; got:\n{md}"
    );
    assert!(
        md.contains("Shadowed"),
        "a page-level /Resources must shadow the /Pages dict (fallback), got:\n{md}"
    );
    assert!(
        !md.contains("Xhadowed"),
        "the ancestor /F1 must not leak into the shadowing page; got:\n{md}"
    );
}
