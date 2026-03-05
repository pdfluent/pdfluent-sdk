//! End-to-end integration tests: PDF → XFA → layout → render → verify.
//!
//! These tests exercise the full pipeline from constructing a PDF with embedded
//! XFA to rendering page images, covering the entire native Rust chain.

use pdfium_ffi_bridge::events::{EventResult, FormState, InputEvent};
use pdfium_ffi_bridge::native_renderer::RenderConfig;
use pdfium_ffi_bridge::pdf_reader::PdfReader;
use pdfium_ffi_bridge::pipeline;
use pdfium_ffi_bridge::xfa_extract::parse_xfa_xml;

use xfa_layout_engine::form::*;
use xfa_layout_engine::layout::LayoutEngine;
use xfa_layout_engine::text::FontMetrics;
use xfa_layout_engine::types::*;

use image::Rgba;

// --- Helper: build a minimal PDF with XFA ---

fn build_xfa_pdf(xfa_xml: &str) -> Vec<u8> {
    use lopdf::{dictionary, Document, Object, Stream};

    let mut doc = Document::with_version("1.7");

    let xfa_stream = Stream::new(dictionary! {}, xfa_xml.as_bytes().to_vec());
    let xfa_id = doc.add_object(Object::Stream(xfa_stream));

    let acroform = dictionary! { "XFA" => xfa_id };
    let acroform_id = doc.add_object(Object::Dictionary(acroform));

    let pages_id = doc.new_object_id();
    let page_id = doc.new_object_id();
    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![page_id.into()],
        "Count" => 1,
    };
    doc.objects.insert(pages_id, Object::Dictionary(pages));
    let page = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
    };
    doc.objects.insert(page_id, Object::Dictionary(page));

    let catalog_id = doc.new_object_id();
    let catalog = dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
        "AcroForm" => acroform_id,
    };
    doc.objects
        .insert(catalog_id, Object::Dictionary(catalog));
    doc.trailer.set("Root", catalog_id);

    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();
    buf
}

// --- Tests ---

#[test]
fn pdf_to_xfa_extraction_roundtrip() {
    let xfa_xml = r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="form1">
      <field name="FirstName"/>
      <field name="LastName"/>
    </subform>
  </template>
  <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data>
      <form1><FirstName>John</FirstName><LastName>Doe</LastName></form1>
    </xfa:data>
  </xfa:datasets>
</xdp:xdp>"#;

    let pdf_bytes = build_xfa_pdf(xfa_xml);

    // Extract XFA from the PDF
    let reader = PdfReader::from_bytes(&pdf_bytes).unwrap();
    let packets = reader.extract_xfa().unwrap();

    assert!(packets.template().is_some());
    assert!(packets.datasets().is_some());

    let template = packets.template().unwrap();
    assert!(template.contains("FirstName"));
    assert!(template.contains("LastName"));

    let datasets = packets.get_packet("datasets").unwrap();
    assert!(datasets.contains("John"));
    assert!(datasets.contains("Doe"));
}

#[test]
fn xfa_extraction_then_parse_packets() {
    let xfa_xml = r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="invoice">
      <field name="Amount"/>
    </subform>
  </template>
  <config xmlns="http://www.xfa.org/schema/xci/3.1/">
    <present><pdf><version>1.7</version></pdf></present>
  </config>
</xdp:xdp>"#;

    let pdf_bytes = build_xfa_pdf(xfa_xml);
    let reader = PdfReader::from_bytes(&pdf_bytes).unwrap();
    let packets = reader.extract_xfa().unwrap();

    // Re-parse the full XML to verify it's still valid
    let full = packets.full_xml.as_deref().unwrap();
    let reparsed = parse_xfa_xml(full).unwrap();
    assert_eq!(reparsed.packets.len(), packets.packets.len());
}

#[test]
fn form_tree_to_rendered_png() {
    let (mut tree, root) = build_simple_form();
    let config = RenderConfig::with_dpi(72.0);
    let images = pipeline::render_form_tree(&mut tree, root, &config).unwrap();

    assert_eq!(images.len(), 1);
    assert_eq!(images[0].width(), 300);
    assert_eq!(images[0].height(), 200);

    // Verify pixels: the field area should not be all white
    let img = images[0].as_rgba8().unwrap();
    let field_pixel = img.get_pixel(55, 25); // inside the field rect
    assert_ne!(*field_pixel, Rgba([255, 255, 255, 255]));
}

