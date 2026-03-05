//! FieldMDP signature detection and removal.
//!
//! FieldMDP (Field Modification Detection and Prevention) signatures lock
//! specific form fields in a PDF document. They are stored as signature
//! fields in the AcroForm dictionary with a `Reference` entry whose
//! `TransformMethod` is `FieldMDP`.
//!
//! The `TransformParams` dictionary specifies:
//! - `Action`: "All" (lock all fields), "Include" (lock listed fields),
//!   or "Exclude" (lock all except listed fields)
//! - `Fields`: array of field names affected by the lock
//!
//! Removing a FieldMDP signature unlocks the specified fields for editing
//! while preserving other signatures (DocMDP, UR3, other FieldMDP).

use crate::error::{PdfError, Result};
use crate::pdf_reader::PdfReader;
use lopdf::{Dictionary, Object, ObjectId};

/// The lock action specified by a FieldMDP signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldLockAction {
    /// Lock all fields in the document.
    All,
    /// Lock only the listed fields.
    Include(Vec<String>),
    /// Lock all fields except the listed ones.
    Exclude(Vec<String>),
}

/// Information about a detected FieldMDP signature.
#[derive(Debug, Clone)]
pub struct FieldMdpInfo {
    /// The signature field name (T value).
    pub field_name: Option<String>,
    /// The signature sub-filter.
    pub sub_filter: Option<String>,
    /// Whether a byte range is present.
    pub has_byte_range: bool,
    /// Whether PKCS#7 contents are present.
    pub has_pkcs7: bool,
    /// The lock action and affected fields.
    pub lock_action: Option<FieldLockAction>,
    /// The object ID of the signature field.
    pub field_object_id: ObjectId,
    /// The object ID of the signature value dictionary (V).
    pub sig_value_id: Option<ObjectId>,
}

/// Per-field lock status report.
#[derive(Debug, Clone)]
pub struct FieldLockStatus {
    /// Name of the form field.
    pub field_name: String,
    /// Whether this field is locked.
    pub locked: bool,
    /// Which FieldMDP signature(s) lock this field (by field name).
    pub locked_by: Vec<String>,
}

/// Detect all FieldMDP signatures in a PDF.
///
/// Returns a list of all FieldMDP signatures found in the AcroForm fields.
pub fn detect_fieldmdp(reader: &PdfReader) -> Result<Vec<FieldMdpInfo>> {
    let doc = reader.document();
    let mut results = Vec::new();

    let catalog = doc
        .trailer
        .get_deref(b"Root", doc)
        .and_then(|o| o.as_dict())
        .map_err(|_| PdfError::XfaPacketNotFound("no Root catalog".to_string()))?;

    let acroform = match catalog.get_deref(b"AcroForm", doc) {
        Ok(Object::Dictionary(d)) => d,
        _ => return Ok(results),
    };

    let fields = match acroform.get(b"Fields") {
        Ok(Object::Array(a)) => a,
        Ok(Object::Reference(r)) => match doc.get_object(*r) {
            Ok(Object::Array(a)) => a,
            _ => return Ok(results),
        },
        _ => return Ok(results),
    };

    collect_fieldmdp_recursive(doc, fields, &mut results);

    Ok(results)
}

