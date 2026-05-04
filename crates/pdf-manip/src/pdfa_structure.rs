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
    ensure_core_types(doc)?;
    fix_bdc_lang_tags(doc)?;
    Ok(())
}

/// Ensure Catalog, Pages, and Page objects have correct /Type entries (§6.1.2).
fn ensure_core_types(doc: &mut Document) -> Result<()> {
    let catalog_id = match doc.trailer.get(b"Root").ok() {
        Some(Object::Reference(id)) => Some(*id),
        _ => None,
    };

    if let Some(id) = catalog_id {
        if let Ok(Object::Dictionary(ref mut cat)) = doc.get_object_mut(id) {
            cat.set("Type", Object::Name(b"Catalog".to_vec()));
        }
    }

    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let mut pages_dict_ids = std::collections::HashSet::new();

    for page_id in page_ids {
        if let Ok(Object::Dictionary(ref mut page_dict)) = doc.get_object_mut(page_id) {
            page_dict.set("Type", Object::Name(b"Page".to_vec()));
            if let Ok(Object::Reference(parent_id)) = page_dict.get(b"Parent") {
                pages_dict_ids.insert(*parent_id);
            }
        }
    }

    for pages_id in pages_dict_ids {
        fix_pages_tree_node(doc, pages_id);
    }

    Ok(())
}

fn fix_pages_tree_node(doc: &mut Document, id: ObjectId) {
    let parent_id = if let Ok(Object::Dictionary(ref mut pages_dict)) = doc.get_object_mut(id) {
        pages_dict.set("Type", Object::Name(b"Pages".to_vec()));
        match pages_dict.get(b"Parent") {
            Ok(Object::Reference(pid)) => Some(*pid),
            _ => None,
        }
    } else {
        None
    };
    if let Some(pid) = parent_id {
        fix_pages_tree_node(doc, pid);
    }
}

/// Normalize a BCP-47 language tag (lowercase language, titlecase region).
fn normalize_lang_tag(tag: &str) -> String {
    let parts: Vec<&str> = tag.split('-').collect();
    let mut out = String::new();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            out.push('-');
        }
        if i == 0 {
            out.push_str(&part.to_lowercase());
        } else if part.len() == 2 {
            out.push_str(&part.to_uppercase());
        } else {
            let mut chars = part.chars();
            if let Some(first) = chars.next() {
                out.extend(first.to_uppercase());
                out.push_str(&chars.as_str().to_lowercase());
            }
        }
    }
    out
}

/// Fix invalid language tags in BDC (Marked Content) and structure elements (§6.7.4).
fn fix_bdc_lang_tags(doc: &mut Document) -> Result<()> {
    let catalog_id = match doc.trailer.get(b"Root").ok() {
        Some(Object::Reference(id)) => Some(*id),
        _ => None,
    };

    if let Some(id) = catalog_id {
        if let Ok(Object::Dictionary(ref mut catalog)) = doc.get_object_mut(id) {
            if let Ok(Object::String(bytes, _)) = catalog.get(b"Lang") {
                let lang = String::from_utf8_lossy(bytes).to_string();
                let normalized = normalize_lang_tag(&lang);
                if normalized != lang {
                    catalog.set(
                        "Lang",
                        Object::String(normalized.into_bytes(), lopdf::StringFormat::Literal),
                    );
                }
            }
        }
    }

    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        if let Ok(Object::Dictionary(ref mut dict)) = doc.get_object_mut(id) {
            if let Ok(Object::String(bytes, _)) = dict.get(b"Lang") {
                let lang = String::from_utf8_lossy(bytes).to_string();
                let normalized = normalize_lang_tag(&lang);
                if normalized != lang {
                    dict.set(
                        "Lang",
                        Object::String(normalized.into_bytes(), lopdf::StringFormat::Literal),
                    );
                }
            }
        }
    }

    Ok(())
}

/// Ensure pages using transparency have a /Group entry with /S /Transparency.
/// Required by PDF/A-2 and PDF/A-3 (§6.2.10).
fn fix_transparency_groups(doc: &mut Document) -> Result<()> {
    let page_ids = doc.get_pages();
    for &page_id in page_ids.values() {
        if let Ok(Object::Dictionary(ref mut page_dict)) = doc.get_object_mut(page_id) {
            if page_dict.has(b"Group") {
                continue;
            }

            let group = dictionary! {
                "Type" => "Group",
                "S" => "Transparency",
                "CS" => "DeviceRGB",
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
