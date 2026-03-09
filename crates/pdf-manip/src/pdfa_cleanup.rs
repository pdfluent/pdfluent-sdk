//! PDF/A cleanup: remove JavaScript, EmbeddedFiles, transparency, and other
//! PDF/A-incompatible elements.
//!
//! Strips and repairs document structures for PDF/A compliance.

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
    /// Number of TR keys removed from ExtGState dictionaries.
    pub transfer_functions_removed: usize,
    /// Whether the trailer /ID was added.
    pub trailer_id_added: bool,
    /// Number of annotation flag fixes applied.
    pub annotation_flags_fixed: usize,
    /// Number of LZW streams re-encoded to FlateDecode.
    pub lzw_streams_reencoded: usize,
    /// Number of OCG dictionaries fixed.
    pub ocg_fixes: usize,
}

/// Remove all PDF/A-incompatible elements from the document.
pub fn cleanup_for_pdfa(doc: &mut Document, is_pdfa1: bool) -> Result<PdfACleanupReport> {
    let mut report = PdfACleanupReport {
        js_actions_removed: 0,
        embedded_files_removed: 0,
        transparency_groups_found: 0,
        encryption_removed: false,
        aa_entries_removed: 0,
        transfer_functions_removed: 0,
        trailer_id_added: false,
        annotation_flags_fixed: 0,
        lzw_streams_reencoded: 0,
        ocg_fixes: 0,
    };

    report.js_actions_removed = remove_javascript(doc);
    report.aa_entries_removed = remove_additional_actions(doc);
    report.transparency_groups_found = count_transparency_groups(doc);

    if is_pdfa1 {
        report.embedded_files_removed = remove_embedded_files(doc);
    }

    report.encryption_removed = remove_encryption(doc);
    report.transfer_functions_removed = remove_transfer_functions(doc);
    report.trailer_id_added = ensure_trailer_id(doc);
    report.annotation_flags_fixed = fix_annotation_flags(doc);
    report.lzw_streams_reencoded = reencode_lzw_streams(doc);
    report.ocg_fixes = fix_optional_content(doc);
    fix_need_appearances(doc);

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

/// Remove TR (transfer function) keys from ExtGState dictionaries (6.2.5).
fn remove_transfer_functions(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let has_tr = {
            if let Some(Object::Dictionary(dict)) = doc.objects.get(&id) {
                dict.has(b"TR") || dict.has(b"TR2")
            } else {
                false
            }
        };
        if has_tr {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                if dict.has(b"TR") {
                    dict.remove(b"TR");
                    count += 1;
                }
                if dict.has(b"TR2") {
                    dict.remove(b"TR2");
                    count += 1;
                }
            }
        }
    }
    count
}

/// Ensure the trailer has an /ID entry (6.1.3).
fn ensure_trailer_id(doc: &mut Document) -> bool {
    if doc.trailer.has(b"ID") {
        return false;
    }
    // Generate a deterministic ID based on document content.
    let id_bytes: Vec<u8> = {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        doc.objects.len().hash(&mut h);
        let hash = h.finish();
        let mut bytes = Vec::with_capacity(16);
        bytes.extend_from_slice(&hash.to_be_bytes());
        bytes.extend_from_slice(&hash.to_le_bytes());
        bytes
    };
    let id1 = Object::String(id_bytes.clone(), lopdf::StringFormat::Hexadecimal);
    let id2 = Object::String(id_bytes, lopdf::StringFormat::Hexadecimal);
    doc.trailer.set("ID", Object::Array(vec![id1, id2]));
    true
}

