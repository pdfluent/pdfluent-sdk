//! PDF/A cleanup: remove JavaScript, EmbeddedFiles, and transparency.
//!
//! Strips PDF/A-incompatible elements from documents.

use crate::error::Result;
use lopdf::{Document, Object, ObjectId};

/// Report from PDF/A cleanup pass.
#[derive(Debug, Clone)]
pub struct PdfACleanupReport {
    /// Number of JavaScript actions removed.
    pub js_actions_removed: usize,
    /// Number of embedded file entries removed.
    pub embedded_files_removed: usize,
    /// Number of transparency groups detected.
    pub transparency_groups_found: usize,
    /// Whether encryption was present and removed.
    pub encryption_removed: bool,
    /// Number of additional-actions (AA) entries removed.
    pub aa_entries_removed: usize,
}

/// Remove all PDF/A-incompatible elements from the document.
pub fn cleanup_for_pdfa(doc: &mut Document, is_pdfa1: bool) -> Result<PdfACleanupReport> {
    let mut report = PdfACleanupReport {
        js_actions_removed: 0,
        embedded_files_removed: 0,
        transparency_groups_found: 0,
        encryption_removed: false,
        aa_entries_removed: 0,
    };

    report.js_actions_removed = remove_javascript(doc);
    report.aa_entries_removed = remove_additional_actions(doc);
    report.transparency_groups_found = count_transparency_groups(doc);

    if is_pdfa1 {
        report.embedded_files_removed = remove_embedded_files(doc);
    }

    report.encryption_removed = remove_encryption(doc);

    Ok(report)
}

/// Remove all JavaScript from the document.
pub fn remove_javascript(doc: &mut Document) -> usize {
    let mut count = 0;

    // Remove /JavaScript name tree from catalog /Names.
    if let Some(catalog_id) = get_catalog_id(doc) {
        // Handle Names as reference.
        let names_id = {
            if let Some(Object::Dictionary(catalog)) = doc.objects.get(&catalog_id) {
                match catalog.get(b"Names").ok() {
                    Some(Object::Reference(id)) => Some(*id),
                    _ => None,
                }
            } else {
                None
            }
        };

        if let Some(nid) = names_id {
            if let Some(Object::Dictionary(ref mut names)) = doc.objects.get_mut(&nid) {
                if names.has(b"JavaScript") {
                    names.remove(b"JavaScript");
                    count += 1;
                }
            }
        }

        // Handle inline Names dict in catalog.
        let has_inline_js = {
            if let Some(Object::Dictionary(catalog)) = doc.objects.get(&catalog_id) {
                if let Ok(Object::Dictionary(names)) = catalog.get(b"Names") {
                    names.has(b"JavaScript")
                } else {
                    false
                }
            } else {
                false
            }
        };

        if has_inline_js {
            if let Some(Object::Dictionary(ref mut catalog)) = doc.objects.get_mut(&catalog_id) {
                if let Ok(Object::Dictionary(ref mut names)) = catalog.get_mut(b"Names") {
                    names.remove(b"JavaScript");
                    count += 1;
                }
            }
        }
    }

    // Remove JavaScript actions from all objects.
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let has_js = {
            if let Some(Object::Dictionary(dict)) = doc.objects.get(&id) {
                is_javascript_action(dict)
            } else {
                false
            }
        };

        if has_js {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                dict.remove(b"JS");
                dict.remove(b"S");
                count += 1;
            }
        }
    }

    count
}

/// Remove Additional Actions (AA) entries from all objects.
pub fn remove_additional_actions(doc: &mut Document) -> usize {
    let mut count = 0;

    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let has_aa = {
            if let Some(Object::Dictionary(dict)) = doc.objects.get(&id) {
                dict.has(b"AA")
            } else {
                false
            }
        };

        if has_aa {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                dict.remove(b"AA");
                count += 1;
            }
        }
    }

    // Remove OpenAction from catalog if it's JavaScript.
    if let Some(catalog_id) = get_catalog_id(doc) {
        let remove_open_action = {
            if let Some(Object::Dictionary(catalog)) = doc.objects.get(&catalog_id) {
                match catalog.get(b"OpenAction").ok() {
                    Some(Object::Reference(action_id)) => {
                        if let Some(Object::Dictionary(action)) = doc.objects.get(action_id) {
                            is_javascript_action(action)
                        } else {
                            false
                        }
                    }
                    Some(Object::Dictionary(action)) => is_javascript_action(action),
                    _ => false,
                }
            } else {
                false
            }
        };

        if remove_open_action {
            if let Some(Object::Dictionary(ref mut catalog)) = doc.objects.get_mut(&catalog_id) {
                catalog.remove(b"OpenAction");
                count += 1;
            }
        }
    }

    count
}

