//! XFA flattening: parse XFA template, run layout, write PDF content streams.
//!
//! `flatten_xfa_to_pdf` is the single entry point. It:
//! 1. Extracts the XFA packets from the PDF (via `extract::extract_xfa`).
//! 2. Parses the `<template>` packet into a `FormTree`.
//! 3. Runs `LayoutEngine::layout()` to produce a `LayoutDom`.
//! 4. Converts each layout page into PDF content stream bytes.
//! 5. Writes the streams back into the PDF pages (replacing empty streams),
//!    adding Helvetica as a /Font resource, and expanding the page tree to
//!    match the layout page count.
//! 6. Removes the /AcroForm entry from the catalog.
//!
//! The result is a static PDF with no XFA dependency: it can be rendered
//! by any standard PDF viewer.

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream};

use crate::error::{Result, XfaError};
use crate::extract::extract_xfa_from_bytes;
use crate::render_bridge::{generate_all_overlays, XfaRenderConfig};
use crate::template_parser::parse_template;
use xfa_layout_engine::layout::LayoutEngine;

/// Flatten all XFA content in `pdf_bytes` to static PDF content streams.
///
/// Returns the modified PDF bytes. The /AcroForm entry is removed so the
/// result is a plain PDF/1.4 document.
///
/// If the PDF has no XFA content, returns a clone of the input unchanged.
pub fn flatten_xfa_to_pdf(pdf_bytes: &[u8]) -> Result<Vec<u8>> {
    // 1. Extract XFA packets.
    let packets = match extract_xfa_from_bytes(pdf_bytes.to_vec()) {
        Ok(p) => p,
        Err(_) => {
            // No XFA — return as-is.
            return Ok(pdf_bytes.to_vec());
        }
    };

    let template_xml = match packets.template() {
        Some(t) => t.to_string(),
        None => {
            // No template packet — nothing to flatten.
            return Ok(pdf_bytes.to_vec());
        }
    };

    // 1b. Detect pre-rendered pages: if the PDF's existing pages already contain
    // substantial static content (non-empty content streams), this is a "hybrid"
    // XFA+static PDF.  iText 5 preserves the existing static rendering in this
    // case rather than re-rendering from the template.  We do the same: strip the
    // AcroForm and Widget annotations, keep the original page content.
    //
    // This handles government forms (gen-776, gen-778, r3-PDFBOX-2755-0, etc.)
    // that carry both an XFA template and pre-flattened PDF page streams.
    if let Ok(doc) = Document::load_mem(pdf_bytes) {
        if pages_have_static_content(&doc) {
            let mut doc_mut = Document::load_mem(pdf_bytes)
                .map_err(|e| XfaError::LoadFailed(format!("lopdf load: {e}")))?;
            strip_widgets_and_acroform(&mut doc_mut);
            let mut out = Vec::new();
            doc_mut
                .save_to(&mut out)
                .map_err(|e| XfaError::LayoutFailed(format!("save: {e}")))?;
            return Ok(out);
        }
    }

    // 2. Try XFA template → layout → render pipeline.
    //    If this fails (parse error, empty template, layout 0 pages, lopdf error),
    //    fall back to preserving the existing page content with AcroForm stripped.
    match xfa_flatten_inner(pdf_bytes, &template_xml, packets.datasets()) {
        Ok(out) => Ok(out),
        Err(_) => static_fallback(pdf_bytes),
    }
}

