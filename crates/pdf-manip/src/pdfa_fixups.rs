//! Supplementary PDF/A compliance fixes.
//!
//! Additional passes that address remaining veraPDF rule failures
//! not fully covered by pdfa_cleanup or pdfa_fonts modules.

use lopdf::{dictionary, Document, Object, ObjectId};

/// Run all supplementary PDF/A fixups.
pub fn run_fixups(doc: &mut Document) -> FixupReport {
    let tt_encoding_diffs_fixed = fix_truetype_encoding_differences(doc);
    let devicen_colorants_fixed = fix_devicen_colorants(doc);
    let forbidden_annots_removed = fix_forbidden_annotations_extra(doc);
    let crypt_filters_removed = fix_crypt_filters(doc);
    let file_spec_ef_stripped = fix_file_spec_ef_extra(doc);
    let content_resources_added = fix_content_stream_resources_extra(doc);
    let stream_lengths_fixed = fix_stream_lengths(doc);
    let cmap_wmode_fixed = fix_cmap_wmode(doc);
    let cidtogidmap_fixed = fix_cidtogidmap_extra(doc);

    FixupReport {
        tt_encoding_diffs_fixed,
        devicen_colorants_fixed,
        forbidden_annots_removed,
        crypt_filters_removed,
        file_spec_ef_stripped,
        content_resources_added,
        stream_lengths_fixed,
        cmap_wmode_fixed,
        cidtogidmap_fixed,
    }
}

/// Report from supplementary fixups.
#[derive(Debug, Clone, Default)]
pub struct FixupReport {
    pub tt_encoding_diffs_fixed: usize,
    pub devicen_colorants_fixed: usize,
    pub forbidden_annots_removed: usize,
    pub crypt_filters_removed: usize,
    pub file_spec_ef_stripped: usize,
    pub content_resources_added: usize,
    pub stream_lengths_fixed: usize,
    pub cmap_wmode_fixed: usize,
    pub cidtogidmap_fixed: usize,
}

// ---------------------------------------------------------------------------
// 6.2.11.6:2 — TrueType encoding Differences validation
// ---------------------------------------------------------------------------
//
// Non-symbolic TrueType fonts must not have Differences entries with glyph
// names outside the Adobe Glyph List. Additionally, if Differences exist,
// the embedded font must contain a (3,1) cmap subtable.
//
// The main fix_truetype_encoding in pdfa_fonts handles most cases but may
// skip fonts that already have a valid BaseEncoding. This pass catches
// remaining Differences arrays with non-AGL names.

fn fix_truetype_encoding_differences(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();

    for font_id in ids {
        let fix_action = analyze_tt_differences(doc, font_id);
        match fix_action {
            TtDiffAction::None => {}
            TtDiffAction::StripDifferences => {
                // Remove Differences, keep BaseEncoding.
                strip_differences_from_encoding(doc, font_id);
                count += 1;
            }
            TtDiffAction::SanitizeDifferences => {
                // Replace non-AGL names with space (not .notdef, which causes 6.2.11.8:1).
                sanitize_differences(doc, font_id);
                count += 1;
            }
        }
    }
    count
}

enum TtDiffAction {
    None,
    StripDifferences,
    SanitizeDifferences,
}

fn analyze_tt_differences(doc: &Document, font_id: ObjectId) -> TtDiffAction {
    let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
        return TtDiffAction::None;
    };

    // Only TrueType simple fonts.
    if get_name_val(dict, b"Subtype").as_deref() != Some("TrueType") {
        return TtDiffAction::None;
    }

    // Skip symbolic fonts.
    if is_symbolic(doc, dict) {
        return TtDiffAction::None;
    }

    // Get encoding — may be inline dict or reference.
    let enc_dict = match dict.get(b"Encoding").ok() {
        Some(Object::Dictionary(d)) => Some(d.clone()),
        Some(Object::Reference(enc_id)) => match doc.objects.get(enc_id) {
            Some(Object::Dictionary(d)) => Some(d.clone()),
            _ => None,
        },
        _ => None,
    };

    let Some(enc_dict) = enc_dict else {
        return TtDiffAction::None;
    };

    // Must have Differences array.
    let differences = match enc_dict.get(b"Differences").ok() {
        Some(Object::Array(arr)) => arr.clone(),
        _ => return TtDiffAction::None,
    };

    // Check if font has (3,1) cmap. If not, Differences are forbidden.
    let fd_id = match dict.get(b"FontDescriptor").ok() {
        Some(Object::Reference(id)) => Some(*id),
        _ => None,
    };

    let has_31_cmap = fd_id
        .and_then(|fid| read_font_data(doc, fid))
        .map(|data| {
            ttf_parser::Face::parse(&data, 0)
                .ok()
                .map(|face| face_has_31_cmap(&face))
                .unwrap_or(false)
        })
        .unwrap_or(false);

    if !has_31_cmap {
        // Font lacks (3,1) cmap — strip all Differences.
        return TtDiffAction::StripDifferences;
    }

    // Font has (3,1) cmap — check if any Differences names are outside AGL.
    let has_non_agl = differences.iter().any(|obj| {
        if let Object::Name(name) = obj {
            let name_str = String::from_utf8_lossy(name);
            !is_agl_name(&name_str)
        } else {
            false
        }
    });

    if has_non_agl {
        TtDiffAction::SanitizeDifferences
    } else {
        TtDiffAction::None
    }
}

fn strip_differences_from_encoding(doc: &mut Document, font_id: ObjectId) {
    let enc_ref = {
        let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
            return;
        };
        match dict.get(b"Encoding").ok() {
            Some(Object::Reference(id)) => Some(*id),
            Some(Object::Dictionary(_)) => None, // inline
            _ => return,
        }
    };

    if let Some(enc_id) = enc_ref {
        // Encoding is a referenced dict — modify it.
        if let Some(Object::Dictionary(ref mut enc)) = doc.objects.get_mut(&enc_id) {
            enc.remove(b"Differences");
        }
    } else {
        // Encoding is inline in the font dict.
        if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&font_id) {
            // Extract BaseEncoding name, replace entire Encoding with just the name.
            let base = {
                if let Ok(Object::Dictionary(enc)) = dict.get(b"Encoding") {
                    get_name_val(enc, b"BaseEncoding")
                } else {
                    None
                }
            };
            if let Some(base_name) = base {
                dict.set("Encoding", Object::Name(base_name.into_bytes()));
            } else {
                // No BaseEncoding — set WinAnsiEncoding as safe default.
                dict.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
            }
        }
    }
}

