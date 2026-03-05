//! Performance benchmarks — timing measurements for core operations.
//!
//! These are not micro-benchmarks but coarse-grained timing tests
//! that verify performance stays within acceptable bounds.

use pdfium_ffi_bridge::dataset_sync::sync_datasets;
use pdfium_ffi_bridge::native_renderer::RenderConfig;
use pdfium_ffi_bridge::pdf_reader::PdfReader;
use pdfium_ffi_bridge::pipeline;
use pdfium_ffi_bridge::ur3::detect_ur3;
use pdfium_ffi_bridge::xfa_extract::parse_xfa_xml;
use xfa_dom_resolver::data_dom::DataDom;
use xfa_layout_engine::form::*;
use xfa_layout_engine::layout::LayoutEngine;
use xfa_layout_engine::text::FontMetrics;
use xfa_layout_engine::types::*;

use lopdf::{dictionary, Document, Object, Stream};

use std::time::Instant;

// --- Helpers ---

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

fn build_large_xfa(num_fields: usize) -> String {
    let mut fields = String::new();
    let mut data = String::from("<form1>");
    for i in 0..num_fields {
        fields.push_str(&format!("      <field name=\"F{i}\"/>\n"));
        data.push_str(&format!("<F{i}>Value for field {i}</F{i}>"));
    }
    data.push_str("</form1>");

    format!(
        r#"<?xml version="1.0"?>
<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
  <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
    <subform name="form1">
{fields}    </subform>
  </template>
  <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
    <xfa:data>{data}</xfa:data>
  </xfa:datasets>
</xdp:xdp>"#
    )
}

