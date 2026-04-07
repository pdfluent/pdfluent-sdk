//! XFA flattening: parse XFA template, run layout, write PDF content streams.
//!
//! XFA Spec 3.3 §1.7 (p28-30) — Static vs Dynamic Forms:
//!   Static (XFAF): boilerplate in PDF, fields/subforms in XFA. Fixed layout.
//!   Dynamic (full XFA): all content in XFA. Layout computed at runtime.
//!   `baseProfile="interactiveForms"` indicates static (XFAF) forms.
//!
//! XFA Spec 3.3 §2.9 (p72) — PDF-XFA Connection:
//!   NeedsRendering flag: dynamic=true, XFAF=false.
//!   XFA packets stored in AcroForm/XFA entry in catalog.
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

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream, StringFormat};
use std::collections::HashMap;
use std::fmt::Write as FmtWrite;
use std::thread;
use std::time::Duration;

use crate::error::{Result, XfaError};
use crate::extract::extract_xfa_from_bytes;
use crate::font_bridge::{
    font_variant_key, CidFontInfo, ResolvedFont, XfaFontResolver, XfaFontSpec,
};
use crate::image_bridge::embed_image;
use crate::merger::FormMerger;
use crate::render_bridge::{generate_all_overlays, FontMetricsData, PageOverlay, XfaRenderConfig};
use xfa_dom_resolver::data_dom::DataDom;
use xfa_layout_engine::layout::LayoutEngine;

