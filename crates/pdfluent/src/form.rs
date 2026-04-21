//! Form field reading and mutation.
//!
//! # 1.0 scope — field hierarchy
//!
//! Both the read side ([`PdfDocument::form_fields`]) and the write side
//! ([`PdfFormMut`]) walk the top-level `/AcroForm/Fields` array and do **not**
//! recurse into `/Kids`. Hierarchical / fully-qualified field names
//! (`Address.Street`) are therefore not addressable in 1.0 — forms
//! authored as flat top-level fields (the common convention) work; forms
//! that nest fields through `/Kids` parents are out of scope until 1.1.
//!
//! This is an honest-now/fix-later limitation; attempting to set a kid by
//! its fully-qualified name surfaces as `field not found` rather than
//! silently mutating the parent.
//!
//! [`PdfDocument::form_fields`]: crate::PdfDocument::form_fields

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
        set_field_value(self.lopdf, name, FieldKind::Text(value))?;
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
        set_field_value(self.lopdf, name, FieldKind::Checkbox(value))?;
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
        set_field_value(self.lopdf, name, FieldKind::Radio(value))?;
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
        set_field_value(self.lopdf, name, FieldKind::Dropdown(value))?;
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
// Mutation helpers (Epic 3 #1245)
// ---------------------------------------------------------------------------

