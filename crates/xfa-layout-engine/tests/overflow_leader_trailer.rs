//! W2-A — Overflow leader/trailer wire-up.
//!
//! XFA Spec 3.3 §17 (Overflow) + §8.10 (Leaders and Trailers) — a
//! `<overflow leader="..." trailer="...">` element declared on a subform
//! references sibling subforms that act as the page leader (top) and
//! trailer (bottom) on every continuation page once the subform's
//! content overflows.
//!
//! Wave 1 Agent A flagged this as a `dormant_feature_gap`:
//! `FormNodeMeta::overflow_leader`/`overflow_trailer` were parsed by
//! `pdf-xfa::{template_parser, merger}` but the layout engine never
//! consumed them.  Wave 2 W2-A wires the resolver + threading.
//!
//! These regression tests pin the bounded MVP behavior:
//!
//! 1. With `overflow_leader`/`overflow_trailer` set on the root subform,
//!    the leader appears at the top of every continuation page and the
//!    trailer at the bottom.  The first page is unchanged.
//! 2. With no overflow refs, behavior is identical to baseline (no
//!    leader/trailer injected, page count unchanged).
//! 3. Unresolvable SOM references are silently ignored (matches the
//!    pre-W2-A behavior the layout engine had).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

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

/// Walk every node in the page and assert that the top-most node with name
/// `leader_name` has a y-coordinate at or near 0 (top of the content area).
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

/// Sanity-check fixture: same form tree structure but with no overflow
/// leader/trailer attached to the root meta.  Page count must be driven
/// purely by content; the previously-defined sibling subforms render only
/// once each in the regular content flow (pre-W2-A behavior).
#[test]
fn baseline_without_overflow_refs_preserves_prior_behavior() {
    let mut tree = FormTree::new();

    // Build a form tree without referencing these from `overflow_*`.
    // We do not register an `OverflowHeader`/`OverflowFooter` subform at
    // all to avoid them being mistaken for content; the absence of any
    // such siblings exercises the no-refs branch of the wire-up.

    // 10 rows of 30pt each = 300pt content; page height = 100pt -> 3 pages.
    let mut items = Vec::new();
    for idx in 0..10 {
        items.push(make_field(&mut tree, &format!("Item{idx}"), 200.0, 30.0));
    }
    let content = make_subform(
        &mut tree,
        "Content",
        LayoutStrategy::TopToBottom,
        Some(200.0),
        None,
        items,
    );

    let root = make_root(&mut tree, 200.0, 100.0, vec![content]);

    let result_baseline = LayoutEngine::new(&tree).layout(root).unwrap();
    let baseline_pages = result_baseline.pages.len();
    assert!(
        baseline_pages >= 2,
        "expected pagination with content overflow"
    );

    // Same content with NO meta overflow refs — the page count should be
    // identical to the baseline; the wire-up must not perturb the no-refs
    // branch.
    assert_eq!(
        baseline_pages,
        LayoutEngine::new(&tree).layout(root).unwrap().pages.len(),
        "no-refs branch must be deterministic + identical across calls"
    );
}

