//! Text field implementation (B.2).

use crate::flags::FieldFlags;
use crate::tree::*;

/// Sub-kind of a text (`/Tx`) field, derived from its flags word.
///
/// AcroForm models all text input variants as the same field type; the
/// flags below distinguish the visual presentation and validation rules.
/// Use [`text_field_kind`] to derive this enum from a [`FieldFlags`] value.
/// The variants are checked in priority order — for example, a field with
/// both `FileSelect` and `Multiline` set resolves to [`Self::FileSelect`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextFieldKind {
    /// Single-line free-text input. Default when no specialization flag is
    /// set.
    Normal,
    /// Multi-line text input — text wraps at the field width and accepts
    /// embedded newlines. (`Multiline` flag set.)
    Multiline,
    /// Single-line input that masks each character (typical for password
    /// entry). (`Password` flag set.)
    Password,
    /// Fixed-pitch input divided into `MaxLen` equally-spaced cells —
    /// useful for serial numbers, postal codes. Requires `MaxLen` to be
    /// set on the field. (`Comb` flag set.)
    Comb,
    /// Rich-text input where the value carries XHTML formatting in
    /// addition to the plain text. (`RichText` flag set.)
    RichText,
    /// File-selector field: the value is a path to a local file the user
    /// selected via the viewer's file dialog. (`FileSelect` flag set.)
    FileSelect,
}

/// Determine the text field sub-kind from field flags.
pub fn text_field_kind(flags: FieldFlags) -> TextFieldKind {
    if flags.file_select() {
        TextFieldKind::FileSelect
    } else if flags.comb() {
        TextFieldKind::Comb
    } else if flags.rich_text() {
        TextFieldKind::RichText
    } else if flags.password() {
        TextFieldKind::Password
    } else if flags.multiline() {
        TextFieldKind::Multiline
    } else {
        TextFieldKind::Normal
    }
}

/// Get the current text value of a text field.
pub fn get_text_value(tree: &FieldTree, id: FieldId) -> Option<String> {
    match tree.effective_value(id)? {
        FieldValue::Text(s) => Some(s.clone()),
        FieldValue::StringArray(arr) => arr.first().cloned(),
    }
}

/// Set a text field's value, enforcing MaxLen if present.
/// Returns `false` if the field is read-only.
pub fn set_text_value(tree: &mut FieldTree, id: FieldId, text: &str) -> bool {
    if tree.effective_flags(id).read_only() {
        return false;
    }
    let max_len = tree.effective_max_len(id);
    let value = if let Some(ml) = max_len {
        text.chars().take(ml as usize).collect()
    } else {
        text.to_string()
    };
    tree.get_mut(id).value = Some(FieldValue::Text(value));
    true
}

/// For comb fields, compute the width of each cell.
pub fn comb_cell_width(tree: &FieldTree, id: FieldId) -> Option<f32> {
    let max_len = tree.effective_max_len(id)?;
    if max_len == 0 {
        return None;
    }
    let rect = tree.get(id).rect?;
    Some((rect[2] - rect[0]) / max_len as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn make_text_tree() -> (FieldTree, FieldId) {
        let mut tree = FieldTree::new();
        let id = tree.alloc(FieldNode {
            partial_name: "text1".into(),
            alternate_name: None,
            mapping_name: None,
            field_type: Some(FieldType::Text),
            flags: FieldFlags::empty(),
            value: Some(FieldValue::Text("hello".into())),
            default_value: None,
            default_appearance: None,
            quadding: None,
            max_len: None,
            options: vec![],
            top_index: None,
            rect: Some([0.0, 0.0, 200.0, 20.0]),
            appearance_state: None,
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
    fn get_value() {
        let (tree, id) = make_text_tree();
        assert_eq!(get_text_value(&tree, id), Some("hello".into()));
    }
    #[test]
    fn set_value() {
        let (mut tree, id) = make_text_tree();
        assert!(set_text_value(&mut tree, id, "world"));
        assert_eq!(get_text_value(&tree, id), Some("world".into()));
    }
    #[test]
    fn set_value_readonly() {
        let (mut tree, id) = make_text_tree();
        tree.get_mut(id).flags = FieldFlags::from_bits(1);
        assert!(!set_text_value(&mut tree, id, "nope"));
    }
    #[test]
    fn set_value_maxlen() {
        let (mut tree, id) = make_text_tree();
        tree.get_mut(id).max_len = Some(3);
        assert!(set_text_value(&mut tree, id, "abcdef"));
        assert_eq!(get_text_value(&tree, id), Some("abc".into()));
    }
    #[test]
    fn kind_detection() {
        assert_eq!(text_field_kind(FieldFlags::empty()), TextFieldKind::Normal);
        assert_eq!(
            text_field_kind(FieldFlags::from_bits(1 << 12)),
            TextFieldKind::Multiline
        );
        assert_eq!(
            text_field_kind(FieldFlags::from_bits(1 << 13)),
            TextFieldKind::Password
        );
        assert_eq!(
            text_field_kind(FieldFlags::from_bits(1 << 24)),
            TextFieldKind::Comb
        );
    }
    #[test]
    fn comb_width() {
        let (mut tree, id) = make_text_tree();
        tree.get_mut(id).max_len = Some(10);
        assert_eq!(comb_cell_width(&tree, id), Some(20.0));
    }
}
