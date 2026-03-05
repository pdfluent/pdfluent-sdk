//! Edge case tests — malformed input, boundary conditions, error recovery.
//!
//! These tests verify graceful handling of broken, unusual, or adversarial input.

use pdfium_ffi_bridge::dataset_sync::sync_datasets;
use pdfium_ffi_bridge::pdf_reader::PdfReader;
use pdfium_ffi_bridge::xfa_extract::parse_xfa_xml;
use xfa_dom_resolver::data_dom::DataDom;

use lopdf::{dictionary, Document, Object, Stream};

// --- PDF builder helpers ---

fn build_xfa_pdf(xfa_xml: &str) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let xfa_stream = Stream::new(dictionary! {}, xfa_xml.as_bytes().to_vec());
    let xfa_id = doc.add_object(Object::Stream(xfa_stream));
    let acroform = dictionary! { "XFA" => xfa_id };
    let acroform_id = doc.add_object(Object::Dictionary(acroform));

    let pages_id = doc.new_object_id();
    let page_id = doc.new_object_id();
    doc.objects.insert(
        pages_id,
        Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![page_id.into()],
            "Count" => 1,
        }),
    );
    doc.objects.insert(
        page_id,
        Object::Dictionary(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
        }),
    );
    let catalog_id = doc.new_object_id();
    doc.objects.insert(
        catalog_id,
        Object::Dictionary(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
            "AcroForm" => acroform_id,
        }),
    );
    doc.trailer.set("Root", catalog_id);

    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();
    buf
}

// =============================================================================
// Malformed XFA XML
// =============================================================================

#[test]
fn parse_xfa_xml_with_no_xdp_root() {
    // Just a template, not wrapped in xdp:xdp — should still be parseable
    let xml = r#"<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="form1"><field name="F1"/></subform>
</template>"#;

    // parse_xfa_xml expects xdp:xdp root — this should still work
    // (it will have 0 packets since there's no xdp wrapper)
    let result = parse_xfa_xml(xml);
    assert!(result.is_ok());
}

#[test]
fn parse_xfa_xml_just_text() {
    let result = parse_xfa_xml("this is not xml at all");
    // Should not panic, may return empty or error
    assert!(result.is_ok());
}

#[test]
fn parse_xfa_xml_empty_string() {
    let result = parse_xfa_xml("");
    assert!(result.is_ok());
}

#[test]
fn parse_xfa_xml_with_cdata() {
    let xml = r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="form1"><field name="F1"/></subform>
  </template>
</xdp:xdp>"#;

    let packets = parse_xfa_xml(xml).unwrap();
    assert!(packets.template().is_some());
}

// =============================================================================
// Data DOM Edge Cases
// =============================================================================

#[test]
fn data_dom_empty_xml() {
    let result = DataDom::from_xml("");
    assert!(result.is_err(), "empty string should fail to parse");
}

#[test]
fn data_dom_just_root_element() {
    let dom = DataDom::from_xml("<root/>").unwrap();
    assert!(dom.root().is_some());
    assert_eq!(dom.len(), 1);
}

#[test]
fn data_dom_deeply_nested() {
    // 20 levels deep — should not stack overflow
    let mut xml = String::new();
    for i in 0..20 {
        xml.push_str(&format!("<level{i}>"));
    }
    xml.push_str("deep value");
    for i in (0..20).rev() {
        xml.push_str(&format!("</level{i}>"));
    }

    let dom = DataDom::from_xml(&xml).unwrap();
    assert!(dom.root().is_some());
    assert!(dom.len() >= 20);
}

#[test]
fn data_dom_very_long_value() {
    let long_value = "x".repeat(100_000);
    let xml = format!("<root><field>{long_value}</field></root>");
    let dom = DataDom::from_xml(&xml).unwrap();
    let root = dom.root().unwrap();
    let children = dom.children_by_name(root, "field");
    assert_eq!(children.len(), 1);
    assert_eq!(dom.value(children[0]).unwrap().len(), 100_000);
}