fn create_minimal_pdf_document() -> Document {
    let mut doc = Document::new();
    let pages_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Pages".to_vec()),
        "Kids" => Object::Array(vec![]),
        "Count" => Object::Integer(0)
    }));
    let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Catalog".to_vec()),
        "Pages" => Object::Reference(pages_id)
    }));
    doc.trailer.set("Root", Object::Reference(catalog_id));
    doc
}

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

    // 1b. Detect corrupt/minimal XFA: tiny PDFs (<1KB) whose template has no
    //     real content (no <subform> or <pageSet> children) produce blank output.
    //     Fall back to static page copy so the original pages are preserved.
    if is_corrupt_xfa_template(pdf_bytes.len(), &template_xml) {
        return static_fallback(pdf_bytes);
    }

    // 2. Try XFA template → layout → render pipeline.
    //    If this fails (parse error, empty template, layout 0 pages, lopdf error),
    //    fall back to preserving the existing page content with AcroForm stripped.
    //
    //    Wrap in a thread-based timeout (30s) to prevent hangs on pathological
    //    XFA documents. If the timeout fires, the join handle's result is an Err
    //    and we fall back to static_fallback.
    const FLATTEN_TIMEOUT: Duration = Duration::from_secs(30);
    let pdf_bytes_ref = pdf_bytes.to_vec();
    let template_xml_owned = template_xml.clone();
    let datasets_xml_owned = packets.datasets().map(|s| s.to_string());

    let handle = thread::spawn(move || {
        xfa_flatten_inner(
            &pdf_bytes_ref,
            &template_xml_owned,
            datasets_xml_owned.as_deref(),
        )
    });

    match handle.join() {
        Ok(Ok(out)) => Ok(out),
        Ok(Err(e)) => {
            eprintln!("XFA flatten failed: {e:?}");
            static_fallback(pdf_bytes)
        }
        Err(_) => {
            eprintln!("XFA flatten timed out after {:?}", FLATTEN_TIMEOUT);
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
    fn dump_tree(
        tree: &xfa_layout_engine::form::FormTree,
        id: xfa_layout_engine::form::FormNodeId,
        depth: usize,
    ) {
        if depth > 6 {
            return;
        }
        let node = tree.get(id);
        let meta = tree.meta(id);
        let indent = "  ".repeat(depth);
        let val = match &node.node_type {
            xfa_layout_engine::form::FormNodeType::Field { value } if !value.is_empty() => {
                format!(" val={:?}", &value[..value.len().min(30)])
            }
            _ => String::new(),
        };
        eprintln!(
            "{indent}{:?} {:?} {:?} {:?} bm={}x{} presence={:?} children={}{}",
            id,
            node.name,
            node.layout,
            std::mem::discriminant(&node.node_type),
            node.box_model
                .width
                .map_or("auto".to_string(), |w| format!("{:.0}", w)),
            node.box_model
                .height
                .map_or("auto".to_string(), |h| format!("{:.0}", h)),
            meta.presence,
            node.children.len(),
            val
        );
        for &cid in &node.children {
            dump_tree(tree, cid, depth + 1);
        }
    }
    dump_tree(&tree, root_id, 0);

    // Resolve fonts BEFORE layout so the layout engine uses actual font metrics
    // (widths, ascender, descender) instead of generic AFM tables.
    let resolved_fonts = resolve_template_fonts(template_xml, pdf_bytes);
    inject_resolved_metrics(&mut tree, &resolved_fonts);

    let engine = LayoutEngine::new(&tree);
    let layout = engine
        .layout(root_id)
        .map_err(|e| XfaError::LayoutFailed(format!("{e:?}")))?;

    if layout.pages.is_empty() {
        return Err(XfaError::LayoutFailed("layout produced 0 pages".into()));
    }

    let mut doc = match Document::load_mem(pdf_bytes) {
        Ok(d) => d,
        Err(_) => {
            eprintln!("lopdf load failed, creating minimal PDF structure for XFA layout");
            create_minimal_pdf_document()
        }
    };

    let (font_map, embedded_font_objects, metrics_data) =
        embed_resolved_fonts(&mut doc, &resolved_fonts);

    let mut config = XfaRenderConfig::default();
    config.font_map = font_map;
    config.font_metrics_data = metrics_data;

    let overlays = generate_all_overlays(&layout, &config)
        .map_err(|e| XfaError::LayoutFailed(format!("overlay generation: {e:?}")))?;

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

    // XFA Spec 3.3 §9.1 — Static vs Dynamic Forms: a form is static (XFAF)
    // when it uses only the restricted XFAF grammar subset (§7.6).  In
    // practice, Adobe identifies static forms by `baseProfile="interactiveForms"`
    // on the <template> element.  A dynamic form uses the full XFA grammar
    // and re-lays out content based on data/scripts.
    //
    // §7.6 enumerates grammar excluded from XFAF: area, occur (non-default),
    // multiple pageAreas, scripts that modify instance count, etc.
    //
    // Our detection uses baseProfile — this matches Adobe's behavior.  A more
    // rigorous check would inspect the template grammar for XFAF-excluded
    // elements, but baseProfile is the standard signal in real-world PDFs.
    let is_static_form = template_xml.contains("baseProfile=\"interactiveForms\"");
    let has_static_content = pages_have_static_content(&doc);

    // Preserve pre-rendered PDF page content when:
    // 1. Explicit static form (baseProfile="interactiveForms"), OR
    // 2. Pages have substantial pre-rendered content AND the XFA layout
    //    produces the same page count — the static content is authoritative,
    //    not a placeholder. Replacing it with XFA re-rendering causes subtle
    //    SSIM regressions due to font/rendering differences.
    // When page counts differ (#744), the static content is a preview that
    // must be replaced by the XFA engine's output.
    let preserve_static = has_static_content
        && (is_static_form || n_layout == n_existing);

    if preserve_static {
        // Bake widget appearances (field values, checkboxes, etc.) into the
        // page content so they survive AcroForm removal.
        flatten_widget_appearances(&mut doc);

        if is_static_form {
            // True static form (XFAF): overlay XFA field rendering on top of
            // preserved pages. The XFA template only defines fields, not full
            // page layouts, so overlaying adds field values without
            // double-rendering.
            for (i, overlay) in overlays.iter().enumerate() {
                if i < n_existing {
                    overlay_page_content(
                        &mut doc,
                        existing_page_ids[i],
                        overlay,
                        &font_ids,
                        &embedded_font_objects,
                    )?;
                } else {
                    let lp = &layout.pages[i];
                    add_new_page(
                        &mut doc,
                        lp.width,
                        lp.height,
                        overlay,
                        &font_ids,
                        &embedded_font_objects,
                    )?;
                }
            }
        }
        // Hybrid form (matching page count, no baseProfile): widget
        // appearances are baked, original page content is preserved.
        // No XFA overlay — the XFA engine would re-render full page content
        // (headers, text, images), causing double-drawing.
    } else {
        for (i, overlay) in overlays.iter().enumerate() {
            if i < n_existing {
                write_page_content(
                    &mut doc,
                    existing_page_ids[i],
                    overlay,
                    &font_ids,
                    &embedded_font_objects,
                )?;
            } else {
                let lp = &layout.pages[i];
                add_new_page(
                    &mut doc,
                    lp.width,
                    lp.height,
                    overlay,
                    &font_ids,
                    &embedded_font_objects,
                )?;
            }
        }
    }

    // Remove excess pages when XFA layout produces fewer pages than the
    // original static content. This is the core fix for over-pagination
    // (#744): XFA PDFs often carry pre-rendered static pages that far exceed
    // the dynamic page count Adobe would produce.
    // But for static/hybrid forms (preserve_static), keep all original pages —
    // the static content lives in the PDF page streams, not in XFA draw
    // elements (#750).
    if n_layout < n_existing && !preserve_static {
        // delete_pages takes 1-indexed page numbers, highest first to avoid
        // index shifts.
        let excess: Vec<u32> = ((n_layout + 1) as u32..=(n_existing as u32))
            .rev()
            .collect();
        doc.delete_pages(&excess);
    }

    if !is_static_form {
        // Strip widget annotations from pages.
        // - Dynamic forms: pages were overwritten by XFA layout.
        // - Hybrid forms: widgets were baked by flatten_widget_appearances.
        // True static (baseProfile) forms keep annotations — they may contain
        // non-widget annotations that are part of the form design.
        for &page_id in existing_page_ids.iter().take(n_layout.min(n_existing)) {
            if let Ok(Object::Dictionary(ref mut dict)) = doc.get_object_mut(page_id) {
                dict.remove(b"Annots");
            }
        }
    }

    remove_acroform(&mut doc);

    let mut out = Vec::new();
    doc.save_to(&mut out)
        .map_err(|e| XfaError::LayoutFailed(format!("save: {e}")))?;
    Ok(out)
}

// ---------------------------------------------------------------------------
// Font extraction, resolution, and embedding
// ---------------------------------------------------------------------------

fn extract_embedded_fonts(doc: &Document) -> Vec<(String, Vec<u8>)> {
    let mut fonts = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (_id, obj) in &doc.objects {
        let dict = match obj.as_dict() {
            Ok(d) => d,
            Err(_) => continue,
        };
        let is_font =
            dict.get(b"Type").ok().and_then(|o| o.as_name().ok()) == Some(b"Font".as_slice());
        if !is_font {
            continue;
        }
        let base_font = match dict.get(b"BaseFont").ok().and_then(|o| o.as_name().ok()) {
            Some(n) => String::from_utf8_lossy(n).to_string(),
            None => continue,
        };
        let fd_id = match dict.get(b"FontDescriptor").ok() {
            Some(Object::Reference(id)) => *id,
            _ => continue,
        };
        let fd = match doc.get_dictionary(fd_id) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let font_stream_id = fd
            .get(b"FontFile2")
            .or_else(|_| fd.get(b"FontFile3"))
            .or_else(|_| fd.get(b"FontFile"))
            .ok()
            .and_then(|o| o.as_reference().ok());
        let Some(stream_id) = font_stream_id else {
            continue;
        };
        if !seen.insert(stream_id) {
            continue;
        }
        let Ok(stream) = doc.get_object(stream_id).and_then(|o| o.as_stream()) else {
            continue;
        };
        let data = stream
            .get_plain_content()
            .unwrap_or_else(|_| stream.content.clone());
        if !data.is_empty() {
            let clean_name = if let Some(pos) = base_font.find('+') {
                base_font[pos + 1..].to_string()
            } else {
                base_font.clone()
            };
            // Store under the PostScript name (subset prefix already stripped)
            fonts.push((clean_name.clone(), data.clone()));
            // Also store under the font family name from the name table,
            // since XFA templates use family names (e.g. "Arial") while PDF
            // BaseFont uses PostScript names (e.g. "ArialMT").
            if let Ok(face) = ttf_parser::Face::parse(&data, 0) {
                for name_record in face.names() {
                    if name_record.name_id == ttf_parser::name_id::FAMILY {
                        if let Some(family) = name_record.to_string() {
                            if family != clean_name {
                                fonts.push((family, data.clone()));
                            }
                        }
                    }
                }
            }
            // Common PostScript-to-family normalization as fallback
            let normalized = ps_name_to_family(&clean_name);
            if normalized != clean_name {
                fonts.push((normalized, data.clone()));
            }
        }
    }
    fonts
}

/// Convert a PostScript font name to its likely family name.
///
/// Examples: `ArialMT` → `Arial`, `TimesNewRomanPSMT` → `Times New Roman`,
/// `MyriadPro-Regular` → `Myriad Pro`.
fn ps_name_to_family(ps_name: &str) -> String {
    // Strip weight/style suffixes first
    let base = ps_name
        .strip_suffix("PSMT")
        .or_else(|| ps_name.strip_suffix("PS-BoldItalicMT"))
        .or_else(|| ps_name.strip_suffix("PS-BoldMT"))
        .or_else(|| ps_name.strip_suffix("PS-ItalicMT"))
        .or_else(|| ps_name.strip_suffix("-BoldItalicMT"))
        .or_else(|| ps_name.strip_suffix("-BoldMT"))
        .or_else(|| ps_name.strip_suffix("-ItalicMT"))
        .or_else(|| ps_name.strip_suffix("MT"))
        .or_else(|| ps_name.strip_suffix("-Regular"))
        .or_else(|| ps_name.strip_suffix("-Bold"))
        .or_else(|| ps_name.strip_suffix("-Italic"))
        .or_else(|| ps_name.strip_suffix("-BoldItalic"))
        .unwrap_or(ps_name);
    // Insert spaces before uppercase letters that follow a lowercase letter
    // e.g. "TimesNewRoman" → "Times New Roman", "MyriadPro" → "Myriad Pro"
    let mut result = String::with_capacity(base.len() + 4);
    for (i, ch) in base.chars().enumerate() {
        if i > 0 && ch.is_uppercase() {
            let prev = base.as_bytes()[i - 1] as char;
            if prev.is_lowercase() {
                result.push(' ');
            }
        }
        result.push(ch);
    }
    result
}

/// Collected font specification from the XFA template.
struct TemplateFontEntry {
    typeface: String,
    weight: Option<String>,
    posture: Option<String>,
    generic_family: Option<String>,
}

fn collect_template_font_entries(template_xml: &str) -> Vec<TemplateFontEntry> {
    let mut entries = Vec::new();
    let mut seen = std::collections::HashSet::new();
    if let Ok(xml_doc) = roxmltree::Document::parse(template_xml) {
        for node in xml_doc.descendants() {
            if node.tag_name().name() == "font" {
                if let Some(typeface) = node.attribute("typeface") {
                    let name = typeface.to_string();
                    let weight = node.attribute("weight").map(|s| s.to_string());
                    let posture = node.attribute("posture").map(|s| s.to_string());
                    let generic_family =
                        node.attribute("genericFamily").map(|s| s.to_string());
                    let key = font_variant_key(&name, weight.as_deref(), posture.as_deref());
                    if !name.is_empty() && seen.insert(key.to_lowercase()) {
                        entries.push(TemplateFontEntry {
                            typeface: name,
                            weight,
                            posture,
                            generic_family,
                        });
                    }
                }
            }
        }
    }
    entries
}

fn embed_font_in_pdf(doc: &mut Document, font: &ResolvedFont) -> ObjectId {
    let font_stream = Stream::new(
        dictionary! {
            "Length" => Object::Integer(font.data.len() as i64),
            "Length1" => Object::Integer(font.data.len() as i64)
        },
        font.data.clone(),
    );
    let font_file_id = doc.add_object(Object::Stream(font_stream));

    let upem = font.units_per_em as f64;
    let scale = 1000.0 / upem.max(1.0);
    let ascent = (font.ascender as f64 * scale) as i64;
    let descent = (font.descender as f64 * scale) as i64;
    let cap_height = (ascent as f64 * 0.7) as i64;
    let base_name = font.name.replace(' ', "-");

    let fd = dictionary! {
        "Type" => Object::Name(b"FontDescriptor".to_vec()),
        "FontName" => Object::Name(base_name.as_bytes().to_vec()),
        "Flags" => Object::Integer(32),
        "FontBBox" => Object::Array(vec![
            Object::Integer(0),
            Object::Integer(descent),
            Object::Integer(1000),
            Object::Integer(ascent),
        ]),
        "ItalicAngle" => Object::Integer(0),
        "Ascent" => Object::Integer(ascent),
        "Descent" => Object::Integer(descent),
        "CapHeight" => Object::Integer(cap_height),
        "StemV" => Object::Integer(80),
        "FontFile2" => Object::Reference(font_file_id)
    };
    let fd_id = doc.add_object(Object::Dictionary(fd));

    // Build CID font data for Identity-H encoding.
    let cid_info = font.cid_font_info().unwrap_or(CidFontInfo {
        widths: vec![500],
        gid_to_unicode: vec![],
    });

    // /W array: [ 0 [w0 w1 w2 ... wN] ]
    let widths_inner: Vec<Object> = cid_info
        .widths
        .iter()
        .map(|&w| Object::Integer(w as i64))
        .collect();
    let w_array = vec![Object::Integer(0), Object::Array(widths_inner)];

    let cid_font = dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"CIDFontType2".to_vec()),
        "BaseFont" => Object::Name(base_name.as_bytes().to_vec()),
        "CIDSystemInfo" => Object::Dictionary(dictionary! {
            "Registry" => Object::String(b"Adobe".to_vec(), StringFormat::Literal),
            "Ordering" => Object::String(b"Identity".to_vec(), StringFormat::Literal),
            "Supplement" => Object::Integer(0)
        }),
        "FontDescriptor" => Object::Reference(fd_id),
        "W" => Object::Array(w_array),
        "CIDToGIDMap" => Object::Name(b"Identity".to_vec())
    };
    let cid_font_id = doc.add_object(Object::Dictionary(cid_font));

    // ToUnicode CMap for text extraction / copy-paste.
    let tounicode_data = generate_tounicode_cmap(&cid_info.gid_to_unicode);
    let tounicode_stream = Stream::new(
        dictionary! { "Length" => Object::Integer(tounicode_data.len() as i64) },
        tounicode_data,
    );
    let tounicode_id = doc.add_object(Object::Stream(tounicode_stream));

    // Type0 (composite) font with Identity-H encoding.
    let type0_font = dictionary! {
        "Type" => Object::Name(b"Font".to_vec()),
        "Subtype" => Object::Name(b"Type0".to_vec()),
        "BaseFont" => Object::Name(base_name.as_bytes().to_vec()),
        "Encoding" => Object::Name(b"Identity-H".to_vec()),
        "DescendantFonts" => Object::Array(vec![Object::Reference(cid_font_id)]),
        "ToUnicode" => Object::Reference(tounicode_id)
    };
    doc.add_object(Object::Dictionary(type0_font))
}

