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
    let cmap_wmode_fixed = fix_cmap_wmode(doc);
    let cidtogidmap_fixed = fix_cidtogidmap_extra(doc);
    let cidsysteminfo_fixed = fix_cidsysteminfo_mismatch(doc);
    let cmap_embedded = embed_nonstandard_cmaps(doc);
    let opi_keys_removed = fix_opi_keys(doc);
    let stream_f_keys_removed = fix_stream_f_keys(doc);
    let postscript_xobjects_removed = fix_postscript_xobjects(doc);
    let reference_xobjects_removed = fix_reference_xobjects(doc);
    let overflow_integers_fixed = fix_overflow_integers(doc);
    let long_strings_fixed = fix_long_strings(doc);
    let jpx_colorspace_fixed = fix_jpx_forbidden_colorspaces(doc);
    // Content stream modifications (decompress/recompress) must run before
    // fix_stream_lengths to ensure Length values are correct.
    let operator_spacing_fixed = fix_content_stream_operator_spacing(doc);
    let tiny_floats_fixed = fix_tiny_floats_in_streams(doc);
    let odd_hex_strings_fixed = fix_odd_hex_strings_in_streams(doc);
    let inline_image_interpolate_fixed = fix_inline_image_interpolate(doc);
    let concatenated_operators_fixed = fix_concatenated_operators(doc);
    // fix_stream_lengths must be LAST — after all other fixes that may modify streams.
    let stream_lengths_fixed = fix_stream_lengths(doc);

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
        cidsysteminfo_fixed,
        cmap_embedded,
        opi_keys_removed,
        stream_f_keys_removed,
        postscript_xobjects_removed,
        reference_xobjects_removed,
        overflow_integers_fixed,
        long_strings_fixed,
        operator_spacing_fixed,
        tiny_floats_fixed,
        odd_hex_strings_fixed,
        inline_image_interpolate_fixed,
        jpx_colorspace_fixed,
        concatenated_operators_fixed,
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
    pub cidsysteminfo_fixed: usize,
    pub cmap_embedded: usize,
    pub opi_keys_removed: usize,
    pub stream_f_keys_removed: usize,
    pub postscript_xobjects_removed: usize,
    pub reference_xobjects_removed: usize,
    pub overflow_integers_fixed: usize,
    pub long_strings_fixed: usize,
    pub operator_spacing_fixed: usize,
    pub tiny_floats_fixed: usize,
    pub odd_hex_strings_fixed: usize,
    pub inline_image_interpolate_fixed: usize,
    pub jpx_colorspace_fixed: usize,
    pub concatenated_operators_fixed: usize,
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

    // Must have Differences array (may be inline or referenced).
    let differences = match enc_dict.get(b"Differences").ok() {
        Some(Object::Array(arr)) => arr.clone(),
        Some(Object::Reference(ref_id)) => match doc.objects.get(ref_id) {
            Some(Object::Array(arr)) => arr.clone(),
            _ => return TtDiffAction::None,
        },
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
        // Encoding is a referenced dict.
        // Check if Differences is inline or also a reference.
        let diff_ref = {
            if let Some(Object::Dictionary(enc)) = doc.objects.get(&enc_id) {
                match enc.get(b"Differences").ok() {
                    Some(Object::Reference(id)) => Some(*id),
                    _ => None,
                }
            } else {
                None
            }
        };
        if let Some(diff_id) = diff_ref {
            // Differences is also a referenced array — modify it directly.
            if let Some(Object::Array(ref mut arr)) = doc.objects.get_mut(&diff_id) {
                sanitize(arr);
            }
        } else if let Some(Object::Dictionary(ref mut enc)) = doc.objects.get_mut(&enc_id) {
            if let Ok(Object::Array(ref mut arr)) = enc.get_mut(b"Differences") {
                sanitize(arr);
            }
        }
    } else if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&font_id) {
        if let Ok(Object::Dictionary(ref mut enc)) = dict.get_mut(b"Encoding") {
            // Check if Differences is a reference.
            let diff_ref = match enc.get(b"Differences").ok() {
                Some(Object::Reference(id)) => Some(*id),
                _ => None,
            };
            if let Some(diff_id) = diff_ref {
                if let Some(Object::Array(ref mut arr)) = doc.objects.get_mut(&diff_id) {
                    sanitize(arr);
                }
            } else if let Ok(Object::Array(ref mut arr)) = enc.get_mut(b"Differences") {
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

    // Phase 3: Type3 fonts — charstring procedures may reference page-level
    // resources not declared in the Type3 font's Resources dict (6.2.2:2).
    count += fix_type3_font_resources(doc);

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

/// Fix Type3 font Resources: Type3 fonts without an explicit Resources dict
/// cause veraPDF to flag all page-level resources as "inherited" by the
/// charstring content streams (6.2.2:2). Copy the parent page's Resources
/// to the Type3 font so charstrings have explicitly associated resources.
fn fix_type3_font_resources(doc: &mut Document) -> usize {
    use std::collections::HashMap;

    // Step 1: Find Type3 font IDs without Resources.
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut type3_ids_needing_fix: Vec<ObjectId> = Vec::new();
    for id in &ids {
        let Some(Object::Dictionary(dict)) = doc.objects.get(id) else {
            continue;
        };
        let is_type3 = matches!(
            dict.get(b"Subtype").ok(),
            Some(Object::Name(ref n)) if n == b"Type3"
        );
        if is_type3 && !dict.has(b"Resources") {
            type3_ids_needing_fix.push(*id);
        }
    }

    if type3_ids_needing_fix.is_empty() {
        return 0;
    }

    // Step 2: Build map of Type3 font ID → page Resources by scanning pages.
    let mut font_to_resources: HashMap<ObjectId, Object> = HashMap::new();
    for id in &ids {
        let page_resources = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(id) else {
                continue;
            };
            if dict.get(b"Type").ok().and_then(|o| o.as_name().ok()) != Some(b"Page") {
                continue;
            }
            dict.get(b"Resources").ok().cloned()
        };
        let Some(resources_obj) = page_resources else {
            continue;
        };

        // Get the Font dictionary from this page's Resources.
        let font_dict = match &resources_obj {
            Object::Dictionary(res) => res.get(b"Font").ok().cloned(),
            Object::Reference(res_id) => {
                if let Some(Object::Dictionary(res)) = doc.objects.get(res_id) {
                    res.get(b"Font").ok().cloned()
                } else {
                    None
                }
            }
            _ => None,
        };
        let Some(font_dict_obj) = font_dict else {
            continue;
        };
        let font_entries: Vec<(Vec<u8>, ObjectId)> = match &font_dict_obj {
            Object::Dictionary(fd) => fd
                .iter()
                .filter_map(|(k, v)| {
                    if let Object::Reference(r) = v {
                        Some((k.clone(), *r))
                    } else {
                        None
                    }
                })
                .collect(),
            Object::Reference(fd_id) => {
                if let Some(Object::Dictionary(fd)) = doc.objects.get(fd_id) {
                    fd.iter()
                        .filter_map(|(k, v)| {
                            if let Object::Reference(r) = v {
                                Some((k.clone(), *r))
                            } else {
                                None
                            }
                        })
                        .collect()
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        };

        // For each Type3 font used on this page, record the page's Resources.
        for (_name, font_id) in &font_entries {
            if type3_ids_needing_fix.contains(font_id) {
                font_to_resources.insert(*font_id, resources_obj.clone());
            }
        }
    }

    // Step 3: Apply page Resources to Type3 fonts.
    let mut count = 0;
    for type3_id in &type3_ids_needing_fix {
        let resources = font_to_resources.get(type3_id).cloned().unwrap_or_else(|| {
            // Fallback: minimal Resources.
            let mut r = lopdf::Dictionary::new();
            r.set(
                "ProcSet",
                Object::Array(vec![
                    Object::Name(b"PDF".to_vec()),
                    Object::Name(b"ImageB".to_vec()),
                ]),
            );
            Object::Dictionary(r)
        });

        if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(type3_id) {
            dict.set("Resources", resources);
            count += 1;
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
            let tokens: Vec<&str> = trimmed.split_whitespace().take(2).collect();
            if tokens.len() == 2 && tokens[1] == "Tf" {
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
    // .notdef is always valid as a glyph name.
    // .null and nonmarkingreturn are valid glyph names but map to U+0000 and
    // U+000D respectively, which veraPDF considers non-Unicode-compliant in
    // TrueType Differences arrays. Exclude them so they get sanitized.
    if name == ".notdef" {
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

// ---------------------------------------------------------------------------
// 6.2.11.3.1:1, 6.2.11.3.3:1 — CIDSystemInfo mismatch
// ---------------------------------------------------------------------------
//
// For Type0 fonts with a CMap encoding, the CIDSystemInfo in the CMap must
// match the CIDSystemInfo in the CIDFont descendant. The Registry and
// Ordering strings must be identical. If they differ, we update the CIDFont's
// CIDSystemInfo to match the CMap's values.
//
// Additionally, the CMap's CIDSystemInfo must be compatible with the embedded
// CMap stream (if any). We also handle the case where a predefined CMap name
// (like "UniGB-UTF16-H") implies specific Registry/Ordering values.

fn fix_cidsysteminfo_mismatch(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();

    for id in &ids {
        let (encoding_obj, cid_font_id) = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(id) else {
                continue;
            };
            let subtype = get_name_val(dict, b"Subtype");
            if subtype.as_deref() != Some("Type0") {
                continue;
            }
            let enc = dict.get(b"Encoding").ok().cloned();
            let cid_id = match dict.get(b"DescendantFonts").ok() {
                Some(Object::Array(arr)) => match arr.first() {
                    Some(Object::Reference(r)) => Some(*r),
                    _ => None,
                },
                _ => None,
            };
            let Some(cid_id) = cid_id else { continue };
            (enc, cid_id)
        };

        let Some(encoding_obj) = encoding_obj else {
            continue;
        };

        // Get the CMap's CIDSystemInfo (from the CMap stream or predefined name).
        let cmap_csi = match &encoding_obj {
            Object::Name(name) => {
                // Predefined CMap name — derive CIDSystemInfo from name.
                let name_str = String::from_utf8_lossy(name);
                predefined_cmap_cidsysteminfo(&name_str)
            }
            Object::Reference(cmap_id) => {
                // CMap stream — extract CIDSystemInfo from the stream content.
                extract_cmap_stream_cidsysteminfo(doc, *cmap_id)
            }
            _ => None,
        };

        let Some((cmap_registry, cmap_ordering)) = cmap_csi else {
            continue;
        };

        // Get the CIDFont's CIDSystemInfo.
        let cidfont_csi = {
            let Some(Object::Dictionary(cid_dict)) = doc.objects.get(&cid_font_id) else {
                continue;
            };
            match cid_dict.get(b"CIDSystemInfo").ok() {
                Some(Object::Dictionary(csi)) => {
                    let reg = match csi.get(b"Registry").ok() {
                        Some(Object::String(s, _)) => String::from_utf8_lossy(s).to_string(),
                        _ => String::new(),
                    };
                    let ord = match csi.get(b"Ordering").ok() {
                        Some(Object::String(s, _)) => String::from_utf8_lossy(s).to_string(),
                        _ => String::new(),
                    };
                    Some((reg, ord))
                }
                Some(Object::Reference(csi_id)) => {
                    if let Some(Object::Dictionary(csi)) = doc.objects.get(csi_id) {
                        let reg = match csi.get(b"Registry").ok() {
                            Some(Object::String(s, _)) => String::from_utf8_lossy(s).to_string(),
                            _ => String::new(),
                        };
                        let ord = match csi.get(b"Ordering").ok() {
                            Some(Object::String(s, _)) => String::from_utf8_lossy(s).to_string(),
                            _ => String::new(),
                        };
                        Some((reg, ord))
                    } else {
                        None
                    }
                }
                _ => None,
            }
        };

        let Some((cidfont_reg, cidfont_ord)) = cidfont_csi else {
            continue;
        };

        // Check if they match.
        if cmap_registry == cidfont_reg && cmap_ordering == cidfont_ord {
            // Registry/Ordering match — check Supplement (CIDFont ≤ CMap).
            let cmap_supplement = get_cmap_supplement(doc, &encoding_obj);
            if let Some(cmap_sup) = cmap_supplement {
                let cidfont_sup = get_cidfont_supplement(doc, cid_font_id);
                if cidfont_sup > cmap_sup {
                    set_cidfont_supplement(doc, cid_font_id, cmap_sup);
                    count += 1;
                }
            }
            continue;
        }

        // Fix: update the CIDFont's CIDSystemInfo to match the CMap's values.
        let csi_ref = {
            let Some(Object::Dictionary(cid_dict)) = doc.objects.get(&cid_font_id) else {
                continue;
            };
            match cid_dict.get(b"CIDSystemInfo").ok() {
                Some(Object::Reference(r)) => Some(*r),
                _ => None,
            }
        };

        if let Some(csi_id) = csi_ref {
            // CIDSystemInfo is a reference — update the referenced dict.
            if let Some(Object::Dictionary(ref mut csi)) = doc.objects.get_mut(&csi_id) {
                csi.set(
                    "Registry",
                    Object::String(
                        cmap_registry.as_bytes().to_vec(),
                        lopdf::StringFormat::Literal,
                    ),
                );
                csi.set(
                    "Ordering",
                    Object::String(
                        cmap_ordering.as_bytes().to_vec(),
                        lopdf::StringFormat::Literal,
                    ),
                );
                count += 1;
            }
        } else {
            // CIDSystemInfo is inline — replace it.
            if let Some(Object::Dictionary(ref mut cid_dict)) = doc.objects.get_mut(&cid_font_id) {
                let new_csi = dictionary! {
                    "Registry" => Object::String(cmap_registry.as_bytes().to_vec(), lopdf::StringFormat::Literal),
                    "Ordering" => Object::String(cmap_ordering.as_bytes().to_vec(), lopdf::StringFormat::Literal),
                    "Supplement" => Object::Integer(0),
                };
                cid_dict.set("CIDSystemInfo", Object::Dictionary(new_csi));
                count += 1;
            }
        }
    }

    count
}

/// Extract CIDSystemInfo from a CMap stream's content.
fn extract_cmap_stream_cidsysteminfo(
    doc: &Document,
    cmap_id: ObjectId,
) -> Option<(String, String)> {
    let stream = match doc.objects.get(&cmap_id) {
        Some(Object::Stream(s)) => s,
        _ => return None,
    };

    let content = stream
        .decompressed_content()
        .ok()
        .unwrap_or_else(|| stream.content.clone());
    let text = String::from_utf8_lossy(&content);

    // Look for /CIDSystemInfo << /Registry (...) /Ordering (...) >> def
    let registry = extract_cmap_string_value(&text, "Registry");
    let ordering = extract_cmap_string_value(&text, "Ordering");

    match (registry, ordering) {
        (Some(r), Some(o)) => Some((r, o)),
        _ => None,
    }
}

/// Extract a string value from a CMap program's CIDSystemInfo dict.
fn extract_cmap_string_value(text: &str, key: &str) -> Option<String> {
    let key_pattern = format!("/{key}");
    let pos = text.find(&key_pattern)?;
    let after = &text[pos + key_pattern.len()..];

    // Look for (value) — PostScript literal string.
    let paren_start = after.find('(')?;
    let paren_end = after[paren_start..].find(')')?;
    let value = &after[paren_start + 1..paren_start + paren_end];
    Some(value.to_string())
}

/// Extract Supplement from a CMap (predefined name or stream).
fn get_cmap_supplement(doc: &Document, encoding_obj: &Object) -> Option<i64> {
    match encoding_obj {
        Object::Name(_) => {
            // Predefined CMap — Supplement is encoded in the name for some.
            // We can't reliably extract it; skip.
            None
        }
        Object::Reference(cmap_id) => {
            // CMap stream — look for /Supplement in the CIDSystemInfo dict on the stream.
            let stream = match doc.objects.get(cmap_id) {
                Some(Object::Stream(s)) => s,
                _ => return None,
            };
            // First check the stream dictionary.
            if let Ok(Object::Dictionary(csi)) = stream.dict.get(b"CIDSystemInfo") {
                if let Ok(Object::Integer(sup)) = csi.get(b"Supplement") {
                    return Some(*sup);
                }
            }
            // Then try parsing the CMap program text.
            let content = stream
                .decompressed_content()
                .ok()
                .unwrap_or_else(|| stream.content.clone());
            let text = String::from_utf8_lossy(&content);
            extract_cmap_int_value(&text, "Supplement")
        }
        _ => None,
    }
}

/// Extract the Supplement value from a CIDFont's CIDSystemInfo.
fn get_cidfont_supplement(doc: &Document, cid_font_id: ObjectId) -> i64 {
    let Some(Object::Dictionary(cid_dict)) = doc.objects.get(&cid_font_id) else {
        return 0;
    };
    match cid_dict.get(b"CIDSystemInfo").ok() {
        Some(Object::Dictionary(csi)) => match csi.get(b"Supplement").ok() {
            Some(Object::Integer(s)) => *s,
            _ => 0,
        },
        Some(Object::Reference(csi_id)) => {
            if let Some(Object::Dictionary(csi)) = doc.objects.get(csi_id) {
                match csi.get(b"Supplement").ok() {
                    Some(Object::Integer(s)) => *s,
                    _ => 0,
                }
            } else {
                0
            }
        }
        _ => 0,
    }
}

/// Update the CIDFont's CIDSystemInfo Supplement value.
fn set_cidfont_supplement(doc: &mut Document, cid_font_id: ObjectId, supplement: i64) {
    let csi_ref = {
        let Some(Object::Dictionary(cid_dict)) = doc.objects.get(&cid_font_id) else {
            return;
        };
        match cid_dict.get(b"CIDSystemInfo").ok() {
            Some(Object::Reference(r)) => Some(*r),
            _ => None,
        }
    };
    if let Some(csi_id) = csi_ref {
        if let Some(Object::Dictionary(ref mut csi)) = doc.objects.get_mut(&csi_id) {
            csi.set("Supplement", Object::Integer(supplement));
        }
    } else if let Some(Object::Dictionary(ref mut cid_dict)) = doc.objects.get_mut(&cid_font_id) {
        if let Ok(Object::Dictionary(ref mut csi)) = cid_dict.get_mut(b"CIDSystemInfo") {
            csi.set("Supplement", Object::Integer(supplement));
        }
    }
}

/// Extract an integer value from a CMap program's CIDSystemInfo dict.
fn extract_cmap_int_value(text: &str, key: &str) -> Option<i64> {
    let key_pattern = format!("/{key}");
    let pos = text.find(&key_pattern)?;
    let after = &text[pos + key_pattern.len()..].trim_start();
    // Parse integer from the text.
    let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// Get CIDSystemInfo (Registry, Ordering) for predefined CMap names.
fn predefined_cmap_cidsysteminfo(cmap_name: &str) -> Option<(String, String)> {
    // Identity CMaps.
    if cmap_name.starts_with("Identity") {
        return Some(("Adobe".to_string(), "Identity".to_string()));
    }

    // Adobe standard CMaps.
    if cmap_name.contains("Japan") || cmap_name.starts_with("90") {
        return Some(("Adobe".to_string(), "Japan1".to_string()));
    }
    if cmap_name.contains("Korea") || cmap_name.starts_with("KS") {
        return Some(("Adobe".to_string(), "Korea1".to_string()));
    }
    if cmap_name.contains("GB") || cmap_name.starts_with("GBK") {
        return Some(("Adobe".to_string(), "GB1".to_string()));
    }
    if cmap_name.contains("CNS") || cmap_name.contains("B5") {
        return Some(("Adobe".to_string(), "CNS1".to_string()));
    }
    if cmap_name.contains("UCS") || cmap_name.contains("UTF") {
        // Adobe-UCS CMaps have "Identity" ordering in practice.
        return Some(("Adobe".to_string(), "Identity".to_string()));
    }

    None
}

// ---------------------------------------------------------------------------
// 6.2.11.3.3 — Embed non-standard CMap files
// ---------------------------------------------------------------------------
//
// All CMaps used in a PDF/A file must either be one of the predefined CMaps
// from ISO 32000-1 Table 118, or be embedded as a stream.
//
// This pass finds Type0 fonts that reference non-standard CMaps by name and
// embeds the CMap file as a stream object.

/// Predefined CMap names from ISO 32000-1 Table 118 that do not require embedding.
const PREDEFINED_CMAPS: &[&str] = &[
    "Identity-H",
    "Identity-V",
    // Japanese
    "83pv-RKSJ-H",
    "90ms-RKSJ-H",
    "90ms-RKSJ-V",
    "90msp-RKSJ-H",
    "90msp-RKSJ-V",
    "90pv-RKSJ-H",
    "Add-RKSJ-H",
    "Add-RKSJ-V",
    "EUC-H",
    "EUC-V",
    "Ext-RKSJ-H",
    "Ext-RKSJ-V",
    "H",
    "V",
    "UniJIS-UCS2-H",
    "UniJIS-UCS2-V",
    "UniJIS-UCS2-HW-H",
    "UniJIS-UCS2-HW-V",
    "UniJIS-UTF16-H",
    "UniJIS-UTF16-V",
    // Korean
    "KSC-EUC-H",
    "KSC-EUC-V",
    "KSCms-UHC-H",
    "KSCms-UHC-V",
    "KSCms-UHC-HW-H",
    "KSCms-UHC-HW-V",
    "KSCpc-EUC-H",
    "UniKS-UCS2-H",
    "UniKS-UCS2-V",
    "UniKS-UTF16-H",
    "UniKS-UTF16-V",
    // Simplified Chinese
    "GB-EUC-H",
    "GB-EUC-V",
    "GBpc-EUC-H",
    "GBpc-EUC-V",
    "GBK-EUC-H",
    "GBK-EUC-V",
    "GBKp-EUC-H",
    "GBKp-EUC-V",
    "GBK2K-H",
    "GBK2K-V",
    "UniGB-UCS2-H",
    "UniGB-UCS2-V",
    "UniGB-UTF16-H",
    "UniGB-UTF16-V",
    // Traditional Chinese
    "B5pc-H",
    "B5pc-V",
    "HKscs-B5-H",
    "HKscs-B5-V",
    "ETen-B5-H",
    "ETen-B5-V",
    "ETenms-B5-H",
    "ETenms-B5-V",
    "CNS-EUC-H",
    "CNS-EUC-V",
    "UniCNS-UCS2-H",
    "UniCNS-UCS2-V",
    "UniCNS-UTF16-H",
    "UniCNS-UTF16-V",
];

/// Directories where CMap files may be found.
const CMAP_SEARCH_DIRS: &[&str] = &[
    "/usr/share/poppler/cMap",
    "/usr/share/fonts/cmap",
    "/usr/share/fonts/cMap",
    "/usr/share/ghostscript/cMap",
];

/// Embed non-standard CMap files referenced by Type0 fonts (6.2.11.3.3).
fn embed_nonstandard_cmaps(doc: &mut Document) -> usize {
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();

    // Collect Type0 fonts that use non-standard CMap names.
    let mut to_embed: Vec<(ObjectId, String)> = Vec::new();

    for id in &ids {
        let cmap_name = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(id) else {
                continue;
            };
            let subtype = dict
                .get(b"Subtype")
                .ok()
                .and_then(|o| {
                    if let Object::Name(n) = o {
                        String::from_utf8(n.clone()).ok()
                    } else {
                        None
                    }
                })
                .unwrap_or_default();
            if subtype != "Type0" {
                continue;
            }
            match dict.get(b"Encoding").ok() {
                Some(Object::Name(n)) => String::from_utf8(n.clone()).unwrap_or_default(),
                _ => continue, // Already a stream reference or missing.
            }
        };

        if cmap_name.is_empty() {
            continue;
        }
        if PREDEFINED_CMAPS.contains(&cmap_name.as_str()) {
            continue;
        }

        to_embed.push((*id, cmap_name));
    }

    let mut embedded = 0;

    for (font_id, cmap_name) in to_embed {
        // Try to find the CMap file on disk.
        let cmap_data = find_cmap_file(&cmap_name);
        let Some(cmap_data) = cmap_data else {
            continue;
        };

        // Create a CMap stream object.
        let mut cmap_dict = lopdf::Dictionary::new();
        cmap_dict.set("Type", Object::Name(b"CMap".to_vec()));
        cmap_dict.set("CMapName", Object::Name(cmap_name.as_bytes().to_vec()));

        // Extract CIDSystemInfo from the CMap data if present.
        if let Some((registry, ordering, supplement)) = extract_cmap_cidsysteminfo(&cmap_data) {
            let csi_dict = dictionary! {
                "Registry" => Object::String(registry.into_bytes(), lopdf::StringFormat::Literal),
                "Ordering" => Object::String(ordering.into_bytes(), lopdf::StringFormat::Literal),
                "Supplement" => Object::Integer(supplement),
            };
            cmap_dict.set("CIDSystemInfo", Object::Dictionary(csi_dict));
        }

        let stream = lopdf::Stream::new(cmap_dict, cmap_data);
        let stream_id = doc.add_object(Object::Stream(stream));

        // Replace the name reference with a stream reference.
        if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&font_id) {
            dict.set("Encoding", Object::Reference(stream_id));
        }

        embedded += 1;
    }

    embedded
}

/// Search for a CMap file in standard directories.
fn find_cmap_file(cmap_name: &str) -> Option<Vec<u8>> {
    use std::path::Path;

    for base_dir in CMAP_SEARCH_DIRS {
        // Try direct path: base/CMapName
        let direct = Path::new(base_dir).join(cmap_name);
        if let Ok(data) = std::fs::read(&direct) {
            return Some(data);
        }

        // Try subdirectories (e.g., poppler/cMap/Adobe-Korea1/Adobe-Korea1-2).
        if let Ok(entries) = std::fs::read_dir(base_dir) {
            for entry in entries.flatten() {
                if entry.path().is_dir() {
                    let sub_path = entry.path().join(cmap_name);
                    if let Ok(data) = std::fs::read(&sub_path) {
                        return Some(data);
                    }
                }
            }
        }
    }

    None
}

/// Extract CIDSystemInfo (Registry, Ordering, Supplement) from CMap PostScript data.
fn extract_cmap_cidsysteminfo(data: &[u8]) -> Option<(String, String, i64)> {
    let text = std::str::from_utf8(data).ok()?;

    // Look for /CIDSystemInfo block.
    let csi_pos = text.find("/CIDSystemInfo")?;
    let block = &text[csi_pos..];

    // Find Registry, Ordering, Supplement in the block.
    let registry = extract_cmap_ps_string(block, "/Registry")?;
    let ordering = extract_cmap_ps_string(block, "/Ordering")?;

    let supplement = {
        let sup_pos = block.find("/Supplement")?;
        let after = block[sup_pos + "/Supplement".len()..].trim_start();
        after
            .split_whitespace()
            .next()?
            .trim_end_matches(|c: char| !c.is_ascii_digit())
            .parse::<i64>()
            .ok()?
    };

    Some((registry, ordering, supplement))
}

/// Extract a PostScript string value after a key (e.g., `/Registry (Adobe)`).
fn extract_cmap_ps_string(block: &str, key: &str) -> Option<String> {
    let pos = block.find(key)?;
    let after = &block[pos + key.len()..];
    let paren_start = after.find('(')?;
    let paren_end = after[paren_start + 1..].find(')')?;
    Some(after[paren_start + 1..paren_start + 1 + paren_end].to_string())
}

// ---------------------------------------------------------------------------
// 6.2.8:2 — Remove OPI keys from Image dictionaries.
// ---------------------------------------------------------------------------

fn fix_opi_keys(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let has_opi = matches!(
            doc.objects.get(&id),
            Some(Object::Stream(s)) if s.dict.has(b"OPI")
        );
        if has_opi {
            if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(&id) {
                s.dict.remove(b"OPI");
                count += 1;
            }
        }
    }
    count
}

// ---------------------------------------------------------------------------
// 6.1.7.1:3 — Remove F, FFilter, FDecodeParms from stream dictionaries.
// ---------------------------------------------------------------------------

fn fix_stream_f_keys(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let has_f = matches!(
            doc.objects.get(&id),
            Some(Object::Stream(s)) if s.dict.has(b"F") || s.dict.has(b"FFilter") || s.dict.has(b"FDecodeParms")
        );
        if has_f {
            if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(&id) {
                s.dict.remove(b"F");
                s.dict.remove(b"FFilter");
                s.dict.remove(b"FDecodeParms");
                count += 1;
            }
        }
    }
    count
}

// ---------------------------------------------------------------------------
// 6.2.9:3 — Remove PostScript XObjects.
// ---------------------------------------------------------------------------

fn fix_postscript_xobjects(doc: &mut Document) -> usize {
    // Collect IDs of PostScript XObjects (Type=XObject, Subtype=PS).
    let ps_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(&id, obj)| {
            if let Object::Stream(s) = obj {
                let is_xobj =
                    s.dict.get(b"Type").ok().and_then(|o| o.as_name().ok()) == Some(b"XObject");
                let is_ps =
                    s.dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) == Some(b"PS");
                if is_xobj && is_ps {
                    return Some(id);
                }
            }
            None
        })
        .collect();

    if ps_ids.is_empty() {
        return 0;
    }

    let count = ps_ids.len();

    // Remove references from all XObject resource dictionaries.
    let all_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in all_ids {
        let mut refs_to_remove = Vec::new();
        if let Some(Object::Dictionary(dict)) = doc.objects.get(&id) {
            if let Ok(Object::Dictionary(xobjects)) = dict.get(b"XObject") {
                for (key, val) in xobjects.iter() {
                    if let Object::Reference(ref_id) = val {
                        if ps_ids.contains(ref_id) {
                            refs_to_remove.push(key.clone());
                        }
                    }
                }
            }
        }
        if !refs_to_remove.is_empty() {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                if let Ok(Object::Dictionary(ref mut xobjects)) = dict.get_mut(b"XObject") {
                    for key in &refs_to_remove {
                        xobjects.remove(key);
                    }
                }
            }
        }
    }

    // Remove the PS XObject streams themselves.
    for id in &ps_ids {
        doc.objects.remove(id);
    }

    count
}

// ---------------------------------------------------------------------------
// 6.2.9:2 — Remove reference XObjects (Ref key in form XObject dictionaries).
// ---------------------------------------------------------------------------

fn fix_reference_xobjects(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let has_ref = match doc.objects.get(&id) {
            Some(Object::Stream(s)) => {
                let is_form =
                    s.dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) == Some(b"Form");
                is_form && s.dict.has(b"Ref")
            }
            _ => false,
        };
        if has_ref {
            if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(&id) {
                s.dict.remove(b"Ref");
                count += 1;
            }
        }
    }
    count
}

