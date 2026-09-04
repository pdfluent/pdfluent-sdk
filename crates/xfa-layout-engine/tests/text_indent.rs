// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use xfa_layout_engine::form::{DrawContent, FormNode, FormNodeId, FormNodeType, FormTree, Occur};
use xfa_layout_engine::layout::{LayoutContent, LayoutEngine, LayoutNode};
use xfa_layout_engine::text::FontMetrics;
use xfa_layout_engine::types::{BoxModel, LayoutStrategy};

fn make_draw_text(tree: &mut FormTree, name: &str, value: &str, width: f64) -> FormNodeId {
    tree.add_node(FormNode {
        name: name.to_string(),
        node_type: FormNodeType::Draw(DrawContent::Text(value.to_string())),
        box_model: BoxModel {
            width: Some(width),
            height: None,
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
    })
}

fn make_root(
    tree: &mut FormTree,
    width: f64,
    height: f64,
    children: Vec<FormNodeId>,
) -> FormNodeId {
    tree.add_node(FormNode {
        name: "Root".to_string(),
        node_type: FormNodeType::Root,
        box_model: BoxModel {
            width: Some(width),
            height: Some(height),
            max_width: f64::MAX,
            max_height: f64::MAX,
            ..Default::default()
        },
        layout: LayoutStrategy::TopToBottom,
        children,
        occur: Occur::once(),
        font: FontMetrics::default(),
        calculate: None,
        validate: None,
        column_widths: vec![],
        col_span: 1,
    })
}

fn find_named_node<'a>(nodes: &'a [LayoutNode], name: &str) -> Option<&'a LayoutNode> {
    for node in nodes {
        if node.name == name {
            return Some(node);
        }
        if let Some(found) = find_named_node(&node.children, name) {
            return Some(found);
        }
    }
    None
}

fn lines_for_draw(value: &str, text_indent: Option<f64>) -> Vec<String> {
    let mut tree = FormTree::new();
    let draw = make_draw_text(&mut tree, "indented_text", value, 200.0);
    tree.meta_mut(draw).style.text_indent_pt = text_indent;

    let root = make_root(&mut tree, 200.0, 100.0, vec![draw]);
    let layout = LayoutEngine::new(&tree).layout(root).unwrap();
    let draw_node = find_named_node(&layout.pages[0].nodes, "indented_text")
        .expect("indented_text should appear in the first page layout");

    match &draw_node.content {
        LayoutContent::WrappedText { lines, .. } => lines.clone(),
        other => panic!("expected WrappedText content, got {other:?}"),
    }
}

#[test]
fn text_indent_reduces_first_line_available_width() {
    let text = "AAAAAA AAAAAA AAAAAA AAAAA AAAAA";
    let measured_width = FontMetrics::default().measure_width(text);
    assert!(
        measured_width > 150.0 && measured_width <= 200.0,
        "test text must fit 200pt but overflow 150pt, got width {measured_width:.2}: {text:?}"
    );

    let lines_without_indent = lines_for_draw(text, None);
    assert_eq!(
        lines_without_indent.len(),
        1,
        "text should stay on one line without textIndent: {text:?}"
    );

    let lines_with_indent = lines_for_draw(text, Some(50.0));
    assert_eq!(
        lines_with_indent.len(),
        2,
        "50pt textIndent should reduce first-line width from 200pt to 150pt"
    );
    assert_eq!(lines_with_indent[0], "AAAAAA AAAAAA AAAAAA");
    assert_eq!(lines_with_indent[1], "AAAAA AAAAA");
}
