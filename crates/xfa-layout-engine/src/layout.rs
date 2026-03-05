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
    pub fn layout(&self, root: FormNodeId) -> Result<LayoutDom> {
        let root_node = self.form.get(root);

        // Find the page set and content subforms
        let (page_areas, content_nodes) = self.extract_page_structure(root_node)?;

        let mut pages = Vec::new();

        if page_areas.is_empty() {
            // No explicit page structure — use root's dimensions or defaults
            let page_w = root_node.box_model.width.unwrap_or(612.0);
            let page_h = root_node.box_model.height.unwrap_or(792.0);
            let area = ContentArea {
                name: String::new(),
                x: 0.0,
                y: 0.0,
                width: page_w,
                height: page_h,
            };
            let page = self.layout_content_on_page(
                &area,
                page_w,
                page_h,
                &content_nodes,
                root_node.layout,
            )?;
            pages.push(page);
        } else {
            // Layout content across page areas
            let mut remaining = content_nodes.clone();
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
            // If there's remaining content, place it on the last page type
            if !remaining.is_empty() {
                let last_pa = page_areas.last().unwrap();
                let ca = last_pa
                    .content_areas
                    .first()
                    .ok_or(LayoutError::NoMatchingPageArea)?;
                let page = self.layout_content_on_page(
                    ca,
                    last_pa.page_width,
                    last_pa.page_height,
                    &remaining,
                    LayoutStrategy::TopToBottom,
                )?;
                pages.push(page);
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

        let mut y_cursor = 0.0;
        let mut placed_count = 0;

        for &child_id in content_ids {
            let child = self.form.get(child_id);
            let child_size = self.compute_extent(child_id);

            if y_cursor + child_size.height > content_area.height && placed_count > 0 {
                break;
            }

            let node = self.layout_single_node(child_id, child, 0.0, y_cursor)?;
            let mut offset_node = node;
            offset_node.rect.x += content_area.x;
            offset_node.rect.y += content_area.y;
            page.nodes.push(offset_node);

            y_cursor += child_size.height;
            placed_count += 1;
        }

        let remaining = content_ids[placed_count..].to_vec();
        Ok((page, remaining))
    }

    /// Layout children within available space using the given strategy.
    fn layout_children(
        &self,
        children: &[FormNodeId],
        available: Size,
        strategy: LayoutStrategy,
    ) -> Result<Vec<LayoutNode>> {
        match strategy {
            LayoutStrategy::Positioned => self.layout_positioned(children),
            LayoutStrategy::TopToBottom => self.layout_tb(children, available),
            LayoutStrategy::LeftToRightTB => self.layout_lr_tb(children, available),
            LayoutStrategy::RightToLeftTB => self.layout_rl_tb(children, available),
            LayoutStrategy::Table => self.layout_table(children, available),
            LayoutStrategy::Row => self.layout_row(children, available),
        }
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

        // For growable dimensions, compute from children
        let mut content_size = Size::default();

        if !node.children.is_empty() {
            match node.layout {
                LayoutStrategy::TopToBottom => {
                    for &child_id in &node.children {
                        let cs = self.compute_extent(child_id);
                        content_size.width = content_size.width.max(cs.width);
                        content_size.height += cs.height;
                    }
                }
                LayoutStrategy::LeftToRightTB | LayoutStrategy::Row => {
                    for &child_id in &node.children {
                        let cs = self.compute_extent(child_id);
                        content_size.width += cs.width;
                        content_size.height = content_size.height.max(cs.height);
                    }
                }
                _ => {
                    // Positioned: envelope all children
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
        });

        let engine = LayoutEngine::new(&tree);
        let result = engine.layout(root).unwrap();

        let page = &result.pages[0];
        // F1 has minW=200, no content so uses 200
        assert_eq!(page.nodes[0].rect.width, 200.0);
        // F2 at x=200
        assert_eq!(page.nodes[1].rect.x, 200.0);
    }
}
