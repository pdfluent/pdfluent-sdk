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
use crate::merger::FormMerger;
use crate::render_bridge::{generate_all_overlays, XfaRenderConfig};
use xfa_dom_resolver::data_dom::DataDom;
use xfa_layout_engine::layout::LayoutEngine;

/// Returns `true` if the PDF bytes contain an `/Encrypt` entry in the trailer.
pub fn is_pdf_encrypted(pdf_bytes: &[u8]) -> bool {
    Document::load_mem(pdf_bytes)
        .map(|doc| doc.trailer.get(b"Encrypt").is_ok())
        .unwrap_or(false)
}

enum DecryptResult {
    NotEncrypted,
    Decrypted(Vec<u8>),
    NeedsPassword,
}

/// Try to handle encryption: if not encrypted return as-is, if encrypted try
/// empty password (owner-only encryption), otherwise report needs-password.
fn try_decrypt_pdf(pdf_bytes: &[u8]) -> DecryptResult {
    let mut doc = match Document::load_mem(pdf_bytes) {
        Ok(d) => d,
        Err(_) => return DecryptResult::NotEncrypted, // Can't parse — let downstream handle it
    };

    // lopdf auto-decrypts with empty password on load and removes /Encrypt.
    // Use was_encrypted() to detect this — the original bytes are still encrypted
    // and downstream parsers (pdf_syntax) can't read them.
    if doc.was_encrypted() {
        // Already decrypted by lopdf — save the decrypted document.
        let mut buf = Vec::new();
        match doc.save_to(&mut buf) {
            Ok(()) => return DecryptResult::Decrypted(buf),
            Err(_) => return DecryptResult::NeedsPassword,
        }
    }

    if doc.trailer.get(b"Encrypt").is_ok() {
        // /Encrypt present but lopdf couldn't auto-decrypt — try explicit empty password.
        match Document::load_mem_with_password(pdf_bytes, "") {
            Ok(mut decrypted_doc) => {
                decrypted_doc.trailer.remove(b"Encrypt");
                let mut buf = Vec::new();
                match decrypted_doc.save_to(&mut buf) {
                    Ok(()) => return DecryptResult::Decrypted(buf),
                    Err(_) => return DecryptResult::NeedsPassword,
                }
            }
            Err(_) => return DecryptResult::NeedsPassword,
        }
    }

    DecryptResult::NotEncrypted
}

