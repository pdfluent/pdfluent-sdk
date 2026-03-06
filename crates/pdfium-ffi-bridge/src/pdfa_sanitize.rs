//! PDF/A sanitization — remove prohibited elements for PDF/A-2b compliance.
//!
//! PDF/A-2b prohibits:
//! - JavaScript actions (§6.1.6)
//! - Embedded files not conforming to PDF/A (§6.8)
//! - Additional actions (AA) on pages and catalog (§6.1.5)
//!
//! This module strips these elements from a lopdf Document.

use lopdf::{Object, ObjectId};

/// Remove all JavaScript and prohibited actions from a PDF document.
///
/// Strips:
/// - `Names.JavaScript` name tree from the catalog
/// - `OpenAction` from the catalog if it references JavaScript
/// - `AA` (Additional Actions) from the catalog
/// - `AA` from all page dictionaries
pub fn remove_javascript(doc: &mut lopdf::Document) {
    let catalog_id = match doc.trailer.get(b"Root") {
        Ok(Object::Reference(id)) => *id,
        _ => return,
    };

    // Collect the Names dict reference (if indirect) for JavaScript removal.
    let names_ref = if let Ok(Object::Dictionary(catalog)) = doc.get_object(catalog_id) {
        match catalog.get(b"Names") {
            Ok(Object::Reference(r)) => Some(*r),
            _ => None,
        }
    } else {
        None
    };

    // Remove JavaScript from Names dictionary.
    if let Some(names_id) = names_ref {
        // Indirect Names dict
        if let Ok(Object::Dictionary(ref mut names)) = doc.get_object_mut(names_id) {
            names.remove(b"JavaScript");
        }
    } else if let Ok(Object::Dictionary(ref mut catalog)) = doc.get_object_mut(catalog_id) {
        // Inline Names dict
        if let Ok(Object::Dictionary(ref mut names)) = catalog.get_mut(b"Names") {
            names.remove(b"JavaScript");
        }
    }

    // Remove OpenAction if it's a JavaScript action.
    let remove_open_action = if let Ok(Object::Dictionary(catalog)) = doc.get_object(catalog_id) {
        is_javascript_action(doc, catalog.get(b"OpenAction"))
    } else {
        false
    };

    if let Ok(Object::Dictionary(ref mut catalog)) = doc.get_object_mut(catalog_id) {
        if remove_open_action {
            catalog.remove(b"OpenAction");
        }
        // Remove AA (Additional Actions) from catalog — always prohibited.
        catalog.remove(b"AA");
    }

    // Remove AA from all pages.
    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    for page_id in page_ids {
        if let Ok(Object::Dictionary(ref mut page)) = doc.get_object_mut(page_id) {
            page.remove(b"AA");
        }
    }
}

/// Remove embedded files from the document.
///
/// Strips `Names.EmbeddedFiles` and `AF` (Associated Files) entries from
/// the catalog. PDF/A-2b only permits embedded files that themselves
/// conform to PDF/A; since we cannot verify that, we remove them all.
pub fn remove_embedded_files(doc: &mut lopdf::Document) {
    let catalog_id = match doc.trailer.get(b"Root") {
        Ok(Object::Reference(id)) => *id,
        _ => return,
    };

    // Remove EmbeddedFiles from Names dictionary.
    let names_ref = if let Ok(Object::Dictionary(catalog)) = doc.get_object(catalog_id) {
        match catalog.get(b"Names") {
            Ok(Object::Reference(r)) => Some(*r),
            _ => None,
        }
    } else {
        None
    };

    if let Some(names_id) = names_ref {
        if let Ok(Object::Dictionary(ref mut names)) = doc.get_object_mut(names_id) {
            names.remove(b"EmbeddedFiles");
        }
    } else if let Ok(Object::Dictionary(ref mut catalog)) = doc.get_object_mut(catalog_id) {
        if let Ok(Object::Dictionary(ref mut names)) = catalog.get_mut(b"Names") {
            names.remove(b"EmbeddedFiles");
        }
    }

    // Remove AF (Associated Files) from catalog.
    if let Ok(Object::Dictionary(ref mut catalog)) = doc.get_object_mut(catalog_id) {
        catalog.remove(b"AF");
    }

    // Remove AF from all pages.
    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    for page_id in page_ids {
        if let Ok(Object::Dictionary(ref mut page)) = doc.get_object_mut(page_id) {
            page.remove(b"AF");
        }
    }
}

