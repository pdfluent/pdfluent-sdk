//! UR3 signature detection and removal.
//!
//! PDF Usage Rights (UR3) signatures grant extended features in Adobe Reader
//! (e.g., saving filled forms, commenting). When modifying XFA content, these
//! signatures become invalid and must be removed to prevent "document modified"
//! warnings in Adobe Reader.
//!
//! UR3 signatures are stored in the PDF's `Perms` dictionary under `UR3`.
//! They may also appear under `DocMDP` for certification signatures.

use crate::error::{PdfError, Result};
use crate::pdf_reader::PdfReader;

/// Information about a detected UR3 signature.
#[derive(Debug, Clone)]
pub struct Ur3Info {
    /// The signature field name (if available).
    pub field_name: Option<String>,
    /// The signature sub-filter (e.g., "adbe.pkcs7.detached").
    pub sub_filter: Option<String>,
    /// Whether a byte range is present (indicates actual signing).
    pub has_byte_range: bool,
}

/// Detect UR3 (Usage Rights) signatures in a PDF.
///
/// Returns `Some(info)` if a UR3 signature is found, `None` otherwise.
pub fn detect_ur3(reader: &PdfReader) -> Result<Option<Ur3Info>> {
    let doc = reader.document();

    // Navigate: trailer → Root → Perms → UR3
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

    // Look for UR3 entry
    let ur3_obj = match perms.get_deref(b"UR3", doc) {
        Ok(obj) => obj,
        Err(_) => {
            // Also check for UR (older format)
            match perms.get_deref(b"UR", doc) {
                Ok(obj) => obj,
                Err(_) => return Ok(None),
            }
        }
    };

    // UR3 should be a signature dictionary
    let sig_dict = match ur3_obj.as_dict() {
        Ok(d) => d,
        Err(_) => return Ok(None),
    };

    let sub_filter = sig_dict
        .get(b"SubFilter")
        .ok()
        .and_then(|o| match o {
            lopdf::Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
            _ => None,
        });

    let has_byte_range = sig_dict.get(b"ByteRange").is_ok();

    let field_name = sig_dict
        .get(b"Name")
        .ok()
        .and_then(|o| match o {
            lopdf::Object::String(s, _) => Some(String::from_utf8_lossy(s).to_string()),
            _ => None,
        });

    Ok(Some(Ur3Info {
        field_name,
        sub_filter,
        has_byte_range,
    }))
}

/// Detect DocMDP (certification) signatures in a PDF.
///
/// Returns true if a DocMDP signature is present in the Perms dictionary.
pub fn has_docmdp(reader: &PdfReader) -> Result<bool> {
    let doc = reader.document();

    let catalog = doc
        .trailer
        .get_deref(b"Root", doc)
        .and_then(|o| o.as_dict())
        .map_err(|_| PdfError::XfaPacketNotFound("no Root catalog".to_string()))?;

    let perms = match catalog.get_deref(b"Perms", doc) {
        Ok(obj) => match obj.as_dict() {
            Ok(d) => d,
            Err(_) => return Ok(false),
        },
        Err(_) => return Ok(false),
    };

    Ok(perms.get(b"DocMDP").is_ok())
}