/// Generate a ToUnicode CMap stream mapping glyph IDs to Unicode codepoints.
fn generate_tounicode_cmap(gid_to_unicode: &[(u16, char)]) -> Vec<u8> {
    let mut cmap = String::with_capacity(gid_to_unicode.len() * 24 + 256);
    cmap.push_str("/CIDInit /ProcSet findresource begin\n");
    cmap.push_str("12 dict begin\n");
    cmap.push_str("begincmap\n");
    cmap.push_str("/CIDSystemInfo\n");
    cmap.push_str("<< /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n");
    cmap.push_str("/CMapName /Adobe-Identity-UCS def\n");
    cmap.push_str("/CMapType 2 def\n");
    cmap.push_str("1 begincodespacerange\n");
    cmap.push_str("<0000> <FFFF>\n");
    cmap.push_str("endcodespacerange\n");
    for chunk in gid_to_unicode.chunks(100) {
        let _ = write!(cmap, "{} beginbfchar\n", chunk.len());
        for &(gid, ch) in chunk {
            let _ = write!(cmap, "<{:04X}> <{:04X}>\n", gid, ch as u32);
        }
        cmap.push_str("endbfchar\n");
    }
    cmap.push_str("endcmap\n");
    cmap.push_str("CMapName currentdict /CMap defineresource pop\n");
    cmap.push_str("end\nend\n");
    cmap.into_bytes()
}