/// Recursively search fields (including Kids) for FieldMDP signatures.
fn collect_fieldmdp_recursive(
    doc: &lopdf::Document,
    fields: &[Object],
    results: &mut Vec<FieldMdpInfo>,
) {
    for field_ref in fields {
        let field_id = match field_ref {
            Object::Reference(r) => *r,
            _ => continue,
        };

        let field_dict = match doc.get_object(field_id) {
            Ok(Object::Dictionary(d)) => d,
            _ => continue,
        };

        // Recurse into Kids
        if let Ok(Object::Array(kids)) = field_dict.get(b"Kids") {
            collect_fieldmdp_recursive(doc, kids, results);
        }

        // Only look at Sig fields
        if !is_sig_field(field_dict) {
            continue;
        }

        // Check if this signature has a FieldMDP reference
        let sig_value_id = match field_dict.get(b"V") {
            Ok(Object::Reference(r)) => Some(*r),
            _ => None,
        };

        let sig_dict = sig_value_id.and_then(|id| match doc.get_object(id) {
            Ok(Object::Dictionary(d)) => Some(d),
            _ => None,
        });

        let lock_action = sig_dict.and_then(|d| extract_fieldmdp_action(d, doc));

        // Only include if this is actually a FieldMDP signature
        if lock_action.is_none() {
            continue;
        }

        let field_name = field_dict.get(b"T").ok().and_then(|o| match o {
            Object::String(s, _) => Some(String::from_utf8_lossy(s).to_string()),
            _ => None,
        });

        let (sub_filter, has_byte_range, has_pkcs7) = if let Some(sd) = sig_dict {
            (
                extract_name_value(sd, b"SubFilter"),
                sd.get(b"ByteRange").is_ok(),
                sd.get(b"Contents").is_ok(),
            )
        } else {
            (None, false, false)
        };

        results.push(FieldMdpInfo {
            field_name,
            sub_filter,
            has_byte_range,
            has_pkcs7,
            lock_action,
            field_object_id: field_id,
            sig_value_id,
        });
    }
}

/// Report per-field lock status based on all FieldMDP signatures.
///
/// Scans all AcroForm fields and determines which are locked by FieldMDP
/// signatures. Returns a status entry for each non-signature field.
pub fn field_lock_status(reader: &PdfReader) -> Result<Vec<FieldLockStatus>> {
    let signatures = detect_fieldmdp(reader)?;
    let doc = reader.document();

    // Collect all non-signature field names
    let catalog = doc
        .trailer
        .get_deref(b"Root", doc)
        .and_then(|o| o.as_dict())
        .map_err(|_| PdfError::XfaPacketNotFound("no Root catalog".to_string()))?;

    let acroform = match catalog.get_deref(b"AcroForm", doc) {
        Ok(Object::Dictionary(d)) => d,
        _ => return Ok(vec![]),
    };

    let fields = match acroform.get(b"Fields") {
        Ok(Object::Array(a)) => a,
        _ => return Ok(vec![]),
    };

    let mut statuses = Vec::new();

    for field_ref in fields {
        let field_id = match field_ref {
            Object::Reference(r) => *r,
            _ => continue,
        };

        let field_dict = match doc.get_object(field_id) {
            Ok(Object::Dictionary(d)) => d,
            _ => continue,
        };

        // Skip signature fields themselves
        if is_sig_field(field_dict) {
            continue;
        }

        let name = match field_dict.get(b"T") {
            Ok(Object::String(s, _)) => String::from_utf8_lossy(s).to_string(),
            _ => continue,
        };

        let mut locked_by = Vec::new();

        for sig in &signatures {
            if is_field_locked(&name, &sig.lock_action) {
                if let Some(ref sig_name) = sig.field_name {
                    locked_by.push(sig_name.clone());
                } else {
                    locked_by.push(format!("({})", sig.field_object_id.0));
                }
            }
        }

        statuses.push(FieldLockStatus {
            field_name: name,
            locked: !locked_by.is_empty(),
            locked_by,
        });
    }

    Ok(statuses)
}

/// Remove a specific FieldMDP signature by field name.
///
/// Removes the signature field from AcroForm and its value dictionary,
/// unlocking the fields it protected. Other signatures are preserved.
///
/// Returns `true` if the signature was found and removed.
pub fn remove_fieldmdp(reader: &mut PdfReader, sig_field_name: &str) -> Result<bool> {
    let sigs = detect_fieldmdp(reader)?;

    let target = sigs
        .iter()
        .find(|s| s.field_name.as_deref().is_some_and(|n| n == sig_field_name));

    let target = match target {
        Some(t) => t.clone(),
        None => return Ok(false),
    };

    remove_fieldmdp_by_info(reader, &target)
}