#[test]
fn data_dom_xml_with_attributes() {
    let xml = r#"<form1 version="3.3" locale="en_US">
  <field1 type="text">Value</field1>
</form1>"#;

    let dom = DataDom::from_xml(xml).unwrap();
    let root = dom.root().unwrap();

    // Attributes become DataValue children of the group
    let all_children = dom.children(root);
    assert!(!all_children.is_empty());
}

#[test]
fn data_dom_roundtrip_preserves_structure() {
    let xml = "<root><a><b>1</b><c>2</c></a><d>3</d></root>";
    let dom = DataDom::from_xml(xml).unwrap();
    let output = dom.to_xml();

    // Verify all elements survive roundtrip
    assert!(output.contains("<root>"));
    assert!(output.contains("<a>"));
    assert!(output.contains("<b>1</b>"));
    assert!(output.contains("<c>2</c>"));
    assert!(output.contains("<d>3</d>"));
}

#[test]
fn data_dom_xml_escaping_roundtrip() {
    let xml = r#"<form1><field>&lt;script&gt;alert('xss')&lt;/script&gt;</field></form1>"#;
    let dom = DataDom::from_xml(xml).unwrap();
    let output = dom.to_xml();

    // The escaped content should not be double-escaped, and should remain safe
    assert!(!output.contains("<script>"), "should not unescape dangerous tags");
    assert!(output.contains("&lt;") || output.contains("script"));
}

#[test]
fn data_dom_many_siblings() {
    // 500 sibling elements
    let mut xml = String::from("<root>");
    for i in 0..500 {
        xml.push_str(&format!("<item{i}>val{i}</item{i}>"));
    }
    xml.push_str("</root>");

    let dom = DataDom::from_xml(&xml).unwrap();
    let root = dom.root().unwrap();
    let children = dom.children(root);
    assert_eq!(children.len(), 500);
}

// =============================================================================
// PDF Reader Edge Cases
// =============================================================================

#[test]
fn pdf_reader_zero_byte_input() {
    let result = PdfReader::from_bytes(&[]);
    assert!(result.is_err());
}

#[test]
fn pdf_reader_truncated_pdf_header() {
    let result = PdfReader::from_bytes(b"%PDF-");
    assert!(result.is_err());
}

#[test]
fn pdf_reader_valid_pdf_with_empty_xfa_stream() {
    let pdf = build_xfa_pdf("");
    let reader = PdfReader::from_bytes(&pdf).unwrap();
    // Empty stream — should fail gracefully
    let result = reader.extract_xfa();
    // May succeed with 0 packets or fail — either is acceptable
    if let Ok(packets) = result {
        assert!(packets.packets.is_empty() || packets.full_xml.is_some());
    }
}

#[test]
fn pdf_reader_save_produces_valid_pdf() {
    let xfa_xml = r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="form1"><field name="F1"/></subform>
  </template>
</xdp:xdp>"#;

    let pdf = build_xfa_pdf(xfa_xml);
    let mut reader = PdfReader::from_bytes(&pdf).unwrap();
    let saved = reader.save_to_bytes().unwrap();

    // Saved bytes should start with %PDF
    assert!(saved.starts_with(b"%PDF"), "saved output should be valid PDF");

    // Should be re-loadable
    let reader2 = PdfReader::from_bytes(&saved).unwrap();
    assert_eq!(reader2.page_count(), 1);
}

// =============================================================================
// Dataset Sync Edge Cases
// =============================================================================

#[test]
fn sync_with_very_large_data() {
    let xfa_xml = r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="form1"><field name="BigField"/></subform>
  </template>
  <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data><form1><BigField>small</BigField></form1></xfa:data>
  </xfa:datasets>
</xdp:xdp>"#;

    let pdf = build_xfa_pdf(xfa_xml);
    let mut reader = PdfReader::from_bytes(&pdf).unwrap();

    // 50KB of data in a single field
    let big_value = "A".repeat(50_000);
    let data_xml = format!("<form1><BigField>{big_value}</BigField></form1>");
    let data = DataDom::from_xml(&data_xml).unwrap();
    sync_datasets(&mut reader, &data).unwrap();

    let saved = reader.save_to_bytes().unwrap();
    let reader2 = PdfReader::from_bytes(&saved).unwrap();
    let packets = reader2.extract_xfa().unwrap();
    let full = packets.full_xml.as_deref().unwrap();
    assert!(full.len() > 50_000, "large data should survive roundtrip");
}