/// Fix annotation flags for PDF/A compliance (6.3.2).
/// Print flag (bit 3) must be set, Hidden/Invisible/ToggleNoView/NoView must be clear.
fn fix_annotation_flags(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let needs_fix = {
            if let Some(Object::Dictionary(dict)) = doc.objects.get(&id) {
                let is_annot = match dict.get(b"Subtype").ok() {
                    Some(Object::Name(n)) => {
                        // Popup annotations are exempt
                        n != b"Popup" && is_annotation_subtype(n)
                    }
                    _ => false,
                };
                if !is_annot {
                    false
                } else {
                    let f = match dict.get(b"F").ok() {
                        Some(Object::Integer(v)) => *v as u32,
                        _ => 0,
                    };
                    // PDF spec F flags (1-indexed bits, stored 0-indexed):
                    //   1=Invisible(0x01), 2=Hidden(0x02), 3=Print(0x04),
                    //   6=NoView(0x20), 9=ToggleNoView(0x100)
                    let print_bit: u32 = 1 << 2;
                    let bad: u32 = (1 << 0) | (1 << 1) | (1 << 5) | (1 << 8);
                    (f & print_bit == 0) || (f & bad != 0)
                }
            } else {
                false
            }
        };
        if needs_fix {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                let f = match dict.get(b"F").ok() {
                    Some(Object::Integer(v)) => *v as u32,
                    _ => 0,
                };
                let print_bit: u32 = 1 << 2;
                let bad: u32 = (1 << 0) | (1 << 1) | (1 << 5) | (1 << 8);
                let new_f = (f | print_bit) & !bad;
                dict.set("F", Object::Integer(new_f as i64));
                count += 1;
            }
        }
    }
    count
}

/// Re-encode LZW-compressed streams to FlateDecode (6.1.7.2).
fn reencode_lzw_streams(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let has_lzw = {
            if let Some(Object::Stream(stream)) = doc.objects.get(&id) {
                match stream.dict.get(b"Filter").ok() {
                    Some(Object::Name(n)) => n == b"LZWDecode",
                    Some(Object::Array(arr)) => arr.iter().any(|o| {
                        if let Object::Name(n) = o {
                            n == b"LZWDecode"
                        } else {
                            false
                        }
                    }),
                    _ => false,
                }
            } else {
                false
            }
        };
        if has_lzw {
            let decoded = {
                if let Some(Object::Stream(stream)) = doc.objects.get(&id) {
                    stream.decompressed_content().ok()
                } else {
                    None
                }
            };
            if let Some(raw_data) = decoded {
                let compressed = {
                    use std::io::Write;
                    let mut encoder =
                        flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
                    if encoder.write_all(&raw_data).is_ok() {
                        encoder.finish().ok()
                    } else {
                        None
                    }
                };
                if let Some(compressed_data) = compressed {
                    if let Some(Object::Stream(ref mut stream)) = doc.objects.get_mut(&id) {
                        stream.set_content(compressed_data);
                        stream
                            .dict
                            .set("Filter", Object::Name(b"FlateDecode".to_vec()));
                        stream.dict.remove(b"DecodeParms");
                        count += 1;
                    }
                }
            }
        }
    }
    count
}

/// Set NeedAppearances to false in AcroForm dictionary (6.4.1 t3).
fn fix_need_appearances(doc: &mut Document) {
    let catalog_id = match get_catalog_id(doc) {
        Some(id) => id,
        None => return,
    };

    let acroform_id = {
        if let Some(Object::Dictionary(catalog)) = doc.objects.get(&catalog_id) {
            match catalog.get(b"AcroForm").ok() {
                Some(Object::Reference(id)) => Some(*id),
                _ => None,
            }
        } else {
            None
        }
    };

    if let Some(af_id) = acroform_id {
        if let Some(Object::Dictionary(ref mut af)) = doc.objects.get_mut(&af_id) {
            if let Ok(Object::Boolean(true)) = af.get(b"NeedAppearances") {
                af.set("NeedAppearances", Object::Boolean(false));
            }
        }
    } else {
        // Check inline AcroForm in catalog
        if let Some(Object::Dictionary(ref mut catalog)) = doc.objects.get_mut(&catalog_id) {
            if let Ok(Object::Dictionary(ref mut af)) = catalog.get_mut(b"AcroForm") {
                if let Ok(Object::Boolean(true)) = af.get(b"NeedAppearances") {
                    af.set("NeedAppearances", Object::Boolean(false));
                }
            }
        }
    }
}