// ---------------------------------------------------------------------------
// 6.1.13:1 — Fix integer overflow (values > 2^31-1 or < -2^31).
// ---------------------------------------------------------------------------

fn fix_overflow_integers(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let obj = match doc.objects.get(&id) {
            Some(o) => o.clone(),
            None => continue,
        };
        let (fixed, n) = fix_overflow_in_object(obj);
        if n > 0 {
            doc.objects.insert(id, fixed);
            count += n;
        }
    }
    count
}

fn fix_overflow_in_object(obj: Object) -> (Object, usize) {
    match obj {
        Object::Integer(v) if v > i64::from(i32::MAX) => (Object::Integer(i64::from(i32::MAX)), 1),
        Object::Integer(v) if v < i64::from(i32::MIN) => (Object::Integer(i64::from(i32::MIN)), 1),
        Object::Array(arr) => {
            let mut total = 0;
            let new_arr: Vec<Object> = arr
                .into_iter()
                .map(|o| {
                    let (fixed, n) = fix_overflow_in_object(o);
                    total += n;
                    fixed
                })
                .collect();
            (Object::Array(new_arr), total)
        }
        Object::Dictionary(dict) => {
            let mut total = 0;
            let mut new_dict = lopdf::Dictionary::new();
            for (key, val) in dict.into_iter() {
                let (fixed, n) = fix_overflow_in_object(val);
                total += n;
                new_dict.set(key, fixed);
            }
            (Object::Dictionary(new_dict), total)
        }
        Object::Stream(mut s) => {
            let mut total = 0;
            let mut new_dict = lopdf::Dictionary::new();
            for (key, val) in s.dict.into_iter() {
                let (fixed, n) = fix_overflow_in_object(val);
                total += n;
                new_dict.set(key, fixed);
            }
            s.dict = new_dict;
            (Object::Stream(s), total)
        }
        other => (other, 0),
    }
}

