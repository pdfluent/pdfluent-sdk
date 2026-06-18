//! Wave 2 browser-SDK edits: pages, forms, annotations, watermark, redaction,
//! compression. All operations take the current document bytes and return a
//! new `Uint8Array` with the result, leaving the original handle unchanged.
//!
//! Pattern follows the existing `merge` / `convertToPdfa` methods in `lib.rs`:
//! load fresh `lopdf::Document` from `self.pdf.data()`, apply the mutation,
//! call `save_to(&mut buf)`, return `buf`.

use wasm_bindgen::prelude::*;

use crate::pdfluent_error::{code, legacy_code, pdfluent_error};
use crate::PdfDoc;

// ---------- helpers ----------------------------------------------------------

/// Build a typed `PdfluentError` for a stateless `PdfDoc` edit.
///
/// Mirrors the `wasm_err_*` helpers in `lib.rs` so JS consumers get a stable
/// `code` / `operation` / `legacyCode` instead of an untyped `Error`. `c8` is
/// the canonical `E-<CATEGORY>-<SPECIFIC>` catalogue code; `legacy` is the
/// SCREAMING_SNAKE_CASE identifier kept for backward compatibility.
fn err(operation: &str, c8: &str, legacy: &str, message: &str) -> JsValue {
    pdfluent_error(operation, c8, legacy, message, "")
}

fn load_doc(src: &PdfDoc) -> Result<lopdf::Document, JsValue> {
    let bytes = src.pdf.data().as_ref();
    lopdf::Document::load_mem(bytes).map_err(|e| {
        err(
            "PdfDoc.open",
            code::PARSE_INVALID_PDF,
            legacy_code::INVALID_PDF,
            &format!("load failed: {e}"),
        )
    })
}

fn save_doc(doc: &mut lopdf::Document) -> Result<Vec<u8>, JsValue> {
    let mut buf = Vec::new();
    doc.save_to(&mut buf).map_err(|e| {
        err(
            "PdfDoc.save",
            code::WASM_SAVE_FAILED,
            legacy_code::SAVE_FAILED,
            &format!("save failed: {e}"),
        )
    })?;
    Ok(buf)
}

fn one_based_pages(zero_based: &[u32]) -> Vec<u32> {
    zero_based.iter().map(|p| p.saturating_add(1)).collect()
}

// ---------- WasmPdfDoc edits ------------------------------------------------

#[wasm_bindgen]
impl PdfDoc {
    // ------ Compress (stream optimisation) ----------------------------------

    /// Re-encode all content streams with deflate compression. Removes
    /// no-op overhead. Returns the optimised PDF as a `Uint8Array`.
    #[wasm_bindgen(js_name = "compress")]
    pub fn compress(&self) -> Result<Vec<u8>, JsValue> {
        let mut doc = load_doc(self)?;
        pdf_manip::optimize::compress_streams(&mut doc).map_err(|e| {
            err(
                "PdfDoc.compress",
                code::INTERNAL,
                legacy_code::OPERATION_FAILED,
                &format!("compress failed: {e}"),
            )
        })?;
        save_doc(&mut doc)
    }

    // ------ Pages -----------------------------------------------------------

    /// Delete the listed pages (0-based) from the document.
    #[wasm_bindgen(js_name = "deletePages")]
    pub fn delete_pages(&self, pages: &[u32]) -> Result<Vec<u8>, JsValue> {
        let mut doc = load_doc(self)?;
        let one_based = one_based_pages(pages);
        pdf_manip::pages::delete_pages(&mut doc, &one_based).map_err(|e| {
            err(
                "PdfDoc.deletePages",
                code::INTERNAL,
                legacy_code::OPERATION_FAILED,
                &format!("deletePages failed: {e}"),
            )
        })?;
        save_doc(&mut doc)
    }

    /// Rotate a single page by 90, 180, or 270 degrees (clockwise).
    /// `degrees` may be negative; values are normalised modulo 360.
    #[wasm_bindgen(js_name = "rotatePage")]
    pub fn rotate_page(&self, page_index: u32, degrees: i32) -> Result<Vec<u8>, JsValue> {
        let normalised = degrees.rem_euclid(360);
        if normalised % 90 != 0 {
            return Err(err(
                "PdfDoc.rotatePage",
                code::WASM_INVALID_ARGUMENT,
                legacy_code::INVALID_ARGUMENT,
                "rotatePage: degrees must be a multiple of 90 (received non-orthogonal value)",
            ));
        }
        let mut doc = load_doc(self)?;
        pdf_manip::pages::rotate_page(&mut doc, page_index.saturating_add(1), normalised as i64)
            .map_err(|e| {
                err(
                    "PdfDoc.rotatePage",
                    code::INTERNAL,
                    legacy_code::OPERATION_FAILED,
                    &format!("rotatePage failed: {e}"),
                )
            })?;
        save_doc(&mut doc)
    }