/// Remove UR3 (Usage Rights) signatures from a PDF.
///
/// This removes the `UR3` (and `UR`) entries from the `Perms` dictionary.
/// If the `Perms` dictionary becomes empty, it is removed entirely.
/// Handles both indirect references and inline Perms dictionaries.
///
/// Returns `true` if a signature was removed, `false` if none was found.
pub fn remove_ur3(reader: &mut PdfReader) -> Result<bool> {
    let doc = reader.document();

    // Get the catalog reference
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

    // Find Perms — may be an indirect reference or an inline dictionary
    let (perms, perms_ref) = match catalog.get(b"Perms") {
        Ok(lopdf::Object::Reference(r)) => {
            let r = *r;
            match doc.get_object(r) {
                Ok(lopdf::Object::Dictionary(d)) => (d.clone(), Some(r)),
                _ => return Ok(false),
            }
        }
        Ok(lopdf::Object::Dictionary(d)) => (d.clone(), None),
        _ => return Ok(false),
    };

    let had_ur3 = perms.get(b"UR3").is_ok();
    let had_ur = perms.get(b"UR").is_ok();

    if !had_ur3 && !had_ur {
        return Ok(false);
    }

    // Build new Perms dictionary without UR3/UR entries
    let doc = reader.document_mut();
    let mut new_perms = perms.clone();
    new_perms.remove(b"UR3");
    new_perms.remove(b"UR");

    let catalog_dict = doc
        .get_object_mut(catalog_ref)
        .and_then(|o| o.as_dict_mut())
        .map_err(|_| PdfError::XfaPacketNotFound("catalog not mutable".to_string()))?;

    if new_perms.is_empty() {
        // Remove the Perms entry entirely from catalog
        catalog_dict.remove(b"Perms");
        // Also remove the indirect perms object if it exists
        if let Some(r) = perms_ref {
            doc.objects.remove(&r);
        }
    } else if let Some(r) = perms_ref {
        // Update the indirect Perms dictionary
        doc.objects
            .insert(r, lopdf::Object::Dictionary(new_perms));
    } else {
        // Update the inline Perms dictionary
        catalog_dict.set("Perms", lopdf::Object::Dictionary(new_perms));
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Document, Object};

    fn build_pdf_with_ur3() -> Vec<u8> {
        let mut doc = Document::with_version("1.7");

        // Create a UR3 signature dictionary
        let ur3_sig = dictionary! {
            "Type" => "Sig",
            "Filter" => "Adobe.PPKLite",
            "SubFilter" => Object::Name(b"adbe.pkcs7.detached".to_vec()),
            "ByteRange" => vec![0.into(), 100.into(), 200.into(), 300.into()],
            "Contents" => Object::String(b"fake-signature".to_vec(), lopdf::StringFormat::Hexadecimal),
        };
        let ur3_id = doc.add_object(Object::Dictionary(ur3_sig));

        // Create Perms dictionary
        let perms = dictionary! {
            "UR3" => ur3_id,
        };
        let perms_id = doc.add_object(Object::Dictionary(perms));

        // Create pages
        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        doc.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            }),
        );

        // Catalog with Perms
        let catalog_id = doc.new_object_id();
        doc.objects.insert(
            catalog_id,
            Object::Dictionary(dictionary! {
                "Type" => "Catalog",
                "Pages" => pages_id,
                "Perms" => perms_id,
            }),
        );
        doc.trailer.set("Root", catalog_id);

        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        buf
    }

    fn build_pdf_without_perms() -> Vec<u8> {
        let mut doc = Document::with_version("1.4");
        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        doc.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            }),
        );
        let catalog_id = doc.new_object_id();
        doc.objects.insert(
            catalog_id,
            Object::Dictionary(dictionary! {
                "Type" => "Catalog",
                "Pages" => pages_id,
            }),
        );
        doc.trailer.set("Root", catalog_id);

        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();
        buf
    }

    #[test]
    fn detect_ur3_finds_signature() {
        let pdf = build_pdf_with_ur3();
        let reader = PdfReader::from_bytes(&pdf).unwrap();
        let info = detect_ur3(&reader).unwrap();
        assert!(info.is_some());
        let info = info.unwrap();
        assert_eq!(info.sub_filter.as_deref(), Some("adbe.pkcs7.detached"));
        assert!(info.has_byte_range);
    }

    #[test]
    fn detect_ur3_returns_none_without_perms() {
        let pdf = build_pdf_without_perms();
        let reader = PdfReader::from_bytes(&pdf).unwrap();
        let info = detect_ur3(&reader).unwrap();
        assert!(info.is_none());
    }

    #[test]
    fn has_docmdp_returns_false_without_perms() {
        let pdf = build_pdf_without_perms();
        let reader = PdfReader::from_bytes(&pdf).unwrap();
        assert!(!has_docmdp(&reader).unwrap());
    }

    #[test]
    fn remove_ur3_removes_signature() {
        let pdf = build_pdf_with_ur3();
        let mut reader = PdfReader::from_bytes(&pdf).unwrap();

        // Verify UR3 exists
        assert!(detect_ur3(&reader).unwrap().is_some());

        // Remove it
        let removed = remove_ur3(&mut reader).unwrap();
        assert!(removed);

        // Verify it's gone
        let saved = reader.save_to_bytes().unwrap();
        let reader2 = PdfReader::from_bytes(&saved).unwrap();
        assert!(detect_ur3(&reader2).unwrap().is_none());
    }

    #[test]
    fn remove_ur3_returns_false_when_none() {
        let pdf = build_pdf_without_perms();
        let mut reader = PdfReader::from_bytes(&pdf).unwrap();
        let removed = remove_ur3(&mut reader).unwrap();
        assert!(!removed);
    }

    #[test]
    fn remove_ur3_preserves_docmdp() {
        // Build PDF with both UR3 and DocMDP
        let mut doc = Document::with_version("1.7");

        let ur3_sig = dictionary! {
            "Type" => "Sig",
            "Filter" => "Adobe.PPKLite",
            "SubFilter" => Object::Name(b"adbe.pkcs7.detached".to_vec()),
        };
        let ur3_id = doc.add_object(Object::Dictionary(ur3_sig));

        let docmdp_sig = dictionary! {
            "Type" => "Sig",
            "Filter" => "Adobe.PPKLite",
        };
        let docmdp_id = doc.add_object(Object::Dictionary(docmdp_sig));

        let perms = dictionary! {
            "UR3" => ur3_id,
            "DocMDP" => docmdp_id,
        };
        let perms_id = doc.add_object(Object::Dictionary(perms));

        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        doc.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
            }),
        );

        let catalog_id = doc.new_object_id();
        doc.objects.insert(
            catalog_id,
            Object::Dictionary(dictionary! {
                "Type" => "Catalog",
                "Pages" => pages_id,
                "Perms" => perms_id,
            }),
        );
        doc.trailer.set("Root", catalog_id);

        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();

        let mut reader = PdfReader::from_bytes(&buf).unwrap();

        // Remove UR3 but keep DocMDP
        let removed = remove_ur3(&mut reader).unwrap();
        assert!(removed);

        let saved = reader.save_to_bytes().unwrap();
        let reader2 = PdfReader::from_bytes(&saved).unwrap();

        // UR3 gone
        assert!(detect_ur3(&reader2).unwrap().is_none());
        // DocMDP preserved
        assert!(has_docmdp(&reader2).unwrap());
    }

    #[test]
    fn remove_ur3_handles_inline_perms() {
        // Build PDF with Perms as an inline dictionary (not an indirect reference)
        let mut doc = Document::with_version("1.7");

        let ur3_sig = dictionary! {
            "Type" => "Sig",
            "Filter" => "Adobe.PPKLite",
            "SubFilter" => Object::Name(b"adbe.pkcs7.detached".to_vec()),
        };
        let ur3_id = doc.add_object(Object::Dictionary(ur3_sig));

        let pages_id = doc.new_object_id();
        let page_id = doc.new_object_id();
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        doc.objects.insert(
            page_id,
            Object::Dictionary(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
            }),
        );

        // Catalog with inline Perms dictionary (UR3 is still an indirect ref)
        let catalog_id = doc.new_object_id();
        doc.objects.insert(
            catalog_id,
            Object::Dictionary(dictionary! {
                "Type" => "Catalog",
                "Pages" => pages_id,
                "Perms" => Object::Dictionary(dictionary! {
                    "UR3" => ur3_id,
                }),
            }),
        );
        doc.trailer.set("Root", catalog_id);

        let mut buf = Vec::new();
        doc.save_to(&mut buf).unwrap();

        let mut reader = PdfReader::from_bytes(&buf).unwrap();

        // UR3 should be detected
        assert!(detect_ur3(&reader).unwrap().is_some());

        // Remove should succeed even with inline Perms
        let removed = remove_ur3(&mut reader).unwrap();
        assert!(removed, "should remove UR3 from inline Perms");

        // Verify it's gone after save/reload
        let saved = reader.save_to_bytes().unwrap();
        let reader2 = PdfReader::from_bytes(&saved).unwrap();
        assert!(detect_ur3(&reader2).unwrap().is_none());
    }
}