// ---------------------------------------------------------------------------
// 6.1.13:3 — Truncate strings longer than 32767 bytes.
// ---------------------------------------------------------------------------

fn fix_long_strings(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let obj = match doc.objects.get(&id) {
            Some(o) => o.clone(),
            None => continue,
        };
        let (fixed, n) = fix_long_strings_in_object(obj);
        if n > 0 {
            doc.objects.insert(id, fixed);
            count += n;
        }
    }
    count
}

fn fix_long_strings_in_object(obj: Object) -> (Object, usize) {
    const MAX_STRING_LEN: usize = 32767;
    match obj {
        Object::String(ref s, fmt) if s.len() > MAX_STRING_LEN => {
            let truncated = s[..MAX_STRING_LEN].to_vec();
            (Object::String(truncated, fmt), 1)
        }
        Object::Array(arr) => {
            let mut total = 0;
            let new_arr: Vec<Object> = arr
                .into_iter()
                .map(|o| {
                    let (fixed, n) = fix_long_strings_in_object(o);
                    total += n;
                    fixed
                })
                .collect();
            (Object::Array(new_arr), total)
        }
        Object::Dictionary(dict) => {
            let mut total = 0;
            let mut new_dict = lopdf::Dictionary::new();
            for (key, val) in dict.into_iter() {
                let (fixed, n) = fix_long_strings_in_object(val);
                total += n;
                new_dict.set(key, fixed);
            }
            (Object::Dictionary(new_dict), total)
        }
        Object::Stream(mut s) => {
            let mut total = 0;
            let mut new_dict = lopdf::Dictionary::new();
            for (key, val) in s.dict.into_iter() {
                let (fixed, n) = fix_long_strings_in_object(val);
                total += n;
                new_dict.set(key, fixed);
            }
            s.dict = new_dict;
            (Object::Stream(s), total)
        }
        other => (other, 0),
    }
}

