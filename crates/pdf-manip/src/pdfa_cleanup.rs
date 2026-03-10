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
    /// Number of invalid rendering intents normalized.
    pub rendering_intents_fixed: usize,
    /// Whether the trailer /ID was added.
    pub trailer_id_added: bool,
    /// Number of annotation flag fixes applied.
    pub annotation_flags_fixed: usize,
    /// Number of LZW streams re-encoded to FlateDecode.
    pub lzw_streams_reencoded: usize,
    /// Number of OCG dictionaries fixed.
    pub ocg_fixes: usize,
    /// Number of CIDToGIDMap entries added.
    pub cidtogidmap_added: usize,
    /// Number of annotation AP fixes.
    pub ap_fixes: usize,
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
        rendering_intents_fixed: 0,
        trailer_id_added: false,
        annotation_flags_fixed: 0,
        lzw_streams_reencoded: 0,
        ocg_fixes: 0,
        cidtogidmap_added: 0,
        ap_fixes: 0,
    };

    // Force PDF version to 1.7 for PDF/A-2 compliance (6.1.2).
    if doc.version.starts_with('2') {
        doc.version = "1.7".to_string();
    }

    report.js_actions_removed = remove_javascript(doc);
    report.aa_entries_removed = remove_additional_actions(doc);
    report.transparency_groups_found = count_transparency_groups(doc);

    if is_pdfa1 {
        report.embedded_files_removed = remove_embedded_files(doc);
    }

    report.encryption_removed = remove_encryption(doc);
    report.transfer_functions_removed = remove_transfer_functions(doc);
    report.rendering_intents_fixed = normalize_rendering_intents(doc);
    report.trailer_id_added = ensure_trailer_id(doc);
    report.annotation_flags_fixed = fix_annotation_flags(doc);
    report.lzw_streams_reencoded = reencode_lzw_streams(doc);
    report.ocg_fixes = fix_optional_content(doc);
    fix_need_appearances(doc);
    remove_forbidden_actions(doc);
    fix_image_interpolate(doc);
    report.cidtogidmap_added = fix_cidtogidmap(doc);
    report.ap_fixes = fix_annotation_ap(doc);
    strip_ap_non_normal(doc);
    fix_ocg_order(doc);
    fix_annotation_contents(doc);
    fix_annotation_contents_type(doc);
    fix_widget_actions(doc);
    remove_xfa_from_acroform(doc);
    fix_widget_btn_appearance(doc);
    fix_form_xobject_keys(doc);
    // NOTE: fix_font_descriptor_keys and fix_font_lastchar_widths disabled —
    // they run before embed_fonts and create incorrect state (zero widths,
    // wrong FontFile key). Font fixes are handled by embed_fonts instead.
    // fix_font_descriptor_keys(doc);
    // fix_font_lastchar_widths(doc);
    ensure_xmp_dc_title(doc);
    truncate_long_names(doc);
    fix_soft_mask_colorspace(doc);
    remove_halftone_names(doc);
    remove_needs_rendering(doc);
    remove_forbidden_annotations(doc);
    fix_file_spec_keys(doc);
    strip_ef_from_file_specs(doc);
    remove_ocg_as_key(doc);
    strip_signatures(doc);
    strip_non_catalog_metadata(doc);

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

    // Remove OpenAction from catalog if it's any forbidden action.
    if let Some(catalog_id) = get_catalog_id(doc) {
        let remove_open_action = {
            if let Some(Object::Dictionary(catalog)) = doc.objects.get(&catalog_id) {
                match catalog.get(b"OpenAction").ok() {
                    Some(Object::Reference(action_id)) => {
                        if let Some(Object::Dictionary(action)) = doc.objects.get(action_id) {
                            is_javascript_action(action) || is_action_forbidden(action)
                        } else {
                            false
                        }
                    }
                    Some(Object::Dictionary(action)) => {
                        is_javascript_action(action) || is_action_forbidden(action)
                    }
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

/// Normalize rendering intents in ExtGState dictionaries (6.2.6).
///
/// Replaces invalid /RI values with "RelativeColorimetric" (the default).
/// Valid values: RelativeColorimetric, AbsoluteColorimetric, Perceptual, Saturation.
fn normalize_rendering_intents(doc: &mut Document) -> usize {
    let valid = [
        b"RelativeColorimetric" as &[u8],
        b"AbsoluteColorimetric",
        b"Perceptual",
        b"Saturation",
    ];
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let needs_fix = {
            if let Some(Object::Dictionary(dict)) = doc.objects.get(&id) {
                if let Ok(Object::Name(ri)) = dict.get(b"RI") {
                    !valid.contains(&ri.as_slice())
                } else {
                    false
                }
            } else {
                false
            }
        };
        if needs_fix {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                dict.set("RI", Object::Name(b"RelativeColorimetric".to_vec()));
                count += 1;
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
/// All annotations must have F key. Print flag (bit 3) must be set,
/// Hidden/Invisible/ToggleNoView/NoView must be clear.
fn fix_annotation_flags(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let needs_fix = {
            if let Some(Object::Dictionary(dict)) = doc.objects.get(&id) {
                let is_annot = match dict.get(b"Subtype").ok() {
                    Some(Object::Name(n)) => is_annotation_subtype(n),
                    _ => false,
                };
                if !is_annot {
                    false
                } else {
                    let f = match dict.get(b"F").ok() {
                        Some(Object::Integer(v)) => *v as u32,
                        _ => 0, // Missing F key = 0 = needs fix
                    };
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

    // Find OCProperties — may be a reference or inline in the Catalog.
    let (ocprops_id, ocprops_inline) = {
        if let Some(Object::Dictionary(catalog)) = doc.objects.get(&catalog_id) {
            match catalog.get(b"OCProperties").ok() {
                Some(Object::Reference(id)) => (Some(*id), false),
                Some(Object::Dictionary(_)) => (None, true),
                _ => return 0,
            }
        } else {
            return 0;
        }
    };

    if !ocprops_inline && ocprops_id.is_none() {
        return 0;
    }

    // Helper: fix D config dict inline.
    fn fix_d_config(d_dict: &mut lopdf::Dictionary, ocgs: &[Object], count: &mut usize) {
        if !d_dict.has(b"Name") {
            d_dict.set(
                "Name",
                Object::String(b"Default".to_vec(), lopdf::StringFormat::Literal),
            );
            *count += 1;
        }
        if !ocgs.is_empty() {
            d_dict.set("Order", Object::Array(ocgs.to_vec()));
            *count += 1;
        }
        if d_dict.has(b"AS") {
            d_dict.remove(b"AS");
            *count += 1;
        }
    }

    if ocprops_inline {
        // OCProperties is inline in the Catalog dict.
        // Extract OCGs list first.
        let ocgs: Vec<Object> = {
            if let Some(Object::Dictionary(catalog)) = doc.objects.get(&catalog_id) {
                if let Ok(Object::Dictionary(ocprops)) = catalog.get(b"OCProperties") {
                    match ocprops.get(b"OCGs").ok() {
                        Some(Object::Array(arr)) => arr.clone(),
                        _ => vec![],
                    }
                } else {
                    vec![]
                }
            } else {
                vec![]
            }
        };

        // Fix the inline D config.
        if let Some(Object::Dictionary(ref mut catalog)) = doc.objects.get_mut(&catalog_id) {
            if let Ok(Object::Dictionary(ref mut ocprops)) = catalog.get_mut(b"OCProperties") {
                // Fix D config.
                if let Ok(Object::Dictionary(ref mut d_dict)) = ocprops.get_mut(b"D") {
                    fix_d_config(d_dict, &ocgs, &mut count);
                }
            }
        }

        return count;
    }

    let ocprops_id = ocprops_id.unwrap();

    // Get OCGs list for Order fixing.
    let ocgs: Vec<Object> = {
        if let Some(Object::Dictionary(ocprops)) = doc.objects.get(&ocprops_id) {
            match ocprops.get(b"OCGs").ok() {
                Some(Object::Array(arr)) => arr.clone(),
                _ => vec![],
            }
        } else {
            vec![]
        }
    };

    // Fix D (default config) dictionary: ensure Name key is present, fix Order, remove AS.
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
            fix_d_config(d_dict, &ocgs, &mut count);
        }
    } else {
        // D might be inline in the OCProperties object.
        if let Some(Object::Dictionary(ref mut ocprops)) = doc.objects.get_mut(&ocprops_id) {
            if let Ok(Object::Dictionary(ref mut d_dict)) = ocprops.get_mut(b"D") {
                fix_d_config(d_dict, &ocgs, &mut count);
            }
        }
    }

    // Fix Configs array entries: ensure Name, remove AS.
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
            if config.has(b"AS") {
                config.remove(b"AS");
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
            | b"Popup"
    )
}

/// Check if an action name is forbidden for PDF/A.
fn is_forbidden_action(s: &[u8]) -> bool {
    matches!(
        s,
        b"Launch"
            | b"Sound"
            | b"Movie"
            | b"ResetForm"
            | b"ImportData"
            | b"Hide"
            | b"SetOCGState"
            | b"Rendition"
            | b"Trans"
            | b"GoTo3DView"
            | b"GoToE"
            | b"SetState"
            | b"NoOp"
            | b"JavaScript"
    )
}

/// Check if a Named action is forbidden.
fn is_forbidden_named_action(dict: &lopdf::Dictionary) -> bool {
    match dict.get(b"N").ok() {
        Some(Object::Name(n)) => !matches!(
            n.as_slice(),
            b"NextPage" | b"PrevPage" | b"FirstPage" | b"LastPage"
        ),
        _ => true,
    }
}

/// Check if an action dict has a forbidden type.
fn is_action_forbidden(dict: &lopdf::Dictionary) -> bool {
    if let Ok(Object::Name(s)) = dict.get(b"S") {
        if is_forbidden_action(s) {
            return true;
        }
        if s == b"Named" && is_forbidden_named_action(dict) {
            return true;
        }
    }
    false
}

/// Remove forbidden action types for PDF/A (6.5.1).
///
/// Handles both top-level action objects AND inline action dicts
/// within /A keys of annotations, outlines, and other objects.
fn remove_forbidden_actions(doc: &mut Document) {
    // Phase 1: Remove /S from top-level action objects with forbidden types.
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let needs_fix = {
            if let Some(Object::Dictionary(dict)) = doc.objects.get(&id) {
                is_action_forbidden(dict)
            } else {
                false
            }
        };
        if needs_fix {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                dict.remove(b"S");
            }
        }
    }

    // Phase 2: Remove /A keys from objects where /A points to a forbidden
    // inline action dict (not a separate object).
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let remove_a = {
            if let Some(Object::Dictionary(dict)) = doc.objects.get(&id) {
                match dict.get(b"A").ok() {
                    Some(Object::Dictionary(action)) => is_action_forbidden(action),
                    _ => false,
                }
            } else {
                false
            }
        };
        if remove_a {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                dict.remove(b"A");
            }
        }
    }

    // Phase 3: Also remove /A from objects where /A is a Reference to a
    // forbidden action (Phase 1 removed /S but the reference still exists).
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let remove_a = {
            if let Some(Object::Dictionary(dict)) = doc.objects.get(&id) {
                match dict.get(b"A").ok() {
                    Some(Object::Reference(action_id)) => {
                        match doc.objects.get(action_id) {
                            Some(Object::Dictionary(action)) => {
                                // Action had /S removed in Phase 1 — check if it's now
                                // missing /S (was forbidden) or still has a valid /S.
                                !action.has(b"S")
                            }
                            _ => false,
                        }
                    }
                    _ => false,
                }
            } else {
                false
            }
        };
        if remove_a {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                dict.remove(b"A");
            }
        }
    }
}

/// Set Interpolate=false on all image XObjects (6.2.8 t3).
fn fix_image_interpolate(doc: &mut Document) {
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let needs_fix = {
            if let Some(Object::Stream(stream)) = doc.objects.get(&id) {
                let is_image = matches!(
                    stream.dict.get(b"Subtype").ok(),
                    Some(Object::Name(ref n)) if n == b"Image"
                );
                is_image
                    && matches!(
                        stream.dict.get(b"Interpolate").ok(),
                        Some(Object::Boolean(true))
                    )
            } else {
                false
            }
        };
        if needs_fix {
            if let Some(Object::Stream(ref mut stream)) = doc.objects.get_mut(&id) {
                stream.dict.set("Interpolate", Object::Boolean(false));
            }
        }
    }
}