/// Flatten all XFA content in `pdf_bytes` to static PDF content streams.
///
/// Returns the modified PDF bytes. The /AcroForm entry is removed so the
/// result is a plain PDF/1.4 document.
///
/// If the PDF has no XFA content, returns a clone of the input unchanged.
pub fn flatten_xfa_to_pdf(pdf_bytes: &[u8]) -> Result<Vec<u8>> {
    // 0. Handle encrypted PDFs: try empty-password decrypt (owner-only encryption),
    //    otherwise reject early — encrypted content produces garbage output.
    let decrypted;
    let pdf_bytes = match try_decrypt_pdf(pdf_bytes) {
        DecryptResult::NotEncrypted => pdf_bytes,
        DecryptResult::Decrypted(bytes) => {
            decrypted = bytes;
            &decrypted
        }
        DecryptResult::NeedsPassword => {
            return Err(XfaError::Encrypted(
                "PDF is encrypted and requires a password".into(),
            ));
        }
    };

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
    // XFA+static PDF.  Preserve the existing static rendering and, when
    // widget appearance streams are available, bake them into the page
    // content before removing the interactive layer.
    if let Ok(doc) = Document::load_mem(pdf_bytes) {
        if pages_have_static_content(&doc) {
            let mut doc_mut = Document::load_mem(pdf_bytes)
                .map_err(|e| XfaError::LoadFailed(format!("lopdf load: {e}")))?;
            if flatten_widget_appearances(&mut doc_mut) == 0 {
                strip_widgets_and_acroform(&mut doc_mut);
            } else {
                remove_acroform(&mut doc_mut);
            }
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
        Err(e) => {
            eprintln!("XFA flatten failed: {e:?}");
            static_fallback(pdf_bytes)
        }
    }
}

/// Core XFA flatten pipeline: parse template, bind data, layout, render.
fn xfa_flatten_inner(
    pdf_bytes: &[u8],
    template_xml: &str,
    datasets_xml: Option<&str>,
) -> Result<Vec<u8>> {
    use crate::dynamic::apply_dynamic_scripts;

    let data_dom = if let Some(ds_xml) = datasets_xml {
        DataDom::from_xml(ds_xml)
            .map_err(|e| XfaError::ParseFailed(format!("datasets parse: {e}")))?
    } else {
        DataDom::new()
    };

    let merger = FormMerger::new(&data_dom);
    let (mut tree, root_id) = merger
        .merge(template_xml)
        .map_err(|e| XfaError::ParseFailed(format!("template merge: {e}")))?;

    let _ = apply_dynamic_scripts(&mut tree, root_id);

    // Temporary tree dump for debugging
    fn dump_tree(tree: &xfa_layout_engine::form::FormTree, id: xfa_layout_engine::form::FormNodeId, depth: usize) {
        if depth > 6 { return; }
        let node = tree.get(id);
        let meta = tree.meta(id);
        let indent = "  ".repeat(depth);
        let val = match &node.node_type {
            xfa_layout_engine::form::FormNodeType::Field { value } if !value.is_empty() => format!(" val={:?}", &value[..value.len().min(30)]),
            _ => String::new(),
        };
        eprintln!("{indent}{:?} {:?} {:?} {:?} bm={}x{} presence={:?} children={}{}",
            id, node.name, node.layout,
            std::mem::discriminant(&node.node_type),
            node.box_model.width.map_or("auto".to_string(), |w| format!("{:.0}", w)),
            node.box_model.height.map_or("auto".to_string(), |h| format!("{:.0}", h)),
            meta.presence, node.children.len(), val);
        for &cid in &node.children {
            dump_tree(tree, cid, depth + 1);
        }
    }
    dump_tree(&tree, root_id, 0);

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

    // Register standard PDF fonts: F1=Times-Roman (serif), F2=Helvetica (sans), F3=Courier (mono).
    let font_ids: [ObjectId; 3] = [
        doc.add_object(Object::Dictionary(dictionary! {
            "Type"     => Object::Name(b"Font".to_vec()),
            "Subtype"  => Object::Name(b"Type1".to_vec()),
            "BaseFont" => Object::Name(b"Times-Roman".to_vec()),
            "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec())
        })),
        doc.add_object(Object::Dictionary(dictionary! {
            "Type"     => Object::Name(b"Font".to_vec()),
            "Subtype"  => Object::Name(b"Type1".to_vec()),
            "BaseFont" => Object::Name(b"Helvetica".to_vec()),
            "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec())
        })),
        doc.add_object(Object::Dictionary(dictionary! {
            "Type"     => Object::Name(b"Font".to_vec()),
            "Subtype"  => Object::Name(b"Type1".to_vec()),
            "BaseFont" => Object::Name(b"Courier".to_vec()),
            "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec())
        })),
    ];

    let existing_page_ids: Vec<ObjectId> = doc.page_iter().collect();
    let n_layout = overlays.len();
    let n_existing = existing_page_ids.len();

    for (i, overlay_bytes) in overlays.iter().enumerate() {
        if i < n_existing {
            write_page_content(&mut doc, existing_page_ids[i], overlay_bytes, &font_ids)?;
        } else {
            let lp = &layout.pages[i];
            add_new_page(&mut doc, lp.width, lp.height, overlay_bytes, &font_ids)?;
        }
    }

    if n_layout < n_existing {
        for &page_id in &existing_page_ids[n_layout..n_existing] {
            write_page_content(&mut doc, page_id, &[], &font_ids)?;
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

        // Count text-drawing operators (Tj/TJ) across all non-placeholder
        // content streams for this page. A real pre-rendered form page has
        // dozens of text operators; a watermark or evaluation overlay has
        // only 1–3. We require ≥5 non-placeholder text operators to
        // consider the page as having substantial static content.
        let mut text_op_count = 0usize;
        for stream in &streams {
            if is_xfa_placeholder_stream(stream) || is_watermark_stream(stream) {
                continue;
            }
            text_op_count += count_text_operators(stream);
        }

        if text_op_count >= 5 {
            return true;
        }
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

/// Count text-drawing operators (Tj / TJ) in a content stream.
fn count_text_operators(stream: &[u8]) -> usize {
    let mut count = 0;
    for window in stream.windows(3) {
        if (window[0] == b' ' || window[0] == b')' || window[0] == b']')
            && window[1] == b'T'
            && (window[2] == b'j' || window[2] == b'J')
        {
            count += 1;
        }
    }
    count
}

fn is_xfa_placeholder_stream(stream: &[u8]) -> bool {
    const PLACEHOLDER_MARKERS: [&[u8]; 5] = [
        b"Please wait",
        b"Adobe Reader",
        b"reader_download",
        b"display this type of document",
        b"To view the full contents",
    ];

    PLACEHOLDER_MARKERS
        .iter()
        .any(|marker| contains_ascii_case_insensitive(stream, marker))
}

/// Detect evaluation-software watermark overlays (e.g. "Qoppa Software",
/// "For Evaluation Only"). These are short streams with ≤3 Tj operators
/// that should not count as real pre-rendered form content.
fn is_watermark_stream(stream: &[u8]) -> bool {
    const WATERMARK_MARKERS: [&[u8]; 3] = [
        b"Evaluation Only",
        b"Qoppa Software",
        b"For Evaluation",
    ];
    WATERMARK_MARKERS
        .iter()
        .any(|marker| contains_ascii_case_insensitive(stream, marker))
}

fn contains_ascii_case_insensitive(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

fn write_ops(buf: &mut Vec<u8>, args: std::fmt::Arguments<'_>) {
    use std::fmt::Write as _;

    let mut text = String::new();
    let _ = text.write_fmt(args);
    buf.extend_from_slice(text.as_bytes());
}

/// Flatten Widget annotation appearances onto their pages.
///
/// Hybrid XFA PDFs often already contain the correct visual representation in
/// widget `/AP` streams. Stripping those widgets outright drops borders, text,
/// checkboxes, and image buttons. This helper bakes the normal appearance onto
/// the page content and removes only the widgets that were successfully
/// flattened. Returns the number of widgets flattened.
fn flatten_widget_appearances(doc: &mut Document) -> usize {
    let page_ids: Vec<ObjectId> = doc.page_iter().collect();
    let mut flattened = 0usize;

    for page_id in page_ids {
        let annots = page_annotations(doc, page_id);
        if annots.is_empty() {
            continue;
        }

        let mut retained = Vec::new();
        let mut overlay_ops = Vec::new();

        for annot in annots {
            let Some(annot_id) = annot.as_reference().ok() else {
                retained.push(annot);
                continue;
            };

            let Ok(annot_dict) = doc.get_dictionary(annot_id).cloned() else {
                retained.push(annot);
                continue;
            };

            let is_widget = annot_dict
                .get(b"Subtype")
                .ok()
                .and_then(|obj| obj.as_name().ok())
                == Some(&b"Widget"[..]);
            if !is_widget {
                retained.push(annot);
                continue;
            }

            let Some(rect) = annotation_rect(&annot_dict) else {
                retained.push(Object::Reference(annot_id));
                continue;
            };
            let Some(ap_id) = resolve_widget_normal_appearance(doc, &annot_dict) else {
                retained.push(Object::Reference(annot_id));
                continue;
            };

            let xobject_name = format!("XfaAp{}", flattened);
            add_xobject_to_page_resources(doc, page_id, &xobject_name, ap_id);
            write_ops(
                &mut overlay_ops,
                format_args!(
                    "q 1 0 0 1 {:.3} {:.3} cm /{} Do Q\n",
                    rect[0], rect[1], xobject_name
                ),
            );
            flattened += 1;
        }

        if overlay_ops.is_empty() {
            continue;
        }

        append_to_page_content(doc, page_id, &overlay_ops);
        set_page_annotations(doc, page_id, retained);
    }

    flattened
}

fn page_annotations(doc: &Document, page_id: ObjectId) -> Vec<Object> {
    let Ok(page_dict) = doc.get_dictionary(page_id) else {
        return Vec::new();
    };

    match page_dict.get(b"Annots") {
        Ok(Object::Array(arr)) => arr.clone(),
        Ok(Object::Reference(id)) => doc
            .get_object(*id)
            .ok()
            .and_then(|obj| obj.as_array().ok().cloned())
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

fn set_page_annotations(doc: &mut Document, page_id: ObjectId, annots: Vec<Object>) {
    if let Ok(Object::Dictionary(ref mut page_dict)) = doc.get_object_mut(page_id) {
        if annots.is_empty() {
            page_dict.remove(b"Annots");
        } else {
            page_dict.set("Annots", Object::Array(annots));
        }
    }
}

fn annotation_rect(dict: &Dictionary) -> Option<[f32; 4]> {
    let rect = dict.get(b"Rect").ok()?.as_array().ok()?;
    if rect.len() != 4 {
        return None;
    }
    Some([
        rect[0].as_float().ok()?,
        rect[1].as_float().ok()?,
        rect[2].as_float().ok()?,
        rect[3].as_float().ok()?,
    ])
}

fn resolve_widget_normal_appearance(
    doc: &mut Document,
    annot_dict: &Dictionary,
) -> Option<ObjectId> {
    let ap = annot_dict.get(b"AP").ok()?.as_dict().ok()?;
    let normal = ap.get(b"N").ok()?;
    resolve_appearance_object(doc, annot_dict, normal)
}

fn resolve_appearance_object(
    doc: &mut Document,
    annot_dict: &Dictionary,
    object: &Object,
) -> Option<ObjectId> {
    match object {
        Object::Reference(id) => match doc.get_object(*id).ok()?.clone() {
            Object::Stream(_) => Some(*id),
            Object::Dictionary(states) => resolve_appearance_state(doc, annot_dict, &states),
            _ => None,
        },
        Object::Stream(stream) => Some(doc.add_object(Object::Stream(stream.clone()))),
        Object::Dictionary(states) => resolve_appearance_state(doc, annot_dict, states),
        _ => None,
    }
}

fn resolve_appearance_state(
    doc: &mut Document,
    annot_dict: &Dictionary,
    states: &Dictionary,
) -> Option<ObjectId> {
    if let Some(state) = selected_widget_state(annot_dict) {
        if let Ok(object) = states.get(state) {
            if let Some(id) = resolve_appearance_object(doc, annot_dict, object) {
                return Some(id);
            }
        }
    }

    for fallback in [b"Yes".as_slice(), b"On".as_slice(), b"Off".as_slice()] {
        if let Ok(object) = states.get(fallback) {
            if let Some(id) = resolve_appearance_object(doc, annot_dict, object) {
                return Some(id);
            }
        }
    }

    for (_name, object) in states.iter() {
        if let Some(id) = resolve_appearance_object(doc, annot_dict, object) {
            return Some(id);
        }
    }

    None
}

fn selected_widget_state<'a>(annot_dict: &'a Dictionary) -> Option<&'a [u8]> {
    annot_dict
        .get(b"AS")
        .ok()
        .and_then(|obj| obj.as_name().ok())
        .or_else(|| annot_dict.get(b"V").ok().and_then(|obj| obj.as_name().ok()))
}

fn add_xobject_to_page_resources(
    doc: &mut Document,
    page_id: ObjectId,
    name: &str,
    xobject_id: ObjectId,
) {
    let resources_ref = doc.get_dictionary(page_id).ok().and_then(|page_dict| {
        page_dict
            .get(b"Resources")
            .ok()
            .and_then(|obj| obj.as_reference().ok())
    });

    if let Some(resources_id) = resources_ref {
        let xobject_ref = doc.get_dictionary(resources_id).ok().and_then(|resources| {
            resources
                .get(b"XObject")
                .ok()
                .and_then(|obj| obj.as_reference().ok())
        });

        if let Some(xobject_dict_id) = xobject_ref {
            if let Ok(Object::Dictionary(ref mut xobjects)) = doc.get_object_mut(xobject_dict_id) {
                xobjects.set(name, Object::Reference(xobject_id));
                return;
            }
        }

        if let Ok(Object::Dictionary(ref mut resources)) = doc.get_object_mut(resources_id) {
            add_xobject_to_resources_dict(resources, name, xobject_id);
            return;
        }
    }

    let inline_xobject_ref = doc.get_dictionary(page_id).ok().and_then(|page_dict| {
        page_dict
            .get(b"Resources")
            .ok()
            .and_then(|obj| obj.as_dict().ok())
            .and_then(|resources| {
                resources
                    .get(b"XObject")
                    .ok()
                    .and_then(|obj| obj.as_reference().ok())
            })
    });

    if let Some(xobject_dict_id) = inline_xobject_ref {
        if let Ok(Object::Dictionary(ref mut xobjects)) = doc.get_object_mut(xobject_dict_id) {
            xobjects.set(name, Object::Reference(xobject_id));
            return;
        }
    }

    if let Ok(Object::Dictionary(ref mut page_dict)) = doc.get_object_mut(page_id) {
        if let Ok(Object::Dictionary(ref mut resources)) = page_dict.get_mut(b"Resources") {
            add_xobject_to_resources_dict(resources, name, xobject_id);
            return;
        }

        let mut resources = Dictionary::new();
        add_xobject_to_resources_dict(&mut resources, name, xobject_id);
        page_dict.set("Resources", Object::Dictionary(resources));
    }
}

fn add_xobject_to_resources_dict(resources: &mut Dictionary, name: &str, xobject_id: ObjectId) {
    if let Ok(Object::Dictionary(ref mut xobjects)) = resources.get_mut(b"XObject") {
        xobjects.set(name, Object::Reference(xobject_id));
    } else {
        let mut xobjects = Dictionary::new();
        xobjects.set(name, Object::Reference(xobject_id));
        resources.set("XObject", Object::Dictionary(xobjects));
    }
}

fn append_to_page_content(doc: &mut Document, page_id: ObjectId, data: &[u8]) {
    let new_stream_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, data.to_vec())));

    let contents = doc
        .get_dictionary(page_id)
        .ok()
        .and_then(|page_dict| page_dict.get(b"Contents").ok().cloned());

    let new_contents = match contents {
        Some(Object::Reference(existing_id)) => Object::Array(vec![
            Object::Reference(existing_id),
            Object::Reference(new_stream_id),
        ]),
        Some(Object::Array(mut arr)) => {
            arr.push(Object::Reference(new_stream_id));
            Object::Array(arr)
        }
        Some(Object::Stream(stream)) => {
            let existing_id = doc.add_object(Object::Stream(stream));
            Object::Array(vec![
                Object::Reference(existing_id),
                Object::Reference(new_stream_id),
            ])
        }
        Some(other) => Object::Array(vec![other, Object::Reference(new_stream_id)]),
        None => Object::Reference(new_stream_id),
    };

    if let Ok(Object::Dictionary(ref mut page_dict)) = doc.get_object_mut(page_id) {
        page_dict.set("Contents", new_contents);
    }
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
    font_ids: &[ObjectId; 3],
) -> Result<()> {
    let resources = make_resources_dict(font_ids);

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
    font_ids: &[ObjectId; 3],
) -> Result<()> {
    let resources = make_resources_dict(font_ids);
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

fn make_resources_dict(font_ids: &[ObjectId; 3]) -> Dictionary {
    let mut fonts = Dictionary::new();
    fonts.set("F1", Object::Reference(font_ids[0])); // Times-Roman (serif)
    fonts.set("F2", Object::Reference(font_ids[1])); // Helvetica (sans-serif)
    fonts.set("F3", Object::Reference(font_ids[2])); // Courier (monospace)
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

    fn build_xfa_pdf_with_widget_appearance(
        page_content: Vec<u8>,
        normal_appearance: Object,
        widget_extra: Dictionary,
    ) -> Vec<u8> {
        use lopdf::{dictionary, Document, Object, Stream};

        let mut doc = Document::with_version("1.4");
        let xdp_bytes = SIMPLE_XDP.as_bytes().to_vec();
        let xfa_stream = Stream::new(
            dictionary! { "Length" => Object::Integer(xdp_bytes.len() as i64) },
            xdp_bytes,
        );
        let xfa_id = doc.add_object(Object::Stream(xfa_stream));

        let pages_id = doc.new_object_id();
        let content_id = doc.add_object(Object::Stream(Stream::new(
            dictionary! { "Length" => Object::Integer(page_content.len() as i64) },
            page_content,
        )));

        let appearance_id = match normal_appearance {
            Object::Reference(id) => id,
            other => doc.add_object(other),
        };

        let widget_id = doc.new_object_id();
        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type"     => Object::Name(b"Page".to_vec()),
            "Parent"   => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(612), Object::Integer(792),
            ]),
            "Contents" => Object::Reference(content_id),
            "Annots"   => Object::Array(vec![Object::Reference(widget_id)]),
            "Resources" => Object::Dictionary(dictionary! {})
        }));

        let mut widget = dictionary! {
            "Type"    => Object::Name(b"Annot".to_vec()),
            "Subtype" => Object::Name(b"Widget".to_vec()),
            "Rect"    => Object::Array(vec![
                Object::Integer(100), Object::Integer(700),
                Object::Integer(220), Object::Integer(730),
            ]),
            "AP"      => Object::Dictionary(dictionary! {
                "N" => Object::Reference(appearance_id)
            }),
            "P"       => Object::Reference(page_id)
        };
        for (key, value) in widget_extra {
            widget.set(key, value);
        }
        doc.objects.insert(widget_id, Object::Dictionary(widget));

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
            "Fields" => Object::Array(vec![Object::Reference(widget_id)])
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

    fn find_last_content_stream<'a>(doc: &'a Document, page_id: ObjectId) -> &'a Stream {
        let page_dict = doc.get_dictionary(page_id).expect("page dict");
        match page_dict.get(b"Contents").expect("contents") {
            Object::Reference(id) => doc
                .get_object(*id)
                .expect("contents object")
                .as_stream()
                .expect("contents stream"),
            Object::Array(arr) => {
                let last = arr.last().expect("last content stream");
                let id = last.as_reference().expect("contents ref");
                doc.get_object(id)
                    .expect("contents object")
                    .as_stream()
                    .expect("contents stream")
            }
            other => other.as_stream().expect("contents stream"),
        }
    }

    fn page_xobjects(doc: &Document, page_id: ObjectId) -> Dictionary {
        let page_dict = doc.get_dictionary(page_id).expect("page dict");
        let resources = page_dict
            .get(b"Resources")
            .expect("resources")
            .as_dict()
            .expect("resources dict");
        resources
            .get(b"XObject")
            .expect("xobjects")
            .as_dict()
            .expect("xobject dict")
            .clone()
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

    #[test]
    fn hybrid_static_pdf_flattens_widget_appearance_into_page_content() {
        let appearance = Object::Stream(Stream::new(
            dictionary! {
                "Type" => Object::Name(b"XObject".to_vec()),
                "Subtype" => Object::Name(b"Form".to_vec()),
                "BBox" => Object::Array(vec![
                    Object::Integer(0), Object::Integer(0),
                    Object::Integer(120), Object::Integer(30),
                ]),
                "Matrix" => Object::Array(vec![
                    Object::Integer(1), Object::Integer(0),
                    Object::Integer(0), Object::Integer(1),
                    Object::Integer(0), Object::Integer(0),
                ]),
                "Resources" => Object::Dictionary(dictionary! {}),
            },
            b"0 G\n0.5 0.5 119 29 re\ns\n".to_vec(),
        ));
        // Enough Tj operators (≥5) to exceed the static content threshold.
        let page_content = b"BT /F1 12 Tf 72 720 Td (Line 1) Tj 0 -14 Td (Line 2) Tj 0 -14 Td (Line 3) Tj 0 -14 Td (Line 4) Tj 0 -14 Td (Line 5) Tj ET\n".to_vec();
        let pdf_bytes = build_xfa_pdf_with_widget_appearance(
            page_content,
            appearance,
            dictionary! {
                "FT" => Object::Name(b"Tx".to_vec()),
                "T" => Object::string_literal("field[0]"),
            },
        );

        let result = flatten_xfa_to_pdf(&pdf_bytes).expect("flatten failed");
        let doc = Document::load_mem(&result).expect("load flattened PDF");
        let page_id = doc.page_iter().next().expect("page");
        let page_dict = doc.get_dictionary(page_id).expect("page dict");

        assert!(
            page_dict.get(b"Annots").is_err(),
            "flattened widgets should be removed from page annotations"
        );

        let stream = find_last_content_stream(&doc, page_id);
        let content = String::from_utf8_lossy(&stream.content);
        assert!(
            content.contains("Do"),
            "flattened page content should paint the widget appearance"
        );

        let xobjects = page_xobjects(&doc, page_id);
        assert_eq!(xobjects.len(), 1, "expected one widget appearance XObject");
    }

    #[test]
    fn hybrid_static_pdf_uses_selected_button_appearance_state() {
        let yes_stream = Object::Stream(Stream::new(
            dictionary! {
                "Type" => Object::Name(b"XObject".to_vec()),
                "Subtype" => Object::Name(b"Form".to_vec()),
                "BBox" => Object::Array(vec![
                    Object::Integer(0), Object::Integer(0),
                    Object::Integer(20), Object::Integer(20),
                ]),
                "Matrix" => Object::Array(vec![
                    Object::Integer(1), Object::Integer(0),
                    Object::Integer(0), Object::Integer(1),
                    Object::Integer(0), Object::Integer(0),
                ]),
                "Resources" => Object::Dictionary(dictionary! {}),
            },
            b"BT /F1 8 Tf 1 1 Td (YES) Tj ET\n".to_vec(),
        ));
        let off_stream = Object::Stream(Stream::new(
            dictionary! {
                "Type" => Object::Name(b"XObject".to_vec()),
                "Subtype" => Object::Name(b"Form".to_vec()),
                "BBox" => Object::Array(vec![
                    Object::Integer(0), Object::Integer(0),
                    Object::Integer(20), Object::Integer(20),
                ]),
                "Matrix" => Object::Array(vec![
                    Object::Integer(1), Object::Integer(0),
                    Object::Integer(0), Object::Integer(1),
                    Object::Integer(0), Object::Integer(0),
                ]),
                "Resources" => Object::Dictionary(dictionary! {}),
            },
            b"BT /F1 8 Tf 1 1 Td (OFF) Tj ET\n".to_vec(),
        ));

        let mut doc = Document::with_version("1.4");
        let state_id = doc.add_object(Object::Dictionary(dictionary! {
            "Yes" => yes_stream,
            "Off" => off_stream,
        }));
        let annot = dictionary! {
            "Subtype" => Object::Name(b"Widget".to_vec()),
            "Rect" => Object::Array(vec![
                Object::Integer(100), Object::Integer(700),
                Object::Integer(120), Object::Integer(720),
            ]),
            "AP" => Object::Dictionary(dictionary! {
                "N" => Object::Reference(state_id),
            }),
            "AS" => Object::Name(b"Yes".to_vec()),
            "FT" => Object::Name(b"Btn".to_vec()),
        };
        let ap_id =
            resolve_widget_normal_appearance(&mut doc, &annot).expect("selected normal appearance");
        let stream = doc
            .get_object(ap_id)
            .expect("appearance stream")
            .as_stream()
            .expect("appearance stream");
        let content = String::from_utf8_lossy(&stream.content);

        assert!(
            content.contains("YES"),
            "flatten should choose the selected normal appearance state"
        );
    }

    #[test]
    fn adding_widget_xobject_preserves_indirect_inline_page_xobjects() {
        let mut doc = Document::with_version("1.4");
        let existing_xobject_id = doc.add_object(Object::Stream(Stream::new(
            dictionary! {
                "Type" => Object::Name(b"XObject".to_vec()),
                "Subtype" => Object::Name(b"Form".to_vec()),
                "BBox" => Object::Array(vec![
                    Object::Integer(0), Object::Integer(0),
                    Object::Integer(10), Object::Integer(10),
                ]),
            },
            b"q Q\n".to_vec(),
        )));
        let xobject_dict_id = doc.add_object(Object::Dictionary(dictionary! {
            "R11" => Object::Reference(existing_xobject_id),
        }));

        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(612), Object::Integer(792),
            ]),
            "Resources" => Object::Dictionary(dictionary! {
                "XObject" => Object::Reference(xobject_dict_id),
            }),
        }));
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type"  => Object::Name(b"Pages".to_vec()),
                "Kids"  => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => Object::Integer(1)
            }),
        );

        let new_xobject_id = doc.add_object(Object::Stream(Stream::new(
            dictionary! {
                "Type" => Object::Name(b"XObject".to_vec()),
                "Subtype" => Object::Name(b"Form".to_vec()),
                "BBox" => Object::Array(vec![
                    Object::Integer(0), Object::Integer(0),
                    Object::Integer(10), Object::Integer(10),
                ]),
            },
            b"0 0 10 10 re S\n".to_vec(),
        )));

        add_xobject_to_page_resources(&mut doc, page_id, "XfaAp0", new_xobject_id);

        let xobjects = doc
            .get_object(xobject_dict_id)
            .expect("xobject dict")
            .as_dict()
            .expect("xobject dict");
        assert!(
            xobjects.get(b"R11").is_ok(),
            "existing page XObject was lost"
        );
        assert!(
            xobjects.get(b"XfaAp0").is_ok(),
            "new flattened widget XObject was not added"
        );
    }

    #[test]
    fn encrypted_pdf_returns_encrypted_error() {
        // Build a minimal PDF with an /Encrypt dictionary in the trailer.
        let mut doc = Document::with_version("1.4");
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type"     => Object::Name(b"Page".to_vec()),
            "Parent"   => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(612), Object::Integer(792),
            ]),
        }));
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type"  => Object::Name(b"Pages".to_vec()),
                "Kids"  => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => Object::Integer(1),
            }),
        );
        let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type"  => Object::Name(b"Catalog".to_vec()),
            "Pages" => Object::Reference(pages_id),
        }));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        // Add a dummy /Encrypt entry to simulate an encrypted PDF.
        let encrypt_id = doc.add_object(Object::Dictionary(dictionary! {
            "Filter" => Object::Name(b"Standard".to_vec()),
            "V"      => Object::Integer(2),
            "Length"  => Object::Integer(128),
        }));
        doc.trailer.set("Encrypt", Object::Reference(encrypt_id));

        let mut buf = Vec::new();
        doc.save_to(&mut buf).expect("save test PDF");

        let result = flatten_xfa_to_pdf(&buf);
        assert!(result.is_err(), "expected Encrypted error");
        let err = result.unwrap_err();
        assert!(
            matches!(err, XfaError::Encrypted(_)),
            "expected XfaError::Encrypted, got: {err:?}"
        );
    }

    #[test]
    fn owner_only_encrypted_pdf_is_handled_transparently() {
        // Owner-only encrypted PDFs (empty user password) are auto-decrypted by lopdf.
        // Verify that flatten_xfa_to_pdf processes them without error.
        let mut doc = Document::with_version("2.0");
        let pages_id = doc.new_object_id();
        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type"     => Object::Name(b"Page".to_vec()),
            "Parent"   => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(612), Object::Integer(792),
            ]),
        }));
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type"  => Object::Name(b"Pages".to_vec()),
                "Kids"  => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => Object::Integer(1),
            }),
        );
        let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type"  => Object::Name(b"Catalog".to_vec()),
            "Pages" => Object::Reference(pages_id),
        }));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        // Encrypt with owner password "secret", empty user password.
        let state = lopdf::aes256_encryption_state("secret", "", lopdf::Permissions::default())
            .expect("create encryption state");
        doc.encrypt(&state).expect("encrypt document");

        let mut buf = Vec::new();
        doc.save_to(&mut buf).expect("save encrypted PDF");

        // lopdf auto-decrypts owner-only encrypted PDFs, so is_pdf_encrypted returns false.
        assert!(!is_pdf_encrypted(&buf), "lopdf should auto-decrypt owner-only PDFs");

        // flatten_xfa_to_pdf should succeed — no XFA content, returns input as-is.
        let result = flatten_xfa_to_pdf(&buf);
        assert!(result.is_ok(), "owner-only encrypted PDF should be handled, got: {result:?}");
    }
}
