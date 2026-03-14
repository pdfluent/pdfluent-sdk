//! Arena-based field tree for AcroForm fields (B.1).

use crate::flags::FieldFlags;

/// Identifier for a node in the field tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FieldId(pub(crate) usize);

/// The type of a form field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    /// Text input field (/Tx).
    Text,
    /// Button field (/Btn) — checkbox, radio, or push button.
    Button,
    /// Choice field (/Ch) — combo box or list box.
    Choice,
    /// Digital signature field (/Sig).
    Signature,
}

/// Text alignment (quadding).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Quadding {
    /// Left-justified (0).
    #[default]
    Left,
    /// Centered (1).
    Center,
    /// Right-justified (2).
    Right,
}

/// A field value.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    /// Text or name value (string).
    Text(String),
    /// Array of selected values (choice fields).
    StringArray(Vec<String>),
}

/// An option in a choice field (/Opt entry).
#[derive(Debug, Clone)]
pub struct ChoiceOption {
    /// Export value (sent when form is submitted).
    pub export: String,
    /// Display value (shown in the field).
    pub display: String,
}

/// Widget appearance characteristics (/MK dictionary).
#[derive(Debug, Clone, Default)]
pub struct MkDict {
    /// Border color (/BC).
    pub border_color: Option<Vec<f32>>,
    /// Background color (/BG).
    pub background_color: Option<Vec<f32>>,
    /// Normal caption (/CA).
    pub caption: Option<String>,
    /// Rollover caption (/RC).
    pub rollover_caption: Option<String>,
    /// Alternate caption (/AC).
    pub alternate_caption: Option<String>,
    /// Icon/caption layout (/TP).
    pub text_position: Option<u32>,
    /// Rotation (/R).
    pub rotation: Option<u32>,
}

/// Border style (/BS dictionary).
#[derive(Debug, Clone)]
pub struct BorderStyle {
    /// Border width (/W).
    pub width: f32,
    /// Border style (/S): S=solid, D=dashed, B=beveled, I=inset, U=underline.
    pub style: u8,
}

impl Default for BorderStyle {
    fn default() -> Self {
        Self {
            width: 1.0,
            style: b'S',
        }
    }
}

/// A single node in the AcroForm field tree.
#[derive(Debug, Clone)]
pub struct FieldNode {
    /// Partial field name (/T).
    pub partial_name: String,
    /// Alternate field name (/TU).
    pub alternate_name: Option<String>,
    /// Mapping name (/TM).
    pub mapping_name: Option<String>,
    /// Field type (/FT): may be inherited from parent.
    pub field_type: Option<FieldType>,
    /// Field flags (/Ff).
    pub flags: FieldFlags,
    /// Current value (/V).
    pub value: Option<FieldValue>,
    /// Default value (/DV).
    pub default_value: Option<FieldValue>,
    /// Default appearance string (/DA).
    pub default_appearance: Option<String>,
    /// Quadding / text alignment (/Q).
    pub quadding: Option<Quadding>,
    /// Maximum length (/MaxLen) for text fields.
    pub max_len: Option<u32>,
    /// Options (/Opt) for choice fields.
    pub options: Vec<ChoiceOption>,
    /// Top index (/TI) for list box scroll position.
    pub top_index: Option<u32>,
    /// Widget rectangle (/Rect).
    pub rect: Option<[f32; 4]>,
    /// Current appearance state (/AS).
    pub appearance_state: Option<String>,
    /// Page index (0-based) this widget appears on.
    pub page_index: Option<usize>,
    /// Parent node in the field tree.
    pub parent: Option<FieldId>,
    /// Child nodes.
    pub children: Vec<FieldId>,
    /// PDF object identifier (obj_number, gen_number).
    pub object_id: Option<(i32, i32)>,
    /// Whether this field has /AA (additional actions).
    pub has_actions: bool,
    /// Widget appearance characteristics (/MK).
    pub mk: Option<MkDict>,
    /// Border style (/BS).
    pub border_style: Option<BorderStyle>,
}

/// The complete AcroForm field tree.
#[derive(Debug)]
pub struct FieldTree {
    nodes: Vec<FieldNode>,
    /// Calculation order (/CO) — field IDs in evaluation order.
    pub calculation_order: Vec<FieldId>,
    /// Document-level default appearance (/DA).
    pub document_da: Option<String>,
    /// Document-level quadding (/Q).
    pub document_quadding: Option<Quadding>,
    /// Whether /NeedAppearances is set.
    pub need_appearances: bool,
    /// SigFlags value.
    pub sig_flags: u32,
}

