//! WASM bindings for XFA form processing.
//!
//! Exposes the XFA engine to JavaScript via `wasm-bindgen`.
//! Supports:
//! - Building a form tree from a JSON schema definition
//! - Running FormCalc calculate scripts
//! - Exporting form data and schema as JSON
//! - Importing JSON data into a FormTree
//! - Getting/setting individual field values
//!
//! # Usage (JavaScript)
//!
//! ```js
//! import init, { XfaEngine } from '@pdfluent/wasm';
//! await init();
//!
//! // Create from a field definition
//! const engine = XfaEngine.fromFields([
//!   { name: "Name", value: "Alice" },
//!   { name: "Total", value: "", calculate: "100 + 21" },
//! ]);
//!
//! engine.runCalculations();
//! console.log(engine.getFieldValue("form1.Total")); // "121"
//!
//! const json = engine.exportJson();
//! engine.importJson('{"form1.Name": "Bob"}');
//! ```

#[cfg(feature = "render")]
pub mod canvas2d_device;

pub mod license;
pub mod edits;
pub mod edit_handle;

#[cfg(all(feature = "render", target_arch = "wasm32"))]
use crate::canvas2d_device::Canvas2DDevice;
#[cfg(all(feature = "render", target_arch = "wasm32"))]
use kurbo::{Affine, Rect, Shape};
use pdf_engine::api_error::PdfError;
use pdf_engine::PdfDocument;
#[cfg(all(feature = "render", target_arch = "wasm32"))]
use pdf_render::pdf_interpret::util::PageExt;
#[cfg(all(feature = "render", target_arch = "wasm32"))]
use pdf_render::pdf_interpret::{
    interpret_page, BlendMode, ClipPath, Context, Device, FillRule, InterpreterSettings,
};
use std::sync::Arc;
use wasm_bindgen::prelude::*;
use xfa_layout_engine::form::{FormNode, FormNodeId, FormNodeType, FormTree, Occur};
use xfa_layout_engine::scripting;
use xfa_layout_engine::text::FontMetrics;
use xfa_layout_engine::types::{BoxModel, LayoutStrategy};

fn wasm_err<E: PdfError>(e: E) -> JsError {
    let code = e.code();
    let msg = e.to_string();
    let help = e.help().unwrap_or_default();
    let docs = e.docs_url();
    JsError::new(&format!(
        "[{}] {} — Fix: {} — Docs: {}",
        code,
        msg.lines().next().unwrap_or(&msg),
        help.lines().next().unwrap_or(&help),
        docs
    ))
}

/// The main XFA processing engine for WASM.
///
/// Holds a parsed FormTree and provides methods to extract/import data.
#[wasm_bindgen]
pub struct XfaEngine {
    tree: FormTree,
    root: FormNodeId,
}

