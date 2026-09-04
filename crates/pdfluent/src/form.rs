//! Form field reading and mutation.
//!
//! # Field hierarchy
//!
//! The write side ([`PdfFormMut`]) delegates to the pdf-forms writeback
//! chain, which recurses through `/Kids`: hierarchical fully-qualified
//! names (`Address.Street`, `3e.0`) are fully addressable. Every set
//! operation updates `/V`, keeps widget `/AS` in sync, regenerates `/AP`
//! appearance streams, and falls back to `/NeedAppearances` only when a
//! value cannot be drawn with the Standard-14 WinAnsi fonts.
//!
//! The legacy flat read surface ([`PdfDocument::form_fields`]) still walks
//! top-level fields only; use [`PdfDocument::form_model`] for the complete
//! hierarchical model with widget geometry and kind-specific data.
//!
//! [`PdfDocument::form_fields`]: crate::PdfDocument::form_fields
//! [`PdfDocument::form_model`]: crate::PdfDocument::form_model

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use crate::capability::Capability;
use crate::error::{internal_error, Result};
use crate::license;

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
/// surface on the individual setter calls when a field does not exist
/// or has an incompatible type.
///
/// This is the immediate-apply pattern: each setter mutates the in-memory
/// document immediately, so there is no separate `commit()` step.
///
/// Mutations are applied to the in-memory `lopdf::Document` and are flushed
/// to disk at the next `save` / `save_with` / `to_bytes` call.
pub struct PdfFormMut<'a> {
    lopdf: &'a mut lopdf::Document,
    /// Per-document license-key override, propagated from
    /// [`crate::OpenOptions::with_license_key`] so that per-doc tier
    /// overrides apply to form-fill operations.
    license_override: Option<&'a str>,
}

impl std::fmt::Debug for PdfFormMut<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The handle is a short-lived borrow into the document; expose
        // only the fact that we hold one so `expect_err` / debug prints
        // don't dump the entire lopdf tree.
        f.debug_struct("PdfFormMut").finish_non_exhaustive()
    }
}

impl<'a> PdfFormMut<'a> {
    /// Internal constructor. Not part of the public surface.
    pub(crate) fn new(lopdf: &'a mut lopdf::Document, license_override: Option<&'a str>) -> Self {
        Self {
            lopdf,
            license_override,
        }
    }

    /// Set a text field value.
    ///
    /// # Errors
    ///
    /// - [`crate::Error::FeatureNotInTier`] if the active tier does not grant
    ///   [`Capability::AcroFormFill`].
    /// - [`crate::Error::Internal`] when the field is not found or is not a
    ///   text field.
    pub fn set_text(&mut self, name: &str, value: &str) -> Result<&mut Self> {
        self.require_fill()?;
        apply(self.lopdf, name, pdf_forms::WriteValue::Text(value))?;
        Ok(self)
    }

    /// Set a checkbox state.
    ///
    /// Uses the convention `/Yes` for *on* and `/Off` for *off*. When the
    /// field's widget annotation declares a non-standard on-state via its
    /// `/AP/N` dictionary (any name other than `/Off`), that name is used
    /// instead of `/Yes`.
    ///
    /// # Errors
    ///
    /// As for [`set_text`](Self::set_text), plus the same tier check.
    pub fn set_checkbox(&mut self, name: &str, value: bool) -> Result<&mut Self> {
        self.require_fill()?;
        apply(self.lopdf, name, pdf_forms::WriteValue::Checkbox(value))?;
        Ok(self)
    }

    /// Select a radio option.
    ///
    /// `value` must be the exported value of the target radio option (the
    /// name that appears as a key in any kid widget's `/AP/N` appearance
    /// dictionary — typically the label written into the PDF by the form
    /// author).
    ///
    /// # Errors
    ///
    /// As for [`set_text`](Self::set_text), plus the same tier check.
    pub fn set_radio(&mut self, name: &str, value: &str) -> Result<&mut Self> {
        self.require_fill()?;
        apply(self.lopdf, name, pdf_forms::WriteValue::Radio(value))?;
        Ok(self)
    }

    /// Set a dropdown selection.
    ///
    /// `value` must match one of the export values in the field's `/Opt`
    /// array. The SDK stores `value` verbatim in `/V`; authors that
    /// distinguish display-label from export-value pairs must pass the
    /// export value.
    ///
    /// # Errors
    ///
    /// As for [`set_text`](Self::set_text), plus the same tier check.
    pub fn set_dropdown(&mut self, name: &str, value: &str) -> Result<&mut Self> {
        self.require_fill()?;
        apply(self.lopdf, name, pdf_forms::WriteValue::Choice(value))?;
        Ok(self)
    }

