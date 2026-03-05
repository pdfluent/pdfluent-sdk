//! Event handling — hit-testing, focus management, and input processing.
//!
//! Maps user interactions (clicks, keystrokes) to form field operations.
//! Works on the LayoutDom to determine which field was targeted and updates
//! field values in the FormTree accordingly.

use xfa_layout_engine::form::{FormNodeId, FormNodeType, FormTree};
use xfa_layout_engine::layout::{LayoutContent, LayoutDom, LayoutNode};

/// A user input event.
#[derive(Debug, Clone)]
pub enum InputEvent {
    /// Mouse click at page coordinates (points).
    Click { page: usize, x: f64, y: f64 },
    /// Character input (typing a character into the focused field).
    CharInput(char),
    /// Backspace key (delete last character).
    Backspace,
    /// Tab key (move focus to next field).
    Tab,
    /// Shift+Tab (move focus to previous field).
    ShiftTab,
}

/// Result of processing an input event.
#[derive(Debug, Clone, PartialEq)]
pub enum EventResult {
    /// No action taken (event not applicable).
    Ignored,
    /// Focus changed to a field.
    FocusChanged(FormNodeId),
    /// Field value was modified.
    ValueChanged(FormNodeId),
    /// Focus was cleared (clicked outside any field).
    FocusCleared,
}

/// Manages form interaction state: focus, hit-testing, and input routing.
pub struct FormState {
    /// Currently focused field (if any).
    focused: Option<FormNodeId>,
    /// Ordered list of focusable fields for Tab navigation.
    tab_order: Vec<FormNodeId>,
}

impl FormState {
    /// Create a new FormState by scanning the LayoutDom for focusable fields.
    pub fn new(layout: &LayoutDom) -> Self {
        let mut tab_order = Vec::new();
        for page in &layout.pages {
            for node in &page.nodes {
                collect_fields(node, &mut tab_order);
            }
        }
        Self {
            focused: None,
            tab_order,
        }
    }

    /// Get the currently focused field.
    pub fn focused(&self) -> Option<FormNodeId> {
        self.focused
    }

    /// Process an input event against the layout and form tree.
    ///
    /// Returns what happened as a result of the event.
    pub fn process_event(
        &mut self,
        event: &InputEvent,
        layout: &LayoutDom,
        form: &mut FormTree,
    ) -> EventResult {
        match event {
            InputEvent::Click { page, x, y } => self.handle_click(*page, *x, *y, layout),
            InputEvent::CharInput(ch) => self.handle_char_input(*ch, form),
            InputEvent::Backspace => self.handle_backspace(form),
            InputEvent::Tab => self.handle_tab(false),
            InputEvent::ShiftTab => self.handle_tab(true),
        }
    }

    /// Handle a click event: find the field at the click position.
    fn handle_click(
        &mut self,
        page_idx: usize,
        x: f64,
        y: f64,
        layout: &LayoutDom,
    ) -> EventResult {
        let Some(page) = layout.pages.get(page_idx) else {
            return EventResult::Ignored;
        };

        // Hit-test: find the deepest field node at (x, y).
        let mut hit = None;
        for node in &page.nodes {
            if let Some(found) = hit_test_node(node, x, y) {
                hit = Some(found);
            }
        }

        match hit {
            Some(field_id) => {
                self.focused = Some(field_id);
                EventResult::FocusChanged(field_id)
            }
            None => {
                if self.focused.is_some() {
                    self.focused = None;
                    EventResult::FocusCleared
                } else {
                    EventResult::Ignored
                }
            }
        }
    }

    /// Handle character input: append character to focused field's value.
    fn handle_char_input(&self, ch: char, form: &mut FormTree) -> EventResult {
        let Some(field_id) = self.focused else {
            return EventResult::Ignored;
        };

        let node = form.get_mut(field_id);
        if let FormNodeType::Field { value } = &mut node.node_type {
            value.push(ch);
            EventResult::ValueChanged(field_id)
        } else {
            EventResult::Ignored
        }
    }

