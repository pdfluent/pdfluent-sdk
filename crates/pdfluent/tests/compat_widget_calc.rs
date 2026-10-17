// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! `module.util_ensure_widget_calc` through the facade (#643).
//!
//! The widget's reference lands in the catalog form's `/CO` array
//! (created with the form when missing); repeating the call adds no
//! duplicate, and non-widgets fail loudly.

use pdfluent::compat::{util_ensure_widget_calc, Annot, PageMut};
use pdfluent::prelude::*;

const ACROFORM_PDF: &[u8] = include_bytes!("../../../fixtures/acroform.pdf");
const ANNOTATIONS_PDF: &[u8] =
    include_bytes!("../../../crates/visual-regression/fixtures/annotations.pdf");

fn calc_order(bytes: &[u8]) -> Vec<(u32, u16)> {
    let doc = lopdf::Document::load_mem(bytes).expect("bytes parse");
    let root = doc
        .trailer
        .get(b"Root")
        .ok()
        .and_then(|o| o.as_reference().ok())
        .expect("catalog resolves");
    let form = doc
        .get_dictionary(root)
        .ok()
        .and_then(|catalog| catalog.get(b"AcroForm").ok().cloned())
        .and_then(|obj| match obj {
            lopdf::Object::Reference(id) => Some(id),
            _ => None,
        })
        .expect("form resolves");
    doc.get_dictionary(form)
        .ok()
        .and_then(|dict| dict.get(b"CO").ok().cloned())
        .and_then(|obj| obj.as_array().ok().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|obj| obj.as_reference().ok())
        .collect()
}

#[test]
fn widget_lands_in_calc_order_once() {
    let mut doc = PdfDocument::from_bytes(ACROFORM_PDF).expect("form opens");
    let widgets = PageMut::new(&mut doc, 0)
        .widgets(None)
        .expect("widgets list");
    assert!(!widgets.is_empty(), "fixture carries a widget");
    let xref = widgets[0].xref;
    assert!(xref > 0, "placed widget has an xref");

    for _ in 0..2 {
        let mut view = PageMut::new(&mut doc, 0);
        let annot = Annot::new(&mut view, xref);
        util_ensure_widget_calc(annot).expect("ensure works");
    }

    let bytes = doc.to_bytes().expect("save works");
    assert_eq!(calc_order(&bytes), vec![(xref as u32, 0)]);
}

#[test]
fn non_widget_fails_loudly() {
    let mut doc = PdfDocument::from_bytes(ANNOTATIONS_PDF).expect("annots open");
    let mut view = PageMut::new(&mut doc, 0);
    let entries = view.annots(None).expect("annots list");
    let plain = entries
        .iter()
        .find(|entry| entry.annot_type.name != "Widget")
        .expect("fixture carries a plain annot");
    let annot = Annot::new(&mut view, plain.xref);
    let err = util_ensure_widget_calc(annot).expect_err("plain annot fails");
    assert!(
        err.to_string().contains("annotation is not a widget"),
        "{err}"
    );
}