fn build_large_form_tree(num_fields: usize) -> (FormTree, FormNodeId) {
    let mut tree = FormTree::new();
    let mut children = Vec::new();

    for i in 0..num_fields {
        let field = tree.add_node(FormNode {
            name: format!("Field{i}"),
            node_type: FormNodeType::Field {
                value: format!("Value {i}"),
            },
            box_model: BoxModel {
                width: Some(200.0),
                height: Some(20.0),
                x: 10.0,
                y: 10.0 + (i as f64 * 25.0),
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

    let root = tree.add_node(FormNode {
        name: "form1".to_string(),
        node_type: FormNodeType::Subform,
        box_model: BoxModel {
            width: Some(612.0),
            height: Some(792.0),
            max_width: f64::MAX,
            max_height: f64::MAX,
            ..Default::default()
        },
        layout: LayoutStrategy::Positioned,
        children,
        occur: Occur::once(),
        font: FontMetrics::default(),
        calculate: None,
        validate: None,
        column_widths: vec![],
        col_span: 1,
    });

    (tree, root)
}

// =============================================================================
// PDF Loading Benchmarks
// =============================================================================

#[test]
#[ignore] // perf benchmark — run explicitly with `cargo test -- --ignored`
fn bench_pdf_load_small() {
    let xfa_xml = build_large_xfa(10);
    let pdf = build_xfa_pdf(&xfa_xml);

    let start = Instant::now();
    let iterations = 100;
    for _ in 0..iterations {
        let _ = PdfReader::from_bytes(&pdf).unwrap();
    }
    let elapsed = start.elapsed();

    let per_op = elapsed / iterations;
    eprintln!("PDF load (10 fields): {per_op:?} per op ({iterations} iterations)");
    assert!(per_op.as_millis() < 50, "PDF load should be fast");
}

#[test]
#[ignore] // perf benchmark — run explicitly with `cargo test -- --ignored`
fn bench_pdf_load_medium() {
    let xfa_xml = build_large_xfa(100);
    let pdf = build_xfa_pdf(&xfa_xml);

    let start = Instant::now();
    let iterations = 50;
    for _ in 0..iterations {
        let _ = PdfReader::from_bytes(&pdf).unwrap();
    }
    let elapsed = start.elapsed();

    let per_op = elapsed / iterations;
    eprintln!("PDF load (100 fields): {per_op:?} per op ({iterations} iterations)");
    assert!(per_op.as_millis() < 100, "PDF load should be < 100ms");
}

// =============================================================================
// XFA Extraction Benchmarks
// =============================================================================

#[test]
#[ignore] // perf benchmark — run explicitly with `cargo test -- --ignored`
fn bench_xfa_extraction() {
    let xfa_xml = build_large_xfa(100);
    let pdf = build_xfa_pdf(&xfa_xml);
    let reader = PdfReader::from_bytes(&pdf).unwrap();

    let start = Instant::now();
    let iterations = 100;
    for _ in 0..iterations {
        let _ = reader.extract_xfa().unwrap();
    }
    let elapsed = start.elapsed();

    let per_op = elapsed / iterations;
    eprintln!("XFA extraction (100 fields): {per_op:?} per op");
    assert!(per_op.as_millis() < 50, "extraction should be < 50ms");
}

#[test]
#[ignore] // perf benchmark — run explicitly with `cargo test -- --ignored`
fn bench_xfa_xml_parsing() {
    let xfa_xml = build_large_xfa(200);

    let start = Instant::now();
    let iterations = 200;
    for _ in 0..iterations {
        let _ = parse_xfa_xml(&xfa_xml).unwrap();
    }
    let elapsed = start.elapsed();

    let per_op = elapsed / iterations;
    eprintln!("XFA XML parse (200 fields): {per_op:?} per op");
    assert!(per_op.as_millis() < 20, "XML parsing should be < 20ms");
}

// =============================================================================
// Layout Benchmarks
// =============================================================================

#[test]
#[ignore] // perf benchmark — run explicitly with `cargo test -- --ignored`
fn bench_layout_small_form() {
    let (tree, root) = build_large_form_tree(20);
    let engine = LayoutEngine::new(&tree);

    let start = Instant::now();
    let iterations = 500;
    for _ in 0..iterations {
        let _ = engine.layout(root).unwrap();
    }
    let elapsed = start.elapsed();

    let per_op = elapsed / iterations;
    eprintln!("Layout (20 fields): {per_op:?} per op");
    assert!(per_op.as_millis() < 10, "layout should be < 10ms");
}

#[test]
#[ignore] // perf benchmark — run explicitly with `cargo test -- --ignored`
fn bench_layout_medium_form() {
    let (tree, root) = build_large_form_tree(100);
    let engine = LayoutEngine::new(&tree);

    let start = Instant::now();
    let iterations = 100;
    for _ in 0..iterations {
        let _ = engine.layout(root).unwrap();
    }
    let elapsed = start.elapsed();

    let per_op = elapsed / iterations;
    eprintln!("Layout (100 fields): {per_op:?} per op");
    assert!(per_op.as_millis() < 50, "layout should be < 50ms");
}

// =============================================================================
// Render Benchmarks
// =============================================================================

#[test]
#[ignore] // perf benchmark — run explicitly with `cargo test -- --ignored`
fn bench_render_small() {
    let (mut tree, root) = build_large_form_tree(20);
    let config = RenderConfig::with_dpi(72.0);

    let start = Instant::now();
    let iterations = 50;
    for _ in 0..iterations {
        let _ = pipeline::render_form_tree(&mut tree, root, &config).unwrap();
    }
    let elapsed = start.elapsed();

    let per_op = elapsed / iterations;
    eprintln!("Render (20 fields, 72dpi): {per_op:?} per op");
    assert!(per_op.as_millis() < 100, "render should be < 100ms");
}

#[test]
#[ignore] // perf benchmark — run explicitly with `cargo test -- --ignored`
fn bench_render_high_dpi() {
    let (mut tree, root) = build_large_form_tree(20);
    let config = RenderConfig::with_dpi(300.0);

    let start = Instant::now();
    let iterations = 10;
    for _ in 0..iterations {
        let _ = pipeline::render_form_tree(&mut tree, root, &config).unwrap();
    }
    let elapsed = start.elapsed();

    let per_op = elapsed / iterations;
    eprintln!("Render (20 fields, 300dpi): {per_op:?} per op");
    assert!(per_op.as_secs() < 2, "300dpi render should be < 2s");
}

// =============================================================================
// Dataset Sync Benchmarks
// =============================================================================

#[test]
#[ignore] // perf benchmark — run explicitly with `cargo test -- --ignored`
fn bench_dataset_sync() {
    let xfa_xml = build_large_xfa(100);
    let pdf = build_xfa_pdf(&xfa_xml);

    // Build data that matches
    let mut data_xml = String::from("<form1>");
    for i in 0..100 {
        data_xml.push_str(&format!("<F{i}>Updated {i}</F{i}>"));
    }
    data_xml.push_str("</form1>");
    let data = DataDom::from_xml(&data_xml).unwrap();

    let start = Instant::now();
    let iterations = 50;
    for _ in 0..iterations {
        let mut reader = PdfReader::from_bytes(&pdf).unwrap();
        sync_datasets(&mut reader, &data).unwrap();
        let _ = reader.save_to_bytes().unwrap();
    }
    let elapsed = start.elapsed();

    let per_op = elapsed / iterations;
    eprintln!("Dataset sync + save (100 fields): {per_op:?} per op");
    assert!(per_op.as_millis() < 200, "sync+save should be < 200ms");
}

// =============================================================================
// UR3 Detection Benchmarks
// =============================================================================

#[test]
#[ignore] // perf benchmark — run explicitly with `cargo test -- --ignored`
fn bench_ur3_detection() {
    let xfa_xml = build_large_xfa(50);
    let pdf = build_xfa_pdf(&xfa_xml);
    let reader = PdfReader::from_bytes(&pdf).unwrap();

    let start = Instant::now();
    let iterations = 1000;
    for _ in 0..iterations {
        let _ = detect_ur3(&reader).unwrap();
    }
    let elapsed = start.elapsed();

    let per_op = elapsed / iterations;
    eprintln!("UR3 detection: {per_op:?} per op");
    assert!(per_op.as_micros() < 500, "UR3 detection should be < 500µs");
}

// =============================================================================
// Full Pipeline Benchmark
// =============================================================================

#[test]
#[ignore] // perf benchmark — run explicitly with `cargo test -- --ignored`
fn bench_full_pipeline() {
    let xfa_xml = build_large_xfa(50);
    let pdf = build_xfa_pdf(&xfa_xml);

    let start = Instant::now();
    let iterations = 20;
    for _ in 0..iterations {
        // Load
        let reader = PdfReader::from_bytes(&pdf).unwrap();
        // Extract
        let packets = reader.extract_xfa().unwrap();
        // Parse template
        let _template = packets.template().unwrap();
        // Parse data
        let _datasets = packets.datasets().unwrap();
    }
    let elapsed = start.elapsed();

    let per_op = elapsed / iterations;
    eprintln!("Full pipeline load+extract (50 fields): {per_op:?} per op");
    assert!(per_op.as_millis() < 100, "full pipeline should be < 100ms");
}