/// Add CIDToGIDMap /Identity to CIDFontType2 fonts if missing (6.2.11.4.1).
fn fix_cidtogidmap(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let needs_fix = {
            if let Some(Object::Dictionary(dict)) = doc.objects.get(&id) {
                let is_cidfont2 = matches!(
                    dict.get(b"Subtype").ok(),
                    Some(Object::Name(ref n)) if n == b"CIDFontType2"
                );
                is_cidfont2 && !dict.has(b"CIDToGIDMap")
            } else {
                false
            }
        };
        if needs_fix {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                dict.set("CIDToGIDMap", Object::Name(b"Identity".to_vec()));
                count += 1;
            }
        }
    }
    count
}

/// Fix annotation AP dictionaries: ensure /N (normal appearance) exists (6.3.3 t2).
fn fix_annotation_ap(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let fix_info = {
            if let Some(Object::Dictionary(dict)) = doc.objects.get(&id) {
                let is_annot = matches!(
                    dict.get(b"Subtype").ok(),
                    Some(Object::Name(ref n)) if is_annotation_subtype(n) && n != b"Popup" && n != b"Link"
                );
                if !is_annot {
                    ApFixInfo::None
                } else {
                    // Check for zero-size rect (exempt from AP requirement).
                    let is_zero_size = match dict.get(b"Rect").ok() {
                        Some(Object::Array(arr)) if arr.len() == 4 => {
                            let nums: Vec<f64> = arr
                                .iter()
                                .filter_map(|o| match o {
                                    Object::Integer(i) => Some(*i as f64),
                                    Object::Real(r) => Some(*r as f64),
                                    _ => None,
                                })
                                .collect();
                            nums.len() == 4
                                && (nums[0] - nums[2]).abs() < 0.01
                                && (nums[1] - nums[3]).abs() < 0.01
                        }
                        _ => false,
                    };
                    if is_zero_size {
                        ApFixInfo::None
                    } else {
                        match dict.get(b"AP").ok() {
                            Some(Object::Dictionary(ap)) => {
                                if ap.has(b"N") {
                                    ApFixInfo::None
                                } else {
                                    let fallback = ap
                                        .get(b"D")
                                        .ok()
                                        .cloned()
                                        .or_else(|| ap.get(b"R").ok().cloned());
                                    ApFixInfo::InlineAp(fallback)
                                }
                            }
                            Some(Object::Reference(ap_id)) => ApFixInfo::RefAp(*ap_id),
                            _ => ApFixInfo::MissingAp, // No AP at all — need to create one.
                        }
                    }
                }
            } else {
                ApFixInfo::None
            }
        };
        match fix_info {
            ApFixInfo::InlineAp(Some(val)) => {
                if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                    if let Ok(Object::Dictionary(ref mut ap)) = dict.get_mut(b"AP") {
                        ap.set("N", val);
                        count += 1;
                    }
                }
            }
            ApFixInfo::RefAp(ap_id) => {
                // Check referenced AP dict.
                let fallback = {
                    if let Some(Object::Dictionary(ap)) = doc.objects.get(&ap_id) {
                        if ap.has(b"N") {
                            None
                        } else {
                            ap.get(b"D")
                                .ok()
                                .cloned()
                                .or_else(|| ap.get(b"R").ok().cloned())
                        }
                    } else {
                        None
                    }
                };
                if let Some(val) = fallback {
                    if let Some(Object::Dictionary(ref mut ap)) = doc.objects.get_mut(&ap_id) {
                        ap.set("N", val);
                        count += 1;
                    }
                }
            }
            ApFixInfo::MissingAp => {
                // Create an empty appearance stream and AP dict.
                let mut stream_dict = lopdf::Dictionary::new();
                stream_dict.set("Type", Object::Name(b"XObject".to_vec()));
                stream_dict.set("Subtype", Object::Name(b"Form".to_vec()));
                stream_dict.set(
                    "BBox",
                    Object::Array(vec![
                        Object::Integer(0),
                        Object::Integer(0),
                        Object::Integer(0),
                        Object::Integer(0),
                    ]),
                );
                let empty_stream = lopdf::Stream::new(stream_dict, Vec::new());
                let stream_id = doc.add_object(Object::Stream(empty_stream));
                if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                    let mut ap = lopdf::Dictionary::new();
                    ap.set("N", Object::Reference(stream_id));
                    dict.set("AP", Object::Dictionary(ap));
                    count += 1;
                }
            }
            _ => {}
        }
    }
    count
}