/// Core XFA flatten pipeline: parse template, bind data, layout, render.
fn xfa_flatten_inner(
    pdf_bytes: &[u8],
    template_xml: &str,
    datasets_xml: Option<&str>,
) -> Result<Vec<u8>> {
    use crate::dynamic::apply_dynamic_scripts;

    let (mut tree, root_id) = parse_template(template_xml, datasets_xml)?;
    let _ = apply_dynamic_scripts(&mut tree, root_id);

    let engine = LayoutEngine::new(&tree);
    let layout = engine
        .layout(root_id)
        .map_err(|e| XfaError::LayoutFailed(format!("{e:?}")))?;

    if layout.pages.is_empty() {
        return Err(XfaError::LayoutFailed("layout produced 0 pages".into()));
    }

    let config = XfaRenderConfig::default();
    let overlays = generate_all_overlays(&layout, &config)
        .map_err(|e| XfaError::LayoutFailed(format!("overlay generation: {e:?}")))?;

    let mut doc = Document::load_mem(pdf_bytes)
        .map_err(|e| XfaError::LoadFailed(format!("lopdf load: {e}")))?;

    let font_dict = dictionary! {
        "Type"     => Object::Name(b"Font".to_vec()),
        "Subtype"  => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec())
    };
    let font_id = doc.add_object(Object::Dictionary(font_dict));

    let existing_page_ids: Vec<ObjectId> = doc.page_iter().collect();
    let n_layout = overlays.len();
    let n_existing = existing_page_ids.len();

    for (i, overlay_bytes) in overlays.iter().enumerate() {
        if i < n_existing {
            write_page_content(&mut doc, existing_page_ids[i], overlay_bytes, font_id)?;
        } else {
            let lp = &layout.pages[i];
            add_new_page(&mut doc, lp.width, lp.height, overlay_bytes, font_id)?;
        }
    }

    if n_layout < n_existing {
        for &page_id in &existing_page_ids[n_layout..n_existing] {
            write_page_content(&mut doc, page_id, &[], font_id)?;
        }
    }

    remove_acroform(&mut doc);

    let mut out = Vec::new();
    doc.save_to(&mut out)
        .map_err(|e| XfaError::LayoutFailed(format!("save: {e}")))?;
    Ok(out)
}

