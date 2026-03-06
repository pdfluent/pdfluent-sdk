//! AcroForm engine for PDF interactive forms.
//!
//! Handles parsing, rendering, and manipulation of AcroForm (non-XFA) form fields.

/// A form field value.
#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    /// Text field value.
    Text(String),
    /// Choice field selection(s).
    Choice(Vec<String>),
    /// Button (checkbox/radio) state.
    Button(bool),
}

/// A single form field in the AcroForm field tree.
#[derive(Debug, Clone)]
pub struct Field {
    /// Fully qualified field name.
    pub name: String,
    /// Field type.
    pub field_type: FieldType,
    /// Current field value, if set.
    pub value: Option<FieldValue>,
}

/// The type of a form field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    /// Text input field.
    Text,
    /// Push button, checkbox, or radio button.
    Button,
    /// List box or combo box.
    Choice,
    /// Digital signature field.
    Signature,
}

/// The complete field tree of an AcroForm.
#[derive(Debug, Default)]
pub struct FieldTree {
    /// All fields in the form.
    pub fields: Vec<Field>,
}

/// An appearance stream for rendering a field widget.
#[derive(Debug, Clone)]
pub struct AppearanceStream {
    /// Raw PDF content stream data.
    pub data: Vec<u8>,
}

/// Core trait for AcroForm engines.
///
/// Implementors provide field tree access, value get/set, and flattening
/// (converting interactive fields into static appearance streams).
pub trait FormEngine {
    /// Returns the form's field tree.
    fn field_tree(&self) -> &FieldTree;

    /// Gets the current value of a field.
    fn get_field_value(&self, field: &Field) -> Option<FieldValue>;

    /// Sets a field's value.
    fn set_field_value(&mut self, field: &Field, value: FieldValue);

    /// Flattens all form fields into static appearance streams,
    /// removing interactivity.
    fn flatten(&self) -> Vec<AppearanceStream>;
}