    /// Handle backspace: remove last character from focused field's value.
    fn handle_backspace(&self, form: &mut FormTree) -> EventResult {
        let Some(field_id) = self.focused else {
            return EventResult::Ignored;
        };

        let node = form.get_mut(field_id);
        if let FormNodeType::Field { value } = &mut node.node_type {
            if value.pop().is_some() {
                EventResult::ValueChanged(field_id)
            } else {
                EventResult::Ignored
            }
        } else {
            EventResult::Ignored
        }
    }

    /// Handle Tab/Shift+Tab: cycle focus through tab order.
    fn handle_tab(&mut self, reverse: bool) -> EventResult {
        if self.tab_order.is_empty() {
            return EventResult::Ignored;
        }

        let next = match self.focused {
            Some(current) => {
                if let Some(pos) = self.tab_order.iter().position(|&id| id == current) {
                    if reverse {
                        if pos == 0 {
                            self.tab_order.len() - 1
                        } else {
                            pos - 1
                        }
                    } else {
                        (pos + 1) % self.tab_order.len()
                    }
                } else {
                    0
                }
            }
            None => {
                if reverse {
                    self.tab_order.len() - 1
                } else {
                    0
                }
            }
        };

        let field_id = self.tab_order[next];
        self.focused = Some(field_id);
        EventResult::FocusChanged(field_id)
    }
}

/// Recursively collect focusable field IDs from the layout tree.
fn collect_fields(node: &LayoutNode, fields: &mut Vec<FormNodeId>) {
    match &node.content {
        LayoutContent::Field { .. } | LayoutContent::WrappedText { .. } => {
            fields.push(node.form_node);
        }
        _ => {}
    }
    for child in &node.children {
        collect_fields(child, fields);
    }
}