fn sanitize_differences(doc: &mut Document, font_id: ObjectId) {
    let enc_ref = {
        let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
            return;
        };
        match dict.get(b"Encoding").ok() {
            Some(Object::Reference(id)) => Some(*id),
            Some(Object::Dictionary(_)) => None,
            _ => return,
        }
    };

    // Replace non-AGL glyph names with "space" rather than ".notdef".
    // Using ".notdef" causes veraPDF 6.2.11.8:1 violations when the
    // character code is referenced by text-showing operators.
    let sanitize = |arr: &mut Vec<Object>| {
        for obj in arr.iter_mut() {
            if let Object::Name(ref name) = obj {
                let name_str = String::from_utf8_lossy(name);
                if !is_agl_name(&name_str) {
                    *obj = Object::Name(b"space".to_vec());
                }
            }
        }
    };

    if let Some(enc_id) = enc_ref {
        if let Some(Object::Dictionary(ref mut enc)) = doc.objects.get_mut(&enc_id) {
            if let Ok(Object::Array(ref mut arr)) = enc.get_mut(b"Differences") {
                sanitize(arr);
            }
        }
    } else if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&font_id) {
        if let Ok(Object::Dictionary(ref mut enc)) = dict.get_mut(b"Encoding") {
            if let Ok(Object::Array(ref mut arr)) = enc.get_mut(b"Differences") {
                sanitize(arr);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 6.2.4.4:1 — DeviceN/NChannel spot colour Colorants
// ---------------------------------------------------------------------------
//
// For any spot colour used in a DeviceN or NChannel colour space, an entry
// in the Colorants dictionary shall be present. If the attributes dict is
// missing or lacks a Colorants dict, we create one from the colorant names.

fn fix_devicen_colorants(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();

    for id in ids {
        let fix_info = analyze_devicen_colorants(doc, id);
        if let Some(missing_names) = fix_info {
            if add_colorants_entries(doc, id, &missing_names) {
                count += 1;
            }
        }
    }
    count
}

fn analyze_devicen_colorants(doc: &Document, id: ObjectId) -> Option<Vec<Vec<u8>>> {
    let Some(Object::Array(arr)) = doc.objects.get(&id) else {
        return None;
    };
    if arr.len() < 4 {
        return None;
    }
    let is_devicen = matches!(&arr[0], Object::Name(n) if n == b"DeviceN" || n == b"NChannel");
    if !is_devicen {
        return None;
    }

    // arr[1] is the array of colorant names.
    let names: Vec<Vec<u8>> = match &arr[1] {
        Object::Array(name_arr) => name_arr
            .iter()
            .filter_map(|o| {
                if let Object::Name(n) = o {
                    Some(n.clone())
                } else {
                    None
                }
            })
            .collect(),
        _ => return None,
    };

    // Filter out process color names (not spot colors).
    let process_names: &[&[u8]] = &[
        b"Cyan", b"Magenta", b"Yellow", b"Black", b"Red", b"Green", b"Blue", b"None", b"All",
    ];
    let spot_names: Vec<Vec<u8>> = names
        .into_iter()
        .filter(|n| !process_names.contains(&n.as_slice()))
        .collect();

    if spot_names.is_empty() {
        return None;
    }

    // Check if attributes dict (index 4) has Colorants entries for all spots.
    let attrs_dict = if arr.len() > 4 {
        match &arr[4] {
            Object::Dictionary(d) => Some(d),
            Object::Reference(ref_id) => {
                if let Some(Object::Dictionary(d)) = doc.objects.get(ref_id) {
                    Some(d)
                } else {
                    None
                }
            }
            _ => None,
        }
    } else {
        None
    };

    let colorants = attrs_dict.and_then(|d| match d.get(b"Colorants").ok() {
        Some(Object::Dictionary(cd)) => Some(cd.clone()),
        Some(Object::Reference(ref_id)) => {
            if let Some(Object::Dictionary(cd)) = doc.objects.get(ref_id) {
                Some(cd.clone())
            } else {
                None
            }
        }
        _ => None,
    });

    let missing: Vec<Vec<u8>> = spot_names
        .into_iter()
        .filter(|name| {
            colorants
                .as_ref()
                .map(|cd| !cd.has(name.as_slice()))
                .unwrap_or(true)
        })
        .collect();

    if missing.is_empty() {
        None
    } else {
        Some(missing)
    }
}

fn add_colorants_entries(doc: &mut Document, devicen_id: ObjectId, missing: &[Vec<u8>]) -> bool {
    // Build Separation arrays for each missing colorant.
    // Separation array: [/Separation /name alternateCS tintTransform]
    // We use the DeviceN's own alternateCS and a trivial identity tint transform.

    let alternate_cs = {
        let Some(Object::Array(arr)) = doc.objects.get(&devicen_id) else {
            return false;
        };
        arr.get(2).cloned()
    };
    let Some(alt_cs) = alternate_cs else {
        return false;
    };
    let tint_fn = {
        let Some(Object::Array(arr)) = doc.objects.get(&devicen_id) else {
            return false;
        };
        arr.get(3).cloned()
    };
    let Some(tint) = tint_fn else {
        return false;
    };

    // Build colorants dict entries.
    let mut colorant_entries: Vec<(Vec<u8>, Object)> = Vec::new();
    for name in missing {
        let sep_arr = Object::Array(vec![
            Object::Name(b"Separation".to_vec()),
            Object::Name(name.clone()),
            alt_cs.clone(),
            tint.clone(),
        ]);
        colorant_entries.push((name.clone(), sep_arr));
    }

    // Find or create attributes dict at arr[4].
    let Some(Object::Array(ref arr)) = doc.objects.get(&devicen_id) else {
        return false;
    };
    let attrs_ref = if arr.len() > 4 {
        match &arr[4] {
            Object::Reference(id) => Some(*id),
            _ => None,
        }
    } else {
        None
    };

    if let Some(attrs_id) = attrs_ref {
        // Attributes dict is a reference — update it.
        let colorants_ref = {
            if let Some(Object::Dictionary(d)) = doc.objects.get(&attrs_id) {
                match d.get(b"Colorants").ok() {
                    Some(Object::Reference(id)) => Some(*id),
                    _ => None,
                }
            } else {
                None
            }
        };

        if let Some(col_id) = colorants_ref {
            // Colorants is a referenced dict — add entries.
            if let Some(Object::Dictionary(ref mut cd)) = doc.objects.get_mut(&col_id) {
                for (name, sep) in &colorant_entries {
                    if !cd.has(name.as_slice()) {
                        let key = String::from_utf8_lossy(name).to_string();
                        cd.set(key, sep.clone());
                    }
                }
            }
        } else {
            // Add/create Colorants dict inline in attrs.
            if let Some(Object::Dictionary(ref mut attrs)) = doc.objects.get_mut(&attrs_id) {
                let mut cd = match attrs.get(b"Colorants").ok() {
                    Some(Object::Dictionary(existing)) => existing.clone(),
                    _ => lopdf::Dictionary::new(),
                };
                for (name, sep) in &colorant_entries {
                    let key = String::from_utf8_lossy(name).to_string();
                    cd.set(key, sep.clone());
                }
                attrs.set("Colorants", Object::Dictionary(cd));
            }
        }
    } else {
        // Attributes dict is inline or missing — modify the array directly.
        if let Some(Object::Array(ref mut arr)) = doc.objects.get_mut(&devicen_id) {
            let mut cd = lopdf::Dictionary::new();
            for (name, sep) in &colorant_entries {
                let key = String::from_utf8_lossy(name).to_string();
                cd.set(key, sep.clone());
            }
            let attrs = dictionary! {
                "Colorants" => Object::Dictionary(cd),
            };
            if arr.len() > 4 {
                // Replace existing inline attrs.
                if let Object::Dictionary(ref mut existing) = arr[4] {
                    let mut colorants_dict = match existing.get(b"Colorants").ok() {
                        Some(Object::Dictionary(d)) => d.clone(),
                        _ => lopdf::Dictionary::new(),
                    };
                    for (name, sep) in &colorant_entries {
                        let key = String::from_utf8_lossy(name).to_string();
                        colorants_dict.set(key, sep.clone());
                    }
                    existing.set("Colorants", Object::Dictionary(colorants_dict));
                }
            } else {
                arr.push(Object::Dictionary(attrs));
            }
        }
    }

    true
}

// ---------------------------------------------------------------------------
// 6.3.1:1 — Forbidden annotation types (supplementary pass)
// ---------------------------------------------------------------------------
//
// The main remove_forbidden_annotations in pdfa_cleanup handles most cases.
// This catches annotations referenced from Annots arrays that lack a Subtype,
// or annotations not directly referenced from page Annots arrays.

fn fix_forbidden_annotations_extra(doc: &mut Document) -> usize {
    let mut count = 0;

    // Find all annotation dicts (by Type=Annot) with forbidden or missing subtypes.
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut forbidden_ids: Vec<ObjectId> = Vec::new();

    for id in &ids {
        let is_forbidden = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(id) else {
                continue;
            };
            let has_type_annot = matches!(
                dict.get(b"Type").ok(),
                Some(Object::Name(ref n)) if n == b"Annot"
            );
            if !has_type_annot {
                continue;
            }
            match dict.get(b"Subtype").ok() {
                Some(Object::Name(ref n)) => {
                    matches!(n.as_slice(), b"3D" | b"Sound" | b"Screen" | b"Movie")
                }
                None => {
                    // Annotation without Subtype — forbidden.
                    true
                }
                _ => false,
            }
        };
        if is_forbidden {
            forbidden_ids.push(*id);
        }
    }

    if forbidden_ids.is_empty() {
        return 0;
    }

    // Remove these annotation IDs from all page Annots arrays.
    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    for page_id in &page_ids {
        let has_refs = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(page_id) else {
                continue;
            };
            match dict.get(b"Annots").ok() {
                Some(Object::Array(arr)) => arr.iter().any(|o| {
                    if let Object::Reference(id) = o {
                        forbidden_ids.contains(id)
                    } else {
                        false
                    }
                }),
                _ => false,
            }
        };
        if has_refs {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(page_id) {
                if let Ok(Object::Array(ref mut arr)) = dict.get_mut(b"Annots") {
                    let before = arr.len();
                    arr.retain(|o| {
                        if let Object::Reference(id) = o {
                            !forbidden_ids.contains(id)
                        } else {
                            true
                        }
                    });
                    count += before - arr.len();
                }
            }
        }
    }

    count
}

// ---------------------------------------------------------------------------
// 6.1.7.2:1 — Forbidden stream filters (Crypt, non-standard)
// ---------------------------------------------------------------------------
//
// LZW is handled by reencode_lzw_streams in pdfa_cleanup. This pass
// catches Crypt filters and any non-standard filter names.

fn fix_crypt_filters(doc: &mut Document) -> usize {
    let mut count = 0;
    let standard_filters: &[&[u8]] = &[
        b"ASCIIHexDecode",
        b"ASCII85Decode",
        b"FlateDecode",
        b"RunLengthDecode",
        b"CCITTFaxDecode",
        b"JBIG2Decode",
        b"DCTDecode",
        b"JPXDecode",
        // LZWDecode is standard but forbidden — handled by pdfa_cleanup.
    ];

    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let action = {
            let Some(Object::Stream(stream)) = doc.objects.get(&id) else {
                continue;
            };
            match stream.dict.get(b"Filter").ok() {
                Some(Object::Name(n)) => {
                    if n == b"Crypt"
                        || (n != b"LZWDecode" && !standard_filters.contains(&n.as_slice()))
                    {
                        CryptAction::RemoveSingle
                    } else {
                        CryptAction::None
                    }
                }
                Some(Object::Array(arr)) => {
                    let has_forbidden = arr.iter().any(|o| {
                        if let Object::Name(n) = o {
                            n == b"Crypt"
                                || (n != b"LZWDecode" && !standard_filters.contains(&n.as_slice()))
                        } else {
                            false
                        }
                    });
                    if has_forbidden {
                        CryptAction::FilterArray
                    } else {
                        CryptAction::None
                    }
                }
                _ => CryptAction::None,
            }
        };

        match action {
            CryptAction::None => {}
            CryptAction::RemoveSingle => {
                // Decompress and re-encode as FlateDecode.
                if reencode_stream(doc, id) {
                    count += 1;
                }
            }
            CryptAction::FilterArray => {
                // Remove Crypt and non-standard filters from the array.
                // First try to decompress, then re-encode.
                if reencode_stream(doc, id) {
                    count += 1;
                }
            }
        }
    }
    count
}