enum FieldKind<'a> {
    Text(&'a str),
    Checkbox(bool),
    Radio(&'a str),
    Dropdown(&'a str),
}

/// Locate a field by its top-level partial name and apply `kind`.
///
/// Two-pass structure avoids borrow-checker clashes: pass 1 walks the
/// AcroForm with an immutable `&lopdf::Document` borrow to resolve the
/// target object id; pass 2 takes a mutable borrow to write `/V`.
fn set_field_value(doc: &mut lopdf::Document, name: &str, kind: FieldKind<'_>) -> Result<()> {
    let target = locate_field(doc, name)?;
    apply_value(doc, target, name, kind)
}

/// Where a field dictionary lives in the document.
#[derive(Debug, Clone, Copy)]
enum FieldLocation {
    /// Indirect object — mutate via `doc.get_object_mut(id)`.
    Indirect(lopdf::ObjectId),
    /// Direct dictionary inline in `/AcroForm/Fields` — mutate via the
    /// array index on the AcroForm dict (itself referenced by id).
    DirectInAcroForm {
        acroform_id: lopdf::ObjectId,
        index: usize,
    },
}

fn locate_field(doc: &lopdf::Document, name: &str) -> Result<FieldLocation> {
    use lopdf::Object;

    let catalog_id = doc
        .trailer
        .get(b"Root")
        .ok()
        .and_then(|o| match o {
            Object::Reference(id) => Some(*id),
            _ => None,
        })
        .ok_or_else(|| internal_error("document has no /Root entry in trailer"))?;

    let catalog = doc
        .get_object(catalog_id)
        .and_then(|o| o.as_dict())
        .map_err(|_| internal_error("document catalog is not a dictionary"))?;

    let acroform_id = match catalog.get(b"AcroForm") {
        Ok(Object::Reference(id)) => *id,
        Ok(Object::Dictionary(_)) => {
            // Direct AcroForm dicts don't carry an object id we can reach
            // for mutation via get_object_mut. Real-world forms almost
            // always store AcroForm as an indirect object; reject the
            // edge case with a clear error.
            return Err(internal_error(
                "document has an inline /AcroForm dictionary; form mutation requires an indirect AcroForm object",
            ));
        }
        _ => return Err(internal_error("document has no /AcroForm dictionary")),
    };

    let acroform = doc
        .get_object(acroform_id)
        .and_then(|o| o.as_dict())
        .map_err(|_| internal_error("/AcroForm is not a dictionary"))?;

    let fields_array = match acroform.get(b"Fields") {
        Ok(Object::Array(arr)) => arr,
        _ => return Err(internal_error("/AcroForm has no /Fields array")),
    };

    for (index, field_obj) in fields_array.iter().enumerate() {
        let (field_dict, loc) = match field_obj {
            Object::Reference(id) => {
                let d = doc
                    .get_object(*id)
                    .and_then(|o| o.as_dict())
                    .map_err(|_| internal_error("field entry is not a dictionary"))?;
                (d, FieldLocation::Indirect(*id))
            }
            Object::Dictionary(d) => (d, FieldLocation::DirectInAcroForm { acroform_id, index }),
            _ => continue,
        };
        let t = field_dict
            .get(b"T")
            .ok()
            .and_then(|o| lopdf::decode_text_string(o).ok())
            .unwrap_or_default();
        if t == name {
            return Ok(loc);
        }
    }

    Err(internal_error(format!("form field '{name}' not found")))
}

fn apply_value(
    doc: &mut lopdf::Document,
    loc: FieldLocation,
    name: &str,
    kind: FieldKind<'_>,
) -> Result<()> {
    use lopdf::Object;

    // Resolve the on-state for checkboxes BEFORE taking the mutable
    // borrow, since we need to read widget /AP dicts.
    let checkbox_on_state = if let FieldKind::Checkbox(true) = kind {
        Some(resolve_checkbox_on_state(doc, loc))
    } else {
        None
    };

    let field_dict: &mut lopdf::Dictionary = match loc {
        FieldLocation::Indirect(id) => doc
            .get_object_mut(id)
            .and_then(|o| o.as_dict_mut())
            .map_err(|_| internal_error("field object vanished between locate and apply"))?,
        FieldLocation::DirectInAcroForm { acroform_id, index } => {
            let acroform = doc
                .get_object_mut(acroform_id)
                .and_then(|o| o.as_dict_mut())
                .map_err(|_| internal_error("/AcroForm not mutably accessible"))?;
            let fields = match acroform.get_mut(b"Fields") {
                Ok(Object::Array(arr)) => arr,
                _ => return Err(internal_error("/AcroForm/Fields changed shape")),
            };
            match fields.get_mut(index) {
                Some(Object::Dictionary(d)) => d,
                _ => return Err(internal_error("direct field dict changed shape")),
            }
        }
    };

    let actual = field_type_of(field_dict);

    match kind {
        FieldKind::Text(value) => {
            if !matches!(actual, FieldType::Text | FieldType::TextArea) {
                return Err(wrong_type_error(name, actual, "text"));
            }
            // Use `lopdf::text_string` so non-ASCII values round-trip:
            // PDFDocEncoding for ASCII, UTF-16BE with BOM otherwise.
            // `decode_text_string` recognises both forms on read-back.
            field_dict.set("V", lopdf::text_string(value));
        }
        FieldKind::Checkbox(on) => {
            if actual != FieldType::Checkbox {
                return Err(wrong_type_error(name, actual, "checkbox"));
            }
            let state: Vec<u8> = if on {
                checkbox_on_state.unwrap_or_else(|| b"Yes".to_vec())
            } else {
                b"Off".to_vec()
            };
            field_dict.set("V", Object::Name(state.clone()));
            // Keep widget appearance state (/AS) in sync with /V for the
            // common case where the field dict itself is the widget
            // (no /Kids). Kid-based button widgets need per-kid /AS
            // updates which is tracked separately.
            if field_dict.get(b"Kids").is_err() {
                field_dict.set("AS", Object::Name(state));
            }
        }
        FieldKind::Radio(value) => {
            if actual != FieldType::Radio {
                return Err(wrong_type_error(name, actual, "radio"));
            }
            field_dict.set("V", Object::Name(value.as_bytes().to_vec()));
        }
        FieldKind::Dropdown(value) => {
            if !matches!(actual, FieldType::Dropdown | FieldType::ListBox) {
                return Err(wrong_type_error(name, actual, "dropdown"));
            }
            // Same Unicode-safe encoding as text fields.
            field_dict.set("V", lopdf::text_string(value));
            // Drop stale /I index — the selected-index cache is only
            // valid against a specific /V; clearing it forces readers
            // to recompute from /V + /Opt.
            let _ = field_dict.remove(b"I");
        }
    }

    Ok(())
}

/// Inspect a field dictionary and return its effective [`FieldType`].
fn field_type_of(field_dict: &lopdf::Dictionary) -> FieldType {
    use lopdf::Object;

    let flags = field_dict
        .get(b"Ff")
        .ok()
        .and_then(|o| match o {
            Object::Integer(i) => Some(*i),
            _ => None,
        })
        .unwrap_or(0);

    field_dict
        .get(b"FT")
        .ok()
        .and_then(|o| match o {
            Object::Name(bytes) => Some(bytes.as_slice()),
            _ => None,
        })
        .map(|ft| classify_field_type(ft, flags))
        .unwrap_or(FieldType::Text)
}

/// Walk `/AP/N` on a checkbox field dict — and any widget kid — to find
/// the first name that isn't `/Off` — that's the on-state. Defaults to
/// `Yes` when no appearance dict yields a candidate, matching the
/// PDF-author convention used by most tooling (including the fixture).
///
/// Many real forms declare the on-state on widget kids (e.g. `/On1` on
/// `/Kids[0]/AP/N`), not on the parent field, so checking only the
/// parent would silently produce `/Yes` for unrelated states and leave
/// the checkbox visually off in viewers that honour `/AS`.
fn resolve_checkbox_on_state(doc: &lopdf::Document, loc: FieldLocation) -> Vec<u8> {
    let default: Vec<u8> = b"Yes".to_vec();

    let field_dict = match loc {
        FieldLocation::Indirect(id) => match doc.get_object(id).and_then(|o| o.as_dict()) {
            Ok(d) => d,
            Err(_) => return default,
        },
        FieldLocation::DirectInAcroForm { acroform_id, index } => {
            use lopdf::Object;
            let acroform = match doc.get_object(acroform_id).and_then(|o| o.as_dict()) {
                Ok(d) => d,
                Err(_) => return default,
            };
            let fields = match acroform.get(b"Fields") {
                Ok(Object::Array(arr)) => arr,
                _ => return default,
            };
            match fields.get(index) {
                Some(Object::Dictionary(d)) => d,
                _ => return default,
            }
        }
    };

    // 1. Parent-level /AP/N.
    if let Some(state) = on_state_from_ap(doc, field_dict) {
        return state;
    }

    // 2. Walk widget kids. Each kid is either an indirect reference or
    //    an inline dictionary. Resolve both shapes.
    if let Ok(lopdf::Object::Array(kids)) = field_dict.get(b"Kids") {
        for kid in kids {
            let kid_dict = match kid {
                lopdf::Object::Reference(id) => {
                    match doc.get_object(*id).and_then(|o| o.as_dict()) {
                        Ok(d) => d,
                        Err(_) => continue,
                    }
                }
                lopdf::Object::Dictionary(d) => d,
                _ => continue,
            };
            if let Some(state) = on_state_from_ap(doc, kid_dict) {
                return state;
            }
        }
    }

    default
}

/// Extract the first non-`Off` key from `dict./AP/N`, or `None` if the
/// appearance dictionary is absent or only declares `/Off`.
fn on_state_from_ap(doc: &lopdf::Document, dict: &lopdf::Dictionary) -> Option<Vec<u8>> {
    use lopdf::Object;

    let ap = match dict.get(b"AP").ok()? {
        Object::Reference(id) => doc.get_object(*id).and_then(|o| o.as_dict()).ok()?,
        Object::Dictionary(d) => d,
        _ => return None,
    };

    let n = match ap.get(b"N").ok()? {
        Object::Reference(id) => doc.get_object(*id).and_then(|o| o.as_dict()).ok()?,
        Object::Dictionary(d) => d,
        _ => return None,
    };

    for (key, _) in n.iter() {
        if key.as_slice() != b"Off" {
            return Some(key.to_vec());
        }
    }
    None
}

fn wrong_type_error(name: &str, actual: FieldType, requested: &str) -> crate::error::Error {
    internal_error(format!(
        "form field '{name}' has type {actual:?}; cannot apply {requested} mutation",
    ))
}
