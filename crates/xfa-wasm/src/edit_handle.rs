//! [`PdfDocMut`] — mutable PDF document handle for WASM callers.
//!
//! Exposes `set_text_run_style` from [`pdf_manip::text_style`] to JavaScript.
//! All operations take a raw `Uint8Array` PDF, apply the mutation, and return
//! new PDF bytes — the caller holds immutable references and never passes
//! ownership back.
//!
//! # Example (JavaScript)
//!
//! ```js
//! const runs = JSON.parse(doc.getTextPositions(0));
//! const result = PdfDocMut.setTextRunStyle(pdfBytes, 1, runs, 0, true, false);
//! // result.pdf   — Uint8Array with modified PDF
//! // result.info  — JSON: { bytesChanged, originalFontName, requestedFontName, ... }
//! ```

use pdf_manip::text_run::extract_page_text_runs;
use pdf_manip::text_style::{StateIsolationStrategy, StyleResult, set_text_run_style};
use wasm_bindgen::prelude::*;

// ---------------------------------------------------------------------------
// JS-facing result type
// ---------------------------------------------------------------------------

/// Serialisable summary of a successful `setTextRunStyle` call.
#[derive(serde::Serialize)]
struct StyleResultJs {
    /// Bytes by which the content stream changed (0 = no-op).
    #[serde(rename = "bytesChanged")]
    bytes_changed: usize,
    /// PDF resource name used before the swap.
    #[serde(rename = "originalResourceName")]
    original_resource_name: String,
    /// PDF resource name used after the swap.
    #[serde(rename = "variantResourceName")]
    variant_resource_name: String,
    /// BaseFont of the original font.
    #[serde(rename = "originalFontName")]
    original_font_name: String,
    /// BaseFont of the variant font.
    #[serde(rename = "requestedFontName")]
    requested_font_name: String,
    /// Isolation strategy applied.
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

// ---------------------------------------------------------------------------
// PdfDocMut
// ---------------------------------------------------------------------------

/// Mutable PDF document handle.
///
/// Unlike [`PdfDoc`](crate::PdfDoc) which is read-only, `PdfDocMut` carries
/// a lopdf `Document` that can be mutated in place.  Construct it with
/// [`PdfDocMut::open`], apply mutations, then call [`PdfDocMut::save`] to
/// obtain the modified bytes.
#[wasm_bindgen]
pub struct PdfDocMut {
    doc: lopdf::Document,
}

#[wasm_bindgen]
impl PdfDocMut {
    /// Open a PDF from raw bytes for mutation.
    pub fn open(data: &[u8]) -> Result<PdfDocMut, JsError> {
        let doc =
            lopdf::Document::load_mem(data).map_err(|e| JsError::new(&format!("load: {e}")))?;
        Ok(PdfDocMut { doc })
    }

    /// Serialise the (possibly mutated) document back to bytes.
    pub fn save(&mut self) -> Result<Vec<u8>, JsError> {
        let mut buf = Vec::new();
        self.doc
            .save_to(&mut buf)
            .map_err(|e| JsError::new(&format!("save: {e}")))?;
        Ok(buf)
    }

    /// Apply bold and/or italic style to the text run at `run_index` on `page_num`.
    ///
    /// `page_num` is **1-based** (matches PDF convention used by pdf-manip).
    ///
    /// `runs_json` must be the JSON array previously returned by
    /// [`PdfDoc::getTextPositions`](crate::PdfDoc::get_text_positions) — only
    /// the `opsRangeStart`/`opsRangeEnd`/`fontName`/`fontSize` fields are used.
    /// For convenience the full JSON blob from `getTextPositions` is accepted;
    /// unknown fields are ignored.
    ///
    /// Returns a JSON object:
    /// ```json
    /// {
    ///   "bytesChanged": 12,
    ///   "originalResourceName": "F1",
    ///   "variantResourceName": "F2",
    ///   "originalFontName": "Helvetica",
    ///   "requestedFontName": "Helvetica-Bold",
    ///   "isolationStrategy": "Direct"
    /// }
    /// ```
    ///
    /// Throws when the variant is not embedded (`FontVariantNotEmbedded`).
    /// The document is left unmodified in that case.
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

        // runs_json is accepted but not re-parsed; we use the freshly extracted runs.
        let _ = runs_json;

        let result = set_text_run_style(&mut self.doc, page_num, run, bold, italic)
            .map_err(|e| JsError::new(&format!("{e}")))?;

        let js = style_result_to_js(&result);
        serde_json::to_string(&js).map_err(|e| JsError::new(&format!("serialize: {e}")))
    }

    /// Convenience wrapper: open bytes, apply one style change, return new bytes.
    ///
    /// Equivalent to `PdfDocMut.open(bytes).setTextRunStyle(...).save()`.
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

    /// Number of pages in the document.
    #[wasm_bindgen(js_name = "pageCount")]
    pub fn page_count(&self) -> usize {
        self.doc.get_pages().len()
    }
}
