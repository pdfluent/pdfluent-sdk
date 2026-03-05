//! End-to-end pipelines — PDF ↔ JSON, PDF → rendered images, PDF → flat PDF.
//!
//! Connects the full chain: PDF → XFA extraction → layout → rendering/flattening.

use crate::error::{PdfError, Result};
use crate::flatten::{flatten_to_pdf, FlattenConfig};
use crate::native_renderer::{render_layout, RenderConfig};
use crate::pdf_reader::PdfReader;
use crate::template_parser;
use crate::xfa_extract::XfaPackets;
use image::DynamicImage;
use std::path::Path;
use xfa_layout_engine::form::{FormNodeId, FormTree};
use xfa_layout_engine::layout::{LayoutDom, LayoutEngine};
use xfa_layout_engine::scripting;

/// Render a pre-built `LayoutDom` to page images.
///
/// This is the primary entry point when the form tree is already constructed
/// and laid out. Returns one image per page.
pub fn render_layout_dom(layout: &LayoutDom, config: &RenderConfig) -> Vec<DynamicImage> {
    render_layout(layout, config)
}

/// Render a `FormTree` to page images.
///
/// Runs scripting (calculate/validate), then performs layout and rendering.
pub fn render_form_tree(
    form: &mut FormTree,
    root: FormNodeId,
    config: &RenderConfig,
) -> Result<Vec<DynamicImage>> {
    // Run calculate scripts to populate computed field values.
    scripting::run_calculations(form)
        .map_err(|e| PdfError::RenderError(format!("scripting: {e}")))?;

    // Layout the form tree into pages.
    let engine = LayoutEngine::new(form);
    let layout = engine
        .layout(root)
        .map_err(|e| PdfError::RenderError(format!("layout: {e}")))?;

    // Render layout to images.
    Ok(render_layout(&layout, config))
}

/// Extract XFA packets from a PDF file.
pub fn extract_xfa_from_file(path: &Path) -> Result<XfaPackets> {
    let reader = PdfReader::from_file(path)?;
    reader.extract_xfa()
}

/// Extract XFA packets from PDF bytes.
pub fn extract_xfa_from_bytes(bytes: &[u8]) -> Result<XfaPackets> {
    let reader = PdfReader::from_bytes(bytes)?;
    reader.extract_xfa()
}

/// Save rendered page images to PNG files.
///
/// Files are written to `{dir}/{prefix}_page_{n}.png`.
/// Returns the list of written file paths.
pub fn save_pages_as_png(
    images: &[DynamicImage],
    dir: &Path,
    prefix: &str,
) -> Result<Vec<std::path::PathBuf>> {
    std::fs::create_dir_all(dir)?;
    let mut paths = Vec::with_capacity(images.len());
    for (i, img) in images.iter().enumerate() {
        let path = dir.join(format!("{prefix}_page_{i}.png"));
        img.save(&path)
            .map_err(|e| PdfError::RenderError(format!("save PNG: {e}")))?;
        paths.push(path);
    }
    Ok(paths)
}

/// End-to-end pipeline: PDF bytes → JSON with field values, metadata, and schema.
///
/// Steps:
/// 1. Extract XFA packets from the PDF
/// 2. Parse the template into a FormTree, merging data values
/// 3. Run FormCalc calculate scripts
/// 4. Export field values and schema as JSON
///
/// Returns a JSON object with `fields` (values) and `schema` (field metadata).
pub fn pdf_to_json(bytes: &[u8]) -> Result<serde_json::Value> {
    let reader = PdfReader::from_bytes(bytes)?;
    let packets = reader.extract_xfa()?;

    let template_xml = packets
        .template()
        .ok_or_else(|| PdfError::XfaPacketNotFound("no template packet in XFA".to_string()))?;

    let datasets_xml = packets.datasets();

    let (mut tree, root) = template_parser::parse_template(template_xml, datasets_xml)?;

    // Run calculate scripts to compute derived field values.
    let _ = scripting::run_calculations(&mut tree);

    // Export field data and schema.
    let data = xfa_json::form_tree_to_json(&tree, root);
    let schema = xfa_json::export_schema(&tree, root);

    let result = serde_json::json!({
        "fields": serde_json::to_value(&data).unwrap_or(serde_json::Value::Null),
        "schema": serde_json::to_value(&schema).unwrap_or(serde_json::Value::Null),
    });

    Ok(result)
}

