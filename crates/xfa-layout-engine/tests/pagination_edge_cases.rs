use xfa_layout_engine::form::{FormNode, FormNodeId, FormNodeType, FormTree, Occur};
use xfa_layout_engine::layout::{LayoutEngine, LayoutNode};
use xfa_layout_engine::text::FontMetrics;
use xfa_layout_engine::types::{BoxModel, LayoutStrategy};

fn make_field(tree: &mut FormTree, name: &str, w: f64, h: f64) -> FormNodeId {
    tree.add_node(FormNode {
        name: name.to_string(),
        node_type: FormNodeType::Field {
            value: name.to_string(),
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

fn make_subform(
    tree: &mut FormTree,
    name: &str,
    strategy: LayoutStrategy,
    w: Option<f64>,
    h: Option<f64>,
    children: Vec<FormNodeId>,
) -> FormNodeId {
    tree.add_node(FormNode {
        name: name.to_string(),
        node_type: FormNodeType::Subform,
        box_model: BoxModel {
            width: w,
            height: h,
            max_width: f64::MAX,
            max_height: f64::MAX,
            ..Default::default()
        },
        layout: strategy,
        children,
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

fn collect_named_nodes<'a>(nodes: &'a [LayoutNode], name: &str, out: &mut Vec<&'a LayoutNode>) {
    for node in nodes {
        if node.name == name {
            out.push(node);
        }
        collect_named_nodes(&node.children, name, out);
    }
}

fn count_nodes_with_prefix(nodes: &[LayoutNode], prefix: &str) -> usize {
    nodes
        .iter()
        .map(|node| {
            usize::from(node.name.starts_with(prefix))
                + count_nodes_with_prefix(&node.children, prefix)
        })
        .sum()
}

#[test]
fn break_before_auto_subform_just_over_page_boundary_creates_second_page() {
    let mut tree = FormTree::new();
    let header = make_field(&mut tree, "Header", 200.0, 70.0);
    let row = make_field(&mut tree, "Row", 200.0, 31.0);
    let auto_break_subform = make_subform(
        &mut tree,
        "AutoBreakBlock",
        LayoutStrategy::TopToBottom,
        Some(200.0),
        None,
        vec![row],
    );
    let root = make_root(&mut tree, 200.0, 100.0, vec![header, auto_break_subform]);

    let result = LayoutEngine::new(&tree).layout(root).unwrap();

    // `breakBefore="auto"` is the default flow behavior: once the subform
    // falls just past the page boundary, layout must continue on page 2.
    assert_eq!(result.pages.len(), 2);
    assert_eq!(result.pages[0].nodes[0].name, "Header");
    assert!(!result.pages[1].nodes.is_empty());
}

#[test]
fn nested_fixed_height_inner_subforms_do_not_split_when_outer_overflows() {
    let mut tree = FormTree::new();

    let inner_a_row0 = make_field(&mut tree, "InnerA_Row0", 200.0, 30.0);
    let inner_a_row1 = make_field(&mut tree, "InnerA_Row1", 200.0, 30.0);
    let inner_a = make_subform(
        &mut tree,
        "InnerFixed",
        LayoutStrategy::TopToBottom,
        Some(200.0),
        Some(60.0),
        vec![inner_a_row0, inner_a_row1],
    );
    let inner_b_row0 = make_field(&mut tree, "InnerB_Row0", 200.0, 30.0);
    let inner_b_row1 = make_field(&mut tree, "InnerB_Row1", 200.0, 30.0);
    let inner_b = make_subform(
        &mut tree,
        "InnerFixed",
        LayoutStrategy::TopToBottom,
        Some(200.0),
        Some(60.0),
        vec![inner_b_row0, inner_b_row1],
    );
    let outer = make_subform(
        &mut tree,
        "OuterOverflow",
        LayoutStrategy::TopToBottom,
        Some(200.0),
        None,
        vec![inner_a, inner_b],
    );
    let root = make_root(&mut tree, 200.0, 100.0, vec![outer]);

    let result = LayoutEngine::new(&tree).layout(root).unwrap();

    assert_eq!(result.pages.len(), 2);

    let mut inner_nodes = Vec::new();
    for page in &result.pages {
        collect_named_nodes(&page.nodes, "InnerFixed", &mut inner_nodes);
    }

    assert_eq!(inner_nodes.len(), 2);
    for inner in inner_nodes {
        assert_eq!(inner.rect.height, 60.0);
        assert_eq!(inner.children.len(), 2);
    }
}

#[test]
fn paginate_subform_starts_new_page_every_three_items() {
    let mut tree = FormTree::new();

    let mut items = Vec::new();
    for idx in 0..9 {
        items.push(make_field(&mut tree, &format!("Item{idx}"), 200.0, 25.0));
    }

    let paginated = make_subform(
        &mut tree,
        "PaginatedItems",
        LayoutStrategy::TopToBottom,
        Some(200.0),
        None,
        items,
    );
    let root = make_root(&mut tree, 200.0, 75.0, vec![paginated]);

    let result = LayoutEngine::new(&tree).layout(root).unwrap();

    assert_eq!(result.pages.len(), 3);
    assert_eq!(count_nodes_with_prefix(&result.pages[0].nodes, "Item"), 3);
    assert_eq!(count_nodes_with_prefix(&result.pages[1].nodes, "Item"), 3);
    assert_eq!(count_nodes_with_prefix(&result.pages[2].nodes, "Item"), 3);
}

#[test]
fn empty_subform_with_break_before_page_still_forces_second_page() {
    let mut tree = FormTree::new();

    let header = make_field(&mut tree, "Header", 200.0, 20.0);
    let empty = make_subform(
        &mut tree,
        "ForcedBreak",
        LayoutStrategy::TopToBottom,
        Some(200.0),
        Some(0.0),
        vec![],
    );
    tree.meta_mut(empty).page_break_before = true;

    let root = make_root(&mut tree, 200.0, 100.0, vec![header, empty]);
    let result = LayoutEngine::new(&tree).layout(root).unwrap();

    assert_eq!(result.pages.len(), 2);
    assert_eq!(result.pages[0].nodes[0].name, "Header");
    assert_eq!(result.pages[1].nodes[0].name, "ForcedBreak");
}