/// Remove EmbeddedFiles from catalog /Names (PDF/A-1 only).
pub fn remove_embedded_files(doc: &mut Document) -> usize {
    let mut count = 0;

    let catalog_id = match get_catalog_id(doc) {
        Some(id) => id,
        None => return 0,
    };

    // Check if Names is a reference.
    let names_id = {
        if let Some(Object::Dictionary(catalog)) = doc.objects.get(&catalog_id) {
            match catalog.get(b"Names").ok() {
                Some(Object::Reference(id)) => Some(*id),
                _ => None,
            }
        } else {
            None
        }
    };

    // Count embedded files before mutating.
    if let Some(nid) = names_id {
        if let Some(Object::Dictionary(names)) = doc.objects.get(&nid) {
            if let Ok(Object::Reference(ef_id)) = names.get(b"EmbeddedFiles") {
                count += count_name_tree_entries(doc, *ef_id);
            }
        }
    }

    // Remove from Names reference.
    if let Some(nid) = names_id {
        if let Some(Object::Dictionary(ref mut names)) = doc.objects.get_mut(&nid) {
            if names.has(b"EmbeddedFiles") {
                if count == 0 {
                    count = 1;
                }
                names.remove(b"EmbeddedFiles");
            }
        }
    }

    // Also check inline Names dict in catalog.
    let has_inline_ef = {
        if let Some(Object::Dictionary(catalog)) = doc.objects.get(&catalog_id) {
            if let Ok(Object::Dictionary(names)) = catalog.get(b"Names") {
                names.has(b"EmbeddedFiles")
            } else {
                false
            }
        } else {
            false
        }
    };

    if has_inline_ef {
        if count == 0 {
            count = 1;
        }
        if let Some(Object::Dictionary(ref mut catalog)) = doc.objects.get_mut(&catalog_id) {
            if let Ok(Object::Dictionary(ref mut names)) = catalog.get_mut(b"Names") {
                names.remove(b"EmbeddedFiles");
            }
        }
    }

    // Remove /AF entry from catalog.
    if let Some(Object::Dictionary(ref mut catalog)) = doc.objects.get_mut(&catalog_id) {
        if catalog.has(b"AF") {
            catalog.remove(b"AF");
        }
    }

    count
}

/// Count transparency groups in the document.
pub fn count_transparency_groups(doc: &Document) -> usize {
    doc.objects
        .values()
        .filter(|obj| {
            if let Object::Dictionary(dict) = obj {
                if let Ok(Object::Dictionary(group)) = dict.get(b"Group") {
                    if let Ok(Object::Name(s)) = group.get(b"S") {
                        return s == b"Transparency";
                    }
                }
            }
            false
        })
        .count()
}

/// Remove encryption dictionaries from the document.
pub fn remove_encryption(doc: &mut Document) -> bool {
    if doc.trailer.has(b"Encrypt") {
        doc.trailer.remove(b"Encrypt");
        true
    } else {
        false
    }
}

fn is_javascript_action(dict: &lopdf::Dictionary) -> bool {
    match dict.get(b"S").ok() {
        Some(Object::Name(s)) => s == b"JavaScript",
        _ => false,
    }
}

fn get_catalog_id(doc: &Document) -> Option<ObjectId> {
    match doc.trailer.get(b"Root").ok()? {
        Object::Reference(id) => Some(*id),
        _ => None,
    }
}