impl FieldTree {
    /// Create an empty field tree.
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            calculation_order: Vec::new(),
            document_da: None,
            document_quadding: None,
            need_appearances: false,
            sig_flags: 0,
        }
    }

    /// Allocate a new node, returning its ID.
    pub fn alloc(&mut self, node: FieldNode) -> FieldId {
        let id = FieldId(self.nodes.len());
        self.nodes.push(node);
        id
    }

    /// Get a node by ID.
    pub fn get(&self, id: FieldId) -> &FieldNode {
        &self.nodes[id.0]
    }

    /// Get a mutable node by ID.
    pub fn get_mut(&mut self, id: FieldId) -> &mut FieldNode {
        &mut self.nodes[id.0]
    }

    /// Number of nodes.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the tree is empty.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Return IDs of all root nodes (no parent).
    pub fn roots(&self) -> Vec<FieldId> {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.parent.is_none())
            .map(|(i, _)| FieldId(i))
            .collect()
    }

    /// Return IDs of all terminal (leaf/widget) fields.
    pub fn terminal_fields(&self) -> Vec<FieldId> {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.children.is_empty())
            .map(|(i, _)| FieldId(i))
            .collect()
    }

    /// Compute the fully qualified field name by walking up the parent chain.
    pub fn fully_qualified_name(&self, id: FieldId) -> String {
        let mut parts = Vec::new();
        let mut cur = Some(id);
        while let Some(cid) = cur {
            let node = self.get(cid);
            if !node.partial_name.is_empty() {
                parts.push(node.partial_name.as_str());
            }
            cur = node.parent;
        }
        parts.reverse();
        parts.join(".")
    }

    /// Walk up the tree to find the effective field type.
    pub fn effective_field_type(&self, id: FieldId) -> Option<FieldType> {
        let mut cur = Some(id);
        while let Some(cid) = cur {
            if let Some(ft) = self.get(cid).field_type {
                return Some(ft);
            }
            cur = self.get(cid).parent;
        }
        None
    }

    /// Walk up the tree to find the effective value.
    pub fn effective_value(&self, id: FieldId) -> Option<&FieldValue> {
        let mut cur = Some(id);
        while let Some(cid) = cur {
            let node = self.get(cid);
            if node.value.is_some() {
                return node.value.as_ref();
            }
            cur = node.parent;
        }
        None
    }

    /// Walk up the tree to find the effective DA string.
    pub fn effective_da(&self, id: FieldId) -> Option<&str> {
        let mut cur = Some(id);
        while let Some(cid) = cur {
            if let Some(ref da) = self.get(cid).default_appearance {
                return Some(da.as_str());
            }
            cur = self.get(cid).parent;
        }
        self.document_da.as_deref()
    }

    /// Walk up the tree to find the effective quadding.
    pub fn effective_quadding(&self, id: FieldId) -> Quadding {
        let mut cur = Some(id);
        while let Some(cid) = cur {
            if let Some(q) = self.get(cid).quadding {
                return q;
            }
            cur = self.get(cid).parent;
        }
        self.document_quadding.unwrap_or_default()
    }

    /// Walk up the tree to get effective flags.
    pub fn effective_flags(&self, id: FieldId) -> FieldFlags {
        self.get(id).flags
    }

    /// Walk up the tree to find the effective MaxLen.
    ///
    /// `/MaxLen` is treated as inheritable (like `/FT`, `/DA`, `/Q`): if a
    /// widget does not carry it directly, the value propagates from the nearest
    /// ancestor that does.
    pub fn effective_max_len(&self, id: FieldId) -> Option<u32> {
        let mut cur = Some(id);
        while let Some(cid) = cur {
            if let Some(ml) = self.get(cid).max_len {
                return Some(ml);
            }
            cur = self.get(cid).parent;
        }
        None
    }

    /// Find a terminal field by fully qualified name.
    pub fn find_by_name(&self, name: &str) -> Option<FieldId> {
        self.terminal_fields()
            .into_iter()
            .find(|&id| self.fully_qualified_name(id) == name)
    }

    /// Return all node IDs.
    pub fn all_ids(&self) -> impl Iterator<Item = FieldId> {
        (0..self.nodes.len()).map(FieldId)
    }
}

impl Default for FieldTree {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(name: &str) -> FieldNode {
        FieldNode {
            partial_name: name.into(),
            alternate_name: None,
            mapping_name: None,
            field_type: None,
            flags: FieldFlags::empty(),
            value: None,
            default_value: None,
            default_appearance: None,
            quadding: None,
            max_len: None,
            options: vec![],
            top_index: None,
            rect: None,
            appearance_state: None,
            page_index: None,
            parent: None,
            children: vec![],
            object_id: None,
            has_actions: false,
            mk: None,
            border_style: None,
        }
    }

    #[test]
    fn fqn_simple() {
        let mut tree = FieldTree::new();
        let root = tree.alloc(make_node("form"));
        let mut child = make_node("name");
        child.parent = Some(root);
        child.field_type = Some(FieldType::Text);
        let child_id = tree.alloc(child);
        tree.get_mut(root).children.push(child_id);
        assert_eq!(tree.fully_qualified_name(child_id), "form.name");
    }

    #[test]
    fn inherited_field_type() {
        let mut tree = FieldTree::new();
        let mut parent = make_node("group");
        parent.field_type = Some(FieldType::Button);
        let parent_id = tree.alloc(parent);
        let mut child = make_node("opt1");
        child.parent = Some(parent_id);
        let child_id = tree.alloc(child);
        tree.get_mut(parent_id).children.push(child_id);
        assert_eq!(tree.effective_field_type(child_id), Some(FieldType::Button));
    }

    #[test]
    fn inherited_da() {
        let mut tree = FieldTree::new();
        tree.document_da = Some("0 g /Helv 12 Tf".into());
        let id = tree.alloc(make_node("field"));
        assert_eq!(tree.effective_da(id), Some("0 g /Helv 12 Tf"));
    }

    #[test]
    fn inherited_max_len() {
        let mut tree = FieldTree::new();
        // Parent carries MaxLen; child does not.
        let mut parent = make_node("group");
        parent.max_len = Some(10);
        let parent_id = tree.alloc(parent);

        let mut child = make_node("field");
        child.parent = Some(parent_id);
        let child_id = tree.alloc(child);
        tree.get_mut(parent_id).children.push(child_id);

        assert_eq!(tree.effective_max_len(child_id), Some(10));
    }

    #[test]
    fn own_max_len_overrides_parent() {
        let mut tree = FieldTree::new();
        let mut parent = make_node("group");
        parent.max_len = Some(10);
        let parent_id = tree.alloc(parent);

        let mut child = make_node("field");
        child.parent = Some(parent_id);
        child.max_len = Some(5);
        let child_id = tree.alloc(child);
        tree.get_mut(parent_id).children.push(child_id);

        assert_eq!(tree.effective_max_len(child_id), Some(5));
    }
}
