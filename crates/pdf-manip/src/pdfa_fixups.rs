//! Supplementary PDF/A compliance fixes.
//!
//! Additional passes that address remaining veraPDF rule failures
//! not fully covered by pdfa_cleanup or pdfa_fonts modules.

use flate2::read::ZlibDecoder;
use lopdf::{dictionary, Document, Object, ObjectId};
use std::io::{Read, Write};

/// Run all supplementary PDF/A fixups.
pub fn run_fixups(doc: &mut Document) -> FixupReport {
    let standard_encoding_fixed = fix_standard_encoding(doc);
    let tt_encoding_diffs_fixed = fix_truetype_encoding_differences(doc);
    let devicen_colorants_fixed = fix_devicen_colorants(doc);
    let forbidden_annots_removed = fix_forbidden_annotations_extra(doc);
    let forbidden_actions_removed = fix_forbidden_actions(doc);
    let _ = forbidden_actions_removed;
    let annotation_opacity_fixed = fix_annotation_opacity(doc);
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
    let inline_f_expanded = fix_inline_image_f_abbrev(doc);
    let _ = inline_f_expanded;
    let names_ef_removed = fix_names_embedded_files(doc);
    let _ = names_ef_removed;
    let postscript_xobjects_removed = fix_postscript_xobjects(doc);
    let reference_xobjects_removed = fix_reference_xobjects(doc);
    let overflow_integers_fixed = fix_overflow_integers(doc);
    let overflow_reals_fixed = fix_overflow_reals(doc);
    let long_strings_fixed = fix_long_strings(doc);
    let jbig2_globals_fixed = fix_jbig2_globals(doc);
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
    let long_names_in_streams_fixed = fix_long_names_in_streams(doc);
    let long_dict_keys_fixed = fix_long_dict_keys(doc);
    let long_strings_in_streams_fixed = fix_long_strings_in_streams(doc);
    let invalid_lang_fixed = fix_invalid_lang_values(doc);
    let inline_image_interpolate_fixed = fix_inline_image_interpolate(doc);
    // Re-encode ASCII85 inline images as FlateDecode-only before stripping unknown operators.
    // veraPDF uses strict EI detection and can find false EI markers within ASCII85-encoded
    // data (printable ASCII), causing it to treat subsequent bytes as content stream operators.
    // Converting to FlateDecode (binary) eliminates the false EI detection. (#fix-ascii85-inline)
    let ascii85_inline_images_fixed = fix_ascii85_inline_images(doc);
    let lzw_inline_images_fixed = fix_lzw_inline_images(doc);
    // Second pass: catch any binary inline images introduced by intermediate
    // fixups (the primary pass runs at the start of cleanup_for_pdfa).
    fix_binary_inline_image_ei(doc);
    let invalid_ri_fixed = fix_invalid_rendering_intents(doc);
    let opm_fixed = fix_extgstate_opm(doc);
    // §6.4.2: Fix SMask dicts with invalid /S (must be Alpha or Luminosity).
    let smask_subtype_fixed = fix_extgstate_smask_subtype(doc);
    // §6.4.1: Normalize non-standard blend mode names (case-insensitive match).
    let blend_mode_fixed = fix_extgstate_blend_modes(doc);
    let concatenated_operators_fixed =
        fix_concatenated_operators(doc) + invalid_ri_fixed + opm_fixed;
    let unknown_operators_stripped = strip_unknown_content_stream_operators(doc);
    let page_boundary_fixed = fix_page_boundary_sizes(doc);
    // §6.1.13: Truncate oversized dictionaries (>4095) and arrays (>8191).
    let long_containers_fixed = fix_long_containers(doc);
    // §6.1.8: Ensure names are valid UTF-8 (or at least valid PDF names).
    let non_utf8_names_fixed = fix_non_utf8_names(doc);
    // §6.3.3: Ensure annotations have appearance streams.
    let annot_ap_fixed = fix_missing_annot_appearances_extra(doc);
    // §6.1.6: Remove non-hex characters from hex strings.
    let hex_garbage_fixed = fix_hex_string_garbage(doc);
    // §6.1.7.1: Remove forbidden external file references from stream dicts.
    let stream_external_f_fixed = fix_stream_external_ref_keys_extra(doc);
    // §6.1.6.2: Promote inline JBIG2Globals to indirect objects and move from DecodeParms.
    let jbig2_globals_promoted = fix_jbig2_globals_promotion(doc);
    // §6.2.4.3: Fix DeviceCMYK usage when OutputIntent is not CMYK.
    let device_cmyk_intent_fixed = fix_device_cmyk_intent_mismatch(doc);
    // §6.2.4.2: Fix ICC profile reuse between ICCBased and OutputIntent.
    let icc_profile_reuse_fixed = fix_icc_profile_reuse(doc);
    // Add /Group to pages using transparency without one (6.2.10-tgroup).
    // Runs after normalize_colorspaces has already added the OutputIntent, so
    // /Group << /S /Transparency >> without /CS is valid. (#496)
    let transparency_groups_added = fix_missing_transparency_groups(doc);
    // Re-balance BMC/BDC/EMC after content stream modifications.
    // Earlier cleanup already balanced them, but strip_unknown_content_stream_operators
    // and other content stream fixups above can remove operators within marked content
    // sequences, leaving BMC/BDC without matching EMC. Run the fix again here.
    crate::pdfa_cleanup::fix_unbalanced_emc(doc);
    // fix_stream_lengths must be LAST — after all other fixes that may modify streams.
    let font_type_fixed = fix_font_type_entries(doc);
    let form_xobject_bbox_fixed = fix_form_xobject_bbox(doc);
    let stream_lengths_fixed = fix_stream_lengths(doc);

    FixupReport {
        standard_encoding_fixed,
        font_type_fixed,
        form_xobject_bbox_fixed,
        tt_encoding_diffs_fixed,
        devicen_colorants_fixed,
        forbidden_annots_removed,
        annotation_opacity_fixed,
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
        overflow_reals_fixed,
        long_strings_fixed,
        operator_spacing_fixed,
        tiny_floats_fixed,
        odd_hex_strings_fixed,
        non_ascii_names_fixed,
        long_names_in_streams_fixed,
        long_dict_keys_fixed,
        long_strings_in_streams_fixed,
        invalid_lang_fixed,
        inline_image_interpolate_fixed,
        ascii85_inline_images_fixed,
        lzw_inline_images_fixed,
        jbig2_globals_fixed,
        jpx_colorspace_fixed,
        concatenated_operators_fixed,
        unknown_operators_stripped,
        page_boundary_fixed,
        transparency_groups_added,
        smask_subtype_fixed,
        blend_mode_fixed,
        long_containers_fixed,
        non_utf8_names_fixed,
        annot_ap_fixed,
        hex_garbage_fixed,
        stream_external_f_fixed,
        jbig2_globals_promoted,
        device_cmyk_intent_fixed,
        icc_profile_reuse_fixed,
    }
}

/// Report from supplementary fixups.
#[derive(Debug, Clone, Default)]
pub struct FixupReport {
    pub standard_encoding_fixed: usize,
    pub tt_encoding_diffs_fixed: usize,
    pub devicen_colorants_fixed: usize,
    pub forbidden_annots_removed: usize,
    pub annotation_opacity_fixed: usize,
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
    pub overflow_reals_fixed: usize,
    pub long_strings_fixed: usize,
    pub operator_spacing_fixed: usize,
    pub tiny_floats_fixed: usize,
    pub odd_hex_strings_fixed: usize,
    pub non_ascii_names_fixed: usize,
    pub long_names_in_streams_fixed: usize,
    pub long_dict_keys_fixed: usize,
    pub long_strings_in_streams_fixed: usize,
    pub invalid_lang_fixed: usize,
    pub inline_image_interpolate_fixed: usize,
    pub ascii85_inline_images_fixed: usize,
    pub lzw_inline_images_fixed: usize,
    pub jbig2_globals_fixed: usize,
    pub jpx_colorspace_fixed: usize,
    pub concatenated_operators_fixed: usize,
    pub unknown_operators_stripped: usize,
    pub page_boundary_fixed: usize,
    pub transparency_groups_added: usize,
    pub smask_subtype_fixed: usize,
    pub blend_mode_fixed: usize,
    pub font_type_fixed: usize,
    pub form_xobject_bbox_fixed: usize,
    pub long_containers_fixed: usize,
    pub non_utf8_names_fixed: usize,
    pub annot_ap_fixed: usize,
    pub hex_garbage_fixed: usize,
    pub stream_external_f_fixed: usize,
    pub jbig2_globals_promoted: usize,
}

// ---------------------------------------------------------------------------
// 6.2.11.6 — Replace /StandardEncoding with /WinAnsiEncoding
// ---------------------------------------------------------------------------
//
// PDF/A §6.2.11.6 only allows /WinAnsiEncoding and /MacRomanEncoding as
// BaseEncoding for non-symbolic simple fonts.  /StandardEncoding is forbidden.
// This pass replaces it everywhere — both as a direct Encoding name and as a
// BaseEncoding value inside an Encoding dictionary.

fn fix_standard_encoding(doc: &mut Document) -> usize {
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut count = 0;

    for id in ids {
        let action = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&id) else {
                continue;
            };
            // Only simple fonts (Type1, TrueType, MMType1).
            let subtype = get_name_val(dict, b"Subtype");
            if !matches!(
                subtype.as_deref(),
                Some("Type1") | Some("TrueType") | Some("MMType1")
            ) {
                continue;
            }
            // For direct /Encoding /StandardEncoding (Name), skip symbolic
            // subset fonts — changing the entire encoding mapping is risky
            // when the subset may lack glyphs expected by WinAnsiEncoding.
            // But for Encoding *dicts* with /BaseEncoding /StandardEncoding,
            // ALWAYS replace: PDF/A §6.2.11.6 requires BaseEncoding =
            // WinAnsiEncoding or MacRomanEncoding regardless of symbolic
            // status or subset. TeX CM fonts (CMSS, CMBX, CMMI) are flagged
            // symbolic but still use StandardEncoding in their Encoding dict.
            // The Differences array already covers all used codes, so changing
            // only the BaseEncoding is safe.
            let bf = get_name_val(dict, b"BaseFont").unwrap_or_default();
            let is_subset = bf.len() > 7 && bf.as_bytes()[6] == b'+';
            let symbolic = is_symbolic(doc, dict);
            match dict.get(b"Encoding").ok() {
                // /Encoding /StandardEncoding — skip symbolic subsets
                Some(Object::Name(n)) if n == b"StandardEncoding" => {
                    if symbolic && is_subset {
                        StdEncAction::None
                    } else {
                        StdEncAction::ReplaceName
                    }
                }
                // /Encoding << /BaseEncoding /StandardEncoding ... >> — always fix
                Some(Object::Dictionary(enc)) => {
                    if matches!(enc.get(b"BaseEncoding").ok(), Some(Object::Name(n)) if n == b"StandardEncoding")
                    {
                        StdEncAction::ReplaceInlineBase
                    } else {
                        StdEncAction::None
                    }
                }
                // /Encoding is an indirect reference — always fix BaseEncoding
                Some(Object::Reference(enc_id)) => match doc.objects.get(enc_id) {
                    Some(Object::Dictionary(enc)) => {
                        if matches!(enc.get(b"BaseEncoding").ok(), Some(Object::Name(n)) if n == b"StandardEncoding")
                        {
                            StdEncAction::ReplaceRefBase(*enc_id)
                        } else {
                            StdEncAction::None
                        }
                    }
                    _ => StdEncAction::None,
                },
                _ => StdEncAction::None,
            }
        };

        match action {
            StdEncAction::None => {}
            StdEncAction::ReplaceName => {
                if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                    dict.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
                    count += 1;
                }
            }
            StdEncAction::ReplaceInlineBase => {
                if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                    if let Ok(Object::Dictionary(ref mut enc)) = dict.get_mut(b"Encoding") {
                        enc.set("BaseEncoding", Object::Name(b"WinAnsiEncoding".to_vec()));
                        fix_differences_128_159(enc);
                        count += 1;
                    }
                }
            }
            StdEncAction::ReplaceRefBase(enc_id) => {
                if let Some(Object::Dictionary(ref mut enc)) = doc.objects.get_mut(&enc_id) {
                    enc.set("BaseEncoding", Object::Name(b"WinAnsiEncoding".to_vec()));
                    fix_differences_128_159(enc);
                    count += 1;
                }
            }
        }
    }
    count
}

/// Remove Differences entries for codes 128-159 that conflict with
/// WinAnsiEncoding. Called after replacing BaseEncoding from
/// StandardEncoding to WinAnsiEncoding.
fn fix_differences_128_159(enc: &mut lopdf::Dictionary) {
    let Some(Object::Array(diffs)) = enc.get(b"Differences").ok().cloned() else {
        return;
    };
    let mut new_diffs: Vec<Object> = Vec::new();
    let mut current_code: i64 = -1;
    let mut skip_next_names = false;
    let mut i = 0;
    while i < diffs.len() {
        match &diffs[i] {
            Object::Integer(n) => {
                current_code = *n;
                skip_next_names = (128..160).contains(&current_code);
                if !skip_next_names {
                    new_diffs.push(diffs[i].clone());
                }
            }
            Object::Name(name) => {
                if skip_next_names && (128..160).contains(&current_code) {
                    let expected = winansi_name_for_code_128_159(current_code as u8);
                    let name_str = String::from_utf8_lossy(name);
                    if !expected.is_empty() && name_str != expected {
                        // Skip this conflicting entry.
                        current_code += 1;
                        i += 1;
                        continue;
                    }
                }
                // Emit code prefix if needed (after filtering, the previous
                // code integer may have been skipped).
                if new_diffs.is_empty()
                    || !matches!(new_diffs.last(), Some(Object::Integer(_)) | Some(Object::Name(_)))
                    || needs_code_prefix(&new_diffs, current_code)
                {
                    new_diffs.push(Object::Integer(current_code));
                }
                new_diffs.push(diffs[i].clone());
                current_code += 1;
            }
            _ => {
                new_diffs.push(diffs[i].clone());
            }
        }
        i += 1;
    }
    enc.set("Differences", Object::Array(new_diffs));
}

/// Check if we need to emit a code integer before the next name in the
/// rebuilt Differences array.
fn needs_code_prefix(arr: &[Object], code: i64) -> bool {
    // The array tracks (code, name, name, ...) — we need a code prefix
    // when the last entry is not a name at (code - 1), i.e. the sequence
    // is not consecutive.
    match arr.last() {
        Some(Object::Integer(_)) => false, // Already have a code prefix
        None => true,
        _ => {
            // Walk backwards to find the last code integer and count names after it.
            let mut names_after_last_int = 0;
            let mut last_int = 0i64;
            for item in arr.iter().rev() {
                match item {
                    Object::Name(_) => names_after_last_int += 1,
                    Object::Integer(n) => {
                        last_int = *n;
                        break;
                    }
                    _ => {}
                }
            }
            // Expected code for next name = last_int + names_after_last_int
            (last_int + names_after_last_int) != code
        }
    }
}

/// Standard WinAnsiEncoding glyph names for codes 128-159.
fn winansi_name_for_code_128_159(code: u8) -> &'static str {
    match code {
        128 => "Euro",
        130 => "quotesinglbase",
        131 => "florin",
        132 => "quotedblbase",
        133 => "ellipsis",
        134 => "dagger",
        135 => "daggerdbl",
        136 => "circumflex",
        137 => "perthousand",
        138 => "Scaron",
        139 => "guilsinglleft",
        140 => "OE",
        142 => "Zcaron",
        145 => "quoteleft",
        146 => "quoteright",
        147 => "quotedblleft",
        148 => "quotedblright",
        149 => "bullet",
        150 => "endash",
        151 => "emdash",
        152 => "tilde",
        153 => "trademark",
        154 => "scaron",
        155 => "guilsinglright",
        156 => "oe",
        158 => "zcaron",
        159 => "Ydieresis",
        _ => "",
    }
}

/// Glyph name for a code in StandardEncoding (PDF spec Table D.1).
#[allow(dead_code)]
fn standard_encoding_glyph_name(code: u8) -> Option<&'static str> {
    match code {
        32 => Some("space"),
        33 => Some("exclam"),
        34 => Some("quotedbl"),
        35 => Some("numbersign"),
        36 => Some("dollar"),
        37 => Some("percent"),
        38 => Some("ampersand"),
        39 => Some("quoteright"),
        40 => Some("parenleft"),
        41 => Some("parenright"),
        42 => Some("asterisk"),
        43 => Some("plus"),
        44 => Some("comma"),
        45 => Some("hyphen"),
        46 => Some("period"),
        47 => Some("slash"),
        48..=57 => {
            const D: &[&str] = &[
                "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
            ];
            Some(D[(code - 48) as usize])
        }
        58 => Some("colon"),
        59 => Some("semicolon"),
        60 => Some("less"),
        61 => Some("equal"),
        62 => Some("greater"),
        63 => Some("question"),
        64 => Some("at"),
        65..=90 => {
            const U: &[&str] = &[
                "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P",
                "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z",
            ];
            Some(U[(code - 65) as usize])
        }
        91 => Some("bracketleft"),
        92 => Some("backslash"),
        93 => Some("bracketright"),
        94 => Some("asciicircum"),
        95 => Some("underscore"),
        96 => Some("quoteleft"),
        97..=122 => {
            const L: &[&str] = &[
                "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p",
                "q", "r", "s", "t", "u", "v", "w", "x", "y", "z",
            ];
            Some(L[(code - 97) as usize])
        }
        123 => Some("braceleft"),
        124 => Some("bar"),
        125 => Some("braceright"),
        126 => Some("asciitilde"),
        161 => Some("exclamdown"),
        162 => Some("cent"),
        163 => Some("sterling"),
        164 => Some("fraction"),
        165 => Some("yen"),
        166 => Some("florin"),
        167 => Some("section"),
        168 => Some("currency"),
        169 => Some("quotesingle"),
        170 => Some("quotedblleft"),
        171 => Some("guillemotleft"),
        172 => Some("guilsinglleft"),
        173 => Some("guilsinglright"),
        174 => Some("fi"),
        175 => Some("fl"),
        177 => Some("endash"),
        178 => Some("dagger"),
        179 => Some("daggerdbl"),
        180 => Some("periodcentered"),
        182 => Some("paragraph"),
        183 => Some("bullet"),
        184 => Some("quotesinglbase"),
        185 => Some("quotedblbase"),
        186 => Some("quotedblright"),
        187 => Some("guillemotright"),
        188 => Some("ellipsis"),
        189 => Some("perthousand"),
        191 => Some("questiondown"),
        193 => Some("grave"),
        194 => Some("acute"),
        195 => Some("circumflex"),
        196 => Some("tilde"),
        197 => Some("macron"),
        198 => Some("breve"),
        199 => Some("dotaccent"),
        200 => Some("dieresis"),
        202 => Some("ring"),
        203 => Some("cedilla"),
        205 => Some("hungarumlaut"),
        206 => Some("ogonek"),
        207 => Some("caron"),
        208 => Some("emdash"),
        225 => Some("AE"),
        227 => Some("ordfeminine"),
        232 => Some("Lslash"),
        233 => Some("Oslash"),
        234 => Some("OE"),
        235 => Some("ordmasculine"),
        241 => Some("ae"),
        245 => Some("dotlessi"),
        248 => Some("lslash"),
        249 => Some("oslash"),
        250 => Some("oe"),
        251 => Some("germandbls"),
        _ => None,
    }
}

