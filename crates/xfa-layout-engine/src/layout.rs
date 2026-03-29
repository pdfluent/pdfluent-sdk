//! Layout engine — positions form nodes into layout rectangles.
//!
//! Implements XFA 3.3 §4 (Box Model) and §8 (Layout for Growable Objects).
//! Supports positioned layout and flowed layout (tb, lr-tb, rl-tb).

use crate::error::Result;
use crate::form::{ContentArea, FormNode, FormNodeId, FormNodeType, FormTree};
use crate::text::{self, FontFamily};
use crate::types::{LayoutStrategy, Rect, Size, TextAlign};

/// A unique identifier for a layout node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutNodeId(pub usize);

/// The output of the layout engine: positioned rectangles on pages.
#[derive(Debug)]
pub struct LayoutDom {
    pub pages: Vec<LayoutPage>,
}

/// A single page in the layout output.
#[derive(Debug)]
pub struct LayoutPage {
    pub width: f64,
    pub height: f64,
    pub nodes: Vec<LayoutNode>,
}

/// A positioned element on a page.
#[derive(Debug, Clone)]
pub struct LayoutNode {
    /// The form node this layout node represents.
    pub form_node: FormNodeId,
    /// Bounding rectangle in page coordinates (points).
    pub rect: Rect,
    /// The node's display name (for debugging).
    pub name: String,
    /// Content for leaf nodes.
    pub content: LayoutContent,
    /// Children laid out within this node.
    pub children: Vec<LayoutNode>,
    /// Per-node visual style (colors, borders) from the XFA template.
    pub style: crate::form::FormNodeStyle,
}

/// Content type for layout leaf nodes.
#[derive(Debug, Clone)]
pub enum LayoutContent {
    None,
    Text(String),
    Field {
        value: String,
        field_kind: crate::form::FieldKind,
    },
    /// Pre-wrapped text lines for rendering.
    WrappedText {
        lines: Vec<String>,
        font_size: f64,
        /// Horizontal text alignment (from XFA `<para hAlign>`).
        text_align: TextAlign,
        /// Font family for selecting the correct PDF font resource.
        font_family: FontFamily,
    },
}

/// A content node queued for pagination, carrying page-break flags.
#[derive(Debug, Clone, Copy)]
struct QueuedNode {
    id: FormNodeId,
    break_before: bool,
}

/// The layout engine.
pub struct LayoutEngine<'a> {
    form: &'a FormTree,
}

impl<'a> LayoutEngine<'a> {
    pub fn new(form: &'a FormTree) -> Self {
        Self { form }
    }

    /// Perform layout on the entire form tree starting from the root node.
    ///
    /// Supports multi-page pagination: when content overflows a page's content
    /// area, remaining nodes are placed on subsequent pages. The last page
    /// template is repeated as needed for overflow content.
    pub fn layout(&self, root: FormNodeId) -> Result<LayoutDom> {
        let root_node = self.form.get(root);

        let (page_areas, raw_content_nodes) = self.extract_page_structure(root_node)?;
        // Build queued nodes with break_before flags and occur expansion.
        let content_queued = self.queue_content(&raw_content_nodes);

        let mut pages = Vec::new();

        if page_areas.is_empty() {
            // No explicit page structure — use root's dimensions
            let page_w = root_node.box_model.width.unwrap_or(612.0);
            let page_h = root_node.box_model.height.unwrap_or(792.0);
            let area = ContentArea {
                name: String::new(),
                x: 0.0,
                y: 0.0,
                width: page_w,
                height: page_h,
                leader: None,
                trailer: None,
            };

            if root_node.layout == LayoutStrategy::TopToBottom {
                // TB layout supports pagination: split content across pages
                let mut remaining = content_queued;
                while !remaining.is_empty() {
                    let (page, rest, consumed_break_only) =
                        self.layout_content_fitting(&area, &remaining, page_w, page_h)?;
                    if page.nodes.is_empty() && !consumed_break_only {
                        // Force place one item to prevent infinite loop
                        let forced = self.layout_content_on_page(
                            &area,
                            page_w,
                            page_h,
                            &[remaining[0].id],
                            root_node.layout,
                        )?;
                        pages.push(forced);
                        remaining = remaining[1..].to_vec();
                    } else if consumed_break_only {
                        // Break-only page: skip the blank page, continue with rest
                        remaining = rest;
                    } else {
                        pages.push(page);
                        remaining = rest;
                    }
                }
            } else {
                // Non-TB layouts: place everything on one page (layout_children expands occur)
                let page = self.layout_content_on_page(
                    &area,
                    page_w,
                    page_h,
                    &raw_content_nodes,
                    root_node.layout,
                )?;
                pages.push(page);
            }
        } else {
            // Layout content across page areas, then repeat last template for overflow.
            let mut remaining = content_queued;
            for pa in &page_areas {
                if remaining.is_empty() {
                    break;
                }
                let ca = primary_content_area(pa);
                let (mut placed, rest, consumed_break_only) =
                    self.layout_content_fitting(ca, &remaining, pa.page_width, pa.page_height)?;
                if consumed_break_only {
                    remaining = rest;
                } else if !placed.nodes.is_empty() {
                    self.prepend_fixed_nodes(&pa.fixed_nodes, &mut placed)?;
                    pages.push(placed);
                    remaining = rest;
                } else {
                    remaining = rest;
                }
            }

            // Overflow: repeat page templates until all content is placed.
            if !remaining.is_empty() {
                let last_idx = page_areas.len() - 1;
                while !remaining.is_empty() {
                    let pa_idx = last_idx;
                    let pa = &page_areas[pa_idx];
                    let ca = primary_content_area(pa);

                    let (mut page, rest, consumed_break_only) =
                        self.layout_content_fitting(ca, &remaining, pa.page_width, pa.page_height)?;
                    if page.nodes.is_empty() && !consumed_break_only {
                        let mut forced = self.layout_content_on_page(
                            ca,
                            pa.page_width,
                            pa.page_height,
                            &[remaining[0].id],
                            LayoutStrategy::TopToBottom,
                        )?;
                        self.prepend_fixed_nodes(&pa.fixed_nodes, &mut forced)?;
                        pages.push(forced);
                        remaining = remaining[1..].to_vec();
                    } else if consumed_break_only {
                        remaining = rest;
                    } else {
                        self.prepend_fixed_nodes(&pa.fixed_nodes, &mut page)?;
                        pages.push(page);
                        remaining = rest;
                    }
                }
            }
        }

        Ok(LayoutDom { pages })
    }

    // -------------------------------------------------------------------
    // Helper methods for queued/hidden-aware pagination
    // -------------------------------------------------------------------

    /// Returns true if the node should be completely skipped during layout
    /// (XFA `presence="hidden"` — no layout space consumed, OR the node
    /// targets a specific contentArea via `breakBefore targetType="contentArea"`
    /// and should not participate in the primary content flow).
    /// `presence="invisible"` still takes space, so it is NOT hidden for layout.
    fn is_layout_hidden(&self, id: FormNodeId) -> bool {
        let meta = self.form.meta(id);
        if meta.presence_hidden && !meta.presence_invisible {
            return true;
        }
        // Content-area-targeted nodes (breakBefore targetType="contentArea")
        // are excluded from the primary flow ONLY when they are small
        // decorative elements (< 50pt tall).  Large content subforms that
        // target the primary contentArea should still be laid out.
        if meta.content_area_break {
            let node = self.form.get(id);
            let h = node.box_model.height.unwrap_or(f64::MAX);
            return h < 50.0;
        }
        false
    }

    /// Build a queue of `QueuedNode`s from a list of child IDs, skipping
    /// nodes that are layout-hidden and expanding occur rules.
    fn queue_content(&self, children: &[FormNodeId]) -> Vec<QueuedNode> {
        let expanded = self.expand_occur(children);
        expanded
            .into_iter()
            .filter(|&id| !self.is_layout_hidden(id))
            .map(|id| {
                let meta = self.form.meta(id);
                QueuedNode {
                    id,
                    break_before: meta.page_break_before,
                }
            })
            .collect()
    }

    /// Returns true when a node and its entire subtree produce no visible
    /// content (used for blank-page detection).
    fn subtree_is_blank(&self, id: FormNodeId) -> bool {
        let node = self.form.get(id);
        match &node.node_type {
            FormNodeType::Field { value } => value.is_empty(),
            FormNodeType::Draw { content } => content.is_empty(),
            FormNodeType::Root | FormNodeType::PageSet | FormNodeType::PageArea { .. } => true,
            FormNodeType::Subform => node.children.iter().all(|&c| self.subtree_is_blank(c)),
        }
    }

    /// Whether `current_id` has a keep-with-next constraint relative to
    /// `next_id`, or `next_id` has keep-with-previous.
    #[allow(dead_code)]
    fn keep_links_content(&self, current_id: FormNodeId, next_id: FormNodeId) -> bool {
        let cur_meta = self.form.meta(current_id);
        let nxt_meta = self.form.meta(next_id);
        cur_meta.keep_next_content_area
            || nxt_meta.keep_previous_content_area
            || self.is_spacer_keep_with_next(current_id)
    }

    /// Compute the cumulative height of a keep-chain starting at
    /// `start_idx` within `children`.
    #[allow(dead_code)]
    fn keep_chain_height(&self, children: &[FormNodeId], start_idx: usize, available: Size) -> f64 {
        let mut total = 0.0;
        for i in start_idx..children.len() {
            let sz = self.compute_extent_with_available(children[i], Some(available));
            total += sz.height;
            if i + 1 < children.len() && !self.keep_links_content(children[i], children[i + 1]) {
                break;
            }
        }
        total
    }

    /// Compute the cumulative height of a keep-chain starting at
    /// `start_idx` within a queued-node slice.
    #[allow(dead_code)]
    fn queued_keep_chain_height(
        &self,
        children: &[QueuedNode],
        start_idx: usize,
        available: Size,
    ) -> f64 {
        let mut total = 0.0;
        for i in start_idx..children.len() {
            let sz = self.compute_extent_with_available(children[i].id, Some(available));
            total += sz.height;
            if i + 1 < children.len()
                && !self.keep_links_content(children[i].id, children[i + 1].id)
            {
                break;
            }
        }
        total
    }

    /// Returns true when the node is a blank spacer with a keep-intact
    /// constraint (used as a "glue" between siblings).
    fn is_spacer_keep_with_next(&self, id: FormNodeId) -> bool {
        let meta = self.form.meta(id);
        meta.keep_intact_content_area && self.subtree_is_blank(id)
    }

    /// Compute the cumulative height of a keep-chain starting at index
    /// `start_idx` within a filtered visible-ids list. The chain extends
    /// as long as consecutive nodes have `keep_next_content_area` or the
    /// next node has `keep_previous_content_area`.
    fn visible_keep_chain_height(
        &self,
        visible_ids: &[(usize, FormNodeId)],
        start_idx: usize,
        available: Size,
    ) -> f64 {
        let mut total = 0.0;
        for i in start_idx..visible_ids.len() {
            let (_, id) = visible_ids[i];
            let sz = self.compute_extent_with_available(id, Some(available));
            total += sz.height;
            // Check if the chain continues to the next node.
            if i + 1 < visible_ids.len() {
                let (_, next_id) = visible_ids[i + 1];
                let cur_meta = self.form.meta(id);
                let nxt_meta = self.form.meta(next_id);
                let keep = cur_meta.keep_next_content_area
                    || nxt_meta.keep_previous_content_area
                    || (cur_meta.keep_intact_content_area && self.subtree_is_blank(id));
                if !keep {
                    break;
                }
            }
        }
        total
    }

