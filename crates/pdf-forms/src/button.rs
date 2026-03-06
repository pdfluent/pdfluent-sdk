//! Checkbox, radio button, and push button implementation (B.3 + B.5).

use crate::flags::FieldFlags;
use crate::tree::*;

/// Button sub-kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Checkbox,
    Radio,
    PushButton,
}

/// Determine button sub-kind from flags.
pub fn button_kind(flags: FieldFlags) -> ButtonKind {
    if flags.push_button() {
        ButtonKind::PushButton
    } else if flags.radio() {
        ButtonKind::Radio
    } else {
        ButtonKind::Checkbox
    }
}

/// Check if a button field is currently "on" (checked/selected).
pub fn is_checked(tree: &FieldTree, id: FieldId) -> bool {
    if let Some(ref state) = tree.get(id).appearance_state {
        return state != "Off";
    }
    matches!(tree.effective_value(id), Some(FieldValue::Text(s)) if s != "Off")
}

/// Get the "on" state name for a button widget.
pub fn on_state_name(tree: &FieldTree, id: FieldId) -> String {
    if let Some(ref state) = tree.get(id).appearance_state {
        if state != "Off" {
            return state.clone();
        }
    }
    if let Some(FieldValue::Text(s)) = tree.effective_value(id) {
        if s != "Off" {
            return s.clone();
        }
    }
    "Yes".into()
}

/// Toggle a checkbox field. Returns `false` if read-only or not a checkbox.
pub fn toggle_checkbox(tree: &mut FieldTree, id: FieldId) -> bool {
    let flags = tree.effective_flags(id);
    if flags.read_only() || button_kind(flags) != ButtonKind::Checkbox {
        return false;
    }
    let new_state = if is_checked(tree, id) {
        "Off".to_string()
    } else {
        on_state_name(tree, id)
    };
    tree.get_mut(id).value = Some(FieldValue::Text(new_state.clone()));
    tree.get_mut(id).appearance_state = Some(new_state);
    true
}

/// Select a radio button, deselecting siblings. Returns `false` if read-only.
pub fn select_radio(tree: &mut FieldTree, id: FieldId) -> bool {
    if tree.effective_flags(id).read_only() {
        return false;
    }
    let on_name = on_state_name(tree, id);
    if let Some(pid) = tree.get(id).parent {
        let siblings: Vec<FieldId> = tree.get(pid).children.clone();
        for sib in siblings {
            if sib != id {
                tree.get_mut(sib).value = Some(FieldValue::Text("Off".into()));
                tree.get_mut(sib).appearance_state = Some("Off".into());
            }
        }
        tree.get_mut(pid).value = Some(FieldValue::Text(on_name.clone()));
    }
    tree.get_mut(id).value = Some(FieldValue::Text(on_name.clone()));
    tree.get_mut(id).appearance_state = Some(on_name);
    true
}

/// Parsed submit-form action.
#[derive(Debug, Clone)]
pub struct SubmitAction {
    pub url: String,
    pub flags: u32,
}

/// Parsed reset-form action.
#[derive(Debug, Clone)]
pub struct ResetAction {
    pub fields: Vec<String>,
    pub flags: u32,
}

/// Icon/caption layout for push buttons (/TP values).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconCaptionLayout {
    CaptionOnly,
    IconOnly,
    CaptionBelow,
    CaptionAbove,
    CaptionRight,
    CaptionLeft,
    CaptionOverlay,
}

impl From<u32> for IconCaptionLayout {
    fn from(v: u32) -> Self {
        match v {
            1 => Self::IconOnly,
            2 => Self::CaptionBelow,
            3 => Self::CaptionAbove,
            4 => Self::CaptionRight,
            5 => Self::CaptionLeft,
            6 => Self::CaptionOverlay,
            _ => Self::CaptionOnly,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn make_checkbox() -> (FieldTree, FieldId) {
        let mut tree = FieldTree::new();
        let id = tree.alloc(FieldNode {
            partial_name: "cb".into(),
            alternate_name: None,
            mapping_name: None,
            field_type: Some(FieldType::Button),
            flags: FieldFlags::empty(),
            value: Some(FieldValue::Text("Off".into())),
            default_value: None,
            default_appearance: None,
            quadding: None,
            max_len: None,
            options: vec![],
            top_index: None,
            rect: Some([0.0, 0.0, 12.0, 12.0]),
            appearance_state: Some("Off".into()),
            page_index: None,
            parent: None,
            children: vec![],
            object_id: None,
            has_actions: false,
            mk: None,
            border_style: None,
        });
        (tree, id)
    }

    #[test]
    fn checkbox_initially_off() {
        let (tree, id) = make_checkbox();
        assert!(!is_checked(&tree, id));
    }
    #[test]
    fn toggle_checkbox_on_off() {
        let (mut tree, id) = make_checkbox();
        assert!(toggle_checkbox(&mut tree, id));
        assert!(is_checked(&tree, id));
        assert!(toggle_checkbox(&mut tree, id));
        assert!(!is_checked(&tree, id));
    }
    #[test]
    fn radio_mutual_exclusion() {
        let mut tree = FieldTree::new();
        let group = tree.alloc(FieldNode {
            partial_name: "rg".into(),
            alternate_name: None,
            mapping_name: None,
            field_type: Some(FieldType::Button),
            flags: FieldFlags::from_bits((1 << 15) | (1 << 14)),
            value: Some(FieldValue::Text("Off".into())),
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
        });
        let mk = |tree: &mut FieldTree, n: &str| -> FieldId {
            let id = tree.alloc(FieldNode {
                partial_name: n.into(),
                alternate_name: None,
                mapping_name: None,
                field_type: None,
                flags: FieldFlags::from_bits((1 << 15) | (1 << 14)),
                value: Some(FieldValue::Text("Off".into())),
                default_value: None,
                default_appearance: None,
                quadding: None,
                max_len: None,
                options: vec![],
                top_index: None,
                rect: Some([0.0, 0.0, 12.0, 12.0]),
                appearance_state: Some("Off".into()),
                page_index: None,
                parent: Some(group),
                children: vec![],
                object_id: None,
                has_actions: false,
                mk: None,
                border_style: None,
            });
            tree.get_mut(group).children.push(id);
            id
        };
        let r1 = mk(&mut tree, "opt1");
        let r2 = mk(&mut tree, "opt2");
        assert!(select_radio(&mut tree, r1));
        assert!(is_checked(&tree, r1));
        assert!(!is_checked(&tree, r2));
        assert!(select_radio(&mut tree, r2));
        assert!(!is_checked(&tree, r1));
        assert!(is_checked(&tree, r2));
    }
}