/// Remove a FieldMDP signature by its object IDs.
fn remove_fieldmdp_by_info(reader: &mut PdfReader, info: &FieldMdpInfo) -> Result<bool> {
    let doc = reader.document();

    let catalog_ref = doc
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .map_err(|_| PdfError::XfaPacketNotFound("no Root in trailer".to_string()))?;

    let catalog = doc
        .get_object(catalog_ref)
        .and_then(|o| o.as_dict())
        .map_err(|_| PdfError::XfaPacketNotFound("Root not a dictionary".to_string()))?
        .clone();

    let acroform_ref = match catalog.get(b"AcroForm") {
        Ok(Object::Reference(r)) => Some(*r),
        _ => None,
    };
    let acroform_is_inline = matches!(catalog.get(b"AcroForm"), Ok(Object::Dictionary(_)));

    // Collect page annotation removals
    let page_annot_removals = collect_page_annotation_refs(doc, info.field_object_id);

    // --- Begin mutations ---
    let doc = reader.document_mut();

    // 1. Remove field from AcroForm.Fields
    if let Some(af_ref) = acroform_ref {
        // AcroForm is an indirect reference
        if let Ok(Object::Dictionary(ref mut af)) = doc.get_object_mut(af_ref) {
            if let Ok(Object::Array(ref mut fields)) = af.get_mut(b"Fields") {
                fields.retain(|f| !matches!(f, Object::Reference(r) if *r == info.field_object_id));
            }
        }
    } else if acroform_is_inline {
        // AcroForm is inline in the catalog dictionary
        if let Ok(Object::Dictionary(ref mut cat)) = doc.get_object_mut(catalog_ref) {
            if let Ok(Object::Dictionary(ref mut af)) = cat.get_mut(b"AcroForm") {
                if let Ok(Object::Array(ref mut fields)) = af.get_mut(b"Fields") {
                    fields.retain(
                        |f| !matches!(f, Object::Reference(r) if *r == info.field_object_id),
                    );
                }
            }
        }
    }

    // 2. Remove the signature value object
    if let Some(sig_id) = info.sig_value_id {
        doc.objects.remove(&sig_id);
    }

    // 3. Remove the field object
    doc.objects.remove(&info.field_object_id);

    // 4. Remove annotations from pages
    for (page_id, annot_id) in page_annot_removals {
        // Check if Annots is indirect
        let annots_ref = if let Ok(Object::Dictionary(page)) = doc.get_object(page_id) {
            if let Ok(Object::Reference(r)) = page.get(b"Annots") {
                Some(*r)
            } else {
                None
            }
        } else {
            None
        };

        if let Some(annots_id) = annots_ref {
            // Annots is indirect — mutate the target array object
            if let Ok(Object::Array(ref mut annots)) = doc.get_object_mut(annots_id) {
                annots.retain(|a| !matches!(a, Object::Reference(r) if *r == annot_id));
            }
        } else {
            // Annots is inline in the page dictionary
            if let Ok(Object::Dictionary(ref mut page)) = doc.get_object_mut(page_id) {
                if let Ok(Object::Array(ref mut annots)) = page.get_mut(b"Annots") {
                    annots.retain(|a| !matches!(a, Object::Reference(r) if *r == annot_id));
                }
            }
        }
    }

    Ok(true)
}

/// Remove all FieldMDP signatures from a PDF.
///
/// Returns the number of signatures removed.
pub fn remove_all_fieldmdp(reader: &mut PdfReader) -> Result<usize> {
    let sigs = detect_fieldmdp(reader)?;
    let count = sigs.len();

    for sig in &sigs {
        remove_fieldmdp_by_info(reader, sig)?;
    }

    Ok(count)
}