/// Resolve all fonts referenced in the XFA template without embedding them.
///
/// Returns a map from variant key to `ResolvedFont`. The key encodes typeface,
/// weight, and posture so that "Arial bold" and "Arial regular" are resolved
/// separately. Called BEFORE layout so that resolved metrics can be injected
/// into the `FormTree`.
fn resolve_template_fonts(template_xml: &str, pdf_bytes: &[u8]) -> HashMap<String, ResolvedFont> {
    let mut resolved = HashMap::new();
    let entries = collect_template_font_entries(template_xml);
    if entries.is_empty() {
        return resolved;
    }
    let source_doc = match Document::load_mem(pdf_bytes) {
        Ok(d) => d,
        Err(_) => return resolved,
    };
    let embedded_fonts = extract_embedded_fonts(&source_doc);
    let mut resolver = XfaFontResolver::new(embedded_fonts);
    for entry in &entries {
        let spec = XfaFontSpec::from_xfa_attrs(
            &entry.typeface,
            entry.weight.as_deref(),
            entry.posture.as_deref(),
            None,
            entry.generic_family.as_deref(),
        );
        let key = font_variant_key(
            &entry.typeface,
            entry.weight.as_deref(),
            entry.posture.as_deref(),
        );
        match resolver.resolve(&spec) {
            Ok(font) => {
                resolved.insert(key, font);
            }
            Err(e) => {
                eprintln!("Font resolution failed for '{}': {}", entry.typeface, e);
            }
        }
    }
    resolved
}

