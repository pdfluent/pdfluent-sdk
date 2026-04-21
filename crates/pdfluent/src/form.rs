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
/// Returned by [`crate::PdfDocument::form_mut`] unconditionally — the handle
/// is always constructable, even if the document has no form. Errors
/// surface on the individual setter calls when a field does not exist.
pub struct PdfFormMut<'a> {
    _doc: std::marker::PhantomData<&'a mut crate::PdfDocument>,
}

impl<'a> PdfFormMut<'a> {
    /// Set a text field value.
    ///
    /// # Errors
    ///
    /// - [`crate::Error::Internal`] wrapping `FieldNotFound` when the field
    ///   does not exist in the document.
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

// ---------------------------------------------------------------------------
// Read helpers for `PdfDocument::form_fields` (Epic 2 #1245)
// ---------------------------------------------------------------------------

/// Walk the lopdf AcroForm dictionary and return a flat list of
/// [`FormField`] values.
///
/// Returns an empty Vec when the document has no AcroForm, no catalog, or
/// an empty `/Fields` array. XFA-only documents also return an empty Vec;
/// XFA field enumeration is tracked separately.
pub(crate) fn read_acroform_fields(doc: &lopdf::Document) -> Vec<FormField> {
    use lopdf::Object;

    let catalog_id = match doc.trailer.get(b"Root") {
        Ok(Object::Reference(id)) => *id,
        _ => return Vec::new(),
    };
    let catalog = match doc.get_object(catalog_id).and_then(|o| o.as_dict()) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    let acroform = match catalog.get(b"AcroForm") {
        Ok(Object::Reference(id)) => match doc.get_object(*id).and_then(|o| o.as_dict()) {
            Ok(d) => d,
            Err(_) => return Vec::new(),
        },
        Ok(Object::Dictionary(d)) => d,
        _ => return Vec::new(),
    };
    let fields_array = match acroform.get(b"Fields") {
        Ok(Object::Array(arr)) => arr,
        _ => return Vec::new(),
    };

    let mut out = Vec::with_capacity(fields_array.len());
    for field_obj in fields_array {
        let field_dict = match field_obj {
            Object::Reference(id) => match doc.get_object(*id).and_then(|o| o.as_dict()) {
                Ok(d) => d,
                Err(_) => continue,
            },
            Object::Dictionary(d) => d,
            _ => continue,
        };

        let name = field_dict
            .get(b"T")
            .ok()
            .and_then(|o| lopdf::decode_text_string(o).ok())
            .unwrap_or_default();

        let field_type = field_dict
            .get(b"FT")
            .ok()
            .and_then(|o| match o {
                Object::Name(bytes) => Some(bytes.as_slice()),
                _ => None,
            })
            .map(classify_field_type)
            .unwrap_or(FieldType::Text);

        let value = field_dict
            .get(b"V")
            .ok()
            .and_then(|o| lopdf::decode_text_string(o).ok())
            .unwrap_or_default();

        let flags = field_dict
            .get(b"Ff")
            .ok()
            .and_then(|o| match o {
                Object::Integer(i) => Some(*i),
                _ => None,
            })
            .unwrap_or(0);

        // PDF 32000 §12.7.3.1: ReadOnly = bit 1 (0x1), Required = bit 2 (0x2).
        let read_only = (flags & 0x1) != 0;
        let required = (flags & 0x2) != 0;

        out.push(FormField {
            name,
            field_type,
            value,
            required,
            read_only,
        });
    }
    out
}

fn classify_field_type(ft: &[u8]) -> FieldType {
    match ft {
        b"Tx" => FieldType::Text,
        // `Btn` can be Button/Checkbox/Radio; distinguishing requires the
        // Ff flag bits which we defer to the mutation wiring.
        b"Btn" => FieldType::Checkbox,
        // `Ch` can be Combo/List; Combo is the common case.
        b"Ch" => FieldType::Dropdown,
        b"Sig" => FieldType::Signature,
        _ => FieldType::Text,
    }
}