    fn extract_page_structure(
        &self,
        root: &FormNode,
    ) -> Result<(Vec<PageAreaInfo>, Vec<FormNodeId>)> {
        let mut page_areas = Vec::new();
        let mut content_nodes = Vec::new();

        for &child_id in &root.children {
            let child = self.form.get(child_id);
            match &child.node_type {
                FormNodeType::PageSet => {
                    for &pa_id in &child.children {
                        let pa_node = self.form.get(pa_id);
                        if let FormNodeType::PageArea { content_areas } = &pa_node.node_type {
                            let pa_meta = self.form.meta(pa_id);
                            let fixed: Vec<FormNodeId> = pa_node
                                .children
                                .iter()
                                .copied()
                                .filter(|&cid| {
                                    matches!(
                                        self.form.get(cid).node_type,
                                        FormNodeType::Draw { .. } | FormNodeType::Subform
                                    )
                                })
                                .collect();
                            page_areas.push(PageAreaInfo {
                                name: pa_node.name.clone(),
                                xfa_id: pa_meta.xfa_id.clone(),
                                content_areas: content_areas.clone(),
                                page_width: pa_node.box_model.width.unwrap_or(612.0),
                                page_height: pa_node.box_model.height.unwrap_or(792.0),
                                fixed_nodes: fixed,
                            });
                        }
                    }
                }
                FormNodeType::PageArea { content_areas } => {
                    let pa_meta = self.form.meta(child_id);
                    let fixed: Vec<FormNodeId> = child
                        .children
                        .iter()
                        .copied()
                        .filter(|&cid| {
                            matches!(
                                self.form.get(cid).node_type,
                                FormNodeType::Draw { .. } | FormNodeType::Subform
                            )
                        })
                        .collect();
                    page_areas.push(PageAreaInfo {
                        name: child.name.clone(),
                        xfa_id: pa_meta.xfa_id.clone(),
                        content_areas: content_areas.clone(),
                        page_width: child.box_model.width.unwrap_or(612.0),
                        page_height: child.box_model.height.unwrap_or(792.0),
                        fixed_nodes: fixed,
                    });
                }
                // XFA's canonical nesting: <subform layout="paginate"> wraps
                // <pageSet> and content siblings.  Look one level deeper so the
                // inner pageSet defines the page geometry and the remaining
                // children become the content nodes.  If no pageSet is found
                // inside, fall through and treat the subform as content.
                // (Fixes xl_02_row_layout.pdf and xl_09_field_types.pdf blank
                // render: the pageSet's 792pt height was consuming the full page
                // and pushing form content to page 2.)
                FormNodeType::Subform => {
                    // Only recurse into subforms that directly contain a
                    // PageSet child.  Blind recursion into content subforms
                    // (especially Positioned ones) would shatter their
                    // internal structure and lose breakBefore/overflow
                    // semantics.
                    let has_pageset = child
                        .children
                        .iter()
                        .any(|&cid| matches!(self.form.get(cid).node_type, FormNodeType::PageSet));
                    if has_pageset {
                        let (inner_areas, inner_content) = self.extract_page_structure(child)?;
                        page_areas.extend(inner_areas);
                        content_nodes.extend(inner_content);
                    } else if child.layout == LayoutStrategy::TopToBottom {
                        // TB subform without inner PageSet — still recurse in
                        // case an inner TB child wraps a PageSet.
                        let (inner_areas, inner_content) = self.extract_page_structure(child)?;
                        if !inner_areas.is_empty() {
                            page_areas.extend(inner_areas);
                            content_nodes.extend(inner_content);
                        } else {
                            content_nodes.push(child_id);
                        }
                    } else {
                        content_nodes.push(child_id);
                    }
                }
                _ => {
                    content_nodes.push(child_id);
                }
            }
        }

        Ok((page_areas, content_nodes))
    }

    fn layout_content_on_page(
        &self,
        content_area: &ContentArea,
        page_width: f64,
        page_height: f64,
        content_ids: &[FormNodeId],
        strategy: LayoutStrategy,
    ) -> Result<LayoutPage> {
        let mut page = LayoutPage {
            width: page_width,
            height: page_height,
            nodes: Vec::new(),
        };

        let available = Size {
            width: content_area.width,
            height: content_area.height,
        };

        let nodes = self.layout_children(content_ids, available, strategy)?;

        // Offset nodes to content area position
        for mut node in nodes {
            node.rect.x += content_area.x;
            node.rect.y += content_area.y;
            page.nodes.push(node);
        }

        Ok(page)
    }

    /// Lay out page-area fixed nodes (headers, footers, lines) at their
    /// absolute positions and prepend them to the page's node list so they
    /// render behind flowing content.
    fn prepend_fixed_nodes(&self, fixed_ids: &[FormNodeId], page: &mut LayoutPage) -> Result<()> {
        if fixed_ids.is_empty() {
            return Ok(());
        }
        let fixed_laid = self.layout_positioned(fixed_ids)?;
        let mut merged = fixed_laid;
        merged.append(&mut page.nodes);
        page.nodes = merged;
        Ok(())
    }

    fn layout_content_fitting(
        &self,
        content_area: &ContentArea,
        content_ids: &[QueuedNode],
        page_width: f64,
        page_height: f64,
    ) -> Result<(LayoutPage, Vec<QueuedNode>, bool)> {
        let mut page = LayoutPage {
            width: page_width,
            height: page_height,
            nodes: Vec::new(),
        };

        // Compute leader/trailer heights and place them first
        let mut leader_height = 0.0;
        let mut trailer_height = 0.0;

        if let Some(leader_id) = content_area.leader {
            let leader_size = self.compute_extent(leader_id);
            leader_height = leader_size.height;
            let leader_node = self.form.get(leader_id);
            let node = self.layout_single_node(leader_id, leader_node, 0.0, 0.0)?;
            let mut offset = node;
            offset.rect.x += content_area.x;
            offset.rect.y += content_area.y;
            page.nodes.push(offset);
        }

        if let Some(trailer_id) = content_area.trailer {
            let trailer_size = self.compute_extent(trailer_id);
            trailer_height = trailer_size.height;
            // Trailer is placed at the bottom of the content area
            let trailer_y = content_area.height - trailer_height;
            let trailer_node = self.form.get(trailer_id);
            let node = self.layout_single_node(trailer_id, trailer_node, 0.0, trailer_y)?;
            let mut offset = node;
            offset.rect.x += content_area.x;
            offset.rect.y += content_area.y;
            page.nodes.push(offset);
        }

        // Available height for content = total - leader - trailer
        let content_height = content_area.height - leader_height - trailer_height;
        let available = Size {
            width: content_area.width,
            height: content_height,
        };

        let mut y_cursor = leader_height;
        let mut placed_count = 0;
        let mut split_remaining: Vec<QueuedNode> = Vec::new();
        let content_bottom = leader_height + content_height;
        let mut consumed_break_only = false;

        // Count leader/trailer nodes placed so far (for force-place detection).
        let header_node_count = (if content_area.leader.is_some() { 1 } else { 0 })
            + (if content_area.trailer.is_some() { 1 } else { 0 });

        // Pre-compute a visible-node list for keep-chain look-ahead.
        // Each entry is (index-into-content_ids, FormNodeId).
        let visible_ids: Vec<(usize, FormNodeId)> = content_ids
            .iter()
            .enumerate()
            .filter(|(_, qn)| !self.is_layout_hidden(qn.id))
            .map(|(i, qn)| (i, qn.id))
            .collect();
        let mut vis_pos = 0; // current position in visible_ids

        for (idx, qn) in content_ids.iter().enumerate() {
            let child_id = qn.id;

            // Skip layout-hidden nodes — they consume no space.
            if self.is_layout_hidden(child_id) {
                placed_count += 1;
                continue;
            }

            // Handle break_before: if this node requests a page break and
            // we already placed content on this page, stop here so the
            // caller starts a new page with this node.
            if qn.break_before && placed_count > 0 {
                // Check if all placed content is blank spacers — if so,
                // fold the blank page: mark as consumed_break_only so the
                // caller skips the empty page.
                let only_blanks = page.nodes.len() <= header_node_count;
                if only_blanks {
                    consumed_break_only = true;
                }
                break;
            }

            let child = self.form.get(child_id);
            let child_size = self.compute_extent_with_available(child_id, Some(available));

            // Keep-chain look-ahead: if this node starts a keep chain and
            // the chain doesn't fit in remaining space (but WOULD fit on a
            // fresh page), break now so the chain starts on the next page.
            if placed_count > 0 && vis_pos < visible_ids.len() {
                let chain_height = self.visible_keep_chain_height(&visible_ids, vis_pos, available);
                let remaining_on_page = content_bottom - y_cursor;
                if chain_height > remaining_on_page && chain_height <= content_height {
                    // Chain fits on a fresh page — break now.
                    break;
                }
                // Unsatisfiable keep chain (exceeds page height):
                // fall through and use child's own height for placement.
            }

            if y_cursor + child_size.height > content_bottom {
                let remaining_height = content_bottom - y_cursor;

                // Try to split this node if it's a splittable tb-layout container.
                if remaining_height > 0.0 && self.can_split(child_id) {
                    let (partial, rest_children) =
                        self.split_tb_node(child_id, y_cursor, remaining_height, available)?;
                    // Place the partial if it fits, OR if the split was
                    // productive (multiple children placed — the overflow
                    // is tolerable).  Only defer when the partial is a
                    // single oversized child that didn't fit — re-splitting
                    // on a fresh page with full height will do better.
                    let partial_fits = partial.rect.height <= remaining_height + 1.0;
                    let split_productive = partial.children.len() > 1;
                    if !partial.children.is_empty() && (partial_fits || split_productive) {
                        let mut offset_node = partial;
                        offset_node.rect.x += content_area.x;
                        offset_node.rect.y += content_area.y;
                        page.nodes.push(offset_node);
                        placed_count += 1;
                        // After a split, rest children go to a fresh page.
                        // Clear break_before on ALL — the overflow mechanism
                        // handles page transitions correctly; keeping break
                        // flags causes premature splits that waste space.
                        split_remaining = rest_children
                            .into_iter()
                            .map(|cid| QueuedNode {
                                id: cid,
                                break_before: false,
                            })
                            .collect();
                    } else if placed_count > 0 {
                        // Single oversized child doesn't fit and page has
                        // content — defer to a fresh page where re-split
                        // with full height can do better.
                        break;
                    }
                } else if idx == 0 || page.nodes.len() <= header_node_count {
                    // First content item too large and can't split — force place it
                    let node = self.layout_single_node_with_extent(
                        child_id, child, 0.0, y_cursor, child_size,
                    )?;
                    let mut offset_node = node;
                    offset_node.rect.x += content_area.x;
                    offset_node.rect.y += content_area.y;
                    page.nodes.push(offset_node);
                    placed_count += 1;
                }
                break;
            }

            // Proactive split: if the child fits on the page but contains
            // inner page-break-before children, split it using the FULL
            // content height so the inner break is detected.
            if self.has_inner_break(child_id) && self.can_split(child_id) {
                let (partial, rest_children) =
                    self.split_tb_node(child_id, y_cursor, content_height, available)?;
                let remaining_on_page = content_bottom - y_cursor;
                if !partial.children.is_empty() && partial.rect.height <= remaining_on_page {
                    let mut offset_node = partial;
                    offset_node.rect.x += content_area.x;
                    offset_node.rect.y += content_area.y;
                    page.nodes.push(offset_node);
                    placed_count += 1;
                    split_remaining = rest_children
                        .into_iter()
                        .map(|cid| {
                            let m = self.form.meta(cid);
                            QueuedNode {
                                id: cid,
                                break_before: m.page_break_before,
                            }
                        })
                        .collect();
                } else if placed_count > 0 {
                    break;
                } else {
                    if !partial.children.is_empty() {
                        let mut offset_node = partial;
                        offset_node.rect.x += content_area.x;
                        offset_node.rect.y += content_area.y;
                        page.nodes.push(offset_node);
                    }
                    placed_count += 1;
                    split_remaining = rest_children
                        .into_iter()
                        .map(|cid| {
                            let m = self.form.meta(cid);
                            QueuedNode {
                                id: cid,
                                break_before: m.page_break_before,
                            }
                        })
                        .collect();
                }
                break;
            }

            let node =
                self.layout_single_node_with_extent(child_id, child, 0.0, y_cursor, child_size)?;
            let mut offset_node = node;
            offset_node.rect.x += content_area.x;
            offset_node.rect.y += content_area.y;
            page.nodes.push(offset_node);

            y_cursor += child_size.height;
            placed_count += 1;
            vis_pos += 1;
        }

        let mut remaining = split_remaining;
        remaining.extend(content_ids[placed_count..].iter().copied());
        Ok((page, remaining, consumed_break_only))
    }

