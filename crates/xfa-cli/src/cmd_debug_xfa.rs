//! Debug subcommand — dump the XFA render tree for inspection.
//!
//! Developer tool for analysing the intermediate representation produced by
//! the XFA flatten pipeline.  Not a user-facing feature.
//!
//! Usage:
//!   pdfluent debug-xfa <input.pdf> [--format tree|json]

use anyhow::{Context, Result};
use std::path::Path;

/// Output format for the render-tree dump.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugFormat {
    /// Human-readable, indented tree (default).
    Tree,
    /// Machine-readable JSON-like representation.
    Json,
}

impl std::str::FromStr for DebugFormat {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "tree" => Ok(DebugFormat::Tree),
            "json" => Ok(DebugFormat::Json),
            other => Err(format!(
                "unknown format '{other}' — expected 'tree' or 'json'"
            )),
        }
    }
}

pub fn run(input: &Path, format: DebugFormat) -> Result<()> {
    let pdf_bytes =
        std::fs::read(input).with_context(|| format!("failed to read '{}'", input.display()))?;

    // --- Step 1: Extract XFA packets from the PDF. ---
    let packets =
        pdf_xfa::extract::extract_xfa_from_bytes(pdf_bytes.clone()).with_context(|| {
            format!(
                "'{}' does not appear to contain an XFA form (no XFA packets found)",
                input.display()
            )
        })?;

    let template_xml = match packets.template() {
        Some(t) => t.to_string(),
        None => {
            anyhow::bail!(
                "'{}' has XFA packets but no template packet — cannot build render tree",
                input.display()
            );
        }
    };

    // --- Step 2: Build the form tree via FormMerger. ---
    let datasets_xml = packets.datasets().map(|s| s.to_string());

    let data_dom = if let Some(ref ds) = datasets_xml {
        pdf_xfa::dom_resolver::data_dom::DataDom::from_xml(ds)
            .unwrap_or_else(|_| pdf_xfa::dom_resolver::data_dom::DataDom::new())
    } else {
        pdf_xfa::dom_resolver::data_dom::DataDom::new()
    };

    let merger = pdf_xfa::merger::FormMerger::new(&data_dom);
    let (tree, root_id) = merger
        .merge(&template_xml)
        .context("failed to merge XFA template — the template XML may be malformed")?;

    // --- Step 3: Run the layout engine. ---
    let engine = xfa_layout_engine::layout::LayoutEngine::new(&tree);
    let layout = engine
        .layout(root_id)
        .context("layout engine failed — the XFA template structure may be unsupported")?;

    if layout.pages.is_empty() {
        eprintln!("Warning: layout produced 0 pages — render tree will be empty");
    }

    // --- Step 4: Build the render tree. ---
    let config = pdf_xfa::render_bridge::XfaRenderConfig::default();
    let render_tree = pdf_xfa::render_bridge::layout_dom_to_render_tree(&layout, &config);

    // --- Step 5: Output. ---
    match format {
        DebugFormat::Tree => {
            println!("Render tree for: {}", input.display());
            println!("Pages: {}", render_tree.pages.len());
            println!();
            print!("{}", render_tree.to_debug_string());
        }
        DebugFormat::Json => {
            print_json(&render_tree);
        }
    }

    Ok(())
}

/// Print a JSON-like representation of the render tree.
/// Uses a hand-rolled emitter to avoid adding a serde dependency.
fn print_json(tree: &pdf_xfa::render_bridge::RenderTree) {
    println!("{{");
    println!("  \"pages\": [");
    for (pi, page) in tree.pages.iter().enumerate() {
        let comma = if pi + 1 < tree.pages.len() { "," } else { "" };
        print_json_node(page, 4);
        println!("{comma}");
    }
    println!("  ]");
    println!("}}");
}

fn print_json_node(node: &pdf_xfa::render_bridge::RenderNode, indent: usize) {
    use pdf_xfa::render_bridge::RenderNode;
    let pad = " ".repeat(indent);
    let pad2 = " ".repeat(indent + 2);
    match node {
        RenderNode::Page {
            width,
            height,
            children,
        } => {
            print!(
                "{pad}{{\"type\":\"Page\",\"width\":{width:.1},\"height\":{height:.1},\"children\":["
            );
            if children.is_empty() {
                print!("]}}");
            } else {
                println!();
                for (i, c) in children.iter().enumerate() {
                    let comma = if i + 1 < children.len() { "," } else { "" };
                    print_json_node(c, indent + 2);
                    print!("{comma}");
                    println!();
                }
                print!("{pad2}]}}");
            }
        }
        RenderNode::Text {
            x,
            y,
            content,
            font,
            size,
        } => {
            let escaped = json_escape(content);
            let font_esc = json_escape(font);
            print!(
                "{pad}{{\"type\":\"Text\",\"x\":{x:.1},\"y\":{y:.1},\
                \"font\":\"{font_esc}\",\"size\":{size:.1},\"content\":\"{escaped}\"}}"
            );
        }
        RenderNode::Rect {
            x,
            y,
            width,
            height,
            fill,
            stroke,
        } => {
            let fill_str = fill
                .map(|[r, g, b]| format!("\"#{r:02X}{g:02X}{b:02X}\""))
                .unwrap_or_else(|| "null".to_string());
            let stroke_str = stroke
                .map(|[r, g, b]| format!("\"#{r:02X}{g:02X}{b:02X}\""))
                .unwrap_or_else(|| "null".to_string());
            print!(
                "{pad}{{\"type\":\"Rect\",\"x\":{x:.1},\"y\":{y:.1},\
                \"width\":{width:.1},\"height\":{height:.1},\
                \"fill\":{fill_str},\"stroke\":{stroke_str}}}"
            );
        }
        RenderNode::Image {
            x,
            y,
            width,
            height,
            data_len,
        } => {
            print!(
                "{pad}{{\"type\":\"Image\",\"x\":{x:.1},\"y\":{y:.1},\
                \"width\":{width:.1},\"height\":{height:.1},\"dataLen\":{data_len}}}"
            );
        }
        RenderNode::Widget {
            x,
            y,
            width,
            height,
            field_name,
            value,
        } => {
            let name_esc = json_escape(field_name);
            let val_esc = json_escape(value);
            print!(
                "{pad}{{\"type\":\"Widget\",\"x\":{x:.1},\"y\":{y:.1},\
                \"width\":{width:.1},\"height\":{height:.1},\
                \"fieldName\":\"{name_esc}\",\"value\":\"{val_esc}\"}}"
            );
        }
        RenderNode::Group { children } => {
            print!("{pad}{{\"type\":\"Group\",\"children\":[");
            if children.is_empty() {
                print!("]}}");
            } else {
                println!();
                for (i, c) in children.iter().enumerate() {
                    let comma = if i + 1 < children.len() { "," } else { "" };
                    print_json_node(c, indent + 2);
                    print!("{comma}");
                    println!();
                }
                print!("{pad2}]}}");
            }
        }
    }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}