/// Check if a field dictionary is a signature field (FT=Sig).
fn is_sig_field(dict: &Dictionary) -> bool {
    matches!(dict.get(b"FT"), Ok(Object::Name(n)) if n == b"Sig")
}

/// Extract the FieldMDP lock action from a signature dictionary's Reference array.
fn extract_fieldmdp_action(
    sig_dict: &Dictionary,
    doc: &lopdf::Document,
) -> Option<FieldLockAction> {
    let refs = sig_dict.get(b"Reference").ok()?;
    let arr = match refs {
        Object::Array(a) => a,
        _ => return None,
    };

    for item in arr {
        let ref_dict = match item {
            Object::Dictionary(d) => d,
            Object::Reference(r) => match doc.get_object(*r) {
                Ok(Object::Dictionary(d)) => d,
                _ => continue,
            },
            _ => continue,
        };

        // Check TransformMethod is FieldMDP
        let method = extract_name_value(ref_dict, b"TransformMethod")?;
        if method != "FieldMDP" {
            continue;
        }

        let params = match ref_dict.get(b"TransformParams") {
            Ok(Object::Dictionary(d)) => d,
            Ok(Object::Reference(r)) => match doc.get_object(*r) {
                Ok(Object::Dictionary(d)) => d,
                _ => continue,
            },
            _ => continue,
        };

        let action = extract_name_value(params, b"Action")?;
        let field_names = extract_string_array(params, b"Fields");

        return Some(match action.as_str() {
            "All" => FieldLockAction::All,
            "Include" => FieldLockAction::Include(field_names),
            "Exclude" => FieldLockAction::Exclude(field_names),
            _ => continue,
        });
    }

    None
}

/// Extract a Name value from a dictionary as a String.
fn extract_name_value(dict: &Dictionary, key: &[u8]) -> Option<String> {
    dict.get(key).ok().and_then(|o| match o {
        Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
        _ => None,
    })
}

/// Extract an array of strings from a dictionary.
fn extract_string_array(dict: &Dictionary, key: &[u8]) -> Vec<String> {
    match dict.get(key) {
        Ok(Object::Array(arr)) => arr
            .iter()
            .filter_map(|o| match o {
                Object::String(s, _) => Some(String::from_utf8_lossy(s).to_string()),
                Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
                _ => None,
            })
            .collect(),
        _ => vec![],
    }
}

/// Check whether a field is locked by a given lock action.
fn is_field_locked(field_name: &str, action: &Option<FieldLockAction>) -> bool {
    match action {
        Some(FieldLockAction::All) => true,
        Some(FieldLockAction::Include(fields)) => fields.iter().any(|f| f == field_name),
        Some(FieldLockAction::Exclude(fields)) => !fields.iter().any(|f| f == field_name),
        None => false,
    }
}