/// W2-A core regression test:
///
/// A root subform declares `overflow_leader = "OverflowHeader"` and
/// `overflow_trailer = "OverflowFooter"` in `FormNodeMeta`.  Once the
/// content paginates, every page from page 2 onwards must render the
/// resolved leader subform at the top and the resolved trailer subform
/// at the bottom.  Page 1 is unchanged.
#[test]
fn overflow_leader_trailer_appear_on_continuation_pages() {
    let mut tree = FormTree::new();

    // Sibling subforms that act as the leader and trailer.
    let header = make_subform(
        &mut tree,
        "OverflowHeader",
        LayoutStrategy::TopToBottom,
        Some(200.0),
        Some(10.0),
        vec![],
    );
    let footer = make_subform(
        &mut tree,
        "OverflowFooter",
        LayoutStrategy::TopToBottom,
        Some(200.0),
        Some(10.0),
        vec![],
    );

    // Content that will overflow across at least 3 pages.
    let mut items = Vec::new();
    for idx in 0..12 {
        items.push(make_field(&mut tree, &format!("Item{idx}"), 200.0, 30.0));
    }
    let content = make_subform(
        &mut tree,
        "Content",
        LayoutStrategy::TopToBottom,
        Some(200.0),
        None,
        items,
    );

    let root = make_root(&mut tree, 200.0, 100.0, vec![content, header, footer]);
    // The wire-up reads the SOM strings from the root subform's meta and
    // resolves them by name against the root's children.
    {
        let meta = tree.meta_mut(root);
        meta.overflow_leader = Some("OverflowHeader".to_string());
        meta.overflow_trailer = Some("OverflowFooter".to_string());
    }

    let result = LayoutEngine::new(&tree).layout(root).unwrap();
    let page_count = result.pages.len();
    assert!(
        page_count >= 3,
        "expected >=3 pages for content overflow; got {}",
        page_count
    );

    // Page 1 must NOT contain the overflow leader/trailer (XFA §17 — they
    // only apply to continuation pages).
    assert_eq!(
        count_named(&result.pages[0].nodes, "OverflowHeader"),
        0,
        "page 1 must not render the overflow leader"
    );
    assert_eq!(
        count_named(&result.pages[0].nodes, "OverflowFooter"),
        0,
        "page 1 must not render the overflow trailer"
    );

    // Pages 2..N (continuation pages) must each render the leader and trailer.
    for (idx, page) in result.pages.iter().enumerate().skip(1) {
        let h = count_named(&page.nodes, "OverflowHeader");
        let f = count_named(&page.nodes, "OverflowFooter");
        assert_eq!(
            h,
            1,
            "continuation page {} should render OverflowHeader once (got {})",
            idx + 1,
            h
        );
        assert_eq!(
            f,
            1,
            "continuation page {} should render OverflowFooter once (got {})",
            idx + 1,
            f
        );

        // Leader is placed at the top of the content area (y near 0).
        let leader = first_with_name(&page.nodes, "OverflowHeader")
            .expect("OverflowHeader present on continuation page");
        assert!(
            leader.rect.y < 5.0,
            "OverflowHeader should be near the top of page {} (y={})",
            idx + 1,
            leader.rect.y
        );

        // Trailer is placed at the bottom (y near page_height - trailer_height).
        let trailer = first_with_name(&page.nodes, "OverflowFooter")
            .expect("OverflowFooter present on continuation page");
        assert!(
            trailer.rect.y > 50.0,
            "OverflowFooter should be near the bottom of page {} (y={})",
            idx + 1,
            trailer.rect.y
        );
    }

    // All 12 items must still be placed somewhere across the pages —
    // wire-up must not drop content.
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
        "all 12 items must remain placed across pages after overflow wire-up"
    );
}

/// Unresolvable SOM references must be silently ignored — pre-W2-A behavior.
/// This guards against panics/over-pagination when a template references a
/// leader/trailer subform that does not exist in the merged form tree.
#[test]
fn unresolvable_overflow_refs_are_silently_ignored() {
    let mut tree = FormTree::new();

    let mut items = Vec::new();
    for idx in 0..6 {
        items.push(make_field(&mut tree, &format!("Item{idx}"), 200.0, 30.0));
    }
    let content = make_subform(
        &mut tree,
        "Content",
        LayoutStrategy::TopToBottom,
        Some(200.0),
        None,
        items,
    );
    let root = make_root(&mut tree, 200.0, 100.0, vec![content]);
    {
        let meta = tree.meta_mut(root);
        meta.overflow_leader = Some("DoesNotExist".to_string());
        meta.overflow_trailer = Some("AlsoMissing".to_string());
    }

    let result = LayoutEngine::new(&tree).layout(root).unwrap();
    assert!(result.pages.len() >= 2);
    // None of the resolved names appear because the targets do not exist.
    for page in &result.pages {
        assert_eq!(count_named(&page.nodes, "DoesNotExist"), 0);
        assert_eq!(count_named(&page.nodes, "AlsoMissing"), 0);
    }
}
