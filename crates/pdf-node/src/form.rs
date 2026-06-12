//! Form field access bindings for Node.js.

use napi_derive::napi;
use pdf_forms::facade::FormAccess;
use pdf_forms::{parse_acroform, FieldTree, FieldType, FieldValue};
use pdf_syntax::Pdf;
use std::sync::Mutex;

/// Information about a single form field.
#[napi(object)]
pub struct FormFieldInfo {
    /// Fully qualified field name (e.g., "form.name").
    pub name: String,
    /// Field type: "text", "button", "choice", or "signature".
    pub field_type: String,
    /// Current value, if any.
    pub value: Option<String>,
    /// Whether the field is read-only.
    pub read_only: bool,
}

/// Form engine wrapping a parsed AcroForm field tree.
pub(crate) struct FormEngine {
    tree: Mutex<FieldTree>,
}

impl FormEngine {
    pub fn from_pdf(pdf: &Pdf) -> Option<Self> {
        let tree = parse_acroform(pdf)?;
        Some(Self {
            tree: Mutex::new(tree),
        })
    }

    pub fn fields(&self) -> Vec<FormFieldInfo> {
        let tree = self.tree.lock().unwrap();
        tree.terminal_fields()
            .into_iter()
            .map(|id| {
                let name = tree.fully_qualified_name(id);
                let ft = tree.effective_field_type(id);
                let field_type = match ft {
                    Some(FieldType::Text) => "text",
                    Some(FieldType::Button) => "button",
                    Some(FieldType::Choice) => "choice",
                    Some(FieldType::Signature) => "signature",
                    None => "unknown",
                };
                let value = tree.effective_value(id).map(|v| match v {
                    FieldValue::Text(s) => s.clone(),
                    FieldValue::StringArray(arr) => arr.join(", "),
                });
                let read_only = matches!(ft, Some(FieldType::Signature))
                    || tree.effective_flags(id).read_only();
                FormFieldInfo {
                    name,
                    field_type: field_type.into(),
                    value,
                    read_only,
                }
            })
            .collect()
    }

    pub fn get_value(&self, name: &str) -> Option<String> {
        let tree = self.tree.lock().unwrap();
        tree.get_value(name)
    }

    pub fn set_value(&self, name: &str, value: &str) -> napi::Result<()> {
        let mut tree = self.tree.lock().unwrap();
        tree.set_value(name, value)
            .map_err(|e| napi::Error::from_reason(format!("{e}")))
    }
}
