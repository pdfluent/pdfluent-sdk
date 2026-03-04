//! XFA Data DOM — two-node-type tree representing XML data.
//!
//! Per XFA 3.3 §3: The Data DOM has only two node types:
//! - `DataGroup`: grouping elements (contain child elements)
//! - `DataValue`: leaf elements and attributes (contain text/data)

use crate::error::{Result, XfaDomError};

/// Unique identifier for a node in the Data DOM arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DataNodeId(pub(crate) usize);

/// How null data values are serialized on output (XFA 3.3 §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NullType {
    /// Null values are not written to output XML.
    Exclude,
    /// Written as empty elements: `<element/>`.
    Empty,
    /// Written with `xsi:nil="true"` attribute.
    Xsi,
}

/// Whether a DataValue contains actual data or metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataContains {
    Data,
    MetaData,
}

/// A node in the XFA Data DOM.
#[derive(Debug, Clone)]
pub enum DataNode {
    DataGroup {
        name: String,
        namespace: Option<String>,
        children: Vec<DataNodeId>,
        is_record: bool,
        parent: Option<DataNodeId>,
    },
    DataValue {
        name: String,
        namespace: Option<String>,
        value: String,
        contains: DataContains,
        content_type: Option<String>,
        is_null: bool,
        null_type: NullType,
        parent: Option<DataNodeId>,
    },
}

impl DataNode {
    /// Returns the name of this node.
    pub fn name(&self) -> &str {
        match self {
            DataNode::DataGroup { name, .. } | DataNode::DataValue { name, .. } => name,
        }
    }

    /// Returns the parent node ID, if any.
    pub fn parent(&self) -> Option<DataNodeId> {
        match self {
            DataNode::DataGroup { parent, .. } | DataNode::DataValue { parent, .. } => *parent,
        }
    }

    /// Returns true if this is a DataGroup.
    pub fn is_group(&self) -> bool {
        matches!(self, DataNode::DataGroup { .. })
    }

    /// Returns true if this is a DataValue.
    pub fn is_value(&self) -> bool {
        matches!(self, DataNode::DataValue { .. })
    }

    /// Returns the string value for DataValue nodes, empty string for groups.
    pub fn value(&self) -> &str {
        match self {
            DataNode::DataValue { value, .. } => value,
            DataNode::DataGroup { .. } => "",
        }
    }
}

/// Arena-based Data DOM tree.
#[derive(Debug)]
pub struct DataDom {
    nodes: Vec<DataNode>,
    root: Option<DataNodeId>,
}

