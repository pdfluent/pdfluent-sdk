//! DocMDP signature detection and removal.
//!
//! DocMDP (Document Modification Detection and Prevention) certification
//! signatures lock the entire PDF document. They are stored in the `Perms`
//! dictionary under the `DocMDP` key and reference a signature dictionary
//! containing a PKCS#7 container and byte range.
//!
//! Removing the DocMDP signature makes the document editable again for
//! template creation, while preserving the underlying content.

use crate::error::{PdfError, Result};
use crate::pdf_reader::PdfReader;
use lopdf::{Dictionary, Object, ObjectId};

/// Information about a detected DocMDP certification signature.
#[derive(Debug, Clone)]
pub struct DocMdpInfo {
    /// The signature field name (if available).
    pub field_name: Option<String>,
    /// The signature sub-filter (e.g., "adbe.pkcs7.detached", "ETSI.CAdES.detached").
    pub sub_filter: Option<String>,
    /// Whether a byte range is present (indicates actual signing).
    pub has_byte_range: bool,
    /// Whether PKCS#7 contents are present.
    pub has_pkcs7: bool,
    /// The modification permission level (P value): 1=no changes, 2=fill+sign, 3=annotate.
    pub permission: Option<i64>,
    /// The object ID of the signature dictionary.
    pub sig_object_id: Option<ObjectId>,
    /// The object ID of the signature field widget (if found in AcroForm).
    pub field_object_id: Option<ObjectId>,
}

/// Detect DocMDP (certification) signatures in a PDF.
///
/// Returns detailed information about the DocMDP signature if found.
pub fn detect_docmdp(reader: &PdfReader) -> Result<Option<DocMdpInfo>> {
    let doc = reader.document();

    let catalog = doc
        .trailer
        .get_deref(b"Root", doc)
        .and_then(|o| o.as_dict())
        .map_err(|_| PdfError::XfaPacketNotFound("no Root catalog".to_string()))?;

    let perms = match catalog.get_deref(b"Perms", doc) {
        Ok(obj) => match obj.as_dict() {
            Ok(d) => d,
            Err(_) => return Ok(None),
        },
        Err(_) => return Ok(None),
    };

    // Get DocMDP entry — may be a reference or inline dict
    let (sig_dict, sig_obj_id) = match perms.get(b"DocMDP") {
        Ok(Object::Reference(r)) => {
            let r = *r;
            match doc.get_object(r) {
                Ok(Object::Dictionary(d)) => (d, Some(r)),
                _ => return Ok(None),
            }
        }
        Ok(Object::Dictionary(d)) => (d, None),
        _ => return Ok(None),
    };

    let sub_filter = extract_name(sig_dict, b"SubFilter");
    let has_byte_range = sig_dict.get(b"ByteRange").is_ok();
    let has_pkcs7 = sig_dict.get(b"Contents").is_ok();

    let field_name = sig_dict.get(b"Name").ok().and_then(|o| match o {
        Object::String(s, _) => Some(String::from_utf8_lossy(s).to_string()),
        _ => None,
    });

    // Extract permission level from TransformParams → P
    let permission = extract_permission(sig_dict, doc);

    // Find the visual signature field in AcroForm
    let field_object_id = find_signature_field(catalog, doc, sig_obj_id);

    Ok(Some(DocMdpInfo {
        field_name,
        sub_filter,
        has_byte_range,
        has_pkcs7,
        permission,
        sig_object_id: sig_obj_id,
        field_object_id,
    }))
}

