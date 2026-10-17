// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A Form XObject gets a Resources dictionary of its own, whatever colour it
//! draws in.
//!
//! ISO 19005-2 6.2.2:2: "a content stream that references other objects, such
//! as images and fonts that are necessary to fully render or process the
//! stream, shall have an explicitly associated Resources dictionary". veraPDF
//! enforces it by name -- a form with no `/Resources` inherits the page's, and
//! resolving a name through that inheritance is the failure.
//!
//! Nothing in the converter was doing this on purpose. `add_default_*_colorspace`
//! wrote a Resources dictionary on the way past, for every form, pattern and
//! appearance stream that had none, because it needed somewhere to put
//! `/DefaultCMYK`. Making those profiles conditional (#379 round 2) took the
//! side effect away with them, and the gate corpus said so within the hour:
//! `PDFBOX-4372-...-p4_reduced.pdf` went from conformant to not, on one nested
//! form with `/Resources null` and one `/Fm0` resolved through the page.
//!
//! That is what this test is: the accident, made deliberate, so the next person
//! to change the colour pass does not take it away again.
//!
//! `/Resources null` rather than a missing key, because that is the shape the
//! document that found this carries, and it is the shape a `has("Resources")`
//! test gets wrong.

use lopdf::{dictionary, Document, Object, Stream};

/// One page invoking one form, and the form says `/Resources null`.
///
/// Nothing here paints. That is the shape of the corpus document and it is
/// load-bearing: a content stream starts in DeviceGray, so a single fill
/// anywhere makes the census say gray, `add_default_gray_colorspace` runs, and
/// it writes the Resources dictionary on its way past -- the very accident this
/// test exists to replace. A document that draws nothing is the one where
/// nothing else will do it.
fn page_drawing_a_form_with_null_resources() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let form_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
            "Resources" => Object::Null,
        },
        b"/Fm0 Do\n".to_vec(),
    )));
    let content_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        b"q /Fm0 Do Q\n".to_vec(),
    )));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Page",
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => Object::Reference(content_id),
        "Resources" => dictionary! {
            "XObject" => dictionary! { "Fm0" => Object::Reference(form_id) },
        },
    }));
    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Pages",
        "Kids" => vec![Object::Reference(page_id)],
        "Count" => 1_i64,
    }));
    if let Ok(Object::Dictionary(page)) = doc.get_object_mut(page_id) {
        page.set("Parent", Object::Reference(pages_id));
    }
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));
    let mut buffer = Vec::new();
    doc.save_to(&mut buffer).expect("the fixture must save");
    buffer
}

/// Whether every Form XObject in the document has a Resources dictionary that
/// is a dictionary, rather than absent or null.
fn forms_without_resources(pdf: &[u8]) -> usize {
    let doc = Document::load_mem(pdf).expect("the converted document must load");
    doc.objects
        .values()
        .filter(|object| match object {
            Object::Stream(stream) => {
                stream.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Form")
                    && !matches!(
                        stream.dict.get(b"Resources").ok().map(|resources| {
                            match resources {
                                Object::Reference(id) => {
                                    matches!(doc.objects.get(id), Some(Object::Dictionary(_)))
                                }
                                other => matches!(other, Object::Dictionary(_)),
                            }
                        }),
                        Some(true)
                    )
            }
            _ => false,
        })
        .count()
}

#[test]
fn a_form_that_draws_only_gray_still_gets_its_own_resources() {
    let source = page_drawing_a_form_with_null_resources();
    assert!(
        !pdf_manip::pdfa_colorspace::census_device_colors(
            &Document::load_mem(&source).expect("the fixture must load")
        )
        .gray,
        "fixture check: this document must reach no device colour at all, or a \
         Default* pass writes the Resources dictionary and the test proves \
         nothing"
    );
    assert_eq!(
        forms_without_resources(&source),
        1,
        "fixture check: the form must start with no usable Resources, or this \
         test passes without exercising anything"
    );

    let opts = pdf_manip::pdfa::PdfAConvertOptions::default();
    let converted = pdf_manip::pdfa::convert_bytes(&source, &opts).expect("conversion failed");

    assert_eq!(
        forms_without_resources(&converted),
        0,
        "a Form XObject came out of the conversion still resolving its names \
         through the page's Resources. ISO 19005-2 6.2.2:2 is a veraPDF \
         failure on that, and this form draws in gray, so no Default* entry \
         will put the dictionary there for it any more."
    );
}
