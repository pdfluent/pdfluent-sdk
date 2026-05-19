//! `PdfDocMut` — a stateful editing handle for the PDFluent browser SDK.
//!
//! Holds one mutable `lopdf::Document` for the lifetime of the editing
//! session. All mutations operate in place; `save()` serialises once at
//! the end. Compared to the stateless mutations on `PdfDoc`, an editor
//! session with N operations costs **1 parse + 1 serialise** rather than
//! N parses + N serialises.
//!
//! See `benchmarks/runs/wasm_sdk_dx/ROUND1_PDFDOCMUT_API_DESIGN.md` for
//! the full design.

use std::collections::BTreeMap;

use lopdf::Document as LopdfDocument;
use pdf_manip::text_run::extract_page_text_runs;
use pdf_manip::text_style::{set_text_run_style, StateIsolationStrategy, StyleResult};
use pdf_text_format::{
    format_text_run, FormatResult, StateIsolationStrategy as FormatIsolation, TextRunLocator,
};
use wasm_bindgen::prelude::*;

// ---------- the handle -----------------------------------------------------

/// Stateful editing handle.
///
/// Construct with [`PdfDocMut::open`], apply any sequence of mutations
/// (all `&mut self`), and finally call [`PdfDocMut::save`] to get the
/// modified PDF bytes. `save` is non-consuming; you can continue
/// editing after taking a snapshot.
#[wasm_bindgen]
pub struct PdfDocMut {
    doc: LopdfDocument,
}

// ---- G6: Text style result type -------------------------------------------

#[derive(serde::Serialize)]
struct StyleResultJs {
    #[serde(rename = "bytesChanged")]
    bytes_changed: usize,
    #[serde(rename = "originalResourceName")]
    original_resource_name: String,
    #[serde(rename = "variantResourceName")]
    variant_resource_name: String,
    #[serde(rename = "originalFontName")]
    original_font_name: String,
    #[serde(rename = "requestedFontName")]
    requested_font_name: String,
    #[serde(rename = "isolationStrategy")]
    isolation_strategy: String,
}

fn style_result_to_js(r: &StyleResult) -> StyleResultJs {
    StyleResultJs {
        bytes_changed: r.bytes_changed,
        original_resource_name: r.font_swap.original_resource_name.clone(),
        variant_resource_name: r.font_swap.variant_resource_name.clone(),
        original_font_name: r.original_font_name.clone(),
        requested_font_name: r.requested_font_name.clone(),
        isolation_strategy: match &r.isolation_strategy {
            StateIsolationStrategy::Direct => "Direct".to_string(),
            StateIsolationStrategy::Restore { restore_op_index } => {
                format!("Restore({restore_op_index})")
            }
        },
    }
}

#[wasm_bindgen]
impl PdfDocMut {
    /// Open a PDF for editing.
    pub fn open(bytes: &[u8]) -> Result<PdfDocMut, JsError> {
        let doc = LopdfDocument::load_mem(bytes)
            .map_err(|e| JsError::new(&format!("open failed: {e}")))?;
        Ok(PdfDocMut { doc })
    }

    /// Number of pages in the current document state.
    #[wasm_bindgen(js_name = "pageCount")]
    pub fn page_count(&self) -> usize {
        self.doc.get_pages().len()
    }

    /// Serialise the current document state to a `Uint8Array`.
    ///
    /// Non-consuming: subsequent mutations are still possible on the
    /// same handle.
    pub fn save(&self) -> Result<Vec<u8>, JsError> {
        let mut buf = Vec::new();
        // lopdf save_to takes &mut Document; clone the doc structure to
        // keep save() non-consuming on &self.
        let mut clone = self.doc.clone();
        clone
            .save_to(&mut buf)
            .map_err(|e| JsError::new(&format!("save failed: {e}")))?;
        Ok(buf)
    }

    // ------ Pages ----------------------------------------------------------