enum ApFixInfo {
    None,
    InlineAp(Option<Object>),
    RefAp(ObjectId),
    MissingAp,
}

/// Remove D and R entries from annotation AP dicts (6.3.3:2).
/// PDF/A requires AP dict to contain only the N (Normal) key.
fn strip_ap_non_normal(doc: &mut Document) {
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let ap_ref = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&id) else {
                continue;
            };
            let is_annot = matches!(
                dict.get(b"Subtype").ok(),
                Some(Object::Name(ref n)) if is_annotation_subtype(n)
            );
            if !is_annot {
                continue;
            }
            match dict.get(b"AP").ok() {
                Some(Object::Dictionary(ap)) => {
                    if ap.has(b"D") || ap.has(b"R") {
                        None // inline — handle below
                    } else {
                        continue;
                    }
                }
                Some(Object::Reference(ap_id)) => Some(*ap_id),
                _ => continue,
            }
        };

        match ap_ref {
            Some(ap_id) => {
                // AP is a reference — clean the referenced dict.
                if let Some(Object::Dictionary(ref mut ap)) = doc.objects.get_mut(&ap_id) {
                    ap.remove(b"D");
                    ap.remove(b"R");
                }
            }
            None => {
                // AP is inline in the annotation dict.
                if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                    if let Ok(Object::Dictionary(ref mut ap)) = dict.get_mut(b"AP") {
                        ap.remove(b"D");
                        ap.remove(b"R");
                    }
                }
            }
        }
    }
}

/// Remove forbidden keys from Form XObjects (6.2.9:1).
///
/// Form XObjects must not contain OPI, Subtype2 (with value PS), or PS keys.
fn fix_form_xobject_keys(doc: &mut Document) {
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let is_form_xobject = {
            let Some(Object::Stream(stream)) = doc.objects.get(&id) else {
                continue;
            };
            matches!(
                stream.dict.get(b"Subtype").ok(),
                Some(Object::Name(ref n)) if n == b"Form"
            )
        };
        if !is_form_xobject {
            continue;
        }
        if let Some(Object::Stream(ref mut stream)) = doc.objects.get_mut(&id) {
            stream.dict.remove(b"OPI");
            stream.dict.remove(b"PS");
            // Remove Subtype2 if value is PS.
            if matches!(
                stream.dict.get(b"Subtype2").ok(),
                Some(Object::Name(ref n)) if n == b"PS"
            ) {
                stream.dict.remove(b"Subtype2");
            }
        }
    }
}

/// Ensure OCG D config has Order array listing all OCGs (6.9 t3).
fn fix_ocg_order(doc: &mut Document) {
    // This is now handled in fix_optional_content — kept for backwards compat.
    let _ = doc;
}

/// Add /Contents to annotations that lack it (6.3.3 t1).
fn fix_annotation_contents(doc: &mut Document) {
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let needs_fix = {
            if let Some(Object::Dictionary(dict)) = doc.objects.get(&id) {
                let is_annot = matches!(
                    dict.get(b"Subtype").ok(),
                    Some(Object::Name(ref n)) if is_annotation_subtype(n) && n != b"Popup"
                );
                if !is_annot {
                    false
                } else if !dict.has(b"Contents") {
                    true // 6.3.3:1 — missing
                } else {
                    // 6.3.3:2 — must be a text string
                    !matches!(dict.get(b"Contents").ok(), Some(Object::String(..)))
                }
            } else {
                false
            }
        };
        if needs_fix {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                dict.set(
                    "Contents",
                    Object::String(Vec::new(), lopdf::StringFormat::Literal),
                );
            }
        }
    }
}

/// Ensure annotation /Contents values are text strings, not other types (6.3.3:2).
fn fix_annotation_contents_type(doc: &mut Document) {
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let needs_fix = {
            if let Some(Object::Dictionary(dict)) = doc.objects.get(&id) {
                let is_annot = matches!(
                    dict.get(b"Subtype").ok(),
                    Some(Object::Name(ref n)) if is_annotation_subtype(n)
                );
                if !is_annot {
                    false
                } else {
                    match dict.get(b"Contents").ok() {
                        Some(Object::String(_, _)) => false, // Already a string — OK.
                        Some(_) => true, // Name, Array, Integer, etc. — needs fix.
                        None => false,   // Missing handled by fix_annotation_contents.
                    }
                }
            } else {
                false
            }
        };
        if needs_fix {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                // Convert whatever value to an empty string.
                dict.set(
                    "Contents",
                    Object::String(Vec::new(), lopdf::StringFormat::Literal),
                );
            }
        }
    }
}

