// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! `<bookend leader trailer>` — the first and the last page (XFA 3.3 §17).
//!
//! XFA has two mechanisms that look alike and serve different pages:
//!
//! * `<overflow>` covers the **continuation** pages. When content does not
//!   fit, the leader appears above every following page.
//! * `<bookend>` covers the **outer** two: the leader on the first page of
//!   this subform, the trailer on the last.
//!
//! Until #150 `bookend` existed nowhere in this codebase. A `<bookend>` in a
//! template was silently ignored: no error, no warning, just a header or
//! footer that never appeared.
//!
//! These tests put the two mechanisms side by side, because the easiest
//! mistake to make is swapping them.

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

/// A form that runs over several pages, with a header and a footer named as
/// the bookend leader and trailer.
fn build(with_leader: bool, with_trailer: bool) -> (FormTree, FormNodeId) {
    let mut tree = FormTree::new();

    let header = make_subform(
        &mut tree,
        "BookHeader",
        LayoutStrategy::TopToBottom,
        Some(200.0),
        Some(10.0),
        vec![],
    );
    let footer = make_subform(
        &mut tree,
        "BookFooter",
        LayoutStrategy::TopToBottom,
        Some(200.0),
        Some(10.0),
        vec![],
    );

    let mut items = Vec::new();
    for idx in 0..12 {
        items.push(make_field(&mut tree, &format!("Item{idx}"), 200.0, 30.0));
    }

    let root = make_root(&mut tree, 200.0, 100.0, {
        let mut children = items;
        children.push(header);
        children.push(footer);
        children
    });

    {
        let meta = tree.meta_mut(root);
        if with_leader {
            meta.bookend_leader = Some("BookHeader".to_string());
        }
        if with_trailer {
            meta.bookend_trailer = Some("BookFooter".to_string());
        }
    }

    (tree, root)
}

/// The leader stands on page 1 and nowhere else.
///
/// This is exactly the difference with `overflow`, where the leader stands
/// *not* on page 1 but on all the following ones. Swap the two and you get a
/// header on every page except the first -- which from a distance looks like
/// something that works.
#[test]
fn the_bookend_leader_stands_only_on_the_first_page() {
    let (tree, root) = build(true, false);
    let result = LayoutEngine::new(&tree).layout(root).unwrap();

    assert!(
        result.pages.len() >= 3,
        "the fixture must run over several pages; got {}",
        result.pages.len()
    );

    assert_eq!(
        count_named(&result.pages[0].nodes, "BookHeader"),
        1,
        "the bookend leader belongs on the first page"
    );

    for (n, page) in result.pages.iter().enumerate().skip(1) {
        assert_eq!(
            count_named(&page.nodes, "BookHeader"),
            0,
            "page {} carries the bookend leader, which belongs on the first only",
            n + 1
        );
    }
}

/// The trailer stands on the last page and nowhere else.
#[test]
fn the_bookend_trailer_stands_only_on_the_last_page() {
    let (tree, root) = build(false, true);
    let result = LayoutEngine::new(&tree).layout(root).unwrap();

    let last = result.pages.len() - 1;
    assert!(last >= 2, "the fixture must run over several pages");

    assert_eq!(
        count_named(&result.pages[last].nodes, "BookFooter"),
        1,
        "the bookend trailer belongs on the last page"
    );

    for (n, page) in result.pages.iter().enumerate().take(last) {
        assert_eq!(
            count_named(&page.nodes, "BookFooter"),
            0,
            "page {} carries the bookend trailer, which belongs on the last only",
            n + 1
        );
    }
}

/// Without `<bookend>` the engine invents no headers or footers.
///
/// The two subforms are then simply content -- they are children of the root
/// and nobody named them as a leader. What must not happen is that they get
/// repeated: that would mean the engine started treating them as leaders
/// without the template asking for it.
#[test]
fn without_a_bookend_the_engine_invents_no_headers() {
    let (tree_without, root_without) = build(false, false);
    let without = LayoutEngine::new(&tree_without)
        .layout(root_without)
        .unwrap();

    assert!(without.pages.len() >= 3);

    let total = |name: &str| -> usize {
        without
            .pages
            .iter()
            .map(|p| count_named(&p.nodes, name))
            .sum()
    };
    assert_eq!(
        total("BookHeader"),
        1,
        "the header was repeated without anything asking for it"
    );
    assert_eq!(
        total("BookFooter"),
        1,
        "the footer was repeated without anything asking for it"
    );
}

/// No content may fall off the last page because the trailer joins it.
///
/// The trailer is placed by laying that page out again. If it does not fit, it
/// would push the form's last line off -- and a document that gets shorter from
/// a footer is worse than a missing footer.
#[test]
fn the_trailer_pushes_no_content_off_the_page() {
    let (tree_without, root_without) = build(false, false);
    let without = LayoutEngine::new(&tree_without)
        .layout(root_without)
        .unwrap();

    let (tree_with, root_with) = build(false, true);
    let with = LayoutEngine::new(&tree_with).layout(root_with).unwrap();

    let fields = |r: &xfa_layout_engine::layout::LayoutDom| -> usize {
        r.pages
            .iter()
            .map(|p| {
                (0..12)
                    .map(|i| count_named(&p.nodes, &format!("Item{i}")))
                    .sum::<usize>()
            })
            .sum()
    };

    assert_eq!(
        fields(&with),
        fields(&without),
        "fewer fields stand in the document as soon as the trailer joins"
    );
}