/// Remove DocMDP certification signature from a PDF.
///
/// This safely removes:
/// 1. The DocMDP entry from the Perms dictionary
/// 2. The PKCS#7 signature dictionary object
/// 3. The visual signature field from AcroForm (if present)
/// 4. The signature appearance annotation from the page
///
/// Non-DocMDP signatures (e.g., UR3) are preserved.
///
/// Returns `true` if a signature was removed, `false` if none was found.
pub fn remove_docmdp(reader: &mut PdfReader) -> Result<bool> {
    // First, detect and gather all the info we need
    let info = match detect_docmdp(reader)? {
        Some(info) => info,
        None => return Ok(false),
    };

    let doc = reader.document();

    // Get catalog ref
    let catalog_ref = doc
        .trailer
        .get(b"Root")
        .and_then(|o| o.as_reference())
        .map_err(|_| PdfError::XfaPacketNotFound("no Root in trailer".to_string()))?;

    // Get Perms reference (if indirect)
    let catalog = doc
        .get_object(catalog_ref)
        .and_then(|o| o.as_dict())
        .map_err(|_| PdfError::XfaPacketNotFound("Root not a dictionary".to_string()))?
        .clone();

    let perms_ref = match catalog.get(b"Perms") {
        Ok(Object::Reference(r)) => Some(*r),
        _ => None,
    };

    let perms_dict = match catalog.get(b"Perms") {
        Ok(Object::Reference(r)) => {
            let r = *r;
            match doc.get_object(r) {
                Ok(Object::Dictionary(d)) => Some(d.clone()),
                _ => None,
            }
        }
        Ok(Object::Dictionary(d)) => Some(d.clone()),
        _ => None,
    };

    // Collect signature field annotations to remove from pages
    let page_annot_removals = if let Some(field_id) = info.field_object_id {
        collect_page_annotation_refs(doc, field_id)
    } else {
        vec![]
    };

    // Collect AcroForm info for field removal (indirect or inline)
    let acroform_ref = catalog.get(b"AcroForm").ok().and_then(|o| {
        if let Object::Reference(r) = o {
            Some(*r)
        } else {
            None
        }
    });
    let acroform_is_inline = matches!(catalog.get(b"AcroForm"), Ok(Object::Dictionary(_)));

    // --- Begin mutations ---
    let doc = reader.document_mut();

    // 1. Remove DocMDP from Perms dictionary
    if let Some(mut perms) = perms_dict {
        perms.remove(b"DocMDP");

        if perms.is_empty() {
            // Remove Perms entirely from catalog
            if let Ok(Object::Dictionary(ref mut cat)) = doc.get_object_mut(catalog_ref) {
                cat.remove(b"Perms");
            }
            if let Some(r) = perms_ref {
                doc.objects.remove(&r);
            }
        } else if let Some(r) = perms_ref {
            doc.objects.insert(r, Object::Dictionary(perms));
        } else if let Ok(Object::Dictionary(ref mut cat)) = doc.get_object_mut(catalog_ref) {
            cat.set("Perms", Object::Dictionary(perms));
        }
    }

    // 2. Remove the PKCS#7 signature dictionary object
    if let Some(sig_id) = info.sig_object_id {
        doc.objects.remove(&sig_id);
    }

    // 3. Remove the signature field from AcroForm.Fields
    if let Some(field_id) = info.field_object_id {
        if let Some(af_ref) = acroform_ref {
            // AcroForm is an indirect reference
            if let Ok(Object::Dictionary(ref mut af)) = doc.get_object_mut(af_ref) {
                if let Ok(Object::Array(ref mut fields)) = af.get_mut(b"Fields") {
                    fields.retain(|f| {
                        if let Object::Reference(r) = f {
                            *r != field_id
                        } else {
                            true
                        }
                    });
                }
            }
        } else if acroform_is_inline {
            // AcroForm is inline in the catalog dictionary
            if let Ok(Object::Dictionary(ref mut cat)) = doc.get_object_mut(catalog_ref) {
                if let Ok(Object::Dictionary(ref mut af)) = cat.get_mut(b"AcroForm") {
                    if let Ok(Object::Array(ref mut fields)) = af.get_mut(b"Fields") {
                        fields.retain(|f| {
                            if let Object::Reference(r) = f {
                                *r != field_id
                            } else {
                                true
                            }
                        });
                    }
                }
            }
        }
        // Remove the field object itself
        doc.objects.remove(&field_id);
    }

    // 4. Remove signature annotations from pages
    for (page_id, annot_id) in page_annot_removals {
        // First, check if Annots is an indirect reference
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

/// Extract the Name value from a dictionary.
fn extract_name(dict: &Dictionary, key: &[u8]) -> Option<String> {
    dict.get(key).ok().and_then(|o| match o {
        Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
        _ => None,
    })
}

/// Extract the DocMDP permission level from the signature's TransformParams.
fn extract_permission(sig_dict: &Dictionary, doc: &lopdf::Document) -> Option<i64> {
    // The permission is in Reference → TransformParams → P
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

        // Check TransformMethod is DocMDP
        if let Some(method) = extract_name(ref_dict, b"TransformMethod") {
            if method != "DocMDP" {
                continue;
            }
        }

        let params = match ref_dict.get(b"TransformParams") {
            Ok(Object::Dictionary(d)) => d,
            Ok(Object::Reference(r)) => match doc.get_object(*r) {
                Ok(Object::Dictionary(d)) => d,
                _ => continue,
            },
            _ => continue,
        };

        if let Ok(Object::Integer(p)) = params.get(b"P") {
            return Some(*p);
        }
    }

    None
}

/// Find the signature field in AcroForm that references the given signature object.
fn find_signature_field(
    catalog: &Dictionary,
    doc: &lopdf::Document,
    sig_obj_id: Option<ObjectId>,
) -> Option<ObjectId> {
    let sig_id = sig_obj_id?;

    let acroform = match catalog.get_deref(b"AcroForm", doc) {
        Ok(Object::Dictionary(d)) => d,
        _ => return None,
    };

    let fields = match acroform.get(b"Fields") {
        Ok(Object::Array(a)) => a,
        _ => return None,
    };

    for field_ref in fields {
        let field_id = match field_ref {
            Object::Reference(r) => *r,
            _ => continue,
        };

        let field_dict = match doc.get_object(field_id) {
            Ok(Object::Dictionary(d)) => d,
            _ => continue,
        };

        // Check if this field's V (value) references the signature dict
        if let Ok(Object::Reference(v_ref)) = field_dict.get(b"V") {
            if *v_ref == sig_id {
                return Some(field_id);
            }
        }
    }

    None
}

/// Collect (page_id, annotation_id) pairs where the annotation references
/// the given field object.
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

        // Only look at Page objects
        if let Ok(Object::Name(t)) = page_dict.get(b"Type") {
            if t != b"Page" {
                continue;
            }
        } else {
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

    /// Build a PDF with a DocMDP certification signature.
    fn build_pdf_with_docmdp(permission: i64) -> Vec<u8> {
        let mut doc = Document::with_version("1.7");

        // PKCS#7 signature dictionary
        let sig_dict = dictionary! {
            "Type" => Object::Name(b"Sig".to_vec()),
            "Filter" => Object::Name(b"Adobe.PPKLite".to_vec()),
            "SubFilter" => Object::Name(b"adbe.pkcs7.detached".to_vec()),
            "ByteRange" => Object::Array(vec![
                0.into(), 100.into(), 200.into(), 300.into(),
            ]),
            "Contents" => Object::String(b"fake-pkcs7-container".to_vec(), lopdf::StringFormat::Hexadecimal),
            "Name" => Object::String(b"Test Signer".to_vec(), lopdf::StringFormat::Literal),
            "Reference" => Object::Array(vec![
                Object::Dictionary(dictionary! {
                    "TransformMethod" => Object::Name(b"DocMDP".to_vec()),
                    "TransformParams" => Object::Dictionary(dictionary! {
                        "Type" => Object::Name(b"TransformParams".to_vec()),
                        "P" => Object::Integer(permission),
                        "V" => Object::Name(b"1.2".to_vec()),
                    }),
                }),
            ]),
        };
        let sig_id = doc.add_object(Object::Dictionary(sig_dict));

        // Perms dictionary
        let perms = dictionary! {
            "DocMDP" => Object::Reference(sig_id),
        };
        let perms_id = doc.add_object(Object::Dictionary(perms));

        // Signature field widget
        let ap_stream = Stream::new(Dictionary::new(), b"q Q".to_vec());
        let ap_id = doc.add_object(Object::Stream(ap_stream));
        let sig_field = dictionary! {
            "Type" => Object::Name(b"Annot".to_vec()),
            "Subtype" => Object::Name(b"Widget".to_vec()),
            "FT" => Object::Name(b"Sig".to_vec()),
            "T" => Object::String(b"Signature1".to_vec(), lopdf::StringFormat::Literal),
            "V" => Object::Reference(sig_id),
            "Rect" => Object::Array(vec![0.into(), 0.into(), 200.into(), 50.into()]),
            "AP" => Object::Dictionary(dictionary! {
                "N" => Object::Reference(ap_id),
            }),
        };
        let field_id = doc.add_object(Object::Dictionary(sig_field));

        // AcroForm with Fields
        let acroform = dictionary! {
            "Fields" => Object::Array(vec![Object::Reference(field_id)]),
            "SigFlags" => Object::Integer(3),
        };
        let acroform_id = doc.add_object(Object::Dictionary(acroform));

        // Pages
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

        // Set Parent on page
        if let Ok(Object::Dictionary(ref mut p)) = doc.get_object_mut(page_id) {
            p.set("Parent", Object::Reference(pages_id));
        }

        // Catalog
        let catalog = dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "Pages" => Object::Reference(pages_id),
            "Perms" => Object::Reference(perms_id),
            "AcroForm" => Object::Reference(acroform_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        buf
    }

    /// Build a PDF with both DocMDP and UR3 signatures.
    fn build_pdf_with_docmdp_and_ur3() -> Vec<u8> {
        let mut doc = Document::with_version("1.7");

        let docmdp_sig = dictionary! {
            "Type" => Object::Name(b"Sig".to_vec()),
            "Filter" => Object::Name(b"Adobe.PPKLite".to_vec()),
            "SubFilter" => Object::Name(b"adbe.pkcs7.detached".to_vec()),
            "Contents" => Object::String(b"docmdp-pkcs7".to_vec(), lopdf::StringFormat::Hexadecimal),
        };
        let docmdp_id = doc.add_object(Object::Dictionary(docmdp_sig));

        let ur3_sig = dictionary! {
            "Type" => Object::Name(b"Sig".to_vec()),
            "Filter" => Object::Name(b"Adobe.PPKLite".to_vec()),
            "SubFilter" => Object::Name(b"adbe.pkcs7.detached".to_vec()),
        };
        let ur3_id = doc.add_object(Object::Dictionary(ur3_sig));

        let perms = dictionary! {
            "DocMDP" => Object::Reference(docmdp_id),
            "UR3" => Object::Reference(ur3_id),
        };
        let perms_id = doc.add_object(Object::Dictionary(perms));

        let pages = dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => Object::Array(vec![]),
            "Count" => Object::Integer(0),
        };
        let pages_id = doc.add_object(Object::Dictionary(pages));

        let catalog = dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "Pages" => Object::Reference(pages_id),
            "Perms" => Object::Reference(perms_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        buf
    }

    fn build_pdf_without_perms() -> Vec<u8> {
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
        buf
    }

    #[test]
    fn detect_docmdp_finds_signature() {
        let pdf = build_pdf_with_docmdp(2);
        let reader = PdfReader::from_bytes(&pdf).unwrap();
        let info = detect_docmdp(&reader).unwrap();
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.sub_filter.as_deref(), Some("adbe.pkcs7.detached"));
        assert!(info.has_byte_range);
        assert!(info.has_pkcs7);
        assert_eq!(info.permission, Some(2));
        assert_eq!(info.field_name.as_deref(), Some("Test Signer"));
        assert!(info.sig_object_id.is_some());
        assert!(info.field_object_id.is_some());
    }

    #[test]
    fn detect_docmdp_permission_levels() {
        for p in [1, 2, 3] {
            let pdf = build_pdf_with_docmdp(p);
            let reader = PdfReader::from_bytes(&pdf).unwrap();
            let info = detect_docmdp(&reader).unwrap().unwrap();
            assert_eq!(info.permission, Some(p));
        }
    }

    #[test]
    fn detect_docmdp_returns_none_without_perms() {
        let pdf = build_pdf_without_perms();
        let reader = PdfReader::from_bytes(&pdf).unwrap();
        assert!(detect_docmdp(&reader).unwrap().is_none());
    }

    #[test]
    fn remove_docmdp_removes_signature() {
        let pdf = build_pdf_with_docmdp(2);
        let mut reader = PdfReader::from_bytes(&pdf).unwrap();

        // Verify it exists
        assert!(detect_docmdp(&reader).unwrap().is_some());

        // Remove it
        let removed = remove_docmdp(&mut reader).unwrap();
        assert!(removed);

        // Save and verify it's gone
        let saved = reader.save_to_bytes().unwrap();
        let reader2 = PdfReader::from_bytes(&saved).unwrap();
        assert!(detect_docmdp(&reader2).unwrap().is_none());
    }

    #[test]
    fn remove_docmdp_returns_false_when_none() {
        let pdf = build_pdf_without_perms();
        let mut reader = PdfReader::from_bytes(&pdf).unwrap();
        assert!(!remove_docmdp(&mut reader).unwrap());
    }

    #[test]
    fn remove_docmdp_preserves_ur3() {
        let pdf = build_pdf_with_docmdp_and_ur3();
        let mut reader = PdfReader::from_bytes(&pdf).unwrap();

        // Both should exist
        assert!(detect_docmdp(&reader).unwrap().is_some());
        assert!(crate::ur3::has_docmdp(&reader).unwrap());

        // Remove DocMDP
        let removed = remove_docmdp(&mut reader).unwrap();
        assert!(removed);

        // Save and verify DocMDP is gone but UR3 remains
        let saved = reader.save_to_bytes().unwrap();
        let reader2 = PdfReader::from_bytes(&saved).unwrap();
        assert!(detect_docmdp(&reader2).unwrap().is_none());
        assert!(crate::ur3::detect_ur3(&reader2).unwrap().is_some());
    }

    #[test]
    fn remove_docmdp_removes_signature_field() {
        let pdf = build_pdf_with_docmdp(1);
        let mut reader = PdfReader::from_bytes(&pdf).unwrap();

        let info = detect_docmdp(&reader).unwrap().unwrap();
        let field_id = info.field_object_id.unwrap();

        // Field exists before removal
        assert!(reader.document().get_object(field_id).is_ok());

        remove_docmdp(&mut reader).unwrap();

        // Field object should be removed
        assert!(reader.document().get_object(field_id).is_err());
    }

    #[test]
    fn remove_docmdp_removes_pkcs7_object() {
        let pdf = build_pdf_with_docmdp(2);
        let mut reader = PdfReader::from_bytes(&pdf).unwrap();

        let info = detect_docmdp(&reader).unwrap().unwrap();
        let sig_id = info.sig_object_id.unwrap();

        // Signature object exists before removal
        assert!(reader.document().get_object(sig_id).is_ok());

        remove_docmdp(&mut reader).unwrap();

        // Signature object should be removed
        assert!(reader.document().get_object(sig_id).is_err());
    }

    #[test]
    fn remove_docmdp_removes_page_annotation() {
        let pdf = build_pdf_with_docmdp(2);
        let mut reader = PdfReader::from_bytes(&pdf).unwrap();

        // Count annotations before
        let annot_count_before: usize = reader
            .document()
            .objects
            .values()
            .filter(|o| {
                if let Object::Dictionary(d) = o {
                    if let Ok(Object::Name(t)) = d.get(b"Type") {
                        if t == b"Page" {
                            return d
                                .get(b"Annots")
                                .ok()
                                .and_then(|a| a.as_array().ok())
                                .map_or(0, |a| a.len())
                                > 0;
                        }
                    }
                }
                false
            })
            .count();
        assert!(annot_count_before > 0);

        remove_docmdp(&mut reader).unwrap();

        // After removal, page Annots should be empty
        let has_annots: bool = reader.document().objects.values().any(|o| {
            if let Object::Dictionary(d) = o {
                if let Ok(Object::Name(t)) = d.get(b"Type") {
                    if t == b"Page" {
                        if let Ok(Object::Array(annots)) = d.get(b"Annots") {
                            return !annots.is_empty();
                        }
                    }
                }
            }
            false
        });
        assert!(
            !has_annots,
            "Page annotations should be empty after removal"
        );
    }

    #[test]
    fn remove_docmdp_pdf_remains_valid() {
        let pdf = build_pdf_with_docmdp(3);
        let mut reader = PdfReader::from_bytes(&pdf).unwrap();

        remove_docmdp(&mut reader).unwrap();

        // Save and reload to verify structural integrity
        let saved = reader.save_to_bytes().unwrap();
        let reader2 = PdfReader::from_bytes(&saved).unwrap();
        assert_eq!(reader2.page_count(), 1);
    }
}