    /// Delete the listed pages (0-based) from the document.
    #[wasm_bindgen(js_name = "deletePages")]
    pub fn delete_pages(&mut self, pages: &[u32]) -> Result<(), JsError> {
        let one_based = one_based(pages);
        pdf_manip::pages::delete_pages(&mut self.doc, &one_based)
            .map_err(|e| JsError::new(&format!("deletePages failed: {e}")))?;
        Ok(())
    }

    /// Rotate a single page by 90/180/270 (or negative). Multiples of 90 only.
    #[wasm_bindgen(js_name = "rotatePage")]
    pub fn rotate_page(&mut self, page_index: u32, degrees: i32) -> Result<(), JsError> {
        let normalised = degrees.rem_euclid(360);
        if normalised % 90 != 0 {
            return Err(JsError::new("rotatePage: degrees must be a multiple of 90"));
        }
        pdf_manip::pages::rotate_page(
            &mut self.doc,
            page_index.saturating_add(1),
            normalised as i64,
        )
        .map_err(|e| JsError::new(&format!("rotatePage failed: {e}")))?;
        Ok(())
    }

    /// Re-order pages. `new_order` is a permutation of `0..page_count`.
    #[wasm_bindgen(js_name = "reorderPages")]
    pub fn reorder_pages(&mut self, new_order: &[u32]) -> Result<(), JsError> {
        let one_based = one_based(new_order);
        let reordered = pdf_manip::pages::rearrange_pages(&self.doc, &one_based)
            .map_err(|e| JsError::new(&format!("reorderPages failed: {e}")))?;
        self.doc = reordered;
        Ok(())
    }

    /// Extract the listed pages (0-based) into a **new** document and
    /// return its bytes. Leaves the current editing session untouched.
    ///
    /// Use for split-style workflows where the original keeps the full
    /// set of pages and a subset is exported.
    #[wasm_bindgen(js_name = "extractPages")]
    pub fn extract_pages(&self, pages: &[u32]) -> Result<Vec<u8>, JsError> {
        let one_based = one_based(pages);
        let mut sub = pdf_manip::pages::extract_pages(&self.doc, &one_based)
            .map_err(|e| JsError::new(&format!("extractPages failed: {e}")))?;
        let mut buf = Vec::new();
        sub.save_to(&mut buf)
            .map_err(|e| JsError::new(&format!("extractPages save failed: {e}")))?;
        Ok(buf)
    }

    // ------ Text watermark -------------------------------------------------

    /// Apply a diagonal text watermark to every page.
    #[wasm_bindgen(js_name = "addTextWatermark")]
    pub fn add_text_watermark(&mut self, text: &str, opacity: f32) -> Result<(), JsError> {
        if text.is_empty() {
            return Err(JsError::new("addTextWatermark: text must be non-empty"));
        }
        if !(0.0..=1.0).contains(&opacity) {
            return Err(JsError::new(
                "addTextWatermark: opacity must be in 0.0..=1.0",
            ));
        }
        use pdf_manip::watermark::{
            apply_text_watermark, Color, Layer, PageSelection, Position, TextWatermark,
        };
        let wm = TextWatermark {
            text: text.to_string(),
            font_size: 48.0,
            rotation: 45.0,
            opacity,
            color: Color::Gray(0.5),
            position: Position::Center,
            layer: Layer::Foreground,
        };
        apply_text_watermark(&mut self.doc, &wm, &PageSelection::All)
            .map_err(|e| JsError::new(&format!("addTextWatermark failed: {e}")))?;
        Ok(())
    }

    // ------ Forms ----------------------------------------------------------

    /// Set a single AcroForm text-field value.
    ///
    /// Note: pdfluent's high-level `PdfFormMut` requires a
    /// `pdfluent::PdfDocument`. To stay in-place on this handle, we use
    /// the lower-level lopdf walk via the `pdf-forms` engine. For 1.0,
    /// this supports flat top-level field hierarchies (no `/Kids`
    /// recursion) — same constraint as the Rust core.
    #[wasm_bindgen(js_name = "setFormField")]
    pub fn set_form_field(&mut self, path: &str, value: &str) -> Result<(), JsError> {
        set_text_field_inplace(&mut self.doc, path, value)
    }