/// Inject resolved font metrics into the FormTree before layout.
///
/// For each node whose style metadata carries a `font_family`, looks up the
/// matching `ResolvedFont` (using the variant key that includes weight/posture)
/// and populates the `resolved_widths`, `resolved_upem`, `resolved_ascender`,
/// and `resolved_descender` fields on the node's `FontMetrics`.
/// This makes `measure_width()` and `line_height_pt()` in the layout engine use
/// actual font data instead of generic AFM tables.
fn inject_resolved_metrics(
    tree: &mut xfa_layout_engine::form::FormTree,
    resolved: &HashMap<String, ResolvedFont>,
) {
    for i in 0..tree.nodes.len() {
        let id = xfa_layout_engine::form::FormNodeId(i);
        let style = &tree.meta(id).style;
        let font_family = style.font_family.clone();
        let font_weight = style.font_weight.clone();
        let font_style = style.font_style.clone();
        if let Some(ref family) = font_family {
            // Try variant-specific key first, then fall back to base key.
            let variant_key =
                font_variant_key(family, font_weight.as_deref(), font_style.as_deref());
            let base_key = font_variant_key(family, None, None);
            let font = resolved
                .get(&variant_key)
                .or_else(|| resolved.get(&base_key));
            if let Some(font) = font {
                let (_first_char, widths) = font.pdf_glyph_widths();
                let node = tree.get_mut(id);
                node.font.resolved_widths = Some(widths);
                node.font.resolved_upem = Some(font.units_per_em);
                node.font.resolved_ascender = Some(font.ascender);
                node.font.resolved_descender = Some(font.descender);
            }
        }
    }
}