    /// Check if a node can be split across pages.
    ///
    /// Only tb-layout subforms with children can be split.
    fn can_split(&self, id: FormNodeId) -> bool {
        let node = self.form.get(id);
        node.layout == LayoutStrategy::TopToBottom && !node.children.is_empty()
    }

    /// Check if any direct (expanded) child of a tb-layout subform has
    /// `page_break_before` set, meaning the node must be split at that
    /// point even if it fits in the remaining space.
    fn has_inner_break(&self, id: FormNodeId) -> bool {
        let node = self.form.get(id);
        if node.layout != LayoutStrategy::TopToBottom {
            return false;
        }
        let expanded = self.expand_occur(&node.children);
        expanded
            .iter()
            .any(|&cid| self.form.meta(cid).page_break_before)
    }

    /// Split a tb-layout node: place children that fit in `remaining_height`,
    /// return a partial layout node and the remaining child IDs.
    ///
    /// Respects keep constraints: if a child has `keep_next_content_area`,
    /// the split will not occur between that child and its successor.
    /// Also respects `page_break_before` on children as mandatory split points.
    fn split_tb_node(
        &self,
        id: FormNodeId,
        y_offset: f64,
        remaining_height: f64,
        available: Size,
    ) -> Result<(LayoutNode, Vec<FormNodeId>)> {
        let node = self.form.get(id);
        let expanded_children = self.expand_occur(&node.children);

        let mut placed_children = Vec::new();
        let mut child_y = 0.0;
        let mut split_idx = 0;
        // Track the last valid split point (respecting keep constraints).
        let mut last_valid_split = 0;
        let mut last_valid_y = 0.0_f64;

        for (i, &child_id) in expanded_children.iter().enumerate() {
            let child = self.form.get(child_id);
            let child_size = self.compute_extent(child_id);
            let child_meta = self.form.meta(child_id);

            // NOTE: page_break_before inside split_tb_node is intentionally
            // NOT used as a mandatory split point.  The overflow check below
            // already splits when content exceeds the page; honouring breaks
            // in addition creates premature splits that waste space.  The
            // break_before flag is respected at the TOP level in
            // layout_content_fitting (line ~554) where it triggers a clean
            // page transition between queued content items.

            // If this child has keep_intact and doesn't fit, split BEFORE it
            // so it moves to the next page entirely.
            if child_meta.keep_intact_content_area
                && child_y + child_size.height > remaining_height
                && !placed_children.is_empty()
            {
                split_idx = i;
                break;
            }

            // Overflow detection: child doesn't fit in remaining space.
            if child_y + child_size.height > remaining_height && !placed_children.is_empty() {
                // Overflow: split at the last valid split point.
                if last_valid_split > 0 && last_valid_split < placed_children.len() {
                    // Trim placed_children to the last valid split point.
                    placed_children.truncate(last_valid_split);
                    child_y = last_valid_y;
                    split_idx = last_valid_split;
                } else {
                    split_idx = i;
                }
                break;
            }

            // When child_y > 0 (content already placed) and the child overflows,
            // we already handled that above. Now also try splitting even if
            // fits_on_fresh_page when there's already content.
            // (This is handled by the overflow check above being >= not >.)

            let child_node = self.layout_single_node(child_id, child, 0.0, child_y)?;
            placed_children.push(child_node);
            child_y += child_size.height;
            split_idx = i + 1;

            // Check if this is a valid split point (no keep constraint).
            let has_keep = child_meta.keep_next_content_area;
            let next_has_keep_prev = if i + 1 < expanded_children.len() {
                self.form
                    .meta(expanded_children[i + 1])
                    .keep_previous_content_area
            } else {
                false
            };
            if !has_keep && !next_has_keep_prev {
                last_valid_split = split_idx;
                last_valid_y = child_y;
            }
        }

        let content = match &node.node_type {
            FormNodeType::Field { value } => LayoutContent::Field {
                value: value.clone(),
                field_kind: self.form.meta(id).field_kind,
            },
            FormNodeType::Draw { content } => LayoutContent::Text(content.clone()),
            _ => LayoutContent::None,
        };

        // Compute partial extent: full width, height = content that fit
        let partial_width = self
            .compute_extent_with_available(id, Some(available))
            .width;

        let partial_node = LayoutNode {
            form_node: id,
            rect: Rect::new(0.0, y_offset, partial_width, child_y),
            name: node.name.clone(),
            content,
            children: placed_children,
            style: self.form.meta(id).style.clone(),
        };

        let rest = expanded_children[split_idx..].to_vec();
        Ok((partial_node, rest))
    }

    /// Layout children within available space using the given strategy.
    ///
    /// Children with `occur.count() > 1` are expanded into multiple instances.
    fn layout_children(
        &self,
        children: &[FormNodeId],
        available: Size,
        strategy: LayoutStrategy,
    ) -> Result<Vec<LayoutNode>> {
        let expanded = self.expand_occur(children);
        match strategy {
            LayoutStrategy::Positioned => self.layout_positioned(&expanded),
            LayoutStrategy::TopToBottom => self.layout_tb(&expanded, available),
            LayoutStrategy::LeftToRightTB => self.layout_lr_tb(&expanded, available),
            LayoutStrategy::RightToLeftTB => self.layout_rl_tb(&expanded, available),
            LayoutStrategy::Table => self.layout_table(&expanded, available),
            LayoutStrategy::Row => self.layout_row(&expanded, available),
        }
    }

    /// Expand children based on occur rules.
    ///
    /// A child with `occur.count() == 3` produces three entries in the output.
    /// Each entry refers to the same FormNodeId (the template), which the layout
    /// engine treats as separate instances at different positions.
    ///
    /// Nodes with `presence="hidden"` (not `"invisible"`) are skipped entirely
    /// because they consume no layout space (XFA 3.3 §3.2.8).
    fn expand_occur(&self, children: &[FormNodeId]) -> Vec<FormNodeId> {
        let mut expanded = Vec::new();
        for &child_id in children {
            // Skip layout-hidden nodes — they occupy no space (XFA 3.3 §3.2.8).
            if self.is_layout_hidden(child_id) {
                continue;
            }
            let child = self.form.get(child_id);
            let count = child.occur.count();
            for _ in 0..count {
                expanded.push(child_id);
            }
        }
        expanded
    }

    /// Positioned layout: each child uses its own x,y from the box model.
    fn layout_positioned(&self, children: &[FormNodeId]) -> Result<Vec<LayoutNode>> {
        let mut nodes = Vec::new();
        for &child_id in children {
            let child = self.form.get(child_id);
            let node =
                self.layout_single_node(child_id, child, child.box_model.x, child.box_model.y)?;
            nodes.push(node);
        }
        Ok(nodes)
    }

    /// Top-to-bottom flow layout.
    fn layout_tb(&self, children: &[FormNodeId], available: Size) -> Result<Vec<LayoutNode>> {
        let mut nodes = Vec::new();
        let mut y_cursor = 0.0;

        for &child_id in children {
            let child = self.form.get(child_id);
            let child_size = self.compute_extent_with_available(child_id, Some(available));

            let node =
                self.layout_single_node_with_extent(child_id, child, 0.0, y_cursor, child_size)?;
            nodes.push(node);

            y_cursor += child_size.height;

            // If overflow, we just continue (pagination will handle splitting)
            if y_cursor > available.height {
                // In a full implementation, this would trigger pagination
            }
        }
        Ok(nodes)
    }

    /// Left-to-right, top-to-bottom wrapping layout.
    fn layout_lr_tb(&self, children: &[FormNodeId], available: Size) -> Result<Vec<LayoutNode>> {
        let mut nodes = Vec::new();
        let mut x_cursor = 0.0;
        let mut y_cursor = 0.0;
        let mut row_height = 0.0_f64;

        for &child_id in children {
            let child = self.form.get(child_id);
            let child_size = self.compute_extent(child_id);

            // Wrap to next row if doesn't fit horizontally
            if x_cursor + child_size.width > available.width && x_cursor > 0.0 {
                y_cursor += row_height;
                x_cursor = 0.0;
                row_height = 0.0;
            }

            let node = self.layout_single_node(child_id, child, x_cursor, y_cursor)?;
            nodes.push(node);

            x_cursor += child_size.width;
            row_height = row_height.max(child_size.height);
        }
        Ok(nodes)
    }

    /// Right-to-left, top-to-bottom wrapping layout.
    fn layout_rl_tb(&self, children: &[FormNodeId], available: Size) -> Result<Vec<LayoutNode>> {
        let mut nodes = Vec::new();
        let mut x_cursor = available.width;
        let mut y_cursor = 0.0;
        let mut row_height = 0.0_f64;

        for &child_id in children {
            let child = self.form.get(child_id);
            let child_size = self.compute_extent(child_id);

            // Wrap to next row if doesn't fit
            if x_cursor - child_size.width < 0.0 && x_cursor < available.width {
                y_cursor += row_height;
                x_cursor = available.width;
                row_height = 0.0;
            }

            x_cursor -= child_size.width;
            let node = self.layout_single_node(child_id, child, x_cursor, y_cursor)?;
            nodes.push(node);

            row_height = row_height.max(child_size.height);
        }
        Ok(nodes)
    }

    /// Table layout: resolve column widths from the table node, then delegate
    /// to `layout_table_rows`. This stub is still called from `layout_children`
    /// but the real table path is intercepted in `layout_single_node_with_extent`
    /// where we have access to the parent FormNode's `column_widths`.
    fn layout_table(&self, children: &[FormNodeId], available: Size) -> Result<Vec<LayoutNode>> {
        // Fallback: equal-width columns based on max cell count
        let max_cells = children
            .iter()
            .map(|&row_id| self.form.get(row_id).children.len())
            .max()
            .unwrap_or(0);
        if max_cells == 0 {
            return Ok(Vec::new());
        }
        let col_width = available.width / max_cells as f64;
        let col_widths: Vec<f64> = vec![col_width; max_cells];
        self.layout_table_rows(children, available, &col_widths)
    }