/// End-to-end pipeline: PDF file → JSON with field values and schema.
pub fn pdf_file_to_json(path: &Path) -> Result<serde_json::Value> {
    let bytes = std::fs::read(path)?;
    pdf_to_json(&bytes)
}

/// End-to-end pipeline: merge JSON data into an XFA PDF template.
///
/// Steps:
/// 1. Extract XFA packets from the template PDF
/// 2. Parse the template into a FormTree
/// 3. Import JSON field values into the FormTree
/// 4. Run FormCalc calculate scripts
/// 5. Serialize updated data back into the PDF's datasets packet
/// 6. Return the updated PDF bytes
///
/// The `data` parameter accepts the `fields` portion of `pdf_to_json` output,
/// i.e. `{"fields": {"form1.Name": "Alice", ...}}` or a flat key-value object.
pub fn json_to_pdf(template: &[u8], data: &serde_json::Value) -> Result<Vec<u8>> {
    let mut reader = PdfReader::from_bytes(template)?;
    let packets = reader.extract_xfa()?;

    let template_xml = packets
        .template()
        .ok_or_else(|| PdfError::XfaPacketNotFound("no template packet in XFA".to_string()))?;

    let datasets_xml = packets.datasets();

    let (mut tree, root) = template_parser::parse_template(template_xml, datasets_xml)?;

    // Parse the JSON data into FormData and import into the FormTree.
    let form_data = parse_json_input(data)?;
    xfa_json::json_to_form_tree(&form_data, &mut tree, root);

    // Run calculate scripts to update computed fields.
    let _ = scripting::run_calculations(&mut tree);

    // Build a DataDom from the updated FormTree and sync into the PDF.
    let data_dom = form_tree_to_data_dom(&tree, root);
    crate::dataset_sync::sync_datasets(&mut reader, &data_dom)?;

    reader.save_to_bytes()
}

/// Parse JSON input into FormData.
///
/// Accepts either:
/// - `{"fields": {"form1.Name": "Alice"}}` (pdf_to_json output format)
/// - `{"form1.Name": "Alice"}` (flat key-value)
fn parse_json_input(data: &serde_json::Value) -> Result<xfa_json::FormData> {
    // Try the wrapped format first
    if let Some(fields_obj) = data.get("fields") {
        if let Ok(form_data) = serde_json::from_value::<xfa_json::FormData>(fields_obj.clone()) {
            return Ok(form_data);
        }
        // If fields is a plain object, try parsing directly
        if let Ok(form_data) = serde_json::from_value::<xfa_json::FormData>(data.clone()) {
            return Ok(form_data);
        }
    }

    // Try flat format: the entire value is the fields map
    if data.is_object() {
        // Convert plain JSON object to FormData with string-keyed FieldValues
        let mut fields = indexmap::IndexMap::new();
        if let Some(obj) = data.as_object() {
            for (key, val) in obj {
                let field_val = json_value_to_field_value(val);
                fields.insert(key.clone(), field_val);
            }
        }
        return Ok(xfa_json::FormData { fields });
    }

    Err(PdfError::LoadFailed(
        "invalid JSON data: expected object with field values".to_string(),
    ))
}

/// Convert a serde_json::Value to a FieldValue.
fn json_value_to_field_value(val: &serde_json::Value) -> xfa_json::FieldValue {
    match val {
        serde_json::Value::Null => xfa_json::FieldValue::Null,
        serde_json::Value::Bool(b) => xfa_json::FieldValue::Boolean(*b),
        serde_json::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                xfa_json::FieldValue::Number(f)
            } else {
                xfa_json::FieldValue::Text(n.to_string())
            }
        }
        serde_json::Value::String(s) => xfa_json::FieldValue::Text(s.clone()),
        serde_json::Value::Array(arr) => {
            let instances: Vec<indexmap::IndexMap<String, xfa_json::FieldValue>> = arr
                .iter()
                .filter_map(|item| {
                    item.as_object().map(|obj| {
                        obj.iter()
                            .map(|(k, v)| (k.clone(), json_value_to_field_value(v)))
                            .collect()
                    })
                })
                .collect();
            xfa_json::FieldValue::Array(instances)
        }
        serde_json::Value::Object(_) => xfa_json::FieldValue::Text(val.to_string()),
    }
}

