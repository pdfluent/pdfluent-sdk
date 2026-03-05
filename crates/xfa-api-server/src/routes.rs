//! API route handlers.

use crate::error::ApiError;
use crate::state::AppState;
use axum::extract::{Multipart, Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use pdfium_ffi_bridge::pdf_reader::PdfReader;
use serde::Serialize;
use xfa_layout_engine::form::FormNodeId;

/// Health check response.
#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
}

/// Extract response — JSON field values.
#[derive(Serialize)]
pub struct ExtractResponse {
    pub form_id: String,
    pub fields: serde_json::Value,
    pub field_count: usize,
}

/// Validate response.
#[derive(Serialize)]
pub struct ValidateResponse {
    pub valid: bool,
    pub has_xfa: bool,
    pub has_acroform: bool,
    pub page_count: usize,
    pub issues: Vec<String>,
}

/// GET /health — health check.
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// POST /api/v1/forms/extract — extract field values from XFA PDF.
///
/// Accepts multipart/form-data with a `file` field containing the PDF.
/// Returns JSON field values.
pub async fn extract_fields(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<ExtractResponse>, ApiError> {
    let pdf_bytes = read_pdf_from_multipart(&mut multipart).await?;

    let reader =
        PdfReader::from_bytes(&pdf_bytes).map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let packets = reader
        .extract_xfa()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let template_xml = packets
        .template()
        .ok_or_else(|| ApiError::BadRequest("no template packet in XFA".to_string()))?;

    let datasets_xml = packets.datasets();

    let (tree, root) = build_form_tree(template_xml, datasets_xml).map_err(ApiError::Internal)?;

    let json = xfa_json::form_tree_to_value(&tree, root);
    let field_count = count_fields(&json);

    // Store for later retrieval
    let form_id = uuid::Uuid::new_v4().to_string();
    {
        let mut forms = state.forms.lock().unwrap();
        forms.insert(form_id.clone(), crate::state::StoredForm { pdf_bytes });
    }

    Ok(Json(ExtractResponse {
        form_id,
        fields: json,
        field_count,
    }))
}

/// POST /api/v1/forms/fill — fill an XFA PDF with JSON field values.
///
/// Accepts multipart/form-data with `file` (PDF) and `data` (JSON) fields.
/// Returns the filled PDF.
pub async fn fill_form(mut multipart: Multipart) -> Result<Response, ApiError> {
    let mut pdf_bytes = None;
    let mut json_data = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(format!("multipart error: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                pdf_bytes = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| ApiError::BadRequest(format!("read file: {e}")))?,
                );
            }
            "data" => {
                let text = field
                    .text()
                    .await
                    .map_err(|e| ApiError::BadRequest(format!("read data: {e}")))?;
                json_data = Some(text);
            }
            _ => {}
        }
    }

    let pdf_bytes =
        pdf_bytes.ok_or_else(|| ApiError::BadRequest("missing 'file' field".to_string()))?;
    let json_text =
        json_data.ok_or_else(|| ApiError::BadRequest("missing 'data' field".to_string()))?;

    let form_data: xfa_json::FormData =
        serde_json::from_str(&json_text).map_err(|e| ApiError::BadRequest(format!("JSON: {e}")))?;

    let mut reader =
        PdfReader::from_bytes(&pdf_bytes).map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let packets = reader
        .extract_xfa()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let template_xml = packets
        .template()
        .ok_or_else(|| ApiError::BadRequest("no template packet".to_string()))?;

    let datasets_xml = packets.datasets();

    let (mut tree, root) =
        build_form_tree(template_xml, datasets_xml).map_err(ApiError::Internal)?;

    // Apply JSON data to FormTree
    xfa_json::json_to_form_tree(&form_data, &mut tree, root);

    // Convert FormTree back to data XML and sync into PDF
    let data_json = xfa_json::form_tree_to_json(&tree, root);
    let data_xml = form_data_to_xml(&data_json);
    let data_dom = xfa_dom_resolver::data_dom::DataDom::from_xml(&data_xml)
        .map_err(|e| ApiError::Internal(format!("build data DOM: {e}")))?;

    pdfium_ffi_bridge::dataset_sync::sync_datasets(&mut reader, &data_dom)
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let output = reader
        .save_to_bytes()
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/pdf"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"filled.pdf\"",
            ),
        ],
        output,
    )
        .into_response())
}