impl DataDom {
    /// Create an empty Data DOM.
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            root: None,
        }
    }

    /// Parse XML data into a Data DOM.
    ///
    /// Per XFA 3.3 §4: Elements containing only character data become DataValue nodes.
    /// Elements containing child elements become DataGroup nodes.
    /// Attributes become DataValue children of their element's DataGroup.
    pub fn from_xml(xml: &str) -> Result<Self> {
        let doc = roxmltree::Document::parse(xml)?;
        let mut dom = Self::new();
        let root_element = doc.root_element();
        let root_id = dom.build_from_xml_node(&root_element, None);
        dom.root = Some(root_id);
        Ok(dom)
    }

    fn build_from_xml_node(
        &mut self,
        node: &roxmltree::Node,
        parent: Option<DataNodeId>,
    ) -> DataNodeId {
        let name = node.tag_name().name().to_string();
        let namespace = node.tag_name().namespace().map(|s| s.to_string());

        let has_element_children = node.children().any(|c| c.is_element());

        if has_element_children {
            // DataGroup node
            let id = self.alloc(DataNode::DataGroup {
                name,
                namespace,
                children: Vec::new(),
                is_record: false,
                parent,
            });

            // Add attributes as DataValue children
            for attr in node.attributes() {
                // Skip namespace declarations and xsi:nil
                if attr.name() == "xmlns"
                    || attr.namespace() == Some("http://www.w3.org/2000/xmlns/")
                {
                    continue;
                }
                if attr.name() == "nil"
                    && attr.namespace() == Some("http://www.w3.org/2001/XMLSchema-instance")
                {
                    continue;
                }
                let attr_id = self.alloc(DataNode::DataValue {
                    name: attr.name().to_string(),
                    namespace: attr.namespace().map(|s| s.to_string()),
                    value: attr.value().to_string(),
                    contains: DataContains::Data,
                    content_type: None,
                    is_null: false,
                    null_type: NullType::Exclude,
                    parent: Some(id),
                });
                self.add_child(id, attr_id);
            }

            // Add element children
            for child in node.children().filter(|c| c.is_element()) {
                let child_id = self.build_from_xml_node(&child, Some(id));
                self.add_child(id, child_id);
            }

            id
        } else {
            // DataValue node (leaf element)
            let text = node.text().map(|s| s.to_string()).unwrap_or_default();

            let is_null = node.attribute(("http://www.w3.org/2001/XMLSchema-instance", "nil"))
                == Some("true");

            self.alloc(DataNode::DataValue {
                name,
                namespace,
                value: text,
                contains: DataContains::Data,
                content_type: node.attribute("contentType").map(|s| s.to_string()),
                is_null,
                null_type: if is_null {
                    NullType::Xsi
                } else {
                    NullType::Exclude
                },
                parent,
            })
        }
    }

    /// Allocate a new node in the arena (does not attach to any parent).
    pub fn alloc(&mut self, node: DataNode) -> DataNodeId {
        let id = DataNodeId(self.nodes.len());
        self.nodes.push(node);
        id
    }

    fn add_child(&mut self, parent: DataNodeId, child: DataNodeId) {
        if let DataNode::DataGroup { children, .. } = &mut self.nodes[parent.0] {
            children.push(child);
        }
    }

    /// Get a node by ID.
    pub fn get(&self, id: DataNodeId) -> Option<&DataNode> {
        self.nodes.get(id.0)
    }

    /// Get a mutable reference to a node by ID.
    pub fn get_mut(&mut self, id: DataNodeId) -> Option<&mut DataNode> {
        self.nodes.get_mut(id.0)
    }

    /// Get the root node ID.
    pub fn root(&self) -> Option<DataNodeId> {
        self.root
    }

    /// Get children of a DataGroup node.
    pub fn children(&self, id: DataNodeId) -> &[DataNodeId] {
        match self.get(id) {
            Some(DataNode::DataGroup { children, .. }) => children,
            _ => &[],
        }
    }

    /// Find direct children with a given name.
    pub fn children_by_name(&self, parent: DataNodeId, name: &str) -> Vec<DataNodeId> {
        self.children(parent)
            .iter()
            .filter(|&&child_id| self.get(child_id).is_some_and(|n| n.name() == name))
            .copied()
            .collect()
    }

    /// Get the value of a DataValue node.
    pub fn value(&self, id: DataNodeId) -> Result<&str> {
        match self.get(id) {
            Some(DataNode::DataValue { value, .. }) => Ok(value),
            Some(node) => Err(XfaDomError::InvalidNodeType {
                expected: "DataValue",
                got: format!("DataGroup({})", node.name()),
            }),
            None => Err(XfaDomError::NodeNotFound(format!("DataNodeId({})", id.0))),
        }
    }

    /// Set the value of a DataValue node.
    pub fn set_value(&mut self, id: DataNodeId, new_value: String) -> Result<()> {
        match self.get_mut(id) {
            Some(DataNode::DataValue { value, is_null, .. }) => {
                *value = new_value;
                *is_null = false;
                Ok(())
            }
            Some(node) => Err(XfaDomError::InvalidNodeType {
                expected: "DataValue",
                got: format!("DataGroup({})", node.name()),
            }),
            None => Err(XfaDomError::NodeNotFound(format!("DataNodeId({})", id.0))),
        }
    }

    // ── CRUD: Create ──────────────────────────────────────────

    /// Create a new DataGroup node and attach it to a parent.
    pub fn create_group(&mut self, parent: DataNodeId, name: &str) -> Result<DataNodeId> {
        self.ensure_group(parent)?;
        let id = self.alloc(DataNode::DataGroup {
            name: name.to_string(),
            namespace: None,
            children: Vec::new(),
            is_record: false,
            parent: Some(parent),
        });
        self.add_child(parent, id);
        Ok(id)
    }

    /// Create a new DataValue node and attach it to a parent.
    pub fn create_value(
        &mut self,
        parent: DataNodeId,
        name: &str,
        value: &str,
    ) -> Result<DataNodeId> {
        self.ensure_group(parent)?;
        let id = self.alloc(DataNode::DataValue {
            name: name.to_string(),
            namespace: None,
            value: value.to_string(),
            contains: DataContains::Data,
            content_type: None,
            is_null: false,
            null_type: NullType::Exclude,
            parent: Some(parent),
        });
        self.add_child(parent, id);
        Ok(id)
    }

    // ── CRUD: Delete / Remove ────────────────────────────────

    /// Remove a child from its parent's children list (does not deallocate).
    pub fn remove_child(&mut self, parent: DataNodeId, child: DataNodeId) -> Result<()> {
        self.ensure_group(parent)?;
        if let DataNode::DataGroup { children, .. } = &mut self.nodes[parent.0] {
            if let Some(pos) = children.iter().position(|&id| id == child) {
                children.remove(pos);
            }
        }
        // Clear the child's parent reference
        match &mut self.nodes[child.0] {
            DataNode::DataGroup { parent, .. } | DataNode::DataValue { parent, .. } => {
                *parent = None;
            }
        }
        Ok(())
    }

    /// Remove a node from its parent and mark it as orphaned.
    pub fn detach(&mut self, id: DataNodeId) -> Result<()> {
        let parent_id = self
            .get(id)
            .ok_or_else(|| XfaDomError::NodeNotFound(format!("DataNodeId({})", id.0)))?
            .parent();
        if let Some(pid) = parent_id {
            self.remove_child(pid, id)?;
        }
        Ok(())
    }

    // ── CRUD: Update (rename) ────────────────────────────────

    /// Rename a node.
    pub fn rename(&mut self, id: DataNodeId, new_name: &str) -> Result<()> {
        match self.get_mut(id) {
            Some(DataNode::DataGroup { name, .. }) | Some(DataNode::DataValue { name, .. }) => {
                *name = new_name.to_string();
                Ok(())
            }
            None => Err(XfaDomError::NodeNotFound(format!("DataNodeId({})", id.0))),
        }
    }

    // ── Insert at position ───────────────────────────────────

    /// Insert a child at a specific index in the parent's children list.
    pub fn insert_child_at(
        &mut self,
        parent: DataNodeId,
        index: usize,
        child: DataNodeId,
    ) -> Result<()> {
        self.ensure_group(parent)?;
        // Update child's parent reference
        match &mut self.nodes[child.0] {
            DataNode::DataGroup { parent: p, .. } | DataNode::DataValue { parent: p, .. } => {
                *p = Some(parent);
            }
        }
        if let DataNode::DataGroup { children, .. } = &mut self.nodes[parent.0] {
            let idx = index.min(children.len());
            children.insert(idx, child);
        }
        Ok(())
    }

    // ── Move node ────────────────────────────────────────────

    /// Move a node from its current parent to a new parent.
    pub fn move_node(&mut self, node: DataNodeId, new_parent: DataNodeId) -> Result<()> {
        self.ensure_group(new_parent)?;
        self.detach(node)?;
        match &mut self.nodes[node.0] {
            DataNode::DataGroup { parent, .. } | DataNode::DataValue { parent, .. } => {
                *parent = Some(new_parent);
            }
        }
        self.add_child(new_parent, node);
        Ok(())
    }

    // ── Serialisation ────────────────────────────────────────

    /// Serialize the DOM back to XML.
    pub fn to_xml(&self) -> String {
        let mut out = String::new();
        if let Some(root) = self.root {
            self.write_xml_node(root, &mut out, 0);
        }
        out
    }

    fn write_xml_node(&self, id: DataNodeId, out: &mut String, depth: usize) {
        let node = match self.get(id) {
            Some(n) => n,
            None => return,
        };
        let indent = "  ".repeat(depth);
        match node {
            DataNode::DataGroup { name, children, .. } => {
                if children.is_empty() {
                    out.push_str(&format!("{indent}<{name}/>\n"));
                } else {
                    out.push_str(&format!("{indent}<{name}>\n"));
                    for &child in children {
                        self.write_xml_node(child, out, depth + 1);
                    }
                    out.push_str(&format!("{indent}</{name}>\n"));
                }
            }
            DataNode::DataValue {
                name,
                value,
                is_null,
                ..
            } => {
                if *is_null {
                    out.push_str(&format!("{indent}<{name} xsi:nil=\"true\"/>\n"));
                } else if value.is_empty() {
                    out.push_str(&format!("{indent}<{name}/>\n"));
                } else {
                    let escaped = xml_escape(value);
                    out.push_str(&format!("{indent}<{name}>{escaped}</{name}>\n"));
                }
            }
        }
    }

    // ── Helpers ──────────────────────────────────────────────

    fn ensure_group(&self, id: DataNodeId) -> Result<()> {
        match self.get(id) {
            Some(DataNode::DataGroup { .. }) => Ok(()),
            Some(node) => Err(XfaDomError::InvalidNodeType {
                expected: "DataGroup",
                got: format!("DataValue({})", node.name()),
            }),
            None => Err(XfaDomError::NodeNotFound(format!("DataNodeId({})", id.0))),
        }
    }

    /// Total number of nodes in the arena.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Returns true if the DOM is empty.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