/// Check whether a catalog entry is a JavaScript action.
fn is_javascript_action(
    doc: &lopdf::Document,
    entry: std::result::Result<&Object, lopdf::Error>,
) -> bool {
    let obj = match entry {
        Ok(Object::Reference(r)) => match doc.get_object(*r) {
            Ok(o) => o,
            Err(_) => return false,
        },
        Ok(o) => o,
        Err(_) => return false,
    };

    if let Ok(dict) = obj.as_dict() {
        if let Ok(Object::Name(s)) = dict.get(b"S") {
            return s == b"JavaScript";
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Dictionary, Object};

    fn make_doc_with_catalog(catalog: Dictionary) -> (lopdf::Document, ObjectId) {
        let mut doc = lopdf::Document::new();
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));
        (doc, catalog_id)
    }

    #[test]
    fn remove_javascript_from_names() {
        let names = dictionary! {
            "JavaScript" => Object::Array(vec![
                Object::String(b"script1".to_vec(), lopdf::StringFormat::Literal),
            ]),
            "Dests" => Object::Array(vec![]),
        };
        let catalog = dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "Names" => Object::Dictionary(names),
        };
        let (mut doc, catalog_id) = make_doc_with_catalog(catalog);

        remove_javascript(&mut doc);

        let cat = doc.get_object(catalog_id).unwrap().as_dict().unwrap();
        let names = cat.get(b"Names").unwrap().as_dict().unwrap();
        assert!(
            names.get(b"JavaScript").is_err(),
            "JavaScript should be removed"
        );
        assert!(names.get(b"Dests").is_ok(), "Dests should be preserved");
    }

    #[test]
    fn remove_javascript_open_action() {
        let mut doc = lopdf::Document::new();
        let js_action = dictionary! {
            "S" => Object::Name(b"JavaScript".to_vec()),
            "JS" => Object::String(b"alert('hi')".to_vec(), lopdf::StringFormat::Literal),
        };
        let js_id = doc.add_object(Object::Dictionary(js_action));
        let catalog = dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "OpenAction" => Object::Reference(js_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        remove_javascript(&mut doc);

        let cat = doc.get_object(catalog_id).unwrap().as_dict().unwrap();
        assert!(
            cat.get(b"OpenAction").is_err(),
            "JS OpenAction should be removed"
        );
    }

    #[test]
    fn preserve_non_js_open_action() {
        let goto_action = dictionary! {
            "S" => Object::Name(b"GoTo".to_vec()),
        };
        let catalog = dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "OpenAction" => Object::Dictionary(goto_action),
        };
        let (mut doc, catalog_id) = make_doc_with_catalog(catalog);

        remove_javascript(&mut doc);

        let cat = doc.get_object(catalog_id).unwrap().as_dict().unwrap();
        assert!(
            cat.get(b"OpenAction").is_ok(),
            "GoTo OpenAction should be preserved"
        );
    }

    #[test]
    fn remove_aa_from_catalog() {
        let aa = dictionary! {
            "WC" => Object::Dictionary(dictionary! {
                "S" => Object::Name(b"JavaScript".to_vec()),
            }),
        };
        let catalog = dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "AA" => Object::Dictionary(aa),
        };
        let (mut doc, catalog_id) = make_doc_with_catalog(catalog);

        remove_javascript(&mut doc);

        let cat = doc.get_object(catalog_id).unwrap().as_dict().unwrap();
        assert!(cat.get(b"AA").is_err(), "AA should be removed");
    }

    #[test]
    fn remove_embedded_files_from_names() {
        let names = dictionary! {
            "EmbeddedFiles" => Object::Array(vec![]),
            "Dests" => Object::Array(vec![]),
        };
        let catalog = dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "Names" => Object::Dictionary(names),
        };
        let (mut doc, catalog_id) = make_doc_with_catalog(catalog);

        remove_embedded_files(&mut doc);

        let cat = doc.get_object(catalog_id).unwrap().as_dict().unwrap();
        let names = cat.get(b"Names").unwrap().as_dict().unwrap();
        assert!(
            names.get(b"EmbeddedFiles").is_err(),
            "EmbeddedFiles should be removed"
        );
        assert!(names.get(b"Dests").is_ok(), "Dests should be preserved");
    }

    #[test]
    fn remove_af_from_catalog() {
        let catalog = dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "AF" => Object::Array(vec![]),
        };
        let (mut doc, catalog_id) = make_doc_with_catalog(catalog);

        remove_embedded_files(&mut doc);

        let cat = doc.get_object(catalog_id).unwrap().as_dict().unwrap();
        assert!(cat.get(b"AF").is_err(), "AF should be removed");
    }
}
