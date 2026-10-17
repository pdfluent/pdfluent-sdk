// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! One fixture per place a device colour space can come from.
//!
//! Round 2 of #379 makes the Default{CMYK,RGB,Gray} ICC profiles conditional.
//! Unconditional, they cost 16,333 bytes in every output -- 4,899,900 over the
//! 300-document tuning set against a total overshoot of 5,400,759
//! (`benchmarks/pdfa/size/ROUND1.md`). Conditional, they cost a census, and a
//! census with a hole in it is not a saving: it is a conformance regression on
//! a document nobody has, found by somebody else, because the profile that
//! would have characterised a DeviceCMYK mark was left out on the strength of
//! a container the census never looked in.
//!
//! So the census is not tested by one pass over a corpus, which can only say
//! that the documents we happen to hold are fine. It is tested here by one
//! document per source, each drawing CMYK by exactly one route and by no
//! other. Skip a route -- stop reading tiling patterns, stop reading Type3
//! glyph procedures, stop decompressing before scanning -- and exactly one
//! case below goes red and names the route.
//!
//! CMYK is the probe throughout because it is unambiguous in the output: a
//! four-component ICC profile is there or it is not. RGB is not usable for
//! that, since the sRGB output intent embeds the same profile bytes for its
//! own reasons, and gray is deliberately not conditional on much -- see
//! `a_painted_mark_is_a_gray_mark`.

use lopdf::{dictionary, Document, Object, Stream};
use pdf_manip::pdfa_colorspace::{census_device_colors, DeviceColorUse};

/// A one-page document with the given page-level extras, so each fixture
/// differs only in the route it uses to reach DeviceCMYK.
fn document(
    content: &[u8],
    resources: lopdf::Dictionary,
    extra_page: lopdf::Dictionary,
) -> Document {
    let mut doc = Document::with_version("1.7");
    let content_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        content.to_vec(),
    )));
    let mut page = dictionary! {
        "Type" => "Page",
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        "Contents" => Object::Reference(content_id),
        "Resources" => Object::Dictionary(resources),
    };
    for (key, value) in extra_page.iter() {
        page.set(String::from_utf8_lossy(key).to_string(), value.clone());
    }
    let page_id = doc.add_object(Object::Dictionary(page));
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
    doc
}

/// The page draws a gray box and nothing else; whatever reaches CMYK is in the
/// resources or the page dictionary the caller passes.
const GRAY_PAGE: &[u8] = b"0 g\n10 10 100 100 re f\n";

/// A page whose only colour is CMYK, set with all four operands.
fn cmyk_in_page_content() -> Document {
    document(
        b"0 0 0 1 k\n10 10 100 100 re f\n",
        dictionary! {},
        dictionary! {},
    )
}

/// The CMYK operator is inside a Form XObject the page invokes.
fn cmyk_in_form_xobject() -> Document {
    let mut doc = document(
        b"q /Fm0 Do Q\n",
        dictionary! { "XObject" => dictionary! {} },
        dictionary! {},
    );
    let form = Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
        },
        b"0 0 0 1 K\n0 0 100 100 re S\n".to_vec(),
    );
    let form_id = doc.add_object(Object::Stream(form));
    attach_resource(&mut doc, "XObject", "Fm0", form_id);
    doc
}

/// The CMYK operator is inside a tiling pattern's content stream.
fn cmyk_in_tiling_pattern() -> Document {
    let mut doc = document(
        b"q /Pattern cs /P0 scn 10 10 100 100 re f Q\n",
        dictionary! { "Pattern" => dictionary! {} },
        dictionary! {},
    );
    let pattern = Stream::new(
        dictionary! {
            "Type" => "Pattern",
            "PatternType" => 1_i64,
            "PaintType" => 1_i64,
            "TilingType" => 1_i64,
            "BBox" => vec![0.into(), 0.into(), 10.into(), 10.into()],
            "XStep" => 10_i64,
            "YStep" => 10_i64,
            "Resources" => dictionary! {},
        },
        b"0 0 0 1 k\n0 0 5 5 re f\n".to_vec(),
    );
    let pattern_id = doc.add_object(Object::Stream(pattern));
    attach_resource(&mut doc, "Pattern", "P0", pattern_id);
    doc
}

