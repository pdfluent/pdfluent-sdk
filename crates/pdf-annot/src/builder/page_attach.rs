//! Helper to attach an annotation reference to a page's /Annots array.

#[cfg(feature = "write")]
use lopdf::{Document, Object, ObjectId};

#[cfg(feature = "write")]
use crate::error::AnnotBuildError;

#[cfg(feature = "write")]
enum AnnotsAction {
    SetArray(Vec<Object>),
    AppendIndirect(ObjectId),
}

/// Add an annotation reference to a page's /Annots array.
///
/// Handles both inline arrays and indirect (referenced) arrays.  When the
/// indirect Annots array cannot be resolved (e.g. it lives in a compressed
/// ObjStm that lopdf failed to expand), falls back to replacing the /Annots
/// entry with a new inline array so the annotation is never silently dropped.
#[cfg(feature = "write")]
pub fn add_annotation_to_page(
    doc: &mut Document,
    page_num: u32,
    annot_id: ObjectId,
) -> Result<(), AnnotBuildError> {
    let pages = doc.get_pages();
    let page_count = pages.len();
    let page_id = *pages
        .get(&page_num)
        .ok_or(AnnotBuildError::PageOutOfRange(page_num, page_count))?;

    // Read the current /Annots entry using get_dictionary (follows indirect
    // refs, handles compressed-stream pages).
    let annots_action = {
        match doc.get_dictionary(page_id) {
            Ok(page_dict) => match page_dict.get(b"Annots").ok() {
                Some(Object::Array(arr)) => {
                    let mut new_arr = arr.clone();
                    new_arr.push(Object::Reference(annot_id));
                    AnnotsAction::SetArray(new_arr)
                }
                Some(Object::Reference(r)) => AnnotsAction::AppendIndirect(*r),
                _ => AnnotsAction::SetArray(vec![Object::Reference(annot_id)]),
            },
            // Page dict not accessible: still attempt to set an inline Annots.
            Err(_) => AnnotsAction::SetArray(vec![Object::Reference(annot_id)]),
        }
    };

    match annots_action {
        AnnotsAction::SetArray(arr) => {
            // Propagate failure: if the page dict cannot be mutated (e.g. the
            // page was extracted from a fully-compressed ObjStm and lopdf has
            // trouble writing it back), return an explicit error instead of
            // silently dropping the annotation.  Fixes #470.
            if let Ok(page_dict) = doc.get_dictionary_mut(page_id) {
                page_dict.set("Annots", Object::Array(arr));
            } else {
                return Err(AnnotBuildError::PageMutationFailed);
            }
        }
        AnnotsAction::AppendIndirect(annots_ref) => {
            // Attempt to mutate the indirect array in place.
            let appended = {
                if let Ok(Object::Array(ref mut arr)) = doc.get_object_mut(annots_ref) {
                    arr.push(Object::Reference(annot_id));
                    true
                } else {
                    false
                }
            };

            if !appended {
                // Fallback: indirect array not accessible (e.g. lives in a
                // compressed ObjStm). Try a read-only access first — lopdf can
                // often decompress ObjStm objects for reading even when it
                // cannot hand out a mutable reference.  This preserves existing
                // annotations instead of silently dropping them. Fixes #466 bug 7.
                let existing: Vec<Object> = match doc.get_object(annots_ref) {
                    Ok(Object::Array(ref arr)) => arr.clone(),
                    _ => Vec::new(),
                };
                let mut new_annots = existing;
                new_annots.push(Object::Reference(annot_id));
                if let Ok(page_dict) = doc.get_dictionary_mut(page_id) {
                    page_dict.set("Annots", Object::Array(new_annots));
                } else {
                    return Err(AnnotBuildError::PageMutationFailed);
                }
            }
        }
    }

    Ok(())
}