/// Fallback: preserve existing page content, strip AcroForm/widgets only.
/// If lopdf can't parse the PDF (corrupt xref), return the original bytes
/// unchanged — the PDF is too corrupt for us to modify but still renderable.
fn static_fallback(pdf_bytes: &[u8]) -> Result<Vec<u8>> {
    let mut doc = match Document::load_mem(pdf_bytes) {
        Ok(d) => d,
        Err(_) => return Ok(pdf_bytes.to_vec()), // Too corrupt — return as-is
    };
    strip_widgets_and_acroform(&mut doc);
    let mut out = Vec::new();
    doc.save_to(&mut out)
        .map_err(|e| XfaError::LayoutFailed(format!("fallback save: {e}")))?;
    Ok(out)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Returns `true` when the PDF's pages already carry substantial static content.
///
/// An array /Contents entry (multiple streams) or any individual stream larger
/// than 200 bytes indicates pre-flattened page content that should be preserved
/// rather than replaced by XFA re-rendering. Adobe's default XFA fallback
/// page ("Please wait..." / Adobe Reader upgrade text) is explicitly ignored:
/// those bytes are not real pre-rendered form content and must not suppress
/// XFA flattening.
fn pages_have_static_content(doc: &Document) -> bool {
    for page_id in doc.page_iter() {
        let streams = page_content_streams(doc, page_id);
        if streams.is_empty() {
            continue;
        }

        // Require a MUCH higher byte threshold — simple form chrome
        // (borders, labels, headers) can easily exceed 200 bytes without
        // containing actual field data.  Use 20 KB as the threshold:
        // only truly pre-rendered pages (with full form data embedded)
        // would be this large.  This prevents the early-return path from
        // suppressing XFA flatten for forms like USCIS I-765 where the
        // static rendering has layout but no data values.
        let has_substantial_content = streams.iter().map(|s| s.len()).sum::<usize>() > 20_000;
        if !has_substantial_content {
            continue;
        }

        if streams
            .iter()
            .all(|stream| is_xfa_placeholder_stream(stream))
        {
            continue;
        }

        return true;
    }
    false
}

fn page_content_streams(doc: &Document, page_id: ObjectId) -> Vec<Vec<u8>> {
    let Ok(page_dict) = doc.get_dictionary(page_id) else {
        return Vec::new();
    };

    match page_dict.get(b"Contents") {
        Ok(Object::Array(arr)) => arr
            .iter()
            .filter_map(|object| resolve_stream_content(doc, object))
            .collect(),
        Ok(object) => resolve_stream_content(doc, object).into_iter().collect(),
        Err(_) => Vec::new(),
    }
}

fn resolve_stream_content(doc: &Document, object: &Object) -> Option<Vec<u8>> {
    let stream = match object {
        Object::Reference(id) => doc.get_object(*id).ok()?.as_stream().ok()?,
        Object::Stream(stream) => stream,
        _ => return None,
    };

    stream
        .get_plain_content()
        .ok()
        .or_else(|| Some(stream.content.clone()))
}

fn is_xfa_placeholder_stream(stream: &[u8]) -> bool {
    const PLACEHOLDER_MARKERS: [&[u8]; 4] = [
        b"Please wait",
        b"Adobe Reader",
        b"reader_download",
        b"display this type of document",
    ];

    PLACEHOLDER_MARKERS
        .iter()
        .any(|marker| contains_ascii_case_insensitive(stream, marker))
}

fn contains_ascii_case_insensitive(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

/// Remove Widget annotations from all pages and strip /AcroForm from the catalog.
///
/// This is the "static-strip" flatten path used for hybrid XFA+static PDFs:
/// the original page content is preserved and only the interactive XFA/AcroForm
/// layer is removed.
fn strip_widgets_and_acroform(doc: &mut Document) {
    // Collect Widget annotation object IDs.
    let widget_ids: std::collections::HashSet<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(&id, obj)| {
            let dict = obj.as_dict().ok()?;
            let subtype = dict.get(b"Subtype").ok()?;
            if matches!(subtype, Object::Name(n) if n == b"Widget") {
                Some(id)
            } else {
                None
            }
        })
        .collect();

    // Remove Widget refs from page /Annots arrays.
    let page_ids: Vec<ObjectId> = doc.page_iter().collect();
    for page_id in page_ids {
        let annots_ref = {
            let Ok(page_dict) = doc.get_dictionary(page_id) else {
                continue;
            };
            match page_dict.get(b"Annots") {
                Ok(Object::Reference(r)) => Some(*r),
                _ => None,
            }
        };

        if let Some(ref_id) = annots_ref {
            if let Ok(Object::Array(arr)) = doc.get_object(ref_id).cloned() {
                let filtered: Vec<Object> = arr
                    .into_iter()
                    .filter(|o| !matches!(o, Object::Reference(r) if widget_ids.contains(r)))
                    .collect();
                doc.objects.insert(ref_id, Object::Array(filtered));
            }
        }
    }

    // Strip /AcroForm from catalog.
    remove_acroform(doc);
}

/// Replace a page's /Contents stream with XFA overlay bytes and add font resource.
fn write_page_content(
    doc: &mut Document,
    page_id: ObjectId,
    content: &[u8],
    font_id: ObjectId,
) -> Result<()> {
    // Build resources dict with Helvetica.
    let resources = make_resources_dict(font_id);

    // Build content stream.
    let stream = Stream::new(
        dictionary! { "Length" => Object::Integer(content.len() as i64) },
        content.to_vec(),
    );
    let stream_id = doc.add_object(Object::Stream(stream));

    // Mutate the page dict.
    if let Ok(Object::Dictionary(ref mut page_dict)) = doc.get_object_mut(page_id) {
        page_dict.set("Contents", Object::Reference(stream_id));
        page_dict.set("Resources", Object::Dictionary(resources));
    }
    Ok(())
}

/// Add a new page to the document's /Pages tree.
fn add_new_page(
    doc: &mut Document,
    w: f64,
    h: f64,
    content: &[u8],
    font_id: ObjectId,
) -> Result<()> {
    let resources = make_resources_dict(font_id);
    let stream = Stream::new(
        dictionary! { "Length" => Object::Integer(content.len() as i64) },
        content.to_vec(),
    );
    let stream_id = doc.add_object(Object::Stream(stream));

    // Find the /Pages root to append to.
    let pages_id = find_pages_root(doc)?;

    let page_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type"      => Object::Name(b"Page".to_vec()),
        "Parent"    => Object::Reference(pages_id),
        "MediaBox"  => Object::Array(vec![
            Object::Integer(0), Object::Integer(0),
            Object::Real(w as f32), Object::Real(h as f32),
        ]),
        "Contents"  => Object::Reference(stream_id),
        "Resources" => Object::Dictionary(resources)
    }));

    // Append to /Kids and increment /Count.
    if let Ok(Object::Dictionary(ref mut pages_dict)) = doc.get_object_mut(pages_id) {
        if let Ok(Object::Array(ref mut kids)) = pages_dict.get_mut(b"Kids") {
            kids.push(Object::Reference(page_id));
        }
        if let Ok(Object::Integer(ref mut count)) = pages_dict.get_mut(b"Count") {
            *count += 1;
        }
    }
    Ok(())
}