/// Remove /A and /AA keys from Widget annotations (6.4.1:1).
fn fix_widget_actions(doc: &mut Document) {
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let is_widget_with_actions = {
            if let Some(Object::Dictionary(dict)) = doc.objects.get(&id) {
                matches!(
                    dict.get(b"Subtype").ok(),
                    Some(Object::Name(ref n)) if n == b"Widget"
                ) && (dict.has(b"A") || dict.has(b"AA"))
            } else {
                false
            }
        };
        if is_widget_with_actions {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                dict.remove(b"A");
                dict.remove(b"AA");
            }
        }
    }
}

/// Remove /XFA key from AcroForm dictionary (6.4.2:1).
fn remove_xfa_from_acroform(doc: &mut Document) {
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
            af.remove(b"XFA");
        }
    } else if let Some(Object::Dictionary(ref mut catalog)) = doc.objects.get_mut(&catalog_id) {
        if let Ok(Object::Dictionary(ref mut af)) = catalog.get_mut(b"AcroForm") {
            af.remove(b"XFA");
        }
    }
}

/// Fix Widget/Btn annotations: ensure AP/N is a subdictionary (6.3.3:3).
fn fix_widget_btn_appearance(doc: &mut Document) {
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let needs_fix = {
            if let Some(Object::Dictionary(dict)) = doc.objects.get(&id) {
                let is_widget_btn = matches!(
                    dict.get(b"Subtype").ok(),
                    Some(Object::Name(ref n)) if n == b"Widget"
                ) && matches!(
                    dict.get(b"FT").ok(),
                    Some(Object::Name(ref n)) if n == b"Btn"
                );
                if !is_widget_btn {
                    false
                } else {
                    match dict.get(b"AP").ok() {
                        Some(Object::Dictionary(ap)) => match ap.get(b"N").ok() {
                            Some(Object::Dictionary(_) | Object::Reference(_)) => false,
                            Some(_) => true,
                            None => false,
                        },
                        _ => false,
                    }
                }
            } else {
                false
            }
        };
        if needs_fix {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                if let Ok(Object::Dictionary(ref mut ap)) = dict.get_mut(b"AP") {
                    let n_val = ap.get(b"N").ok().cloned();
                    if let Some(val) = n_val {
                        let mut sub = lopdf::Dictionary::new();
                        sub.set("Off", val.clone());
                        sub.set("Yes", val);
                        ap.set("N", Object::Dictionary(sub));
                    }
                }
            }
        }
    }
}

/// Fix font descriptor keys: ensure FontFile type matches font Subtype (6.2.11.4.1:1).
///
/// - Type1 fonts → FontFile
/// - TrueType fonts → FontFile2
/// - CIDFontType0 (CFF) → FontFile3
///
/// If a font descriptor has the wrong FontFile key, rename it to the correct one.
#[allow(dead_code)]
fn fix_font_descriptor_keys(doc: &mut Document) {
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();

    // Collect font subtype → descriptor reference mappings.
    let mut descriptor_subtypes: Vec<(ObjectId, &'static [u8])> = Vec::new();
    for &id in &ids {
        if let Some(Object::Dictionary(dict)) = doc.objects.get(&id) {
            let subtype = match dict.get(b"Subtype").ok() {
                Some(Object::Name(ref n)) => n.as_slice(),
                _ => continue,
            };
            let expected_key: &'static [u8] = match subtype {
                b"Type1" | b"MMType1" => b"FontFile",
                b"TrueType" => b"FontFile2",
                b"CIDFontType0" => b"FontFile3",
                b"CIDFontType2" => b"FontFile2",
                _ => continue,
            };
            // Get the font descriptor reference.
            let desc_id = match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(id)) => *id,
                _ => continue,
            };
            descriptor_subtypes.push((desc_id, expected_key));
        }
    }

    // Fix descriptor keys.
    for (desc_id, expected_key) in descriptor_subtypes {
        let wrong_keys: Vec<Vec<u8>> = {
            let all_keys: &[&[u8]] = &[b"FontFile", b"FontFile2", b"FontFile3"];
            if let Some(Object::Dictionary(desc)) = doc.objects.get(&desc_id) {
                all_keys
                    .iter()
                    .filter(|&&k| k != expected_key && desc.has(k))
                    .map(|k| k.to_vec())
                    .collect()
            } else {
                continue;
            }
        };

        if wrong_keys.is_empty() {
            continue;
        }

        // Move the font file reference from wrong key to expected key.
        if let Some(Object::Dictionary(ref mut desc)) = doc.objects.get_mut(&desc_id) {
            if !desc.has(expected_key) {
                // Take value from the first wrong key.
                if let Some(val) = desc.get(&wrong_keys[0]).ok().cloned() {
                    desc.set(
                        std::str::from_utf8(expected_key).unwrap_or("FontFile2"),
                        val,
                    );
                }
            }
            for wrong_key in &wrong_keys {
                desc.remove(wrong_key);
            }
        }
    }
}