/// Build a DataDom from a FormTree's current field values.
///
/// Walks the FormTree and constructs a DataDom with the same structure,
/// suitable for writing back into the PDF's datasets packet.
fn form_tree_to_data_dom(tree: &FormTree, root: FormNodeId) -> xfa_dom_resolver::data_dom::DataDom {
    use xfa_dom_resolver::data_dom::{DataDom, DataNode};
    use xfa_layout_engine::form::FormNodeType;

    let mut dom = DataDom::new();

    fn walk(
        tree: &FormTree,
        node_id: FormNodeId,
        dom: &mut DataDom,
        parent: Option<xfa_dom_resolver::data_dom::DataNodeId>,
    ) {
        let node = tree.get(node_id);

        match &node.node_type {
            FormNodeType::Root | FormNodeType::PageSet | FormNodeType::PageArea { .. } => {
                // Skip structural nodes, recurse into children
                for &child_id in &node.children {
                    walk(tree, child_id, dom, parent);
                }
            }
            FormNodeType::Subform => {
                if node.name.is_empty() {
                    // Unnamed subform: recurse without creating a group
                    for &child_id in &node.children {
                        walk(tree, child_id, dom, parent);
                    }
                } else if let Some(pid) = parent {
                    let group_id = dom.create_group(pid, &node.name).unwrap();
                    for &child_id in &node.children {
                        walk(tree, child_id, dom, Some(group_id));
                    }
                } else if dom.root().is_none() {
                    // Root-level subform becomes the data root
                    let root_id = dom.alloc(DataNode::DataGroup {
                        name: node.name.clone(),
                        namespace: None,
                        children: Vec::new(),
                        is_record: false,
                        parent: None,
                    });
                    dom.set_root(root_id);
                    for &child_id in &node.children {
                        walk(tree, child_id, dom, Some(root_id));
                    }
                } else if let Some(rid) = dom.root() {
                    let group_id = dom.create_group(rid, &node.name).unwrap();
                    for &child_id in &node.children {
                        walk(tree, child_id, dom, Some(group_id));
                    }
                }
            }
            FormNodeType::Field { value } => {
                if let Some(pid) = parent {
                    let _ = dom.create_value(pid, &node.name, value);
                }
            }
            FormNodeType::Draw { .. } => {
                // Draw elements are not stored in the data DOM
            }
        }
    }

    let root_node = tree.get(root);
    match &root_node.node_type {
        FormNodeType::Root | FormNodeType::PageSet | FormNodeType::PageArea { .. } => {
            for &child_id in &root_node.children {
                walk(tree, child_id, &mut dom, None);
            }
        }
        _ => {
            walk(tree, root, &mut dom, None);
        }
    }

    dom
}