    /// Bulk-set multiple AcroForm text fields from a JSON object
    /// `{"field.path": "value", ...}`.
    #[wasm_bindgen(js_name = "setFormFields")]
    pub fn set_form_fields(&mut self, fields_json: &str) -> Result<(), JsError> {
        let parsed: BTreeMap<String, String> = serde_json::from_str(fields_json)
            .map_err(|e| JsError::new(&format!("setFormFields: invalid JSON: {e}")))?;
        for (path, value) in &parsed {
            // Cannot format JsError directly; rewrap with the field name
            // for context.
            if let Err(e) = set_text_field_inplace(&mut self.doc, path, value) {
                let _ = e; // discard the WASM JsError to keep type clean
                return Err(JsError::new(&format!(
                    "setFormFields ({path}): field not writable"
                )));
            }
        }
        Ok(())
    }

    // ------ Annotations ----------------------------------------------------

    /// Add a highlight annotation over the given page rectangle.
    /// `color_hex` like `"#ffeb3b"` — defaults to yellow when `None`.
    #[cfg(feature = "annotate")]
    #[wasm_bindgen(js_name = "addHighlight")]
    pub fn add_highlight(
        &mut self,
        page_index: u32,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        color_hex: Option<String>,
    ) -> Result<(), JsError> {
        use pdf_annot::builder::{add_annotation_to_page, AnnotRect, AnnotationBuilder};
        let (r, g, b) = parse_color_hex(color_hex.as_deref()).unwrap_or((1.0, 0.92, 0.23));
        let rect = AnnotRect::new(x, y, x + w, y + h);
        let annot_id = AnnotationBuilder::highlight(rect)
            .color(r, g, b)
            .opacity(0.4)
            .build(&mut self.doc)
            .map_err(|e| JsError::new(&format!("addHighlight build failed: {e}")))?;
        add_annotation_to_page(&mut self.doc, page_index.saturating_add(1), annot_id)
            .map_err(|e| JsError::new(&format!("addHighlight attach failed: {e}")))?;
        Ok(())
    }

    /// Add a sticky-note annotation at the given page position.
    #[cfg(feature = "annotate")]
    #[wasm_bindgen(js_name = "addStickyNote")]
    pub fn add_sticky_note(
        &mut self,
        page_index: u32,
        x: f64,
        y: f64,
        contents: &str,
    ) -> Result<(), JsError> {
        use pdf_annot::builder::{add_annotation_to_page, AnnotRect, AnnotationBuilder, TextIcon};
        let rect = AnnotRect::new(x, y, x + 24.0, y + 24.0);
        let annot_id = AnnotationBuilder::sticky_note(rect, TextIcon::Comment)
            .contents(contents)
            .build(&mut self.doc)
            .map_err(|e| JsError::new(&format!("addStickyNote build failed: {e}")))?;
        add_annotation_to_page(&mut self.doc, page_index.saturating_add(1), annot_id)
            .map_err(|e| JsError::new(&format!("addStickyNote attach failed: {e}")))?;
        Ok(())
    }

    /// Add a free-text annotation at the given page rectangle.
    #[cfg(feature = "annotate")]
    #[wasm_bindgen(js_name = "addFreeText")]
    pub fn add_free_text(
        &mut self,
        page_index: u32,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        contents: &str,
    ) -> Result<(), JsError> {
        use pdf_annot::builder::{add_annotation_to_page, AnnotRect, AnnotationBuilder};
        let rect = AnnotRect::new(x, y, x + w, y + h);
        let annot_id = AnnotationBuilder::free_text(rect, contents, 12.0)
            .build(&mut self.doc)
            .map_err(|e| JsError::new(&format!("addFreeText build failed: {e}")))?;
        add_annotation_to_page(&mut self.doc, page_index.saturating_add(1), annot_id)
            .map_err(|e| JsError::new(&format!("addFreeText attach failed: {e}")))?;
        Ok(())
    }

    // ------ Redaction ------------------------------------------------------