    /// Re-order pages. `new_order` is a permutation of 0..pageCount.
    #[wasm_bindgen(js_name = "reorderPages")]
    pub fn reorder_pages(&self, new_order: &[u32]) -> Result<Vec<u8>, JsValue> {
        let doc = load_doc(self)?;
        let one_based = one_based_pages(new_order);
        let new_doc = pdf_manip::pages::rearrange_pages(&doc, &one_based).map_err(|e| {
            err(
                "PdfDoc.reorderPages",
                code::INTERNAL,
                legacy_code::OPERATION_FAILED,
                &format!("reorderPages failed: {e}"),
            )
        })?;
        let mut new_doc = new_doc;
        save_doc(&mut new_doc)
    }

    /// Extract the listed pages (0-based) into a new document.
    /// Used to implement "split" workflows in the browser.
    #[wasm_bindgen(js_name = "extractPages")]
    pub fn extract_pages(&self, pages: &[u32]) -> Result<Vec<u8>, JsValue> {
        let doc = load_doc(self)?;
        let one_based = one_based_pages(pages);
        let mut sub = pdf_manip::pages::extract_pages(&doc, &one_based).map_err(|e| {
            err(
                "PdfDoc.extractPages",
                code::INTERNAL,
                legacy_code::OPERATION_FAILED,
                &format!("extractPages failed: {e}"),
            )
        })?;
        save_doc(&mut sub)
    }

    // ------ Text watermark --------------------------------------------------

    /// Apply a diagonal text watermark to every page.
    /// `opacity` in 0.0..=1.0. Default rotation 45 deg, default colour gray.
    #[wasm_bindgen(js_name = "addTextWatermark")]
    pub fn add_text_watermark(&self, text: &str, opacity: f32) -> Result<Vec<u8>, JsValue> {
        if text.is_empty() {
            return Err(err(
                "PdfDoc.addTextWatermark",
                code::WASM_INVALID_ARGUMENT,
                legacy_code::INVALID_ARGUMENT,
                "addTextWatermark: text must be non-empty",
            ));
        }
        if !(0.0..=1.0).contains(&opacity) {
            return Err(err(
                "PdfDoc.addTextWatermark",
                code::WASM_INVALID_ARGUMENT,
                legacy_code::INVALID_ARGUMENT,
                "addTextWatermark: opacity must be in 0.0..=1.0",
            ));
        }
        use pdf_manip::watermark::{
            apply_text_watermark, Color, Layer, PageSelection, Position, TextWatermark,
        };

        let mut doc = load_doc(self)?;
        let wm = TextWatermark {
            text: text.to_string(),
            font_size: 48.0,
            rotation: 45.0,
            opacity,
            color: Color::Gray(0.5),
            position: Position::Center,
            layer: Layer::Foreground,
        };
        apply_text_watermark(&mut doc, &wm, &PageSelection::All).map_err(|e| {
            err(
                "PdfDoc.addTextWatermark",
                code::INTERNAL,
                legacy_code::OPERATION_FAILED,
                &format!("addTextWatermark failed: {e}"),
            )
        })?;
        save_doc(&mut doc)
    }

    // ------ AcroForm writes -------------------------------------------------

