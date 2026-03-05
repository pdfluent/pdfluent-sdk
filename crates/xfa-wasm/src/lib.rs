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
}