#[wasm_bindgen]
impl XfaEngine {
    /// Create an XfaEngine from a JSON array of field definitions.
    ///
    /// Expected format:
    /// ```json
    /// [
    ///   {"name": "FieldName", "value": "initial value", "calculate": "optional script"},
    ///   ...
    /// ]
    /// ```
    #[wasm_bindgen(js_name = "fromFields")]
    pub fn from_fields(fields_json: &str) -> Result<XfaEngine, JsError> {
        let fields: Vec<FieldDef> = serde_json::from_str(fields_json)
            .map_err(|e| JsError::new(&format!("JSON parse error: {e}")))?;

        let mut tree = FormTree::new();
        let mut child_ids = Vec::new();

        for field in &fields {
            let id = tree.add_node(FormNode {
                name: field.name.clone(),
                node_type: FormNodeType::Field {
                    value: field.value.clone().unwrap_or_default(),
                },
                box_model: BoxModel::default(),
                layout: LayoutStrategy::Positioned,
                children: vec![],
                occur: Occur::once(),
                font: FontMetrics::default(),
                calculate: field.calculate.clone(),
                validate: field.validate.clone(),
                column_widths: vec![],
                col_span: 1,
            });
            child_ids.push(id);
        }

        let root = tree.add_node(FormNode {
            name: "form1".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel::default(),
            layout: LayoutStrategy::Positioned,
            children: child_ids,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        Ok(XfaEngine { tree, root })
    }

    /// Create an XfaEngine from exported JSON data (e.g. from `exportJson`).
    ///
    /// Rebuilds the form tree from a flat field map.
    #[wasm_bindgen(js_name = "fromJson")]
    pub fn from_json(json_str: &str) -> Result<XfaEngine, JsError> {
        let form_data: xfa_json::FormData = serde_json::from_str(json_str)
            .map_err(|e| JsError::new(&format!("JSON parse error: {e}")))?;

        let mut tree = FormTree::new();
        let mut child_ids = Vec::new();

        for (name, field_val) in &form_data.fields {
            match field_val {
                xfa_json::FieldValue::Array(instances) => {
                    // Repeating section — create a subform per instance
                    let section_name = name.rsplit('.').next().unwrap_or(name).to_string();
                    for instance in instances {
                        let mut instance_children = Vec::new();
                        for (sub_key, sub_val) in instance {
                            let value = fv_to_string(sub_val);
                            let sub_id = tree.add_node(FormNode {
                                name: sub_key.clone(),
                                node_type: FormNodeType::Field { value },
                                box_model: BoxModel::default(),
                                layout: LayoutStrategy::Positioned,
                                children: vec![],
                                occur: Occur::once(),
                                font: FontMetrics::default(),
                                calculate: None,
                                validate: None,
                                column_widths: vec![],
                                col_span: 1,
                            });
                            instance_children.push(sub_id);
                        }
                        let sub_id = tree.add_node(FormNode {
                            name: section_name.clone(),
                            node_type: FormNodeType::Subform,
                            box_model: BoxModel::default(),
                            layout: LayoutStrategy::Positioned,
                            children: instance_children,
                            occur: Occur::repeating(0, None, 0),
                            font: FontMetrics::default(),
                            calculate: None,
                            validate: None,
                            column_widths: vec![],
                            col_span: 1,
                        });
                        child_ids.push(sub_id);
                    }
                }
                _ => {
                    let value = fv_to_string(field_val);
                    // Build nested subform structure from SOM path segments
                    let segments: Vec<&str> = name.split('.').collect();
                    let field_name = segments.last().copied().unwrap_or(name);
                    let id = tree.add_node(FormNode {
                        name: field_name.to_string(),
                        node_type: FormNodeType::Field { value },
                        box_model: BoxModel::default(),
                        layout: LayoutStrategy::Positioned,
                        children: vec![],
                        occur: Occur::once(),
                        font: FontMetrics::default(),
                        calculate: None,
                        validate: None,
                        column_widths: vec![],
                        col_span: 1,
                    });
                    // Wrap in intermediate subforms for path segments (skip root "form1")
                    let mut wrapped = id;
                    let end = segments.len().saturating_sub(1);
                    for &seg in segments[1..end.max(1)].iter().rev() {
                        wrapped = tree.add_node(FormNode {
                            name: seg.to_string(),
                            node_type: FormNodeType::Subform,
                            box_model: BoxModel::default(),
                            layout: LayoutStrategy::Positioned,
                            children: vec![wrapped],
                            occur: Occur::once(),
                            font: FontMetrics::default(),
                            calculate: None,
                            validate: None,
                            column_widths: vec![],
                            col_span: 1,
                        });
                    }
                    child_ids.push(wrapped);
                }
            }
        }

        let root = tree.add_node(FormNode {
            name: "form1".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel::default(),
            layout: LayoutStrategy::Positioned,
            children: child_ids,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        Ok(XfaEngine { tree, root })
    }

    /// Run FormCalc calculate scripts to compute derived field values.
    #[wasm_bindgen(js_name = "runCalculations")]
    pub fn run_calculations(&mut self) -> Result<(), JsError> {
        scripting::run_calculations(&mut self.tree)
            .map_err(|e| JsError::new(&format!("scripting error: {e}")))?;
        Ok(())
    }

    /// Export all field values as a JSON string.
    ///
    /// Returns `{"form1.FieldName": "value", ...}`.
    #[wasm_bindgen(js_name = "exportJson")]
    pub fn export_json(&self) -> Result<String, JsError> {
        let data = xfa_json::form_tree_to_json(&self.tree, self.root);
        serde_json::to_string(&data).map_err(|e| JsError::new(&format!("JSON serialize: {e}")))
    }

    /// Export the form schema as a JSON string.
    ///
    /// Returns metadata about each field (name, type, constraints).
    #[wasm_bindgen(js_name = "exportSchema")]
    pub fn export_schema(&self) -> Result<String, JsError> {
        let schema = xfa_json::export_schema(&self.tree, self.root);
        serde_json::to_string(&schema).map_err(|e| JsError::new(&format!("JSON serialize: {e}")))
    }

    /// Import field values from a JSON string.
    ///
    /// Accepts either:
    /// - `{"fields": {"form1.Name": "value"}}` (FormData format)
    /// - `{"form1.Name": "value"}` (flat format)
    #[wasm_bindgen(js_name = "importJson")]
    pub fn import_json(&mut self, json_str: &str) -> Result<(), JsError> {
        let form_data = parse_import_json(json_str).map_err(|e| JsError::new(&e))?;
        xfa_json::json_to_form_tree(&form_data, &mut self.tree, self.root);
        Ok(())
    }

    /// Get a single field value by SOM path (e.g., "form1.Name").
    #[wasm_bindgen(js_name = "getFieldValue")]
    pub fn get_field_value(&self, path: &str) -> Option<String> {
        find_field_value(&self.tree, self.root, path)
    }

    /// Set a single field value by SOM path.
    #[wasm_bindgen(js_name = "setFieldValue")]
    pub fn set_field_value(&mut self, path: &str, value: &str) -> bool {
        set_field_value_by_path(&mut self.tree, self.root, path, value)
    }

    /// Get the number of form nodes.
    #[wasm_bindgen(js_name = "nodeCount")]
    pub fn node_count(&self) -> usize {
        self.tree.nodes.len()
    }

    /// Get engine version string.
    #[wasm_bindgen]
    pub fn version() -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }
}

/// Parse JSON import data, accepting both FormData and flat formats.
/// Convert a FieldValue to its string representation.
fn fv_to_string(val: &xfa_json::FieldValue) -> String {
    match val {
        xfa_json::FieldValue::Text(s) => s.clone(),
        xfa_json::FieldValue::Number(n) => n.to_string(),
        xfa_json::FieldValue::Boolean(b) => {
            if *b {
                "1".to_string()
            } else {
                "0".to_string()
            }
        }
        xfa_json::FieldValue::Null => String::new(),
        xfa_json::FieldValue::Array(_) => String::new(),
    }
}

fn parse_import_json(json_str: &str) -> std::result::Result<xfa_json::FormData, String> {
    // Try FormData format first: {"fields": {"key": "value"}}
    if let Ok(form_data) = serde_json::from_str::<xfa_json::FormData>(json_str) {
        return Ok(form_data);
    }

    // Try flat format: {"key": "value"}
    if let Ok(obj) = serde_json::from_str::<serde_json::Value>(json_str) {
        if let Some(map) = obj.as_object() {
            let mut fields = indexmap::IndexMap::new();
            for (key, val) in map {
                let field_val = match val {
                    serde_json::Value::String(s) => xfa_json::FieldValue::Text(s.clone()),
                    serde_json::Value::Number(n) => {
                        xfa_json::FieldValue::Number(n.as_f64().unwrap_or(0.0))
                    }
                    serde_json::Value::Bool(b) => xfa_json::FieldValue::Boolean(*b),
                    serde_json::Value::Null => xfa_json::FieldValue::Null,
                    _ => xfa_json::FieldValue::Text(val.to_string()),
                };
                fields.insert(key.clone(), field_val);
            }
            return Ok(xfa_json::FormData { fields });
        }
    }

    Err(format!(
        "invalid JSON import data: {}",
        &json_str[..json_str.len().min(100)]
    ))
}

/// Field definition for `fromFields`.
#[derive(serde::Deserialize)]
struct FieldDef {
    name: String,
    value: Option<String>,
    calculate: Option<String>,
    validate: Option<String>,
}

/// Find a field value by dot-separated SOM path.
fn find_field_value(tree: &FormTree, root: FormNodeId, path: &str) -> Option<String> {
    fn search(
        tree: &FormTree,
        node_id: FormNodeId,
        parts: &[&str],
        depth: usize,
    ) -> Option<String> {
        let node = tree.get(node_id);
        let matches = node.name == parts.get(depth).copied().unwrap_or("");

        if matches || depth == 0 {
            let next_depth = if matches { depth + 1 } else { depth };

            // If we've consumed all path parts, return value if it's a field
            if next_depth >= parts.len() {
                if let FormNodeType::Field { value } = &node.node_type {
                    return Some(value.clone());
                }
            }

            // Search children
            for &child_id in &node.children {
                if let Some(val) = search(tree, child_id, parts, next_depth) {
                    return Some(val);
                }
            }
        }

        None
    }

    let parts: Vec<&str> = path.split('.').collect();
    search(tree, root, &parts, 0)
}

/// Set a field value by dot-separated SOM path.
fn set_field_value_by_path(tree: &mut FormTree, root: FormNodeId, path: &str, value: &str) -> bool {
    fn search_and_set(
        tree: &mut FormTree,
        node_id: FormNodeId,
        parts: &[&str],
        depth: usize,
        value: &str,
    ) -> bool {
        let name = tree.get(node_id).name.clone();
        let children = tree.get(node_id).children.clone();
        let matches = name == parts.get(depth).copied().unwrap_or("");

        if matches || depth == 0 {
            let next_depth = if matches { depth + 1 } else { depth };

            if next_depth >= parts.len() {
                if let FormNodeType::Field { value: ref mut v } = tree.get_mut(node_id).node_type {
                    *v = value.to_string();
                    return true;
                }
            }

            for child_id in children {
                if search_and_set(tree, child_id, parts, next_depth, value) {
                    return true;
                }
            }
        }

        false
    }

    let parts: Vec<&str> = path.split('.').collect();
    search_and_set(tree, root, &parts, 0, value)
}

// --- PdfDoc: PDF analysis via pdf-syntax (WASM-safe, no rayon) ---

/// PDF document handle for analysis and text extraction.
///
/// Uses `pdf-syntax` for metadata/geometry/signatures/compliance and
/// `pdf-engine` for text extraction so the WASM path matches native decoding.
#[wasm_bindgen]
pub struct PdfDoc {
    pub(crate) pdf: pdf_syntax::Pdf,
    pub(crate) engine: PdfDocument,
}

#[derive(serde::Serialize)]
struct TextRun {
    text: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    #[serde(rename = "fontSize")]
    font_size: f64,

    // ---- G1 read-only metadata (additive; old consumers ignore unknown keys) ----
    /// PostScript name (subset prefix stripped). Omitted from JSON when `None`.
    #[serde(rename = "fontName", skip_serializing_if = "Option::is_none")]
    font_name: Option<String>,
    /// Inferred bold style. Always emitted (boolean, default `false`).
    #[serde(rename = "isBold")]
    is_bold: bool,
    /// Inferred italic style. Always emitted (boolean, default `false`).
    #[serde(rename = "isItalic")]
    is_italic: bool,
    /// Fill color as `[r, g, b, a]` 0–255. Omitted when source paint is a
    /// pattern/shading or color could not be resolved.
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<[u8; 4]>,
}

#[wasm_bindgen]
impl PdfDoc {
    /// Open a PDF from raw bytes.
    pub fn open(data: &[u8]) -> Result<PdfDoc, JsError> {
        let raw = Arc::new(data.to_vec());
        let pdf = pdf_syntax::Pdf::new(raw.clone()).map_err(|e| JsError::new(&format!("{e:?}")))?;
        let engine = PdfDocument::open(raw).map_err(wasm_err)?;
        Ok(PdfDoc { pdf, engine })
    }

    /// Number of pages.
    #[wasm_bindgen(js_name = "pageCount")]
    pub fn page_count(&self) -> usize {
        self.pdf.pages().len()
    }

    /// Extract plain text from a page (0-based index).
    ///
    /// Returns an empty string if the page index is out of range or text
    /// extraction fails.
    pub fn text(&self, page_index: usize) -> String {
        if let Some(flattened_engine) = self.open_flattened_xfa_engine() {
            return flattened_engine
                .extract_text(page_index)
                .unwrap_or_default();
        }
        self.engine.extract_text(page_index).unwrap_or_default()
    }

    /// Document metadata as JSON.
    pub fn metadata(&self) -> String {
        let meta = self.pdf.metadata();
        let result = serde_json::json!({
            "title": meta.title.as_ref().map(|b| bytes_to_pdf_string(b)),
            "author": meta.author.as_ref().map(|b| bytes_to_pdf_string(b)),
            "subject": meta.subject.as_ref().map(|b| bytes_to_pdf_string(b)),
            "keywords": meta.keywords.as_ref().map(|b| bytes_to_pdf_string(b)),
            "creator": meta.creator.as_ref().map(|b| bytes_to_pdf_string(b)),
            "producer": meta.producer.as_ref().map(|b| bytes_to_pdf_string(b)),
        });
        serde_json::to_string(&result).unwrap_or_default()
    }

    /// Signature info as JSON array.
    pub fn signatures(&self) -> String {
        let sigs = pdf_sign::signature_fields(&self.pdf);
        let arr: Vec<serde_json::Value> = sigs
            .iter()
            .map(|s| {
                serde_json::json!({
                    "field_name": s.field_name,
                    "signer": s.sig.signer_name(),
                    "reason": s.sig.reason(),
                    "location": s.sig.location(),
                    "signing_time": s.sig.signing_time(),
                    "sub_filter": s.sig.sub_filter().map(|sf| format!("{sf:?}")),
                })
            })
            .collect();
        serde_json::to_string(&arr).unwrap_or_default()
    }

    /// Validate against a PDF/A level. Returns compliance report as JSON.
    #[wasm_bindgen(js_name = "validatePdfA")]
    pub fn validate_pdfa(&self, level: &str) -> Result<String, JsError> {
        let pdfa_level = match level
            .to_lowercase()
            .replace(['-', '/', '_', ' '], "")
            .as_str()
        {
            "pdfa1a" | "a1a" | "1a" => pdf_compliance::PdfALevel::A1a,
            "pdfa1b" | "a1b" | "1b" => pdf_compliance::PdfALevel::A1b,
            "pdfa2a" | "a2a" | "2a" => pdf_compliance::PdfALevel::A2a,
            "pdfa2b" | "a2b" | "2b" => pdf_compliance::PdfALevel::A2b,
            "pdfa2u" | "a2u" | "2u" => pdf_compliance::PdfALevel::A2u,
            "pdfa3a" | "a3a" | "3a" => pdf_compliance::PdfALevel::A3a,
            "pdfa3b" | "a3b" | "3b" => pdf_compliance::PdfALevel::A3b,
            "pdfa3u" | "a3u" | "3u" => pdf_compliance::PdfALevel::A3u,
            "pdfa4" | "a4" | "4" => pdf_compliance::PdfALevel::A4,
            "pdfa4f" | "a4f" | "4f" => pdf_compliance::PdfALevel::A4f,
            "pdfa4e" | "a4e" | "4e" => pdf_compliance::PdfALevel::A4e,
            other => {
                return Err(JsError::new(&format!(
                    "unknown PDF/A level: {other:?} — expected e.g. \"2b\", \"3b\", \"1b\""
                )))
            }
        };
        let report = pdf_compliance::validate_pdfa(&self.pdf, pdfa_level);
        let result = serde_json::json!({
            "compliant": report.is_compliant(),
            "errors": report.error_count(),
            "warnings": report.warning_count(),
            "issues": report.issues.iter().map(|i| serde_json::json!({
                "rule": i.rule,
                "severity": format!("{:?}", i.severity),
                "message": i.message,
            })).collect::<Vec<_>>(),
        });
        serde_json::to_string(&result).map_err(|e| JsError::new(&e.to_string()))
    }

    /// Check if the document has any signatures.
    #[wasm_bindgen(js_name = "hasSignatures")]
    pub fn has_signatures(&self) -> bool {
        !pdf_sign::signature_fields(&self.pdf).is_empty()
    }

    /// DSS (Document Security Store) info as JSON, or null if absent.
    #[wasm_bindgen(js_name = "dssInfo")]
    pub fn dss_info(&self) -> Option<String> {
        let dss = pdf_sign::DocumentSecurityStore::from_pdf(&self.pdf)?;
        let result = serde_json::json!({
            "has_ltv": dss.has_ltv_data(),
            "certificates": dss.certificates.len(),
            "ocsp_responses": dss.ocsp_responses.len(),
            "crls": dss.crls.len(),
            "vri_entries": dss.vri_entries.len(),
        });
        Some(serde_json::to_string(&result).unwrap_or_default())
    }

    /// Flatten XFA form fields into static PDF content.
    ///
    /// Returns the flattened PDF as a `Uint8Array`.
    /// Throws if the document has no XFA stream or flattening fails.
    #[wasm_bindgen(js_name = "flattenXfa")]
    pub fn flatten_xfa(&self) -> Result<Vec<u8>, JsError> {
        pdf_engine::xfa::flatten(&self.engine)
            .map_err(|e| JsError::new(&format!("XFA flatten failed: {e}")))
    }

    // ---- Page geometry ----

    /// Get page width in PDF points.
    #[wasm_bindgen(js_name = "pageWidth")]
    pub fn page_width(&self, page_index: usize) -> f64 {
        self.engine
            .page_geometry(page_index)
            .map(|geometry| geometry.effective_dimensions().0)
            .unwrap_or(0.0)
    }

    /// Get page height in PDF points.
    #[wasm_bindgen(js_name = "pageHeight")]
    pub fn page_height(&self, page_index: usize) -> f64 {
        self.engine
            .page_geometry(page_index)
            .map(|geometry| geometry.effective_dimensions().1)
            .unwrap_or(0.0)
    }

    // ---- Page rendering (feature: render) ----

    /// Render a page to RGBA pixels.
    ///
    /// Returns a Uint8Array with layout: `[width:4LE][height:4LE][RGBA pixels...]`.
    /// Use with Canvas ImageData:
    /// ```js
    /// const raw = doc.renderPage(0, 1.5);
    /// const view = new DataView(raw.buffer);
    /// const w = view.getUint32(0, true);
    /// const h = view.getUint32(4, true);
    /// const pixels = raw.slice(8);
    /// const imageData = new ImageData(new Uint8ClampedArray(pixels), w, h);
    /// ctx.putImageData(imageData, 0, 0);
    /// ```
    #[cfg(feature = "render")]
    #[wasm_bindgen(js_name = "renderPage")]
    pub fn render_page(&self, page_index: usize, scale: f32) -> Result<Vec<u8>, JsError> {
        let page = Self::render_engine_page(&self.engine, page_index, scale)?;
        let mut buf = Vec::with_capacity(8 + page.pixels.len());
        buf.extend_from_slice(&page.width.to_le_bytes());
        buf.extend_from_slice(&page.height.to_le_bytes());
        buf.extend_from_slice(&page.pixels);
        Ok(buf)
    }

    /// Render a thumbnail constrained to a maximum dimension.
    ///
    /// Same return format as `renderPage`.
    #[cfg(feature = "render")]
    #[wasm_bindgen(js_name = "renderThumbnail")]
    pub fn render_thumbnail(
        &self,
        page_index: usize,
        max_dimension: u32,
    ) -> Result<Vec<u8>, JsError> {
        let page_count = self.engine.page_count();
        if page_index >= page_count {
            return Err(JsError::new(&format!(
                "page index {page_index} out of range (0..{page_count})"
            )));
        }
        let geom = self.engine.page_geometry(page_index).map_err(wasm_err)?;
        let pw = geom.media_box.width().abs() as f32;
        let ph = geom.media_box.height().abs() as f32;
        let max_side = pw.max(ph);
        let scale = if max_side > 0.0 {
            max_dimension as f32 / max_side
        } else {
            1.0
        };
        self.render_page(page_index, scale)
    }

    /// Render a page directly to an HTML canvas element (wasm32 only).
    ///
    /// Calls `canvas.getContext("2d")`, sets the canvas dimensions to the
    /// rendered pixel size, and calls `putImageData` — no round-trip through
    /// a JS Uint8Array.  XFA documents are auto-flattened before rendering.
    ///
    /// ```js
    /// const canvas = document.getElementById('viewer');
    /// await doc.renderPageToCanvas(canvas, 0, 1.5);  // page 0, 108 DPI
    /// ```
    #[cfg(all(feature = "render", target_arch = "wasm32"))]
    #[wasm_bindgen(js_name = "renderPageToCanvas")]
    pub fn render_page_to_canvas(
        &self,
        canvas: &web_sys::HtmlCanvasElement,
        page_index: usize,
        scale: f32,
    ) -> Result<(), JsError> {
        let has_xfa = pdf_engine::xfa::has_xfa(&self.engine);
        web_sys::console::log_1(&format!("has_xfa: {has_xfa}").into());

        if has_xfa {
            web_sys::console::log_1(&"XFA detected, flattening...".into());
            match pdf_engine::xfa::flatten(&self.engine) {
                Ok(flattened_bytes) => {
                    web_sys::console::log_1(
                        &format!("Flattened: {} bytes", flattened_bytes.len()).into(),
                    );
                    match PdfDocument::open(Arc::new(flattened_bytes)) {
                        Ok(flattened_engine) => {
                            return self.render_engine_to_canvas(
                                &flattened_engine,
                                canvas,
                                page_index,
                                scale,
                            );
                        }
                        Err(error) => {
                            web_sys::console::log_1(
                                &format!("flatten open failed: {error}").into(),
                            );
                        }
                    }
                }
                Err(error) => {
                    web_sys::console::log_1(&format!("Flatten failed: {error}").into());
                }
            }
        }

        web_sys::console::log_1(&"Rendering original PDF bytes".into());
        self.render_engine_to_canvas(&self.engine, canvas, page_index, scale)
    }

    /// Render a page using Canvas2D vector draw calls.
    ///
    /// Falls back to the raster renderer if the vector device hits an
    /// unsupported PDF feature.
    #[cfg(all(feature = "render", target_arch = "wasm32"))]
    #[wasm_bindgen(js_name = "renderPageToCanvasVector")]
    pub fn render_page_to_canvas_vector(
        &self,
        canvas: &web_sys::HtmlCanvasElement,
        page_index: usize,
        scale: f32,
    ) -> Result<(), JsError> {
        let has_xfa = pdf_engine::xfa::has_xfa(&self.engine);
        web_sys::console::log_1(&format!("has_xfa: {has_xfa}").into());

        let mut flattened_engine = None;

        if has_xfa {
            web_sys::console::log_1(&"XFA detected, flattening for vector render...".into());
            match pdf_engine::xfa::flatten(&self.engine) {
                Ok(flattened_bytes) => {
                    web_sys::console::log_1(
                        &format!("Flattened: {} bytes", flattened_bytes.len()).into(),
                    );
                    match PdfDocument::open(Arc::new(flattened_bytes)) {
                        Ok(engine) => flattened_engine = Some(engine),
                        Err(error) => {
                            web_sys::console::log_1(
                                &format!("flatten open failed: {error}").into(),
                            );
                        }
                    }
                }
                Err(error) => {
                    web_sys::console::log_1(&format!("Flatten failed: {error}").into());
                }
            }
        }

        let engine = flattened_engine.as_ref().unwrap_or(&self.engine);
        match self.render_engine_to_canvas_vector(engine, canvas, page_index, scale) {
            Ok(()) => Ok(()),
            Err(error) => {
                web_sys::console::warn_1(
                    &format!("Vector canvas render failed, falling back to raster: {error:?}")
                        .into(),
                );
                self.render_engine_to_canvas(engine, canvas, page_index, scale)
            }
        }
    }

    /// Text-run positions for a page.
    ///
    /// Returns a JSON array of `{text, x, y, width, height, fontSize}`
    /// objects in page space with origin at the top-left.  Each entry is a
    /// contiguous text run emitted by the same PDF interpreter that drives the
    /// renderer, which keeps the selection overlay aligned with the canvas.
    ///
    /// ```js
    /// const runs = JSON.parse(doc.getTextPositions(0));
    /// // runs[i] = {
    /// //   text: "Hello world",
    /// //   x: 72.0, y: 100.0, width: 96.0, height: 12.0, fontSize: 12.0,
    /// //   fontName: "Helvetica-Bold", // G1: omitted when unknown
    /// //   isBold: true, isItalic: false,
    /// //   color: [0, 0, 0, 255]       // G1: omitted when unknown
    /// // }
    /// ```
    ///
    /// `fontName` and `color` are omitted from the JSON when the source
    /// metadata is unavailable (Type1/standard-14 fonts, Pattern paints,
    /// or unsupported color spaces). `isBold`/`isItalic` are always present
    /// so editor toolbars can render unconditionally.
    #[wasm_bindgen(js_name = "getTextPositions")]
    pub fn get_text_positions(&self, page_index: usize) -> Result<String, JsError> {
        let text_engine = self.open_flattened_xfa_engine();
        let engine = text_engine.as_ref().unwrap_or(&self.engine);
        let page_height = engine
            .page_geometry(page_index)
            .map(|geometry| geometry.effective_dimensions().1)
            .unwrap_or(0.0);
        let runs: Vec<TextRun> = engine
            .extract_text_blocks(page_index)
            .map_err(|e| JsError::new(&format!("text extract: {e}")))?
            .into_iter()
            .flat_map(|block| block.spans.into_iter())
            .filter(|span| !span.text.is_empty())
            .map(|span| TextRun {
                text: span.text,
                x: span.x.max(0.0),
                y: (page_height - span.y - span.height).max(0.0),
                width: span.width.max(1.0),
                height: span.height.max(1.0),
                font_size: span.font_size.max(1.0),
                font_name: span.font_name,
                is_bold: span.is_bold,
                is_italic: span.is_italic,
                color: span.color,
            })
            .collect();
        serde_json::to_string(&runs).map_err(|e| JsError::new(&format!("serialize text runs: {e}")))
    }

    // ---- Annotation reading ----

    /// Parse existing annotations on a page as JSON.
    ///
    /// Returns a JSON array of annotation objects.
    #[cfg(feature = "annotate")]
    #[wasm_bindgen(js_name = "getAnnotations")]
    pub fn get_annotations(&self, page_index: usize) -> Result<String, JsError> {
        let pages = self.pdf.pages();
        if page_index >= pages.len() {
            return Err(JsError::new(&format!(
                "page index {page_index} out of range (0..{})",
                pages.len()
            )));
        }
        let page = &pages[page_index];
        let annots = pdf_annot::Annotation::from_page(page);
        let arr: Vec<serde_json::Value> = annots
            .iter()
            .map(|a| {
                let rect = a.rect().map(|r| {
                    serde_json::json!({
                        "x0": r.x0,
                        "y0": r.y0,
                        "x1": r.x1,
                        "y1": r.y1,
                    })
                });
                serde_json::json!({
                    "subtype": format!("{:?}", a.annotation_type()),
                    "rect": rect,
                    "contents": a.contents(),
                })
            })
            .collect();
        serde_json::to_string(&arr).map_err(|e| JsError::new(&e.to_string()))
    }

    // ---- Annotation creation (feature: annotate) ----


    /// Verify all digital signatures in the document.
    ///
    /// Returns JSON array with verification results per signature.
    #[wasm_bindgen(js_name = "verifySignatures")]
    pub fn verify_signatures(&self) -> String {
        let sigs = pdf_sign::signature_fields(&self.pdf);
        let results: Vec<serde_json::Value> = sigs
            .iter()
            .map(|s| {
                let structural_ok = s
                    .sig
                    .cms_signed_data()
                    .map(|cms| cms.verify_structural_integrity())
                    .unwrap_or(false);
                serde_json::json!({
                    "field_name": s.field_name,
                    "signer": s.sig.signer_name(),
                    "structural_integrity": structural_ok,
                    "sub_filter": s.sig.sub_filter().map(|sf| format!("{sf:?}")),
                    "signing_time": s.sig.signing_time(),
                })
            })
            .collect();
        serde_json::to_string(&results).unwrap_or_default()
    }

    /// Merge another PDF document into this one.
    ///
    /// Returns the merged PDF as a `Uint8Array`.
    #[wasm_bindgen(js_name = "merge")]
    pub fn merge(&self, other: &[u8]) -> Result<Vec<u8>, JsError> {
        let self_bytes = self.pdf.data().as_ref();
        let mut self_doc =
            lopdf::Document::load_mem(self_bytes).map_err(|e| JsError::new(&format!("{e}")))?;
        let other_doc =
            lopdf::Document::load_mem(other).map_err(|e| JsError::new(&format!("{e}")))?;
        let page_count = self_doc.get_pages().len() as u32;
        pdf_manip::pages::insert_pages(&mut self_doc, &other_doc, page_count + 1)
            .map_err(|e| JsError::new(&format!("merge failed: {e}")))?;
        let mut buf = Vec::new();
        self_doc
            .save_to(&mut buf)
            .map_err(|e| JsError::new(&format!("save failed: {e}")))?;
        Ok(buf)
    }

    /// Convert this PDF to PDF/A-1b, PDF/A-2b, or PDF/A-3b.
    ///
    /// `level` must be "1b", "2b", or "3b".
    /// Returns the converted PDF as a `Uint8Array`.
    #[wasm_bindgen(js_name = "convertToPdfa")]
    pub fn convert_to_pdfa(&self, level: &str) -> Result<Vec<u8>, JsError> {
        use pdf_manip::pdfa_xmp::PdfAConformance;
        let conformance = match level
            .to_lowercase()
            .replace(['-', '/', '_', ' '], "")
            .as_str()
        {
            "pdfa1b" | "a1b" | "1b" => PdfAConformance::A1b,
            "pdfa2b" | "a2b" | "2b" => PdfAConformance::A2b,
            "pdfa3b" | "a3b" | "3b" => PdfAConformance::A3b,
            other => {
                return Err(JsError::new(&format!(
                    "unknown PDF/A level: {other:?} — expected \"1b\", \"2b\", or \"3b\""
                )))
            }
        };
        let self_bytes = self.pdf.data().as_ref();
        let mut doc =
            lopdf::Document::load_mem(self_bytes).map_err(|e| JsError::new(&format!("{e}")))?;
        let is_pdfa1 = matches!(conformance, PdfAConformance::A1b);
        let _ = pdf_manip::pdfa_cleanup::cleanup_for_pdfa(&mut doc, is_pdfa1)
            .map_err(|e| JsError::new(&format!("pdfa cleanup: {e}")))?;
        let _ = pdf_manip::pdfa_fonts::enforce_pdfa_font_compliance(&mut doc);
        let _ = pdf_manip::pdfa_colorspace::normalize_colorspaces(&mut doc)
            .map_err(|e| JsError::new(&format!("colorspace: {e}")))?;
        pdf_manip::pdfa_fixups::run_fixups(&mut doc);
        let _ = pdf_manip::pdfa_xmp::repair_xmp_metadata(&mut doc, conformance, None)
            .map_err(|e| JsError::new(&format!("xmp repair: {e}")))?;
        let mut buf = Vec::new();
        doc.save_to(&mut buf)
            .map_err(|e| JsError::new(&format!("save: {e}")))?;
        pdf_manip::pdfa_cleanup::fix_pdf_header(&mut buf);
        pdf_manip::pdfa_cleanup::fix_startxref(&mut buf);
        Ok(buf)
    }
}

impl PdfDoc {
    fn open_flattened_xfa_engine(&self) -> Option<PdfDocument> {
        if !pdf_engine::xfa::has_xfa(&self.engine) {
            return None;
        }

        let flattened_bytes = pdf_engine::xfa::flatten(&self.engine).ok()?;
        PdfDocument::open(Arc::new(flattened_bytes)).ok()
    }

    #[cfg(feature = "render")]
    fn render_engine_page(
        engine: &PdfDocument,
        page_index: usize,
        scale: f32,
    ) -> Result<pdf_engine::RenderedPage, JsError> {
        let options = pdf_engine::RenderOptions {
            dpi: (scale * 72.0) as f64,
            ..Default::default()
        };
        engine.render_page(page_index, &options).map_err(wasm_err)
    }

    #[cfg(all(feature = "render", target_arch = "wasm32"))]
    fn render_engine_to_canvas(
        &self,
        engine: &PdfDocument,
        canvas: &web_sys::HtmlCanvasElement,
        page_index: usize,
        scale: f32,
    ) -> Result<(), JsError> {
        use wasm_bindgen::JsCast;

        let rendered = Self::render_engine_page(engine, page_index, scale)?;
        canvas.set_width(rendered.width);
        canvas.set_height(rendered.height);
        let ctx = canvas
            .get_context("2d")
            .map_err(|e| JsError::new(&format!("getContext: {e:?}")))?
            .ok_or_else(|| JsError::new("no 2d context"))?;
        let ctx: web_sys::CanvasRenderingContext2d = ctx
            .dyn_into()
            .map_err(|_| JsError::new("context is not CanvasRenderingContext2d"))?;
        let image_data = web_sys::ImageData::new_with_u8_clamped_array_and_sh(
            wasm_bindgen::Clamped(&rendered.pixels),
            rendered.width,
            rendered.height,
        )
        .map_err(|e| JsError::new(&format!("ImageData: {e:?}")))?;
        ctx.put_image_data(&image_data, 0.0, 0.0)
            .map_err(|e| JsError::new(&format!("putImageData: {e:?}")))?;
        Ok(())
    }

    #[cfg(all(feature = "render", target_arch = "wasm32"))]
    fn render_engine_to_canvas_vector(
        &self,
        engine: &PdfDocument,
        canvas: &web_sys::HtmlCanvasElement,
        page_index: usize,
        scale: f32,
    ) -> Result<(), JsError> {
        use wasm_bindgen::JsCast;

        let pages = engine.pdf().pages();
        if page_index >= pages.len() {
            return Err(JsError::new(&format!(
                "page index {page_index} out of range (0..{})",
                pages.len()
            )));
        }

        let page = &pages[page_index];
        let (page_width, page_height) = page.render_dimensions();
        let width = (page_width * scale).ceil().max(1.0) as u32;
        let height = (page_height * scale).ceil().max(1.0) as u32;

        canvas.set_width(width);
        canvas.set_height(height);

        let ctx = canvas
            .get_context("2d")
            .map_err(|e| JsError::new(&format!("getContext: {e:?}")))?
            .ok_or_else(|| JsError::new("no 2d context"))?;
        let ctx: web_sys::CanvasRenderingContext2d = ctx
            .dyn_into()
            .map_err(|_| JsError::new("context is not CanvasRenderingContext2d"))?;

        ctx.reset_transform()
            .map_err(|e| JsError::new(&format!("resetTransform: {e:?}")))?;
        ctx.set_global_alpha(1.0);
        ctx.set_global_composite_operation("source-over")
            .map_err(|e| JsError::new(&format!("globalCompositeOperation: {e:?}")))?;
        ctx.clear_rect(0.0, 0.0, width as f64, height as f64);
        ctx.set_fill_style_str("rgba(255, 255, 255, 1)");
        ctx.fill_rect(0.0, 0.0, width as f64, height as f64);

        let initial_transform =
            Affine::scale_non_uniform(scale as f64, scale as f64) * page.initial_transform(true);
        let viewport = Rect::new(0.0, 0.0, width as f64, height as f64);
        let mut context = Context::new(
            initial_transform,
            viewport,
            page.xref(),
            InterpreterSettings::default(),
        );
        let mut device = Canvas2DDevice::new(ctx);

        device.push_clip_path(&ClipPath {
            path: viewport.to_path(0.1),
            fill: FillRule::NonZero,
        });
        device.push_transparency_group(1.0, None, BlendMode::Normal);
        interpret_page(page, &mut context, &mut device);
        device.pop_transparency_group();
        device.pop_clip_path();

        if let Some(reason) = device.fallback_reason() {
            return Err(JsError::new(reason));
        }

        Ok(())
    }
}

/// Convert PDF string bytes to a Rust String (UTF-8/UTF-16/Latin-1).
fn bytes_to_pdf_string(bytes: &[u8]) -> String {
    // UTF-16 BOM
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let chars: Vec<u16> = bytes[2..]
            .chunks(2)
            .filter_map(|c| {
                if c.len() == 2 {
                    Some(u16::from_be_bytes([c[0], c[1]]))
                } else {
                    None
                }
            })
            .collect();
        return String::from_utf16_lossy(&chars);
    }
    // UTF-8 with Latin-1 fallback
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => bytes.iter().map(|&b| b as char).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{Document as LoDocument, Object};
    use std::path::PathBuf;

    fn corpus_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../corpus")
            .join(name)
    }

    fn normalize_text(text: &str) -> String {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    fn strip_type0_tounicode(data: &[u8]) -> (Vec<u8>, usize) {
        fn get_name(dict: &lopdf::Dictionary, key: &[u8]) -> Option<Vec<u8>> {
            match dict.get(key).ok()? {
                Object::Name(name) => Some(name.clone()),
                _ => None,
            }
        }

        fn descendant_is_cidfont_type2(doc: &LoDocument, type0: &lopdf::Dictionary) -> bool {
            let Some(Object::Array(descendants)) = type0.get(b"DescendantFonts").ok() else {
                return false;
            };
            let Some(Object::Reference(desc_id)) = descendants.first() else {
                return false;
            };
            let Ok(Object::Dictionary(descendant)) = doc.get_object(*desc_id) else {
                return false;
            };
            matches!(
                descendant.get(b"Subtype").ok(),
                Some(Object::Name(name)) if name.as_slice() == b"CIDFontType2"
            )
        }

        let mut doc = LoDocument::load_mem(data).expect("load stripped-to-unicode fixture");
        let ids: Vec<_> = doc.objects.keys().copied().collect();
        let mut removed = 0usize;

        for id in ids {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&id) else {
                continue;
            };
            if !matches!(
                dict.get(b"Subtype").ok(),
                Some(Object::Name(name)) if name.as_slice() == b"Type0"
            ) {
                continue;
            }
            if !matches!(
                get_name(dict, b"Encoding").as_deref(),
                Some(b"Identity-H") | Some(b"Identity-V")
            ) {
                continue;
            }
            if !descendant_is_cidfont_type2(&doc, dict) {
                continue;
            }

            if let Some(Object::Dictionary(type0)) = doc.objects.get_mut(&id) {
                if type0.has(b"ToUnicode") {
                    type0.remove(b"ToUnicode");
                    removed += 1;
                }
            }
        }

        let mut out = Vec::new();
        doc.save_to(&mut out)
            .expect("save stripped-to-unicode fixture");
        (out, removed)
    }

    #[test]
    fn from_fields_basic() {
        let json = r#"[
            {"name": "Name", "value": "Alice"},
            {"name": "Email", "value": "alice@example.com"}
        ]"#;
        let engine = XfaEngine::from_fields(json).unwrap();
        assert_eq!(engine.node_count(), 3); // 2 fields + 1 root subform
    }

