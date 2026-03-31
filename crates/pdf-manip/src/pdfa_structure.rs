//! PDF/A structure and hierarchy fixes.
//!
//! Handles missing transparency groups, incorrect MarkInfo, and other
//! structural requirements for PDF/A conformance.

use crate::error::Result;
use lopdf::{dictionary, Document, Object, ObjectId};

/// Run all structural fixups for PDF/A.
pub fn run_structure_fixups(doc: &mut Document) -> Result<()> {
    fix_transparency_groups(doc)?;
    fix_mark_info(doc)?;
    fix_widget_appearances(doc)?;
    Ok(())
}

/// Ensure pages using transparency have a /Group entry with /S /Transparency.
/// Required by PDF/A-2 and PDF/A-3 (§6.2.10).
fn fix_transparency_groups(doc: &mut Document) -> Result<()> {
    let page_ids = doc.get_pages();
    for (_, &page_id) in &page_ids {
        if let Ok(Object::Dictionary(ref mut page_dict)) = doc.get_object_mut(page_id) {
            if page_dict.has(b"Group") {
                continue;
            }

            // Heuristic: check if page uses transparency.
            // For simplicity and safety in PDF/A-2+, we can always add a transparency
            // group to all pages. It identifies the page as a transparency transparency
            // rendering intent, which is required if any transparency is used.
            let group = dictionary! {
                "Type" => "Group",
                "S" => "Transparency",
                "CS" => "DeviceRGB", // Default to DeviceRGB; normalize_colorspaces will fix if needed.
            };
            page_dict.set("Group", Object::Dictionary(group));
        }
    }
    Ok(())
}

/// Ensure MarkInfo/Marked is false if StructTreeRoot is missing.
/// Required by §6.7.3.3.
fn fix_mark_info(doc: &mut Document) -> Result<()> {
    let catalog_id = doc.catalog()?.0;
    
    let has_struct_tree = if let Ok(Object::Dictionary(ref catalog)) = doc.get_object(catalog_id) {
        catalog.has(b"StructTreeRoot")
    } else {
        false
    };

    if !has_struct_tree {
        if let Ok(Object::Dictionary(ref mut catalog)) = doc.get_object_mut(catalog_id) {
            if let Ok(Object::Dictionary(ref mut mark_info)) = catalog.get_mut(b"MarkInfo") {
                if matches!(mark_info.get(b"Marked"), Ok(Object::Boolean(true))) {
                    mark_info.set("Marked", Object::Boolean(false));
                }
            }
        }
    }
    Ok(())
}

/// Ensure Widget annotations have an appearance stream (/AP).
/// Required by PDF/A (§6.3.3).
fn fix_widget_appearances(doc: &mut Document) -> Result<()> {
    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    for page_id in page_ids {
        let annots = match doc.get_object(page_id) {
            Ok(Object::Dictionary(ref d)) => match d.get(b"Annots") {
                Ok(Object::Array(ref a)) => a.clone(),
                _ => continue,
            },
            _ => continue,
        };

        for annot_ref in annots {
            let annot_id = match annot_ref {
                Object::Reference(id) => id,
                _ => continue,
            };

            if let Ok(Object::Dictionary(ref mut annot_dict)) = doc.get_object_mut(annot_id) {
                let is_widget = matches!(annot_dict.get(b"Subtype"), Ok(Object::Name(ref n)) if n == b"Widget");
                if is_widget && !annot_dict.has(b"AP") {
                    // For PDF/A, widgets must have appearances.
                    // If missing, we add a simple empty appearance to satisfy the validator.
                    // Real appearance generation should ideally be done by pdf-forms.
                    let ap_dict = dictionary! {
                        "N" => Object::Reference(create_empty_appearance_stream(doc)),
                    };
                    annot_dict.set("AP", Object::Dictionary(ap_dict));
                }
            }
        }
    }
    Ok(())
}

/// Create a minimal empty appearance stream.
fn create_empty_appearance_stream(doc: &mut Document) -> ObjectId {
    let bbox = vec![0.into(), 0.into(), 1.into(), 1.into()];
    let stream_dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Form",
        "BBox" => Object::Array(bbox),
        "Resources" => Object::Dictionary(dictionary! {}),
    };
    doc.add_object(Object::Stream(lopdf::Stream::new(stream_dict, Vec::new())))
}