    /// Permanently remove content within the given page rectangle.
    #[wasm_bindgen(js_name = "redactRegion")]
    pub fn redact_region(
        &mut self,
        page_index: u32,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    ) -> Result<(), JsError> {
        let area =
            pdf_redact::RedactionArea::new(page_index.saturating_add(1), [x, y, x + w, y + h]);
        let mut redactor = pdf_redact::Redactor::new();
        redactor.mark(area);
        redactor
            .apply(&mut self.doc)
            .map_err(|e| JsError::new(&format!("redactRegion failed: {e}")))?;
        Ok(())
    }

    /// Search-and-redact: every literal match of `query` is permanently
    /// removed from every page.
    #[wasm_bindgen(js_name = "redactSearch")]
    pub fn redact_search(&mut self, query: &str) -> Result<(), JsError> {
        if query.is_empty() {
            return Err(JsError::new("redactSearch: query must be non-empty"));
        }
        let opts = pdf_redact::RedactSearchOptions::default();
        pdf_redact::search_and_redact(&mut self.doc, query, &opts)
            .map_err(|e| JsError::new(&format!("redactSearch failed: {e}")))?;
        Ok(())
    }

    // ------ Compress -------------------------------------------------------

    /// Re-encode content streams with deflate compression.
    pub fn compress(&mut self) -> Result<(), JsError> {
        pdf_manip::optimize::compress_streams(&mut self.doc)
            .map_err(|e| JsError::new(&format!("compress failed: {e}")))?;
        Ok(())
    }

    // ---- G6: Text style editing ----------------------------------------

    /// Apply bold/italic style to a text run on `page_num` (1-based).
    #[wasm_bindgen(js_name = "setTextRunStyle")]
    pub fn set_text_run_style_js(
        &mut self,
        page_num: u32,
        runs_json: &str,
        run_index: usize,
        bold: Option<bool>,
        italic: Option<bool>,
    ) -> Result<String, JsError> {
        let runs = extract_page_text_runs(&self.doc, page_num)
            .map_err(|e| JsError::new(&format!("extract runs: {e}")))?;
        let run = runs.get(run_index).ok_or_else(|| {
            JsError::new(&format!(
                "run index {run_index} out of range (page has {} runs)",
                runs.len()
            ))
        })?;
        let _ = runs_json;
        let result = set_text_run_style(&mut self.doc, page_num, run, bold, italic)
            .map_err(|e| JsError::new(&format!("{e}")))?;
        let js = style_result_to_js(&result);
        serde_json::to_string(&js).map_err(|e| JsError::new(&format!("serialize: {e}")))
    }

    /// Convenience: open bytes, apply style, return new bytes.
    #[wasm_bindgen(js_name = "applyTextRunStyle")]
    pub fn apply_text_run_style(
        data: &[u8],
        page_num: u32,
        run_index: usize,
        bold: Option<bool>,
        italic: Option<bool>,
    ) -> Result<Vec<u8>, JsError> {
        let mut handle = PdfDocMut::open(data)?;
        handle.set_text_run_style_js(page_num, "[]", run_index, bold, italic)?;
        handle.save()
    }

    // ---- WASM3: Text formatting (font size + fill color) -------------------