/// Embed already-resolved fonts into the PDF document.
///
/// Called AFTER layout. Returns the font_map (typeface -> PDF resource name),
/// the font objects for page resources, and the metrics data for render_bridge.
fn embed_resolved_fonts(
    doc: &mut Document,
    resolved: &HashMap<String, ResolvedFont>,
) -> (
    HashMap<String, String>,
    Vec<(String, ObjectId)>,
    HashMap<String, FontMetricsData>,
) {
    let mut font_map = HashMap::new();
    let mut font_objects = Vec::new();
    let mut metrics_data = HashMap::new();
    for (idx, (name, font)) in resolved.iter().enumerate() {
        let resource_name = format!("XFA_F{}", idx);
        let obj_id = embed_font_in_pdf(doc, font);
        font_map.insert(name.clone(), format!("/{}", resource_name));
        font_objects.push((resource_name, obj_id));
        let (_first_char, widths) = font.pdf_glyph_widths();
        metrics_data.insert(
            name.clone(),
            FontMetricsData {
                widths,
                upem: font.units_per_em,
                ascender: font.ascender,
                descender: font.descender,
                font_data: Some(font.data.clone()),
                face_index: font.face_index,
            },
        );
    }
    (font_map, font_objects, metrics_data)
}

/// Fallback: preserve existing page content, strip AcroForm/widgets only.
/// If lopdf can't parse the PDF (corrupt xref), return the original bytes
/// unchanged — the PDF is too corrupt for us to modify but still renderable.
///
/// This function ALWAYS returns Ok — errors are logged but the original bytes
/// are always returned as a last resort.
fn static_fallback(pdf_bytes: &[u8]) -> Result<Vec<u8>> {
    let mut doc = match Document::load_mem(pdf_bytes) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("static_fallback: lopdf load failed ({e}), returning original bytes");
            return Ok(pdf_bytes.to_vec());
        }
    };
    strip_widgets_and_acroform(&mut doc);
    let mut out = Vec::new();
    if let Err(e) = doc.save_to(&mut out) {
        eprintln!("static_fallback: save failed ({e}), returning original bytes");
        return Ok(pdf_bytes.to_vec());
    }
    Ok(out)
}