fn make_resources_dict(font_id: ObjectId) -> Dictionary {
    let mut fonts = Dictionary::new();
    fonts.set("F1", Object::Reference(font_id));
    let mut resources = Dictionary::new();
    resources.set("Font", Object::Dictionary(fonts));
    resources
}

fn find_pages_root(doc: &Document) -> Result<ObjectId> {
    let root_id = doc
        .trailer
        .get(b"Root")
        .ok()
        .and_then(|o: &Object| o.as_reference().ok())
        .ok_or_else(|| XfaError::LoadFailed("no /Root in trailer".to_string()))?;
    let catalog = doc
        .get_dictionary(root_id)
        .map_err(|e| XfaError::LoadFailed(format!("catalog: {e}")))?;
    catalog
        .get(b"Pages")
        .ok()
        .and_then(|o: &Object| o.as_reference().ok())
        .ok_or_else(|| XfaError::LoadFailed("no /Pages in catalog".to_string()))
}

fn remove_acroform(doc: &mut Document) {
    let root_id = match doc.trailer.get(b"Root") {
        Ok(Object::Reference(id)) => *id,
        _ => return,
    };
    if let Ok(Object::Dictionary(ref mut dict)) = doc.get_object_mut(root_id) {
        dict.remove(b"AcroForm");
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal XFA PDF in memory (same as generate_xfa_layout_fixtures).
    fn build_xfa_pdf_with_content(xdp: &str, page_content: Vec<u8>) -> Vec<u8> {
        use lopdf::{dictionary, Document, Object, Stream};
        let mut doc = Document::with_version("1.4");
        let xdp_bytes = xdp.as_bytes().to_vec();
        let xfa_stream = Stream::new(
            dictionary! { "Length" => Object::Integer(xdp_bytes.len() as i64) },
            xdp_bytes,
        );
        let xfa_id = doc.add_object(Object::Stream(xfa_stream));
        let pages_id = doc.new_object_id();
        let content_stream = Stream::new(
            dictionary! { "Length" => Object::Integer(page_content.len() as i64) },
            page_content,
        );
        let content_id = doc.add_object(Object::Stream(content_stream));
        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type"     => Object::Name(b"Page".to_vec()),
            "Parent"   => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(612), Object::Integer(792),
            ]),
            "Contents" => Object::Reference(content_id)
        }));
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type"  => Object::Name(b"Pages".to_vec()),
                "Kids"  => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => Object::Integer(1)
            }),
        );
        let acroform_id = doc.add_object(Object::Dictionary(dictionary! {
            "XFA"    => Object::Reference(xfa_id),
            "Fields" => Object::Array(vec![])
        }));
        let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type"     => Object::Name(b"Catalog".to_vec()),
            "Pages"    => Object::Reference(pages_id),
            "AcroForm" => Object::Reference(acroform_id)
        }));
        doc.trailer.set("Root", Object::Reference(catalog_id));
        let mut out = Vec::new();
        doc.save_to(&mut out).unwrap();
        out
    }

    fn build_xfa_pdf(xdp: &str) -> Vec<u8> {
        build_xfa_pdf_with_content(xdp, Vec::new())
    }

    const SIMPLE_XDP: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="paginate">
    <pageSet>
      <pageArea name="Page1">
        <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
        <medium stock="default" short="8.5in" long="11in"/>
      </pageArea>
    </pageSet>
    <subform name="section" layout="tb" w="7.5in">
      <field name="firstName" w="3.5in" h="0.3in">
        <caption><value><text>First Name</text></value></caption>
        <ui><textEdit/></ui>
        <value><text>John</text></value>
      </field>
    </subform>
  </subform>
