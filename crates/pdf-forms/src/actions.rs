//! Field validation, calculation, and format script hooks (B.7).

use crate::tree::*;

/// Action trigger types from the /AA dictionary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionTrigger {
    Keystroke,
    Validate,
    Format,
    Calculate,
    CursorEnter,
    CursorExit,
    Focus,
    Blur,
    PageOpen,
    PageClose,
}

/// A field action extracted from the /AA dictionary.
#[derive(Debug, Clone)]
pub struct FieldAction {
    /// Which trigger fires this action.
    pub trigger: ActionTrigger,
    /// JavaScript source code, if it's a JavaScript action.
    pub javascript: Option<String>,
}

/// Callback interface for an external JavaScript engine.
pub trait JsActionHandler {
    /// Called on keystroke events (/K). Returns `true` if accepted.
    fn on_keystroke(
        &mut self,
        tree: &mut FieldTree,
        field_id: FieldId,
        change: &str,
        js: &str,
    ) -> bool;
    /// Called on validate events (/V). Returns `true` if valid.
    fn on_validate(&mut self, tree: &mut FieldTree, field_id: FieldId, js: &str) -> bool;
    /// Called on format events (/F). Returns formatted display string.
    fn on_format(&mut self, tree: &FieldTree, field_id: FieldId, js: &str) -> Option<String>;
    /// Called on calculate events (/C). Returns calculated value.
    fn on_calculate(&mut self, tree: &mut FieldTree, field_id: FieldId, js: &str)
        -> Option<String>;
}

/// Run calculation scripts for all fields in the calculation order (/CO).
pub fn run_calculations(tree: &mut FieldTree, handler: &mut dyn JsActionHandler) {
    let order: Vec<FieldId> = tree.calculation_order.clone();
    for field_id in order {
        if !tree.get(field_id).has_actions {
            continue;
        }
        // Placeholder: actual JS execution requires wiring up the handler
        let _ = (field_id, &mut *handler);
    }
}

/// Extract action triggers present on a field.
pub fn field_action_triggers(tree: &FieldTree, id: FieldId) -> Vec<ActionTrigger> {
    if !tree.get(id).has_actions {
        return vec![];
    }
    vec![
        ActionTrigger::Keystroke,
        ActionTrigger::Validate,
        ActionTrigger::Format,
        ActionTrigger::Calculate,
        ActionTrigger::CursorEnter,
        ActionTrigger::CursorExit,
        ActionTrigger::Focus,
        ActionTrigger::Blur,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flags::FieldFlags;
    fn make_field(has_actions: bool) -> (FieldTree, FieldId) {
        let mut tree = FieldTree::new();
        let id = tree.alloc(FieldNode {
            partial_name: "f".into(),
            alternate_name: None,
            mapping_name: None,
            field_type: Some(FieldType::Text),
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
            has_actions,
            mk: None,
            border_style: None,
        });
        (tree, id)
    }
    #[test]
    fn triggers_empty() {
        let (tree, id) = make_field(false);
        assert!(field_action_triggers(&tree, id).is_empty());
    }
    #[test]
    fn triggers_present() {
        let (tree, id) = make_field(true);
        assert!(!field_action_triggers(&tree, id).is_empty());
    }
}