/// Collect IDs of content streams (page Contents, Form XObjects, Tiling Patterns).
fn collect_content_stream_ids(doc: &Document) -> std::collections::HashSet<ObjectId> {
    let mut ids = std::collections::HashSet::new();
    for obj in doc.objects.values() {
        if let Object::Dictionary(dict) = obj {
            if let Ok(Object::Name(t)) = dict.get(b"Type") {
                if t == b"Page" {
                    if let Ok(Object::Reference(cid)) = dict.get(b"Contents") {
                        ids.insert(*cid);
                    }
                    if let Ok(Object::Array(arr)) = dict.get(b"Contents") {
                        for item in arr {
                            if let Object::Reference(cid) = item {
                                ids.insert(*cid);
                            }
                        }
                    }
                }
            }
        }
    }
    for (&id, obj) in doc.objects.iter() {
        if let Object::Stream(s) = obj {
            let is_form =
                s.dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) == Some(b"Form");
            let is_pattern = s
                .dict
                .get(b"PatternType")
                .ok()
                .and_then(|o| o.as_i64().ok())
                == Some(1);
            if is_form || is_pattern {
                ids.insert(id);
            }
        }
    }
    ids
}

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// 6.2.8:3 — Fix Interpolate=true in inline images.
// Inline images use BI <dict> ID <data> EI in content streams.
// Replace /I true or /Interpolate true with /I false within BI...ID blocks.
// ---------------------------------------------------------------------------