</template>
</xdp:xdp>"#;

    #[test]
    fn flatten_simple_form_produces_non_empty_content() {
        let pdf_bytes = build_xfa_pdf(SIMPLE_XDP);
        let result = flatten_xfa_to_pdf(&pdf_bytes).expect("flatten failed");

        // Load the result and check the content stream is non-empty.
        let doc = Document::load_mem(&result).expect("load flattened PDF");
        let pages: Vec<ObjectId> = doc.page_iter().collect();
        assert!(!pages.is_empty(), "flattened PDF has no pages");

        // At least one page should have a non-empty content stream.
        let mut found_content = false;
        for page_id in &pages {
            if let Ok(page_dict) = doc.get_dictionary(*page_id) {
                if let Ok(contents_ref) = page_dict.get(b"Contents") {
                    if let Object::Reference(stream_id) = contents_ref {
                        if let Ok(obj) = doc.get_object(*stream_id) {
                            if let Ok(stream) = obj.as_stream() {
                                if !stream.content.is_empty() {
                                    found_content = true;
                                }
                            }
                        }
                    }
                }
            }
        }
        assert!(found_content, "all content streams are empty after flatten");
    }

    /// Tests the canonical XFA nesting: <subform layout="paginate"> wraps
    /// <pageSet> + lr-tb content rows.  Verifies the flatten produces a single
    /// page with visible field content (border operators in the content stream).
    /// Before the extract_page_structure fix this produced 2 pages: page 1
    /// was blank (pageSet occupied 792pt) and page 2 had the actual fields.
    #[test]
    fn flatten_paginate_subform_with_nested_pageset_produces_visible_content() {
        const LR_TB_XDP: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
  <subform name="form1" layout="paginate" locale="en_US">
    <pageSet>
      <pageArea name="Page1" id="Page1">
        <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
        <medium stock="default" short="8.5in" long="11in"/>
      </pageArea>
    </pageSet>
    <subform name="row1" layout="lr-tb" w="7.5in" h="0.4in">
      <field name="firstName" w="3.5in" h="0.4in">
        <caption><value><text>First</text></value></caption>
        <ui><textEdit/></ui>
        <value><text>John</text></value>
      </field>
      <field name="lastName" w="3.5in" h="0.4in">
        <caption><value><text>Last</text></value></caption>
        <ui><textEdit/></ui>
        <value><text>Doe</text></value>
      </field>
    </subform>
  </subform>
</template>
</xdp:xdp>"#;

        let pdf_bytes = build_xfa_pdf(LR_TB_XDP);
        let result = flatten_xfa_to_pdf(&pdf_bytes).expect("flatten failed");

        let doc = Document::load_mem(&result).expect("load flattened PDF");
        let pages: Vec<ObjectId> = doc.page_iter().collect();

        // Must produce exactly 1 page (not 2 as with the blank-first-page bug).
        assert_eq!(pages.len(), 1, "expected 1 page, got {}", pages.len());

        // Page 1 must contain visible text operators from the field values.
        // (Fields with non-empty values produce WrappedText → BT/ET operators.)
        if let Ok(page_dict) = doc.get_dictionary(pages[0]) {
            if let Ok(lopdf::Object::Reference(stream_id)) = page_dict.get(b"Contents") {
                if let Ok(obj) = doc.get_object(*stream_id) {
                    if let Ok(stream) = obj.as_stream() {
                        let content = String::from_utf8_lossy(&stream.content);
                        assert!(
                            content.contains("BT\n"),
                            "no text operators in page 1 content stream (should have BT from field values)"
                        );
                        assert!(
                            content.contains("Tj\n"),
                            "no text show operators in page 1 content stream"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn flatten_removes_acroform() {
        let pdf_bytes = build_xfa_pdf(SIMPLE_XDP);
        let result = flatten_xfa_to_pdf(&pdf_bytes).expect("flatten failed");
        let doc = Document::load_mem(&result).expect("load flattened PDF");
        let root_id = doc.trailer.get(b"Root").unwrap().as_reference().unwrap();
        let catalog = doc.get_dictionary(root_id).unwrap();
        assert!(
            catalog.get(b"AcroForm").is_err(),
            "/AcroForm still present after flatten"
        );
    }

    #[test]
    fn flatten_non_xfa_pdf_unchanged() {
        // A PDF with no XFA should be returned as-is (no error).
        let mut doc = Document::with_version("1.4");
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type"   => Object::Name(b"Page".to_vec()),
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(612), Object::Integer(792),
            ])
        }));
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type"  => Object::Name(b"Pages".to_vec()),
                "Kids"  => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => Object::Integer(1)
            }),
        );
        let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type"  => Object::Name(b"Catalog".to_vec()),
            "Pages" => Object::Reference(pages_id)
        }));
        doc.trailer.set("Root", Object::Reference(catalog_id));
        let mut raw = Vec::new();
        doc.save_to(&mut raw).unwrap();

        // flatten_xfa_to_pdf should return Ok (with the same bytes).
        let result = flatten_xfa_to_pdf(&raw).expect("flatten non-XFA failed");
        assert!(!result.is_empty());
    }

    #[test]
    fn placeholder_only_page_does_not_trigger_static_strip_path() {
        const PLACEHOLDER_STREAM: &str = r#"BT
/Helv 24 Tf
72 720 Td
(Please wait...) Tj
0 -32 Td
(If this message is not eventually replaced by the proper contents of the document,) Tj
0 -32 Td
(your PDF viewer may not be able to display this type of document.) Tj
0 -32 Td
(You can upgrade to the latest version of Adobe Reader by visiting reader_download.) Tj
ET
"#;

        let pdf_bytes =
            build_xfa_pdf_with_content(SIMPLE_XDP, PLACEHOLDER_STREAM.as_bytes().to_vec());
        let result = flatten_xfa_to_pdf(&pdf_bytes).expect("flatten failed");

        let doc = Document::load_mem(&result).expect("load flattened PDF");
        let page_id = doc.page_iter().next().expect("flattened page");
        let page_dict = doc.get_dictionary(page_id).expect("page dict");
        let contents_id = page_dict
            .get(b"Contents")
            .ok()
            .and_then(|object| object.as_reference().ok())
            .expect("contents ref");
        let stream = doc
            .get_object(contents_id)
            .expect("contents object")
            .as_stream()
            .expect("contents stream");
        let content = String::from_utf8_lossy(&stream.content);

        assert!(
            content.contains("John"),
            "flattened page should contain XFA-rendered field content"
        );
        assert!(
            !content.contains("Please wait"),
            "placeholder text should not survive XFA flattening"
        );
    }
}
