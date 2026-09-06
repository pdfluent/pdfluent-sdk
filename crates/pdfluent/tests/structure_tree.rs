//! End-to-end tests for `PdfDocument::structure_tree` (tagged-PDF structure).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdfluent::prelude::*;
use pdfluent::OpenOptions;
use std::path::PathBuf;

fn mini(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/corpus-mini")
        .join(name)
}

fn read_and_fix_simple_pdf() -> Vec<u8> {
    let mut bytes = std::fs::read(mini("simple.pdf")).expect("read simple.pdf");
    if let Some(pos) = bytes.windows(8).position(|w| w == b"//Length") {
        bytes[pos..pos + 8].copy_from_slice(b"/Length ");
    }
    bytes
}

#[test]
fn untagged_document_has_no_structure_tree() {
    let doc = PdfDocument::from_bytes_with(&read_and_fix_simple_pdf(), OpenOptions::new())
        .expect("open untagged");
    assert!(
        doc.structure_tree().is_none(),
        "a document without /StructTreeRoot must report no structure tree"
    );
}

#[test]
fn tagged_document_exposes_headings_and_alt_text() {
    use lopdf::{dictionary, Object, StringFormat};

    let mut lopdf_doc = lopdf::Document::load_mem(&read_and_fix_simple_pdf()).expect("load");
    lopdf_doc.max_id = lopdf_doc
        .objects
        .keys()
        .map(|&(id, _)| id)
        .max()
        .unwrap_or(0);

    // A minimal logical structure: an H1 heading and a Figure with alt text.
    let h1 = lopdf_doc.add_object(dictionary! {
        "Type" => Object::Name(b"StructElem".to_vec()),
        "S" => Object::Name(b"H1".to_vec()),
        "Alt" => Object::String(b"Section One".to_vec(), StringFormat::Literal),
    });
    let figure = lopdf_doc.add_object(dictionary! {
        "Type" => Object::Name(b"StructElem".to_vec()),
        "S" => Object::Name(b"Figure".to_vec()),
        "Alt" => Object::String(b"A chart".to_vec(), StringFormat::Literal),
    });
    let struct_root = lopdf_doc.add_object(dictionary! {
        "Type" => Object::Name(b"StructTreeRoot".to_vec()),
        "K" => Object::Array(vec![Object::Reference(h1), Object::Reference(figure)]),
    });
    let catalog_id = lopdf_doc
        .trailer
        .get(b"Root")
        .ok()
        .and_then(|o| o.as_reference().ok())
        .expect("catalog /Root");
    if let Ok(Object::Dictionary(cat)) = lopdf_doc.get_object_mut(catalog_id) {
        cat.set("StructTreeRoot", Object::Reference(struct_root));
        cat.set(
            "Lang",
            Object::String(b"en-US".to_vec(), StringFormat::Literal),
        );
    }

    let mut bytes = Vec::new();
    lopdf_doc.save_to(&mut bytes).expect("save tagged");

    let doc = PdfDocument::from_bytes_with(&bytes, OpenOptions::new()).expect("open tagged");

    let structure = doc
        .structure_tree()
        .expect("a tagged document must report a structure tree");
    assert_eq!(structure.language.as_deref(), Some("en-US"));
    assert_eq!(structure.elements.len(), 2);

    let heading = &structure.elements[0];
    assert_eq!(heading.tag, "H1");
    assert_eq!(heading.heading_level, Some(1));

    let fig = &structure.elements[1];
    assert_eq!(fig.tag, "Figure");
    assert_eq!(fig.alt_text.as_deref(), Some("A chart"));
    assert_eq!(fig.heading_level, None);
}
