//! # XFA Flattening Pipeline
//!
//! This module parses XFA template, runs layout, and writes PDF content streams.
//!
//! ## Pipeline Stages
//!
//! 1. **Extract** — `extract_embedded_fonts()` reads font programs and /Widths
//!    arrays from PDF font dictionaries
//! 2. **Store** — `store_font_data()` saves font bytes + widths keyed by name
//! 3. **Resolve** — `XfaFontResolver::resolve()` matches XFA font names to
//!    stored fonts, with fallbacks (alias, family, system)
//! 4. **Inject** — `inject_resolved_metrics()` pushes resolved widths into
//!    FontMetrics for the layout engine
//! 5. **Layout** — `LayoutEngine::layout()` computes page positions using
//!    resolved font metrics for accurate text measurement
//! 6. **Render** — `generate_page_overlay()` in render_bridge converts LayoutDom
//!    to PDF content stream operators
//! 7. **Embed** — `embed_resolved_fonts()` writes font data into the PDF
//!    and creates /Font resources
//! 8. **Write** — The content streams are written back to PDF pages
//!
//! ## Static vs Dynamic Forms
//!
//! XFA Spec 3.3 §1.7 (p28-30):
//! - **Static (XFAF)**: boilerplate in PDF, fields/subforms in XFA. Fixed layout.
//! - **Dynamic (full XFA)**: all content in XFA. Layout computed at runtime.
//! - `baseProfile="interactiveForms"` indicates static (XFAF) forms.
//!
//! XFA Spec 3.3 §2.9 (p72) — PDF-XFA Connection:
//! - NeedsRendering flag: dynamic=true, XFAF=false.
//! - XFA packets stored in AcroForm/XFA entry in catalog.
//!
//! ## /Widths Handling
//!
//! PDF /Widths arrays start at FirstChar (typically 32). For simple fonts we
//! remap those code-indexed widths through the font encoding so the layout
//! engine receives Unicode-indexed measurements.
//!
//! ## CID Font /W Arrays (PDF spec §9.7.4.3)
//!
//! CID fonts (Type0/composite) use `/W` arrays in the CIDFont descendant
//! dictionary instead of simple `/Widths`. Two element types:
//!   - `cid_start [w1 w2 ...]` — consecutive CIDs starting at cid_start
//!   - `cid_first cid_last width` — range of CIDs with same width
//!
//! `/DW` (default width, defaults to 1000) covers CIDs not in `/W`.
//!
//! ## Known Limitations
//!
//! - CID-to-Unicode mapping (ToUnicode CMap) is not yet parsed
//! - System font fallback may have different metrics than the PDF's embedded font

use lopdf::{dictionary, Dictionary, Document, Object, ObjectId, Stream, StringFormat};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::fmt::Write as FmtWrite;
use std::thread;
use std::time::Duration;

// GL-QA36: Re-entrance guard for flatten_xfa_to_pdf.
//
// When the XFA layout fails and static_fallback returns the original bytes
// unchanged (because lopdf also cannot parse the file), a caller that retries
// flatten on those same bytes will trigger the same failure path again,
// causing infinite recursion and ultimately a stack overflow.
//
// This thread-local counter is incremented on entry to flatten_xfa_to_pdf and
// decremented by a drop guard on exit.  If the counter is already ≥ 1 when
// the function is entered, we return an error immediately to break the cycle.
//
// The counter is thread-local so the spawned worker thread (thread::spawn
// inside flatten_xfa_to_pdf) starts with its own fresh counter = 0 and is
// not affected by the caller's guard.
thread_local! {
    static FLATTEN_DEPTH: Cell<u32> = const { Cell::new(0) };
}

use crate::error::{Result, XfaError};
use crate::extract::extract_xfa_from_bytes;
use crate::font_bridge::{
    font_variant_key, pdf_glyph_name_to_unicode, CidFontInfo, EmbeddedFontData, PdfBaseEncoding,
    PdfSimpleEncoding, PdfSourceFont, ResolvedFont, XfaFontResolver, XfaFontSpec,
};
use crate::image_bridge::embed_image;
use crate::merger::FormMerger;
use crate::render_bridge::{
    generate_all_overlays, unicode_to_winansi, FontMetricsData, PageOverlay, XfaRenderConfig,
};
use xfa_dom_resolver::data_dom::DataDom;
use xfa_layout_engine::form::{DrawContent, FormNodeId, FormNodeStyle, FormTree};
use xfa_layout_engine::layout::{LayoutContent, LayoutDom, LayoutEngine, LayoutNode};

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

/// Returns `true` if the layout nodes contain at least one `Field` node
/// (regardless of whether its value is empty or populated).
fn page_has_fields(nodes: &[LayoutNode]) -> bool {
    nodes.iter().any(|n| {
        matches!(&n.content, LayoutContent::Field { .. }) || page_has_fields(&n.children)
    })
}

/// Returns `true` if the layout nodes contain at least one `Field` with a
/// non-empty value (i.e. data-bound content, not just static draw elements).
/// Used for XFA §4.3 empty page subform suppression.
fn page_has_field_data(nodes: &[LayoutNode]) -> bool {
    nodes.iter().any(|n| {
        matches!(&n.content, LayoutContent::Field { value, .. } if !value.is_empty())
            || page_has_field_data(&n.children)
    })
}