enum CryptAction {
    None,
    RemoveSingle,
    FilterArray,
}

fn reencode_stream(doc: &mut Document, id: ObjectId) -> bool {
    let decoded = {
        let Some(Object::Stream(stream)) = doc.objects.get(&id) else {
            return false;
        };
        stream.decompressed_content().ok()
    };
    let Some(raw_data) = decoded else {
        return false;
    };

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
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// 6.8:2, 6.8:5 — Embedded file spec fixes (supplementary)
// ---------------------------------------------------------------------------
//
// pdfa_cleanup checks /Type /Filespec, but file spec dicts may exist without
// an explicit Type. This catches dicts that have /EF but no /Type /Filespec.

fn fix_file_spec_ef_extra(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();

    for id in ids {
        let needs_fix = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&id) else {
                continue;
            };
            // Detect file spec dicts by presence of EF key (regardless of Type).
            if !dict.has(b"EF") {
                continue;
            }
            // Skip if already handled (has /Type /Filespec).
            let has_type = matches!(
                dict.get(b"Type").ok(),
                Some(Object::Name(ref n)) if n == b"Filespec"
            );
            !has_type
        };

        if needs_fix {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                // Strip EF to avoid non-compliant embedded files.
                dict.remove(b"EF");
                // Ensure F and UF keys exist.
                if !dict.has(b"F") && !dict.has(b"UF") {
                    dict.set(
                        "F",
                        Object::String(b"attachment".to_vec(), lopdf::StringFormat::Literal),
                    );
                    dict.set(
                        "UF",
                        Object::String(b"attachment".to_vec(), lopdf::StringFormat::Literal),
                    );
                } else if !dict.has(b"F") {
                    let uf = dict.get(b"UF").ok().cloned().unwrap_or_else(|| {
                        Object::String(b"attachment".to_vec(), lopdf::StringFormat::Literal)
                    });
                    dict.set("F", uf);
                } else if !dict.has(b"UF") {
                    let f = dict.get(b"F").ok().cloned().unwrap_or_else(|| {
                        Object::String(b"attachment".to_vec(), lopdf::StringFormat::Literal)
                    });
                    dict.set("UF", f);
                }
                count += 1;
            }
        }
    }
    count
}

