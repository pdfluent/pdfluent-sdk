//! Unified form access facade for language bindings.
//!
//! Provides [`FormAccess`] and [`DocumentOps`] traits that abstract over
//! AcroForm and XFA form technologies.  Language bindings (C, Python, WASM,
//! Node.js) wrap these traits instead of individual crate APIs.

use crate::tree::{FieldTree, FieldType, FieldValue};

/// The kind of forms in a PDF document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormKind {
    /// AcroForm interactive forms (ISO 32000 §12.7).
    AcroForm,
    /// XFA forms (XML Forms Architecture).
    Xfa,
    /// No forms present.
    None,
}

/// Error type for form operations.
#[derive(Debug, thiserror::Error)]
pub enum FormError {
    /// The requested field was not found.
    #[error("field not found: {0}")]
    FieldNotFound(String),
    /// The field is read-only and cannot be modified.
    #[error("read-only field: {0}")]
    ReadOnly(String),
    /// The provided value is invalid for the field type.
    #[error("invalid value for field type")]
    InvalidValue,
}

/// Unified form field access — works for AcroForm or XFA.
///
/// This trait provides a common interface for reading and writing form field
/// values regardless of the underlying form technology.
pub trait FormAccess {
    /// Returns the kind of form (AcroForm, XFA, or None).
    fn form_type(&self) -> FormKind;

    /// Returns all fully-qualified field names in the form.
    fn field_names(&self) -> Vec<String>;

    /// Gets the current value of a field by its fully-qualified name.
    fn get_value(&self, path: &str) -> Option<String>;

    /// Sets the value of a field by its fully-qualified name.
    fn set_value(&mut self, path: &str, value: &str) -> Result<(), FormError>;
}

/// Unified document operations.
///
/// Provides access to form data, page count, and other document-level
/// operations through a single interface.
pub trait DocumentOps {
    /// Returns the number of pages in the document.
    fn page_count(&self) -> usize;

    /// Returns read-only access to the form engine, if any.
    fn form(&self) -> Option<&dyn FormAccess>;

    /// Returns mutable access to the form engine, if any.
    fn form_mut(&mut self) -> Option<&mut dyn FormAccess>;
}

impl FormAccess for FieldTree {
    fn form_type(&self) -> FormKind {
        FormKind::AcroForm
    }

    fn field_names(&self) -> Vec<String> {
        self.terminal_fields()
            .into_iter()
            .map(|id| self.fully_qualified_name(id))
            .collect()
    }

    fn get_value(&self, path: &str) -> Option<String> {
        let id = self.find_by_name(path)?;
        let value = self.effective_value(id)?;
        match value {
            FieldValue::Text(s) => Some(s.clone()),
            FieldValue::StringArray(arr) => Some(arr.join(", ")),
        }
    }

    fn set_value(&mut self, path: &str, value: &str) -> Result<(), FormError> {
        let id = self
            .find_by_name(path)
            .ok_or_else(|| FormError::FieldNotFound(path.to_string()))?;

        let ft = self
            .effective_field_type(id)
            .ok_or(FormError::InvalidValue)?;

        match ft {
            FieldType::Text | FieldType::Button => {
                self.get_mut(id).value = Some(FieldValue::Text(value.to_string()));
            }
            FieldType::Choice => {
                self.get_mut(id).value = Some(FieldValue::StringArray(vec![value.to_string()]));
            }
            FieldType::Signature => {
                return Err(FormError::ReadOnly(path.to_string()));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flags::FieldFlags;
    use crate::tree::{FieldNode, FieldTree, FieldType, FieldValue};

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

    fn sample_tree() -> FieldTree {
        let mut tree = FieldTree::new();

        // Root "form" node
        let form_id = tree.alloc(make_node("form"));

        // Text field: form.name = "Alice"
        let mut name_node = make_node("name");
        name_node.field_type = Some(FieldType::Text);
        name_node.value = Some(FieldValue::Text("Alice".to_string()));
        name_node.parent = Some(form_id);
        let name_id = tree.alloc(name_node);

        // Button field: form.agree = "true"
        let mut agree_node = make_node("agree");
        agree_node.field_type = Some(FieldType::Button);
        agree_node.value = Some(FieldValue::Text("true".to_string()));
        agree_node.parent = Some(form_id);
        let agree_id = tree.alloc(agree_node);

        // Choice field: form.country = ["NL"]
        let mut country_node = make_node("country");
        country_node.field_type = Some(FieldType::Choice);
        country_node.value = Some(FieldValue::StringArray(vec!["NL".to_string()]));
        country_node.parent = Some(form_id);
        let country_id = tree.alloc(country_node);

        // Signature field: form.sig (no value)
        let mut sig_node = make_node("sig");
        sig_node.field_type = Some(FieldType::Signature);
        sig_node.parent = Some(form_id);
        let sig_id = tree.alloc(sig_node);

        // Empty text field: form.empty
        let mut empty_node = make_node("empty");
        empty_node.field_type = Some(FieldType::Text);
        empty_node.parent = Some(form_id);
        let empty_id = tree.alloc(empty_node);

        // Wire children
        let form = tree.get_mut(form_id);
        form.children = vec![name_id, agree_id, country_id, sig_id, empty_id];

        tree
    }

    #[test]
    fn field_names_returns_all() {
        let tree = sample_tree();
        let names = tree.field_names();
        assert_eq!(names.len(), 5);
        assert!(names.contains(&"form.name".to_string()));
        assert!(names.contains(&"form.agree".to_string()));
    }

    #[test]
    fn get_value_existing_text() {
        let tree = sample_tree();
        assert_eq!(tree.get_value("form.name"), Some("Alice".to_string()));
    }

    #[test]
    fn get_value_returns_none_for_unknown() {
        let tree = sample_tree();
        assert_eq!(tree.get_value("nonexistent"), None);
    }

    #[test]
    fn get_value_returns_none_for_empty() {
        let tree = sample_tree();
        assert_eq!(tree.get_value("form.empty"), None);
    }

    #[test]
    fn set_value_updates_text() {
        let mut tree = sample_tree();
        tree.set_value("form.name", "Bob").unwrap();
        assert_eq!(tree.get_value("form.name"), Some("Bob".to_string()));
    }

    #[test]
    fn set_value_unknown_field_errors() {
        let mut tree = sample_tree();
        let err = tree.set_value("nonexistent", "x").unwrap_err();
        assert!(matches!(err, FormError::FieldNotFound(_)));
    }

    #[test]
    fn set_value_signature_errors() {
        let mut tree = sample_tree();
        let err = tree.set_value("form.sig", "x").unwrap_err();
        assert!(matches!(err, FormError::ReadOnly(_)));
    }

    #[test]
    fn form_type_is_acroform() {
        let tree = sample_tree();
        assert_eq!(tree.form_type(), FormKind::AcroForm);
    }

    #[test]
    fn object_safe() {
        let tree = sample_tree();
        let _dyn_ref: &dyn FormAccess = &tree;
        assert_eq!(_dyn_ref.form_type(), FormKind::AcroForm);
    }
}