/// Fix Optional Content (OCG) dictionaries for PDF/A compliance (6.9).
fn fix_optional_content(doc: &mut Document) -> usize {
    let mut count = 0;

    let catalog_id = match get_catalog_id(doc) {
        Some(id) => id,
        None => return 0,
    };

    let ocprops_id = {
        if let Some(Object::Dictionary(catalog)) = doc.objects.get(&catalog_id) {
            match catalog.get(b"OCProperties").ok() {
                Some(Object::Reference(id)) => Some(*id),
                _ => None,
            }
        } else {
            None
        }
    };

    let ocprops_id = match ocprops_id {
        Some(id) => id,
        None => return 0,
    };

    // Fix D (default config) dictionary: ensure Name key is present
    let d_id = {
        if let Some(Object::Dictionary(ocprops)) = doc.objects.get(&ocprops_id) {
            match ocprops.get(b"D").ok() {
                Some(Object::Reference(id)) => Some(*id),
                _ => None,
            }
        } else {
            None
        }
    };

    if let Some(did) = d_id {
        if let Some(Object::Dictionary(ref mut d_dict)) = doc.objects.get_mut(&did) {
            if !d_dict.has(b"Name") {
                d_dict.set(
                    "Name",
                    Object::String(b"Default".to_vec(), lopdf::StringFormat::Literal),
                );
                count += 1;
            }
        }
    } else {
        // D might be inline
        if let Some(Object::Dictionary(ref mut ocprops)) = doc.objects.get_mut(&ocprops_id) {
            if let Ok(Object::Dictionary(ref mut d_dict)) = ocprops.get_mut(b"D") {
                if !d_dict.has(b"Name") {
                    d_dict.set(
                        "Name",
                        Object::String(b"Default".to_vec(), lopdf::StringFormat::Literal),
                    );
                    count += 1;
                }
            }
        }
    }

    // Fix Configs array entries similarly
    let config_ids: Vec<ObjectId> = {
        if let Some(Object::Dictionary(ocprops)) = doc.objects.get(&ocprops_id) {
            match ocprops.get(b"Configs").ok() {
                Some(Object::Array(arr)) => arr
                    .iter()
                    .filter_map(|o| {
                        if let Object::Reference(id) = o {
                            Some(*id)
                        } else {
                            None
                        }
                    })
                    .collect(),
                _ => vec![],
            }
        } else {
            vec![]
        }
    };

    for config_id in config_ids {
        if let Some(Object::Dictionary(ref mut config)) = doc.objects.get_mut(&config_id) {
            if !config.has(b"Name") {
                config.set(
                    "Name",
                    Object::String(b"Config".to_vec(), lopdf::StringFormat::Literal),
                );
                count += 1;
            }
        }
    }

    count
}

fn is_annotation_subtype(name: &[u8]) -> bool {
    matches!(
        name,
        b"Text"
            | b"Link"
            | b"FreeText"
            | b"Line"
            | b"Square"
            | b"Circle"
            | b"Polygon"
            | b"PolyLine"
            | b"Highlight"
            | b"Underline"
            | b"Squiggly"
            | b"StrikeOut"
            | b"Stamp"
            | b"Caret"
            | b"Ink"
            | b"FileAttachment"
            | b"Sound"
            | b"Movie"
            | b"Widget"
            | b"Screen"
            | b"PrinterMark"
            | b"TrapNet"
            | b"Watermark"
            | b"3D"
            | b"Redact"
    )
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
    use lopdf::{Stream, dictionary};

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

    #[test]
    fn test_remove_transfer_functions() {
        let mut doc = make_basic_doc();
        let gs = dictionary! {
            "TR" => Object::Name(b"Identity".to_vec()),
        };
        doc.add_object(Object::Dictionary(gs));
        assert_eq!(remove_transfer_functions(&mut doc), 1);
    }

    #[test]
    fn test_ensure_trailer_id() {
        let mut doc = make_basic_doc();
        assert!(!doc.trailer.has(b"ID"));
        assert!(ensure_trailer_id(&mut doc));
        assert!(doc.trailer.has(b"ID"));
        // Second call should be no-op
        assert!(!ensure_trailer_id(&mut doc));
    }

    #[test]
    fn test_fix_annotation_flags() {
        let mut doc = make_basic_doc();
        let annot = dictionary! {
            "Subtype" => Object::Name(b"Text".to_vec()),
            "F" => Object::Integer(0), // Print not set
        };
        doc.add_object(Object::Dictionary(annot));
        assert_eq!(fix_annotation_flags(&mut doc), 1);
    }
}
