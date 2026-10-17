// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! What a CID font declares as a width is what its program draws.
//!
//! ISO 19005-2 §6.2.11.5 compares the width in the font dictionary with the
//! advance in the embedded program, for every glyph a page **uses**. Two
//! things move under that comparison during a conversion: `embed_fonts`
//! writes `/W` from the metric table of the face it substituted, and
//! `subset_fonts` afterwards replaces the program with one of its own. The CID
//! width pass ran before both of those, so a document could ship a `/W` that
//! described neither. Where the reconciliation *runs* is the fix, so that is
//! what this file states; whether the two agree afterwards is measured on the
//! corpus by a validator, not by a second implementation of §6.2.11.5 here.
//!
//! It stayed invisible for as long as the conversion was deleting the codes:
//! a page that draws nothing uses no glyph, and veraPDF has nothing to compare.
//! pdf.js `bug898853` is that document -- eight spaces on one page, all eight
//! deleted, `( ) Tj` reduced to `<> Tj`. Keeping the word separator (#485)
//! gave the page its glyphs back and the mismatch its first chance to fail:
//! Arial's 278 declared for a space the embedded program draws at 222.

use lopdf::{dictionary, Document, Object, Stream};

/// One page drawing `cids`, with an **unembedded** CID font, so the conversion
/// has to substitute a face and subset it -- the two steps that move a program
/// out from under the widths that describe it.
///
/// `/W` deliberately declares a width no substitute will have.
fn document_drawing(cids: &[u16]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let pages_id = doc.new_object_id();

    let fd_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => "AnUnavailableFace",
        "Flags" => Object::Integer(4),
        "ItalicAngle" => Object::Integer(0),
        "StemV" => Object::Integer(80),
        "Ascent" => Object::Integer(750),
        "Descent" => Object::Integer(-250),
        "CapHeight" => Object::Integer(700),
        "FontBBox" => Object::Array(vec![
            Object::Integer(-200), Object::Integer(-250),
            Object::Integer(1000), Object::Integer(900),
        ]),
    }));
    let descendant = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "CIDFontType0",
        "BaseFont" => "AnUnavailableFace",
        "CIDSystemInfo" => dictionary! {
            "Registry" => Object::string_literal("Adobe"),
            "Ordering" => Object::string_literal("Identity"),
            "Supplement" => Object::Integer(0),
        },
        "FontDescriptor" => Object::Reference(fd_id),
        "DW" => Object::Integer(1000),
        // The claim under test: a width that is nobody's advance.
        "W" => Object::Array(vec![
            Object::Integer(32),
            Object::Array(vec![Object::Integer(999)]),
            Object::Integer(65),
            Object::Array(vec![Object::Integer(998)]),
        ]),
    }));

    let cmap = "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
         /CMapName /Test def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
         2 beginbfrange\n<0020> <0020> <0020>\n<0041> <0041> <0041>\nendbfrange\n\
         endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n";
    let tounicode = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        cmap.as_bytes().to_vec(),
    )));
    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type0",
        "BaseFont" => "AnUnavailableFace",
        "Encoding" => Object::Name(b"Identity-H".to_vec()),
        "DescendantFonts" => Object::Array(vec![Object::Reference(descendant)]),
        "ToUnicode" => Object::Reference(tounicode),
    }));

    let mut content = b"BT /F1 12 Tf 72 700 Td <".to_vec();
    for cid in cids {
        content.extend_from_slice(format!("{cid:04X}").as_bytes());
    }
    content.extend_from_slice(b"> Tj ET");
    let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));

    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference(pages_id),
        "MediaBox" => Object::Array(vec![
            Object::Integer(0), Object::Integer(0),
            Object::Integer(612), Object::Integer(792),
        ]),
        "Contents" => Object::Reference(content_id),
        "Resources" => dictionary! {
            "Font" => dictionary! { "F1" => Object::Reference(font_id) },
        },
    }));
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(1),
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
        }),
    );
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages_id),
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));

    let mut out = Vec::new();
    doc.save_to(&mut out).expect("save the fixture");
    out
}

/// The reconciliation runs *after* the subsetter, and there is no earlier
/// place that would do.
///
/// The pass that reads the program and writes `/W` ran three times before
/// `subset_fonts` and never after it, so on pdf.js `bug898853` it read the
/// substituted face -- whose widths it could not resolve at that point -- and
/// left Arial's metric table standing over a program with different advances.
/// Moving it back before the subsetter, or dropping it, puts the file back in
/// that state; the step order is the fix, so the step order is the assertion.
#[test]
fn the_widths_are_reconciled_after_the_subsetter_has_run() {
    let source = document_drawing(&[0x0041, 0x0020]);
    let steps = std::cell::RefCell::new(Vec::new());
    let record = |name: &str| steps.borrow_mut().push(name.to_string());
    let opts = pdf_manip::pdfa::PdfAConvertOptions {
        conformance: pdf_manip::pdfa_xmp::PdfAConformance::A2b,
        on_step: Some(&record),
        ..Default::default()
    };
    pdf_manip::pdfa::convert_bytes(&source, &opts).expect("convert");

    let steps = steps.into_inner();
    let at = |name: &str| steps.iter().position(|s| s == name);
    let (Some(subset), Some(widths)) = (at("subset_fonts"), at("cid_widths_after_subset")) else {
        panic!("the conversion ran neither the subsetter nor the width pass after it: {steps:?}");
    };
    assert!(
        widths > subset,
        "the CID widths are reconciled before the subsetter replaces the program: {steps:?}"
    );
}
