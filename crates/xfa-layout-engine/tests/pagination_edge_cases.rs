use xfa_layout_engine::form::{DrawContent, FormNode, FormNodeId, FormNodeType, FormTree, Occur};
use xfa_layout_engine::layout::{LayoutContent, LayoutEngine, LayoutNode};
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

    // When a splittable subform exceeds the remaining space by ≤1pt, the
    // split tolerance absorbs the overshoot and the content fits on one page.
    // (Previously, the keep-chain heuristic pushed the entire subform to
    // page 2, wasting space.  Fixed in #866.)
    assert_eq!(result.pages.len(), 1);
    assert_eq!(result.pages[0].nodes[0].name, "Header");
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

/// TB subform with explicit height whose children exceed that height AND
/// the page height should be split across pages (#768 — under-pagination).
#[test]
fn tb_subform_with_explicit_height_overflowing_content_paginates() {
    let mut tree = FormTree::new();

    // TB subform with h=60 but 5 children of 30pt each = 150pt content.
    // Page height is 100pt, so 150pt requires >=2 pages.
    // Before #768 compute_extent returned 60pt (explicit h), fitting on
    // one page; now it returns 150pt, triggering pagination.
    let r1 = make_field(&mut tree, "Row1", 200.0, 30.0);
    let r2 = make_field(&mut tree, "Row2", 200.0, 30.0);
    let r3 = make_field(&mut tree, "Row3", 200.0, 30.0);
    let r4 = make_field(&mut tree, "Row4", 200.0, 30.0);
    let r5 = make_field(&mut tree, "Row5", 200.0, 30.0);
    let inner = make_subform(
        &mut tree,
        "OverflowTB",
        LayoutStrategy::TopToBottom,
        Some(200.0),
        Some(60.0), // explicit height << actual content (150pt)
        vec![r1, r2, r3, r4, r5],
    );
    let root = make_root(&mut tree, 200.0, 100.0, vec![inner]);

    let result = LayoutEngine::new(&tree).layout(root).unwrap();

    assert!(
        result.pages.len() >= 2,
        "expected >=2 pages for overflowing TB subform, got {}",
        result.pages.len()
    );
    // All 5 rows should appear across all pages
    let total_rows: usize = result
        .pages
        .iter()
        .map(|p| count_nodes_with_prefix(&p.nodes, "Row"))
        .sum();
    assert_eq!(total_rows, 5, "all 5 rows should be placed across pages");
}

/// XFA Spec 3.3 §8.7 — multiline text that overflows a page should split
/// between lines, placing fitting lines on the current page and the rest
/// on the next.
#[test]
fn multiline_text_splits_across_pages() {
    let mut tree = FormTree::new();

    // Create a Draw(Text) node with 10 explicit lines (\n separated).
    // Default font: 10pt, line_height=1.2 -> 12pt per line -> 120pt total.
    // Page height = 72pt -> ~6 lines fit, 4 overflow.
    let long_text = "Line 1\nLine 2\nLine 3\nLine 4\nLine 5\n\
                     Line 6\nLine 7\nLine 8\nLine 9\nLine 10";
    let text_node = tree.add_node(FormNode {
        name: "LongText".to_string(),
        node_type: FormNodeType::Draw(DrawContent::Text(long_text.to_string())),
        box_model: BoxModel {
            width: Some(200.0),
            height: None,
            max_width: f64::MAX,
            max_height: f64::MAX,
            ..Default::default()
        },
        layout: LayoutStrategy::Positioned,
        children: vec![],
        occur: Occur::once(),
        font: FontMetrics::default(), // 10pt, lh=12pt
        calculate: None,
        validate: None,
        column_widths: vec![],
        col_span: 1,
    });

    // Wrap in a TB subform so the layout engine can paginate.
    let wrapper = make_subform(
        &mut tree,
        "Wrapper",
        LayoutStrategy::TopToBottom,
        Some(200.0),
        None,
        vec![text_node],
    );

    let root = make_root(&mut tree, 200.0, 72.0, vec![wrapper]);
    let result = LayoutEngine::new(&tree).layout(root).unwrap();

    // Should produce 2 pages: first page has some lines, second has the rest.
    assert!(
        result.pages.len() >= 2,
        "expected >=2 pages for multiline text split, got {}",
        result.pages.len()
    );

    // Collect all "LongText" nodes across all pages
    let mut text_nodes = Vec::new();
    for page in &result.pages {
        collect_named_nodes(&page.nodes, "LongText", &mut text_nodes);
    }
    assert!(
        text_nodes.len() >= 2,
        "expected text node on at least 2 pages, got {} occurrences",
        text_nodes.len()
    );

    // Both should have WrappedText content with non-empty lines
    let mut total_lines = 0;
    for tn in &text_nodes {
        if let LayoutContent::WrappedText { lines, .. } = &tn.content {
            assert!(!lines.is_empty(), "split text part should have lines");
            total_lines += lines.len();
        }
    }
    assert_eq!(
        total_lines, 10,
        "total lines across pages should equal original 10"
    );
}