#[test]
fn form_tree_to_png_files() {
    let (mut tree, root) = build_simple_form();
    let config = RenderConfig::default();
    let images = pipeline::render_form_tree(&mut tree, root, &config).unwrap();

    let dir = std::env::temp_dir().join("xfa_integration_test");
    let paths = pipeline::save_pages_as_png(&images, &dir, "int_test").unwrap();
    assert_eq!(paths.len(), 1);
    assert!(paths[0].exists());

    // Verify the PNG can be re-loaded
    let reloaded = image::open(&paths[0]).unwrap();
    assert_eq!(reloaded.width(), 300);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn calculate_script_then_render() {
    let mut tree = FormTree::new();
    let price = tree.add_node(FormNode {
        name: "Price".to_string(),
        node_type: FormNodeType::Field {
            value: "100".to_string(),
        },
        box_model: BoxModel {
            width: Some(80.0),
            height: Some(20.0),
            x: 10.0,
            y: 10.0,
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
    let tax = tree.add_node(FormNode {
        name: "Tax".to_string(),
        node_type: FormNodeType::Field {
            value: String::new(),
        },
        box_model: BoxModel {
            width: Some(80.0),
            height: Some(20.0),
            x: 10.0,
            y: 40.0,
            ..Default::default()
        },
        layout: LayoutStrategy::Positioned,
        children: vec![],
        occur: Occur::once(),
        font: FontMetrics::default(),
        calculate: Some("100 * 0.21".to_string()),
        validate: None,
        column_widths: vec![],
        col_span: 1,
    });
    let root = tree.add_node(FormNode {
        name: "invoice".to_string(),
        node_type: FormNodeType::Subform,
        box_model: BoxModel {
            width: Some(200.0),
            height: Some(100.0),
            ..Default::default()
        },
        layout: LayoutStrategy::Positioned,
        children: vec![price, tax],
        occur: Occur::once(),
        font: FontMetrics::default(),
        calculate: None,
        validate: None,
        column_widths: vec![],
        col_span: 1,
    });

    let config = RenderConfig::default();
    let images = pipeline::render_form_tree(&mut tree, root, &config).unwrap();
    assert_eq!(images.len(), 1);

    // Verify the calculate script ran
    if let FormNodeType::Field { value } = &tree.get(tax).node_type {
        assert_eq!(value, "21");
    } else {
        panic!("expected Field");
    }
}

#[test]
fn event_handling_with_layout() {
    let (mut tree, root) = build_simple_form();

    // Layout the form
    let engine = LayoutEngine::new(&tree);
    let layout = engine.layout(root).unwrap();

    // Create form state and interact
    let mut state = FormState::new(&layout, &tree);

    // Click on the Name field (at x=55, y=25)
    let result = state.process_event(
        &InputEvent::Click {
            page: 0,
            x: 55.0,
            y: 25.0,
        },
        &layout,
        &mut tree,
    );
    assert_eq!(result, EventResult::FocusChanged(FormNodeId(0)));

    // Type into the field
    state.process_event(&InputEvent::CharInput('!'), &layout, &mut tree);

    if let FormNodeType::Field { value } = &tree.get(FormNodeId(0)).node_type {
        assert_eq!(value, "John!");
    }
}

#[test]
fn dpi_scaling_produces_correct_dimensions() {
    let (mut tree, root) = build_simple_form();

    for dpi in [72.0, 144.0, 300.0] {
        let config = RenderConfig::with_dpi(dpi);
        let images = pipeline::render_form_tree(&mut tree, root, &config).unwrap();
        let scale = dpi / 72.0;
        let expected_w = (300.0 * scale) as u32;
        let expected_h = (200.0 * scale) as u32;
        assert_eq!(images[0].width(), expected_w, "width at {dpi} DPI");
        assert_eq!(images[0].height(), expected_h, "height at {dpi} DPI");
    }
}

#[test]
fn multipage_form_renders_all_pages() {
    let mut tree = FormTree::new();

    // Create enough children to overflow a small page
    let mut children = Vec::new();
    for i in 0..10 {
        let field = tree.add_node(FormNode {
            name: format!("Field{i}"),
            node_type: FormNodeType::Field {
                value: format!("Value {i}"),
            },
            box_model: BoxModel {
                height: Some(30.0),
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
            column_widths: vec![],
            col_span: 1,
        });
        children.push(field);
    }

    let content_area = ContentArea {
        name: "content".to_string(),
        width: 200.0,
        height: 100.0, // Small page: only ~3 fields fit
        ..Default::default()
    };
    let page_area = tree.add_node(FormNode {
        name: "Page1".to_string(),
        node_type: FormNodeType::PageArea {
            content_areas: vec![content_area],
        },
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
    let page_set = tree.add_node(FormNode {
        name: "PageSet".to_string(),
        node_type: FormNodeType::PageSet,
        box_model: BoxModel::default(),
        layout: LayoutStrategy::Positioned,
        children: vec![page_area],
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
            width: Some(200.0),
            height: Some(100.0),
            max_width: f64::MAX,
            max_height: f64::MAX,
            ..Default::default()
        },
        layout: LayoutStrategy::TopToBottom,
        children: {
            let mut c = vec![page_set];
            c.extend(children);
            c
        },
        occur: Occur::once(),
        font: FontMetrics::default(),
        calculate: None,
        validate: None,
        column_widths: vec![],
        col_span: 1,
    });

    let config = RenderConfig::default();
    let images = pipeline::render_form_tree(&mut tree, root, &config).unwrap();
    assert!(images.len() > 1, "expected multiple pages, got {}", images.len());
}

#[test]
fn save_reload_pdf_preserves_structure() {
    let xfa_xml = r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="form1"><field name="F1"/></subform>
  </template>
</xdp:xdp>"#;

    let pdf_bytes = build_xfa_pdf(xfa_xml);
    let mut reader = PdfReader::from_bytes(&pdf_bytes).unwrap();
    let page_count = reader.page_count();

    // Save and reload
    let saved = reader.save_to_bytes().unwrap();
    let reader2 = PdfReader::from_bytes(&saved).unwrap();
    assert_eq!(reader2.page_count(), page_count);

    // XFA should still be extractable
    let packets = reader2.extract_xfa().unwrap();
    assert!(packets.template().is_some());
}

#[test]
fn pdf_without_xfa_is_detected() {
    // Build a minimal PDF without XFA
    use lopdf::{dictionary, Document, Object};
    let mut doc = Document::with_version("1.4");
    let pages_id = doc.new_object_id();
    let page_id = doc.new_object_id();
    let pages = dictionary! {
        "Type" => "Pages",
        "Kids" => vec![page_id.into()],
        "Count" => 1,
    };
    doc.objects
        .insert(pages_id, Object::Dictionary(pages));
    let page = dictionary! {
        "Type" => "Page",
        "Parent" => pages_id,
        "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
    };
    doc.objects.insert(page_id, Object::Dictionary(page));
    let catalog_id = doc.new_object_id();
    let catalog = dictionary! {
        "Type" => "Catalog",
        "Pages" => pages_id,
    };
    doc.objects
        .insert(catalog_id, Object::Dictionary(catalog));
    doc.trailer.set("Root", catalog_id);

    let mut buf = Vec::new();
    doc.save_to(&mut buf).unwrap();

    let reader = PdfReader::from_bytes(&buf).unwrap();
    assert!(reader.extract_xfa().is_err());
}

#[test]
fn save_rendered_pages_to_disk_and_verify() {
    let (mut tree, root) = build_simple_form();
    let config = RenderConfig::with_dpi(144.0);
    let images = pipeline::render_form_tree(&mut tree, root, &config).unwrap();

    let dir = std::env::temp_dir().join("xfa_integration_verify");
    let paths = pipeline::save_pages_as_png(&images, &dir, "verify").unwrap();

    for path in &paths {
        assert!(path.exists());
        let meta = std::fs::metadata(path).unwrap();
        assert!(meta.len() > 100, "PNG file should not be trivially small");
    }

    let _ = std::fs::remove_dir_all(&dir);
}

// --- Helpers ---

fn build_simple_form() -> (FormTree, FormNodeId) {
    let mut tree = FormTree::new();
    let field = tree.add_node(FormNode {
        name: "Name".to_string(),
        node_type: FormNodeType::Field {
            value: "John".to_string(),
        },
        box_model: BoxModel {
            width: Some(120.0),
            height: Some(20.0),
            x: 30.0,
            y: 20.0,
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
        column_widths: vec![],
        col_span: 1,
    });
    let root = tree.add_node(FormNode {
        name: "form1".to_string(),
        node_type: FormNodeType::Subform,
        box_model: BoxModel {
            width: Some(300.0),
            height: Some(200.0),
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
        column_widths: vec![],
        col_span: 1,
    });
    (tree, root)
}