/// Collect (page_id, annotation_id) pairs referencing the given field.
fn collect_page_annotation_refs(
    doc: &lopdf::Document,
    field_id: ObjectId,
) -> Vec<(ObjectId, ObjectId)> {
    let mut removals = Vec::new();

    for (&page_id, page_obj) in &doc.objects {
        let page_dict = match page_obj {
            Object::Dictionary(d) => d,
            _ => continue,
        };

        if !matches!(page_dict.get(b"Type"), Ok(Object::Name(t)) if t == b"Page") {
            continue;
        }

        // Dereference Annots — may be inline array or indirect reference
        let annots = match page_dict.get(b"Annots") {
            Ok(Object::Array(a)) => a,
            Ok(Object::Reference(r)) => match doc.get_object(*r) {
                Ok(Object::Array(a)) => a,
                _ => continue,
            },
            _ => continue,
        };

        for annot_ref in annots {
            if let Object::Reference(annot_id) = annot_ref {
                if *annot_id == field_id {
                    removals.push((page_id, *annot_id));
                }
            }
        }
    }

    removals
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Document, Object, Stream};

    /// Build a PDF with a FieldMDP signature locking specific fields.
    fn build_pdf_with_fieldmdp(action: &str, locked_fields: &[&str]) -> Vec<u8> {
        let mut doc = Document::with_version("1.7");

        // Build the Fields array for TransformParams
        let fields_arr: Vec<Object> = locked_fields
            .iter()
            .map(|f| Object::String(f.as_bytes().to_vec(), lopdf::StringFormat::Literal))
            .collect();

        // Signature value dictionary with FieldMDP reference
        let sig_dict = dictionary! {
            "Type" => Object::Name(b"Sig".to_vec()),
            "Filter" => Object::Name(b"Adobe.PPKLite".to_vec()),
            "SubFilter" => Object::Name(b"adbe.pkcs7.detached".to_vec()),
            "ByteRange" => Object::Array(vec![0.into(), 100.into(), 200.into(), 300.into()]),
            "Contents" => Object::String(b"fake-pkcs7".to_vec(), lopdf::StringFormat::Hexadecimal),
            "Reference" => Object::Array(vec![
                Object::Dictionary(dictionary! {
                    "TransformMethod" => Object::Name(b"FieldMDP".to_vec()),
                    "TransformParams" => Object::Dictionary(dictionary! {
                        "Type" => Object::Name(b"TransformParams".to_vec()),
                        "Action" => Object::Name(action.as_bytes().to_vec()),
                        "Fields" => Object::Array(fields_arr),
                    }),
                }),
            ]),
        };
        let sig_id = doc.add_object(Object::Dictionary(sig_dict));

        // Signature field
        let ap_stream = Stream::new(Dictionary::new(), b"q Q".to_vec());
        let ap_id = doc.add_object(Object::Stream(ap_stream));
        let sig_field = dictionary! {
            "Type" => Object::Name(b"Annot".to_vec()),
            "Subtype" => Object::Name(b"Widget".to_vec()),
            "FT" => Object::Name(b"Sig".to_vec()),
            "T" => Object::String(b"FieldLock1".to_vec(), lopdf::StringFormat::Literal),
            "V" => Object::Reference(sig_id),
            "Rect" => Object::Array(vec![0.into(), 0.into(), 0.into(), 0.into()]),
            "AP" => Object::Dictionary(dictionary! {
                "N" => Object::Reference(ap_id),
            }),
        };
        let field_id = doc.add_object(Object::Dictionary(sig_field));

        // Regular form fields
        let name_field = dictionary! {
            "FT" => Object::Name(b"Tx".to_vec()),
            "T" => Object::String(b"Name".to_vec(), lopdf::StringFormat::Literal),
            "V" => Object::String(b"John".to_vec(), lopdf::StringFormat::Literal),
        };
        let name_id = doc.add_object(Object::Dictionary(name_field));

        let email_field = dictionary! {
            "FT" => Object::Name(b"Tx".to_vec()),
            "T" => Object::String(b"Email".to_vec(), lopdf::StringFormat::Literal),
            "V" => Object::String(b"john@example.com".to_vec(), lopdf::StringFormat::Literal),
        };
        let email_id = doc.add_object(Object::Dictionary(email_field));

        let amount_field = dictionary! {
            "FT" => Object::Name(b"Tx".to_vec()),
            "T" => Object::String(b"Amount".to_vec(), lopdf::StringFormat::Literal),
            "V" => Object::String(b"100".to_vec(), lopdf::StringFormat::Literal),
        };
        let amount_id = doc.add_object(Object::Dictionary(amount_field));

        // AcroForm
        let acroform = dictionary! {
            "Fields" => Object::Array(vec![
                Object::Reference(field_id),
                Object::Reference(name_id),
                Object::Reference(email_id),
                Object::Reference(amount_id),
            ]),
        };
        let acroform_id = doc.add_object(Object::Dictionary(acroform));

        // Page with annotation for the sig field
        let page = dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
            "Annots" => Object::Array(vec![Object::Reference(field_id)]),
        };
        let page_id = doc.add_object(Object::Dictionary(page));

        let pages = dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
            "Count" => Object::Integer(1),
        };
        let pages_id = doc.add_object(Object::Dictionary(pages));

        if let Ok(Object::Dictionary(ref mut p)) = doc.get_object_mut(page_id) {
            p.set("Parent", Object::Reference(pages_id));
        }

        let catalog = dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "Pages" => Object::Reference(pages_id),
            "AcroForm" => Object::Reference(acroform_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        buf
    }

    /// Build a PDF with two FieldMDP signatures.
    fn build_pdf_with_two_fieldmdp() -> Vec<u8> {
        let mut doc = Document::with_version("1.7");

        // First FieldMDP: locks Name
        let sig1 = dictionary! {
            "Type" => Object::Name(b"Sig".to_vec()),
            "Filter" => Object::Name(b"Adobe.PPKLite".to_vec()),
            "SubFilter" => Object::Name(b"adbe.pkcs7.detached".to_vec()),
            "Contents" => Object::String(b"sig1".to_vec(), lopdf::StringFormat::Hexadecimal),
            "Reference" => Object::Array(vec![
                Object::Dictionary(dictionary! {
                    "TransformMethod" => Object::Name(b"FieldMDP".to_vec()),
                    "TransformParams" => Object::Dictionary(dictionary! {
                        "Action" => Object::Name(b"Include".to_vec()),
                        "Fields" => Object::Array(vec![
                            Object::String(b"Name".to_vec(), lopdf::StringFormat::Literal),
                        ]),
                    }),
                }),
            ]),
        };
        let sig1_id = doc.add_object(Object::Dictionary(sig1));

        // Second FieldMDP: locks Email
        let sig2 = dictionary! {
            "Type" => Object::Name(b"Sig".to_vec()),
            "Filter" => Object::Name(b"Adobe.PPKLite".to_vec()),
            "SubFilter" => Object::Name(b"adbe.pkcs7.detached".to_vec()),
            "Contents" => Object::String(b"sig2".to_vec(), lopdf::StringFormat::Hexadecimal),
            "Reference" => Object::Array(vec![
                Object::Dictionary(dictionary! {
                    "TransformMethod" => Object::Name(b"FieldMDP".to_vec()),
                    "TransformParams" => Object::Dictionary(dictionary! {
                        "Action" => Object::Name(b"Include".to_vec()),
                        "Fields" => Object::Array(vec![
                            Object::String(b"Email".to_vec(), lopdf::StringFormat::Literal),
                        ]),
                    }),
                }),
            ]),
        };
        let sig2_id = doc.add_object(Object::Dictionary(sig2));

        let field1 = dictionary! {
            "FT" => Object::Name(b"Sig".to_vec()),
            "T" => Object::String(b"Lock1".to_vec(), lopdf::StringFormat::Literal),
            "V" => Object::Reference(sig1_id),
        };
        let field1_id = doc.add_object(Object::Dictionary(field1));

        let field2 = dictionary! {
            "FT" => Object::Name(b"Sig".to_vec()),
            "T" => Object::String(b"Lock2".to_vec(), lopdf::StringFormat::Literal),
            "V" => Object::Reference(sig2_id),
        };
        let field2_id = doc.add_object(Object::Dictionary(field2));

        let name_field = dictionary! {
            "FT" => Object::Name(b"Tx".to_vec()),
            "T" => Object::String(b"Name".to_vec(), lopdf::StringFormat::Literal),
        };
        let name_id = doc.add_object(Object::Dictionary(name_field));

        let email_field = dictionary! {
            "FT" => Object::Name(b"Tx".to_vec()),
            "T" => Object::String(b"Email".to_vec(), lopdf::StringFormat::Literal),
        };
        let email_id = doc.add_object(Object::Dictionary(email_field));

        let acroform = dictionary! {
            "Fields" => Object::Array(vec![
                Object::Reference(field1_id),
                Object::Reference(field2_id),
                Object::Reference(name_id),
                Object::Reference(email_id),
            ]),
        };
        let acroform_id = doc.add_object(Object::Dictionary(acroform));

        let pages = dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => Object::Array(vec![]),
            "Count" => Object::Integer(0),
        };
        let pages_id = doc.add_object(Object::Dictionary(pages));

        let catalog = dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "Pages" => Object::Reference(pages_id),
            "AcroForm" => Object::Reference(acroform_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        buf
    }

    #[test]
    fn detect_fieldmdp_include() {
        let pdf = build_pdf_with_fieldmdp("Include", &["Name", "Email"]);
        let reader = PdfReader::from_bytes(&pdf).unwrap();
        let sigs = detect_fieldmdp(&reader).unwrap();
        assert_eq!(sigs.len(), 1);
        let sig = &sigs[0];
        assert_eq!(sig.field_name.as_deref(), Some("FieldLock1"));
        assert_eq!(sig.sub_filter.as_deref(), Some("adbe.pkcs7.detached"));
        assert!(sig.has_byte_range);
        assert!(sig.has_pkcs7);
        assert_eq!(
            sig.lock_action,
            Some(FieldLockAction::Include(vec![
                "Name".to_string(),
                "Email".to_string()
            ]))
        );
    }

    #[test]
    fn detect_fieldmdp_exclude() {
        let pdf = build_pdf_with_fieldmdp("Exclude", &["Amount"]);
        let reader = PdfReader::from_bytes(&pdf).unwrap();
        let sigs = detect_fieldmdp(&reader).unwrap();
        assert_eq!(sigs.len(), 1);
        assert_eq!(
            sigs[0].lock_action,
            Some(FieldLockAction::Exclude(vec!["Amount".to_string()]))
        );
    }

    #[test]
    fn detect_fieldmdp_all() {
        let pdf = build_pdf_with_fieldmdp("All", &[]);
        let reader = PdfReader::from_bytes(&pdf).unwrap();
        let sigs = detect_fieldmdp(&reader).unwrap();
        assert_eq!(sigs.len(), 1);
        assert_eq!(sigs[0].lock_action, Some(FieldLockAction::All));
    }

    #[test]
    fn detect_fieldmdp_none_without_signatures() {
        let mut doc = Document::with_version("1.4");
        let pages = dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => Object::Array(vec![]),
            "Count" => Object::Integer(0),
        };
        let pages_id = doc.add_object(Object::Dictionary(pages));
        let catalog = dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();

        let reader = PdfReader::from_bytes(&buf).unwrap();
        let sigs = detect_fieldmdp(&reader).unwrap();
        assert!(sigs.is_empty());
    }

    #[test]
    fn field_lock_status_include() {
        let pdf = build_pdf_with_fieldmdp("Include", &["Name", "Email"]);
        let reader = PdfReader::from_bytes(&pdf).unwrap();
        let statuses = field_lock_status(&reader).unwrap();

        assert_eq!(statuses.len(), 3); // Name, Email, Amount

        let name = statuses.iter().find(|s| s.field_name == "Name").unwrap();
        assert!(name.locked);

        let email = statuses.iter().find(|s| s.field_name == "Email").unwrap();
        assert!(email.locked);

        let amount = statuses.iter().find(|s| s.field_name == "Amount").unwrap();
        assert!(!amount.locked);
    }

    #[test]
    fn field_lock_status_exclude() {
        let pdf = build_pdf_with_fieldmdp("Exclude", &["Amount"]);
        let reader = PdfReader::from_bytes(&pdf).unwrap();
        let statuses = field_lock_status(&reader).unwrap();

        let name = statuses.iter().find(|s| s.field_name == "Name").unwrap();
        assert!(name.locked, "Name should be locked (not excluded)");

        let amount = statuses.iter().find(|s| s.field_name == "Amount").unwrap();
        assert!(!amount.locked, "Amount should be unlocked (excluded)");
    }

    #[test]
    fn field_lock_status_all() {
        let pdf = build_pdf_with_fieldmdp("All", &[]);
        let reader = PdfReader::from_bytes(&pdf).unwrap();
        let statuses = field_lock_status(&reader).unwrap();

        for status in &statuses {
            assert!(status.locked, "{} should be locked", status.field_name);
        }
    }

    #[test]
    fn remove_fieldmdp_by_name() {
        let pdf = build_pdf_with_fieldmdp("Include", &["Name"]);
        let mut reader = PdfReader::from_bytes(&pdf).unwrap();

        assert_eq!(detect_fieldmdp(&reader).unwrap().len(), 1);

        let removed = remove_fieldmdp(&mut reader, "FieldLock1").unwrap();
        assert!(removed);

        // Verify removal persists after save/reload
        let saved = reader.save_to_bytes().unwrap();
        let reader2 = PdfReader::from_bytes(&saved).unwrap();
        assert!(detect_fieldmdp(&reader2).unwrap().is_empty());
    }

    #[test]
    fn remove_fieldmdp_nonexistent_returns_false() {
        let pdf = build_pdf_with_fieldmdp("Include", &["Name"]);
        let mut reader = PdfReader::from_bytes(&pdf).unwrap();
        assert!(!remove_fieldmdp(&mut reader, "NoSuchField").unwrap());
    }

    #[test]
    fn remove_fieldmdp_selective_in_multi_sig() {
        let pdf = build_pdf_with_two_fieldmdp();
        let mut reader = PdfReader::from_bytes(&pdf).unwrap();

        assert_eq!(detect_fieldmdp(&reader).unwrap().len(), 2);

        // Remove only Lock1 (which locks Name)
        let removed = remove_fieldmdp(&mut reader, "Lock1").unwrap();
        assert!(removed);

        let saved = reader.save_to_bytes().unwrap();
        let reader2 = PdfReader::from_bytes(&saved).unwrap();

        let remaining = detect_fieldmdp(&reader2).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].field_name.as_deref(), Some("Lock2"));

        // Name should now be unlocked, Email still locked
        let statuses = field_lock_status(&reader2).unwrap();
        let name = statuses.iter().find(|s| s.field_name == "Name").unwrap();
        assert!(!name.locked, "Name should be unlocked after removing Lock1");
        let email = statuses.iter().find(|s| s.field_name == "Email").unwrap();
        assert!(email.locked, "Email should still be locked by Lock2");
    }

    #[test]
    fn remove_all_fieldmdp_signatures() {
        let pdf = build_pdf_with_two_fieldmdp();
        let mut reader = PdfReader::from_bytes(&pdf).unwrap();

        let count = remove_all_fieldmdp(&mut reader).unwrap();
        assert_eq!(count, 2);

        let saved = reader.save_to_bytes().unwrap();
        let reader2 = PdfReader::from_bytes(&saved).unwrap();
        assert!(detect_fieldmdp(&reader2).unwrap().is_empty());

        // All fields should be unlocked
        let statuses = field_lock_status(&reader2).unwrap();
        for status in &statuses {
            assert!(!status.locked, "{} should be unlocked", status.field_name);
        }
    }

    #[test]
    fn remove_fieldmdp_pdf_remains_valid() {
        let pdf = build_pdf_with_fieldmdp("All", &[]);
        let mut reader = PdfReader::from_bytes(&pdf).unwrap();

        remove_fieldmdp(&mut reader, "FieldLock1").unwrap();

        let saved = reader.save_to_bytes().unwrap();
        let reader2 = PdfReader::from_bytes(&saved).unwrap();
        assert_eq!(reader2.page_count(), 1);
    }
}