    /// Set a single AcroForm **text** field value. Returns the new PDF bytes.
    ///
    /// Updates `/V` (ASCII literal or UTF-16BE+BOM), per-widget `/AS`, and
    /// regenerates the `/AP` appearance stream so the fill is visible without
    /// `/NeedAppearances` processing. Read-only fields throw.
    ///
    /// **Text fields only.** For checkbox, radio, and choice fields use
    /// `PdfDocMut.setFormField`, which runs the full type-aware dispatch.
    #[wasm_bindgen(js_name = "setFormField")]
    pub fn set_form_field(&self, path: &str, value: &str) -> Result<Vec<u8>, JsValue> {
        let bytes = self.pdf.data().as_ref().to_vec();
        let mut doc = pdfluent::PdfDocument::from_bytes(&bytes).map_err(|e| {
            err(
                "PdfDoc.setFormField",
                code::PARSE_INVALID_PDF,
                legacy_code::INVALID_PDF,
                &format!("open failed: {e}"),
            )
        })?;
        {
            let mut form = doc.form_mut();
            form.set_text(path, value).map_err(|e| {
                err(
                    "PdfDoc.setFormField",
                    code::INTERNAL,
                    legacy_code::OPERATION_FAILED,
                    &format!("setFormField failed: {e}"),
                )
            })?;
        }
        doc.to_bytes().map_err(|e| {
            err(
                "PdfDoc.setFormField",
                code::WASM_SAVE_FAILED,
                legacy_code::SAVE_FAILED,
                &format!("save failed: {e}"),
            )
        })
    }

    /// Bulk-set multiple AcroForm **text** fields from a JSON object
    /// `{"field.path": "value", ...}`. Returns the new PDF bytes.
    #[wasm_bindgen(js_name = "setFormFields")]
    pub fn set_form_fields(&self, fields_json: &str) -> Result<Vec<u8>, JsValue> {
        let parsed: std::collections::BTreeMap<String, String> = serde_json::from_str(fields_json)
            .map_err(|e| {
                err(
                    "PdfDoc.setFormFields",
                    code::WASM_INVALID_JSON,
                    legacy_code::INVALID_JSON,
                    &format!("setFormFields: invalid JSON: {e}"),
                )
            })?;
        let bytes = self.pdf.data().as_ref().to_vec();
        let mut doc = pdfluent::PdfDocument::from_bytes(&bytes).map_err(|e| {
            err(
                "PdfDoc.setFormFields",
                code::PARSE_INVALID_PDF,
                legacy_code::INVALID_PDF,
                &format!("open failed: {e}"),
            )
        })?;
        {
            let mut form = doc.form_mut();
            for (path, value) in &parsed {
                form.set_text(path, value).map_err(|e| {
                    err(
                        "PdfDoc.setFormFields",
                        code::INTERNAL,
                        legacy_code::OPERATION_FAILED,
                        &format!("setFormFields ({path}): {e}"),
                    )
                })?;
            }
        }
        doc.to_bytes().map_err(|e| {
            err(
                "PdfDoc.setFormFields",
                code::WASM_SAVE_FAILED,
                legacy_code::SAVE_FAILED,
                &format!("save failed: {e}"),
            )
        })
    }

    // ------ Annotations (feature = "annotate") ------------------------------

    /// Add a highlight annotation over the given page rectangle.
    /// Rectangle in PDF user-space coordinates (origin bottom-left).
    /// Optional `color_hex` like `"#ffeb3b"`; default yellow if `None`.
    #[cfg(feature = "annotate")]
    #[wasm_bindgen(js_name = "addHighlight")]
    pub fn add_highlight(
        &self,
        page_index: u32,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        color_hex: Option<String>,
    ) -> Result<Vec<u8>, JsValue> {
        use pdf_annot::builder::{add_annotation_to_page, AnnotRect, AnnotationBuilder};

        let (r, g, b) = parse_color_hex(color_hex.as_deref()).unwrap_or((1.0, 0.92, 0.23));
        let rect = AnnotRect::new(x, y, x + w, y + h);
        let mut doc = load_doc(self)?;
        let annot_id = AnnotationBuilder::highlight(rect)
            .color(r, g, b)
            .opacity(0.4)
            .build(&mut doc)
            .map_err(|e| {
                err(
                    "PdfDoc.addHighlight",
                    code::INTERNAL,
                    legacy_code::OPERATION_FAILED,
                    &format!("addHighlight build failed: {e}"),
                )
            })?;
        add_annotation_to_page(&mut doc, page_index.saturating_add(1), annot_id).map_err(|e| {
            err(
                "PdfDoc.addHighlight",
                code::INTERNAL,
                legacy_code::OPERATION_FAILED,
                &format!("addHighlight attach failed: {e}"),
            )
        })?;
        save_doc(&mut doc)
    }