    /// Format a single text run on `page_num` (1-based) by changing its font
    /// size and/or fill color. The mutation is scoped so other runs on the
    /// page are unaffected (q…Q isolation injected as needed).
    ///
    /// Parameters:
    /// - `page_num`: 1-based page index.
    /// - `run_index`: 0-based index into `extract_page_text_runs(page_num)`.
    /// - `font_size`: optional new font size (points). Pass `None` to keep.
    /// - `color`: optional hex color string (e.g. `"#FF0000"`); pass `None`
    ///   to keep the existing fill color.
    ///
    /// Returns a JSON string with the canonical `FormatResult` shape:
    /// `{ "formatted": bool, "bytesChanged": usize, "isolationStrategy": str,
    ///   "originalSize": f32?, "originalColor": [f32; 3]?, "code": str? }`.
    ///
    /// Passing both `font_size = None` and `color = None` is a no-op:
    /// `formatted = false`, `bytesChanged = 0`, no error.
    #[wasm_bindgen(js_name = "formatTextSpan")]
    pub fn format_text_span_js(
        &mut self,
        page_num: u32,
        run_index: usize,
        font_size: Option<f32>,
        color: Option<String>,
    ) -> Result<String, JsError> {
        let runs = extract_page_text_runs(&self.doc, page_num)
            .map_err(|e| JsError::new(&format!("formatTextSpan: extract runs: {e}")))?;
        let run = runs.get(run_index).ok_or_else(|| {
            JsError::new(&format!(
                "formatTextSpan: run index {run_index} out of range (page has {} runs)",
                runs.len()
            ))
        })?;
        let locator = TextRunLocator::from_run(run);

        let color_rgb = match color.as_deref() {
            Some(hex) => Some(parse_color_hex_f32(Some(hex)).ok_or_else(|| {
                JsError::new(&format!(
                    "formatTextSpan: invalid color hex '{hex}' (expected '#RRGGBB')"
                ))
            })?),
            None => None,
        };

        let result = format_text_run(&mut self.doc, page_num, locator, font_size, color_rgb)
            .map_err(|e| JsError::new(&format!("formatTextSpan: {e}")))?;

        let js = format_result_to_js(&result);
        serde_json::to_string(&js)
            .map_err(|e| JsError::new(&format!("formatTextSpan: serialize: {e}")))
    }

    /// Convenience: open bytes, format a single run, return new bytes.
    ///
    /// One-shot equivalent of `open → formatTextSpan → save`.
    #[wasm_bindgen(js_name = "applyTextFormat")]
    pub fn apply_text_format(
        data: &[u8],
        page_num: u32,
        run_index: usize,
        font_size: Option<f32>,
        color: Option<String>,
    ) -> Result<Vec<u8>, JsError> {
        let mut handle = PdfDocMut::open(data)?;
        handle.format_text_span_js(page_num, run_index, font_size, color)?;
        handle.save()
    }
}

// ---- WASM3: FormatResult JSON projection ----------------------------------

#[derive(serde::Serialize)]
struct FormatResultJs {
    /// `true` when at least one operator was injected; `false` for a no-op.
    formatted: bool,
    #[serde(rename = "bytesChanged")]
    bytes_changed: usize,
    /// `"NoIsolation"` / `"AddQGroup"` / `"ReuseExistingQGroup"`.
    #[serde(rename = "isolationStrategy")]
    isolation_strategy: &'static str,
    /// Pre-format font size (points), if a preceding `Tf` was found.
    #[serde(rename = "originalSize", skip_serializing_if = "Option::is_none")]
    original_size: Option<f32>,
    /// Pre-format fill color `[r, g, b]` in `0.0..=1.0`, if resolvable.
    #[serde(rename = "originalColor", skip_serializing_if = "Option::is_none")]
    original_color: Option<[f32; 3]>,
}

fn format_result_to_js(r: &FormatResult) -> FormatResultJs {
    FormatResultJs {
        formatted: r.bytes_changed > 0,
        bytes_changed: r.bytes_changed,
        isolation_strategy: match r.state_isolation_strategy {
            FormatIsolation::NoIsolation => "NoIsolation",
            FormatIsolation::AddQGroup => "AddQGroup",
            FormatIsolation::ReuseExistingQGroup => "ReuseExistingQGroup",
        },
        original_size: r.original_size,
        original_color: r.original_color,
    }
}

/// Parse `"#RRGGBB"` into `[r, g, b]` in `0.0..=1.0`.
fn parse_color_hex_f32(hex: Option<&str>) -> Option<[f32; 3]> {
    let s = hex?.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some([
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
    ])
}

// ---------- helpers --------------------------------------------------------

fn one_based(zero_based: &[u32]) -> Vec<u32> {
    zero_based.iter().map(|p| p.saturating_add(1)).collect()
}

fn parse_color_hex(hex: Option<&str>) -> Option<(f64, f64, f64)> {
    let s = hex?.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some((r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0))
}