/// Glyph name for a code in WinAnsiEncoding (PDF spec Table D.1).
#[allow(dead_code)]
fn winansi_encoding_glyph_name(code: u8) -> Option<&'static str> {
    match code {
        32 => Some("space"),
        33 => Some("exclam"),
        34 => Some("quotedbl"),
        35 => Some("numbersign"),
        36 => Some("dollar"),
        37 => Some("percent"),
        38 => Some("ampersand"),
        39 => Some("quotesingle"),
        40 => Some("parenleft"),
        41 => Some("parenright"),
        42 => Some("asterisk"),
        43 => Some("plus"),
        44 => Some("comma"),
        45 => Some("hyphen"),
        46 => Some("period"),
        47 => Some("slash"),
        48..=57 => {
            const D: &[&str] = &[
                "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine",
            ];
            Some(D[(code - 48) as usize])
        }
        58 => Some("colon"),
        59 => Some("semicolon"),
        60 => Some("less"),
        61 => Some("equal"),
        62 => Some("greater"),
        63 => Some("question"),
        64 => Some("at"),
        65..=90 => {
            const U: &[&str] = &[
                "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P",
                "Q", "R", "S", "T", "U", "V", "W", "X", "Y", "Z",
            ];
            Some(U[(code - 65) as usize])
        }
        91 => Some("bracketleft"),
        92 => Some("backslash"),
        93 => Some("bracketright"),
        94 => Some("asciicircum"),
        95 => Some("underscore"),
        96 => Some("grave"),
        97..=122 => {
            const L: &[&str] = &[
                "a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l", "m", "n", "o", "p",
                "q", "r", "s", "t", "u", "v", "w", "x", "y", "z",
            ];
            Some(L[(code - 97) as usize])
        }
        123 => Some("braceleft"),
        124 => Some("bar"),
        125 => Some("braceright"),
        126 => Some("asciitilde"),
        128 => Some("Euro"),
        130 => Some("quotesinglbase"),
        131 => Some("florin"),
        132 => Some("quotedblbase"),
        133 => Some("ellipsis"),
        134 => Some("dagger"),
        135 => Some("daggerdbl"),
        136 => Some("circumflex"),
        137 => Some("perthousand"),
        138 => Some("Scaron"),
        139 => Some("guilsinglleft"),
        140 => Some("OE"),
        142 => Some("Zcaron"),
        145 => Some("quoteleft"),
        146 => Some("quoteright"),
        147 => Some("quotedblleft"),
        148 => Some("quotedblright"),
        149 => Some("bullet"),
        150 => Some("endash"),
        151 => Some("emdash"),
        152 => Some("tilde"),
        153 => Some("trademark"),
        154 => Some("scaron"),
        155 => Some("guilsinglright"),
        156 => Some("oe"),
        158 => Some("zcaron"),
        159 => Some("Ydieresis"),
        160 => Some("space"),
        161 => Some("exclamdown"),
        162 => Some("cent"),
        163 => Some("sterling"),
        164 => Some("currency"),
        165 => Some("yen"),
        166 => Some("brokenbar"),
        167 => Some("section"),
        168 => Some("dieresis"),
        169 => Some("copyright"),
        170 => Some("ordfeminine"),
        171 => Some("guillemotleft"),
        172 => Some("logicalnot"),
        173 => Some("hyphen"),
        174 => Some("registered"),
        175 => Some("macron"),
        176 => Some("degree"),
        177 => Some("plusminus"),
        178 => Some("twosuperior"),
        179 => Some("threesuperior"),
        180 => Some("acute"),
        181 => Some("mu"),
        182 => Some("paragraph"),
        183 => Some("periodcentered"),
        184 => Some("cedilla"),
        185 => Some("onesuperior"),
        186 => Some("ordmasculine"),
        187 => Some("guillemotright"),
        188 => Some("onequarter"),
        189 => Some("onehalf"),
        190 => Some("threequarters"),
        191 => Some("questiondown"),
        192 => Some("Agrave"),
        193 => Some("Aacute"),
        194 => Some("Acircumflex"),
        195 => Some("Atilde"),
        196 => Some("Adieresis"),
        197 => Some("Aring"),
        198 => Some("AE"),
        199 => Some("Ccedilla"),
        200 => Some("Egrave"),
        201 => Some("Eacute"),
        202 => Some("Ecircumflex"),
        203 => Some("Edieresis"),
        204 => Some("Igrave"),
        205 => Some("Iacute"),
        206 => Some("Icircumflex"),
        207 => Some("Idieresis"),
        208 => Some("Eth"),
        209 => Some("Ntilde"),
        210 => Some("Ograve"),
        211 => Some("Oacute"),
        212 => Some("Ocircumflex"),
        213 => Some("Otilde"),
        214 => Some("Odieresis"),
        215 => Some("multiply"),
        216 => Some("Oslash"),
        217 => Some("Ugrave"),
        218 => Some("Uacute"),
        219 => Some("Ucircumflex"),
        220 => Some("Udieresis"),
        221 => Some("Yacute"),
        222 => Some("Thorn"),
        223 => Some("germandbls"),
        224 => Some("agrave"),
        225 => Some("aacute"),
        226 => Some("acircumflex"),
        227 => Some("atilde"),
        228 => Some("adieresis"),
        229 => Some("aring"),
        230 => Some("ae"),
        231 => Some("ccedilla"),
        232 => Some("egrave"),
        233 => Some("eacute"),
        234 => Some("ecircumflex"),
        235 => Some("edieresis"),
        236 => Some("igrave"),
        237 => Some("iacute"),
        238 => Some("icircumflex"),
        239 => Some("idieresis"),
        240 => Some("eth"),
        241 => Some("ntilde"),
        242 => Some("ograve"),
        243 => Some("oacute"),
        244 => Some("ocircumflex"),
        245 => Some("otilde"),
        246 => Some("odieresis"),
        247 => Some("divide"),
        248 => Some("oslash"),
        249 => Some("ugrave"),
        250 => Some("uacute"),
        251 => Some("ucircumflex"),
        252 => Some("udieresis"),
        253 => Some("yacute"),
        254 => Some("thorn"),
        255 => Some("ydieresis"),
        _ => None,
    }
}

enum StdEncAction {
    None,
    ReplaceName,
    ReplaceInlineBase,
    ReplaceRefBase(ObjectId),
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