impl Default for DataDom {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_data() {
        let xml = r#"<data>
            <Receipt>
                <Description>Widget A</Description>
                <Units>2</Units>
                <Unit_Price>10.00</Unit_Price>
            </Receipt>
        </data>"#;

        let dom = DataDom::from_xml(xml).unwrap();
        let root = dom.root().unwrap();
        assert_eq!(dom.get(root).unwrap().name(), "data");

        // data -> Receipt
        let receipts = dom.children_by_name(root, "Receipt");
        assert_eq!(receipts.len(), 1);
        let receipt = receipts[0];
        assert!(dom.get(receipt).unwrap().is_group());

        // Receipt -> Description (DataValue)
        let descs = dom.children_by_name(receipt, "Description");
        assert_eq!(descs.len(), 1);
        assert_eq!(dom.value(descs[0]).unwrap(), "Widget A");

        // Receipt -> Units
        let units = dom.children_by_name(receipt, "Units");
        assert_eq!(dom.value(units[0]).unwrap(), "2");
    }

    #[test]
    fn parse_attributes_as_data_values() {
        let xml = r#"<data>
            <item id="42" type="widget">
                <name>Test</name>
            </item>
        </data>"#;

        let dom = DataDom::from_xml(xml).unwrap();
        let root = dom.root().unwrap();
        let items = dom.children_by_name(root, "item");
        assert_eq!(items.len(), 1);

        // Attributes become DataValue children
        let id_nodes = dom.children_by_name(items[0], "id");
        assert_eq!(id_nodes.len(), 1);
        assert_eq!(dom.value(id_nodes[0]).unwrap(), "42");

        let type_nodes = dom.children_by_name(items[0], "type");
        assert_eq!(type_nodes.len(), 1);
        assert_eq!(dom.value(type_nodes[0]).unwrap(), "widget");
    }