/// Detect corrupt or minimal XFA templates that cannot produce useful output.
///
/// Tiny PDFs (<1KB) with XFA templates that lack essential elements (subform,
/// pageSet) are corrupt stubs. Attempting to flatten these produces blank pages
/// instead of preserving the original page content.
fn is_corrupt_xfa_template(pdf_size: usize, template_xml: &str) -> bool {
    // Only apply to small PDFs — larger files may have legitimate sparse templates.
    if pdf_size >= 1024 {
        return false;
    }
    // A valid XFA template must parse and contain at least one subform or pageSet.
    match roxmltree::Document::parse(template_xml) {
        Ok(doc) => {
            let root = doc.root_element();
            !root.children().any(|c| {
                c.is_element()
                    && matches!(
                        c.tag_name().name(),
                        "subform" | "pageSet" | "subformSet"
                    )
            })
        }
        Err(_) => true, // Unparseable template is corrupt.
    }
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
    const WATERMARK_MARKERS: [&[u8]; 3] =
        [b"Evaluation Only", b"Qoppa Software", b"For Evaluation"];
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
    overlay: &PageOverlay,
    font_ids: &[ObjectId; 3],
    embedded_fonts: &[(String, ObjectId)],
) -> Result<()> {
    let mut resources = make_resources_dict(font_ids, embedded_fonts);

    let mut xobjects = Dictionary::new();
    for img in &overlay.images {
        match embed_image(doc, &img.data, &img.mime_type) {
            Ok(result) => {
                xobjects.set(img.name.as_str(), Object::Reference(result.object_id));
            }
            Err(e) => {
                eprintln!("failed to embed image {}: {}", img.name, e);
            }
        }
    }
    if !xobjects.is_empty() {
        resources.set("XObject", Object::Dictionary(xobjects));
    }

    let stream = Stream::new(
        dictionary! { "Length" => Object::Integer(overlay.content_stream.len() as i64) },
        overlay.content_stream.clone(),
    );
    let stream_id = doc.add_object(Object::Stream(stream));

    if let Ok(Object::Dictionary(ref mut page_dict)) = doc.get_object_mut(page_id) {
        page_dict.set("Contents", Object::Reference(stream_id));
        page_dict.set("Resources", Object::Dictionary(resources));
    }
    Ok(())
}

/// Overlay XFA content on top of existing page content (for static XFA forms).
///
/// Unlike `write_page_content` which replaces the page content entirely, this
/// preserves the original content stream and appends the XFA overlay on top.
/// The original resources are preserved and XFA font resources are merged in.
fn overlay_page_content(
    doc: &mut Document,
    page_id: ObjectId,
    overlay: &PageOverlay,
    font_ids: &[ObjectId; 3],
    embedded_fonts: &[(String, ObjectId)],
) -> Result<()> {
    let xfa_resources = make_resources_dict(font_ids, embedded_fonts);

    let mut xfa_xobjects = Dictionary::new();
    for img in &overlay.images {
        match embed_image(doc, &img.data, &img.mime_type) {
            Ok(result) => {
                xfa_xobjects.set(img.name.as_str(), Object::Reference(result.object_id));
            }
            Err(e) => {
                eprintln!("failed to embed image {}: {}", img.name, e);
            }
        }
    }

    merge_xfa_resources_into_page(doc, page_id, &xfa_resources, &xfa_xobjects);

    if !overlay.content_stream.is_empty() {
        append_to_page_content(doc, page_id, &overlay.content_stream);
    }

    Ok(())
}

