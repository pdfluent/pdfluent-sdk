//! QF1-F — Per-subform overflow leader/trailer extension.
//!
//! XFA Spec 3.3 §17 (Overflow) + §8.10 (Leaders and Trailers) — a
//! `<overflow leader="..." trailer="...">` element may be declared on
//! any subform, not just the root.  When a non-root subform overflows
//! pages, its own overflow refs should govern the leader/trailer on
//! the continuation pages.
//!
//! W2-A established the bounded MVP: only the root subform's
//! `FormNodeMeta::overflow_leader`/`overflow_trailer` are consulted.
//! Wave-1 corpus evidence found **zero** docs adversely affected by
//! this gap, so the per-subform extension is pure Adobe-class
//! fidelity work.
//!
//! QF1-F extends the wire-up to consult per-subform overflow refs.
//! The "active" subform — the closest ancestor of the next queued
//! node that declares overflow refs — wins over the root's refs.
//! When no nested subform declares overflow, behaviour collapses to
//! W2-A (root-only) semantics.
//!
//! These regression tests pin the extended behaviour:
//!
//! 1. A nested subform with its own overflow refs renders ITS
//!    declared leader/trailer on the continuation pages where its
//!    content paginates.
//! 2. With no overflow refs anywhere (neither root nor nested),
//!    behaviour is identical to the W2-A baseline.

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

fn count_named(nodes: &[LayoutNode], name: &str) -> usize {
    nodes
        .iter()
        .map(|n| usize::from(n.name == name) + count_named(&n.children, name))
        .sum()
}

fn first_with_name<'a>(nodes: &'a [LayoutNode], name: &str) -> Option<&'a LayoutNode> {
    for n in nodes {
        if n.name == name {
            return Some(n);
        }
        if let Some(found) = first_with_name(&n.children, name) {
            return Some(found);
        }
    }
    None
}

