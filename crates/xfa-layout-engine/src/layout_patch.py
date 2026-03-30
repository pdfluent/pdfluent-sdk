import sys
content = open("src/layout.rs").read()

# 1. Enhance compute_extent to calculate proper row heights
row_height_fn = """
    fn compute_table_row_height(&self, row_id: FormNodeId, col_widths: &[f64]) -> f64 {
        let row = self.form.get(row_id);
        let mut max_cell_height = 0.0_f64;
        let mut col_idx = 0usize;
        let expanded = self.expand_occur(&row.children);
        for &cell_id in &expanded {
            if col_idx >= col_widths.len() { break; }
            let cell = self.form.get(cell_id);
            let span = cell.col_span;
            
            let cell_width = if span == -1 {
                col_widths[col_idx..].iter().sum::<f64>()
            } else {
                let span_count = (span.max(1) as usize).min(col_widths.len().saturating_sub(col_idx));
                col_widths[col_idx..col_idx + span_count].iter().sum::<f64>()
            };
            
            let cell_available = Size { width: cell_width, height: f64::MAX };
            let cell_extent = self.compute_extent_with_available(cell_id, Some(cell_available));
            max_cell_height = max_cell_height.max(cell_extent.height);
            
            if span == -1 {
                col_idx = col_widths.len();
            } else {
                col_idx += span.max(1) as usize;
            }
        }
        max_cell_height
    }
"""

content = content.replace("fn compute_extent_with_available", row_height_fn + "\n    fn compute_extent_with_available")

old_table_extent = """                LayoutStrategy::Table => {
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
                }"""
new_table_extent = """                LayoutStrategy::Table => {
                    let avail_w = available.map(|a| a.width).unwrap_or(f64::MAX);
                    let col_widths = self.resolve_column_widths(node, avail_w);
                    let table_width: f64 = col_widths.iter().sum();
                    content_size.width = content_size.width.max(table_width);
                    for &row_id in &expanded {
                        content_size.height += self.compute_table_row_height(row_id, &col_widths);
                    }
                }"""
content = content.replace(old_table_extent, new_table_extent)

# 2. Extract layout_single_row_with_widths from layout_table_rows
single_row_fn = """
    fn layout_single_row_with_widths(
        &self,
        row_id: FormNodeId,
        available: Size,
        col_widths: &[f64],
        y_cursor: f64,
    ) -> Result<LayoutNode> {
        let row_node = self.form.get(row_id);
        let row_children = self.expand_occur(&row_node.children);
        let mut cells = Vec::new();
        let mut x_cursor = 0.0;
        let mut col_idx = 0usize;
        let mut max_cell_height = 0.0_f64;

        for &cell_id in &row_children {
            if col_idx >= col_widths.len() { break; }
            let cell = self.form.get(cell_id);
            let span = cell.col_span;
            let cell_width = if span == -1 {
                col_widths[col_idx..].iter().sum::<f64>()
            } else {
                let span_count = (span.max(1) as usize).min(col_widths.len().saturating_sub(col_idx));
                col_widths[col_idx..col_idx + span_count].iter().sum::<f64>()
            };

            let cell_available = Size { width: cell_width, height: available.height - y_cursor };
            let cell_height = self.compute_extent_with_available(cell_id, Some(cell_available)).height;
            let cell_extent = Size { width: cell_width, height: cell_height };

            let cell_node = self.layout_single_node_with_extent(cell_id, cell, x_cursor, 0.0, cell_extent)?;
            max_cell_height = max_cell_height.max(cell_extent.height);
            cells.push(cell_node);

            x_cursor += cell_width;
            if span == -1 {
                col_idx = col_widths.len();
            } else {
                col_idx += span.max(1) as usize;
            }
        }

        for cell in &mut cells {
            cell.rect.height = max_cell_height;
        }

        Ok(LayoutNode {
            form_node: row_id,
            rect: Rect::new(0.0, y_cursor, available.width, max_cell_height),
            name: row_node.name.clone(),
            content: LayoutContent::None,
            children: cells,
            style: self.form.meta(row_id).style.clone(),
        })
    }
"""
content = content.replace("    fn layout_table_rows(", single_row_fn + "\n    fn layout_table_rows(")

old_table_rows = """    fn layout_table_rows(
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

            y_cursor += max_cell_height;
            nodes.push(row_layout);
        }

        Ok(nodes)
    }"""
new_table_rows = """    fn layout_table_rows(
        &self,
        children: &[FormNodeId],
        available: Size,
        col_widths: &[f64],
    ) -> Result<Vec<LayoutNode>> {
        let expanded = self.expand_occur(children);
        let mut nodes = Vec::new();
        let mut y_cursor = 0.0;
        for &row_id in &expanded {
            let row_layout = self.layout_single_row_with_widths(row_id, available, col_widths, y_cursor)?;
            y_cursor += row_layout.rect.height;
            nodes.push(row_layout);
        }
        Ok(nodes)
    }"""
content = content.replace(old_table_rows, new_table_rows)

# 3. Fix split_tb_node to layout the row with column widths
old_child_layout = """            let child_node = self.layout_single_node(child_id, child, 0.0, child_y)?;"""
new_child_layout = """            let child_node = if let Some(ref cw) = table_col_widths {
                self.layout_single_row_with_widths(child_id, available, cw, child_y)?
            } else {
                self.layout_single_node(child_id, child, 0.0, child_y)?
            };"""
content = content.replace(old_child_layout, new_child_layout)

content = content.replace("""        let mut last_valid_y = 0.0_f64;""", """        let mut last_valid_y = 0.0_f64;

        let table_col_widths = if node.layout == LayoutStrategy::Table {
            Some(self.resolve_column_widths(node, available.width))
        } else {
            None
        };""")

old_child_size = """for (i, &child_id) in expanded_children.iter().enumerate() {
            let child = self.form.get(child_id);
            let child_size = self.compute_extent(child_id);"""
new_child_size = """for (i, &child_id) in expanded_children.iter().enumerate() {
            let child = self.form.get(child_id);
            let child_size = if let Some(ref cw) = table_col_widths {
                Size { width: available.width, height: self.compute_table_row_height(child_id, cw) }
            } else {
                self.compute_extent(child_id)
            };"""
content = content.replace(old_child_size, new_child_size)

open("src/layout.rs", "w").write(content)