/// POST /api/v1/forms/validate — validate a PDF's XFA structure.
pub async fn validate_form(mut multipart: Multipart) -> Result<Json<ValidateResponse>, ApiError> {
    let pdf_bytes = read_pdf_from_multipart(&mut multipart).await?;
    let mut issues = Vec::new();

    let reader = match PdfReader::from_bytes(&pdf_bytes) {
        Ok(r) => r,
        Err(e) => {
            return Ok(Json(ValidateResponse {
                valid: false,
                has_xfa: false,
                has_acroform: false,
                page_count: 0,
                issues: vec![format!("Invalid PDF: {e}")],
            }))
        }
    };

    let page_count = reader.page_count();
    let has_xfa;
    let has_acroform;

    match reader.extract_xfa() {
        Ok(packets) => {
            has_xfa = true;
            has_acroform = true;

            if packets.template().is_none() {
                issues.push("Missing template packet".to_string());
            }
            if packets.datasets().is_none() {
                issues.push("Missing datasets packet".to_string());
            }

            // Try to parse the template
            if let Some(tmpl) = packets.template() {
                if roxmltree::Document::parse(tmpl).is_err() {
                    issues.push("Template XML is malformed".to_string());
                }
            }
        }
        Err(_) => {
            has_xfa = false;
            // Check for AcroForm without XFA
            has_acroform = reader
                .document()
                .trailer
                .get_deref(b"Root", reader.document())
                .and_then(|o| o.as_dict())
                .ok()
                .and_then(|cat| cat.get(b"AcroForm").ok())
                .is_some();

            if !has_xfa {
                issues.push("No XFA content found".to_string());
            }
        }
    }

    Ok(Json(ValidateResponse {
        valid: issues.is_empty() && has_xfa,
        has_xfa,
        has_acroform,
        page_count,
        issues,
    }))
}

/// GET /api/v1/forms/{id}/schema — get JSON Schema for a stored form.
pub async fn get_schema(
    State(state): State<AppState>,
    Path(form_id): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let pdf_bytes = {
        let forms = state.forms.lock().unwrap();
        forms
            .get(&form_id)
            .map(|f| f.pdf_bytes.clone())
            .ok_or_else(|| ApiError::NotFound(format!("form {form_id} not found")))?
    };

    let reader =
        PdfReader::from_bytes(&pdf_bytes).map_err(|e| ApiError::Internal(e.to_string()))?;

    let packets = reader
        .extract_xfa()
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let template_xml = packets
        .template()
        .ok_or_else(|| ApiError::Internal("no template".to_string()))?;

    let datasets_xml = packets.datasets();

    let (tree, root) = build_form_tree(template_xml, datasets_xml).map_err(ApiError::Internal)?;

    let schema = xfa_json::export_schema(&tree, root);
    let schema_json = serde_json::to_value(&schema)
        .map_err(|e| ApiError::Internal(format!("serialize schema: {e}")))?;

    Ok(Json(schema_json))
}

/// Read PDF bytes from the "file" field in a multipart upload.
async fn read_pdf_from_multipart(multipart: &mut Multipart) -> Result<Vec<u8>, ApiError> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::BadRequest(format!("multipart error: {e}")))?
    {
        if field.name() == Some("file") {
            let bytes = field
                .bytes()
                .await
                .map_err(|e| ApiError::BadRequest(format!("read file: {e}")))?;
            return Ok(bytes.to_vec());
        }
    }

    Err(ApiError::BadRequest(
        "missing 'file' field in multipart upload".to_string(),
    ))
}