/// End-to-end pipeline: flatten a `FormTree` to a static PDF.
///
/// Steps:
/// 1. Run FormCalc calculate scripts
/// 2. Layout the form tree into pages
/// 3. Generate appearance streams for all fields/draws
/// 4. Embed as Form XObjects in a new static PDF (no AcroForm/XFA)
///
/// Returns the flattened PDF as bytes.
pub fn flatten_form_tree(
    form: &mut FormTree,
    root: FormNodeId,
    config: &FlattenConfig,
) -> Result<Vec<u8>> {
    // Run calculate scripts to populate computed field values.
    scripting::run_calculations(form)
        .map_err(|e| PdfError::RenderError(format!("scripting: {e}")))?;

    // Layout the form tree into pages.
    let engine = LayoutEngine::new(form);
    let layout = engine
        .layout(root)
        .map_err(|e| PdfError::RenderError(format!("layout: {e}")))?;

    flatten_to_pdf(&layout, config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use xfa_layout_engine::form::*;
    use xfa_layout_engine::text::FontMetrics;
    use xfa_layout_engine::types::*;

    fn simple_form() -> (FormTree, FormNodeId) {
        let mut tree = FormTree::new();
        let field = tree.add_node(FormNode {
            name: "Name".to_string(),
            node_type: FormNodeType::Field {
                value: "John".to_string(),
            },
            box_model: BoxModel {
                width: Some(200.0),
                height: Some(25.0),
                x: 20.0,
                y: 20.0,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });
        let root = tree.add_node(FormNode {
            name: "form1".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: Some(612.0),
                height: Some(792.0),
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![field],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });
        (tree, root)
    }

    #[test]
    fn render_form_tree_produces_images() {
        let (mut tree, root) = simple_form();
        let config = RenderConfig::default();
        let images = render_form_tree(&mut tree, root, &config).unwrap();
        assert_eq!(images.len(), 1);
        assert_eq!(images[0].width(), 612);
        assert_eq!(images[0].height(), 792);
    }

    #[test]
    fn render_form_tree_with_dpi_scaling() {
        let (mut tree, root) = simple_form();
        let config = RenderConfig::with_dpi(144.0);
        let images = render_form_tree(&mut tree, root, &config).unwrap();
        assert_eq!(images[0].width(), 1224); // 612 * 2
        assert_eq!(images[0].height(), 1584); // 792 * 2
    }

    #[test]
    fn render_form_tree_with_calculate_script() {
        let mut tree = FormTree::new();
        let field = tree.add_node(FormNode {
            name: "Total".to_string(),
            node_type: FormNodeType::Field {
                value: String::new(),
            },
            box_model: BoxModel {
                width: Some(100.0),
                height: Some(25.0),
                x: 10.0,
                y: 10.0,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: Some("10 + 20".to_string()),
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });
        let root = tree.add_node(FormNode {
            name: "form1".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: Some(200.0),
                height: Some(100.0),
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![field],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });
        let config = RenderConfig::default();
        let images = render_form_tree(&mut tree, root, &config).unwrap();
        assert_eq!(images.len(), 1);
        // The field value should have been computed by the calculate script
        if let FormNodeType::Field { value } = &tree.get(field).node_type {
            assert_eq!(value, "30");
        } else {
            panic!("expected Field node");
        }
    }

    #[test]
    fn save_pages_creates_files() {
        let (mut tree, root) = simple_form();
        let config = RenderConfig::default();
        let images = render_form_tree(&mut tree, root, &config).unwrap();

        let dir = std::env::temp_dir().join("xfa_pipeline_test");
        let paths = save_pages_as_png(&images, &dir, "test").unwrap();
        assert_eq!(paths.len(), 1);
        assert!(paths[0].exists());
        // Cleanup
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn extract_xfa_from_invalid_bytes_fails() {
        let result = extract_xfa_from_bytes(b"not a pdf");
        assert!(result.is_err());
    }

    #[test]
    fn flatten_form_tree_produces_valid_pdf() {
        let (mut tree, root) = simple_form();
        let config = FlattenConfig::default();
        let pdf_bytes = flatten_form_tree(&mut tree, root, &config).unwrap();
        assert!(!pdf_bytes.is_empty());

        // Should be a valid PDF
        let doc = lopdf::Document::load_mem(&pdf_bytes).unwrap();
        assert_eq!(doc.get_pages().len(), 1);

        // Should not have AcroForm (flattened)
        let catalog_id = match doc.trailer.get(b"Root").unwrap() {
            lopdf::Object::Reference(id) => *id,
            _ => panic!("No Root"),
        };
        let catalog = doc.get_object(catalog_id).unwrap().as_dict().unwrap();
        assert!(catalog.get(b"AcroForm").is_err());
    }

    #[test]
    fn flatten_form_tree_with_calculate_script() {
        let mut tree = FormTree::new();
        let field = tree.add_node(FormNode {
            name: "Total".to_string(),
            node_type: FormNodeType::Field {
                value: String::new(),
            },
            box_model: BoxModel {
                width: Some(100.0),
                height: Some(25.0),
                x: 10.0,
                y: 10.0,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: Some("10 + 20".to_string()),
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });
        let root = tree.add_node(FormNode {
            name: "form1".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: Some(200.0),
                height: Some(100.0),
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![field],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });
        let config = FlattenConfig::default();
        let pdf_bytes = flatten_form_tree(&mut tree, root, &config).unwrap();

        // Calculate script should have run before flattening
        if let FormNodeType::Field { value } = &tree.get(field).node_type {
            assert_eq!(value, "30");
        } else {
            panic!("expected Field node");
        }

        let doc = lopdf::Document::load_mem(&pdf_bytes).unwrap();
        assert_eq!(doc.get_pages().len(), 1);
    }
}