/// QF1-F core regression test:
///
/// A NON-root subform (`InnerContent`) declares its own
/// `overflow_leader`/`overflow_trailer` SOM refs.  The root subform
/// declares no overflow refs.  When `InnerContent` paginates across
/// multiple pages, the per-subform extension must render
/// `InnerContent`'s declared leader at the top of every continuation
/// page and trailer at the bottom.
///
/// Pre-QF1-F (W2-A only): would NOT render any leader/trailer because
/// only the root's refs were consulted.
#[test]
fn per_subform_overflow_refs_render_on_continuation_pages() {
    let mut tree = FormTree::new();

    // Sibling subforms inside `InnerContent`'s parent scope that act
    // as the per-subform leader/trailer.  They must be siblings of
    // `InnerContent` so `lookup_overflow_target` resolves them via
    // the parent-chain sibling walk.
    let inner_header = make_subform(
        &mut tree,
        "InnerHeader",
        LayoutStrategy::TopToBottom,
        Some(200.0),
        Some(10.0),
        vec![],
    );
    let inner_footer = make_subform(
        &mut tree,
        "InnerFooter",
        LayoutStrategy::TopToBottom,
        Some(200.0),
        Some(10.0),
        vec![],
    );

    // Content nested inside `InnerContent` that will paginate.
    let mut items = Vec::new();
    for idx in 0..12 {
        items.push(make_field(&mut tree, &format!("Item{idx}"), 200.0, 30.0));
    }
    let inner_content = make_subform(
        &mut tree,
        "InnerContent",
        LayoutStrategy::TopToBottom,
        Some(200.0),
        None,
        items,
    );

    // Root has three children: InnerContent (the overflowing subform)
    // plus the two leader/trailer siblings.  No overflow refs on root.
    let root = make_root(
        &mut tree,
        200.0,
        100.0,
        vec![inner_content, inner_header, inner_footer],
    );

    // Attach overflow refs to InnerContent only (NOT the root).
    {
        let meta = tree.meta_mut(inner_content);
        meta.overflow_leader = Some("InnerHeader".to_string());
        meta.overflow_trailer = Some("InnerFooter".to_string());
    }

    let result = LayoutEngine::new(&tree).layout(root).unwrap();
    let page_count = result.pages.len();
    assert!(
        page_count >= 3,
        "expected >=3 pages for content overflow; got {}",
        page_count
    );

    // Page 1: no leader/trailer (overflow refs apply only to
    // continuation pages, matching XFA §17 semantics consistent with
    // W2-A behaviour for the root-level case).
    assert_eq!(
        count_named(&result.pages[0].nodes, "InnerHeader"),
        0,
        "page 1 must not render the per-subform leader"
    );
    assert_eq!(
        count_named(&result.pages[0].nodes, "InnerFooter"),
        0,
        "page 1 must not render the per-subform trailer"
    );

    // Continuation pages: per-subform leader/trailer present.
    for (idx, page) in result.pages.iter().enumerate().skip(1) {
        let h = count_named(&page.nodes, "InnerHeader");
        let f = count_named(&page.nodes, "InnerFooter");
        assert_eq!(
            h,
            1,
            "continuation page {} should render InnerHeader once (got {})",
            idx + 1,
            h
        );
        assert_eq!(
            f,
            1,
            "continuation page {} should render InnerFooter once (got {})",
            idx + 1,
            f
        );

        let leader = first_with_name(&page.nodes, "InnerHeader")
            .expect("InnerHeader present on continuation page");
        assert!(
            leader.rect.y < 5.0,
            "InnerHeader should be near the top of page {} (y={})",
            idx + 1,
            leader.rect.y
        );
        let trailer = first_with_name(&page.nodes, "InnerFooter")
            .expect("InnerFooter present on continuation page");
        assert!(
            trailer.rect.y > 50.0,
            "InnerFooter should be near the bottom of page {} (y={})",
            idx + 1,
            trailer.rect.y
        );
    }

    // All 12 items must remain placed somewhere — leader/trailer
    // resolution must not drop content.
    let total_items: usize = result
        .pages
        .iter()
        .map(|p| {
            (0..12)
                .map(|i| count_named(&p.nodes, &format!("Item{i}")))
                .sum::<usize>()
        })
        .sum();
    assert_eq!(
        total_items, 12,
        "all 12 items must remain placed across pages after per-subform wire-up"
    );

    // Filter guard: the resolved per-subform leader/trailer subforms
    // must NOT also render as regular content (no double-render).
    // Total leader occurrences across all pages = page_count - 1
    // (continuation pages only).  If they double-rendered we'd see
    // page_count occurrences instead.
    let total_leader: usize = result
        .pages
        .iter()
        .map(|p| count_named(&p.nodes, "InnerHeader"))
        .sum();
    let total_trailer: usize = result
        .pages
        .iter()
        .map(|p| count_named(&p.nodes, "InnerFooter"))
        .sum();
    assert_eq!(
        total_leader,
        page_count - 1,
        "InnerHeader must render once per continuation page (no double-render)"
    );
    assert_eq!(
        total_trailer,
        page_count - 1,
        "InnerFooter must render once per continuation page (no double-render)"
    );
}

/// QF1-F regression guard:
///
/// When neither the root nor any nested subform declares overflow
/// refs, the per-subform extension must collapse to the W2-A
/// baseline (no leader/trailer injected, page count purely
/// content-driven).  This pins the no-refs branch identity.
#[test]
fn per_subform_no_refs_anywhere_preserves_baseline() {
    let mut tree = FormTree::new();

    let mut items = Vec::new();
    for idx in 0..10 {
        items.push(make_field(&mut tree, &format!("Item{idx}"), 200.0, 30.0));
    }
    // Wrap items inside a nested subform with NO overflow refs.
    let inner = make_subform(
        &mut tree,
        "Inner",
        LayoutStrategy::TopToBottom,
        Some(200.0),
        None,
        items,
    );
    let root = make_root(&mut tree, 200.0, 100.0, vec![inner]);

    let result = LayoutEngine::new(&tree).layout(root).unwrap();
    let page_count = result.pages.len();
    assert!(
        page_count >= 2,
        "expected pagination with content overflow; got {}",
        page_count
    );

    // Deterministic identity across calls — the per-subform code path
    // must not produce non-determinism when no refs are declared.
    assert_eq!(
        page_count,
        LayoutEngine::new(&tree).layout(root).unwrap().pages.len(),
        "no-refs branch must be deterministic + identical across calls"
    );

    // No phantom leader/trailer subforms appear anywhere.
    for page in &result.pages {
        // "Inner" is the content subform itself; it renders normally
        // and is not a leader/trailer.  We assert that no NEW phantom
        // subform names are introduced by the per-subform code path.
        assert_eq!(
            count_named(&page.nodes, "InnerHeader"),
            0,
            "no leader subform should ever appear when no refs declared"
        );
        assert_eq!(
            count_named(&page.nodes, "InnerFooter"),
            0,
            "no trailer subform should ever appear when no refs declared"
        );
    }
}