// ---------------------------------------------------------------------------
// 6.2.2:2 — Content stream Resources (supplementary)
// ---------------------------------------------------------------------------
//
// pdfa_cleanup's ensure_page_resources handles pages and Form XObjects.
// This pass catches Pattern streams and other content-bearing streams
// that reference resources but lack a Resources dict.

fn fix_content_stream_resources_extra(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();

    for id in &ids {
        let needs_resources = {
            let Some(Object::Stream(s)) = doc.objects.get(id) else {
                continue;
            };
            // Tiling patterns (Type=Pattern, PatternType=1) must have Resources.
            let is_tiling = matches!(s.dict.get(b"PatternType").ok(), Some(Object::Integer(1)))
                && matches!(
                    s.dict.get(b"Type").ok(),
                    Some(Object::Name(ref n)) if n == b"Pattern"
                );
            is_tiling && !s.dict.has(b"Resources")
        };

        if needs_resources {
            if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(id) {
                s.dict
                    .set("Resources", Object::Dictionary(lopdf::Dictionary::new()));
                count += 1;
            }
        }
    }

    // Phase 2: For Form XObjects that reference fonts in their content stream but
    // don't have those fonts in their Resources, find the missing font refs from
    // any other Form XObject or page that has them, and add them.
    count += propagate_missing_font_resources(doc);

    count
}

/// Find Form XObjects that use font resources (e.g. /F1) in their content but
/// don't declare them in their Resources/Font dict. Fix by finding the font
/// reference from another object that has it and adding it.
fn propagate_missing_font_resources(doc: &mut Document) -> usize {
    use std::collections::{HashMap, HashSet};

    // Step 1: Build a global map of font name → object reference from all Resources/Font dicts.
    let mut global_fonts: HashMap<Vec<u8>, Object> = HashMap::new();
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();

    for id in &ids {
        let font_dict = match doc.objects.get(id) {
            Some(Object::Stream(s)) => get_font_dict_from_resources(&s.dict, doc),
            Some(Object::Dictionary(d)) => get_font_dict_from_resources(d, doc),
            _ => None,
        };
        if let Some(fd) = font_dict {
            for (name, val) in fd.iter() {
                if !global_fonts.contains_key(name) {
                    global_fonts.insert(name.clone(), val.clone());
                }
            }
        }
    }

    if global_fonts.is_empty() {
        return 0;
    }

    // Step 2: For each Form XObject, check if content references fonts not in Resources.
    let mut count = 0;
    for id in &ids {
        let missing_fonts = {
            let Some(Object::Stream(s)) = doc.objects.get(id) else {
                continue;
            };
            let is_form = matches!(
                s.dict.get(b"Subtype").ok(),
                Some(Object::Name(ref n)) if n == b"Form"
            );
            if !is_form {
                continue;
            }
            // Parse content to find font references (/Fn where n is a name).
            let content = s.decompressed_content().ok().unwrap_or_else(|| {
                if s.content.is_empty() {
                    vec![]
                } else {
                    s.content.clone()
                }
            });
            let used_fonts = extract_font_names_from_content(&content);
            if used_fonts.is_empty() {
                continue;
            }
            // Check which are missing from Resources/Font.
            let existing_fonts: HashSet<Vec<u8>> = get_font_dict_from_resources(&s.dict, doc)
                .map(|fd| fd.iter().map(|(k, _)| k.clone()).collect())
                .unwrap_or_default();

            let mut missing = Vec::new();
            for fname in used_fonts {
                if !existing_fonts.contains(&fname) {
                    if let Some(ref_obj) = global_fonts.get(&fname) {
                        missing.push((fname, ref_obj.clone()));
                    }
                }
            }
            missing
        };

        if missing_fonts.is_empty() {
            continue;
        }

        // Add missing fonts to the Form XObject's Resources/Font dict.
        if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(id) {
            // Ensure Resources dict exists.
            if !s.dict.has(b"Resources") {
                s.dict
                    .set("Resources", Object::Dictionary(lopdf::Dictionary::new()));
            }
            if let Ok(Object::Dictionary(ref mut resources)) = s.dict.get_mut(b"Resources") {
                // Ensure Font dict exists.
                if !resources.has(b"Font") {
                    resources.set("Font", Object::Dictionary(lopdf::Dictionary::new()));
                }
                if let Ok(Object::Dictionary(ref mut font_dict)) = resources.get_mut(b"Font") {
                    for (name, obj) in missing_fonts {
                        let key_str = String::from_utf8_lossy(&name).to_string();
                        font_dict.set(key_str, obj);
                    }
                    count += 1;
                }
            }
        }
    }
    count
}

