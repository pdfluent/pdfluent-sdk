//! Checkbox, radio button, and push button implementation (B.3 + B.5).

use crate::flags::FieldFlags;
use crate::tree::*;

/// Sub-kind of a button field.
///
/// AcroForm models all of checkbox, radio button, and push button as the
/// same `/Btn` field type, distinguished only by the `Pushbutton` and `Radio`
/// flags in the field flags word. Use [`button_kind`] to derive this enum
/// from a [`FieldFlags`] value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    /// Two-state toggle. Default kind when neither `Pushbutton` nor `Radio`
    /// flags are set. Drawn as a tickbox; user clicks to flip its value
    /// between the off-state and an "on" appearance state.
    Checkbox,
    /// Mutually-exclusive option within a parent radio group. Selecting one
    /// radio child automatically de-selects its siblings — see
    /// [`select_radio`].
    Radio,
    /// Click-to-action button without persistent state. Typically wired to
    /// a JavaScript or submit/reset action via the `/AA` dictionary; its
    /// `value` is not meaningful as form data.
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

/// Parsed submit-form action attached to a button.
///
/// Produced when the parser encounters an `/A` dictionary with `/S /SubmitForm`.
/// Triggered when the user clicks a push button configured to send form data
/// to a remote endpoint.
#[derive(Debug, Clone)]
pub struct SubmitAction {
    /// The submission target URL (`/F` entry of the action). Usually HTTP/HTTPS;
    /// PDF also allows `mailto:` and FTP URLs.
    pub url: String,
    /// Bit flags from the `/Flags` entry controlling submit format (FDF, HTML,
    /// XFDF, JSON), field inclusion (include vs. exclude), and HTTP method.
    /// See ISO 32000-2 §12.7.5.2 Table 257 for the bit assignments.
    pub flags: u32,
}

/// Parsed reset-form action attached to a button.
///
/// Produced when the parser encounters an `/A` dictionary with `/S /ResetForm`.
/// Triggered when the user clicks a push button configured to clear form
/// fields back to their default values.
#[derive(Debug, Clone)]
pub struct ResetAction {
    /// Fully-qualified field names (parent.child notation) targeted by the
    /// reset. Empty means "reset all fields in the form".
    pub fields: Vec<String>,
    /// Bit flags controlling include-vs-exclude semantics (`/Fields` is the
    /// list to reset, or the list to skip, depending on bit 0). See ISO
    /// 32000-2 §12.7.5.3.
    pub flags: u32,
}

/// Icon and caption arrangement for a push button's appearance.
///
/// Maps to the `/TP` entry of the button's appearance characteristics
/// dictionary (`/MK`). Determines whether the button shows text only, an
/// image only, or both — and where the caption sits relative to the icon.
/// PDF default when `/TP` is absent or unrecognized is [`Self::CaptionOnly`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IconCaptionLayout {
    /// `/TP 0` — show only the caption (`/CA`). Icon (if any) is ignored.
    CaptionOnly,
    /// `/TP 1` — show only the icon. Caption is ignored.
    IconOnly,
    /// `/TP 2` — caption rendered below the icon.
    CaptionBelow,
    /// `/TP 3` — caption rendered above the icon.
    CaptionAbove,
    /// `/TP 4` — caption rendered to the right of the icon.
    CaptionRight,
    /// `/TP 5` — caption rendered to the left of the icon.
    CaptionLeft,
    /// `/TP 6` — caption overlaid on top of the icon.
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
            on_state: None,
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
            on_state: None,
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
                on_state: None,
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
