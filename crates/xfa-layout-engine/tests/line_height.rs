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

fn layout_draw(value: &str, line_height_pt: Option<f64>) -> LayoutNode {
    let mut tree = FormTree::new();
    let draw = make_draw_text(&mut tree, "multi_line_text", value, 200.0);
    tree.meta_mut(draw).style.line_height_pt = line_height_pt;

    let root = make_root(&mut tree, 200.0, 100.0, vec![draw]);
    let layout = LayoutEngine::new(&tree).layout(root).unwrap();

    find_named_node(&layout.pages[0].nodes, "multi_line_text")
        .expect("multi_line_text should appear in the first page layout")
        .clone()
}

#[test]
fn line_height_override_increases_draw_text_height() {
    let text = "Line 1\nLine 2\nLine 3";

    let default_node = layout_draw(text, None);
    let overridden_node = layout_draw(text, Some(20.0));

    match &default_node.content {
        LayoutContent::WrappedText { lines, .. } => assert_eq!(lines.len(), 3),
        other => panic!("expected WrappedText content, got {other:?}"),
    }
    match &overridden_node.content {
        LayoutContent::WrappedText { lines, .. } => assert_eq!(lines.len(), 3),
        other => panic!("expected WrappedText content, got {other:?}"),
    }

    assert!(
        overridden_node.rect.height > default_node.rect.height,
        "lineHeight override should increase total text height: default={} override={}",
        default_node.rect.height,
        overridden_node.rect.height
    );
    assert!(
        (default_node.rect.height - 36.0).abs() < 0.01,
        "default 3-line height should be 36pt, got {}",
        default_node.rect.height
    );
    assert!(
        (overridden_node.rect.height - 60.0).abs() < 0.01,
        "lineHeight=20pt should produce 60pt total height, got {}",
        overridden_node.rect.height
    );
}