    /// Add a sticky-note (text) annotation at the given page position.
    #[cfg(feature = "annotate")]
    #[wasm_bindgen(js_name = "addStickyNote")]
    pub fn add_sticky_note(
        &self,
        page_index: u32,
        x: f64,
        y: f64,
        contents: &str,
    ) -> Result<Vec<u8>, JsValue> {
        use pdf_annot::builder::{add_annotation_to_page, AnnotRect, AnnotationBuilder, TextIcon};

        // Sticky-notes are conventionally rendered as a 24x24pt icon. We give
        // the underlying rect the same dimensions; the icon decides the look.
        let rect = AnnotRect::new(x, y, x + 24.0, y + 24.0);
        let mut doc = load_doc(self)?;
        let annot_id = AnnotationBuilder::sticky_note(rect, TextIcon::Comment)
            .contents(contents)
            .build(&mut doc)
            .map_err(|e| {
                err(
                    "PdfDoc.addStickyNote",
                    code::INTERNAL,
                    legacy_code::OPERATION_FAILED,
                    &format!("addStickyNote build failed: {e}"),
                )
            })?;
        add_annotation_to_page(&mut doc, page_index.saturating_add(1), annot_id).map_err(|e| {
            err(
                "PdfDoc.addStickyNote",
                code::INTERNAL,
                legacy_code::OPERATION_FAILED,
                &format!("addStickyNote attach failed: {e}"),
            )
        })?;
        save_doc(&mut doc)
    }

    /// Add a free-text (callout-style) annotation at the given page rectangle.
    #[cfg(feature = "annotate")]
    #[wasm_bindgen(js_name = "addFreeText")]
    pub fn add_free_text(
        &self,
        page_index: u32,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        contents: &str,
    ) -> Result<Vec<u8>, JsValue> {
        use pdf_annot::builder::{add_annotation_to_page, AnnotRect, AnnotationBuilder};

        let rect = AnnotRect::new(x, y, x + w, y + h);
        let mut doc = load_doc(self)?;
        let annot_id = AnnotationBuilder::free_text(rect, contents, 12.0)
            .build(&mut doc)
            .map_err(|e| {
                err(
                    "PdfDoc.addFreeText",
                    code::INTERNAL,
                    legacy_code::OPERATION_FAILED,
                    &format!("addFreeText build failed: {e}"),
                )
            })?;
        add_annotation_to_page(&mut doc, page_index.saturating_add(1), annot_id).map_err(|e| {
            err(
                "PdfDoc.addFreeText",
                code::INTERNAL,
                legacy_code::OPERATION_FAILED,
                &format!("addFreeText attach failed: {e}"),
            )
        })?;
        save_doc(&mut doc)
    }

    // ------ Redaction -------------------------------------------------------

    /// Permanently remove content within the given page rectangle.
    /// Coordinates in PDF user-space (origin bottom-left).
    #[wasm_bindgen(js_name = "redactRegion")]
    pub fn redact_region(
        &self,
        page_index: u32,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
    ) -> Result<Vec<u8>, JsValue> {
        let mut doc = load_doc(self)?;
        let area =
            pdf_redact::RedactionArea::new(page_index.saturating_add(1), [x, y, x + w, y + h]);
        let mut redactor = pdf_redact::Redactor::new();
        redactor.mark(area);
        redactor.apply(&mut doc).map_err(|e| {
            err(
                "PdfDoc.redactRegion",
                code::INTERNAL,
                legacy_code::OPERATION_FAILED,
                &format!("redactRegion failed: {e}"),
            )
        })?;
        save_doc(&mut doc)
    }

    /// Search-and-redact: every match of `query` (literal text, case-sensitive)
    /// is permanently removed from every page.
    #[wasm_bindgen(js_name = "redactSearch")]
    pub fn redact_search(&self, query: &str) -> Result<Vec<u8>, JsValue> {
        if query.is_empty() {
            return Err(err(
                "PdfDoc.redactSearch",
                code::WASM_INVALID_ARGUMENT,
                legacy_code::INVALID_ARGUMENT,
                "redactSearch: query must be non-empty",
            ));
        }
        let mut doc = load_doc(self)?;
        let opts = pdf_redact::RedactSearchOptions::default();
        pdf_redact::search_and_redact(&mut doc, query, &opts).map_err(|e| {
            err(
                "PdfDoc.redactSearch",
                code::INTERNAL,
                legacy_code::OPERATION_FAILED,
                &format!("redactSearch failed: {e}"),
            )
        })?;
        save_doc(&mut doc)
    }
}

// ---------- internal helpers ------------------------------------------------

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