fn fix_inline_image_interpolate(doc: &mut Document) -> usize {
    let content_stream_ids = collect_content_stream_ids(doc);
    let mut total_count = 0;

    for id in content_stream_ids {
        let decompressed = if let Some(Object::Stream(s)) = doc.objects.get(&id) {
            match s.decompressed_content() {
                Ok(d) => d,
                Err(_) => s.content.clone(),
            }
        } else {
            continue;
        };

        // Quick check: does this stream have inline images with Interpolate?
        if !decompressed.windows(2).any(|w| w == b"BI") {
            continue;
        }

        let mut new_content = Vec::with_capacity(decompressed.len());
        let mut i = 0;
        let mut count = 0;

        while i < decompressed.len() {
            // Look for BI (begin inline image) preceded by whitespace/newline.
            if i + 2 < decompressed.len()
                && &decompressed[i..i + 2] == b"BI"
                && (i == 0 || decompressed[i - 1].is_ascii_whitespace())
                && decompressed[i + 2].is_ascii_whitespace()
            {
                // Find matching ID marker.
                let bi_start = i;
                new_content.extend_from_slice(b"BI");
                i += 2;

                // Scan through the BI dictionary until ID.
                while i < decompressed.len() {
                    // Check for ID preceded by whitespace.
                    if i + 2 < decompressed.len()
                        && &decompressed[i..i + 2] == b"ID"
                        && (i == 0 || decompressed[i - 1].is_ascii_whitespace())
                        && (i + 2 >= decompressed.len()
                            || decompressed[i + 2] == b' '
                            || decompressed[i + 2] == b'\n'
                            || decompressed[i + 2] == b'\r')
                    {
                        break;
                    }

                    // Check for /I true or /Interpolate true patterns.
                    let replaced = try_replace_interpolate(&decompressed, i, &mut new_content);
                    if let Some(advance) = replaced {
                        i += advance;
                        count += 1;
                        continue;
                    }

                    new_content.push(decompressed[i]);
                    i += 1;
                }

                if count > 0 && i >= decompressed.len() {
                    // Didn't find ID — revert by not counting.
                    count = 0;
                    new_content.truncate(bi_start);
                    new_content.extend_from_slice(&decompressed[bi_start..]);
                    break;
                }
                continue;
            }

            new_content.push(decompressed[i]);
            i += 1;
        }

        if count > 0 {
            total_count += count;
            if let Some(Object::Stream(ref mut stream)) = doc.objects.get_mut(&id) {
                stream.set_plain_content(new_content);
            }
        }
    }

    total_count
}