/// The CMYK operator is inside a Type3 glyph procedure.
fn cmyk_in_type3_glyph() -> Document {
    let mut doc = document(
        b"BT /T3 12 Tf (a) Tj ET\n",
        dictionary! { "Font" => dictionary! {} },
        dictionary! {},
    );
    let proc_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {},
        b"10 0 0 0 10 10 d1\n0 0 0 1 k\n0 0 10 10 re f\n".to_vec(),
    )));
    let font_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Font",
        "Subtype" => "Type3",
        "FontBBox" => vec![0.into(), 0.into(), 10.into(), 10.into()],
        "FontMatrix" => vec![
            Object::Real(0.001), 0.into(), 0.into(),
            Object::Real(0.001), 0.into(), 0.into(),
        ],
        "CharProcs" => dictionary! { "a" => Object::Reference(proc_id) },
        "Encoding" => dictionary! {
            "Type" => "Encoding",
            "Differences" => vec![97.into(), Object::Name(b"a".to_vec())],
        },
        "FirstChar" => 97_i64,
        "LastChar" => 97_i64,
        "Widths" => vec![10.into()],
    }));
    attach_resource(&mut doc, "Font", "T3", font_id);
    doc
}

/// The CMYK operator is inside an annotation's appearance stream, which the
/// page content stream never mentions.
fn cmyk_in_annotation_appearance() -> Document {
    let mut doc = document(GRAY_PAGE, dictionary! {}, dictionary! {});
    let ap_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), 20.into(), 20.into()],
        },
        b"0 0 0 1 k\n0 0 20 20 re f\n".to_vec(),
    )));
    let annot_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => "Annot",
        "Subtype" => "Square",
        "Rect" => vec![0.into(), 0.into(), 20.into(), 20.into()],
        "F" => 4_i64,
        "AP" => dictionary! { "N" => Object::Reference(ap_id) },
    }));
    set_on_page(
        &mut doc,
        "Annots",
        Object::Array(vec![Object::Reference(annot_id)]),
    );
    doc
}

/// The device space is a shading dictionary's /ColorSpace -- a name in the
/// object graph that no content stream operator repeats.
fn cmyk_in_shading_colorspace() -> Document {
    let mut doc = document(
        b"q /Sh0 sh Q\n",
        dictionary! { "Shading" => dictionary! {} },
        dictionary! {},
    );
    let shading_id = doc.add_object(Object::Dictionary(dictionary! {
        "ShadingType" => 2_i64,
        "ColorSpace" => Object::Name(b"DeviceCMYK".to_vec()),
        "Coords" => vec![0.into(), 0.into(), 100.into(), 100.into()],
    }));
    attach_resource(&mut doc, "Shading", "Sh0", shading_id);
    doc
}

/// The device space is an image XObject's /ColorSpace. The sample data is
/// opaque payload and is never scanned; the dictionary around it is.
fn cmyk_in_image_colorspace() -> Document {
    let mut doc = document(
        b"q 100 0 0 100 10 10 cm /Im0 Do Q\n",
        dictionary! { "XObject" => dictionary! {} },
        dictionary! {},
    );
    let image_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Image",
            "Width" => 1_i64,
            "Height" => 1_i64,
            "BitsPerComponent" => 8_i64,
            "ColorSpace" => Object::Name(b"DeviceCMYK".to_vec()),
        },
        vec![0, 0, 0, 255],
    )));
    attach_resource(&mut doc, "XObject", "Im0", image_id);
    doc
}

/// The device space is a Separation's alternate space. veraPDF follows that
/// link, so the census has to as well.
fn cmyk_in_separation_alternate() -> Document {
    let mut doc = document(
        b"q /Sep0 cs 1 scn 10 10 100 100 re f Q\n",
        dictionary! { "ColorSpace" => dictionary! {} },
        dictionary! {},
    );
    let tint_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {
            "FunctionType" => 4_i64,
            "Domain" => vec![0.into(), 1.into()],
            "Range" => vec![
                0.into(), 1.into(), 0.into(), 1.into(),
                0.into(), 1.into(), 0.into(), 1.into(),
            ],
        },
        b"{ dup dup dup }".to_vec(),
    )));
    let sep_id = doc.add_object(Object::Array(vec![
        Object::Name(b"Separation".to_vec()),
        Object::Name(b"Spot".to_vec()),
        Object::Name(b"DeviceCMYK".to_vec()),
        Object::Reference(tint_id),
    ]));
    attach_resource(&mut doc, "ColorSpace", "Sep0", sep_id);
    doc
}

/// The device space is an inline image's /CS abbreviation, in a dictionary
/// that is not an object in the document at all.
fn cmyk_in_inline_image() -> Document {
    let mut content = b"q 100 0 0 100 10 10 cm\nBI /W 1 /H 1 /BPC 8 /CS /CMYK\nID ".to_vec();
    content.extend_from_slice(&[0, 0, 0, 255]);
    content.extend_from_slice(b"\nEI\nQ\n");
    document(&content, dictionary! {}, dictionary! {})
}

