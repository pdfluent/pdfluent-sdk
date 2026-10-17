// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! `/A` in a resource dictionary is a resource name, not an action.
//!
//! `remove_forbidden_actions` deletes the key `/A` from any dictionary whose
//! `/A` does not resolve to an action it recognises. A font resource *named*
//! `/A` is such a key: pdfTeX and dvips name their one-glyph Type 3 font
//! exactly that, so `/Resources/Font` reads `/A 4 0 R`, the referenced object
//! is a font dictionary with no `/S`, and the sweep removed it while the
//! content stream kept `/A 1 Tf`.
//!
//! Measured on govdocs 002_002145: the fonts on the page are `A R10 R12 R14
//! R8` after `fix_need_appearances` and `R10 R12 R14 R8` after
//! `remove_forbidden_actions`, with the content trace unchanged through every
//! font pass. veraPDF says `SEVERE: Undefined font /A` at 2u, returns ERROR at
//! 3u -- which took ten more documents down with it in the same batch -- and
//! passes at 2b only because it evaluates nothing for a font it cannot find.
//! 002_002202 lost the glyph off page 3 on both platforms.

use lopdf::content::{Content, Operation};
use lopdf::{dictionary, Dictionary, Document, Object, Stream, StringFormat};

/// The one-glyph Type 3 font pdfTeX emits, and the CharProc it draws with.
///
/// A Type 3 font dictionary carries no `/S`, which is exactly why the action
/// sweep mistook it for an action object whose type had already been stripped.
fn type3_font_named_a(doc: &mut Document) -> Object {
    let glyph = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        b"750 0 0 0 750 750 d1\n0 0 750 750 re f\n".to_vec(),
    )));
    let font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type3",
        "FontBBox" => vec![0.into(), 0.into(), 750.into(), 750.into()],
        "FontMatrix" => vec![
            Object::Real(0.001), 0.into(), 0.into(),
            Object::Real(0.001), 0.into(), 0.into(),
        ],
        "CharProcs" => dictionary! { "a" => Object::Reference(glyph) },
        "Encoding" => dictionary! {
            "Type" => "Encoding",
            "Differences" => vec![97.into(), Object::Name(b"a".to_vec())],
        },
        "FirstChar" => 97_i64,
        "LastChar" => 97_i64,
        "Widths" => vec![750.into()],
        "Resources" => Object::Dictionary(dictionary! {}),
    }));
    Object::Reference(font)
}

/// A page drawing that font under the name `/A`, plus a link annotation whose
/// `/A` really is a forbidden action.
///
/// Both live in the same document on purpose: the fix has to keep the first
/// and still remove the second, or it is a fix by deleting the pass.
fn page_with_a_font_named_a_and_a_launch_action() -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let font = type3_font_named_a(&mut doc);

    let content = Content {
        operations: vec![
            Operation::new("BT", vec![]),
            Operation::new("Td", vec![72.into(), 700.into()]),
            Operation::new("Tf", vec![Object::Name(b"A".to_vec()), 12.into()]),
            Operation::new(
                "Tj",
                vec![Object::String(b"a".to_vec(), StringFormat::Literal)],
            ),
            Operation::new("ET", vec![]),
        ],
    };
    let stream_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        content.encode().unwrap(),
    )));
    let annot_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Link",
        "Rect" => vec![72.into(), 690.into(), 200.into(), 720.into()],
        "A" => dictionary! { "S" => "Launch", "F" => Object::String(b"run.exe".to_vec(), StringFormat::Literal) },
    }));
    // Indirect, the way pdfTeX writes it: the page names `/Resources 5 0 R`
    // and that dictionary names `/Font 6 0 R`. The sweep walks the document's
    // objects, so a resource dictionary only comes past it when it is one.
    let fonts_id = doc.add_object(Object::Dictionary(dictionary! { "A" => font }));
    let resources_id = doc.add_object(Object::Dictionary(dictionary! {
        "Font" => Object::Reference(fonts_id),
    }));
    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Page",
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => Object::Reference(stream_id),
        "Annots" => vec![Object::Reference(annot_id)],
        "Resources" => Object::Reference(resources_id),
    }));
    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Pages",
        "Kids" => vec![Object::Reference(page_id)],
        "Count" => 1_i64,
    }));
    if let Ok(Object::Dictionary(d)) = doc.get_object_mut(page_id) {
        d.set("Parent", Object::Reference(pages_id));
    }
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));

    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save fixture");
    out
}