/// Hit-test a layout node tree: return the deepest field at (x, y).
fn hit_test_node(node: &LayoutNode, x: f64, y: f64) -> Option<FormNodeId> {
    if !node.rect.contains(x, y) {
        return None;
    }

    // Check children first (deeper nodes take priority).
    for child in &node.children {
        if let Some(found) = hit_test_node(child, x, y) {
            return Some(found);
        }
    }

    // If this node is a field, it's a hit.
    match &node.content {
        LayoutContent::Field { .. } | LayoutContent::WrappedText { .. } => {
            Some(node.form_node)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use xfa_layout_engine::form::*;
    use xfa_layout_engine::layout::*;
    use xfa_layout_engine::text::FontMetrics;
    use xfa_layout_engine::types::*;

    fn test_layout() -> (FormTree, LayoutDom) {
        let mut tree = FormTree::new();
        let f1 = tree.add_node(FormNode {
            name: "Name".to_string(),
            node_type: FormNodeType::Field {
                value: "Alice".to_string(),
            },
            box_model: BoxModel {
                width: Some(100.0),
                height: Some(20.0),
                x: 10.0,
                y: 10.0,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
        });
        let f2 = tree.add_node(FormNode {
            name: "Email".to_string(),
            node_type: FormNodeType::Field {
                value: String::new(),
            },
            box_model: BoxModel {
                width: Some(100.0),
                height: Some(20.0),
                x: 10.0,
                y: 40.0,
                ..Default::default()
            },
            layout: LayoutStrategy::Positioned,
            children: vec![],
            occur: Occur::once(),
            font: FontMetrics::default(),
            calculate: None,
            validate: None,
        });

        let layout = LayoutDom {
            pages: vec![LayoutPage {
                width: 200.0,
                height: 100.0,
                nodes: vec![
                    LayoutNode {
                        form_node: f1,
                        rect: Rect::new(10.0, 10.0, 100.0, 20.0),
                        name: "Name".to_string(),
                        content: LayoutContent::Field {
                            value: "Alice".to_string(),
                        },
                        children: vec![],
                    },
                    LayoutNode {
                        form_node: f2,
                        rect: Rect::new(10.0, 40.0, 100.0, 20.0),
                        name: "Email".to_string(),
                        content: LayoutContent::Field {
                            value: String::new(),
                        },
                        children: vec![],
                    },
                ],
            }],
        };

        (tree, layout)
    }

    #[test]
    fn click_focuses_field() {
        let (mut tree, layout) = test_layout();
        let mut state = FormState::new(&layout);

        let result = state.process_event(
            &InputEvent::Click { page: 0, x: 50.0, y: 15.0 },
            &layout,
            &mut tree,
        );
        assert_eq!(result, EventResult::FocusChanged(FormNodeId(0)));
        assert_eq!(state.focused(), Some(FormNodeId(0)));
    }

    #[test]
    fn click_outside_clears_focus() {
        let (mut tree, layout) = test_layout();
        let mut state = FormState::new(&layout);

        // Focus a field first
        state.process_event(
            &InputEvent::Click { page: 0, x: 50.0, y: 15.0 },
            &layout,
            &mut tree,
        );
        // Click outside
        let result = state.process_event(
            &InputEvent::Click { page: 0, x: 180.0, y: 80.0 },
            &layout,
            &mut tree,
        );
        assert_eq!(result, EventResult::FocusCleared);
        assert_eq!(state.focused(), None);
    }

    #[test]
    fn char_input_appends_to_field() {
        let (mut tree, layout) = test_layout();
        let mut state = FormState::new(&layout);

        // Focus the Email field
        state.process_event(
            &InputEvent::Click { page: 0, x: 50.0, y: 45.0 },
            &layout,
            &mut tree,
        );

        // Type characters
        state.process_event(&InputEvent::CharInput('a'), &layout, &mut tree);
        state.process_event(&InputEvent::CharInput('b'), &layout, &mut tree);

        if let FormNodeType::Field { value } = &tree.get(FormNodeId(1)).node_type {
            assert_eq!(value, "ab");
        } else {
            panic!("expected Field");
        }
    }

    #[test]
    fn backspace_removes_character() {
        let (mut tree, layout) = test_layout();
        let mut state = FormState::new(&layout);

        // Focus Name field (has "Alice")
        state.process_event(
            &InputEvent::Click { page: 0, x: 50.0, y: 15.0 },
            &layout,
            &mut tree,
        );

        let result = state.process_event(&InputEvent::Backspace, &layout, &mut tree);
        assert_eq!(result, EventResult::ValueChanged(FormNodeId(0)));
        if let FormNodeType::Field { value } = &tree.get(FormNodeId(0)).node_type {
            assert_eq!(value, "Alic");
        }
    }

    #[test]
    fn tab_cycles_focus() {
        let (mut tree, layout) = test_layout();
        let mut state = FormState::new(&layout);

        // Tab without focus: goes to first field
        let result = state.process_event(&InputEvent::Tab, &layout, &mut tree);
        assert_eq!(result, EventResult::FocusChanged(FormNodeId(0)));

        // Tab again: goes to second field
        let result = state.process_event(&InputEvent::Tab, &layout, &mut tree);
        assert_eq!(result, EventResult::FocusChanged(FormNodeId(1)));

        // Tab again: wraps to first
        let result = state.process_event(&InputEvent::Tab, &layout, &mut tree);
        assert_eq!(result, EventResult::FocusChanged(FormNodeId(0)));
    }

    #[test]
    fn shift_tab_reverses() {
        let (mut tree, layout) = test_layout();
        let mut state = FormState::new(&layout);

        // Shift+Tab without focus: goes to last field
        let result = state.process_event(&InputEvent::ShiftTab, &layout, &mut tree);
        assert_eq!(result, EventResult::FocusChanged(FormNodeId(1)));

        // Shift+Tab again: wraps to first
        let result = state.process_event(&InputEvent::ShiftTab, &layout, &mut tree);
        assert_eq!(result, EventResult::FocusChanged(FormNodeId(0)));
    }

    #[test]
    fn char_input_without_focus_is_ignored() {
        let (mut tree, layout) = test_layout();
        let mut state = FormState::new(&layout);

        let result = state.process_event(&InputEvent::CharInput('x'), &layout, &mut tree);
        assert_eq!(result, EventResult::Ignored);
    }

    #[test]
    fn click_invalid_page_is_ignored() {
        let (mut tree, layout) = test_layout();
        let mut state = FormState::new(&layout);

        let result = state.process_event(
            &InputEvent::Click { page: 5, x: 50.0, y: 15.0 },
            &layout,
            &mut tree,
        );
        assert_eq!(result, EventResult::Ignored);
    }
}
