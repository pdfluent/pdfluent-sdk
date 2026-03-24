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

    // 2. Parse template → FormTree.
    let (tree, root_id) = parse_template(&template_xml)?;

    // 3. Layout.
    let engine = LayoutEngine::new(&tree);
    let layout = engine
        .layout(root_id)
        .map_err(|e| XfaError::LayoutFailed(format!("{e:?}")))?;

    if layout.pages.is_empty() {
        return Err(XfaError::LayoutFailed(
            "layout produced 0 pages".to_string(),
        ));
    }

    // 4. Generate content stream bytes for each layout page.
    let config = XfaRenderConfig::default();
    let overlays = generate_all_overlays(&layout, &config)
        .map_err(|e| XfaError::LayoutFailed(format!("overlay generation: {e:?}")))?;

    // 5. Mutate the lopdf document.
    let mut doc = Document::load_mem(pdf_bytes)
        .map_err(|e| XfaError::LoadFailed(format!("lopdf load: {e}")))?;

    // Build a font resource dictionary (Helvetica / Type 1).
    let font_dict = dictionary! {
        "Type"     => Object::Name(b"Font".to_vec()),
        "Subtype"  => Object::Name(b"Type1".to_vec()),
        "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec())
    };
    let font_id = doc.add_object(Object::Dictionary(font_dict));

    // Get the ordered list of existing page IDs.
    let existing_page_ids: Vec<ObjectId> = doc.page_iter().collect();
    let n_layout = overlays.len();
    let n_existing = existing_page_ids.len();

    // Reuse existing pages for the first `min(n_layout, n_existing)` layout pages.
    for (i, overlay_bytes) in overlays.iter().enumerate() {
        if i < n_existing {
            let page_id = existing_page_ids[i];
            // Replace content stream with the XFA overlay.
            write_page_content(&mut doc, page_id, overlay_bytes, font_id)?;
        } else {
            // Create an additional page for overflow pages.
            let lp = &layout.pages[i];
            let w = lp.width;
            let h = lp.height;
            add_new_page(&mut doc, w, h, overlay_bytes, font_id)?;
        }
    }

    // If layout produced fewer pages than the original PDF had, remove extras.
    if n_layout < n_existing {
        // We leave surplus pages blank rather than delete them (simpler,
        // and avoids page-tree corruption). They will render as white pages.
        for &page_id in &existing_page_ids[n_layout..n_existing] {
            write_page_content(&mut doc, page_id, &[], font_id)?;
        }
    }

    // 6. Remove /AcroForm from catalog.
    remove_acroform(&mut doc);

    let mut out = Vec::new();
    doc.save_to(&mut out)
        .map_err(|e| XfaError::LayoutFailed(format!("save: {e}")))?;
    Ok(out)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

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
    fn build_xfa_pdf(xdp: &str) -> Vec<u8> {
        use lopdf::{dictionary, Document, Object, Stream};
        let mut doc = Document::with_version("1.4");
        let xdp_bytes = xdp.as_bytes().to_vec();
        let xfa_stream = Stream::new(
            dictionary! { "Length" => Object::Integer(xdp_bytes.len() as i64) },
            xdp_bytes,
        );
        let xfa_id = doc.add_object(Object::Stream(xfa_stream));
        let pages_id = doc.new_object_id();
        let content_stream = Stream::new(dictionary! { "Length" => Object::Integer(0i64) }, vec![]);
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
}
