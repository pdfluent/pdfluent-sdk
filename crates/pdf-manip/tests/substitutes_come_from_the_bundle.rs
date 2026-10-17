// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A substitute font must not come from the machine.
//!
//! PDF/A requires every font to be embedded, and many documents only reference
//! a typeface. Taking the replacement from the host makes the output depend on
//! what is installed: measured on 119 govdocs holdout documents on 24-08-2026,
//! 37 of them borrowed 129 fonts from this Mac, and the same corpus scored
//! 286/300 on macOS against 300/300 on Linux for that reason alone.
//!
//! Two changes brought that to zero. The non-Standard-14 faces are now carried
//! in `pdf-substitute-fonts`, and the heuristic that maps an unknown typeface
//! onto a Standard 14 name now reads the compiled-in set rather than looking
//! on disk -- 88 of the 129 borrows were that second path, 47 of them landing
//! on macOS's Helvetica.ttc.
//!
//! An archive keeps these files for decades and may verify them by hash. "Same
//! input, same bytes, any machine" is the promise; this is what holds it.

#[test]
fn converting_a_document_borrows_no_font_from_this_machine() {
    const SOURCE: &[u8] = include_bytes!("../../../tests/corpus-mini/simple.pdf");

    let mut doc = lopdf::Document::load_mem(SOURCE).expect("fixture does not load");
    let report = pdf_manip::pdfa_fonts::embed_fonts(&mut doc).expect("embedding failed");

    assert_eq!(
        report.system_font_fallbacks, 0,
        "{} substitute font(s) came from this machine, so this output cannot be \
         reproduced elsewhere",
        report.system_font_fallbacks
    );
}

#[test]
fn a_document_naming_an_unbundled_typeface_still_stays_off_the_host() {
    // Tahoma, Verdana and Calibri are not in the Standard 14 and we do not ship
    // them. They are exactly the names that used to reach for the filesystem.
    for typeface in ["Tahoma", "Verdana", "Calibri", "ComicSansMS", "Wingdings"] {
        let mut doc = document_naming(typeface);
        let report = pdf_manip::pdfa_fonts::embed_fonts(&mut doc).expect("embedding failed");
        assert_eq!(
            report.system_font_fallbacks, 0,
            "{typeface} sent us to the filesystem"
        );
    }
}

#[test]
fn a_sans_typeface_gets_a_metric_compatible_sans_not_a_generic_fallback() {
    // Staying off the filesystem is only half of it. The heuristic alone would
    // do that -- it maps anything unknown onto a Standard 14 name, and those
    // are compiled in. Deleting the bundled-substitute lookup leaves the
    // fallback count at zero and every assertion above green, which is how this
    // gap was found: a test that checked the bundle rather than what the
    // conversion picked out of it.
    //
    // The bundled substitutes earn their place on quality. Liberation Sans is
    // metric-compatible with Arial and Tahoma -- same advance widths, so the
    // text does not reflow. Falling through to Helvetica would be a different
    // face with different widths.
    let mut doc = document_naming("Tahoma");
    pdf_manip::pdfa_fonts::embed_fonts(&mut doc).expect("embedding failed");

    let families = embedded_font_families(&doc);
    assert!(
        families.iter().any(|n| n.contains("Liberation")),
        "a Tahoma document was not given a metric-compatible sans; embedded: {families:?}"
    );
}

/// A one-page document whose only font is `typeface`, not embedded.
fn document_naming(typeface: &str) -> lopdf::Document {
    use lopdf::{dictionary, Document, Object};
    let mut doc = Document::with_version("1.7");
    let content = doc.add_object(Object::Stream(lopdf::Stream::new(
        dictionary! {},
        b"BT /F0 12 Tf 72 720 Td (test) Tj ET".to_vec(),
    )));
    let font = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "TrueType",
        "BaseFont" => typeface,
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
    }));
    let pages = doc.new_object_id();
    let page = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Page",
        "Parent" => Object::Reference(pages),
        "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
        "Resources" => Object::Dictionary(dictionary! {
        "Font" => Object::Dictionary(dictionary! { "F0" => Object::Reference(font) }),
        }),
        "Contents" => Object::Reference(content),
    }));
    doc.objects.insert(
        pages,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => Object::Array(vec![Object::Reference(page)]),
            "Count" => 1i64,
        }),
    );
    let catalog = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Catalog",
        "Pages" => Object::Reference(pages),
    }));
    doc.trailer.set("Root", Object::Reference(catalog));

    doc
}

/// The family names of every font program embedded in the document.
fn embedded_font_families(doc: &lopdf::Document) -> Vec<String> {
    let mut families = Vec::new();
    for obj in doc.objects.values() {
        let lopdf::Object::Stream(stream) = obj else {
            continue;
        };
        if !stream.dict.has(b"Length1") {
            continue;
        }
        if let Ok(face) = ttf_parser::Face::parse(&stream.content, 0) {
            for entry in face.names() {
                if entry.name_id == ttf_parser::name_id::FAMILY {
                    if let Some(s) = entry.to_string() {
                        families.push(s);
                    }
                }
            }
        }
    }
    families
}