    #[test]
    fn parse_repeated_elements() {
        let xml = r#"<data>
            <Receipt>
                <Detail><Description>A</Description></Detail>
                <Detail><Description>B</Description></Detail>
                <Detail><Description>C</Description></Detail>
            </Receipt>
        </data>"#;

        let dom = DataDom::from_xml(xml).unwrap();
        let root = dom.root().unwrap();
        let receipt = dom.children_by_name(root, "Receipt")[0];
        let details = dom.children_by_name(receipt, "Detail");
        assert_eq!(details.len(), 3);

        let descs: Vec<&str> = details
            .iter()
            .map(|&d| {
                let desc = dom.children_by_name(d, "Description")[0];
                dom.value(desc).unwrap()
            })
            .collect();
        assert_eq!(descs, vec!["A", "B", "C"]);
    }

    #[test]
    fn parse_null_value() {
        let xml = r#"<data xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
            <field xsi:nil="true"/>
        </data>"#;

        let dom = DataDom::from_xml(xml).unwrap();
        let root = dom.root().unwrap();
        let fields = dom.children_by_name(root, "field");
        assert_eq!(fields.len(), 1);

        if let DataNode::DataValue {
            is_null, null_type, ..
        } = dom.get(fields[0]).unwrap()
        {
            assert!(*is_null);
            assert_eq!(*null_type, NullType::Xsi);
        } else {
            panic!("Expected DataValue");
        }
    }

