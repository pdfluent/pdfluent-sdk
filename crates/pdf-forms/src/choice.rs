//! Dropdown and listbox implementation (B.4).

use crate::flags::FieldFlags;
use crate::tree::*;

/// Sub-kind of a choice (`/Ch`) field, derived from its flags word.
///
/// AcroForm models combo boxes and list boxes as the same field type;
/// the four variants below distinguish the visual presentation and
/// selection semantics. Use [`choice_kind`] to derive this enum from a
/// [`FieldFlags`] value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChoiceKind {
    /// Closed-list combo box: pick one of the predefined `/Opt` entries.
    /// Free-text input not allowed. (`Combo` flag set; `Edit` clear.)
    ComboBox,
    /// Combo box that also accepts free-text input not in the option list.
    /// Useful for "common cities, but type your own" patterns.
    /// (`Combo` and `Edit` both set.)
    EditableCombo,
    /// Single-selection list box; one option visible-and-selected at a
    /// time, others scroll. (`Combo` clear; `MultiSelect` clear.)
    ListBox,
    /// List box allowing multiple options to be selected simultaneously
    /// (Ctrl/Cmd-click). The field's value is then a name array rather
    /// than a single name. (`MultiSelect` flag set.)
    MultiSelectListBox,
}

/// Determine choice sub-kind from flags.
pub fn choice_kind(flags: FieldFlags) -> ChoiceKind {
    if flags.combo() {
        if flags.edit() {
            ChoiceKind::EditableCombo
        } else {
            ChoiceKind::ComboBox
        }
    } else if flags.multi_select() {
        ChoiceKind::MultiSelectListBox
    } else {
        ChoiceKind::ListBox
    }
}

/// Get the currently selected value(s).
pub fn get_selection(tree: &FieldTree, id: FieldId) -> Vec<String> {
    match tree.effective_value(id) {
        Some(FieldValue::Text(s)) => vec![s.clone()],
        Some(FieldValue::StringArray(arr)) => arr.clone(),
        None => vec![],
    }
}

/// Get the list of available options.
pub fn get_options(tree: &FieldTree, id: FieldId) -> &[ChoiceOption] {
    &tree.get(id).options
}

/// Set the selection for a single-select choice field.
/// For non-editable combos, value must match an option. Returns `false` if read-only or invalid.
pub fn set_selection(tree: &mut FieldTree, id: FieldId, value: &str) -> bool {
    let flags = tree.effective_flags(id);
    if flags.read_only() {
        return false;
    }
    if choice_kind(flags) == ChoiceKind::ComboBox
        && !tree
            .get(id)
            .options
            .iter()
            .any(|o| o.export == value || o.display == value)
    {
        return false;
    }
    tree.get_mut(id).value = Some(FieldValue::Text(value.to_string()));
    true
}

/// Set multiple selections for a multi-select list box. Returns `false` if read-only or not multi-select.
pub fn set_multi_selection(tree: &mut FieldTree, id: FieldId, values: Vec<String>) -> bool {
    let flags = tree.effective_flags(id);
    if flags.read_only() || !flags.multi_select() {
        return false;
    }
    tree.get_mut(id).value = Some(FieldValue::StringArray(values));
    true
}

/// Get the index of the first selected option, if any.
pub fn selected_index(tree: &FieldTree, id: FieldId) -> Option<usize> {
    let first = get_selection(tree, id).into_iter().next()?;
    tree.get(id)
        .options
        .iter()
        .position(|o| o.export == first || o.display == first)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn make_choice_tree() -> (FieldTree, FieldId) {
        let mut tree = FieldTree::new();
        let id = tree.alloc(FieldNode {
            partial_name: "dd".into(),
            alternate_name: None,
            mapping_name: None,
            field_type: Some(FieldType::Choice),
            flags: FieldFlags::from_bits(1 << 17),
            value: None,
            default_value: None,
            default_appearance: None,
            quadding: None,
            max_len: None,
            options: vec![
                ChoiceOption {
                    export: "a".into(),
                    display: "Alpha".into(),
                },
                ChoiceOption {
                    export: "b".into(),
                    display: "Beta".into(),
                },
                ChoiceOption {
                    export: "c".into(),
                    display: "Gamma".into(),
                },
            ],
            top_index: None,
            rect: Some([0.0, 0.0, 150.0, 20.0]),
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
        (tree, id)
    }
    #[test]
    fn kind_combo() {
        assert_eq!(
            choice_kind(FieldFlags::from_bits(1 << 17)),
            ChoiceKind::ComboBox
        );
    }
    #[test]
    fn kind_editable() {
        assert_eq!(
            choice_kind(FieldFlags::from_bits((1 << 17) | (1 << 18))),
            ChoiceKind::EditableCombo
        );
    }
    #[test]
    fn kind_listbox() {
        assert_eq!(choice_kind(FieldFlags::empty()), ChoiceKind::ListBox);
    }
    #[test]
    fn kind_multi() {
        assert_eq!(
            choice_kind(FieldFlags::from_bits(1 << 21)),
            ChoiceKind::MultiSelectListBox
        );
    }
    #[test]
    fn set_valid() {
        let (mut tree, id) = make_choice_tree();
        assert!(set_selection(&mut tree, id, "a"));
        assert_eq!(get_selection(&tree, id), vec!["a"]);
    }
    #[test]
    fn set_invalid() {
        let (mut tree, id) = make_choice_tree();
        assert!(!set_selection(&mut tree, id, "nope"));
    }
    #[test]
    fn sel_index() {
        let (mut tree, id) = make_choice_tree();
        set_selection(&mut tree, id, "b");
        assert_eq!(selected_index(&tree, id), Some(1));
    }
}