/// QF1-F precedence test:
///
/// When BOTH the root and a nested subform declare overflow refs, the
/// nested subform's refs must win on pages where the nested subform's
/// content is being paginated.  This pins the "closest ancestor wins"
/// semantics described in QF1-F (XFA §17 most-specific scope).
#[test]
fn per_subform_refs_override_root_refs() {
    let mut tree = FormTree::new();

    // Root-level leader/trailer.
    let root_header = make_subform(
        &mut tree,
        "RootHeader",
        LayoutStrategy::TopToBottom,
        Some(200.0),
        Some(10.0),
        vec![],
    );
    let root_footer = make_subform(
        &mut tree,
        "RootFooter",
        LayoutStrategy::TopToBottom,
        Some(200.0),
        Some(10.0),
        vec![],
    );
    // Inner-level leader/trailer that should take precedence.
    let inner_header = make_subform(
        &mut tree,
        "InnerHeader",
        LayoutStrategy::TopToBottom,
        Some(200.0),
        Some(10.0),
        vec![],
    );
    let inner_footer = make_subform(
        &mut tree,
        "InnerFooter",
        LayoutStrategy::TopToBottom,
        Some(200.0),
        Some(10.0),
        vec![],
    );

    let mut items = Vec::new();
    for idx in 0..12 {
        items.push(make_field(&mut tree, &format!("Item{idx}"), 200.0, 30.0));
    }
    let inner_content = make_subform(
        &mut tree,
        "InnerContent",
        LayoutStrategy::TopToBottom,
        Some(200.0),
        None,
        items,
    );

    let root = make_root(
        &mut tree,
        200.0,
        100.0,
        vec![
            inner_content,
            inner_header,
            inner_footer,
            root_header,
            root_footer,
        ],
    );
    // Root declares its own refs.
    {
        let meta = tree.meta_mut(root);
        meta.overflow_leader = Some("RootHeader".to_string());
        meta.overflow_trailer = Some("RootFooter".to_string());
    }
    // Inner subform overrides with its own refs.
    {
        let meta = tree.meta_mut(inner_content);
        meta.overflow_leader = Some("InnerHeader".to_string());
        meta.overflow_trailer = Some("InnerFooter".to_string());
    }

    let result = LayoutEngine::new(&tree).layout(root).unwrap();
    let page_count = result.pages.len();
    assert!(
        page_count >= 3,
        "expected >=3 pages for content overflow; got {}",
        page_count
    );

    // Continuation pages should render the INNER leader/trailer
    // because the inner subform is the closest ancestor of the
    // queued items, not the root.
    let mut continuation_pages_with_inner_leader = 0;
    let mut continuation_pages_with_inner_trailer = 0;
    for page in result.pages.iter().skip(1) {
        if count_named(&page.nodes, "InnerHeader") >= 1 {
            continuation_pages_with_inner_leader += 1;
        }
        if count_named(&page.nodes, "InnerFooter") >= 1 {
            continuation_pages_with_inner_trailer += 1;
        }
    }
    assert_eq!(
        continuation_pages_with_inner_leader,
        page_count - 1,
        "InnerHeader (closest ancestor) must win over RootHeader on continuation pages"
    );
    assert_eq!(
        continuation_pages_with_inner_trailer,
        page_count - 1,
        "InnerFooter (closest ancestor) must win over RootFooter on continuation pages"
    );

    // Root-level leader/trailer subforms must NEVER render: they were
    // overridden by inner-level refs and must not appear as regular
    // content either (the all-overflow-ids filter must exclude them).
    let total_root_leader: usize = result
        .pages
        .iter()
        .map(|p| count_named(&p.nodes, "RootHeader"))
        .sum();
    let total_root_trailer: usize = result
        .pages
        .iter()
        .map(|p| count_named(&p.nodes, "RootFooter"))
        .sum();
    assert_eq!(
        total_root_leader, 0,
        "RootHeader must NOT render when overridden by inner subform"
    );
    assert_eq!(
        total_root_trailer, 0,
        "RootFooter must NOT render when overridden by inner subform"
    );
}
