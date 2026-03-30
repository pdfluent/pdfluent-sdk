import sys

content = open("src/layout.rs").read()

# 1. Thread override_children in layout_single_node_with_extent
content = content.replace(
"""    fn layout_single_node_with_extent(
        &self,
        id: FormNodeId,
        node: &FormNode,
        x: f64,
        y: f64,
        extent: Size,
    ) -> Result<LayoutNode> {""",
"""    fn layout_single_node_with_extent(
        &self,
        id: FormNodeId,
        node: &FormNode,
        x: f64,
        y: f64,
        extent: Size,
        override_children: Option<&[FormNodeId]>,
    ) -> Result<LayoutNode> {"""
)

# Fix calls to layout_single_node_with_extent to pass None by default, except where we have override_children
content = content.replace("self.layout_single_node_with_extent(id, node, x, y, extent)", "self.layout_single_node_with_extent(id, node, x, y, extent, None)")
content = content.replace("self.layout_single_node_with_extent(cell_id, cell, x_cursor, 0.0, cell_extent)?;", "self.layout_single_node_with_extent(cell_id, cell, x_cursor, 0.0, cell_extent, None)?;")
# specifically in layout_content_fitting
content = content.replace("""                    let node = self.layout_single_node_with_extent(
                        child_id, child, 0.0, y_cursor, child_size,
                    )?;""", """                    let node = self.layout_single_node_with_extent(
                        child_id, child, 0.0, y_cursor, child_size, qn.override_children.as_deref()
                    )?;""")
content = content.replace("""                    let child_node = self.layout_single_node_with_extent(
                        child_id,
                        child,
                        0.0,
                        y_cursor,
                        child_size,
                    )?;""", """                    let child_node = self.layout_single_node_with_extent(
                        child_id,
                        child,
                        0.0,
                        y_cursor,
                        child_size,
                        qn.override_children.as_deref()
                    )?;""")
content = content.replace("""                let default_node =
                    self.layout_single_node_with_extent(child_id, child, 0.0, y_cursor, child_size)?;""", """                let default_node =
                    self.layout_single_node_with_extent(child_id, child, 0.0, y_cursor, child_size, qn.override_children.as_deref())?;""")

# 2. In layout_single_node_with_extent, use override_children
old_children = """        let children = if node.children.is_empty() {
            Vec::new()
        } else if node.layout == LayoutStrategy::Table {
            // Table layout: resolve column widths from the parent node,
            // then distribute cells across rows.
            let col_widths = self.resolve_column_widths(node, child_available.width);
            self.layout_table_rows(&node.children, child_available, &col_widths)?
        } else {
            self.layout_children(&node.children, child_available, node.layout)?
        };"""
new_children = """        let node_children_ref = override_children.unwrap_or(&node.children);
        let children = if node_children_ref.is_empty() {
            Vec::new()
        } else if node.layout == LayoutStrategy::Table {
            let col_widths = self.resolve_column_widths(node, child_available.width);
            self.layout_table_rows(node_children_ref, child_available, &col_widths)?
        } else {
            self.layout_children(node_children_ref, child_available, node.layout)?
        };"""
content = content.replace(old_children, new_children)

# 3. Thread override_children in compute_extent_with_available
content = content.replace("""    fn compute_extent_with_available(&self, id: FormNodeId, available: Option<Size>) -> Size {""", """    fn compute_extent_with_available(&self, id: FormNodeId, available: Option<Size>, override_children: Option<&[FormNodeId]>) -> Size {""")
content = content.replace("self.compute_extent_with_available(id, None)", "self.compute_extent_with_available(id, None, None)")
content = content.replace("self.compute_extent_with_available(cell_id, Some(cell_available))", "self.compute_extent_with_available(cell_id, Some(cell_available), None)")

# In layout_content_fitting
content = content.replace("let child_size = self.compute_extent_with_available(child_id, Some(available));", "let child_size = self.compute_extent_with_available(child_id, Some(available), qn.override_children.as_deref());")

# Inside compute_extent_with_available
content = content.replace("""        if !node.children.is_empty() {
            let expanded = self.expand_occur(&node.children);""", """        let target_children = override_children.unwrap_or(&node.children);
        if !target_children.is_empty() {
            let expanded = self.expand_occur(target_children);""")

# 4. Thread override_children in split_tb_node
content = content.replace("""    fn split_tb_node(
        &self,
        id: FormNodeId,
        y_offset: f64,
        remaining_height: f64,
        available: Size,
    ) -> Result<(LayoutNode, Vec<FormNodeId>)> {""", """    fn split_tb_node(
        &self,
        id: FormNodeId,
        y_offset: f64,
        remaining_height: f64,
        available: Size,
        override_children: Option<&[FormNodeId]>,
    ) -> Result<(LayoutNode, Vec<FormNodeId>)> {""")

content = content.replace("""        let node = self.form.get(id);
        let expanded_children = self.expand_occur(&node.children);""", """        let node = self.form.get(id);
        let expanded_children = self.expand_occur(override_children.unwrap_or(&node.children));""")

content = content.replace("self.split_tb_node(child_id, y_cursor, remaining_height, available)?", "self.split_tb_node(child_id, y_cursor, remaining_height, available, qn.override_children.as_deref())?")
content = content.replace("self.split_tb_node(child_id, y_cursor, content_height, available)?", "self.split_tb_node(child_id, y_cursor, content_height, available, qn.override_children.as_deref())?")

open("src/layout.rs", "w").write(content)

