//! Form node types — the input to the layout engine.
//!
//! These represent the merged Form DOM nodes that the layout engine processes.
//! In a full implementation, these would come from xfa-dom-resolver's merge step.

use crate::types::{BoxModel, LayoutStrategy};

/// A unique identifier for a form node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FormNodeId(pub usize);

/// The form tree: a node-based representation of the merged template+data.
#[derive(Debug)]
pub struct FormTree {
    pub nodes: Vec<FormNode>,
}

impl FormTree {
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    pub fn add_node(&mut self, node: FormNode) -> FormNodeId {
        let id = FormNodeId(self.nodes.len());
        self.nodes.push(node);
        id
    }

    pub fn get(&self, id: FormNodeId) -> &FormNode {
        &self.nodes[id.0]
    }

    pub fn get_mut(&mut self, id: FormNodeId) -> &mut FormNode {
        &mut self.nodes[id.0]
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

/// A content area within a page area.
#[derive(Debug, Clone)]
pub struct ContentArea {
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Default for ContentArea {
    fn default() -> Self {
        Self {
            name: String::new(),
            x: 0.0,
            y: 0.0,
            width: 612.0,  // US Letter width in points
            height: 792.0, // US Letter height in points
        }
    }
}
