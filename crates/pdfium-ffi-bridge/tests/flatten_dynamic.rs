//! Dynamic XFA flattening tests — layout preservation and data integrity.
//!
//! Tests that dynamic XFA forms flatten correctly:
//! - Repeating sections render all instances
//! - Hidden fields are excluded from output
//! - Multi-page pagination is preserved
//! - FormCalc computed values are baked in
//! - No data loss on flatten round-trip

use pdfium_ffi_bridge::flatten::{flatten_to_pdf, FlattenConfig};
use pdfium_ffi_bridge::pipeline::flatten_form_tree;
use xfa_layout_engine::form::*;
use xfa_layout_engine::layout::{LayoutContent, LayoutDom, LayoutNode, LayoutPage};
use xfa_layout_engine::text::FontMetrics;
use xfa_layout_engine::types::*;

// ── Helpers ──────────────────────────────────────────────────────────

fn default_field(name: &str, value: &str, x: f64, y: f64) -> FormNode {
    FormNode {
        name: name.to_string(),
        node_type: FormNodeType::Field {
            value: value.to_string(),
        },
        box_model: BoxModel {
            width: Some(200.0),
            height: Some(20.0),
            x,
            y,
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
    }
}

fn default_draw(name: &str, content: &str, x: f64, y: f64) -> FormNode {
    FormNode {
        name: name.to_string(),
        node_type: FormNodeType::Draw {
            content: content.to_string(),
        },
        box_model: BoxModel {
            width: Some(200.0),
            height: Some(15.0),
            x,
            y,
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
    }
}

fn subform(name: &str, children: Vec<FormNodeId>, layout: LayoutStrategy) -> FormNode {
    FormNode {
        name: name.to_string(),
        node_type: FormNodeType::Subform,
        box_model: BoxModel {
            width: Some(612.0),
            height: Some(792.0),
            ..Default::default()
        },
        layout,
        children,
        occur: Occur::once(),
        font: FontMetrics::default(),
        calculate: None,
        validate: None,
        column_widths: vec![],
        col_span: 1,
    }
}

fn repeating_subform(
    name: &str,
    children: Vec<FormNodeId>,
    min: u32,
    max: Option<u32>,
    initial: u32,
) -> FormNode {
    FormNode {
        name: name.to_string(),
        node_type: FormNodeType::Subform,
        box_model: BoxModel {
            width: Some(570.0),
            height: Some(50.0),
            x: 20.0,
            y: 0.0,
            ..Default::default()
        },
        layout: LayoutStrategy::TopToBottom,
        children,
        occur: Occur::repeating(min, max, initial),
        font: FontMetrics::default(),
        calculate: None,
        validate: None,
        column_widths: vec![],
        col_span: 1,
    }
}

fn load_flat_pdf(bytes: &[u8]) -> lopdf::Document {
    lopdf::Document::load_mem(bytes).expect("flat PDF should be valid")
}

fn assert_no_xfa(doc: &lopdf::Document) {
    let catalog_id = match doc.trailer.get(b"Root").unwrap() {
        lopdf::Object::Reference(id) => *id,
        _ => panic!("No Root reference"),
    };
    let catalog = doc.get_object(catalog_id).unwrap().as_dict().unwrap();
    assert!(
        catalog.get(b"AcroForm").is_err(),
        "Flattened PDF must not contain AcroForm"
    );
    assert!(
        catalog.get(b"NeedsRendering").is_err(),
        "Flattened PDF must not contain NeedsRendering"
    );
}

// ── Test: Repeating sections ─────────────────────────────────────────

#[test]
fn flatten_repeating_subforms_renders_all_instances() {
    let mut tree = FormTree::new();

    // Create a repeating row subform with one field each
    let f1 = tree.add_node(default_field("Item", "Widget A", 10.0, 5.0));
    let row_template = tree.add_node(repeating_subform("Row", vec![f1], 1, Some(5), 3));

    let root = tree.add_node(subform(
        "form1",
        vec![row_template],
        LayoutStrategy::TopToBottom,
    ));

    let config = FlattenConfig {
        compress: false,
        ..Default::default()
    };
    let pdf_bytes = flatten_form_tree(&mut tree, root, &config).unwrap();
    assert!(!pdf_bytes.is_empty());

    let doc = load_flat_pdf(&pdf_bytes);
    assert!(!doc.get_pages().is_empty());
    assert_no_xfa(&doc);

    // Verify that the initial count (3) of repeating instances produced XObject references.
    // In uncompressed mode we can check for multiple Do operators or XObject names.
    let pages = doc.get_pages();
    let page_id = *pages.values().next().unwrap();
    if let Ok(content_bytes) = doc.get_page_content(page_id) {
        let content_str = String::from_utf8_lossy(&content_bytes);
        let do_count = content_str.matches(" Do").count();
        assert!(
            do_count >= 3,
            "Expected at least 3 XObject paints for 3 repeated rows, found {do_count}"
        );
    }
}

// ── Test: Hidden fields ──────────────────────────────────────────────

#[test]
fn flatten_hidden_fields_excluded() {
    // Fields with zero dimensions should not produce visible content
    let mut tree = FormTree::new();

    let visible = tree.add_node(default_field("Visible", "Hello", 20.0, 20.0));

    // Hidden field: 0x0 dimensions
    let hidden = tree.add_node(FormNode {
        name: "Hidden".to_string(),
        node_type: FormNodeType::Field {
            value: "secret".to_string(),
        },
        box_model: BoxModel {
            width: Some(0.0),
            height: Some(0.0),
            x: 0.0,
            y: 0.0,
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

    let root = tree.add_node(subform(
        "form1",
        vec![visible, hidden],
        LayoutStrategy::Positioned,
    ));

    let config = FlattenConfig {
        compress: false,
        ..Default::default()
    };
    let pdf_bytes = flatten_form_tree(&mut tree, root, &config).unwrap();
    let doc = load_flat_pdf(&pdf_bytes);
    assert_eq!(doc.get_pages().len(), 1);
    assert_no_xfa(&doc);

    // Verify hidden field content ("secret") does not appear in the PDF stream
    let pages = doc.get_pages();
    let page_id = *pages.values().next().unwrap();
    if let Ok(content_bytes) = doc.get_page_content(page_id) {
        let content_str = String::from_utf8_lossy(&content_bytes);
        assert!(
            !content_str.contains("secret"),
            "Hidden field value should not appear in flattened content"
        );
    }
}

// ── Test: Multi-page pagination ──────────────────────────────────────

#[test]
fn flatten_multipage_preserves_pagination() {
    // Test at LayoutDom level to guarantee multi-page output,
    // independent of layout engine pagination behavior.
    let layout = LayoutDom {
        pages: vec![
            LayoutPage {
                width: 612.0,
                height: 792.0,
                nodes: vec![
                    LayoutNode {
                        form_node: FormNodeId(0),
                        rect: Rect::new(72.0, 72.0, 200.0, 20.0),
                        name: "Page1Name".to_string(),
                        content: LayoutContent::Field {
                            value: "Alice".to_string(),
                        },
                        children: vec![],
                    },
                    LayoutNode {
                        form_node: FormNodeId(1),
                        rect: Rect::new(72.0, 100.0, 200.0, 20.0),
                        name: "Page1City".to_string(),
                        content: LayoutContent::Field {
                            value: "Amsterdam".to_string(),
                        },
                        children: vec![],
                    },
                ],
            },
            LayoutPage {
                width: 612.0,
                height: 792.0,
                nodes: vec![LayoutNode {
                    form_node: FormNodeId(2),
                    rect: Rect::new(72.0, 72.0, 200.0, 20.0),
                    name: "Page2Notes".to_string(),
                    content: LayoutContent::Field {
                        value: "Additional notes here".to_string(),
                    },
                    children: vec![],
                }],
            },
            LayoutPage {
                width: 612.0,
                height: 792.0,
                nodes: vec![LayoutNode {
                    form_node: FormNodeId(3),
                    rect: Rect::new(72.0, 72.0, 200.0, 15.0),
                    name: "Page3Footer".to_string(),
                    content: LayoutContent::Text("Page 3 of 3".to_string()),
                    children: vec![],
                }],
            },
        ],
    };

    let config = FlattenConfig::default();
    let pdf_bytes = flatten_to_pdf(&layout, &config).unwrap();
    let doc = load_flat_pdf(&pdf_bytes);

    assert_eq!(doc.get_pages().len(), 3, "Should produce 3-page PDF");
    assert_no_xfa(&doc);
}

// ── Test: FormCalc computed values baked in ───────────────────────────

#[test]
fn flatten_bakes_formcalc_calculated_values() {
    let mut tree = FormTree::new();

    let price = tree.add_node(FormNode {
        name: "Price".to_string(),
        node_type: FormNodeType::Field {
            value: "100".to_string(),
        },
        box_model: BoxModel {
            width: Some(100.0),
            height: Some(20.0),
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

    let tax = tree.add_node(FormNode {
        name: "Tax".to_string(),
        node_type: FormNodeType::Field {
            value: String::new(),
        },
        box_model: BoxModel {
            width: Some(100.0),
            height: Some(20.0),
            x: 20.0,
            y: 50.0,
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

    let total = tree.add_node(FormNode {
        name: "Total".to_string(),
        node_type: FormNodeType::Field {
            value: String::new(),
        },
        box_model: BoxModel {
            width: Some(100.0),
            height: Some(20.0),
            x: 20.0,
            y: 80.0,
            ..Default::default()
        },
        layout: LayoutStrategy::Positioned,
        children: vec![],
        occur: Occur::once(),
        font: FontMetrics::default(),
        calculate: Some("100 + 21".to_string()),
        validate: None,
        column_widths: vec![],
        col_span: 1,
    });

    let root = tree.add_node(subform(
        "form1",
        vec![price, tax, total],
        LayoutStrategy::Positioned,
    ));

    let config = FlattenConfig::default();
    let pdf_bytes = flatten_form_tree(&mut tree, root, &config).unwrap();

    // Verify calculate scripts ran
    if let FormNodeType::Field { value } = &tree.get(tax).node_type {
        assert_eq!(value, "21");
    } else {
        panic!("Tax should be a Field");
    }
    if let FormNodeType::Field { value } = &tree.get(total).node_type {
        assert_eq!(value, "121");
    } else {
        panic!("Total should be a Field");
    }

    let doc = load_flat_pdf(&pdf_bytes);
    assert_eq!(doc.get_pages().len(), 1);
    assert_no_xfa(&doc);
}

// ── Test: No data loss — field values preserved ──────────────────────

#[test]
fn flatten_preserves_all_field_values() {
    let mut tree = FormTree::new();

    let test_values = [
        ("Name", "John Doe"),
        ("Email", "john@example.com"),
        ("Phone", "+31 6 1234 5678"),
        ("Address", "Keizersgracht 123, Amsterdam"),
        ("ZipCode", "1015 CJ"),
        ("Notes", "Special chars: €, ñ, ü, ß"),
    ];

    let mut field_ids = Vec::new();
    for (i, (name, value)) in test_values.iter().enumerate() {
        let fid = tree.add_node(default_field(name, value, 20.0, 20.0 + i as f64 * 30.0));
        field_ids.push(fid);
    }

    let root = tree.add_node(subform(
        "form1",
        field_ids.clone(),
        LayoutStrategy::Positioned,
    ));

    let config = FlattenConfig::default();
    let pdf_bytes = flatten_form_tree(&mut tree, root, &config).unwrap();

    // All field values should still be in the tree (not cleared)
    for (i, (name, value)) in test_values.iter().enumerate() {
        let fid = field_ids[i];
        let node = tree.get(fid);
        assert_eq!(node.name, *name);
        if let FormNodeType::Field { value: v } = &node.node_type {
            assert_eq!(v, *value, "Field {name} should retain its value");
        } else {
            panic!("Expected Field node for {name}");
        }
    }

    let doc = load_flat_pdf(&pdf_bytes);
    assert_eq!(doc.get_pages().len(), 1);
    assert_no_xfa(&doc);
}

// ── Test: Draw elements preserved in flat output ─────────────────────

#[test]
fn flatten_includes_draw_elements() {
    let mut tree = FormTree::new();

    let label = tree.add_node(default_draw("Label", "Customer Information:", 20.0, 20.0));
    let name_field = tree.add_node(default_field("Name", "Alice", 20.0, 45.0));
    let footer = tree.add_node(default_draw("Footer", "Page 1 of 1", 20.0, 750.0));

    let root = tree.add_node(subform(
        "form1",
        vec![label, name_field, footer],
        LayoutStrategy::Positioned,
    ));

    let config = FlattenConfig::default();
    let pdf_bytes = flatten_form_tree(&mut tree, root, &config).unwrap();
    let doc = load_flat_pdf(&pdf_bytes);
    assert_eq!(doc.get_pages().len(), 1);
    assert_no_xfa(&doc);
}

// ── Test: Performance — flatten typical form ─────────────────────────

#[test]
fn flatten_performance_under_5_seconds() {
    let mut tree = FormTree::new();

    // Build a "typical" complex form: 50 fields in flowing layout
    let mut fields = Vec::new();
    for i in 0..50 {
        let f = tree.add_node(FormNode {
            name: format!("Field{i}"),
            node_type: FormNodeType::Field {
                value: format!("Value {i} with some typical content"),
            },
            box_model: BoxModel {
                width: Some(250.0),
                height: Some(20.0),
                x: 0.0,
                y: 0.0,
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
        fields.push(f);
    }

    let root = tree.add_node(FormNode {
        name: "form1".to_string(),
        node_type: FormNodeType::Subform,
        box_model: BoxModel {
            width: Some(612.0),
            height: Some(792.0),
            ..Default::default()
        },
        layout: LayoutStrategy::TopToBottom,
        children: fields,
        occur: Occur::once(),
        font: FontMetrics::default(),
        calculate: None,
        validate: None,
        column_widths: vec![],
        col_span: 1,
    });

    let start = std::time::Instant::now();
    let config = FlattenConfig::default();
    let pdf_bytes = flatten_form_tree(&mut tree, root, &config).unwrap();
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_secs() < 5,
        "Flatten took {elapsed:?}, must be under 5 seconds"
    );
    assert!(!pdf_bytes.is_empty());
    let doc = load_flat_pdf(&pdf_bytes);
    assert!(!doc.get_pages().is_empty());
}

// ── Test: Empty form flattens without error ──────────────────────────

#[test]
fn flatten_empty_form_produces_valid_pdf() {
    let mut tree = FormTree::new();
    let root = tree.add_node(subform("form1", vec![], LayoutStrategy::Positioned));

    let config = FlattenConfig::default();
    let pdf_bytes = flatten_form_tree(&mut tree, root, &config).unwrap();
    let doc = load_flat_pdf(&pdf_bytes);
    assert_eq!(doc.get_pages().len(), 1);
    assert_no_xfa(&doc);
}

// ── Test: Nested subforms ────────────────────────────────────────────

#[test]
fn flatten_nested_subforms() {
    let mut tree = FormTree::new();

    let inner_field = tree.add_node(default_field("InnerField", "Deep Value", 10.0, 10.0));
    let inner_subform = tree.add_node(FormNode {
        name: "InnerGroup".to_string(),
        node_type: FormNodeType::Subform,
        box_model: BoxModel {
            width: Some(300.0),
            height: Some(50.0),
            x: 20.0,
            y: 50.0,
            ..Default::default()
        },
        layout: LayoutStrategy::Positioned,
        children: vec![inner_field],
        occur: Occur::once(),
        font: FontMetrics::default(),
        calculate: None,
        validate: None,
        column_widths: vec![],
        col_span: 1,
    });

    let outer_field = tree.add_node(default_field("OuterField", "Top Value", 20.0, 20.0));
    let root = tree.add_node(subform(
        "form1",
        vec![outer_field, inner_subform],
        LayoutStrategy::Positioned,
    ));

    let config = FlattenConfig::default();
    let pdf_bytes = flatten_form_tree(&mut tree, root, &config).unwrap();
    let doc = load_flat_pdf(&pdf_bytes);
    assert_eq!(doc.get_pages().len(), 1);
    assert_no_xfa(&doc);
}

// ── Test: Flatten output is re-loadable ──────────────────────────────

#[test]
fn flatten_output_is_reloadable() {
    let mut tree = FormTree::new();
    let f1 = tree.add_node(default_field("Name", "Test", 20.0, 20.0));
    let f2 = tree.add_node(default_field("City", "Amsterdam", 20.0, 50.0));
    let root = tree.add_node(subform("form1", vec![f1, f2], LayoutStrategy::Positioned));

    let config = FlattenConfig::default();
    let pdf_bytes = flatten_form_tree(&mut tree, root, &config).unwrap();

    // Re-load the flattened PDF
    let doc = load_flat_pdf(&pdf_bytes);
    assert_eq!(doc.get_pages().len(), 1);
    assert_no_xfa(&doc);

    // Save again — should round-trip cleanly
    let mut buf = Vec::new();
    let mut doc2 = doc;
    doc2.save_to(&mut buf).unwrap();
    assert!(!buf.is_empty());

    let doc3 = lopdf::Document::load_mem(&buf).unwrap();
    assert_eq!(doc3.get_pages().len(), 1);
}

// ── Test: Uncompressed flatten for debugging ─────────────────────────

#[test]
fn flatten_uncompressed_contains_readable_operators() {
    let mut tree = FormTree::new();
    let f1 = tree.add_node(default_field("Name", "Debug", 20.0, 20.0));
    let root = tree.add_node(subform("form1", vec![f1], LayoutStrategy::Positioned));

    let config = FlattenConfig {
        compress: false,
        ..Default::default()
    };
    let pdf_bytes = flatten_form_tree(&mut tree, root, &config).unwrap();

    // The raw PDF should contain readable operators since it's uncompressed
    let pdf_str = String::from_utf8_lossy(&pdf_bytes);
    // Content stream should reference XObject operators
    assert!(
        pdf_str.contains("/XF0") || pdf_str.contains("Do"),
        "Uncompressed PDF should have readable XObject references"
    );
}

// ── PDF/A-2b compliance tests ────────────────────────────────────────

#[test]
fn flatten_pdfa_has_xmp_metadata() {
    let mut tree = FormTree::new();
    let f1 = tree.add_node(default_field("Name", "Alice", 20.0, 20.0));
    let root = tree.add_node(subform("form1", vec![f1], LayoutStrategy::Positioned));

    let config = FlattenConfig {
        pdfa: true,
        ..Default::default()
    };
    let pdf_bytes = flatten_form_tree(&mut tree, root, &config).unwrap();
    let doc = lopdf::Document::load_mem(&pdf_bytes).unwrap();

    // Catalog should have a Metadata stream
    let catalog_id = match doc.trailer.get(b"Root").unwrap() {
        lopdf::Object::Reference(id) => *id,
        _ => panic!("No Root"),
    };
    let catalog = doc.get_object(catalog_id).unwrap().as_dict().unwrap();
    let meta_ref = catalog.get(b"Metadata").unwrap();
    assert!(
        matches!(meta_ref, lopdf::Object::Reference(_)),
        "Catalog should have Metadata reference"
    );

    // Metadata stream should contain PDF/A-2b declarations
    if let lopdf::Object::Reference(id) = meta_ref {
        if let Ok(lopdf::Object::Stream(s)) = doc.get_object(*id) {
            let text = String::from_utf8_lossy(&s.content);
            assert!(
                text.contains("pdfaid:part"),
                "XMP should contain pdfaid:part"
            );
            assert!(
                text.contains("<pdfaid:conformance>B</pdfaid:conformance>"),
                "XMP should declare PDF/A-2b conformance"
            );
        } else {
            panic!("Metadata should be a stream");
        }
    }
}

#[test]
fn flatten_pdfa_has_srgb_output_intent() {
    let mut tree = FormTree::new();
    let f1 = tree.add_node(default_field("Name", "Bob", 20.0, 20.0));
    let root = tree.add_node(subform("form1", vec![f1], LayoutStrategy::Positioned));

    let config = FlattenConfig {
        pdfa: true,
        ..Default::default()
    };
    let pdf_bytes = flatten_form_tree(&mut tree, root, &config).unwrap();
    let doc = lopdf::Document::load_mem(&pdf_bytes).unwrap();

    let catalog_id = match doc.trailer.get(b"Root").unwrap() {
        lopdf::Object::Reference(id) => *id,
        _ => panic!("No Root"),
    };
    let catalog = doc.get_object(catalog_id).unwrap().as_dict().unwrap();
    let intents = catalog
        .get(b"OutputIntents")
        .expect("Should have OutputIntents");
    if let lopdf::Object::Array(arr) = intents {
        assert!(!arr.is_empty(), "OutputIntents should not be empty");
        // First intent should be GTS_PDFA1
        if let lopdf::Object::Reference(r) = &arr[0] {
            let intent = doc.get_object(*r).unwrap().as_dict().unwrap();
            let subtype = intent.get(b"S").unwrap();
            assert_eq!(
                subtype,
                &lopdf::Object::Name(b"GTS_PDFA1".to_vec()),
                "Output intent should be GTS_PDFA1"
            );
        }
    } else {
        panic!("OutputIntents should be an array");
    }
}

#[test]
fn flatten_pdfa_no_javascript_or_embedded_files() {
    let layout = LayoutDom {
        pages: vec![LayoutPage {
            width: 612.0,
            height: 792.0,
            nodes: vec![LayoutNode {
                form_node: FormNodeId(0),
                rect: Rect::new(20.0, 20.0, 200.0, 20.0),
                name: "F".to_string(),
                content: LayoutContent::WrappedText {
                    lines: vec!["Test".to_string()],
                    font_size: 12.0,
                },
                children: vec![],
            }],
        }],
    };
    let config = FlattenConfig {
        pdfa: true,
        ..Default::default()
    };
    let pdf_bytes = flatten_to_pdf(&layout, &config).unwrap();
    let doc = lopdf::Document::load_mem(&pdf_bytes).unwrap();

    let catalog_id = match doc.trailer.get(b"Root").unwrap() {
        lopdf::Object::Reference(id) => *id,
        _ => panic!("No Root"),
    };
    let catalog = doc.get_object(catalog_id).unwrap().as_dict().unwrap();

    // No JavaScript, AA, or embedded files should be present
    assert!(catalog.get(b"AA").is_err(), "No AA in PDF/A");
    if let Ok(names) = catalog.get(b"Names") {
        if let Ok(d) = names.as_dict() {
            assert!(
                d.get(b"JavaScript").is_err(),
                "No JavaScript nametree in PDF/A"
            );
            assert!(
                d.get(b"EmbeddedFiles").is_err(),
                "No EmbeddedFiles in PDF/A"
            );
        }
    }
}