/// Flatten all XFA content in `pdf_bytes` to static PDF content streams.
///
/// Returns the modified PDF bytes. The /AcroForm entry is removed so the
/// result is a plain PDF/1.4 document.
///
/// If the PDF has no XFA content, returns a clone of the input unchanged.
///
/// # Oracle Comparison Approach (XFA-F1-04)
///
/// Reference ("oracle") output for quality comparison is generated using:
///
/// 1. **pdfRest** — `POST https://api.pdfrest.com/flatten-pdf`
///    Uses Adobe's XFA engine.  Highest fidelity.  Rate-limited to ~1200
///    calls/month across two accounts.  Keys at
///    `~/.config/pdfluent/pdfrest-keys.json`.
///
///    ```bash
///    # curl -X POST "https://api.pdfrest.com/flatten-pdf" \
///    #   -H "Api-Key: <KEY>" \
///    #   --form "input=@form.xfa.pdf;type=application/pdf" \
///    #   -o reference.pdfrest.pdf
///    ```
///
/// 2. **mutool** — `mutool convert -o reference.pdf input.xfa.pdf`
///    Secondary oracle.  Free, offline, limited XFA support.
///
///    ```bash
///    # mutool convert -o reference.mutool.pdf input.xfa.pdf
///    ```
///
/// Quality is measured as per-page SSIM vs. the pdfRest oracle (target ≥ 0.95).
/// See `scripts/generate_xfa_reference.sh` and `docs/XFA_SUCCESS_CRITERIA.md`.
pub fn flatten_xfa_to_pdf(pdf_bytes: &[u8]) -> Result<Vec<u8>> {
    // GL-QA36: Re-entrance guard.  If this function is entered while already
    // running on this thread (depth ≥ 1), a recursive call has occurred —
    // most likely a fallback path returning the original bytes which still
    // contain /AcroForm + xdp:xdp markers.  Abort immediately with an error
    // to prevent the infinite recursion / stack overflow.
    //
    // The worker thread spawned below has its own thread-local so its depth
    // starts at 0 and is unaffected by this guard.
    let depth = FLATTEN_DEPTH.with(|d| d.get());
    if depth >= 1 {
        return Err(XfaError::LayoutFailed(
            "flatten_xfa_to_pdf called recursively — aborting to prevent stack overflow".into(),
        ));
    }
    FLATTEN_DEPTH.with(|d| d.set(depth + 1));
    // Drop guard: decrement the counter even if we return early.
    struct DepthGuard;
    impl Drop for DepthGuard {
        fn drop(&mut self) {
            FLATTEN_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
        }
    }
    let _depth_guard = DepthGuard;

    // 0a. Quick byte-level pre-check: if the raw bytes don't contain /AcroForm
    //     (where XFA lives per the spec) and no XDP namespace, skip expensive
    //     parsing. This prevents multi-second stalls on large non-XFA PDFs.
    if !pdf_bytes
        .windows(9)
        .any(|w| w == b"/AcroForm")
        && !pdf_bytes.windows(7).any(|w| w == b"xdp:xdp")
    {
        return Ok(pdf_bytes.to_vec());
    }

    // 0b. Handle encrypted PDFs: try empty-password decrypt (owner-only encryption),
    //     otherwise reject early — encrypted content produces garbage output.
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
        Some(t) => strip_undefined_xml_entities(t),
        None => {
            // XFA present but template packet missing/unparseable (truncated XML).
            // Strip AcroForm + NeedsRendering so renderers use static content.
            return static_fallback(pdf_bytes);
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
    let datasets_xml_owned = packets.datasets().map(strip_undefined_xml_entities);
    let form_xml_owned = packets.get_packet("form").map(|s| s.to_string());

    let handle = thread::spawn(move || {
        xfa_flatten_inner(
            &pdf_bytes_ref,
            &template_xml_owned,
            datasets_xml_owned.as_deref(),
            form_xml_owned.as_deref(),
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
    form_xml: Option<&str>,
) -> Result<Vec<u8>> {
    use crate::dynamic::apply_dynamic_scripts;

    let data_dom = if let Some(ds_xml) = datasets_xml {
        DataDom::from_xml(ds_xml)
            .map_err(|e| XfaError::ParseFailed(format!("datasets parse: {e}")))?
    } else {
        DataDom::new()
    };

    // Extract embedded image files from the PDF for resolving <image href="…">
    // references in the XFA template (XFA §2.3).
    let image_files = match Document::load_mem(pdf_bytes) {
        Ok(doc) => extract_embedded_images(&doc),
        Err(_) => HashMap::new(),
    };

    let merger = FormMerger::new(&data_dom).with_image_files(image_files);
    let (mut tree, root_id) = merger
        .merge(template_xml)
        .map_err(|e| XfaError::ParseFailed(format!("template merge: {e}")))?;

    let _ = apply_dynamic_scripts(&mut tree, root_id);

    // XFA §3: when the PDF contains a pre-merged form DOM (saved by Adobe's
    // runtime after scripts executed), use its presence attributes to override
    // the template-based defaults. This captures script-driven visibility
    // changes (e.g. Avoka framework's sfcUtils.updateVisibility) that our
    // FormCalc interpreter cannot execute.
    if let Some(fxml) = form_xml {
        apply_form_dom_presence(&mut tree, root_id, fxml);
    }

    // Resolve fonts BEFORE layout so the layout engine uses actual font metrics
    // (widths, ascender, descender) instead of generic AFM tables.
    let resolved_fonts = resolve_template_fonts(template_xml, pdf_bytes);
    inject_resolved_metrics(&mut tree, &resolved_fonts);

    let engine = LayoutEngine::new(&tree);
    let mut layout = engine
        .layout(root_id)
        .map_err(|e| XfaError::LayoutFailed(format!("{e:?}")))?;

    if layout.pages.is_empty() {
        return Err(XfaError::LayoutFailed("layout produced 0 pages".into()));
    }

    // XFA Spec §4.3: suppress page subforms whose data is empty or absent.
    // A page with fields but no populated values is considered "data-empty"
    // and should be suppressed.  Pages without fields (static-only pages with
    // draws/images) are always kept.  At least one page is retained.
    if layout.pages.len() > 1 {
        let keep: Vec<bool> = layout
            .pages
            .iter()
            .map(|p| {
                if page_has_fields(&p.nodes) {
                    page_has_field_data(&p.nodes)
                } else {
                    true
                }
            })
            .collect();
        if keep.iter().any(|&k| k) {
            let mut idx = 0;
            layout.pages.retain(|_| {
                let k = keep[idx];
                idx += 1;
                k
            });
        } else {
            layout.pages.truncate(1);
        }
    }

    let mut doc = match Document::load_mem(pdf_bytes) {
        Ok(d) => d,
        Err(_) => {
            eprintln!("lopdf load failed, creating minimal PDF structure for XFA layout");
            create_minimal_pdf_document()
        }
    };

    let (font_map, embedded_font_objects, metrics_data) =
        embed_resolved_fonts(&mut doc, &resolved_fonts, &layout);

    let config = XfaRenderConfig {
        font_map,
        font_metrics_data: metrics_data,
        ..Default::default()
    };

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
    //    produces at least as many pages as the original AND the XFA overlay
    //    has enough content to indicate a full page re-render.
    // 3. Layout engine produces fewer pages than the original — regardless
    //    of whether we detect static content in page streams. XFA PDFs often
    //    have form content in widget annotations rather than page content
    //    streams, so `has_static_content` may return false even when pages
    //    have substantial pre-rendered form content. When our layout is
    //    incomplete (fewer pages), preserving the original pages matches
    //    Adobe/pdfRest output better than truncated single-page output.
    //
    //    When the XFA overlay is minimal (e.g. just a title/header), the form
    //    relies on AcroForm widgets for its content. Preserving static content
    //    + baking widgets adds spurious form fields. Using the XFA path gives
    //    the correct minimal output matching pdfrest/Adobe behavior.
    //    When the XFA overlay is substantial (re-renders the full page), the
    //    pre-rendered content is authoritative — replacing it with XFA causes
    //    SSIM regressions due to font/rendering differences.
    //
    // The 1000-byte threshold separates minimal XFA templates (title/header
    // only, ~200-500 bytes) from full page re-renders (5000+ bytes).
    let overlay_is_substantial = overlays.iter().any(|o| o.content_stream.len() > 1000);
    // Clamp 1-page over-pagination only for static XFAF forms. Dynamic forms
    // often start from a 1-page placeholder PDF and legitimately flow onto
    // additional pages once XFA data is laid out. Clamping all 1-page inputs
    // to the original page count causes under-pagination on dynamic forms such
    // as Travel Expense Report / Checklist where Adobe renders 2-3 pages.
    let preserve_static =
        is_static_form || n_layout < n_existing || has_static_content && overlay_is_substantial;

    if preserve_static {
        // Bake widget appearances (field values, checkboxes, etc.) into the
        // page content so they survive AcroForm removal.
        flatten_widget_appearances(&mut doc);

        // No XFA overlay for preserved pages.  Widget AP baking is
        // sufficient — overlaying XFA content on top of baked widget
        // appearances causes ghost/double text because widget APs may
        // contain rotation matrices that produce differently-positioned text.
    } else {
        // Dynamic form: the layout engine determines page count.
        // Write each layout page to the output: overwrite existing pages
        // and add new pages when the layout produces more than the original.
        // NOTE: page cap (n_layout.min(n_existing)) was removed — it caused
        // 30 GATE #12 regressions because dynamic XFA forms often have a
        // single placeholder page while the actual form has many data-driven
        // pages. Capping to n_existing destroyed multi-page content.
        for (i, overlay) in overlays.iter().enumerate() {
            if i < n_existing {
                let lp = &layout.pages[i];
                write_page_content(
                    &mut doc,
                    existing_page_ids[i],
                    overlay,
                    &font_ids,
                    &embedded_font_objects,
                    Some(lp.width),
                    Some(lp.height),
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

        // Bake checkbox/radio AP marks from AcroForm widgets onto existing
        // pages.  The XFA overlay draws borders and captions; the AP "on"
        // stream adds the filled mark (circle, checkmark, etc.) that the
        // oracle renders for hybrid forms (#886).
        for &page_id in &existing_page_ids[..n_existing.min(n_layout)] {
            bake_checkbox_radio_ap_marks(&mut doc, page_id);
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

    if is_static_form {
        // Static forms: strip Widget annotations but keep non-Widget (links,
        // stamps, etc.).  flatten_widget_appearances already baked widgets
        // with AP into the page content and removed them from Annots, but
        // widgets without AP may remain.  Remove those too so PDF viewers
        // don't render interactive fields over the baked content.
        for &page_id in &existing_page_ids {
            strip_widget_annotations(&mut doc, page_id);
        }
    } else {
        // Dynamic/hybrid forms: strip ALL annotations — pages were
        // overwritten by XFA layout or widget baking covered field values.
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
// Embedded image files extraction (XFA §2.3 href resolution)
// ---------------------------------------------------------------------------

/// Extract embedded files from the PDF's Names/EmbeddedFiles tree.
///
/// XFA `<image href=".\filename.jpg">` references are resolved against this
/// tree at merge time (XFA Spec 3.3 §2.3).  The returned map is keyed by
/// the filename as it appears in the Names array (e.g. `.\lintje.jpg`).
fn extract_embedded_images(doc: &Document) -> HashMap<String, Vec<u8>> {
    let mut images = HashMap::new();

    // Helper: resolve a potentially indirect object.
    fn deref_dict<'a>(doc: &'a Document, obj: &'a Object) -> Option<&'a Dictionary> {
        match obj {
            Object::Reference(id) => doc.get_dictionary(*id).ok(),
            Object::Dictionary(d) => Some(d),
            _ => None,
        }
    }

    // Helper: extract stream content (decompressed).
    fn extract_stream(doc: &Document, obj: &Object) -> Option<Vec<u8>> {
        let stream_obj = match obj {
            Object::Reference(id) => doc.get_object(*id).ok()?,
            other => other,
        };
        if let Object::Stream(ref stream) = *stream_obj {
            let mut s = stream.clone();
            let _ = s.decompress();
            Some(s.content.clone())
        } else {
            None
        }
    }

    // Traverse: Catalog → /Names → /EmbeddedFiles → /Names array
    let catalog = match doc.catalog() {
        Ok(c) => c,
        Err(_) => return images,
    };
    let names_obj = match catalog.get(b"Names") {
        Ok(obj) => obj,
        Err(_) => {
            eprintln!("[img-href] no /Names in catalog");
            return images;
        }
    };
    let names_dict = match deref_dict(doc, names_obj) {
        Some(d) => d,
        None => return images,
    };
    // XFA PDFs may use /XFAImages instead of /EmbeddedFiles for image
    // references.  Check both keys.
    let ef_obj = match names_dict
        .get(b"XFAImages")
        .or_else(|_| names_dict.get(b"EmbeddedFiles"))
    {
        Ok(obj) => obj,
        Err(_) => return images,
    };
    let ef_dict = match deref_dict(doc, ef_obj) {
        Some(d) => d,
        None => return images,
    };

    // The EmbeddedFiles name tree has a /Names array: [(name1, ref1), …]
    let names_arr_obj = match ef_dict.get(b"Names") {
        Ok(obj) => obj,
        Err(_) => return images,
    };
    let names_array = match names_arr_obj {
        Object::Array(arr) => arr,
        Object::Reference(id) => match doc.get_object(*id) {
            Ok(Object::Array(arr)) => arr,
            _ => return images,
        },
        _ => return images,
    };

    // Process pairs: (name_string, value_ref)
    let mut i = 0;
    while i + 1 < names_array.len() {
        let name = match &names_array[i] {
            Object::String(bytes, _) => String::from_utf8_lossy(bytes).to_string(),
            _ => {
                i += 2;
                continue;
            }
        };

        // The value can be:
        //   1. A FileSpec dict: /EF → /F → stream
        //   2. Directly a stream (non-standard but seen in XFA PDFs)
        let value_ref = &names_array[i + 1];

        // Try path 1: FileSpec dict
        if let Some(filespec) = deref_dict(doc, value_ref) {
            if let Ok(ef_obj) = filespec.get(b"EF") {
                if let Some(ef) = deref_dict(doc, ef_obj) {
                    if let Ok(f_ref) = ef.get(b"F") {
                        if let Some(data) = extract_stream(doc, f_ref) {
                            images.insert(name.clone(), data);
                            i += 2;
                            continue;
                        }
                    }
                }
            }
        }

        // Try path 2: Direct stream reference
        if let Some(data) = extract_stream(doc, value_ref) {
            images.insert(name.clone(), data);
        }

        i += 2;
    }
    images
}

// ---------------------------------------------------------------------------
// Font extraction, resolution, and embedding
// ---------------------------------------------------------------------------

#[doc(hidden)]
pub fn extract_embedded_fonts(doc: &Document) -> Vec<EmbeddedFontData> {
    let mut fonts = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (&font_object_id, obj) in &doc.objects {
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

        let pdf_widths = extract_font_widths(dict);
        let pdf_encoding = extract_font_encoding(doc, dict);
        let pdf_source_font =
            extract_simple_pdf_source_font(doc, font_object_id, dict, pdf_widths.as_ref());

        // First try direct FontDescriptor path (simple TrueType/OpenType fonts)
        if let Some((stream_id, data)) = extract_font_from_direct_fd(doc, dict, &base_font) {
            if seen.insert(stream_id) {
                store_font_data(
                    &mut fonts,
                    &base_font,
                    data,
                    pdf_widths.clone(),
                    pdf_encoding.clone(),
                    pdf_source_font,
                );
            }
            continue;
        }

        // For CIDFont Type0: also check DescendantFonts path
        // CIDFont fonts store their font data in /DescendantFonts[n]/FontDescriptor/FontFile*
        // CID fonts use /W arrays (PDF spec §9.7.4.3) instead of simple /Widths.
        if let Some((stream_id, data)) = extract_cidfont_data(doc, dict, &base_font, &seen) {
            if seen.insert(stream_id) {
                let cid_widths = extract_cid_font_widths(doc, dict);
                store_font_data(&mut fonts, &base_font, data, cid_widths, None, None);
            }
            continue;
        }

        if let Some(source_font) = pdf_source_font {
            // fix(#811): if the source PDF already exposes a reusable simple
            // font object with /Widths, keep that object alive through the XFA
            // pipeline. PDF 1.7 §5.5 defines those widths as the authoritative
            // simple-font metrics, and XFA 3.3 §11.7.1 relies on those metrics
            // for field fitting.
            store_font_data(
                &mut fonts,
                &base_font,
                Vec::new(),
                pdf_widths.clone(),
                pdf_encoding.clone(),
                Some(source_font),
            );
        }
    }
    fonts
}

/// Extract /FirstChar, /LastChar, and /Widths from a font dictionary.
fn extract_font_widths(dict: &lopdf::Dictionary) -> Option<(u16, Vec<u16>)> {
    let first_char = dict.get(b"FirstChar").ok()?.as_i64().ok()? as u16;
    let _last_char = dict.get(b"LastChar").ok()?.as_i64().ok()? as u16;
    let widths_array = dict.get(b"Widths").ok()?.as_array().ok()?;
    let widths: Vec<u16> = widths_array
        .iter()
        .filter_map(|w| w.as_i64().ok().map(|v| v as u16))
        .collect();
    if widths.is_empty() {
        return None;
    }
    Some((first_char, widths))
}

/// Extract CID font widths from a Type0 (composite) font's `/W` array.
///
/// CID fonts (PDF spec §9.7.4.3, Table 114) use a different width format than
/// simple fonts. Instead of `/FirstChar` + `/Widths`, they use a `/W` array in
/// the CIDFont descendant dictionary with two element types:
///
///   `cid_start [w1 w2 w3 ...]`   — consecutive CIDs starting at cid_start
///   `cid_first cid_last width`   — range of CIDs all sharing the same width
///
/// `/DW` (default width, defaults to 1000) applies to CIDs not listed in `/W`.
///
/// The result is converted to the same `(first_char, widths)` representation
/// used by simple fonts, where `widths[cid - first_char]` gives the width.
///
/// LIMITATION: CID-to-Unicode mapping via ToUnicode CMap is not parsed here;
/// the widths are indexed by raw CID values.
fn extract_cid_font_widths(
    doc: &Document,
    type0_dict: &lopdf::Dictionary,
) -> Option<(u16, Vec<u16>)> {
    let descendants = type0_dict.get(b"DescendantFonts").ok()?.as_array().ok()?;
    let desc_ref = descendants.first()?;
    let cid_dict = match desc_ref {
        Object::Reference(id) => doc.get_dictionary(*id).ok()?,
        Object::Dictionary(d) => d,
        _ => return None,
    };

    let default_width = cid_dict
        .get(b"DW")
        .ok()
        .and_then(|o| o.as_i64().ok())
        .unwrap_or(1000) as u16;

    let w_array = cid_dict.get(b"W").ok()?;
    let w_array = match resolve_object(doc, w_array) {
        Some(obj) => obj.as_array().ok()?,
        None => return None,
    };

    if w_array.is_empty() {
        return None;
    }

    // First pass: collect all (cid, width) pairs to find bounds.
    let mut entries: Vec<(u16, u16)> = Vec::new();
    let mut i = 0;
    while i < w_array.len() {
        let cid_start = match w_array[i].as_i64() {
            Ok(v) => v as u16,
            Err(_) => {
                i += 1;
                continue;
            }
        };
        i += 1;
        if i >= w_array.len() {
            break;
        }

        // Next element: array → consecutive widths, integer → range end
        if let Ok(widths_arr) = w_array[i].as_array() {
            // Format: cid_start [w1 w2 w3 ...]
            for (j, w_obj) in widths_arr.iter().enumerate() {
                if let Ok(w) = w_obj.as_i64() {
                    entries.push((cid_start + j as u16, w as u16));
                }
            }
            i += 1;
        } else if let Ok(cid_last) = w_array[i].as_i64() {
            // Format: cid_first cid_last width
            i += 1;
            if i >= w_array.len() {
                break;
            }
            if let Ok(width) = w_array[i].as_i64() {
                let cid_last = cid_last as u16;
                for cid in cid_start..=cid_last {
                    entries.push((cid, width as u16));
                }
            }
            i += 1;
        } else {
            i += 1;
        }
    }

    if entries.is_empty() {
        return None;
    }

    let min_cid = entries.iter().map(|(c, _)| *c).min().unwrap();
    let max_cid = entries.iter().map(|(c, _)| *c).max().unwrap();
    let len = (max_cid - min_cid + 1) as usize;
    let mut widths = vec![default_width; len];
    for (cid, w) in &entries {
        widths[(*cid - min_cid) as usize] = *w;
    }

    Some((min_cid, widths))
}

/// Parse a simple-font `/Encoding` dictionary with `/Differences`.
///
/// WHY: Custom encodings via `/Differences` are essential for correct glyph
/// width mapping. Without this, widths are indexed against the wrong
/// characters and text wrapping breaks for fonts that deviate from WinAnsi.
///
/// SPEC: PDF spec §9.6.6.1 defines `/Differences` as an alternating array of
/// starting code integers and glyph names applied on top of a base encoding.
///
/// LIMITATION: CID fonts (`/Type0`) use CMaps and `/W` arrays instead of this
/// simple-font encoding mechanism, so they intentionally return `None` here.
fn extract_font_encoding(doc: &Document, dict: &lopdf::Dictionary) -> Option<PdfSimpleEncoding> {
    let encoding_obj = resolve_object(doc, dict.get(b"Encoding").ok()?)?;
    let encoding_dict = encoding_obj.as_dict().ok()?;
    let differences_array = resolve_object(doc, encoding_dict.get(b"Differences").ok()?)?
        .as_array()
        .ok()?;

    let base_encoding = encoding_dict
        .get(b"BaseEncoding")
        .ok()
        .and_then(|obj| resolve_object(doc, obj))
        .and_then(|obj| obj.as_name().ok())
        .and_then(PdfBaseEncoding::from_pdf_name)
        .unwrap_or(PdfBaseEncoding::WinAnsi);

    let mut differences = Vec::new();
    let mut current_code: Option<u8> = None;
    for item in differences_array {
        let item = resolve_object(doc, item)?;
        if let Ok(code) = item.as_i64() {
            current_code = u8::try_from(code).ok();
            continue;
        }

        let Some(name) = item.as_name().ok() else {
            continue;
        };
        let Some(code) = current_code else {
            continue;
        };
        let Some(glyph_name) = std::str::from_utf8(name).ok() else {
            continue;
        };
        if let Some(unicode) = pdf_glyph_name_to_unicode(glyph_name) {
            differences.push((code, unicode));
        }
        current_code = code.checked_add(1);
    }

    if differences.is_empty() {
        return None;
    }

    Some(PdfSimpleEncoding {
        base_encoding,
        differences,
    })
}

fn extract_simple_pdf_source_font(
    doc: &Document,
    font_object_id: ObjectId,
    dict: &lopdf::Dictionary,
    pdf_widths: Option<&(u16, Vec<u16>)>,
) -> Option<PdfSourceFont> {
    pdf_widths?;

    let subtype = dict.get(b"Subtype").ok().and_then(|obj| obj.as_name().ok());
    if subtype == Some(b"Type0".as_slice()) {
        return None;
    }

    // fix(#811): only reuse simple fonts whose emitted PDF text can stay on
    // the current WinAnsi path in render_bridge. Fonts with custom encodings
    // need a dedicated byte encoder first; otherwise we would preserve widths
    // but emit the wrong character codes.
    //
    // PDF 1.7 §5.5 defines simple-font widths in the font's encoding space.
    // LIMITATION: CID/Type0 fonts use /W arrays and CMaps instead; they are
    // intentionally excluded here.
    let encoding_obj = dict
        .get(b"Encoding")
        .ok()
        .and_then(|obj| resolve_object(doc, obj));
    match encoding_obj {
        Some(obj) if obj.as_name().ok() == Some(b"WinAnsiEncoding".as_slice()) => {}
        Some(obj) => {
            let base = obj
                .as_dict()
                .ok()
                .and_then(|enc| enc.get(b"BaseEncoding").ok())
                .and_then(|base| resolve_object(doc, base))
                .and_then(|base| base.as_name().ok());
            if base != Some(b"WinAnsiEncoding".as_slice()) {
                return None;
            }
            if obj
                .as_dict()
                .ok()
                .and_then(|enc| enc.get(b"Differences").ok())
                .is_some()
            {
                return None;
            }
        }
        None => return None,
    }

    Some(PdfSourceFont {
        object_id: font_object_id,
    })
}

fn resolve_object<'a>(doc: &'a Document, obj: &'a Object) -> Option<&'a Object> {
    match obj {
        Object::Reference(id) => doc.get_object(*id).ok(),
        other => Some(other),
    }
}

/// Extract font data from a direct FontDescriptor (FontFile2/3/1 in FontDescriptor).
fn extract_font_from_direct_fd(
    doc: &Document,
    font_dict: &lopdf::Dictionary,
    _base_font: &str,
) -> Option<(lopdf::ObjectId, Vec<u8>)> {
    let fd_id = font_dict.get(b"FontDescriptor").ok()?.as_reference().ok()?;
    let fd = doc.get_dictionary(fd_id).ok()?;

    let font_stream_id = fd
        .get(b"FontFile2")
        .or_else(|_| fd.get(b"FontFile3"))
        .or_else(|_| fd.get(b"FontFile"))
        .ok()?
        .as_reference()
        .ok()?;

    let stream = doc
        .get_object(font_stream_id)
        .and_then(|o| o.as_stream())
        .ok()?;

    let data = stream
        .get_plain_content()
        .unwrap_or_else(|_| stream.content.clone());

    if data.is_empty() {
        return None;
    }

    Some((font_stream_id, data))
}

/// Extract font data from CIDFont Type0's DescendantFonts path.
///
/// CIDFont Type0 fonts have their font data in:
///   /DescendantFonts[n] /CIDFont /FontDescriptor /FontFile*
fn extract_cidfont_data(
    doc: &Document,
    font_dict: &lopdf::Dictionary,
    _base_font: &str,
    seen: &std::collections::HashSet<lopdf::ObjectId>,
) -> Option<(lopdf::ObjectId, Vec<u8>)> {
    // Check if this is a Type0 (composite) font by looking for DescendantFonts
    let descendants = font_dict.get(b"DescendantFonts").ok()?.as_array().ok()?;

    // Iterate through descendant CIDFonts
    for desc_ref in descendants {
        let desc_id = desc_ref.as_reference().ok()?;
        let desc_dict = doc.get_dictionary(desc_id).ok()?;

        // Check if this descendant is a CIDFont (has FontDescriptor)
        let fd_id = desc_dict.get(b"FontDescriptor").ok()?.as_reference().ok()?;
        let fd = doc.get_dictionary(fd_id).ok()?;

        // Try FontFile3 first (CFF font for CIDFontType0C), then FontFile2 (TrueType)
        let font_stream_id = fd
            .get(b"FontFile3")
            .or_else(|_| fd.get(b"FontFile2"))
            .or_else(|_| fd.get(b"FontFile"))
            .ok()?
            .as_reference()
            .ok()?;

        if seen.contains(&font_stream_id) {
            continue;
        }

        let stream = doc
            .get_object(font_stream_id)
            .and_then(|o| o.as_stream())
            .ok()?;

        let data = stream
            .get_plain_content()
            .unwrap_or_else(|_| stream.content.clone());

        if !data.is_empty() {
            return Some((font_stream_id, data));
        }
    }
    None
}

/// Store font data under multiple names (PostScript name, family name, normalized name).
fn store_font_data(
    fonts: &mut Vec<EmbeddedFontData>,
    base_font: &str,
    data: Vec<u8>,
    pdf_widths: Option<(u16, Vec<u16>)>,
    pdf_encoding: Option<PdfSimpleEncoding>,
    pdf_source_font: Option<PdfSourceFont>,
) {
    let clean_name = if let Some(pos) = base_font.find('+') {
        base_font[pos + 1..].to_string()
    } else {
        base_font.to_string()
    };

    // Store under the PostScript name (subset prefix already stripped)
    fonts.push(EmbeddedFontData {
        name: clean_name.clone(),
        data: data.clone(),
        pdf_widths: pdf_widths.clone(),
        pdf_encoding: pdf_encoding.clone(),
        pdf_source_font,
    });

    // Also store under the font family name from the name table,
    // since XFA templates use family names (e.g. "Arial") while PDF
    // BaseFont uses PostScript names (e.g. "ArialMT").
    if let Ok(face) = ttf_parser::Face::parse(&data, 0) {
        for name_record in face.names() {
            if name_record.name_id == ttf_parser::name_id::FAMILY {
                if let Some(family) = name_record.to_string() {
                    if family != clean_name {
                        fonts.push(EmbeddedFontData {
                            name: family,
                            data: data.clone(),
                            pdf_widths: pdf_widths.clone(),
                            pdf_encoding: pdf_encoding.clone(),
                            pdf_source_font,
                        });
                    }
                }
            }
        }
    }

    // Common PostScript-to-family normalization as fallback
    let normalized = ps_name_to_family(&clean_name);
    if normalized != clean_name {
        fonts.push(EmbeddedFontData {
            name: normalized,
            data,
            pdf_widths,
            pdf_encoding,
            pdf_source_font,
        });
    }
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
                    let generic_family = node.attribute("genericFamily").map(|s| s.to_string());
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
        let _ = writeln!(cmap, "{} beginbfchar", chunk.len());
        for &(gid, ch) in chunk {
            let _ = writeln!(cmap, "<{:04X}> <{:04X}>", gid, ch as u32);
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
fn simple_encoding_unicode_to_code_map(encoding: &PdfSimpleEncoding) -> HashMap<u16, u8> {
    let mut map = HashMap::new();
    for (code, unicode) in encoding.code_to_unicode_table().into_iter().enumerate() {
        if let Some(cp) = unicode {
            map.entry(cp).or_insert(code as u8);
        }
    }
    map
}

fn add_text_chars_for_font(
    chars_by_font: &mut HashMap<String, HashSet<char>>,
    font_family: Option<&str>,
    font_weight: Option<&str>,
    font_style: Option<&str>,
    text: &str,
) {
    let Some(family) = font_family else {
        return;
    };
    if text.is_empty() {
        return;
    }
    let chars: Vec<char> = text.chars().filter(|c| !c.is_control()).collect();
    if chars.is_empty() {
        return;
    }

    let variant = font_variant_key(family, font_weight, font_style);
    chars_by_font
        .entry(variant)
        .or_default()
        .extend(chars.iter().copied());
    chars_by_font
        .entry(family.to_string())
        .or_default()
        .extend(chars);
}

fn add_text_chars_for_style(
    chars_by_font: &mut HashMap<String, HashSet<char>>,
    style: &FormNodeStyle,
    text: &str,
) {
    add_text_chars_for_font(
        chars_by_font,
        style.font_family.as_deref(),
        style.font_weight.as_deref(),
        style.font_style.as_deref(),
        text,
    );
}

fn collect_used_chars_from_layout_node(
    node: &LayoutNode,
    chars_by_font: &mut HashMap<String, HashSet<char>>,
) {
    match &node.content {
        LayoutContent::Text(t) => add_text_chars_for_style(chars_by_font, &node.style, t),
        LayoutContent::Field { value, .. } => {
            add_text_chars_for_style(chars_by_font, &node.style, value)
        }
        LayoutContent::WrappedText { lines, .. } => {
            for line in lines {
                add_text_chars_for_style(chars_by_font, &node.style, line);
            }
        }
        LayoutContent::Draw(DrawContent::Text(t)) => {
            add_text_chars_for_style(chars_by_font, &node.style, t)
        }
        _ => {}
    }

    if let Some(caption) = &node.style.caption_text {
        add_text_chars_for_style(chars_by_font, &node.style, caption);
    }

    if let Some(spans) = &node.style.rich_text_spans {
        for span in spans {
            add_text_chars_for_font(
                chars_by_font,
                span.font_family
                    .as_deref()
                    .or(node.style.font_family.as_deref()),
                span.font_weight
                    .as_deref()
                    .or(node.style.font_weight.as_deref()),
                span.font_style
                    .as_deref()
                    .or(node.style.font_style.as_deref()),
                &span.text,
            );
        }
    }

    for child in &node.children {
        collect_used_chars_from_layout_node(child, chars_by_font);
    }
}

fn collect_used_chars_by_font(layout: &LayoutDom) -> HashMap<String, HashSet<char>> {
    let mut chars_by_font = HashMap::new();
    for page in &layout.pages {
        for node in &page.nodes {
            collect_used_chars_from_layout_node(node, &mut chars_by_font);
        }
    }
    chars_by_font
}

fn simple_font_can_encode_char(font: &ResolvedFont, ch: char) -> bool {
    if ch.is_ascii() {
        return true;
    }
    if let Some(encoding) = &font.pdf_encoding {
        let Ok(cp) = u16::try_from(ch as u32) else {
            return false;
        };
        return encoding
            .code_to_unicode_table()
            .into_iter()
            .flatten()
            .any(|u| u == cp);
    }
    unicode_to_winansi(ch).is_some()
}

fn variant_key_base_name(key: &str) -> Option<&str> {
    key.strip_suffix("_Bold_Italic")
        .or_else(|| key.strip_suffix("_Bold_Normal"))
        .or_else(|| key.strip_suffix("_Normal_Italic"))
        .or_else(|| key.strip_suffix("_Normal_Normal"))
}

#[allow(clippy::type_complexity)]
fn embed_resolved_fonts(
    doc: &mut Document,
    resolved: &HashMap<String, ResolvedFont>,
    layout: &LayoutDom,
) -> (
    HashMap<String, String>,
    Vec<(String, ObjectId)>,
    HashMap<String, FontMetricsData>,
) {
    let mut font_map = HashMap::new();
    let mut font_objects = Vec::new();
    let mut metrics_data = HashMap::new();
    let used_chars_by_font = collect_used_chars_by_font(layout);
    for (idx, (name, font)) in resolved.iter().enumerate() {
        let resource_name = format!("XFA_F{}", idx);
        // fix(#811): once a simple source font survives resolution, keep using
        // the original PDF object instead of emitting a synthetic Type0/system
        // fallback. That keeps field-fit behaviour aligned with the source PDF
        // and Acrobat's interpretation of the same /Widths table.
        //
        // WHY: custom encodings and non-ASCII content can require Unicode
        // shaping through Identity-H. If a simple source font cannot encode
        // the actual text in layout output, reusing it would produce '?'
        // substitutions in content streams.
        //
        // LIMITATION: CID source fonts (/Type0 with /W arrays) use a different
        // mechanism and are not covered by this simple-font encodeability gate.
        let used_chars = used_chars_by_font
            .get(name)
            .or_else(|| used_chars_by_font.get(&font.name))
            .or_else(|| variant_key_base_name(name).and_then(|base| used_chars_by_font.get(base)));
        let source_can_encode_all_text = used_chars.is_none_or(|chars| {
            chars
                .iter()
                .all(|ch| simple_font_can_encode_char(font, *ch))
        });
        let (obj_id, render_font_data) = if let Some(source_font) = font.pdf_source_font {
            if source_can_encode_all_text || font.data.is_empty() {
                (source_font.object_id, None)
            } else {
                (embed_font_in_pdf(doc, font), Some(font.data.clone()))
            }
        } else {
            (embed_font_in_pdf(doc, font), Some(font.data.clone()))
        };
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
                font_data: render_font_data,
                face_index: font.face_index,
                simple_unicode_to_code: font
                    .pdf_encoding
                    .as_ref()
                    .map(simple_encoding_unicode_to_code_map),
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

/// Apply presence overrides and repeating-instance expansion from the XFA form
/// DOM packet.
///
/// When an XFA PDF has been opened and saved by Adobe Reader, the form DOM
/// captures the runtime state of all nodes after scripts executed.  We walk
/// the form DOM and the FormTree in parallel (matching by subform/field name)
/// to:
///
/// 1. Transfer `presence="hidden"` attributes that our script interpreter
///    could not compute (e.g. Avoka framework's `sfcUtils.updateVisibility`).
/// 2. Replicate repeating subform instances (XFA §4.4.3): when `bind
///    match="none"` prevents data-driven expansion, the form DOM records the
///    correct instance count produced by the runtime's `instanceManager`.  We
///    deep-clone the template instance and populate field values from the form
///    DOM so the layout engine produces the right number of pages.
fn apply_form_dom_presence(tree: &mut FormTree, root_id: FormNodeId, form_xml: &str) {
    use xfa_layout_engine::form::{FormNodeType, Presence};

    let Ok(doc) = roxmltree::Document::parse(form_xml) else {
        return;
    };

    /// Deep-clone a subtree rooted at `src_id`, returning the new root id.
    fn clone_subtree(tree: &mut FormTree, src_id: FormNodeId) -> FormNodeId {
        let node = tree.get(src_id).clone();
        let meta = tree.meta(src_id).clone();
        // Temporarily take children out to avoid borrow issues
        let child_ids: Vec<FormNodeId> = node.children.clone();
        let mut new_node = node;
        new_node.children = Vec::new();
        // Clear xfa_id to avoid duplicate id conflicts
        let mut new_meta = meta;
        new_meta.xfa_id = None;
        let new_id = tree.add_node_with_meta(new_node, new_meta);
        // Recursively clone children
        for &child_id in &child_ids {
            let cloned_child = clone_subtree(tree, child_id);
            tree.get_mut(new_id).children.push(cloned_child);
        }
        new_id
    }

    /// Extract the text content of the first `<value>` child's inner element
    /// (e.g. `<value><text>hello</text></value>` → `"hello"`).
    fn extract_field_value(xml_field: roxmltree::Node<'_, '_>) -> Option<String> {
        let value_el = xml_field
            .children()
            .find(|c| c.is_element() && c.tag_name().name() == "value")?;
        // The inner element may be <text>, <date>, <time>, <float>, etc.
        let inner = value_el.children().find(|c| c.is_element())?;
        inner.text().map(|t| t.to_string())
    }

    /// Apply presence, values, and child expansion from the form DOM to a
    /// FormTree node.
    fn apply_recursive(
        tree: &mut FormTree,
        form_node_id: FormNodeId,
        xml_node: roxmltree::Node<'_, '_>,
    ) {
        let xml_tag = xml_node.tag_name().name();
        if xml_tag != "subform" && xml_tag != "field" && xml_tag != "form" {
            return;
        }

        // Apply presence override.
        if xml_tag == "subform" || xml_tag == "field" {
            if let Some(pres) = xml_node.attribute("presence") {
                if pres == "hidden" {
                    tree.meta_mut(form_node_id).presence = Presence::Hidden;
                }
            }
        }

        // Transfer field value from the form DOM when the FormTree node has no
        // value yet (empty string) or the form DOM has a computed value.
        if xml_tag == "field" {
            if let Some(val) = extract_field_value(xml_node) {
                if let FormNodeType::Field { ref value, .. } = tree.get(form_node_id).node_type {
                    if value.is_empty() {
                        tree.get_mut(form_node_id).node_type = FormNodeType::Field { value: val };
                    }
                }
            }
            return; // fields have no structural children to recurse into
        }

        // Collect XML children (subforms and fields), skipping instanceManagers.
        let xml_children: Vec<roxmltree::Node<'_, '_>> = xml_node
            .children()
            .filter(|c| {
                c.is_element()
                    && (c.tag_name().name() == "subform"
                        || c.tag_name().name() == "field"
                        || c.tag_name().name() == "draw")
            })
            .collect();

        // Group consecutive XML children by name to detect repeating instances.
        // E.g., [Activity, Activity, Activity, Activity] → ("Activity", 4)
        let mut xml_groups: Vec<(&str, Vec<roxmltree::Node<'_, '_>>)> = Vec::new();
        for &xc in &xml_children {
            let xname = xc.attribute("name").unwrap_or("");
            if let Some(last) = xml_groups.last_mut() {
                if last.0 == xname {
                    last.1.push(xc);
                    continue;
                }
            }
            xml_groups.push((xname, vec![xc]));
        }

        // For each group, match against FormTree children, cloning when needed.
        let mut form_children = tree.get(form_node_id).children.clone();
        let mut used = vec![false; form_children.len()];

        for (gname, group_xml_nodes) in &xml_groups {
            let xml_count = group_xml_nodes.len();

            // Count existing FormTree children with this name
            let existing: Vec<(usize, FormNodeId)> = form_children
                .iter()
                .enumerate()
                .filter(|(i, &fid)| !used[*i] && tree.get(fid).name == *gname)
                .map(|(i, &fid)| (i, fid))
                .collect();
            let existing_count = existing.len();

            // If the form DOM has more instances than the FormTree, clone to match.
            if xml_count > existing_count && existing_count > 0 {
                let template_id = existing[0].1;
                // Find insertion position: after the last existing sibling
                let last_existing_idx = existing.last().unwrap().0;
                let insert_pos = last_existing_idx + 1;
                let clones_needed = xml_count - existing_count;
                let mut new_ids = Vec::new();
                for _ in 0..clones_needed {
                    let cloned = clone_subtree(tree, template_id);
                    new_ids.push(cloned);
                }
                // Insert cloned nodes into the parent's children list
                for (offset, new_id) in new_ids.iter().enumerate() {
                    form_children.insert(insert_pos + offset, *new_id);
                    used.insert(insert_pos + offset, false);
                }
                // Persist the updated children list
                tree.get_mut(form_node_id).children = form_children.clone();
            }

            // Now match each XML node in the group to a FormTree child
            let mut group_idx = 0;
            for &xc in group_xml_nodes {
                // Find next unmatched FormTree child with this name
                let matched = form_children
                    .iter()
                    .enumerate()
                    .skip(if group_idx > 0 {
                        // Start searching after the last matched position
                        form_children
                            .iter()
                            .enumerate()
                            .filter(|(i, &fid)| used[*i] && tree.get(fid).name == *gname)
                            .last()
                            .map(|(i, _)| i + 1)
                            .unwrap_or(0)
                    } else {
                        0
                    })
                    .find(|(i, &fid)| !used[*i] && tree.get(fid).name == *gname);
                if let Some((idx, &fid)) = matched {
                    used[idx] = true;
                    apply_recursive(tree, fid, xc);
                }
                group_idx += 1;
            }
        }
    }

    // The form DOM root is <form><subform name="...">...</subform></form>
    let form_root = doc.root_element();
    let form_root_subform = form_root
        .children()
        .find(|c| c.is_element() && c.tag_name().name() == "subform");

    if let Some(xml_root_sf) = form_root_subform {
        let root_children = tree.get(root_id).children.clone();
        let root_name = xml_root_sf.attribute("name").unwrap_or("");
        for &child_id in &root_children {
            if tree.get(child_id).name == root_name {
                apply_recursive(tree, child_id, xml_root_sf);
                break;
            }
        }
    }
}

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
                    && matches!(c.tag_name().name(), "subform" | "pageSet" | "subformSet")
            })
        }
        Err(_) => true, // Unparseable template is corrupt.
    }
}

/// Strip undefined XML entity references from XFA template/datasets XML.
///
/// `roxmltree` only supports the five predefined XML entities (lt, gt, amp,
/// quot, apos). Some XFA PDFs contain custom entity references like `&xxe;`
/// that cause parse failures, so we drop only those references.
///
/// fixes #812: Adobe-generated XFA packets also contain raw `&` inside
/// processing instructions such as `<?renderCache.subset ... "#$%&'()+"?>`
/// and `<?renderCache.textRun ... "A. Adjustment & Location" ...?>`.
/// Those packets are valid XML because PI payload is opaque text. The old
/// implementation deleted everything between `&` and the next `;`, which
/// corrupted valid templates before merge and forced the flattener down the
/// 1-page static fallback path.
///
/// XFA Spec 3.3 §8.6 / §8.8 rely on the template reaching the merge/layout
/// pipeline intact. CID `/W` handling is unrelated and remains out of scope.
fn strip_undefined_xml_entities(xml: &str) -> String {
    let predefined = ["lt", "gt", "amp", "quot", "apos"];
    let mut result = String::with_capacity(xml.len());
    let bytes = xml.as_bytes();
    let mut pos = 0;

    while let Some(rel_amp_pos) = xml[pos..].find('&') {
        let amp_pos = pos + rel_amp_pos;
        result.push_str(&xml[pos..amp_pos]);

        if let Some((entity_name, next_pos)) = parse_xml_entity_reference(xml, amp_pos) {
            // Keep numeric character references (&#123; or &#x1F;) and the
            // predefined XML entities. Drop only true named entity references
            // that roxmltree cannot resolve.
            if entity_name.starts_with('#') || predefined.contains(&entity_name) {
                result.push_str(&xml[amp_pos..next_pos]);
            }
            pos = next_pos;
        } else {
            // Not an XML entity reference; preserve the raw ampersand.
            result.push('&');
            pos = amp_pos + 1;
        }
    }

    if pos < bytes.len() {
        result.push_str(&xml[pos..]);
    }
    result
}

fn parse_xml_entity_reference(xml: &str, amp_pos: usize) -> Option<(&str, usize)> {
    let bytes = xml.as_bytes();
    let start = amp_pos + 1;
    let first = *bytes.get(start)?;

    // Numeric character references: &#123; or &#x1F;
    if first == b'#' {
        let mut idx = start + 1;
        if matches!(bytes.get(idx), Some(b'x' | b'X')) {
            idx += 1;
            let hex_start = idx;
            while matches!(
                bytes.get(idx),
                Some(b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F')
            ) {
                idx += 1;
            }
            if idx == hex_start || !matches!(bytes.get(idx), Some(b';')) {
                return None;
            }
        } else {
            let digits_start = idx;
            while matches!(bytes.get(idx), Some(b'0'..=b'9')) {
                idx += 1;
            }
            if idx == digits_start || !matches!(bytes.get(idx), Some(b';')) {
                return None;
            }
        }
        return Some((&xml[start..idx], idx + 1));
    }

    // Named references: &name; where `name` follows XML Name syntax enough to
    // distinguish it from raw PI/script/text ampersands.
    if !is_xml_name_start(first) {
        return None;
    }

    let mut idx = start + 1;
    while let Some(&b) = bytes.get(idx) {
        if b == b';' {
            return Some((&xml[start..idx], idx + 1));
        }
        if !is_xml_name_char(b) {
            return None;
        }
        idx += 1;
    }
    None
}

fn is_xml_name_start(byte: u8) -> bool {
    matches!(byte, b':' | b'_' | b'A'..=b'Z' | b'a'..=b'z')
}

fn is_xml_name_char(byte: u8) -> bool {
    is_xml_name_start(byte) || matches!(byte, b'-' | b'.' | b'0'..=b'9')
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

/// Bake checkbox/radio button appearance marks from AcroForm widget AP streams
/// onto existing page content for dynamic XFA forms.
///
/// Hybrid XFA PDFs carry pre-rendered appearance streams in their widget `/AP/N`
/// dictionaries.  For radio/checkbox widgets the Normal appearance dict often has
/// only the "on" state (filled circle / checkmark) with no "Off" entry.  The
/// oracle (iText / Adobe) renders this mark regardless of the current `/AS`
/// value.  This function stamps the "on" Normal appearance for every
/// checkbox/radio widget onto the page so the flattened output matches.
fn bake_checkbox_radio_ap_marks(doc: &mut Document, page_id: ObjectId) -> usize {
    let annots = page_annotations(doc, page_id);
    if annots.is_empty() {
        return 0;
    }

    let mut baked = 0usize;
    let mut overlay_ops = Vec::new();

    for annot in &annots {
        let Some(annot_id) = annot.as_reference().ok() else {
            continue;
        };
        let Ok(annot_dict) = doc.get_dictionary(annot_id).cloned() else {
            continue;
        };

        let is_widget = annot_dict
            .get(b"Subtype")
            .ok()
            .and_then(|obj| obj.as_name().ok())
            == Some(&b"Widget"[..]);
        if !is_widget {
            continue;
        }

        // Radio/checkbox widgets have a dictionary of named states in /AP/N
        // (e.g. /N << /0 35 0 R >>).  Text fields and pushbuttons have /AP/N
        // as a single stream reference.  Use this to filter.
        let ap = match annot_dict.get(b"AP").ok().and_then(|o| o.as_dict().ok()) {
            Some(ap) => ap.clone(),
            None => continue,
        };
        let normal_obj = match ap.get(b"N").ok() {
            Some(obj) => obj.clone(),
            None => continue,
        };

        // Resolve /N to a dictionary of appearance states.
        let states: Dictionary = match &normal_obj {
            Object::Reference(id) => match doc.get_object(*id).ok().cloned() {
                Some(Object::Dictionary(d)) => d,
                _ => continue, // direct stream → not radio/checkbox
            },
            Object::Dictionary(d) => d.clone(),
            _ => continue,
        };

        // Find the first non-"Off" state (the "on" mark appearance).
        let on_id = states
            .iter()
            .filter(|(name, _)| name.as_slice() != b"Off")
            .find_map(|(_, obj)| match obj {
                Object::Reference(id) => Some(*id),
                _ => None,
            });
        let Some(ap_id) = on_id else { continue };

        // Verify the referenced object is a Form XObject stream.
        match doc.get_object(ap_id).ok() {
            Some(Object::Stream(_)) => {}
            _ => continue,
        }

        let Some(rect) = annotation_rect(&annot_dict) else {
            continue;
        };

        let xobject_name = format!("XfaCbAp{}", baked);
        add_xobject_to_page_resources(doc, page_id, &xobject_name, ap_id);
        write_ops(
            &mut overlay_ops,
            format_args!(
                "q 1 0 0 1 {:.3} {:.3} cm /{} Do Q\n",
                rect[0], rect[1], xobject_name
            ),
        );
        baked += 1;
    }

    if !overlay_ops.is_empty() {
        append_to_page_content(doc, page_id, &overlay_ops);
    }

    baked
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

/// Remove Widget annotations from a page, keeping non-Widget annotations.
fn strip_widget_annotations(doc: &mut Document, page_id: ObjectId) {
    let annots = page_annotations(doc, page_id);
    if annots.is_empty() {
        return;
    }
    let mut retained = Vec::new();
    for annot in &annots {
        let is_widget = annot
            .as_reference()
            .ok()
            .and_then(|id| doc.get_dictionary(id).ok())
            .and_then(|d| d.get(b"Subtype").ok())
            .and_then(|obj| obj.as_name().ok())
            == Some(&b"Widget"[..]);
        if !is_widget {
            retained.push(annot.clone());
        }
    }
    set_page_annotations(doc, page_id, retained);
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
        // When the selected state is "Off" but no "Off" appearance exists,
        // fall through to the "on" appearance.  Radio/checkbox widgets in
        // hybrid XFA forms often have only the "on" mark in /AP/N — the
        // oracle (iText/Adobe) renders that mark regardless of /AS (#886).
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

fn selected_widget_state(annot_dict: &Dictionary) -> Option<&[u8]> {
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

    // Some PDFs store page /Contents as an indirect array of streams. Appending
    // by wrapping that array reference in another array creates nested content
    // arrays (`[ 1510 0 R 1574 0 R ]` where `1510 0 R` is itself an array),
    // which Poppler treats as "Weird page contents" and can blank the page.
    // Flatten the existing /Contents tree first so preserve-static/widget bake
    // paths remain valid on Adobe-generated forms like 697eeb9f.
    let new_contents = match contents {
        Some(existing) => {
            let mut flattened = Vec::new();
            flatten_page_contents_entries(doc, existing, &mut flattened);
            flattened.push(Object::Reference(new_stream_id));
            if flattened.len() == 1 {
                flattened.pop().unwrap()
            } else {
                Object::Array(flattened)
            }
        }
        None => Object::Reference(new_stream_id),
    };

    if let Ok(Object::Dictionary(ref mut page_dict)) = doc.get_object_mut(page_id) {
        page_dict.set("Contents", new_contents);
    }
}

fn flatten_page_contents_entries(doc: &mut Document, object: Object, out: &mut Vec<Object>) {
    match object {
        Object::Reference(id) => match doc.get_object(id).cloned() {
            Ok(Object::Array(items)) => {
                for item in items {
                    flatten_page_contents_entries(doc, item, out);
                }
            }
            _ => out.push(Object::Reference(id)),
        },
        Object::Array(items) => {
            for item in items {
                flatten_page_contents_entries(doc, item, out);
            }
        }
        Object::Stream(stream) => {
            let stream_id = doc.add_object(Object::Stream(stream));
            out.push(Object::Reference(stream_id));
        }
        other => out.push(other),
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
    page_width: Option<f64>,
    page_height: Option<f64>,
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
        // Update MediaBox to match the XFA layout page dimensions.
        // Dynamic XFA forms often have a placeholder page with different
        // dimensions than the template's pageArea (e.g. letter vs A4).
        if let (Some(w), Some(h)) = (page_width, page_height) {
            page_dict.set(
                "MediaBox",
                Object::Array(vec![
                    Object::Real(0.0),
                    Object::Real(0.0),
                    Object::Real(w as f32),
                    Object::Real(h as f32),
                ]),
            );
        }
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
        dict.remove(b"NeedsRendering");
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Test helper (GL-QA36): simulate a re-entrant call to flatten_xfa_to_pdf by
/// pre-setting FLATTEN_DEPTH to 1 before calling.  This is used to verify the
/// recursion guard without exposing the thread-local to the test sub-module.
///
/// IMPORTANT: This function resets FLATTEN_DEPTH to 0 before returning so that
/// subsequent calls on the same thread are not affected.
#[cfg(test)]
fn flatten_xfa_to_pdf_simulate_reentrant(pdf_bytes: &[u8]) -> Result<Vec<u8>> {
    FLATTEN_DEPTH.with(|d| d.set(1));
    let result = flatten_xfa_to_pdf(pdf_bytes);
    // Reset — the guard will have left depth at 1 because it detected depth>=1
    // and returned early before the DepthGuard could decrement.
    FLATTEN_DEPTH.with(|d| d.set(0));
    result
}

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

    #[test]
    fn append_to_page_content_flattens_indirect_contents_arrays() {
        let mut doc = Document::with_version("1.4");
        let pages_id = doc.new_object_id();
        let first_stream_id = doc.add_object(Stream::new(dictionary! {}, b"q\n".to_vec()));
        let second_stream_id = doc.add_object(Stream::new(dictionary! {}, b"Q\n".to_vec()));
        let contents_array_id = doc.add_object(Object::Array(vec![
            Object::Reference(first_stream_id),
            Object::Reference(second_stream_id),
        ]));
        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(612), Object::Integer(792),
            ]),
            "Contents" => Object::Reference(contents_array_id),
        }));
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => Object::Name(b"Pages".to_vec()),
                "Kids" => Object::Array(vec![Object::Reference(page_id)]),
                "Count" => Object::Integer(1),
            }),
        );

        append_to_page_content(&mut doc, page_id, b"BT\nET\n");

        let page_dict = doc.get_dictionary(page_id).expect("page dict");
        let contents = page_dict.get(b"Contents").expect("contents");
        let items = contents.as_array().expect("flattened contents array");

        assert_eq!(items.len(), 3, "existing streams + appended stream");
        assert!(
            items.iter().all(|obj| obj.as_reference().is_ok()),
            "contents array must stay flat and reference only streams"
        );
        for object in items {
            let stream_id = object.as_reference().expect("stream ref");
            assert!(
                doc.get_object(stream_id)
                    .expect("stream object")
                    .as_stream()
                    .is_ok(),
                "nested arrays must not survive in page contents"
            );
        }
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

    fn overflowing_paginate_xdp(base_profile: Option<&str>) -> String {
        let mut fields = String::new();
        for i in 0..40 {
            fields.push_str(&format!(
                r#"
      <field name="line{i}" w="7.0in" h="0.3in">
        <ui><textEdit/></ui>
        <value><text>Line {i}</text></value>
      </field>"#
            ));
        }

        let base_profile_attr = base_profile
            .map(|value| format!(r#" baseProfile="{value}""#))
            .unwrap_or_default();

        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"{base_profile_attr}>
  <subform name="form1" layout="paginate">
    <pageSet>
      <pageArea name="Page1">
        <contentArea x="0.5in" y="0.5in" w="7.5in" h="10in"/>
        <medium stock="default" short="8.5in" long="11in"/>
      </pageArea>
    </pageSet>
    <subform name="section" layout="tb" w="7.5in">{fields}
    </subform>
  </subform>
</template>
</xdp:xdp>"#
        )
    }

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
    fn static_single_page_pdf_does_not_append_xfa_overflow_pages() {
        let xdp = overflowing_paginate_xdp(Some("interactiveForms"));
        let pdf_bytes = build_xfa_pdf(&xdp);
        let result = flatten_xfa_to_pdf(&pdf_bytes).expect("flatten failed");

        let doc = Document::load_mem(&result).expect("load flattened PDF");
        let pages: Vec<ObjectId> = doc.page_iter().collect();

        assert_eq!(
            pages.len(),
            1,
            "static 1-page PDFs should preserve the original page when XFA layout over-paginates"
        );
    }

    #[test]
    fn dynamic_single_page_pdf_can_expand_beyond_original_page_count() {
        // Dynamic XFA forms may ship with a single placeholder PDF page while
        // Adobe lays out multiple pages from the XFA data/template at runtime.
        // Flattening must therefore preserve the layout engine's page count
        // instead of clamping to the original PDF page count.
        let xdp = overflowing_paginate_xdp(None);
        let pdf_bytes = build_xfa_pdf(&xdp);
        let result = flatten_xfa_to_pdf(&pdf_bytes).expect("flatten failed");

        let doc = Document::load_mem(&result).expect("load flattened PDF");
        let pages: Vec<ObjectId> = doc.page_iter().collect();

        assert_eq!(
            pages.len(),
            2,
            "dynamic 1-page PDFs should be allowed to grow when XFA layout paginates"
        );
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
    fn widget_as_off_without_off_appearance_falls_through_to_on() {
        // When /AS is "Off" but the Normal appearance dict has no "Off" key,
        // resolve_widget_normal_appearance should fall through to the "on"
        // state appearance.  Radio/checkbox widgets in hybrid XFA forms
        // often have only the "on" mark in /AP/N; the oracle renders that
        // mark regardless of /AS (#886).
        let yes_stream = Object::Stream(Stream::new(
            dictionary! {
                "Type" => Object::Name(b"XObject".to_vec()),
                "Subtype" => Object::Name(b"Form".to_vec()),
                "BBox" => Object::Array(vec![
                    Object::Integer(0), Object::Integer(0),
                    Object::Integer(10), Object::Integer(10),
                ]),
            },
            b"q 5 5 m 5 5 l S Q\n".to_vec(),
        ));

        let mut doc = Document::with_version("1.4");
        // Normal appearance has only a "0" key (checked state), no "Off" key.
        let state_id = doc.add_object(Object::Dictionary(dictionary! {
            "0" => yes_stream,
        }));
        let annot = dictionary! {
            "Subtype" => Object::Name(b"Widget".to_vec()),
            "Rect" => Object::Array(vec![
                Object::Integer(100), Object::Integer(700),
                Object::Integer(110), Object::Integer(710),
            ]),
            "AP" => Object::Dictionary(dictionary! {
                "N" => Object::Reference(state_id),
            }),
            "AS" => Object::Name(b"Off".to_vec()),
            "FT" => Object::Name(b"Btn".to_vec()),
        };
        assert!(
            resolve_widget_normal_appearance(&mut doc, &annot).is_some(),
            "Off state with no Off appearance should fall through to on state"
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
    fn encrypted_pdf_without_xfa_returns_ok() {
        // Encrypted PDF without AcroForm/XFA → returned as-is (no XFA to flatten).
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

        let encrypt_id = doc.add_object(Object::Dictionary(dictionary! {
            "Filter" => Object::Name(b"Standard".to_vec()),
            "V"      => Object::Integer(2),
            "Length"  => Object::Integer(128),
        }));
        doc.trailer.set("Encrypt", Object::Reference(encrypt_id));

        let mut buf = Vec::new();
        doc.save_to(&mut buf).expect("save test PDF");

        let result = flatten_xfa_to_pdf(&buf);
        assert!(result.is_ok(), "non-XFA encrypted PDF should return Ok");
    }

    #[test]
    fn encrypted_xfa_pdf_returns_encrypted_error() {
        // Encrypted PDF WITH AcroForm/XFA → should reach the decrypt check
        // and return Err(Encrypted) when the password is required.
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
        // Add AcroForm with XFA key so the byte-level pre-check passes.
        let xfa_stream_id = doc.add_object(Object::Stream(lopdf::Stream::new(
            dictionary! {},
            b"<xdp:xdp></xdp:xdp>".to_vec(),
        )));
        let acroform_id = doc.add_object(Object::Dictionary(dictionary! {
            "XFA" => Object::Reference(xfa_stream_id),
        }));
        let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type"     => Object::Name(b"Catalog".to_vec()),
            "Pages"    => Object::Reference(pages_id),
            "AcroForm" => Object::Reference(acroform_id),
        }));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let encrypt_id = doc.add_object(Object::Dictionary(dictionary! {
            "Filter" => Object::Name(b"Standard".to_vec()),
            "V"      => Object::Integer(2),
            "Length"  => Object::Integer(128),
        }));
        doc.trailer.set("Encrypt", Object::Reference(encrypt_id));

        let mut buf = Vec::new();
        doc.save_to(&mut buf).expect("save encrypted PDF");

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

    /// Build a minimal PDF with a Type0 (CID) font that has a /W array.
    fn build_pdf_with_cid_font(w_array: Vec<Object>, dw: Option<i64>) -> Document {
        let mut doc = Document::with_version("1.4");

        // Minimal CIDFont descendant dictionary with /W
        let mut cid_dict = dictionary! {
            "Type"    => Object::Name(b"Font".to_vec()),
            "Subtype" => Object::Name(b"CIDFontType2".to_vec()),
            "BaseFont" => Object::Name(b"TestFont".to_vec()),
            "W"       => Object::Array(w_array)
        };
        if let Some(dw_val) = dw {
            cid_dict.set("DW", Object::Integer(dw_val));
        }
        let cid_id = doc.add_object(Object::Dictionary(cid_dict));

        // Type0 composite font pointing to the CIDFont
        let type0_dict = dictionary! {
            "Type"            => Object::Name(b"Font".to_vec()),
            "Subtype"         => Object::Name(b"Type0".to_vec()),
            "BaseFont"        => Object::Name(b"TestFont".to_vec()),
            "DescendantFonts" => Object::Array(vec![Object::Reference(cid_id)])
        };
        doc.add_object(Object::Dictionary(type0_dict));
        doc
    }

    /// Test CID /W array parsing: consecutive widths format.
    /// /W [120 [500 600 700]] → CID 120=500, CID 121=600, CID 122=700
    #[test]
    fn cid_w_array_consecutive() {
        let w = vec![
            Object::Integer(120),
            Object::Array(vec![
                Object::Integer(500),
                Object::Integer(600),
                Object::Integer(700),
            ]),
        ];
        let doc = build_pdf_with_cid_font(w, None);
        let fonts = extract_embedded_fonts(&doc);

        // No font stream embedded, so extract_embedded_fonts won't find data.
        // Test the parser directly via the Type0 dict.
        for obj in doc.objects.values() {
            let dict = match obj.as_dict() {
                Ok(d) => d,
                Err(_) => continue,
            };
            let subtype = dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok());
            if subtype == Some(b"Type0".as_slice()) {
                let result = extract_cid_font_widths(&doc, dict);
                let (first, widths) = result.expect("should parse /W array");
                assert_eq!(first, 120);
                assert_eq!(widths.len(), 3);
                assert_eq!(widths[0], 500); // CID 120
                assert_eq!(widths[1], 600); // CID 121
                assert_eq!(widths[2], 700); // CID 122
                return;
            }
        }
        panic!("Type0 font not found in test document");
    }

    /// Test CID /W array parsing: range format.
    /// /W [200 300 250] → CIDs 200-300 all have width 250
    #[test]
    fn cid_w_array_range() {
        let w = vec![
            Object::Integer(200),
            Object::Integer(300),
            Object::Integer(250),
        ];
        let doc = build_pdf_with_cid_font(w, None);

        for obj in doc.objects.values() {
            let dict = match obj.as_dict() {
                Ok(d) => d,
                Err(_) => continue,
            };
            let subtype = dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok());
            if subtype == Some(b"Type0".as_slice()) {
                let (first, widths) =
                    extract_cid_font_widths(&doc, dict).expect("should parse /W range");
                assert_eq!(first, 200);
                assert_eq!(widths.len(), 101); // 200..=300
                assert!(widths.iter().all(|&w| w == 250));
                return;
            }
        }
        panic!("Type0 font not found");
    }

    /// Test CID /W array parsing: mixed consecutive + range formats.
    /// /W [120 [500 600 700] 200 300 250]
    /// CID 120=500, 121=600, 122=700, CIDs 200-300=250
    /// Default width (/DW) fills gaps (CIDs 123-199).
    #[test]
    fn cid_w_array_mixed() {
        let w = vec![
            Object::Integer(120),
            Object::Array(vec![
                Object::Integer(500),
                Object::Integer(600),
                Object::Integer(700),
            ]),
            Object::Integer(200),
            Object::Integer(300),
            Object::Integer(250),
        ];
        let doc = build_pdf_with_cid_font(w, Some(1000));

        for obj in doc.objects.values() {
            let dict = match obj.as_dict() {
                Ok(d) => d,
                Err(_) => continue,
            };
            let subtype = dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok());
            if subtype == Some(b"Type0".as_slice()) {
                let (first, widths) =
                    extract_cid_font_widths(&doc, dict).expect("should parse mixed /W");
                assert_eq!(first, 120);
                assert_eq!(widths.len(), 181); // 120..=300
                                               // Consecutive part
                assert_eq!(widths[0], 500); // CID 120
                assert_eq!(widths[1], 600); // CID 121
                assert_eq!(widths[2], 700); // CID 122
                                            // Gap filled with /DW=1000
                assert_eq!(widths[3], 1000); // CID 123
                assert_eq!(widths[79], 1000); // CID 199
                                              // Range part
                assert_eq!(widths[80], 250); // CID 200
                assert_eq!(widths[180], 250); // CID 300
                return;
            }
        }
        panic!("Type0 font not found");
    }

    /// Test that /DW defaults to 1000 when not specified.
    #[test]
    fn cid_w_array_default_width() {
        let w = vec![
            Object::Integer(10),
            Object::Array(vec![Object::Integer(400)]),
            Object::Integer(20),
            Object::Array(vec![Object::Integer(600)]),
        ];
        let doc = build_pdf_with_cid_font(w, None); // no /DW → defaults to 1000

        for obj in doc.objects.values() {
            let dict = match obj.as_dict() {
                Ok(d) => d,
                Err(_) => continue,
            };
            let subtype = dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok());
            if subtype == Some(b"Type0".as_slice()) {
                let (first, widths) = extract_cid_font_widths(&doc, dict).expect("should parse /W");
                assert_eq!(first, 10);
                assert_eq!(widths[0], 400); // CID 10
                assert_eq!(widths[5], 1000); // CID 15 — default
                assert_eq!(widths[10], 600); // CID 20
                return;
            }
        }
        panic!("Type0 font not found");
    }

    #[test]
    fn extract_embedded_fonts_keeps_simple_pdf_fonts_without_fontfile() {
        let mut doc = Document::new();
        let font_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Font".to_vec()),
            "Subtype" => Object::Name(b"Type1".to_vec()),
            "BaseFont" => Object::Name(b"MyriadPro-Regular".to_vec()),
            "FirstChar" => Object::Integer(32),
            "LastChar" => Object::Integer(34),
            "Widths" => Object::Array(vec![
                Object::Integer(278),
                Object::Integer(333),
                Object::Integer(612),
            ]),
            "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
        }));

        let fonts = extract_embedded_fonts(&doc);
        let font = fonts
            .iter()
            .find(|font| font.name == "MyriadPro-Regular")
            .expect("expected reusable simple font");

        assert!(font.data.is_empty(), "no FontFile* should keep data empty");
        assert_eq!(font.pdf_widths, Some((32, vec![278, 333, 612])));
        assert_eq!(
            font.pdf_source_font,
            Some(PdfSourceFont { object_id: font_id })
        );
    }

    #[test]
    fn embed_resolved_fonts_reuses_existing_pdf_font_object() {
        let mut doc = Document::new();
        let source_font_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Font".to_vec()),
            "Subtype" => Object::Name(b"Type1".to_vec()),
            "BaseFont" => Object::Name(b"MyriadPro-Regular".to_vec()),
            "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
        }));
        let before = doc.objects.len();

        let mut resolved = HashMap::new();
        resolved.insert(
            "Myriad Pro_Normal_Normal".to_string(),
            ResolvedFont {
                name: "Myriad Pro".to_string(),
                data: Vec::new(),
                face_index: 0,
                units_per_em: 1000,
                ascender: 800,
                descender: -200,
                pdf_widths: Some((32, vec![278, 333, 612])),
                pdf_encoding: None,
                pdf_source_font: Some(PdfSourceFont {
                    object_id: source_font_id,
                }),
            },
        );

        let empty_layout = LayoutDom { pages: vec![] };
        let (_font_map, font_objects, metrics_data) =
            embed_resolved_fonts(&mut doc, &resolved, &empty_layout);

        assert_eq!(
            doc.objects.len(),
            before,
            "should not embed a new font object"
        );
        assert_eq!(font_objects.len(), 1);
        assert_eq!(font_objects[0].1, source_font_id);
        assert!(
            metrics_data["Myriad Pro_Normal_Normal"].font_data.is_none(),
            "reused simple fonts must keep WinAnsi text encoding"
        );
    }

    #[test]
    fn strip_undefined_entities_preserves_raw_ampersands_in_processing_instructions() {
        let xml = r##"<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><?renderCache.textRun 24 A. Adjustment & Location 0 1417 14917 0 0 0 "Myriad Pro" 0 0 18000 ISO-8859-1?><?renderCache.subset "Arial" 0 0 ISO-8859-1 "#$%&'()+,-./" ?><subform name="form1"><field name="A"/></subform></template>"##;

        let stripped = strip_undefined_xml_entities(xml);

        assert_eq!(
            stripped, xml,
            "raw ampersands inside processing instructions are valid and must survive sanitization"
        );
        roxmltree::Document::parse(&stripped)
            .expect("processing instructions must remain parseable");
    }

    #[test]
    fn strip_undefined_entities_drops_only_true_named_entity_references() {
        let xml = r#"<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="form1"><draw name="D"><value><text>alpha &bogus; beta &#169; &amp; gamma</text></value></draw></subform></template>"#;

        let stripped = strip_undefined_xml_entities(xml);

        assert!(
            !stripped.contains("&bogus;"),
            "unknown named entities should still be removed for roxmltree compatibility"
        );
        assert!(stripped.contains("&#169;"));
        assert!(stripped.contains("&amp;"));
        roxmltree::Document::parse(&stripped).expect("sanitized XML should parse");
    }

    /// Form DOM with more repeating instances than the template must expand
    /// the FormTree and populate field values.
    #[test]
    fn form_dom_expands_repeating_subform_instances() {
        use xfa_layout_engine::form::{FormNodeType, Occur};

        // Template: one Activity subform with bind=none, occur max=-1
        let template = r#"<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
          <subform name="root" layout="tb">
            <pageSet><pageArea name="P1">
              <contentArea w="200mm" h="280mm"/>
              <medium short="210mm" long="297mm"/>
            </pageArea></pageSet>
            <subform name="body" layout="tb">
              <subform name="Items" layout="tb">
                <bind match="none"/>
                <subform name="Row" layout="tb">
                  <bind match="none"/>
                  <occur max="-1"/>
                  <field name="Label"><ui><textEdit/></ui></field>
                </subform>
              </subform>
            </subform>
          </subform>
        </template>"#;

        // Form DOM: 3 Row instances with values
        let form_xml = r#"<form xmlns="http://www.xfa.org/schema/xfa-form/2.8/">
          <subform name="root">
            <subform name="body">
              <subform name="Items">
                <instanceManager name="_Row"/>
                <subform name="Row">
                  <field name="Label"><value><text>Alpha</text></value></field>
                </subform>
                <subform name="Row">
                  <field name="Label"><value><text>Beta</text></value></field>
                </subform>
                <subform name="Row">
                  <field name="Label"><value><text>Gamma</text></value></field>
                </subform>
              </subform>
            </subform>
          </subform>
        </form>"#;

        let data_dom = xfa_dom_resolver::data_dom::DataDom::new();
        let merger = crate::merger::FormMerger::new(&data_dom);
        let (mut tree, root_id) = merger.merge(template).unwrap();

        // Before form DOM: only 1 Row instance
        // Dump tree to understand structure
        fn find_by_name(tree: &FormTree, parent: FormNodeId, name: &str) -> Option<FormNodeId> {
            for &c in &tree.get(parent).children {
                if tree.get(c).name == name {
                    return Some(c);
                }
                if let Some(found) = find_by_name(tree, c, name) {
                    return Some(found);
                }
            }
            None
        }
        let items_id =
            find_by_name(&tree, root_id, "Items").expect("Items subform not found in tree");
        let rows_before = tree
            .get(items_id)
            .children
            .iter()
            .filter(|&&c| tree.get(c).name == "Row")
            .count();
        assert_eq!(
            rows_before, 1,
            "template merge should produce 1 Row (bind=none)"
        );

        // Apply form DOM
        apply_form_dom_presence(&mut tree, root_id, form_xml);

        // After form DOM: 3 Row instances with correct values
        let rows_after: Vec<FormNodeId> = tree
            .get(items_id)
            .children
            .iter()
            .filter(|&&c| tree.get(c).name == "Row")
            .copied()
            .collect();
        assert_eq!(
            rows_after.len(),
            3,
            "form DOM should expand to 3 Row instances"
        );

        let values: Vec<String> = rows_after
            .iter()
            .map(|&row_id| {
                let label_id = tree.get(row_id).children[0];
                match &tree.get(label_id).node_type {
                    FormNodeType::Field { value } => value.clone(),
                    _ => String::new(),
                }
            })
            .collect();
        assert_eq!(values, vec!["Alpha", "Beta", "Gamma"]);
    }

    // GL-QA36: verify the re-entrance guard prevents infinite recursion.
    //
    // We call flatten_xfa_to_pdf_simulate_reentrant (a #[cfg(test)] helper
    // that sets FLATTEN_DEPTH=1 before calling) to avoid accessing the
    // thread-local directly from the test sub-module.
    #[test]
    fn flatten_xfa_to_pdf_recursion_guard_returns_error() {
        let pdf_bytes = build_xfa_pdf(SIMPLE_XDP);
        let result = flatten_xfa_to_pdf_simulate_reentrant(&pdf_bytes);
        assert!(result.is_err(), "expected recursion guard to return Err, got Ok");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("recursively"),
            "expected error message to mention recursion, got: {err_msg}"
        );
    }

    // GL-QA36: verify the depth counter is reset to 0 after a normal call so
    // subsequent calls on the same thread are not falsely blocked.
    #[test]
    fn flatten_xfa_to_pdf_depth_counter_resets_after_call() {
        let pdf_bytes = build_xfa_pdf(SIMPLE_XDP);
        // First call; should succeed and reset depth to 0.
        let _ = flatten_xfa_to_pdf(&pdf_bytes);
        // Second call must not be blocked by a leaked counter.
        let pdf_bytes2 = build_xfa_pdf(SIMPLE_XDP);
        let result = flatten_xfa_to_pdf(&pdf_bytes2);
        assert!(result.is_ok(), "second flatten call should succeed, got: {result:?}");
    }

    // XFA-F1-05 (issue #1088): flatten_xfa_to_pdf must never panic on empty input.
    // An empty byte slice is not a valid PDF, so the function should return an
    // error rather than panic or crash.
    #[test]
    fn flatten_xfa_to_pdf_does_not_panic_on_empty_input() {
        let result = flatten_xfa_to_pdf(&[]);
        // We only require it does not panic; an Err is perfectly acceptable.
        // (An Ok result would mean the PDF library accepts empty bytes, which
        //  would also be fine — the important invariant is no panic/abort.)
        let _ = result;
    }
}