/// The CMYK operator is there, behind a Flate filter. A census that scans raw
/// stream bytes sees compressed noise and reports no colour at all.
fn cmyk_in_a_compressed_stream() -> Document {
    let mut doc = document(b"", dictionary! {}, dictionary! {});
    let mut program = b"0 0 0 1 k\n".to_vec();
    for _ in 0..64 {
        program.extend_from_slice(b"10 10 100 100 re f\n");
    }
    let mut stream = Stream::new(dictionary! {}, program);
    stream.compress().expect("the fixture stream must compress");
    assert!(
        stream.dict.has(b"Filter"),
        "fixture check: the content has to be filtered for this case to mean anything"
    );
    let content_id = doc.add_object(Object::Stream(stream));
    set_on_page(&mut doc, "Contents", Object::Reference(content_id));
    doc
}

/// The CMYK operator is in a content stream the strict parser refuses. On the
/// 300-document tuning set that is 8,040 streams across 238 of the 300
/// documents, so a census that reads a parser complaint as "unreadable" has
/// given up on four documents in five.
fn cmyk_in_a_stream_the_parser_refuses() -> Document {
    // An unbalanced `<<` is enough: the dictionary operand never closes, so
    // the stream does not parse, and every byte of it is still there to read.
    document(
        b"0 0 0 1 k\n10 10 100 100 re f\n<< /Broken\n",
        dictionary! {},
        dictionary! {},
    )
}

/// A stream that will not decode. Nothing can say what colour it sets, so
/// every family counts as used -- the one rule that makes the rest of this
/// safe to be precise about.
fn an_undecodable_content_stream() -> Document {
    let mut doc = document(GRAY_PAGE, dictionary! {}, dictionary! {});
    let broken = Stream::new(
        dictionary! { "Filter" => "FlateDecode" },
        b"this is not a zlib stream".to_vec(),
    );
    let broken_id = doc.add_object(Object::Stream(broken));
    set_on_page(&mut doc, "Contents", Object::Reference(broken_id));
    doc
}

/// A document that reaches no device space but gray.
fn gray_only() -> Document {
    document(GRAY_PAGE, dictionary! {}, dictionary! {})
}

fn attach_resource(doc: &mut Document, category: &str, name: &str, target: (u32, u16)) {
    let page_id = *doc.get_pages().values().next().expect("one page");
    let Ok(Object::Dictionary(page)) = doc.get_object_mut(page_id) else {
        panic!("the fixture page is a dictionary");
    };
    let Ok(Object::Dictionary(resources)) = page.get_mut(b"Resources") else {
        panic!("the fixture page has inline resources");
    };
    let mut group = match resources.get(category.as_bytes()) {
        Ok(Object::Dictionary(d)) => d.clone(),
        _ => lopdf::Dictionary::new(),
    };
    group.set(name, Object::Reference(target));
    resources.set(category, Object::Dictionary(group));
}

fn set_on_page(doc: &mut Document, key: &str, value: Object) {
    let page_id = *doc.get_pages().values().next().expect("one page");
    if let Ok(Object::Dictionary(page)) = doc.get_object_mut(page_id) {
        page.set(key, value);
    }
}

