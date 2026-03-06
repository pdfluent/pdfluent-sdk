//! Unified form access facade for language bindings.
//!
//! Provides [`FormAccess`] and [`DocumentOps`] traits that abstract over
//! AcroForm and XFA form technologies.  Language bindings (C, Python, WASM,
//! Node.js) wrap these traits instead of individual crate APIs.

use crate::{FieldTree, FieldType, FieldValue};

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
        self.fields.iter().map(|f| f.name.clone()).collect()
    }

    fn get_value(&self, path: &str) -> Option<String> {
        let field = self.fields.iter().find(|f| f.name == path)?;
        match &field.value {
            Some(FieldValue::Text(s)) => Some(s.clone()),
            Some(FieldValue::Choice(arr)) => Some(arr.join(", ")),
            Some(FieldValue::Button(b)) => Some(b.to_string()),
            None => None,
        }
    }

    fn set_value(&mut self, path: &str, value: &str) -> Result<(), FormError> {
        let field = self
            .fields
            .iter_mut()
            .find(|f| f.name == path)
            .ok_or_else(|| FormError::FieldNotFound(path.to_string()))?;

        match field.field_type {
            FieldType::Text => {
                field.value = Some(FieldValue::Text(value.to_string()));
            }
            FieldType::Button => {
                let b = matches!(value, "true" | "Yes" | "On" | "1");
                field.value = Some(FieldValue::Button(b));
            }
            FieldType::Choice => {
                field.value = Some(FieldValue::Choice(vec![value.to_string()]));
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
    use crate::{Field, FieldTree, FieldType, FieldValue};

    fn sample_tree() -> FieldTree {
        FieldTree {
            fields: vec![
                Field {
                    name: "form.name".to_string(),
                    field_type: FieldType::Text,
                    value: Some(FieldValue::Text("Alice".to_string())),
                },
                Field {
                    name: "form.agree".to_string(),
                    field_type: FieldType::Button,
                    value: Some(FieldValue::Button(true)),
                },
                Field {
                    name: "form.country".to_string(),
                    field_type: FieldType::Choice,
                    value: Some(FieldValue::Choice(vec!["NL".to_string()])),
                },
                Field {
                    name: "form.sig".to_string(),
                    field_type: FieldType::Signature,
                    value: None,
                },
                Field {
                    name: "form.empty".to_string(),
                    field_type: FieldType::Text,
                    value: None,
                },
            ],
        }
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
        // Verify FormAccess is object-safe by creating a trait object.
        let tree = sample_tree();
        let _dyn_ref: &dyn FormAccess = &tree;
        assert_eq!(_dyn_ref.form_type(), FormKind::AcroForm);
    }
}