/// Fix simple fonts: ensure LastChar exists and Widths array has correct length (6.2.11.2:5, 6.2.11.2:6).
#[allow(dead_code)]
fn fix_font_lastchar_widths(doc: &mut Document) {
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let fix_info = {
            if let Some(Object::Dictionary(dict)) = doc.objects.get(&id) {
                let subtype = match dict.get(b"Subtype").ok() {
                    Some(Object::Name(ref n)) => n.as_slice(),
                    _ => continue,
                };
                // Only simple fonts (Type1, TrueType, MMType1) need FirstChar/LastChar/Widths.
                if !matches!(subtype, b"Type1" | b"TrueType" | b"MMType1") {
                    continue;
                }
                // Skip if it has no BaseFont (not a real font dict).
                if !dict.has(b"BaseFont") {
                    continue;
                }
                let first_char = match dict.get(b"FirstChar").ok() {
                    Some(Object::Integer(v)) => Some(*v),
                    _ => None,
                };
                let last_char = match dict.get(b"LastChar").ok() {
                    Some(Object::Integer(v)) => Some(*v),
                    _ => None,
                };
                let widths_len = match dict.get(b"Widths").ok() {
                    Some(Object::Array(arr)) => Some(arr.len() as i64),
                    _ => None,
                };
                Some((first_char, last_char, widths_len))
            } else {
                continue;
            }
        };

        if let Some((first_char, last_char, widths_len)) = fix_info {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                let fc = first_char.unwrap_or(0);
                let lc = last_char.unwrap_or(255);

                // Ensure FirstChar exists.
                if first_char.is_none() {
                    dict.set("FirstChar", Object::Integer(fc));
                }
                // Ensure LastChar exists (6.2.11.2:5).
                if last_char.is_none() {
                    dict.set("LastChar", Object::Integer(lc));
                }

                let expected_len = (lc - fc + 1).max(0);

                // Fix Widths array length (6.2.11.2:6).
                match widths_len {
                    Some(wl) if wl != expected_len => {
                        // Get existing widths and pad/truncate.
                        let existing = match dict.get(b"Widths").ok() {
                            Some(Object::Array(arr)) => arr.clone(),
                            _ => vec![],
                        };
                        let mut new_widths = Vec::with_capacity(expected_len as usize);
                        for i in 0..expected_len as usize {
                            if i < existing.len() {
                                new_widths.push(existing[i].clone());
                            } else {
                                // Default to 0 width for missing entries.
                                new_widths.push(Object::Integer(0));
                            }
                        }
                        dict.set("Widths", Object::Array(new_widths));
                    }
                    None if expected_len > 0 => {
                        // No Widths array — create one with default widths.
                        let widths: Vec<Object> =
                            (0..expected_len).map(|_| Object::Integer(0)).collect();
                        dict.set("Widths", Object::Array(widths));
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Ensure XMP metadata stream contains dc:title (6.6.2.3.1:1).
///
/// If the XMP stream exists but lacks dc:title, inserts a minimal dc:title element.
fn ensure_xmp_dc_title(doc: &mut Document) {
    let catalog_id = match get_catalog_id(doc) {
        Some(id) => id,
        None => return,
    };

    let meta_id = {
        if let Some(Object::Dictionary(cat)) = doc.objects.get(&catalog_id) {
            match cat.get(b"Metadata").ok() {
                Some(Object::Reference(id)) => Some(*id),
                _ => None,
            }
        } else {
            None
        }
    };

    let meta_id = match meta_id {
        Some(id) => id,
        None => return, // No XMP stream — nothing to fix here.
    };

    let xmp_bytes = {
        if let Some(Object::Stream(stream)) = doc.objects.get(&meta_id) {
            stream.content.clone()
        } else {
            return;
        }
    };

    let xmp_str = String::from_utf8_lossy(&xmp_bytes);

    // Check if dc:title already exists.
    if xmp_str.contains("dc:title") {
        return;
    }

    // Find insertion point: before closing </rdf:RDF> or </x:xmpmeta>.
    let insertion_target = "</rdf:RDF>";
    let insert_pos = match xmp_str.find(insertion_target) {
        Some(pos) => pos,
        None => return, // Not a recognizable XMP structure.
    };

    // Build a dc:title RDF description block.
    let dc_title_block = concat!(
        "<rdf:Description rdf:about=\"\"\n",
        "  xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n",
        "  <dc:title>\n",
        "    <rdf:Alt>\n",
        "      <rdf:li xml:lang=\"x-default\">Untitled</rdf:li>\n",
        "    </rdf:Alt>\n",
        "  </dc:title>\n",
        "</rdf:Description>\n",
    );

    let mut new_xmp = String::with_capacity(xmp_str.len() + dc_title_block.len());
    new_xmp.push_str(&xmp_str[..insert_pos]);
    new_xmp.push_str(dc_title_block);
    new_xmp.push_str(&xmp_str[insert_pos..]);

    let new_bytes = new_xmp.into_bytes();
    if let Some(Object::Stream(ref mut stream)) = doc.objects.get_mut(&meta_id) {
        stream.set_content(new_bytes);
    }
}

/// Remove /NeedsRendering from catalog (6.4.2:2).
fn remove_needs_rendering(doc: &mut Document) {
    let catalog_id = match get_catalog_id(doc) {
        Some(id) => id,
        None => return,
    };
    if let Some(Object::Dictionary(ref mut catalog)) = doc.objects.get_mut(&catalog_id) {
        catalog.remove(b"NeedsRendering");
    }
}

/// Remove forbidden annotation subtypes: 3D, Sound, Screen, Movie (6.3.1:1).
fn remove_forbidden_annotations(doc: &mut Document) {
    let page_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for page_id in &page_ids {
        let forbidden_refs: Vec<ObjectId> = {
            let annots = if let Some(Object::Dictionary(dict)) = doc.objects.get(page_id) {
                match dict.get(b"Annots").ok() {
                    Some(Object::Array(arr)) => Some(arr.clone()),
                    _ => None,
                }
            } else {
                None
            };
            if let Some(arr) = annots {
                arr.iter()
                    .filter_map(|obj| {
                        if let Object::Reference(id) = obj {
                            if let Some(Object::Dictionary(d)) = doc.objects.get(id) {
                                if matches!(
                                    d.get(b"Subtype").ok(),
                                    Some(Object::Name(ref n)) if matches!(n.as_slice(), b"3D" | b"Sound" | b"Screen" | b"Movie")
                                ) {
                                    return Some(*id);
                                }
                            }
                        }
                        None
                    })
                    .collect()
            } else {
                Vec::new()
            }
        };
        if !forbidden_refs.is_empty() {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(page_id) {
                if let Ok(Object::Array(ref mut arr)) = dict.get_mut(b"Annots") {
                    arr.retain(|obj| {
                        if let Object::Reference(id) = obj {
                            !forbidden_refs.contains(id)
                        } else {
                            true
                        }
                    });
                }
            }
        }
    }
}

/// Fix embedded file specification dicts: ensure F and UF keys (6.8:2).
fn fix_file_spec_keys(doc: &mut Document) {
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let needs_fix = {
            if let Some(Object::Dictionary(dict)) = doc.objects.get(&id) {
                matches!(
                    dict.get(b"Type").ok(),
                    Some(Object::Name(ref n)) if n == b"Filespec"
                ) && (!dict.has(b"F") || !dict.has(b"UF"))
            } else {
                false
            }
        };
        if needs_fix {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                let name = dict
                    .get(b"F")
                    .ok()
                    .or_else(|| dict.get(b"UF").ok())
                    .cloned()
                    .unwrap_or_else(|| {
                        Object::String(b"attachment".to_vec(), lopdf::StringFormat::Literal)
                    });
                if !dict.has(b"F") {
                    dict.set("F", name.clone());
                }
                if !dict.has(b"UF") {
                    dict.set("UF", name);
                }
            }
        }
    }
}

/// Strip /EF key from all file specification dicts (6.8:5).
///
/// Embedded files referenced by EF must be PDF/A compliant.
/// Since we can't validate them, remove EF to ensure compliance.
fn strip_ef_from_file_specs(doc: &mut Document) {
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let needs_strip = {
            if let Some(Object::Dictionary(dict)) = doc.objects.get(&id) {
                matches!(
                    dict.get(b"Type").ok(),
                    Some(Object::Name(ref n)) if n == b"Filespec"
                ) && dict.has(b"EF")
            } else {
                false
            }
        };
        if needs_strip {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                dict.remove(b"EF");
            }
        }
    }
}

/// Remove /AS key from OCG configuration dicts (6.9:4).
fn remove_ocg_as_key(doc: &mut Document) {
    let catalog_id = match get_catalog_id(doc) {
        Some(id) => id,
        None => return,
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
        None => return,
    };

    // Remove AS from /D config
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

    if let Some(d_id) = d_id {
        if let Some(Object::Dictionary(ref mut d)) = doc.objects.get_mut(&d_id) {
            d.remove(b"AS");
        }
    } else if let Some(Object::Dictionary(ref mut ocprops)) = doc.objects.get_mut(&ocprops_id) {
        if let Ok(Object::Dictionary(ref mut d)) = ocprops.get_mut(b"D") {
            d.remove(b"AS");
        }
    }

    // Remove AS from alternate configs in /Configs array
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
                _ => Vec::new(),
            }
        } else {
            Vec::new()
        }
    };
    for cfg_id in config_ids {
        if let Some(Object::Dictionary(ref mut cfg)) = doc.objects.get_mut(&cfg_id) {
            cfg.remove(b"AS");
        }
    }
}