/// Build a FormTree from template XML and optional datasets XML.
fn build_form_tree(
    template_xml: &str,
    _datasets_xml: Option<&str>,
) -> std::result::Result<(xfa_layout_engine::form::FormTree, FormNodeId), String> {
    let template_doc =
        roxmltree::Document::parse(template_xml).map_err(|e| format!("parse template: {e}"))?;

    let mut tree = xfa_layout_engine::form::FormTree::new();

    // Find the root subform in the template
    let root_el = template_doc
        .root_element()
        .children()
        .find(|n| n.is_element() && n.tag_name().name() == "subform")
        .ok_or_else(|| "no root subform in template".to_string())?;

    let root_id = build_node_recursive(&mut tree, &root_el)
        .ok_or_else(|| "failed to build root node".to_string())?;

    Ok((tree, root_id))
}

/// Recursively build FormTree nodes from template XML.
fn build_node_recursive(
    tree: &mut xfa_layout_engine::form::FormTree,
    el: &roxmltree::Node,
) -> Option<FormNodeId> {
    use xfa_layout_engine::form::*;
    use xfa_layout_engine::text::FontMetrics;
    use xfa_layout_engine::types::*;

    let tag = el.tag_name().name();

    match tag {
        "subform" => {
            let name = el.attribute("name").unwrap_or("").to_string();
            let layout = parse_layout(el);
            let box_model = parse_box_model(el);

            let node_id = tree.add_node(FormNode {
                name,
                node_type: FormNodeType::Subform,
                box_model,
                layout,
                children: vec![],
                occur: Occur::once(),
                font: FontMetrics::default(),
                calculate: None,
                validate: None,
                column_widths: vec![],
                col_span: 1,
            });

            for child in el.children().filter(|c| c.is_element()) {
                if let Some(child_id) = build_node_recursive(tree, &child) {
                    tree.get_mut(node_id).children.push(child_id);
                }
            }

            Some(node_id)
        }
        "field" => {
            let name = el.attribute("name").unwrap_or("").to_string();
            let box_model = parse_box_model(el);

            let node_id = tree.add_node(FormNode {
                name,
                node_type: FormNodeType::Field {
                    value: String::new(),
                },
                box_model,
                layout: LayoutStrategy::Positioned,
                children: vec![],
                occur: Occur::once(),
                font: FontMetrics::default(),
                calculate: None,
                validate: None,
                column_widths: vec![],
                col_span: 1,
            });

            Some(node_id)
        }
        _ => None,
    }
}

/// Parse layout strategy from an element.
fn parse_layout(el: &roxmltree::Node) -> xfa_layout_engine::types::LayoutStrategy {
    use xfa_layout_engine::types::LayoutStrategy;

    match el.attribute("layout") {
        Some("tb") => LayoutStrategy::TopToBottom,
        Some("lr-tb") => LayoutStrategy::LeftToRightTB,
        Some("rl-tb") => LayoutStrategy::RightToLeftTB,
        Some("table") => LayoutStrategy::Table,
        _ => LayoutStrategy::Positioned,
    }
}

/// Parse box model from an element's attributes.
fn parse_box_model(el: &roxmltree::Node) -> xfa_layout_engine::types::BoxModel {
    use xfa_layout_engine::types::{BoxModel, Measurement};

    let mut bm = BoxModel::default();

    if let Some(w) = el.attribute("w") {
        bm.width = Measurement::parse(w).map(|m| m.to_points());
    }
    if let Some(h) = el.attribute("h") {
        bm.height = Measurement::parse(h).map(|m| m.to_points());
    }

    for child in el.children().filter(|c| c.is_element()) {
        if child.tag_name().name() == "margin" {
            if let Some(l) = child.attribute("leftInset") {
                bm.margins.left = Measurement::parse(l).map_or(0.0, |m| m.to_points());
            }
            if let Some(t) = child.attribute("topInset") {
                bm.margins.top = Measurement::parse(t).map_or(0.0, |m| m.to_points());
            }
            if let Some(r) = child.attribute("rightInset") {
                bm.margins.right = Measurement::parse(r).map_or(0.0, |m| m.to_points());
            }
            if let Some(b) = child.attribute("bottomInset") {
                bm.margins.bottom = Measurement::parse(b).map_or(0.0, |m| m.to_points());
            }
        }
    }

    bm
}

