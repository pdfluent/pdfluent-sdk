//! Supplementary PDF/A compliance fixes.
//!
//! Additional passes that address remaining veraPDF rule failures
//! not fully covered by pdfa_cleanup or pdfa_fonts modules.

use flate2::read::ZlibDecoder;
use lopdf::{dictionary, Document, Object, ObjectId};
use std::io::Read;

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
    let usecmap_stripped = strip_nonstandard_usecmap_references(doc);
    let opi_keys_removed = fix_opi_keys(doc);
    let stream_f_keys_removed = fix_stream_f_keys(doc);
    let postscript_xobjects_removed = fix_postscript_xobjects(doc);
    let reference_xobjects_removed = fix_reference_xobjects(doc);
    let overflow_integers_fixed = fix_overflow_integers(doc);
    let long_strings_fixed = fix_long_strings(doc);
    let jpx_colorspace_fixed = fix_jpx_forbidden_colorspaces(doc);
    // Content stream modifications (decompress/recompress) must run before
    // fix_stream_lengths to ensure Length values are correct.
    let unreadable_streams_fixed = fix_unreadable_content_streams(doc);
    let invalid_prefix_fixed = fix_invalid_operator_preamble(doc);
    let gs_nesting_fixed = fix_graphics_state_nesting_limit(doc);
    let operator_spacing_fixed = fix_content_stream_operator_spacing(doc)
        + invalid_prefix_fixed
        + unreadable_streams_fixed
        + gs_nesting_fixed;
    let tiny_floats_fixed =
        fix_tiny_floats_in_streams(doc) + fix_non_finite_numbers_in_streams(doc);
    let odd_hex_strings_fixed = fix_odd_hex_strings_in_streams(doc);
    let non_ascii_names_fixed = fix_non_ascii_pdf_names(doc);
    let inline_image_interpolate_fixed = fix_inline_image_interpolate(doc);
    let concatenated_operators_fixed = fix_concatenated_operators(doc);
    let unknown_operators_stripped = strip_unknown_content_stream_operators(doc);
    let page_boundary_fixed = fix_page_boundary_sizes(doc);
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
        usecmap_stripped,
        opi_keys_removed,
        stream_f_keys_removed,
        postscript_xobjects_removed,
        reference_xobjects_removed,
        overflow_integers_fixed,
        long_strings_fixed,
        operator_spacing_fixed,
        tiny_floats_fixed,
        odd_hex_strings_fixed,
        non_ascii_names_fixed,
        inline_image_interpolate_fixed,
        jpx_colorspace_fixed,
        concatenated_operators_fixed,
        unknown_operators_stripped,
        page_boundary_fixed,
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
    pub usecmap_stripped: usize,
    pub opi_keys_removed: usize,
    pub stream_f_keys_removed: usize,
    pub postscript_xobjects_removed: usize,
    pub reference_xobjects_removed: usize,
    pub overflow_integers_fixed: usize,
    pub long_strings_fixed: usize,
    pub operator_spacing_fixed: usize,
    pub tiny_floats_fixed: usize,
    pub odd_hex_strings_fixed: usize,
    pub non_ascii_names_fixed: usize,
    pub inline_image_interpolate_fixed: usize,
    pub jpx_colorspace_fixed: usize,
    pub concatenated_operators_fixed: usize,
    pub unknown_operators_stripped: usize,
    pub page_boundary_fixed: usize,
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

    // Remove forbidden annotation IDs from all page Annots arrays.
    if !forbidden_ids.is_empty() {
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
    }

    // Fix Stamp annotations where AP.N is a sub-appearance-states dict instead
    // of a direct stream (6.3.3/4). veraPDF requires N to be a single stream.
    // Collapse the dict by picking the first stream value and pointing N there.
    for id in &ids {
        let fix = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(id) else {
                continue;
            };
            let is_stamp = matches!(
                dict.get(b"Subtype").ok(),
                Some(Object::Name(ref n)) if n == b"Stamp"
            );
            if !is_stamp {
                continue;
            }
            // Get the AP dict.
            let ap_ref: Option<ObjectId> = match dict.get(b"AP").ok() {
                Some(Object::Reference(r)) => Some(*r),
                _ => None,
            };
            ap_ref
        };
        let Some(ap_id) = fix else { continue };

        // Check if AP.N is a dict (not a stream).
        let target: Option<ObjectId> = {
            let Some(Object::Dictionary(ap)) = doc.objects.get(&ap_id) else {
                continue;
            };
            match ap.get(b"N").ok() {
                Some(Object::Reference(n_id)) => {
                    // N points to an object — check if it's a dict (not a stream).
                    match doc.objects.get(n_id) {
                        Some(Object::Dictionary(nd)) => {
                            // Pick the first reference value in this sub-dict.
                            nd.iter().find_map(|(_, v)| {
                                if let Object::Reference(r) = v {
                                    Some(*r)
                                } else {
                                    None
                                }
                            })
                        }
                        _ => None,
                    }
                }
                _ => None,
            }
        };

        if let Some(stream_id) = target {
            if let Some(Object::Dictionary(ref mut ap)) = doc.objects.get_mut(&ap_id) {
                ap.set("N", Object::Reference(stream_id));
                count += 1;
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

/// Expand a PDF filter abbreviation to its full name (ISO 32000-1 Table 6).
/// Returns None if `name` is already a full name or is unknown.
fn expand_filter_abbrev(name: &[u8]) -> Option<&'static [u8]> {
    match name {
        b"AHx" => Some(b"ASCIIHexDecode"),
        b"A85" => Some(b"ASCII85Decode"),
        b"Fl" => Some(b"FlateDecode"),
        b"RL" => Some(b"RunLengthDecode"),
        b"CCF" => Some(b"CCITTFaxDecode"),
        b"DCT" => Some(b"DCTDecode"),
        _ => None,
    }
}

fn fix_crypt_filters(doc: &mut Document) -> usize {
    let mut count = 0;
    // Full filter names permitted by PDF/A-2b (6.1.7.2:1).
    // LZWDecode is standard but forbidden — handled by pdfa_cleanup.
    let standard_filters: &[&[u8]] = &[
        b"ASCIIHexDecode",
        b"ASCII85Decode",
        b"FlateDecode",
        b"RunLengthDecode",
        b"CCITTFaxDecode",
        b"JBIG2Decode",
        b"DCTDecode",
        b"JPXDecode",
    ];

    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let action = {
            let Some(Object::Stream(stream)) = doc.objects.get(&id) else {
                continue;
            };
            match stream.dict.get(b"Filter").ok() {
                Some(Object::Name(n)) => {
                    if expand_filter_abbrev(n).is_some() {
                        // Abbreviated filter name — expand in-place, no re-encode needed.
                        CryptAction::ExpandAbbreviations
                    } else if n == b"Crypt"
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
                            expand_filter_abbrev(n).is_none()
                                && (n == b"Crypt"
                                    || (n != b"LZWDecode"
                                        && !standard_filters.contains(&n.as_slice())))
                        } else {
                            false
                        }
                    });
                    let has_abbrev = arr
                        .iter()
                        .any(|o| matches!(o, Object::Name(n) if expand_filter_abbrev(n).is_some()));
                    if has_forbidden {
                        CryptAction::FilterArray
                    } else if has_abbrev {
                        CryptAction::ExpandAbbreviations
                    } else {
                        CryptAction::None
                    }
                }
                _ => CryptAction::None,
            }
        };

        match action {
            CryptAction::None => {}
            CryptAction::ExpandAbbreviations => {
                // Rename abbreviated filter names to their full equivalents.
                // Content bytes are valid as-is — no re-encoding needed.
                let new_filter = {
                    let Some(Object::Stream(stream)) = doc.objects.get(&id) else {
                        continue;
                    };
                    match stream.dict.get(b"Filter").ok() {
                        Some(Object::Name(n)) => {
                            expand_filter_abbrev(n).map(|full| Object::Name(full.to_vec()))
                        }
                        Some(Object::Array(arr)) => {
                            let expanded: Vec<Object> = arr
                                .iter()
                                .map(|o| {
                                    if let Object::Name(n) = o {
                                        if let Some(full) = expand_filter_abbrev(n) {
                                            return Object::Name(full.to_vec());
                                        }
                                    }
                                    o.clone()
                                })
                                .collect();
                            Some(Object::Array(expanded))
                        }
                        _ => None,
                    }
                };
                if let Some(filter_obj) = new_filter {
                    if let Some(Object::Stream(ref mut stream)) = doc.objects.get_mut(&id) {
                        stream.dict.set("Filter", filter_obj);
                        count += 1;
                    }
                }
            }
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
    ExpandAbbreviations,
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
    // If proper decompression failed, fall back to wrapping the raw bytes in
    // FlateDecode so the forbidden filter is removed even for corrupt streams.
    let raw_data = if let Some(d) = decoded {
        d
    } else {
        let Some(Object::Stream(stream)) = doc.objects.get(&id) else {
            return false;
        };
        stream.content.clone()
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
    fn fix_filespec_dict(dict: &mut lopdf::Dictionary) -> bool {
        if !dict.has(b"EF") {
            return false;
        }
        dict.remove(b"EF");
        // Ensure F and UF keys exist (required by 6.8/2).
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
        true
    }

    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();

    for id in ids {
        match doc.objects.get_mut(&id) {
            Some(Object::Dictionary(dict)) => {
                // Fix standalone file spec dicts.
                if fix_filespec_dict(dict) {
                    count += 1;
                }
                // Also fix inline /FS sub-dicts (e.g. in FileAttachment annotations).
                if let Ok(Object::Dictionary(ref mut fs_dict)) = dict.get_mut(b"FS") {
                    if fix_filespec_dict(fs_dict) {
                        count += 1;
                    }
                }
            }
            Some(Object::Stream(s)) => {
                if fix_filespec_dict(&mut s.dict) {
                    count += 1;
                }
                if let Ok(Object::Dictionary(ref mut fs_dict)) = s.dict.get_mut(b"FS") {
                    if fix_filespec_dict(fs_dict) {
                        count += 1;
                    }
                }
            }
            _ => {}
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

/// Find Form XObjects (and page content streams) that use resources not declared
/// in their explicit Resources dict. Propagates missing Font, ExtGState,
/// ColorSpace, XObject, and Pattern resources from the global document pool.
fn propagate_missing_font_resources(doc: &mut Document) -> usize {
    use std::collections::{HashMap, HashSet};

    type ResMap = HashMap<Vec<u8>, Object>;

    // Step 1: Build global resource maps from ALL dicts/stream dicts in the doc.
    let mut global: [ResMap; 5] = [
        HashMap::new(), // Font
        HashMap::new(), // ExtGState
        HashMap::new(), // ColorSpace
        HashMap::new(), // XObject
        HashMap::new(), // Pattern
    ];
    const CATS: [&[u8]; 5] = [b"Font", b"ExtGState", b"ColorSpace", b"XObject", b"Pattern"];

    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();

    for id in &ids {
        let source_dict = match doc.objects.get(id) {
            Some(Object::Stream(s)) => Some(&s.dict),
            Some(Object::Dictionary(d)) => Some(d),
            _ => None,
        };
        let Some(source_dict) = source_dict else {
            continue;
        };
        for (i, cat) in CATS.iter().enumerate() {
            if let Some(rd) = get_named_resource_dict_from_resources(source_dict, doc, cat) {
                for (name, val) in rd.iter() {
                    global[i].entry(name.clone()).or_insert_with(|| val.clone());
                }
            }
        }
    }

    if global.iter().all(|m| m.is_empty()) {
        return 0;
    }

    // Helper: operators that name a resource in the preceding name operand.
    // Format: (operator_bytes, resource_category_index)
    const OP_TO_CAT: &[(&[u8], usize)] = &[
        (b"Tf", 0),  // /FontName size Tf → Font
        (b"gs", 1),  // /GSName gs → ExtGState
        (b"cs", 2),  // /CSName cs → ColorSpace
        (b"CS", 2),  // /CSName CS → ColorSpace
        (b"Do", 3),  // /XObjName Do → XObject
        (b"scn", 4), // /PatName scn → Pattern
        (b"SCN", 4), // /PatName SCN → Pattern
    ];

    // Step 2: For each Form XObject, find missing resources and add them.
    let mut count = 0;
    for id in &ids {
        let (content, existing_by_cat, is_form) = {
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
            let content = s.decompressed_content().ok().unwrap_or_else(|| {
                if s.content.is_empty() {
                    vec![]
                } else {
                    s.content.clone()
                }
            });
            let mut existing_by_cat: [HashSet<Vec<u8>>; 5] = Default::default();
            for (i, cat) in CATS.iter().enumerate() {
                if let Some(rd) = get_named_resource_dict_from_resources(&s.dict, doc, cat) {
                    existing_by_cat[i] = rd.iter().map(|(k, _)| k.clone()).collect();
                }
            }
            (content, existing_by_cat, is_form)
        };
        let _ = is_form;

        // Scan content for resource name references.
        // Operators and their name-operand offset from the operator token:
        //   Tf: /FontName size Tf  → name is at i-2
        //   all others:  /Name op  → name is at i-1
        let mut missing_by_cat: [Vec<(Vec<u8>, Object)>; 5] = Default::default();
        let tokens: Vec<&[u8]> = content
            .split(|&b| b == b' ' || b == b'\n' || b == b'\r' || b == b'\t')
            .filter(|t| !t.is_empty())
            .collect();
        // Deduplicate within this XObject.
        let mut seen: [std::collections::HashSet<Vec<u8>>; 5] = Default::default();
        for i in 0..tokens.len() {
            let tok = tokens[i];
            for &(op, cat_idx) in OP_TO_CAT {
                // Match exact operator OR operator immediately followed by non-alphanumeric
                // (e.g. "Tf[<..." where "[" follows immediately without space).
                let matches_op = tok == op
                    || (tok.starts_with(op)
                        && tok.get(op.len()).is_none_or(|b| !b.is_ascii_alphanumeric()));
                if !matches_op {
                    continue;
                }
                // Tf takes two operands before it; all others take one.
                let name_offset = if op == b"Tf" { 2 } else { 1 };
                if i < name_offset {
                    continue;
                }
                let prev = tokens[i - name_offset];
                if !prev.starts_with(b"/") {
                    continue;
                }
                let name = prev[1..].to_vec();
                if existing_by_cat[cat_idx].contains(&name) || seen[cat_idx].contains(&name) {
                    continue;
                }
                if let Some(obj) = global[cat_idx].get(&name) {
                    seen[cat_idx].insert(name.clone());
                    missing_by_cat[cat_idx].push((name, obj.clone()));
                }
            }
        }

        if missing_by_cat.iter().all(|v| v.is_empty()) {
            continue;
        }

        // Add missing resources to the Form XObject's Resources dictionary.
        if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(id) {
            if !s.dict.has(b"Resources") {
                s.dict
                    .set("Resources", Object::Dictionary(lopdf::Dictionary::new()));
            }
            if let Ok(Object::Dictionary(ref mut resources)) = s.dict.get_mut(b"Resources") {
                let mut changed = false;
                for (i, cat) in CATS.iter().enumerate() {
                    if missing_by_cat[i].is_empty() {
                        continue;
                    }
                    let cat_str = String::from_utf8_lossy(cat).to_string();
                    if !resources.has(cat) {
                        resources.set(
                            cat_str.clone(),
                            Object::Dictionary(lopdf::Dictionary::new()),
                        );
                    }
                    if let Ok(Object::Dictionary(ref mut cat_dict)) =
                        resources.get_mut(cat.as_ref())
                    {
                        for (name, obj) in &missing_by_cat[i] {
                            let key_str = String::from_utf8_lossy(name).to_string();
                            if !cat_dict.has(name.as_slice()) {
                                cat_dict.set(key_str, obj.clone());
                                changed = true;
                            }
                        }
                    }
                }
                if changed {
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
/// Get a named resource sub-dictionary (e.g. Font, ExtGState, ColorSpace)
/// from a Resources dictionary that may be inline or indirect.
fn get_named_resource_dict_from_resources(
    dict: &lopdf::Dictionary,
    doc: &Document,
    key: &[u8],
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
    match resources.get(key).ok() {
        Some(Object::Dictionary(fd)) => Some(fd.clone()),
        Some(Object::Reference(ref_id)) => match doc.objects.get(ref_id) {
            Some(Object::Dictionary(fd)) => Some(fd.clone()),
            _ => None,
        },
        _ => None,
    }
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
        // Exclude standard font-file keys Length1/Length2/Length3 which are
        // legitimate PDF stream dict entries (ISO 32000-1 Table 126).
        let has_corrupted_length = stream.dict.iter().any(|(k, _)| {
            // Standard keys that start with "Length" followed by a digit are valid.
            let is_standard_length_n =
                k.len() == 7 && k[..6].eq_ignore_ascii_case(b"Length") && k[6].is_ascii_digit();
            if is_standard_length_n {
                return false;
            }
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
                // Remove corrupted length-like keys (but preserve standard LengthN keys).
                let corrupt_keys: Vec<Vec<u8>> = stream
                    .dict
                    .iter()
                    .filter(|(k, _)| {
                        let is_standard_length_n = k.len() == 7
                            && k[..6].eq_ignore_ascii_case(b"Length")
                            && k[6].is_ascii_digit();
                        if is_standard_length_n {
                            return false;
                        }
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
    let mut ref_targets: Vec<ObjectId> = Vec::new();
    // (descendant CIDFont id, replacement gid, replacement width in text space)
    let mut custom_map_targets: Vec<(ObjectId, u16, i64)> = Vec::new();
    // (container object, is_type0_dict, index_in_desc_array)
    let mut inline_targets: Vec<(ObjectId, bool, usize)> = Vec::new();

    let has_valid_cidtogid = |doc: &Document, cidfont: &lopdf::Dictionary| -> bool {
        match cidfont.get(b"CIDToGIDMap").ok() {
            Some(Object::Name(n)) if n == b"Identity" => true,
            Some(Object::Reference(r)) => matches!(doc.objects.get(r), Some(Object::Stream(_))),
            Some(Object::Stream(_)) => true,
            _ => false,
        }
    };

    let type0_uses_identity = |doc: &Document, type0: &lopdf::Dictionary| -> bool {
        match type0.get(b"Encoding").ok() {
            Some(Object::Name(n)) => {
                let s = String::from_utf8_lossy(n).to_ascii_lowercase();
                s == "identity-h" || s == "identity-v"
            }
            Some(Object::Reference(r)) => match doc.objects.get(r) {
                Some(Object::Name(n)) => {
                    let s = String::from_utf8_lossy(n).to_ascii_lowercase();
                    s == "identity-h" || s == "identity-v"
                }
                Some(Object::Dictionary(d)) => match d.get(b"CMapName").ok() {
                    Some(Object::Name(n)) => {
                        let s = String::from_utf8_lossy(n).to_ascii_lowercase();
                        s == "identity-h" || s == "identity-v"
                    }
                    _ => false,
                },
                Some(Object::Stream(s)) => match s.dict.get(b"CMapName").ok() {
                    Some(Object::Name(n)) => {
                        let s = String::from_utf8_lossy(n).to_ascii_lowercase();
                        s == "identity-h" || s == "identity-v"
                    }
                    _ => false,
                },
                _ => false,
            },
            _ => false,
        }
    };

    let tt_glyph_has_data = |face: &ttf_parser::Face, gid: ttf_parser::GlyphId| -> bool {
        let raw = face.raw_face();
        let Some(head) = raw.table(ttf_parser::Tag::from_bytes(b"head")) else {
            return true;
        };
        let Some(loca) = raw.table(ttf_parser::Tag::from_bytes(b"loca")) else {
            return true;
        };
        if head.len() < 52 {
            return true;
        }
        let idx_format = i16::from_be_bytes([head[50], head[51]]);
        let g = gid.0 as usize;

        if idx_format == 0 {
            let off = g * 2;
            if off + 4 > loca.len() {
                return true;
            }
            let o1 = u16::from_be_bytes([loca[off], loca[off + 1]]) as u32;
            let o2 = u16::from_be_bytes([loca[off + 2], loca[off + 3]]) as u32;
            o2 > o1
        } else {
            let off = g * 4;
            if off + 8 > loca.len() {
                return true;
            }
            let o1 = u32::from_be_bytes([loca[off], loca[off + 1], loca[off + 2], loca[off + 3]]);
            let o2 =
                u32::from_be_bytes([loca[off + 4], loca[off + 5], loca[off + 6], loca[off + 7]]);
            o2 > o1
        }
    };

    let cidfont_replacement_gid_dw =
        |doc: &Document, cidfont: &lopdf::Dictionary| -> Option<(u16, i64)> {
            let fd_id = match cidfont.get(b"FontDescriptor").ok() {
                Some(Object::Reference(fd_id)) => *fd_id,
                _ => return None,
            };
            let fd = match doc.objects.get(&fd_id) {
                Some(Object::Dictionary(fd)) => fd,
                _ => return None,
            };
            let ff2_obj = fd.get(b"FontFile2").ok()?;
            let mut font_stream = match ff2_obj {
                Object::Reference(r) => match doc.objects.get(r) {
                    Some(Object::Stream(s)) => s.clone(),
                    _ => return None,
                },
                Object::Stream(s) => s.clone(),
                _ => return None,
            };
            let _ = font_stream.decompress();
            let font_data = font_stream.content;
            let face = ttf_parser::Face::parse(&font_data, 0).ok()?;
            let upm = face.units_per_em() as f64;
            if upm <= 0.0 {
                return None;
            }
            let scale = 1000.0 / upm;
            let num = face.number_of_glyphs();
            if num <= 1 {
                return None;
            }

            let mut gid = face.glyph_index(' ').map(|g| g.0).filter(|g| {
                *g > 0
                    && face.glyph_hor_advance(ttf_parser::GlyphId(*g)).unwrap_or(0) > 0
                    && tt_glyph_has_data(&face, ttf_parser::GlyphId(*g))
            });

            if gid.is_none() {
                for g in 1..num {
                    let adv = face.glyph_hor_advance(ttf_parser::GlyphId(g)).unwrap_or(0);
                    if adv > 0 && tt_glyph_has_data(&face, ttf_parser::GlyphId(g)) {
                        gid = Some(g);
                        break;
                    }
                }
            }
            let gid = gid?;
            let adv = face
                .glyph_hor_advance(ttf_parser::GlyphId(gid))
                .unwrap_or(1000);
            let dw = (adv as f64 * scale).round() as i64;
            Some((gid, dw))
        };

    let is_cid2_with_embedded_ff2 = |doc: &Document, cidfont: &lopdf::Dictionary| -> bool {
        let has_embedded_ff2 = match cidfont.get(b"FontDescriptor").ok() {
            Some(Object::Reference(fd_id)) => {
                matches!(
                    doc.objects.get(fd_id),
                    Some(Object::Dictionary(fd)) if fd.has(b"FontFile2")
                )
            }
            _ => false,
        };
        let is_cid2 = matches!(
            cidfont.get(b"Subtype").ok(),
            Some(Object::Name(ref n)) if n == b"CIDFontType2"
        ) || has_embedded_ff2;
        is_cid2 && has_embedded_ff2
    };

    let needs_cidtogid_fix = |doc: &Document, cidfont: &lopdf::Dictionary| -> bool {
        is_cid2_with_embedded_ff2(doc, cidfont) && !has_valid_cidtogid(doc, cidfont)
    };

    for id in ids {
        let Some(Object::Dictionary(dict)) = doc.objects.get(&id) else {
            continue;
        };
        let is_type0 = matches!(
            dict.get(b"Subtype").ok(),
            Some(Object::Name(ref n)) if n == b"Type0"
        );
        if !is_type0 {
            continue;
        }
        let identity_parent = type0_uses_identity(doc, dict);

        match dict.get(b"DescendantFonts").ok() {
            Some(Object::Array(arr)) => {
                for (idx, item) in arr.iter().enumerate() {
                    match item {
                        Object::Reference(desc_id) => {
                            let Some(Object::Dictionary(cidfont)) = doc.objects.get(desc_id) else {
                                continue;
                            };
                            if identity_parent {
                                if needs_cidtogid_fix(doc, cidfont) {
                                    ref_targets.push(*desc_id);
                                }
                            } else if is_cid2_with_embedded_ff2(doc, cidfont)
                                && !has_valid_cidtogid(doc, cidfont)
                            {
                                // If fix_truetype_cid_widths already populated a W array,
                                // the per-GID widths are correct for Identity mapping —
                                // use Identity instead of a degenerate single-glyph map so
                                // we don't clobber those widths.
                                if cidfont.has(b"W") {
                                    ref_targets.push(*desc_id);
                                } else if let Some((gid, dw)) =
                                    cidfont_replacement_gid_dw(doc, cidfont)
                                {
                                    custom_map_targets.push((*desc_id, gid, dw));
                                } else if needs_cidtogid_fix(doc, cidfont) {
                                    ref_targets.push(*desc_id);
                                }
                            }
                        }
                        Object::Dictionary(cidfont) => {
                            if needs_cidtogid_fix(doc, cidfont) {
                                inline_targets.push((id, true, idx));
                            }
                        }
                        _ => {}
                    }
                }
            }
            Some(Object::Reference(arr_id)) => {
                let Some(Object::Array(arr)) = doc.objects.get(arr_id) else {
                    continue;
                };
                for (idx, item) in arr.iter().enumerate() {
                    match item {
                        Object::Reference(desc_id) => {
                            let Some(Object::Dictionary(cidfont)) = doc.objects.get(desc_id) else {
                                continue;
                            };
                            if identity_parent {
                                if needs_cidtogid_fix(doc, cidfont) {
                                    ref_targets.push(*desc_id);
                                }
                            } else if is_cid2_with_embedded_ff2(doc, cidfont)
                                && !has_valid_cidtogid(doc, cidfont)
                            {
                                // If fix_truetype_cid_widths already populated a W array,
                                // the per-GID widths are correct for Identity mapping —
                                // use Identity instead of a degenerate single-glyph map so
                                // we don't clobber those widths.
                                if cidfont.has(b"W") {
                                    ref_targets.push(*desc_id);
                                } else if let Some((gid, dw)) =
                                    cidfont_replacement_gid_dw(doc, cidfont)
                                {
                                    custom_map_targets.push((*desc_id, gid, dw));
                                } else if needs_cidtogid_fix(doc, cidfont) {
                                    ref_targets.push(*desc_id);
                                }
                            }
                        }
                        Object::Dictionary(cidfont) => {
                            if needs_cidtogid_fix(doc, cidfont) {
                                inline_targets.push((*arr_id, false, idx));
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    for desc_id in ref_targets {
        if let Some(Object::Dictionary(ref mut cidfont)) = doc.objects.get_mut(&desc_id) {
            cidfont.set("CIDToGIDMap", Object::Name(b"Identity".to_vec()));
            count += 1;
        }
    }

    for (desc_id, gid, dw) in custom_map_targets {
        // Build a full 65536-entry map (2 bytes per CID) to one valid glyph.
        let gid_be = gid.to_be_bytes();
        let mut map = vec![0u8; 65536 * 2];
        for i in (0..map.len()).step_by(2) {
            map[i] = gid_be[0];
            map[i + 1] = gid_be[1];
        }
        let map_id = doc.add_object(Object::Stream(lopdf::Stream::new(dictionary! {}, map)));

        if let Some(Object::Dictionary(ref mut cidfont)) = doc.objects.get_mut(&desc_id) {
            cidfont.set("CIDToGIDMap", Object::Reference(map_id));
            // Keep dictionary width in sync with the forced replacement glyph.
            cidfont.set("DW", Object::Integer(dw));
            cidfont.remove(b"W");
            count += 1;
        }
    }

    for (container_id, is_type0_dict, idx) in inline_targets {
        if is_type0_dict {
            if let Some(Object::Dictionary(ref mut type0)) = doc.objects.get_mut(&container_id) {
                if let Ok(Object::Array(arr)) = type0.get_mut(b"DescendantFonts") {
                    if let Some(Object::Dictionary(ref mut cidfont)) = arr.get_mut(idx) {
                        cidfont.set("CIDToGIDMap", Object::Name(b"Identity".to_vec()));
                        count += 1;
                    }
                }
            }
        } else if let Some(Object::Array(ref mut arr)) = doc.objects.get_mut(&container_id) {
            if let Some(Object::Dictionary(ref mut cidfont)) = arr.get_mut(idx) {
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

        // Identity-H/V CMaps are exempt from CIDSystemInfo matching (ISO 19005-2 §6.2.11.3.1:
        // "If the Encoding key … is Identity-H or Identity-V, any values … may be used").
        // Skipping here prevents us from overwriting a valid CIDSystemInfo that is shared
        // with another descendant font using a non-Identity CMap.
        if let Object::Name(name) = &encoding_obj {
            let n = String::from_utf8_lossy(name);
            if n == "Identity-H" || n == "Identity-V" {
                continue;
            }
        }

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
            // Registry/Ordering match — keep Supplement in sync with the CMap.
            // veraPDF validates exact compatibility here, not just CIDFont <= CMap.
            let cmap_supplement = get_cmap_supplement(doc, &encoding_obj);
            if let Some(cmap_sup) = cmap_supplement {
                let cidfont_sup = get_cidfont_supplement(doc, cid_font_id);
                if cidfont_sup != cmap_sup {
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
        Object::Name(name) => predefined_cmap_supplement(&String::from_utf8_lossy(name)),
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

fn predefined_cmap_supplement(cmap_name: &str) -> Option<i64> {
    match cmap_name {
        // Adobe-GB1-0: base GB 2312-80 CMaps (Supplement 0).
        "GB-EUC-H" | "GB-EUC-V" | "GBpc-EUC-H" | "GBpc-EUC-V" => return Some(0),
        // Adobe-GB1-2: GBK extension CMaps (Supplement 2).
        "GBK-EUC-H" | "GBK-EUC-V" | "GBKp-EUC-H" | "GBKp-EUC-V" => return Some(2),
        // Adobe-GB1-4: GBK2K and UniGB-UCS2 CMaps (Supplement 4).
        "GBK2K-H" | "GBK2K-V" | "UniGB-UCS2-H" | "UniGB-UCS2-V" => return Some(4),
        // Adobe-GB1-6: UniGB UTF-16 CMaps (Supplement 6).
        "UniGB-UTF16-H" | "UniGB-UTF16-V" => return Some(6),
        // Adobe-Japan1-0: base EUC CMaps (Supplement 0).
        "EUC-H" | "EUC-V" => return Some(0),
        // Adobe Tech Note #5094: 90pv-RKSJ-H/V were introduced with
        // Adobe-Japan1-1.
        "90pv-RKSJ-H" | "90pv-RKSJ-V" => return Some(1),
        // Adobe Tech Note #5094: these legacy CMaps were introduced with
        // Adobe-Japan1-2.
        "90ms-RKSJ-H" | "90ms-RKSJ-V" | "90msp-RKSJ-H" | "90msp-RKSJ-V" | "78ms-RKSJ-H"
        | "78ms-RKSJ-V" | "UniJIS-UTF8-H" | "UniJIS-UTF8-V" => return Some(2),
        // The vendored Adobe-Japan1-7 UniJIS UCS2 CMaps advertise
        // CIDSystemInfo Supplement 4 in their headers.
        "UniJIS-UCS2-H" | "UniJIS-UCS2-V" | "UniJIS-UCS2-HW-H" | "UniJIS-UCS2-HW-V" => {
            return Some(4)
        }
        _ => {}
    }
    if cmap_name.contains("UniKS") {
        return Some(1);
    }
    None
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
    if cmap_name.contains("Japan") || cmap_name.starts_with("90") || cmap_name.contains("UniJIS") {
        return Some(("Adobe".to_string(), "Japan1".to_string()));
    }
    if cmap_name.contains("Korea") || cmap_name.starts_with("KS") || cmap_name.contains("UniKS") {
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
    concat!(env!("CARGO_MANIFEST_DIR"), "/resources/cmap"),
    "/usr/share/poppler/cMap",
    "/usr/share/fonts/cmap",
    "/usr/share/fonts/cMap",
    "/usr/share/ghostscript/cMap",
];

fn cmap_requires_embedding_for_verapdf(cmap_name: &str) -> bool {
    // veraPDF 1.28's 6.2.11.3.3 whitelist does not treat these UniGB UTF16
    // names as predefined, so keep them embedded to avoid false negatives.
    matches!(cmap_name, "UniGB-UTF16-H" | "UniGB-UTF16-V")
}

/// Strip non-standard /UseCMap references from embedded CMap dictionaries.
///
/// PDF/A-2 6.2.11.3.3:3 allows references only to predefined CMaps from
/// ISO 32000-1 Table 118. Keeping custom references (e.g. Adobe-Korea1-2)
/// triggers both 6.2.11.3.3:3 and 6.2.11.3.3:1 in veraPDF.
fn strip_nonstandard_usecmap_references(doc: &mut Document) -> usize {
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut stripped = 0usize;

    for id in ids {
        let should_strip = {
            let Some(Object::Stream(stream)) = doc.objects.get(&id) else {
                continue;
            };
            let is_cmap = matches!(
                stream.dict.get(b"Type").ok(),
                Some(Object::Name(ref n)) if n == b"CMap"
            ) || stream.dict.has(b"CMapName")
                || stream.dict.has(b"UseCMap");
            if !is_cmap {
                continue;
            }

            match stream.dict.get(b"UseCMap").ok() {
                Some(Object::Name(name)) => {
                    let usecmap = String::from_utf8_lossy(name).to_string();
                    !PREDEFINED_CMAPS.contains(&usecmap.as_str())
                }
                Some(Object::Reference(r)) => {
                    // If the reference cannot be resolved to a predefined CMap name,
                    // strip it to avoid forbidden non-standard CMap chains.
                    match doc.objects.get(r) {
                        Some(Object::Name(name)) => {
                            let usecmap = String::from_utf8_lossy(name).to_string();
                            !PREDEFINED_CMAPS.contains(&usecmap.as_str())
                        }
                        Some(Object::Dictionary(d)) => {
                            let name = d.get(b"CMapName").ok().and_then(|o| match o {
                                Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
                                _ => None,
                            });
                            name.is_none_or(|n| !PREDEFINED_CMAPS.contains(&n.as_str()))
                        }
                        Some(Object::Stream(s)) => {
                            let name = s.dict.get(b"CMapName").ok().and_then(|o| match o {
                                Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
                                _ => None,
                            });
                            name.is_none_or(|n| !PREDEFINED_CMAPS.contains(&n.as_str()))
                        }
                        _ => true,
                    }
                }
                Some(_) => true,
                None => false,
            }
        };

        if should_strip {
            if let Some(Object::Stream(ref mut stream)) = doc.objects.get_mut(&id) {
                stream.dict.remove(b"UseCMap");
                stripped += 1;
            }
        }
    }

    stripped
}

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
        if PREDEFINED_CMAPS.contains(&cmap_name.as_str())
            && !cmap_requires_embedding_for_verapdf(&cmap_name)
        {
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

/// Replace unreadable content streams (bad filter data) with empty streams.
///
/// Corrupt compressed content can surface as undefined operators during
/// validation. For such streams, keep a valid but empty content stream.
fn fix_unreadable_content_streams(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = collect_content_stream_ids(doc).into_iter().collect();

    for id in ids {
        let action = if let Some(Object::Stream(s)) = doc.objects.get(&id) {
            let unreadable_by_lopdf = s.dict.has(b"Filter") && s.decompressed_content().is_err();
            // lopdf can occasionally report Ok(empty) for corrupt Flate payloads.
            // Detect those with a strict zlib decode and neutralize the stream.
            let unreadable_single_flate = is_single_flate_filter(s) && strict_flate_decode_fails(s);
            if unreadable_by_lopdf || unreadable_single_flate {
                1usize // clear
            } else if is_single_flate_filter(s) {
                // Detect double-compressed streams: FlateDecode content that starts
                // with a zlib header (0x78) after decompression — re-decompress.
                if let Ok(dec1) = s.decompressed_content() {
                    if dec1.len() > 2 && dec1[0] == 0x78 {
                        let mut dec2 = Vec::new();
                        let mut decoder = ZlibDecoder::new(dec1.as_slice());
                        if decoder.read_to_end(&mut dec2).is_ok() && !dec2.is_empty() {
                            2 // re-decompress and store uncompressed
                        } else {
                            0
                        }
                    } else {
                        0
                    }
                } else {
                    0
                }
            } else {
                0
            }
        } else {
            0
        };

        match action {
            1 => {
                if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(&id) {
                    s.dict.remove(b"Filter");
                    s.dict.remove(b"DecodeParms");
                    s.content.clear();
                    count += 1;
                }
            }
            2 => {
                // Re-decompress double-compressed content, then store re-compressed.
                let double_dec = if let Some(Object::Stream(s)) = doc.objects.get(&id) {
                    s.decompressed_content().ok().and_then(|dec1| {
                        let mut dec2 = Vec::new();
                        let mut decoder = ZlibDecoder::new(dec1.as_slice());
                        decoder.read_to_end(&mut dec2).ok().map(|_| dec2)
                    })
                } else {
                    None
                };
                if let Some(inner) = double_dec {
                    if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(&id) {
                        s.dict.remove(b"Filter");
                        s.dict.remove(b"DecodeParms");
                        s.content = inner;
                        let _ = s.compress();
                        count += 1;
                    }
                }
            }
            _ => {}
        }
    }

    count
}

fn is_single_flate_filter(stream: &lopdf::Stream) -> bool {
    match stream.dict.get(b"Filter").ok() {
        Some(Object::Name(n)) => n == b"FlateDecode" || n == b"Fl",
        Some(Object::Array(arr)) if arr.len() == 1 => {
            matches!(&arr[0], Object::Name(n) if n == b"FlateDecode" || n == b"Fl")
        }
        _ => false,
    }
}

fn strict_flate_decode_fails(stream: &lopdf::Stream) -> bool {
    if stream.content.is_empty() {
        return false;
    }

    let mut decoder = ZlibDecoder::new(stream.content.as_slice());
    let mut decoded = Vec::new();
    decoder.read_to_end(&mut decoded).is_err()
}

/// Strip invalid non-ASCII bytes before the first content token.
///
/// Some malformed content streams start with junk bytes (for example 0x80/0xC2),
/// which validators report as undefined operators (PDF/A 6.2.2:1).
fn fix_invalid_operator_preamble(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = collect_content_stream_ids(doc).into_iter().collect();

    for id in ids {
        let decoded = if let Some(Object::Stream(s)) = doc.objects.get(&id) {
            match s.decompressed_content() {
                Ok(d) => d,
                Err(_) => s.content.clone(),
            }
        } else {
            continue;
        };
        if decoded.is_empty() {
            continue;
        }

        let mut cut = 0usize;
        while cut < decoded.len() {
            let b = decoded[cut];
            if b.is_ascii_whitespace() {
                cut += 1;
                continue;
            }
            if b.is_ascii_alphanumeric()
                || matches!(
                    b,
                    b'/' | b'+'
                        | b'-'
                        | b'.'
                        | b'['
                        | b']'
                        | b'<'
                        | b'>'
                        | b'('
                        | b'%'
                        | b'q'
                        | b'Q'
                )
            {
                break;
            }
            cut += 1;
        }

        if cut == 0 {
            continue;
        }

        let new_content = decoded[cut..].to_vec();
        if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(&id) {
            s.dict.remove(b"Filter");
            s.dict.remove(b"DecodeParms");
            s.content = new_content;
            let _ = s.compress();
            count += 1;
        }
    }

    count
}

/// Fix cumulative q/Q depth across a page's multi-stream content array.
///
/// When a page's /Contents is an array, all streams are concatenated by the
/// renderer, so q operators accumulate across streams. This pass rewrites
/// individual streams to ensure the cumulative depth never exceeds 28.
fn fix_page_content_stream_nesting(doc: &mut Document) -> usize {
    use crate::content_editor::ContentEditor;

    let page_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            if let Object::Dictionary(d) = obj {
                if d.get(b"Type").ok() == Some(&Object::Name(b"Page".to_vec())) {
                    return Some(*id);
                }
            }
            None
        })
        .collect();

    let mut total_removed = 0usize;

    for page_id in page_ids {
        // Collect content stream IDs for this page (array form only).
        let stream_ids: Vec<ObjectId> = {
            let Some(Object::Dictionary(page_dict)) = doc.objects.get(&page_id) else {
                continue;
            };
            match page_dict.get(b"Contents").ok() {
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
                _ => continue, // single stream or missing — handled by per-stream pass
            }
        };

        if stream_ids.len() < 2 {
            continue;
        }

        // Compute cumulative q depth at the END of each stream.
        let mut cum_depths: Vec<i64> = Vec::with_capacity(stream_ids.len());
        let mut cur: i64 = 0;
        let mut max_cum: i64 = 0;
        for id in &stream_ids {
            let delta: i64 = if let Some(Object::Stream(s)) = doc.objects.get(id) {
                let dec = s
                    .decompressed_content()
                    .unwrap_or_else(|_| s.content.clone());
                dec.split(|b| b" \t\r\n".contains(b))
                    .filter(|t| !t.is_empty())
                    .map(|t| {
                        if t == b"q" {
                            1
                        } else if t == b"Q" {
                            -1
                        } else {
                            0
                        }
                    })
                    .sum()
            } else {
                0
            };
            cur += delta;
            max_cum = max_cum.max(cur);
            cum_depths.push(cur);
        }

        if max_cum <= 28 {
            continue; // no fix needed
        }

        // Re-simulate and fix: rewrite streams to cap cumulative depth at 28.
        let mut depth: i64 = 0;
        let mut skipped_q: i64 = 0;

        for id in &stream_ids {
            let decoded = if let Some(Object::Stream(s)) = doc.objects.get(id) {
                s.decompressed_content()
                    .unwrap_or_else(|_| s.content.clone())
            } else {
                continue;
            };

            let editor = match ContentEditor::from_stream(&decoded) {
                Ok(e) => e,
                Err(_) => continue,
            };

            let mut modified = false;
            let mut new_ops = Vec::with_capacity(editor.operations().len());

            for op in editor.operations() {
                match op.operator.as_str() {
                    "q" => {
                        if depth >= 28 {
                            skipped_q += 1;
                            total_removed += 1;
                            modified = true;
                            continue;
                        }
                        depth += 1;
                        new_ops.push(op.clone());
                    }
                    "Q" => {
                        if skipped_q > 0 {
                            skipped_q -= 1;
                            total_removed += 1;
                            modified = true;
                            continue;
                        }
                        depth = (depth - 1).max(0);
                        new_ops.push(op.clone());
                    }
                    _ => new_ops.push(op.clone()),
                }
            }

            if modified {
                let new_content = match ContentEditor::from_operations(new_ops).encode() {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(id) {
                    s.dict.remove(b"Filter");
                    s.dict.remove(b"DecodeParms");
                    s.content = new_content;
                    let _ = s.compress();
                }
            }
        }
    }

    total_removed
}

/// Enforce PDF/A graphics state nesting limit (6.1.13:8).
///
/// veraPDF requires q/Q nesting depth to stay <= 28. For malformed streams
/// with deeper nesting, drop only the overflowing q operators and their
/// corresponding Q operators, preserving valid depth transitions.
fn fix_graphics_state_nesting_limit(doc: &mut Document) -> usize {
    let mut removed_ops = 0usize;

    // Pass 1: fix cumulative depth across multi-stream pages.
    removed_ops += fix_page_content_stream_nesting(doc);

    // Pass 2: fix individual content streams (Form XObjects, etc.).
    let ids: Vec<ObjectId> = collect_content_stream_ids(doc).into_iter().collect();

    for id in ids {
        let decoded = if let Some(Object::Stream(s)) = doc.objects.get(&id) {
            match s.decompressed_content() {
                Ok(d) => d,
                Err(_) => s.content.clone(),
            }
        } else {
            continue;
        };
        if !decoded.iter().any(|b| *b == b'q' || *b == b'Q') {
            continue;
        }

        let editor = match crate::content_editor::ContentEditor::from_stream(&decoded) {
            Ok(e) => e,
            Err(_) => continue,
        };

        let mut depth = 0usize;
        let mut skipped_q = 0usize;
        let mut modified = false;
        let mut new_ops = Vec::with_capacity(editor.operations().len());

        for op in editor.operations() {
            match op.operator.as_str() {
                "q" => {
                    if depth >= 28 {
                        skipped_q += 1;
                        removed_ops += 1;
                        modified = true;
                        continue;
                    }
                    depth += 1;
                    new_ops.push(op.clone());
                }
                "Q" => {
                    if skipped_q > 0 {
                        skipped_q -= 1;
                        removed_ops += 1;
                        modified = true;
                        continue;
                    }
                    depth = depth.saturating_sub(1);
                    new_ops.push(op.clone());
                }
                _ => new_ops.push(op.clone()),
            }
        }

        if !modified {
            continue;
        }

        let new_content =
            match crate::content_editor::ContentEditor::from_operations(new_ops).encode() {
                Ok(v) => v,
                Err(_) => continue,
            };

        if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(&id) {
            s.dict.remove(b"Filter");
            s.dict.remove(b"DecodeParms");
            s.content = new_content;
            let _ = s.compress();
        }
    }

    removed_ops
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

/// Replace non-finite numeric tokens in content streams with `0`.
///
/// Some generators emit `NaN`, `Infinity`, or `-Infinity` where ISO 32000-1
/// expects a numeric operand. Validators then treat those tokens as undefined
/// operators (for example `NaN w`). Sanitize only standalone tokens outside
/// of strings/comments so the surrounding operator sequence remains valid.
fn fix_non_finite_numbers_in_streams(doc: &mut Document) -> usize {
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

        if !decompressed.windows(3).any(|w| w == b"NaN")
            && !decompressed.windows(8).any(|w| w == b"Infinity")
        {
            continue;
        }

        let mut new_content = Vec::with_capacity(decompressed.len());
        let mut i = 0usize;
        let mut fixed_any = false;
        let mut string_depth = 0usize;
        let mut escape = false;
        let mut in_hex_string = false;
        let mut in_comment = false;

        while i < decompressed.len() {
            let b = decompressed[i];

            if in_comment {
                new_content.push(b);
                if b == b'\n' || b == b'\r' {
                    in_comment = false;
                }
                i += 1;
                continue;
            }

            if string_depth > 0 {
                new_content.push(b);
                if escape {
                    escape = false;
                } else {
                    match b {
                        b'\\' => escape = true,
                        b'(' => string_depth += 1,
                        b')' => string_depth = string_depth.saturating_sub(1),
                        _ => {}
                    }
                }
                i += 1;
                continue;
            }

            if in_hex_string {
                new_content.push(b);
                if b == b'>' {
                    in_hex_string = false;
                }
                i += 1;
                continue;
            }

            match b {
                b'%' => {
                    in_comment = true;
                    new_content.push(b);
                    i += 1;
                    continue;
                }
                b'(' => {
                    string_depth = 1;
                    new_content.push(b);
                    i += 1;
                    continue;
                }
                b'<' if i + 1 < decompressed.len() && decompressed[i + 1] != b'<' => {
                    in_hex_string = true;
                    new_content.push(b);
                    i += 1;
                    continue;
                }
                _ => {}
            }

            if let Some(token_len) = non_finite_number_token_len(&decompressed, i) {
                let prev = if i == 0 {
                    None
                } else {
                    Some(decompressed[i - 1])
                };
                let next = decompressed.get(i + token_len).copied();
                let prev_ok = prev.is_none_or(is_non_finite_token_delimiter);
                let next_ok = next.is_none_or(is_non_finite_token_delimiter);
                let prev_is_name = prev == Some(b'/');
                if prev_ok && next_ok && !prev_is_name {
                    new_content.push(b'0');
                    i += token_len;
                    count += 1;
                    fixed_any = true;
                    continue;
                }
            }

            new_content.push(b);
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

fn non_finite_number_token_len(bytes: &[u8], start: usize) -> Option<usize> {
    let tail = &bytes[start..];
    if tail.starts_with(b"NaN") {
        return Some(3);
    }
    if tail.starts_with(b"+Infinity") || tail.starts_with(b"-Infinity") {
        return Some(9);
    }
    if tail.starts_with(b"Infinity") {
        return Some(8);
    }
    None
}

fn is_non_finite_token_delimiter(b: u8) -> bool {
    b.is_ascii_whitespace()
        || matches!(
            b,
            b'[' | b']' | b'<' | b'>' | b'(' | b')' | b'/' | b'{' | b'}' | b'%'
        )
}

// ---------------------------------------------------------------------------
// 6.2.8.3 — JPEG2000 constraints:
//   - test 1: nrColorChannels must be 1, 3, or 4
//   - test 4: forbidden enumerated colour space 19 (CIEJab)
// ---------------------------------------------------------------------------

fn is_jpx_filter(obj: &Object) -> bool {
    match obj {
        Object::Name(n) => n == b"JPXDecode",
        Object::Array(arr) => arr
            .iter()
            .any(|o| matches!(o, Object::Name(n) if n == b"JPXDecode")),
        _ => false,
    }
}

/// Parse number of channels from JP2 header (`ihdr`) or codestream SIZ marker.
fn jpx_channel_count(data: &[u8]) -> Option<u16> {
    // Prefer JP2 box `ihdr` when present.
    let mut pos = 0usize;
    while pos + 8 <= data.len() {
        let lbox = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]);
        let box_type = &data[pos + 4..pos + 8];
        let (box_len, header_len) = if lbox == 1 {
            if pos + 16 > data.len() {
                break;
            }
            let xl = u64::from_be_bytes([
                data[pos + 8],
                data[pos + 9],
                data[pos + 10],
                data[pos + 11],
                data[pos + 12],
                data[pos + 13],
                data[pos + 14],
                data[pos + 15],
            ]) as usize;
            (xl, 16usize)
        } else if lbox == 0 {
            (data.len() - pos, 8usize)
        } else {
            (lbox as usize, 8usize)
        };

        if box_len < header_len || pos + box_len > data.len() {
            break;
        }

        if box_type == b"ihdr" {
            let payload = pos + header_len;
            if payload + 10 <= data.len() {
                return Some(u16::from_be_bytes([data[payload + 8], data[payload + 9]]));
            }
        }

        if box_len == 0 {
            break;
        }
        pos += box_len;
    }

    // Fallback: find codestream SIZ marker (FF51) and read Csiz.
    let mut i = 0usize;
    while i + 3 < data.len() {
        if data[i] == 0xFF && data[i + 1] == 0x51 {
            let lsiz = u16::from_be_bytes([data[i + 2], data[i + 3]]) as usize;
            if lsiz >= 38 && i + 2 + lsiz <= data.len() {
                let csiz_pos = i + 4 + 2 + 32; // after Lsiz + Rsiz + 8x u32 geometry fields
                if csiz_pos + 1 < i + 2 + lsiz {
                    return Some(u16::from_be_bytes([data[csiz_pos], data[csiz_pos + 1]]));
                }
            }
        }
        i += 1;
    }
    None
}

/// Replace invalid JPX image stream with a minimal DeviceGray image placeholder.
fn replace_invalid_jpx_with_placeholder(stream: &mut lopdf::Stream) {
    stream.dict.set("Width", Object::Integer(1));
    stream.dict.set("Height", Object::Integer(1));
    stream.dict.set("BitsPerComponent", Object::Integer(8));
    stream
        .dict
        .set("ColorSpace", Object::Name(b"DeviceGray".to_vec()));
    stream.dict.remove(b"SMask");
    stream.dict.remove(b"Mask");
    stream.dict.remove(b"SMaskInData");
    stream.dict.remove(b"Decode");
    stream.dict.remove(b"DecodeParms");
    stream.dict.remove(b"ImageMask");
    stream.dict.remove(b"Filter");
    stream.content = vec![255u8];
}

fn fix_jpx_forbidden_colorspaces(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();

    for id in ids {
        let is_jpx = if let Some(Object::Stream(s)) = doc.objects.get(&id) {
            s.dict.get(b"Filter").ok().is_some_and(is_jpx_filter)
        } else {
            false
        };
        if !is_jpx {
            continue;
        }

        if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(&id) {
            // PDF/A-2 6.2.8.3:1: allowed JPX channel counts are 1, 3 or 4.
            // If the codestream contains an unsupported channel count, replace
            // the stream with a minimal non-JPX image to keep the file compliant.
            if let Some(channels) = jpx_channel_count(&s.content) {
                if channels != 1 && channels != 3 && channels != 4 {
                    replace_invalid_jpx_with_placeholder(s);
                    count += 1;
                    continue;
                }
            }

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
    // Only process actual content streams. Scanning arbitrary binary streams
    // (font programs, images, ICC profiles) can corrupt embedded resources.
    let ids: Vec<ObjectId> = collect_content_stream_ids(doc).into_iter().collect();

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
                i += 1;
                let content_start = i;
                while i < decompressed.len() && decompressed[i] != b'>' {
                    i += 1;
                }
                let content_end = i;
                if i < decompressed.len() {
                    i += 1; // skip '>'
                }
                let raw = &decompressed[content_start..content_end];

                // Collect only valid hex digits and whitespace; strip anything else.
                let mut hex_chars: Vec<u8> = raw
                    .iter()
                    .filter(|&&b| b.is_ascii_hexdigit() || b.is_ascii_whitespace())
                    .copied()
                    .collect();

                // Count non-whitespace hex chars.
                let hex_count = hex_chars
                    .iter()
                    .filter(|&&b| !b.is_ascii_whitespace())
                    .count();

                // Detect if anything changed (invalid chars removed or odd count).
                let had_invalid = hex_chars.len() != raw.len();
                let is_odd = hex_count % 2 != 0;

                if had_invalid || is_odd {
                    // Pad if odd.
                    if is_odd {
                        // Insert '0' at end (before any trailing whitespace).
                        hex_chars.push(b'0');
                    }
                    new_content.push(b'<');
                    new_content.extend_from_slice(&hex_chars);
                    new_content.push(b'>');
                    count += 1;
                    fixed_any = true;
                } else {
                    // Unchanged — copy original bytes verbatim.
                    new_content.push(b'<');
                    new_content.extend_from_slice(raw);
                    new_content.push(b'>');
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

// ---------------------------------------------------------------------------
// 6.2.2:1 / 6.2.2:2 — Strip unknown/non-ISO content stream operators
// ---------------------------------------------------------------------------
//
// Some PDFs (especially fuzzer-generated or vendor-specific) contain operator
// tokens that are not defined in ISO 32000-1. veraPDF fails 6.2.2:1 ("operator
// not defined") and 6.2.2:2 ("undefined keyword") for these.
//
// Strategy: tokenise each content stream; emit only tokens that are either
// operands or valid ISO 32000-1 operators. Discard unknown operator tokens
// along with their preceding operands. BX/EX compatibility sections are passed
// through verbatim. Inline-image blocks (BI…EI) are treated as opaque.

/// All operator tokens defined in ISO 32000-1 §8 (content stream operators).
const ISO32000_OPERATORS: &[&[u8]] = &[
    // Graphics state
    b"q", b"Q", b"cm", b"w", b"J", b"j", b"M", b"d", b"ri", b"i", b"gs",
    // Path construction
    b"m", b"l", b"c", b"v", b"y", b"h", b"re", // Path painting
    b"S", b"s", b"F", b"f", b"f*", b"B", b"B*", b"b", b"b*", b"n", // Clipping
    b"W", b"W*", // Text objects
    b"BT", b"ET", // Text state
    b"Tc", b"Tw", b"Tz", b"TL", b"Tf", b"Tr", b"Ts", // Text positioning
    b"Td", b"TD", b"Tm", b"T*", // Text showing
    b"Tj", b"TJ", b"'", b"\"", // Type 3
    b"d0", b"d1", // Colour space / colour
    b"CS", b"cs", b"SC", b"SCN", b"sc", b"scn", b"G", b"g", b"RG", b"rg", b"K", b"k",
    // Shading
    b"sh",
    // Inline images (BI/ID/EI handled specially, but list them so they're not stripped)
    b"BI", b"ID", b"EI", // XObjects
    b"Do", // Marked content
    b"MP", b"DP", b"BMC", b"BDC", b"EMC", // Compatibility
    b"BX", b"EX",
];

fn strip_unknown_content_stream_operators(doc: &mut Document) -> usize {
    let ids: Vec<ObjectId> = collect_content_stream_ids(doc).into_iter().collect();
    let mut count = 0;

    for id in ids {
        let decoded = if let Some(Object::Stream(s)) = doc.objects.get(&id) {
            match s.decompressed_content() {
                Ok(d) => d,
                Err(_) => s.content.clone(),
            }
        } else {
            continue;
        };

        if let Some(new_content) = strip_unknown_ops_in_stream(&decoded) {
            if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(&id) {
                s.dict.remove(b"Filter");
                s.dict.remove(b"DecodeParms");
                s.content = new_content;
                let _ = s.compress();
                count += 1;
            }
        }
    }
    count
}

/// Returns `Some(new_bytes)` if any unknown operators were removed, else `None`.
fn strip_unknown_ops_in_stream(data: &[u8]) -> Option<Vec<u8>> {
    let mut out: Vec<u8> = Vec::with_capacity(data.len());
    // Pending buffer: operands (and whitespace) accumulated since the last
    // emitted operator.  Flushed on a known operator, discarded on unknown.
    let mut pending: Vec<u8> = Vec::new();
    let mut i = 0;
    let mut modified = false;
    // BX/EX nesting depth: inside BX…EX all operators are considered valid.
    let mut bx_depth: u32 = 0;

    while i < data.len() {
        // ── Whitespace → accumulate in pending ───────────────────────────────
        if data[i].is_ascii_whitespace() {
            pending.push(data[i]);
            i += 1;
            continue;
        }

        // ── Comment % … EOL → accumulate in pending ──────────────────────────
        if data[i] == b'%' {
            let start = i;
            while i < data.len() && data[i] != b'\n' && data[i] != b'\r' {
                i += 1;
            }
            pending.extend_from_slice(&data[start..i]);
            continue;
        }

        // ── Literal string (…) → operand ─────────────────────────────────────
        if data[i] == b'(' {
            let start = i;
            i += 1;
            let mut depth: u32 = 1;
            while i < data.len() && depth > 0 {
                match data[i] {
                    b'\\' => {
                        i += 1;
                        if i < data.len() {
                            i += 1;
                        }
                    }
                    b'(' => {
                        depth += 1;
                        i += 1;
                    }
                    b')' => {
                        depth -= 1;
                        i += 1;
                    }
                    _ => {
                        i += 1;
                    }
                }
            }
            pending.extend_from_slice(&data[start..i]);
            continue;
        }

        // ── Dict << … >> → operand ───────────────────────────────────────────
        if data[i] == b'<' && i + 1 < data.len() && data[i + 1] == b'<' {
            let start = i;
            i += 2;
            let mut depth: u32 = 1;
            while i + 1 < data.len() && depth > 0 {
                if data[i] == b'<' && data[i + 1] == b'<' {
                    depth += 1;
                    i += 2;
                } else if data[i] == b'>' && data[i + 1] == b'>' {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
            if depth > 0 {
                i = data.len();
            }
            pending.extend_from_slice(&data[start..i]);
            continue;
        }

        // ── Hex string <…> → operand ─────────────────────────────────────────
        if data[i] == b'<' {
            let start = i;
            i += 1;
            while i < data.len() && data[i] != b'>' {
                i += 1;
            }
            if i < data.len() {
                i += 1;
            }
            pending.extend_from_slice(&data[start..i]);
            continue;
        }

        // ── Name /… → operand ────────────────────────────────────────────────
        if data[i] == b'/' {
            let start = i;
            i += 1;
            while i < data.len() && !is_pdf_delimiter(data[i]) {
                i += 1;
            }
            pending.extend_from_slice(&data[start..i]);
            continue;
        }

        // ── Array […] → operand ──────────────────────────────────────────────
        if data[i] == b'[' {
            let start = i;
            i += 1;
            let mut depth: u32 = 1;
            while i < data.len() && depth > 0 {
                match data[i] {
                    b'[' => {
                        depth += 1;
                        i += 1;
                    }
                    b']' => {
                        depth -= 1;
                        i += 1;
                    }
                    b'(' => {
                        // literal string inside array
                        i += 1;
                        let mut sd: u32 = 1;
                        while i < data.len() && sd > 0 {
                            match data[i] {
                                b'\\' => {
                                    i += 1;
                                    if i < data.len() {
                                        i += 1;
                                    }
                                }
                                b'(' => {
                                    sd += 1;
                                    i += 1;
                                }
                                b')' => {
                                    sd -= 1;
                                    i += 1;
                                }
                                _ => {
                                    i += 1;
                                }
                            }
                        }
                    }
                    _ => {
                        i += 1;
                    }
                }
            }
            pending.extend_from_slice(&data[start..i]);
            continue;
        }

        // ── Number: digit / +digit / -digit / .digit → operand ───────────────
        if data[i].is_ascii_digit()
            || ((data[i] == b'+' || data[i] == b'-')
                && i + 1 < data.len()
                && (data[i + 1].is_ascii_digit() || data[i + 1] == b'.'))
            || (data[i] == b'.' && i + 1 < data.len() && data[i + 1].is_ascii_digit())
        {
            let start = i;
            i += 1;
            while i < data.len()
                && (data[i].is_ascii_digit()
                    || data[i] == b'.'
                    || data[i] == b'e'
                    || data[i] == b'E')
            {
                i += 1;
            }
            pending.extend_from_slice(&data[start..i]);
            continue;
        }

        // ── Keyword / operator token: scan to next delimiter ─────────────────
        let tok_start = i;
        while i < data.len() && !is_pdf_delimiter(data[i]) {
            i += 1;
        }
        if i == tok_start {
            // Single unrecognised delimiter character — skip it silently.
            i += 1;
            continue;
        }
        let token = &data[tok_start..i];

        // Boolean / null → operand
        if token == b"true" || token == b"false" || token == b"null" {
            pending.extend_from_slice(token);
            continue;
        }

        // ── Inline image block BI … ID … EI → pass through opaque ────────────
        if token == b"BI" {
            // Flush pending operands then copy the whole BI…EI block verbatim.
            out.extend_from_slice(&pending);
            pending.clear();
            out.extend_from_slice(b"BI");
            // Scan to ID marker (whitespace-preceded).
            let mut found_ei = false;
            while i < data.len() {
                // Look for whitespace + "ID"
                if (data[i] == b' ' || data[i] == b'\n' || data[i] == b'\r' || data[i] == b'\t')
                    && i + 3 <= data.len()
                    && &data[i + 1..i + 3] == b"ID"
                    && (i + 3 >= data.len()
                        || data[i + 3] == b' '
                        || data[i + 3] == b'\n'
                        || data[i + 3] == b'\r')
                {
                    out.extend_from_slice(&data[tok_start + 2..i + 3]); // dict + "ID"
                    i += 3;
                    // Now scan binary data until EI
                    while i < data.len() {
                        if (data[i] == b'\n' || data[i] == b' ' || data[i] == b'\r')
                            && i + 3 <= data.len()
                            && &data[i + 1..i + 3] == b"EI"
                            && (i + 3 >= data.len()
                                || data[i + 3].is_ascii_whitespace()
                                || data[i + 3] == b'Q')
                        {
                            out.extend_from_slice(&data[i..i + 3]);
                            i += 3;
                            found_ei = true;
                            break;
                        }
                        out.push(data[i]);
                        i += 1;
                    }
                    break;
                }
                out.push(data[i]);
                i += 1;
            }
            if !found_ei {
                // Malformed: no EI found; continue from current position.
            }
            continue;
        }

        // ── BX / EX compatibility markers ────────────────────────────────────
        if token == b"BX" {
            bx_depth += 1;
            out.extend_from_slice(&pending);
            pending.clear();
            out.extend_from_slice(b"BX");
            continue;
        }
        if token == b"EX" {
            bx_depth = bx_depth.saturating_sub(1);
            out.extend_from_slice(&pending);
            pending.clear();
            out.extend_from_slice(b"EX");
            continue;
        }

        // ── Operator: check validity ──────────────────────────────────────────
        let is_valid = bx_depth > 0
            || token.iter().all(|b| b.is_ascii()) // only strip tokens with all-ASCII bytes
               && ISO32000_OPERATORS.contains(&token);

        if is_valid {
            out.extend_from_slice(&pending);
            pending.clear();
            out.extend_from_slice(token);
        } else if token.iter().all(|b| b.is_ascii()) {
            // Unknown ASCII operator: discard it and its pending operands.
            modified = true;
            pending.clear();
            // Ensure token separation after the discard.
            out.push(b'\n');
        } else {
            // Contains non-ASCII bytes: likely binary garbage in content
            // stream. Pass through unchanged to avoid corrupting binary data.
            pending.extend_from_slice(token);
        }
    }

    // Flush any remaining pending operands (orphaned at end of stream).
    out.extend_from_slice(&pending);

    if modified {
        Some(out)
    } else {
        None
    }
}

/// Returns true if `b` is a PDF token delimiter character.
#[inline]
fn is_pdf_delimiter(b: u8) -> bool {
    matches!(
        b,
        b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%'
    ) || b.is_ascii_whitespace()
}

// ---------------------------------------------------------------------------
// 6.1.13:11 — Page boundary dimensions must be >= 3 and <= 14400 user units.
// ISO 19005-2:2011 §6.1.13 test 11: "The size of any of the page boundaries
// shall not be less than 3 units in either direction, nor shall it be greater
// than 14 400 units in either direction."
// Applies to MediaBox, CropBox, BleedBox, TrimBox, ArtBox.
// ---------------------------------------------------------------------------

pub(crate) fn fix_page_boundary_sizes(doc: &mut Document) -> usize {
    const MIN_DIM: f64 = 3.0;
    const MAX_DIM: f64 = 14400.0;

    let box_keys: &[&[u8]] = &[b"MediaBox", b"CropBox", b"BleedBox", b"TrimBox", b"ArtBox"];

    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut count = 0;

    for id in ids {
        let Some(Object::Dictionary(dict)) = doc.objects.get(&id) else {
            continue;
        };

        // Only process Page objects (and Pages that can inherit boxes).
        let type_name = dict
            .get(b"Type")
            .ok()
            .and_then(|o| o.as_name().ok())
            .map(|n| n.to_vec());
        let is_page = matches!(type_name.as_deref(), Some(b"Page") | Some(b"Pages"));
        if !is_page {
            continue;
        }

        let mut changed = false;
        let mut new_dict = dict.clone();

        for key in box_keys {
            let Ok(Object::Array(arr)) = new_dict.get(key) else {
                continue;
            };
            if arr.len() != 4 {
                continue;
            }

            // Parse [llx lly urx ury].
            let mut vals = [0.0f64; 4];
            let mut parseable = true;
            for (i, obj) in arr.iter().enumerate() {
                match obj {
                    Object::Integer(v) => vals[i] = *v as f64,
                    Object::Real(v) => vals[i] = *v as f64,
                    _ => {
                        parseable = false;
                        break;
                    }
                }
            }
            if !parseable {
                continue;
            }

            let llx = vals[0].min(vals[2]);
            let lly = vals[1].min(vals[3]);
            let urx = vals[0].max(vals[2]);
            let ury = vals[1].max(vals[3]);

            let width = urx - llx;
            let height = ury - lly;

            if (MIN_DIM..=MAX_DIM).contains(&width) && (MIN_DIM..=MAX_DIM).contains(&height) {
                continue; // Already valid.
            }

            // Compute new urx/ury while keeping llx/lly fixed.
            let new_width = width.clamp(MIN_DIM, MAX_DIM);
            let new_height = height.clamp(MIN_DIM, MAX_DIM);
            let new_urx = llx + new_width;
            let new_ury = lly + new_height;

            new_dict.set(
                *key,
                Object::Array(vec![
                    Object::Real(llx as f32),
                    Object::Real(lly as f32),
                    Object::Real(new_urx as f32),
                    Object::Real(new_ury as f32),
                ]),
            );
            changed = true;
        }

        if changed {
            doc.objects.insert(id, Object::Dictionary(new_dict));
            count += 1;
        }
    }
    count
}

// ---------------------------------------------------------------------------
// 6.1.8:1 — Fix non-ASCII bytes in PDF name objects.
//
// PDF/A-1b requires that all PDF name values represent valid UTF-8 sequences.
// Names that contain raw non-ASCII bytes (e.g. from Chinese/Japanese font names
// like YYAAAA+\xCB\xCE\xCC\xE5) fail this check.
//
// Fix: replace each byte > 127 with its two-character ASCII hex representation
// so that \xCB becomes the ASCII string "CB". The resulting name is pure ASCII
// and therefore valid UTF-8. This is done consistently for all Name objects in
// the document (dictionaries and arrays), excluding stream content.
// ---------------------------------------------------------------------------

fn fix_non_ascii_pdf_names(doc: &mut Document) -> usize {
    /// Sanitize a single name: replace bytes > 127 with uppercase hex ASCII.
    fn sanitize_name(name: &[u8]) -> Option<Vec<u8>> {
        if name.iter().all(|&b| b <= 127) {
            return None; // already ASCII-clean
        }
        let mut out = Vec::with_capacity(name.len() * 2);
        for &b in name {
            if b > 127 {
                out.push(b"0123456789ABCDEF"[(b >> 4) as usize]);
                out.push(b"0123456789ABCDEF"[(b & 0xf) as usize]);
            } else {
                out.push(b);
            }
        }
        Some(out)
    }

    fn fix_obj(obj: &mut Object) -> usize {
        match obj {
            Object::Name(n) => {
                if let Some(fixed) = sanitize_name(n) {
                    *n = fixed;
                    1
                } else {
                    0
                }
            }
            Object::Array(arr) => arr.iter_mut().map(fix_obj).sum(),
            Object::Dictionary(dict) => dict.iter_mut().map(|(_, v)| fix_obj(v)).sum(),
            // Do not recurse into streams — stream content is binary and must
            // not be modified here. Stream *dictionaries* are handled separately.
            Object::Stream(s) => s.dict.iter_mut().map(|(_, v)| fix_obj(v)).sum(),
            _ => 0,
        }
    }

    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut count = 0;
    for id in ids {
        if let Some(obj) = doc.objects.get_mut(&id) {
            count += fix_obj(obj);
        }
    }
    count
}