/// Try to replace `/I true` or `/Interpolate true` with false at position `i`.
/// Returns Some(bytes_consumed) if replacement was made.
fn try_replace_interpolate(data: &[u8], i: usize, out: &mut Vec<u8>) -> Option<usize> {
    // Match /I true (with whitespace)
    if i + 7 <= data.len() && &data[i..i + 2] == b"/I" && data[i + 2].is_ascii_whitespace() {
        // Check it's not /Interpolate (longer name).
        if i + 3 < data.len() && data[i + 3] != b'n' {
            // Check for "true"
            let rest = &data[i + 2..];
            let trimmed = rest.iter().position(|&b| !b.is_ascii_whitespace())?;
            if rest[trimmed..].starts_with(b"true") {
                let after_true = trimmed + 4;
                if after_true >= rest.len()
                    || rest[after_true].is_ascii_whitespace()
                    || rest[after_true] == b'/'
                {
                    out.extend_from_slice(b"/I false");
                    return Some(2 + after_true);
                }
            }
        }
    }

    // Match /Interpolate true
    if i + 18 <= data.len() && &data[i..i + 12] == b"/Interpolate" {
        let rest = &data[i + 12..];
        let trimmed = rest.iter().position(|&b| !b.is_ascii_whitespace())?;
        if rest[trimmed..].starts_with(b"true") {
            let after_true = trimmed + 4;
            if after_true >= rest.len()
                || rest[after_true].is_ascii_whitespace()
                || rest[after_true] == b'/'
            {
                out.extend_from_slice(b"/I false");
                return Some(12 + after_true);
            }
        }
    }

    None
}

// ---------------------------------------------------------------------------
// 6.2.2:1 — Fix `>>BDC` / `>>BMC` / `>>DP` without whitespace in content streams.
// veraPDF treats `>>BDC` as a single undefined operator. Insert a space.
// ---------------------------------------------------------------------------

fn fix_content_stream_operator_spacing(doc: &mut Document) -> usize {
    let content_stream_ids = collect_content_stream_ids(doc);

    let mut count = 0;
    let ids: Vec<ObjectId> = content_stream_ids.into_iter().collect();
    for id in ids {
        // Get decompressed content to check.
        let decompressed = if let Some(Object::Stream(s)) = doc.objects.get(&id) {
            match s.decompressed_content() {
                Ok(d) => d,
                Err(_) => s.content.clone(),
            }
        } else {
            continue;
        };

        let has_spacing_issue = decompressed
            .windows(5)
            .any(|w| w == b">>BDC" || w == b">>BMC")
            || decompressed.windows(4).any(|w| w == b">>DP");

        // Also check for standalone >> without matching << (corrupted BDC/BMC).
        // Check both spaced (">> BDC") and unspaced (">>BDC") since the first
        // pass will add the space — re-evaluated after first pass below.
        let has_standalone_issue = decompressed
            .windows(6)
            .any(|w| w == b">> BDC" || w == b">> BMC")
            || decompressed
                .windows(5)
                .any(|w| w == b">>BDC" || w == b">>BMC");

        if !has_spacing_issue && !has_standalone_issue {
            continue;
        }

        // Fix the decompressed content.
        let mut new_content = Vec::with_capacity(decompressed.len() + 64);
        let mut i = 0;
        while i < decompressed.len() {
            if i + 5 <= decompressed.len()
                && (&decompressed[i..i + 5] == b">>BDC" || &decompressed[i..i + 5] == b">>BMC")
            {
                new_content.extend_from_slice(b">> ");
                new_content.push(decompressed[i + 2]);
                new_content.push(decompressed[i + 3]);
                new_content.push(decompressed[i + 4]);
                i += 5;
                count += 1;
                continue;
            }
            if i + 4 <= decompressed.len() && &decompressed[i..i + 4] == b">>DP" {
                new_content.extend_from_slice(b">> DP");
                i += 4;
                count += 1;
                continue;
            }
            new_content.push(decompressed[i]);
            i += 1;
        }

        // Second pass: fix standalone >> in BDC/BMC without matching <<.
        // Pattern: " N >> BDC" → " <</MCID N >> BDC"
        // Re-evaluate after first pass: >>BDC may have become >> BDC.
        let has_standalone_issue = new_content
            .windows(6)
            .any(|w| w == b">> BDC" || w == b">> BMC");
        if has_standalone_issue {
            let text = new_content.clone();
            new_content.clear();
            let lines: Vec<&[u8]> = text.split(|&b| b == b'\n').collect();
            for (idx, line) in lines.iter().enumerate() {
                if idx > 0 {
                    new_content.push(b'\n');
                }
                if (line.windows(6).any(|w| w == b">> BDC" || w == b">> BMC"))
                    && !line.windows(2).any(|w| w == b"<<")
                {
                    // Find ">>" position and extract the number before it.
                    if let Some(gg) = line.windows(2).position(|w| w == b">>") {
                        // Walk backwards from >> skipping whitespace to find
                        // the number.
                        let before = &line[..gg];
                        let trimmed_end = before
                            .iter()
                            .rposition(|b| !b.is_ascii_whitespace())
                            .map(|p| p + 1)
                            .unwrap_or(0);
                        let num_start = before[..trimmed_end]
                            .iter()
                            .rposition(|b| !b.is_ascii_digit())
                            .map(|p| p + 1)
                            .unwrap_or(0);
                        if num_start < trimmed_end {
                            let prefix = &line[..num_start];
                            let num = &line[num_start..trimmed_end];
                            let suffix_start = gg + 2; // after >>
                            let suffix = &line[suffix_start..];
                            new_content.extend_from_slice(prefix);
                            new_content.extend_from_slice(b"<</MCID ");
                            new_content.extend_from_slice(num);
                            new_content.extend_from_slice(b">>");
                            new_content.extend_from_slice(suffix);
                            count += 1;
                            continue;
                        }
                    }
                    new_content.extend_from_slice(line);
                } else {
                    new_content.extend_from_slice(line);
                }
            }
        }

        // Store decompressed+fixed content; remove Filter so lopdf writes it raw.
        if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(&id) {
            s.dict.remove(b"Filter");
            s.dict.remove(b"DecodeParms");
            s.content = new_content;
            // Re-compress for smaller output.
            let _ = s.compress();
        }
    }
    count
}

