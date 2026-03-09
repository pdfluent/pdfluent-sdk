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
//! import init, { XfaEngine } from 'xfa-wasm';
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

use wasm_bindgen::prelude::*;
use xfa_layout_engine::form::{FormNode, FormNodeId, FormNodeType, FormTree, Occur};
use xfa_layout_engine::scripting;
use xfa_layout_engine::text::FontMetrics;
use xfa_layout_engine::types::{BoxModel, LayoutStrategy};

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

/// PDF document handle for analysis (metadata, signatures, compliance).
///
/// Uses pdf-syntax directly (pure Rust, no rayon) for WASM compatibility.
#[wasm_bindgen]
pub struct PdfDoc {
    pdf: pdf_syntax::Pdf,
}

#[wasm_bindgen]
impl PdfDoc {
    /// Open a PDF from raw bytes.
    pub fn open(data: &[u8]) -> Result<PdfDoc, JsError> {
        let pdf =
            pdf_syntax::Pdf::new(data.to_vec()).map_err(|e| JsError::new(&format!("{e:?}")))?;
        Ok(PdfDoc { pdf })
    }

    /// Number of pages.
    #[wasm_bindgen(js_name = "pageCount")]
    pub fn page_count(&self) -> usize {
        self.pdf.pages().len()
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
        let pdfa_level = match level.to_lowercase().replace(['-', '/'], "").as_str() {
            "pdfa1a" | "a1a" => pdf_compliance::PdfALevel::A1a,
            "pdfa1b" | "a1b" => pdf_compliance::PdfALevel::A1b,
            "pdfa2a" | "a2a" => pdf_compliance::PdfALevel::A2a,
            "pdfa2b" | "a2b" => pdf_compliance::PdfALevel::A2b,
            "pdfa2u" | "a2u" => pdf_compliance::PdfALevel::A2u,
            "pdfa3a" | "a3a" => pdf_compliance::PdfALevel::A3a,
            "pdfa3b" | "a3b" => pdf_compliance::PdfALevel::A3b,
            "pdfa3u" | "a3u" => pdf_compliance::PdfALevel::A3u,
            other => return Err(JsError::new(&format!("unknown PDF/A level: {other}"))),
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

    // ---- Page geometry ----

    /// Get page width in PDF points.
    #[wasm_bindgen(js_name = "pageWidth")]
    pub fn page_width(&self, page_index: usize) -> f64 {
        let pages = self.pdf.pages();
        if page_index >= pages.len() {
            return 0.0;
        }
        let mb = pages[page_index].media_box();
        (mb.x1 - mb.x0).abs()
    }

    /// Get page height in PDF points.
    #[wasm_bindgen(js_name = "pageHeight")]
    pub fn page_height(&self, page_index: usize) -> f64 {
        let pages = self.pdf.pages();
        if page_index >= pages.len() {
            return 0.0;
        }
        let mb = pages[page_index].media_box();
        (mb.y1 - mb.y0).abs()
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
        let pages = self.pdf.pages();
        if page_index >= pages.len() {
            return Err(JsError::new(&format!(
                "page index {page_index} out of range (0..{})",
                pages.len()
            )));
        }
        let page = &pages[page_index];
        let interp_settings = pdf_render::pdf_interpret::InterpreterSettings::default();
        let render_settings = pdf_render::RenderSettings {
            x_scale: scale,
            y_scale: scale,
            ..Default::default()
        };
        let pixmap = pdf_render::render(page, &interp_settings, &render_settings);
        let w = pixmap.width() as u32;
        let h = pixmap.height() as u32;
        let rgba = pixmap.data_as_u8_slice();
        let mut buf = Vec::with_capacity(8 + rgba.len());
        buf.extend_from_slice(&w.to_le_bytes());
        buf.extend_from_slice(&h.to_le_bytes());
        buf.extend_from_slice(rgba);
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
        let pages = self.pdf.pages();
        if page_index >= pages.len() {
            return Err(JsError::new(&format!(
                "page index {page_index} out of range (0..{})",
                pages.len()
            )));
        }
        let page = &pages[page_index];
        let media_box = page.media_box();
        let pw = (media_box.x1 - media_box.x0).abs() as f32;
        let ph = (media_box.y1 - media_box.y0).abs() as f32;
        let max_side = pw.max(ph);
        let scale = if max_side > 0.0 {
            max_dimension as f32 / max_side
        } else {
            1.0
        };
        self.render_page(page_index, scale)
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

    /// Add a highlight annotation to a page.
    ///
    /// Takes the PDF bytes and returns new PDF bytes with the annotation added.
    #[cfg(feature = "annotate")]
    #[wasm_bindgen(js_name = "addHighlight")]
    #[allow(clippy::too_many_arguments)]
    pub fn add_highlight(
        data: &[u8],
        page_index: u32,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        r: f64,
        g: f64,
        b: f64,
    ) -> Result<Vec<u8>, JsError> {
        let mut doc = lopdf::Document::load_mem(data).map_err(|e| JsError::new(&format!("{e}")))?;
        let rect = pdf_annot::builder::AnnotRect { x0, y0, x1, y1 };
        let annot_id = pdf_annot::builder::AnnotationBuilder::highlight(rect)
            .color(r, g, b)
            .quad_points_from_rect(&rect)
            .build(&mut doc)
            .map_err(|e| JsError::new(&format!("{e}")))?;
        pdf_annot::builder::add_annotation_to_page(&mut doc, page_index, annot_id)
            .map_err(|e| JsError::new(&format!("{e}")))?;
        let mut buf = Vec::new();
        doc.save_to(&mut buf)
            .map_err(|e| JsError::new(&format!("{e}")))?;
        Ok(buf)
    }

    /// Add a sticky note (text annotation) to a page.
    ///
    /// Returns new PDF bytes with the annotation.
    #[cfg(feature = "annotate")]
    #[wasm_bindgen(js_name = "addStickyNote")]
    pub fn add_sticky_note(
        data: &[u8],
        page_index: u32,
        x: f64,
        y: f64,
        text: &str,
    ) -> Result<Vec<u8>, JsError> {
        let mut doc = lopdf::Document::load_mem(data).map_err(|e| JsError::new(&format!("{e}")))?;
        let rect = pdf_annot::builder::AnnotRect {
            x0: x,
            y0: y,
            x1: x + 24.0,
            y1: y + 24.0,
        };
        let annot_id = pdf_annot::builder::AnnotationBuilder::sticky_note(
            rect,
            pdf_annot::builder::TextIcon::Note,
        )
        .contents(text)
        .color(1.0, 0.95, 0.0)
        .build(&mut doc)
        .map_err(|e| JsError::new(&format!("{e}")))?;
        pdf_annot::builder::add_annotation_to_page(&mut doc, page_index, annot_id)
            .map_err(|e| JsError::new(&format!("{e}")))?;
        let mut buf = Vec::new();
        doc.save_to(&mut buf)
            .map_err(|e| JsError::new(&format!("{e}")))?;
        Ok(buf)
    }

    /// Add a free text annotation to a page.
    ///
    /// Returns new PDF bytes with the annotation.
    #[cfg(feature = "annotate")]
    #[wasm_bindgen(js_name = "addFreeText")]
    #[allow(clippy::too_many_arguments)]
    pub fn add_free_text(
        data: &[u8],
        page_index: u32,
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        text: &str,
        font_size: f64,
    ) -> Result<Vec<u8>, JsError> {
        let mut doc = lopdf::Document::load_mem(data).map_err(|e| JsError::new(&format!("{e}")))?;
        let rect = pdf_annot::builder::AnnotRect { x0, y0, x1, y1 };
        let annot_id = pdf_annot::builder::AnnotationBuilder::free_text(rect, text, font_size)
            .build(&mut doc)
            .map_err(|e| JsError::new(&format!("{e}")))?;
        pdf_annot::builder::add_annotation_to_page(&mut doc, page_index, annot_id)
            .map_err(|e| JsError::new(&format!("{e}")))?;
        let mut buf = Vec::new();
        doc.save_to(&mut buf)
            .map_err(|e| JsError::new(&format!("{e}")))?;
        Ok(buf)
    }

    // ---- Signature verification ----

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
}
