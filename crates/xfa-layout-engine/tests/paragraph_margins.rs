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

fn lines_for_field(
    value: &str,
    margin_left: Option<f64>,
    margin_right: Option<f64>,
) -> Vec<String> {
    let mut tree = FormTree::new();
    let field = make_text_field(&mut tree, "wrapped_field", value, 200.0, 60.0);
    let style = &mut tree.meta_mut(field).style;
    style.margin_left_pt = margin_left;
    style.margin_right_pt = margin_right;

    let root = make_root(&mut tree, 200.0, 100.0, vec![field]);
    let layout = LayoutEngine::new(&tree).layout(root).unwrap();
    let field_node = find_named_node(&layout.pages[0].nodes, "wrapped_field")
        .expect("wrapped_field should appear in the first page layout");

    match &field_node.content {
        LayoutContent::WrappedText { lines, .. } => lines.clone(),
        other => panic!("expected WrappedText content, got {other:?}"),
    }
}

#[test]
fn paragraph_margins_reduce_available_width_for_wrapping() {
    let text = "AAAAAA AAAAAA AAAAAA AAAAA AAAAA";
    let measured_width = FontMetrics::default().measure_width(text);
    assert!(
        measured_width > 180.0 && measured_width <= 200.0,
        "test text must fit 200pt but overflow 180pt, got width {measured_width:.2}: {text:?}"
    );

    let lines_without_margins = lines_for_field(text, None, None);
    assert_eq!(
        lines_without_margins.len(),
        1,
        "text should stay on one line without paragraph margins: {text:?}"
    );

    let lines_with_margins = lines_for_field(text, Some(10.0), Some(10.0));
    assert_eq!(
        lines_with_margins.len(),
        2,
        "10pt left/right paragraph margins should reduce available width from 200pt to 180pt"
    );
    assert_eq!(lines_with_margins[0], "AAAAAA AAAAAA AAAAAA AAAAA");
    assert_eq!(lines_with_margins[1], "AAAAA");
}