// ---------------------------------------------------------------------------
// 6.1.13:5 — Replace tiny non-zero floats (|x| < 1.175e-38) with 0.
// These appear in content streams and violate PDF/A number limits.
// ---------------------------------------------------------------------------

fn fix_tiny_floats_in_streams(doc: &mut Document) -> usize {
    const MIN_POSITIVE: f64 = 1.175e-38;
    let mut count = 0;
    let content_ids = collect_content_stream_ids(doc);
    let ids: Vec<ObjectId> = content_ids.into_iter().collect();

    for id in ids {
        let decompressed = if let Some(Object::Stream(s)) = doc.objects.get(&id) {
            match s.decompressed_content() {
                Ok(d) => d,
                Err(_) => s.content.clone(),
            }
        } else {
            continue;
        };

        // Quick check: look for patterns like "0.000000" with many zeros.
        if !decompressed.windows(8).any(|w| w == b"0.000000") {
            continue;
        }

        // Scan for number tokens and check if they're tiny.
        let mut new_content = Vec::with_capacity(decompressed.len());
        let mut i = 0;
        let mut fixed_any = false;

        while i < decompressed.len() {
            // Check if we're at the start of a number token.
            let at_number = (decompressed[i] == b'0' || decompressed[i] == b'-')
                && i + 1 < decompressed.len()
                && (decompressed[i + 1] == b'.' || decompressed[i + 1] == b'0');

            if !at_number || (i > 0 && is_number_byte(decompressed[i - 1])) {
                new_content.push(decompressed[i]);
                i += 1;
                continue;
            }

            // Extract the full number token.
            let start = i;
            if decompressed[i] == b'-' {
                i += 1;
            }
            while i < decompressed.len() && is_number_byte(decompressed[i]) {
                i += 1;
            }
            let token = &decompressed[start..i];

            // Only check tokens with many decimal digits (potential tiny values).
            if token.len() > 10 {
                if let Ok(s) = std::str::from_utf8(token) {
                    if let Ok(val) = s.parse::<f64>() {
                        if val != 0.0 && val.abs() < MIN_POSITIVE {
                            new_content.push(b'0');
                            count += 1;
                            fixed_any = true;
                            continue;
                        }
                    }
                }
            }
            new_content.extend_from_slice(token);
        }

        if fixed_any {
            if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(&id) {
                s.dict.remove(b"Filter");
                s.dict.remove(b"DecodeParms");
                s.content = new_content;
                let _ = s.compress();
            }
        }
    }
    count
}

fn is_number_byte(b: u8) -> bool {
    b.is_ascii_digit() || b == b'.' || b == b'-'
}

// ---------------------------------------------------------------------------
// 6.2.8.3:4 — Fix JPEG2000 forbidden enumerated colour spaces.
// CIEJab (19) is not allowed in PDF/A. Change to sRGB (16).
// ---------------------------------------------------------------------------