    /// Select multiple options on a multi-select list box (`/Ff` MultiSelect
    /// bit 22).
    ///
    /// Each entry of `values` must match an export or display value in the
    /// field's `/Opt` array (for non-editable list boxes). The SDK writes
    /// `/V` as an array of text strings and rebuilds `/I` as the sorted
    /// selected-index cache, matching what Adobe Acrobat produces. Pass an
    /// empty slice to clear the selection.
    ///
    /// Per-option highlight rendering is viewer-native, so this path sets
    /// `/NeedAppearances` rather than synthesising an appearance stream.
    ///
    /// # Errors
    ///
    /// As for [`set_text`](Self::set_text), plus the same tier check. Returns
    /// an error if the field is not a multi-select list box or if any value
    /// is not in `/Opt` (non-editable fields).
    pub fn set_multi_select(&mut self, name: &str, values: &[&str]) -> Result<&mut Self> {
        self.require_fill()?;
        let owned: Vec<String> = values.iter().map(|s| (*s).to_string()).collect();
        pdf_forms::apply_choice_multi(self.lopdf, name, &owned)
            .map_err(|e| internal_error(e.to_string()))?;
        Ok(self)
    }

    fn require_fill(&self) -> Result<()> {
        license::require_capability_with_override(Capability::AcroFormFill, self.license_override)
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

        let flags = field_dict
            .get(b"Ff")
            .ok()
            .and_then(|o| match o {
                Object::Integer(i) => Some(*i),
                _ => None,
            })
            .unwrap_or(0);

        let field_type = field_dict
            .get(b"FT")
            .ok()
            .and_then(|o| match o {
                Object::Name(bytes) => Some(bytes.as_slice()),
                _ => None,
            })
            .map(|ft| classify_field_type(ft, flags))
            .unwrap_or(FieldType::Text);

        // `/V` is either a text string (text / dropdown / listbox fields)
        // or a name (button/checkbox/radio fields). The name form isn't
        // accepted by `decode_text_string`, so handle both explicitly.
        let value = field_dict
            .get(b"V")
            .ok()
            .and_then(|o| match o {
                Object::Name(bytes) => std::str::from_utf8(bytes).ok().map(str::to_owned),
                _ => lopdf::decode_text_string(o).ok(),
            })
            .unwrap_or_default();

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

/// Classify a PDF `/FT` value into a [`FieldType`], using the `/Ff` flag
/// bits to distinguish Checkbox/Radio/PushButton and Combo/ListBox.
///
/// PDF 32000 §12.7.4.2 (button flags): bit 16 (0x8000) = Radio, bit 17
/// (0x10000) = PushButton. §12.7.4.4 (choice flags): bit 18 (0x20000) =
/// Combo.
fn classify_field_type(ft: &[u8], flags: i64) -> FieldType {
    match ft {
        b"Tx" => {
            // Bit 13 (0x1000) = Multiline → TextArea per §12.7.4.3.
            if (flags & 0x1000) != 0 {
                FieldType::TextArea
            } else {
                FieldType::Text
            }
        }
        b"Btn" => {
            if (flags & 0x10000) != 0 {
                FieldType::Button
            } else if (flags & 0x8000) != 0 {
                FieldType::Radio
            } else {
                FieldType::Checkbox
            }
        }
        b"Ch" => {
            if (flags & 0x20000) != 0 {
                FieldType::Dropdown
            } else {
                FieldType::ListBox
            }
        }
        b"Sig" => FieldType::Signature,
        _ => FieldType::Text,
    }
}

// ---------------------------------------------------------------------------
// Mutation: delegated to the pdf-forms writeback chain (#acroform-foundation)
// ---------------------------------------------------------------------------

/// Apply a value through [`pdf_forms::apply_field_value`] — the single
/// SDK writeback chain (updates `/V`, widget `/AS`, regenerates `/AP`, and
/// only falls back to `/NeedAppearances` for non-WinAnsi text). Errors are
/// mapped onto the crate's error type with stable, descriptive messages.
fn apply(
    doc: &mut lopdf::Document,
    name: &str,
    value: pdf_forms::WriteValue<'_>,
) -> Result<pdf_forms::WriteOutcome> {
    pdf_forms::apply_field_value(doc, name, value).map_err(|e| internal_error(e.to_string()))
}
