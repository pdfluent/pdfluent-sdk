//! XFA-Native-Rust CLI — demo pipeline combining all modules.
//!
//! This binary demonstrates the full XFA processing pipeline:
//! 1. Parse XFA XML data (xfa-dom-resolver)
//! 2. Execute FormCalc scripts (formcalc-interpreter)
//! 3. Lay out form elements (xfa-layout-engine)
//! 4. Optionally render via PDFium (pdfium-ffi-bridge)

use formcalc_interpreter::interpreter::Interpreter;
use formcalc_interpreter::lexer::tokenize;
use formcalc_interpreter::parser;
use xfa_dom_resolver::data_dom::DataDom;
use xfa_dom_resolver::som;
use xfa_layout_engine::form::{FormNode, FormNodeId, FormNodeType, FormTree, Occur};
use xfa_layout_engine::layout::LayoutEngine;
use xfa_layout_engine::text::FontMetrics;
use xfa_layout_engine::types::{BoxModel, LayoutStrategy};

fn main() {
    println!("XFA-Native-Rust Engine v{}", env!("CARGO_PKG_VERSION"));
    println!();

    // Demo 1: Data DOM + SOM resolution
    demo_data_dom();
    println!();

    // Demo 2: FormCalc interpreter
    demo_formcalc();
    println!();

    // Demo 3: Layout engine
    demo_layout();
}

fn demo_data_dom() {
    println!("=== Data DOM + SOM Resolution ===");

    let xml = r#"<data>
        <Invoice>
            <Customer>
                <Name>Acme Corp</Name>
                <Address>123 Main St</Address>
            </Customer>
            <Item>
                <Description>Widget A</Description>
                <Qty>10</Qty>
                <Price>5.00</Price>
            </Item>
            <Item>
                <Description>Widget B</Description>
                <Qty>5</Qty>
                <Price>12.50</Price>
            </Item>
            <Total>112.50</Total>
        </Invoice>
    </data>"#;

    let dom = DataDom::from_xml(xml).expect("Failed to parse XML");

    // Resolve various SOM paths
    let paths = [
        "$data.Invoice.Customer.Name",
        "$data.Invoice.Item[0].Description",
        "$data.Invoice.Item[1].Description",
        "$data.Invoice.Item[*]",
        "$data.Invoice.Total",
    ];

    for path in &paths {
        let results = som::resolve_data_path(&dom, path, None).expect("SOM resolve failed");
        if results.is_empty() {
            println!("  {path} -> (no match)");
        } else if results.len() == 1 {
            let val = dom.value(results[0]).unwrap_or("(group)");
            println!("  {path} -> {val}");
        } else {
            println!("  {path} -> [{} matches]", results.len());
        }
    }
}

fn demo_formcalc() {
    println!("=== FormCalc Interpreter ===");

    let scripts = [
        ("Arithmetic", "var price = 5.00\nvar qty = 10\nprice * qty"),
        (
            "String ops",
            r#"Concat("Hello, ", Upper("world"), "!")"#,
        ),
        (
            "Tax calc",
            "var subtotal = 112.50\nvar tax_rate = 0.21\nvar tax = Round(subtotal * tax_rate, 2)\nConcat(\"Tax: \", tax)",
        ),
        (
            "Control flow",
            "var total = 0\nfor i = 1 upto 10 do\n  total = total + i\nendfor\ntotal",
        ),
        (
            "User function",
            "func discount(amount, pct)\n  Round(amount * (1 - pct / 100), 2)\nendfunc\ndiscount(112.50, 15)",
        ),
    ];

    for (label, script) in &scripts {
        let tokens = tokenize(script).expect("Tokenize failed");
        let ast = parser::parse(tokens).expect("Parse failed");
        let mut interp = Interpreter::new();
        let result = interp.exec(&ast).expect("Exec failed");
        println!("  {label}: {result}");
    }
}

fn demo_layout() {
    println!("=== Layout Engine ===");

    let mut tree = FormTree::new();

    // Create form fields
    let name_field = tree.add_node(FormNode {
        name: "CustomerName".to_string(),
        node_type: FormNodeType::Field {
            value: "Acme Corp".to_string(),
        },
        box_model: BoxModel {
            width: Some(300.0),
            height: Some(25.0),
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

    let addr_field = tree.add_node(FormNode {
        name: "Address".to_string(),
        node_type: FormNodeType::Field {
            value: "123 Main St".to_string(),
        },
        box_model: BoxModel {
            width: Some(300.0),
            height: Some(25.0),
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

    let total_field = tree.add_node(FormNode {
        name: "Total".to_string(),
        node_type: FormNodeType::Field {
            value: "112.50".to_string(),
        },
        box_model: BoxModel {
            width: Some(150.0),
            height: Some(25.0),
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

    // Create a subform container
    let detail_ids: Vec<FormNodeId> = vec![name_field, addr_field, total_field];
    let form_subform = tree.add_node(FormNode {
        name: "InvoiceForm".to_string(),
        node_type: FormNodeType::Subform,
        box_model: BoxModel {
            width: Some(500.0),
            height: Some(200.0),
            max_width: f64::MAX,
            max_height: f64::MAX,
            ..Default::default()
        },
        layout: LayoutStrategy::TopToBottom,
        children: detail_ids,
        occur: Occur::once(),
        font: FontMetrics::default(),
        calculate: None,
        validate: None,
        column_widths: vec![],
        col_span: 1,
    });

    // Root
    let root = tree.add_node(FormNode {
        name: "Root".to_string(),
        node_type: FormNodeType::Root,
        box_model: BoxModel {
            width: Some(612.0),
            height: Some(792.0),
            max_width: f64::MAX,
            max_height: f64::MAX,
            ..Default::default()
        },
        layout: LayoutStrategy::TopToBottom,
        children: vec![form_subform],
        occur: Occur::once(),
        font: FontMetrics::default(),
        calculate: None,
        validate: None,
        column_widths: vec![],
        col_span: 1,
    });

    let engine = LayoutEngine::new(&tree);
    let layout = engine.layout(root).expect("Layout failed");

    println!("  Pages: {}", layout.pages.len());
    for (i, page) in layout.pages.iter().enumerate() {
        println!("  Page {}: {}x{} pt", i + 1, page.width, page.height);
        print_layout_tree(&page.nodes, 2);
    }
}

fn print_layout_tree(nodes: &[xfa_layout_engine::layout::LayoutNode], indent: usize) {
    for node in nodes {
        let pad = " ".repeat(indent * 2);
        let content = match &node.content {
            xfa_layout_engine::layout::LayoutContent::Field { value } => {
                format!(" = \"{value}\"")
            }
            xfa_layout_engine::layout::LayoutContent::Text(t) => format!(" text=\"{t}\""),
            xfa_layout_engine::layout::LayoutContent::WrappedText {
                ref lines,
                font_size,
            } => {
                format!(" wrapped_text[{}lines, {font_size}pt]", lines.len())
            }
            xfa_layout_engine::layout::LayoutContent::None => String::new(),
        };
        println!(
            "  {pad}{} @ ({:.0}, {:.0}) {}x{}{content}",
            node.name, node.rect.x, node.rect.y, node.rect.width, node.rect.height
        );
        if !node.children.is_empty() {
            print_layout_tree(&node.children, indent + 1);
        }
    }
}