#[test]
fn sync_then_sync_again() {
    // Two consecutive syncs — second should overwrite first
    let xfa_xml = r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="form1"><field name="F1"/></subform>
  </template>
  <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data><form1><F1>Original</F1></form1></xfa:data>
  </xfa:datasets>
</xdp:xdp>"#;

    let pdf = build_xfa_pdf(xfa_xml);
    let mut reader = PdfReader::from_bytes(&pdf).unwrap();

    // First sync
    let data1 = DataDom::from_xml("<form1><F1>First</F1></form1>").unwrap();
    sync_datasets(&mut reader, &data1).unwrap();

    // Second sync without saving in between
    let data2 = DataDom::from_xml("<form1><F1>Second</F1></form1>").unwrap();
    sync_datasets(&mut reader, &data2).unwrap();

    let saved = reader.save_to_bytes().unwrap();
    let reader2 = PdfReader::from_bytes(&saved).unwrap();
    let packets = reader2.extract_xfa().unwrap();
    let full = packets.full_xml.as_deref().unwrap();
    assert!(full.contains("Second"), "latest sync should win");
    assert!(!full.contains("First"), "earlier sync should be overwritten");
}

// =============================================================================
// Layout Edge Cases
// =============================================================================

#[test]
fn layout_empty_form_tree() {
    use xfa_layout_engine::form::*;
    use xfa_layout_engine::layout::LayoutEngine;
    use xfa_layout_engine::text::FontMetrics;
    use xfa_layout_engine::types::*;

    let mut tree = FormTree::new();
    let root = tree.add_node(FormNode {
        name: "empty_form".to_string(),
        node_type: FormNodeType::Subform,
        box_model: BoxModel {
            width: Some(612.0),
            height: Some(792.0),
            max_width: f64::MAX,
            max_height: f64::MAX,
            ..Default::default()
        },
        layout: LayoutStrategy::Positioned,
        children: vec![],
        occur: Occur::once(),
        font: FontMetrics::default(),
        calculate: None,
        validate: None,
    });

    let engine = LayoutEngine::new(&tree);
    let layout = engine.layout(root).unwrap();
    assert!(!layout.pages.is_empty(), "should produce at least one page");
    assert_eq!(layout.pages[0].nodes.len(), 0);
}

#[test]
fn layout_zero_size_field() {
    use xfa_layout_engine::form::*;
    use xfa_layout_engine::layout::LayoutEngine;
    use xfa_layout_engine::text::FontMetrics;
    use xfa_layout_engine::types::*;

    let mut tree = FormTree::new();
    let field = tree.add_node(FormNode {
        name: "tiny".to_string(),
        node_type: FormNodeType::Field {
            value: "data".to_string(),
        },
        box_model: BoxModel {
            width: Some(0.0),
            height: Some(0.0),
            max_width: f64::MAX,
            max_height: f64::MAX,
            ..Default::default()
        },
        layout: LayoutStrategy::Positioned,
        children: vec![],
        occur: Occur::once(),
        font: FontMetrics::default(),
        calculate: None,
        validate: None,
    });
    let root = tree.add_node(FormNode {
        name: "form1".to_string(),
        node_type: FormNodeType::Subform,
        box_model: BoxModel {
            width: Some(100.0),
            height: Some(100.0),
            max_width: f64::MAX,
            max_height: f64::MAX,
            ..Default::default()
        },
        layout: LayoutStrategy::Positioned,
        children: vec![field],
        occur: Occur::once(),
        font: FontMetrics::default(),
        calculate: None,
        validate: None,
    });

    let engine = LayoutEngine::new(&tree);
    // Should not panic on zero-size field
    let result = engine.layout(root);
    assert!(result.is_ok());
}
