//! Layout engine — positions form nodes into layout rectangles.
//!
//! Implements XFA 3.3 §4 (Box Model) and §8 (Layout for Growable Objects).
//! Supports positioned layout and flowed layout (tb, lr-tb, rl-tb).

use crate::error::{LayoutError, Result};
use crate::form::{ContentArea, FormNode, FormNodeId, FormNodeType, FormTree};
use crate::types::{LayoutStrategy, Rect, Size};

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
}

/// Content type for layout leaf nodes.
#[derive(Debug, Clone)]
pub enum LayoutContent {
    None,
    Text(String),
    Field { value: String },
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
        // Expand occur rules for top-level content used by layout_content_fitting
        // (layout_content_on_page uses layout_children which expands internally)
        let content_nodes_expanded = self.expand_occur(&raw_content_nodes);

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
                let mut remaining = content_nodes_expanded;
                while !remaining.is_empty() {
                    let (page, rest) =
                        self.layout_content_fitting(&area, &remaining, page_w, page_h)?;
                    if page.nodes.is_empty() {
                        // Force place one item to prevent infinite loop
                        let forced = self.layout_content_on_page(
                            &area,
                            page_w,
                            page_h,
                            &remaining[..1],
                            root_node.layout,
                        )?;
                        pages.push(forced);
                        remaining = remaining[1..].to_vec();
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
            // Layout content across page areas, then repeat last template for overflow
            let mut remaining = content_nodes_expanded;
            for pa in &page_areas {
                if remaining.is_empty() {
                    break;
                }
                for ca in &pa.content_areas {
                    if remaining.is_empty() {
                        break;
                    }
                    let (placed, rest) =
                        self.layout_content_fitting(ca, &remaining, pa.page_width, pa.page_height)?;
                    if !placed.nodes.is_empty() {
                        pages.push(placed);
                    }
                    remaining = rest;
                }
            }

            // Overflow: repeat last page template until all content is placed
            if !remaining.is_empty() {
                let last_pa = page_areas.last().unwrap();
                let ca = last_pa
                    .content_areas
                    .first()
                    .ok_or(LayoutError::NoMatchingPageArea)?;

                while !remaining.is_empty() {
                    let (page, rest) = self.layout_content_fitting(
                        ca,
                        &remaining,
                        last_pa.page_width,
                        last_pa.page_height,
                    )?;
                    if page.nodes.is_empty() {
                        // Force place one item to prevent infinite loop
                        let forced = self.layout_content_on_page(
                            ca,
                            last_pa.page_width,
                            last_pa.page_height,
                            &remaining[..1],
                            LayoutStrategy::TopToBottom,
                        )?;
                        pages.push(forced);
                        remaining = remaining[1..].to_vec();
                    } else {
                        pages.push(page);
                        remaining = rest;
                    }
                }
            }
        }

        Ok(LayoutDom { pages })
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
                            page_areas.push(PageAreaInfo {
                                content_areas: content_areas.clone(),
                                page_width: pa_node.box_model.width.unwrap_or(612.0),
                                page_height: pa_node.box_model.height.unwrap_or(792.0),
                            });
                        }
                    }
                }
                FormNodeType::PageArea { content_areas } => {
                    page_areas.push(PageAreaInfo {
                        content_areas: content_areas.clone(),
                        page_width: child.box_model.width.unwrap_or(612.0),
                        page_height: child.box_model.height.unwrap_or(792.0),
                    });
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

    fn layout_content_fitting(
        &self,
        content_area: &ContentArea,
        content_ids: &[FormNodeId],
        page_width: f64,
        page_height: f64,
    ) -> Result<(LayoutPage, Vec<FormNodeId>)> {
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
        let mut split_remaining: Vec<FormNodeId> = Vec::new();
        let content_bottom = leader_height + content_height;

        for &child_id in content_ids {
            let child = self.form.get(child_id);
            let child_size = self.compute_extent_with_available(child_id, Some(available));

            if y_cursor + child_size.height > content_bottom {
                let remaining_height = content_bottom - y_cursor;

                // Try to split this node if it's a splittable tb-layout container
                if remaining_height > 0.0 && self.can_split(child_id) {
                    let (partial, rest_children) =
                        self.split_tb_node(child_id, y_cursor, remaining_height, available)?;
                    if !partial.children.is_empty() {
                        let mut offset_node = partial;
                        offset_node.rect.x += content_area.x;
                        offset_node.rect.y += content_area.y;
                        page.nodes.push(offset_node);
                        placed_count += 1;
                        split_remaining = rest_children;
                    }
                } else if placed_count == 0 {
                    // First item too large and can't split — force place it
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

            let node =
                self.layout_single_node_with_extent(child_id, child, 0.0, y_cursor, child_size)?;
            let mut offset_node = node;
            offset_node.rect.x += content_area.x;
            offset_node.rect.y += content_area.y;
            page.nodes.push(offset_node);

            y_cursor += child_size.height;
            placed_count += 1;
        }

        let mut remaining = split_remaining;
        remaining.extend_from_slice(&content_ids[placed_count..]);
        Ok((page, remaining))
    }

    /// Check if a node can be split across pages.
    ///
    /// Only tb-layout subforms with children can be split.
    fn can_split(&self, id: FormNodeId) -> bool {
        let node = self.form.get(id);
        node.layout == LayoutStrategy::TopToBottom && !node.children.is_empty()
    }

    /// Split a tb-layout node: place children that fit in `remaining_height`,
    /// return a partial layout node and the remaining child IDs.
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

        for (i, &child_id) in expanded_children.iter().enumerate() {
            let child = self.form.get(child_id);
            let child_size = self.compute_extent(child_id);

            if child_y + child_size.height > remaining_height && !placed_children.is_empty() {
                split_idx = i;
                break;
            }

            let child_node = self.layout_single_node(child_id, child, 0.0, child_y)?;
            placed_children.push(child_node);
            child_y += child_size.height;
            split_idx = i + 1;
        }

        let content = match &node.node_type {
            FormNodeType::Field { value } => LayoutContent::Field {
                value: value.clone(),
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
    fn expand_occur(&self, children: &[FormNodeId]) -> Vec<FormNodeId> {
        let mut expanded = Vec::new();
        for &child_id in children {
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

    /// Table layout: children are rows, cells fill column widths.
    fn layout_table(&self, children: &[FormNodeId], available: Size) -> Result<Vec<LayoutNode>> {
        // Simplified table: each child (row) gets full width, stacked vertically
        self.layout_tb(children, available)
    }

    /// Row layout: children fill horizontally within the row.
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
            FormNodeType::Field { value } => LayoutContent::Field {
                value: value.clone(),
            },
            FormNodeType::Draw { content } => LayoutContent::Text(content.clone()),
            _ => LayoutContent::None,
        };

        let child_available = Size {
            width: node.box_model.content_width().min(extent.width),
            height: node.box_model.content_height().min(extent.height),
        };

        let children = if node.children.is_empty() {
            Vec::new()
        } else {
            self.layout_children(&node.children, child_available, node.layout)?
        };

        Ok(LayoutNode {
            form_node: id,
            rect: Rect::new(x, y, extent.width, extent.height),
            name: node.name.clone(),
            content,
            children,
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

        // For growable dimensions, compute from children (with occur expansion)
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

/// Internal helper for page structure extraction.
#[derive(Debug)]
struct PageAreaInfo {
    content_areas: Vec<ContentArea>,
    page_width: f64,
    page_height: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::form::Occur;
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
}