// ---------- in-place AcroForm text setter ---------------------------------
//
// We can't borrow pdfluent::PdfFormMut here because that wraps a
// pdfluent::PdfDocument, which would force us to round-trip bytes again.
// Instead we walk the AcroForm /Fields array directly and write /V on
// the matching field dict. Same 1.0 scope constraint as the Rust core:
// no /Kids recursion. The implementation mirrors pdfluent::form's
// `set_field_value` path but operates directly on our owned
// lopdf::Document.

fn set_text_field_inplace(
    doc: &mut lopdf::Document,
    name: &str,
    value: &str,
) -> Result<(), JsError> {
    use lopdf::{Object, ObjectId};

    // Resolve the /AcroForm/Fields array as references to field dicts.
    let catalog_id = doc
        .trailer
        .get(b"Root")
        .ok()
        .and_then(|o| match o {
            Object::Reference(id) => Some(*id),
            _ => None,
        })
        .ok_or_else(|| JsError::new("setFormField: catalog not found"))?;

    let acroform_id: ObjectId = {
        let catalog = doc
            .get_object(catalog_id)
            .map_err(|e| JsError::new(&format!("setFormField: catalog: {e}")))?;
        let catalog_dict = catalog
            .as_dict()
            .map_err(|e| JsError::new(&format!("setFormField: catalog not a dict: {e}")))?;
        let af = catalog_dict.get(b"AcroForm").map_err(|_| {
            JsError::new(&format!(
                "setFormField: field '{name}' not found (no AcroForm)"
            ))
        })?;
        match af {
            Object::Reference(id) => *id,
            _ => {
                return Err(JsError::new(
                    "setFormField: inline /AcroForm not supported (use indirect)",
                ))
            }
        }
    };

    let field_ids: Vec<ObjectId> = {
        let acroform = doc
            .get_object(acroform_id)
            .map_err(|e| JsError::new(&format!("setFormField: AcroForm: {e}")))?;
        let af_dict = acroform
            .as_dict()
            .map_err(|e| JsError::new(&format!("setFormField: AcroForm not a dict: {e}")))?;
        let fields = af_dict.get(b"Fields").map_err(|_| {
            JsError::new(&format!(
                "setFormField: field '{name}' not found (no /Fields)"
            ))
        })?;
        let arr = match fields {
            Object::Array(a) => a.clone(),
            Object::Reference(id) => {
                let obj = doc
                    .get_object(*id)
                    .map_err(|e| JsError::new(&format!("setFormField: /Fields ref: {e}")))?;
                match obj {
                    Object::Array(a) => a.clone(),
                    _ => return Err(JsError::new("setFormField: /Fields is not an array")),
                }
            }
            _ => return Err(JsError::new("setFormField: /Fields is not an array")),
        };
        arr.into_iter()
            .filter_map(|o| match o {
                Object::Reference(id) => Some(id),
                _ => None,
            })
            .collect()
    };

    // Find the matching top-level field by /T.
    let mut target: Option<ObjectId> = None;
    for fid in &field_ids {
        let obj = doc
            .get_object(*fid)
            .map_err(|e| JsError::new(&format!("setFormField: field: {e}")))?;
        if let Object::Dictionary(d) = obj {
            if let Ok(Object::String(bytes, _)) = d.get(b"T") {
                if bytes == name.as_bytes() {
                    target = Some(*fid);
                    break;
                }
            }
        }
    }
    let target_id =
        target.ok_or_else(|| JsError::new(&format!("setFormField: field '{name}' not found")))?;

    // Write /V on the target dict.
    let target_obj = doc
        .get_object_mut(target_id)
        .map_err(|e| JsError::new(&format!("setFormField: get_mut: {e}")))?;
    if let Object::Dictionary(d) = target_obj {
        d.set(
            b"V".to_vec(),
            Object::String(value.as_bytes().to_vec(), lopdf::StringFormat::Literal),
        );
        Ok(())
    } else {
        Err(JsError::new(&format!(
            "setFormField: target '{name}' is not a dict"
        )))
    }
}