    /// Layout table rows: stack rows vertically, distributing cells across
    /// resolved column widths with row height equalization.
    fn layout_table_rows(
        &self,
        children: &[FormNodeId],
        available: Size,
        col_widths: &[f64],
    ) -> Result<Vec<LayoutNode>> {
        let expanded = self.expand_occur(children);
        let mut nodes = Vec::new();
        let mut y_cursor = 0.0;

        for &row_id in &expanded {
            let row_node = self.form.get(row_id);
            let row_children = self.expand_occur(&row_node.children);

            // Layout cells within this row using column widths
            let mut cells = Vec::new();
            let mut x_cursor = 0.0;
            let mut col_idx = 0usize;
            let mut max_cell_height = 0.0_f64;

            for &cell_id in &row_children {
                if col_idx >= col_widths.len() {
                    break;
                }
                let cell = self.form.get(cell_id);
                let span = cell.col_span;

                // Calculate cell width from column widths
                let cell_width = if span == -1 {
                    // Span remaining columns
                    col_widths[col_idx..].iter().sum::<f64>()
                } else {
                    let span_count =
                        (span.max(1) as usize).min(col_widths.len().saturating_sub(col_idx));
                    col_widths[col_idx..col_idx + span_count]
                        .iter()
                        .sum::<f64>()
                };

                // Layout cell with forced width
                let cell_available = Size {
                    width: cell_width,
                    height: available.height - y_cursor,
                };
                let cell_height = self
                    .compute_extent_with_available(cell_id, Some(cell_available))
                    .height;
                let cell_extent = Size {
                    width: cell_width,
                    height: cell_height,
                };

                let cell_node =
                    self.layout_single_node_with_extent(cell_id, cell, x_cursor, 0.0, cell_extent)?;
                max_cell_height = max_cell_height.max(cell_extent.height);
                cells.push(cell_node);

                x_cursor += cell_width;
                if span == -1 {
                    col_idx = col_widths.len();
                } else {
                    col_idx += span.max(1) as usize;
                }
            }

            // Equalize row height: all cells expand to tallest
            for cell in &mut cells {
                cell.rect.height = max_cell_height;
            }

            // Create row layout node
            let row_layout = LayoutNode {
                form_node: row_id,
                rect: Rect::new(0.0, y_cursor, available.width, max_cell_height),
                name: row_node.name.clone(),
                content: LayoutContent::None,
                children: cells,
                style: self.form.meta(row_id).style.clone(),
            };
            nodes.push(row_layout);

            y_cursor += max_cell_height;
        }
        Ok(nodes)
    }

    /// Resolve column widths for a table subform.
    ///
    /// Specified widths >= 0 are used as-is (points). Widths of -1 auto-size
    /// to the widest single-span cell in that column across all rows.
    fn resolve_column_widths(&self, table_node: &FormNode, available_width: f64) -> Vec<f64> {
        let specified = &table_node.column_widths;

        // Determine number of columns
        let max_cols_from_rows = table_node
            .children
            .iter()
            .map(|&row_id| {
                let row = self.form.get(row_id);
                row.children
                    .iter()
                    .map(|&cell_id| {
                        let cell = self.form.get(cell_id);
                        cell.col_span.max(1) as usize
                    })
                    .sum::<usize>()
            })
            .max()
            .unwrap_or(0);

        let num_cols = specified.len().max(max_cols_from_rows);
        if num_cols == 0 {
            return vec![];
        }

        let mut widths = Vec::with_capacity(num_cols);
        for i in 0..num_cols {
            let spec_value = specified.get(i).copied().unwrap_or(-1.0);
            if spec_value >= 0.0 {
                widths.push(spec_value);
            } else {
                // Auto-size: find widest single-span cell in this column
                let mut max_w = 0.0_f64;
                for &row_id in &table_node.children {
                    let row = self.form.get(row_id);
                    let mut col_idx = 0usize;
                    for &cell_id in &row.children {
                        let cell = self.form.get(cell_id);
                        let span = cell.col_span;
                        if col_idx == i && span == 1 {
                            let cell_extent = self.compute_extent(cell_id);
                            max_w = max_w.max(cell_extent.width);
                        }
                        col_idx += span.max(1) as usize;
                    }
                }
                widths.push(max_w);
            }
        }

        // Scale down proportionally if total exceeds available width
        let total: f64 = widths.iter().sum();
        if total > available_width && total > 0.0 {
            let scale = available_width / total;
            for w in &mut widths {
                *w *= scale;
            }
        }

        widths
    }

    /// Row layout: children fill horizontally within the row.
    /// Used for standalone Row-layout subforms (not inside a table context).
    fn layout_row(&self, children: &[FormNodeId], available: Size) -> Result<Vec<LayoutNode>> {
        let mut nodes = Vec::new();
        let mut x_cursor = 0.0;

        for &child_id in children {
            let child = self.form.get(child_id);
            let child_size = self.compute_extent(child_id);

            let node = self.layout_single_node(child_id, child, x_cursor, 0.0)?;
            nodes.push(node);

            x_cursor += child_size.width;

            if x_cursor > available.width {
                break;
            }
        }
        Ok(nodes)
    }

    /// Layout a single node: compute its rect and recursively layout children.
    fn layout_single_node(
        &self,
        id: FormNodeId,
        node: &FormNode,
        x: f64,
        y: f64,
    ) -> Result<LayoutNode> {
        let extent = self.compute_extent(id);
        self.layout_single_node_with_extent(id, node, x, y, extent)
    }

    /// Layout a single node with a pre-computed extent.
    fn layout_single_node_with_extent(
        &self,
        id: FormNodeId,
        node: &FormNode,
        x: f64,
        y: f64,
        extent: Size,
    ) -> Result<LayoutNode> {
        let content = match &node.node_type {
            FormNodeType::Field { value } => {
                if !value.is_empty() && node.children.is_empty() {
                    let insets_w =
                        node.box_model.margins.horizontal() + node.box_model.border_width * 2.0;
                    let max_w = (extent.width - insets_w).max(0.0);
                    let wrapped = text::wrap_text(value, max_w, &node.font);
                    LayoutContent::WrappedText {
                        lines: wrapped.lines,
                        font_size: node.font.size,
                        text_align: node.font.text_align,
                        font_family: node.font.typeface,
                    }
                } else {
                    LayoutContent::Field {
                        value: value.clone(),
                        field_kind: self.form.meta(id).field_kind,
                    }
                }
            }
            FormNodeType::Draw { content } => {
                if !content.is_empty() && node.children.is_empty() {
                    let insets_w =
                        node.box_model.margins.horizontal() + node.box_model.border_width * 2.0;
                    let max_w = (extent.width - insets_w).max(0.0);
                    let wrapped = text::wrap_text(content, max_w, &node.font);
                    LayoutContent::WrappedText {
                        lines: wrapped.lines,
                        font_size: node.font.size,
                        text_align: node.font.text_align,
                        font_family: node.font.typeface,
                    }
                } else {
                    LayoutContent::Text(content.clone())
                }
            }
            _ => LayoutContent::None,
        };

        let child_available = Size {
            width: node.box_model.content_width().min(extent.width),
            height: node.box_model.content_height().min(extent.height),
        };

        let children = if node.children.is_empty() {
            Vec::new()
        } else if node.layout == LayoutStrategy::Table {
            // Table layout: resolve column widths from the parent node,
            // then distribute cells across rows.
            let col_widths = self.resolve_column_widths(node, child_available.width);
            self.layout_table_rows(&node.children, child_available, &col_widths)?
        } else {
            self.layout_children(&node.children, child_available, node.layout)?
        };

        Ok(LayoutNode {
            form_node: id,
            rect: Rect::new(x, y, extent.width, extent.height),
            name: node.name.clone(),
            content,
            children,
            style: self.form.meta(id).style.clone(),
        })
    }

    /// Compute the outer extent (total bounding box) of a form node.
    ///
    /// When `available` is provided, growable dimensions may expand to fill
    /// the available space (XFA §8: growable objects fill the parent container).
    pub fn compute_extent(&self, id: FormNodeId) -> Size {
        self.compute_extent_with_available(id, None)
    }

    /// Compute extent with optional available-space constraint.
    ///
    /// For growable dimensions (width/height = None), the element sizes to fit
    /// its content. When `available` is given, a growable dimension expands to
    /// at least the available space (but content can make it larger, subject to
    /// max constraints).
    fn compute_extent_with_available(&self, id: FormNodeId, available: Option<Size>) -> Size {
        let node = self.form.get(id);
        let bm = &node.box_model;

        // If explicit size is set, use it
        if let (Some(w), Some(h)) = (bm.width, bm.height) {
            return Size {
                width: w,
                height: h,
            };
        }

        // For growable dimensions, compute from children or text content
        let mut content_size = Size::default();

        if !node.children.is_empty() {
            let expanded = self.expand_occur(&node.children);
            match node.layout {
                LayoutStrategy::TopToBottom => {
                    for &child_id in &expanded {
                        let cs = self.compute_extent(child_id);
                        content_size.width = content_size.width.max(cs.width);
                        content_size.height += cs.height;
                    }
                }
                LayoutStrategy::LeftToRightTB | LayoutStrategy::Row => {
                    for &child_id in &expanded {
                        let cs = self.compute_extent(child_id);
                        content_size.width += cs.width;
                        content_size.height = content_size.height.max(cs.height);
                    }
                }
                LayoutStrategy::Table => {
                    // Table width = sum of resolved column widths
                    let avail_w = available.map(|a| a.width).unwrap_or(f64::MAX);
                    let col_widths = self.resolve_column_widths(node, avail_w);
                    let table_width: f64 = col_widths.iter().sum();
                    content_size.width = content_size.width.max(table_width);
                    // Table height = sum of row heights
                    for &row_id in &expanded {
                        let row_extent = self.compute_extent(row_id);
                        content_size.height += row_extent.height;
                    }
                }
                _ => {
                    // Positioned: envelope all children (occur doesn't stack in positioned)
                    for &child_id in &node.children {
                        let child = self.form.get(child_id);
                        let cs = self.compute_extent(child_id);
                        content_size.width = content_size.width.max(child.box_model.x + cs.width);
                        content_size.height =
                            content_size.height.max(child.box_model.y + cs.height);
                    }
                }
            }
        } else {
            // Leaf node: measure text content for Draw/Field nodes
            let text_content = match &node.node_type {
                FormNodeType::Draw { content } => Some(content.as_str()),
                FormNodeType::Field { value } => Some(value.as_str()),
                _ => None,
            };

            if let Some(txt) = text_content {
                if !txt.is_empty() {
                    let insets_w = bm.margins.horizontal() + bm.border_width * 2.0;
                    // If width is fixed, wrap text within that width minus insets
                    // If width is growable, measure without wrapping
                    let text_size = if let Some(w) = bm.width {
                        let max_text_width = (w - insets_w).max(0.0);
                        text::wrap_text(txt, max_text_width, &node.font).size
                    } else if let Some(avail) = available {
                        let max_text_width = (avail.width - insets_w).max(0.0);
                        text::wrap_text(txt, max_text_width, &node.font).size
                    } else {
                        text::measure_text(txt, &node.font)
                    };
                    content_size.width = content_size.width.max(text_size.width);
                    content_size.height = content_size.height.max(text_size.height);
                }
            }
        }

        // When available space is given, growable dims expand to fill it
        if let Some(avail) = available {
            if bm.width.is_none() {
                let insets_w = bm.margins.horizontal() + bm.border_width * 2.0;
                content_size.width = content_size.width.max(avail.width - insets_w);
            }
        }

        bm.outer_size(content_size)
    }
}