    #[test]
    fn set_value_works() {
        let xml = r#"<data><field>old</field></data>"#;
        let mut dom = DataDom::from_xml(xml).unwrap();
        let root = dom.root().unwrap();
        let field = dom.children_by_name(root, "field")[0];
        assert_eq!(dom.value(field).unwrap(), "old");

        dom.set_value(field, "new".to_string()).unwrap();
        assert_eq!(dom.value(field).unwrap(), "new");
    }

    #[test]
    fn parent_references() {
        let xml = r#"<data><child>val</child></data>"#;
        let dom = DataDom::from_xml(xml).unwrap();
        let root = dom.root().unwrap();
        let child = dom.children_by_name(root, "child")[0];

        assert_eq!(dom.get(child).unwrap().parent(), Some(root));
        assert_eq!(dom.get(root).unwrap().parent(), None);
    }

    #[test]
    fn create_group_and_value() {
        let xml = r#"<data><placeholder>x</placeholder></data>"#;
        let mut dom = DataDom::from_xml(xml).unwrap();
        let root = dom.root().unwrap();

        let grp = dom.create_group(root, "Invoice").unwrap();
        assert!(dom.get(grp).unwrap().is_group());
        assert_eq!(dom.get(grp).unwrap().parent(), Some(root));

        let val = dom.create_value(grp, "Total", "99.50").unwrap();
        assert_eq!(dom.value(val).unwrap(), "99.50");
        assert_eq!(dom.get(val).unwrap().parent(), Some(grp));

        assert_eq!(dom.children(root).len(), 2); // placeholder + Invoice
        assert_eq!(dom.children(grp).len(), 1);
    }

    #[test]
    fn create_on_value_node_fails() {
        let xml = r#"<data><leaf>x</leaf></data>"#;
        let mut dom = DataDom::from_xml(xml).unwrap();
        let root = dom.root().unwrap();
        let leaf = dom.children_by_name(root, "leaf")[0];

        assert!(dom.create_group(leaf, "child").is_err());
        assert!(dom.create_value(leaf, "child", "v").is_err());
    }

    #[test]
    fn remove_child_works() {
        let xml = r#"<data><a>1</a><b>2</b><c>3</c></data>"#;
        let mut dom = DataDom::from_xml(xml).unwrap();
        let root = dom.root().unwrap();
        assert_eq!(dom.children(root).len(), 3);

        let b = dom.children_by_name(root, "b")[0];
        dom.remove_child(root, b).unwrap();
        assert_eq!(dom.children(root).len(), 2);
        assert!(dom.get(b).unwrap().parent().is_none());

        let names: Vec<&str> = dom
            .children(root)
            .iter()
            .map(|&id| dom.get(id).unwrap().name())
            .collect();
        assert_eq!(names, vec!["a", "c"]);
    }

