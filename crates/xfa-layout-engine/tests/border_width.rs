// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use xfa_layout_engine::form::{FormNode, FormNodeId, FormNodeType, FormTree, Occur};
use xfa_layout_engine::layout::{LayoutContent, LayoutEngine, LayoutNode};
use xfa_layout_engine::text::FontMetrics;
use xfa_layout_engine::types::{BoxModel, LayoutStrategy};

fn make_text_field(tree: &mut FormTree, name: &str, value: &str, w: f64, h: f64) -> FormNodeId {
    tree.add_node(FormNode {
        name: name.to_string(),
        node_type: FormNodeType::Field {
            value: value.to_string(),
        },
        box_model: BoxModel {
            width: Some(w),
            height: Some(h),
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

fn lines_for_field(value: &str, border_width_pt: Option<f64>) -> Vec<String> {
    let mut tree = FormTree::new();
    let field = make_text_field(&mut tree, "wrapped_field", value, 100.0, 60.0);

    {
        let style = &mut tree.meta_mut(field).style;
        style.margin_left_pt = Some(0.0);
        style.margin_right_pt = Some(0.0);
        style.border_width_pt = border_width_pt;
    }
    tree.get_mut(field).box_model.border_width = border_width_pt.unwrap_or(0.0);

    let root = make_root(&mut tree, 100.0, 100.0, vec![field]);
    let layout = LayoutEngine::new(&tree).layout(root).unwrap();
    let field_node = find_named_node(&layout.pages[0].nodes, "wrapped_field")
        .expect("wrapped_field should appear in the first page layout");

    match &field_node.content {
        LayoutContent::WrappedText { lines, .. } => lines.clone(),
        other => panic!("expected WrappedText content, got {other:?}"),
    }
}

#[test]
fn border_width_reduces_available_width_for_wrapping() {
    let text = "AAAAAA AAAAAA AA";
    let measured_width = FontMetrics::default().measure_width(text);
    assert!(
        measured_width > 90.0 && measured_width <= 100.0,
        "test text must fit 100pt but overflow 90pt, got width {measured_width:.2}: {text:?}"
    );

    let lines_without_border = lines_for_field(text, None);
    assert_eq!(
        lines_without_border.len(),
        1,
        "text should stay on one line without border width: {text:?}"
    );

    let lines_with_border = lines_for_field(text, Some(5.0));
    assert_eq!(
        lines_with_border.len(),
        2,
        "5pt border on each side should reduce available width from 100pt to 90pt"
    );
    assert_eq!(lines_with_border[0], "AAAAAA AAAAAA");
    assert_eq!(lines_with_border[1], "AA");
}

#[test]
fn two_point_border_shrinks_content_width_by_four_points() {
    let no_border = BoxModel {
        width: Some(100.0),
        max_width: f64::MAX,
        max_height: f64::MAX,
        ..Default::default()
    };
    let two_point_border = BoxModel {
        width: Some(100.0),
        border_width: 2.0,
        max_width: f64::MAX,
        max_height: f64::MAX,
        ..Default::default()
    };

    assert_eq!(no_border.content_width(), 100.0);
    assert_eq!(two_point_border.content_width(), 96.0);
    assert_eq!(
        no_border.content_width() - two_point_border.content_width(),
        4.0,
        "2pt border on both sides should shrink the content area by exactly 4pt"
    );
}