/// One named route into DeviceCMYK and the document that takes it.
type Route = (&'static str, fn() -> Document);

/// Every route to DeviceCMYK.
fn cmyk_sources() -> Vec<Route> {
    vec![
        (
            "page content stream",
            cmyk_in_page_content as fn() -> Document,
        ),
        ("Form XObject", cmyk_in_form_xobject),
        ("tiling pattern", cmyk_in_tiling_pattern),
        ("Type3 glyph procedure", cmyk_in_type3_glyph),
        (
            "annotation appearance stream",
            cmyk_in_annotation_appearance,
        ),
        ("shading /ColorSpace", cmyk_in_shading_colorspace),
        ("image /ColorSpace", cmyk_in_image_colorspace),
        ("Separation alternate", cmyk_in_separation_alternate),
        ("inline image /CS", cmyk_in_inline_image),
        (
            "a Flate-compressed content stream",
            cmyk_in_a_compressed_stream,
        ),
        (
            "a stream the parser refuses",
            cmyk_in_a_stream_the_parser_refuses,
        ),
        ("an undecodable stream", an_undecodable_content_stream),
    ]
}

#[test]
fn every_route_to_devicecmyk_is_counted() {
    // Every route, not the first one that fails: a census loses whole classes
    // of container at a time -- read only page /Contents and forms, patterns,
    // glyph procedures and appearance streams all go dark together -- and a
    // test that stops at the first of them reports one route missing where six
    // are.
    let missed: Vec<&str> = cmyk_sources()
        .into_iter()
        .filter(|(_, build)| !census_device_colors(&build()).cmyk)
        .map(|(source, _)| source)
        .collect();
    assert!(
        missed.is_empty(),
        "the census missed DeviceCMYK reached through: {}. That is not a lost \
         byte: the conversion will now leave out the ICC profile that \
         characterises the mark these documents make, and veraPDF 6.2.4.3 \
         fails on them. Add the route to `census_device_colors` -- every \
         branch of it is there because one of these documents went red.",
        missed.join(", ")
    );
}

#[test]
fn a_document_that_reaches_no_cmyk_is_not_charged_for_it() {
    let census = census_device_colors(&gray_only());
    assert_eq!(
        census,
        DeviceColorUse {
            gray: true,
            rgb: false,
            cmyk: false,
            undecodable_stream: false,
        },
        "a page that fills a gray box uses DeviceGray and nothing else. If \
         this reports CMYK or RGB the conditional profiles save nothing, which \
         is the whole of round 2 of #379."
    );
}

#[test]
fn an_undecodable_stream_counts_as_every_colour() {
    let census = census_device_colors(&an_undecodable_content_stream());
    assert!(
        census.gray && census.rgb && census.cmyk && census.undecodable_stream,
        "a stream that will not decode may set any colour. Reporting less than \
         everything here is how a census acquires a hole: got {census:?}"
    );
}

#[test]
fn a_painted_mark_is_a_gray_mark() {
    // PDF 32000-1 8.6.8: a content stream starts in DeviceGray. A mark made
    // before any colour operator is therefore a DeviceGray mark, and deciding
    // per mark whether a colour was selected first needs a graphics-state
    // interpreter across q/Q, form invocation and pattern nesting. The profile
    // this keeps is 126 bytes.
    let doc = document(b"10 10 100 100 re f\n", dictionary! {}, dictionary! {});
    assert!(
        census_device_colors(&doc).gray,
        "a fill with no colour operator in front of it is a DeviceGray fill"
    );
}

#[test]
fn an_icc_profiles_alternate_is_not_a_use_of_that_device_space() {
    // /Alternate names what a reader would fall back to if it could not use
    // the profile; the colour space in use is the ICCBased one. Counting it
    // would charge every document carrying a CMYK ICC profile -- and every
    // output of ours, whose sRGB output intent has /Alternate /DeviceRGB.
    let mut doc = document(GRAY_PAGE, dictionary! {}, dictionary! {});
    let icc_id = doc.add_object(Object::Stream(Stream::new(
        dictionary! {
            "N" => 4_i64,
            "Alternate" => Object::Name(b"DeviceCMYK".to_vec()),
        },
        vec![0u8; 32],
    )));
    let cs_id = doc.add_object(Object::Array(vec![
        Object::Name(b"ICCBased".to_vec()),
        Object::Reference(icc_id),
    ]));
    attach_resource(&mut doc, "ColorSpace", "Cs0", cs_id);
    assert!(
        !census_device_colors(&doc).cmyk,
        "an ICCBased colour space is the colour space in use; its /Alternate is \
         a fallback, not a mark on a page"
    );
}

/// Every ICC profile in a converted document, by component count.
fn icc_components(pdf: &[u8]) -> Vec<i64> {
    let doc = Document::load_mem(pdf).expect("the converted document must load");
    let mut components: Vec<i64> = doc
        .objects
        .values()
        .filter_map(|object| match object {
            Object::Stream(stream) => stream.dict.get(b"N").ok()?.as_i64().ok(),
            _ => None,
        })
        .collect();
    components.sort_unstable();
    components
}

fn convert(doc: Document) -> Vec<u8> {
    let mut buffer = Vec::new();
    let mut doc = doc;
    doc.save_to(&mut buffer).expect("the fixture must save");
    let opts = pdf_manip::pdfa::PdfAConvertOptions::default();
    pdf_manip::pdfa::convert_bytes(&buffer, &opts).expect("conversion failed")
}

#[test]
fn the_census_decides_what_a_conversion_embeds() {
    let missed: Vec<&str> = cmyk_sources()
        .into_iter()
        .filter(|(_, build)| !icc_components(&convert(build())).contains(&4))
        .map(|(source, _)| source)
        .collect();
    assert!(
        missed.is_empty(),
        "documents that reach DeviceCMYK through {} were converted without a \
         four-component ICC profile in them. The census may be right and the \
         wiring wrong: `normalize_colorspaces` only calls \
         `add_default_cmyk_colorspace` when the census says so.",
        missed.join(", ")
    );

    let converted = convert(gray_only());
    assert!(
        !icc_components(&converted).contains(&4),
        "a document that never reaches DeviceCMYK was still charged 8,325 \
         bytes for a CMYK profile. Sizes now: {:?}",
        icc_components(&converted)
    );
}