/// Find a page area by break target string. The target can match the page
/// area's `name`, its `xfa_id`, or a `pageArea[N]` index reference.
#[allow(dead_code)]
fn find_page_area_by_target(page_areas: &[PageAreaInfo], target: &str) -> Option<usize> {
    // Try matching by name first (e.g. "MP3").
    if let Some(idx) = page_areas.iter().position(|pa| pa.name == target) {
        return Some(idx);
    }
    // Try matching by xfa_id (e.g. "Page4_ID").
    if let Some(idx) = page_areas
        .iter()
        .position(|pa| pa.xfa_id.as_deref() == Some(target))
    {
        return Some(idx);
    }
    // Try matching "pageArea[N]" index reference.
    if let Some(rest) = target.strip_prefix("pageArea") {
        if let Some(idx_str) = rest.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
            if let Ok(idx) = idx_str.parse::<usize>() {
                if idx < page_areas.len() {
                    return Some(idx);
                }
            }
        }
        // "pageArea" without index → first.
        if rest.is_empty() && !page_areas.is_empty() {
            return Some(0);
        }
    }
    None
}

/// Get the primary (largest) content area from a page area.
fn primary_content_area(pa: &PageAreaInfo) -> &ContentArea {
    let max_area = pa
        .content_areas
        .iter()
        .map(|ca| ca.width * ca.height)
        .fold(0.0_f64, f64::max);
    pa.content_areas
        .iter()
        .find(|ca| {
            let a = ca.width * ca.height;
            a >= max_area * 0.90 || pa.content_areas.len() == 1
        })
        .unwrap_or(&pa.content_areas[0])
}

struct PageAreaInfo {
    /// Name of the page area (e.g. "MP1", "MP3") for break targeting.
    name: String,
    /// XFA id attribute (e.g. "Page1", "Page4_ID") for break targeting.
    xfa_id: Option<String>,
    content_areas: Vec<ContentArea>,
    page_width: f64,
    page_height: f64,
    /// Fixed-position nodes (e.g., page-level headers/footers) placed on every
    /// page that uses this page area.
    fixed_nodes: Vec<FormNodeId>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::form::Occur;
    use crate::text::FontMetrics;
    use crate::types::BoxModel;

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