    // Only TrueType simple fonts — Type1 fonts have different §6.2.11.6 rules
    // and sanitizing their Differences causes width/CharSet regressions.
    let is_truetype = get_name_val(dict, b"Subtype").as_deref() == Some("TrueType");
    if !is_truetype {
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

    // For TrueType: check if font has (3,1) cmap. If not, Differences are forbidden.
    if is_truetype {
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

    // Pass 1: fix standalone DeviceN array objects (stored as top-level objects).
    for id in &ids {
        let fix_info = analyze_devicen_colorants(doc, *id);
        if let Some(missing_names) = fix_info {
            if add_colorants_entries(doc, *id, &missing_names) {
                count += 1;
            }
        }
    }

    // Pass 2: fix inline DeviceN arrays embedded within dictionary/stream objects.
    // Resource /ColorSpace sub-dicts often store DeviceN arrays inline as dict
    // values (e.g. /CS1 [/DeviceN [/Pantone#20540#20blue] /DeviceCMYK 2402 0 R])
    // rather than as standalone objects, so Pass 1 misses them.
    // Form XObjects also have inline Resources in their stream dicts. (#gen-544)
    for id in &ids {
        // Clone the object to avoid simultaneous mutable/immutable borrows.
        let obj_clone = doc.objects.get(id).cloned();
        match obj_clone {
            Some(Object::Dictionary(d)) => {
                let mut d_patched = d.clone();
                let fixed = fix_inline_devicen_in_dict(&mut d_patched);
                if fixed > 0 {
                    doc.objects.insert(*id, Object::Dictionary(d_patched));
                    count += fixed;
                }
            }
            Some(Object::Stream(s)) => {
                let mut s_patched = s.clone();
                let fixed = fix_inline_devicen_in_dict(&mut s_patched.dict);
                if fixed > 0 {
                    doc.objects.insert(*id, Object::Stream(s_patched));
                    count += fixed;
                }
            }
            _ => {}
        }
    }

    count
}

/// Walk every value in `dict` (and nested dicts/arrays) looking for inline
/// DeviceN colorspace arrays that are missing a Colorants entry. Patch them
/// in place and return the number of arrays fixed.
fn fix_inline_devicen_in_dict(dict: &mut lopdf::Dictionary) -> usize {
    let mut count = 0;
    for (_, val) in dict.iter_mut() {
        count += fix_inline_devicen_in_value(val);
    }
    count
}

fn fix_inline_devicen_in_value(val: &mut Object) -> usize {
    match val {
        Object::Array(arr) => {
            let fixed = try_fix_inline_devicen_array(arr);
            // Also recurse into array elements that are dicts.
            let mut count = if fixed { 1 } else { 0 };
            for item in arr.iter_mut() {
                if let Object::Dictionary(inner) = item {
                    count += fix_inline_devicen_in_dict(inner);
                }
            }
            count
        }
        Object::Dictionary(inner) => fix_inline_devicen_in_dict(inner),
        _ => 0,
    }
}

/// Try to fix an inline DeviceN colorspace array `arr` that is missing its
/// Colorants entry. Returns true if arr was modified.
///
/// A DeviceN colorspace array has 4–5 elements:
///   [/DeviceN [colorant-names...] altCS tintFn (optional-attrs)]
fn try_fix_inline_devicen_array(arr: &mut Vec<Object>) -> bool {
    if arr.len() < 4 {
        return false;
    }
    match &arr[0] {
        Object::Name(n) if n == b"DeviceN" || n == b"NChannel" => {}
        _ => return false,
    }

    // Extract spot colorant names (skip process colors).
    let process_names: &[&[u8]] = &[
        b"Cyan", b"Magenta", b"Yellow", b"Black", b"Red", b"Green", b"Blue", b"None", b"All",
    ];
    let spot_names: Vec<Vec<u8>> = match &arr[1] {
        Object::Array(names) => names
            .iter()
            .filter_map(|o| {
                if let Object::Name(n) = o {
                    if !process_names.contains(&n.as_slice()) {
                        Some(n.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect(),
        _ => return false,
    };
    if spot_names.is_empty() {
        return false;
    }

    // Check if an attrs dict at arr[4] already has all Colorants entries.
    if arr.len() > 4 {
        if let Object::Dictionary(attrs) = &arr[4] {
            let all_present = spot_names
                .iter()
                .all(|n| match attrs.get(b"Colorants").ok() {
                    Some(Object::Dictionary(cd)) => cd.has(n.as_slice()),
                    _ => false,
                });
            if all_present {
                return false;
            }
        }
    }

    // Build Separation arrays for each missing spot color.
    let alt_cs = arr[2].clone();
    let tint_fn = arr[3].clone();
    let mut colorant_dict = lopdf::Dictionary::new();
    for name in &spot_names {
        let sep = Object::Array(vec![
            Object::Name(b"Separation".to_vec()),
            Object::Name(name.clone()),
            alt_cs.clone(),
            tint_fn.clone(),
        ]);
        colorant_dict.set(String::from_utf8_lossy(name).to_string(), sep);
    }

    if arr.len() > 4 {
        // Update existing attrs dict.
        if let Object::Dictionary(ref mut attrs) = arr[4] {
            let mut existing_cd = match attrs.get(b"Colorants").ok() {
                Some(Object::Dictionary(cd)) => cd.clone(),
                _ => lopdf::Dictionary::new(),
            };
            for name in &spot_names {
                if !existing_cd.has(name.as_slice()) {
                    let key = String::from_utf8_lossy(name).to_string();
                    if let Some(sep) = colorant_dict.get(name.as_slice()).ok().cloned() {
                        existing_cd.set(key, sep);
                    }
                }
            }
            attrs.set("Colorants", Object::Dictionary(existing_cd));
            return true;
        }
    }

    // No attrs dict yet — add one.
    let attrs = lopdf::dictionary! {
        "Colorants" => Object::Dictionary(colorant_dict),
    };
    arr.push(Object::Dictionary(attrs));
    true
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
                    matches!(n.as_slice(), b"3D" | b"Sound" | b"Screen" | b"Movie" | b"FileAttachment")
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

    // Fix non-Btn annotations where AP.N is a sub-appearance-states dict
    // instead of a direct stream (§6.3.3:4). veraPDF requires N to be a
    // single stream for all annotations except Btn widgets.
    // Collapse the dict by picking the first stream value and pointing N there.
    for id in &ids {
        let fix = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(id) else {
                continue;
            };
            // Skip Btn widgets — they NEED AP.N to be a subdictionary (§6.3.3:3).
            let is_btn_widget = {
                let is_widget = matches!(
                    dict.get(b"Subtype").ok(),
                    Some(Object::Name(ref n)) if n == b"Widget"
                );
                if is_widget {
                    // Check /FT directly or via /Parent chain.
                    if matches!(dict.get(b"FT").ok(), Some(Object::Name(ref n)) if n == b"Btn") {
                        true
                    } else {
                        let mut found = false;
                        let mut cur = dict.get(b"Parent").ok().and_then(|o| {
                            if let Object::Reference(r) = o {
                                Some(*r)
                            } else {
                                None
                            }
                        });
                        let mut depth = 0;
                        while let Some(pid) = cur {
                            depth += 1;
                            if depth > 20 {
                                break;
                            }
                            if let Some(Object::Dictionary(pd)) = doc.objects.get(&pid) {
                                if matches!(pd.get(b"FT").ok(), Some(Object::Name(ref n)) if n == b"Btn")
                                {
                                    found = true;
                                    break;
                                }
                                cur = pd.get(b"Parent").ok().and_then(|o| {
                                    if let Object::Reference(r) = o {
                                        Some(*r)
                                    } else {
                                        None
                                    }
                                });
                            } else {
                                break;
                            }
                        }
                        found
                    }
                } else {
                    false
                }
            };
            if is_btn_widget {
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

    // Fix Button (Btn) widgets where AP.N is a single Stream instead of a
    // subdictionary (§6.3.3:3). For radio buttons and checkboxes, /N must be
    // a dict mapping state names (e.g. /Yes, /Off) to appearance streams.
    // Wrap the stream in a dict: /N << /Yes <stream-ref> >>.
    let ids3: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids3 {
        let wrap_info: Option<(ObjectId, ObjectId)> = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&id) else {
                continue;
            };
            let is_widget = matches!(
                dict.get(b"Subtype").ok(),
                Some(Object::Name(ref n)) if n == b"Widget"
            );
            if !is_widget {
                continue;
            }
            // Check if it's a Btn field (directly or via inherited /FT).
            let is_btn = if matches!(
                dict.get(b"FT").ok(),
                Some(Object::Name(ref n)) if n == b"Btn"
            ) {
                true
            } else {
                // Follow /Parent chain to find inherited /FT.
                let mut found = false;
                let mut cur = dict.get(b"Parent").ok().and_then(|o| {
                    if let Object::Reference(r) = o {
                        Some(*r)
                    } else {
                        None
                    }
                });
                let mut depth = 0;
                while let Some(pid) = cur {
                    depth += 1;
                    if depth > 20 {
                        break;
                    } // prevent loops
                    if let Some(Object::Dictionary(pd)) = doc.objects.get(&pid) {
                        if matches!(pd.get(b"FT").ok(), Some(Object::Name(ref n)) if n == b"Btn") {
                            found = true;
                            break;
                        }
                        cur = pd.get(b"Parent").ok().and_then(|o| {
                            if let Object::Reference(r) = o {
                                Some(*r)
                            } else {
                                None
                            }
                        });
                    } else {
                        break;
                    }
                }
                found
            };
            if !is_btn {
                continue;
            }
            // Get the AP dict.
            let ap_id = match dict.get(b"AP").ok() {
                Some(Object::Reference(r)) => *r,
                _ => continue,
            };
            // Check if AP.N is a stream reference (not a dict).
            match doc.objects.get(&ap_id) {
                Some(Object::Dictionary(ap)) => match ap.get(b"N").ok() {
                    Some(Object::Reference(n_id)) => match doc.objects.get(n_id) {
                        Some(Object::Stream(_)) => Some((ap_id, *n_id)),
                        _ => None,
                    },
                    _ => None,
                },
                _ => None,
            }
        };
        if let Some((ap_id, stream_id)) = wrap_info {
            // Determine the "on" state name from /AS or default to "Yes".
            let state_name = {
                let Some(Object::Dictionary(dict)) = doc.objects.get(&id) else {
                    continue;
                };
                match dict.get(b"AS").ok() {
                    Some(Object::Name(n)) if n != b"Off" => String::from_utf8_lossy(n).to_string(),
                    _ => "Yes".to_string(),
                }
            };
            if let Some(Object::Dictionary(ref mut ap)) = doc.objects.get_mut(&ap_id) {
                let mut sub_dict = lopdf::Dictionary::new();
                sub_dict.set(state_name, Object::Reference(stream_id));
                ap.set("N", Object::Dictionary(sub_dict));
                count += 1;
            }
        }
    }

    count
}

// ---------------------------------------------------------------------------
// 6.5.3 — Annotation opacity (CA must be 1.0)
// ---------------------------------------------------------------------------
//
// ---------------------------------------------------------------------------
// §6.5.1 — Strip forbidden actions from annotations and outlines.
// ---------------------------------------------------------------------------
// PDF/A forbids: Launch, Sound, Movie, ResetForm, ImportData, Hide,
// SetOCGState, Rendition, Trans, GoTo3DView, JavaScript.
// Named actions: only NextPage, PrevPage, FirstPage, LastPage are allowed.

/// Check if an action dictionary is forbidden by PDF/A §6.5.1.
fn is_action_forbidden(action: &lopdf::Dictionary) -> bool {
    const ALLOWED_ACTION_TYPES: &[&[u8]] = &[
        b"GoTo",
        b"GoToR",
        b"GoToE",
        b"Thread",
        b"URI",
        b"Named",
        b"SubmitForm",
    ];
    const ALLOWED_NAMED: &[&[u8]] = &[b"NextPage", b"PrevPage", b"FirstPage", b"LastPage"];

    let s = action.get(b"S").ok().and_then(|o| {
        if let Object::Name(n) = o {
            Some(n.clone())
        } else {
            None
        }
    });
    match s {
        None => true,
        Some(ref s) if !ALLOWED_ACTION_TYPES.iter().any(|a| s == *a) => true,
        Some(ref s) if s == b"Named" => {
            let n = action.get(b"N").ok().and_then(|o| {
                if let Object::Name(n) = o {
                    Some(n.clone())
                } else {
                    None
                }
            });
            match n {
                None => true,
                Some(ref n) => !ALLOWED_NAMED.iter().any(|a| n == *a),
            }
        }
        _ => false,
    }
}

/// Replace a forbidden action dict with a harmless GoTo.
fn neutralize_action(action: &mut lopdf::Dictionary) {
    action.set("S", Object::Name(b"GoTo".to_vec()));
    action.remove(b"N");
    action.set(
        "D",
        Object::Array(vec![Object::Integer(0), Object::Name(b"Fit".to_vec())]),
    );
}

fn fix_forbidden_actions(doc: &mut Document) -> usize {
    // PDF/A §6.5.1: only these action types are permitted.
    const ALLOWED_ACTION_TYPES: &[&[u8]] = &[
        b"GoTo",
        b"GoToR",
        b"GoToE",
        b"Thread",
        b"URI",
        b"Named",
        b"SubmitForm",
    ];
    const ALLOWED_NAMED: &[&[u8]] = &[b"NextPage", b"PrevPage", b"FirstPage", b"LastPage"];

    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let should_remove_action = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&id) else {
                continue;
            };
            // Check /A (Action) dict
            let action = match dict.get(b"A").ok() {
                Some(Object::Dictionary(a)) => Some(a.clone()),
                Some(Object::Reference(r)) => {
                    if let Some(Object::Dictionary(a)) = doc.objects.get(r) {
                        Some(a.clone())
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some(action) = action {
                let s = action.get(b"S").ok().and_then(|o| {
                    if let Object::Name(n) = o {
                        Some(n.clone())
                    } else {
                        None
                    }
                });
                match s {
                    None => true, // No S key → unknown action type → remove
                    Some(ref s) if !ALLOWED_ACTION_TYPES.iter().any(|a| s == *a) => true,
                    Some(ref s) if s == b"Named" => {
                        // Check N key for named action
                        let n = action.get(b"N").ok().and_then(|o| {
                            if let Object::Name(n) = o {
                                Some(n.clone())
                            } else {
                                None
                            }
                        });
                        match n {
                            None => true, // Missing N → remove
                            Some(ref n) => !ALLOWED_NAMED.iter().any(|a| n == *a),
                        }
                    }
                    _ => false, // Allowed action type
                }
            } else {
                false // No /A key
            }
        };
        if should_remove_action {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                dict.remove(b"A");
                count += 1;
            }
        }
    }

    // Strategy 2: Find action OBJECTS that are forbidden and replace their
    // /S and /N with an allowed action type. This catches actions referenced
    // via indirect references from annotations that our Strategy 1 missed.
    let ids2: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids2 {
        let is_forbidden = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&id) else {
                continue;
            };
            let has_s = dict.has(b"S");
            let has_type_action = matches!(
                dict.get(b"Type").ok(),
                Some(Object::Name(ref n)) if n == b"Action"
            );
            if !has_s && !has_type_action {
                continue;
            }
            let s = dict.get(b"S").ok().and_then(|o| {
                if let Object::Name(n) = o {
                    Some(n.clone())
                } else {
                    None
                }
            });
            match s {
                None => true,
                Some(ref s) if !ALLOWED_ACTION_TYPES.iter().any(|a| s == *a) => true,
                Some(ref s) if s == b"Named" => {
                    let n = dict.get(b"N").ok().and_then(|o| {
                        if let Object::Name(n) = o {
                            Some(n.clone())
                        } else {
                            None
                        }
                    });
                    match n {
                        None => true,
                        Some(ref n) => !ALLOWED_NAMED.iter().any(|a| n == *a),
                    }
                }
                _ => false,
            }
        };
        if is_forbidden {
            // Replace with a harmless GoTo action that goes nowhere
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                neutralize_action(dict);
                count += 1;
            }
        }
    }

    // Strategy 3: Handle inline annotation dicts embedded in Annots arrays.
    // Some PDFs store annotations as inline dicts in the page Annots array,
    // not as indirect references. These are invisible to Strategy 1/2 which
    // only iterate doc.objects.
    // Also handles the case where /A is a Reference to an action that
    // pdfa_cleanup::remove_forbidden_actions already stripped /S from.
    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    for page_id in &page_ids {
        let annots_ref: Option<ObjectId> = match doc.objects.get(page_id) {
            Some(Object::Dictionary(d)) => match d.get(b"Annots").ok() {
                Some(Object::Reference(r)) => Some(*r),
                _ => None,
            },
            _ => None,
        };
        // Case 1: Annots is an indirect array.
        if let Some(arr_id) = annots_ref {
            let len = match doc.objects.get(&arr_id) {
                Some(Object::Array(arr)) => arr.len(),
                _ => continue,
            };
            for i in 0..len {
                let needs_fix = match doc.objects.get(&arr_id) {
                    Some(Object::Array(arr)) => match &arr[i] {
                        Object::Dictionary(annot) => is_annot_action_forbidden(annot, doc),
                        _ => false,
                    },
                    _ => false,
                };
                if needs_fix {
                    if let Some(Object::Array(ref mut arr)) = doc.objects.get_mut(&arr_id) {
                        if let Object::Dictionary(ref mut annot) = arr[i] {
                            annot.remove(b"A");
                            count += 1;
                        }
                    }
                }
            }
        }
        // Case 2: Annots is an inline array on the page dict.
        let len = match doc.objects.get(page_id) {
            Some(Object::Dictionary(d)) => match d.get(b"Annots").ok() {
                Some(Object::Array(arr)) => arr.len(),
                _ => 0,
            },
            _ => 0,
        };
        for i in 0..len {
            let needs_fix = match doc.objects.get(page_id) {
                Some(Object::Dictionary(d)) => match d.get(b"Annots").ok() {
                    Some(Object::Array(arr)) => match &arr[i] {
                        Object::Dictionary(annot) => is_annot_action_forbidden(annot, doc),
                        _ => false,
                    },
                    _ => false,
                },
                _ => false,
            };
            if needs_fix {
                if let Some(Object::Dictionary(ref mut d)) = doc.objects.get_mut(page_id) {
                    if let Ok(Object::Array(ref mut arr)) = d.get_mut(b"Annots") {
                        if let Object::Dictionary(ref mut annot) = arr[i] {
                            annot.remove(b"A");
                            count += 1;
                        }
                    }
                }
            }
        }
    }

    count
}

/// Check if an annotation dict's /A action is forbidden.
/// Handles inline action dicts, indirect references, and actions
/// that had /S stripped by the cleanup phase (leaving invalid state).
fn is_annot_action_forbidden(annot: &lopdf::Dictionary, doc: &Document) -> bool {
    match annot.get(b"A").ok() {
        Some(Object::Dictionary(a)) => is_action_forbidden(a),
        Some(Object::Reference(r)) => match doc.objects.get(r) {
            Some(Object::Dictionary(a)) => {
                // If /S was removed (by cleanup), the action is invalid → remove.
                if !a.has(b"S") {
                    return true;
                }
                is_action_forbidden(a)
            }
            _ => false,
        },
        _ => false,
    }
}

// PDF/A-2 §6.5.3 requires annotation /CA to be 1.0 (fully opaque).
// Annotations with CA < 1.0 violate this rule. Remove the /CA key
// (default value is 1.0 per PDF spec) to satisfy veraPDF. Fixes #482.

fn fix_annotation_opacity(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let needs_fix = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&id) else {
                continue;
            };
            let has_type_annot = matches!(
                dict.get(b"Type").ok(),
                Some(Object::Name(ref n)) if n == b"Annot"
            );
            if !has_type_annot {
                continue;
            }
            match dict.get(b"CA").ok() {
                Some(Object::Real(ca)) => (*ca - 1.0_f32).abs() > f32::EPSILON,
                Some(Object::Integer(ca)) => *ca != 1,
                _ => false,
            }
        };
        if needs_fix {
            if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                dict.remove(b"CA");
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
        // Remove /Type /Filespec so the dict is no longer identified as a file
        // specification (§6.9: FileSpec without /EF = forbidden external reference).
        if matches!(dict.get(b"Type").ok(), Some(Object::Name(ref n)) if n.eq_ignore_ascii_case(b"Filespec"))
        {
            dict.remove(b"Type");
        }
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
        let (content, existing_by_cat) = {
            let Some(Object::Stream(s)) = doc.objects.get(id) else {
                continue;
            };
            // Handle Form XObjects and tiling patterns (PatternType=1).
            // Both are content streams that must declare their own Resources.
            // Fixes #465: tiling patterns referencing Pattern resources not in
            // their own Resources dict (6.2.2:2).
            let is_form = matches!(
                s.dict.get(b"Subtype").ok(),
                Some(Object::Name(ref n)) if n == b"Form"
            );
            let is_tiling_pattern =
                matches!(s.dict.get(b"PatternType").ok(), Some(Object::Integer(1)));
            if !is_form && !is_tiling_pattern {
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
            (content, existing_by_cat)
        };

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
// 6.1.7.1 — Expand inline-image /F abbreviation in content streams.
// ---------------------------------------------------------------------------
//
// PDF inline images (BI/ID/EI) use abbreviated keys: /F for /Filter, /DP for
// /DecodeParms, etc. Our §6.1.7.1 checker (and some veraPDF rules) falsely
// flag "/F" inside content streams as a file specification. Expanding the
// abbreviation to "/Filter" avoids the false positive without changing the
// rendered output.

fn fix_inline_image_f_abbrev(doc: &mut Document) -> usize {
    let ids: Vec<ObjectId> = collect_content_stream_ids(doc).into_iter().collect();
    let mut count = 0;

    for id in ids {
        let decompressed = if let Some(Object::Stream(s)) = doc.objects.get(&id) {
            match s.decompressed_content() {
                Ok(d) => d,
                Err(_) => continue,
            }
        } else {
            continue;
        };

        // Quick check: does this stream contain BI (inline image)?
        if !decompressed.windows(3).any(|w| {
            w[0] == b'B'
                && w[1] == b'I'
                && (w[2] == b' ' || w[2] == b'\n' || w[2] == b'\r' || w[2] == b'/')
        }) {
            continue;
        }

        // Replace /F with /Filter and /DP with /DecodeParms inside BI..ID blocks.
        let mut out = Vec::with_capacity(decompressed.len() + 64);
        let mut i = 0;
        let mut changed = false;
        let mut in_bi = false;

        while i < decompressed.len() {
            if !in_bi {
                // Look for "BI" preceded by whitespace or start.
                if i + 2 <= decompressed.len()
                    && decompressed[i] == b'B'
                    && decompressed[i + 1] == b'I'
                    && (i == 0 || decompressed[i - 1].is_ascii_whitespace())
                    && (i + 2 == decompressed.len()
                        || decompressed[i + 2] == b' '
                        || decompressed[i + 2] == b'\n'
                        || decompressed[i + 2] == b'\r'
                        || decompressed[i + 2] == b'/')
                {
                    out.extend_from_slice(b"BI");
                    i += 2;
                    in_bi = true;
                    continue;
                }
                out.push(decompressed[i]);
                i += 1;
            } else {
                // Inside BI..ID: scan for "/F " or "/F/" and replace with "/Filter "
                if decompressed[i] == b'I'
                    && i + 1 < decompressed.len()
                    && decompressed[i + 1] == b'D'
                    && (i + 2 >= decompressed.len()
                        || decompressed[i + 2] == b' '
                        || decompressed[i + 2] == b'\n')
                {
                    // End of BI header — copy rest of inline image as-is.
                    in_bi = false;
                    out.push(decompressed[i]);
                    i += 1;
                    continue;
                }
                if decompressed[i] == b'/'
                    && i + 2 < decompressed.len()
                    && decompressed[i + 1] == b'F'
                    && (decompressed[i + 2] == b' ' || decompressed[i + 2] == b'/')
                {
                    out.extend_from_slice(b"/Filter");
                    i += 2; // skip "/F", the space/slash stays
                    changed = true;
                    continue;
                }
                // /DP → /DecodeParms
                if decompressed[i] == b'/'
                    && i + 3 < decompressed.len()
                    && decompressed[i + 1] == b'D'
                    && decompressed[i + 2] == b'P'
                    && (decompressed[i + 3] == b' ' || decompressed[i + 3] == b'<')
                {
                    out.extend_from_slice(b"/DecodeParms");
                    i += 3;
                    changed = true;
                    continue;
                }
                out.push(decompressed[i]);
                i += 1;
            }
        }

        if changed {
            if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(&id) {
                s.set_plain_content(out);
                count += 1;
            }
        }
    }
    count
}

// ---------------------------------------------------------------------------
// 6.9 — Remove file specification entries from document names tree.
// ---------------------------------------------------------------------------
//
// PDF/A-2b forbids file specifications with embedded files (/EF). After
// fix_file_spec_ef_extra strips /EF, the remaining FileSpec dicts can still
// trigger §6.9 if they appear in the /Names /EmbeddedFiles tree. This pass
// removes the /EmbeddedFiles entry from the /Names dictionary.

fn fix_names_embedded_files(doc: &mut Document) -> usize {
    // Resolve /Root → /Names → /EmbeddedFiles.
    let catalog_id = match doc.trailer.get(b"Root").ok() {
        Some(Object::Reference(id)) => *id,
        _ => return 0,
    };
    let names_id = match doc.objects.get(&catalog_id) {
        Some(Object::Dictionary(cat)) => match cat.get(b"Names").ok() {
            Some(Object::Reference(id)) => Some(*id),
            Some(Object::Dictionary(_)) => None, // inline — handle below
            _ => return 0,
        },
        _ => return 0,
    };

    if let Some(nid) = names_id {
        // /Names is an indirect reference.
        if let Some(Object::Dictionary(ref mut names)) = doc.objects.get_mut(&nid) {
            if names.has(b"EmbeddedFiles") {
                names.remove(b"EmbeddedFiles");
                return 1;
            }
        }
    } else {
        // /Names might be inline in the catalog.
        if let Some(Object::Dictionary(ref mut cat)) = doc.objects.get_mut(&catalog_id) {
            if let Ok(Object::Dictionary(ref mut names)) = cat.get_mut(b"Names") {
                if names.has(b"EmbeddedFiles") {
                    names.remove(b"EmbeddedFiles");
                    return 1;
                }
            }
        }
    }
    0
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
// 6.1.13 — Clamp Real values exceeding ±32767 or subnormal in PDF objects.
// ---------------------------------------------------------------------------

fn fix_overflow_reals(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let obj = match doc.objects.get(&id) {
            Some(o) => o.clone(),
            None => continue,
        };
        let (fixed, n) = fix_overflow_reals_in_object(obj, 0);
        if n > 0 {
            doc.objects.insert(id, fixed);
            count += n;
        }
    }
    count
}

fn fix_overflow_reals_in_object(obj: Object, depth: usize) -> (Object, usize) {
    const MAX_REAL: f64 = 32767.0;
    const MIN_POSITIVE: f64 = 1.175e-38;
    if depth > MAX_OBJECT_DEPTH {
        return (obj, 0);
    }
    match obj {
        Object::Real(v) => {
            let val = v as f64;
            if val != 0.0 && val.abs() < MIN_POSITIVE {
                (Object::Real(0.0), 1)
            } else if val.abs() > MAX_REAL {
                let clamped = if val > 0.0 { MAX_REAL } else { -MAX_REAL };
                (Object::Real(clamped as f32), 1)
            } else {
                (Object::Real(v), 0)
            }
        }
        Object::Array(arr) => {
            let mut total = 0;
            let new_arr: Vec<Object> = arr
                .into_iter()
                .map(|o| {
                    let (fixed, n) = fix_overflow_reals_in_object(o, depth + 1);
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
                let (fixed, n) = fix_overflow_reals_in_object(val, depth + 1);
                total += n;
                new_dict.set(key, fixed);
            }
            (Object::Dictionary(new_dict), total)
        }
        Object::Stream(mut s) => {
            let mut total = 0;
            let mut new_dict = lopdf::Dictionary::new();
            for (key, val) in s.dict.into_iter() {
                let (fixed, n) = fix_overflow_reals_in_object(val, depth + 1);
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
        let (fixed, n) = fix_overflow_in_object(obj, 0);
        if n > 0 {
            doc.objects.insert(id, fixed);
            count += n;
        }
    }
    count
}

// Depth limit for inline-object recursion — prevents stack overflow on
// pathological PDFs with thousands of nested arrays/dicts. (#oracle-gen-stackoverflow)
const MAX_OBJECT_DEPTH: usize = 128;

fn fix_overflow_in_object(obj: Object, depth: usize) -> (Object, usize) {
    if depth > MAX_OBJECT_DEPTH {
        return (obj, 0);
    }
    match obj {
        Object::Integer(v) if v > i64::from(i32::MAX) => (Object::Integer(i64::from(i32::MAX)), 1),
        Object::Integer(v) if v < i64::from(i32::MIN) => (Object::Integer(i64::from(i32::MIN)), 1),
        Object::Array(arr) => {
            let mut total = 0;
            let new_arr: Vec<Object> = arr
                .into_iter()
                .map(|o| {
                    let (fixed, n) = fix_overflow_in_object(o, depth + 1);
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
                let (fixed, n) = fix_overflow_in_object(val, depth + 1);
                total += n;
                new_dict.set(key, fixed);
            }
            (Object::Dictionary(new_dict), total)
        }
        Object::Stream(mut s) => {
            let mut total = 0;
            let mut new_dict = lopdf::Dictionary::new();
            for (key, val) in s.dict.into_iter() {
                let (fixed, n) = fix_overflow_in_object(val, depth + 1);
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
        let (fixed, n) = fix_long_strings_in_object(obj, 0);
        if n > 0 {
            doc.objects.insert(id, fixed);
            count += n;
        }
    }
    count
}

fn fix_long_strings_in_object(obj: Object, depth: usize) -> (Object, usize) {
    if depth > MAX_OBJECT_DEPTH {
        return (obj, 0);
    }
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
                    let (fixed, n) = fix_long_strings_in_object(o, depth + 1);
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
                let (fixed, n) = fix_long_strings_in_object(val, depth + 1);
                total += n;
                new_dict.set(key, fixed);
            }
            (Object::Dictionary(new_dict), total)
        }
        Object::Stream(mut s) => {
            let mut total = 0;
            let mut new_dict = lopdf::Dictionary::new();
            for (key, val) in s.dict.into_iter() {
                let (fixed, n) = fix_long_strings_in_object(val, depth + 1);
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
                    match dict.get(b"Contents").ok() {
                        Some(Object::Reference(cid)) => {
                            // Contents may be a reference to a stream or to an Array
                            // of stream references. Dereference one level to handle
                            // the indirect-array case (e.g. Contents: 7 0 R where
                            // obj 7 is an array). Without this, streams inside the
                            // array are silently skipped by all fixups. Fixes #479.
                            match doc.objects.get(cid) {
                                Some(Object::Array(arr)) => {
                                    for item in arr {
                                        if let Object::Reference(sid) = item {
                                            ids.insert(*sid);
                                        }
                                    }
                                }
                                _ => {
                                    ids.insert(*cid);
                                }
                            }
                        }
                        Some(Object::Array(arr)) => {
                            for item in arr {
                                if let Object::Reference(cid) = item {
                                    ids.insert(*cid);
                                }
                            }
                        }
                        _ => {}
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
        // Catch annotation appearance streams (/AP) explicitly, even if they lack /Subtype /Form.
        if let Object::Dictionary(dict) = obj {
            let is_annot = matches!(dict.get(b"Type").ok(), Some(Object::Name(ref n)) if n == b"Annotation" || n == b"Annot");
            if is_annot {
                if let Ok(Object::Dictionary(ap_dict)) = dict.get(b"AP") {
                    for (_, ap_val) in ap_dict.iter() {
                        match ap_val {
                            Object::Reference(rid) => {
                                ids.insert(*rid);
                            }
                            Object::Dictionary(sub_ap) => {
                                for (_, sub_ap_val) in sub_ap.iter() {
                                    if let Object::Reference(rid) = sub_ap_val {
                                        ids.insert(*rid);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        // Type 3 font CharProcs — each value is a content stream reference.
        if let Object::Dictionary(dict) = obj {
            let is_type3 =
                dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) == Some(b"Type3");
            if is_type3 {
                let charprocs_dict = match dict.get(b"CharProcs").ok() {
                    Some(Object::Dictionary(d)) => Some(d),
                    Some(Object::Reference(rid)) => {
                        if let Some(Object::Dictionary(d)) = doc.objects.get(rid) {
                            Some(d)
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
                if let Some(charprocs) = charprocs_dict {
                    for (_, val) in charprocs.iter() {
                        if let Object::Reference(sid) = val {
                            ids.insert(*sid);
                        }
                    }
                }
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
                        s.set_content(inner); // also updates /Length (#FP-6.1.7.1-len)
                        let _ = s.compress_with_level(1); // level 1: fast intermediate pass (#534 perf)
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
            s.set_content(new_content); // also updates /Length (#FP-6.1.7.1-len)
            let _ = s.compress_with_level(1); // level 1: fast intermediate pass (#534 perf)
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
                    s.set_content(new_content); // also updates /Length (#FP-6.1.7.1-len)
                    let _ = s.compress_with_level(1); // level 1: fast intermediate pass (#534 perf)
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
            s.set_content(new_content); // also updates /Length (#FP-6.1.7.1-len)
            let _ = s.compress_with_level(1); // level 1: fast intermediate pass (#534 perf)
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
                    // Check for ID preceded by whitespace.  Per ISO 32000-1 §8.9.7
                    // the ID keyword is followed by "a single white-space character"
                    // which can be any PDF white-space: SP HT LF FF CR NUL.
                    if i + 2 < decompressed.len()
                        && &decompressed[i..i + 2] == b"ID"
                        && (i == 0 || decompressed[i - 1].is_ascii_whitespace())
                        && (i + 2 >= decompressed.len()
                            || matches!(
                                decompressed[i + 2],
                                b' ' | b'\n' | b'\r' | b'\t' | 0x0C | 0x00
                            ))
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
            s.set_content(new_content); // also updates /Length (#FP-6.1.7.1-len)
                                        // Re-compress for smaller output.
            let _ = s.compress_with_level(1); // level 1: fast intermediate pass (#534 perf)
        }
    }
    count
}

// ---------------------------------------------------------------------------
// 6.1.13 — Fix extreme numeric values in content streams.
//
// PDF/A requires:
// - Real values: |val| ≤ 32767.0 and not subnormal (|val| ≥ 1.175e-38 or 0)
// - Integer values: |val| ≤ 2,147,483,647 (i32::MAX)
// - String literals: ≤ 32767 bytes
// - Name tokens: ≤ 127 bytes
//
// This function handles real/integer clamping. Names and strings in content
// streams are handled by fix_long_names_in_streams / fix_long_strings_in_streams.
// ---------------------------------------------------------------------------

fn fix_tiny_floats_in_streams(doc: &mut Document) -> usize {
    const MIN_POSITIVE: f64 = 1.175e-38;
    const MAX_REAL: f64 = 32767.0;
    const MAX_INT: i64 = 2_147_483_647;

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

        let mut new_content = Vec::with_capacity(decompressed.len());
        let mut i = 0;
        let mut fixed_any = false;
        let mut string_depth = 0u32;
        let mut escape = false;
        let mut in_hex_string = false;
        let mut in_comment = false;

        while i < decompressed.len() {
            let b = decompressed[i];

            // Skip comments.
            if in_comment {
                new_content.push(b);
                if b == b'\n' || b == b'\r' {
                    in_comment = false;
                }
                i += 1;
                continue;
            }

            // Skip literal strings.
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

            // Clean hex strings: remove non-hex characters (§6.1.6).
            if in_hex_string {
                if b == b'>' {
                    new_content.push(b);
                    in_hex_string = false;
                } else if b.is_ascii_hexdigit() || b.is_ascii_whitespace() {
                    new_content.push(b);
                } else {
                    // Skip non-hex garbage.
                    fixed_any = true;
                    count += 1;
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

            // Skip inline image binary data: BI ... ID <binary> EI.
            // Uses inline_image_data_length to skip past binary payload reliably.
            if b == b'I'
                && i + 2 < decompressed.len()
                && decompressed[i + 1] == b'D'
                && (i == 0 || decompressed[i - 1].is_ascii_whitespace())
                && decompressed[i + 2].is_ascii_whitespace()
            {
                // Find start of BI block to parse its dict.
                let mut bi_start = i;
                while bi_start > 0 && &decompressed[bi_start..bi_start + 2] != b"BI" {
                    bi_start -= 1;
                }
                let bi_dict = &decompressed[bi_start..i];
                let expected_len = inline_image_data_length(bi_dict);

                new_content.push(b'I');
                new_content.push(b'D');
                new_content.push(decompressed[i + 2]);
                i += 3;

                if expected_len > 0 {
                    let end = (i + expected_len).min(decompressed.len());
                    new_content.extend_from_slice(&decompressed[i..end]);
                    i = end;
                } else {
                    // Fallback: scan for EI.
                    while i + 2 < decompressed.len() {
                        if (decompressed[i] == b'\n'
                            || decompressed[i] == b'\r'
                            || decompressed[i] == b' ')
                            && decompressed[i + 1] == b'E'
                            && decompressed[i + 2] == b'I'
                            && (i + 3 >= decompressed.len()
                                || decompressed[i + 3].is_ascii_whitespace()
                                || decompressed[i + 3] == b'/')
                        {
                            break;
                        }
                        new_content.push(decompressed[i]);
                        i += 1;
                    }
                }
                continue;
            }

            // Check if we're at the start of a number token.
            let is_num_start = b.is_ascii_digit() || b == b'-' || b == b'+' || b == b'.';
            let prev_is_num = i > 0 && is_number_byte(decompressed[i - 1]);

            if !is_num_start || prev_is_num {
                new_content.push(b);
                i += 1;
                continue;
            }

            // Check that the next byte is also numeric (to avoid matching operators like -).
            if (b == b'-' || b == b'+')
                && (i + 1 >= decompressed.len()
                    || (!decompressed[i + 1].is_ascii_digit() && decompressed[i + 1] != b'.'))
            {
                new_content.push(b);
                i += 1;
                continue;
            }

            // Extract the full number token.
            let start = i;
            if b == b'-' || b == b'+' {
                i += 1;
            }
            while i < decompressed.len() && is_number_byte(decompressed[i]) {
                i += 1;
            }
            let token = &decompressed[start..i];

            // Parse and check range.
            if let Ok(s) = std::str::from_utf8(token) {
                let is_float = s.contains('.') || s.contains('e') || s.contains('E');
                if is_float {
                    if let Ok(val) = s.parse::<f64>() {
                        // Subnormal: tiny non-zero → 0
                        if val != 0.0 && val.abs() < MIN_POSITIVE {
                            new_content.push(b'0');
                            count += 1;
                            fixed_any = true;
                            continue;
                        }
                        // Too large: clamp to ±32767
                        if val.abs() > MAX_REAL {
                            let clamped = if val > 0.0 { MAX_REAL } else { -MAX_REAL };
                            let s = format_float_compact(clamped);
                            new_content.extend_from_slice(s.as_bytes());
                            count += 1;
                            fixed_any = true;
                            continue;
                        }
                    }
                } else if let Ok(val) = s.parse::<i64>() {
                    // Integer outside i32 range.
                    if val.unsigned_abs() > MAX_INT as u64 {
                        let clamped = val.clamp(-MAX_INT, MAX_INT);
                        let s = clamped.to_string();
                        new_content.extend_from_slice(s.as_bytes());
                        count += 1;
                        fixed_any = true;
                        continue;
                    }
                } else if s.len() > 10
                    && s.trim_start_matches(['-', '+'])
                        .chars()
                        .all(|c| c.is_ascii_digit())
                {
                    // Integer too large even for i64 — clamp to ±i32 max.
                    let negative = s.starts_with('-');
                    let clamped = if negative { -MAX_INT } else { MAX_INT };
                    let s = clamped.to_string();
                    new_content.extend_from_slice(s.as_bytes());
                    count += 1;
                    fixed_any = true;
                    continue;
                }
            }
            new_content.extend_from_slice(token);
        }

        if fixed_any {
            if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(&id) {
                s.dict.remove(b"Filter");
                s.dict.remove(b"DecodeParms");
                s.set_content(new_content); // also updates /Length (#FP-6.1.7.1-len)
                let _ = s.compress_with_level(1); // level 1: fast intermediate pass (#534 perf)
            }
        }
    }
    count
}

/// Format a float compactly without trailing zeros.
fn format_float_compact(val: f64) -> String {
    if val == val.trunc() {
        // Integer value — write without decimal point.
        format!("{}", val as i64)
    } else {
        let s = format!("{:.6}", val);
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn is_number_byte(b: u8) -> bool {
    b.is_ascii_digit() || b == b'.' || b == b'-' || b == b'+' || b == b'e' || b == b'E'
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
                s.set_content(new_content); // also updates /Length (#FP-6.1.7.1-len)
                let _ = s.compress_with_level(1); // level 1: fast intermediate pass (#534 perf)
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
    stream.set_content(vec![255u8]); // also updates /Length (#FP-6.1.7.1-len)
}

// ---------------------------------------------------------------------------
// §6.1.6.2 — Re-encode JBIG2 streams that use global segments
// ---------------------------------------------------------------------------
//
// PDF/A forbids JBIG2Decode with /JBIG2Globals in DecodeParms.
// Fix: decode the JBIG2 image (merging globals), then re-encode as 1bpp
// FlateDecode. The image dimensions (/Width, /Height) are preserved.

#[cfg(feature = "pdfa-convert")]
fn fix_jbig2_globals(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();

    for id in ids {
        // Phase 1: detect JBIG2Decode + JBIG2Globals and collect data.
        let decode_info: Option<(Vec<u8>, Vec<u8>)> = {
            let Some(Object::Stream(stream)) = doc.objects.get(&id) else {
                continue;
            };
            // Check if this stream uses JBIG2Decode.
            let is_jbig2 = match stream.dict.get(b"Filter").ok() {
                Some(Object::Name(n)) => n == b"JBIG2Decode",
                Some(Object::Array(arr)) => arr
                    .iter()
                    .any(|o| matches!(o, Object::Name(n) if n == b"JBIG2Decode")),
                _ => false,
            };
            if !is_jbig2 {
                continue;
            }
            // Check for /JBIG2Globals in DecodeParms.
            let globals_ref: Option<ObjectId> =
                if let Some(Object::Dictionary(dp)) = stream.dict.get(b"DecodeParms").ok() {
                    match dp.get(b"JBIG2Globals").ok() {
                        Some(Object::Reference(r)) => Some(*r),
                        _ => None,
                    }
                } else if let Some(Object::Array(arr)) = stream.dict.get(b"DecodeParms").ok() {
                    arr.iter().find_map(|o| {
                        if let Object::Dictionary(dp) = o {
                            match dp.get(b"JBIG2Globals").ok() {
                                Some(Object::Reference(r)) => Some(*r),
                                _ => None,
                            }
                        } else {
                            None
                        }
                    })
                } else {
                    None
                };
            let Some(globals_id) = globals_ref else {
                continue; // JBIG2 without globals — allowed in PDF/A.
            };
            // Get the globals stream data.
            let globals_data = match doc.objects.get(&globals_id) {
                Some(Object::Stream(gs)) => {
                    let mut gs_clone = gs.clone();
                    if gs_clone.decompress().is_ok() {
                        gs_clone.content.clone()
                    } else {
                        gs.content.clone()
                    }
                }
                _ => continue,
            };
            // Get the JBIG2 image data (raw stream content).
            let mut s_clone = stream.clone();
            let image_data = if s_clone.decompress().is_ok() {
                // Multi-filter: other filters decoded first, leaving raw JBIG2.
                // But if the ONLY filter is JBIG2Decode, lopdf can't decode it,
                // so decompress() may fail — use raw content.
                s_clone.content.clone()
            } else {
                stream.content.clone()
            };
            Some((image_data, globals_data))
        };

        let Some((image_data, globals_data)) = decode_info else {
            continue;
        };

        // Phase 2: decode JBIG2 and re-encode.
        let Ok(image) = hayro_jbig2::decode_embedded(&image_data, Some(&globals_data)) else {
            continue; // Decode failed — leave stream unchanged.
        };

        // Extract raw 1bpp bitmap bytes from the decoded image.
        let bytes_per_row = image.width.div_ceil(8) as usize;
        let mut raw_bitmap = Vec::with_capacity(bytes_per_row * image.height as usize);
        struct ByteCollector<'a> {
            out: &'a mut Vec<u8>,
            row_buf: Vec<u8>,
            pixel_count: u32,
        }
        impl hayro_jbig2::Decoder for ByteCollector<'_> {
            fn push_pixel(&mut self, black: bool) {
                let bit_in_byte = self.pixel_count % 8;
                if bit_in_byte == 0 {
                    self.row_buf.push(0);
                }
                if black {
                    let last = self.row_buf.last_mut().unwrap();
                    *last |= 1 << (7 - bit_in_byte);
                }
                self.pixel_count += 1;
            }
            fn push_pixel_chunk(&mut self, black: bool, chunk_count: u32) {
                // Called only when byte-aligned; each chunk = 8 pixels = 1 byte.
                let byte_val = if black { 0xFF } else { 0x00 };
                for _ in 0..chunk_count {
                    self.row_buf.push(byte_val);
                }
                self.pixel_count += chunk_count * 8;
            }
            fn next_line(&mut self) {
                self.out.extend_from_slice(&self.row_buf);
                self.row_buf.clear();
                self.pixel_count = 0;
            }
        }
        let mut collector = ByteCollector {
            out: &mut raw_bitmap,
            row_buf: Vec::with_capacity(bytes_per_row),
            pixel_count: 0,
        };
        image.decode(&mut collector);
        // Flush any remaining row data.
        let leftover = std::mem::take(&mut collector.row_buf);
        drop(collector);
        if !leftover.is_empty() {
            raw_bitmap.extend_from_slice(&leftover);
        }

        // Compress with flate.
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        if encoder.write_all(&raw_bitmap).is_err() {
            continue;
        }
        let Ok(compressed) = encoder.finish() else {
            continue;
        };

        // Phase 3: replace stream.
        if let Some(Object::Stream(stream)) = doc.objects.get_mut(&id) {
            stream
                .dict
                .set("Filter", Object::Name(b"FlateDecode".to_vec()));
            stream.dict.remove(b"DecodeParms");
            stream
                .dict
                .set("Width", Object::Integer(image.width as i64));
            stream
                .dict
                .set("Height", Object::Integer(image.height as i64));
            stream.dict.set("BitsPerComponent", Object::Integer(1));
            stream.set_content(compressed);
            count += 1;
        }
    }
    count
}

#[cfg(not(feature = "pdfa-convert"))]
fn fix_jbig2_globals(_doc: &mut Document) -> usize {
    0
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

            // Find and fix all "colr" boxes in JP2 data.
            // Use a recursive-style iteration to handle nested boxes (e.g. inside jp2h).
            let mut pos = 0usize;
            let mut modified = false;
            let mut box_stack = vec![s.content.len()]; // limits

            while pos + 8 <= s.content.len() {
                let lbox = u32::from_be_bytes([
                    s.content[pos],
                    s.content[pos + 1],
                    s.content[pos + 2],
                    s.content[pos + 3],
                ]);
                let box_type_bytes = [
                    s.content[pos + 4],
                    s.content[pos + 5],
                    s.content[pos + 6],
                    s.content[pos + 7],
                ];
                let box_type = &box_type_bytes;
                let (box_len, header_len) = if lbox == 1 {
                    if pos + 16 > s.content.len() {
                        break;
                    }
                    let xl = u64::from_be_bytes([
                        s.content[pos + 8],
                        s.content[pos + 9],
                        s.content[pos + 10],
                        s.content[pos + 11],
                        s.content[pos + 12],
                        s.content[pos + 13],
                        s.content[pos + 14],
                        s.content[pos + 15],
                    ]) as usize;
                    (xl, 16usize)
                } else if lbox == 0 {
                    // Box extends to end of file.
                    (*box_stack.last().unwrap() - pos, 8usize)
                } else {
                    (lbox as usize, 8usize)
                };

                if box_type == b"colr" {
                    // colr box layout: [1 byte method][1 byte precedence][1 byte approximation][4 bytes enumCS (if method == 1)]
                    let method_pos = pos + header_len;
                    if method_pos < s.content.len() && s.content[method_pos] == 1 {
                        let enum_pos = method_pos + 3;
                        if enum_pos + 4 <= s.content.len() {
                            let enum_cs = u32::from_be_bytes([
                                s.content[enum_pos],
                                s.content[enum_pos + 1],
                                s.content[enum_pos + 2],
                                s.content[enum_pos + 3],
                            ]);
                            // PDF/A-2 §6.2.8.3: only sRGB(16), greyscale(17), sYCC(18)
                            // are allowed. Any other enumCS must be replaced.
                            if enum_cs != 16 && enum_cs != 17 && enum_cs != 18 {
                                // Pick replacement based on channel count.
                                let channels = jpx_channel_count(&s.content);
                                let replacement = match channels {
                                    Some(1) => 17u32, // greyscale
                                    _ => 16u32,       // sRGB
                                };
                                let bytes = replacement.to_be_bytes();
                                s.content[enum_pos] = bytes[0];
                                s.content[enum_pos + 1] = bytes[1];
                                s.content[enum_pos + 2] = bytes[2];
                                s.content[enum_pos + 3] = bytes[3];
                                modified = true;
                            }
                        }
                    }
                }

                if box_type == b"jp2h" || box_type == b"res " {
                    // These boxes contain other boxes. Enter them.
                    box_stack.push(pos + box_len);
                    pos += header_len;
                } else {
                    if box_len == 0 || pos + box_len > s.content.len() {
                        break;
                    }
                    pos += box_len;

                    // Pop from stack if we've reached the end of a container box.
                    while pos >= *box_stack.last().unwrap() && box_stack.len() > 1 {
                        box_stack.pop();
                    }
                }
            }

            if modified {
                count += 1;
            }
        }
    }
    count
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
                s.set_content(new_content); // also updates /Length (#FP-6.1.7.1-len)
                let _ = s.compress_with_level(1); // level 1: fast intermediate pass (#534 perf)
            }
        }
    }
    count
}

// ---------------------------------------------------------------------------
// §6.2.6:1 — Invalid rendering intent in content streams
// ---------------------------------------------------------------------------
//
// PDF/A-2 allows only four rendering intent names: RelativeColorimetric,
// AbsoluteColorimetric, Perceptual, Saturation. Some PDFs include vendor-
// specific names (e.g. "FICL:RI_to_be_removed" from Enfocus PitStop) via the
// `ri` operator. veraPDF fails §6.2.6:1 for any non-standard intent name.
//
// Fix: scan content streams for `/<name> ri` where name is not in the valid
// set; replace the operand with /RelativeColorimetric. (#gen-168)

fn fix_invalid_rendering_intents(doc: &mut Document) -> usize {
    const VALID_INTENTS: &[&[u8]] = &[
        b"RelativeColorimetric",
        b"AbsoluteColorimetric",
        b"Perceptual",
        b"Saturation",
    ];

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

        // Quick check: contains "ri" at all?
        if !decoded.windows(2).any(|w| w == b"ri") {
            continue;
        }

        let mut new_content = Vec::with_capacity(decoded.len());
        let mut i = 0;
        let len = decoded.len();
        let mut fixed = false;

        while i < len {
            // Look for '/' which starts a Name token.
            if decoded[i] == b'/' {
                // Scan to end of name: any byte that is not whitespace or PDF delimiter.
                let name_start = i + 1;
                let mut j = name_start;
                while j < len && !is_pdf_delimiter(decoded[j]) {
                    j += 1;
                }
                let name = &decoded[name_start..j];

                // Skip whitespace after name.
                let mut k = j;
                while k < len && decoded[k].is_ascii_whitespace() {
                    k += 1;
                }

                // Check if next token is "ri" followed by whitespace/delimiter/EOF.
                if k + 2 <= len
                    && &decoded[k..k + 2] == b"ri"
                    && (k + 2 >= len || is_pdf_delimiter(decoded[k + 2]))
                {
                    // This is an `ri` operator call.
                    if !VALID_INTENTS.contains(&name) {
                        // Replace with /RelativeColorimetric ri (preserving trailing
                        // whitespace that follows the original "ri" token).
                        new_content.extend_from_slice(b"/RelativeColorimetric ri");
                        // Copy whatever follows "ri" (whitespace etc.)
                        i = k + 2;
                        fixed = true;
                        count += 1;
                        continue;
                    }
                }
            }
            new_content.push(decoded[i]);
            i += 1;
        }

        if fixed {
            if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(&id) {
                s.dict.remove(b"Filter");
                s.dict.remove(b"DecodeParms");
                s.set_content(new_content);
                let _ = s.compress_with_level(1);
            }
        }
    }
    count
}

// ---------------------------------------------------------------------------
// §6.2.4.2:2 — OPM (Overprint Mode) must be 0 for ICCBased CMYK colorspaces
// ---------------------------------------------------------------------------
//
// veraPDF §6.2.4.2:2 fails when ExtGState has OPM=1 and overprinting is
// enabled with an ICCBased CMYK colorspace. PDF/A-2 allows OPM only when the
// colorspace is DeviceCMYK with OPM=1. Setting OPM=0 everywhere is safe — it
// is the default and disables the special overprint behavior. (#gen-631)

fn fix_extgstate_opm(doc: &mut Document) -> usize {
    // Walk every document object. For each Dictionary or Stream object,
    // recursively set OPM=0 on any inline ExtGState dict with OPM=1.
    // Standalone ExtGState objects and inline dicts in Form XObject /Resources
    // are both handled by walking the dict (or stream dict) recursively.
    // Form XObjects store /Resources in the STREAM DICT (s.dict), not in
    // their own Object::Dictionary — both must be walked. (#gen-631, §6.2.4.2:2)
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut count = 0;

    for id in ids {
        match doc.objects.get_mut(&id) {
            Some(Object::Dictionary(d)) => {
                count += fix_opm_in_dict_recursive(d);
            }
            Some(Object::Stream(s)) => {
                count += fix_opm_in_dict_recursive(&mut s.dict);
            }
            _ => {}
        }
    }
    count
}

/// Recursively set OPM=0 in any dict or inline sub-dict that has OPM=1.
/// Returns the number of OPM values changed.
fn fix_opm_in_dict_recursive(d: &mut lopdf::Dictionary) -> usize {
    let mut count = 0;
    if d.has(b"OPM") {
        if let Ok(Object::Integer(v)) = d.get(b"OPM") {
            if *v == 1 {
                d.set("OPM", Object::Integer(0));
                count += 1;
            }
        }
    }
    for (_, val) in d.iter_mut() {
        match val {
            Object::Dictionary(inner) => {
                count += fix_opm_in_dict_recursive(inner);
            }
            Object::Array(arr) => {
                for item in arr.iter_mut() {
                    if let Object::Dictionary(inner) = item {
                        count += fix_opm_in_dict_recursive(inner);
                    }
                }
            }
            _ => {}
        }
    }
    count
}

// ---------------------------------------------------------------------------
// §6.4.2 — Fix ExtGState SMask /S subtype
// ---------------------------------------------------------------------------
//
// PDF/A requires that SMask dictionaries in ExtGState have /S set to either
// /Alpha or /Luminosity. Some malformed PDFs have /S /GoTo or other invalid
// values. Fix: set invalid /S values to /Alpha (the more common default).

fn fix_extgstate_smask_subtype(doc: &mut Document) -> usize {
    // Walk every object. Any dictionary with /SMask (dict) whose /S is not
    // Alpha or Luminosity is fixed. This covers standalone ExtGState objects
    // (indirect refs), inline dicts in Resources, and Form XObject stream dicts.
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut count = 0;

    for id in ids {
        let has_bad_smask = match doc.objects.get(&id) {
            Some(Object::Dictionary(d)) => dict_has_bad_smask(d),
            Some(Object::Stream(s)) => dict_has_bad_smask(&s.dict),
            _ => false,
        };
        if !has_bad_smask {
            continue;
        }
        match doc.objects.get_mut(&id) {
            Some(Object::Dictionary(d)) => {
                count += fix_bad_smask_recursive(d);
            }
            Some(Object::Stream(s)) => {
                count += fix_bad_smask_recursive(&mut s.dict);
            }
            _ => {}
        }
    }
    count
}

/// Check if any dict or nested sub-dict has an SMask with invalid /S.
fn dict_has_bad_smask(d: &lopdf::Dictionary) -> bool {
    if let Ok(Object::Dictionary(smask)) = d.get(b"SMask") {
        match smask.get(b"S").ok() {
            Some(Object::Name(s)) if s == b"Alpha" || s == b"Luminosity" => {}
            Some(Object::Name(_)) | None => return true,
            _ => {}
        }
    }
    for (_, val) in d.iter() {
        match val {
            Object::Dictionary(inner) => {
                if dict_has_bad_smask(inner) {
                    return true;
                }
            }
            Object::Array(arr) => {
                for item in arr {
                    if let Object::Dictionary(inner) = item {
                        if dict_has_bad_smask(inner) {
                            return true;
                        }
                    }
                }
            }
            _ => {}
        }
    }
    false
}

/// Recursively fix SMask /S values in a dict and all nested sub-dicts.
fn fix_bad_smask_recursive(d: &mut lopdf::Dictionary) -> usize {
    let mut count = 0;
    if let Ok(Object::Dictionary(smask)) = d.get_mut(b"SMask") {
        let needs_fix = match smask.get(b"S").ok() {
            Some(Object::Name(s)) if s == b"Alpha" || s == b"Luminosity" => false,
            Some(Object::Name(_)) | None => true,
            _ => false,
        };
        if needs_fix {
            smask.set("S", Object::Name(b"Alpha".to_vec()));
            count += 1;
        }
    }
    for (_, val) in d.iter_mut() {
        match val {
            Object::Dictionary(inner) => {
                count += fix_bad_smask_recursive(inner);
            }
            Object::Array(arr) => {
                for item in arr.iter_mut() {
                    if let Object::Dictionary(inner) = item {
                        count += fix_bad_smask_recursive(inner);
                    }
                }
            }
            _ => {}
        }
    }
    count
}

// ---------------------------------------------------------------------------
// §6.4.1 — Normalize non-standard blend mode names in ExtGState
// ---------------------------------------------------------------------------
//
// PDF spec defines specific blend mode names (Normal, Multiply, Screen, etc.).
// Some PDFs have case-insensitive variants (e.g. "MuLtiply"). PDF/A requires
// exact casing. Fix: normalize to the canonical name.

fn fix_extgstate_blend_modes(doc: &mut Document) -> usize {
    // Walk every object, recursively fix /BM values that aren't standard.
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut count = 0;

    for id in ids {
        match doc.objects.get_mut(&id) {
            Some(Object::Dictionary(d)) => {
                count += fix_blend_recursive(d);
            }
            Some(Object::Stream(s)) => {
                count += fix_blend_recursive(&mut s.dict);
            }
            _ => {}
        }
    }
    count
}

const VALID_BLEND_MODES: &[&[u8]] = &[
    b"Normal",
    b"Compatible",
    b"Multiply",
    b"Screen",
    b"Overlay",
    b"Darken",
    b"Lighten",
    b"ColorDodge",
    b"ColorBurn",
    b"HardLight",
    b"SoftLight",
    b"Difference",
    b"Exclusion",
    b"Hue",
    b"Saturation",
    b"Color",
    b"Luminosity",
];

fn fix_blend_recursive(d: &mut lopdf::Dictionary) -> usize {
    let mut count = 0;
    if let Ok(Object::Name(bm)) = d.get(b"BM") {
        let bm_clone = bm.clone();
        if !VALID_BLEND_MODES.iter().any(|v| *v == bm_clone.as_slice()) {
            if let Some(canonical) = find_canonical_blend_mode(&bm_clone) {
                d.set("BM", Object::Name(canonical.to_vec()));
                count += 1;
            } else {
                d.set("BM", Object::Name(b"Normal".to_vec()));
                count += 1;
            }
        }
    }
    for (_, val) in d.iter_mut() {
        match val {
            Object::Dictionary(inner) => {
                count += fix_blend_recursive(inner);
            }
            Object::Array(arr) => {
                for item in arr.iter_mut() {
                    if let Object::Dictionary(inner) = item {
                        count += fix_blend_recursive(inner);
                    }
                }
            }
            _ => {}
        }
    }
    count
}

fn find_canonical_blend_mode(name: &[u8]) -> Option<&'static [u8]> {
    let lower: Vec<u8> = name.iter().map(|b| b.to_ascii_lowercase()).collect();
    for &v in VALID_BLEND_MODES {
        let vl: Vec<u8> = v.iter().map(|b| b.to_ascii_lowercase()).collect();
        if lower == vl {
            return Some(v);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Fix concatenated PDF operators (e.g. "Qq" → "Q q")
// ---------------------------------------------------------------------------
//
// Some broken PDF producers concatenate operators without whitespace separators.
// "Qq" is the most common — Q (restore graphics state) followed by q (save
// graphics state). This is not a valid single operator, causing veraPDF to flag
// rule 6.2.2:1 ("operator not defined in ISO 32000-1").

const CONCATENATED_OPERATOR_PATTERNS: &[(&[u8], &[u8])] = &[
    (b"cmBI", b"cm BI"),
    (b"BDCBT", b"BDC BT"),
    (b"DoQq", b"Do Q q"),
    (b"DoQ", b"Do Q"),
    (b"Qq", b"Q q"),
];

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

        // Quick check: does this stream contain any of the patterns?
        // Also check for "ref" (re+f concatenation, #gen-389).
        let has_match = CONCATENATED_OPERATOR_PATTERNS
            .iter()
            .any(|(pat, _)| decompressed.windows(pat.len()).any(|w| w == *pat))
            || decompressed.windows(3).any(|w| w == b"ref");
        if !has_match {
            continue;
        }

        if let Some((new_content, fixed_count)) = rewrite_concatenated_operators(&decompressed) {
            if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(&id) {
                s.dict.remove(b"Filter");
                s.dict.remove(b"DecodeParms");
                s.set_content(new_content); // also updates /Length (#FP-6.1.7.1-len)
                let _ = s.compress_with_level(1); // level 1: fast intermediate pass (#534 perf)
            }
            count += fixed_count;
        }
    }
    count
}

fn rewrite_concatenated_operators(data: &[u8]) -> Option<(Vec<u8>, usize)> {
    let mut out = Vec::with_capacity(data.len() + 64);
    let mut i = 0;
    let len = data.len();
    let mut count = 0usize;

    'outer: while i < len {
        match data[i] {
            b'%' => {
                let start = i;
                while i < len && data[i] != b'\n' && data[i] != b'\r' {
                    i += 1;
                }
                out.extend_from_slice(&data[start..i]);
                continue;
            }
            b'(' => {
                let start = i;
                i += 1;
                let mut depth = 1u32;
                while i < len && depth > 0 {
                    match data[i] {
                        b'\\' => {
                            i += 1;
                            if i < len {
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
                        _ => i += 1,
                    }
                }
                out.extend_from_slice(&data[start..i]);
                continue;
            }
            b'<' if i + 1 < len && data[i + 1] == b'<' => {
                let start = i;
                i += 2;
                let mut depth = 1u32;
                while i + 1 < len && depth > 0 {
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
                    i = len;
                }
                out.extend_from_slice(&data[start..i]);
                continue;
            }
            b'<' => {
                let start = i;
                i += 1;
                while i < len && data[i] != b'>' {
                    i += 1;
                }
                if i < len {
                    i += 1;
                }
                out.extend_from_slice(&data[start..i]);
                continue;
            }
            b'/' => {
                let start = i;
                i += 1;
                while i < len && !is_pdf_delimiter_or_ws(data[i]) {
                    i += 1;
                }
                out.extend_from_slice(&data[start..i]);
                continue;
            }
            b'[' => {
                let start = i;
                i += 1;
                let mut depth = 1u32;
                while i < len && depth > 0 {
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
                            i += 1;
                            let mut str_depth = 1u32;
                            while i < len && str_depth > 0 {
                                match data[i] {
                                    b'\\' => {
                                        i += 1;
                                        if i < len {
                                            i += 1;
                                        }
                                    }
                                    b'(' => {
                                        str_depth += 1;
                                        i += 1;
                                    }
                                    b')' => {
                                        str_depth -= 1;
                                        i += 1;
                                    }
                                    _ => i += 1,
                                }
                            }
                        }
                        _ => i += 1,
                    }
                }
                out.extend_from_slice(&data[start..i]);
                continue;
            }
            _ => {}
        }

        for (pat, repl) in CONCATENATED_OPERATOR_PATTERNS {
            let plen = pat.len();
            if i + plen <= len && &data[i..i + plen] == *pat {
                let before_ok = i == 0 || is_pdf_delimiter_or_ws(data[i - 1]);
                let after_ok = i + plen >= len || is_pdf_delimiter_or_ws(data[i + plen]);
                if before_ok && after_ok {
                    out.extend_from_slice(repl);
                    i += plen;
                    count += 1;
                    continue 'outer;
                }
            }
        }

        // Special case: "ref" = "re" + "f" concatenated without whitespace.
        // Only rewrite it in operator position, never inside string operands.
        if i + 3 <= len && &data[i..i + 3] == b"ref" {
            let before_ok = i == 0 || is_pdf_delimiter_or_ws(data[i - 1]);
            let after_ok = i + 3 >= len || {
                let a = data[i + 3];
                is_pdf_delimiter_or_ws(a)
                    || a.is_ascii_digit()
                    || a == b'-'
                    || a == b'+'
                    || a == b'.'
            };
            if before_ok && after_ok {
                out.extend_from_slice(b"re f ");
                i += 3;
                count += 1;
                continue;
            }
        }

        out.push(data[i]);
        i += 1;
    }

    if count > 0 {
        Some((out, count))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::rewrite_concatenated_operators;

    #[test]
    fn concatenated_operator_fix_skips_literal_strings() {
        let data = b"(see ref. 17) Tj";
        assert!(rewrite_concatenated_operators(data).is_none());
    }

    #[test]
    fn concatenated_operator_fix_preserves_strings_but_splits_operator() {
        let data = b"(see ref. 17) Qq";
        let (fixed, count) = rewrite_concatenated_operators(data).expect("rewrite");
        assert_eq!(count, 1);
        assert_eq!(fixed, b"(see ref. 17) Q q");
    }

    #[test]
    fn concatenated_operator_fix_still_handles_ref_number_pattern() {
        let data = b"10 20 30 40 ref354.48";
        let (fixed, count) = rewrite_concatenated_operators(data).expect("rewrite");
        assert_eq!(count, 1);
        assert_eq!(fixed, b"10 20 30 40 re f 354.48");
    }
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

// ---------------------------------------------------------------------------
// Inline image ASCII85 re-encoding (§6.2.2 / §8.9.7)
//
// Inline images that use ASCII85Decode as the outer filter (`/F [/A85 /Fl]`)
// contain printable-ASCII-encoded image data. That data can include byte
// sequences such as `\nEI ` which certain PDF validators (veraPDF) mistake
// for the inline-image end marker, causing bytes from the ASCII85 payload to
// be parsed as content-stream operators (e.g., `#`, `?T`). PDF/A §6.2.2 then
// flags those as undefined operators.
//
// This pass re-encodes such inline images: we strip the ASCII85 layer (the
// data between ID and `~>` is ASCII85-decoded, revealing raw FlateDecode-
// compressed pixels) and update the filter to `/Fl` only. The resulting
// binary FlateDecode stream is parsed reliably by all validators.
// ---------------------------------------------------------------------------

fn fix_ascii85_inline_images(doc: &mut Document) -> usize {
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

        // Quick check: must have both BI and A85 in the stream
        if !decoded.windows(2).any(|w| w == b"BI") {
            continue;
        }
        if !decoded.windows(3).any(|w| w == b"A85") {
            continue;
        }

        if let Some(new_content) = reencode_ascii85_inline_images_in_stream(&decoded) {
            if let Some(Object::Stream(ref mut stream)) = doc.objects.get_mut(&id) {
                stream.set_plain_content(new_content);
                count += 1;
            }
        }
    }
    count
}

/// Scans a decompressed content stream for BI blocks that use ASCII85Decode
/// filter, strips the ASCII85 layer, and returns the modified stream.
/// Returns `None` if no ASCII85 inline images are found.
fn reencode_ascii85_inline_images_in_stream(data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    let mut modified = false;

    while i < data.len() {
        // Look for "BI" preceded by a PDF delimiter/whitespace (or at start).
        // Must also be followed by whitespace so we don't match e.g. "BIG".
        if i + 2 <= data.len()
            && &data[i..i + 2] == b"BI"
            && (i == 0 || is_pdf_delimiter_or_ws(data[i - 1]))
            && (i + 2 >= data.len() || data[i + 2].is_ascii_whitespace())
        {
            let bi_content_start = i + 2; // right after "BI"

            // Scan for " ID " (whitespace + "ID" + whitespace) without writing
            let mut id_at = None;
            let mut j = bi_content_start;
            while j < data.len() {
                if j + 3 <= data.len()
                    && data[j].is_ascii_whitespace()
                    && &data[j + 1..j + 3] == b"ID"
                    && (j + 3 >= data.len() || data[j + 3].is_ascii_whitespace())
                {
                    id_at = Some(j);
                    break;
                }
                j += 1;
            }

            let Some(id_pos) = id_at else {
                // No ID found — write "BI" and continue from bi_content_start
                out.extend_from_slice(b"BI");
                i = bi_content_start;
                continue;
            };

            let dict_bytes = &data[bi_content_start..id_pos];

            // Check if this inline image uses ASCII85 filter
            let has_a85 = dict_bytes.windows(3).any(|w| w == b"A85")
                || dict_bytes.windows(11).any(|w| w == b"ASCII85D");

            if !has_a85 {
                // Not ASCII85: copy verbatim BI…EI block with normal EI detection
                out.extend_from_slice(b"BI");
                out.extend_from_slice(dict_bytes);
                // include " ID"
                out.extend_from_slice(&data[id_pos..id_pos + 3]);
                i = id_pos + 3; // skip past " ID"
                                // Skip single space/newline after "ID" (§8.9.7: single WS after ID)
                if i < data.len() && data[i].is_ascii_whitespace() {
                    out.push(data[i]);
                    i += 1;
                }
                // Copy image data until whitespace+EI+(whitespace|Q)
                while i < data.len() {
                    if data[i].is_ascii_whitespace()
                        && i + 3 <= data.len()
                        && &data[i + 1..i + 3] == b"EI"
                        && (i + 3 >= data.len()
                            || data[i + 3].is_ascii_whitespace()
                            || data[i + 3] == b'Q')
                    {
                        out.extend_from_slice(&data[i..i + 3]);
                        i += 3;
                        break;
                    }
                    out.push(data[i]);
                    i += 1;
                }
                continue;
            }

            // ASCII85 inline image — re-encode to FlateDecode only.
            // After " ID" there is one mandatory space (§8.9.7), then the A85 data.
            let mut data_start = id_pos + 3;
            if data_start < data.len() && data[data_start].is_ascii_whitespace() {
                data_start += 1; // skip the single mandatory space after ID
            }

            // Find ~> (ASCII85 end-of-stream marker)
            let mut tilde_pos = None;
            let mut k = data_start;
            while k + 1 < data.len() {
                if data[k] == b'~' && data[k + 1] == b'>' {
                    tilde_pos = Some(k);
                    break;
                }
                k += 1;
            }

            let Some(tp) = tilde_pos else {
                // No ~> found — malformed; copy verbatim and fall through
                out.extend_from_slice(b"BI");
                out.extend_from_slice(dict_bytes);
                out.extend_from_slice(&data[id_pos..data_start]);
                i = data_start;
                while i < data.len() {
                    if data[i].is_ascii_whitespace()
                        && i + 3 <= data.len()
                        && &data[i + 1..i + 3] == b"EI"
                        && (i + 3 >= data.len()
                            || data[i + 3].is_ascii_whitespace()
                            || data[i + 3] == b'Q')
                    {
                        out.extend_from_slice(&data[i..i + 3]);
                        i += 3;
                        break;
                    }
                    out.push(data[i]);
                    i += 1;
                }
                continue;
            };

            // ASCII85-decode the data (data_start..tp, the ~> is the terminator)
            let a85_bytes = &data[data_start..tp + 2]; // include ~> for the decoder
            let fl_compressed = decode_ascii85(a85_bytes);

            // Skip past ~> and any whitespace/newlines before EI
            i = tp + 2;
            while i < data.len() && data[i].is_ascii_whitespace() {
                i += 1;
            }
            if i + 2 <= data.len() && &data[i..i + 2] == b"EI" {
                i += 2;
            }

            // Build new dict: replace A85 from the filter key
            let new_dict = remove_a85_from_inline_image_dict(dict_bytes);

            // Write new inline image block: BI + updated_dict + \nID + fl_data + \nEI
            out.extend_from_slice(b"BI");
            out.extend_from_slice(&new_dict);
            out.extend_from_slice(b"\nID ");
            out.extend_from_slice(&fl_compressed);
            out.extend_from_slice(b"\nEI");
            modified = true;
            continue;
        }

        out.push(data[i]);
        i += 1;
    }

    if modified {
        Some(out)
    } else {
        None
    }
}

/// Decodes ASCII85-encoded data. The input may include the `~>` end marker.
/// Returns the decoded binary bytes.
fn decode_ascii85(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut group = [0u8; 5];
    let mut glen = 0usize;
    let mut i = 0;

    while i < data.len() {
        let b = data[i];
        i += 1;

        if b == b'~' {
            // End marker ~>; flush partial group if any
            if glen > 0 && i < data.len() && data[i] == b'>' {
                group[glen..5].fill(b'u'); // pad with 'u' (value 84 in base-85)
                let v: u32 = (0..5).fold(0u32, |acc, k| acc * 85 + (group[k] - 33) as u32);
                let bytes = v.to_be_bytes();
                for byte in bytes.iter().take(glen - 1) {
                    out.push(*byte);
                }
            }
            break;
        }

        if b.is_ascii_whitespace() {
            continue;
        }

        if b == b'z' {
            // Special: 4 zero bytes
            out.extend_from_slice(&[0u8; 4]);
            continue;
        }

        if (b'!'..=b'u').contains(&b) {
            group[glen] = b;
            glen += 1;
            if glen == 5 {
                let v: u32 = (0..5).fold(0u32, |acc, k| acc * 85 + (group[k] - 33) as u32);
                out.extend_from_slice(&v.to_be_bytes());
                glen = 0;
            }
        }
        // Bytes outside valid range are ignored (malformed input)
    }

    out
}

/// Removes ASCII85Decode (`/A85` or `/ASCII85Decode`) from the inline image
/// dict bytes, keeping FlateDecode (`/Fl` or `/FlateDecode`).
///
/// Handles common patterns: `/F [/A85 /Fl]` → `/F /Fl`, etc.
/// If a pattern is not recognised, returns the dict unchanged (safe fallback).
fn remove_a85_from_inline_image_dict(dict: &[u8]) -> Vec<u8> {
    // Common filter array forms to simplify
    const REPLACEMENTS: &[(&[u8], &[u8])] = &[
        (b"[/A85 /Fl]", b"/Fl"),
        (b"[/A85  /Fl]", b"/Fl"),
        (b"[ /A85 /Fl]", b"/Fl"),
        (b"[ /A85 /Fl ]", b"/Fl"),
        (b"[/A85 /FlateDecode]", b"/FlateDecode"),
        (b"[/ASCII85Decode /Fl]", b"/Fl"),
        (b"[/ASCII85Decode /FlateDecode]", b"/FlateDecode"),
        // Reversed order (rare but possible)
        (b"[/Fl /A85]", b"/Fl"),
        (b"[/FlateDecode /A85]", b"/FlateDecode"),
        (b"[/Fl /ASCII85Decode]", b"/Fl"),
        (b"[/FlateDecode /ASCII85Decode]", b"/FlateDecode"),
    ];

    let mut result = dict.to_vec();
    for (pattern, replacement) in REPLACEMENTS {
        if let Some(pos) = result.windows(pattern.len()).position(|w| w == *pattern) {
            let mut new = Vec::with_capacity(result.len() - pattern.len() + replacement.len());
            new.extend_from_slice(&result[..pos]);
            new.extend_from_slice(replacement);
            new.extend_from_slice(&result[pos + pattern.len()..]);
            result = new;
            return result; // only one filter key per dict
        }
    }
    result // no recognised pattern — return unchanged
}

// ---------------------------------------------------------------------------
// §6.1.10:1 — LZW compression forbidden in inline images
// ---------------------------------------------------------------------------
//
// PDF/A forbids LZWDecode in inline images (same as in regular streams).
// This pass re-encodes LZW-compressed inline images to FlateDecode.
// Pattern mirrors fix_ascii85_inline_images above.
// ---------------------------------------------------------------------------

fn fix_lzw_inline_images(doc: &mut Document) -> usize {
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

        // Quick check: must have both BI and LZW in the stream
        if !decoded.windows(2).any(|w| w == b"BI") {
            continue;
        }
        if !decoded.windows(3).any(|w| w == b"LZW") {
            continue;
        }

        if let Some(new_content) = reencode_lzw_inline_images_in_stream(&decoded) {
            if let Some(Object::Stream(ref mut stream)) = doc.objects.get_mut(&id) {
                stream.set_plain_content(new_content);
                count += 1;
            }
        }
    }
    count
}

/// Compress unfiltered inline images whose binary pixel data contains byte
/// sequences that match veraPDF's EI-end-of-image heuristic ("\nEI\n", "\rEI ",
/// etc.).  FlateDecode-encoding the data eliminates the false pattern.
pub fn fix_binary_inline_image_ei(doc: &mut Document) -> usize {
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

        // Quick check: must have inline images in the stream
        if !decoded.windows(2).any(|w| w == b"BI") {
            continue;
        }

        if let Some(new_content) = compress_binary_inline_images_with_ei(&decoded) {
            if let Some(Object::Stream(ref mut stream)) = doc.objects.get_mut(&id) {
                stream.set_plain_content(new_content);
                count += 1;
            }
        }
    }
    count
}

/// Scan a content stream for unfiltered inline images whose raw pixel data
/// contains the EI termination pattern.  For those images, add `/F /Fl` to the
/// dict and FlateDecode-compress the image data.
fn compress_binary_inline_images_with_ei(data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    let mut modified = false;

    while i < data.len() {
        // Look for "BI" preceded by a delimiter/whitespace (or at start).
        if i + 2 <= data.len()
            && &data[i..i + 2] == b"BI"
            && (i == 0 || is_pdf_delimiter_or_ws(data[i - 1]))
            && (i + 2 >= data.len() || data[i + 2].is_ascii_whitespace())
        {
            let bi_start = i;
            let bi_content_start = i + 2;

            // Find " ID " marker
            let mut id_at = None;
            let mut j = bi_content_start;
            while j < data.len() {
                if j + 3 <= data.len()
                    && data[j].is_ascii_whitespace()
                    && &data[j + 1..j + 3] == b"ID"
                    && (j + 3 >= data.len()
                        || data[j + 3] == b' '
                        || data[j + 3] == b'\n'
                        || data[j + 3] == b'\r')
                {
                    id_at = Some(j);
                    break;
                }
                j += 1;
            }
            let Some(id_pos) = id_at else {
                out.push(data[i]);
                i += 1;
                continue;
            };

            let dict_bytes = &data[bi_content_start..id_pos];

            // Skip images that already have a filter — only unfiltered ones
            // have predictable data lengths and are susceptible to the EI issue.
            let has_filter = dict_bytes.windows(2).any(|w| w == b"/F")
                || dict_bytes.windows(7).any(|w| w == b"/Filter");
            if has_filter {
                // Pass through as-is until EI
                let after_id = id_pos + 3; // skip " ID"
                out.extend_from_slice(&data[bi_start..after_id]);
                // Skip past whitespace byte after ID + find EI by pattern
                i = after_id;
                while i < data.len() {
                    if (data[i] == b'\n' || data[i] == b' ' || data[i] == b'\r')
                        && i + 3 <= data.len()
                        && &data[i + 1..i + 3] == b"EI"
                        && (i + 3 >= data.len()
                            || data[i + 3].is_ascii_whitespace()
                            || data[i + 3] == b'Q')
                    {
                        out.extend_from_slice(&data[after_id..i + 3]);
                        i += 3;
                        break;
                    }
                    i += 1;
                }
                continue;
            }

            // Calculate expected image data length
            let expected_len = inline_image_data_length(dict_bytes);
            if expected_len == 0 {
                // Can't determine length — pass through
                out.push(data[i]);
                i += 1;
                continue;
            }

            // The image data starts after "ID" + one whitespace byte
            let data_start = id_pos + 3 + 1; // " ID" + whitespace
            let data_end = (data_start + expected_len).min(data.len());
            let image_data = &data[data_start..data_end];

            // ASCIIHex-encode the image data unconditionally.
            // ASCIIHexDecode output uses only 0-9 and A-F, so "EI" can never
            // appear ("I" is not a hex digit).  FlateDecode is NOT safe here
            // because compressed output can still spell "\nEI".
            {
                let hex: Vec<u8> = image_data
                    .iter()
                    .flat_map(|b| format!("{b:02X}").into_bytes())
                    .chain(std::iter::once(b'>'))
                    .collect();
                // Write: BI <modified dict with /F /AHx> ID <hex data> EI
                out.extend_from_slice(b"BI");
                out.extend_from_slice(dict_bytes);
                out.extend_from_slice(b" /F /AHx");
                out.extend_from_slice(b"\nID ");
                out.extend_from_slice(&hex);
                out.extend_from_slice(b"\nEI");

                // Skip past the original EI
                i = data_end;
                while i < data.len() {
                    if (data[i] == b'\n' || data[i] == b' ' || data[i] == b'\r')
                        && i + 3 <= data.len()
                        && &data[i + 1..i + 3] == b"EI"
                        && (i + 3 >= data.len()
                            || data[i + 3].is_ascii_whitespace()
                            || data[i + 3] == b'Q')
                    {
                        i += 3;
                        break;
                    }
                    i += 1;
                }
                modified = true;
                continue;
            }
        }
        out.push(data[i]);
        i += 1;
    }

    if modified {
        Some(out)
    } else {
        None
    }
}

/// Scans a decompressed content stream for BI blocks that use LZWDecode
/// filter, decodes LZW, re-encodes as FlateDecode, and returns the modified
/// stream. Returns `None` if no LZW inline images are found.
fn reencode_lzw_inline_images_in_stream(data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(data.len());
    let mut i = 0;
    let mut modified = false;

    while i < data.len() {
        // Look for "BI" preceded by a PDF delimiter/whitespace (or at start).
        if i + 2 <= data.len()
            && &data[i..i + 2] == b"BI"
            && (i == 0 || is_pdf_delimiter_or_ws(data[i - 1]))
            && (i + 2 >= data.len() || data[i + 2].is_ascii_whitespace())
        {
            let bi_content_start = i + 2;

            // Scan for " ID " (whitespace + "ID" + whitespace)
            let mut id_at = None;
            let mut j = bi_content_start;
            while j < data.len() {
                if j + 3 <= data.len()
                    && data[j].is_ascii_whitespace()
                    && &data[j + 1..j + 3] == b"ID"
                    && (j + 3 >= data.len() || data[j + 3].is_ascii_whitespace())
                {
                    id_at = Some(j);
                    break;
                }
                j += 1;
            }

            let Some(id_pos) = id_at else {
                out.extend_from_slice(b"BI");
                i = bi_content_start;
                continue;
            };

            let dict_bytes = &data[bi_content_start..id_pos];

            // Check if this inline image uses LZW filter
            let has_lzw = dict_bytes.windows(4).any(|w| w == b"/LZW")
                || dict_bytes.windows(10).any(|w| w == b"/LZWDecode");

            if !has_lzw {
                // Not LZW: copy verbatim BI…EI block
                out.extend_from_slice(b"BI");
                out.extend_from_slice(dict_bytes);
                out.extend_from_slice(&data[id_pos..id_pos + 3]);
                i = id_pos + 3;
                if i < data.len() && data[i].is_ascii_whitespace() {
                    out.push(data[i]);
                    i += 1;
                }
                while i < data.len() {
                    if data[i].is_ascii_whitespace()
                        && i + 3 <= data.len()
                        && &data[i + 1..i + 3] == b"EI"
                        && (i + 3 >= data.len()
                            || data[i + 3].is_ascii_whitespace()
                            || data[i + 3] == b'Q')
                    {
                        out.extend_from_slice(&data[i..i + 3]);
                        i += 3;
                        break;
                    }
                    out.push(data[i]);
                    i += 1;
                }
                continue;
            }

            // LZW inline image — re-encode to FlateDecode.
            // After " ID" there is one mandatory whitespace byte (§8.9.7).
            let mut data_start = id_pos + 3;
            if data_start < data.len() && data[data_start].is_ascii_whitespace() {
                data_start += 1;
            }

            // Find the EI marker. LZW data is binary, so use heuristic:
            // scan for whitespace + "EI" + (whitespace | Q | EOF).
            // Collect all candidates and pick the right one by trying LZW decode.
            let mut ei_candidates: Vec<usize> = Vec::new();
            let mut k = data_start;
            while k < data.len() {
                if data[k].is_ascii_whitespace()
                    && k + 3 <= data.len()
                    && &data[k + 1..k + 3] == b"EI"
                    && (k + 3 >= data.len()
                        || data[k + 3].is_ascii_whitespace()
                        || data[k + 3] == b'Q')
                {
                    ei_candidates.push(k);
                }
                k += 1;
            }

            // Try each candidate: decode LZW, pick the first that succeeds
            let mut decoded_data = None;
            let mut ei_end = 0usize;
            for &ei_ws_pos in &ei_candidates {
                let lzw_bytes = &data[data_start..ei_ws_pos];
                if let Some(dec) = inline_lzw_decode(lzw_bytes) {
                    decoded_data = Some(dec);
                    ei_end = ei_ws_pos + 3; // skip ws + "EI"
                    break;
                }
            }

            if decoded_data.is_none() {
                // LZW decode failed for all candidates. Use last candidate
                // and just strip the filter (safe fallback — image may be broken
                // but the PDF won't have a forbidden filter).
                if let Some(&last_ei) = ei_candidates.last() {
                    let lzw_bytes = &data[data_start..last_ei];
                    // Flate-compress the raw LZW bytes as fallback
                    if let Some(compressed) = inline_flate_compress(lzw_bytes) {
                        let new_dict = remove_lzw_from_inline_image_dict(dict_bytes);
                        out.extend_from_slice(b"BI");
                        out.extend_from_slice(&new_dict);
                        out.extend_from_slice(b"\nID ");
                        out.extend_from_slice(&compressed);
                        out.extend_from_slice(b"\nEI");
                        modified = true;
                        i = last_ei + 3;
                        continue;
                    }
                }
                // Total failure: copy verbatim
                out.extend_from_slice(b"BI");
                out.extend_from_slice(dict_bytes);
                out.extend_from_slice(&data[id_pos..data_start]);
                i = data_start;
                while i < data.len() {
                    if data[i].is_ascii_whitespace()
                        && i + 3 <= data.len()
                        && &data[i + 1..i + 3] == b"EI"
                        && (i + 3 >= data.len()
                            || data[i + 3].is_ascii_whitespace()
                            || data[i + 3] == b'Q')
                    {
                        out.extend_from_slice(&data[i..i + 3]);
                        i += 3;
                        break;
                    }
                    out.push(data[i]);
                    i += 1;
                }
                continue;
            }

            let raw_pixels = decoded_data.unwrap();

            // Flate-compress the decoded data
            let Some(compressed) = inline_flate_compress(&raw_pixels) else {
                // Compression failed — copy verbatim
                out.extend_from_slice(b"BI");
                out.extend_from_slice(dict_bytes);
                out.extend_from_slice(&data[id_pos..data_start]);
                out.extend_from_slice(&data[data_start..ei_end]);
                i = ei_end;
                continue;
            };

            // Build new dict: replace LZW with Fl
            let new_dict = remove_lzw_from_inline_image_dict(dict_bytes);

            // Write new inline image block
            out.extend_from_slice(b"BI");
            out.extend_from_slice(&new_dict);
            out.extend_from_slice(b"\nID ");
            out.extend_from_slice(&compressed);
            out.extend_from_slice(b"\nEI");
            modified = true;
            i = ei_end;
            continue;
        }

        out.push(data[i]);
        i += 1;
    }

    if modified {
        Some(out)
    } else {
        None
    }
}

/// LZW-decode inline image data. Uses MSB byte order, min code size = 8.
fn inline_lzw_decode(data: &[u8]) -> Option<Vec<u8>> {
    let mut decoder = weezl::decode::Decoder::with_tiff_size_switch(weezl::BitOrder::Msb, 8);
    decoder.decode(data).ok()
}

/// Flate-compress data for inline image re-encoding.
fn inline_flate_compress(data: &[u8]) -> Option<Vec<u8>> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    if encoder.write_all(data).is_ok() {
        encoder.finish().ok()
    } else {
        None
    }
}

/// Removes LZWDecode (`/LZW` or `/LZWDecode`) from the inline image dict
/// bytes, replacing with FlateDecode (`/Fl`).
fn remove_lzw_from_inline_image_dict(dict: &[u8]) -> Vec<u8> {
    // Single filter patterns (most common)
    const REPLACEMENTS: &[(&[u8], &[u8])] = &[
        // Abbreviated: /F /LZW → /F /Fl
        (b"/F /LZW", b"/F /Fl"),
        (b"/F/LZW", b"/F /Fl"),
        // Full name: /Filter /LZWDecode → /Filter /FlateDecode
        (b"/Filter /LZWDecode", b"/Filter /FlateDecode"),
        (b"/Filter/LZWDecode", b"/Filter /FlateDecode"),
        // Array forms: [/LZW /Fl] → /Fl (remove LZW layer)
        (b"[/LZW /Fl]", b"/Fl"),
        (b"[/LZW  /Fl]", b"/Fl"),
        (b"[ /LZW /Fl]", b"/Fl"),
        (b"[ /LZW /Fl ]", b"/Fl"),
        (b"[/LZWDecode /FlateDecode]", b"/FlateDecode"),
        (b"[/LZWDecode /Fl]", b"/Fl"),
        // Reversed order
        (b"[/Fl /LZW]", b"/Fl"),
        (b"[/FlateDecode /LZWDecode]", b"/FlateDecode"),
    ];

    let result = dict.to_vec();
    for (pattern, replacement) in REPLACEMENTS {
        if let Some(pos) = result.windows(pattern.len()).position(|w| w == *pattern) {
            let mut new = Vec::with_capacity(result.len() - pattern.len() + replacement.len());
            new.extend_from_slice(&result[..pos]);
            new.extend_from_slice(replacement);
            new.extend_from_slice(&result[pos + pattern.len()..]);
            return new;
        }
    }
    result // no recognised pattern — return unchanged
}

#[inline]
fn is_pdf_delimiter_or_ws(b: u8) -> bool {
    is_pdf_delimiter(b) || b.is_ascii_whitespace()
}

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
                s.set_content(new_content); // also updates /Length (#FP-6.1.7.1-len)
                let _ = s.compress_with_level(1); // level 1: fast intermediate pass (#534 perf)
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

        // ── Keyword / operator token: scan to next delimiter or whitespace ────
        let tok_start = i;
        while i < data.len() && !is_pdf_delimiter(data[i]) && !data[i].is_ascii_whitespace() {
            i += 1;
        }
        if i == tok_start {
            // Stray delimiter character (e.g. orphaned `>` from `>>`): skip
            // and mark the stream as modified so it gets rewritten without it.
            modified = true;
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
            // Scan to ID marker (whitespace-preceded) — do NOT push dict bytes
            // one-by-one here; write the full range at once when ID is found to
            // avoid double-writing the dict. (#fix-bi-handler-double-write)
            let bi_dict_start = i; // right after "BI"
            let mut id_found = false;
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
                    out.extend_from_slice(&data[bi_dict_start..i + 3]); // dict + "ID"
                    i += 3;
                    id_found = true;
                    break;
                }
                i += 1; // advance without writing — written as one range above
            }
            if !id_found {
                // Malformed: no ID found; write what we scanned and continue.
                out.extend_from_slice(&data[bi_dict_start..i]);
                continue;
            }

            // Calculate expected image data length from the BI dict so we can
            // skip past the binary data reliably.  Pattern-matching for EI in
            // binary image data produces false positives when pixel bytes happen
            // to spell "\nEI\n" (§6.2.2 stray-EI regression).
            let bi_dict = &data[bi_dict_start..i.saturating_sub(3)];
            let expected_len = inline_image_data_length(bi_dict);

            // Now scan binary data until EI.
            // If we know the expected length, skip past it first and then
            // look for EI — this avoids false matches inside the pixel data.
            let mut found_ei = false;
            let scan_start = if expected_len > 0 {
                // After "ID" there is one mandatory whitespace byte, then
                // exactly `expected_len` bytes of image data.
                let skip = i + 1 + expected_len; // +1 for whitespace after ID
                                                 // Copy the whitespace + image data verbatim.
                let safe_end = skip.min(data.len());
                out.extend_from_slice(&data[i..safe_end]);
                safe_end
            } else {
                i
            };
            let mut j = scan_start;
            while j < data.len() {
                if (data[j] == b'\n' || data[j] == b' ' || data[j] == b'\r')
                    && j + 3 <= data.len()
                    && &data[j + 1..j + 3] == b"EI"
                    && (j + 3 >= data.len()
                        || data[j + 3].is_ascii_whitespace()
                        || data[j + 3] == b'Q')
                {
                    // Write any data between the scan start and the EI marker.
                    if expected_len > 0 && j > scan_start {
                        out.extend_from_slice(&data[scan_start..j]);
                    } else if expected_len == 0 {
                        out.extend_from_slice(&data[i..j]);
                    }
                    out.extend_from_slice(&data[j..j + 3]);
                    i = j + 3;
                    found_ei = true;
                    break;
                }
                j += 1;
            }
            if !found_ei {
                // Malformed: no EI found; write remaining data.
                if expected_len > 0 && j > scan_start {
                    out.extend_from_slice(&data[scan_start..j]);
                } else if expected_len == 0 {
                    out.extend_from_slice(&data[i..j]);
                }
                i = j;
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
        // PDF/A §6.2.2 forbids undefined operators even inside BX/EX compatibility
        // blocks ("even if such operators are bracketed by the BX/EX compatibility
        // operators"). Do NOT treat bx_depth > 0 as a validity shield. (#FN-6.2.2)
        let _ = bx_depth; // tracked above but not used for validity in PDF/A mode
        let is_valid = ISO32000_OPERATORS.contains(&token);

        if is_valid {
            out.extend_from_slice(&pending);
            pending.clear();
            out.extend_from_slice(token);
        } else {
            // Unknown operator token: discard it and its pending operands.
            // This covers both pure-ASCII unknown operators and tokens with
            // embedded non-ASCII bytes (e.g. "ic\x80RGB" from corrupt streams).
            // All ISO 32000 operators are pure ASCII, so non-ASCII tokens in
            // operator position are always garbage. Fixes #465.
            modified = true;
            pending.clear();
            // Ensure token separation after the discard.
            out.push(b'\n');
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

/// Parse the inline image BI dict to compute expected image data length.
///
/// Returns 0 if the length cannot be determined (unknown colorspace,
/// missing /W or /H, filter present, etc.).  The caller should fall back
/// to pattern-based EI scanning in that case.
fn inline_image_data_length(dict_bytes: &[u8]) -> usize {
    // Extract integer value for a given key (/W, /H, /BPC).
    // Supports both full and abbreviated names.
    fn extract_int(dict: &[u8], keys: &[&[u8]]) -> Option<usize> {
        for &key in keys {
            let mut idx = 0;
            while idx + key.len() < dict.len() {
                if &dict[idx..idx + key.len()] == key {
                    let after = idx + key.len();
                    // Skip whitespace
                    let mut j = after;
                    while j < dict.len() && dict[j].is_ascii_whitespace() {
                        j += 1;
                    }
                    // Parse integer
                    let mut end = j;
                    while end < dict.len() && dict[end].is_ascii_digit() {
                        end += 1;
                    }
                    if end > j {
                        if let Some(v) = std::str::from_utf8(&dict[j..end])
                            .ok()
                            .and_then(|s| s.parse::<usize>().ok())
                        {
                            return Some(v);
                        }
                    }
                }
                idx += 1;
            }
        }
        None
    }

    fn extract_name<'a>(dict: &'a [u8], keys: &[&[u8]]) -> Option<&'a [u8]> {
        for &key in keys {
            let mut idx = 0;
            while idx + key.len() < dict.len() {
                if &dict[idx..idx + key.len()] == key {
                    let after = idx + key.len();
                    let mut j = after;
                    while j < dict.len() && dict[j].is_ascii_whitespace() {
                        j += 1;
                    }
                    if j < dict.len() && dict[j] == b'/' {
                        j += 1; // skip /
                        let name_start = j;
                        while j < dict.len() && !dict[j].is_ascii_whitespace() && dict[j] != b'/' {
                            j += 1;
                        }
                        return Some(&dict[name_start..j]);
                    }
                }
                idx += 1;
            }
        }
        None
    }

    // If there's a filter, the data length depends on the compressed size
    // which we can't compute from the dict alone.  Return 0 to fall back.
    // Check for /F key (abbreviated) or /Filter key.
    for window_start in 0..dict_bytes.len().saturating_sub(2) {
        if dict_bytes[window_start] == b'/' && window_start + 1 < dict_bytes.len() {
            let after_slash = &dict_bytes[window_start + 1..];
            if after_slash.starts_with(b"F ")
                || after_slash.starts_with(b"F\n")
                || after_slash.starts_with(b"F\r")
                || after_slash.starts_with(b"Filter")
            {
                // Has a filter — can't predict data length
                return 0;
            }
        }
    }

    let w = match extract_int(dict_bytes, &[b"/W ", b"/W\n", b"/W\r", b"/Width "]) {
        Some(v) if v > 0 => v,
        _ => return 0,
    };
    let h = extract_int(dict_bytes, &[b"/H ", b"/H\n", b"/H\r", b"/Height "]).unwrap_or(1);
    let bpc = extract_int(dict_bytes, &[b"/BPC ", b"/BPC\n", b"/BitsPerComponent "]).unwrap_or(8);

    // ImageMask (/IM true) images have 1 component and no colorspace.
    let is_imagemask = dict_bytes.windows(3).any(|w| w == b"/IM")
        && (dict_bytes.windows(8).any(|w| w == b"/IM true")
            || dict_bytes.windows(10).any(|w| w == b"/ImageMask"));

    let cs = extract_name(dict_bytes, &[b"/CS ", b"/CS\n", b"/CS\r", b"/ColorSpace "]);
    let components = if is_imagemask {
        1
    } else {
        match cs {
            Some(b"G") | Some(b"DeviceGray") => 1,
            Some(b"RGB") | Some(b"DeviceRGB") => 3,
            Some(b"CMYK") | Some(b"DeviceCMYK") => 4,
            _ => return 0, // ICCBased, Indexed, etc. — can't determine
        }
    };

    // Row length in bytes (ceiling division)
    let bits_per_row = w * components * bpc;
    let bytes_per_row = (bits_per_row + 7) / 8;
    bytes_per_row * h
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
    /// Truncates to 127 bytes to comply with ISO 19005-2 rule 6.1.13:4
    /// ("A conforming file shall not contain any name longer than 127 bytes").
    fn sanitize_name(name: &[u8]) -> Option<Vec<u8>> {
        if name.iter().all(|&b| b <= 127) && name.len() <= 127 {
            return None; // already ASCII-clean and within length limit
        }
        // Pure-ASCII but too long — truncate.
        if name.iter().all(|&b| b <= 127) {
            return Some(name[..127].to_vec());
        }
        let mut out = Vec::with_capacity(name.len() * 2);
        for &b in name {
            if b > 127 {
                // Each non-ASCII byte becomes 2 hex digits; stop before exceeding 127.
                if out.len() + 2 > 127 {
                    break;
                }
                out.push(b"0123456789ABCDEF"[(b >> 4) as usize]);
                out.push(b"0123456789ABCDEF"[(b & 0xf) as usize]);
            } else {
                if out.len() >= 127 {
                    break;
                }
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

// ---------------------------------------------------------------------------
// 6.2.10-tgroup — Pages using transparency must have a /Group entry
// ---------------------------------------------------------------------------
//
// PDF/A-2 §6.2.10: pages that use transparency features (blending modes,
// opacity, soft masks) must carry a /Group << /S /Transparency >> entry.
// When the document has an OutputIntent (always present after
// normalize_colorspaces), no /CS entry is required in the group dict.
// Fixes violations: "Page uses transparency but has no /Group entry". (#496)

fn fix_missing_transparency_groups(doc: &mut Document) -> usize {
    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let mut count = 0;

    // Find an existing sRGB ICC profile stream (N=3) created by
    // normalize_colorspaces.  We use this in /Group /CS to avoid depending
    // on OutputIntent (which may be lost during lopdf serialization).
    let icc_id = find_srgb_icc_stream(doc);
    let cs_value = icc_id.map(|id| {
        Object::Array(vec![
            Object::Name(b"ICCBased".to_vec()),
            Object::Reference(id),
        ])
    });

    /// Check if a /Group dict needs fixing: wrong /S value, device CS, or missing CS.
    fn group_dict_needs_fix(grp: &lopdf::Dictionary) -> bool {
        // /S must be /Transparency — any other value (GoTo, etc.) needs fixing.
        match grp.get(b"S").ok() {
            Some(Object::Name(s)) if s == b"Transparency" => {}
            _ => return true,
        }
        // /CS must not be a device color space.
        match grp.get(b"CS").ok() {
            Some(Object::Name(cs))
                if cs == b"DeviceRGB" || cs == b"DeviceCMYK" || cs == b"DeviceGray" =>
            {
                true
            }
            None => true,
            _ => false,
        }
    }

    // First pass: fix indirect /Group dicts.
    let mut indirect_fixes: Vec<ObjectId> = Vec::new();
    for page_id in &page_ids {
        let Some(Object::Dictionary(pd)) = doc.objects.get(page_id) else {
            continue;
        };
        if let Ok(Object::Reference(grp_id)) = pd.get(b"Group") {
            if let Some(Object::Dictionary(grp)) = doc.objects.get(grp_id) {
                if group_dict_needs_fix(grp) {
                    indirect_fixes.push(*grp_id);
                }
            }
        }
    }
    for grp_id in indirect_fixes {
        if let Some(Object::Dictionary(ref mut grp)) = doc.objects.get_mut(&grp_id) {
            grp.set("S", Object::Name(b"Transparency".to_vec()));
            if let Some(ref cs) = cs_value {
                grp.set("CS", cs.clone());
            }
            count += 1;
        }
    }

    // Second pass: fix inline /Group dicts and add /Group to pages without one.
    for page_id in &page_ids {
        let Some(Object::Dictionary(pd)) = doc.objects.get(page_id) else {
            continue;
        };
        let group_val = pd.get(b"Group");
        let needs_group_fix = match group_val {
            Ok(Object::Reference(grp_id)) => {
                !matches!(doc.objects.get(grp_id), Some(Object::Dictionary(_)))
            }
            Ok(Object::Dictionary(grp_dict)) => group_dict_needs_fix(grp_dict),
            _ => true,
        };
        if !needs_group_fix {
            continue;
        }
        let Some(Object::Dictionary(ref mut pd)) = doc.objects.get_mut(page_id) else {
            continue;
        };
        let mut group_dict = lopdf::dictionary! {
            "S" => Object::Name(b"Transparency".to_vec()),
        };
        if let Some(ref cs) = cs_value {
            group_dict.set("CS", cs.clone());
        }
        pd.set("Group", Object::Dictionary(group_dict));
        count += 1;
    }

    // Third pass: fix /Group dicts on Form XObjects.
    let xobj_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in xobj_ids {
        let needs_fix = if let Some(Object::Stream(s)) = doc.objects.get(&id) {
            let is_form = matches!(s.dict.get(b"Subtype"), Ok(Object::Name(ref n)) if n == b"Form");
            if !is_form {
                false
            } else {
                match s.dict.get(b"Group").ok() {
                    Some(Object::Dictionary(grp)) => group_dict_needs_fix(grp),
                    _ => false,
                }
            }
        } else {
            false
        };
        if needs_fix {
            if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(&id) {
                let mut group_dict = lopdf::dictionary! {
                    "S" => Object::Name(b"Transparency".to_vec()),
                };
                if let Some(ref cs) = cs_value {
                    group_dict.set("CS", cs.clone());
                }
                s.dict.set("Group", Object::Dictionary(group_dict));
                count += 1;
            }
        }
    }

    count
}

/// Find an existing ICC profile stream with N=3 (sRGB) in the document.
/// Returns None if no suitable stream exists.
fn find_srgb_icc_stream(doc: &Document) -> Option<ObjectId> {
    for (&id, obj) in &doc.objects {
        if let Object::Stream(s) = obj {
            if let Ok(Object::Integer(3)) = s.dict.get(b"N") {
                return Some(id);
            }
        }
    }
    None
}

/// Return true if the page's ExtGState resources use transparency.
///
/// Checks for CA < 1, ca < 1, non-None SMask, or non-Normal/Compatible BM.
/// Also checks Form XObjects with their own /Group /S /Transparency.
#[allow(dead_code)]
fn page_uses_transparency_lopdf(page_dict: &lopdf::Dictionary, doc: &Document) -> bool {
    // Check ExtGState entries.
    if let Some(gs_dict) = get_named_resource_dict_from_resources(page_dict, doc, b"ExtGState") {
        if extgstate_dict_has_transparency(&gs_dict, doc) {
            return true;
        }
    }
    // Check annotations for transparency: /BM, /CA, /ca on the annotation dict,
    // and appearance streams with transparency groups or ExtGState transparency.
    if page_annots_use_transparency_lopdf(page_dict, doc) {
        return true;
    }
    // Check Form XObjects: a Form XObject with /Group /S /Transparency implies
    // the parent page uses transparency blending.
    if let Some(xobj_dict) = get_named_resource_dict_from_resources(page_dict, doc, b"XObject") {
        for (_, xobj_val) in xobj_dict.iter() {
            let stream_id = match xobj_val {
                Object::Reference(id) => *id,
                _ => continue,
            };
            let Some(Object::Stream(s)) = doc.objects.get(&stream_id) else {
                continue;
            };
            // Only Form XObjects.
            if s.dict.get(b"Subtype").ok() != Some(&Object::Name(b"Form".to_vec())) {
                continue;
            }
            // Form XObject with its own transparency group.
            if let Ok(Object::Dictionary(grp)) = s.dict.get(b"Group") {
                if grp.get(b"S").ok() == Some(&Object::Name(b"Transparency".to_vec())) {
                    return true;
                }
            }
            // Form XObject with ExtGState transparency in its own resources.
            if let Some(gs) =
                get_named_resource_dict_from_stream_resources(&s.dict, doc, b"ExtGState")
            {
                if extgstate_dict_has_transparency(&gs, doc) {
                    return true;
                }
            }
        }
    }
    false
}

/// Check whether annotations on a page use transparency features.
///
/// Mirrors the compliance checker's `page_annots_use_transparency`: checks
/// /BM, /CA, /ca on annotation dicts and transparency in appearance streams.
#[allow(dead_code)]
fn page_annots_use_transparency_lopdf(page_dict: &lopdf::Dictionary, doc: &Document) -> bool {
    let annots_arr = match page_dict.get(b"Annots").ok() {
        Some(Object::Array(arr)) => arr.clone(),
        Some(Object::Reference(id)) => match doc.objects.get(id) {
            Some(Object::Array(arr)) => arr.clone(),
            _ => return false,
        },
        _ => return false,
    };
    for annot_obj in &annots_arr {
        let annot = match annot_obj {
            Object::Reference(id) => match doc.objects.get(id) {
                Some(Object::Dictionary(d)) => d,
                _ => continue,
            },
            Object::Dictionary(d) => d,
            _ => continue,
        };
        // /BM on annotation dict
        if let Ok(Object::Name(bm)) = annot.get(b"BM") {
            if bm != b"Normal" && bm != b"Compatible" {
                return true;
            }
        }
        // /CA (stroke opacity) < 1
        if let Ok(Object::Real(ca)) = annot.get(b"CA") {
            if *ca < 1.0 {
                return true;
            }
        }
        if let Ok(Object::Real(ca)) = annot.get(b"ca") {
            if *ca < 1.0 {
                return true;
            }
        }
        // Check appearance streams (/AP /N).
        let ap = match annot.get(b"AP").ok() {
            Some(Object::Dictionary(d)) => d,
            Some(Object::Reference(id)) => match doc.objects.get(id) {
                Some(Object::Dictionary(d)) => d,
                _ => continue,
            },
            _ => continue,
        };
        // Collect all /N appearance streams (direct or state dict).
        let mut ap_ids: Vec<ObjectId> = Vec::new();
        match ap.get(b"N").ok() {
            Some(Object::Reference(id)) => ap_ids.push(*id),
            Some(Object::Dictionary(state_dict)) => {
                for (_, v) in state_dict.iter() {
                    if let Object::Reference(id) = v {
                        ap_ids.push(*id);
                    }
                }
            }
            _ => {}
        }
        for ap_id in ap_ids {
            let Some(Object::Stream(s)) = doc.objects.get(&ap_id) else {
                continue;
            };
            // Form XObject with transparency group
            if let Ok(Object::Dictionary(grp)) = s.dict.get(b"Group") {
                if grp.get(b"S").ok() == Some(&Object::Name(b"Transparency".to_vec())) {
                    return true;
                }
            }
            // ExtGState transparency in appearance resources
            if let Some(gs) =
                get_named_resource_dict_from_stream_resources(&s.dict, doc, b"ExtGState")
            {
                if extgstate_dict_has_transparency(&gs, doc) {
                    return true;
                }
            }
        }
    }
    false
}

/// Check whether any entry in an ExtGState dictionary uses transparency.
#[allow(dead_code)]
fn extgstate_dict_has_transparency(gs_dict: &lopdf::Dictionary, doc: &Document) -> bool {
    for (_, gs_val) in gs_dict.iter() {
        let gs = match gs_val {
            Object::Dictionary(d) => d.clone(),
            Object::Reference(id) => match doc.objects.get(id) {
                Some(Object::Dictionary(d)) => d.clone(),
                _ => continue,
            },
            _ => continue,
        };
        if extgstate_entry_has_transparency(&gs) {
            return true;
        }
    }
    false
}

/// Check a single ExtGState dict for transparency features.
#[allow(dead_code)]
fn extgstate_entry_has_transparency(gs: &lopdf::Dictionary) -> bool {
    // /SMask present and not /None
    match gs.get(b"SMask").ok() {
        Some(Object::Name(n)) if n == b"None" => {}
        Some(Object::Name(_)) | Some(Object::Dictionary(_)) | Some(Object::Reference(_)) => {
            return true
        }
        _ => {}
    }
    // /BM not Normal or Compatible
    if let Ok(Object::Name(bm)) = gs.get(b"BM") {
        if bm != b"Normal" && bm != b"Compatible" {
            return true;
        }
    }
    // /CA (stroke opacity) < 1
    if opacity_less_than_one(gs.get(b"CA").ok()) {
        return true;
    }
    // /ca (fill opacity) < 1
    if opacity_less_than_one(gs.get(b"ca").ok()) {
        return true;
    }
    false
}

#[allow(dead_code)]
fn opacity_less_than_one(obj: Option<&Object>) -> bool {
    match obj {
        Some(Object::Real(f)) => *f < 1.0,
        Some(Object::Integer(i)) => *i < 1,
        _ => false,
    }
}

/// Like `get_named_resource_dict_from_resources` but works on a stream dict
/// (Form XObject) rather than a page dict — the resource dict is found directly
/// under the stream dict's /Resources key.
#[allow(dead_code)]
fn get_named_resource_dict_from_stream_resources(
    stream_dict: &lopdf::Dictionary,
    doc: &Document,
    key: &[u8],
) -> Option<lopdf::Dictionary> {
    let resources = match stream_dict.get(b"Resources").ok() {
        Some(Object::Dictionary(d)) => d.clone(),
        Some(Object::Reference(id)) => match doc.objects.get(id) {
            Some(Object::Dictionary(d)) => d.clone(),
            _ => return None,
        },
        _ => return None,
    };
    match resources.get(key).ok() {
        Some(Object::Dictionary(d)) => Some(d.clone()),
        Some(Object::Reference(id)) => match doc.objects.get(id) {
            Some(Object::Dictionary(d)) => Some(d.clone()),
            _ => None,
        },
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// 6.2.11.2:1 — Font dictionary /Type entry repair
// ---------------------------------------------------------------------------
//
// Some PDFs have font dicts with an empty or wrong /Type entry (e.g. "" or
// an empty name), causing veraPDF §6.2.11.2:1 "A Font dictionary has value
// '' of Type entry instead of Font". Fix: set /Type /Font for all dicts that
// look like font dicts (have /Subtype equal to a known font subtype) but have
// a missing or non-"Font" /Type.

// ---------------------------------------------------------------------------
// 6.1.10:1 — Form XObject dictionaries shall include a BBox entry.
// ISO 32000-1 Table 95 lists BBox as required for Form XObjects.
// Some PDFs have Form XObjects (including appearance streams) that are missing
// this required entry.  Add a default [0 0 612 792] bbox so veraPDF passes.
// ---------------------------------------------------------------------------

fn fix_form_xobject_bbox(doc: &mut Document) -> usize {
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut fixed = 0;
    for id in ids {
        let needs_fix = match doc.objects.get(&id) {
            Some(Object::Stream(s)) => {
                let is_form =
                    s.dict.get(b"Subtype").ok().and_then(|o| o.as_name().ok()) == Some(b"Form");
                is_form && !s.dict.has(b"BBox")
            }
            _ => false,
        };
        if needs_fix {
            if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(&id) {
                // Use US Letter as a safe default — any rect satisfies the requirement.
                s.dict.set(
                    "BBox",
                    Object::Array(vec![
                        Object::Integer(0),
                        Object::Integer(0),
                        Object::Integer(612),
                        Object::Integer(792),
                    ]),
                );
                fixed += 1;
            }
        }
    }
    fixed
}

fn fix_font_type_entries(doc: &mut Document) -> usize {
    const FONT_SUBTYPES: &[&[u8]] = &[
        b"Type1",
        b"MMType1",
        b"TrueType",
        b"Type3",
        b"CIDFontType0",
        b"CIDFontType2",
        b"Type0",
    ];
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut fixed = 0;
    for id in ids {
        let needs_fix = match doc.objects.get(&id) {
            Some(Object::Dictionary(d)) => {
                let subtype_ok = d
                    .get(b"Subtype")
                    .ok()
                    .and_then(|o| match o {
                        Object::Name(n) => Some(FONT_SUBTYPES.contains(&n.as_slice())),
                        _ => None,
                    })
                    .unwrap_or(false);
                if !subtype_ok {
                    false
                } else {
                    match d.get(b"Type") {
                        Ok(Object::Name(n)) => n.as_slice() != b"Font",
                        Ok(Object::String(s, _)) => {
                            // Empty string or wrong string value.
                            s.is_empty() || s.as_slice() != b"Font"
                        }
                        Err(_) => true, // Missing /Type.
                        _ => false,
                    }
                }
            }
            _ => false,
        };
        if needs_fix {
            if let Some(Object::Dictionary(d)) = doc.objects.get_mut(&id) {
                d.set("Type", Object::Name(b"Font".to_vec()));
                fixed += 1;
            }
        }
    }
    fixed
}

// ---------------------------------------------------------------------------
// 6.1.13:2 — Truncate name tokens > 127 bytes in content streams.
// ---------------------------------------------------------------------------

fn fix_long_names_in_streams(doc: &mut Document) -> usize {
    const MAX_NAME_LEN: usize = 127;
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

        let mut new_content = Vec::with_capacity(decompressed.len());
        let mut i = 0;
        let mut fixed_any = false;
        let mut string_depth = 0u32;
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
                b'/' => {
                    // PDF name token starting at i.
                    let start = i;
                    i += 1;
                    while i < decompressed.len() && !is_pdf_delimiter(decompressed[i]) && !decompressed[i].is_ascii_whitespace() {
                        i += 1;
                    }
                    let name_token = &decompressed[start..i];
                    let name_val = &name_token[1..]; // skip '/'
                    
                    let mut modified_name = false;
                    let mut sanitized = Vec::new();
                    
                    // 1. Sanitize for UTF-8 validity (§6.1.8).
                    if String::from_utf8(name_val.to_vec()).is_err() {
                        sanitized = name_val.iter().map(|&b| if b.is_ascii_graphic() || b == b' ' { b } else { b'_' }).collect();
                        modified_name = true;
                    }
                    
                    // 2. Truncate to 127 bytes (§6.1.13).
                    let current_name = if sanitized.is_empty() { name_val } else { &sanitized };
                    if current_name.len() > MAX_NAME_LEN {
                        if sanitized.is_empty() { sanitized = name_val.to_vec(); }
                        sanitized.truncate(MAX_NAME_LEN);
                        modified_name = true;
                    }

                    if modified_name {
                        new_content.push(b'/');
                        new_content.extend_from_slice(if sanitized.is_empty() { name_val } else { &sanitized });
                        fixed_any = true;
                        count += 1;
                    } else {
                        new_content.extend_from_slice(name_token);
                    }
                    continue;
                }
                _ => {
                    new_content.push(b);
                    i += 1;
                    continue;
                }
            }
        }

        if fixed_any {
            if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(&id) {
                s.set_plain_content(new_content);
            }
        }
    }
    count
}

// ---------------------------------------------------------------------------
// 6.1.13:2 — Truncate dictionary key names > 127 bytes in PDF objects.
// Also truncates Name values > 127 bytes.
// ---------------------------------------------------------------------------

/// Compute the serialized length of a PDF name (lopdf hex-encodes non-printable bytes).
fn name_serialized_len(name: &[u8]) -> usize {
    name.iter()
        .map(|&b| {
            if b" \t\n\r\x0C()<>[]{}/%#".contains(&b) || !(33..=126).contains(&b) {
                3 // #XX
            } else {
                1
            }
        })
        .sum()
}

/// Truncate a name so its serialized form is at most `max_len` bytes.
fn truncate_name_for_serialization(name: &[u8], max_len: usize) -> Vec<u8> {
    let mut serialized_len = 0;
    let mut end = 0;
    for &b in name {
        let char_len = if b" \t\n\r\x0C()<>[]{}/%#".contains(&b) || !(33..=126).contains(&b) {
            3
        } else {
            1
        };
        if serialized_len + char_len > max_len {
            break;
        }
        serialized_len += char_len;
        end += 1;
    }
    name[..end].to_vec()
}

fn fix_long_dict_keys(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();

    for id in ids {
        let obj = match doc.objects.get(&id) {
            Some(o) => o.clone(),
            None => continue,
        };
        let (fixed, n) = fix_long_keys_in_object(obj, 0);
        if n > 0 {
            doc.objects.insert(id, fixed);
            count += n;
        }
    }
    count
}

fn fix_long_keys_in_object(obj: Object, depth: usize) -> (Object, usize) {
    if depth > MAX_OBJECT_DEPTH {
        return (obj, 0);
    }
    match obj {
        Object::Name(ref n) if name_serialized_len(n) > 127 => {
            (Object::Name(truncate_name_for_serialization(n, 127)), 1)
        }
        Object::Array(arr) => {
            let mut total = 0;
            let new_arr: Vec<Object> = arr
                .into_iter()
                .map(|o| {
                    let (fixed, n) = fix_long_keys_in_object(o, depth + 1);
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
                let truncated_key = if name_serialized_len(&key) > 127 {
                    total += 1;
                    truncate_name_for_serialization(&key, 127)
                } else {
                    key
                };
                let (fixed_val, n) = fix_long_keys_in_object(val, depth + 1);
                total += n;
                new_dict.set(truncated_key, fixed_val);
            }
            (Object::Dictionary(new_dict), total)
        }
        Object::Stream(mut s) => {
            let mut total = 0;
            let mut new_dict = lopdf::Dictionary::new();
            for (key, val) in s.dict.into_iter() {
                let truncated_key = if name_serialized_len(&key) > 127 {
                    total += 1;
                    truncate_name_for_serialization(&key, 127)
                } else {
                    key
                };
                let (fixed_val, n) = fix_long_keys_in_object(val, depth + 1);
                total += n;
                new_dict.set(truncated_key, fixed_val);
            }
            s.dict = new_dict;
            (Object::Stream(s), total)
        }
        other => (other, 0),
    }
}

// ---------------------------------------------------------------------------
// 6.1.13:3 — Truncate literal strings > 32767 bytes in content streams.
//
// fix_long_strings handles strings in PDF objects; this covers strings inside
// content streams where they appear as raw `(...)` tokens.
// ---------------------------------------------------------------------------

fn fix_long_strings_in_streams(doc: &mut Document) -> usize {
    const MAX_STRING_LEN: usize = 32767;
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

        // Quick check: skip streams shorter than the limit.
        if decompressed.len() <= MAX_STRING_LEN {
            continue;
        }

        let mut new_content = Vec::with_capacity(decompressed.len());
        let mut i = 0;
        let mut fixed_any = false;

        while i < decompressed.len() {
            let b = decompressed[i];
            if b == b'(' {
                // Scan to matching ')' respecting nesting and escapes.
                let start = i;
                i += 1;
                let mut depth = 1u32;
                let mut esc = false;
                while i < decompressed.len() && depth > 0 {
                    if esc {
                        esc = false;
                        i += 1;
                        continue;
                    }
                    match decompressed[i] {
                        b'\\' => esc = true,
                        b'(' => depth += 1,
                        b')' => depth -= 1,
                        _ => {}
                    }
                    i += 1;
                }
                // decompressed[start..i] is the full string including parens.
                let string_body_len = if i > start + 1 { i - start - 2 } else { 0 };
                if string_body_len > MAX_STRING_LEN {
                    new_content.push(b'(');
                    new_content
                        .extend_from_slice(&decompressed[start + 1..start + 1 + MAX_STRING_LEN]);
                    new_content.push(b')');
                    fixed_any = true;
                    count += 1;
                } else {
                    new_content.extend_from_slice(&decompressed[start..i]);
                }
            } else {
                new_content.push(b);
                i += 1;
            }
        }

        if fixed_any {
            if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(&id) {
                s.set_plain_content(new_content);
            }
        }
    }
    count
}

// ---------------------------------------------------------------------------
// 6.7.4 — Fix invalid /Lang values in Catalog and StructElem dicts.
//
// PDF/A requires /Lang values to be valid BCP 47 language tags. Common
// violations:
// - "x-unknown" (used by some generators as placeholder)
// - Wrong case: "EN-US" instead of "en-US", "DE" instead of "de"
// - Invalid tags: empty strings, garbage values
// ---------------------------------------------------------------------------

fn fix_invalid_lang_values(doc: &mut Document) -> usize {
    let mut count = 0;

    // Fix /Lang in all objects (covers Catalog, StructElem, and anything else).
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let action = if let Some(Object::Dictionary(dict)) = doc.objects.get(&id) {
            match dict.get(b"Lang") {
                Ok(Object::String(bytes, _)) => {
                    let s = String::from_utf8_lossy(bytes);
                    let s = s.trim();
                    if s.is_empty() {
                        Some(LangAction::Remove)
                    } else if !is_valid_bcp47(s) {
                        match normalize_bcp47(s) {
                            Some(normalized) if normalized != s => {
                                Some(LangAction::Set(normalized))
                            }
                            Some(_) => None, // already valid
                            None => Some(LangAction::Remove),
                        }
                    } else {
                        None
                    }
                }
                Ok(Object::Name(bytes)) => {
                    // Some PDFs use Name instead of String for /Lang.
                    let s = String::from_utf8_lossy(bytes);
                    let s_ref = s.trim();
                    if s_ref.is_empty() {
                        Some(LangAction::Remove)
                    } else {
                        match normalize_bcp47(s_ref) {
                            Some(normalized) => Some(LangAction::Set(normalized)),
                            None => Some(LangAction::Remove),
                        }
                    }
                }
                _ => None,
            }
        } else {
            None
        };
        match action {
            Some(LangAction::Set(val)) => {
                if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                    dict.set(
                        "Lang",
                        Object::String(val.into_bytes(), lopdf::StringFormat::Literal),
                    );
                    count += 1;
                }
            }
            Some(LangAction::Remove) => {
                if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
                    dict.remove(b"Lang");
                    count += 1;
                }
            }
            None => {}
        }
    }

    count
}

enum LangAction {
    Set(String),
    Remove,
}

/// Check if a string is a valid BCP 47 language tag with correct casing.
fn is_valid_bcp47(tag: &str) -> bool {
    let tag = tag.trim();
    if tag.is_empty() || tag.eq_ignore_ascii_case("x-unknown") {
        return false;
    }

    let parts: Vec<&str> = tag.split('-').collect();
    if parts.is_empty() {
        return false;
    }

    // Primary subtag: 2-3 lowercase alpha.
    let primary = parts[0];
    if primary.len() < 2 || primary.len() > 3 || !primary.bytes().all(|b| b.is_ascii_lowercase()) {
        return false;
    }

    for part in parts.iter().skip(1) {
        if part.is_empty() {
            return false;
        }
        if part.len() == 2 && part.bytes().all(|b| b.is_ascii_alphabetic()) {
            // Region: must be UPPERCASE.
            if !part.bytes().all(|b| b.is_ascii_uppercase()) {
                return false;
            }
        } else if part.len() == 4 && part.bytes().all(|b| b.is_ascii_alphabetic()) {
            // Script: must be Title Case.
            let mut chars = part.chars();
            if let Some(first) = chars.next() {
                if !first.is_ascii_uppercase() {
                    return false;
                }
                if !chars.all(|c| c.is_ascii_lowercase()) {
                    return false;
                }
            }
        } else if !part.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return false;
        }
    }
    true
}

/// Normalize a BCP 47 language tag. Returns `Some(normalized)` if the tag can
/// be fixed, `None` if it should be removed (e.g. "x-unknown", empty, garbage).
fn normalize_bcp47(tag: &str) -> Option<String> {
    let tag = tag.trim();
    if tag.is_empty() || tag.eq_ignore_ascii_case("x-unknown") {
        return None;
    }

    let parts: Vec<&str> = tag.split('-').collect();
    if parts.is_empty() {
        return None;
    }

    // Primary language subtag: 2-3 letter alpha, lowercase.
    let primary = parts[0];
    if primary.len() < 2 || primary.len() > 3 || !primary.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }

    let mut result = primary.to_ascii_lowercase();

    for part in parts.iter().skip(1) {
        if part.is_empty() {
            continue;
        }
        result.push('-');
        if part.len() == 2 && part.bytes().all(|b| b.is_ascii_alphabetic()) {
            // Region subtag: 2 letters, UPPERCASE.
            result.push_str(&part.to_ascii_uppercase());
        } else if part.len() == 4 && part.bytes().all(|b| b.is_ascii_alphabetic()) {
            // Script subtag: 4 letters, Title Case.
            let mut chars = part.chars();
            if let Some(first) = chars.next() {
                result.push(first.to_ascii_uppercase());
                for c in chars {
                    result.push(c.to_ascii_lowercase());
                }
            }
        } else if part.bytes().all(|b| b.is_ascii_alphanumeric()) {
            // Other subtags (variant, extension): lowercase.
            result.push_str(&part.to_ascii_lowercase());
        } else {
            return None;
        }
    }

    Some(result)
}

// ---------------------------------------------------------------------------
// 6.1.13: Truncate oversized dictionaries (>4095) and arrays (>8191).
// ---------------------------------------------------------------------------

fn fix_long_containers(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let obj = match doc.objects.get(&id) {
            Some(o) => o.clone(),
            None => continue,
        };
        let (fixed, n) = fix_long_containers_in_object(obj, 0);
        if n > 0 {
            doc.objects.insert(id, fixed);
            count += n;
        }
    }
    count
}

fn fix_long_containers_in_object(obj: Object, depth: usize) -> (Object, usize) {
    if depth > MAX_OBJECT_DEPTH {
        return (obj, 0);
    }
    match obj {
        Object::Array(mut arr) => {
            let mut total = 0;
            if arr.len() > 8191 {
                arr.truncate(8191);
                total += 1;
            }
            let new_arr: Vec<Object> = arr
                .into_iter()
                .map(|o| {
                    let (fixed, n) = fix_long_containers_in_object(o, depth + 1);
                    total += n;
                    fixed
                })
                .collect();
            (Object::Array(new_arr), total)
        }
        Object::Dictionary(mut dict) => {
            let mut total = 0;
            if dict.len() > 4095 {
                let keys: Vec<Vec<u8>> = dict.iter().map(|(k, _)| k.clone()).collect();
                for key in keys.into_iter().skip(4095) {
                    dict.remove(&key);
                }
                total += 1;
            }
            let mut new_dict = lopdf::Dictionary::new();
            for (key, val) in dict.into_iter() {
                let (fixed, n) = fix_long_containers_in_object(val, depth + 1);
                total += n;
                new_dict.set(key, fixed);
            }
            (Object::Dictionary(new_dict), total)
        }
        Object::Stream(mut s) => {
            let mut total = 0;
            if s.dict.len() > 4095 {
                let keys: Vec<Vec<u8>> = s.dict.iter().map(|(k, _)| k.clone()).collect();
                for key in keys.into_iter().skip(4095) {
                    s.dict.remove(&key);
                }
                total += 1;
            }
            let mut new_dict = lopdf::Dictionary::new();
            for (key, val) in s.dict.into_iter() {
                let (fixed, n) = fix_long_containers_in_object(val, depth + 1);
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
// 6.1.8 — Name value is not a valid UTF-8 sequence.
// ---------------------------------------------------------------------------

fn fix_non_utf8_names(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let obj = match doc.objects.get(&id) {
            Some(o) => o.clone(),
            None => continue,
        };
        let (fixed, n) = sanitize_names_in_object(obj, 0);
        if n > 0 {
            doc.objects.insert(id, fixed);
            count += n;
        }
    }
    count
}

fn sanitize_names_in_object(obj: Object, depth: usize) -> (Object, usize) {
    if depth > MAX_OBJECT_DEPTH {
        return (obj, 0);
    }
    match obj {
        Object::Name(bytes) => {
            let mut total = 0;
            let mut sanitized = bytes;
            if String::from_utf8(sanitized.clone()).is_err() {
                sanitized = sanitized
                    .into_iter()
                    .map(|b| if b.is_ascii_graphic() || b == b' ' { b } else { b'_' })
                    .collect();
                total += 1;
            }
            if name_serialized_len(&sanitized) > 127 {
                sanitized = truncate_name_for_serialization(&sanitized, 127);
                total += 1;
            }
            (Object::Name(sanitized), total)
        }
        Object::Array(arr) => {
            let mut total = 0;
            let new_arr: Vec<Object> = arr
                .into_iter()
                .map(|o| {
                    let (fixed, n) = sanitize_names_in_object(o, depth + 1);
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
                let mut fixed_key = key;
                if String::from_utf8(fixed_key.clone()).is_err() {
                    fixed_key = fixed_key
                        .into_iter()
                        .map(|b| if b.is_ascii_graphic() || b == b' ' { b } else { b'_' })
                        .collect();
                    total += 1;
                }
                if name_serialized_len(&fixed_key) > 127 {
                    fixed_key = truncate_name_for_serialization(&fixed_key, 127);
                    total += 1;
                }
                let (fixed_val, n) = sanitize_names_in_object(val, depth + 1);
                total += n;
                new_dict.set(fixed_key, fixed_val);
            }
            (Object::Dictionary(new_dict), total)
        }
        Object::Stream(mut s) => {
            let mut total = 0;
            let mut new_dict = lopdf::Dictionary::new();
            for (key, val) in s.dict.into_iter() {
                let mut fixed_key = key;
                if String::from_utf8(fixed_key.clone()).is_err() {
                    fixed_key = fixed_key
                        .into_iter()
                        .map(|b| if b.is_ascii_graphic() || b == b' ' { b } else { b'_' })
                        .collect();
                    total += 1;
                }
                if name_serialized_len(&fixed_key) > 127 {
                    fixed_key = truncate_name_for_serialization(&fixed_key, 127);
                    total += 1;
                }
                let (fixed_val, n) = sanitize_names_in_object(val, depth + 1);
                total += n;
                new_dict.set(fixed_key, fixed_val);
            }
            s.dict = new_dict;
            (Object::Stream(s), total)
        }
        other => (other, 0),
    }
}

// ---------------------------------------------------------------------------
// 6.3.3 — Text/Highlight/etc annotation missing /AP (appearance dict).
// ---------------------------------------------------------------------------

fn fix_missing_annot_appearances_extra(doc: &mut Document) -> usize {
    let mut count = 0;
    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    for page_id in page_ids {
        let annots = match doc.get_object(page_id) {
            Ok(Object::Dictionary(ref d)) => match d.get(b"Annots") {
                Ok(Object::Array(ref a)) => a.clone(),
                _ => continue,
            },
            _ => continue,
        };

        // Collect annot IDs + rects first, then mutate in a second pass
        // to avoid double-mutable-borrow of doc.
        let mut needs_ap: Vec<(ObjectId, Vec<Object>)> = Vec::new();
        for annot_ref in annots {
            let annot_id = match annot_ref {
                Object::Reference(id) => id,
                _ => continue,
            };
            if let Ok(Object::Dictionary(ref annot_dict)) = doc.get_object(annot_id) {
                let is_annot = matches!(annot_dict.get(b"Type").ok(), Some(Object::Name(ref n)) if n == b"Annotation" || n == b"Annot");
                if is_annot && !annot_dict.has(b"AP") {
                    let rect = match annot_dict.get(b"Rect") {
                        Ok(Object::Array(a)) => a.clone(),
                        _ => vec![0.into(), 0.into(), 1.into(), 1.into()],
                    };
                    needs_ap.push((annot_id, rect));
                }
            }
        }
        for (annot_id, rect) in needs_ap {
            let ap_stream = create_empty_ap_stream_extra(doc, &rect);
            if let Ok(Object::Dictionary(ref mut annot_dict)) = doc.get_object_mut(annot_id) {
                annot_dict.set("AP", dictionary! { "N" => Object::Reference(ap_stream) });
                count += 1;
            }
        }
    }
    count
}

fn obj_as_f64(obj: &Object) -> Option<f64> {
    match obj {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(f) => Some(*f as f64),
        _ => None,
    }
}

fn create_empty_ap_stream_extra(doc: &mut Document, rect: &[Object]) -> ObjectId {
    let bbox = if rect.len() == 4 {
        let x1 = obj_as_f64(&rect[0]).unwrap_or(0.0);
        let y1 = obj_as_f64(&rect[1]).unwrap_or(0.0);
        let x2 = obj_as_f64(&rect[2]).unwrap_or(1.0);
        let y2 = obj_as_f64(&rect[3]).unwrap_or(1.0);
        vec![0.into(), 0.into(), (x2 - x1).into(), (y2 - y1).into()]
    } else {
        vec![0.into(), 0.into(), 1.into(), 1.into()]
    };

    let dict = dictionary! {
        "Type" => "XObject",
        "Subtype" => "Form",
        "BBox" => Object::Array(bbox),
        "Resources" => dictionary! {},
    };
    doc.add_object(Object::Stream(lopdf::Stream::new(dict, Vec::new())))
}

// ---------------------------------------------------------------------------
// 6.1.6 — Hexadecimal string contains non-hex characters.
// ---------------------------------------------------------------------------

fn fix_hex_string_garbage(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let obj = match doc.objects.get(&id) {
            Some(o) => o.clone(),
            None => continue,
        };
        let (fixed, n) = clean_hex_in_object(obj, 0);
        if n > 0 {
            doc.objects.insert(id, fixed);
            count += n;
        }
    }
    count
}

fn clean_hex_in_object(obj: Object, depth: usize) -> (Object, usize) {
    if depth > MAX_OBJECT_DEPTH {
        return (obj, 0);
    }
    match obj {
        Object::String(bytes, lopdf::StringFormat::Hexadecimal) => {
            (Object::String(bytes, lopdf::StringFormat::Hexadecimal), 0)
        }
        Object::Array(arr) => {
            let mut total = 0;
            let new_arr: Vec<Object> = arr
                .into_iter()
                .map(|o| {
                    let (fixed, n) = clean_hex_in_object(o, depth + 1);
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
                let (fixed, n) = clean_hex_in_object(val, depth + 1);
                total += n;
                new_dict.set(key, fixed);
            }
            (Object::Dictionary(new_dict), total)
        }
        Object::Stream(mut s) => {
            let mut total = 0;
            let mut new_dict = lopdf::Dictionary::new();
            for (key, val) in s.dict.into_iter() {
                let (fixed, n) = clean_hex_in_object(val, depth + 1);
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
// 6.1.7.1 — Remove forbidden external file references from stream dicts.
// ---------------------------------------------------------------------------

fn fix_stream_external_ref_keys_extra(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(&id) {
            if s.dict.has(b"F") || s.dict.has(b"FFilter") || s.dict.has(b"FDecodeParms") {
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
// 6.1.6.2 — Promote inline JBIG2Globals to indirect objects.
// ---------------------------------------------------------------------------

fn fix_jbig2_globals_promotion(doc: &mut Document) -> usize {
    let mut count = 0;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let mut globals_to_move = None;
        if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(&id) {
            // §6.1.6.2: JBIG2Globals MUST NOT be in DecodeParms.
            if let Ok(Object::Dictionary(ref mut dp)) = s.dict.get_mut(b"DecodeParms") {
                if let Ok(globals) = dp.get(b"JBIG2Globals") {
                    globals_to_move = Some(globals.clone());
                    dp.remove(b"JBIG2Globals");
                }
            } else if let Ok(Object::Array(ref mut dpa)) = s.dict.get_mut(b"DecodeParms") {
                for item in dpa.iter_mut() {
                    if let Object::Dictionary(ref mut dp) = item {
                        if let Ok(globals) = dp.get(b"JBIG2Globals") {
                            globals_to_move = Some(globals.clone());
                            dp.remove(b"JBIG2Globals");
                            break;
                        }
                    }
                }
            }
            
            // Also check if it's already in the stream dict but inline.
            if globals_to_move.is_none() {
                if let Ok(globals) = s.dict.get(b"JBIG2Globals") {
                    if !matches!(globals, Object::Reference(_)) {
                        globals_to_move = Some(globals.clone());
                    }
                }
            }
        }

        if let Some(globals_obj) = globals_to_move {
            let gid = if let Object::Reference(rid) = globals_obj {
                rid
            } else {
                doc.add_object(globals_obj)
            };
            if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(&id) {
                s.dict.set("JBIG2Globals", Object::Reference(gid));
                count += 1;
            }
        }
    }
    count
}

// ---------------------------------------------------------------------------
// §6.2.4.3 — Fix DeviceCMYK usage when OutputIntent is not CMYK.
// ---------------------------------------------------------------------------

fn fix_device_cmyk_intent_mismatch(doc: &mut Document) -> usize {
    let mut count = 0;
    
    // Check if OutputIntent is CMYK.
    let intent_is_cmyk = if let Ok(catalog_id) = doc.catalog() {
        if let Ok(catalog) = doc.get_object(catalog_id.0).and_then(|o| o.as_dict()) {
            if let Ok(oi_arr) = catalog.get(b"OutputIntents").and_then(|o| o.as_array()) {
                oi_arr.iter().any(|oi| {
                    let dict = if let Ok(d) = oi.as_dict() {
                        Some(d)
                    } else if let Ok(id) = oi.as_reference() {
                        doc.get_object(id).ok().and_then(|o| o.as_dict())
                    } else {
                        None
                    };
                    if let Some(d) = dict {
                        if d.get(b"S").ok().and_then(|o| o.as_name().ok()) == Some(b"GTS_PDFA1") {
                            if let Ok(profile_id) = d.get(b"DestOutputProfile").and_then(|o| o.as_reference()) {
                                if let Some(Object::Stream(s)) = doc.objects.get(&profile_id) {
                                    return s.dict.get(b"N").ok().and_then(|o| o.as_i64().ok()) == Some(4);
                                }
                            }
                        }
                    }
                    false
                })
            } else { false }
        } else { false }
    } else { false };

    if intent_is_cmyk {
        return 0; // Compliant.
    }

    // If DeviceCMYK is used, but intent is not CMYK, we convert DeviceCMYK to
    // an ICCBased CMYK colorspace.
    let mut cmyk_profile_id = None;
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let mut modified = false;
        if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&id) {
            if let Ok(Object::Dictionary(ref mut cs_dict)) = dict.get_mut(b"ColorSpace") {
                for (_, val) in cs_dict.iter_mut() {
                    if val.as_name().ok() == Some(b"DeviceCMYK") {
                        if cmyk_profile_id.is_none() {
                            cmyk_profile_id = Some(ensure_cmyk_profile_extra(doc));
                        }
                        if let Some(pid) = cmyk_profile_id {
                            *val = Object::Array(vec![
                                Object::Name(b"ICCBased".to_vec()),
                                Object::Reference(pid),
                            ]);
                            modified = true;
                            count += 1;
                        }
                    }
                }
            }
        }
        if !modified {
            if let Some(Object::Stream(ref mut s)) = doc.objects.get_mut(&id) {
                if let Ok(Object::Dictionary(ref mut cs_dict)) = s.dict.get_mut(b"ColorSpace") {
                    for (_, val) in cs_dict.iter_mut() {
                        if val.as_name().ok() == Some(b"DeviceCMYK") {
                            if cmyk_profile_id.is_none() {
                                cmyk_profile_id = Some(ensure_cmyk_profile_extra(doc));
                            }
                            if let Some(pid) = cmyk_profile_id {
                                *val = Object::Array(vec![
                                    Object::Name(b"ICCBased".to_vec()),
                                    Object::Reference(pid),
                                ]);
                                count += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    count
}

fn ensure_cmyk_profile_extra(doc: &mut Document) -> ObjectId {
    // Look for any existing CMYK profile first.
    for (&id, obj) in &doc.objects {
        if let Object::Stream(ref s) = obj {
            if s.dict.get(b"N").ok().and_then(|o| o.as_i64().ok()) == Some(4) {
                if s.dict.get(b"Type").ok().and_then(|o| o.as_name().ok()) == Some(b"ICCBased") {
                    return id;
                }
            }
        }
    }
    // Fallback: create a minimal one.
    let dict = dictionary! {
        "Type" => "ICCBased",
        "N" => 4,
    };
    doc.add_object(Object::Stream(lopdf::Stream::new(dict, Vec::new())))
}

// ---------------------------------------------------------------------------
// §6.2.4.2 — Fix ICC profile reuse.
// ---------------------------------------------------------------------------

fn fix_icc_profile_reuse(doc: &mut Document) -> usize {
    let mut count = 0;
    let oi_profile_id = if let Ok(catalog_id) = doc.catalog() {
        if let Ok(catalog) = doc.get_object(catalog_id.0).and_then(|o| o.as_dict()) {
            if let Ok(oi_arr) = catalog.get(b"OutputIntents").and_then(|o| o.as_array()) {
                oi_arr.iter().find_map(|oi| {
                    let dict = if let Ok(d) = oi.as_dict() {
                        Some(d)
                    } else if let Ok(id) = oi.as_reference() {
                        doc.get_object(id).ok().and_then(|o| o.as_dict())
                    } else {
                        None
                    };
                    dict.and_then(|d| d.get(b"DestOutputProfile").ok()).and_then(|o| o.as_reference().ok())
                })
            } else { None }
        } else { None }
    } else { None };

    let Some(profile_id) = oi_profile_id else { return 0; };

    // Find all ICCBased colorspaces that use this profile_id.
    let mut to_replace = Vec::new();
    for (&id, obj) in &doc.objects {
        if let Object::Array(ref arr) = obj {
            if arr.len() == 2 && arr[0].as_name().ok() == Some(b"ICCBased") {
                if arr[1].as_reference().ok() == Some(profile_id) {
                    to_replace.push(id);
                }
            }
        }
    }

    if !to_replace.is_empty() {
        if let Ok(profile_obj) = doc.get_object(profile_id).cloned() {
            let new_profile_id = doc.add_object(profile_obj);
            for id in to_replace {
                if let Some(Object::Array(ref mut arr)) = doc.objects.get_mut(&id) {
                    arr[1] = Object::Reference(new_profile_id);
                    count += 1;
                }
            }
        }
    }

    count
}