    #[test]
    fn get_set_field_value() {
        let json = r#"[{"name": "Name", "value": "Alice"}]"#;
        let mut engine = XfaEngine::from_fields(json).unwrap();

        assert_eq!(
            engine.get_field_value("form1.Name"),
            Some("Alice".to_string())
        );

        assert!(engine.set_field_value("form1.Name", "Bob"));
        assert_eq!(
            engine.get_field_value("form1.Name"),
            Some("Bob".to_string())
        );
    }

    #[test]
    fn export_json() {
        let json = r#"[
            {"name": "Name", "value": "Alice"},
            {"name": "City", "value": "Amsterdam"}
        ]"#;
        let engine = XfaEngine::from_fields(json).unwrap();
        let exported = engine.export_json().unwrap();
        assert!(exported.contains("Alice"));
        assert!(exported.contains("Amsterdam"));
    }

    #[test]
    fn run_calculations() {
        let json = r#"[
            {"name": "Total", "value": "", "calculate": "10 + 20"}
        ]"#;
        let mut engine = XfaEngine::from_fields(json).unwrap();
        engine.run_calculations().unwrap();

        assert_eq!(
            engine.get_field_value("form1.Total"),
            Some("30".to_string())
        );
    }

    #[test]
    fn import_json() {
        let json = r#"[{"name": "Name", "value": "Alice"}]"#;
        let mut engine = XfaEngine::from_fields(json).unwrap();

        // Use FormData format: {"fields": {"form1.Name": "Charlie"}}
        let import_data = r#"{"fields": {"form1.Name": "Charlie"}}"#;
        engine.import_json(import_data).unwrap();

        assert_eq!(
            engine.get_field_value("form1.Name"),
            Some("Charlie".to_string())
        );
    }

    #[test]
    fn from_json_roundtrip() {
        let json = r#"[
            {"name": "Name", "value": "Alice"},
            {"name": "Age", "value": "30"}
        ]"#;
        let engine = XfaEngine::from_fields(json).unwrap();
        let exported = engine.export_json().unwrap();

        let engine2 = XfaEngine::from_json(&exported).unwrap();
        let exported2 = engine2.export_json().unwrap();

        // Same data after round-trip
        let v1: serde_json::Value = serde_json::from_str(&exported).unwrap();
        let v2: serde_json::Value = serde_json::from_str(&exported2).unwrap();
        assert_eq!(v1, v2);
    }

    #[test]
    fn export_schema() {
        let json = r#"[{"name": "Name", "value": "Alice"}]"#;
        let engine = XfaEngine::from_fields(json).unwrap();
        let schema = engine.export_schema().unwrap();
        assert!(schema.contains("Name"));
    }

    #[test]
    fn get_missing_field_returns_none() {
        let json = r#"[{"name": "Name", "value": "Alice"}]"#;
        let engine = XfaEngine::from_fields(json).unwrap();
        assert_eq!(engine.get_field_value("form1.Missing"), None);
    }

    #[test]
    fn set_missing_field_returns_false() {
        let json = r#"[{"name": "Name", "value": "Alice"}]"#;
        let mut engine = XfaEngine::from_fields(json).unwrap();
        assert!(!engine.set_field_value("form1.Missing", "X"));
    }

    #[test]
    fn version_is_set() {
        let v = XfaEngine::version();
        assert!(!v.is_empty());
    }

    #[test]
    fn pdf_doc_text_handles_type0_without_tounicode() {
        let original = std::fs::read(corpus_path("sf181.pdf")).expect("read sf181 fixture");
        let (stripped, removed) = strip_type0_tounicode(&original);
        assert!(
            removed > 0,
            "expected to strip at least one Type0 ToUnicode"
        );

        let actual = PdfDoc::open(&stripped)
            .expect("open stripped sf181")
            .text(0);
        let actual_norm = normalize_text(&actual);

        assert!(
            actual_norm.contains("Guide to Personnel Data Standards"),
            "missing main heading after stripping ToUnicode: {actual_norm}"
        );
        assert!(
            actual_norm.contains("Privacy Act Statement"),
            "missing body text after stripping ToUnicode: {actual_norm}"
        );
    }

    #[test]
    fn bytes_to_pdf_string_utf8() {
        assert_eq!(bytes_to_pdf_string(b"hello"), "hello");
    }

    #[test]
    fn bytes_to_pdf_string_utf16() {
        let bytes = &[0xFE, 0xFF, 0x00, 0x48, 0x00, 0x69]; // "Hi"
        assert_eq!(bytes_to_pdf_string(bytes), "Hi");
    }

    #[test]
    fn bytes_to_pdf_string_latin1() {
        let bytes = &[0xC4, 0xD6, 0xDC]; // ÄÖÜ
        assert_eq!(bytes_to_pdf_string(bytes), "ÄÖÜ");
    }

    // ---- G1: getTextPositions JSON shape ----

    #[test]
    fn g1_text_run_serializes_with_metadata() {
        let run = TextRun {
            text: "Hello".into(),
            x: 72.0,
            y: 100.0,
            width: 30.0,
            height: 12.0,
            font_size: 12.0,
            font_name: Some("Helvetica-Bold".into()),
            is_bold: true,
            is_italic: false,
            color: Some([255, 0, 0, 255]),
        };
        let json = serde_json::to_string(&run).expect("serialize");
        let v: serde_json::Value = serde_json::from_str(&json).expect("parse");
        assert_eq!(v["text"], "Hello");
        assert_eq!(v["x"], 72.0);
        assert_eq!(v["y"], 100.0);
        assert_eq!(v["width"], 30.0);
        assert_eq!(v["height"], 12.0);
        assert_eq!(v["fontSize"], 12.0);
        assert_eq!(v["fontName"], "Helvetica-Bold");
        assert_eq!(v["isBold"], true);
        assert_eq!(v["isItalic"], false);
        assert_eq!(v["color"], serde_json::json!([255, 0, 0, 255]));
    }

    #[test]
    fn g1_text_run_omits_unknown_fontname_and_color() {
        // Fallback case: Type1/standard-14 + pattern paint. JSON must elide
        // the unknown keys so editor consumers can `'fontName' in run` test.
        let run = TextRun {
            text: "Hi".into(),
            x: 1.0,
            y: 2.0,
            width: 10.0,
            height: 12.0,
            font_size: 12.0,
            font_name: None,
            is_bold: false,
            is_italic: false,
            color: None,
        };
        let json = serde_json::to_string(&run).expect("serialize");
        let v: serde_json::Value = serde_json::from_str(&json).expect("parse");
        assert!(v.get("fontName").is_none(), "fontName must be omitted when None");
        assert!(v.get("color").is_none(), "color must be omitted when None");
        // isBold/isItalic always present (default false) so toolbars render.
        assert_eq!(v["isBold"], false);
        assert_eq!(v["isItalic"], false);
        // All legacy fields still present.
        assert_eq!(v["text"], "Hi");
        assert_eq!(v["fontSize"], 12.0);
    }
}