/// Truncate Name objects longer than 127 bytes (6.1.13:4).
fn truncate_long_names(doc: &mut Document) {
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let long_keys: Vec<Vec<u8>> = {
            if let Some(Object::Dictionary(dict)) = doc.objects.get(&id) {
                dict.iter()
                    .filter_map(|(key, val)| {
                        // Check Name values > 127 bytes.
                        if let Object::Name(n) = val {
                            if n.len() > 127 {
                                return Some(key.clone());
                            }
                        }
                        None
                    })
                    .collect()
            } else {
                vec![]
            }
        };
        for key in long_keys {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                if let Ok(Object::Name(ref n)) = dict.get(&key) {
                    let truncated = n[..127].to_vec();
                    let key_str = std::str::from_utf8(&key).unwrap_or("?");
                    dict.set(key_str, Object::Name(truncated));
                }
            }
        }
    }
}

/// Fix soft mask images to use DeviceGray colorspace (6.8:5).
fn fix_soft_mask_colorspace(doc: &mut Document) {
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let needs_fix = {
            if let Some(Object::Stream(stream)) = doc.objects.get(&id) {
                let is_image = matches!(
                    stream.dict.get(b"Subtype").ok(),
                    Some(Object::Name(ref n)) if n == b"Image"
                );
                if !is_image {
                    false
                } else {
                    // Check if this is a soft mask (SMask value of another image,
                    // or has /Matte key which indicates it's a soft mask).
                    let has_matte = stream.dict.has(b"Matte");
                    let cs_not_gray = match stream.dict.get(b"ColorSpace").ok() {
                        Some(Object::Name(ref n)) => n != b"DeviceGray",
                        None => false, // If no colorspace, likely already grayscale.
                        _ => true,
                    };
                    has_matte && cs_not_gray
                }
            } else {
                false
            }
        };
        if needs_fix {
            if let Some(Object::Stream(ref mut stream)) = doc.objects.get_mut(&id) {
                stream
                    .dict
                    .set("ColorSpace", Object::Name(b"DeviceGray".to_vec()));
            }
        }
    }

    // Also check images that are referenced as SMask values.
    let smask_ids: Vec<ObjectId> = {
        let mut ids_out = Vec::new();
        for obj in doc.objects.values() {
            if let Object::Stream(stream) = obj {
                if let Ok(Object::Reference(smask_id)) = stream.dict.get(b"SMask") {
                    ids_out.push(*smask_id);
                }
            }
        }
        ids_out
    };

    for smask_id in smask_ids {
        let needs_fix = {
            if let Some(Object::Stream(stream)) = doc.objects.get(&smask_id) {
                match stream.dict.get(b"ColorSpace").ok() {
                    Some(Object::Name(ref n)) => n != b"DeviceGray",
                    _ => false,
                }
            } else {
                false
            }
        };
        if needs_fix {
            if let Some(Object::Stream(ref mut stream)) = doc.objects.get_mut(&smask_id) {
                stream
                    .dict
                    .set("ColorSpace", Object::Name(b"DeviceGray".to_vec()));
            }
        }
    }
}

/// Remove HalftoneName and TransferFunction from halftone dictionaries (6.2.5 t5, t6).
fn remove_halftone_names(doc: &mut Document) {
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let needs_fix = {
            if let Some(Object::Dictionary(dict)) = doc.objects.get(&id) {
                // Halftone dicts have HalftoneType or Type=Halftone.
                let is_halftone = dict.has(b"HalftoneType")
                    || matches!(
                        dict.get(b"Type").ok(),
                        Some(Object::Name(ref n)) if n == b"Halftone"
                    );
                is_halftone && (dict.has(b"HalftoneName") || dict.has(b"TransferFunction"))
            } else {
                false
            }
        };
        if needs_fix {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                dict.remove(b"HalftoneName");
                dict.remove(b"TransferFunction");
            }
        }
    }
}

/// Strip digital signatures that become invalid after PDF modification (6.4.3).
fn strip_signatures(doc: &mut Document) {
    let catalog_id = match get_catalog_id(doc) {
        Some(id) => id,
        None => return,
    };

    // 1. Remove /Perms from catalog (signature-based permissions).
    if let Some(Object::Dictionary(ref mut catalog)) = doc.objects.get_mut(&catalog_id) {
        catalog.remove(b"Perms");
    }

    // 2. Remove /SigFlags from AcroForm.
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
            af.remove(b"SigFlags");
        }
    }

    // 3. Clear signature values from all Sig fields and Sig value dicts.
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let is_sig_field = {
            if let Some(Object::Dictionary(dict)) = doc.objects.get(&id) {
                matches!(
                    dict.get(b"FT").ok(),
                    Some(Object::Name(ref n)) if n == b"Sig"
                ) && dict.has(b"V")
            } else {
                false
            }
        };
        if is_sig_field {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                dict.remove(b"V");
            }
        }

        // Also clear ByteRange/Contents from any Sig value dicts.
        let is_sig_dict = {
            if let Some(Object::Dictionary(dict)) = doc.objects.get(&id) {
                matches!(
                    dict.get(b"Type").ok(),
                    Some(Object::Name(ref n)) if n == b"Sig"
                ) && (dict.has(b"ByteRange") || dict.has(b"Contents"))
            } else {
                false
            }
        };
        if is_sig_dict {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                dict.remove(b"ByteRange");
                dict.remove(b"Contents");
                dict.remove(b"Filter");
                dict.remove(b"SubFilter");
            }
        }
    }
}

