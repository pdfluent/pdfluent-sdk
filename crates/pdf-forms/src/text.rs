//! Text field implementation (B.2).

use crate::flags::FieldFlags;
use crate::tree::*;

/// Text field sub-kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextFieldKind {
    Normal,
    Multiline,
    Password,
    Comb,
    RichText,
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
    let max_len = tree.get(id).max_len;
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
    let node = tree.get(id);
    let max_len = node.max_len?;
    if max_len == 0 {
        return None;
    }
    let rect = node.rect?;
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