fn count_name_tree_entries(doc: &Document, tree_id: ObjectId) -> usize {
    if let Some(Object::Dictionary(tree)) = doc.objects.get(&tree_id) {
        if let Ok(Object::Array(names)) = tree.get(b"Names") {
            return names.len() / 2;
        }
        if let Ok(Object::Array(kids)) = tree.get(b"Kids") {
            return kids
                .iter()
                .map(|kid| {
                    if let Object::Reference(kid_id) = kid {
                        count_name_tree_entries(doc, *kid_id)
                    } else {
                        0
                    }
                })
                .sum();
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Stream};

    fn make_basic_doc() -> Document {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();

        let content = Stream::new(dictionary! {}, b"BT /F1 12 Tf (Hello) Tj ET".to_vec());
        let content_id = doc.add_object(Object::Stream(content));

        let page = dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(612), Object::Integer(792),
            ]),
            "Contents" => Object::Reference(content_id),
        };
        let page_id = doc.add_object(Object::Dictionary(page));

        let pages = dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(1),
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        doc
    }

    #[test]
    fn test_remove_javascript_empty() {
        let mut doc = make_basic_doc();
        let count = remove_javascript(&mut doc);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_remove_javascript_action() {
        let mut doc = make_basic_doc();

        let js_action = dictionary! {
            "S" => Object::Name(b"JavaScript".to_vec()),
            "JS" => Object::String(b"app.alert('hello')".to_vec(), lopdf::StringFormat::Literal),
        };
        doc.add_object(Object::Dictionary(js_action));

        let count = remove_javascript(&mut doc);
        assert!(count >= 1);
    }

    #[test]
    fn test_remove_additional_actions() {
        let mut doc = make_basic_doc();

        let aa_dict = dictionary! {
            "O" => Object::Dictionary(dictionary! {
                "S" => Object::Name(b"JavaScript".to_vec()),
                "JS" => Object::String(b"console.println()".to_vec(), lopdf::StringFormat::Literal),
            }),
        };

        let page_with_aa = dictionary! {
            "Type" => "Page",
            "AA" => Object::Dictionary(aa_dict),
        };
        doc.add_object(Object::Dictionary(page_with_aa));

        let count = remove_additional_actions(&mut doc);
        assert!(count >= 1);
    }

    #[test]
    fn test_remove_embedded_files() {
        let mut doc = make_basic_doc();
        let catalog_id = get_catalog_id(&doc).unwrap();

        let ef_tree = dictionary! {
            "Names" => Object::Array(vec![
                Object::String(b"test.txt".to_vec(), lopdf::StringFormat::Literal),
                Object::Null,
            ]),
        };
        let ef_id = doc.add_object(Object::Dictionary(ef_tree));

        let names = dictionary! {
            "EmbeddedFiles" => Object::Reference(ef_id),
        };
        let names_id = doc.add_object(Object::Dictionary(names));

        if let Some(Object::Dictionary(ref mut catalog)) = doc.objects.get_mut(&catalog_id) {
            catalog.set("Names", Object::Reference(names_id));
        }

        let count = remove_embedded_files(&mut doc);
        assert!(count >= 1);

        if let Some(Object::Dictionary(names_dict)) = doc.objects.get(&names_id) {
            assert!(!names_dict.has(b"EmbeddedFiles"));
        }
    }

    #[test]
    fn test_remove_encryption() {
        let mut doc = make_basic_doc();
        doc.trailer
            .set("Encrypt", Object::Reference((99, 0).into()));

        assert!(remove_encryption(&mut doc));
        assert!(!doc.trailer.has(b"Encrypt"));
    }

    #[test]
    fn test_remove_encryption_none() {
        let mut doc = make_basic_doc();
        assert!(!remove_encryption(&mut doc));
    }

    #[test]
    fn test_transparency_groups() {
        let mut doc = make_basic_doc();

        let group = dictionary! {
            "S" => Object::Name(b"Transparency".to_vec()),
            "CS" => Object::Name(b"DeviceRGB".to_vec()),
        };
        let xobj = dictionary! {
            "Type" => Object::Name(b"XObject".to_vec()),
            "Subtype" => Object::Name(b"Form".to_vec()),
            "Group" => Object::Dictionary(group),
        };
        doc.add_object(Object::Dictionary(xobj));

        assert_eq!(count_transparency_groups(&doc), 1);
    }

    #[test]
    fn test_full_cleanup() {
        let mut doc = make_basic_doc();

        let js = dictionary! {
            "S" => Object::Name(b"JavaScript".to_vec()),
            "JS" => Object::String(b"void(0)".to_vec(), lopdf::StringFormat::Literal),
        };
        doc.add_object(Object::Dictionary(js));

        let group = dictionary! {
            "S" => Object::Name(b"Transparency".to_vec()),
        };
        let xobj = dictionary! {
            "Group" => Object::Dictionary(group),
        };
        doc.add_object(Object::Dictionary(xobj));

        let report = cleanup_for_pdfa(&mut doc, true).unwrap();
        assert!(report.js_actions_removed >= 1);
        assert_eq!(report.transparency_groups_found, 1);
    }
}