fn convert(source: &[u8]) -> Document {
    let opts = pdf_manip::pdfa::PdfAConvertOptions {
        conformance: pdf_manip::pdfa_xmp::PdfAConformance::A2b,
        ..Default::default()
    };
    let out = pdf_manip::pdfa::convert_bytes(source, &opts).expect("PDF/A conversion");
    Document::load_mem(&out).expect("load converted output")
}

/// The `/Font` sub-dictionary of the first page, resolved through references.
fn page_fonts(doc: &Document) -> Dictionary {
    let page_id = doc.page_iter().next().expect("the output has no page");
    let resources = doc
        .get_dictionary(page_id)
        .expect("page is not a dictionary")
        .get(b"Resources")
        .expect("page has no /Resources");
    let resources = match resources {
        Object::Reference(id) => doc.get_dictionary(*id).expect("/Resources is not a dict"),
        Object::Dictionary(d) => d,
        other => panic!("/Resources is a {other:?}"),
    };
    match resources.get(b"Font").expect("page has no /Font resources") {
        Object::Reference(id) => doc
            .get_dictionary(*id)
            .expect("/Font is not a dict")
            .clone(),
        Object::Dictionary(d) => d.clone(),
        other => panic!("/Font is a {other:?}"),
    }
}

/// The names the content stream selects with `Tf`.
fn fonts_the_page_selects(doc: &Document) -> Vec<String> {
    let page_id = doc.page_iter().next().expect("the output has no page");
    let content = doc.get_page_content(page_id);
    let parsed = Content::decode(&content).expect("content stream does not decode");
    parsed
        .operations
        .iter()
        .filter(|op| op.operator == "Tf")
        .filter_map(|op| op.operands.first())
        .filter_map(|o| o.as_name().ok())
        .map(|n| String::from_utf8_lossy(n).into_owned())
        .collect()
}

#[test]
fn the_font_named_a_survives_the_conversion() {
    let converted = convert(&page_with_a_font_named_a_and_a_launch_action());

    let fonts = page_fonts(&converted);
    let names: Vec<_> = fonts
        .iter()
        .map(|(k, _)| String::from_utf8_lossy(k).into_owned())
        .collect();
    assert!(
        fonts.has(b"A"),
        "the page selects /A with Tf and its font resources are {names:?}"
    );

    // And it still resolves to a font, not to a husk left behind.
    let font = match fonts.get(b"A").expect("checked above") {
        Object::Reference(id) => converted
            .get_dictionary(*id)
            .expect("/A does not resolve to a dictionary"),
        Object::Dictionary(d) => d,
        other => panic!("/A is a {other:?}"),
    };
    assert_eq!(
        font.get(b"Type").and_then(Object::as_name).ok(),
        Some(b"Font".as_slice()),
        "/A resolves to something that is not a font: {font:?}"
    );
    assert!(
        font.has(b"CharProcs"),
        "the Type 3 font lost the procedure that draws its one glyph"
    );
}

#[test]
fn the_page_still_selects_the_font_it_draws_with() {
    // The other half of the same defect: a page that selects a font it no
    // longer has is what veraPDF reports as `Undefined font /A`.
    let converted = convert(&page_with_a_font_named_a_and_a_launch_action());
    let selected = fonts_the_page_selects(&converted);
    let fonts = page_fonts(&converted);
    for name in &selected {
        assert!(
            fonts.has(name.as_bytes()),
            "the content stream selects /{name} and the page's resources are {fonts:?}"
        );
    }
}

#[test]
fn a_launch_action_on_an_annotation_is_still_removed() {
    // The guard against fixing the above by not sweeping at all. /Launch is
    // forbidden by ISO 19005-2 6.5.1 and must not survive.
    let converted = convert(&page_with_a_font_named_a_and_a_launch_action());
    for (id, obj) in &converted.objects {
        let Ok(dict) = obj.as_dict() else { continue };
        if dict.get(b"Subtype").and_then(Object::as_name).ok() != Some(b"Link".as_slice()) {
            continue;
        }
        let action = match dict.get(b"A") {
            Ok(Object::Dictionary(d)) => Some(d.clone()),
            Ok(Object::Reference(r)) => converted.get_dictionary(*r).ok().cloned(),
            _ => None,
        };
        if let Some(action) = action {
            assert!(
                action.get(b"S").and_then(Object::as_name).ok() != Some(b"Launch".as_slice()),
                "object {id:?} kept its /Launch action"
            );
        }
    }
}