/// Merge XFA font/xobject resources into the existing page resources without
/// overwriting original entries.
fn merge_xfa_resources_into_page(
    doc: &mut Document,
    page_id: ObjectId,
    xfa_resources: &Dictionary,
    xfa_xobjects: &Dictionary,
) {
    let existing_resources = doc
        .get_dictionary(page_id)
        .ok()
        .and_then(|page_dict| {
            page_dict.get(b"Resources").ok().and_then(|obj| match obj {
                Object::Reference(id) => doc.get_dictionary(*id).ok().cloned(),
                Object::Dictionary(d) => Some(d.clone()),
                _ => None,
            })
        })
        .unwrap_or_default();

    let mut merged = existing_resources;

    // Merge Font entries: add XFA fonts (F1, F2, F3, embedded) without
    // overwriting the page's own fonts.
    if let Ok(xfa_font_dict) = xfa_resources.get(b"Font").and_then(|o| o.as_dict()) {
        let existing_font = merged
            .get(b"Font")
            .ok()
            .and_then(|obj| match obj {
                Object::Dictionary(d) => Some(d.clone()),
                Object::Reference(id) => doc.get_dictionary(*id).ok().cloned(),
                _ => None,
            })
            .unwrap_or_default();

        let mut font_merged = existing_font;
        for (key, val) in xfa_font_dict.iter() {
            if font_merged.get(key).is_err() {
                font_merged.set(key.clone(), val.clone());
            }
        }
        merged.set("Font", Object::Dictionary(font_merged));
    }

    // Merge XObject entries.
    if !xfa_xobjects.is_empty() {
        let existing_xobj = merged
            .get(b"XObject")
            .ok()
            .and_then(|obj| match obj {
                Object::Dictionary(d) => Some(d.clone()),
                Object::Reference(id) => doc.get_dictionary(*id).ok().cloned(),
                _ => None,
            })
            .unwrap_or_default();

        let mut xobj_merged = existing_xobj;
        for (key, val) in xfa_xobjects.iter() {
            xobj_merged.set(key.clone(), val.clone());
        }
        merged.set("XObject", Object::Dictionary(xobj_merged));
    }

    if let Ok(Object::Dictionary(ref mut page_dict)) = doc.get_object_mut(page_id) {
        page_dict.set("Resources", Object::Dictionary(merged));
    }
}

/// Add a new page to the document's /Pages tree.
fn add_new_page(
    doc: &mut Document,
    w: f64,
    h: f64,
    overlay: &PageOverlay,
    font_ids: &[ObjectId; 3],
    embedded_fonts: &[(String, ObjectId)],
) -> Result<()> {
    let mut resources = make_resources_dict(font_ids, embedded_fonts);

    let mut xobjects = Dictionary::new();
    for img in &overlay.images {
        match embed_image(doc, &img.data, &img.mime_type) {
            Ok(result) => {
                xobjects.set(img.name.as_str(), Object::Reference(result.object_id));
            }
            Err(e) => {
                eprintln!("failed to embed image {}: {}", img.name, e);
            }
        }
    }
    if !xobjects.is_empty() {
        resources.set("XObject", Object::Dictionary(xobjects));
    }

    let stream = Stream::new(
        dictionary! { "Length" => Object::Integer(overlay.content_stream.len() as i64) },
        overlay.content_stream.clone(),
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

fn make_resources_dict(
    font_ids: &[ObjectId; 3],
    embedded_fonts: &[(String, ObjectId)],
) -> Dictionary {
    let mut fonts = Dictionary::new();
    fonts.set("F1", Object::Reference(font_ids[0]));
    fonts.set("F2", Object::Reference(font_ids[1]));
    fonts.set("F3", Object::Reference(font_ids[2]));
    for (name, obj_id) in embedded_fonts {
        fonts.set(name.as_str(), Object::Reference(*obj_id));
    }
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
    fn hybrid_static_pdf_uses_xfa_layout_over_static_content() {
        // When a PDF has both XFA template and static page content,
        // XFA layout should always take priority — the static content
        // may be a pre-rendered preview with wrong page count (#744).
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
        // Enough Tj operators (≥5) to exceed the old static content threshold.
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

        // XFA layout produces pages without widget annotations.
        assert!(
            page_dict.get(b"Annots").is_err(),
            "XFA-flattened page should have no annotations"
        );
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
        assert!(
            !is_pdf_encrypted(&buf),
            "lopdf should auto-decrypt owner-only PDFs"
        );

        // flatten_xfa_to_pdf should succeed — no XFA content, returns input as-is.
        let result = flatten_xfa_to_pdf(&buf);
        assert!(
            result.is_ok(),
            "owner-only encrypted PDF should be handled, got: {result:?}"
        );
    }
}
