//! Form field reading and mutation.

use crate::error::Result;

/// Field type of a form field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FieldType {
    /// Single-line text input.
    Text,
    /// Multi-line text input.
    TextArea,
    /// Single checkbox.
    Checkbox,
    /// Radio button group.
    Radio,
    /// Combobox / dropdown.
    Dropdown,
    /// List box (multi-select).
    ListBox,
    /// Signature field (unfilled).
    Signature,
    /// Push button.
    Button,
}

/// A form field, read-only view.
#[derive(Debug, Clone)]
pub struct FormField {
    /// Field name as it appears in the PDF.
    pub name: String,
    /// Field type.
    pub field_type: FieldType,
    /// Current value as a string.
    pub value: String,
    /// Whether the field is required.
    pub required: bool,
    /// Whether the field is read-only.
    pub read_only: bool,
}

/// Mutable form handle.
///
/// Use via [`crate::PdfDocument::form_mut`]. Operations take effect on the
/// owning document immediately; there is no commit step.
pub struct PdfFormMut<'a> {
    _doc: std::marker::PhantomData<&'a mut crate::PdfDocument>,
}

impl<'a> PdfFormMut<'a> {
    /// Set a text field value.
    pub fn set_text(&mut self, _name: &str, _value: &str) -> Result<&mut Self> {
        unimplemented!("Epic 2 #1245");
    }

    /// Set a checkbox state.
    pub fn set_checkbox(&mut self, _name: &str, _value: bool) -> Result<&mut Self> {
        unimplemented!("Epic 2 #1245");
    }

    /// Select a radio option.
    pub fn set_radio(&mut self, _name: &str, _value: &str) -> Result<&mut Self> {
        unimplemented!("Epic 2 #1245");
    }

    /// Set a dropdown selection.
    pub fn set_dropdown(&mut self, _name: &str, _value: &str) -> Result<&mut Self> {
        unimplemented!("Epic 2 #1245");
    }
}
