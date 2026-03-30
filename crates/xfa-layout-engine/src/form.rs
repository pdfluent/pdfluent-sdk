//! Form node types — the input to the layout engine.
//!
//! These represent the merged Form DOM nodes that the layout engine processes.
//! In a full implementation, these would come from xfa-dom-resolver's merge step.

use std::collections::HashMap;

use crate::text::FontMetrics;
use crate::types::{BoxModel, LayoutStrategy};

/// A unique identifier for a form node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FormNodeId(pub usize);

/// The form tree: a node-based representation of the merged template+data.
#[derive(Debug)]
pub struct FormTree {
    pub nodes: Vec<FormNode>,
    /// Per-node metadata (parallel to `nodes`).
    pub metadata: Vec<FormNodeMeta>,
    /// Lookup table: XFA `id` attribute → `FormNodeId`.
    pub node_ids: HashMap<String, FormNodeId>,
}

impl FormTree {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            metadata: Vec::new(),
            node_ids: HashMap::new(),
        }
    }

    pub fn add_node(&mut self, node: FormNode) -> FormNodeId {
        let id = FormNodeId(self.nodes.len());
        self.nodes.push(node);
        self.metadata.push(FormNodeMeta::default());
        id
    }

    /// Add a node together with its metadata. If the meta has an `xfa_id`,
    /// it is registered in the `node_ids` lookup table.
    pub fn add_node_with_meta(&mut self, node: FormNode, meta: FormNodeMeta) -> FormNodeId {
        let id = FormNodeId(self.nodes.len());
        if let Some(ref xfa_id) = meta.xfa_id {
            self.node_ids.insert(xfa_id.clone(), id);
        }
        self.nodes.push(node);
        self.metadata.push(meta);
        id
    }

    pub fn get(&self, id: FormNodeId) -> &FormNode {
        &self.nodes[id.0]
    }

    pub fn get_mut(&mut self, id: FormNodeId) -> &mut FormNode {
        &mut self.nodes[id.0]
    }

    /// Access the metadata for a node.
    pub fn meta(&self, id: FormNodeId) -> &FormNodeMeta {
        &self.metadata[id.0]
    }

    /// Mutably access the metadata for a node.
    pub fn meta_mut(&mut self, id: FormNodeId) -> &mut FormNodeMeta {
        &mut self.metadata[id.0]
    }

    /// Look up a node by its XFA `id` attribute.
    pub fn find_by_xfa_id(&self, id: &str) -> Option<FormNodeId> {
        self.node_ids.get(id).copied()
    }
}

impl Default for FormTree {
    fn default() -> Self {
        Self::new()
    }
}

/// A single node in the Form DOM.
#[derive(Debug, Clone)]
pub struct FormNode {
    pub name: String,
    pub node_type: FormNodeType,
    pub box_model: BoxModel,
    pub layout: LayoutStrategy,
    pub children: Vec<FormNodeId>,
    /// Occurrence rules for repeating subforms.
    pub occur: Occur,
    /// Font metrics for text measurement (Draw/Field nodes).
    pub font: FontMetrics,
    /// FormCalc calculate script (XFA §14.3.2): runs to compute the field's value.
    pub calculate: Option<String>,
    /// FormCalc validate script: runs to validate the field's value, returns bool.
    pub validate: Option<String>,
    /// Column widths for table-layout subforms (XFA columnWidths attribute).
    /// Positive values are fixed widths in points; -1.0 means auto-size.
    /// Empty for non-table nodes.
    pub column_widths: Vec<f64>,
    /// Column span for cells inside a table row (XFA colSpan attribute).
    /// 1 = single column (default), N = span N columns, -1 = span remaining.
    pub col_span: i32,
}

/// The type of form node.
#[derive(Debug, Clone)]
pub enum FormNodeType {
    /// Root subform.
    Root,
    /// A page set containing page areas.
    PageSet,
    /// A page area (page template) with content areas.
    PageArea { content_areas: Vec<ContentArea> },
    /// A generic subform container.
    Subform,
    /// A form field (text field, checkbox, etc.).
    Field { value: String },
    /// A static draw element (text, image, line, etc.).
    Draw { content: String },
}

/// Occurrence rules for repeating subforms (XFA §3.3 occur element).
///
/// Controls how many instances of a subform are created. The layout engine
/// expands templates based on the `initial` count, bounded by `min` and `max`.
#[derive(Debug, Clone)]
pub struct Occur {
    /// Minimum number of occurrences (default 1).
    pub min: u32,
    /// Maximum number of occurrences (-1 = unlimited). Default 1.
    /// Using `Option<u32>` where `None` means unlimited.
    pub max: Option<u32>,
    /// Initial number of occurrences (default = min).
    pub initial: u32,
}