fn fix_jpx_forbidden_colorspaces(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();

    for id in ids {
        let is_jpx = if let Some(Object::Stream(s)) = doc.objects.get(&id) {
            s.dict
                .get(b"Filter")
                .ok()
                .and_then(|o| o.as_name().ok())
                .map(|n| n == b"JPXDecode")
                .unwrap_or(false)
        } else {
            false
        };
        if !is_jpx {
            continue;
        }

        if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(&id) {
            // Find "colr" box in JP2 data and check enumCS.
            if let Some(pos) = find_subsequence(&s.content, b"colr") {
                // colr box layout: [4 bytes len][4 bytes "colr"][1 byte method][...]
                // method 1 = enumerated colorspace: [3 bytes prec+approx][4 bytes enumCS]
                let method_pos = pos + 4;
                if method_pos < s.content.len() && s.content[method_pos] == 1 {
                    let enum_pos = method_pos + 3;
                    if enum_pos + 4 <= s.content.len() {
                        let enum_cs = u32::from_be_bytes([
                            s.content[enum_pos],
                            s.content[enum_pos + 1],
                            s.content[enum_pos + 2],
                            s.content[enum_pos + 3],
                        ]);
                        // CIEJab = 19, not allowed in PDF/A.
                        if enum_cs == 19 {
                            // Replace with sRGB (16), also 3-component.
                            let srgb: [u8; 4] = 16u32.to_be_bytes();
                            s.content[enum_pos] = srgb[0];
                            s.content[enum_pos + 1] = srgb[1];
                            s.content[enum_pos + 2] = srgb[2];
                            s.content[enum_pos + 3] = srgb[3];
                            count += 1;
                        }
                    }
                }
            }
        }
    }
    count
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

// ---------------------------------------------------------------------------
// 6.1.6:1 — Fix hex strings with odd number of hex characters.
// Pad with trailing 0 before the closing >.
// ---------------------------------------------------------------------------

fn fix_odd_hex_strings_in_streams(doc: &mut Document) -> usize {
    let mut count = 0;
    // Process all streams except binary ones (images, fonts, ICC profiles).
    let ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(&id, obj)| {
            if let Object::Stream(s) = obj {
                // Skip binary streams that aren't text-based.
                let subtype = s.dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok());
                if matches!(
                    subtype,
                    Some(b"Image" | b"CIDFontType0C" | b"CIDFontType2" | b"Type1C" | b"OpenType")
                ) {
                    return None;
                }
                // Skip font file streams (have Length1/Length2).
                if s.dict.has(b"Length1") || s.dict.has(b"Length2") {
                    return None;
                }
                // Skip ICC profile streams.
                if s.dict.has(b"N") && s.dict.has(b"Alternate") {
                    return None;
                }
                return Some(id);
            }
            None
        })
        .collect();

    for id in ids {
        let decompressed = if let Some(Object::Stream(s)) = doc.objects.get(&id) {
            match s.decompressed_content() {
                Ok(d) => d,
                Err(_) => s.content.clone(),
            }
        } else {
            continue;
        };

        // Quick check for hex strings.
        if !decompressed.contains(&b'<')
            || decompressed
                .windows(2)
                .all(|w| !(w[0] == b'<' && w[1] != b'<'))
        {
            continue;
        }

        let mut new_content = Vec::with_capacity(decompressed.len() + 16);
        let mut i = 0;
        let mut fixed_any = false;

        while i < decompressed.len() {
            // Skip inline image data: after "ID" (preceded by whitespace),
            // binary data continues until "\nEI" or " EI" followed by
            // whitespace or end of stream. We must not scan binary image
            // data for hex strings.
            if decompressed[i] == b'I'
                && i + 2 < decompressed.len()
                && decompressed[i + 1] == b'D'
                && (decompressed[i + 2] == b' '
                    || decompressed[i + 2] == b'\n'
                    || decompressed[i + 2] == b'\r')
                && (i == 0
                    || decompressed[i - 1] == b' '
                    || decompressed[i - 1] == b'\n'
                    || decompressed[i - 1] == b'\r')
            {
                // Find EI marker: a whitespace char, then 'E', 'I',
                // then whitespace or end of stream.
                let start = i;
                i += 3; // skip "ID" + whitespace
                loop {
                    if i + 2 >= decompressed.len() {
                        // Reached end without finding EI — copy rest.
                        i = decompressed.len();
                        break;
                    }
                    if (decompressed[i] == b'\n'
                        || decompressed[i] == b' '
                        || decompressed[i] == b'\r')
                        && decompressed[i + 1] == b'E'
                        && decompressed[i + 2] == b'I'
                        && (i + 3 >= decompressed.len()
                            || decompressed[i + 3] == b' '
                            || decompressed[i + 3] == b'\n'
                            || decompressed[i + 3] == b'\r'
                            || decompressed[i + 3] == b'Q')
                    {
                        i += 3; // skip ws + "EI"
                        break;
                    }
                    i += 1;
                }
                new_content.extend_from_slice(&decompressed[start..i]);
                continue;
            }
            // Skip literal strings (...) — < and > inside are text, not
            // hex string delimiters. Track nesting for balanced parens.
            if decompressed[i] == b'(' {
                let start = i;
                i += 1;
                let mut depth = 1u32;
                while i < decompressed.len() && depth > 0 {
                    match decompressed[i] {
                        b'\\' => {
                            i += 1; // skip escaped char
                            if i >= decompressed.len() {
                                break;
                            }
                        }
                        b'(' => depth += 1,
                        b')' => depth -= 1,
                        _ => {}
                    }
                    i += 1;
                }
                let end = i.min(decompressed.len());
                new_content.extend_from_slice(&decompressed[start..end]);
                continue;
            }
            // Skip dict begin markers << so we don't treat the second < as
            // a hex string start.
            if decompressed[i] == b'<' && i + 1 < decompressed.len() && decompressed[i + 1] == b'<'
            {
                new_content.push(b'<');
                new_content.push(b'<');
                i += 2;
                continue;
            }
            // Skip dict end markers >>.
            if decompressed[i] == b'>' && i + 1 < decompressed.len() && decompressed[i + 1] == b'>'
            {
                new_content.push(b'>');
                new_content.push(b'>');
                i += 2;
                continue;
            }
            if decompressed[i] == b'<' {
                // Start of hex string — find matching >.
                let start = i;
                i += 1;
                let mut hex_count = 0u32;
                while i < decompressed.len() && decompressed[i] != b'>' {
                    if decompressed[i].is_ascii_hexdigit() {
                        hex_count += 1;
                    }
                    i += 1;
                }
                if i < decompressed.len() && decompressed[i] == b'>' && !hex_count.is_multiple_of(2)
                {
                    // Odd hex count — insert 0 before >.
                    new_content.extend_from_slice(&decompressed[start..i]);
                    new_content.push(b'0');
                    new_content.push(b'>');
                    i += 1;
                    count += 1;
                    fixed_any = true;
                    continue;
                }
                // Even or not a valid hex string — copy as-is.
                new_content.extend_from_slice(&decompressed[start..=i.min(decompressed.len() - 1)]);
                if i < decompressed.len() {
                    i += 1;
                }
                continue;
            }
            new_content.push(decompressed[i]);
            i += 1;
        }

        if fixed_any {
            if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(&id) {
                s.dict.remove(b"Filter");
                s.dict.remove(b"DecodeParms");
                s.content = new_content;
                let _ = s.compress();
            }
        }
    }
    count
}

// ---------------------------------------------------------------------------
// Fix concatenated PDF operators (e.g. "Qq" → "Q q")
// ---------------------------------------------------------------------------
//
// Some broken PDF producers concatenate operators without whitespace separators.
// "Qq" is the most common — Q (restore graphics state) followed by q (save
// graphics state). This is not a valid single operator, causing veraPDF to flag
// rule 6.2.2:1 ("operator not defined in ISO 32000-1").

fn fix_concatenated_operators(doc: &mut Document) -> usize {
    let content_stream_ids = collect_content_stream_ids(doc);
    let mut count = 0;

    for id in content_stream_ids {
        let decompressed = if let Some(Object::Stream(s)) = doc.objects.get(&id) {
            match s.decompressed_content() {
                Ok(d) => d,
                Err(_) => s.content.clone(),
            }
        } else {
            continue;
        };

        // Quick check: does this stream contain "Qq" at all?
        if !decompressed.windows(2).any(|w| w == b"Qq") {
            continue;
        }

        // Replace "Qq" with "Q q" only when it appears as a standalone operator
        // sequence (preceded by whitespace/newline/SOF and followed by
        // whitespace/newline/EOF).
        let mut new_content = Vec::with_capacity(decompressed.len() + 64);
        let mut i = 0;
        let len = decompressed.len();
        let mut fixed = false;

        while i < len {
            if i + 2 <= len && &decompressed[i..i + 2] == b"Qq" {
                let before_ok = i == 0 || decompressed[i - 1].is_ascii_whitespace();
                let after_ok = i + 2 >= len || decompressed[i + 2].is_ascii_whitespace();
                if before_ok && after_ok {
                    new_content.extend_from_slice(b"Q q");
                    i += 2;
                    fixed = true;
                    count += 1;
                    continue;
                }
            }
            new_content.push(decompressed[i]);
            i += 1;
        }

        if fixed {
            if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(&id) {
                s.dict.remove(b"Filter");
                s.dict.remove(b"DecodeParms");
                s.content = new_content;
                let _ = s.compress();
            }
        }
    }
    count
}