/// Ensure saved PDF bytes have a proper binary comment after the header (6.1.2).
/// Must be called on the bytes AFTER `doc.save_to()`.
pub fn fix_pdf_header(data: &mut Vec<u8>) {
    // PDF/A-2 requires: %PDF-1.n\n%<4+ high bytes>\n
    // where n is 0-7, and no extra bytes between the version and EOL.
    if data.len() >= 9 && &data[0..5] == b"%PDF-" {
        // Fix version 2.x → 1.7
        if data[5] == b'2' {
            data[5] = b'1';
            data[7] = b'7';
        }
        // Ensure version is at least 1.4 for PDF/A-2.
        if data[5] == b'1' && data[7] < b'4' {
            data[7] = b'4';
        }
        // Remove any trailing characters (like spaces) between version and EOL.
        // Header should be exactly "%PDF-1.n" (8 bytes) followed by EOL.
        if data.len() > 8 && data[8] != b'\n' && data[8] != b'\r' {
            // Find the next EOL.
            if let Some(eol_pos) = data[8..].iter().position(|&b| b == b'\n' || b == b'\r') {
                let eol_pos = eol_pos + 8;
                // Remove bytes between position 8 and the EOL.
                data.drain(8..eol_pos);
            }
        }
    }

    // Find the first newline after %PDF header.
    if let Some(pos) = data.iter().position(|&b| b == b'\n') {
        let next = pos + 1;
        // Check if there's already a binary comment with high bytes.
        if next < data.len() && data[next] == b'%' {
            let end = data[next..]
                .iter()
                .position(|&b| b == b'\n' || b == b'\r')
                .unwrap_or(5);
            let has_high = data[next..next + end]
                .iter()
                .filter(|&&b| b >= 0x80)
                .count()
                >= 4;
            if has_high {
                return; // Already has a proper binary comment.
            }
        }
        // Insert binary comment line: %âãÏÓ\n
        let comment = b"\x25\xe2\xe3\xcf\xd3\x0a";
        data.splice(next..next, comment.iter().copied());
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

/// Strip /Metadata from all objects except the document catalog (6.6.2.3.1).
///
/// Embedded XMP metadata in images and other objects often contains
/// non-standard properties (photoshop, exif, camera-raw, pdfx) that
/// violate 6.6.2.3.1 unless proper extension schemas are present.
/// Removing these metadata streams is safe and avoids the violation.
fn strip_non_catalog_metadata(doc: &mut Document) {
    let catalog_id = get_catalog_id(doc);

    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        if catalog_id == Some(id) {
            continue;
        }
        let has_metadata = matches!(
            doc.objects.get(&id),
            Some(Object::Dictionary(d)) if d.has(b"Metadata")
        ) || matches!(
            doc.objects.get(&id),
            Some(Object::Stream(s)) if s.dict.has(b"Metadata")
        );
        if has_metadata {
            match doc.objects.get_mut(&id) {
                Some(Object::Dictionary(ref mut d)) => {
                    d.remove(b"Metadata");
                }
                Some(Object::Stream(ref mut s)) => {
                    s.dict.remove(b"Metadata");
                }
                _ => {}
            }
        }
    }
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

    #[test]
    fn test_fix_cidtogidmap() {
        let mut doc = make_basic_doc();
        let cidfont = dictionary! {
            "Type" => "Font",
            "Subtype" => Object::Name(b"CIDFontType2".to_vec()),
            "BaseFont" => "TestFont",
        };
        doc.add_object(Object::Dictionary(cidfont));
        assert_eq!(fix_cidtogidmap(&mut doc), 1);

        // Second call should be no-op.
        assert_eq!(fix_cidtogidmap(&mut doc), 0);
    }

    #[test]
    fn test_fix_annotation_ap() {
        let mut doc = make_basic_doc();
        // Annotation with AP dict that has /D but no /N.
        let ap = dictionary! {
            "D" => Object::Reference((99, 0).into()),
        };
        let annot = dictionary! {
            "Subtype" => Object::Name(b"Stamp".to_vec()),
            "AP" => Object::Dictionary(ap),
            "F" => Object::Integer(4),
        };
        doc.add_object(Object::Dictionary(annot));
        assert_eq!(fix_annotation_ap(&mut doc), 1);
    }

    #[test]
    fn test_fix_annotation_contents() {
        let mut doc = make_basic_doc();
        let annot = dictionary! {
            "Subtype" => Object::Name(b"Text".to_vec()),
            "F" => Object::Integer(4),
        };
        doc.add_object(Object::Dictionary(annot));
        fix_annotation_contents(&mut doc);
        // All annotations should now have Contents.
        for obj in doc.objects.values() {
            if let Object::Dictionary(dict) = obj {
                if matches!(dict.get(b"Subtype").ok(), Some(Object::Name(ref n)) if n == b"Text") {
                    assert!(dict.has(b"Contents"));
                }
            }
        }
    }

    #[test]
    fn test_fix_annotation_contents_type() {
        let mut doc = make_basic_doc();
        // Annotation with /Contents as Name instead of String.
        let annot = dictionary! {
            "Subtype" => Object::Name(b"Text".to_vec()),
            "Contents" => Object::Name(b"SomeValue".to_vec()),
            "F" => Object::Integer(4),
        };
        doc.add_object(Object::Dictionary(annot));
        fix_annotation_contents_type(&mut doc);
        for obj in doc.objects.values() {
            if let Object::Dictionary(dict) = obj {
                if matches!(dict.get(b"Subtype").ok(), Some(Object::Name(ref n)) if n == b"Text") {
                    assert!(
                        matches!(dict.get(b"Contents").ok(), Some(Object::String(_, _))),
                        "/Contents should be a String"
                    );
                }
            }
        }
    }

    #[test]
    fn test_fix_font_descriptor_keys() {
        let mut doc = make_basic_doc();
        // TrueType font with FontFile (wrong — should be FontFile2).
        let font_stream_id =
            doc.add_object(Object::Stream(Stream::new(dictionary! {}, vec![0u8; 10])));
        let desc = dictionary! {
            "Type" => "FontDescriptor",
            "FontFile" => Object::Reference(font_stream_id),
        };
        let desc_id = doc.add_object(Object::Dictionary(desc));
        let font = dictionary! {
            "Type" => "Font",
            "Subtype" => Object::Name(b"TrueType".to_vec()),
            "FontDescriptor" => Object::Reference(desc_id),
        };
        doc.add_object(Object::Dictionary(font));
        fix_font_descriptor_keys(&mut doc);
        if let Some(Object::Dictionary(d)) = doc.objects.get(&desc_id) {
            assert!(!d.has(b"FontFile"), "FontFile should be removed");
            assert!(d.has(b"FontFile2"), "FontFile2 should be added");
        }
    }

    #[test]
    fn test_ensure_xmp_dc_title() {
        let mut doc = make_basic_doc();
        let catalog_id = get_catalog_id(&doc).unwrap();
        // Create an XMP stream without dc:title.
        let xmp = br#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description rdf:about=""
  xmlns:pdfaid="http://www.aiim.org/pdfa/ns/id/">
  <pdfaid:part>2</pdfaid:part>
  <pdfaid:conformance>B</pdfaid:conformance>
</rdf:Description>
</rdf:RDF>
</x:xmpmeta><?xpacket end="w"?>"#;
        let meta_stream = Stream::new(
            dictionary! {
                "Type" => "Metadata",
                "Subtype" => "XML",
            },
            xmp.to_vec(),
        );
        let meta_id = doc.add_object(Object::Stream(meta_stream));
        if let Some(Object::Dictionary(ref mut cat)) = doc.objects.get_mut(&catalog_id) {
            cat.set("Metadata", Object::Reference(meta_id));
        }
        ensure_xmp_dc_title(&mut doc);
        if let Some(Object::Stream(stream)) = doc.objects.get(&meta_id) {
            let content = String::from_utf8_lossy(&stream.content);
            assert!(
                content.contains("dc:title"),
                "XMP should now contain dc:title"
            );
        }
    }

    #[test]
    fn test_ensure_xmp_dc_title_already_present() {
        let mut doc = make_basic_doc();
        let catalog_id = get_catalog_id(&doc).unwrap();
        let xmp = br#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
<rdf:Description rdf:about=""
  xmlns:dc="http://purl.org/dc/elements/1.1/">
  <dc:title><rdf:Alt><rdf:li xml:lang="x-default">My Title</rdf:li></rdf:Alt></dc:title>
</rdf:Description>
</rdf:RDF>
</x:xmpmeta><?xpacket end="w"?>"#;
        let meta_stream = Stream::new(
            dictionary! {
                "Type" => "Metadata",
                "Subtype" => "XML",
            },
            xmp.to_vec(),
        );
        let meta_id = doc.add_object(Object::Stream(meta_stream));
        if let Some(Object::Dictionary(ref mut cat)) = doc.objects.get_mut(&catalog_id) {
            cat.set("Metadata", Object::Reference(meta_id));
        }
        let original_len = {
            if let Some(Object::Stream(s)) = doc.objects.get(&meta_id) {
                s.content.len()
            } else {
                0
            }
        };
        ensure_xmp_dc_title(&mut doc);
        if let Some(Object::Stream(stream)) = doc.objects.get(&meta_id) {
            assert_eq!(
                stream.content.len(),
                original_len,
                "should not modify XMP when dc:title exists"
            );
        }
    }

    #[test]
    fn test_fix_font_lastchar_widths_missing() {
        let mut doc = make_basic_doc();
        // Type1 font without LastChar or Widths.
        let font = dictionary! {
            "Type" => "Font",
            "Subtype" => Object::Name(b"Type1".to_vec()),
            "BaseFont" => "TestFont",
            "FirstChar" => Object::Integer(32),
        };
        let font_id = doc.add_object(Object::Dictionary(font));
        fix_font_lastchar_widths(&mut doc);
        if let Some(Object::Dictionary(d)) = doc.objects.get(&font_id) {
            assert!(d.has(b"LastChar"), "LastChar should be added");
            assert!(d.has(b"Widths"), "Widths should be added");
            if let Ok(Object::Array(w)) = d.get(b"Widths") {
                let lc = match d.get(b"LastChar").ok() {
                    Some(Object::Integer(v)) => *v,
                    _ => 0,
                };
                assert_eq!(w.len() as i64, lc - 32 + 1, "Widths length should match");
            }
        }
    }

    #[test]
    fn test_fix_font_widths_wrong_length() {
        let mut doc = make_basic_doc();
        // TrueType font with wrong Widths length.
        let font = dictionary! {
            "Type" => "Font",
            "Subtype" => Object::Name(b"TrueType".to_vec()),
            "BaseFont" => "TestFont",
            "FirstChar" => Object::Integer(0),
            "LastChar" => Object::Integer(9),
            "Widths" => Object::Array(vec![Object::Integer(500); 5]), // 5 instead of 10
        };
        let font_id = doc.add_object(Object::Dictionary(font));
        fix_font_lastchar_widths(&mut doc);
        if let Some(Object::Dictionary(d)) = doc.objects.get(&font_id) {
            if let Ok(Object::Array(w)) = d.get(b"Widths") {
                assert_eq!(w.len(), 10, "Widths should be padded to 10");
            }
        }
    }

    #[test]
    fn test_truncate_long_names() {
        let mut doc = make_basic_doc();
        let long_name = vec![b'A'; 200];
        let dict = dictionary! {
            "SomeKey" => Object::Name(long_name),
        };
        let id = doc.add_object(Object::Dictionary(dict));
        truncate_long_names(&mut doc);
        if let Some(Object::Dictionary(d)) = doc.objects.get(&id) {
            if let Ok(Object::Name(n)) = d.get(b"SomeKey") {
                assert!(n.len() <= 127, "Name should be truncated to 127 bytes");
            }
        }
    }

    #[test]
    fn test_truncate_long_names_short_ok() {
        let mut doc = make_basic_doc();
        let dict = dictionary! {
            "Key" => Object::Name(b"ShortName".to_vec()),
        };
        let id = doc.add_object(Object::Dictionary(dict));
        truncate_long_names(&mut doc);
        if let Some(Object::Dictionary(d)) = doc.objects.get(&id) {
            if let Ok(Object::Name(n)) = d.get(b"Key") {
                assert_eq!(n, b"ShortName");
            }
        }
    }

    #[test]
    fn test_fix_soft_mask_colorspace() {
        let mut doc = make_basic_doc();
        // Soft mask image (has /Matte) with non-DeviceGray colorspace.
        let smask_stream = Stream::new(
            dictionary! {
                "Subtype" => Object::Name(b"Image".to_vec()),
                "ColorSpace" => Object::Name(b"DeviceRGB".to_vec()),
                "Matte" => Object::Array(vec![Object::Integer(0)]),
                "Width" => Object::Integer(10),
                "Height" => Object::Integer(10),
                "BitsPerComponent" => Object::Integer(8),
            },
            vec![0u8; 100],
        );
        let smask_id = doc.add_object(Object::Stream(smask_stream));
        fix_soft_mask_colorspace(&mut doc);
        if let Some(Object::Stream(s)) = doc.objects.get(&smask_id) {
            assert_eq!(
                s.dict.get(b"ColorSpace").ok(),
                Some(&Object::Name(b"DeviceGray".to_vec())),
                "Soft mask should be DeviceGray"
            );
        }
    }

    #[test]
    fn test_fix_soft_mask_via_smask_ref() {
        let mut doc = make_basic_doc();
        let smask_stream = Stream::new(
            dictionary! {
                "Subtype" => Object::Name(b"Image".to_vec()),
                "ColorSpace" => Object::Name(b"DeviceRGB".to_vec()),
                "Width" => Object::Integer(10),
                "Height" => Object::Integer(10),
                "BitsPerComponent" => Object::Integer(8),
            },
            vec![0u8; 100],
        );
        let smask_id = doc.add_object(Object::Stream(smask_stream));
        // Parent image referencing the soft mask.
        let parent_stream = Stream::new(
            dictionary! {
                "Subtype" => Object::Name(b"Image".to_vec()),
                "ColorSpace" => Object::Name(b"DeviceRGB".to_vec()),
                "Width" => Object::Integer(10),
                "Height" => Object::Integer(10),
                "BitsPerComponent" => Object::Integer(8),
                "SMask" => Object::Reference(smask_id),
            },
            vec![0u8; 300],
        );
        doc.add_object(Object::Stream(parent_stream));
        fix_soft_mask_colorspace(&mut doc);
        if let Some(Object::Stream(s)) = doc.objects.get(&smask_id) {
            assert_eq!(
                s.dict.get(b"ColorSpace").ok(),
                Some(&Object::Name(b"DeviceGray".to_vec())),
                "SMask referenced image should be DeviceGray"
            );
        }
    }

    #[test]
    fn test_version_fix() {
        let mut doc = make_basic_doc();
        doc.version = "2.0".to_string();
        let _ = cleanup_for_pdfa(&mut doc, false).unwrap();
        assert_eq!(doc.version, "1.7");
    }

    #[test]
    fn test_fix_pdf_header_v2() {
        let mut data = b"%PDF-2.0\ntest".to_vec();
        fix_pdf_header(&mut data);
        assert!(data.starts_with(b"%PDF-1.7"));
    }
}