impl Default for Occur {
    fn default() -> Self {
        Self {
            min: 1,
            max: Some(1),
            initial: 1,
        }
    }
}

impl Occur {
    /// Occur rule that means "exactly once" (the default).
    pub fn once() -> Self {
        Self::default()
    }

    /// Occur rule for a repeating subform.
    pub fn repeating(min: u32, max: Option<u32>, initial: u32) -> Self {
        let initial = initial.max(min);
        let initial = match max {
            Some(m) => initial.min(m),
            None => initial,
        };
        Self { min, max, initial }
    }

    /// How many instances should be created.
    pub fn count(&self) -> u32 {
        self.initial
    }

    /// Whether the subform can repeat (max > 1 or unlimited).
    pub fn is_repeating(&self) -> bool {
        match self.max {
            Some(m) => m > 1,
            None => true,
        }
    }
}

/// A content area within a page area.
#[derive(Debug, Clone)]
pub struct ContentArea {
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// Leader (header) node placed at the top of each page's content area.
    pub leader: Option<FormNodeId>,
    /// Trailer (footer) node placed at the bottom of each page's content area.
    pub trailer: Option<FormNodeId>,
}

impl Default for ContentArea {
    fn default() -> Self {
        Self {
            name: String::new(),
            x: 0.0,
            y: 0.0,
            width: 612.0,  // US Letter width in points
            height: 792.0, // US Letter height in points
            leader: None,
            trailer: None,
        }
    }
}

// ---------------------------------------------------------------------------
// Metadata, style, and kind types
// ---------------------------------------------------------------------------

/// Extended metadata for a form node.
///
/// Carries XFA attributes that the layout engine and dynamic scripting
/// system need but that are not part of the core `FormNode` shape.
#[derive(Debug, Clone, Default)]
pub struct FormNodeMeta {
    /// Optional XFA `id` attribute.
    pub xfa_id: Option<String>,
    /// Whether the element has `presence="hidden"` or `"inactive"`.
    pub presence_hidden: bool,
    /// Whether the element has `presence="invisible"` (layout space kept, not rendered).
    pub presence_invisible: bool,
    /// Whether a page break should be inserted before this node.
    pub page_break_before: bool,
    /// Target page area name/id for the break (e.g. "MP3", "Page4_ID").
    pub break_target: Option<String>,
    /// Whether this node targets a specific content area via
    /// `breakBefore targetType="contentArea"`.  Such nodes should be
    /// excluded from the primary content flow (they go into small
    /// decorative areas like "flatten" or "eSign").
    pub content_area_break: bool,
    /// Overflow leader reference name.
    pub overflow_leader: Option<String>,
    /// Overflow trailer reference name.
    pub overflow_trailer: Option<String>,
    /// Keep with next content area.
    pub keep_next_content_area: bool,
    /// Keep with previous content area.
    pub keep_previous_content_area: bool,
    /// Keep intact within content area.
    pub keep_intact_content_area: bool,
    /// Layout-ready script (XFA §14.3).
    pub layout_ready_script: Option<String>,
    /// Event scripts collected from `<event>` and `<calculate>` children.
    pub event_scripts: Vec<String>,
    /// Explicit XFA data binding ref from `<bind ref="...">`.
    pub data_bind_ref: Option<String>,
    /// Whether the node explicitly opts out of data binding via `<bind match="none">`.
    pub data_bind_none: bool,
    /// Visual style (font, colors, borders).
    pub style: FormNodeStyle,
    /// The kind of field (text, checkbox, radio, etc.).
    pub field_kind: FieldKind,
    /// The kind of group (none or exclusive choice).
    pub group_kind: GroupKind,
    /// Item value for fields inside an exclGroup.
    pub item_value: Option<String>,
    /// Check box / radio button size in points.
    pub check_size: Option<f64>,
}

/// The kind of group container.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GroupKind {
    #[default]
    None,
    ExclusiveChoice,
}

/// The kind of form field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FieldKind {
    #[default]
    Text,
    Checkbox,
    Radio,
    Button,
    Dropdown,
    Signature,
    DateTimePicker,
    NumericEdit,
    PasswordEdit,
    ImageEdit,
    Barcode,
}

/// Visual style properties for a form node.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FormNodeStyle {
    pub font_family: Option<String>,
    pub font_size: Option<f64>,
    pub font_weight: Option<String>,
    pub font_style: Option<String>,
    pub text_color: Option<(u8, u8, u8)>,
    pub bg_color: Option<(u8, u8, u8)>,
    pub border_color: Option<(u8, u8, u8)>,
    pub border_width_pt: Option<f64>,
}