    #[test]
    fn positioned_layout() {
        let mut tree = FormTree::new();
        let f1 = tree.add_node(FormNode {
            name: "Field1".to_string(),
            node_type: FormNodeType::Field {
                value: "A".to_string(),
            },
            box_model: BoxModel {
                width: Some(100.0),
                height: Some(20.0),
                x: 10.0,
                y: 30.0,
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
        });
        let f2 = tree.add_node(FormNode {
            name: "Field2".to_string(),
            node_type: FormNodeType::Field {
                value: "B".to_string(),
            },
            box_model: BoxModel {
                width: Some(100.0),
                height: Some(20.0),
                x: 10.0,
                y: 60.0,
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
        });
        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(612.0),
                height: Some(792.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![f1, f2],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        assert_eq!(result.pages.len(), 1);
        let page = &result.pages[0];
        assert_eq!(page.nodes.len(), 2);
        assert_eq!(page.nodes[0].rect.x, 10.0);
        assert_eq!(page.nodes[0].rect.y, 30.0);
        assert_eq!(page.nodes[1].rect.x, 10.0);
        assert_eq!(page.nodes[1].rect.y, 60.0);
    }

    #[test]
    fn tb_layout() {
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "Field1", 200.0, 30.0);
        let f2 = make_field(&mut tree, "Field2", 200.0, 30.0);
        let f3 = make_field(&mut tree, "Field3", 200.0, 30.0);

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(612.0),
                height: Some(792.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![f1, f2, f3],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        assert_eq!(result.pages.len(), 1);
        let page = &result.pages[0];
        assert_eq!(page.nodes.len(), 3);
        assert_eq!(page.nodes[0].rect.y, 0.0);
        assert_eq!(page.nodes[1].rect.y, 30.0);
        assert_eq!(page.nodes[2].rect.y, 60.0);
    }

    #[test]
    fn lr_tb_wrapping() {
        let mut tree = FormTree::new();
        // 3 fields of 250pt width in a 600pt container → 2 fit on first row, 1 wraps
        let f1 = make_field(&mut tree, "F1", 250.0, 30.0);
        let f2 = make_field(&mut tree, "F2", 250.0, 30.0);
        let f3 = make_field(&mut tree, "F3", 250.0, 30.0);

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(600.0),
                height: Some(792.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::LeftToRightTB,
            children: vec![f1, f2, f3],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        let page = &result.pages[0];
        assert_eq!(page.nodes.len(), 3);
        // First two on row 1
        assert_eq!(page.nodes[0].rect.x, 0.0);
        assert_eq!(page.nodes[0].rect.y, 0.0);
        assert_eq!(page.nodes[1].rect.x, 250.0);
        assert_eq!(page.nodes[1].rect.y, 0.0);
        // Third wraps to row 2
        assert_eq!(page.nodes[2].rect.x, 0.0);
        assert_eq!(page.nodes[2].rect.y, 30.0);
    }

    #[test]
    fn nested_subforms() {
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "Name", 200.0, 25.0);
        let f2 = make_field(&mut tree, "Email", 200.0, 25.0);

        let sub = make_subform(
            &mut tree,
            "PersonalInfo",
            LayoutStrategy::TopToBottom,
            Some(300.0),
            Some(100.0),
            vec![f1, f2],
        );

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(612.0),
                height: Some(792.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![sub],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        let page = &result.pages[0];
        assert_eq!(page.nodes.len(), 1);
        let subform = &page.nodes[0];
        assert_eq!(subform.name, "PersonalInfo");
        assert_eq!(subform.rect.width, 300.0);
        assert_eq!(subform.children.len(), 2);
        assert_eq!(subform.children[0].rect.y, 0.0);
        assert_eq!(subform.children[1].rect.y, 25.0);
    }

    #[test]
    fn page_area_layout() {
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "Field1", 200.0, 30.0);

        let page_area = tree.add_node(FormNode {
            name: "Page1".to_string(),
            node_type: FormNodeType::PageArea {
                content_areas: vec![ContentArea {
                    name: "Body".to_string(),
                    x: 36.0,
                    y: 36.0,
                    width: 540.0,
                    height: 720.0,
                    leader: None,
                    trailer: None,
                }],
            },
            box_model: BoxModel {
                width: Some(612.0),
                height: Some(792.0),
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
        });

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(612.0),
                height: Some(792.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![page_area, f1],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        assert_eq!(result.pages.len(), 1);
        let page = &result.pages[0];
        // Field should be offset by content area position (36, 36)
        assert_eq!(page.nodes[0].rect.x, 36.0);
        assert_eq!(page.nodes[0].rect.y, 36.0);
    }

    #[test]
    fn growable_extent() {
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "F1", 100.0, 20.0);
        let f2 = make_field(&mut tree, "F2", 150.0, 20.0);

        // Subform with no explicit size — should grow to fit children
        let sub = make_subform(
            &mut tree,
            "Container",
            LayoutStrategy::TopToBottom,
            None,
            None,
            vec![f1, f2],
        );

        let engine = LayoutEngine::new(&tree);
        let extent = engine.compute_extent(sub);

        // Width = max child width = 150, Height = sum = 40
        assert_eq!(extent.width, 150.0);
        assert_eq!(extent.height, 40.0);
    }

    #[test]
    fn rl_tb_layout() {
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "F1", 100.0, 30.0);
        let f2 = make_field(&mut tree, "F2", 100.0, 30.0);

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(400.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::RightToLeftTB,
            children: vec![f1, f2],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();
        let page = &result.pages[0];

        // RL: first field at right edge, second to its left
        assert_eq!(page.nodes[0].rect.x, 300.0); // 400 - 100
        assert_eq!(page.nodes[1].rect.x, 200.0); // 400 - 100 - 100
    }

    // --- Dynamic sizing tests (Epic 3.5) ---

    #[test]
    fn growable_clamped_by_min() {
        // A container with tiny content but min constraints
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "F1", 50.0, 10.0);

        let sub = tree.add_node(FormNode {
            name: "Container".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: None,
                height: None,
                min_width: 200.0,
                min_height: 100.0,
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![f1],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let extent = engine.compute_extent(sub);

        // Content is 50x10 but min clamps to 200x100
        assert_eq!(extent.width, 200.0);
        assert_eq!(extent.height, 100.0);
    }

    #[test]
    fn growable_clamped_by_max() {
        // A container with large content but max constraints
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "F1", 500.0, 300.0);

        let sub = tree.add_node(FormNode {
            name: "Container".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: None,
                height: None,
                min_width: 0.0,
                min_height: 0.0,
                max_width: 200.0,
                max_height: 100.0,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![f1],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let extent = engine.compute_extent(sub);

        // Content is 500x300 but max clamps to 200x100
        assert_eq!(extent.width, 200.0);
        assert_eq!(extent.height, 100.0);
    }

    #[test]
    fn partially_growable_width_fixed() {
        // Width fixed, height growable
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "F1", 100.0, 25.0);
        let f2 = make_field(&mut tree, "F2", 100.0, 25.0);

        let sub = tree.add_node(FormNode {
            name: "Container".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: Some(300.0),
                height: None,
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![f1, f2],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let extent = engine.compute_extent(sub);

        // Width fixed at 300, height grows to content (25+25=50)
        assert_eq!(extent.width, 300.0);
        assert_eq!(extent.height, 50.0);
    }

    #[test]
    fn partially_growable_height_fixed() {
        // Height fixed, width growable
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "F1", 100.0, 25.0);
        let f2 = make_field(&mut tree, "F2", 150.0, 25.0);

        let sub = tree.add_node(FormNode {
            name: "Container".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: None,
                height: Some(200.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![f1, f2],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let extent = engine.compute_extent(sub);

        // Height fixed at 200, width grows to max child (150)
        assert_eq!(extent.width, 150.0);
        assert_eq!(extent.height, 200.0);
    }

    #[test]
    fn growable_fills_available_width_in_tb() {
        // A growable subform inside a tb-layout parent should fill parent width
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "F1", 100.0, 25.0);

        let growable_sub = tree.add_node(FormNode {
            name: "GrowableSub".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: None,
                height: None,
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![f1],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(500.0),
                height: Some(400.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![growable_sub],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        let page = &result.pages[0];
        // Growable subform should fill the parent's available width (500)
        assert_eq!(page.nodes[0].rect.width, 500.0);
        // Height should be content-based (25)
        assert_eq!(page.nodes[0].rect.height, 25.0);
    }

    #[test]
    fn growable_fill_capped_by_max() {
        // A growable subform filling parent, but capped by maxW
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "F1", 100.0, 25.0);

        let growable_sub = tree.add_node(FormNode {
            name: "GrowableSub".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: None,
                height: None,
                max_width: 300.0,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![f1],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(500.0),
                height: Some(400.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![growable_sub],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        let page = &result.pages[0];
        // Would fill 500, but maxW caps it to 300
        assert_eq!(page.nodes[0].rect.width, 300.0);
    }

    #[test]
    fn growable_with_margins_in_tb() {
        // Growable container with margins should fill parent minus margins
        use crate::types::Insets;
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "F1", 50.0, 20.0);

        let growable_sub = tree.add_node(FormNode {
            name: "GrowableSub".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: None,
                height: None,
                margins: Insets {
                    top: 5.0,
                    right: 10.0,
                    bottom: 5.0,
                    left: 10.0,
                },
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![f1],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(300.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![growable_sub],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        let page = &result.pages[0];
        // Width: content fills 400 - margins(20) = 380, outer = 380 + 20 = 400
        assert_eq!(page.nodes[0].rect.width, 400.0);
        // Height: content 20, outer = 20 + margins(10) = 30
        assert_eq!(page.nodes[0].rect.height, 30.0);
    }

    #[test]
    fn nested_growable_containers() {
        // Nested growable containers should propagate constraints correctly
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "F1", 80.0, 20.0);

        let inner = tree.add_node(FormNode {
            name: "Inner".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: None,
                height: None,
                min_width: 150.0,
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![f1],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        // Outer container with maxW constraint
        let outer = tree.add_node(FormNode {
            name: "Outer".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: None,
                height: None,
                max_width: 400.0,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![inner],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let extent = engine.compute_extent(outer);

        // Inner: content 80, minW clamps to 150 → 150x20
        // Outer: child is 150, maxW is 400 → 150x20
        assert_eq!(extent.width, 150.0);
        assert_eq!(extent.height, 20.0);
    }

    #[test]
    fn min_max_in_lr_tb_layout() {
        // Min/max constraints on children in lr-tb layout
        let mut tree = FormTree::new();

        let f1 = tree.add_node(FormNode {
            name: "F1".to_string(),
            node_type: FormNodeType::Field {
                value: "A".to_string(),
            },
            box_model: BoxModel {
                width: None,
                height: Some(30.0),
                min_width: 200.0,
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
        });

        let f2 = make_field(&mut tree, "F2", 200.0, 30.0);

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(500.0),
                height: Some(400.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::LeftToRightTB,
            children: vec![f1, f2],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        let page = &result.pages[0];
        // F1 has minW=200, no content so uses 200
        assert_eq!(page.nodes[0].rect.width, 200.0);
        // F2 at x=200
        assert_eq!(page.nodes[1].rect.x, 200.0);
    }

    // --- Occur rules tests (Epic 3.6) ---

    #[test]
    fn occur_default_once() {
        // Default occur = 1, should produce exactly one instance
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "F1", 100.0, 30.0);

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(400.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![f1],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();
        assert_eq!(result.pages[0].nodes.len(), 1);
    }

    #[test]
    fn occur_repeating_tb() {
        // A subform with occur(initial=3) in tb layout should produce 3 instances
        let mut tree = FormTree::new();
        let f1 = tree.add_node(FormNode {
            name: "Row".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: Some(200.0),
                height: Some(30.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::repeating(1, Some(10), 3),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(400.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![f1],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();
        let page = &result.pages[0];

        // 3 instances stacked vertically
        assert_eq!(page.nodes.len(), 3);
        assert_eq!(page.nodes[0].rect.y, 0.0);
        assert_eq!(page.nodes[1].rect.y, 30.0);
        assert_eq!(page.nodes[2].rect.y, 60.0);
    }

    #[test]
    fn occur_repeating_lr_tb() {
        // Repeating subform in lr-tb layout
        let mut tree = FormTree::new();
        let f1 = tree.add_node(FormNode {
            name: "Cell".to_string(),
            node_type: FormNodeType::Field {
                value: "X".to_string(),
            },
            box_model: BoxModel {
                width: Some(100.0),
                height: Some(30.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::repeating(1, None, 5),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(350.0),
                height: Some(400.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::LeftToRightTB,
            children: vec![f1],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();
        let page = &result.pages[0];

        // 5 instances of 100pt wide in 350pt container:
        // Row 1: 3 cells (0, 100, 200), Row 2: 2 cells (0, 100)
        assert_eq!(page.nodes.len(), 5);
        assert_eq!(page.nodes[0].rect.x, 0.0);
        assert_eq!(page.nodes[0].rect.y, 0.0);
        assert_eq!(page.nodes[1].rect.x, 100.0);
        assert_eq!(page.nodes[2].rect.x, 200.0);
        assert_eq!(page.nodes[3].rect.x, 0.0);
        assert_eq!(page.nodes[3].rect.y, 30.0);
        assert_eq!(page.nodes[4].rect.x, 100.0);
        assert_eq!(page.nodes[4].rect.y, 30.0);
    }

    #[test]
    fn occur_min_enforced() {
        // Occur with min=2, initial=2 should always produce at least 2
        let occur = Occur::repeating(2, Some(5), 2);
        assert_eq!(occur.count(), 2);
        assert!(occur.is_repeating());
    }

    #[test]
    fn occur_max_caps_initial() {
        // Occur with max=3 but initial=5 should cap at 3
        let occur = Occur::repeating(1, Some(3), 5);
        assert_eq!(occur.count(), 3);
    }

    #[test]
    fn occur_initial_raised_to_min() {
        // Occur with min=3 but initial=1 should raise to 3
        let occur = Occur::repeating(3, Some(10), 1);
        assert_eq!(occur.count(), 3);
    }

    #[test]
    fn occur_unlimited_max() {
        let occur = Occur::repeating(0, None, 5);
        assert_eq!(occur.count(), 5);
        assert!(occur.is_repeating());
    }

    #[test]
    fn occur_mixed_children() {
        // Mix of repeating and non-repeating children
        let mut tree = FormTree::new();
        let header = make_field(&mut tree, "Header", 200.0, 40.0);
        let row = tree.add_node(FormNode {
            name: "DataRow".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: Some(200.0),
                height: Some(25.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::repeating(1, Some(10), 4),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });
        let footer = make_field(&mut tree, "Footer", 200.0, 30.0);

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(600.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![header, row, footer],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();
        let page = &result.pages[0];

        // Header(1) + DataRow(4) + Footer(1) = 6 nodes
        assert_eq!(page.nodes.len(), 6);
        assert_eq!(page.nodes[0].name, "Header");
        assert_eq!(page.nodes[0].rect.y, 0.0);
        // 4 data rows at y=40, 65, 90, 115
        assert_eq!(page.nodes[1].name, "DataRow");
        assert_eq!(page.nodes[1].rect.y, 40.0);
        assert_eq!(page.nodes[2].rect.y, 65.0);
        assert_eq!(page.nodes[3].rect.y, 90.0);
        assert_eq!(page.nodes[4].rect.y, 115.0);
        // Footer at y=140
        assert_eq!(page.nodes[5].name, "Footer");
        assert_eq!(page.nodes[5].rect.y, 140.0);
    }

    #[test]
    fn occur_growable_extent() {
        // A growable container with a repeating child should size to all instances
        let mut tree = FormTree::new();
        let row = tree.add_node(FormNode {
            name: "Row".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: Some(150.0),
                height: Some(20.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::repeating(1, None, 5),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let container = tree.add_node(FormNode {
            name: "Container".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: None,
                height: None,
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![row],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let extent = engine.compute_extent(container);

        // 5 rows of 150x20 stacked: width=150, height=100
        assert_eq!(extent.width, 150.0);
        assert_eq!(extent.height, 100.0);
    }

    // --- Pagination tests (Epic 3.7) ---

    #[test]
    fn pagination_single_page_no_overflow() {
        // Content fits on one page — no extra pages
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "F1", 200.0, 30.0);
        let f2 = make_field(&mut tree, "F2", 200.0, 30.0);

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(200.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![f1, f2],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        assert_eq!(result.pages.len(), 1);
        assert_eq!(result.pages[0].nodes.len(), 2);
    }

    #[test]
    fn pagination_overflow_creates_pages() {
        // 10 fields of 30pt each = 300pt total, page height 100pt → 3 pages
        let mut tree = FormTree::new();
        let mut fields = Vec::new();
        for i in 0..10 {
            fields.push(make_field(&mut tree, &format!("F{i}"), 200.0, 30.0));
        }

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(100.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: fields,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        // 100pt fits 3 fields (0+30+30+30=90 < 100). 4th at 90+30=120 > 100.
        // Page 1: 3 fields, Page 2: 3 fields, Page 3: 3 fields, Page 4: 1 field
        assert_eq!(result.pages.len(), 4);
        assert_eq!(result.pages[0].nodes.len(), 3);
        assert_eq!(result.pages[1].nodes.len(), 3);
        assert_eq!(result.pages[2].nodes.len(), 3);
        assert_eq!(result.pages[3].nodes.len(), 1);
    }

    #[test]
    fn pagination_with_page_area() {
        // PageArea with content area, content overflows to multiple pages
        let mut tree = FormTree::new();
        let mut fields = Vec::new();
        for i in 0..6 {
            fields.push(make_field(&mut tree, &format!("F{i}"), 200.0, 50.0));
        }

        let page_area = tree.add_node(FormNode {
            name: "Page1".to_string(),
            node_type: FormNodeType::PageArea {
                content_areas: vec![ContentArea {
                    name: "Body".to_string(),
                    x: 20.0,
                    y: 20.0,
                    width: 360.0,
                    height: 160.0, // fits 3 fields of 50pt
                    leader: None,
                    trailer: None,
                }],
            },
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(200.0),
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
        });

        let mut root_children = vec![page_area];
        root_children.extend(fields);

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(200.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: root_children,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        // 6 fields × 50pt = 300pt, content area is 160pt → 2 pages
        assert_eq!(result.pages.len(), 2);
        assert_eq!(result.pages[0].nodes.len(), 3);
        assert_eq!(result.pages[1].nodes.len(), 3);

        // Nodes should be offset by content area position (20, 20)
        assert_eq!(result.pages[0].nodes[0].rect.x, 20.0);
        assert_eq!(result.pages[0].nodes[0].rect.y, 20.0);
        assert_eq!(result.pages[0].nodes[1].rect.y, 70.0); // 20 + 50
    }

    #[test]
    fn pagination_with_occur_repeating() {
        // Repeating subform creating many instances that overflow
        let mut tree = FormTree::new();
        let row = tree.add_node(FormNode {
            name: "DataRow".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: Some(200.0),
                height: Some(25.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::repeating(1, None, 8),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(100.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![row],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        // 8 rows × 25pt = 200pt, page 100pt → 2 pages (4+4)
        assert_eq!(result.pages.len(), 2);
        assert_eq!(result.pages[0].nodes.len(), 4);
        assert_eq!(result.pages[1].nodes.len(), 4);
    }

    #[test]
    fn pagination_oversized_item_forced() {
        // Single item taller than page — should still be placed (forced)
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "Big", 200.0, 200.0); // taller than page
        let f2 = make_field(&mut tree, "Small", 200.0, 30.0);

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(100.0), // page shorter than f1
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![f1, f2],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        // Big item forced onto page 1, Small on page 2
        assert_eq!(result.pages.len(), 2);
        assert_eq!(result.pages[0].nodes[0].name, "Big");
        assert_eq!(result.pages[1].nodes[0].name, "Small");
    }

    #[test]
    fn pagination_page_dimensions_correct() {
        // All pages should have correct dimensions
        let mut tree = FormTree::new();
        let mut fields = Vec::new();
        for i in 0..5 {
            fields.push(make_field(&mut tree, &format!("F{i}"), 200.0, 50.0));
        }

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(500.0),
                height: Some(120.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: fields,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        for page in &result.pages {
            assert_eq!(page.width, 500.0);
            assert_eq!(page.height, 120.0);
        }
    }

    // --- Content splitting tests (Epic 3.8) ---

    #[test]
    fn split_subform_across_pages() {
        // A tb-subform with 6 children of 30pt each (180pt total)
        // on a page with only 100pt remaining after a header.
        // The subform should be split: some children on page 1, rest on page 2.
        let mut tree = FormTree::new();
        let header = make_field(&mut tree, "Header", 300.0, 40.0);

        let mut sub_children = Vec::new();
        for i in 0..6 {
            sub_children.push(make_field(&mut tree, &format!("Row{i}"), 300.0, 30.0));
        }

        let subform = tree.add_node(FormNode {
            name: "DataBlock".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: Some(300.0),
                height: None, // growable
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: sub_children,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(160.0), // header(40) + 120pt left → fits 4 rows
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![header, subform],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        // Page 1: header + partial subform (4 rows fit in 120pt)
        // Page 2: remaining 2 rows
        assert!(result.pages.len() >= 2);
        // Page 1 has header + partial subform
        let p1 = &result.pages[0];
        assert_eq!(p1.nodes[0].name, "Header");
        assert_eq!(p1.nodes[1].name, "DataBlock");
        let split_sub = &p1.nodes[1];
        assert_eq!(split_sub.children.len(), 4); // 4 rows fit

        // Page 2 has the remaining 2 rows
        let p2 = &result.pages[1];
        assert_eq!(p2.nodes.len(), 2);
    }

    #[test]
    fn split_preserves_node_positions() {
        // Verify that split children have correct y positions within their partial container
        let mut tree = FormTree::new();
        let mut sub_children = Vec::new();
        for i in 0..4 {
            sub_children.push(make_field(&mut tree, &format!("Row{i}"), 200.0, 25.0));
        }

        let subform = tree.add_node(FormNode {
            name: "Block".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: Some(200.0),
                height: None,
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: sub_children,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(60.0), // fits 2 rows of 25pt (50pt < 60pt, 75pt > 60pt)
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![subform],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        // First page: partial subform with 2 children
        let split_sub = &result.pages[0].nodes[0];
        assert_eq!(split_sub.children.len(), 2);
        assert_eq!(split_sub.children[0].rect.y, 0.0);
        assert_eq!(split_sub.children[1].rect.y, 25.0);
    }

    #[test]
    fn no_split_for_non_tb_layout() {
        // A positioned subform should NOT be split — goes entirely to next page
        let mut tree = FormTree::new();
        let header = make_field(&mut tree, "Header", 300.0, 80.0);

        let f1 = tree.add_node(FormNode {
            name: "Child1".to_string(),
            node_type: FormNodeType::Field {
                value: "A".to_string(),
            },
            box_model: BoxModel {
                width: Some(100.0),
                height: Some(50.0),
                x: 0.0,
                y: 0.0,
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
        });

        let subform = tree.add_node(FormNode {
            name: "PositionedBlock".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                width: Some(200.0),
                height: Some(100.0), // fixed size, doesn't fit after header
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned, // can't split
            children: vec![f1],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(100.0), // header takes 80pt, subform needs 100pt → overflow
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![header, subform],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        // Header on page 1, positioned subform on page 2 (not split)
        assert_eq!(result.pages.len(), 2);
        assert_eq!(result.pages[0].nodes[0].name, "Header");
        assert_eq!(result.pages[1].nodes[0].name, "PositionedBlock");
    }

    #[test]
    fn can_split_checks() {
        let mut tree = FormTree::new();
        let f1 = make_field(&mut tree, "F1", 100.0, 20.0);

        let tb_sub = tree.add_node(FormNode {
            name: "TB".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![f1],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let pos_sub = tree.add_node(FormNode {
            name: "Pos".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![f1],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let empty_sub = tree.add_node(FormNode {
            name: "Empty".to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        assert!(engine.can_split(tb_sub));
        assert!(!engine.can_split(pos_sub));
        assert!(!engine.can_split(empty_sub));
    }

    // --- Leaders & trailers tests (Epic 3.10) ---

    #[test]
    fn leader_placed_at_top() {
        // Leader (header) should appear at the top of each page
        let mut tree = FormTree::new();
        let header = make_field(&mut tree, "PageHeader", 300.0, 30.0);
        let f1 = make_field(&mut tree, "Content1", 300.0, 50.0);
        let f2 = make_field(&mut tree, "Content2", 300.0, 50.0);

        let page_area = tree.add_node(FormNode {
            name: "Page1".to_string(),
            node_type: FormNodeType::PageArea {
                content_areas: vec![ContentArea {
                    name: "Body".to_string(),
                    x: 0.0,
                    y: 0.0,
                    width: 400.0,
                    height: 200.0,
                    leader: Some(header),
                    trailer: None,
                }],
            },
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(200.0),
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
        });

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(200.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![page_area, f1, f2],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        let page = &result.pages[0];
        // Header at y=0, content starts at y=30
        assert_eq!(page.nodes[0].name, "PageHeader");
        assert_eq!(page.nodes[0].rect.y, 0.0);
        // Content after header
        assert!(page.nodes.len() >= 2);
        // First content node at y=30 (after header)
        let first_content = page.nodes.iter().find(|n| n.name == "Content1").unwrap();
        assert_eq!(first_content.rect.y, 30.0);
    }

    #[test]
    fn trailer_placed_at_bottom() {
        // Trailer (footer) should appear at the bottom of the content area
        let mut tree = FormTree::new();
        let footer = make_field(&mut tree, "PageFooter", 300.0, 25.0);
        let f1 = make_field(&mut tree, "Content1", 300.0, 50.0);

        let page_area = tree.add_node(FormNode {
            name: "Page1".to_string(),
            node_type: FormNodeType::PageArea {
                content_areas: vec![ContentArea {
                    name: "Body".to_string(),
                    x: 0.0,
                    y: 0.0,
                    width: 400.0,
                    height: 200.0,
                    leader: None,
                    trailer: Some(footer),
                }],
            },
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(200.0),
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
        });

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(200.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: vec![page_area, f1],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        let page = &result.pages[0];
        // Footer at bottom: y = 200 - 25 = 175
        let footer_node = page.nodes.iter().find(|n| n.name == "PageFooter").unwrap();
        assert_eq!(footer_node.rect.y, 175.0);
    }

    #[test]
    fn leader_and_trailer_reduce_content_space() {
        // With both leader and trailer, content space is reduced
        let mut tree = FormTree::new();
        let header = make_field(&mut tree, "Header", 300.0, 30.0);
        let footer = make_field(&mut tree, "Footer", 300.0, 20.0);

        // 5 fields of 30pt each = 150pt total
        let mut fields = Vec::new();
        for i in 0..5 {
            fields.push(make_field(&mut tree, &format!("F{i}"), 300.0, 30.0));
        }

        let page_area = tree.add_node(FormNode {
            name: "Page1".to_string(),
            node_type: FormNodeType::PageArea {
                content_areas: vec![ContentArea {
                    name: "Body".to_string(),
                    x: 0.0,
                    y: 0.0,
                    width: 400.0,
                    height: 200.0, // 200 - 30(header) - 20(footer) = 150pt for content
                    leader: Some(header),
                    trailer: Some(footer),
                }],
            },
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(200.0),
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
        });

        let mut root_children = vec![page_area];
        root_children.extend(fields);

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(200.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: root_children,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        // Content space is 150pt, 5 fields × 30pt = 150pt → exactly fits on 1 page
        assert_eq!(result.pages.len(), 1);
        let page = &result.pages[0];
        // header + 5 content + footer = 7 nodes
        assert_eq!(page.nodes.len(), 7);
    }

    #[test]
    fn leader_trailer_repeated_on_overflow_pages() {
        // When content overflows, leaders/trailers should appear on each page
        let mut tree = FormTree::new();
        let header = make_field(&mut tree, "Header", 300.0, 30.0);
        let footer = make_field(&mut tree, "Footer", 300.0, 20.0);

        // 8 fields of 30pt = 240pt, available per page = 200-30-20 = 150pt
        // Page 1: 5 fields (150pt), Page 2: 3 fields (90pt)
        let mut fields = Vec::new();
        for i in 0..8 {
            fields.push(make_field(&mut tree, &format!("F{i}"), 300.0, 30.0));
        }

        let page_area = tree.add_node(FormNode {
            name: "Page1".to_string(),
            node_type: FormNodeType::PageArea {
                content_areas: vec![ContentArea {
                    name: "Body".to_string(),
                    x: 0.0,
                    y: 0.0,
                    width: 400.0,
                    height: 200.0,
                    leader: Some(header),
                    trailer: Some(footer),
                }],
            },
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(200.0),
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
        });

        let mut root_children = vec![page_area];
        root_children.extend(fields);

        let root = tree.add_node(FormNode {
            name: "Root".to_string(),
            node_type: FormNodeType::Root,
            box_model: BoxModel {
                width: Some(400.0),
                height: Some(200.0),
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::TopToBottom,
            children: root_children,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        assert_eq!(result.pages.len(), 2);

        // Both pages should have header and footer
        for page in &result.pages {
            let has_header = page.nodes.iter().any(|n| n.name == "Header");
            let has_footer = page.nodes.iter().any(|n| n.name == "Footer");
            assert!(has_header, "Page missing header");
            assert!(has_footer, "Page missing footer");
        }
    }

    // ── Text placement tests ──────────────────────────────────────────

    #[test]
    fn draw_node_growable_height_from_text() {
        // A Draw node with fixed width but no height should grow to fit text
        let mut tree = FormTree::new();
        let draw = tree.add_node(FormNode {
            name: "Label".to_string(),
            node_type: FormNodeType::Draw {
                content: "Hello World".to_string(),
            },
            box_model: BoxModel {
                width: Some(200.0),
                height: None, // growable
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(), // 10pt, avg_char_width=0.5
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });
        let root = make_subform(
            &mut tree,
            "Root",
            LayoutStrategy::TopToBottom,
            Some(612.0),
            Some(792.0),
            vec![draw],
        );

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        let page = &result.pages[0];
        let label = &page.nodes[0];
        // "Hello World" = 11 chars * 10pt * 0.5 = 55pt wide, fits in 200pt
        // 1 line * 10pt * 1.2 = 12pt tall
        assert_eq!(label.rect.height, 12.0);
    }

    #[test]
    fn draw_node_text_wraps_in_narrow_width() {
        // A Draw node with narrow width should wrap text and grow taller
        let mut tree = FormTree::new();
        let draw = tree.add_node(FormNode {
            name: "Label".to_string(),
            node_type: FormNodeType::Draw {
                content: "Hello World".to_string(),
            },
            box_model: BoxModel {
                width: Some(40.0), // narrow: "Hello" = 25pt fits, "World" wraps
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
        });
        let root = make_subform(
            &mut tree,
            "Root",
            LayoutStrategy::TopToBottom,
            Some(612.0),
            Some(792.0),
            vec![draw],
        );

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        let label = &result.pages[0].nodes[0];
        // 2 lines * 12pt = 24pt
        assert_eq!(label.rect.height, 24.0);
    }

    #[test]
    fn field_produces_wrapped_text_content() {
        let mut tree = FormTree::new();
        let field = tree.add_node(FormNode {
            name: "Name".to_string(),
            node_type: FormNodeType::Field {
                value: "John".to_string(),
            },
            box_model: BoxModel {
                width: Some(200.0),
                height: Some(20.0),
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
        });
        let root = make_subform(
            &mut tree,
            "Root",
            LayoutStrategy::TopToBottom,
            Some(612.0),
            Some(792.0),
            vec![field],
        );

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        let node = &result.pages[0].nodes[0];
        match &node.content {
            LayoutContent::WrappedText {
                lines, font_size, ..
            } => {
                assert_eq!(lines.len(), 1);
                assert_eq!(lines[0], "John");
                assert_eq!(*font_size, 10.0);
            }
            other => panic!("Expected WrappedText, got {:?}", other),
        }
    }

    #[test]
    fn draw_growable_width_and_height_from_text() {
        // Both width and height are growable: should size to text content
        let mut tree = FormTree::new();
        let draw = tree.add_node(FormNode {
            name: "Auto".to_string(),
            node_type: FormNodeType::Draw {
                content: "Test".to_string(),
            },
            box_model: BoxModel {
                width: None,
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
        });

        let engine = LayoutEngine::new(&tree);
        let size = engine.compute_extent(draw);
        // "Test" measured with Helvetica AFM widths: T=611 e=556 s=500 t=278 = 1945/1000*10 = 19.45
        assert!((size.width - 19.45).abs() < 0.1, "width={}", size.width);
        assert_eq!(size.height, 12.0);
    }

    #[test]
    fn custom_font_size_affects_layout() {
        let mut tree = FormTree::new();
        let draw = tree.add_node(FormNode {
            name: "Big".to_string(),
            node_type: FormNodeType::Draw {
                content: "Hi".to_string(),
            },
            box_model: BoxModel {
                width: None,
                height: None,
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::new(20.0), // 20pt font
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        });

        let engine = LayoutEngine::new(&tree);
        let size = engine.compute_extent(draw);
        // "Hi" measured with Helvetica AFM widths: H=722 i=222 = 944/1000*20 = 18.88
        assert!((size.width - 18.88).abs() < 0.1, "width={}", size.width);
        assert_eq!(size.height, 24.0);
    }

    // =========================================================================
    // Table Layout Tests
    // =========================================================================

    fn make_cell(tree: &mut FormTree, name: &str, w: f64, h: f64, col_span: i32) -> FormNodeId {
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
            col_span,
        })
    }

    fn make_row(tree: &mut FormTree, name: &str, cells: Vec<FormNodeId>) -> FormNodeId {
        tree.add_node(FormNode {
            name: name.to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Row,
            children: cells,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths: vec![],
            col_span: 1,
        })
    }

    fn make_table(
        tree: &mut FormTree,
        name: &str,
        column_widths: Vec<f64>,
        rows: Vec<FormNodeId>,
    ) -> FormNodeId {
        tree.add_node(FormNode {
            name: name.to_string(),
            node_type: FormNodeType::Subform,
            box_model: BoxModel {
                max_width: f64::MAX,
                max_height: f64::MAX,
                ..Default::default()
            },
            layout: LayoutStrategy::Table,
            children: rows,
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
            column_widths,
            col_span: 1,
        })
    }

    #[test]
    fn table_basic_fixed_columns() {
        let mut tree = FormTree::new();

        // 3 columns: 100, 150, 200
        let c1 = make_cell(&mut tree, "A1", 100.0, 30.0, 1);
        let c2 = make_cell(&mut tree, "A2", 150.0, 30.0, 1);
        let c3 = make_cell(&mut tree, "A3", 200.0, 30.0, 1);
        let r1 = make_row(&mut tree, "Row1", vec![c1, c2, c3]);

        let c4 = make_cell(&mut tree, "B1", 100.0, 25.0, 1);
        let c5 = make_cell(&mut tree, "B2", 150.0, 25.0, 1);
        let c6 = make_cell(&mut tree, "B3", 200.0, 25.0, 1);
        let r2 = make_row(&mut tree, "Row2", vec![c4, c5, c6]);

        let table = make_table(&mut tree, "Table", vec![100.0, 150.0, 200.0], vec![r1, r2]);

        let page_area = make_subform(
            &mut tree,
            "Page",
            LayoutStrategy::TopToBottom,
            Some(612.0),
            Some(792.0),
            vec![table],
        );

        let engine = LayoutEngine::new(&tree);
        let layout = engine.layout(page_area).unwrap();

        assert_eq!(layout.pages.len(), 1);
        let page = &layout.pages[0];
        // Table node should be on the page
        assert_eq!(page.nodes.len(), 1);
        let table_node = &page.nodes[0];
        assert_eq!(table_node.name, "Table");

        // 2 rows
        assert_eq!(table_node.children.len(), 2);
        let row1 = &table_node.children[0];
        let row2 = &table_node.children[1];

        // Row 1: 3 cells at x=0, 100, 250
        assert_eq!(row1.children.len(), 3);
        assert_eq!(row1.children[0].rect.x, 0.0);
        assert_eq!(row1.children[0].rect.width, 100.0);
        assert_eq!(row1.children[1].rect.x, 100.0);
        assert_eq!(row1.children[1].rect.width, 150.0);
        assert_eq!(row1.children[2].rect.x, 250.0);
        assert_eq!(row1.children[2].rect.width, 200.0);

        // Row 2 stacked below row 1
        assert_eq!(row2.rect.y, 30.0); // row 1 height = 30
        assert_eq!(row2.children[0].rect.x, 0.0);
    }

    #[test]
    fn table_auto_columns() {
        let mut tree = FormTree::new();

        // Auto columns: -1 means auto-size
        let c1 = make_cell(&mut tree, "A", 80.0, 20.0, 1);
        let c2 = make_cell(&mut tree, "B", 120.0, 20.0, 1);
        let r1 = make_row(&mut tree, "Row1", vec![c1, c2]);

        let c3 = make_cell(&mut tree, "C", 60.0, 20.0, 1);
        let c4 = make_cell(&mut tree, "D", 150.0, 20.0, 1);
        let r2 = make_row(&mut tree, "Row2", vec![c3, c4]);

        // Auto-size: widest in col 0 = 80, col 1 = 150
        let table = make_table(&mut tree, "Table", vec![-1.0, -1.0], vec![r1, r2]);

        let page = make_subform(
            &mut tree,
            "Page",
            LayoutStrategy::TopToBottom,
            Some(612.0),
            Some(792.0),
            vec![table],
        );

        let engine = LayoutEngine::new(&tree);
        let layout = engine.layout(page).unwrap();

        let table_node = &layout.pages[0].nodes[0];
        let row1 = &table_node.children[0];

        // Column 0 auto-sized to 80 (widest), Column 1 auto-sized to 150
        assert_eq!(row1.children[0].rect.width, 80.0);
        assert_eq!(row1.children[1].rect.width, 150.0);
        assert_eq!(row1.children[1].rect.x, 80.0);
    }

    #[test]
    fn table_col_span() {
        let mut tree = FormTree::new();

        // 3 fixed columns: 100, 100, 100
        let c1 = make_cell(&mut tree, "Span2", 200.0, 20.0, 2); // spans 2 columns
        let c2 = make_cell(&mut tree, "Single", 100.0, 20.0, 1);
        let r1 = make_row(&mut tree, "Row1", vec![c1, c2]);

        let table = make_table(&mut tree, "Table", vec![100.0, 100.0, 100.0], vec![r1]);

        let page = make_subform(
            &mut tree,
            "Page",
            LayoutStrategy::TopToBottom,
            Some(612.0),
            Some(792.0),
            vec![table],
        );

        let engine = LayoutEngine::new(&tree);
        let layout = engine.layout(page).unwrap();

        let row = &layout.pages[0].nodes[0].children[0];
        // First cell spans 2 columns: width = 100 + 100 = 200
        assert_eq!(row.children[0].rect.width, 200.0);
        assert_eq!(row.children[0].rect.x, 0.0);
        // Second cell at x=200, width=100
        assert_eq!(row.children[1].rect.x, 200.0);
        assert_eq!(row.children[1].rect.width, 100.0);
    }

    #[test]
    fn table_col_span_rest() {
        let mut tree = FormTree::new();

        // 3 fixed columns: 100, 100, 100
        let c1 = make_cell(&mut tree, "First", 100.0, 20.0, 1);
        let c2 = make_cell(&mut tree, "Rest", 200.0, 20.0, -1); // span remaining
        let r1 = make_row(&mut tree, "Row1", vec![c1, c2]);

        let table = make_table(&mut tree, "Table", vec![100.0, 100.0, 100.0], vec![r1]);

        let page = make_subform(
            &mut tree,
            "Page",
            LayoutStrategy::TopToBottom,
            Some(612.0),
            Some(792.0),
            vec![table],
        );

        let engine = LayoutEngine::new(&tree);
        let layout = engine.layout(page).unwrap();

        let row = &layout.pages[0].nodes[0].children[0];
        // First cell: width=100 at x=0
        assert_eq!(row.children[0].rect.width, 100.0);
        // Second cell: spans remaining = 100 + 100 = 200 at x=100
        assert_eq!(row.children[1].rect.x, 100.0);
        assert_eq!(row.children[1].rect.width, 200.0);
    }

    #[test]
    fn table_row_height_equalization() {
        let mut tree = FormTree::new();

        // Cells with different heights: 30, 50, 20
        let c1 = make_cell(&mut tree, "Short", 100.0, 30.0, 1);
        let c2 = make_cell(&mut tree, "Tall", 100.0, 50.0, 1);
        let c3 = make_cell(&mut tree, "Tiny", 100.0, 20.0, 1);
        let r1 = make_row(&mut tree, "Row1", vec![c1, c2, c3]);

        let table = make_table(&mut tree, "Table", vec![100.0, 100.0, 100.0], vec![r1]);

        let page = make_subform(
            &mut tree,
            "Page",
            LayoutStrategy::TopToBottom,
            Some(612.0),
            Some(792.0),
            vec![table],
        );

        let engine = LayoutEngine::new(&tree);
        let layout = engine.layout(page).unwrap();

        let row = &layout.pages[0].nodes[0].children[0];
        // All cells should have height = 50 (tallest cell)
        assert_eq!(row.children[0].rect.height, 50.0);
        assert_eq!(row.children[1].rect.height, 50.0);
        assert_eq!(row.children[2].rect.height, 50.0);
        // Row itself should be 50
        assert_eq!(row.rect.height, 50.0);
    }

    #[test]
    fn table_growable_height() {
        let mut tree = FormTree::new();

        let c1 = make_cell(&mut tree, "A", 100.0, 30.0, 1);
        let r1 = make_row(&mut tree, "Row1", vec![c1]);

        let c2 = make_cell(&mut tree, "B", 100.0, 40.0, 1);
        let r2 = make_row(&mut tree, "Row2", vec![c2]);

        // Table with no explicit height (growable)
        let table = make_table(&mut tree, "Table", vec![100.0], vec![r1, r2]);

        let engine = LayoutEngine::new(&tree);
        let extent = engine.compute_extent(table);

        // Table height = sum of row heights = 30 + 40 = 70
        assert_eq!(extent.height, 70.0);
        // Table width = column width = 100
        assert_eq!(extent.width, 100.0);
    }

    #[test]
    fn table_empty() {
        let mut tree = FormTree::new();
        let table = make_table(&mut tree, "EmptyTable", vec![100.0, 200.0], vec![]);

        let page = make_subform(
            &mut tree,
            "Page",
            LayoutStrategy::TopToBottom,
            Some(612.0),
            Some(792.0),
            vec![table],
        );

        let engine = LayoutEngine::new(&tree);
        let layout = engine.layout(page).unwrap();

        // Table exists but has no row children
        let table_node = &layout.pages[0].nodes[0];
        assert_eq!(table_node.children.len(), 0);
    }
}