/// Get the Font dictionary from a Resources dictionary (which may be inline or referenced).
fn get_font_dict_from_resources(
    dict: &lopdf::Dictionary,
    doc: &Document,
) -> Option<lopdf::Dictionary> {
    let resources = match dict.get(b"Resources").ok() {
        Some(Object::Dictionary(d)) => Some(d.clone()),
        Some(Object::Reference(ref_id)) => match doc.objects.get(ref_id) {
            Some(Object::Dictionary(d)) => Some(d.clone()),
            _ => None,
        },
        _ => None,
    };
    let resources = resources?;
    match resources.get(b"Font").ok() {
        Some(Object::Dictionary(fd)) => Some(fd.clone()),
        Some(Object::Reference(ref_id)) => match doc.objects.get(ref_id) {
            Some(Object::Dictionary(fd)) => Some(fd.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// Extract font resource names (e.g. "F1", "F2") referenced in content stream bytes.
fn extract_font_names_from_content(content: &[u8]) -> Vec<Vec<u8>> {
    let mut names = Vec::new();
    let text = String::from_utf8_lossy(content);
    // Look for /Fn Tf patterns (font selection in content streams).
    let mut i = 0;
    let bytes = text.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'/' {
            // Read the name
            let start = i + 1;
            let mut end = start;
            while end < bytes.len()
                && !matches!(
                    bytes[end],
                    b' ' | b'\t' | b'\n' | b'\r' | b'/' | b'[' | b']' | b'(' | b')' | b'<' | b'>'
                )
            {
                end += 1;
            }
            let name = &bytes[start..end];
            // Check if followed by a number and "Tf" (font selection operator).
            let rest = &text[end..];
            let trimmed = rest.trim_start();
            // Pattern: /FontName <size> Tf
            if trimmed
                .split_whitespace()
                .take(3)
                .collect::<Vec<_>>()
                .last()
                .copied()
                == Some("Tf")
            {
                let name_vec = name.to_vec();
                if !names.contains(&name_vec) {
                    names.push(name_vec);
                }
            }
            i = end;
        } else {
            i += 1;
        }
    }
    names
}

// ---------------------------------------------------------------------------
// 6.1.7.1:1 — Stream Length must match actual bytes
// ---------------------------------------------------------------------------
//
// Some PDFs have corrupted Length keys (e.g. "Qength" instead of "Length")
// or missing Length keys entirely. After lopdf loads and re-saves, the Length
// is recalculated, but corrupted keys may be preserved as-is.
// This pass ensures all streams have a correct /Length key.

fn fix_stream_lengths(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let Some(Object::Stream(stream)) = doc.objects.get(&id) else {
            continue;
        };
        let actual_len = stream.content.len() as i64;
        let has_length = stream.dict.has(b"Length");
        let length_correct = match stream.dict.get(b"Length").ok() {
            Some(Object::Integer(l)) => *l == actual_len,
            _ => false,
        };
        // Also check for corrupted key names that look like Length.
        let has_corrupted_length = stream.dict.iter().any(|(k, _)| {
            k.as_slice() != b"Length" && k.len() >= 4 && k.len() <= 8 && {
                // Heuristic: key is similar to "Length" (e.g. "Qength", "Lngth").
                let lower: Vec<u8> = k.iter().map(|c| c.to_ascii_lowercase()).collect();
                lower.contains(&b'e')
                    && lower.contains(&b'n')
                    && lower.contains(&b'g')
                    && lower.contains(&b't')
                    && lower.contains(&b'h')
            }
        });

        if !has_length || !length_correct || has_corrupted_length {
            if let Some(Object::Stream(ref mut stream)) = doc.objects.get_mut(&id) {
                let actual = stream.content.len() as i64;
                stream.dict.set("Length", Object::Integer(actual));
                // Remove corrupted length-like keys.
                let corrupt_keys: Vec<Vec<u8>> = stream
                    .dict
                    .iter()
                    .filter(|(k, _)| {
                        k.as_slice() != b"Length" && k.len() >= 4 && k.len() <= 8 && {
                            let lower: Vec<u8> = k.iter().map(|c| c.to_ascii_lowercase()).collect();
                            lower.contains(&b'e')
                                && lower.contains(&b'n')
                                && lower.contains(&b'g')
                                && lower.contains(&b't')
                                && lower.contains(&b'h')
                        }
                    })
                    .map(|(k, _)| k.clone())
                    .collect();
                for key in corrupt_keys {
                    stream.dict.remove(key.as_slice());
                }
                count += 1;
            }
        }
    }
    count
}

// ---------------------------------------------------------------------------
// 6.2.11.3.3:2 — CMap WMode mismatch
// ---------------------------------------------------------------------------
//
// The WMode entry in the CMap dictionary must match the WMode value in the
// embedded CMap stream. If they differ, we update the dictionary entry to
// match the stream value.

fn fix_cmap_wmode(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();

    for id in ids {
        let fix_info = {
            let Some(Object::Stream(stream)) = doc.objects.get(&id) else {
                continue;
            };
            // Must be a CMap stream (Type = CMap or has CMapName or UseCMap).
            let is_cmap = matches!(
                stream.dict.get(b"Type").ok(),
                Some(Object::Name(ref n)) if n == b"CMap"
            ) || stream.dict.has(b"CMapName")
                || stream.dict.has(b"UseCMap");
            if !is_cmap {
                continue;
            }
            let dict_wmode = match stream.dict.get(b"WMode").ok() {
                Some(Object::Integer(w)) => Some(*w),
                _ => None,
            };
            // Parse the stream content to find WMode in the CMap program.
            let stream_wmode = extract_cmap_wmode(stream);
            match (dict_wmode, stream_wmode) {
                (Some(dw), Some(sw)) if dw != sw => Some(sw),
                (None, Some(sw)) if sw != 0 => Some(sw), // dict absent defaults to 0
                _ => None,
            }
        };
        if let Some(correct_wmode) = fix_info {
            if let Some(Object::Stream(ref mut stream)) = doc.objects.get_mut(&id) {
                stream.dict.set("WMode", Object::Integer(correct_wmode));
                count += 1;
            }
        }
    }
    count
}

/// Extract WMode value from CMap stream content.
fn extract_cmap_wmode(stream: &lopdf::Stream) -> Option<i64> {
    let content = stream.decompressed_content().ok().unwrap_or_else(|| {
        if stream.content.is_empty() {
            vec![]
        } else {
            stream.content.clone()
        }
    });
    let text = String::from_utf8_lossy(&content);
    // Look for /WMode <value> def patterns in CMap programs.
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.contains("/WMode") {
            // Pattern: /WMode 1 def  or  /WMode 0 def
            if let Some(pos) = trimmed.find("/WMode") {
                let after = &trimmed[pos + 6..];
                for part in after.split_whitespace() {
                    if let Ok(v) = part.parse::<i64>() {
                        return Some(v);
                    }
                    if part == "def" {
                        break;
                    }
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// 6.2.11.3.2:1 — CIDToGIDMap for Type 2 CIDFonts (supplementary)
// ---------------------------------------------------------------------------
//
// pdfa_cleanup::fix_cidtogidmap handles direct CIDFontType2 dicts, but some
// CIDFonts are referenced via a Type 0 font's DescendantFonts array as
// indirect references, and the CIDFont dict may lack the explicit Subtype.
// This supplementary pass catches those cases.

fn fix_cidtogidmap_extra(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();

    for id in ids {
        let needs_fix = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&id) else {
                continue;
            };
            // Type 0 fonts have DescendantFonts referencing CIDFont dicts.
            let is_type0 = matches!(
                dict.get(b"Subtype").ok(),
                Some(Object::Name(ref n)) if n == b"Type0"
            );
            if !is_type0 {
                continue;
            }
            // Check DescendantFonts array.
            let desc_refs: Vec<ObjectId> = match dict.get(b"DescendantFonts").ok() {
                Some(Object::Array(arr)) => arr
                    .iter()
                    .filter_map(|o| {
                        if let Object::Reference(r) = o {
                            Some(*r)
                        } else {
                            None
                        }
                    })
                    .collect(),
                _ => vec![],
            };
            let mut needs = Vec::new();
            for desc_id in desc_refs {
                let Some(Object::Dictionary(cidfont)) = doc.objects.get(&desc_id) else {
                    continue;
                };
                let is_cid2 = matches!(
                    cidfont.get(b"Subtype").ok(),
                    Some(Object::Name(ref n)) if n == b"CIDFontType2"
                );
                if is_cid2 && !cidfont.has(b"CIDToGIDMap") {
                    // Also verify it has an embedded font program (FontDescriptor with FontFile2).
                    let has_embedded = match cidfont.get(b"FontDescriptor").ok() {
                        Some(Object::Reference(fd_id)) => {
                            matches!(
                                doc.objects.get(fd_id),
                                Some(Object::Dictionary(fd)) if fd.has(b"FontFile2")
                            )
                        }
                        _ => false,
                    };
                    if has_embedded {
                        needs.push(desc_id);
                    }
                }
            }
            needs
        };
        for desc_id in needs_fix {
            if let Some(Object::Dictionary(ref mut cidfont)) = doc.objects.get_mut(&desc_id) {
                cidfont.set("CIDToGIDMap", Object::Name(b"Identity".to_vec()));
                count += 1;
            }
        }
    }
    count
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn get_name_val(dict: &lopdf::Dictionary, key: &[u8]) -> Option<String> {
    match dict.get(key).ok()? {
        Object::Name(n) => String::from_utf8(n.clone()).ok(),
        _ => None,
    }
}

fn is_symbolic(doc: &Document, font_dict: &lopdf::Dictionary) -> bool {
    let fd = match font_dict.get(b"FontDescriptor") {
        Ok(Object::Reference(id)) => doc.get_object(*id).ok(),
        Ok(obj) => Some(obj),
        _ => None,
    };
    if let Some(Object::Dictionary(fd_dict)) = fd {
        if let Ok(Object::Integer(flags)) = fd_dict.get(b"Flags") {
            let symbolic = (*flags & 4) != 0;
            let nonsymbolic = (*flags & 32) != 0;
            if nonsymbolic {
                return false;
            }
            if symbolic {
                return true;
            }
        }
    }
    false
}

fn read_font_data(doc: &Document, fd_id: ObjectId) -> Option<Vec<u8>> {
    let fd = match doc.objects.get(&fd_id) {
        Some(Object::Dictionary(d)) => d,
        _ => return None,
    };

    // Try FontFile2 (TrueType), FontFile (Type1), FontFile3 (CFF/OpenType).
    let stream_id = fd
        .get(b"FontFile2")
        .ok()
        .or_else(|| fd.get(b"FontFile").ok())
        .or_else(|| fd.get(b"FontFile3").ok())
        .and_then(|obj| {
            if let Object::Reference(id) = obj {
                Some(*id)
            } else {
                None
            }
        })?;

    let stream = match doc.objects.get(&stream_id) {
        Some(Object::Stream(s)) => s,
        _ => return None,
    };

    stream.decompressed_content().ok().or_else(|| {
        if stream.content.is_empty() {
            None
        } else {
            Some(stream.content.clone())
        }
    })
}

fn face_has_31_cmap(face: &ttf_parser::Face) -> bool {
    let Some(cmap) = face.tables().cmap.as_ref() else {
        return false;
    };
    for st in cmap.subtables.into_iter() {
        if st.platform_id == ttf_parser::PlatformId::Windows && st.encoding_id == 1 {
            return true;
        }
    }
    false
}

/// Check if a glyph name is in the Adobe Glyph List (AGL).
///
/// Includes the full AGL plus common PDF standard names.
fn is_agl_name(name: &str) -> bool {
    // .notdef is always valid.
    if name == ".notdef" || name == ".null" || name == "nonmarkingreturn" {
        return true;
    }

    // Names of the form "uniXXXX" or "uXXXXX" are valid AGL names.
    if name.starts_with("uni") && name.len() >= 7 {
        let hex = &name[3..];
        if hex.len().is_multiple_of(4) && hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return true;
        }
    }
    if name.starts_with('u') && name.len() >= 5 && name.len() <= 7 {
        let hex = &name[1..];
        if hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return true;
        }
    }

    // Check against the core AGL set (all 600+ standard names).
    AGL_NAMES.binary_search(&name).is_ok()
}

/// Adobe Glyph List names (sorted for binary search).
/// Source: https://github.com/adobe-type-tools/agl-aglfn/blob/master/aglfn.txt
static AGL_NAMES: &[&str] = &[
    ".notdef",
    "A",
    "AE",
    "AEacute",
    "Aacute",
    "Abreve",
    "Acircumflex",
    "Adieresis",
    "Agrave",
    "Alpha",
    "Alphatonos",
    "Amacron",
    "Aogonek",
    "Aring",
    "Aringacute",
    "Atilde",
    "B",
    "Beta",
    "C",
    "Cacute",
    "Ccaron",
    "Ccedilla",
    "Ccircumflex",
    "Cdotaccent",
    "Chi",
    "D",
    "Dcaron",
    "Dcroat",
    "Delta",
    "E",
    "Eacute",
    "Ebreve",
    "Ecaron",
    "Ecircumflex",
    "Edieresis",
    "Edotaccent",
    "Egrave",
    "Emacron",
    "Eng",
    "Eogonek",
    "Epsilon",
    "Epsilontonos",
    "Eta",
    "Etatonos",
    "Eth",
    "Euro",
    "F",
    "G",
    "Gamma",
    "Gbreve",
    "Gcircumflex",
    "Gcommaaccent",
    "Gdotaccent",
    "H",
    "Hbar",
    "Hcircumflex",
    "I",
    "IJ",
    "Iacute",
    "Ibreve",
    "Icircumflex",
    "Idieresis",
    "Idotaccent",
    "Igrave",
    "Imacron",
    "Iogonek",
    "Iota",
    "Iotadieresis",
    "Iotatonos",
    "Itilde",
    "J",
    "Jcircumflex",
    "K",
    "Kappa",
    "Kcommaaccent",
    "L",
    "Lacute",
    "Lambda",
    "Lcaron",
    "Lcommaaccent",
    "Ldot",
    "Lslash",
    "M",
    "Mu",
    "N",
    "Nacute",
    "Ncaron",
    "Ncommaaccent",
    "Ntilde",
    "Nu",
    "O",
    "OE",
    "Oacute",
    "Obreve",
    "Ocircumflex",
    "Odieresis",
    "Ograve",
    "Ohorn",
    "Ohungarumlaut",
    "Omacron",
    "Omega",
    "Omegatonos",
    "Omicron",
    "Omicrontonos",
    "Oslash",
    "Oslashacute",
    "Otilde",
    "P",
    "Phi",
    "Pi",
    "Psi",
    "Q",
    "R",
    "Racute",
    "Rcaron",
    "Rcommaaccent",
    "Rho",
    "S",
    "Sacute",
    "Scaron",
    "Scedilla",
    "Scircumflex",
    "Scommaaccent",
    "Sigma",
    "T",
    "Tau",
    "Tbar",
    "Tcaron",
    "Tcommaaccent",
    "Theta",
    "Thorn",
    "U",
    "Uacute",
    "Ubreve",
    "Ucircumflex",
    "Udieresis",
    "Ugrave",
    "Uhorn",
    "Uhungarumlaut",
    "Umacron",
    "Uogonek",
    "Upsilon",
    "Upsilon1",
    "Upsilondieresis",
    "Upsilontonos",
    "Uring",
    "Utilde",
    "V",
    "W",
    "Wacute",
    "Wcircumflex",
    "Wdieresis",
    "Wgrave",
    "X",
    "Xi",
    "Y",
    "Yacute",
    "Ycircumflex",
    "Ydieresis",
    "Ygrave",
    "Z",
    "Zacute",
    "Zcaron",
    "Zdotaccent",
    "Zeta",
    "a",
    "aacute",
    "abreve",
    "acircumflex",
    "acute",
    "acutecomb",
    "adieresis",
    "ae",
    "aeacute",
    "afii00208",
    "afii10017",
    "afii10018",
    "afii10019",
    "afii10020",
    "afii10021",
    "afii10022",
    "afii10023",
    "afii10024",
    "afii10025",
    "afii10026",
    "afii10027",
    "afii10028",
    "afii10029",
    "afii10030",
    "afii10031",
    "afii10032",
    "afii10033",
    "afii10034",
    "afii10035",
    "afii10036",
    "afii10037",
    "afii10038",
    "afii10039",
    "afii10040",
    "afii10041",
    "afii10042",
    "afii10043",
    "afii10044",
    "afii10045",
    "afii10046",
    "afii10047",
    "afii10048",
    "afii10049",
    "afii10050",
    "afii10051",
    "afii10052",
    "afii10053",
    "afii10054",
    "afii10055",
    "afii10056",
    "afii10057",
    "afii10058",
    "afii10059",
    "afii10060",
    "afii10061",
    "afii10062",
    "afii10063",
    "afii10064",
    "afii10065",
    "afii10066",
    "afii10067",
    "afii10068",
    "afii10069",
    "afii10070",
    "afii10071",
    "afii10072",
    "afii10073",
    "afii10074",
    "afii10075",
    "afii10076",
    "afii10077",
    "afii10078",
    "afii10079",
    "afii10080",
    "afii10081",
    "afii10082",
    "afii10083",
    "afii10084",
    "afii10085",
    "afii10086",
    "afii10087",
    "afii10088",
    "afii10089",
    "afii10090",
    "afii10091",
    "afii10092",
    "afii10093",
    "afii10094",
    "afii10095",
    "afii10096",
    "afii10097",
    "afii10098",
    "afii10099",
    "afii10100",
    "afii10101",
    "afii10102",
    "afii10103",
    "afii10104",
    "afii10105",
    "afii10106",
    "afii10107",
    "afii10108",
    "afii10109",
    "afii10110",
    "afii10145",
    "afii10146",
    "afii10147",
    "afii10148",
    "afii10192",
    "afii10193",
    "afii10194",
    "afii10195",
    "afii10196",
    "afii10831",
    "afii10832",
    "afii57381",
    "afii57388",
    "afii57392",
    "afii57393",
    "afii57394",
    "afii57395",
    "afii57396",
    "afii57397",
    "afii57398",
    "afii57399",
    "afii57400",
    "afii57401",
    "afii57403",
    "afii57407",
    "afii57409",
    "afii57410",
    "afii57411",
    "afii57412",
    "afii57413",
    "afii57414",
    "afii57415",
    "afii57416",
    "afii57417",
    "afii57418",
    "afii57419",
    "afii57420",
    "afii57421",
    "afii57422",
    "afii57423",
    "afii57424",
    "afii57425",
    "afii57426",
    "afii57427",
    "afii57428",
    "afii57429",
    "afii57430",
    "afii57431",
    "afii57432",
    "afii57433",
    "afii57434",
    "afii57440",
    "afii57441",
    "afii57442",
    "afii57443",
    "afii57444",
    "afii57445",
    "afii57446",
    "afii57448",
    "afii57449",
    "afii57450",
    "afii57451",
    "afii57452",
    "afii57453",
    "afii57454",
    "afii57455",
    "afii57456",
    "afii57457",
    "afii57458",
    "afii57470",
    "afii57505",
    "afii57506",
    "afii57507",
    "afii57508",
    "afii57509",
    "afii57511",
    "afii57512",
    "afii57513",
    "afii57514",
    "afii57519",
    "afii57534",
    "afii57636",
    "afii57645",
    "afii57658",
    "afii57664",
    "afii57665",
    "afii57666",
    "afii57667",
    "afii57668",
    "afii57669",
    "afii57670",
    "afii57671",
    "afii57672",
    "afii57673",
    "afii57674",
    "afii57675",
    "afii57676",
    "afii57677",
    "afii57678",
    "afii57679",
    "afii57680",
    "afii57681",
    "afii57682",
    "afii57683",
    "afii57684",
    "afii57685",
    "afii57686",
    "afii57687",
    "afii57688",
    "afii57689",
    "afii57690",
    "afii57694",
    "afii57695",
    "afii57700",
    "afii57705",
    "afii57716",
    "afii57717",
    "afii57718",
    "afii57723",
    "afii57793",
    "afii57794",
    "afii57795",
    "afii57796",
    "afii57797",
    "afii57798",
    "afii57799",
    "afii57800",
    "afii57801",
    "afii57802",
    "afii57803",
    "afii57804",
    "afii57806",
    "afii57807",
    "afii57839",
    "afii57841",
    "afii57842",
    "afii57929",
    "afii61248",
    "afii61289",
    "afii61352",
    "afii61573",
    "afii61574",
    "afii61575",
    "afii61664",
    "afii63167",
    "afii64937",
    "agrave",
    "alpha",
    "alphatonos",
    "amacron",
    "ampersand",
    "angle",
    "angleleft",
    "angleright",
    "anoteleia",
    "aogonek",
    "approxequal",
    "aring",
    "aringacute",
    "arrowboth",
    "arrowdblboth",
    "arrowdbldown",
    "arrowdblleft",
    "arrowdblright",
    "arrowdblup",
    "arrowdown",
    "arrowleft",
    "arrowright",
    "arrowup",
    "arrowupdn",
    "arrowupdnbse",
    "asciicircum",
    "asciitilde",
    "asterisk",
    "asteriskmath",
    "at",
    "atilde",
    "b",
    "backslash",
    "bar",
    "beta",
    "block",
    "braceleft",
    "braceright",
    "bracketleft",
    "bracketright",
    "breve",
    "brokenbar",
    "bullet",
    "c",
    "cacute",
    "caron",
    "carriagereturn",
    "ccaron",
    "ccedilla",
    "ccircumflex",
    "cdotaccent",
    "cedilla",
    "cent",
    "chi",
    "circle",
    "circumflex",
    "club",
    "colon",
    "colonmonetary",
    "comma",
    "commaaccent",
    "congruent",
    "copyright",
    "currency",
    "d",
    "dagger",
    "daggerdbl",
    "dcaron",
    "dcroat",
    "degree",
    "delta",
    "diamond",
    "dieresis",
    "dieresistonos",
    "divide",
    "dkshade",
    "dnblock",
    "dollar",
    "dong",
    "dotaccent",
    "dotbelowcomb",
    "dotlessi",
    "dotmath",
    "e",
    "eacute",
    "ebreve",
    "ecaron",
    "ecircumflex",
    "edieresis",
    "edotaccent",
    "egrave",
    "eight",
    "element",
    "ellipsis",
    "emacron",
    "emdash",
    "emptyset",
    "endash",
    "eng",
    "eogonek",
    "epsilon",
    "epsilontonos",
    "equal",
    "equivalence",
    "estimated",
    "eta",
    "etatonos",
    "eth",
    "exclam",
    "exclamdbl",
    "exclamdown",
    "existential",
    "f",
    "female",
    "fi",
    "figuredash",
    "filledbox",
    "filledrect",
    "five",
    "fiveeighths",
    "fl",
    "florin",
    "four",
    "fraction",
    "franc",
    "g",
    "gamma",
    "gbreve",
    "gcircumflex",
    "gcommaaccent",
    "gdotaccent",
    "germandbls",
    "gradient",
    "grave",
    "gravecomb",
    "greaterequal",
    "guillemotleft",
    "guillemotright",
    "guilsinglleft",
    "guilsinglright",
    "h",
    "hbar",
    "hcircumflex",
    "heart",
    "hookabovecomb",
    "house",
    "hungarumlaut",
    "hyphen",
    "i",
    "iacute",
    "ibreve",
    "icircumflex",
    "idieresis",
    "igrave",
    "ij",
    "imacron",
    "infinity",
    "integral",
    "integralbt",
    "integralex",
    "integraltp",
    "intersection",
    "invbullet",
    "invcircle",
    "invsmileface",
    "iogonek",
    "iota",
    "iotadieresis",
    "iotadieresistonos",
    "iotatonos",
    "itilde",
    "j",
    "jcircumflex",
    "k",
    "kappa",
    "kcommaaccent",
    "kgreenlandic",
    "l",
    "lacute",
    "lambda",
    "lcaron",
    "lcommaaccent",
    "ldot",
    "less",
    "lessequal",
    "lfblock",
    "lira",
    "logicaland",
    "logicalnot",
    "logicalor",
    "longs",
    "lozenge",
    "lslash",
    "m",
    "macron",
    "male",
    "minus",
    "minute",
    "mu",
    "multiply",
    "musicalnote",
    "musicalnotedbl",
    "n",
    "nacute",
    "napostrophe",
    "nbspace",
    "ncaron",
    "ncommaaccent",
    "nine",
    "notelement",
    "notequal",
    "notsubset",
    "ntilde",
    "nu",
    "numbersign",
    "o",
    "oacute",
    "obreve",
    "ocircumflex",
    "odieresis",
    "oe",
    "ogonek",
    "ograve",
    "ohorn",
    "ohungarumlaut",
    "omacron",
    "omega",
    "omega1",
    "omegatonos",
    "omicron",
    "omicrontonos",
    "one",
    "onedotenleader",
    "oneeighth",
    "onehalf",
    "onequarter",
    "onesuperior",
    "onethird",
    "openbullet",
    "ordfeminine",
    "ordmasculine",
    "orthogonal",
    "oslash",
    "oslashacute",
    "otilde",
    "overline",
    "p",
    "paragraph",
    "parenleft",
    "parenright",
    "partialdiff",
    "percent",
    "period",
    "periodcentered",
    "perpendicular",
    "perthousand",
    "peseta",
    "phi",
    "phi1",
    "pi",
    "plus",
    "plusminus",
    "prescription",
    "product",
    "propersubset",
    "propersuperset",
    "proportional",
    "psi",
    "q",
    "question",
    "questiondown",
    "quotedbl",
    "quotedblbase",
    "quotedblleft",
    "quotedblright",
    "quoteleft",
    "quotereversed",
    "quoteright",
    "quotesinglbase",
    "quotesingle",
    "r",
    "racute",
    "radical",
    "rcaron",
    "rcommaaccent",
    "reflexsubset",
    "reflexsuperset",
    "registered",
    "revlogicalnot",
    "rho",
    "ring",
    "rtblock",
    "s",
    "sacute",
    "scaron",
    "scedilla",
    "scircumflex",
    "scommaaccent",
    "second",
    "section",
    "semicolon",
    "seven",
    "seveneighths",
    "sfthyphen",
    "shade",
    "sigma",
    "sigma1",
    "similar",
    "six",
    "slash",
    "smileface",
    "space",
    "spade",
    "sterling",
    "suchthat",
    "summation",
    "sun",
    "t",
    "tau",
    "tbar",
    "tcaron",
    "tcommaaccent",
    "therefore",
    "theta",
    "theta1",
    "thorn",
    "three",
    "threeeighths",
    "threequarters",
    "threequartersemdash",
    "threesuperior",
    "tilde",
    "tildecomb",
    "tonos",
    "trademark",
    "triagdn",
    "triaglf",
    "triagrt",
    "triagup",
    "two",
    "twodotenleader",
    "twosuperior",
    "twothirds",
    "u",
    "uacute",
    "ubreve",
    "ucircumflex",
    "udieresis",
    "ugrave",
    "uhorn",
    "uhungarumlaut",
    "umacron",
    "underscore",
    "underscoredbl",
    "union",
    "universal",
    "uogonek",
    "upblock",
    "upsilon",
    "upsilondieresis",
    "upsilondieresistonos",
    "upsilontonos",
    "uring",
    "utilde",
    "v",
    "w",
    "wacute",
    "wcircumflex",
    "wdieresis",
    "wgrave",
    "wpgrave",
    "x",
    "xi",
    "y",
    "yacute",
    "ycircumflex",
    "ydieresis",
    "yen",
    "ygrave",
    "z",
    "zacute",
    "zcaron",
    "zdotaccent",
    "zero",
    "zeta",
];