    #[test]
    fn detach_node() {
        let xml = r#"<data><item>val</item></data>"#;
        let mut dom = DataDom::from_xml(xml).unwrap();
        let root = dom.root().unwrap();
        let item = dom.children_by_name(root, "item")[0];

        dom.detach(item).unwrap();
        assert_eq!(dom.children(root).len(), 0);
        assert!(dom.get(item).unwrap().parent().is_none());
    }

    #[test]
    fn rename_node() {
        let xml = r#"<data><old>val</old></data>"#;
        let mut dom = DataDom::from_xml(xml).unwrap();
        let root = dom.root().unwrap();
        let node = dom.children_by_name(root, "old")[0];

        dom.rename(node, "new").unwrap();
        assert_eq!(dom.get(node).unwrap().name(), "new");
        assert_eq!(dom.children_by_name(root, "new").len(), 1);
        assert_eq!(dom.children_by_name(root, "old").len(), 0);
    }

    #[test]
    fn insert_child_at_position() {
        let xml = r#"<data><a>1</a><c>3</c></data>"#;
        let mut dom = DataDom::from_xml(xml).unwrap();
        let root = dom.root().unwrap();

        let b = dom.alloc(DataNode::DataValue {
            name: "b".to_string(),
            namespace: None,
            value: "2".to_string(),
            contains: DataContains::Data,
            content_type: None,
            is_null: false,
            null_type: NullType::Exclude,
            parent: None,
        });
        dom.insert_child_at(root, 1, b).unwrap();

        let names: Vec<&str> = dom
            .children(root)
            .iter()
            .map(|&id| dom.get(id).unwrap().name())
            .collect();
        assert_eq!(names, vec!["a", "b", "c"]);
        assert_eq!(dom.get(b).unwrap().parent(), Some(root));
    }

    #[test]
    fn move_node_between_parents() {
        let xml = r#"<data><src><item>val</item></src><dst><placeholder>x</placeholder></dst></data>"#;
        let mut dom = DataDom::from_xml(xml).unwrap();
        let root = dom.root().unwrap();
        let src = dom.children_by_name(root, "src")[0];
        let dst = dom.children_by_name(root, "dst")[0];
        let item = dom.children_by_name(src, "item")[0];

        dom.move_node(item, dst).unwrap();
        assert_eq!(dom.children(src).len(), 0);
        assert_eq!(dom.children(dst).len(), 2); // placeholder + item
        assert_eq!(dom.get(item).unwrap().parent(), Some(dst));
    }

    #[test]
    fn to_xml_roundtrip() {
        let mut dom = DataDom::new();
        let root = dom.alloc(DataNode::DataGroup {
            name: "data".to_string(),
            namespace: None,
            children: Vec::new(),
            is_record: false,
            parent: None,
        });
        dom.root = Some(root);

        let inv = dom.create_group(root, "Invoice").unwrap();
        dom.create_value(inv, "Total", "112.50").unwrap();
        dom.create_value(inv, "Tax", "23.63").unwrap();

        let xml = dom.to_xml();
        assert!(xml.contains("<Invoice>"));
        assert!(xml.contains("<Total>112.50</Total>"));
        assert!(xml.contains("<Tax>23.63</Tax>"));
        assert!(xml.contains("</Invoice>"));
    }

    #[test]
    fn to_xml_escapes_special_chars() {
        let mut dom = DataDom::new();
        let root = dom.alloc(DataNode::DataGroup {
            name: "data".to_string(),
            namespace: None,
            children: Vec::new(),
            is_record: false,
            parent: None,
        });
        dom.root = Some(root);
        dom.create_value(root, "note", "A & B < C").unwrap();

        let xml = dom.to_xml();
        assert!(xml.contains("A &amp; B &lt; C"));
    }
}