/// Convert FormData to a simple XML string for DataDom.
fn form_data_to_xml(data: &xfa_json::FormData) -> String {
    let mut xml = String::from("<form1>");
    for (key, value) in &data.fields {
        let text = match value {
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
            xfa_json::FieldValue::Array(_) => continue, // Skip arrays in flat XML
        };
        let escaped = text
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        xml.push_str(&format!("<{key}>{escaped}</{key}>"));
    }
    xml.push_str("</form1>");
    xml
}

/// Count the number of fields in a JSON value.
fn count_fields(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::Object(map) => map
            .values()
            .map(|v| match v {
                serde_json::Value::Object(_) => count_fields(v),
                _ => 1,
            })
            .sum(),
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_endpoint_returns_ok() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let response = health().await;
            assert_eq!(response.status, "ok");
        });
    }

    #[test]
    fn count_fields_works() {
        let json: serde_json::Value = serde_json::json!({
            "Name": "John",
            "Address": {
                "Street": "123 Main St",
                "City": "Springfield"
            }
        });
        assert_eq!(count_fields(&json), 3);
    }

    #[test]
    fn count_fields_empty() {
        let json = serde_json::json!({});
        assert_eq!(count_fields(&json), 0);
    }

    #[test]
    fn parse_layout_from_attribute() {
        let xml = r#"<subform layout="tb" xmlns="http://www.xfa.org/schema/xfa-template/3.3/"/>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let el = doc.root_element();
        let layout = parse_layout(&el);
        assert!(matches!(
            layout,
            xfa_layout_engine::types::LayoutStrategy::TopToBottom
        ));
    }

    #[test]
    fn parse_layout_default_positioned() {
        let xml = r#"<subform xmlns="http://www.xfa.org/schema/xfa-template/3.3/"/>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let el = doc.root_element();
        let layout = parse_layout(&el);
        assert!(matches!(
            layout,
            xfa_layout_engine::types::LayoutStrategy::Positioned
        ));
    }

    #[test]
    fn parse_box_model_with_dimensions() {
        let xml =
            r#"<field w="200pt" h="25pt" xmlns="http://www.xfa.org/schema/xfa-template/3.3/"/>"#;
        let doc = roxmltree::Document::parse(xml).unwrap();
        let el = doc.root_element();
        let bm = parse_box_model(&el);
        assert_eq!(bm.width, Some(200.0));
        assert_eq!(bm.height, Some(25.0));
    }

    #[test]
    fn build_form_tree_from_template() {
        let template = r#"<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
            <subform name="form1">
                <field name="Name"/>
                <field name="Email"/>
            </subform>
        </template>"#;

        let (tree, root) = build_form_tree(template, None).unwrap();
        assert_eq!(tree.get(root).name, "form1");
        assert_eq!(tree.get(root).children.len(), 2);
    }

    #[test]
    fn form_data_to_xml_basic() {
        let mut data = xfa_json::FormData {
            fields: indexmap::IndexMap::new(),
        };
        data.fields.insert(
            "Name".to_string(),
            xfa_json::FieldValue::Text("John".to_string()),
        );
        data.fields
            .insert("Age".to_string(), xfa_json::FieldValue::Number(30.0));

        let xml = form_data_to_xml(&data);
        assert!(xml.contains("<Name>John</Name>"));
        assert!(xml.contains("<Age>30</Age>"));
    }

    #[test]
    fn form_data_to_xml_escapes_special_chars() {
        let mut data = xfa_json::FormData {
            fields: indexmap::IndexMap::new(),
        };
        data.fields.insert(
            "Note".to_string(),
            xfa_json::FieldValue::Text("a < b & c > d".to_string()),
        );

        let xml = form_data_to_xml(&data);
        assert!(xml.contains("a &lt; b &amp; c &gt; d"));
    }
}
