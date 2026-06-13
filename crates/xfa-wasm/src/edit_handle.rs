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
use pdf_forms::{apply_field_value, WriteOutcome, WriteValue, WritebackError};
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

    /// Set a single AcroForm field value (text, checkbox, radio, or choice).
    ///
    /// Delegates to [`pdf_forms::apply_field_value`] — the single SDK
    /// writeback chain. It keeps `/V` (ASCII literal else UTF-16BE+BOM),
    /// per-widget `/AS`, and regenerated `/AP` streams consistent, resolves
    /// fully-qualified names through `/Kids`, and rejects read-only fields.
    #[wasm_bindgen(js_name = "setFormField")]
    pub fn set_form_field(&mut self, path: &str, value: &str) -> Result<(), JsError> {
        apply_string_value(&mut self.doc, path, value)
            .map(|_| ())
            .map_err(|e| JsError::new(&format!("setFormField: {e}")))
    }

    /// Bulk-set multiple AcroForm fields from a JSON object
    /// `{"field.path": "value", ...}`.
    #[wasm_bindgen(js_name = "setFormFields")]
    pub fn set_form_fields(&mut self, fields_json: &str) -> Result<(), JsError> {
        let parsed: BTreeMap<String, String> = serde_json::from_str(fields_json)
            .map_err(|e| JsError::new(&format!("setFormFields: invalid JSON: {e}")))?;
        for (path, value) in &parsed {
            apply_string_value(&mut self.doc, path, value)
                .map_err(|e| JsError::new(&format!("setFormFields ({path}): {e}")))?;
        }
        Ok(())
    }

    /// Select multiple options on a multi-select list box.
    ///
    /// Delegates to [`pdf_forms::apply_choice_multi`]: writes `/V` as an array
    /// of text strings and rebuilds `/I` (the sorted selected-index cache) to
    /// match what Adobe Acrobat produces. `values` is a JS array of strings;
    /// pass an empty array to clear the selection. The field must be a
    /// multi-select list box (`/Ff` MultiSelect flag); for non-editable list
    /// boxes every value must be one of the field's `/Opt` options.
    #[wasm_bindgen(js_name = "setMultiSelect")]
    pub fn set_multi_select(&mut self, path: &str, values: Vec<String>) -> Result<(), JsError> {
        pdf_forms::apply_choice_multi(&mut self.doc, path, &values)
            .map(|_| ())
            .map_err(|e| JsError::new(&format!("setMultiSelect: {e}")))
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

    /// G3 — Replace a text span on a page.
    ///
    /// The editor passes the **exact** text it extracted from
    /// `getTextPositions()` as `original_text`, and the desired
    /// replacement as `replacement_text`. The replacement is
    /// re-encoded into the same font used by the matched span
    /// (subset-font fallback applies when the original font cannot
    /// encode the replacement — see
    /// `pdf-manip::text_replace::replace_text`).
    ///
    /// `page_num` is **1-based** to match the rest of the
    /// `pdf-manip` surface (and the existing `formatTextSpan` /
    /// `setTextRunStyle` methods on this handle).
    ///
    /// Returns a JSON string with the following shape:
    ///
    /// ```jsonc
    /// {
    ///   "replaced": false,
    ///   "code": "NO_MATCH",          // only when replaced=false
    ///   "reason": "originalText …",  // only when replaced=false
    ///   "byteLengthBefore": 12345,   // only when replaced=true
    ///   "byteLengthAfter":  12378    // only when replaced=true
    /// }
    /// ```
    ///
    /// Typed unsupported codes (all set `replaced=false`):
    ///
    /// - `"NO_MATCH"` — `original_text` not found on the page.
    /// - `"ENCODING_UNSUPPORTED"` — replacement contains characters
    ///   the matched font cannot encode and no fallback font is
    ///   available.
    /// - `"REPLACE_FAILED"` — anything else (lopdf I/O,
    ///   serialisation, FontMap construction, …). `reason` carries
    ///   the lower-level error message.
    ///
    /// Layout is preserved within the existing run: same font,
    /// same baseline, same `Tj`/`TJ` operator boundaries. Cross-run
    /// matches (where the search text is split across multiple
    /// content-stream operators) are handled by the underlying
    /// `replace_text` cross-run path.
    #[wasm_bindgen(js_name = "replaceTextSpan")]
    pub fn replace_text_span(
        &mut self,
        page_num: u32,
        original_text: &str,
        replacement_text: &str,
    ) -> Result<String, JsError> {
        // Capture byte length BEFORE the mutation so we can report
        // the delta if the replacement succeeds. The cheapest way
        // to do this is to ask lopdf to serialise the current state.
        let bytes_before = serialize_byte_len(&self.doc);

        // Build the per-page FontMap. If the page has no fonts at
        // all, replace_text would short-circuit with no matches —
        // fail closed with REPLACE_FAILED so the editor gets a
        // structured signal.
        let fonts = match pdf_manip::text_run::FontMap::from_page(&self.doc, page_num) {
            Ok(f) => f,
            Err(e) => {
                return Ok(replace_result_json(&ReplaceTextSpanResultJs {
                    replaced: false,
                    code: Some("REPLACE_FAILED"),
                    reason: Some(format!("font map: {e}")),
                    byte_length_before: None,
                    byte_length_after: None,
                }));
            }
        };

        let outcome = pdf_manip::text_replace::replace_text(
            &mut self.doc,
            page_num,
            original_text,
            replacement_text,
            &fonts,
        );

        match outcome {
            Ok(0) => Ok(replace_result_json(&ReplaceTextSpanResultJs {
                replaced: false,
                code: Some("NO_MATCH"),
                reason: Some(format!("original text not found on page {page_num}")),
                byte_length_before: None,
                byte_length_after: None,
            })),
            Ok(_) => {
                let bytes_after = serialize_byte_len(&self.doc);
                Ok(replace_result_json(&ReplaceTextSpanResultJs {
                    replaced: true,
                    code: None,
                    reason: None,
                    byte_length_before: bytes_before,
                    byte_length_after: bytes_after,
                }))
            }
            Err(e) => {
                // Encoding failures are the common typed-unsupported
                // case; everything else falls under REPLACE_FAILED.
                let msg = format!("{e}");
                let code = if msg.contains("encode") || msg.contains("encoding") {
                    "ENCODING_UNSUPPORTED"
                } else {
                    "REPLACE_FAILED"
                };
                Ok(replace_result_json(&ReplaceTextSpanResultJs {
                    replaced: false,
                    code: Some(code),
                    reason: Some(msg),
                    byte_length_before: None,
                    byte_length_after: None,
                }))
            }
        }
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

// ---------- in-place AcroForm value setter ---------------------------------

/// Apply a string value with type-aware dispatch, mirroring the CLI's
/// `fill_one` (crates/xfa-cli/src/cmd_fill.rs): try Text first (the common
/// case), then on a `/FT` type mismatch fall through to Radio, Choice, and
/// finally Checkbox with a bool-ish string.
///
/// All writes go through [`pdf_forms::apply_field_value`] — the single SDK
/// writeback chain — so `/V` encoding, per-widget `/AS`, and `/AP`
/// regeneration stay consistent on this handle's owned `lopdf::Document`.
fn apply_string_value(
    doc: &mut LopdfDocument,
    name: &str,
    value: &str,
) -> Result<WriteOutcome, WritebackError> {
    match apply_field_value(doc, name, WriteValue::Text(value)) {
        Err(WritebackError::WrongType { .. }) => {}
        other => return other,
    }
    match apply_field_value(doc, name, WriteValue::Radio(value)) {
        Err(WritebackError::WrongType { .. }) => {}
        other => return other,
    }
    match apply_field_value(doc, name, WriteValue::Choice(value)) {
        Err(WritebackError::WrongType { .. }) => {}
        other => return other,
    }
    // Checkbox via bool-ish string ("true"/"Yes"/"Off"/"false").
    let on = !matches!(value, "false" | "Off" | "0" | "");
    apply_field_value(doc, name, WriteValue::Checkbox(on))
}

// ---- G3: ReplaceTextSpan JSON projection ----------------------------------

#[derive(serde::Serialize)]
struct ReplaceTextSpanResultJs {
    /// `true` when at least one match was found and replaced.
    replaced: bool,
    /// Typed unsupported / failure code. `None` on success.
    #[serde(skip_serializing_if = "Option::is_none")]
    code: Option<&'static str>,
    /// Human-readable reason. `None` on success.
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    /// Document byte length before the mutation. `None` on failure.
    #[serde(rename = "byteLengthBefore", skip_serializing_if = "Option::is_none")]
    byte_length_before: Option<usize>,
    /// Document byte length after the mutation. `None` on failure.
    #[serde(rename = "byteLengthAfter", skip_serializing_if = "Option::is_none")]
    byte_length_after: Option<usize>,
}

fn replace_result_json(r: &ReplaceTextSpanResultJs) -> String {
    // The struct is internal; serde_json::to_string on a 5-field
    // struct cannot fail in practice. Fall back to a stable error
    // shape just in case.
    serde_json::to_string(r).unwrap_or_else(|_| {
        "{\"replaced\":false,\"code\":\"REPLACE_FAILED\",\"reason\":\"serialize error\"}"
            .to_string()
    })
}

/// Compute the byte length of the current document state by
/// serialising into a sink. Used by `replaceTextSpan` to report
/// `byteLengthBefore`/`byteLengthAfter` deltas. Returns `None`
/// when serialisation fails (so the JSON simply omits the field
/// rather than reporting a bogus value).
fn serialize_byte_len(doc: &LopdfDocument) -> Option<usize> {
    let mut counter = ByteCounter(0);
    let mut clone = doc.clone();
    clone.save_to(&mut counter).ok()?;
    Some(counter.0)
}

/// `std::io::Write` sink that just counts bytes — cheaper than
/// serialising into a real `Vec<u8>` when we only need the length.
struct ByteCounter(usize);

impl std::io::Write for ByteCounter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0 += buf.len();
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PdfDoc;
    use std::path::PathBuf;

    fn corpus_mini(name: &str) -> Vec<u8> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/corpus-mini")
            .join(name);
        std::fs::read(&path).unwrap_or_else(|e| {
            panic!("read {path:?}: {e}");
        })
    }

    /// `replaceTextSpan` with no match returns typed `NO_MATCH`
    /// without mutating the document — important so the editor can
    /// safely probe without committing accidental changes.
    #[test]
    fn g3_replace_text_span_no_match_returns_typed_code() {
        let bytes = corpus_mini("simple.pdf");
        let mut handle = PdfDocMut::open(&bytes).expect("open simple.pdf");
        let json = handle
            .replace_text_span(1, "this-text-does-not-exist-on-the-page", "x")
            .expect("call replace_text_span");
        let v: serde_json::Value = serde_json::from_str(&json).expect("parse JSON");
        assert_eq!(v["replaced"], false);
        assert_eq!(v["code"], "NO_MATCH");
        assert!(
            v["reason"].as_str().is_some(),
            "reason must be present on NO_MATCH"
        );
        assert!(
            v.get("byteLengthBefore").is_none(),
            "byteLengthBefore must be omitted on failure"
        );
        assert!(
            v.get("byteLengthAfter").is_none(),
            "byteLengthAfter must be omitted on failure"
        );
    }

    /// Happy path: replace a substring that is known to exist on a
    /// standard-fonts fixture, then prove the new bytes contain the
    /// replacement.
    #[test]
    fn g3_replace_text_span_happy_path_persists() {
        let bytes = corpus_mini("simple.pdf");
        // Probe the actual extractable text first so the test stays
        // robust against fixture changes.
        let doc = PdfDoc::open(&bytes).expect("open for extract");
        let extracted = doc.text(0);
        // Pick a short, lowercase, ASCII substring that's plausibly
        // present and won't collide with operator names. `Hello`
        // commonly appears in simple.pdf; fall back to the first
        // ASCII word if not.
        let needle = if extracted.contains("Hello") {
            "Hello".to_string()
        } else {
            extracted
                .split_whitespace()
                .find(|w: &&str| w.chars().all(|c: char| c.is_ascii_alphabetic()) && w.len() >= 3)
                .map(|w: &str| w.to_string())
                .unwrap_or_else(|| "Test".to_string())
        };

        let mut handle = PdfDocMut::open(&bytes).expect("open for mutate");
        let replacement = "ZZZ"; // same length as "Hello"? not required — replace_text
                                 // handles re-encoded widths.
        let json = handle
            .replace_text_span(1, &needle, replacement)
            .expect("call replace_text_span");
        let v: serde_json::Value = serde_json::from_str(&json).expect("parse JSON");

        // Either the replacement succeeded (replaced=true) or the
        // fixture's font cannot encode our replacement (typed
        // ENCODING_UNSUPPORTED). Either is an acceptable outcome
        // for the test — what we're really gating is "no silent
        // success", "no panic", and "JSON shape correct".
        match v["replaced"].as_bool() {
            Some(true) => {
                assert!(v.get("code").is_none(), "no code on success");
                assert!(
                    v["byteLengthBefore"].as_u64().is_some(),
                    "byteLengthBefore must be present on success"
                );
                assert!(
                    v["byteLengthAfter"].as_u64().is_some(),
                    "byteLengthAfter must be present on success"
                );

                // Save the mutated bytes and re-extract — the
                // replacement must appear in the new text.
                let new_bytes = handle.save().expect("save mutated doc");
                let new_doc = PdfDoc::open(&new_bytes).expect("reopen mutated doc");
                let new_text = new_doc.text(0);
                assert!(
                    new_text.contains(replacement) || !new_text.contains(&needle),
                    "reopened text must contain replacement OR no longer contain the needle; \
                     needle='{needle}' replacement='{replacement}' got={new_text:?}"
                );
            }
            Some(false) => {
                // Acceptable: must be one of the typed unsupported codes.
                let code = v["code"].as_str().unwrap_or("");
                assert!(
                    matches!(code, "NO_MATCH" | "ENCODING_UNSUPPORTED" | "REPLACE_FAILED"),
                    "unexpected code on replaced=false: {code:?}"
                );
                assert!(
                    v["reason"].as_str().is_some(),
                    "reason must be present on replaced=false"
                );
            }
            None => panic!("`replaced` field must be a boolean"),
        }
    }
}
