//! PDF/A structure and hierarchy fixes.
//!
//! Handles missing transparency groups, incorrect MarkInfo, and other
//! structural requirements for PDF/A conformance.

use crate::error::Result;
use lopdf::{dictionary, Document, Object, ObjectId, Stream};

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
    let has_struct_tree = doc
        .catalog()
        .map(|c| c.has(b"StructTreeRoot"))
        .unwrap_or(false);

    if !has_struct_tree {
        if let Ok(catalog) = doc.catalog_mut() {
            if let Ok(mark_info) = catalog.get_mut(b"MarkInfo").and_then(|o| o.as_dict_mut()) {
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

            let needs_ap = if let Ok(Object::Dictionary(ref annot_dict)) = doc.get_object(annot_id)
            {
                let is_widget =
                    matches!(annot_dict.get(b"Subtype"), Ok(Object::Name(ref n)) if n == b"Widget");
                is_widget && !annot_dict.has(b"AP")
            } else {
                false
            };

            if needs_ap {
                let ap_id = create_empty_appearance_stream(doc);
                if let Ok(Object::Dictionary(ref mut annot_dict)) = doc.get_object_mut(annot_id) {
                    let ap_dict = dictionary! {
                        "N" => Object::Reference(ap_id),
                    };
                    annot_dict.set("AP", Object::Dictionary(ap_dict));
                }
            }
        }
    }
    Ok(())
}

/// Create a simple empty appearance stream for PDF/A conformance.
fn create_empty_appearance_stream(doc: &mut Document) -> ObjectId {
    let dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Form",
        "BBox" => vec![0.into(), 0.into(), 1.into(), 1.into()],
        "Resources" => dictionary! {},
    };
    doc.add_object(Object::Stream(Stream::new(dict, Vec::new())))
}
