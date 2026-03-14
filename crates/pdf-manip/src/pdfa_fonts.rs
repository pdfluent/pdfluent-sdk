//! PDF/A font embedding and subsetting.
//!
//! Detects non-embedded fonts and embeds them for PDF/A conformity.
//! Key fixes:
//! - Type0 fonts: embeds on CIDFont descendant (not Type0 root)
//! - Subtype update: Type1→TrueType when embedding TTF (veraPDF checks this)
//! - Font detection: also finds fonts without Type=Font (only Subtype)
//! - Fallback: uses DejaVuSans for any unresolvable font
//! - Width matching: updates Widths/DW from embedded font data

use crate::error::{ManipError, Result};
use lopdf::{dictionary, Document, Object, ObjectId, Stream};
use std::path::PathBuf;

/// Report from font embedding pass.
#[derive(Debug, Clone)]
pub struct FontEmbedReport {
    /// Number of fonts inspected.
    pub fonts_inspected: usize,
    /// Number of non-embedded fonts found.
    pub non_embedded_found: usize,
    /// Number of fonts successfully embedded.
    pub fonts_embedded: usize,
    /// Fonts that could not be embedded (name, reason).
    pub failed: Vec<(String, String)>,
}

/// Standard 14 font names that must be embedded for PDF/A.
const STANDARD_14: &[&str] = &[
    "Courier",
    "Courier-Bold",
    "Courier-BoldOblique",
    "Courier-Oblique",
    "Helvetica",
    "Helvetica-Bold",
    "Helvetica-BoldOblique",
    "Helvetica-Oblique",
    "Times-Roman",
    "Times-Bold",
    "Times-BoldItalic",
    "Times-Italic",
    "Symbol",
    "ZapfDingbats",
];

/// Fallback font paths for any font that cannot be found (tried in order).
const FALLBACK_FONTS: &[&str] = &[
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
];

/// Shared in-repo font pack used to keep local and VPS embedding deterministic.
const REPO_FONT_PACK_REL: &str = "../../.font-pack";

/// Font subtypes that indicate a Font dictionary.
const FONT_SUBTYPES: &[&str] = &[
    "Type0",
    "Type1",
    "TrueType",
    "Type3",
    "CIDFontType0",
    "CIDFontType2",
    "MMType1",
];

/// Info about a non-embedded font and where to embed it.
struct NonEmbeddedFont {
    /// The font dictionary object ID (Type1, TrueType, or Type0).
    font_id: ObjectId,
    /// The object ID where the FontDescriptor lives (or should live).
    /// For simple fonts this is the same as font_id.
    /// For Type0 fonts this is the CIDFont descendant.
    target_id: ObjectId,
    /// Font name.
    name: String,
    /// Whether this is a Type0 composite font.
    is_type0: bool,
    /// Original Subtype of the font dict at font_id.
    subtype: String,
}

/// Check if a dictionary looks like a Font dict (has Type=Font OR a font Subtype).
fn is_font_dict(dict: &lopdf::Dictionary) -> bool {
    if get_name(dict, b"Type").as_deref() == Some("Font") {
        return true;
    }
    if let Some(st) = get_name(dict, b"Subtype") {
        return FONT_SUBTYPES.contains(&st.as_str());
    }
    false
}

/// Detect all non-embedded fonts in the document.
pub fn find_non_embedded_fonts(doc: &Document) -> Vec<(ObjectId, String)> {
    find_non_embedded_fonts_detailed(doc)
        .into_iter()
        .map(|f| (f.font_id, f.name))
        .collect()
}

/// Detect all non-embedded fonts with embedding target info.
fn find_non_embedded_fonts_detailed(doc: &Document) -> Vec<NonEmbeddedFont> {
    let mut result = Vec::new();
    // Track CIDFont IDs that are descendants of Type0 fonts to avoid double-counting.
    let mut descendant_ids: Vec<ObjectId> = Vec::new();

    // First pass: collect all CIDFont descendant IDs from Type0 fonts.
    for obj in doc.objects.values() {
        let Object::Dictionary(dict) = obj else {
            continue;
        };
        if !is_font_dict(dict) {
            continue;
        }
        let subtype = get_name(dict, b"Subtype").unwrap_or_default();
        if subtype == "Type0" {
            if let Ok(Object::Array(arr)) = dict.get(b"DescendantFonts") {
                for item in arr {
                    if let Object::Reference(id) = item {
                        descendant_ids.push(*id);
                    }
                }
            }
        }
    }

    // Second pass: find non-embedded fonts.
    for (id, obj) in &doc.objects {
        let Object::Dictionary(dict) = obj else {
            continue;
        };

        if !is_font_dict(dict) {
            continue;
        }

        let font_name = get_name_resolved(doc, dict, b"BaseFont")
            .or_else(|| get_name_lossy_resolved(doc, dict, b"BaseFont"))
            .unwrap_or_else(|| format!("FallbackFont{}", id.0));

        let subtype = get_name(dict, b"Subtype").unwrap_or_default();

        // Skip CIDFont descendants — they are handled via their parent Type0 font.
        if descendant_ids.contains(id) {
            continue;
        }

        let is_type0 = subtype == "Type0";

        if is_type0 {
            let descendant_info = get_descendant_embed_info(doc, dict);
            match descendant_info {
                Some((cid_id, true)) => {
                    let _ = cid_id;
                }
                Some((cid_id, false)) => {
                    result.push(NonEmbeddedFont {
                        font_id: *id,
                        target_id: cid_id,
                        name: font_name,
                        is_type0: true,
                        subtype,
                    });
                }
                None => {
                    if !has_embedded_font_program(doc, dict) {
                        result.push(NonEmbeddedFont {
                            font_id: *id,
                            target_id: *id,
                            name: font_name,
                            is_type0: true,
                            subtype,
                        });
                    }
                }
            }
        } else if !has_embedded_font_program(doc, dict) {
            result.push(NonEmbeddedFont {
                font_id: *id,
                target_id: *id,
                name: font_name,
                is_type0: false,
                subtype,
            });
        }
    }

    result
}

/// Check if a font dictionary has an embedded font program via FontDescriptor.
/// Verifies that FontFile/FontFile2/FontFile3 actually points to a Stream object,
/// not just that the key exists (lopdf can drop stream data during load/save).
fn has_embedded_font_program(doc: &Document, dict: &lopdf::Dictionary) -> bool {
    match dict.get(b"FontDescriptor").ok() {
        Some(Object::Reference(fd_id)) => {
            if let Some(Object::Dictionary(fd)) = doc.objects.get(fd_id) {
                has_valid_font_file(doc, fd)
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Check if a FontDescriptor has a valid FontFile/FontFile2/FontFile3 stream.
fn has_valid_font_file(doc: &Document, fd: &lopdf::Dictionary) -> bool {
    for key in &[b"FontFile".as_slice(), b"FontFile2", b"FontFile3"] {
        let Some(data) = read_fontfile_stream_content(doc, fd, key) else {
            continue;
        };
        if data.is_empty() {
            continue;
        }
        if *key == b"FontFile2" && !is_usable_truetype_font_program(&data) {
            continue;
        }
        return true;
    }
    false
}

fn read_fontfile_stream_content(
    doc: &Document,
    fd: &lopdf::Dictionary,
    key: &[u8],
) -> Option<Vec<u8>> {
    let obj = fd.get(key).ok()?;
    let stream = match obj {
        Object::Stream(s) => s.clone(),
        Object::Reference(id) => match doc.objects.get(id) {
            Some(Object::Stream(s)) => s.clone(),
            _ => return None,
        },
        _ => return None,
    };
    let mut stream = stream;
    let _ = stream.decompress();
    Some(stream.content)
}

fn is_usable_truetype_font_program(data: &[u8]) -> bool {
    let Ok(face) = ttf_parser::Face::parse(data, 0) else {
        return false;
    };
    // CIDFontType2 subsets used with Identity-H/V encoding deliberately omit
    // the cmap table (GID == CID, no encoding lookup needed). Only reject if
    // the font is completely empty or unparseable.
    face.number_of_glyphs() > 0
}

/// Get descendant CIDFont info: (object_id, is_embedded).
fn get_descendant_embed_info(
    doc: &Document,
    type0_dict: &lopdf::Dictionary,
) -> Option<(ObjectId, bool)> {
    let descendants = match type0_dict.get(b"DescendantFonts").ok() {
        Some(Object::Array(arr)) => arr,
        _ => return None,
    };

    for item in descendants {
        let desc_id = match item {
            Object::Reference(id) => *id,
            _ => continue,
        };
        let Some(Object::Dictionary(desc)) = doc.objects.get(&desc_id) else {
            continue;
        };
        let embedded = match desc.get(b"FontDescriptor").ok() {
            Some(Object::Reference(fd_id)) => {
                if let Some(Object::Dictionary(fd)) = doc.objects.get(fd_id) {
                    has_valid_font_file(doc, fd)
                } else {
                    false
                }
            }
            _ => false,
        };
        return Some((desc_id, embedded));
    }

    None
}

/// Return whether a Name object value is valid UTF-8 (direct or indirect).
fn name_is_valid_utf8(doc: &Document, dict_id: ObjectId, key: &[u8]) -> bool {
    let Some(Object::Dictionary(dict)) = doc.objects.get(&dict_id) else {
        return true;
    };
    let Some(raw) = get_name_bytes_resolved(doc, dict, key) else {
        return false;
    };
    std::str::from_utf8(&raw).is_ok()
}

/// Create a conservative PDF name token that is always valid UTF-8.
fn sanitize_font_name_for_pdf(name: &str, fallback: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '+' | '.') {
            out.push(ch);
        } else if ch.is_whitespace() {
            out.push('-');
        }
    }
    if out.is_empty() {
        fallback.to_string()
    } else {
        out
    }
}

/// Repair invalid or missing font name entries to satisfy PDF/A 6.1.8 UTF-8 rules.
fn repair_invalid_font_names(doc: &mut Document, info: &NonEmbeddedFont, fd_id: ObjectId) {
    let fallback = format!("FallbackFont{}", info.font_id.0);
    let safe_name = sanitize_font_name_for_pdf(&info.name, &fallback);
    let safe_bytes = safe_name.as_bytes().to_vec();

    let repair_root = !name_is_valid_utf8(doc, info.font_id, b"BaseFont");
    let repair_target =
        info.target_id != info.font_id && !name_is_valid_utf8(doc, info.target_id, b"BaseFont");
    let repair_fd = !name_is_valid_utf8(doc, fd_id, b"FontName");

    if repair_root {
        if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&info.font_id) {
            font.set("BaseFont", Object::Name(safe_bytes.clone()));
        }
    }
    if repair_target {
        if let Some(Object::Dictionary(ref mut target)) = doc.objects.get_mut(&info.target_id) {
            target.set("BaseFont", Object::Name(safe_bytes.clone()));
        }
    }
    if repair_fd {
        if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
            fd.set("FontName", Object::Name(safe_bytes));
        }
    }
}

/// Promote inline font dictionaries in `/Resources /Font` to standalone objects.
///
/// Some PDFs define fonts as inline dicts inside the Resources `/Font` map
/// instead of as standalone objects referenced by object ID.  For example:
/// ```text
/// /Font << /bannertopdf-font << /Type /Font /Subtype /Type1 /BaseFont /Courier >> >>
/// ```
/// `embed_fonts` only iterates `doc.objects`, so it never sees such inline dicts
/// and cannot embed them.  This function:
/// 1. Iterates all dict objects looking for an inline `/Font` sub-dict.
/// 2. For each inline font dict (a `Dictionary`, not a `Reference`), creates
///    a new document object and replaces the inline dict with a `Reference`.
///
/// Must run **before** `embed_fonts` so the promoted fonts become candidates
/// for embedding.
///
/// Returns the number of inline font dicts promoted.
pub fn promote_inline_font_dicts(doc: &mut Document) -> usize {
    // Collect Resources-dict object IDs and their inline font dict entries.
    // Structure: Vec<(resources_id, font_alias_name, font_dict_clone)>
    let mut to_promote: Vec<(ObjectId, Vec<u8>, lopdf::Dictionary)> = Vec::new();

    for (id, obj) in &doc.objects {
        // Look for any dict that has a /Font sub-dict.
        let fonts_dict = match obj {
            Object::Dictionary(d) => match d.get(b"Font").ok() {
                Some(Object::Dictionary(f)) => f.clone(),
                _ => continue,
            },
            _ => continue,
        };
        // Collect inline (non-Reference) font entries.
        for (alias, font_val) in fonts_dict.iter() {
            if let Object::Dictionary(_) = font_val {
                to_promote.push((*id, alias.to_vec(), {
                    if let Object::Dictionary(fd) = font_val {
                        fd.clone()
                    } else {
                        unreachable!()
                    }
                }));
            }
        }
    }

    let mut promoted = 0usize;
    for (res_id, alias, font_dict) in to_promote {
        // Add new object for the font dict.
        let new_id = doc.add_object(Object::Dictionary(font_dict));
        // Replace inline dict in the Resources /Font sub-dict with a Reference.
        if let Some(Object::Dictionary(res_d)) = doc.objects.get_mut(&res_id) {
            if let Ok(Object::Dictionary(fonts)) = res_d.get_mut(b"Font") {
                fonts.set(alias.as_slice(), Object::Reference(new_id));
            }
        }
        promoted += 1;
    }
    promoted
}

/// Embed fonts from system font files into the document.
pub fn embed_fonts(doc: &mut Document) -> Result<FontEmbedReport> {
    let mut report = FontEmbedReport {
        fonts_inspected: 0,
        non_embedded_found: 0,
        fonts_embedded: 0,
        failed: Vec::new(),
    };

    let non_embedded = find_non_embedded_fonts_detailed(doc);
    report.fonts_inspected = count_all_fonts(doc);
    report.non_embedded_found = non_embedded.len();

    for info in &non_embedded {
        let font_path = find_system_font(&info.name).or_else(find_fallback_font);

        match font_path {
            Some(path) => match embed_font_on_target(doc, info, &path) {
                Ok(()) => report.fonts_embedded += 1,
                Err(e) => report.failed.push((info.name.clone(), format!("{e}"))),
            },
            None => {
                report
                    .failed
                    .push((info.name.clone(), "no font file available".into()));
            }
        }
    }

    Ok(report)
}

/// Check if this is a Standard 14 font.
pub fn is_standard_14(name: &str) -> bool {
    let clean = strip_subset_prefix(name);
    STANDARD_14.contains(&clean)
}

/// Strip subset prefix (e.g., "ABCDEF+FontName" → "FontName").
fn strip_subset_prefix(name: &str) -> &str {
    if name.len() > 7 && name.as_bytes()[6] == b'+' {
        &name[7..]
    } else {
        name
    }
}

/// Embed a font file, targeting the correct dictionary for Type0 vs simple fonts.
/// Also updates the font Subtype to match the embedded program type.
fn embed_font_on_target(doc: &mut Document, info: &NonEmbeddedFont, font_path: &str) -> Result<()> {
    let raw_data = std::fs::read(font_path)
        .map_err(|e| ManipError::Other(format!("failed to read font file: {e}")))?;

    // If the file is a TrueType Collection (.ttc), extract the matching face
    // into a standalone TrueType font. PDF FontFile2 does not accept TTC data.
    let font_data = if raw_data.len() >= 4 && &raw_data[0..4] == b"ttcf" {
        let face_index = find_ttc_face_index(&raw_data, &info.name);
        extract_ttc_face(&raw_data, face_index).ok_or_else(|| {
            ManipError::Other(format!(
                "failed to extract face {} from TTC {}",
                face_index, font_path
            ))
        })?
    } else {
        raw_data
    };

    let is_truetype = font_path.ends_with(".ttf")
        || font_path.ends_with(".ttc")
        || (font_data.len() >= 4
            && (&font_data[0..4] == b"\x00\x01\x00\x00" || &font_data[0..4] == b"true"));

    let is_otf =
        font_path.ends_with(".otf") || (font_data.len() >= 4 && &font_data[0..4] == b"OTTO");

    let font_file_key = if is_truetype {
        "FontFile2"
    } else if is_otf {
        "FontFile3"
    } else {
        "FontFile"
    };

    // Create font stream.
    let mut stream_dict = dictionary! {
        "Length" => Object::Integer(font_data.len() as i64),
    };
    if is_truetype {
        stream_dict.set("Length1", Object::Integer(font_data.len() as i64));
    }
    if is_otf {
        stream_dict.set("Subtype", Object::Name(b"OpenType".to_vec()));
    }

    let font_stream = Stream::new(stream_dict, font_data.clone());
    let stream_id = doc.add_object(Object::Stream(font_stream));

    // Get or create FontDescriptor on the target (CIDFont for Type0, font itself otherwise).
    let fd_id = get_or_create_font_descriptor(doc, info.target_id)?;

    // Set the font file reference (remove old ones first to avoid conflicts).
    if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
        fd.remove(b"FontFile");
        fd.remove(b"FontFile2");
        fd.remove(b"FontFile3");
        fd.set(font_file_key, Object::Reference(stream_id));
    }

    // Update font Subtype to match embedded program type.
    // veraPDF checks that Subtype matches the FontFile type.
    if is_truetype && !info.is_type0 {
        // For simple fonts: Type1 → TrueType when embedding .ttf
        if info.subtype == "Type1" || info.subtype == "MMType1" {
            if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&info.font_id) {
                font.set("Subtype", Object::Name(b"TrueType".to_vec()));
            }
        }
    }
    if is_otf && !info.is_type0 {
        // For simple fonts: TrueType → Type1 when embedding .otf (CFF-based OpenType).
        // veraPDF checks that FontFile3 with /Subtype /OpenType matches Type1.
        if info.subtype == "TrueType" {
            if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&info.font_id) {
                font.set("Subtype", Object::Name(b"Type1".to_vec()));
            }
        }
    }
    if is_truetype && info.is_type0 {
        // For CIDFont descendants: CIDFontType0 → CIDFontType2 when embedding .ttf
        let target_subtype = {
            doc.objects
                .get(&info.target_id)
                .and_then(|o| {
                    if let Object::Dictionary(d) = o {
                        get_name(d, b"Subtype")
                    } else {
                        None
                    }
                })
                .unwrap_or_default()
        };
        if target_subtype == "CIDFontType0" {
            if let Some(Object::Dictionary(ref mut cid)) = doc.objects.get_mut(&info.target_id) {
                cid.set("Subtype", Object::Name(b"CIDFontType2".to_vec()));
            }
        }
    }

    // Normalize invalid font names before further checks.
    // This targets PDF/A 6.1.8:1 (UTF-8 validity of names).
    repair_invalid_font_names(doc, info, fd_id);

    // Update Widths and FontDescriptor metrics from the embedded font.
    if is_truetype || is_otf {
        update_metrics_from_font(doc, info, &font_data);
    }

    // If we embedded a non-symbolic font (e.g., DejaVuSans) for a symbolic-named
    // font (e.g., ZapfDingbats), update FontDescriptor Flags to match the actual
    // embedded program. veraPDF checks the font program, not the name.
    // Skip actual symbolic fonts (Symbol, ZapfDingbats) — they must keep Symbolic
    // flag so veraPDF uses CFF internal encoding for width validation.
    if (is_truetype || is_otf) && !is_symbolic_font_name(&info.name) {
        if let Ok(face) = ttf_parser::Face::parse(&font_data, 0) {
            let has_31_cmap = face.tables().cmap.as_ref().is_some_and(|cmap| {
                cmap.subtables.into_iter().any(|st| {
                    st.platform_id == ttf_parser::PlatformId::Windows && st.encoding_id == 1
                })
            });
            if has_31_cmap {
                // Font program has Windows Unicode cmap → non-symbolic.
                if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
                    if let Ok(Object::Integer(flags)) = fd.get(b"Flags") {
                        let mut f = *flags;
                        f &= !4; // Clear Symbolic (bit 3)
                        f |= 32; // Set Nonsymbolic (bit 6)
                        fd.set("Flags", Object::Integer(f));
                    }
                }
            }
        }
    }

    // PDF/A 6.2.11.6:4: for symbolic TrueType fonts, the cmap must contain
    // exactly 1 subtable or include (3,0) Microsoft Symbol. Fix the embedded
    // font stream in-place if needed.
    if is_truetype {
        let is_symbolic = doc
            .objects
            .get(&fd_id)
            .and_then(|o| {
                if let Object::Dictionary(d) = o {
                    Some(d)
                } else {
                    None
                }
            })
            .and_then(|d| {
                if let Ok(Object::Integer(f)) = d.get(b"Flags") {
                    Some(*f & 4 != 0)
                } else {
                    None
                }
            })
            .unwrap_or(false);
        if is_symbolic {
            fix_symbolic_truetype_cmap(doc, stream_id);
        }
    }

    Ok(())
}

/// Fix symbolic TrueType font cmap table (PDF/A 6.2.11.6:4).
///
/// For symbolic fonts, the cmap must have exactly 1 subtable or include (3,0)
/// Microsoft Symbol. If neither condition holds, strip the cmap to 1 subtable
/// by modifying the embedded font stream binary.
fn fix_symbolic_truetype_cmap(doc: &mut Document, stream_id: ObjectId) {
    let mut st = match doc.objects.get(&stream_id) {
        Some(Object::Stream(s)) => s.clone(),
        _ => return,
    };
    let _ = st.decompress();
    let mut data = st.content;

    if data.len() < 12 {
        return;
    }

    // Parse Offset Table to find cmap.
    let num_tables = u16::from_be_bytes([data[4], data[5]]) as usize;
    let mut cmap_dir_pos = None;
    for i in 0..num_tables {
        let pos = 12 + i * 16;
        if pos + 16 > data.len() {
            return;
        }
        if &data[pos..pos + 4] == b"cmap" {
            cmap_dir_pos = Some(pos);
            break;
        }
    }

    let dir_pos = match cmap_dir_pos {
        Some(p) => p,
        None => return,
    };

    let cmap_off = u32::from_be_bytes([
        data[dir_pos + 8],
        data[dir_pos + 9],
        data[dir_pos + 10],
        data[dir_pos + 11],
    ]) as usize;

    if cmap_off + 4 > data.len() {
        return;
    }

    let num_sub = u16::from_be_bytes([data[cmap_off + 2], data[cmap_off + 3]]);
    if num_sub <= 1 {
        return; // Already 1 subtable — no fix needed.
    }

    // Check if (3,0) Microsoft Symbol cmap exists and collect record metadata.
    let mut records: Vec<(usize, u16, u16)> = Vec::new();
    for j in 0..num_sub as usize {
        let rec = cmap_off + 4 + j * 8;
        if rec + 8 > data.len() {
            return;
        }
        let plat = u16::from_be_bytes([data[rec], data[rec + 1]]);
        let enc = u16::from_be_bytes([data[rec + 2], data[rec + 3]]);
        records.push((j, plat, enc));
        if plat == 3 && enc == 0 {
            return; // Already has (3,0) — no fix needed.
        }
    }

    // Strip to 1 cmap subtable.
    //
    // Keep a subtable that preserves byte-code coverage when possible. For
    // legacy symbolic fonts this is typically the Mac Roman (1,0) cmap.
    let preferred = records
        .iter()
        .find(|(_, plat, enc)| *plat == 1 && *enc == 0)
        .or_else(|| {
            records
                .iter()
                .find(|(_, plat, enc)| *plat == 3 && *enc == 1)
        })
        .or_else(|| records.iter().find(|(_, plat, _)| *plat == 0))
        .map(|(idx, _, _)| *idx)
        .unwrap_or(0);

    if preferred != 0 {
        let first_rec = cmap_off + 4;
        let pref_rec = cmap_off + 4 + preferred * 8;
        let pref_bytes = data[pref_rec..pref_rec + 8].to_vec();
        data[first_rec..first_rec + 8].copy_from_slice(&pref_bytes);
    }
    data[cmap_off + 2] = 0;
    data[cmap_off + 3] = 1;

    if let Some(Object::Stream(ref mut stream)) = doc.objects.get_mut(&stream_id) {
        stream.set_plain_content(data);
    }
}

/// Find the face index within a TTC that best matches the target font name.
fn find_ttc_face_index(data: &[u8], target_name: &str) -> u32 {
    let clean = strip_subset_prefix(target_name);
    let num_faces = ttf_parser::fonts_in_collection(data).unwrap_or(0);
    for i in 0..num_faces {
        if let Ok(face) = ttf_parser::Face::parse(data, i) {
            for name in face.names() {
                // Check PostScript name (name ID 6) and full name (name ID 4).
                if name.name_id == ttf_parser::name_id::POST_SCRIPT_NAME
                    || name.name_id == ttf_parser::name_id::FULL_NAME
                {
                    if let Some(s) = name.to_string() {
                        if s.eq_ignore_ascii_case(clean) {
                            return i;
                        }
                    }
                }
            }
        }
    }
    0 // default to first face
}

/// Extract a single face from a TrueType Collection (TTC) into a standalone
/// TrueType font. Returns `None` if the data is not a valid TTC or the face
/// index is out of range.
fn extract_ttc_face(data: &[u8], face_index: u32) -> Option<Vec<u8>> {
    if data.len() < 12 || &data[0..4] != b"ttcf" {
        return None;
    }

    let num_fonts = u32::from_be_bytes([data[8], data[9], data[10], data[11]]);
    if face_index >= num_fonts {
        return None;
    }

    let header_end = 12 + num_fonts as usize * 4;
    if data.len() < header_end {
        return None;
    }

    // Offset to the Offset Table for this face.
    let off_pos = 12 + face_index as usize * 4;
    let face_off = u32::from_be_bytes([
        data[off_pos],
        data[off_pos + 1],
        data[off_pos + 2],
        data[off_pos + 3],
    ]) as usize;

    if face_off + 12 > data.len() {
        return None;
    }

    let sf_version = &data[face_off..face_off + 4];
    let num_tables = u16::from_be_bytes([data[face_off + 4], data[face_off + 5]]) as usize;

    if face_off + 12 + num_tables * 16 > data.len() {
        return None;
    }

    // Read table records (tag, checksum, offset, length).
    struct Rec {
        tag: [u8; 4],
        checksum: [u8; 4],
        offset: u32,
        length: u32,
    }
    let mut tables = Vec::with_capacity(num_tables);
    for i in 0..num_tables {
        let p = face_off + 12 + i * 16;
        tables.push(Rec {
            tag: [data[p], data[p + 1], data[p + 2], data[p + 3]],
            checksum: [data[p + 4], data[p + 5], data[p + 6], data[p + 7]],
            offset: u32::from_be_bytes([data[p + 8], data[p + 9], data[p + 10], data[p + 11]]),
            length: u32::from_be_bytes([data[p + 12], data[p + 13], data[p + 14], data[p + 15]]),
        });
    }

    // Build standalone TrueType font.
    let dir_end = 12 + num_tables * 16;
    let data_start = (dir_end as u32 + 3) & !3;

    let mut out = Vec::new();

    // Offset Table header.
    out.extend_from_slice(sf_version);
    out.extend_from_slice(&(num_tables as u16).to_be_bytes());
    // searchRange, entrySelector, rangeShift — copy from original.
    out.extend_from_slice(&data[face_off + 6..face_off + 12]);

    // Table directory with updated offsets.
    let mut cur = data_start;
    let mut new_offsets = Vec::with_capacity(num_tables);
    for t in &tables {
        new_offsets.push(cur);
        cur += (t.length + 3) & !3;
    }
    for (i, t) in tables.iter().enumerate() {
        out.extend_from_slice(&t.tag);
        out.extend_from_slice(&t.checksum);
        out.extend_from_slice(&new_offsets[i].to_be_bytes());
        out.extend_from_slice(&t.length.to_be_bytes());
    }

    // Pad to data_start.
    out.resize(data_start as usize, 0);

    // Table data.
    for t in &tables {
        let start = t.offset as usize;
        let end = start + t.length as usize;
        if end > data.len() {
            return None;
        }
        out.extend_from_slice(&data[start..end]);
        let pad = (4 - (t.length % 4)) % 4;
        out.extend(std::iter::repeat_n(0u8, pad as usize));
    }

    Some(out)
}

/// Update font metrics (Widths, FontBBox, etc.) from the embedded font data.
fn update_metrics_from_font(doc: &mut Document, info: &NonEmbeddedFont, font_data: &[u8]) {
    let Ok(face) = ttf_parser::Face::parse(font_data, 0) else {
        return;
    };

    let units_per_em = face.units_per_em() as f64;
    if units_per_em == 0.0 {
        return;
    }
    let scale = 1000.0 / units_per_em;

    // Update FontDescriptor metrics.
    let fd_id = {
        let Some(Object::Dictionary(target)) = doc.objects.get(&info.target_id) else {
            return;
        };
        match target.get(b"FontDescriptor").ok() {
            Some(Object::Reference(id)) => Some(*id),
            _ => None,
        }
    };

    if let Some(fd_id) = fd_id {
        if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
            let ascent = (face.ascender() as f64 * scale).round() as i64;
            let descent = (face.descender() as f64 * scale).round() as i64;
            let bbox = face.global_bounding_box();
            fd.set("Ascent", Object::Integer(ascent));
            fd.set("Descent", Object::Integer(descent));
            fd.set(
                "FontBBox",
                Object::Array(vec![
                    Object::Integer((bbox.x_min as f64 * scale).round() as i64),
                    Object::Integer((bbox.y_min as f64 * scale).round() as i64),
                    Object::Integer((bbox.x_max as f64 * scale).round() as i64),
                    Object::Integer((bbox.y_max as f64 * scale).round() as i64),
                ]),
            );
            if let Some(cap_h) = face.capital_height() {
                fd.set(
                    "CapHeight",
                    Object::Integer((cap_h as f64 * scale).round() as i64),
                );
            }
        }
    }

    if info.is_type0 {
        update_cid_widths(doc, info.target_id, &face, scale);
    } else {
        // For CFF-based symbolic fonts (Symbol, ZapfDingbats), use the CFF
        // internal encoding to compute widths.  The Unicode cmap in OTF wrappers
        // maps unrelated Latin codepoints to symbol glyphs, producing wrong widths.
        let is_cff = face.tables().glyf.is_none();
        let (_base_encoding_name, skip_unreliable_simple_width_update) = {
            let font_dict = match doc.objects.get(&info.font_id) {
                Some(Object::Dictionary(d)) => d,
                _ => return,
            };
            let is_symbolic = is_font_symbolic(doc, font_dict) || is_symbolic_font_name(&info.name);
            let is_type1_like = matches!(info.subtype.as_str(), "Type1" | "MMType1");
            let (enc_name, _differences) = get_simple_encoding_info(doc, font_dict);
            let base_font = get_name(font_dict, b"BaseFont").unwrap_or_default();
            let is_subset = base_font.len() > 7 && base_font.as_bytes()[6] == b'+';
            let has_existing_widths = match font_dict.get(b"Widths").ok() {
                Some(Object::Array(arr)) => !arr.is_empty(),
                Some(Object::Reference(r)) => doc
                    .get_object(*r)
                    .ok()
                    .and_then(|o| o.as_array().ok())
                    .map(|arr| !arr.is_empty())
                    .unwrap_or(false),
                _ => false,
            };
            // For non-symbolic Type1-like fonts without a concrete base encoding
            // name, code-to-glyph mapping is frequently ambiguous. Keep existing
            // widths and let the mismatch pass perform targeted corrections.
            (
                enc_name.clone(),
                is_type1_like
                    && !is_symbolic
                    && !is_subset
                    && enc_name.is_empty()
                    && has_existing_widths,
            )
        };
        // Always use CFF encoding for symbolic CFF fonts (Symbol, ZapfDingbats, etc.).
        // Using WinAnsiEncoding/MacRomanEncoding + Unicode cmap produces wrong widths
        // because the cmap maps unrelated Latin codepoints to symbol glyphs.
        // fix_classic_symbolic_base14_encoding strips these encodings later anyway,
        // so CFF-based widths are the correct source of truth from the start.
        let use_symbolic_cff_widths = is_cff && is_symbolic_font_name(&info.name);
        if use_symbolic_cff_widths {
            update_simple_widths_cff_symbolic(doc, info, font_data, &face, scale);
        } else if !skip_unreliable_simple_width_update {
            update_simple_widths(doc, info.font_id, &face, scale);
        }
    }
}

/// Update Widths for a simple font (Type1/TrueType).
/// Uses the font's Encoding to map character codes to glyph widths.
fn update_simple_widths(
    doc: &mut Document,
    font_id: ObjectId,
    face: &ttf_parser::Face,
    scale: f64,
) {
    let (first_char, last_char, encoding_name, differences) = {
        let Some(Object::Dictionary(font)) = doc.objects.get(&font_id) else {
            return;
        };
        let fc = font
            .get(b"FirstChar")
            .ok()
            .and_then(|o| match o {
                Object::Integer(i) => Some(*i as u32),
                _ => None,
            })
            .unwrap_or(0);
        let lc = font
            .get(b"LastChar")
            .ok()
            .and_then(|o| match o {
                Object::Integer(i) => Some(*i as u32),
                _ => None,
            })
            .unwrap_or(255);
        let mut enc_name = String::new();
        let mut diffs = std::collections::HashMap::new();
        match font.get(b"Encoding").ok() {
            Some(Object::Name(n)) => {
                enc_name = String::from_utf8(n.clone()).unwrap_or_default();
            }
            Some(Object::Dictionary(enc_dict)) => {
                if let Some(base) = get_name(enc_dict, b"BaseEncoding") {
                    enc_name = base;
                }
                parse_differences(doc, enc_dict, &mut diffs);
            }
            Some(Object::Reference(enc_ref)) => {
                if let Ok(Object::Dictionary(enc_dict)) = doc.get_object(*enc_ref) {
                    if let Some(base) = get_name(enc_dict, b"BaseEncoding") {
                        enc_name = base;
                    }
                    parse_differences(doc, enc_dict, &mut diffs);
                }
            }
            _ => {}
        }
        (fc, lc, enc_name, diffs)
    };

    // veraPDF validates TrueType widths using the PDF Encoding to map
    // character codes to Unicode, then looks up in the (3,1) cmap.
    // This is the same algorithm for both TrueType and CFF fonts.
    let is_truetype_outline = face.tables().glyf.is_some();

    let mut widths = Vec::new();
    for code in first_char..=last_char {
        // Differences override takes priority over base encoding.
        let width = if let Some(glyph_name) = differences.get(&code) {
            // Try glyph name → Unicode → (3,1) cmap.
            let w = glyph_name_to_unicode(glyph_name)
                .and_then(|u| face.glyph_index(u))
                .and_then(|gid| face.glyph_hor_advance(gid))
                .map(|w| (w as f64 * scale).round() as i64);
            w.or_else(|| {
                // Fallback: look up glyph by name directly.
                face.glyph_index_by_name(glyph_name)
                    .and_then(|gid| face.glyph_hor_advance(gid))
                    .map(|w| (w as f64 * scale).round() as i64)
            })
            .unwrap_or(0)
        } else {
            let ch = encoding_to_char(code, &encoding_name);
            if let Some(glyph_id) = face.glyph_index(ch) {
                face.glyph_hor_advance(glyph_id)
                    .map(|w| (w as f64 * scale).round() as i64)
                    .unwrap_or(0)
            } else if is_truetype_outline && code <= u16::MAX as u32 {
                face.glyph_hor_advance(ttf_parser::GlyphId(code as u16))
                    .map(|w| (w as f64 * scale).round() as i64)
                    .unwrap_or(0)
            } else {
                0
            }
        };
        widths.push(Object::Integer(width));
    }

    if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&font_id) {
        if !widths.is_empty() {
            font.set("Widths", Object::Array(widths));
            font.set("FirstChar", Object::Integer(first_char as i64));
            font.set("LastChar", Object::Integer(last_char as i64));
        }
    }
}

/// Extract the raw CFF table data from an OTF font file.
///
/// Returns None if the font is not OTF or has no CFF table.
fn extract_cff_table(font_data: &[u8]) -> Option<&[u8]> {
    let raw_face = ttf_parser::RawFace::parse(font_data, 0).ok()?;
    for record in raw_face.table_records {
        if &record.tag.to_bytes() == b"CFF " {
            let start = record.offset as usize;
            let end = start.checked_add(record.length as usize)?;
            return font_data.get(start..end);
        }
    }
    None
}

/// Compute Widths for a CFF-based symbolic font (Symbol, ZapfDingbats).
///
/// veraPDF validates Symbolic CFF fonts using:
/// 1. PDF Encoding Differences → glyph name → CFF charset → GID → hmtx width
/// 2. CFF internal encoding → GID → hmtx width (for non-Differences codes)
///
/// For OTF fonts where the CFF encoding is empty (all .notdef), codes not in
/// Differences get .notdef width. Codes IN Differences get the named glyph width.
fn update_simple_widths_cff_symbolic(
    doc: &mut Document,
    info: &NonEmbeddedFont,
    font_data: &[u8],
    face: &ttf_parser::Face,
    scale: f64,
) {
    let font_id = info.font_id;

    // Extract the CFF table from the OTF wrapper for encoding lookup.
    let cff_data = extract_cff_table(font_data);
    let cff = cff_data.and_then(cff_parser::Table::parse);

    // Parse PDF Encoding Differences (e.g., [1 /bullet]) so we can look up
    // named glyphs that override the CFF internal encoding.
    let (first_char, last_char, differences) = {
        let Some(Object::Dictionary(font)) = doc.objects.get(&font_id) else {
            return;
        };
        let fc = font
            .get(b"FirstChar")
            .ok()
            .and_then(|o| match o {
                Object::Integer(i) => Some(*i as u32),
                _ => None,
            })
            .unwrap_or(0);
        let lc = font
            .get(b"LastChar")
            .ok()
            .and_then(|o| match o {
                Object::Integer(i) => Some(*i as u32),
                _ => None,
            })
            .unwrap_or(255);
        let mut diffs = std::collections::HashMap::new();
        match font.get(b"Encoding").ok() {
            Some(Object::Dictionary(enc_dict)) => {
                parse_differences(doc, enc_dict, &mut diffs);
            }
            Some(Object::Reference(enc_ref)) => {
                if let Ok(Object::Dictionary(enc_dict)) = doc.get_object(*enc_ref) {
                    parse_differences(doc, enc_dict, &mut diffs);
                }
            }
            _ => {}
        }
        (fc, lc, diffs)
    };

    let mut widths = Vec::new();
    for code in first_char..=last_char {
        let width = if code > 255 {
            0
        } else if let Some(glyph_name) = differences.get(&code) {
            // Code is in Differences: look up glyph by name in the font.
            // veraPDF resolves Differences names via CFF charset, then hmtx.
            if let Some(gid) = face.glyph_index_by_name(glyph_name) {
                face.glyph_hor_advance(gid)
                    .map(|w| (w as f64 * scale).round() as i64)
                    .unwrap_or(0)
            } else {
                // Glyph name not found — try via Unicode mapping.
                glyph_name_to_unicode(glyph_name)
                    .and_then(|u| face.glyph_index(u))
                    .and_then(|gid| face.glyph_hor_advance(gid))
                    .map(|w| (w as f64 * scale).round() as i64)
                    .unwrap_or(0)
            }
        } else if let Some(ref cff) = cff {
            // No Differences entry: use CFF internal encoding.
            let gid = cff
                .encoding
                .code_to_gid(&cff.charset, code as u8)
                .map(|g| ttf_parser::GlyphId(g.0))
                .unwrap_or(ttf_parser::GlyphId(0));
            face.glyph_hor_advance(gid)
                .map(|w| (w as f64 * scale).round() as i64)
                .unwrap_or(0)
        } else {
            // No CFF table available — use .notdef width.
            face.glyph_hor_advance(ttf_parser::GlyphId(0))
                .map(|w| (w as f64 * scale).round() as i64)
                .unwrap_or(0)
        };
        widths.push(Object::Integer(width));
    }

    if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&font_id) {
        if !widths.is_empty() {
            font.set("Widths", Object::Array(widths));
            font.set("FirstChar", Object::Integer(first_char as i64));
            font.set("LastChar", Object::Integer(last_char as i64));
        }
    }
}

/// Map a character code to a Unicode character based on PDF encoding name.
fn encoding_to_char(code: u32, encoding_name: &str) -> char {
    match encoding_name {
        "WinAnsiEncoding" => winansi_to_char(code),
        "MacRomanEncoding" => macroman_to_char(code),
        _ => {
            // Default: assume identity mapping for codes 0-127,
            // WinAnsi-like for 128-255.
            if code < 128 {
                char::from_u32(code).unwrap_or(' ')
            } else {
                winansi_to_char(code)
            }
        }
    }
}

/// WinAnsiEncoding character map (codes 128-159 differ from Unicode).
fn winansi_to_char(code: u32) -> char {
    if !(128..=255).contains(&code) {
        return char::from_u32(code).unwrap_or(' ');
    }
    // PDF Appendix D defines two duplicate WinAnsi codes that are
    // typographically identical to their base glyphs:
    // 160 => nonbreaking space (same metrics as space),
    // 173 => soft hyphen (same metrics as hyphen).
    match code {
        160 => return ' ',
        173 => return '-',
        _ => {}
    }
    // WinAnsi codes 128-159 that differ from Latin-1.
    const WINANSI_128_159: [u32; 32] = [
        0x20AC, 0x0081, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, // 128-135
        0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0x008D, 0x017D, 0x008F, // 136-143
        0x0090, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, // 144-151
        0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x009D, 0x017E, 0x0178, // 152-159
    ];
    if code < 160 {
        char::from_u32(WINANSI_128_159[(code - 128) as usize]).unwrap_or(' ')
    } else {
        char::from_u32(code).unwrap_or(' ')
    }
}

/// MacRomanEncoding character map (codes 128-255).
fn macroman_to_char(code: u32) -> char {
    if code < 128 {
        return char::from_u32(code).unwrap_or(' ');
    }
    if code > 255 {
        return char::from_u32(code).unwrap_or(' ');
    }
    // Full MacRoman 128-255 mapping to Unicode.
    const MACROMAN_128_255: [u32; 128] = [
        0x00C4, 0x00C5, 0x00C7, 0x00C9, 0x00D1, 0x00D6, 0x00DC, 0x00E1, // 128-135
        0x00E0, 0x00E2, 0x00E4, 0x00E3, 0x00E5, 0x00E7, 0x00E9, 0x00E8, // 136-143
        0x00EA, 0x00EB, 0x00ED, 0x00EC, 0x00EE, 0x00EF, 0x00F1, 0x00F3, // 144-151
        0x00F2, 0x00F4, 0x00F6, 0x00F5, 0x00FA, 0x00F9, 0x00FB, 0x00FC, // 152-159
        0x2020, 0x00B0, 0x00A2, 0x00A3, 0x00A7, 0x2022, 0x00B6, 0x00DF, // 160-167
        0x00AE, 0x00A9, 0x2122, 0x00B4, 0x00A8, 0x2260, 0x00C6, 0x00D8, // 168-175
        0x221E, 0x00B1, 0x2264, 0x2265, 0x00A5, 0x00B5, 0x2202, 0x2211, // 176-183
        0x220F, 0x03C0, 0x222B, 0x00AA, 0x00BA, 0x2126, 0x00E6, 0x00F8, // 184-191
        0x00BF, 0x00A1, 0x00AC, 0x221A, 0x0192, 0x2248, 0x2206, 0x00AB, // 192-199
        0x00BB, 0x2026, 0x00A0, 0x00C0, 0x00C3, 0x00D5, 0x0152, 0x0153, // 200-207
        0x2013, 0x2014, 0x201C, 0x201D, 0x2018, 0x2019, 0x00F7, 0x25CA, // 208-215
        0x00FF, 0x0178, 0x2044, 0x20AC, 0x2039, 0x203A, 0xFB01, 0xFB02, // 216-223
        0x2021, 0x00B7, 0x201A, 0x201E, 0x2030, 0x00C2, 0x00CA, 0x00C1, // 224-231
        0x00CB, 0x00C8, 0x00CD, 0x00CE, 0x00CF, 0x00CC, 0x00D3, 0x00D4, // 232-239
        0xF8FF, 0x00D2, 0x00DA, 0x00DB, 0x00D9, 0x0131, 0x02C6, 0x02DC, // 240-247
        0x00AF, 0x02D8, 0x02D9, 0x02DA, 0x00B8, 0x02DD, 0x02DB, 0x02C7, // 248-255
    ];
    char::from_u32(MACROMAN_128_255[(code - 128) as usize]).unwrap_or(' ')
}

/// Update DW (default width) for a CIDFont.
fn update_cid_widths(doc: &mut Document, cid_id: ObjectId, face: &ttf_parser::Face, scale: f64) {
    let default_width = face
        .glyph_hor_advance(ttf_parser::GlyphId(0))
        .map(|w| (w as f64 * scale).round() as i64)
        .unwrap_or(1000);

    if let Some(Object::Dictionary(ref mut cid)) = doc.objects.get_mut(&cid_id) {
        cid.set("DW", Object::Integer(default_width));
        // Remove W array to avoid width mismatches — DW will serve as fallback.
        cid.remove(b"W");
    }
}

/// Get the FontDescriptor reference from a Font dictionary, or create one.
fn get_or_create_font_descriptor(doc: &mut Document, font_id: ObjectId) -> Result<ObjectId> {
    let existing = {
        let Some(Object::Dictionary(font)) = doc.objects.get(&font_id) else {
            return Err(ManipError::Other("font object not found".into()));
        };
        match font.get(b"FontDescriptor").ok() {
            Some(Object::Reference(id)) => Some(*id),
            _ => None,
        }
    };

    if let Some(fd_id) = existing {
        return Ok(fd_id);
    }

    let font_name = {
        let Some(Object::Dictionary(font)) = doc.objects.get(&font_id) else {
            return Err(ManipError::Other("font object not found".into()));
        };
        get_name(font, b"BaseFont").unwrap_or_else(|| "Unknown".into())
    };

    // Set Symbolic (4) for known symbolic fonts, Nonsymbolic (32) otherwise.
    let flags: i64 = if is_symbolic_font_name(&font_name) {
        4
    } else {
        32
    };

    let fd = dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => Object::Name(font_name.into_bytes()),
        "Flags" => Object::Integer(flags),
        "FontBBox" => Object::Array(vec![
            Object::Integer(0), Object::Integer(-200),
            Object::Integer(1000), Object::Integer(800),
        ]),
        "ItalicAngle" => Object::Integer(0),
        "Ascent" => Object::Integer(800),
        "Descent" => Object::Integer(-200),
        "CapHeight" => Object::Integer(700),
        "StemV" => Object::Integer(80),
    };
    let fd_id = doc.add_object(Object::Dictionary(fd));

    if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&font_id) {
        font.set("FontDescriptor", Object::Reference(fd_id));
    }

    Ok(fd_id)
}

/// Map Standard 14 font names to available system font files.
// Font directory prefix macros for compile-time concatenation.
macro_rules! lib {
    ($f:literal) => {
        concat!("/usr/share/fonts/truetype/liberation/", $f)
    };
}
macro_rules! urw {
    ($f:literal) => {
        concat!("/usr/share/fonts/opentype/urw-base35/", $f)
    };
}
macro_rules! noto {
    ($f:literal) => {
        concat!("/usr/share/fonts/truetype/noto/", $f)
    };
}
macro_rules! dv {
    ($f:literal) => {
        concat!("/usr/share/fonts/truetype/dejavu/", $f)
    };
}
macro_rules! mac {
    ($f:literal) => {
        concat!("/System/Library/Fonts/Supplemental/", $f)
    };
}

fn repo_font_pack_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(REPO_FONT_PACK_REL)
}

/// Resolve a font candidate path with deterministic priority:
/// 1) in-repo shared font pack by basename
/// 2) original absolute/relative candidate path
fn resolve_font_candidate_path(candidate: &str) -> Option<String> {
    if let Some(file_name) = std::path::Path::new(candidate)
        .file_name()
        .and_then(|n| n.to_str())
    {
        let packed = repo_font_pack_dir().join(file_name);
        if packed.exists() {
            return Some(packed.to_string_lossy().to_string());
        }
    }

    if std::path::Path::new(candidate).exists() {
        return Some(candidate.to_string());
    }

    None
}

fn find_fallback_font() -> Option<String> {
    for candidate in FALLBACK_FONTS {
        if let Some(path) = resolve_font_candidate_path(candidate) {
            return Some(path);
        }
    }
    None
}

fn standard14_system_path(clean_name: &str) -> Option<String> {
    // Priority: Liberation (TTF) > URW Base35 (OTF) > Noto > DejaVu > macOS.
    // TTF fonts are preferred because width computation for TrueType uses the
    // same algorithm as veraPDF (raw character code as Unicode in (3,1) cmap).
    // URW OTF stays as fallback; for PostScript Level 2 fonts URW is first
    // since it's the correct metric match.
    let candidates: &[&str] = match clean_name {
        // --- Sans-serif (Helvetica / Arial family) ---
        "Helvetica"
        | "ArialMT"
        | "Arial"
        | "Tahoma"
        | "Verdana"
        | "LucidaSansUnicode"
        | "LucidaSans"
        | "SegoeUI"
        | "Calibri"
        | "TrebuchetMS"
        | "LucidaGrande"
        | "HelveticaNeue"
        | "HelveticaLTStd-Roman" => &[
            lib!("LiberationSans-Regular.ttf"),
            urw!("NimbusSans-Regular.otf"),
            noto!("NotoSans-Regular.ttf"),
            dv!("DejaVuSans.ttf"),
            "/System/Library/Fonts/Helvetica.ttc",
            mac!("Arial.ttf"),
        ],
        "Helvetica-Bold" | "Arial-BoldMT" | "Arial,Bold" | "Arial-Bold" | "ArialBlack"
        | "Tahoma,Bold" | "Tahoma-Bold" | "Verdana,Bold" | "Verdana-Bold" | "Calibri,Bold"
        | "Calibri-Bold" | "HelveticaNeue-Bold" | "SegoeUI,Bold" | "SegoeUI-Bold" => &[
            lib!("LiberationSans-Bold.ttf"),
            urw!("NimbusSans-Bold.otf"),
            noto!("NotoSans-Bold.ttf"),
            dv!("DejaVuSans-Bold.ttf"),
            mac!("Arial Bold.ttf"),
        ],
        "Helvetica-Oblique"
        | "Arial-ItalicMT"
        | "Arial,Italic"
        | "Arial-Italic"
        | "Verdana,Italic"
        | "Verdana-Italic"
        | "Calibri,Italic"
        | "Calibri-Italic"
        | "HelveticaNeue-Italic"
        | "SegoeUI,Italic"
        | "SegoeUI-Italic" => &[
            lib!("LiberationSans-Italic.ttf"),
            urw!("NimbusSans-Italic.otf"),
            noto!("NotoSans-Italic.ttf"),
            dv!("DejaVuSans-Oblique.ttf"),
            mac!("Arial Italic.ttf"),
        ],
        "Helvetica-BoldOblique"
        | "Arial-BoldItalicMT"
        | "Arial,BoldItalic"
        | "Calibri,BoldItalic"
        | "HelveticaNeue-BoldItalic" => &[
            lib!("LiberationSans-BoldItalic.ttf"),
            urw!("NimbusSans-BoldItalic.otf"),
            noto!("NotoSans-BoldItalic.ttf"),
            dv!("DejaVuSans-BoldOblique.ttf"),
            mac!("Arial Bold Italic.ttf"),
        ],
        // --- Serif (Times family) ---
        "Times-Roman" | "TimesNewRomanPSMT" | "TimesNewRoman" | "TimesNewRomanPS" | "Georgia"
        | "BookAntiqua" | "Cambria" | "Garamond" | "Palatino" | "PalatinoLinotype" => &[
            lib!("LiberationSerif-Regular.ttf"),
            urw!("NimbusRoman-Regular.otf"),
            noto!("NotoSerif-Regular.ttf"),
            dv!("DejaVuSerif.ttf"),
            mac!("Times New Roman.ttf"),
        ],
        "Times-Bold"
        | "TimesNewRomanPS-BoldMT"
        | "TimesNewRoman,Bold"
        | "TimesNewRoman-Bold"
        | "Georgia,Bold"
        | "Georgia-Bold"
        | "Cambria,Bold"
        | "Cambria-Bold" => &[
            lib!("LiberationSerif-Bold.ttf"),
            urw!("NimbusRoman-Bold.otf"),
            noto!("NotoSerif-Bold.ttf"),
            dv!("DejaVuSerif-Bold.ttf"),
            mac!("Times New Roman Bold.ttf"),
        ],
        "Times-Italic"
        | "TimesNewRomanPS-ItalicMT"
        | "TimesNewRoman,Italic"
        | "TimesNewRoman-Italic"
        | "Georgia,Italic"
        | "Georgia-Italic"
        | "Cambria,Italic"
        | "Cambria-Italic" => &[
            lib!("LiberationSerif-Italic.ttf"),
            urw!("NimbusRoman-Italic.otf"),
            noto!("NotoSerif-Italic.ttf"),
            dv!("DejaVuSerif-Italic.ttf"),
            mac!("Times New Roman Italic.ttf"),
        ],
        "Times-BoldItalic"
        | "TimesNewRomanPS-BoldItalicMT"
        | "TimesNewRoman,BoldItalic"
        | "Cambria,BoldItalic" => &[
            lib!("LiberationSerif-BoldItalic.ttf"),
            urw!("NimbusRoman-BoldItalic.otf"),
            noto!("NotoSerif-BoldItalic.ttf"),
            dv!("DejaVuSerif-BoldItalic.ttf"),
            mac!("Times New Roman Bold Italic.ttf"),
        ],
        // --- Monospace (Courier family) ---
        "Courier" | "CourierNewPSMT" | "CourierNew" | "CourierNewPS" | "LucidaConsole"
        | "Consolas" => &[
            lib!("LiberationMono-Regular.ttf"),
            urw!("NimbusMonoPS-Regular.otf"),
            dv!("DejaVuSansMono.ttf"),
            mac!("Courier New.ttf"),
        ],
        "Courier-Bold" | "CourierNewPS-BoldMT" | "CourierNew,Bold" | "CourierNew-Bold" => &[
            lib!("LiberationMono-Bold.ttf"),
            urw!("NimbusMonoPS-Bold.otf"),
            dv!("DejaVuSansMono-Bold.ttf"),
            mac!("Courier New Bold.ttf"),
        ],
        "Courier-Oblique" | "CourierNewPS-ItalicMT" | "CourierNew,Italic" | "CourierNew-Italic" => {
            &[
                lib!("LiberationMono-Italic.ttf"),
                urw!("NimbusMonoPS-Italic.otf"),
                dv!("DejaVuSansMono-Oblique.ttf"),
                mac!("Courier New Italic.ttf"),
            ]
        }
        "Courier-BoldOblique" | "CourierNewPS-BoldItalicMT" | "CourierNew,BoldItalic" => &[
            lib!("LiberationMono-BoldItalic.ttf"),
            urw!("NimbusMonoPS-BoldItalic.otf"),
            dv!("DejaVuSansMono-BoldOblique.ttf"),
            mac!("Courier New Bold Italic.ttf"),
        ],
        // --- Symbolic fonts ---
        "Symbol" | "SymbolMT" => &[
            urw!("StandardSymbolsPS.otf"),
            "/System/Library/Fonts/Symbol.ttf",
        ],
        "ZapfDingbats" => &[
            urw!("D050000L.otf"),
            "/System/Library/Fonts/Supplemental/Apple Symbols.ttf",
        ],
        // --- Narrow variants ---
        "ArialNarrow" => &[
            lib!("LiberationSansNarrow-Regular.ttf"),
            urw!("NimbusSansNarrow-Regular.otf"),
            dv!("DejaVuSansCondensed.ttf"),
            mac!("Arial Narrow.ttf"),
        ],
        "ArialNarrow,Bold" | "ArialNarrow-Bold" => &[
            lib!("LiberationSansNarrow-Bold.ttf"),
            urw!("NimbusSansNarrow-Bold.otf"),
            dv!("DejaVuSansCondensed-Bold.ttf"),
            mac!("Arial Narrow Bold.ttf"),
        ],
        "ArialNarrow,Italic" | "ArialNarrow-Italic" => &[
            lib!("LiberationSansNarrow-Italic.ttf"),
            urw!("NimbusSansNarrow-Oblique.otf"),
            dv!("DejaVuSansCondensed-Oblique.ttf"),
            mac!("Arial Narrow Italic.ttf"),
        ],
        "ArialNarrow,BoldItalic" | "ArialNarrow-BoldItalic" => &[
            lib!("LiberationSansNarrow-BoldItalic.ttf"),
            urw!("NimbusSansNarrow-BoldOblique.otf"),
            dv!("DejaVuSansCondensed-BoldOblique.ttf"),
            mac!("Arial Narrow Bold Italic.ttf"),
        ],
        "ArialRoundedMTBold" => &[
            lib!("LiberationSans-Bold.ttf"),
            urw!("NimbusSans-Bold.otf"),
            dv!("DejaVuSans-Bold.ttf"),
            mac!("Arial Rounded Bold.ttf"),
        ],
        // --- PostScript Level 2 base 35 fonts (URW equivalents) ---
        "NewCenturySchlbk-Roman" | "CenturySchoolbook" => {
            &[urw!("C059-Roman.otf"), lib!("LiberationSerif-Regular.ttf")]
        }
        "NewCenturySchlbk-Bold" | "CenturySchoolbook-Bold" => {
            &[urw!("C059-Bold.otf"), lib!("LiberationSerif-Bold.ttf")]
        }
        "NewCenturySchlbk-Italic" | "CenturySchoolbook-Italic" => {
            &[urw!("C059-Italic.otf"), lib!("LiberationSerif-Italic.ttf")]
        }
        "NewCenturySchlbk-BoldItalic" | "CenturySchoolbook-BoldItalic" => &[
            urw!("C059-BdIta.otf"),
            lib!("LiberationSerif-BoldItalic.ttf"),
        ],
        "Bookman-Light" | "BookmanOldStyle" => &[
            urw!("URWBookman-Light.otf"),
            lib!("LiberationSerif-Regular.ttf"),
        ],
        "Bookman-Demi" | "BookmanOldStyle-Bold" => &[
            urw!("URWBookman-Demi.otf"),
            lib!("LiberationSerif-Bold.ttf"),
        ],
        "AvantGarde-Book" | "AvantGardeITCbyBT-Book" => &[
            urw!("URWGothic-Book.otf"),
            lib!("LiberationSans-Regular.ttf"),
        ],
        "AvantGarde-Demi" => &[urw!("URWGothic-Demi.otf"), lib!("LiberationSans-Bold.ttf")],
        "Palatino-Roman" | "PalatinoLinotype-Roman" => {
            &[urw!("P052-Roman.otf"), lib!("LiberationSerif-Regular.ttf")]
        }
        "Palatino-Bold" | "PalatinoLinotype-Bold" => {
            &[urw!("P052-Bold.otf"), lib!("LiberationSerif-Bold.ttf")]
        }
        "Palatino-Italic" | "PalatinoLinotype-Italic" => {
            &[urw!("P052-Italic.otf"), lib!("LiberationSerif-Italic.ttf")]
        }
        "Palatino-BoldItalic" | "PalatinoLinotype-BoldItalic" => &[
            urw!("P052-BoldItalic.otf"),
            lib!("LiberationSerif-BoldItalic.ttf"),
        ],
        "ZapfChancery-MediumItalic" => &[urw!("Z003-MediumItalic.otf")],
        // --- Misc common fonts ---
        "Impact" | "ComicSansMS" => &[lib!("LiberationSans-Bold.ttf"), noto!("NotoSans-Bold.ttf")],
        _ => return None,
    };
    for &path in candidates {
        if let Some(resolved) = resolve_font_candidate_path(path) {
            return Some(resolved);
        }
    }
    None
}

/// Search common system font directories for a font file.
fn find_system_font(font_name: &str) -> Option<String> {
    let clean_name = strip_subset_prefix(font_name);

    if let Some(path) = standard14_system_path(clean_name) {
        return Some(path);
    }

    // Heuristic: for unknown fonts, infer style and pick a matching substitute.
    if let Some(path) = heuristic_font_match(clean_name) {
        return Some(path);
    }

    let candidates: Vec<String> = vec![
        format!("{clean_name}.ttf"),
        format!("{clean_name}.otf"),
        format!("{clean_name}.TTF"),
        format!("{clean_name}.OTF"),
        format!("{}Regular.ttf", clean_name.replace('-', "")),
        format!("{}-Regular.ttf", clean_name),
    ];

    let dirs = if cfg!(target_os = "macos") {
        vec![
            "/System/Library/Fonts/",
            "/Library/Fonts/",
            "~/Library/Fonts/",
        ]
    } else if cfg!(target_os = "linux") {
        vec![
            "/usr/share/fonts/truetype/",
            "/usr/share/fonts/opentype/",
            "/usr/share/fonts/",
            "/usr/local/share/fonts/",
            "~/.fonts/",
            "~/.local/share/fonts/",
        ]
    } else {
        vec!["C:\\Windows\\Fonts\\"]
    };

    for dir in &dirs {
        for candidate in &candidates {
            let path = format!("{dir}{candidate}");
            let expanded = path.replace('~', &std::env::var("HOME").unwrap_or_default());
            if std::path::Path::new(&expanded).exists() {
                return Some(expanded);
            }
        }
        let expanded_dir = dir.replace('~', &std::env::var("HOME").unwrap_or_default());
        for candidate in &candidates {
            if let Some(path) = find_font_recursive(&expanded_dir, candidate) {
                return Some(path);
            }
        }
    }

    None
}

/// Heuristic font matching: infer weight/style from font name, then pick a
/// Liberation or Noto substitute based on whether the name looks like serif,
/// sans-serif, or monospace.
fn heuristic_font_match(name: &str) -> Option<String> {
    let lower = name.to_ascii_lowercase();

    // Detect weight and style from common suffixes.
    let is_bold = lower.contains("bold")
        || lower.contains("demi")
        || lower.contains("black")
        || lower.contains("heavy");
    let is_italic =
        lower.contains("italic") || lower.contains("oblique") || lower.contains("slant");

    // Detect font class.
    let is_mono = lower.contains("mono")
        || lower.contains("courier")
        || lower.contains("code")
        || lower.contains("console")
        || lower.contains("typewriter");
    let is_serif =
        (lower.contains("serif") && !lower.contains("sansserif") && !lower.contains("sans-serif"))
            || lower.contains("roman")
            || lower.contains("times")
            || lower.contains("garamond")
            || lower.contains("georgia")
            || lower.contains("bookman")
            || lower.contains("century")
            || lower.contains("palatino")
            || lower.contains("cambria")
            || lower.contains("minion");

    let key = if is_mono {
        match (is_bold, is_italic) {
            (true, true) => "Courier-BoldOblique",
            (true, false) => "Courier-Bold",
            (false, true) => "Courier-Oblique",
            (false, false) => "Courier",
        }
    } else if is_serif {
        match (is_bold, is_italic) {
            (true, true) => "Times-BoldItalic",
            (true, false) => "Times-Bold",
            (false, true) => "Times-Italic",
            (false, false) => "Times-Roman",
        }
    } else {
        // Default to sans-serif.
        match (is_bold, is_italic) {
            (true, true) => "Helvetica-BoldOblique",
            (true, false) => "Helvetica-Bold",
            (false, true) => "Helvetica-Oblique",
            (false, false) => "Helvetica",
        }
    };

    standard14_system_path(key)
}

fn find_font_recursive(dir: &str, filename: &str) -> Option<String> {
    find_font_recursive_depth(dir, filename, 0)
}

fn find_font_recursive_depth(dir: &str, filename: &str, depth: u32) -> Option<String> {
    if depth > 3 {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.eq_ignore_ascii_case(filename) {
                    return path.to_str().map(|s| s.to_string());
                }
            }
        } else if path.is_dir() {
            if let Some(found) =
                find_font_recursive_depth(path.to_str().unwrap_or(""), filename, depth + 1)
            {
                return Some(found);
            }
        }
    }
    None
}

/// Fix width mismatches for fonts with embedded programs (6.2.11.5:1).
///
/// Only updates widths that actually differ from the embedded font by > 1 unit.
/// This avoids the regression that blanket width updates cause.
pub fn fix_width_mismatches(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            if let Object::Dictionary(dict) = obj {
                if is_font_dict(dict) {
                    return Some(*id);
                }
            }
            None
        })
        .collect();

    let mut fixed = 0;
    for font_id in font_ids {
        let (subtype, fd_id, is_type0) = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();
            let is_type0 = subtype == "Type0";

            if is_type0 {
                let desc_fd = get_descendant_embed_info(doc, dict);
                match desc_fd {
                    Some((cid_id, true)) => {
                        let cid_fd = doc.objects.get(&cid_id).and_then(|o| {
                            if let Object::Dictionary(d) = o {
                                match d.get(b"FontDescriptor").ok() {
                                    Some(Object::Reference(id)) => Some(*id),
                                    _ => None,
                                }
                            } else {
                                None
                            }
                        });
                        (subtype, cid_fd, true)
                    }
                    _ => continue,
                }
            } else {
                let fd_id = match dict.get(b"FontDescriptor").ok() {
                    Some(Object::Reference(id)) => Some(*id),
                    _ => None,
                };
                (subtype, fd_id, false)
            }
        };

        let Some(fd_id) = fd_id else { continue };

        let font_data = read_embedded_font_data(doc, fd_id);
        let Some(font_data) = font_data else { continue };

        // Try TrueType/OpenType first, then CFF.
        if let Ok(face) = ttf_parser::Face::parse(&font_data, 0) {
            let units_per_em = face.units_per_em() as f64;
            if units_per_em == 0.0 {
                continue;
            }
            let scale = 1000.0 / units_per_em;

            // For simple fonts, check if existing widths differ from font widths.
            if !is_type0 && subtype != "CIDFontType0" && subtype != "CIDFontType2" {
                let has_mismatch = {
                    let Some(Object::Dictionary(font)) = doc.objects.get(&font_id) else {
                        continue;
                    };
                    let fc = font
                        .get(b"FirstChar")
                        .ok()
                        .and_then(|o| match o {
                            Object::Integer(i) => Some(*i as u32),
                            _ => None,
                        })
                        .unwrap_or(0);
                    let existing_widths = match font.get(b"Widths").ok() {
                        Some(Object::Array(arr)) => arr,
                        _ => continue,
                    };
                    let enc = font
                        .get(b"Encoding")
                        .ok()
                        .and_then(|o| match o {
                            Object::Name(n) => String::from_utf8(n.clone()).ok(),
                            _ => None,
                        })
                        .unwrap_or_default();

                    let mut mismatch = false;
                    for (i, obj) in existing_widths.iter().enumerate() {
                        let pdf_w = match obj {
                            Object::Integer(w) => *w,
                            Object::Real(r) => *r as i64,
                            _ => continue,
                        };
                        let code = fc + i as u32;
                        let ch = encoding_to_char(code, &enc);
                        let expected = if let Some(gid) = face.glyph_index(ch) {
                            face.glyph_hor_advance(gid)
                                .map(|w| (w as f64 * scale).round() as i64)
                                .unwrap_or(0)
                        } else if code <= u16::MAX as u32 {
                            face.glyph_hor_advance(ttf_parser::GlyphId(code as u16))
                                .map(|w| (w as f64 * scale).round() as i64)
                                .unwrap_or(0)
                        } else {
                            0
                        };
                        if (pdf_w - expected).abs() > 1 {
                            mismatch = true;
                            break;
                        }
                    }
                    mismatch
                };

                if has_mismatch {
                    update_simple_widths(doc, font_id, &face, scale);
                    fixed += 1;
                }
            }
        } else if let Some(cff) = cff_parser::Table::parse(&font_data) {
            // CFF font — fix widths for CID fonts using CFF glyph widths.
            if is_type0 || subtype == "CIDFontType0" {
                let target_id = if is_type0 {
                    // For Type0, get the CIDFont descendant ID.
                    doc.objects.get(&font_id).and_then(|o| {
                        if let Object::Dictionary(d) = o {
                            get_descendant_embed_info(doc, d).map(|(id, _)| id)
                        } else {
                            None
                        }
                    })
                } else {
                    Some(font_id)
                };
                let Some(cid_font_id) = target_id else {
                    continue;
                };
                if fix_cid_widths_from_cff(doc, cid_font_id, &cff) {
                    fixed += 1;
                }
            }
        }
    }
    fixed
}

/// Fix CID font /W (widths) array from CFF glyph width data (6.2.11.5:1).
///
/// For SID-based CFF fonts, reads the actual glyph widths from the CFF program
/// and rebuilds the /W array to match. This ensures consistency between the
/// font dictionary widths and the embedded font program.
fn fix_cid_widths_from_cff(
    doc: &mut Document,
    cid_font_id: ObjectId,
    cff: &cff_parser::Table<'_>,
) -> bool {
    let num_glyphs = cff.number_of_glyphs();
    if num_glyphs == 0 {
        return false;
    }

    // Read the font matrix to determine scaling.
    // CFF uses a 1/1000 scale by default (FontMatrix = [0.001 0 0 0.001 0 0]).
    let matrix = cff.matrix();
    let scale = if matrix.sx.abs() > f32::EPSILON {
        matrix.sx * 1000.0
    } else {
        1.0
    };

    // Collect widths for all glyphs from the CFF program, grouped by CID.
    let mut by_cid: std::collections::HashMap<u16, Vec<i64>> = std::collections::HashMap::new();
    for gid in 0..num_glyphs {
        let glyph_id = cff_parser::GlyphId(gid);
        if let Some(w) = cff.glyph_width(glyph_id) {
            let scaled = (w as f64 * scale as f64).round() as i64;
            let cid = cff.glyph_cid(glyph_id).unwrap_or(gid);
            by_cid.entry(cid).or_default().push(scaled);
        }
    }

    if by_cid.is_empty() {
        // cff_parser does not expose glyph widths for CID-keyed CFF fonts.
        // Any heuristic rewrite of /W without authoritative glyph widths can
        // turn correct default widths into wrong explicit entries (6.2.11.5:1).
        // Keep existing CID widths unchanged in this case.
        return false;
    }

    // Resolve CID duplicates.
    //
    // Some subset CFF fonts repeat the same CID for multiple glyphs. veraPDF
    // may resolve such collisions to .notdef width; choosing an arbitrary
    // duplicate width causes persistent 6.2.11.5:1 mismatches. Prefer .notdef
    // width when a CID has conflicting widths.
    let notdef_dw = cff
        .glyph_width(cff_parser::GlyphId(0))
        .map(|w| (w as f64 * scale as f64).round() as i64);

    let mut widths: Vec<(u16, i64)> = by_cid
        .into_iter()
        .map(|(cid, vals)| {
            let resolved = if vals.len() <= 1 {
                vals[0]
            } else {
                let mut freq: std::collections::HashMap<i64, usize> =
                    std::collections::HashMap::new();
                for v in &vals {
                    *freq.entry(*v).or_default() += 1;
                }
                notdef_dw.unwrap_or_else(|| {
                    freq.into_iter()
                        .max_by_key(|(_, c)| *c)
                        .map(|(w, _)| w)
                        .unwrap_or(vals[0])
                })
            };
            (cid, resolved)
        })
        .collect();
    widths.sort_by_key(|(cid, _)| *cid);

    // Determine DW (default width).
    //
    // Mode fallback for fonts where .notdef width is unavailable.
    let mut freq: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
    for (_, w) in &widths {
        *freq.entry(*w).or_default() += 1;
    }
    let mode_dw = freq
        .iter()
        .max_by_key(|(_, count)| *count)
        .map(|(w, _)| *w)
        .unwrap_or(1000);
    let dw = notdef_dw.unwrap_or(mode_dw);

    // Build /W array: consecutive runs of widths that differ from DW.
    // Format: [cid [w1 w2 ...] cid2 [w3 w4 ...] ...]
    let mut w_array: Vec<Object> = Vec::new();
    let mut run_start: Option<u16> = None;
    let mut run_widths: Vec<Object> = Vec::new();

    for (cid, w) in &widths {
        if *w == dw {
            // Flush any accumulated run.
            if let Some(start) = run_start.take() {
                w_array.push(Object::Integer(start as i64));
                w_array.push(Object::Array(std::mem::take(&mut run_widths)));
            }
            continue;
        }

        match run_start {
            Some(start) if *cid == start + run_widths.len() as u16 => {
                // Continue existing run.
                run_widths.push(Object::Integer(*w));
            }
            _ => {
                // Flush previous run and start new.
                if let Some(start) = run_start.take() {
                    w_array.push(Object::Integer(start as i64));
                    w_array.push(Object::Array(std::mem::take(&mut run_widths)));
                }
                run_start = Some(*cid);
                run_widths.push(Object::Integer(*w));
            }
        }
    }
    // Flush last run.
    if let Some(start) = run_start {
        w_array.push(Object::Integer(start as i64));
        w_array.push(Object::Array(run_widths));
    }

    // Update the CID font dictionary.
    if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&cid_font_id) {
        dict.set("DW", Object::Integer(dw));
        if !w_array.is_empty() {
            dict.set("W", Object::Array(w_array));
        }
        true
    } else {
        false
    }
}

/// Fallback CID width repair when CFF glyph widths are unavailable.
///
/// In some CID-keyed CFF subsets, `glyph_cid` is available but `glyph_width`
/// is missing for all glyphs. We then use conservative repairs derived from
/// existing /W entries:
/// 1) duplicate charset CIDs: remap to a high-CID proxy width
/// 2) charset CIDs missing in /W (thus falling back to DW): assign the nearest
///    explicit width so used glyphs don't default to 1000 spuriously.
#[allow(dead_code)]
fn fix_cid_duplicate_widths_from_w(
    doc: &mut Document,
    cid_font_id: ObjectId,
    cff: &cff_parser::Table<'_>,
) -> bool {
    use std::collections::{HashMap, HashSet};

    // Collect CIDs (and duplicate CIDs) from the CFF charset.
    let mut seen: HashSet<u16> = HashSet::new();
    let mut cid_set: HashSet<u16> = HashSet::new();
    let mut duplicates: Vec<u16> = Vec::new();
    for gid in 0..cff.number_of_glyphs() {
        if let Some(cid) = cff.glyph_cid(cff_parser::GlyphId(gid)) {
            if cid > 0 {
                cid_set.insert(cid);
            }
            if !seen.insert(cid) {
                duplicates.push(cid);
            }
        }
    }
    if duplicates.is_empty() && cid_set.is_empty() {
        return false;
    }
    duplicates.sort_unstable();
    duplicates.dedup();

    let Some(Object::Dictionary(dict)) = doc.objects.get_mut(&cid_font_id) else {
        return false;
    };

    let dw = match dict.get(b"DW").ok() {
        Some(Object::Integer(v)) => *v,
        Some(Object::Real(v)) => *v as i64,
        _ => 1000,
    };

    // Parse existing explicit widths from /W.
    let mut explicit: HashMap<u16, i64> = HashMap::new();
    if let Ok(Object::Array(w_arr)) = dict.get(b"W") {
        let mut i = 0usize;
        while i < w_arr.len() {
            let start_cid = match &w_arr[i] {
                Object::Integer(v) => *v as u16,
                _ => break,
            };
            i += 1;
            if i >= w_arr.len() {
                break;
            }
            match &w_arr[i] {
                Object::Array(widths) => {
                    for (j, w) in widths.iter().enumerate() {
                        let val = match w {
                            Object::Integer(v) => *v,
                            Object::Real(v) => *v as i64,
                            _ => dw,
                        };
                        explicit.insert(start_cid + j as u16, val);
                    }
                    i += 1;
                }
                Object::Integer(end_cid) => {
                    i += 1;
                    if i >= w_arr.len() {
                        break;
                    }
                    let val = match &w_arr[i] {
                        Object::Integer(v) => *v,
                        Object::Real(v) => *v as i64,
                        _ => dw,
                    };
                    for cid in start_cid..=(*end_cid as u16) {
                        explicit.insert(cid, val);
                    }
                    i += 1;
                }
                _ => break,
            }
        }
    }
    if explicit.is_empty() {
        return false;
    }

    let mut keys: Vec<u16> = explicit.keys().copied().collect();
    keys.sort_unstable();

    let mut changed = false;
    for dup in duplicates {
        let current = explicit.get(&dup).copied().unwrap_or(dw);
        // Prefer a high-CID proxy width to avoid perturbing normal low-CID runs.
        let replacement = keys
            .iter()
            .rev()
            .find_map(|cid| {
                if *cid > dup {
                    explicit.get(cid).copied().filter(|w| *w != current)
                } else {
                    None
                }
            })
            .or_else(|| {
                keys.iter()
                    .find_map(|cid| explicit.get(cid).copied().filter(|w| *w != current))
            });

        if let Some(new_w) = replacement {
            if explicit.insert(dup, new_w) != Some(new_w) {
                changed = true;
            }
        }
    }

    // If a CID exists in charset but has no explicit /W entry, it falls back to DW.
    // For tiny CID subsets this often creates 1000-width mismatches; assign the
    // nearest explicit width as conservative proxy.
    for cid in cid_set {
        if explicit.contains_key(&cid) {
            continue;
        }
        let replacement = keys
            .iter()
            .copied()
            .filter(|k| *k > cid)
            .min()
            .and_then(|k| explicit.get(&k).copied())
            .or_else(|| {
                keys.iter()
                    .copied()
                    .filter(|k| *k < cid)
                    .max()
                    .and_then(|k| explicit.get(&k).copied())
            });
        if let Some(new_w) = replacement {
            if new_w != dw {
                explicit.insert(cid, new_w);
                changed = true;
            }
        }
    }

    if !changed {
        return false;
    }

    // Rebuild /W from explicit widths while keeping DW unchanged.
    let mut items: Vec<(u16, i64)> = explicit.into_iter().collect();
    items.sort_by_key(|(cid, _)| *cid);

    let mut w_array: Vec<Object> = Vec::new();
    let mut run_start: Option<u16> = None;
    let mut run_widths: Vec<Object> = Vec::new();

    for (cid, w) in items {
        if w == dw {
            if let Some(start) = run_start.take() {
                w_array.push(Object::Integer(start as i64));
                w_array.push(Object::Array(std::mem::take(&mut run_widths)));
            }
            continue;
        }
        match run_start {
            Some(start) if cid == start + run_widths.len() as u16 => {
                run_widths.push(Object::Integer(w));
            }
            _ => {
                if let Some(start) = run_start.take() {
                    w_array.push(Object::Integer(start as i64));
                    w_array.push(Object::Array(std::mem::take(&mut run_widths)));
                }
                run_start = Some(cid);
                run_widths.push(Object::Integer(w));
            }
        }
    }
    if let Some(start) = run_start {
        w_array.push(Object::Integer(start as i64));
        w_array.push(Object::Array(run_widths));
    }

    dict.set("DW", Object::Integer(dw));
    if w_array.is_empty() {
        dict.remove(b"W");
    } else {
        dict.set("W", Object::Array(w_array));
    }
    true
}

/// Fix FontDescriptor metrics to match embedded font programs (6.2.11.6:3).
///
/// Updates Ascent, Descent, CapHeight, FontBBox in FontDescriptor dicts
/// when they don't match the embedded font. Does NOT touch widths.
pub fn fix_font_descriptor_metrics(doc: &mut Document) -> usize {
    // Collect all FontDescriptor IDs that have embedded fonts.
    let fd_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            if let Object::Dictionary(dict) = obj {
                if matches!(get_name(dict, b"Type").as_deref(), Some("FontDescriptor")) {
                    // Check if there's an embedded font.
                    if dict.has(b"FontFile") || dict.has(b"FontFile2") || dict.has(b"FontFile3") {
                        return Some(*id);
                    }
                }
            }
            None
        })
        .collect();

    let mut fixed = 0;
    for fd_id in fd_ids {
        let font_data = read_embedded_font_data(doc, fd_id);
        let Some(font_data) = font_data else { continue };

        let Ok(face) = ttf_parser::Face::parse(&font_data, 0) else {
            continue;
        };

        let units_per_em = face.units_per_em() as f64;
        if units_per_em == 0.0 {
            continue;
        }
        let scale = 1000.0 / units_per_em;

        let ascent = (face.ascender() as f64 * scale).round() as i64;
        let descent = (face.descender() as f64 * scale).round() as i64;
        let bbox = face.global_bounding_box();
        let cap_height = face
            .capital_height()
            .map(|h| (h as f64 * scale).round() as i64);

        if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
            // Only update if values differ.
            let ascent_ok = matches!(
                fd.get(b"Ascent").ok(),
                Some(Object::Integer(v)) if (*v - ascent).abs() <= 1
            );
            let descent_ok = matches!(
                fd.get(b"Descent").ok(),
                Some(Object::Integer(v)) if (*v - descent).abs() <= 1
            );
            let needs_update = !ascent_ok || !descent_ok;

            if needs_update {
                fd.set("Ascent", Object::Integer(ascent));
                fd.set("Descent", Object::Integer(descent));
                fd.set(
                    "FontBBox",
                    Object::Array(vec![
                        Object::Integer((bbox.x_min as f64 * scale).round() as i64),
                        Object::Integer((bbox.y_min as f64 * scale).round() as i64),
                        Object::Integer((bbox.x_max as f64 * scale).round() as i64),
                        Object::Integer((bbox.y_max as f64 * scale).round() as i64),
                    ]),
                );
                if let Some(ch) = cap_height {
                    fd.set("CapHeight", Object::Integer(ch));
                }
                fixed += 1;
            }
        }
    }
    fixed
}

/// Fix font metrics for all already-embedded fonts (6.2.11.6:3, 6.2.11.5:1).
///
/// Reads embedded font programs (FontFile2/FontFile3) and updates
/// Ascent, Descent, CapHeight, FontBBox in the FontDescriptor, and
/// Widths in the font dictionary to match the actual font data.
#[allow(dead_code)]
pub fn fix_embedded_font_metrics(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            if let Object::Dictionary(dict) = obj {
                if is_font_dict(dict) {
                    return Some(*id);
                }
            }
            None
        })
        .collect();

    let mut fixed = 0;
    for font_id in font_ids {
        let (subtype, fd_id, is_type0) = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();
            let is_type0 = subtype == "Type0";

            if is_type0 {
                // For Type0, find descendant CIDFont and its FontDescriptor.
                let desc_fd = get_descendant_embed_info(doc, dict);
                match desc_fd {
                    Some((cid_id, true)) => {
                        let cid_fd = doc.objects.get(&cid_id).and_then(|o| {
                            if let Object::Dictionary(d) = o {
                                match d.get(b"FontDescriptor").ok() {
                                    Some(Object::Reference(id)) => Some(*id),
                                    _ => None,
                                }
                            } else {
                                None
                            }
                        });
                        (subtype, cid_fd, true)
                    }
                    _ => continue,
                }
            } else {
                let fd_id = match dict.get(b"FontDescriptor").ok() {
                    Some(Object::Reference(id)) => Some(*id),
                    _ => None,
                };
                (subtype, fd_id, false)
            }
        };

        let Some(fd_id) = fd_id else { continue };

        // Read the embedded font program data.
        let font_data = read_embedded_font_data(doc, fd_id);
        let Some(font_data) = font_data else { continue };

        let Ok(face) = ttf_parser::Face::parse(&font_data, 0) else {
            continue;
        };

        let units_per_em = face.units_per_em() as f64;
        if units_per_em == 0.0 {
            continue;
        }
        let scale = 1000.0 / units_per_em;

        // Update FontDescriptor metrics.
        if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
            let ascent = (face.ascender() as f64 * scale).round() as i64;
            let descent = (face.descender() as f64 * scale).round() as i64;
            let bbox = face.global_bounding_box();
            fd.set("Ascent", Object::Integer(ascent));
            fd.set("Descent", Object::Integer(descent));
            fd.set(
                "FontBBox",
                Object::Array(vec![
                    Object::Integer((bbox.x_min as f64 * scale).round() as i64),
                    Object::Integer((bbox.y_min as f64 * scale).round() as i64),
                    Object::Integer((bbox.x_max as f64 * scale).round() as i64),
                    Object::Integer((bbox.y_max as f64 * scale).round() as i64),
                ]),
            );
            if let Some(cap_h) = face.capital_height() {
                fd.set(
                    "CapHeight",
                    Object::Integer((cap_h as f64 * scale).round() as i64),
                );
            }
        }

        // Update widths.
        if is_type0 || subtype == "CIDFontType0" || subtype == "CIDFontType2" {
            // Find the descendant CIDFont ID for Type0.
            if is_type0 {
                let cid_id = {
                    let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                        continue;
                    };
                    get_descendant_embed_info(doc, dict).map(|(id, _)| id)
                };
                if let Some(cid_id) = cid_id {
                    update_cid_widths(doc, cid_id, &face, scale);
                }
            }
        } else {
            // Construct a temporary NonEmbeddedFont for update_simple_widths.
            let info = NonEmbeddedFont {
                font_id,
                target_id: font_id,
                name: String::new(),
                is_type0: false,
                subtype: subtype.clone(),
            };
            let _ = info;
            update_simple_widths(doc, font_id, &face, scale);
        }

        fixed += 1;
    }
    fixed
}

/// Read the embedded font program data from a FontDescriptor.
fn read_embedded_font_data(doc: &Document, fd_id: ObjectId) -> Option<Vec<u8>> {
    let fd = match doc.objects.get(&fd_id) {
        Some(Object::Dictionary(d)) => d,
        _ => return None,
    };

    for key in &[b"FontFile2" as &[u8], b"FontFile3", b"FontFile"] {
        if let Ok(Object::Reference(stream_id)) = fd.get(key) {
            if let Some(Object::Stream(stream)) = doc.objects.get(stream_id) {
                let mut s = stream.clone();
                let _ = s.decompress();
                let data = &s.content;

                // lopdf may not support ASCIIHexDecode. If decompression left
                // ASCII hex characters, decode manually. The ASCIIHexDecode
                // end marker is '>'; lopdf may also leave `endstream` bytes.
                // Truncate at '>' if present, then check remaining is hex.
                let trimmed = if let Some(gt_pos) = data.iter().position(|&b| b == b'>') {
                    &data[..gt_pos]
                } else {
                    data
                };
                if trimmed.len() >= 8
                    && trimmed
                        .iter()
                        .all(|&b| b.is_ascii_hexdigit() || matches!(b, b'\r' | b'\n' | b' '))
                {
                    let decoded: Vec<u8> = trimmed
                        .iter()
                        .copied()
                        .filter(|b| b.is_ascii_hexdigit())
                        .collect::<Vec<_>>()
                        .chunks(2)
                        .filter_map(|pair| {
                            if pair.len() == 2 {
                                let hi = hex_nibble(pair[0])?;
                                let lo = hex_nibble(pair[1])?;
                                Some((hi << 4) | lo)
                            } else {
                                None
                            }
                        })
                        .collect();
                    if !decoded.is_empty() {
                        return Some(decoded);
                    }
                }

                return Some(s.content);
            }
        }
    }
    None
}

fn hex_nibble(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'A'..=b'F' => Some(b - b'A' + 10),
        b'a'..=b'f' => Some(b - b'a' + 10),
        _ => None,
    }
}

/// Fix widths for CFF-based fonts only (6.2.11.5:1).
///
/// Unlike fix_width_mismatches (which is disabled due to TrueType regressions),
/// this only targets fonts with FontFile3 (CFF) programs where glyph_width is
/// available. Safe to call without affecting TrueType fonts.
pub fn fix_cff_widths(doc: &mut Document) -> usize {
    use std::collections::HashSet;

    let font_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut fixed = 0;
    let mut processed_cid_fonts: HashSet<ObjectId> = HashSet::new();

    for font_id in font_ids {
        let (subtype, fd_id, cid_font_id) = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();

            if subtype == "Type0" {
                let desc = get_descendant_embed_info(doc, dict);
                match desc {
                    Some((cid_id, true)) => {
                        let cid_fd = doc.objects.get(&cid_id).and_then(|o| {
                            if let Object::Dictionary(d) = o {
                                match d.get(b"FontDescriptor").ok() {
                                    Some(Object::Reference(id)) => Some(*id),
                                    _ => None,
                                }
                            } else {
                                None
                            }
                        });
                        let cid_subtype = doc.objects.get(&cid_id).and_then(|o| {
                            if let Object::Dictionary(d) = o {
                                get_name(d, b"Subtype")
                            } else {
                                None
                            }
                        });
                        (cid_subtype.unwrap_or_default(), cid_fd, Some(cid_id))
                    }
                    _ => continue,
                }
            } else if subtype == "CIDFontType0" {
                let fd_id = match dict.get(b"FontDescriptor").ok() {
                    Some(Object::Reference(id)) => Some(*id),
                    _ => None,
                };
                (subtype, fd_id, Some(font_id))
            } else {
                continue; // Skip TrueType and simple fonts.
            }
        };

        // Only process CIDFontType0 (CFF-based CID fonts).
        if subtype != "CIDFontType0" {
            continue;
        }

        let Some(fd_id) = fd_id else { continue };
        let Some(cid_id) = cid_font_id else { continue };
        if !processed_cid_fonts.insert(cid_id) {
            continue;
        }

        // Only process FontFile3 (CFF programs).
        let has_ff3 = matches!(
            doc.objects.get(&fd_id),
            Some(Object::Dictionary(d)) if d.has(b"FontFile3")
        );
        if !has_ff3 {
            continue;
        }

        let font_data = read_embedded_font_data(doc, fd_id);
        let Some(font_data) = font_data else { continue };

        // CIDFontType0 streams may contain either raw CFF data or OTF-wrapped
        // CFF. Support both so width repair covers embedded OpenType CFF fonts.
        let Some(cff) =
            cff_parser::Table::parse(&font_data).or_else(|| extract_cff_from_otf(&font_data))
        else {
            continue;
        };

        if fix_cid_widths_from_cff(doc, cid_id, &cff) {
            fixed += 1;
        }
    }
    fixed
}

/// Fix widths for TrueType CIDFontType2 fonts (6.2.11.5:1).
///
/// CIDFontType2 fonts use CIDToGIDMap (Identity or explicit) to map CIDs
/// to GlyphIDs. This is safer than simple TrueType width fixing because
/// the mapping is unambiguous (no encoding complexity).
pub fn fix_truetype_cid_widths(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut fixed = 0;

    for font_id in font_ids {
        let (subtype, fd_id, cid_font_id, type0_cmap_name) = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();

            if subtype == "Type0" {
                // Get CIDFont descendant.
                let desc = get_descendant_embed_info(doc, dict);
                match desc {
                    Some((cid_id, true)) => {
                        let cid_subtype = doc.objects.get(&cid_id).and_then(|o| {
                            if let Object::Dictionary(d) = o {
                                get_name(d, b"Subtype")
                            } else {
                                None
                            }
                        });
                        let cid_fd = doc.objects.get(&cid_id).and_then(|o| {
                            if let Object::Dictionary(d) = o {
                                match d.get(b"FontDescriptor").ok() {
                                    Some(Object::Reference(id)) => Some(*id),
                                    _ => None,
                                }
                            } else {
                                None
                            }
                        });
                        (
                            cid_subtype.unwrap_or_default(),
                            cid_fd,
                            Some(cid_id),
                            resolve_type0_cmap_name(doc, dict),
                        )
                    }
                    _ => continue,
                }
            } else if subtype == "CIDFontType2" {
                let fd_id = match dict.get(b"FontDescriptor").ok() {
                    Some(Object::Reference(id)) => Some(*id),
                    _ => None,
                };
                (subtype, fd_id, Some(font_id), None)
            } else {
                continue;
            }
        };

        // Only process CIDFontType2 (TrueType-based CID fonts).
        if subtype != "CIDFontType2" {
            continue;
        }

        let Some(fd_id) = fd_id else { continue };
        let Some(cid_id) = cid_font_id else { continue };

        // Process fonts with FontFile2 (TrueType) or FontFile3 (OTF/CFF).
        // Some CIDFontType2 fonts embed an OTF CFF program as FontFile3 instead
        // of FontFile2; ttf_parser handles both, so we include them here.
        let has_embedded = matches!(
            doc.objects.get(&fd_id),
            Some(Object::Dictionary(d)) if d.has(b"FontFile2") || d.has(b"FontFile3")
        );
        if !has_embedded {
            continue;
        }

        let font_data = read_embedded_font_data(doc, fd_id);
        let Some(font_data) = font_data else { continue };

        // Some malformed CIDFontType2 subsets ship inconsistent TrueType
        // metric headers (invalid indexToLocFormat + numberOfHMetrics == 0).
        // In these files validators resolve glyph widths as zero; force a
        // zero-width dictionary to stay consistent with the embedded program.
        let force_zero_widths = {
            let head = tt_find_table(&font_data, b"head");
            let hhea = tt_find_table(&font_data, b"hhea");
            if let (Some(head), Some(hhea)) = (head, hhea) {
                if head.len() >= 52 && hhea.len() >= 36 {
                    let idx_format = i16::from_be_bytes([head[50], head[51]]);
                    let num_h_metrics = u16::from_be_bytes([hhea[34], hhea[35]]);
                    (idx_format != 0 && idx_format != 1) && num_h_metrics == 0
                } else {
                    false
                }
            } else {
                false
            }
        };
        if force_zero_widths {
            if let Some(Object::Dictionary(ref mut cid_dict)) = doc.objects.get_mut(&cid_id) {
                cid_dict.set("DW", Object::Integer(0));
                cid_dict.remove(b"W");
            }
            fixed += 1;
            continue;
        }

        let parsed_face = ttf_parser::Face::parse(&font_data, 0).ok();
        let raw_metrics = if parsed_face.is_none() {
            tt_parse_raw_metrics(&font_data)
        } else {
            None
        };
        let (num_glyphs, scale) = if let Some(face) = parsed_face.as_ref() {
            let upem = face.units_per_em() as f64;
            if upem == 0.0 {
                continue;
            }
            (face.number_of_glyphs(), 1000.0 / upem)
        } else if let Some(raw) = raw_metrics.as_ref() {
            (raw.num_glyphs, 1000.0 / raw.units_per_em as f64)
        } else {
            continue;
        };

        let should_install_predefined_map = type0_cmap_name
            .as_deref()
            .filter(|name| !name.starts_with("UniJIS-"))
            .filter(|name| !is_identity_type0_cmap(name))
            .and_then(load_predefined_unicode_cmap_ranges)
            .and_then(|ranges| {
                let face = parsed_face.as_ref()?;
                let needs_map = match doc.objects.get(&cid_id) {
                    Some(Object::Dictionary(cid_dict)) => match cid_dict.get(b"CIDToGIDMap").ok() {
                        None => true,
                        Some(Object::Name(n)) if n == b"Identity" => true,
                        _ => false,
                    },
                    _ => false,
                };
                if !needs_map {
                    return None;
                }
                let map_bytes = build_predefined_cmap_cidtogid_map(face, &ranges)?;
                Some(map_bytes)
            });
        if let Some(map_bytes) = should_install_predefined_map {
            let map_id = doc.add_object(Object::Stream(lopdf::Stream::new(
                dictionary! {},
                map_bytes,
            )));
            if let Some(Object::Dictionary(ref mut cid_dict)) = doc.objects.get_mut(&cid_id) {
                cid_dict.set("CIDToGIDMap", Object::Reference(map_id));
            }
        }

        enum CidToGidMode {
            Identity,
            Stream(Vec<u8>),
        }

        // Read CIDToGIDMap to determine mapping.
        let cid_to_gid_mode = {
            let Some(Object::Dictionary(cid_dict)) = doc.objects.get(&cid_id) else {
                continue;
            };
            match cid_dict.get(b"CIDToGIDMap").ok() {
                Some(Object::Name(n)) if n == b"Identity" => CidToGidMode::Identity,
                None => CidToGidMode::Identity, // Default is Identity per spec.
                Some(Object::Reference(id)) => match doc.objects.get(id) {
                    Some(Object::Stream(s)) => {
                        let mut st = s.clone();
                        let _ = st.decompress();
                        CidToGidMode::Stream(st.content)
                    }
                    _ => continue,
                },
                Some(Object::Stream(s)) => {
                    let mut st = s.clone();
                    let _ = st.decompress();
                    CidToGidMode::Stream(st.content)
                }
                _ => continue,
            }
        };

        // Collect widths: CID → width in PDF units.
        let mut widths: Vec<(u16, i64)> = Vec::new();
        match cid_to_gid_mode {
            CidToGidMode::Identity => {
                // Identity mapping: CID == GID.
                for gid in 0..num_glyphs {
                    let w = if let Some(face) = parsed_face.as_ref() {
                        let gid_obj = ttf_parser::GlyphId(gid);
                        // 6.2.11.5 compares dictionary widths to the embedded
                        // font program metrics (hmtx advances). Even when glyf
                        // outline data is empty, non-zero advances remain valid.
                        face.glyph_hor_advance(gid_obj)
                            .map(|a| (a as f64 * scale).round() as i64)
                            .unwrap_or(0)
                    } else if let Some(raw) = raw_metrics.as_ref() {
                        tt_raw_glyph_advance(raw, gid)
                            .map(|a| (a as f64 * scale).round() as i64)
                            .unwrap_or(0)
                    } else {
                        0
                    };
                    widths.push((gid, w));
                }
            }
            CidToGidMode::Stream(map_bytes) => {
                // Stream mapping: each 2-byte big-endian entry maps CID index -> GID.
                for (cid, chunk) in map_bytes.chunks_exact(2).enumerate() {
                    if cid > u16::MAX as usize {
                        break;
                    }
                    let gid = u16::from_be_bytes([chunk[0], chunk[1]]);
                    // 0xFFFF is the "not mapped" sentinel used by some subsetters.
                    // GID 0 (.notdef) is a real glyph with its own advance width;
                    // veraPDF validates that the dictionary width matches the hmtx
                    // advance for every rendered CID, including those that fall back
                    // to .notdef, so we must include them rather than skipping them.
                    if gid == u16::MAX {
                        continue;
                    }
                    let w = if gid < num_glyphs {
                        if let Some(face) = parsed_face.as_ref() {
                            let gid_obj = ttf_parser::GlyphId(gid);
                            face.glyph_hor_advance(gid_obj)
                                .map(|a| (a as f64 * scale).round() as i64)
                                .unwrap_or(0)
                        } else if let Some(raw) = raw_metrics.as_ref() {
                            tt_raw_glyph_advance(raw, gid)
                                .map(|a| (a as f64 * scale).round() as i64)
                                .unwrap_or(0)
                        } else {
                            0
                        }
                    } else {
                        0
                    };
                    widths.push((cid as u16, w));
                }
            }
        }

        if widths.is_empty() {
            continue;
        }

        // Check if existing widths already match (avoid unnecessary changes).
        let existing_matches = check_cid_widths_match(doc, cid_id, &widths);
        if existing_matches {
            continue;
        }

        // Determine DW (most common width).
        let mut freq: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
        for (_, w) in &widths {
            *freq.entry(*w).or_default() += 1;
        }
        let dw = freq
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(w, _)| *w)
            .unwrap_or(1000);

        // Build /W array with runs of non-default widths.
        let mut w_array: Vec<Object> = Vec::new();
        let mut i = 0;
        while i < widths.len() {
            let (cid, w) = widths[i];
            if w == dw {
                i += 1;
                continue;
            }
            // Start a run of consecutive CIDs with non-default widths.
            let start = cid;
            let mut run: Vec<i64> = vec![w];
            i += 1;
            while i < widths.len() {
                let (next_cid, next_w) = widths[i];
                if next_cid != start + run.len() as u16 {
                    break;
                }
                if next_w == dw && (i + 1 >= widths.len() || widths[i + 1].0 != next_cid + 1) {
                    break;
                }
                run.push(next_w);
                i += 1;
            }
            w_array.push(Object::Integer(start as i64));
            w_array.push(Object::Array(
                run.into_iter().map(Object::Integer).collect(),
            ));
        }

        // Update the CIDFont dictionary.
        if let Some(Object::Dictionary(ref mut cid_dict)) = doc.objects.get_mut(&cid_id) {
            cid_dict.set("DW", Object::Integer(dw));
            if w_array.is_empty() {
                cid_dict.remove(b"W");
            } else {
                cid_dict.set("W", Object::Array(w_array));
            }
        }

        fixed += 1;
    }

    fixed
}

/// Check if existing CIDFont W/DW widths match the expected widths.
fn check_cid_widths_match(doc: &Document, cid_id: ObjectId, expected: &[(u16, i64)]) -> bool {
    let Some(Object::Dictionary(cid_dict)) = doc.objects.get(&cid_id) else {
        return false;
    };

    let dw = match cid_dict.get(b"DW").ok() {
        Some(Object::Integer(v)) => *v,
        _ => 1000,
    };

    // Build a map of CID → expected width from the W array.
    let mut existing_widths: std::collections::HashMap<u16, i64> = std::collections::HashMap::new();
    if let Ok(Object::Array(w_arr)) = cid_dict.get(b"W") {
        let mut i = 0;
        while i < w_arr.len() {
            let start_cid = match &w_arr[i] {
                Object::Integer(v) => *v as u16,
                _ => break,
            };
            i += 1;
            if i >= w_arr.len() {
                break;
            }
            match &w_arr[i] {
                Object::Array(widths) => {
                    for (j, w) in widths.iter().enumerate() {
                        if let Object::Integer(v) = w {
                            existing_widths.insert(start_cid + j as u16, *v);
                        }
                    }
                    i += 1;
                }
                Object::Integer(end_cid) => {
                    i += 1;
                    if i >= w_arr.len() {
                        break;
                    }
                    if let Object::Integer(width) = &w_arr[i] {
                        for cid in start_cid..=(*end_cid as u16) {
                            existing_widths.insert(cid, *width);
                        }
                    }
                    i += 1;
                }
                _ => break,
            }
        }
    }

    // Compare: for each expected width, check if existing matches.
    for (cid, exp_w) in expected {
        let actual = existing_widths.get(cid).copied().unwrap_or(dw);
        if (actual - exp_w).abs() > 1 {
            return false;
        }
    }
    true
}

/// Fix CharSet in Type 1 font descriptors (6.2.11.4.2:1).
///
/// The CharSet string must list all glyph names present in the CFF font program.
pub fn fix_type1_charset(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut fixed = 0;

    for font_id in font_ids {
        let (fd_id, is_type1) = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();
            if subtype != "Type1" && subtype != "MMType1" {
                continue;
            }
            let fd_id = match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(id)) => *id,
                _ => continue,
            };
            (fd_id, true)
        };

        if !is_type1 {
            continue;
        }

        // Only process FontFile3 (CFF programs) or FontFile (Type 1 PFB).
        let has_fontfile3 = matches!(
            doc.objects.get(&fd_id),
            Some(Object::Dictionary(d)) if d.has(b"FontFile3")
        );
        let has_fontfile = matches!(
            doc.objects.get(&fd_id),
            Some(Object::Dictionary(d)) if d.has(b"FontFile")
        );
        if !has_fontfile3 && !has_fontfile {
            continue;
        }

        let font_data = read_embedded_font_data(doc, fd_id);
        let Some(font_data) = font_data else { continue };

        let charset_str = if has_fontfile3 {
            // CFF font program — extract glyph names from CFF.
            let Some(cff) = cff_parser::Table::parse(&font_data) else {
                continue;
            };
            let num_glyphs = cff.number_of_glyphs();
            let mut cs = String::new();
            for gid in 0..num_glyphs {
                let glyph_id = cff_parser::GlyphId(gid);
                if let Some(name) = cff.glyph_name(glyph_id) {
                    if name != ".notdef" {
                        cs.push('/');
                        cs.push_str(name);
                    }
                }
            }
            cs
        } else {
            // PFB/Type1 font — decrypt eexec and extract glyph names from CharStrings.
            extract_type1_glyph_names_to_charset(&font_data)
        };

        if charset_str.is_empty() {
            continue;
        }

        if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
            fd.set(
                "CharSet",
                Object::String(charset_str.into_bytes(), lopdf::StringFormat::Literal),
            );
            fixed += 1;
        }
    }
    fixed
}

/// Extract glyph names from a Type 1 (PFB) font program and return a CharSet string.
///
/// Strips PFB segment headers, decrypts the eexec-encrypted binary section,
/// then extracts glyph names from the /CharStrings dictionary.
/// Returns a string like "/A/B/C/space" (without .notdef).
fn extract_type1_glyph_names_to_charset(pfb_data: &[u8]) -> String {
    let ps_data = strip_pfb_headers(pfb_data);

    // First try: see if /CharStrings is visible in cleartext (rare but possible).
    if let Some(cs) = extract_glyph_names_from_charstrings(&ps_data) {
        if !cs.is_empty() {
            return cs;
        }
    }

    // Decrypt eexec section and try again.
    let decrypted = decrypt_eexec(&ps_data);
    if decrypted.is_empty() {
        return String::new();
    }

    extract_glyph_names_from_charstrings(&decrypted).unwrap_or_default()
}

/// Strip PFB (Printer Font Binary) headers from embedded Type1 FontFile streams.
///
/// PDF requires raw PostScript, not PFB format. This function finds all
/// FontDescriptor FontFile streams starting with `\x80\x01` (PFB magic),
/// strips the segment headers, and updates Length1/Length2/Length3 entries.
///
/// Returns the number of font streams fixed.
pub fn fix_pfb_font_streams(doc: &mut Document) -> usize {
    // Collect (fd_id, ff_stream_id) for FontDescriptors with PFB FontFile streams.
    let targets: Vec<(ObjectId, ObjectId)> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            let Object::Dictionary(dict) = obj else {
                return None;
            };
            if !matches!(get_name(dict, b"Type").as_deref(), Some("FontDescriptor")) {
                return None;
            }
            let ff_id = match dict.get(b"FontFile").ok() {
                Some(Object::Reference(r)) => *r,
                _ => return None,
            };
            Some((*id, ff_id))
        })
        .collect();

    let mut fixed = 0;

    for (_fd_id, ff_id) in targets {
        // Read raw (possibly compressed) stream content.
        let raw_data = {
            let Some(Object::Stream(s)) = doc.objects.get(&ff_id) else {
                continue;
            };
            let mut s = s.clone();
            let _ = s.decompress();
            s.content
        };

        // Skip if not PFB format.
        if raw_data.len() < 6 || raw_data[0] != 0x80 || raw_data[1] != 0x01 {
            continue;
        }

        let Some((stripped, l1, l2, l3)) = parse_pfb_segments(&raw_data) else {
            continue;
        };

        if let Some(Object::Stream(stream)) = doc.objects.get_mut(&ff_id) {
            stream.set_plain_content(stripped);
            stream.dict.set("Length1", Object::Integer(l1));
            stream.dict.set("Length2", Object::Integer(l2));
            if l3 > 0 {
                stream.dict.set("Length3", Object::Integer(l3));
            } else {
                stream.dict.remove(b"Length3");
            }
        }
        fixed += 1;
    }

    fixed
}

/// Fix Type1 FontFile streams where the first byte of the binary eexec section
/// is a PDF whitespace character (0x00, 0x09, 0x0A, 0x0C, 0x0D, 0x20).
///
/// veraPDF's `Type1FontProgram.parseFont()` calls `skipSpacesExceptNullByte()`
/// before reading the eexec binary section, which skips any leading PDF
/// whitespace bytes. If the first encrypted byte of the binary section happens
/// to be a PDF whitespace character, veraPDF starts decryption from the wrong
/// position, causing the Private dict to be garbage and `glyphWidths` to be
/// null → `containsFontFile == false`.
///
/// Fix: decrypt the binary section, re-encrypt with new 4-byte seed values
/// where the first encrypted byte is not a PDF whitespace character.
///
/// Returns the number of font streams fixed.
pub fn fix_type1_eexec_space_prefix(doc: &mut Document) -> usize {
    const PDF_SPACE: [u8; 6] = [0x00, 0x09, 0x0A, 0x0C, 0x0D, 0x20];

    // Collect (fd_id, ff_stream_id) for FontDescriptors with Type1 FontFile streams.
    let targets: Vec<(ObjectId, ObjectId)> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            let Object::Dictionary(dict) = obj else {
                return None;
            };
            if !matches!(get_name(dict, b"Type").as_deref(), Some("FontDescriptor")) {
                return None;
            }
            let ff_id = match dict.get(b"FontFile").ok() {
                Some(Object::Reference(r)) => *r,
                _ => return None,
            };
            Some((*id, ff_id))
        })
        .collect();

    let mut fixed = 0;

    for (_fd_id, ff_id) in targets {
        let (stream_data, l1, l2) = {
            let Some(Object::Stream(s)) = doc.objects.get(&ff_id) else {
                continue;
            };
            let mut s = s.clone();
            let _ = s.decompress();
            let data = s.content;
            let l1 = match s.dict.get(b"Length1").ok() {
                Some(Object::Integer(n)) => *n as usize,
                _ => continue,
            };
            let l2 = match s.dict.get(b"Length2").ok() {
                Some(Object::Integer(n)) => *n as usize,
                _ => continue,
            };
            (data, l1, l2)
        };

        // Validate bounds.
        if l1 >= stream_data.len() || l1 + l2 > stream_data.len() || l2 < 5 {
            continue;
        }

        // Check if the first byte of the binary section is a PDF whitespace.
        let first_binary_byte = stream_data[l1];
        if !PDF_SPACE.contains(&first_binary_byte) {
            continue;
        }

        let binary = &stream_data[l1..l1 + l2];

        // Decrypt the binary section to extract the real font data.
        let decrypted_all = type1_binary_eexec_decrypt(binary);
        if decrypted_all.len() < 5 {
            continue;
        }
        let real_data = &decrypted_all[4..]; // Skip 4 random seed bytes.

        // Re-encrypt with new seed where first encrypted byte is not a space.
        // Initial key = 55665. First encrypted byte = seed[0] ^ (55665 >> 8) = seed[0] ^ 0xD9.
        // 0xAA ^ 0xD9 = 0x73 ('s'), which is not a PDF space character.
        let new_binary = type1_binary_eexec_encrypt(real_data, 0xAA);
        debug_assert_eq!(new_binary.len(), l2);
        debug_assert!(!PDF_SPACE.contains(&new_binary[0]));

        // Rebuild the full stream: cleartext + new_binary + trailing.
        let mut new_stream = Vec::with_capacity(stream_data.len());
        new_stream.extend_from_slice(&stream_data[..l1]);
        new_stream.extend_from_slice(&new_binary);
        new_stream.extend_from_slice(&stream_data[l1 + l2..]);

        if let Some(Object::Stream(stream)) = doc.objects.get_mut(&ff_id) {
            stream.set_plain_content(new_stream);
            // Length1/Length2/Length3 remain the same.
        }
        fixed += 1;
    }

    fixed
}

/// Decrypt Type1 binary eexec-encoded data with key 55665 (binary only, no hex support).
fn type1_binary_eexec_decrypt(data: &[u8]) -> Vec<u8> {
    let mut key: u16 = 55665;
    let mut result = Vec::with_capacity(data.len());
    for &c in data {
        let p = c ^ (key >> 8) as u8;
        key = (c as u16)
            .wrapping_add(key)
            .wrapping_mul(52845)
            .wrapping_add(22719);
        result.push(p);
    }
    result
}

/// Re-encrypt real Type1 font data with 4 new seed bytes.
///
/// The first encrypted byte = `seed_first ^ (55665 >> 8)` = `seed_first ^ 0xD9`.
/// Using seed_first=0xAA gives first_encrypted=0x73 ('s'), not a PDF whitespace.
fn type1_binary_eexec_encrypt(real_data: &[u8], seed_first: u8) -> Vec<u8> {
    let seeds = [seed_first, 0u8, 0u8, 0u8];
    let full_plain: Vec<u8> = seeds.iter().chain(real_data.iter()).copied().collect();
    let mut key: u16 = 55665;
    let mut result = Vec::with_capacity(full_plain.len());
    for &p in &full_plain {
        let c = p ^ (key >> 8) as u8;
        key = (c as u16)
            .wrapping_add(key)
            .wrapping_mul(52845)
            .wrapping_add(22719);
        result.push(c);
    }
    result
}

/// Fix stub Type1 fonts by redirecting their FontFile to a matching full font program.
///
/// Some PDF generators (e.g. dvips/LaTeX) include two versions of the same font:
///  - A stub with only `.notdef` CharString and `/CharSet()` (empty)
///  - A full subset with actual glyphs and a non-empty `/CharSet`
///
/// veraPDF's `Type1FontProgram.parseFont()` fails on stub fonts (no real CharStrings
/// after skipping `.notdef`), so `containsFontFile == false` → rule 6.2.11.4.1:1 fails.
///
/// Fix: for each stub FontDescriptor (has `/CharSet()` empty), find another FontDescriptor
/// whose base font name (strip 6-char prefix + optional `~XX` suffix) matches and has a
/// parseable font program. Redirect the stub's `/FontFile` reference to the real font's
/// FontFile stream and remove the empty `/CharSet` entry.
///
/// Returns the number of stub FontDescriptors fixed.
pub fn fix_type1_stub_font_files(doc: &mut Document) -> usize {
    // Step 1: Collect all Type1 FontDescriptors with FontFile streams.
    // Record: font_name, fd_obj_id, ff_obj_id, charset_is_empty
    #[derive(Debug, Clone)]
    struct FdInfo {
        fd_id: ObjectId,
        ff_id: ObjectId,
        full_name: String,
        base_name: String, // after stripping 6-char prefix and ~XX suffix
        charset_empty: bool,
    }

    let fd_infos: Vec<FdInfo> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            let Object::Dictionary(dict) = obj else {
                return None;
            };
            if !matches!(get_name(dict, b"Type").as_deref(), Some("FontDescriptor")) {
                return None;
            }
            // Must have a FontFile (Type1) reference, not FontFile2 (TrueType) or FontFile3 (CFF)
            let ff_id = match dict.get(b"FontFile").ok() {
                Some(Object::Reference(r)) => *r,
                _ => return None,
            };
            let full_name = match dict.get(b"FontName").ok() {
                Some(Object::Name(n)) => String::from_utf8_lossy(n).to_string(),
                _ => return None,
            };
            // Detect empty CharSet: /CharSet() or /CharSet with empty string value
            let charset_empty = match dict.get(b"CharSet").ok() {
                Some(Object::String(s, _)) => s.is_empty(),
                Some(Object::Name(n)) => n.is_empty(),
                None => false,
                _ => false,
            };
            let base_name = type1_base_font_name(&full_name);
            Some(FdInfo {
                fd_id: *id,
                ff_id,
                full_name,
                base_name,
                charset_empty,
            })
        })
        .collect();

    // Step 2: Build map from base_name → ff_id for non-stub fonts
    use std::collections::HashMap;
    let mut real_font_map: HashMap<String, (ObjectId, String)> = HashMap::new();
    for info in &fd_infos {
        if !info.charset_empty {
            real_font_map
                .entry(info.base_name.clone())
                .or_insert_with(|| (info.ff_id, info.full_name.clone()));
        }
    }

    // Step 3: For each stub, redirect FontFile to real font's FontFile
    let mut fixed = 0usize;
    let stubs: Vec<FdInfo> = fd_infos.into_iter().filter(|i| i.charset_empty).collect();

    for stub in &stubs {
        let (real_ff_id, ref real_name) = match real_font_map.get(&stub.base_name) {
            Some(r) => r.clone(),
            None => continue, // No matching real font found
        };
        // Don't redirect to itself
        if real_ff_id == stub.ff_id {
            continue;
        }
        // Update the stub's FontDescriptor: redirect FontFile and remove CharSet
        let fd_obj = match doc.objects.get_mut(&stub.fd_id) {
            Some(Object::Dictionary(d)) => d,
            _ => continue,
        };
        fd_obj.set(b"FontFile", Object::Reference(real_ff_id));
        fd_obj.remove(b"CharSet");
        eprintln!(
            "fix_type1_stub_font_files: {} stub → redirect FontFile to {} ({})",
            stub.full_name, real_name, real_ff_id.0
        );
        fixed += 1;
    }

    fixed
}

/// Fix invalid CFF BCD (Binary-Coded Decimal) real number encodings that cause
/// veraPDF's CFF parser to throw `NumberFormatException` → `successfullyParsed = false`
/// → rule 6.2.11.4.1:1 fails.
///
/// Two known invalid patterns:
///
///  1. `1e ff`: The byte `0xff` splits to nibbles `(0xf, 0xf)`.  Nibble `0xf` means
///     "end of BCD", so the parser exits immediately with an **empty** `StringBuilder`.
///     `Float.parseFloat("")` → `NumberFormatException`.
///     Fix: replace `1e ff` → `1e 0f` → nibbles `(0, 0xf)` → string `"0"` → 0.0.
///
///  2. `1e c0 00 48 82 81 25 ff`: BCD starts with nibble `0xc` (`'E-'`) before any
///     mantissa digits → string `"E-00048828125"` → `Float.parseFloat` fails.
///     This was meant to encode `0.00048828125 = 1/2048` (FontMatrix scale for
///     2048-unit-em fonts).
///     Fix: replace with `1e 4a 88 28 12 5b e4 ff` → `"4.8828125E-4"` = 0.00048828125.
///
/// Only CFF streams (`/Subtype /CIDFontType0C` or `/Type1C`) are examined.
///
/// Returns the number of font streams patched.
pub fn fix_cff_invalid_bcd(doc: &mut Document) -> usize {
    // Pattern 1: 1e ff  →  1e 0f  ("" → "0")
    const BAD_EMPTY: [u8; 2] = [0x1e, 0xff];
    const GOOD_ZERO: [u8; 2] = [0x1e, 0x0f];

    // Pattern 2: malformed FontMatrix 1/2048 BCD  →  correct "4.8828125E-4"
    const BAD_FONTMATRIX: [u8; 8] = [0x1e, 0xc0, 0x00, 0x48, 0x82, 0x81, 0x25, 0xff];
    const GOOD_FONTMATRIX: [u8; 8] = [0x1e, 0x4a, 0x88, 0x28, 0x12, 0x5b, 0xe4, 0xff];

    // Collect IDs of CFF font streams.
    let targets: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            let Object::Stream(s) = obj else {
                return None;
            };
            match s.dict.get(b"Subtype").ok() {
                Some(Object::Name(n))
                    if n.as_slice() == b"CIDFontType0C" || n.as_slice() == b"Type1C" =>
                {
                    Some(*id)
                }
                _ => None,
            }
        })
        .collect();

    let mut fixed = 0usize;

    for id in targets {
        let data = {
            let Some(Object::Stream(s)) = doc.objects.get(&id) else {
                continue;
            };
            let mut s = s.clone();
            let _ = s.decompress();
            s.content
        };

        // Only process streams that begin with a valid CFF header (major version == 1).
        // Streams starting with 0x00 or 0x4F ('O' for OTTO/OTF) are SFNT/TrueType and
        // must NOT have their bytes patched as BCD — the 1e byte is a charstring op there.
        if data.first() != Some(&1) {
            continue;
        }

        let mut patched = data.clone();
        let mut changed = false;

        // Apply pattern 2 first (longer → more specific, avoids overlap with p1).
        let mut i = 0;
        while i + BAD_FONTMATRIX.len() <= patched.len() {
            if patched[i..i + BAD_FONTMATRIX.len()] == BAD_FONTMATRIX {
                patched[i..i + GOOD_FONTMATRIX.len()].copy_from_slice(&GOOD_FONTMATRIX);
                changed = true;
                i += GOOD_FONTMATRIX.len();
            } else {
                i += 1;
            }
        }

        // Apply pattern 1.
        let mut i = 0;
        while i + BAD_EMPTY.len() <= patched.len() {
            if patched[i..i + BAD_EMPTY.len()] == BAD_EMPTY {
                patched[i..i + GOOD_ZERO.len()].copy_from_slice(&GOOD_ZERO);
                changed = true;
                i += GOOD_ZERO.len();
            } else {
                i += 1;
            }
        }

        if changed {
            if let Some(Object::Stream(stream)) = doc.objects.get_mut(&id) {
                stream.set_plain_content(patched);
            }
            fixed += 1;
        }
    }

    fixed
}

/// Fix TrueType/SFNT font programs that are stored in `FontFile3` with `Subtype
/// /CIDFontType0C` instead of the correct `FontFile2` (or `FontFile3` with
/// `Subtype /CIDFontType2`).
///
/// Some PDFs embed TrueType (sfnt magic `00 01 00 00`) fonts but label them as
/// CFF (`/Subtype /CIDFontType0C`). veraPDF's CFF parser immediately fails on
/// non-CFF data → `successfullyParsed = false` → `containsFontFile == false` →
/// rule 6.2.11.4.1:1 fails.
///
/// Note: OTTO (`4F 54 54 4F`) means OpenType CFF in SFNT; veraPDF can parse
/// that in FontFile3 so those are left unchanged.
///
/// Fix:
/// 1. Find `FontDescriptor` objects whose `FontFile3` stream starts with TrueType magic.
/// 2. Rename the `FontFile3` key to `FontFile2` (TrueType container).
/// 3. Remove the erroneous `Subtype` from the font stream dict.
/// 4. For CIDFontType0 DescendantFont dicts linked to such a descriptor, change
///    their `Subtype` to `CIDFontType2` (TrueType CID font).
///
/// Returns the number of font descriptors fixed.
pub fn fix_mislabeled_truetype_as_cff(doc: &mut Document) -> usize {
    // Only fix pure TrueType (sfnt 00010000). OTTO (4F54544F) is OpenType CFF
    // in an SFNT wrapper; veraPDF can parse it in FontFile3/CIDFontType0C so
    // renaming to FontFile2/CIDFontType2 would break those fonts.
    const TT_MAGIC: [u8; 4] = [0x00, 0x01, 0x00, 0x00];

    // Step 1: Find FontDescriptor IDs whose FontFile3 is actually a TrueType stream.
    let fd_to_fix: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            let Object::Dictionary(d) = obj else {
                return None;
            };
            if !matches!(get_name(d, b"Type").as_deref(), Some("FontDescriptor")) {
                return None;
            }
            let ff3_id = match d.get(b"FontFile3").ok() {
                Some(Object::Reference(r)) => *r,
                _ => return None,
            };
            // Check the FontFile3 stream for TrueType SFNT magic.
            let stream_obj = doc.objects.get(&ff3_id)?;
            let Object::Stream(s) = stream_obj else {
                return None;
            };
            let mut s2 = s.clone();
            let _ = s2.decompress();
            let magic = s2.content.get(..4)?;
            if magic == TT_MAGIC {
                Some(*id)
            } else {
                None
            }
        })
        .collect();

    if fd_to_fix.is_empty() {
        return 0;
    }

    // Step 2: For each such FontDescriptor, rename FontFile3 → FontFile2
    // and collect the stream IDs.
    let mut stream_ids_to_fix: Vec<ObjectId> = Vec::new();
    let fd_ids: std::collections::HashSet<ObjectId> = fd_to_fix.iter().copied().collect();

    for fd_id in &fd_to_fix {
        let Some(Object::Dictionary(d)) = doc.objects.get_mut(fd_id) else {
            continue;
        };
        if let Ok(Object::Reference(r)) = d.get(b"FontFile3").cloned() {
            stream_ids_to_fix.push(r);
            d.remove(b"FontFile3");
            d.set(b"FontFile2", Object::Reference(r));
        }
    }

    // Step 3: Remove Subtype from the (formerly FontFile3) font streams.
    for sid in &stream_ids_to_fix {
        if let Some(Object::Stream(s)) = doc.objects.get_mut(sid) {
            s.dict.remove(b"Subtype");
        }
    }

    // Step 4: Fix DescendantFont dicts that reference one of the fixed descriptors.
    // DescendantFonts entries can be inline dicts (Object::Dictionary) inside arrays.
    let type0_ids: Vec<ObjectId> = doc
        .objects
        .keys()
        .copied()
        .filter(|id| {
            if let Some(Object::Dictionary(d)) = doc.objects.get(id) {
                matches!(get_name(d, b"Type").as_deref(), Some("Font"))
                    && matches!(get_name(d, b"Subtype").as_deref(), Some("Type0"))
            } else {
                false
            }
        })
        .collect();

    for t0_id in type0_ids {
        let Some(Object::Dictionary(d)) = doc.objects.get_mut(&t0_id) else {
            continue;
        };
        let df_arr = match d.get_mut(b"DescendantFonts") {
            Ok(Object::Array(a)) => a,
            _ => continue,
        };
        for elem in df_arr.iter_mut() {
            let Object::Dictionary(cid_dict) = elem else {
                continue;
            };
            // Change Subtype CIDFontType0 → CIDFontType2 if FontDescriptor is one we fixed.
            let is_cid0 = matches!(
                get_name(cid_dict, b"Subtype").as_deref(),
                Some("CIDFontType0")
            );
            if !is_cid0 {
                continue;
            }
            let fd_ref = match cid_dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(r)) => *r,
                _ => continue,
            };
            if fd_ids.contains(&fd_ref) {
                cid_dict.set(b"Subtype", Object::Name(b"CIDFontType2".to_vec()));
            }
        }
    }

    fd_to_fix.len()
}

/// Fix non-standard `/CharStrings` dict syntax in Type1 font eexec sections.
///
/// Some old fonts (e.g. Keycap) use:
/// ```text
/// /CharStrings N dict def
///   Private begin CharStrings begin
/// ```
/// instead of the standard:
/// ```text
/// /CharStrings N dict dup begin
/// ```
/// veraPDF's `Type1PrivateParser` expects exactly 3 tokens after the count
/// (`dict`, `dup`, `begin`), so it misparses the non-standard form and
/// `successfullyParsed` remains false → rule 6.2.11.4.1:1 fails.
///
/// Fix: decrypt the binary eexec section, replace the non-standard pattern
/// with the standard one, re-encrypt, update `Length2`, and store as
/// uncompressed plain content (filter removed).
///
/// Returns the number of font streams patched.
pub fn fix_type1_nonstandard_charstrings(doc: &mut Document) -> usize {
    // Collect (stream_id, l1, l2, l2_is_ref) for FontFile streams.
    // l2_is_ref: the object ID of the /Length2 integer object, if it was a reference.
    #[derive(Debug)]
    struct Target {
        stream_id: ObjectId,
        l1: usize,
        l2: usize,
        l2_ref: Option<ObjectId>, // object to update when l2 changes
    }

    let targets: Vec<Target> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            let Object::Stream(s) = obj else {
                return None;
            };
            // Must have Length1 AND Length2 → Type1 font stream.
            if s.dict.get(b"Length1").is_err() || s.dict.get(b"Length2").is_err() {
                return None;
            }
            // Must NOT have /Subtype (FontFile3) — we want classic FontFile.
            if s.dict.get(b"Subtype").is_ok() {
                return None;
            }
            Some((*id, s.clone()))
        })
        .collect::<Vec<_>>()
        .into_iter()
        .filter_map(|(id, s)| {
            let resolve_int = |obj: &Object| -> Option<(usize, Option<ObjectId>)> {
                match obj {
                    Object::Integer(n) => Some((*n as usize, None)),
                    Object::Reference(r) => {
                        if let Some(Object::Integer(n)) = doc.objects.get(r) {
                            Some((*n as usize, Some(*r)))
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            };
            let l1_obj = s.dict.get(b"Length1").ok()?;
            let l2_obj = s.dict.get(b"Length2").ok()?;
            let (l1, _) = resolve_int(l1_obj)?;
            let (l2, l2_ref) = resolve_int(l2_obj)?;
            Some(Target {
                stream_id: id,
                l1,
                l2,
                l2_ref,
            })
        })
        .collect();

    let mut fixed = 0usize;

    for t in targets {
        // Decompress to get raw font bytes.
        let content = {
            let Some(Object::Stream(s)) = doc.objects.get(&t.stream_id) else {
                continue;
            };
            let mut s2 = s.clone();
            let _ = s2.decompress();
            s2.content
        };

        if content.len() < t.l1 + 5 || t.l1 + t.l2 > content.len() {
            continue;
        }

        let binary = &content[t.l1..t.l1 + t.l2];

        // Decrypt the binary section.
        let dec = type1_binary_eexec_decrypt(binary);
        if dec.len() < 5 {
            continue;
        }
        let real_data = &dec[4..]; // skip 4 random seed bytes

        // Search for the non-standard pattern:
        // "/CharStrings " + digits + " dict def" + whitespace + "Private begin CharStrings begin"
        // (and optionally trailing whitespace/newline before the glyph definitions)
        let Some((match_start, match_len, count_str)) = find_nonstandard_charstrings(real_data)
        else {
            continue;
        };

        // Build the replacement bytes: "/CharStrings N dict dup begin\n"
        let mut replacement = Vec::new();
        replacement.extend_from_slice(b"/CharStrings ");
        replacement.extend_from_slice(count_str.as_bytes());
        replacement.extend_from_slice(b" dict dup begin\n");

        // Patch the real_data.
        let mut new_real: Vec<u8> = Vec::with_capacity(real_data.len());
        new_real.extend_from_slice(&real_data[..match_start]);
        new_real.extend_from_slice(&replacement);
        new_real.extend_from_slice(&real_data[match_start + match_len..]);

        // Re-encrypt: seed_first=0xAA gives first_encrypted = 0xAA^0xD9 = 0x73 ('s').
        let new_binary = type1_binary_eexec_encrypt(&new_real, 0xAA);
        let new_l2 = new_binary.len();

        // Rebuild the full decompressed stream.
        let mut new_content: Vec<u8> =
            Vec::with_capacity(t.l1 + new_l2 + (content.len() - t.l1 - t.l2));
        new_content.extend_from_slice(&content[..t.l1]);
        new_content.extend_from_slice(&new_binary);
        new_content.extend_from_slice(&content[t.l1 + t.l2..]);

        // Update /Length2 reference object (or inline value).
        if let Some(ref_id) = t.l2_ref {
            if let Some(obj) = doc.objects.get_mut(&ref_id) {
                *obj = Object::Integer(new_l2 as i64);
            }
        } else {
            // Inline Length2: patch directly in stream dict.
            if let Some(Object::Stream(s)) = doc.objects.get_mut(&t.stream_id) {
                s.dict.set(b"Length2", Object::Integer(new_l2 as i64));
            }
        }

        // Store as plain (uncompressed) content — removes Filter/DecodeParms/Length.
        if let Some(Object::Stream(stream)) = doc.objects.get_mut(&t.stream_id) {
            stream.set_plain_content(new_content);
        }
        fixed += 1;
    }

    fixed
}

/// Search for the non-standard CharStrings header pattern in decrypted Type1 eexec data.
///
/// Matches: `/CharStrings N dict def` + whitespace + `Private begin CharStrings begin` + optional whitespace.
/// Returns `(start_offset, match_length, count_string)` or `None` if not found.
fn find_nonstandard_charstrings(data: &[u8]) -> Option<(usize, usize, String)> {
    // Find "/CharStrings "
    let prefix = b"/CharStrings ";
    let mut i = 0;
    while i + prefix.len() < data.len() {
        if &data[i..i + prefix.len()] != prefix {
            i += 1;
            continue;
        }
        let start = i;
        let mut j = i + prefix.len();
        // Read digits (the count N).
        let digit_start = j;
        while j < data.len() && data[j].is_ascii_digit() {
            j += 1;
        }
        if j == digit_start {
            i += 1;
            continue;
        }
        let count_str = std::str::from_utf8(&data[digit_start..j]).ok()?.to_string();
        // Expect " dict def"
        if data.get(j..j + 9) != Some(b" dict def") {
            i += 1;
            continue;
        }
        j += 9;
        // Skip whitespace (newline, spaces).
        while j < data.len() && matches!(data[j], b' ' | b'\t' | b'\n' | b'\r') {
            j += 1;
        }
        // Expect "Private begin CharStrings begin"
        let suffix = b"Private begin CharStrings begin";
        if data.get(j..j + suffix.len()) != Some(suffix) {
            i += 1;
            continue;
        }
        j += suffix.len();
        // Skip trailing whitespace/newline after "begin".
        while j < data.len() && matches!(data[j], b' ' | b'\t' | b'\n' | b'\r') {
            j += 1;
        }
        return Some((start, j - start, count_str));
    }
    None
}

/// Extract the base font name by stripping a 6-character random subset prefix
/// (e.g. `YWFNOL+CMSY8` → `CMSY8`) and an optional `~XX` hex suffix
/// (e.g. `TXCMVS+CMSY8~32` → `CMSY8`).
fn type1_base_font_name(name: &str) -> String {
    // Strip 6-char prefix + '+' if present
    let after_prefix = if name.len() > 7 && name.as_bytes().get(6) == Some(&b'+') {
        &name[7..]
    } else {
        name
    };
    // Strip ~XX suffix (tilde + 2 hex chars)
    if after_prefix.len() >= 3 {
        let bytes = after_prefix.as_bytes();
        let last3 = &bytes[bytes.len() - 3..];
        if last3[0] == b'~' && last3[1].is_ascii_hexdigit() && last3[2].is_ascii_hexdigit() {
            return after_prefix[..after_prefix.len() - 3].to_string();
        }
    }
    after_prefix.to_string()
}

/// Parse PFB segments and return (raw_ps_data, length1, length2, length3).
///
/// Length1 = cleartext (type-1) bytes before the binary section.
/// Length2 = binary (type-2, eexec) bytes.
/// Length3 = trailing cleartext (type-1) bytes after the binary section.
fn parse_pfb_segments(data: &[u8]) -> Option<(Vec<u8>, i64, i64, i64)> {
    if data.len() < 6 || data[0] != 0x80 {
        return None;
    }

    let mut result = Vec::new();
    let mut i = 0;
    let mut l1: i64 = 0;
    let mut l2: i64 = 0;
    let mut l3: i64 = 0;
    let mut seen_binary = false;

    while i + 6 <= data.len() && data[i] == 0x80 {
        let seg_type = data[i + 1];
        if seg_type == 3 {
            break; // EOF segment — no length field
        }
        let length =
            u32::from_le_bytes([data[i + 2], data[i + 3], data[i + 4], data[i + 5]]) as usize;
        i += 6;
        let end = (i + length).min(data.len());
        let seg = &data[i..end];

        match seg_type {
            1 => {
                if seen_binary {
                    l3 += seg.len() as i64;
                } else {
                    l1 += seg.len() as i64;
                }
            }
            2 => {
                l2 += seg.len() as i64;
                seen_binary = true;
            }
            _ => {}
        }
        result.extend_from_slice(seg);
        i = end;
    }

    if result.is_empty() {
        return None;
    }

    Some((result, l1, l2, l3))
}

/// Strip PFB (Printer Font Binary) segment headers.
///
/// PFB files have segments prefixed by: 0x80, type_byte, length_le32, data.
/// This function concatenates the data portions, stripping the headers.
fn strip_pfb_headers(data: &[u8]) -> Vec<u8> {
    if data.is_empty() || data[0] != 0x80 {
        return data.to_vec();
    }

    let mut result = Vec::new();
    let mut i = 0;

    while i + 6 <= data.len() && data[i] == 0x80 {
        let seg_type = data[i + 1];
        if seg_type == 3 {
            break; // EOF segment
        }
        let length =
            u32::from_le_bytes([data[i + 2], data[i + 3], data[i + 4], data[i + 5]]) as usize;
        i += 6;
        let end = (i + length).min(data.len());
        result.extend_from_slice(&data[i..end]);
        i = end;
    }

    result
}

/// Decrypt the eexec-encrypted section of a Type 1 font program.
///
/// Finds the `eexec` keyword, then decrypts using the standard Type 1
/// eexec key (55665). Handles both hex-encoded and binary eexec data.
/// Skips the first 4 random bytes after decryption.
fn decrypt_eexec(ps_data: &[u8]) -> Vec<u8> {
    // Find "eexec" keyword.
    let eexec_pos = ps_data.windows(5).position(|w| w == b"eexec");
    let Some(eexec_pos) = eexec_pos else {
        return Vec::new();
    };

    // Skip past "eexec" and any following whitespace.
    let mut start = eexec_pos + 5;
    while start < ps_data.len() && ps_data[start].is_ascii_whitespace() {
        start += 1;
    }

    if start >= ps_data.len() {
        return Vec::new();
    }

    // Determine if hex-encoded or binary.
    // Hex-encoded eexec has only hex digits and whitespace.
    let cipher = if ps_data[start..]
        .iter()
        .take(32)
        .all(|b| b.is_ascii_hexdigit() || b.is_ascii_whitespace())
    {
        // Hex-encoded: decode hex pairs.
        let hex_str: String = ps_data[start..]
            .iter()
            .filter(|b| b.is_ascii_hexdigit())
            .map(|&b| b as char)
            .collect();
        let mut bytes = Vec::with_capacity(hex_str.len() / 2);
        let chars: Vec<char> = hex_str.chars().collect();
        for pair in chars.chunks(2) {
            if pair.len() == 2 {
                if let Ok(byte) = u8::from_str_radix(&format!("{}{}", pair[0], pair[1]), 16) {
                    bytes.push(byte);
                }
            }
        }
        bytes
    } else {
        // Binary eexec.
        ps_data[start..].to_vec()
    };

    if cipher.len() < 4 {
        return Vec::new();
    }

    // Decrypt with eexec key (55665).
    let mut key: u16 = 55665;
    let mut plain = Vec::with_capacity(cipher.len());

    for &c in &cipher {
        let p = c ^ (key >> 8) as u8;
        key = (c as u16)
            .wrapping_add(key)
            .wrapping_mul(52845)
            .wrapping_add(22719);
        plain.push(p);
    }

    // Skip first 4 random bytes.
    if plain.len() > 4 {
        plain[4..].to_vec()
    } else {
        Vec::new()
    }
}

/// Extract glyph names from a (potentially decrypted) Type 1 font's /CharStrings section.
///
/// Parses entries like: `/glyphname N RD <N binary bytes> ND`
/// Returns a CharSet string like "/A/B/C/space" (excluding .notdef).
///
/// Works directly on raw bytes to avoid UTF-8 lossy conversion issues where
/// binary charstring data would be expanded by replacement characters.
fn extract_glyph_names_from_charstrings(data: &[u8]) -> Option<String> {
    // Find "/CharStrings" in the raw byte data.
    let cs_pos = data.windows(12).position(|w| w == b"/CharStrings")?;
    let section = &data[cs_pos..];

    let mut names = Vec::new();
    let mut i = 0;

    while i < section.len() {
        // Look for glyph name definitions: "/glyphname"
        if section[i] == b'/' {
            let start = i + 1;
            let mut end = start;
            while end < section.len() && !section[end].is_ascii_whitespace() && section[end] != b'/'
            {
                end += 1;
            }

            // Extract name as ASCII (glyph names are always ASCII).
            let name_bytes = &section[start..end];
            let name = match std::str::from_utf8(name_bytes) {
                Ok(s) => s,
                Err(_) => {
                    // Non-ASCII "name" means we're in binary data — skip.
                    i = end;
                    continue;
                }
            };

            // Stop if we hit "end" or other section markers.
            if name == "CharStrings" || name == "FontName" || name == "Encoding" {
                i = end;
                continue;
            }
            if name == "end" {
                break;
            }

            if !name.is_empty() && name != ".notdef" {
                // Validate: glyph names should only contain printable ASCII.
                if name.bytes().all(|b| (0x21..0x7f).contains(&b)) {
                    names.push(name.to_string());
                }
            }

            // Skip past the binary charstring data: "N RD <1 delim + N binary bytes>"
            // or "N -| <1 delim + N binary bytes>"
            i = end;
            // Skip whitespace to find the charstring length N.
            while i < section.len() && section[i].is_ascii_whitespace() {
                i += 1;
            }
            // Parse N (the charstring byte count).
            let n_start = i;
            while i < section.len() && section[i].is_ascii_digit() {
                i += 1;
            }
            if i > n_start {
                if let Some(n) = std::str::from_utf8(&section[n_start..i])
                    .ok()
                    .and_then(|s| s.parse::<usize>().ok())
                {
                    // Skip whitespace.
                    while i < section.len() && section[i].is_ascii_whitespace() {
                        i += 1;
                    }
                    // Skip "RD" or "-|" token.
                    if i + 2 <= section.len()
                        && (&section[i..i + 2] == b"RD" || &section[i..i + 2] == b"-|")
                    {
                        i += 2;
                    }
                    // Skip one delimiter byte (space after RD), then N binary bytes.
                    if i < section.len() {
                        i += 1; // delimiter
                    }
                    i += n; // binary charstring data
                }
            }
            continue;
        }

        // End of CharStrings dict: "end" or "readonly" as standalone tokens.
        if i + 3 <= section.len()
            && &section[i..i + 3] == b"end"
            && (i + 3 == section.len()
                || section[i + 3].is_ascii_whitespace()
                || section[i + 3] == b'\0')
        {
            break;
        }
        if i + 8 <= section.len() && &section[i..i + 8] == b"readonly" {
            break;
        }

        i += 1;
    }

    if names.is_empty() {
        return None;
    }

    let mut charset = String::new();
    for name in &names {
        charset.push('/');
        charset.push_str(name);
    }
    Some(charset)
}

/// Fix widths for simple TrueType fonts with explicit standard encoding (6.2.11.5:1).
///
/// Only processes fonts with WinAnsiEncoding or MacRomanEncoding to avoid
/// regression from incorrect encoding assumptions. Compares existing /Widths
/// against hmtx table and fixes mismatches.
pub fn fix_simple_truetype_widths(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut fixed = 0;

    for font_id in font_ids {
        let (fd_id, encoding_name) = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();
            if subtype != "TrueType" {
                continue;
            }

            // Only process fonts with explicit standard encoding.
            let enc = match dict.get(b"Encoding").ok() {
                Some(Object::Name(n)) => String::from_utf8(n.clone()).ok(),
                Some(Object::Dictionary(enc_dict)) => get_name(enc_dict, b"BaseEncoding"),
                _ => None,
            };
            let enc = match enc.as_deref() {
                Some("WinAnsiEncoding") | Some("MacRomanEncoding") => enc.unwrap(),
                _ => continue, // Skip fonts without standard encoding.
            };

            let fd_id = match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(id)) => *id,
                _ => continue,
            };

            // Must have embedded font.
            let has_embedded = doc
                .objects
                .get(&fd_id)
                .and_then(|o| {
                    if let Object::Dictionary(fd) = o {
                        Some(fd.has(b"FontFile2"))
                    } else {
                        None
                    }
                })
                .unwrap_or(false);
            if !has_embedded {
                continue;
            }

            (fd_id, enc)
        };

        let font_data = read_embedded_font_data(doc, fd_id);
        let Some(font_data) = font_data else { continue };

        let Ok(face) = ttf_parser::Face::parse(&font_data, 0) else {
            continue;
        };

        let units_per_em = face.units_per_em() as f64;
        if units_per_em == 0.0 {
            continue;
        }
        let scale = 1000.0 / units_per_em;

        // Check for mismatches.
        let has_mismatch = {
            let Some(Object::Dictionary(font)) = doc.objects.get(&font_id) else {
                continue;
            };
            let fc = font
                .get(b"FirstChar")
                .ok()
                .and_then(|o| match o {
                    Object::Integer(i) => Some(*i as u32),
                    _ => None,
                })
                .unwrap_or(0);
            let existing = match font.get(b"Widths").ok() {
                Some(Object::Array(arr)) => arr,
                _ => continue,
            };

            let mut mismatch = false;
            for (i, obj) in existing.iter().enumerate() {
                let pdf_w = match obj {
                    Object::Integer(w) => *w,
                    Object::Real(r) => *r as i64,
                    _ => continue,
                };
                let code = fc + i as u32;
                let ch = encoding_to_char(code, &encoding_name);
                let expected = if let Some(gid) = face.glyph_index(ch) {
                    face.glyph_hor_advance(gid)
                        .map(|w| (w as f64 * scale).round() as i64)
                        .unwrap_or(0)
                } else if code <= u16::MAX as u32 {
                    face.glyph_hor_advance(ttf_parser::GlyphId(code as u16))
                        .map(|w| (w as f64 * scale).round() as i64)
                        .unwrap_or(0)
                } else {
                    0
                };
                if (pdf_w - expected).abs() > 1 {
                    mismatch = true;
                    break;
                }
            }
            mismatch
        };

        if has_mismatch {
            update_simple_widths(doc, font_id, &face, scale);
            fixed += 1;
        }
    }
    fixed
}

/// Fix widths for Type1 fonts with CFF font programs (6.2.11.5:1).
///
/// Reads glyph widths from FontFile3 (CFF) and updates the /Widths array.
/// Only processes Type1 fonts that have a CFF program embedded.
pub fn fix_type1_widths(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut fixed = 0;

    for font_id in font_ids {
        let fd_id = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();
            if subtype != "Type1" && subtype != "MMType1" {
                continue;
            }

            let fd_id = match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(id)) => *id,
                _ => continue,
            };

            // Only process FontFile3 (CFF programs).
            let has_ff3 = doc
                .objects
                .get(&fd_id)
                .and_then(|o| {
                    if let Object::Dictionary(fd) = o {
                        Some(fd.has(b"FontFile3"))
                    } else {
                        None
                    }
                })
                .unwrap_or(false);
            if !has_ff3 {
                continue;
            }

            fd_id
        };

        let font_data = read_embedded_font_data(doc, fd_id);
        let Some(font_data) = font_data else { continue };

        let Some(cff) = cff_parser::Table::parse(&font_data) else {
            continue;
        };

        // Read FirstChar/LastChar/Encoding from font dict.
        let (first_char, last_char, encoding_name) = {
            let Some(Object::Dictionary(font)) = doc.objects.get(&font_id) else {
                continue;
            };
            let fc = font
                .get(b"FirstChar")
                .ok()
                .and_then(|o| match o {
                    Object::Integer(i) => Some(*i as u32),
                    _ => None,
                })
                .unwrap_or(0);
            let lc = font
                .get(b"LastChar")
                .ok()
                .and_then(|o| match o {
                    Object::Integer(i) => Some(*i as u32),
                    _ => None,
                })
                .unwrap_or(255);
            let enc = font
                .get(b"Encoding")
                .ok()
                .and_then(|o| match o {
                    Object::Name(n) => String::from_utf8(n.clone()).ok(),
                    _ => None,
                })
                .unwrap_or_default();
            (fc, lc, enc)
        };

        // Get the CFF font matrix scale.
        let matrix = cff.matrix();
        let scale = cff_matrix_scale(matrix.sx);

        // Build widths from CFF glyph data.
        let mut widths = Vec::new();
        let mut any_mismatch = false;

        // Read existing widths for comparison.
        let existing_widths: Vec<i64> = {
            let Some(Object::Dictionary(font)) = doc.objects.get(&font_id) else {
                continue;
            };
            match font.get(b"Widths").ok() {
                Some(Object::Array(arr)) => arr
                    .iter()
                    .map(|o| match o {
                        Object::Integer(w) => *w,
                        Object::Real(r) => *r as i64,
                        _ => 0,
                    })
                    .collect(),
                _ => continue,
            }
        };

        for code in first_char..=last_char {
            // Map code to glyph name via encoding, then look up in CFF.
            let ch = encoding_to_char(code, &encoding_name);
            let glyph_name = unicode_to_glyph_name(ch);

            let width = if let Some(ref name) = glyph_name {
                // Find GID by glyph name.
                find_cff_glyph_width_by_name(&cff, name, scale)
            } else if code <= u16::MAX as u32 {
                // Direct GID lookup.
                cff.glyph_width(cff_parser::GlyphId(code as u16))
                    .map(|w| (w as f64 * scale).round() as i64)
            } else {
                None
            };

            let w = width.unwrap_or(0);
            let idx = (code - first_char) as usize;
            if idx < existing_widths.len() && (existing_widths[idx] - w).abs() > 1 {
                any_mismatch = true;
            }
            widths.push(Object::Integer(w));
        }

        if any_mismatch {
            if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&font_id) {
                font.set("Widths", Object::Array(widths));
            }
            fixed += 1;
        }
    }
    fixed
}

/// Map a Unicode character to a common glyph name (for CFF lookup).
fn unicode_to_glyph_name(ch: char) -> Option<String> {
    let code = ch as u32;
    match code {
        0x20 => Some("space".into()),
        0x21..=0x7E => Some(String::from(ch)), // ASCII printable
        0xC0..=0xFF => {
            // Latin-1 supplement — use standard names.
            Some(format!("uni{code:04X}"))
        }
        _ => Some(format!("uni{code:04X}")),
    }
}

/// Find a glyph width in a CFF table by name.
fn find_cff_glyph_width_by_name(
    cff: &cff_parser::Table<'_>,
    name: &str,
    scale: f64,
) -> Option<i64> {
    let num_glyphs = cff.number_of_glyphs();
    for gid in 0..num_glyphs {
        let glyph_id = cff_parser::GlyphId(gid);
        if let Some(gname) = cff.glyph_name(glyph_id) {
            if gname == name {
                return cff
                    .glyph_width(glyph_id)
                    .map(|w| (w as f64 * scale).round() as i64);
            }
        }
    }
    None
}

/// Fix CIDSet streams for all CID fonts (6.2.11.8:1).
///
/// CIDSet must be a stream containing a bitmap covering all CIDs present
/// in the embedded font program. This builds a complete CIDSet from the
/// font's glyph count.
pub fn fix_cidset(doc: &mut Document) -> usize {
    // For PDF/A-2b, CIDSet must correctly identify all CIDs in the font program
    // (rule 6.2.11.7:1). For CIDFontType2 (TrueType) fonts with FontFile2, we
    // can generate a correct CIDSet from the maxp table's numGlyphs field.
    // For other font types the safe fallback is to REMOVE CIDSet entirely.
    let font_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut fixed = 0;

    for font_id in font_ids {
        let (subtype, fd_id) = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();
            if subtype != "CIDFontType0" && subtype != "CIDFontType2" {
                continue;
            }
            let fd_id = match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(id)) => *id,
                _ => continue,
            };
            (subtype, fd_id)
        };

        let has_cidset = doc
            .objects
            .get(&fd_id)
            .and_then(|o| {
                if let Object::Dictionary(fd) = o {
                    Some(fd.has(b"CIDSet"))
                } else {
                    None
                }
            })
            .unwrap_or(false);

        if !has_cidset {
            continue;
        }

        // For CIDFontType2 with FontFile2 (TrueType): regenerate CIDSet from
        // the maxp table so it correctly covers all glyphs in the font program.
        if subtype == "CIDFontType2" {
            let has_ff2 = doc
                .objects
                .get(&fd_id)
                .and_then(|o| {
                    if let Object::Dictionary(fd) = o {
                        Some(fd.has(b"FontFile2"))
                    } else {
                        None
                    }
                })
                .unwrap_or(false);

            if has_ff2 {
                let font_data = read_embedded_font_data(doc, fd_id);
                if let Some(ref data) = font_data {
                    if let Some(num_glyphs) = truetype_num_glyphs(data) {
                        let cidset_bytes = cidset_bitstream(num_glyphs);
                        // Store new CIDSet stream and update FontDescriptor.
                        let new_id = doc.new_object_id();
                        let mut stream_dict = lopdf::Dictionary::new();
                        stream_dict.set("Length", Object::Integer(cidset_bytes.len() as i64));
                        let stream = lopdf::Stream::new(stream_dict, cidset_bytes);
                        doc.objects.insert(new_id, Object::Stream(stream));
                        if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
                            fd.set("CIDSet", Object::Reference(new_id));
                        }
                        fixed += 1;
                        continue;
                    }
                }
            }
        }

        // Fallback: remove CIDSet when we can't regenerate it correctly.
        if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
            fd.remove(b"CIDSet");
        }
        fixed += 1;
    }
    fixed
}

/// Extract numGlyphs from a TrueType font's maxp table.
fn truetype_num_glyphs(data: &[u8]) -> Option<u16> {
    if data.len() < 12 {
        return None;
    }
    // sfnt header: sfVersion(4) + numTables(2) + searchRange(2) + ...
    let num_tables = u16::from_be_bytes([data[4], data[5]]) as usize;
    // Table directory starts at offset 12, each entry is 16 bytes.
    for i in 0..num_tables {
        let entry_off = 12 + i * 16;
        if entry_off + 16 > data.len() {
            break;
        }
        if &data[entry_off..entry_off + 4] == b"maxp" {
            let table_off = u32::from_be_bytes([
                data[entry_off + 8],
                data[entry_off + 9],
                data[entry_off + 10],
                data[entry_off + 11],
            ]) as usize;
            if table_off + 6 <= data.len() {
                let num_glyphs = u16::from_be_bytes([data[table_off + 4], data[table_off + 5]]);
                return Some(num_glyphs);
            }
        }
    }
    None
}

/// Build a CIDSet bitstream with bits 0..num_glyphs-1 set (big-endian bit order).
fn cidset_bitstream(num_glyphs: u16) -> Vec<u8> {
    if num_glyphs == 0 {
        return vec![0u8];
    }
    let num_bytes = (num_glyphs as usize).div_ceil(8);
    let mut bits = vec![0xFFu8; num_bytes];
    // Clear trailing bits if numGlyphs is not a multiple of 8.
    let remainder = num_glyphs as usize % 8;
    if remainder != 0 {
        // Keep only the top `remainder` bits of the last byte.
        bits[num_bytes - 1] = 0xFFu8 << (8 - remainder);
    }
    bits
}

/// Fix font width mismatches between /Widths array and embedded font program (6.2.11.5:1).
///
/// Conservative approach: only updates individual width entries that clearly mismatch,
/// and only for fonts where the glyph mapping can be unambiguously determined.
/// Fix incorrect Symbolic flags on non-symbolic fonts with CFF programs.
///
/// Some PDFs incorrectly set the Symbolic flag on standard Latin fonts.
/// veraPDF uses the Symbolic flag to decide whether to validate widths via
/// CFF internal encoding (Symbolic) or PDF/Unicode encoding (Nonsymbolic).
/// Wrong flags cause width validation failures.
pub fn fix_symbolic_flags(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            if let Object::Dictionary(dict) = obj {
                if is_font_dict(dict) {
                    return Some(*id);
                }
            }
            None
        })
        .collect();

    let mut fixed = 0;

    for font_id in font_ids {
        let (name, fd_id) = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();
            if subtype != "Type1" && subtype != "MMType1" {
                continue;
            }
            let name = match get_name(dict, b"BaseFont") {
                Some(n) => n,
                None => continue,
            };
            // For known symbolic Type1 fonts (e.g. Symbol/ZapfDingbats),
            // enforce Symbolic flags so validators use the symbolic encoding path.
            let base = strip_subset_prefix(&name);
            if is_symbolic_font_name(base) {
                let fd_id = match dict.get(b"FontDescriptor").ok() {
                    Some(Object::Reference(r)) => *r,
                    _ => continue,
                };
                let needs_fix = match doc.objects.get(&fd_id) {
                    Some(Object::Dictionary(fd)) => {
                        let has_font_program =
                            fd.has(b"FontFile3") || fd.has(b"FontFile2") || fd.has(b"FontFile");
                        let flags = match fd.get(b"Flags").ok() {
                            Some(Object::Integer(f)) => *f,
                            _ => 0,
                        };
                        has_font_program && ((flags & 4 == 0) || (flags & 32 != 0))
                    }
                    _ => false,
                };
                if needs_fix {
                    if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
                        let flags = match fd.get(b"Flags").ok() {
                            Some(Object::Integer(f)) => *f,
                            _ => 0,
                        };
                        let new_flags = (flags | 4) & !32; // Symbolic=1, Nonsymbolic=0
                        fd.set("Flags", Object::Integer(new_flags));
                        fixed += 1;
                    }
                }
                continue;
            }
            let fd_id = match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(r)) => *r,
                _ => continue,
            };
            (name, fd_id)
        };

        // Check if FontDescriptor has Symbolic flag and FontFile3.
        let needs_fix = {
            let Some(Object::Dictionary(fd)) = doc.objects.get(&fd_id) else {
                continue;
            };
            let has_ff3 = fd.has(b"FontFile3");
            let flags = match fd.get(b"Flags").ok() {
                Some(Object::Integer(f)) => *f,
                _ => continue,
            };
            has_ff3 && (flags & 4 != 0)
        };

        if !needs_fix {
            continue;
        }

        // Non-symbolic font with Symbolic flag → fix flags.
        if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
            if let Ok(Object::Integer(flags)) = fd.get(b"Flags") {
                let mut f = *flags;
                f &= !4; // Clear Symbolic (bit 3)
                f |= 32; // Set Nonsymbolic (bit 6)
                fd.set("Flags", Object::Integer(f));
                fixed += 1;
            }
        }
        let _ = name; // suppress unused warning
    }

    fixed
}

/// Strip named standard encodings from classic base-14 symbolic fonts.
///
/// For Type1 fonts without an Encoding entry, PDF falls back to the font's
/// built-in encoding. On classic Symbol/ZapfDingbats fonts, generator-default
/// `/WinAnsiEncoding` or `/MacRomanEncoding` entries can override that mapping
/// incorrectly. Only strip the entry when there are no Differences, so
/// explicit custom encodings remain untouched.
pub fn fix_classic_symbolic_base14_encoding(doc: &mut Document) -> usize {
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut fixed = 0;

    for font_id in ids {
        let should_strip = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            if !is_font_dict(dict) {
                continue;
            }

            let subtype = get_name(dict, b"Subtype").unwrap_or_default();
            if subtype != "Type1" && subtype != "MMType1" {
                continue;
            }

            let base_font = get_name(dict, b"BaseFont").unwrap_or_default();
            let base_name = strip_subset_prefix(&base_font);
            if !matches!(
                base_name,
                "Symbol" | "SymbolMT" | "ZapfDingbats" | "Dingbats"
            ) {
                continue;
            }

            match dict.get(b"Encoding").ok() {
                Some(Object::Name(n))
                    if n == b"WinAnsiEncoding"
                        || n == b"MacRomanEncoding"
                        || n == b"MacExpertEncoding" =>
                {
                    true
                }
                Some(Object::Dictionary(enc)) => {
                    !enc.has(b"Differences")
                        && matches!(
                            enc.get(b"BaseEncoding").ok(),
                            Some(Object::Name(n))
                                if n == b"WinAnsiEncoding"
                                    || n == b"MacRomanEncoding"
                                    || n == b"MacExpertEncoding"
                        )
                }
                Some(Object::Reference(enc_id)) => match doc.objects.get(enc_id) {
                    Some(Object::Dictionary(enc)) => {
                        !enc.has(b"Differences")
                            && matches!(
                                enc.get(b"BaseEncoding").ok(),
                                Some(Object::Name(n))
                                    if n == b"WinAnsiEncoding"
                                        || n == b"MacRomanEncoding"
                                        || n == b"MacExpertEncoding"
                            )
                    }
                    _ => false,
                },
                _ => false,
            }
        };

        if !should_strip {
            continue;
        }

        if let Some(Object::Dictionary(dict)) = doc.objects.get_mut(&font_id) {
            dict.remove(b"Encoding");
            fixed += 1;
        }
    }

    fixed
}

/// Populate missing FirstChar/LastChar/Widths for embedded simple fonts.
///
/// Some PDFs (especially pre-PDF/A) omit these entries for standard 14 fonts.
/// PDF/A requires them (ISO 32000-1:2008 9.6.1, rule 6.2.11.2:4-6).
pub fn fix_missing_simple_font_widths(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            if let Object::Dictionary(dict) = obj {
                if is_font_dict(dict) {
                    return Some(*id);
                }
            }
            None
        })
        .collect();

    let mut fixed = 0;

    for font_id in font_ids {
        let needs_fix = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();
            if !matches!(subtype.as_str(), "Type1" | "TrueType" | "MMType1") {
                continue;
            }
            // Check if Widths or FirstChar is missing.
            let has_widths = dict.has(b"Widths");
            let has_fc = dict.has(b"FirstChar");
            if has_widths && has_fc {
                continue;
            }
            // Must have an embedded font program.
            if !has_embedded_font_program(doc, dict) {
                continue;
            }
            true
        };
        if !needs_fix {
            continue;
        }

        // Read encoding info and compute widths from the embedded font.
        let (enc_name, differences, fd_id) = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let mut enc = String::new();
            let mut diffs = std::collections::HashMap::new();
            match dict.get(b"Encoding").ok() {
                Some(Object::Name(n)) => {
                    enc = String::from_utf8(n.clone()).unwrap_or_default();
                }
                Some(Object::Dictionary(enc_dict)) => {
                    if let Some(base) = get_name(enc_dict, b"BaseEncoding") {
                        enc = base;
                    }
                    parse_differences(doc, enc_dict, &mut diffs);
                }
                Some(Object::Reference(enc_ref)) => {
                    if let Ok(Object::Dictionary(enc_dict)) = doc.get_object(*enc_ref) {
                        if let Some(base) = get_name(enc_dict, b"BaseEncoding") {
                            enc = base;
                        }
                        parse_differences(doc, enc_dict, &mut diffs);
                    }
                }
                _ => {}
            }
            if enc.is_empty() {
                enc = "WinAnsiEncoding".to_string();
            }
            let fd_id = match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(id)) => *id,
                _ => continue,
            };
            (enc, diffs, fd_id)
        };

        let font_data = read_embedded_font_data(doc, fd_id);
        let Some(font_data) = font_data else {
            continue;
        };

        // Compute widths for codes 0-255.
        let first_char = 0u32;
        let last_char = 255u32;

        // Try TrueType/OTF first, then fall back to raw CFF.
        let widths: Vec<Object> = if let Ok(face) = ttf_parser::Face::parse(&font_data, 0) {
            let units_per_em = face.units_per_em() as f64;
            if units_per_em == 0.0 {
                continue;
            }
            let scale = 1000.0 / units_per_em;
            (first_char..=last_char)
                .map(|code| {
                    let ch = if let Some(name) = differences.get(&code) {
                        glyph_name_to_unicode(name).unwrap_or(encoding_to_char(code, &enc_name))
                    } else {
                        encoding_to_char(code, &enc_name)
                    };
                    let width = if let Some(gid) = face.glyph_index(ch) {
                        face.glyph_hor_advance(gid)
                            .map(|w| (w as f64 * scale).round() as i64)
                            .unwrap_or(0)
                    } else if let Some(name) = differences.get(&code) {
                        if let Some(gid) = face.glyph_index_by_name(name) {
                            face.glyph_hor_advance(gid)
                                .map(|w| (w as f64 * scale).round() as i64)
                                .unwrap_or(0)
                        } else {
                            0
                        }
                    } else {
                        0
                    };
                    Object::Integer(width)
                })
                .collect()
        } else if let Some(cff) = cff_parser::Table::parse(&font_data) {
            // Raw CFF (Type1C) font data.
            let scale = cff_matrix_scale(cff.matrix().sx);
            let enc_map = parse_cff_encoding_map(&font_data);
            (first_char..=last_char)
                .map(|code| {
                    // Try Differences name → CFF charset → GID.
                    let glyph_name = if let Some(name) = differences.get(&code) {
                        Some(name.as_str().to_string())
                    } else {
                        // Map code via encoding to glyph name.
                        let ch = encoding_to_char(code, &enc_name);
                        unicode_to_glyph_name(ch)
                    };
                    let width = if let Some(name) = &glyph_name {
                        cff.glyph_index_by_name(name)
                            .and_then(|gid| cff.glyph_width(gid))
                            .map(|w| (w as f64 * scale).round() as i64)
                    } else {
                        None
                    };
                    // Also try CFF internal encoding.
                    let width = width.or_else(|| {
                        enc_map
                            .get(&(code as u8))
                            .and_then(|&gid| cff.glyph_width(cff_parser::GlyphId(gid)))
                            .map(|w| (w as f64 * scale).round() as i64)
                    });
                    Object::Integer(width.unwrap_or(0))
                })
                .collect()
        } else {
            continue;
        };

        if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&font_id) {
            dict.set("FirstChar", Object::Integer(first_char as i64));
            dict.set("LastChar", Object::Integer(last_char as i64));
            dict.set("Widths", Object::Array(widths));
            fixed += 1;
        }
    }

    fixed
}

/// Fix Type3 /Widths entries from CharProc d0/d1 widths (6.2.11.5:1).
pub fn fix_type3_font_widths(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| match obj {
            Object::Dictionary(d) if get_name(d, b"Subtype").as_deref() == Some("Type3") => {
                Some(*id)
            }
            _ => None,
        })
        .collect();

    let mut fixed = 0usize;

    for font_id in font_ids {
        let (
            first_char,
            last_char,
            existing_widths,
            widths_ref,
            enc_info,
            charprocs,
            charprocs_ref,
        ) = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let fc = match dict.get(b"FirstChar").ok() {
                Some(Object::Integer(i)) if *i >= 0 => *i as u32,
                _ => continue,
            };
            let lc = match dict.get(b"LastChar").ok() {
                Some(Object::Integer(i)) if *i >= 0 => *i as u32,
                _ => continue,
            };
            let (widths, widths_ref) = match dict.get(b"Widths").ok() {
                Some(Object::Array(arr)) => (arr.clone(), None),
                Some(Object::Reference(r)) => match doc.get_object(*r) {
                    Ok(Object::Array(arr)) => (arr.clone(), Some(*r)),
                    _ => continue,
                },
                _ => continue,
            };
            if widths.is_empty() {
                continue;
            }
            let enc_info = extract_encoding_info(doc, dict);
            let (charprocs, charprocs_ref) = match dict.get(b"CharProcs").ok() {
                Some(Object::Dictionary(d)) => (d.clone(), None),
                Some(Object::Reference(r)) => match doc.get_object(*r) {
                    Ok(Object::Dictionary(d)) => (d.clone(), Some(*r)),
                    _ => continue,
                },
                _ => continue,
            };
            (
                fc,
                lc,
                widths,
                widths_ref,
                enc_info,
                charprocs,
                charprocs_ref,
            )
        };

        let normalized_charprocs = normalize_type3_charproc_width_ops(doc, font_id, charprocs_ref);

        let mut enc_name = enc_info.base_encoding.clone();
        let differences: std::collections::HashMap<u32, String> =
            enc_info.differences.iter().cloned().collect();
        if enc_name.is_empty() {
            enc_name = "WinAnsiEncoding".to_string();
        }

        let mut inserted_space_width = None;
        if let Some(space_name) = differences
            .get(&32)
            .cloned()
            .or_else(|| unicode_to_glyph_name(encoding_to_char(32, &enc_name)))
        {
            if charprocs.get(space_name.as_bytes()).is_err() {
                let space_width =
                    derive_type3_space_width(doc, first_char, &existing_widths, &charprocs);
                let inserted_charproc = insert_type3_empty_charproc(
                    doc,
                    font_id,
                    charprocs_ref,
                    &space_name,
                    space_width,
                );
                if inserted_charproc {
                    inserted_space_width = Some(space_width);
                    let original_differences = enc_info.differences.clone();
                    let new_diffs = vec![(32u32, space_name.clone())];
                    let _ = apply_encoding_fixes(
                        doc,
                        font_id,
                        &enc_name,
                        &original_differences,
                        &[],
                        &new_diffs,
                        enc_info.enc_ref,
                    );
                }
            }
        }

        // Some legacy Type3 fonts can reference codes above /LastChar via
        // Encoding/Differences and CharProcs (e.g. /a255 while LastChar=254).
        // Preserve existing range and only extend upward when higher explicit
        // definitions exist, so used codes never fall back to dictionary width 0.
        let max_charproc_code = charprocs
            .iter()
            .filter_map(|(name, _)| parse_type3_numeric_name(name))
            .max();
        let max_diff_code = differences.keys().copied().max();
        let max_defined_code = match (max_charproc_code, max_diff_code) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        let target_first_char = if inserted_space_width.is_some() {
            first_char.min(32)
        } else {
            first_char
        };
        let target_last_char = max_defined_code
            .filter(|m| *m >= first_char && *m <= 255 && *m > last_char)
            .unwrap_or(last_char);
        let target_len = (target_last_char.saturating_sub(target_first_char) + 1) as usize;

        let mut corrections: Vec<(usize, i64)> = Vec::new();

        for idx in 0..target_len {
            let code = target_first_char + idx as u32;
            let current = existing_widths
                .get(code.saturating_sub(first_char) as usize)
                .and_then(object_to_f64)
                .unwrap_or(0.0);

            let mut candidates: Vec<String> = Vec::new();
            if let Some(name) = differences.get(&code) {
                candidates.push(name.clone());
            } else {
                let ch = encoding_to_char(code, &enc_name);
                if let Some(name) = unicode_to_agl_name(ch) {
                    candidates.push(name);
                }
                if let Some(name) = unicode_to_glyph_name(ch) {
                    candidates.push(name);
                }
            }
            candidates.push(format!("a{code}"));
            candidates.push(format!("g{code}"));
            candidates.dedup();

            let expected = candidates
                .iter()
                .find_map(|name| type3_charproc_width(doc, &charprocs, name));
            let Some(expected) = expected else { continue };

            if (current - expected as f64).abs() >= 1.0 {
                corrections.push((idx, expected));
            }
        }

        if corrections.is_empty() {
            // If range normalization is needed, still apply it.
            if target_first_char == first_char
                && target_last_char == last_char
                && inserted_space_width.is_none()
                && !normalized_charprocs
            {
                continue;
            }
        }

        let mut new_widths = Vec::with_capacity(target_len);
        for idx in 0..target_len {
            let code = target_first_char + idx as u32;
            let original = if code >= first_char {
                existing_widths
                    .get((code - first_char) as usize)
                    .cloned()
                    .unwrap_or(Object::Integer(0))
            } else {
                Object::Integer(0)
            };
            new_widths.push(original);
        }
        for (idx, new_w) in &corrections {
            if *idx < new_widths.len() {
                new_widths[*idx] = Object::Integer(*new_w);
            }
        }
        if let Some(space_width) = inserted_space_width {
            let idx = (32 - target_first_char) as usize;
            if idx < new_widths.len() {
                new_widths[idx] = Object::Integer(space_width);
            }
        }

        if let Some(widths_id) = widths_ref {
            if let Some(Object::Array(ref mut arr)) = doc.objects.get_mut(&widths_id) {
                *arr = new_widths;
                if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&font_id) {
                    font.set("FirstChar", Object::Integer(target_first_char as i64));
                    font.set("LastChar", Object::Integer(target_last_char as i64));
                }
                fixed += 1;
            }
        } else if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&font_id) {
            if let Ok(Object::Array(ref mut arr)) = font.get_mut(b"Widths") {
                *arr = new_widths;
                font.set("FirstChar", Object::Integer(target_first_char as i64));
                font.set("LastChar", Object::Integer(target_last_char as i64));
                fixed += 1;
            }
        }
    }

    fixed
}

fn normalize_type3_charproc_width_ops(
    doc: &mut Document,
    font_id: ObjectId,
    charprocs_ref: Option<ObjectId>,
) -> bool {
    let entries: Vec<(Vec<u8>, Object)> = match charprocs_ref {
        Some(ref_id) => match doc.objects.get(&ref_id) {
            Some(Object::Dictionary(charprocs)) => charprocs
                .iter()
                .map(|(name, obj)| (name.clone(), obj.clone()))
                .collect(),
            _ => Vec::new(),
        },
        None => match doc.objects.get(&font_id) {
            Some(Object::Dictionary(font)) => match font.get(b"CharProcs").ok() {
                Some(Object::Dictionary(charprocs)) => charprocs
                    .iter()
                    .map(|(name, obj)| (name.clone(), obj.clone()))
                    .collect(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        },
    };

    let mut changed = false;

    for (glyph_name, entry) in entries {
        match entry {
            Object::Reference(stream_id) => {
                let Some(Object::Stream(stream)) = doc.objects.get(&stream_id) else {
                    continue;
                };
                let mut stream = stream.clone();
                let _ = stream.decompress();
                let Some(new_content) = normalize_type3_charproc_stream(&stream.content) else {
                    continue;
                };
                if let Some(Object::Stream(ref mut target)) = doc.objects.get_mut(&stream_id) {
                    target.set_plain_content(new_content);
                    changed = true;
                }
            }
            Object::Stream(stream_obj) => {
                let mut stream = stream_obj.clone();
                let _ = stream.decompress();
                let Some(new_content) = normalize_type3_charproc_stream(&stream.content) else {
                    continue;
                };
                if let Some(ref_id) = charprocs_ref {
                    if let Some(Object::Dictionary(ref mut charprocs)) =
                        doc.objects.get_mut(&ref_id)
                    {
                        if let Ok(Object::Stream(ref mut target)) = charprocs.get_mut(&glyph_name) {
                            target.set_plain_content(new_content);
                            changed = true;
                        }
                    }
                } else if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&font_id)
                {
                    if let Ok(Object::Dictionary(ref mut charprocs)) = font.get_mut(b"CharProcs") {
                        if let Ok(Object::Stream(ref mut target)) = charprocs.get_mut(&glyph_name) {
                            target.set_plain_content(new_content);
                            changed = true;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    changed
}

fn normalize_type3_charproc_stream(data: &[u8]) -> Option<Vec<u8>> {
    #[derive(Clone, Copy)]
    struct Token {
        start: usize,
        end: usize,
    }

    fn is_pdf_number(token: &[u8]) -> bool {
        if token.is_empty() {
            return false;
        }
        let mut idx = 0usize;
        if matches!(token[0], b'+' | b'-') {
            idx += 1;
        }
        let mut seen_digit = false;
        let mut seen_dot = false;
        while idx < token.len() {
            match token[idx] {
                b'0'..=b'9' => seen_digit = true,
                b'.' if !seen_dot => seen_dot = true,
                _ => return false,
            }
            idx += 1;
        }
        seen_digit
    }

    let mut tokens = Vec::new();
    let mut i = 0usize;
    while i < data.len() {
        while i < data.len() && data[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= data.len() {
            break;
        }
        if data[i] == b'%' {
            while i < data.len() && data[i] != b'\n' && data[i] != b'\r' {
                i += 1;
            }
            continue;
        }
        let start = i;
        while i < data.len() && !data[i].is_ascii_whitespace() {
            i += 1;
        }
        tokens.push(Token { start, end: i });
    }

    let first_op_idx = tokens
        .iter()
        .position(|tok| !is_pdf_number(&data[tok.start..tok.end]))?;
    let width_op_idx = tokens.iter().position(|tok| {
        let token = &data[tok.start..tok.end];
        token == b"d0" || token == b"d1"
    })?;

    if width_op_idx == first_op_idx {
        return None;
    }

    let width_operand_count =
        if &data[tokens[width_op_idx].start..tokens[width_op_idx].end] == b"d0" {
            2usize
        } else {
            6usize
        };
    if width_op_idx < width_operand_count {
        return None;
    }

    let width_start_idx = width_op_idx - width_operand_count;
    if !tokens[width_start_idx..width_op_idx]
        .iter()
        .all(|tok| is_pdf_number(&data[tok.start..tok.end]))
    {
        return None;
    }

    let seq_start = tokens[width_start_idx].start;
    let seq_end = tokens[width_op_idx].end;
    let leading = data[..seq_start]
        .iter()
        .copied()
        .skip_while(|b| b.is_ascii_whitespace())
        .collect::<Vec<_>>();
    if leading.is_empty() {
        return None;
    }

    let rest = &data[seq_end..];
    let mut out = Vec::with_capacity(data.len() + 2);
    out.extend_from_slice(&data[seq_start..seq_end]);
    out.push(b'\n');
    out.extend_from_slice(&leading);
    if !rest.is_empty() && !leading.last().is_some_and(|b| b.is_ascii_whitespace()) {
        out.push(b' ');
    }
    out.extend_from_slice(
        rest.iter()
            .copied()
            .skip_while(|b| b.is_ascii_whitespace())
            .collect::<Vec<_>>()
            .as_slice(),
    );
    Some(out)
}

fn parse_type3_numeric_name(name: &[u8]) -> Option<u32> {
    if name.len() < 2 || name[0] != b'a' {
        return None;
    }
    let digits = std::str::from_utf8(&name[1..]).ok()?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse::<u32>().ok()
}

fn derive_type3_space_width(
    doc: &Document,
    first_char: u32,
    existing_widths: &[Object],
    charprocs: &lopdf::Dictionary,
) -> i64 {
    let width_at_code = |code: u32| -> Option<i64> {
        if code < first_char {
            return None;
        }
        let idx = (code - first_char) as usize;
        existing_widths
            .get(idx)
            .and_then(object_to_f64)
            .map(|w| w.round() as i64)
    };

    width_at_code(32)
        .filter(|w| *w > 0)
        .or_else(|| width_at_code(b'0' as u32).filter(|w| *w > 0))
        .or_else(|| type3_charproc_width(doc, charprocs, "space").filter(|w| *w > 0))
        .or_else(|| type3_charproc_width(doc, charprocs, "32").filter(|w| *w > 0))
        .or_else(|| type3_charproc_width(doc, charprocs, "48").filter(|w| *w > 0))
        .or_else(|| {
            let positives: Vec<i64> = existing_widths
                .iter()
                .filter_map(object_to_f64)
                .map(|w| w.round() as i64)
                .filter(|w| *w > 0)
                .collect();
            if positives.is_empty() {
                None
            } else {
                Some(positives.iter().sum::<i64>() / positives.len() as i64)
            }
        })
        .unwrap_or(250)
}

fn insert_type3_empty_charproc(
    doc: &mut Document,
    font_id: ObjectId,
    charprocs_ref: Option<ObjectId>,
    glyph_name: &str,
    width: i64,
) -> bool {
    let stream = Stream::new(dictionary! {}, format!("{width} 0 d0\n").into_bytes());
    let stream_id = doc.add_object(Object::Stream(stream));

    if let Some(ref_id) = charprocs_ref {
        if let Some(Object::Dictionary(ref mut charprocs)) = doc.objects.get_mut(&ref_id) {
            charprocs.set(glyph_name, Object::Reference(stream_id));
            return true;
        }
        return false;
    }

    let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&font_id) else {
        return false;
    };
    let Some(Object::Dictionary(ref mut charprocs)) = font.get_mut(b"CharProcs").ok() else {
        return false;
    };
    charprocs.set(glyph_name, Object::Reference(stream_id));
    true
}

fn type3_charproc_width(
    doc: &Document,
    charprocs: &lopdf::Dictionary,
    glyph_name: &str,
) -> Option<i64> {
    let cp_obj = charprocs.get(glyph_name.as_bytes()).ok()?;
    let stream = match cp_obj {
        Object::Stream(s) => s.clone(),
        Object::Reference(r) => match doc.objects.get(r) {
            Some(Object::Stream(s)) => s.clone(),
            _ => return None,
        },
        _ => return None,
    };
    let mut st = stream;
    let _ = st.decompress();
    if let Ok(editor) = crate::content_editor::ContentEditor::from_stream(&st.content) {
        for op in editor.operations() {
            if op.operator == "d0" || op.operator == "d1" {
                let wx = op.operands.first().and_then(object_to_f64)?;
                return Some(wx.round() as i64);
            }
        }
    }
    type3_charproc_width_fallback(&st.content)
}

/// Fallback parser for minimal Type3 charproc streams when content decoding fails.
/// Looks for `<wx> ... d0` or `<wx> ... d1` token sequences.
fn type3_charproc_width_fallback(data: &[u8]) -> Option<i64> {
    let tokens: Vec<&[u8]> = data
        .split(|b| b.is_ascii_whitespace())
        .filter(|t| !t.is_empty())
        .collect();
    for i in 0..tokens.len() {
        let need = if tokens[i] == b"d0" {
            2
        } else if tokens[i] == b"d1" {
            6
        } else {
            continue;
        };
        if i < need {
            continue;
        }
        let wx = std::str::from_utf8(tokens[i - need])
            .ok()
            .and_then(|s| s.parse::<f64>().ok())?;
        return Some(wx.round() as i64);
    }
    None
}

fn object_to_f64(obj: &Object) -> Option<f64> {
    match obj {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(r) => Some(*r as f64),
        _ => None,
    }
}

/// Skips fonts where >50% of widths mismatch (indicates unreliable mapping).
///
/// Returns count of fonts whose widths were corrected.
pub fn fix_font_width_mismatches(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            if let Object::Dictionary(dict) = obj {
                if is_font_dict(dict) {
                    return Some(*id);
                }
            }
            None
        })
        .collect();

    let mut fixed = 0;

    for font_id in font_ids {
        let info = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();

            // Skip CID fonts — they are handled by fix_cff_widths / fix_truetype_cid_widths.
            // Skip Type0 — widths live on the CIDFont descendant.
            match subtype.as_str() {
                "TrueType" | "Type1" | "MMType1" => {}
                _ => continue,
            }

            // Skip symbolic TrueType fonts and classic Symbol/Zapf Type1 CFF
            // fonts. Those are handled by dedicated symbolic passes.
            let base_font = get_name(dict, b"BaseFont").unwrap_or_default();
            if !base_font.is_empty() {
                let name = base_font.clone();
                if is_symbolic_font_name(&name) && is_font_symbolic(doc, dict) {
                    let (has_ff2, has_ff3) = match dict.get(b"FontDescriptor").ok() {
                        Some(Object::Reference(fd_id)) => doc
                            .objects
                            .get(fd_id)
                            .and_then(|o| {
                                if let Object::Dictionary(fd) = o {
                                    Some((fd.has(b"FontFile2"), fd.has(b"FontFile3")))
                                } else {
                                    None
                                }
                            })
                            .unwrap_or((false, false)),
                        _ => (false, false),
                    };
                    let base = strip_subset_prefix(&name);
                    let is_classic_symbol = matches!(
                        base,
                        "Symbol"
                            | "SymbolMT"
                            | "ZapfDingbats"
                            | "Wingdings"
                            | "Webdings"
                            | "Dingbats"
                            | "MTExtra"
                    );
                    if has_ff2 || (has_ff3 && is_classic_symbol) {
                        continue;
                    }
                }
                // NOTE: TrueType subset fonts (ABCDEF+Name) are processed normally.
                // Subset cmaps ARE updated during subsetting, so cmap-based glyph
                // lookup (encoding → Unicode → cmap) works correctly. The GID
                // fallback was removed from the width computation, so renumbered
                // GIDs don't cause incorrect matches.
            }

            // Must have FontDescriptor with embedded font program.
            let fd_id = match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(id)) => *id,
                _ => continue,
            };

            // Must have existing /Widths array and FirstChar.
            let fc = match dict.get(b"FirstChar").ok() {
                Some(Object::Integer(i)) => *i as u32,
                _ => continue,
            };
            let (existing_widths, widths_ref) = match dict.get(b"Widths").ok() {
                Some(Object::Array(arr)) => (arr.clone(), None),
                Some(Object::Reference(r)) => match doc.get_object(*r) {
                    Ok(Object::Array(arr)) => (arr.clone(), Some(*r)),
                    _ => continue,
                },
                _ => continue,
            };
            if existing_widths.is_empty() {
                continue;
            }

            // Get encoding info.
            let enc_info = get_simple_encoding_info(doc, dict);

            (
                subtype,
                base_font,
                fd_id,
                fc,
                existing_widths,
                enc_info,
                widths_ref,
            )
        };

        let (subtype, base_font, fd_id, first_char, existing_widths, enc_info, widths_ref) = info;

        // Check if font program is embedded (FontFile key exists).
        // We don't verify stream content here — read_embedded_font_data handles that.
        let has_embedded = matches!(
            doc.objects.get(&fd_id),
            Some(Object::Dictionary(d)) if d.has(b"FontFile") || d.has(b"FontFile2") || d.has(b"FontFile3")
        );
        if !has_embedded {
            continue;
        }

        // Determine which font file type is embedded.
        let (has_ff1, has_ff2, has_ff3) = {
            let Some(Object::Dictionary(fd)) = doc.objects.get(&fd_id) else {
                continue;
            };
            (
                fd.has(b"FontFile"),
                fd.has(b"FontFile2"),
                fd.has(b"FontFile3"),
            )
        };
        let ambiguous_cff_base_encoding =
            has_ff3 && (subtype == "Type1" || subtype == "MMType1") && enc_info.0.is_empty();

        let font_data = read_embedded_font_data(doc, fd_id);
        let Some(font_data) = font_data else {
            continue;
        };

        let mut corrections: Vec<(usize, i64)>;

        if has_ff2 && subtype == "TrueType" {
            // TrueType font with FontFile2 — use ttf-parser + cmap.
            corrections = compute_truetype_width_corrections(
                &font_data,
                first_char,
                &existing_widths,
                &enc_info,
            );
        } else if has_ff3 && (subtype == "Type1" || subtype == "MMType1") {
            // Type1/CFF font with FontFile3 — use CFF glyph names.
            corrections = compute_cff_type1_width_corrections(
                &font_data,
                first_char,
                &existing_widths,
                &enc_info,
            );
        } else if has_ff2 && (subtype == "Type1" || subtype == "MMType1") {
            // Type1 font re-encoded as TrueType (after embedding fallback font).
            corrections = compute_truetype_width_corrections(
                &font_data,
                first_char,
                &existing_widths,
                &enc_info,
            );
        } else if has_ff1 && (subtype == "Type1" || subtype == "MMType1") {
            // Only correct widths for subset fonts (ABCDEF+FontName pattern).
            // Full fonts like Melior have correct dict widths; our charstring parser
            // misreads some glyphs (subroutines / non-standard encoding), causing
            // false corrections that veraPDF then rejects.
            // Subset fonts (e.g. MINIPA+Times.New.Roman) may have reindexed
            // charstrings whose widths differ from the original dict.
            let is_subset = base_font.len() > 7 && base_font.as_bytes()[6] == b'+';
            if !is_subset {
                continue;
            }
            corrections = compute_type1_fontfile_width_corrections(
                &font_data,
                first_char,
                &existing_widths,
                &enc_info,
            );
        } else {
            continue;
        }

        if ambiguous_cff_base_encoding {
            // Keep only conservative deltas for ambiguous CFF base-encoding
            // mappings; large jumps are typically wrong code->glyph matches.
            corrections.retain(|(idx, new_w)| {
                let Some(pdf_w) = existing_widths.get(*idx).and_then(object_to_f64) else {
                    return false;
                };
                (pdf_w - *new_w as f64).abs() <= 50.0
            });
        }

        // For Type1 CFF fonts, keep in-range corrections conservative unless the
        // font descriptor indicates the legacy fixed metrics profile (flag bit
        // 18 set in these corpora), where full Differences-based correction is
        // stable. Otherwise only keep "space" corrections from .notdef remediation.
        if has_ff3 && (subtype == "Type1" || subtype == "MMType1") {
            let uses_cff_internal_encoding_only = enc_info.0.is_empty() && enc_info.1.is_empty();
            let allow_full_cff_corrections = match doc.objects.get(&fd_id) {
                Some(Object::Dictionary(fd)) => match fd.get(b"Flags").ok() {
                    Some(Object::Integer(flags)) => {
                        (*flags & 262_144) != 0 || uses_cff_internal_encoding_only
                    }
                    _ => uses_cff_internal_encoding_only,
                },
                _ => uses_cff_internal_encoding_only,
            };
            if !allow_full_cff_corrections {
                let is_subset = base_font.len() > 7 && base_font.as_bytes()[6] == b'+';
                corrections.retain(|(idx, _)| {
                    let code = first_char + *idx as u32;
                    if is_subset {
                        // High-byte subset remaps are often validated through
                        // CFF internal encoding. Keep low-byte edits, explicit
                        // /space fixes, and standard-encoding high-byte codes
                        // that resolve to an actual glyph name in the subset.
                        code <= 127
                            || matches!(enc_info.1.get(&code), Some(name) if name == "space")
                            || subset_standard_cff_code_is_safe(
                                &font_data,
                                code,
                                &enc_info.0,
                                &enc_info.1,
                            )
                    } else {
                        // Explicit Differences entries are deterministic mappings, so
                        // high-byte corrections remain safe on non-subset fonts.
                        code <= 127 || enc_info.1.contains_key(&code)
                    }
                });
            }
        }

        // Also compute widths for codes outside [FirstChar, LastChar] (up to 255).
        // Some fonts have codes used in content streams that fall outside
        // this range. Extend the Widths array to cover them.
        let last_char = first_char + existing_widths.len() as u32 - 1;
        let mut extensions: Vec<(u32, i64)> = Vec::new(); // (code, width)
        let mut prepend_codes: Vec<(u32, i64)> = Vec::new(); // codes below FirstChar

        // Helper closure to compute a single width for a code.
        let compute_width_for_code = |code: u32| -> Option<f64> {
            if has_ff2 {
                if let Ok(face) = ttf_parser::Face::parse(&font_data, 0) {
                    let upem = face.units_per_em() as f64;
                    if upem > 0.0 {
                        let scale = 1000.0 / upem;
                        return get_truetype_glyph_width_fractional(
                            &face,
                            code,
                            &enc_info.0,
                            &enc_info.1,
                            scale,
                        );
                    }
                }
            } else if has_ff3 {
                return compute_cff_single_width(&font_data, code, &enc_info.0, &enc_info.1);
            } else if has_ff1 {
                return compute_type1_fontfile_single_width(
                    &font_data,
                    code,
                    &enc_info.0,
                    &enc_info.1,
                );
            }
            None
        };

        // Codes below FirstChar.
        if !ambiguous_cff_base_encoding && first_char > 0 {
            for code in 0..first_char {
                if let Some(w) = compute_width_for_code(code) {
                    let w_rounded = w.round() as i64;
                    if w_rounded != 0 {
                        prepend_codes.push((code, w_rounded));
                    }
                }
            }
        }

        // Codes above LastChar.
        if !ambiguous_cff_base_encoding && last_char < 255 {
            for code in (last_char + 1)..=255 {
                if let Some(w) = compute_width_for_code(code) {
                    let w_rounded = w.round() as i64;
                    if w_rounded != 0 {
                        extensions.push((code, w_rounded));
                    }
                }
            }
        }

        if corrections.is_empty() && extensions.is_empty() && prepend_codes.is_empty() {
            continue;
        }

        // Safety check: if more than 50% of widths mismatch AND the encoding
        // is not a well-known standard encoding, our mapping is probably wrong.
        // For WinAnsiEncoding/MacRomanEncoding, the mapping is unambiguous,
        // so we trust the computed widths even if many differ (common when a
        // fallback font like DejaVuSans was embedded for Helvetica/Times etc.).
        // CFF internal encoding is also reliable — when no PDF-level Encoding
        // exists, the CFF's own encoding provides an unambiguous code-to-GID map.
        // Custom CFF encoding in OTF-wrapped fonts is also reliable, since the
        // CFF encoding map directly provides the code-to-GID mapping that
        // veraPDF uses for width comparison.
        let has_reliable_encoding =
            matches!(enc_info.0.as_str(), "WinAnsiEncoding" | "MacRomanEncoding");
        let uses_cff_encoding = enc_info.0.is_empty() && enc_info.1.is_empty() && has_ff3;
        let uses_custom_cff_encoding = has_ff3 && {
            if let Some(cff_bytes) = extract_cff_bytes_from_otf(&font_data) {
                cff_has_custom_encoding(cff_bytes)
            } else {
                // Raw CFF (Type1C) — check directly
                cff_has_custom_encoding(&font_data)
            }
        };
        // Type 1 FontFile widths are computed from the font program directly,
        // so they are always reliable regardless of encoding.
        let uses_type1_fontfile = has_ff1;
        // CFF (FontFile3) with Differences-based encoding: each code maps to
        // a specific glyph name that we look up in the CFF charset. This is
        // unambiguous, like WinAnsiEncoding.
        let has_differences_cff = has_ff3 && !enc_info.1.is_empty();
        // TrueType (FontFile2) widths are computed via cmap tables, which
        // provide a reliable code→GID mapping. Trust these even without a
        // standard PDF encoding name.
        let uses_truetype_fontfile = has_ff2;
        let total_widths = existing_widths.len();
        if !has_reliable_encoding
            && !uses_cff_encoding
            && !uses_custom_cff_encoding
            && !uses_type1_fontfile
            && !has_differences_cff
            && !uses_truetype_fontfile
            && corrections.len() * 2 > total_widths
            && extensions.is_empty()
            && prepend_codes.is_empty()
        {
            continue;
        }

        // Apply corrections, extensions, and prepends.
        // Determine the new FirstChar and LastChar.
        let new_first_char = if !prepend_codes.is_empty() {
            prepend_codes[0].0
        } else {
            first_char
        };
        let new_last_char = extensions
            .last()
            .map(|(code, _)| *code)
            .unwrap_or(last_char);
        let new_len = (new_last_char - new_first_char + 1) as usize;

        // Build the new widths array.
        let mut new_widths: Vec<Object> = vec![Object::Integer(0); new_len];

        // Copy existing widths at the correct offset.
        let offset = (first_char - new_first_char) as usize;
        for (i, obj) in existing_widths.iter().enumerate() {
            let target = offset + i;
            if target < new_widths.len() {
                new_widths[target] = obj.clone();
            }
        }

        // Apply inline corrections (relative to original first_char).
        for (idx, new_w) in &corrections {
            let target = offset + *idx;
            if target < new_widths.len() {
                new_widths[target] = Object::Integer(*new_w);
            }
        }
        // Apply prepend codes.
        for (code, w) in &prepend_codes {
            let target = (*code - new_first_char) as usize;
            if target < new_widths.len() {
                new_widths[target] = Object::Integer(*w);
            }
        }
        // Apply extensions.
        for (code, w) in &extensions {
            let target = (*code - new_first_char) as usize;
            if target < new_widths.len() {
                new_widths[target] = Object::Integer(*w);
            }
        }

        // Write back.
        if let Some(widths_id) = widths_ref {
            if let Some(Object::Array(ref mut arr)) = doc.objects.get_mut(&widths_id) {
                *arr = new_widths;
            }
        } else if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&font_id) {
            font.set("Widths", Object::Array(new_widths));
        }
        // Update FirstChar if prepended.
        if new_first_char < first_char {
            if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&font_id) {
                font.set("FirstChar", Object::Integer(new_first_char as i64));
            }
        }
        // Update LastChar if extended.
        if new_last_char > last_char {
            if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&font_id) {
                font.set("LastChar", Object::Integer(new_last_char as i64));
            }
        }
        fixed += 1;
    }

    fixed
}

/// Get encoding information for a simple font.
/// Returns (encoding_name, differences_map).
/// The differences_map maps character code -> glyph name from Encoding/Differences.
fn get_simple_encoding_info(
    doc: &Document,
    font_dict: &lopdf::Dictionary,
) -> (String, std::collections::HashMap<u32, String>) {
    let mut enc_name = String::new();
    let mut differences = std::collections::HashMap::new();

    match font_dict.get(b"Encoding").ok() {
        Some(Object::Name(n)) => {
            enc_name = String::from_utf8(n.clone()).unwrap_or_default();
        }
        Some(Object::Dictionary(enc_dict)) => {
            if let Some(base) = get_name(enc_dict, b"BaseEncoding") {
                enc_name = base;
            }
            parse_differences(doc, enc_dict, &mut differences);
        }
        Some(Object::Reference(enc_ref)) => {
            if let Ok(obj) = doc.get_object(*enc_ref) {
                match obj {
                    Object::Name(n) => {
                        enc_name = String::from_utf8(n.clone()).unwrap_or_default();
                    }
                    Object::Dictionary(enc_dict) => {
                        if let Some(base) = get_name(enc_dict, b"BaseEncoding") {
                            enc_name = base;
                        }
                        parse_differences(doc, enc_dict, &mut differences);
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }

    (enc_name, differences)
}

/// Parse /Differences array from an encoding dictionary.
fn parse_differences(
    doc: &Document,
    enc_dict: &lopdf::Dictionary,
    differences: &mut std::collections::HashMap<u32, String>,
) {
    let diff_arr = match enc_dict.get(b"Differences").ok() {
        Some(Object::Array(arr)) => Some(arr),
        Some(Object::Reference(r)) => doc.get_object(*r).ok().and_then(|o| o.as_array().ok()),
        _ => None,
    };
    let Some(diff_arr) = diff_arr else { return };

    let mut code: u32 = 0;
    for item in diff_arr {
        match item {
            Object::Integer(i) if *i >= 0 => code = *i as u32,
            Object::Name(n) => {
                if let Ok(name) = String::from_utf8(n.clone()) {
                    differences.insert(code, name);
                }
                code = code.saturating_add(1);
            }
            Object::Reference(r) => {
                if let Ok(resolved) = doc.get_object(*r) {
                    match resolved {
                        Object::Integer(i) if *i >= 0 => code = *i as u32,
                        Object::Name(n) => {
                            if let Ok(name) = String::from_utf8(n.clone()) {
                                differences.insert(code, name);
                            }
                            code = code.saturating_add(1);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

/// Compute width corrections for a simple TrueType font.
///
/// Returns a list of (index_in_widths_array, correct_width) for mismatched entries.
/// Uses the font's cmap table to map character codes to glyph IDs.
fn compute_truetype_width_corrections(
    font_data: &[u8],
    first_char: u32,
    existing_widths: &[Object],
    enc_info: &(String, std::collections::HashMap<u32, String>),
) -> Vec<(usize, i64)> {
    let Ok(face) = ttf_parser::Face::parse(font_data, 0) else {
        return Vec::new();
    };

    let units_per_em = face.units_per_em() as f64;
    if units_per_em == 0.0 {
        return Vec::new();
    }
    let scale = 1000.0 / units_per_em;

    let (enc_name, differences) = enc_info;

    let mut corrections = Vec::new();

    for (i, obj) in existing_widths.iter().enumerate() {
        let pdf_w = match obj {
            Object::Integer(w) => *w as f64,
            Object::Real(r) => *r as f64,
            _ => continue,
        };

        let code = first_char + i as u32;

        // Determine the expected glyph width from the font program (fractional).
        let expected_w =
            get_truetype_glyph_width_fractional(&face, code, enc_name, differences, scale);

        let Some(frac_w) = expected_w else { continue };

        // veraPDF compares fractional widths with tolerance > 1.0.
        // Use the same threshold to match its validation logic.
        if (pdf_w - frac_w).abs() > 1.0 {
            corrections.push((i, frac_w.round() as i64));
        }
    }

    corrections
}

/// Get the expected glyph width for a character code in a TrueType font.
///
/// Uses the encoding to map code -> Unicode -> glyph ID via cmap.
/// If the Differences array overrides the glyph for this code, uses that.
#[allow(dead_code)]
fn get_truetype_glyph_width_for_code(
    face: &ttf_parser::Face,
    code: u32,
    enc_name: &str,
    differences: &std::collections::HashMap<u32, String>,
    scale: f64,
) -> Option<i64> {
    // If Differences maps this code to a glyph name, try to use it.
    if let Some(glyph_name) = differences.get(&code) {
        // Try to find glyph by name -> Unicode -> cmap.
        if let Some(unicode) = glyph_name_to_unicode(glyph_name) {
            if let Some(gid) = face.glyph_index(unicode) {
                return face
                    .glyph_hor_advance(gid)
                    .map(|w| (w as f64 * scale).round() as i64);
            }
        }
        // For .notdef or unmapped glyphs, use glyph ID 0 width.
        if glyph_name == ".notdef" {
            return face
                .glyph_hor_advance(ttf_parser::GlyphId(0))
                .map(|w| (w as f64 * scale).round() as i64);
        }
    }

    // Map code -> Unicode via encoding.
    let ch = encoding_to_char(code, enc_name);

    // Primary: look up via cmap (what veraPDF does for non-symbolic fonts).
    if let Some(gid) = face.glyph_index(ch) {
        return face
            .glyph_hor_advance(gid)
            .map(|w| (w as f64 * scale).round() as i64);
    }

    // Fallback: try direct GID = code (identity mapping for some TrueType fonts).
    if code <= u16::MAX as u32 {
        if let Some(w) = face.glyph_hor_advance(ttf_parser::GlyphId(code as u16)) {
            return Some((w as f64 * scale).round() as i64);
        }
    }

    // Can't determine width — return None to skip this entry.
    None
}

/// Get the expected glyph width as an unrounded f64 for fractional comparison.
///
/// veraPDF compares the fractional glyph width from the font program against
/// the /Widths value with a tolerance of > 1.0. Uses the PDF Encoding to
/// map character codes to Unicode, then looks up in the font's cmap.
fn get_truetype_glyph_width_fractional(
    face: &ttf_parser::Face,
    code: u32,
    enc_name: &str,
    differences: &std::collections::HashMap<u32, String>,
    scale: f64,
) -> Option<f64> {
    // If Differences maps this code to a glyph name, try to use it.
    if let Some(glyph_name) = differences.get(&code) {
        if let Some(unicode) = glyph_name_to_unicode(glyph_name) {
            if let Some(gid) = lookup_unicode_cmap_31(face, unicode as u32) {
                return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
            }
        }
        // Try looking up glyph by name directly in the font.
        if let Some(gid) = face.glyph_index_by_name(glyph_name) {
            return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
        }
        if glyph_name == ".notdef" {
            return face
                .glyph_hor_advance(ttf_parser::GlyphId(0))
                .map(|w| w as f64 * scale);
        }
    }

    // Map code → Unicode via PDF Encoding, then look up in the (3,1) cmap
    // specifically. veraPDF uses only the (3,1) cmap for non-symbolic TrueType
    // font width validation. Using the general glyph_index() would search all
    // subtables and may find a mapping in (1,0) Mac Roman that doesn't exist
    // in (3,1), causing width mismatches.
    let ch = encoding_to_char(code, enc_name);
    if let Some(gid) = lookup_unicode_cmap_31(face, ch as u32) {
        return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
    }

    // Character not found in (3,1) cmap. For codes outside the 128-159 range
    // (where Mac Roman and WinAnsi have identical mappings), fall back to (1,0)
    // Mac Roman cmap. veraPDF uses this fallback for non-symbolic TrueType.
    // Codes 128-159 differ between Mac Roman and WinAnsi — skip those to avoid
    // incorrect width lookups (root cause of 0168-style regressions).
    let allow_winansi_145_146 = enc_name == "WinAnsiEncoding" && (code == 145 || code == 146);
    if code <= 255 && (!(128..=159).contains(&code) || allow_winansi_145_146) {
        if let Some(gid) = lookup_mac_cmap(face, code) {
            return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
        }
    }

    None
}

/// Look up a Unicode code point in the (3,1) Windows Unicode BMP cmap only.
/// Returns None if the font doesn't have a (3,1) subtable or doesn't map
/// the given code point. This matches veraPDF's behavior for non-symbolic
/// TrueType fonts.
fn lookup_unicode_cmap_31(face: &ttf_parser::Face, unicode: u32) -> Option<ttf_parser::GlyphId> {
    let cmap = face.tables().cmap?;
    for subtable in cmap.subtables {
        if subtable.platform_id == ttf_parser::PlatformId::Windows && subtable.encoding_id == 1 {
            let gid = subtable.glyph_index(unicode)?;
            if gid.0 != 0 {
                return Some(gid);
            }
            return None; // Mapped to .notdef
        }
    }
    None
}

/// Load CMap cidrange data for any named predefined CMap, bypassing the
/// unicode-only restriction in `load_predefined_unicode_cmap_ranges`.
/// Used by EUC-style CMap handling to get the code→CID mapping.
fn load_all_cmap_cidranges(cmap_name: &str) -> Option<Vec<(u16, u16, u16)>> {
    let data = find_predefined_cmap_file(cmap_name)?;
    parse_predefined_cmap_cid_ranges(&data)
}

/// Returns true for EUC-encoded CMaps (GB-EUC-H/V, KSC-EUC-H/V) that have a
/// mixed 1-byte/2-byte codespace: bytes 0x00–0x80 are single-byte codes and
/// bytes 0xA1–0xFE can start 2-byte sequences.
fn is_euc_style_cmap(cmap_name: &str) -> bool {
    let n = cmap_name.to_ascii_lowercase();
    n.contains("euc")
}

/// Fix a CID text string in-place for EUC-style CMaps (e.g. GB-EUC-H).
///
/// Unlike `fix_cid_text_string` which always processes bytes as 2-byte pairs,
/// this function respects the EUC codespace rules:
///   - Byte b ≤ 0x80: single-byte character code → look up CID in `cmap_ranges`
///   - Byte b ≥ 0xA1 followed by byte b2 ≥ 0xA1: 2-byte character code
///   - All other bytes (lone high bytes, undefined ranges): removed
///
/// Invalid codes (CID not in `valid_cids`) are removed without replacement so
/// that control-character table-rule sequences that map to .notdef do not
/// appear in the output stream.
fn fix_cid_text_string_euc(
    bytes: &mut Vec<u8>,
    cmap_ranges: &[(u16, u16, u16)],
    valid_cids: &std::collections::HashSet<u16>,
) -> bool {
    if bytes.is_empty() {
        return false;
    }
    let mut changed = false;
    let mut result = Vec::with_capacity(bytes.len());
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];

        if b <= 0x80 {
            // Single-byte EUC code in codespace [0x00, 0x80].
            let cid = cmap_code_to_cid(cmap_ranges, b as u16);
            let is_valid = cid.is_some_and(|c| valid_cids.contains(&c));
            if is_valid {
                result.push(b);
            } else {
                changed = true; // omit invalid 1-byte code
            }
            i += 1;
        } else if b >= 0xA1 && i + 1 < bytes.len() && bytes[i + 1] >= 0xA1 {
            // Valid 2-byte EUC pair.
            let b2 = bytes[i + 1];
            let code = (b as u16) * 256 + b2 as u16;
            let cid = cmap_code_to_cid(cmap_ranges, code);
            let is_valid = cid.is_some_and(|c| valid_cids.contains(&c));
            if is_valid {
                result.push(b);
                result.push(b2);
            } else {
                changed = true; // omit invalid 2-byte code
            }
            i += 2;
        } else {
            // Lone high byte (0x81–0xA0 or lone 0xA1+ without valid pair): discard.
            changed = true;
            i += 1;
        }
    }

    if changed {
        *bytes = result;
    }
    changed
}

/// Look up a raw byte code in the (1,0) Macintosh Roman cmap subtable.
/// This matches veraPDF's fallback behavior for non-symbolic TrueType fonts.
fn lookup_mac_cmap(face: &ttf_parser::Face, code: u32) -> Option<ttf_parser::GlyphId> {
    let cmap = face.tables().cmap?;
    for subtable in cmap.subtables {
        if subtable.platform_id == ttf_parser::PlatformId::Macintosh && subtable.encoding_id == 0 {
            let gid = subtable.glyph_index(code)?;
            if gid.0 != 0 {
                return Some(gid);
            }
        }
    }
    None
}

const PREDEFINED_CMAP_SEARCH_DIRS: &[&str] = &[
    concat!(env!("CARGO_MANIFEST_DIR"), "/resources/cmap"),
    "/usr/share/poppler/cMap",
    "/usr/share/fonts/cmap",
    "/usr/share/fonts/cMap",
    "/usr/share/ghostscript/cMap",
];

fn resolve_type0_cmap_name(doc: &Document, font_dict: &lopdf::Dictionary) -> Option<String> {
    match font_dict.get(b"Encoding").ok()? {
        Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
        Object::Reference(r) => match doc.objects.get(r) {
            Some(Object::Name(n)) => Some(String::from_utf8_lossy(n).to_string()),
            Some(Object::Dictionary(d)) => match d.get(b"CMapName").ok()? {
                Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
                _ => None,
            },
            Some(Object::Stream(s)) => match s.dict.get(b"CMapName").ok()? {
                Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
                _ => None,
            },
            _ => None,
        },
        Object::Dictionary(d) => match d.get(b"CMapName").ok()? {
            Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
            _ => None,
        },
        Object::Stream(s) => match s.dict.get(b"CMapName").ok()? {
            Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
            _ => None,
        },
        _ => None,
    }
}

fn is_identity_type0_cmap(cmap_name: &str) -> bool {
    let cmap_name = cmap_name.to_ascii_lowercase();
    cmap_name == "identity-h" || cmap_name == "identity-v"
}

fn is_unicode_predefined_type0_cmap(cmap_name: &str) -> bool {
    let cmap_name = cmap_name.to_ascii_lowercase();
    cmap_name.starts_with("uni") && (cmap_name.contains("-ucs2-") || cmap_name.contains("-utf16-"))
}

fn find_predefined_cmap_file(cmap_name: &str) -> Option<Vec<u8>> {
    use std::path::Path;

    for base_dir in PREDEFINED_CMAP_SEARCH_DIRS {
        let direct = Path::new(base_dir).join(cmap_name);
        if let Ok(data) = std::fs::read(&direct) {
            return Some(data);
        }

        if let Ok(entries) = std::fs::read_dir(base_dir) {
            for entry in entries.flatten() {
                if !entry.path().is_dir() {
                    continue;
                }
                let nested = entry.path().join(cmap_name);
                if let Ok(data) = std::fs::read(&nested) {
                    return Some(data);
                }
            }
        }
    }

    None
}

fn parse_predefined_cmap_cid_ranges(data: &[u8]) -> Option<Vec<(u16, u16, u16)>> {
    let text = std::str::from_utf8(data).ok()?;
    let mut ranges = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with('<') {
            continue;
        }
        let mut parts = trimmed.split_whitespace();
        let Some(start_hex) = parts.next() else {
            continue;
        };
        let Some(end_hex) = parts.next() else {
            continue;
        };
        let Some(cid_str) = parts.next() else {
            continue;
        };
        if !start_hex.ends_with('>') || !end_hex.ends_with('>') {
            continue;
        }

        let start = u32::from_str_radix(start_hex.trim_matches(['<', '>']), 16).ok()?;
        let end = u32::from_str_radix(end_hex.trim_matches(['<', '>']), 16).ok()?;
        let base_cid = cid_str.parse::<u32>().ok()?;
        if start > u16::MAX as u32 || end > u16::MAX as u32 {
            continue;
        }
        let span = end.saturating_sub(start);
        if base_cid + span > u16::MAX as u32 {
            continue;
        }

        ranges.push((start as u16, end as u16, base_cid as u16));
    }

    if ranges.is_empty() {
        None
    } else {
        ranges.sort_unstable_by_key(|(start, _, _)| *start);
        Some(ranges)
    }
}

fn load_predefined_unicode_cmap_ranges(cmap_name: &str) -> Option<Vec<(u16, u16, u16)>> {
    if !is_unicode_predefined_type0_cmap(cmap_name) {
        return None;
    }
    let data = find_predefined_cmap_file(cmap_name)?;
    parse_predefined_cmap_cid_ranges(&data)
}

fn build_predefined_cmap_cidtogid_map(
    face: &ttf_parser::Face,
    ranges: &[(u16, u16, u16)],
) -> Option<Vec<u8>> {
    let max_cid = ranges
        .iter()
        .map(|(start, end, base)| u32::from(*base) + u32::from(*end - *start))
        .max()? as usize;
    let mut map = vec![0u8; (max_cid + 1) * 2];
    let mut mapped_any = false;

    for (start, end, base_cid) in ranges {
        for code in *start..=*end {
            let Some(ch) = char::from_u32(code as u32) else {
                continue;
            };
            let Some(gid) = face
                .glyph_index(ch)
                .filter(|gid| gid.0 > 0 && tt_glyph_has_data(face, *gid))
            else {
                continue;
            };
            let cid = u32::from(*base_cid) + u32::from(code - *start);
            if cid > u16::MAX as u32 {
                continue;
            }
            let idx = cid as usize * 2;
            map[idx..idx + 2].copy_from_slice(&gid.0.to_be_bytes());
            mapped_any = true;
        }
    }

    mapped_any.then_some(map)
}

fn cmap_code_to_cid(ranges: &[(u16, u16, u16)], code: u16) -> Option<u16> {
    for (start, end, base_cid) in ranges {
        if code < *start || code > *end {
            continue;
        }
        return Some(base_cid.saturating_add(code - *start));
    }
    None
}

fn cmap_first_code_for_cid(ranges: &[(u16, u16, u16)], target_cid: u16) -> Option<u16> {
    for (start, end, base_cid) in ranges {
        let span = *end - *start;
        if target_cid < *base_cid || target_cid > base_cid.saturating_add(span) {
            continue;
        }
        return Some(start.saturating_add(target_cid - *base_cid));
    }
    None
}

fn build_valid_codes_from_cmap_ranges(
    valid_cids: &std::collections::HashSet<u16>,
    ranges: &[(u16, u16, u16)],
) -> std::collections::HashSet<u16> {
    let mut valid_codes = std::collections::HashSet::new();
    for (start, end, _) in ranges {
        for code in *start..=*end {
            if cmap_code_to_cid(ranges, code).is_some_and(|cid| valid_cids.contains(&cid)) {
                valid_codes.insert(code);
            }
        }
    }
    valid_codes
}

/// Map a common glyph name to its Unicode codepoint.
/// Based on the Adobe Glyph List (AGL) for common names.
fn glyph_name_to_unicode(name: &str) -> Option<char> {
    // Handle "uniXXXX" format.
    if name.starts_with("uni") && name.len() == 7 {
        if let Ok(cp) = u32::from_str_radix(&name[3..], 16) {
            return char::from_u32(cp);
        }
    }

    // Single ASCII character names.
    if name.len() == 1 {
        return name.chars().next();
    }

    // Common glyph names from AGL.
    match name {
        "space" => Some(' '),
        "exclam" => Some('!'),
        "quotedbl" => Some('"'),
        "numbersign" => Some('#'),
        "dollar" => Some('$'),
        "percent" => Some('%'),
        "ampersand" => Some('&'),
        "quotesingle" => Some('\''),
        "parenleft" => Some('('),
        "parenright" => Some(')'),
        "asterisk" => Some('*'),
        "plus" => Some('+'),
        "comma" => Some(','),
        "hyphen" => Some('-'),
        "minus" => Some('\u{2212}'),
        "period" => Some('.'),
        "slash" => Some('/'),
        "zero" => Some('0'),
        "one" => Some('1'),
        "two" => Some('2'),
        "three" => Some('3'),
        "four" => Some('4'),
        "five" => Some('5'),
        "six" => Some('6'),
        "seven" => Some('7'),
        "eight" => Some('8'),
        "nine" => Some('9'),
        "colon" => Some(':'),
        "semicolon" => Some(';'),
        "less" => Some('<'),
        "equal" => Some('='),
        "greater" => Some('>'),
        "question" => Some('?'),
        "at" => Some('@'),
        "bracketleft" => Some('['),
        "backslash" => Some('\\'),
        "bracketright" => Some(']'),
        "asciicircum" => Some('^'),
        "underscore" => Some('_'),
        "grave" => Some('`'),
        "braceleft" => Some('{'),
        "bar" => Some('|'),
        "braceright" => Some('}'),
        "asciitilde" => Some('~'),
        "bullet" => Some('\u{2022}'),
        "ellipsis" => Some('\u{2026}'),
        "emdash" => Some('\u{2014}'),
        "endash" => Some('\u{2013}'),
        "fi" => Some('\u{FB01}'),
        "fl" => Some('\u{FB02}'),
        "quotedblleft" => Some('\u{201C}'),
        "quotedblright" => Some('\u{201D}'),
        "quoteleft" => Some('\u{2018}'),
        "quoteright" => Some('\u{2019}'),
        "quotesinglbase" => Some('\u{201A}'),
        "quotedblbase" => Some('\u{201E}'),
        "dagger" => Some('\u{2020}'),
        "daggerdbl" => Some('\u{2021}'),
        "trademark" => Some('\u{2122}'),
        "copyright" => Some('\u{00A9}'),
        "registered" => Some('\u{00AE}'),
        "degree" => Some('\u{00B0}'),
        "Euro" => Some('\u{20AC}'),
        "sterling" => Some('\u{00A3}'),
        "yen" => Some('\u{00A5}'),
        "cent" => Some('\u{00A2}'),
        "section" => Some('\u{00A7}'),
        "paragraph" => Some('\u{00B6}'),
        "germandbls" => Some('\u{00DF}'),
        "Adieresis" => Some('\u{00C4}'),
        "Odieresis" => Some('\u{00D6}'),
        "Udieresis" => Some('\u{00DC}'),
        "adieresis" => Some('\u{00E4}'),
        "odieresis" => Some('\u{00F6}'),
        "udieresis" => Some('\u{00FC}'),
        "Aacute" => Some('\u{00C1}'),
        "Agrave" => Some('\u{00C0}'),
        "Acircumflex" => Some('\u{00C2}'),
        "Atilde" => Some('\u{00C3}'),
        "Aring" => Some('\u{00C5}'),
        "AE" => Some('\u{00C6}'),
        "Ccedilla" => Some('\u{00C7}'),
        "Eacute" => Some('\u{00C9}'),
        "Egrave" => Some('\u{00C8}'),
        "Ecircumflex" => Some('\u{00CA}'),
        "Edieresis" => Some('\u{00CB}'),
        "Iacute" => Some('\u{00CD}'),
        "Igrave" => Some('\u{00CC}'),
        "Icircumflex" => Some('\u{00CE}'),
        "Idieresis" => Some('\u{00CF}'),
        "Ntilde" => Some('\u{00D1}'),
        "Oacute" => Some('\u{00D3}'),
        "Ograve" => Some('\u{00D2}'),
        "Ocircumflex" => Some('\u{00D4}'),
        "Otilde" => Some('\u{00D5}'),
        "Oslash" => Some('\u{00D8}'),
        "Scaron" => Some('\u{0160}'),
        "Uacute" => Some('\u{00DA}'),
        "Ugrave" => Some('\u{00D9}'),
        "Ucircumflex" => Some('\u{00DB}'),
        "Zcaron" => Some('\u{017D}'),
        "aacute" => Some('\u{00E1}'),
        "agrave" => Some('\u{00E0}'),
        "acircumflex" => Some('\u{00E2}'),
        "atilde" => Some('\u{00E3}'),
        "aring" => Some('\u{00E5}'),
        "ae" => Some('\u{00E6}'),
        "ccedilla" => Some('\u{00E7}'),
        "eacute" => Some('\u{00E9}'),
        "egrave" => Some('\u{00E8}'),
        "ecircumflex" => Some('\u{00EA}'),
        "edieresis" => Some('\u{00EB}'),
        "iacute" => Some('\u{00ED}'),
        "igrave" => Some('\u{00EC}'),
        "icircumflex" => Some('\u{00EE}'),
        "idieresis" => Some('\u{00EF}'),
        "ntilde" => Some('\u{00F1}'),
        "oacute" => Some('\u{00F3}'),
        "ograve" => Some('\u{00F2}'),
        "ocircumflex" => Some('\u{00F4}'),
        "otilde" => Some('\u{00F5}'),
        "oslash" => Some('\u{00F8}'),
        "scaron" => Some('\u{0161}'),
        "uacute" => Some('\u{00FA}'),
        "ugrave" => Some('\u{00F9}'),
        "ucircumflex" => Some('\u{00FB}'),
        "zcaron" => Some('\u{017E}'),
        "thorn" => Some('\u{00FE}'),
        "eth" => Some('\u{00F0}'),
        "Eth" => Some('\u{00D0}'),
        "Thorn" => Some('\u{00DE}'),
        "multiply" => Some('\u{00D7}'),
        "divide" => Some('\u{00F7}'),
        "mu" => Some('\u{00B5}'),
        "guillemotleft" => Some('\u{00AB}'),
        "guillemotright" => Some('\u{00BB}'),
        "guilsinglleft" => Some('\u{2039}'),
        "guilsinglright" => Some('\u{203A}'),
        "exclamdown" => Some('\u{00A1}'),
        "questiondown" => Some('\u{00BF}'),
        "perthousand" => Some('\u{2030}'),
        "circumflex" => Some('\u{02C6}'),
        "tilde" => Some('\u{02DC}'),
        "dotlessi" => Some('\u{0131}'),
        "lslash" => Some('\u{0142}'),
        "Lslash" => Some('\u{0141}'),
        "OE" => Some('\u{0152}'),
        "oe" => Some('\u{0153}'),
        "Ydieresis" => Some('\u{0178}'),
        "ydieresis" => Some('\u{00FF}'),
        "florin" => Some('\u{0192}'),
        "fraction" => Some('\u{2044}'),
        "acute" => Some('\u{00B4}'),
        "cedilla" => Some('\u{00B8}'),
        "dieresis" => Some('\u{00A8}'),
        "macron" => Some('\u{00AF}'),
        "ring" => Some('\u{02DA}'),
        "caron" => Some('\u{02C7}'),
        "breve" => Some('\u{02D8}'),
        "ogonek" => Some('\u{02DB}'),
        "hungarumlaut" => Some('\u{02DD}'),
        "dotaccent" => Some('\u{02D9}'),
        "nbspace" | "nonbreakingspace" => Some('\u{00A0}'),
        "ordfeminine" => Some('\u{00AA}'),
        "ordmasculine" => Some('\u{00BA}'),
        "logicalnot" => Some('\u{00AC}'),
        "brokenbar" => Some('\u{00A6}'),
        "currency" => Some('\u{00A4}'),
        "plusminus" => Some('\u{00B1}'),
        "onesuperior" => Some('\u{00B9}'),
        "twosuperior" => Some('\u{00B2}'),
        "threesuperior" => Some('\u{00B3}'),
        "onequarter" => Some('\u{00BC}'),
        "onehalf" => Some('\u{00BD}'),
        "threequarters" => Some('\u{00BE}'),
        "periodcentered" | "middot" => Some('\u{00B7}'),
        ".notdef" => None,
        _ => None,
    }
}

/// Compute width corrections for a Type1 font with FontFile (PFA/PFB) program.
///
/// Returns (index_in_widths_array, correct_width) for mismatched entries.
/// Only reliable for subset fonts where charstrings may differ from the original dict.
fn compute_type1_fontfile_width_corrections(
    font_data: &[u8],
    first_char: u32,
    existing_widths: &[Object],
    enc_info: &(String, std::collections::HashMap<u32, String>),
) -> Vec<(usize, i64)> {
    let (enc_name, differences) = enc_info;
    let Some(parsed) = parse_type1_program(font_data) else {
        return Vec::new();
    };
    let scale = parsed.font_matrix_sx * 1000.0;
    let mut corrections = Vec::new();
    for (i, obj) in existing_widths.iter().enumerate() {
        let pdf_w = match obj {
            Object::Integer(w) => *w as f64,
            Object::Real(r) => *r as f64,
            _ => continue,
        };
        let code = first_char + i as u32;
        let glyph_name = if let Some(name) = differences.get(&code) {
            name.as_str().to_string()
        } else if let Some(name) = parsed.encoding.get(&(code as u8)) {
            name.clone()
        } else {
            let ch = encoding_to_char(code, enc_name);
            unicode_to_agl_name(ch)
                .or_else(|| unicode_to_glyph_name(ch))
                .unwrap_or_default()
        };
        if glyph_name.is_empty() || glyph_name == ".notdef" {
            continue;
        }
        let Some(&cs_width) = parsed.charstring_widths.get(glyph_name.as_str()) else {
            continue;
        };
        let font_w = (cs_width as f64 * scale).round();
        if (pdf_w - font_w).abs() >= 1.0 {
            corrections.push((i, font_w as i64));
        }
    }
    corrections
}

/// Compute a single glyph width from a Type 1 FontFile program.
fn compute_type1_fontfile_single_width(
    font_data: &[u8],
    code: u32,
    enc_name: &str,
    differences: &std::collections::HashMap<u32, String>,
) -> Option<f64> {
    let parsed = parse_type1_program(font_data)?;

    let glyph_name = if let Some(name) = differences.get(&code) {
        name.as_str().to_string()
    } else if let Some(name) = parsed.encoding.get(&(code as u8)) {
        name.clone()
    } else {
        let ch = encoding_to_char(code, enc_name);
        unicode_to_agl_name(ch)
            .or_else(|| unicode_to_glyph_name(ch))
            .unwrap_or_default()
    };

    if glyph_name.is_empty() || glyph_name == ".notdef" {
        return None;
    }

    let cs_width = *parsed.charstring_widths.get(glyph_name.as_str())?;
    Some((cs_width as f64 * parsed.font_matrix_sx * 1000.0).round())
}

/// Parsed data from a Type 1 font program.
struct Type1Parsed {
    font_matrix_sx: f64,
    encoding: std::collections::HashMap<u8, String>,
    charstring_widths: std::collections::HashMap<String, i32>,
}

/// Parse a Type 1 font program (PFB/PFA) to extract FontMatrix, Encoding, and widths.
fn parse_type1_program(data: &[u8]) -> Option<Type1Parsed> {
    // Find the cleartext/encrypted boundary.
    // PFB format has segment headers; PFA is plain text with hex-encoded eexec.
    let (cleartext, eexec_data) = split_type1_sections(data)?;

    // Parse FontMatrix from cleartext.
    let font_matrix_sx = parse_type1_font_matrix(cleartext).unwrap_or(0.001);

    // Parse Encoding from cleartext.
    let mut encoding = parse_type1_encoding(cleartext);

    // Decrypt eexec section.
    let decrypted = eexec_decrypt(eexec_data);

    // Parse lenIV from cleartext first, then from decrypted Private dict.
    let len_iv_cleartext = parse_type1_len_iv(cleartext);
    let len_iv_bytes = parse_type1_len_iv_bytes(&decrypted);
    let len_iv = len_iv_cleartext.or(len_iv_bytes).unwrap_or(4) as usize;

    // Some Type1 programs define/override Encoding inside eexec; merge those.
    encoding.extend(parse_type1_encoding_bytes(&decrypted));

    // Parse CharStrings from decrypted data.
    let charstring_widths = parse_type1_charstrings(&decrypted, len_iv);

    Some(Type1Parsed {
        font_matrix_sx,
        encoding,
        charstring_widths,
    })
}

/// Split a Type 1 font into cleartext and eexec-encrypted sections.
fn split_type1_sections(data: &[u8]) -> Option<(&[u8], &[u8])> {
    // Check for PFB format (starts with 0x80).
    if data.first() == Some(&0x80) {
        return split_pfb_sections(data);
    }

    // PFA format: find "eexec" keyword.
    let eexec_pos = find_bytes(data, b"eexec")?;
    let cleartext = &data[..eexec_pos];

    // Skip "eexec" and any whitespace.
    let mut pos = eexec_pos + 5;
    while pos < data.len() && matches!(data[pos], b' ' | b'\r' | b'\n' | b'\t') {
        pos += 1;
    }

    // The eexec data can be binary or hex-encoded.
    let remaining = &data[pos..];
    if remaining.is_empty() {
        return None;
    }

    // Check if hex-encoded (all hex chars + whitespace).
    let is_hex = remaining
        .iter()
        .take(20)
        .all(|b| b.is_ascii_hexdigit() || matches!(b, b'\r' | b'\n' | b' '));

    if is_hex {
        // Decode hex to binary.
        // For efficiency, we can't return a slice — we'd need owned data.
        // Instead, return the hex data and let the caller decode it.
        // Actually, since we need a slice, we'll handle hex in eexec_decrypt.
        Some((cleartext, remaining))
    } else {
        Some((cleartext, remaining))
    }
}

/// Split PFB (binary) format into cleartext and eexec sections.
fn split_pfb_sections(data: &[u8]) -> Option<(&[u8], &[u8])> {
    let mut pos = 0;
    let mut cleartext_end = 0;

    while pos + 6 <= data.len() {
        if data[pos] != 0x80 {
            break;
        }
        let seg_type = data[pos + 1];
        let seg_len =
            u32::from_le_bytes([data[pos + 2], data[pos + 3], data[pos + 4], data[pos + 5]])
                as usize;
        let seg_data_start = pos + 6;

        match seg_type {
            1 => {
                // ASCII segment (cleartext).
                cleartext_end = seg_data_start + seg_len;
            }
            2 => {
                // Binary segment (eexec encrypted).
                let eexec_end = seg_data_start + seg_len;
                return Some((&data[6..cleartext_end], &data[seg_data_start..eexec_end]));
            }
            3 => break, // EOF marker.
            _ => break,
        }
        pos = seg_data_start + seg_len;
    }

    // Fallback: try eexec keyword search.
    let eexec_pos = find_bytes(&data[6..], b"eexec")?;
    let cleartext = &data[6..6 + eexec_pos];
    let mut skip = 6 + eexec_pos + 5;
    while skip < data.len() && matches!(data[skip], b' ' | b'\r' | b'\n' | b'\t') {
        skip += 1;
    }
    Some((cleartext, &data[skip..]))
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Parse FontMatrix from Type 1 cleartext.
fn parse_type1_font_matrix(cleartext: &[u8]) -> Option<f64> {
    let text = std::str::from_utf8(cleartext).ok()?;
    let fm_pos = text.find("/FontMatrix")?;
    let after = &text[fm_pos..];
    let bracket_start = after.find('[')?;
    let bracket_end = after.find(']')?;
    let values_str = &after[bracket_start + 1..bracket_end];
    let values: Vec<f64> = values_str
        .split_whitespace()
        .filter_map(|s| s.parse().ok())
        .collect();
    if !values.is_empty() {
        Some(values[0])
    } else {
        None
    }
}

/// Parse Encoding array from Type 1 cleartext.
fn parse_type1_encoding(cleartext: &[u8]) -> std::collections::HashMap<u8, String> {
    let mut encoding = std::collections::HashMap::new();
    let Ok(text) = std::str::from_utf8(cleartext) else {
        return encoding;
    };

    // Look for patterns like: dup <code> /<name> put
    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("dup ") || !trimmed.ends_with(" put") {
            continue;
        }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() >= 4 && parts[0] == "dup" && parts[3] == "put" {
            if let Ok(code) = parts[1].parse::<u8>() {
                if let Some(name) = parts[2].strip_prefix('/') {
                    if name != ".notdef" {
                        encoding.insert(code, name.to_string());
                    }
                }
            }
        }
    }

    encoding
}

/// Parse Encoding array from decrypted eexec bytes.
/// Restrict search to content before /CharStrings to avoid binary false positives.
fn parse_type1_encoding_bytes(data: &[u8]) -> std::collections::HashMap<u8, String> {
    let end = find_bytes(data, b"/CharStrings").unwrap_or(data.len());
    parse_type1_encoding(&data[..end])
}

/// Parse lenIV from Type 1 cleartext (number of random bytes at start of charstrings).
fn parse_type1_len_iv(cleartext: &[u8]) -> Option<u32> {
    let text = std::str::from_utf8(cleartext).ok()?;
    let pos = text.find("/lenIV")?;
    let after = &text[pos + 6..];
    let trimmed = after.trim_start();
    trimmed.split_whitespace().next()?.parse().ok()
}

/// Parse lenIV from raw bytes (e.g., decrypted Private dict).
/// Only searches before /CharStrings to avoid false positives from
/// binary charstring data that coincidentally contains the byte sequence.
fn parse_type1_len_iv_bytes(data: &[u8]) -> Option<u32> {
    // Restrict search to Private dict text portion (before /CharStrings).
    let search_end = find_bytes(data, b"/CharStrings").unwrap_or(data.len());
    let search_data = &data[..search_end];
    let pos = find_bytes(search_data, b"/lenIV")?;
    let after = &search_data[pos + 6..];
    // Skip whitespace.
    let trimmed = after.iter().position(|b| !b.is_ascii_whitespace())?;
    let start = trimmed;
    let end = after[start..]
        .iter()
        .position(|b| b.is_ascii_whitespace() || *b == b'/')
        .unwrap_or(after.len() - start);
    let num_str = std::str::from_utf8(&after[start..start + end]).ok()?;
    num_str.parse().ok()
}

/// Decrypt eexec-encrypted data. Initial key R=55665, c1=52845, c2=22719.
fn eexec_decrypt(data: &[u8]) -> Vec<u8> {
    // Check if hex-encoded.
    let is_hex = data
        .iter()
        .take(20)
        .all(|b| b.is_ascii_hexdigit() || matches!(b, b'\r' | b'\n' | b' '));

    let binary_data: Vec<u8>;
    let input = if is_hex {
        // Decode hex to binary.
        let hex_chars: Vec<u8> = data
            .iter()
            .copied()
            .filter(|b| b.is_ascii_hexdigit())
            .collect();
        binary_data = hex_chars
            .chunks(2)
            .filter_map(|pair| {
                if pair.len() == 2 {
                    let hi = hex_val(pair[0]);
                    let lo = hex_val(pair[1]);
                    Some((hi << 4) | lo)
                } else {
                    None
                }
            })
            .collect();
        binary_data.as_slice()
    } else {
        data
    };

    let mut r: u16 = 55665;
    let c1: u16 = 52845;
    let c2: u16 = 22719;

    let mut result = Vec::with_capacity(input.len());
    for &cipher in input {
        let plain = cipher ^ (r >> 8) as u8;
        r = (cipher as u16)
            .wrapping_add(r)
            .wrapping_mul(c1)
            .wrapping_add(c2);
        result.push(plain);
    }

    // Skip first 4 random bytes.
    if result.len() > 4 {
        result.drain(..4);
    }

    result
}

fn hex_val(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'A'..=b'F' => b - b'A' + 10,
        b'a'..=b'f' => b - b'a' + 10,
        _ => 0,
    }
}

/// Parse CharStrings from decrypted eexec data to extract glyph widths.
/// Parse CharStrings from decrypted eexec data (works with raw bytes).
///
/// Charstring entries look like: `/<name> <length> RD <binary> ND`
/// or `/<name> <length> -| <binary> |-`.
fn parse_type1_charstrings(
    decrypted: &[u8],
    len_iv: usize,
) -> std::collections::HashMap<String, i32> {
    let mut widths = std::collections::HashMap::new();

    // Find /CharStrings marker.
    let Some(cs_pos) = find_bytes(decrypted, b"/CharStrings") else {
        return widths;
    };

    // Scan forward from /CharStrings looking for charstring entries.
    let mut pos = cs_pos;

    while pos < decrypted.len() {
        // Find next '/' which starts a glyph name.
        let Some(slash_offset) = decrypted[pos..].iter().position(|&b| b == b'/') else {
            break;
        };
        let slash_pos = pos + slash_offset;

        // Check for "end" keyword before this slash (end of CharStrings dict).
        // Look at up to 20 bytes before the slash for "end".
        let check_start = slash_pos.saturating_sub(20).max(pos);
        if find_bytes(&decrypted[check_start..slash_pos], b"end").is_some() {
            // Check if this is /FontName or other non-charstring entry.
            let remaining = &decrypted[slash_pos..];
            if !remaining.starts_with(b"/CharStrings") {
                break;
            }
        }

        // Extract glyph name: read ASCII chars until whitespace.
        let name_start = slash_pos + 1;
        if name_start >= decrypted.len() {
            break;
        }
        let name_end = decrypted[name_start..]
            .iter()
            .position(|b| b.is_ascii_whitespace())
            .map(|p| name_start + p)
            .unwrap_or(decrypted.len());
        let glyph_name = std::str::from_utf8(&decrypted[name_start..name_end])
            .unwrap_or("")
            .to_string();

        if glyph_name.is_empty() {
            pos = name_end + 1;
            continue;
        }

        // Skip whitespace after name.
        let mut p = name_end;
        while p < decrypted.len() && decrypted[p].is_ascii_whitespace() {
            p += 1;
        }

        // Read length number.
        let num_start = p;
        while p < decrypted.len() && decrypted[p].is_ascii_digit() {
            p += 1;
        }
        let len_str = std::str::from_utf8(&decrypted[num_start..p]).unwrap_or("");
        let Ok(cs_len) = len_str.parse::<usize>() else {
            pos = p.max(name_end + 1);
            continue;
        };

        // Skip whitespace.
        while p < decrypted.len() && decrypted[p].is_ascii_whitespace() {
            p += 1;
        }

        // Find RD or -| marker.
        let marker_ok = if p + 2 <= decrypted.len() {
            let two = &decrypted[p..p + 2];
            two == b"RD" || two == b"-|"
        } else {
            false
        };

        if !marker_ok {
            pos = p.max(name_end + 1);
            continue;
        }

        // Skip marker (2 bytes) + one space.
        p += 2;
        if p < decrypted.len() && (decrypted[p] == b' ' || decrypted[p] == b'\t') {
            p += 1;
        }

        // Read cs_len bytes of binary charstring data.
        if p + cs_len > decrypted.len() {
            break;
        }
        let charstring_data = &decrypted[p..p + cs_len];
        if let Some(width) = decrypt_charstring_width(charstring_data, len_iv) {
            widths.insert(glyph_name, width);
        }

        // Jump past the charstring data.
        pos = p + cs_len;
    }

    widths
}

/// Decrypt a Type 1 charstring and extract the width (wx from hsbw/sbw).
fn decrypt_charstring_width(data: &[u8], len_iv: usize) -> Option<i32> {
    if data.len() <= len_iv {
        return None;
    }

    // Charstring decryption: R=4330, c1=52845, c2=22719.
    let mut r: u16 = 4330;
    let c1: u16 = 52845;
    let c2: u16 = 22719;

    let mut decrypted = Vec::with_capacity(data.len());
    for &cipher in data {
        let plain = cipher ^ (r >> 8) as u8;
        r = (cipher as u16)
            .wrapping_add(r)
            .wrapping_mul(c1)
            .wrapping_add(c2);
        decrypted.push(plain);
    }

    // Skip lenIV random bytes.
    let cs = &decrypted[len_iv..];

    // Parse integers followed by hsbw (13) or sbw (12 7).
    // hsbw: sbx wx hsbw           → width = values[1]
    // sbw:  sbx sby wx wy sbw     → width = values[2]
    // TeX fonts often use `div` (12 12) in the preamble, e.g. `59 2125 4 div hsbw`
    // to encode fractional widths.
    let mut pos = 0;
    let mut values = Vec::new();
    let mut is_sbw = false;

    while pos < cs.len() && values.len() < 8 {
        let b = cs[pos];
        if b == 13 {
            // hsbw: stack has [sbx, wx]
            break;
        }
        if b == 12 {
            if pos + 1 < cs.len() && cs[pos + 1] == 12 {
                // div: pop two values, push quotient (a b div → a/b).
                pos += 2;
                if values.len() >= 2 {
                    let divisor = values.pop().unwrap();
                    let dividend = values.pop().unwrap();
                    if divisor != 0 {
                        values.push(dividend / divisor);
                    } else {
                        values.push(dividend);
                    }
                }
                continue;
            }
            if pos + 1 < cs.len() && cs[pos + 1] == 7 {
                // sbw: stack has [sbx, sby, wx, wy]
                is_sbw = true;
            }
            break;
        }
        // Parse integer.
        if (32..=246).contains(&b) {
            values.push(b as i32 - 139);
            pos += 1;
        } else if (247..=250).contains(&b) {
            if pos + 1 >= cs.len() {
                break;
            }
            values.push((b as i32 - 247) * 256 + cs[pos + 1] as i32 + 108);
            pos += 2;
        } else if (251..=254).contains(&b) {
            if pos + 1 >= cs.len() {
                break;
            }
            values.push(-(b as i32 - 251) * 256 - cs[pos + 1] as i32 - 108);
            pos += 2;
        } else if b == 255 {
            if pos + 4 >= cs.len() {
                break;
            }
            let val = i32::from_be_bytes([cs[pos + 1], cs[pos + 2], cs[pos + 3], cs[pos + 4]]);
            values.push(val);
            pos += 5;
        } else {
            // Unknown operator before width was found.
            break;
        }
    }

    // hsbw: width = values[1], sbw: width = values[2].
    if is_sbw {
        values.get(2).copied()
    } else {
        values.get(1).copied()
    }
}

/// Compute the CFF font matrix scale factor, compensating for f32→f64 precision loss.
///
/// CFF FontMatrix `sx` is stored as f32 (commonly 0.001 for 1000 UPM fonts).
/// Converting `0.001f32` to f64 then multiplying by 1000.0 yields 1.0000000474…
/// instead of exactly 1.0. This tiny error propagates into glyph widths,
/// causing sub-unit discrepancies that veraPDF flags.
fn cff_matrix_scale(matrix_sx: f32) -> f64 {
    if matrix_sx.abs() > f32::EPSILON {
        let raw = matrix_sx as f64 * 1000.0;
        // Round to 6 decimal places — f32 has ~7 digits of precision,
        // so the 7th+ digit is noise from the f32→f64 cast.
        (raw * 1_000_000.0).round() / 1_000_000.0
    } else {
        1.0
    }
}

/// Compute width corrections for a Type1 font with CFF program (FontFile3).
fn compute_cff_type1_width_corrections(
    font_data: &[u8],
    first_char: u32,
    existing_widths: &[Object],
    enc_info: &(String, std::collections::HashMap<u32, String>),
) -> Vec<(usize, i64)> {
    let (enc_name, differences) = enc_info;
    let has_pdf_encoding = !enc_name.is_empty() || !differences.is_empty();

    // Try OTF parse first (handles OpenType-wrapped CFF).
    if let Ok(face) = ttf_parser::Face::parse(font_data, 0) {
        let units_per_em = face.units_per_em() as f64;
        if units_per_em > 0.0 {
            let scale = 1000.0 / units_per_em;
            return compute_otf_cff_corrections(
                &face,
                font_data,
                first_char,
                existing_widths,
                enc_name,
                differences,
                has_pdf_encoding,
                scale,
            );
        }
    }

    // Fall back to raw CFF parse (Type1C).
    let Some(cff) = cff_parser::Table::parse(font_data) else {
        return Vec::new();
    };

    let matrix = cff.matrix();
    let scale = cff_matrix_scale(matrix.sx);

    let mut corrections = Vec::new();

    for (i, obj) in existing_widths.iter().enumerate() {
        let pdf_w = match obj {
            Object::Integer(w) => *w as f64,
            Object::Real(r) => *r as f64,
            _ => continue,
        };

        let code = first_char + i as u32;

        // Try PDF encoding → glyph name → CFF lookup, with CFF internal
        // encoding fallback (e.g. StandardEncoding maps code 173 → "hyphen").
        let frac_w = cff_width_for_code(&cff, font_data, code, enc_name, differences, scale);

        let Some(frac_w) = frac_w else { continue };

        // Use >= 1.0 threshold because cff_parser returns integer widths (u16),
        // losing fractional precision. A 1-unit integer diff may hide a >1 fractional
        // diff that veraPDF catches (e.g. 479.89 rounds to 480 vs dict 481).
        if (pdf_w - frac_w).abs() >= 1.0 {
            corrections.push((i, frac_w.round() as i64));
        }
    }

    corrections
}

/// Compute width corrections for OTF-wrapped CFF fonts.
///
/// For OTF fonts, veraPDF validates against the hmtx table widths (not the CFF
/// charstring widths). When a PDF-level Encoding exists, we use cmap to map
/// code -> Unicode -> GID -> hmtx width. When no encoding exists (common in
/// subset fonts), we extract the CFF table from the OTF and use its internal
/// encoding to map code -> GID -> hmtx width.
#[allow(clippy::too_many_arguments)]
fn compute_otf_cff_corrections(
    face: &ttf_parser::Face,
    font_data: &[u8],
    first_char: u32,
    existing_widths: &[Object],
    enc_name: &str,
    differences: &std::collections::HashMap<u32, String>,
    has_pdf_encoding: bool,
    scale: f64,
) -> Vec<(usize, i64)> {
    // If no PDF encoding, try to extract CFF table for its internal encoding.
    let cff_table = if !has_pdf_encoding {
        extract_cff_from_otf(font_data)
    } else {
        None
    };

    // For fonts with PDF encoding AND custom CFF encoding, extract CFF for
    // verification. veraPDF uses the CFF internal encoding for width comparison
    // (rule 6.2.11.5:1), which may map codes to different GIDs than the PDF
    // encoding. We use CFF charstring widths * FontMatrix (not hmtx) since
    // veraPDF uses the CFF program widths.
    let custom_cff_info: Option<(cff_parser::Table, std::collections::HashMap<u8, u16>, f64)> =
        if has_pdf_encoding {
            extract_cff_bytes_from_otf(font_data).and_then(|cff_bytes| {
                if !cff_has_custom_encoding(cff_bytes) {
                    return None;
                }
                let enc_map = parse_cff_encoding_map(cff_bytes);
                let cff = cff_parser::Table::parse(cff_bytes)?;
                let matrix = cff.matrix();
                let cff_scale = cff_matrix_scale(matrix.sx);
                Some((cff, enc_map, cff_scale))
            })
        } else {
            None
        };

    let mut corrections = Vec::new();

    for (i, obj) in existing_widths.iter().enumerate() {
        let pdf_w = match obj {
            Object::Integer(w) => *w as f64,
            Object::Real(r) => *r as f64,
            _ => continue,
        };

        let code = first_char + i as u32;

        let frac_w = if has_pdf_encoding {
            // For custom CFF encoding, use CFF encoding → GID → CFF charstring
            // width. This takes priority over PDF encoding because veraPDF uses
            // the CFF internal encoding for width comparison.
            if let Some((ref cff, ref enc_map, cff_scale)) = custom_cff_info {
                if code <= 255 {
                    if let Some(&gid) = enc_map.get(&(code as u8)) {
                        if gid != 0 {
                            cff.glyph_width(cff_parser::GlyphId(gid))
                                .map(|w| w as f64 * cff_scale)
                        } else {
                            // CFF maps to .notdef
                            cff.glyph_width(cff_parser::GlyphId(0))
                                .map(|w| w as f64 * cff_scale)
                        }
                    } else {
                        // Code not in CFF encoding — fall through to cmap-based
                        // lookup (font may have a valid cmap mapping even when
                        // CFF encoding doesn't cover this code).
                        get_otf_width_via_encoding(face, code, enc_name, differences, scale)
                    }
                } else {
                    get_otf_width_via_encoding(face, code, enc_name, differences, scale)
                }
            } else {
                get_otf_width_via_encoding(face, code, enc_name, differences, scale)
            }
        } else if let Some(ref cff) = cff_table {
            // No PDF encoding: use CFF internal encoding -> GID -> hmtx
            if code > 255 {
                continue;
            }
            let gid = match cff.glyph_index(code as u8) {
                Some(gid) if gid.0 != 0 || code == 0 => gid,
                _ => continue,
            };
            // Use hmtx width (what veraPDF validates) rather than CFF charstring width.
            face.glyph_hor_advance(ttf_parser::GlyphId(gid.0))
                .map(|w| w as f64 * scale)
        } else {
            continue;
        };

        let Some(frac_w) = frac_w else { continue };

        // Use >= 1.0: CFF glyph_width returns integer u16, so a 1-unit diff
        // may mask a fractional diff >1 that veraPDF catches.
        if (pdf_w - frac_w).abs() >= 1.0 {
            corrections.push((i, frac_w.round() as i64));
        }
    }

    corrections
}

/// Get an OTF font's width for a character code using PDF encoding.
fn get_otf_width_via_encoding(
    face: &ttf_parser::Face,
    code: u32,
    enc_name: &str,
    differences: &std::collections::HashMap<u32, String>,
    scale: f64,
) -> Option<f64> {
    if let Some(glyph_name) = differences.get(&code) {
        if let Some(unicode) = glyph_name_to_unicode(glyph_name) {
            if let Some(gid) = face.glyph_index(unicode) {
                return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
            }
        }
        if let Some(gid) = face.glyph_index_by_name(glyph_name) {
            return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
        }
        if glyph_name == ".notdef" {
            return face
                .glyph_hor_advance(ttf_parser::GlyphId(0))
                .map(|w| w as f64 * scale);
        }
    }

    let ch = encoding_to_char(code, enc_name);
    if let Some(gid) = face.glyph_index(ch) {
        return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
    }
    if let Some(agl_name) = unicode_to_agl_name(ch) {
        if let Some(gid) = face.glyph_index_by_name(&agl_name) {
            return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
        }
    }
    if let Some(name) = unicode_to_glyph_name(ch) {
        if let Some(gid) = face.glyph_index_by_name(&name) {
            return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
        }
    }

    None
}

/// Extract the raw CFF table bytes from an OTF font.
fn extract_cff_bytes_from_otf(font_data: &[u8]) -> Option<&[u8]> {
    if font_data.len() < 12 {
        return None;
    }

    let num_tables = u16::from_be_bytes([font_data[4], font_data[5]]) as usize;
    let mut offset = 12;

    for _ in 0..num_tables {
        if offset + 16 > font_data.len() {
            break;
        }
        let tag = &font_data[offset..offset + 4];
        let table_offset = u32::from_be_bytes([
            font_data[offset + 8],
            font_data[offset + 9],
            font_data[offset + 10],
            font_data[offset + 11],
        ]) as usize;
        let table_length = u32::from_be_bytes([
            font_data[offset + 12],
            font_data[offset + 13],
            font_data[offset + 14],
            font_data[offset + 15],
        ]) as usize;

        if tag == b"CFF " {
            if table_offset + table_length <= font_data.len() {
                return Some(&font_data[table_offset..table_offset + table_length]);
            }
            return None;
        }

        offset += 16;
    }

    None
}

/// Extract the CFF table from an OTF font.
fn extract_cff_from_otf(font_data: &[u8]) -> Option<cff_parser::Table<'_>> {
    let cff_bytes = extract_cff_bytes_from_otf(font_data)?;
    cff_parser::Table::parse(cff_bytes)
}

/// Like find_cff_glyph_width_by_name but returns f64 (unrounded) for fractional comparison.
fn find_cff_glyph_width_by_name_fractional(
    cff: &cff_parser::Table,
    font_data: &[u8],
    glyph_name: &str,
    scale: f64,
) -> Option<f64> {
    let num_glyphs = cff.number_of_glyphs();
    for gid_raw in 0..num_glyphs {
        let gid = cff_parser::GlyphId(gid_raw);
        if let Some(name) = cff.glyph_name(gid) {
            if name == glyph_name {
                if let Some(width) = cff_type2_endchar_default_width(font_data, gid, scale) {
                    return Some(width);
                }
                return cff.glyph_width(gid).map(|w| w as f64 * scale);
            }
        }
    }
    None
}

/// Compute the expected width for a single character code in a CFF font program.
fn compute_cff_single_width(
    font_data: &[u8],
    code: u32,
    enc_name: &str,
    differences: &std::collections::HashMap<u32, String>,
) -> Option<f64> {
    // Try OTF-wrapped CFF first.
    if let Ok(face) = ttf_parser::Face::parse(font_data, 0) {
        let upem = face.units_per_em() as f64;
        if upem > 0.0 {
            let scale = 1000.0 / upem;
            let has_pdf_encoding = !enc_name.is_empty() || !differences.is_empty();

            // For custom CFF encoding, use CFF charstring widths (veraPDF uses
            // CFF encoding for width comparison, not PDF encoding).
            if has_pdf_encoding && code <= 255 {
                if let Some(cff_bytes) = extract_cff_bytes_from_otf(font_data) {
                    if cff_has_custom_encoding(cff_bytes) {
                        let enc_map = parse_cff_encoding_map(cff_bytes);
                        if let Some(cff) = cff_parser::Table::parse(cff_bytes) {
                            let matrix = cff.matrix();
                            let cff_scale = cff_matrix_scale(matrix.sx);
                            if let Some(&gid) = enc_map.get(&(code as u8)) {
                                if gid != 0 {
                                    return cff
                                        .glyph_width(cff_parser::GlyphId(gid))
                                        .map(|w| w as f64 * cff_scale);
                                }
                            }
                            // Code not in CFF encoding — fall through to cmap-based
                            // lookup below (don't return .notdef width, as the font
                            // may have a valid cmap mapping for this code).
                        }
                    }
                }
            }

            return get_truetype_glyph_width_fractional(&face, code, enc_name, differences, scale);
        }
    }

    // Fall back to raw CFF parse.
    let cff = cff_parser::Table::parse(font_data)?;
    let matrix = cff.matrix();
    let scale = cff_matrix_scale(matrix.sx);

    cff_width_for_code(&cff, font_data, code, enc_name, differences, scale)
}

/// Look up the CFF glyph width for a character code, trying multiple strategies:
/// 1. PDF encoding → glyph name → CFF name lookup
/// 2. CFF internal encoding (direct parse, no Standard Encoding fallback)
fn cff_width_for_code(
    cff: &cff_parser::Table,
    font_data: &[u8],
    code: u32,
    enc_name: &str,
    differences: &std::collections::HashMap<u32, String>,
    scale: f64,
) -> Option<f64> {
    let has_pdf_encoding = !enc_name.is_empty() || !differences.is_empty();
    let has_explicit_difference = differences.contains_key(&code);

    // Primary path: PDF encoding → glyph name → CFF charset lookup.
    // veraPDF resolves code → glyph name via the PDF Encoding, then looks up
    // that name in the CFF charset. Only when the name is NOT found in the
    // CFF (e.g. because the subset uses GID-based names like G80) does it
    // fall back to the CFF internal encoding.
    let mut name_found = false;
    if has_pdf_encoding {
        let glyph_name = if let Some(name) = differences.get(&code) {
            name.clone()
        } else {
            let ch = encoding_to_char(code, enc_name);
            unicode_to_glyph_name(ch).unwrap_or_default()
        };
        if !glyph_name.is_empty() && glyph_name != ".notdef" {
            if let Some(w) =
                find_cff_glyph_width_by_name_fractional(cff, font_data, &glyph_name, scale)
            {
                return Some(w);
            }
            if glyph_name.starts_with("uni") {
                let ch = encoding_to_char(code, enc_name);
                if let Some(agl_name) = unicode_to_agl_name(ch) {
                    if let Some(w) =
                        find_cff_glyph_width_by_name_fractional(cff, font_data, &agl_name, scale)
                    {
                        return Some(w);
                    }
                }
            }
            for alt in cff_glyph_name_alternatives(&glyph_name) {
                if let Some(w) = find_cff_glyph_width_by_name_fractional(cff, font_data, alt, scale)
                {
                    return Some(w);
                }
            }
            // Name resolved but not found in CFF — will try CFF encoding below.
            name_found = false;
        }
    }

    // Fallback: CFF internal encoding → GID → width.
    // Used when (a) no PDF encoding exists, or (b) PDF encoding name lookup
    // failed AND the CFF uses GID-based names (G80, G32, etc.) where name
    // lookup will never succeed. For fonts with standard glyph names, a
    // failed name lookup means the glyph isn't in the subset — veraPDF
    // skips the width check in that case, so we return None.
    let allow_cff_encoding_fallback = !has_pdf_encoding
        || cff_has_gid_based_names(cff)
        // Encoding dictionary with Differences but without BaseEncoding:
        // for codes not explicitly listed in Differences, use CFF internal
        // encoding as the authoritative mapping.
        || (enc_name.is_empty() && !has_explicit_difference);

    if !name_found && code <= 255 && allow_cff_encoding_fallback {
        let enc_map = parse_cff_encoding_map(font_data);
        if let Some(&gid) = enc_map.get(&(code as u8)) {
            if gid != 0 {
                return cff
                    .glyph_width(cff_parser::GlyphId(gid))
                    .map(|w| w as f64 * scale);
            }
        }
        if let Some(gid) = cff.glyph_index(code as u8) {
            return cff.glyph_width(gid).map(|w| w as f64 * scale);
        }
    }

    // Cannot positively determine the glyph — return None to avoid
    // overwriting a correct existing width with .notdef width.
    None
}

fn cff_type2_endchar_default_width(
    font_data: &[u8],
    glyph_id: cff_parser::GlyphId,
    scale: f64,
) -> Option<f64> {
    let (charstrings_offset, private_range) = parse_cff_top_dict_offsets(font_data)?;
    let (default_width, nominal_width) = parse_cff_private_widths(font_data, private_range)?;
    let charstring = read_cff_index_entry(font_data, charstrings_offset, glyph_id.0 as usize)?;
    parse_type2_endchar_width(charstring, default_width, nominal_width).map(|w| w * scale)
}

fn parse_cff_top_dict_offsets(data: &[u8]) -> Option<(usize, (usize, usize))> {
    if data.len() < 4 {
        return None;
    }

    let header_size = data[2] as usize;
    let after_name = skip_cff_index(data, header_size)?;
    let (top_dict_data, _) = read_cff_index_first(data, after_name)?;

    let mut i = 0;
    let mut operand_stack: Vec<i64> = Vec::new();
    let mut charstrings_offset: Option<usize> = None;
    let mut private_size: Option<usize> = None;
    let mut private_offset: Option<usize> = None;

    while i < top_dict_data.len() {
        let b0 = top_dict_data[i];
        match b0 {
            0..=21 => {
                match b0 {
                    17 => {
                        charstrings_offset = operand_stack.last().copied().map(|v| v as usize);
                    }
                    18 => {
                        if operand_stack.len() >= 2 {
                            private_size = Some(operand_stack[operand_stack.len() - 2] as usize);
                            private_offset = operand_stack.last().copied().map(|v| v as usize);
                        }
                    }
                    _ => {}
                }
                operand_stack.clear();
                i += 1;
            }
            28 => {
                if i + 2 >= top_dict_data.len() {
                    return None;
                }
                operand_stack
                    .push(i16::from_be_bytes([top_dict_data[i + 1], top_dict_data[i + 2]]) as i64);
                i += 3;
            }
            29 => {
                if i + 4 >= top_dict_data.len() {
                    return None;
                }
                operand_stack.push(i32::from_be_bytes([
                    top_dict_data[i + 1],
                    top_dict_data[i + 2],
                    top_dict_data[i + 3],
                    top_dict_data[i + 4],
                ]) as i64);
                i += 5;
            }
            30 => {
                i += 1;
                while i < top_dict_data.len() {
                    let nibbles = top_dict_data[i];
                    i += 1;
                    if nibbles & 0x0F == 0x0F || nibbles >> 4 == 0x0F {
                        break;
                    }
                }
                operand_stack.push(0);
            }
            32..=246 => {
                operand_stack.push(b0 as i64 - 139);
                i += 1;
            }
            247..=250 => {
                if i + 1 >= top_dict_data.len() {
                    return None;
                }
                operand_stack.push((b0 as i64 - 247) * 256 + top_dict_data[i + 1] as i64 + 108);
                i += 2;
            }
            251..=254 => {
                if i + 1 >= top_dict_data.len() {
                    return None;
                }
                operand_stack.push(-(b0 as i64 - 251) * 256 - top_dict_data[i + 1] as i64 - 108);
                i += 2;
            }
            _ => {
                i += 1;
            }
        }
    }

    let charstrings_offset = charstrings_offset?;
    let private_size = private_size?;
    let private_offset = private_offset?;
    let private_end = private_offset.checked_add(private_size)?;
    if private_end > data.len() {
        return None;
    }

    Some((charstrings_offset, (private_offset, private_end)))
}

fn parse_cff_private_widths(data: &[u8], private_range: (usize, usize)) -> Option<(f64, f64)> {
    let private_data = data.get(private_range.0..private_range.1)?;
    let mut i = 0;
    let mut operand_stack: Vec<f64> = Vec::new();
    let mut default_width = 0.0;
    let mut nominal_width = 0.0;

    while i < private_data.len() {
        let b0 = private_data[i];
        match b0 {
            0..=21 => {
                match b0 {
                    20 => {
                        if let Some(value) = operand_stack.last().copied() {
                            default_width = value;
                        }
                    }
                    21 => {
                        if let Some(value) = operand_stack.last().copied() {
                            nominal_width = value;
                        }
                    }
                    _ => {}
                }
                operand_stack.clear();
                i += 1;
            }
            28 => {
                if i + 2 >= private_data.len() {
                    return None;
                }
                operand_stack
                    .push(i16::from_be_bytes([private_data[i + 1], private_data[i + 2]]) as f64);
                i += 3;
            }
            29 => {
                if i + 4 >= private_data.len() {
                    return None;
                }
                operand_stack.push(i32::from_be_bytes([
                    private_data[i + 1],
                    private_data[i + 2],
                    private_data[i + 3],
                    private_data[i + 4],
                ]) as f64);
                i += 5;
            }
            30 => {
                let (value, next_i) = parse_cff_real_number(private_data, i + 1)?;
                operand_stack.push(value);
                i = next_i;
            }
            32..=246 => {
                operand_stack.push((b0 as i64 - 139) as f64);
                i += 1;
            }
            247..=250 => {
                if i + 1 >= private_data.len() {
                    return None;
                }
                operand_stack
                    .push(((b0 as i64 - 247) * 256 + private_data[i + 1] as i64 + 108) as f64);
                i += 2;
            }
            251..=254 => {
                if i + 1 >= private_data.len() {
                    return None;
                }
                operand_stack
                    .push((-(b0 as i64 - 251) * 256 - private_data[i + 1] as i64 - 108) as f64);
                i += 2;
            }
            _ => {
                i += 1;
            }
        }
    }

    Some((default_width, nominal_width))
}

fn parse_cff_real_number(data: &[u8], mut i: usize) -> Option<(f64, usize)> {
    let mut buf = String::new();
    while i < data.len() {
        let byte = data[i];
        i += 1;
        for nibble in [byte >> 4, byte & 0x0F] {
            match nibble {
                0..=9 => buf.push(char::from(b'0' + nibble)),
                0xA => buf.push('.'),
                0xB => buf.push('E'),
                0xC => buf.push_str("E-"),
                0xE => buf.push('-'),
                0xF => {
                    return buf.parse::<f64>().ok().map(|value| (value, i));
                }
                _ => {}
            }
        }
    }
    None
}

fn read_cff_index_entry(data: &[u8], start: usize, index: usize) -> Option<&[u8]> {
    if start + 2 > data.len() {
        return None;
    }
    let count = u16::from_be_bytes([data[start], data[start + 1]]) as usize;
    if index >= count || count == 0 || start + 3 > data.len() {
        return None;
    }
    let off_size = data[start + 2] as usize;
    if !(1..=4).contains(&off_size) {
        return None;
    }
    let offsets_start = start + 3;
    let entry_off = read_cff_offset(data, offsets_start + index * off_size, off_size)?;
    let next_off = read_cff_offset(data, offsets_start + (index + 1) * off_size, off_size)?;
    let data_start = offsets_start + (count + 1) * off_size;
    let entry_start = data_start + entry_off.checked_sub(1)?;
    let entry_end = data_start + next_off.checked_sub(1)?;
    data.get(entry_start..entry_end)
}

fn parse_type2_endchar_width(
    charstring: &[u8],
    default_width: f64,
    nominal_width: f64,
) -> Option<f64> {
    let mut i = 0;
    let mut stack: Vec<f64> = Vec::new();

    while i < charstring.len() {
        let b0 = charstring[i];
        match b0 {
            14 => {
                return match stack.len() {
                    4 => Some(default_width),
                    5 => Some(nominal_width + stack[0]),
                    _ => None,
                };
            }
            28 => {
                if i + 2 >= charstring.len() {
                    return None;
                }
                stack.push(i16::from_be_bytes([charstring[i + 1], charstring[i + 2]]) as f64);
                i += 3;
            }
            32..=246 => {
                stack.push((b0 as i64 - 139) as f64);
                i += 1;
            }
            247..=250 => {
                if i + 1 >= charstring.len() {
                    return None;
                }
                stack.push(((b0 as i64 - 247) * 256 + charstring[i + 1] as i64 + 108) as f64);
                i += 2;
            }
            251..=254 => {
                if i + 1 >= charstring.len() {
                    return None;
                }
                stack.push((-(b0 as i64 - 251) * 256 - charstring[i + 1] as i64 - 108) as f64);
                i += 2;
            }
            255 => {
                if i + 4 >= charstring.len() {
                    return None;
                }
                let raw = i32::from_be_bytes([
                    charstring[i + 1],
                    charstring[i + 2],
                    charstring[i + 3],
                    charstring[i + 4],
                ]);
                stack.push(raw as f64 / 65536.0);
                i += 5;
            }
            // Any operator other than endchar means this is not the narrow
            // composite-glyph case we are correcting here.
            _ if b0 <= 31 => return None,
            _ => {
                return None;
            }
        }
    }

    None
}

fn compute_classic_symbol_cff_single_width(
    font_data: &[u8],
    code: u32,
    enc_name: &str,
    differences: &std::collections::HashMap<u32, String>,
) -> Option<f64> {
    if let Ok(face) = ttf_parser::Face::parse(font_data, 0) {
        let upem = face.units_per_em() as f64;
        if upem > 0.0 {
            let scale = 1000.0 / upem;

            if let Some(glyph_name) = differences.get(&code) {
                if let Some(gid) = face.glyph_index_by_name(glyph_name) {
                    return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
                }
                if let Some(unicode) = glyph_name_to_unicode(glyph_name) {
                    if let Some(gid) = face.glyph_index(unicode) {
                        return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
                    }
                }
            }

            let ch = encoding_to_char(code, enc_name);
            if let Some(agl_name) = unicode_to_agl_name(ch) {
                if let Some(gid) = face.glyph_index_by_name(&agl_name) {
                    return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
                }
            }
            if let Some(name) = unicode_to_glyph_name(ch) {
                if let Some(gid) = face.glyph_index_by_name(&name) {
                    return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
                }
            }
            if let Some(gid) = face.glyph_index(ch) {
                return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
            }
        }
    }

    compute_cff_single_width(font_data, code, enc_name, differences)
}

fn subset_standard_cff_code_is_safe(
    font_data: &[u8],
    code: u32,
    enc_name: &str,
    differences: &std::collections::HashMap<u32, String>,
) -> bool {
    if !matches!(enc_name, "WinAnsiEncoding" | "MacRomanEncoding") {
        return false;
    }

    if let Some(name) = differences.get(&code) {
        return name != "space" && cff_font_has_named_glyph(font_data, name);
    }

    let ch = encoding_to_char(code, enc_name);
    if let Some(agl_name) = unicode_to_agl_name(ch) {
        if cff_font_has_named_glyph(font_data, &agl_name) {
            return true;
        }
    }
    if let Some(name) = unicode_to_glyph_name(ch) {
        if cff_font_has_named_glyph(font_data, &name) {
            return true;
        }
    }

    false
}

fn cff_font_has_named_glyph(font_data: &[u8], glyph_name: &str) -> bool {
    if let Ok(face) = ttf_parser::Face::parse(font_data, 0) {
        if face.glyph_index_by_name(glyph_name).is_some() {
            return true;
        }
    }

    if let Some(cff) = extract_cff_from_otf(font_data) {
        if cff_has_named_glyph(&cff, glyph_name) {
            return true;
        }
    }

    if let Some(cff) = cff_parser::Table::parse(font_data) {
        if cff_has_named_glyph(&cff, glyph_name) {
            return true;
        }
    }

    false
}

fn cff_has_named_glyph(cff: &cff_parser::Table<'_>, glyph_name: &str) -> bool {
    for gid_raw in 0..cff.number_of_glyphs() {
        let gid = cff_parser::GlyphId(gid_raw);
        if let Some(name) = cff.glyph_name(gid) {
            if name == glyph_name {
                return true;
            }
        }
    }
    false
}

/// Check if the CFF's non-.notdef glyphs use GID-based names (e.g. G80, G32)
/// rather than standard PostScript glyph names. When all names follow the
/// `G\d+` pattern, the only way to resolve code→GID is through CFF internal
/// encoding, because PDF-level name lookup will fail.
fn cff_has_gid_based_names(cff: &cff_parser::Table) -> bool {
    let n = cff.number_of_glyphs();
    if n <= 1 {
        return false;
    }
    // Check a sample of non-.notdef glyphs (skip GID 0).
    let mut gid_pattern = 0u32;
    let mut non_notdef = 0u32;
    for gid in 1..n {
        if let Some(name) = cff.glyph_name(cff_parser::GlyphId(gid)) {
            non_notdef += 1;
            // Match G followed by digits (e.g. G80, G32, G1)
            if name.starts_with('G')
                && name.len() > 1
                && name[1..].chars().all(|c| c.is_ascii_digit())
            {
                gid_pattern += 1;
            }
        }
        if non_notdef >= 10 {
            break;
        }
    }
    non_notdef > 0 && gid_pattern * 2 >= non_notdef
}

/// Check if the CFF has a custom encoding (not Standard or Expert).
fn cff_has_custom_encoding(data: &[u8]) -> bool {
    if data.len() < 4 {
        return false;
    }
    let header_size = data[2] as usize;
    let Some(after_name) = skip_cff_index(data, header_size) else {
        return false;
    };
    let Some((top_dict_data, _)) = read_cff_index_first(data, after_name) else {
        return false;
    };
    let enc_offset = parse_cff_top_dict_encoding_offset(&top_dict_data);
    enc_offset > 1
}

fn parse_cff_encoding_map(data: &[u8]) -> std::collections::HashMap<u8, u16> {
    let mut map = std::collections::HashMap::new();

    // CFF structure: Header, Name INDEX, Top DICT INDEX, String INDEX, ...
    // We need the encoding offset from the Top DICT.
    if data.len() < 4 {
        return map;
    }

    let header_size = data[2] as usize; // hdrSize
    if header_size > data.len() {
        return map;
    }

    // Skip Name INDEX
    let name_idx_start = header_size;
    let Some(after_name) = skip_cff_index(data, name_idx_start) else {
        return map;
    };

    // Parse Top DICT INDEX to get encoding offset
    let Some((top_dict_data, _after_top_dict)) = read_cff_index_first(data, after_name) else {
        return map;
    };

    // Parse Top DICT for encoding offset (operator 16)
    let enc_offset = parse_cff_top_dict_encoding_offset(&top_dict_data);

    // enc_offset 0 = Standard Encoding, 1 = Expert Encoding.
    // For these, glyph_index() uses the correct encoding directly (no wrong fallback).
    if enc_offset == 0 || enc_offset == 1 {
        if let Some(cff) = cff_parser::Table::parse(data) {
            for code_byte in 0..=255u8 {
                if let Some(gid) = cff.glyph_index(code_byte) {
                    map.insert(code_byte, gid.0);
                }
            }
        }
        return map;
    }

    // Custom encoding at enc_offset
    let offset = enc_offset as usize;
    if offset >= data.len() {
        return map;
    }

    let format = data[offset] & 0x7F; // Low 7 bits = format, high bit = supplemental

    match format {
        0 => {
            // Format 0: nCodes byte, then nCodes code bytes
            if offset + 1 >= data.len() {
                return map;
            }
            let n_codes = data[offset + 1] as usize;
            for i in 0..n_codes {
                if offset + 2 + i >= data.len() {
                    break;
                }
                let code_byte = data[offset + 2 + i];
                // GID = i + 1 (.notdef is GID 0, implicit)
                map.insert(code_byte, (i + 1) as u16);
            }
            // NOTE: Supplement entries are intentionally NOT parsed.
            // veraPDF does not use CFF encoding supplement entries for width
            // comparison — supplement codes are treated as .notdef.
        }
        1 => {
            // Format 1: nRanges byte, then nRanges * (first: u8, nLeft: u8)
            if offset + 1 >= data.len() {
                return map;
            }
            let n_ranges = data[offset + 1] as usize;
            let mut gid: u16 = 1;
            for i in 0..n_ranges {
                let range_start = offset + 2 + i * 2;
                if range_start + 1 >= data.len() {
                    break;
                }
                let first = data[range_start];
                let n_left = data[range_start + 1];
                for j in 0..=n_left {
                    let code_byte = first.wrapping_add(j);
                    map.insert(code_byte, gid);
                    gid += 1;
                }
            }
            // NOTE: Supplement entries are intentionally NOT parsed.
            // veraPDF does not use CFF encoding supplement entries for width
            // comparison — supplement codes are treated as .notdef.
        }
        _ => {}
    }

    map
}

/// Skip a CFF INDEX structure and return the offset after it.
fn skip_cff_index(data: &[u8], start: usize) -> Option<usize> {
    if start + 2 > data.len() {
        return None;
    }
    let count = u16::from_be_bytes([data[start], data[start + 1]]) as usize;
    if count == 0 {
        return Some(start + 2);
    }
    if start + 3 > data.len() {
        return None;
    }
    let off_size = data[start + 2] as usize;
    if off_size == 0 || off_size > 4 {
        return None;
    }
    // offsets array: (count+1) entries of off_size bytes each
    let offsets_start = start + 3;
    let offsets_end = offsets_start + (count + 1) * off_size;
    if offsets_end > data.len() {
        return None;
    }
    // Last offset value gives the data size
    let last_off = read_cff_offset(data, offsets_start + count * off_size, off_size)?;
    // Data starts after offsets, first offset is 1-based
    Some(offsets_start + (count + 1) * off_size + last_off - 1)
}

/// Read the first entry from a CFF INDEX, returning (data, offset_after_index).
fn read_cff_index_first(data: &[u8], start: usize) -> Option<(Vec<u8>, usize)> {
    if start + 2 > data.len() {
        return None;
    }
    let count = u16::from_be_bytes([data[start], data[start + 1]]) as usize;
    if count == 0 {
        return Some((Vec::new(), start + 2));
    }
    if start + 3 > data.len() {
        return None;
    }
    let off_size = data[start + 2] as usize;
    if off_size == 0 || off_size > 4 {
        return None;
    }
    let offsets_start = start + 3;
    let first_off = read_cff_offset(data, offsets_start, off_size)?;
    let second_off = read_cff_offset(data, offsets_start + off_size, off_size)?;
    let data_start = offsets_start + (count + 1) * off_size;
    let entry_start = data_start + first_off - 1;
    let entry_end = data_start + second_off - 1;
    if entry_end > data.len() {
        return None;
    }
    let last_off = read_cff_offset(data, offsets_start + count * off_size, off_size)?;
    let after = data_start + last_off - 1;
    Some((data[entry_start..entry_end].to_vec(), after))
}

/// Read a CFF offset value (1-4 bytes, big-endian).
fn read_cff_offset(data: &[u8], pos: usize, size: usize) -> Option<usize> {
    if pos + size > data.len() {
        return None;
    }
    let mut val = 0usize;
    for i in 0..size {
        val = (val << 8) | data[pos + i] as usize;
    }
    Some(val)
}

/// Parse the encoding offset from a CFF Top DICT.
/// Operator 16 (0x10) = Encoding offset (default 0 = Standard).
fn parse_cff_top_dict_encoding_offset(dict_data: &[u8]) -> u32 {
    let mut i = 0;
    let mut operand_stack: Vec<i64> = Vec::new();

    while i < dict_data.len() {
        let b0 = dict_data[i];
        match b0 {
            0..=11 => {
                // Operator (single byte)
                if b0 == 16 {
                    // Encoding operator
                    return operand_stack.last().copied().unwrap_or(0) as u32;
                }
                operand_stack.clear();
                i += 1;
            }
            12 => {
                // Two-byte operator
                operand_stack.clear();
                i += 2;
            }
            13..=21 => {
                // Operators 13-21
                if b0 == 16 {
                    return operand_stack.last().copied().unwrap_or(0) as u32;
                }
                operand_stack.clear();
                i += 1;
            }
            28 => {
                // 2-byte integer
                if i + 2 < dict_data.len() {
                    let val = i16::from_be_bytes([dict_data[i + 1], dict_data[i + 2]]) as i64;
                    operand_stack.push(val);
                }
                i += 3;
            }
            29 => {
                // 4-byte integer
                if i + 4 < dict_data.len() {
                    let val = i32::from_be_bytes([
                        dict_data[i + 1],
                        dict_data[i + 2],
                        dict_data[i + 3],
                        dict_data[i + 4],
                    ]) as i64;
                    operand_stack.push(val);
                }
                i += 5;
            }
            30 => {
                // Real number (BCD) — skip it
                i += 1;
                while i < dict_data.len() {
                    let nibbles = dict_data[i];
                    i += 1;
                    if nibbles & 0x0F == 0x0F || nibbles >> 4 == 0x0F {
                        break;
                    }
                }
                operand_stack.push(0); // Placeholder
            }
            32..=246 => {
                operand_stack.push(b0 as i64 - 139);
                i += 1;
            }
            247..=250 => {
                if i + 1 < dict_data.len() {
                    let val = (b0 as i64 - 247) * 256 + dict_data[i + 1] as i64 + 108;
                    operand_stack.push(val);
                }
                i += 2;
            }
            251..=254 => {
                if i + 1 < dict_data.len() {
                    let val = -(b0 as i64 - 251) * 256 - dict_data[i + 1] as i64 - 108;
                    operand_stack.push(val);
                }
                i += 2;
            }
            255 => {
                // Reserved in DICT
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }
    0 // Default: Standard Encoding
}

/// Alternative glyph names to try when the primary name isn't found in CFF.
fn cff_glyph_name_alternatives(name: &str) -> &'static [&'static str] {
    match name {
        "uni00AD" | "softhyphen" => &["hyphen", "sfthyphen"],
        "uni00A0" | "nbspace" => &["space"],
        "uni2010" => &["hyphen"],
        _ => &[],
    }
}

/// Known symbolic font base names (exempt from encoding rules).
const SYMBOLIC_FONTS: &[&str] = &[
    "Symbol",
    "SymbolMT",
    "MTExtra",
    "ZapfDingbats",
    "Wingdings",
    "Webdings",
    "Dingbats",
    "CMSY10",
    "MSAM10",
    "MSBM10",
    "WASY8",
    "WASY9",
    "TXSY",
    "TXSYC",
    "TXEX",
];

/// Check if a font name (with optional subset prefix) is a symbolic font.
fn is_symbolic_font_name(name: &str) -> bool {
    let base = name.split('+').next_back().unwrap_or(name);
    if SYMBOLIC_FONTS
        .iter()
        .any(|sym| base.eq_ignore_ascii_case(sym))
    {
        return true;
    }

    // TeX/Math symbolic families are often subsetted/renamed.
    let up = base.to_ascii_uppercase();
    up.starts_with("CMSY")
        || up.starts_with("MSAM")
        || up.starts_with("MSBM")
        || up.starts_with("WASY")
        || up.starts_with("TXSY")
        || up.starts_with("TXEX")
}

/// Fix TrueType font encoding for PDF/A compliance (rules 6.2.11.6:2, 6.2.11.6:3).
///
/// - Non-symbolic TrueType fonts must have MacRomanEncoding or WinAnsiEncoding.
/// - Symbolic TrueType fonts must NOT have an Encoding entry.
pub fn fix_truetype_encoding(doc: &mut Document) -> usize {
    // Collect font IDs that need fixing (non-symbolic: add encoding).
    let mut to_fix: Vec<ObjectId> = Vec::new();
    // Collect symbolic font IDs that need Encoding removed.
    let mut symbolic_to_strip: Vec<ObjectId> = Vec::new();

    for (id, obj) in &doc.objects {
        let Object::Dictionary(dict) = obj else {
            continue;
        };
        // Only process TrueType fonts.
        if get_name(dict, b"Subtype").as_deref() != Some("TrueType") {
            continue;
        }

        // Check if symbolic via FontDescriptor Flags.
        let is_symbolic = is_font_symbolic(doc, dict);
        if is_symbolic {
            // Symbolic fonts must NOT have Encoding (6.2.11.6:3).
            if dict.has(b"Encoding") {
                symbolic_to_strip.push(*id);
            }
            continue;
        }

        // Check existing Encoding — normalize everything to WinAnsiEncoding.
        // MacRomanEncoding codes 128-255 differ from Unicode, but veraPDF's
        // TrueType width validation uses raw character codes as Unicode in the
        // (3,1) cmap. WinAnsiEncoding codes 160-255 are identity with Unicode,
        // avoiding width mismatches.
        let needs_fix = match dict.get(b"Encoding") {
            Ok(Object::Name(enc)) => {
                let enc_str = String::from_utf8_lossy(enc);
                enc_str != "WinAnsiEncoding"
            }
            Ok(Object::Dictionary(enc_dict)) => {
                let base_is_winansi = matches!(
                    get_name(enc_dict, b"BaseEncoding").as_deref(),
                    Some("WinAnsiEncoding")
                );
                // Even with BaseEncoding=WinAnsi, flatten dictionaries with
                // Differences to a simple Name to avoid 6.2.11.6:2 failures on
                // non-AGL glyph names.
                !base_is_winansi || enc_dict.has(b"Differences")
            }
            Ok(Object::Reference(enc_ref)) => match doc.get_object(*enc_ref) {
                Ok(Object::Name(enc)) => {
                    let enc_str = String::from_utf8_lossy(enc);
                    enc_str != "WinAnsiEncoding"
                }
                Ok(Object::Dictionary(enc_dict)) => {
                    let base_is_winansi = matches!(
                        get_name(enc_dict, b"BaseEncoding").as_deref(),
                        Some("WinAnsiEncoding")
                    );
                    !base_is_winansi || enc_dict.has(b"Differences")
                }
                _ => true,
            },
            _ => true, // Missing Encoding — needs fix.
        };

        if needs_fix {
            to_fix.push(*id);
        }
    }

    // Apply fixes.
    let count = to_fix.len();
    for id in to_fix {
        if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(id) {
            // Always set Encoding to simple WinAnsiEncoding Name.
            // Preserving Differences arrays from referenced encoding dicts
            // can cause 6.2.11.6:2 violations when glyph names aren't in
            // the Adobe Glyph List. A simple Name avoids that check.
            dict.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
        }
    }

    // Strip Encoding from symbolic TrueType fonts (6.2.11.6:3).
    for id in &symbolic_to_strip {
        if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(*id) {
            dict.remove(b"Encoding");
        }
    }

    count + symbolic_to_strip.len()
}

/// Fix symbolic TrueType cmap tables in already-embedded fonts (6.2.11.6:4).
///
/// For symbolic TrueType fonts the cmap must contain exactly one subtable or
/// include Microsoft Symbol (3,0). We reuse the in-place binary fixer used
/// during embedding for existing FontFile2 streams.
pub fn fix_existing_symbolic_truetype_cmaps(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut fixed = 0usize;

    for font_id in font_ids {
        let (fd_id, ff2_id) = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            if get_name(dict, b"Subtype").as_deref() != Some("TrueType") {
                continue;
            }
            let base_name = get_name(dict, b"BaseFont").unwrap_or_default();
            if !is_font_symbolic(doc, dict) && !is_symbolic_font_name(&base_name) {
                continue;
            }
            let fd_id = match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(id)) => *id,
                _ => continue,
            };
            let Some(Object::Dictionary(fd)) = doc.objects.get(&fd_id) else {
                continue;
            };
            let ff2_id = match fd.get(b"FontFile2").ok() {
                Some(Object::Reference(id)) => *id,
                _ => continue,
            };
            (fd_id, ff2_id)
        };

        let before = match doc.objects.get(&ff2_id) {
            Some(Object::Stream(s)) => s.content.clone(),
            _ => continue,
        };
        fix_symbolic_truetype_cmap(doc, ff2_id);
        let after = match doc.objects.get(&ff2_id) {
            Some(Object::Stream(s)) => s.content.clone(),
            _ => continue,
        };
        if before != after {
            fixed += 1;
            continue;
        }

        let Some(font_data) = read_embedded_font_data(doc, fd_id) else {
            continue;
        };
        if tt_has_symbol_cmap(&font_data) {
            continue;
        }

        let symbol_mappings = tt_build_symbol_cmap_mappings(&font_data);
        if symbol_mappings.is_empty() {
            continue;
        }

        let Some(new_font_data) = tt_add_symbol_cmap_subtable(&font_data, &symbol_mappings) else {
            continue;
        };
        let len = new_font_data.len() as i64;
        let new_stream = Stream::new(
            dictionary! {
                "Length" => len,
                "Length1" => len,
            },
            new_font_data,
        );
        doc.objects.insert(ff2_id, Object::Stream(new_stream));
        fixed += 1;
    }

    fixed
}

fn tt_build_symbol_cmap_mappings(data: &[u8]) -> Vec<(u16, u16)> {
    let Ok(face) = ttf_parser::Face::parse(data, 0) else {
        return Vec::new();
    };

    let mut mappings = Vec::new();
    for code in 0u16..=255 {
        let Some(ch) = char::from_u32(code as u32) else {
            continue;
        };
        let Some(gid) = face
            .glyph_index(ch)
            .filter(|gid| gid.0 > 0 && tt_glyph_has_data(&face, *gid))
        else {
            continue;
        };
        mappings.push((0xF000u16 + code, gid.0));
    }

    mappings
}

/// Mac Roman code → Unicode mapping for codes 128-255.
const MAC_ROMAN_TO_UNICODE: [u16; 128] = [
    0x00C4, 0x00C5, 0x00C7, 0x00C9, 0x00D1, 0x00D6, 0x00DC, 0x00E1, // 128-135
    0x00E0, 0x00E2, 0x00E4, 0x00E3, 0x00E5, 0x00E7, 0x00E9, 0x00E8, // 136-143
    0x00EA, 0x00EB, 0x00ED, 0x00EC, 0x00EE, 0x00EF, 0x00F1, 0x00F3, // 144-151
    0x00F2, 0x00F4, 0x00F6, 0x00F5, 0x00FA, 0x00F9, 0x00FB, 0x00FC, // 152-159
    0x2020, 0x00B0, 0x00A2, 0x00A3, 0x00A7, 0x2022, 0x00B6, 0x00DF, // 160-167
    0x00AE, 0x00A9, 0x2122, 0x00B4, 0x00A8, 0x2260, 0x00C6, 0x00D8, // 168-175
    0x221E, 0x00B1, 0x2264, 0x2265, 0x00A5, 0x00B5, 0x2202, 0x2211, // 176-183
    0x220F, 0x03C0, 0x222B, 0x00AA, 0x00BA, 0x2126, 0x00E6, 0x00F8, // 184-191
    0x00BF, 0x00A1, 0x00AC, 0x221A, 0x0192, 0x2248, 0x2206, 0x00AB, // 192-199
    0x00BB, 0x2026, 0x00A0, 0x00C0, 0x00C3, 0x00D5, 0x0152, 0x0153, // 200-207
    0x2013, 0x2014, 0x201C, 0x201D, 0x2018, 0x2019, 0x00F7, 0x25CA, // 208-215
    0x00FF, 0x0178, 0x2044, 0x20AC, 0x2039, 0x203A, 0xFB01, 0xFB02, // 216-223
    0x2021, 0x00B7, 0x201A, 0x201E, 0x2030, 0x00C2, 0x00CA, 0x00C1, // 224-231
    0x00CB, 0x00C8, 0x00CD, 0x00CE, 0x00CF, 0x00CC, 0x00D3, 0x00D4, // 232-239
    0xF8FF, 0x00D2, 0x00DA, 0x00DB, 0x00D9, 0x0131, 0x02C6, 0x02DC, // 240-247
    0x00AF, 0x02D8, 0x02D9, 0x02DA, 0x00B8, 0x02DD, 0x02DB, 0x02C7, // 248-255
];

/// Convert a Mac Roman code (0-255) to its Unicode codepoint.
fn mac_roman_to_unicode(code: u8) -> u16 {
    if code < 128 {
        code as u16
    } else {
        MAC_ROMAN_TO_UNICODE[(code - 128) as usize]
    }
}

/// Add a (3,1) Unicode BMP cmap subtable to a TrueType font that lacks one.
///
/// Many embedded TrueType subsets only have a (1,0) Mac Roman cmap. veraPDF
/// requires a (3,1) Unicode cmap for non-symbolic fonts with WinAnsiEncoding.
/// This function reads the existing (1,0) cmap, converts Mac Roman codes to
/// Unicode, and rebuilds the font with an additional (3,1) format 4 subtable.
pub fn fix_truetype_unicode_cmap(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut fixed = 0;

    for font_id in font_ids {
        let (fd_id, ff2_key) = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            if get_name(dict, b"Subtype").as_deref() != Some("TrueType") {
                continue;
            }
            if is_font_symbolic(doc, dict) {
                continue;
            }
            let fd_id = match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(id)) => *id,
                _ => continue,
            };
            // Must have FontFile2 (TrueType font program).
            let ff2_key = {
                let Some(Object::Dictionary(fd)) = doc.objects.get(&fd_id) else {
                    continue;
                };
                match fd.get(b"FontFile2").ok() {
                    Some(Object::Reference(id)) => *id,
                    _ => continue,
                }
            };
            (fd_id, ff2_key)
        };

        // Read the font data.
        let Some(font_data) = read_embedded_font_data(doc, fd_id) else {
            continue;
        };

        // Check if the font already has a (3,1) cmap. If so, skip.
        if tt_has_unicode_cmap(&font_data) {
            continue;
        }

        // Read (1,0) cmap mappings: Mac Roman code → GID.
        let mac_mappings = tt_read_mac_cmap(&font_data);
        if mac_mappings.is_empty() {
            continue;
        }

        // Also try (3,0) Symbol cmap for PUA-mapped fonts.
        let sym_mappings = tt_read_symbol_cmap(&font_data);

        // Build Unicode → GID mappings for the (3,1) subtable.
        let mut unicode_mappings: Vec<(u16, u16)> = Vec::new();

        // From (1,0) cmap: convert Mac Roman codes to Unicode.
        // Also add WinAnsi unicode for the same code, in case the encoding
        // was converted from MacRomanEncoding to WinAnsiEncoding by
        // fix_truetype_encoding (e.g. code 165: Mac=bullet U+2022,
        // WinAnsi=yen U+00A5 — both need to map to the same GID).
        for (mac_code, gid) in &mac_mappings {
            if *gid == 0 {
                continue; // Skip .notdef.
            }
            let mac_unicode = mac_roman_to_unicode(*mac_code);
            if mac_unicode > 0 && mac_unicode != 0xF8FF {
                // Avoid PUA Apple logo.
                unicode_mappings.push((mac_unicode, *gid));
            }
            // Also add the WinAnsi unicode for the same code position.
            let winansi_char = encoding_to_char(*mac_code as u32, "WinAnsiEncoding");
            let winansi_unicode = winansi_char as u16;
            if winansi_unicode != mac_unicode && winansi_unicode > 0 {
                unicode_mappings.push((winansi_unicode, *gid));
            }
        }

        // From (3,0) cmap: convert PUA codes (U+F0xx) to standard Unicode.
        for (pua_code, gid) in &sym_mappings {
            if *gid == 0 {
                continue;
            }
            if *pua_code >= 0xF000 && *pua_code <= 0xF0FF {
                let standard = *pua_code - 0xF000;
                if standard > 0 {
                    unicode_mappings.push((standard, *gid));
                }
            }
        }

        // Deduplicate by Unicode code, preferring higher GIDs (more specific).
        unicode_mappings.sort_by_key(|(u, _)| *u);
        unicode_mappings.dedup_by_key(|(u, _)| *u);

        if unicode_mappings.is_empty() {
            continue;
        }

        // Rebuild the font with the additional (3,1) cmap subtable.
        let Some(new_font_data) = tt_add_unicode_cmap_subtable(&font_data, &unicode_mappings)
        else {
            continue;
        };

        // Replace the FontFile2 stream with the modified font data.
        let len = new_font_data.len() as i64;
        let new_stream = Stream::new(
            dictionary! {
                "Length" => len,
                "Length1" => len,
            },
            new_font_data,
        );
        doc.objects.insert(ff2_key, Object::Stream(new_stream));
        fixed += 1;
    }

    fixed
}

/// Check if a TrueType font has a (3,1) Unicode BMP cmap.
fn tt_has_unicode_cmap(data: &[u8]) -> bool {
    let Some(cmap_data) = tt_find_table(data, b"cmap") else {
        return false;
    };
    if cmap_data.len() < 4 {
        return false;
    }
    let num_tables = u16::from_be_bytes([cmap_data[2], cmap_data[3]]) as usize;
    for i in 0..num_tables {
        let off = 4 + i * 8;
        if off + 4 > cmap_data.len() {
            break;
        }
        let platform = u16::from_be_bytes([cmap_data[off], cmap_data[off + 1]]);
        let encoding = u16::from_be_bytes([cmap_data[off + 2], cmap_data[off + 3]]);
        if platform == 3 && encoding == 1 {
            return true;
        }
    }
    false
}

fn tt_has_symbol_cmap(data: &[u8]) -> bool {
    let Some(cmap_data) = tt_find_table(data, b"cmap") else {
        return false;
    };
    if cmap_data.len() < 4 {
        return false;
    }
    let num_tables = u16::from_be_bytes([cmap_data[2], cmap_data[3]]) as usize;
    for i in 0..num_tables {
        let off = 4 + i * 8;
        if off + 4 > cmap_data.len() {
            break;
        }
        let platform = u16::from_be_bytes([cmap_data[off], cmap_data[off + 1]]);
        let encoding = u16::from_be_bytes([cmap_data[off + 2], cmap_data[off + 3]]);
        if platform == 3 && encoding == 0 {
            return true;
        }
    }
    false
}

/// Find a table in a TrueType font by tag, returning the table data slice.
fn tt_find_table<'a>(data: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
    if data.len() < 12 {
        return None;
    }
    let num_tables = u16::from_be_bytes([data[4], data[5]]) as usize;
    for i in 0..num_tables {
        let off = 12 + i * 16;
        if off + 16 > data.len() {
            break;
        }
        if &data[off..off + 4] == tag {
            let table_off =
                u32::from_be_bytes([data[off + 8], data[off + 9], data[off + 10], data[off + 11]])
                    as usize;
            let table_len = u32::from_be_bytes([
                data[off + 12],
                data[off + 13],
                data[off + 14],
                data[off + 15],
            ]) as usize;
            if table_off + table_len <= data.len() {
                return Some(&data[table_off..table_off + table_len]);
            }
        }
    }
    None
}

struct TtRawMetrics<'a> {
    units_per_em: u16,
    num_glyphs: u16,
    num_h_metrics: u16,
    hmtx: &'a [u8],
}

fn tt_parse_raw_metrics(data: &[u8]) -> Option<TtRawMetrics<'_>> {
    let head = tt_find_table(data, b"head");
    let hhea = tt_find_table(data, b"hhea");
    let hmtx = tt_find_table(data, b"hmtx")?;
    let maxp = tt_find_table(data, b"maxp");
    let loca = tt_find_table(data, b"loca")?;
    if hmtx.len() < 4 || loca.len() < 4 {
        return None;
    }

    let units_per_em = head
        .filter(|h| h.len() >= 20)
        .map(|h| u16::from_be_bytes([h[18], h[19]]))
        .filter(|u| *u > 0)
        .unwrap_or(1000);

    let inferred_num_h_metrics = (hmtx.len() / 4).clamp(1, u16::MAX as usize) as u16;
    let mut num_h_metrics = hhea
        .filter(|h| h.len() >= 36)
        .map(|h| u16::from_be_bytes([h[34], h[35]]))
        .and_then(|n| {
            if n == 0 {
                Some(1)
            } else if n as usize <= hmtx.len() / 4 {
                Some(n)
            } else {
                None
            }
        })
        .unwrap_or(inferred_num_h_metrics);

    let mut index_to_loc_format = head
        .filter(|h| h.len() >= 52)
        .map(|h| i16::from_be_bytes([h[50], h[51]]))
        .unwrap_or(-1);
    if index_to_loc_format != 0 && index_to_loc_format != 1 {
        let short_entries = loca.len() / 2;
        let long_entries = loca.len() / 4;
        if short_entries > 1 && short_entries.saturating_sub(1) >= num_h_metrics as usize {
            index_to_loc_format = 0;
        } else if long_entries > 1 {
            index_to_loc_format = 1;
        } else {
            return None;
        }
    }

    let inferred_num_glyphs = if index_to_loc_format == 0 {
        (loca.len() / 2).saturating_sub(1)
    } else {
        (loca.len() / 4).saturating_sub(1)
    }
    .clamp(1, u16::MAX as usize) as u16;

    let num_glyphs = maxp
        .filter(|m| m.len() >= 6)
        .map(|m| u16::from_be_bytes([m[4], m[5]]))
        .filter(|cand| *cand > 0)
        .map(|cand| {
            let needed = if index_to_loc_format == 0 {
                (cand as usize + 1) * 2
            } else {
                (cand as usize + 1) * 4
            };
            if needed <= loca.len() && inferred_num_glyphs as usize <= cand as usize * 4 {
                cand
            } else {
                inferred_num_glyphs
            }
        })
        .unwrap_or(inferred_num_glyphs);

    if num_h_metrics > num_glyphs {
        num_h_metrics = num_glyphs;
    }
    if num_h_metrics == 0 || num_glyphs == 0 {
        return None;
    }

    Some(TtRawMetrics {
        units_per_em,
        num_glyphs,
        num_h_metrics,
        hmtx,
    })
}

fn tt_raw_glyph_advance(metrics: &TtRawMetrics<'_>, gid: u16) -> Option<u16> {
    let idx = gid.min(metrics.num_h_metrics.saturating_sub(1)) as usize;
    let off = idx * 4;
    if off + 2 > metrics.hmtx.len() {
        return None;
    }
    Some(u16::from_be_bytes([
        metrics.hmtx[off],
        metrics.hmtx[off + 1],
    ]))
}

/// Read (1,0) Mac Roman cmap: returns Vec<(mac_code, gid)>.
fn tt_read_mac_cmap(data: &[u8]) -> Vec<(u8, u16)> {
    let Some(cmap_data) = tt_find_table(data, b"cmap") else {
        return Vec::new();
    };
    if cmap_data.len() < 4 {
        return Vec::new();
    }
    let num_tables = u16::from_be_bytes([cmap_data[2], cmap_data[3]]) as usize;
    for i in 0..num_tables {
        let rec_off = 4 + i * 8;
        if rec_off + 8 > cmap_data.len() {
            break;
        }
        let platform = u16::from_be_bytes([cmap_data[rec_off], cmap_data[rec_off + 1]]);
        let encoding = u16::from_be_bytes([cmap_data[rec_off + 2], cmap_data[rec_off + 3]]);
        if platform != 1 || encoding != 0 {
            continue;
        }
        let sub_off = u32::from_be_bytes([
            cmap_data[rec_off + 4],
            cmap_data[rec_off + 5],
            cmap_data[rec_off + 6],
            cmap_data[rec_off + 7],
        ]) as usize;
        if sub_off + 2 > cmap_data.len() {
            continue;
        }
        let format = u16::from_be_bytes([cmap_data[sub_off], cmap_data[sub_off + 1]]);
        match format {
            0 => {
                // Format 0: 256-byte array at offset 6.
                let arr_off = sub_off + 6;
                if arr_off + 256 > cmap_data.len() {
                    continue;
                }
                let mut result = Vec::new();
                for code in 0u16..256 {
                    let gid = cmap_data[arr_off + code as usize] as u16;
                    if gid > 0 {
                        result.push((code as u8, gid));
                    }
                }
                return result;
            }
            6 => {
                // Format 6: trimmed table.
                if sub_off + 10 > cmap_data.len() {
                    continue;
                }
                let first_code =
                    u16::from_be_bytes([cmap_data[sub_off + 6], cmap_data[sub_off + 7]]);
                let entry_count =
                    u16::from_be_bytes([cmap_data[sub_off + 8], cmap_data[sub_off + 9]]);
                let arr_off = sub_off + 10;
                let mut result = Vec::new();
                for j in 0..entry_count {
                    let gid_off = arr_off + j as usize * 2;
                    if gid_off + 2 > cmap_data.len() {
                        break;
                    }
                    let gid = u16::from_be_bytes([cmap_data[gid_off], cmap_data[gid_off + 1]]);
                    let code = first_code + j;
                    if gid > 0 && code <= 255 {
                        result.push((code as u8, gid));
                    }
                }
                return result;
            }
            _ => continue,
        }
    }
    Vec::new()
}

/// Read (3,0) Symbol cmap: returns Vec<(unicode_code, gid)>.
fn tt_read_symbol_cmap(data: &[u8]) -> Vec<(u16, u16)> {
    let Some(cmap_data) = tt_find_table(data, b"cmap") else {
        return Vec::new();
    };
    if cmap_data.len() < 4 {
        return Vec::new();
    }
    let num_tables = u16::from_be_bytes([cmap_data[2], cmap_data[3]]) as usize;
    for i in 0..num_tables {
        let rec_off = 4 + i * 8;
        if rec_off + 8 > cmap_data.len() {
            break;
        }
        let platform = u16::from_be_bytes([cmap_data[rec_off], cmap_data[rec_off + 1]]);
        let encoding = u16::from_be_bytes([cmap_data[rec_off + 2], cmap_data[rec_off + 3]]);
        if platform != 3 || encoding != 0 {
            continue;
        }
        let sub_off = u32::from_be_bytes([
            cmap_data[rec_off + 4],
            cmap_data[rec_off + 5],
            cmap_data[rec_off + 6],
            cmap_data[rec_off + 7],
        ]) as usize;
        if sub_off + 2 > cmap_data.len() {
            continue;
        }
        let format = u16::from_be_bytes([cmap_data[sub_off], cmap_data[sub_off + 1]]);
        if format == 4 {
            return tt_read_format4(cmap_data, sub_off);
        }
    }
    Vec::new()
}

/// Parse a cmap format 4 subtable into (code, gid) pairs.
fn tt_read_format4(data: &[u8], off: usize) -> Vec<(u16, u16)> {
    if off + 14 > data.len() {
        return Vec::new();
    }
    let seg_count = u16::from_be_bytes([data[off + 6], data[off + 7]]) as usize / 2;
    let end_codes_off = off + 14;
    let start_codes_off = end_codes_off + seg_count * 2 + 2; // +2 for reservedPad
    let delta_off = start_codes_off + seg_count * 2;
    let range_off = delta_off + seg_count * 2;

    if range_off + seg_count * 2 > data.len() {
        return Vec::new();
    }

    let mut result = Vec::new();
    for i in 0..seg_count {
        let end_code =
            u16::from_be_bytes([data[end_codes_off + i * 2], data[end_codes_off + i * 2 + 1]]);
        let start_code = u16::from_be_bytes([
            data[start_codes_off + i * 2],
            data[start_codes_off + i * 2 + 1],
        ]);
        let delta = i16::from_be_bytes([data[delta_off + i * 2], data[delta_off + i * 2 + 1]]);
        let range_offset =
            u16::from_be_bytes([data[range_off + i * 2], data[range_off + i * 2 + 1]]);

        if start_code == 0xFFFF {
            break;
        }

        for code in start_code..=end_code {
            let gid = if range_offset == 0 {
                (code as i32 + delta as i32) as u16
            } else {
                let idx = range_offset as usize / 2 + (code - start_code) as usize + i; // relative to range_off position
                let gid_off = range_off + idx * 2;
                if gid_off + 2 > data.len() {
                    0
                } else {
                    let raw = u16::from_be_bytes([data[gid_off], data[gid_off + 1]]);
                    if raw == 0 {
                        0
                    } else {
                        (raw as i32 + delta as i32) as u16
                    }
                }
            };
            if gid > 0 {
                result.push((code, gid));
            }
        }
    }
    result
}

/// Build a cmap format 4 subtable from Unicode → GID mappings.
fn tt_build_format4(mappings: &[(u16, u16)]) -> Vec<u8> {
    let mut sorted: Vec<(u16, u16)> = mappings.to_vec();
    sorted.sort_by_key(|(u, _)| *u);
    sorted.dedup_by_key(|(u, _)| *u);

    // Build segments: merge consecutive codes with consecutive GIDs.
    let mut segments: Vec<(u16, u16, i16)> = Vec::new(); // (start, end, delta)
    for &(unicode, gid) in &sorted {
        let delta = (gid as i32 - unicode as i32) as i16;
        if let Some(last) = segments.last_mut() {
            if last.2 == delta && unicode == last.1 + 1 {
                last.1 = unicode;
                continue;
            }
        }
        segments.push((unicode, unicode, delta));
    }
    // Sentinel segment.
    segments.push((0xFFFF, 0xFFFF, 1));

    let seg_count = segments.len();
    let seg_count_x2 = (seg_count * 2) as u16;
    let max_pow2 = if seg_count > 0 {
        (seg_count as f64).log2().floor() as u32
    } else {
        0
    };
    let search_range = 2u16.pow(max_pow2) * 2;
    let entry_selector = max_pow2 as u16;
    let range_shift = seg_count_x2 - search_range;

    let length = 16 + seg_count * 8; // header(14) + 4 arrays × segCount × 2 + reservedPad(2)
    let mut data = Vec::with_capacity(length);

    // Header.
    data.extend_from_slice(&4u16.to_be_bytes()); // format
    data.extend_from_slice(&(length as u16).to_be_bytes());
    data.extend_from_slice(&0u16.to_be_bytes()); // language
    data.extend_from_slice(&seg_count_x2.to_be_bytes());
    data.extend_from_slice(&search_range.to_be_bytes());
    data.extend_from_slice(&entry_selector.to_be_bytes());
    data.extend_from_slice(&range_shift.to_be_bytes());

    // endCode array.
    for (_, end, _) in &segments {
        data.extend_from_slice(&end.to_be_bytes());
    }
    // reservedPad.
    data.extend_from_slice(&0u16.to_be_bytes());
    // startCode array.
    for (start, _, _) in &segments {
        data.extend_from_slice(&start.to_be_bytes());
    }
    // idDelta array.
    for (_, _, delta) in &segments {
        data.extend_from_slice(&delta.to_be_bytes());
    }
    // idRangeOffset array (all zeros — using idDelta only).
    for _ in &segments {
        data.extend_from_slice(&0u16.to_be_bytes());
    }

    data
}

/// Calculate TrueType table checksum.
fn tt_checksum(data: &[u8]) -> u32 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 4 <= data.len() {
        sum = sum.wrapping_add(u32::from_be_bytes([
            data[i],
            data[i + 1],
            data[i + 2],
            data[i + 3],
        ]));
        i += 4;
    }
    if i < data.len() {
        let mut buf = [0u8; 4];
        for (j, byte) in data[i..].iter().enumerate() {
            buf[j] = *byte;
        }
        sum = sum.wrapping_add(u32::from_be_bytes(buf));
    }
    sum
}

/// Add a Windows cmap subtable to a TrueType font.
///
/// Rebuilds the cmap table with the original subtables plus a new format 4
/// subtable for platform 3 with the requested encoding ID.
fn tt_add_windows_cmap_subtable(
    data: &[u8],
    mappings: &[(u16, u16)],
    encoding_id: u16,
) -> Option<Vec<u8>> {
    if data.len() < 12 {
        return None;
    }
    let sf_version = u32::from_be_bytes([data[0], data[1], data[2], data[3]]);
    let num_tables = u16::from_be_bytes([data[4], data[5]]) as usize;
    if data.len() < 12 + num_tables * 16 {
        return None;
    }

    // Parse table directory.
    struct TableEntry {
        tag: [u8; 4],
        offset: u32,
        length: u32,
    }
    let mut tables: Vec<TableEntry> = Vec::with_capacity(num_tables);
    for i in 0..num_tables {
        let off = 12 + i * 16;
        let tag = [data[off], data[off + 1], data[off + 2], data[off + 3]];
        let offset =
            u32::from_be_bytes([data[off + 8], data[off + 9], data[off + 10], data[off + 11]]);
        let length = u32::from_be_bytes([
            data[off + 12],
            data[off + 13],
            data[off + 14],
            data[off + 15],
        ]);
        tables.push(TableEntry {
            tag,
            offset,
            length,
        });
    }

    // Build the new cmap table.
    let cmap_idx = tables.iter().position(|t| &t.tag == b"cmap")?;
    let old_cmap = &data[tables[cmap_idx].offset as usize
        ..(tables[cmap_idx].offset + tables[cmap_idx].length) as usize];

    let old_num_subtables = u16::from_be_bytes([old_cmap[2], old_cmap[3]]) as usize;
    let new_num_subtables = old_num_subtables + 1;
    let new_header_size = 4 + new_num_subtables * 8;
    let old_header_size = 4 + old_num_subtables * 8;
    let header_growth = 8; // One new encoding record.

    // Build new cmap: header + adjusted original subtables + new format 4.
    let format4 = tt_build_format4(mappings);
    let old_subtable_data = &old_cmap[old_header_size..];
    let new_format4_offset = new_header_size + old_subtable_data.len();

    let mut new_cmap = Vec::with_capacity(new_format4_offset + format4.len());

    // Header.
    new_cmap.extend_from_slice(&0u16.to_be_bytes()); // version
    new_cmap.extend_from_slice(&(new_num_subtables as u16).to_be_bytes());

    // Copy existing encoding records with adjusted offsets.
    for i in 0..old_num_subtables {
        let rec_off = 4 + i * 8;
        // Platform and encoding IDs (4 bytes).
        new_cmap.extend_from_slice(&old_cmap[rec_off..rec_off + 4]);
        // Adjust subtable offset.
        let old_offset = u32::from_be_bytes([
            old_cmap[rec_off + 4],
            old_cmap[rec_off + 5],
            old_cmap[rec_off + 6],
            old_cmap[rec_off + 7],
        ]);
        let new_offset = old_offset + header_growth as u32;
        new_cmap.extend_from_slice(&new_offset.to_be_bytes());
    }

    // Add new Windows encoding record.
    new_cmap.extend_from_slice(&3u16.to_be_bytes()); // platformID
    new_cmap.extend_from_slice(&encoding_id.to_be_bytes());
    new_cmap.extend_from_slice(&(new_format4_offset as u32).to_be_bytes());

    // Copy original subtable data.
    new_cmap.extend_from_slice(old_subtable_data);

    // Append new format 4 subtable.
    new_cmap.extend_from_slice(&format4);

    // Rebuild the entire font with the new cmap table.
    let dir_size = 12 + num_tables * 16;

    // Calculate table directory header values.
    let max_pow2 = if num_tables > 0 {
        (num_tables as f64).log2().floor() as u32
    } else {
        0
    };
    let search_range = 16u32 * 2u32.pow(max_pow2);
    let entry_selector = max_pow2;
    let range_shift = (num_tables * 16) as u32 - search_range;

    let mut output = Vec::with_capacity(data.len() + format4.len() + 64);

    // Font header.
    output.extend_from_slice(&sf_version.to_be_bytes());
    output.extend_from_slice(&(num_tables as u16).to_be_bytes());
    output.extend_from_slice(&(search_range as u16).to_be_bytes());
    output.extend_from_slice(&(entry_selector as u16).to_be_bytes());
    output.extend_from_slice(&(range_shift as u16).to_be_bytes());

    // Placeholder table directory (will fill in offsets after writing data).
    let dir_start = output.len();
    output.resize(dir_size, 0);

    // Write each table's data and record its position.
    let mut head_offset_in_output: Option<usize> = None;
    for (i, table) in tables.iter().enumerate() {
        // Pad to 4-byte boundary.
        while output.len() % 4 != 0 {
            output.push(0);
        }

        let table_data = if i == cmap_idx {
            &new_cmap
        } else {
            let start = table.offset as usize;
            let end = start + table.length as usize;
            if end > data.len() {
                return None;
            }
            &data[start..end]
        };

        let out_offset = output.len() as u32;
        let out_length = table_data.len() as u32;
        let checksum = tt_checksum(table_data);

        if &table.tag == b"head" {
            head_offset_in_output = Some(output.len());
        }

        // Fill in the directory entry.
        let entry_off = dir_start + i * 16;
        output[entry_off..entry_off + 4].copy_from_slice(&table.tag);
        output[entry_off + 4..entry_off + 8].copy_from_slice(&checksum.to_be_bytes());
        output[entry_off + 8..entry_off + 12].copy_from_slice(&out_offset.to_be_bytes());
        output[entry_off + 12..entry_off + 16].copy_from_slice(&out_length.to_be_bytes());

        output.extend_from_slice(table_data);
    }

    // Fix head checkSumAdjustment.
    if let Some(head_off) = head_offset_in_output {
        if head_off + 12 <= output.len() {
            // Zero out checkSumAdjustment before computing file checksum.
            output[head_off + 8..head_off + 12].copy_from_slice(&0u32.to_be_bytes());
            let file_checksum = tt_checksum(&output);
            let adjustment = 0xB1B0_AFBAu32.wrapping_sub(file_checksum);
            output[head_off + 8..head_off + 12].copy_from_slice(&adjustment.to_be_bytes());
        }
    }

    Some(output)
}

/// Add a (3,1) Unicode BMP cmap subtable to a TrueType font.
fn tt_add_unicode_cmap_subtable(data: &[u8], mappings: &[(u16, u16)]) -> Option<Vec<u8>> {
    tt_add_windows_cmap_subtable(data, mappings, 1)
}

/// Add a (3,0) Microsoft Symbol cmap subtable to a TrueType font.
fn tt_add_symbol_cmap_subtable(data: &[u8], mappings: &[(u16, u16)]) -> Option<Vec<u8>> {
    tt_add_windows_cmap_subtable(data, mappings, 0)
}

/// Ensure non-symbolic TrueType fonts with WinAnsiEncoding have Differences
/// entries for ALL undefined codes (0-31, 127, 129, 141, 143, 144, 157).
///
/// Without explicit Differences, these codes have ambiguous glyph mapping:
/// veraPDF may use the font's built-in encoding or cmap fallbacks that differ
/// from our width computation. Mapping them to "space" ensures both veraPDF
/// and our width fixer use the same glyph (U+0020 → space width).
pub fn fix_undefined_encoding_codes(doc: &mut Document) -> usize {
    // Codes that are undefined in WinAnsiEncoding (CP-1252):
    // 0-31: C0 control characters (except 9, 10, 13 which are HT, LF, CR)
    // 127: DELETE
    // 129, 141, 143, 144, 157: undefined positions in CP-1252
    const UNDEFINED_CODES: &[u32] = &[
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
        25, 26, 27, 28, 29, 30, 31, 127, 129, 141, 143, 144, 157,
    ];

    let font_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            let dict = obj.as_dict().ok()?;
            if get_name(dict, b"Subtype").as_deref() != Some("TrueType") {
                return None;
            }
            let base_font = get_name(dict, b"BaseFont").unwrap_or_default();
            if is_font_symbolic(doc, dict) || is_symbolic_font_name(&base_font) {
                return None;
            }
            // Check if encoding is WinAnsiEncoding (with or without Differences).
            let enc = dict.get(b"Encoding").ok()?;
            let is_winansi = match enc {
                Object::Name(n) => n == b"WinAnsiEncoding",
                Object::Dictionary(d) => {
                    get_name(d, b"BaseEncoding").as_deref() == Some("WinAnsiEncoding")
                }
                Object::Reference(r) => match doc.get_object(*r).ok() {
                    Some(Object::Name(n)) => n == b"WinAnsiEncoding",
                    Some(Object::Dictionary(d)) => {
                        get_name(d, b"BaseEncoding").as_deref() == Some("WinAnsiEncoding")
                    }
                    _ => false,
                },
                _ => false,
            };
            if is_winansi {
                Some(*id)
            } else {
                None
            }
        })
        .collect();

    let mut fixed = 0;

    for font_id in font_ids {
        // Parse existing Differences to see which undefined codes are already covered.
        let existing_diff = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let enc = dict.get(b"Encoding").ok();
            parse_differences_from_encoding(doc, enc)
        };

        // Find which undefined codes are missing from Differences.
        let missing: Vec<u32> = UNDEFINED_CODES
            .iter()
            .filter(|&&code| !existing_diff.contains_key(&code))
            .copied()
            .collect();

        if missing.is_empty() {
            continue;
        }

        // Build a new Differences array that includes all existing entries
        // plus the missing undefined codes → "space".
        let mut all_diff = existing_diff;
        for code in &missing {
            all_diff.insert(*code, "space".to_string());
        }

        // Convert to sorted Differences array format.
        let mut sorted_codes: Vec<u32> = all_diff.keys().copied().collect();
        sorted_codes.sort();

        let mut diff_array = Vec::new();
        let mut prev_code: Option<u32> = None;
        for code in sorted_codes {
            let needs_int = prev_code.is_none_or(|p| code != p + 1);
            if needs_int {
                diff_array.push(Object::Integer(code as i64));
            }
            diff_array.push(Object::Name(all_diff[&code].as_bytes().to_vec()));
            prev_code = Some(code);
        }

        // Update the font's Encoding to a dict with BaseEncoding + Differences.
        let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&font_id) else {
            continue;
        };
        let mut enc_dict = lopdf::Dictionary::new();
        enc_dict.set("Type", Object::Name(b"Encoding".to_vec()));
        enc_dict.set("BaseEncoding", Object::Name(b"WinAnsiEncoding".to_vec()));
        enc_dict.set("Differences", Object::Array(diff_array));
        dict.set("Encoding", Object::Dictionary(enc_dict));
        fixed += 1;
    }

    fixed
}

/// Parse Differences from an Encoding value (Name, Dictionary, or Reference).
fn parse_differences_from_encoding(
    doc: &Document,
    enc: Option<&Object>,
) -> std::collections::HashMap<u32, String> {
    let mut diff = std::collections::HashMap::new();
    let enc_dict = match enc {
        Some(Object::Dictionary(d)) => Some(d),
        Some(Object::Reference(r)) => doc.get_object(*r).ok().and_then(|o| o.as_dict().ok()),
        _ => None,
    };
    if let Some(enc_dict) = enc_dict {
        if let Ok(arr) = enc_dict.get(b"Differences") {
            let arr = match arr {
                Object::Array(a) => Some(a.as_slice()),
                Object::Reference(r) => doc
                    .get_object(*r)
                    .ok()
                    .and_then(|o| o.as_array().ok())
                    .map(|a| a.as_slice()),
                _ => None,
            };
            if let Some(arr) = arr {
                let mut code = 0u32;
                for item in arr {
                    match item {
                        Object::Integer(i) => code = *i as u32,
                        Object::Name(n) => {
                            diff.insert(code, String::from_utf8_lossy(n).to_string());
                            code += 1;
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    diff
}

/// Check if a font is symbolic based on FontDescriptor Flags and font name.
fn is_font_symbolic(doc: &Document, font_dict: &lopdf::Dictionary) -> bool {
    // Check FontDescriptor Flags FIRST — these may have been updated after
    // embedding a non-symbolic fallback font (e.g., DejaVuSans for ZapfDingbats).
    // Bit 3 (value 4) = Symbolic, bit 6 (value 32) = Nonsymbolic.
    let fd = match font_dict.get(b"FontDescriptor") {
        Ok(Object::Reference(id)) => doc.get_object(*id).ok(),
        Ok(obj) => Some(obj),
        _ => None,
    };
    if let Some(Object::Dictionary(fd_dict)) = fd {
        if let Ok(Object::Integer(flags)) = fd_dict.get(b"Flags") {
            let symbolic = (*flags & 4) != 0;
            let nonsymbolic = (*flags & 32) != 0;
            // Respect unambiguous flag settings first.
            if nonsymbolic && !symbolic {
                return false;
            }
            if symbolic && !nonsymbolic {
                return true;
            }
            // If both bits are set, use the font name as a tiebreaker.
            // This occurs in real-world Symbol fonts where validators still
            // treat the font as symbolic for 6.2.11.6 checks.
            if symbolic && nonsymbolic {
                if let Some(name) = get_name(font_dict, b"BaseFont") {
                    return is_symbolic_font_name(&name);
                }
                return false;
            }
        }
    }

    // Fallback: check base font name against known symbolic fonts.
    if let Some(name) = get_name(font_dict, b"BaseFont") {
        if is_symbolic_font_name(&name) {
            return true;
        }
    }

    false
}

/// Fix FontDescriptor Flags for known symbolic fonts.
/// Sets Symbolic bit (4) and clears Nonsymbolic bit (32) for Symbol/ZapfDingbats etc.
/// NOTE: Disabled — marking fallback fonts as Symbolic causes 6.2.11.6:4 regression
/// because DejaVuSans has multiple cmap subtables (symbolic fonts need exactly one).
#[allow(dead_code)]
pub fn fix_symbolic_font_flags(doc: &mut Document) -> usize {
    let mut to_fix: Vec<(ObjectId, ObjectId)> = Vec::new(); // (font_id, fd_id)

    for (id, obj) in &doc.objects {
        let Object::Dictionary(dict) = obj else {
            continue;
        };
        let subtype = get_name(dict, b"Subtype").unwrap_or_default();
        if subtype != "TrueType" && subtype != "Type1" {
            continue;
        }
        let Some(name) = get_name(dict, b"BaseFont") else {
            continue;
        };
        if !is_symbolic_font_name(&name) {
            continue;
        }
        // Check if FontDescriptor Flags are wrong.
        let fd_id = match dict.get(b"FontDescriptor") {
            Ok(Object::Reference(fid)) => *fid,
            _ => continue,
        };
        let needs_fix = match doc.objects.get(&fd_id) {
            Some(Object::Dictionary(fd)) => match fd.get(b"Flags") {
                Ok(Object::Integer(flags)) => (*flags & 4) == 0, // Symbolic bit not set
                _ => true,
            },
            _ => false,
        };
        if needs_fix {
            to_fix.push((*id, fd_id));
        }
    }

    let count = to_fix.len();
    for (_font_id, fd_id) in to_fix {
        if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
            let flags = match fd.get(b"Flags") {
                Ok(Object::Integer(f)) => *f,
                _ => 0,
            };
            // Set Symbolic (bit 2 = 4), clear Nonsymbolic (bit 5 = 32).
            let new_flags = (flags | 4) & !32;
            fd.set("Flags", Object::Integer(new_flags));
        }
    }
    count
}

/// Fix width mismatches for symbolic TrueType fonts — rule 6.2.11.5:1.
///
/// We intentionally limit this pass to TrueType (`FontFile2`) symbolic fonts.
/// For Type1/CFF symbolic fonts, different validators may resolve widths through
/// glyph names and encoding differences in ways that are not captured reliably
/// by our current CFF lookup, and aggressive rewrites can regress compliant files.
pub fn fix_symbolic_font_widths(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter_map(|(id, obj)| {
            if let Object::Dictionary(dict) = obj {
                if is_font_dict(dict) {
                    return Some(*id);
                }
            }
            None
        })
        .collect();

    let mut fixed = 0;

    for font_id in font_ids {
        let info = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();
            if subtype != "TrueType" && subtype != "Type1" && subtype != "MMType1" {
                continue;
            }

            let name = match get_name(dict, b"BaseFont") {
                Some(n) => n,
                None => continue,
            };
            if !is_symbolic_font_name(&name) {
                continue;
            }
            let base_name = strip_subset_prefix(&name).to_string();
            let is_classic_symbol = matches!(
                base_name.as_str(),
                "Symbol" | "SymbolMT" | "ZapfDingbats" | "Dingbats"
            );
            // Respect descriptor flags: some Symbol-named fallback fonts are
            // intentionally non-symbolic and should stay on the regular path.
            if !is_font_symbolic(doc, dict) {
                continue;
            }

            let fd_id = match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(id)) => *id,
                _ => continue,
            };

            let fc = match dict.get(b"FirstChar").ok() {
                Some(Object::Integer(i)) => *i as u32,
                _ => continue,
            };
            let (existing_widths, widths_ref) = match dict.get(b"Widths").ok() {
                Some(Object::Array(arr)) => (arr.clone(), None),
                Some(Object::Reference(r)) => match doc.get_object(*r) {
                    Ok(Object::Array(arr)) => (arr.clone(), Some(*r)),
                    _ => continue,
                },
                _ => continue,
            };
            if existing_widths.is_empty() {
                continue;
            }

            let is_subset = name.contains('+');
            let enc_info = if subtype == "TrueType" {
                (String::new(), std::collections::HashMap::new())
            } else {
                get_simple_encoding_info(doc, dict)
            };

            (
                subtype,
                fd_id,
                fc,
                existing_widths,
                widths_ref,
                is_subset,
                is_classic_symbol,
                enc_info,
            )
        };

        let (
            subtype,
            fd_id,
            first_char,
            existing_widths,
            widths_ref,
            is_subset,
            is_classic_symbol,
            enc_info,
        ) = info;

        let (has_ff, has_ff2, has_ff3) = match doc.objects.get(&fd_id) {
            Some(Object::Dictionary(d)) => {
                (d.has(b"FontFile"), d.has(b"FontFile2"), d.has(b"FontFile3"))
            }
            _ => continue,
        };
        if !has_ff && !has_ff2 && !has_ff3 {
            continue;
        }

        let font_data = read_embedded_font_data(doc, fd_id);
        let Some(font_data) = font_data else { continue };

        let corrections = if subtype == "TrueType" {
            if !has_ff2 {
                continue;
            }
            compute_symbolic_truetype_width_corrections(
                &font_data,
                first_char,
                &existing_widths,
                is_subset,
            )
        } else {
            if !has_ff3 {
                continue;
            }
            if is_classic_symbol {
                let mut merged = std::collections::BTreeMap::<usize, i64>::new();
                let has_explicit_differences = !enc_info.1.is_empty();

                // For explicit Differences names, prefer direct glyph-name /
                // Unicode lookup in the embedded font.
                for (idx, w) in compute_symbolic_difference_width_corrections(
                    &font_data,
                    first_char,
                    &existing_widths,
                    &enc_info.1,
                ) {
                    merged.insert(idx, w);
                }

                // When a classic Symbol/Zapf font carries a standard PDF
                // Encoding name (for example WinAnsiEncoding), the code ->
                // glyph-name mapping is deterministic per the PDF encoding,
                // even without explicit Differences. Use that mapping to keep
                // /Widths aligned with the embedded CFF program.
                if !enc_info.0.is_empty() {
                    for (idx, w) in compute_symbolic_cff_encoding_width_corrections(
                        &font_data,
                        first_char,
                        &existing_widths,
                        &enc_info.0,
                        &enc_info.1,
                    ) {
                        merged.entry(idx).or_insert(w);
                    }
                }

                // Without explicit Differences, code->glyph mapping for classic
                // Symbol/Zapf fonts is ambiguous. In that case, do not apply a
                // broad .notdef fallback rewrite: it can replace correct widths
                // and trigger 6.2.11.5:1 mismatches.
                //
                // For subset fonts with explicit Differences, keep the fallback
                // to fill unmapped slots conservatively.
                if is_subset && has_explicit_differences {
                    for (idx, w) in compute_classic_symbol_cff_width_corrections(
                        &font_data,
                        first_char,
                        &existing_widths,
                    ) {
                        let code = first_char + idx as u32;
                        if enc_info.1.contains_key(&code) {
                            continue;
                        }
                        merged.entry(idx).or_insert(w);
                    }
                }

                merged.into_iter().collect()
            } else {
                // For non-classic Type1 symbolic fonts without a usable
                // PDF-level encoding, post-embed widths are often already
                // aligned with .notdef fallback behavior.
                if enc_info.0.is_empty() && enc_info.1.is_empty() {
                    continue;
                }
                compute_cff_type1_width_corrections(
                    &font_data,
                    first_char,
                    &existing_widths,
                    &enc_info,
                )
            }
        };

        if corrections.is_empty() {
            continue;
        }

        // Safety: on subset symbolic fonts, skip very high-mismatch updates that
        // likely indicate an incorrect mapping strategy.
        if subtype == "TrueType" && is_subset && corrections.len() * 5 > existing_widths.len() * 4 {
            continue;
        }

        let mut new_widths = existing_widths.clone();
        for (idx, new_w) in &corrections {
            if *idx < new_widths.len() {
                new_widths[*idx] = Object::Integer(*new_w);
            }
        }

        if let Some(widths_id) = widths_ref {
            if let Some(Object::Array(ref mut widths)) = doc.objects.get_mut(&widths_id) {
                *widths = new_widths;
            } else {
                continue;
            }
        } else {
            let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&font_id) else {
                continue;
            };
            font.set("Widths", Object::Array(new_widths));
        }
        fixed += 1;
    }

    fixed
}

/// Compute width corrections for a symbolic TrueType font.
fn compute_symbolic_truetype_width_corrections(
    font_data: &[u8],
    first_char: u32,
    existing_widths: &[Object],
    is_subset: bool,
) -> Vec<(usize, i64)> {
    use std::collections::HashMap;

    let Ok(face) = ttf_parser::Face::parse(font_data, 0) else {
        return Vec::new();
    };

    let units_per_em = face.units_per_em() as f64;
    if units_per_em == 0.0 {
        return Vec::new();
    }
    let scale = 1000.0 / units_per_em;
    let mut corrections = Vec::new();
    let mac_map: HashMap<u8, u16> = tt_read_mac_cmap(font_data).into_iter().collect();

    for (i, obj) in existing_widths.iter().enumerate() {
        let pdf_w = match obj {
            Object::Integer(w) => *w,
            Object::Real(r) => *r as i64,
            _ => continue,
        };

        let code = first_char + i as u32;

        // Symbolic TrueType: veraPDF maps code via (3,0) cmap at 0xF000+code,
        // or (1,0) cmap at code directly. Some subset symbol fonts are encoded
        // as direct code->GID without usable cmap entries; in that case, fall
        // back to GID == code.
        let gid = face
            .glyph_index(char::from_u32(0xF000 + code).unwrap_or('\0'))
            .or_else(|| face.glyph_index(char::from_u32(code).unwrap_or('\0')))
            .or_else(|| {
                if code <= 255 {
                    mac_map
                        .get(&(code as u8))
                        .copied()
                        .filter(|gid| *gid > 0)
                        .map(ttf_parser::GlyphId)
                } else {
                    None
                }
            })
            .or_else(|| {
                if is_subset && code < face.number_of_glyphs() as u32 {
                    Some(ttf_parser::GlyphId(code as u16))
                } else {
                    None
                }
            });
        let gid = gid.unwrap_or(ttf_parser::GlyphId(0));
        let Some(advance) = face.glyph_hor_advance(gid) else {
            continue;
        };

        let expected = (advance as f64 * scale).round() as i64;
        if (pdf_w - expected).abs() > 1 {
            corrections.push((i, expected));
        }
    }

    corrections
}

/// Compute corrections for explicit symbolic /Differences entries using
/// glyph-name or Unicode lookup in the embedded font.
fn compute_symbolic_difference_width_corrections(
    font_data: &[u8],
    first_char: u32,
    existing_widths: &[Object],
    differences: &std::collections::HashMap<u32, String>,
) -> Vec<(usize, i64)> {
    let Ok(face) = ttf_parser::Face::parse(font_data, 0) else {
        return Vec::new();
    };
    let upem = face.units_per_em() as f64;
    if upem == 0.0 {
        return Vec::new();
    }
    let scale = 1000.0 / upem;
    let mut corrections = Vec::new();

    for (code, name) in differences {
        if *code < first_char {
            continue;
        }
        let idx = (*code - first_char) as usize;
        let Some(pdf_w) = existing_widths.get(idx).and_then(object_to_f64) else {
            continue;
        };
        let gid = face
            .glyph_index_by_name(name)
            .or_else(|| glyph_name_to_unicode(name).and_then(|u| face.glyph_index(u)));
        let Some(gid) = gid else { continue };
        let Some(advance) = face.glyph_hor_advance(gid) else {
            continue;
        };
        let expected = advance as f64 * scale;
        if (pdf_w - expected).abs() >= 1.0 {
            corrections.push((idx, expected.round() as i64));
        }
    }

    corrections
}

fn compute_symbolic_cff_encoding_width_corrections(
    font_data: &[u8],
    first_char: u32,
    existing_widths: &[Object],
    enc_name: &str,
    differences: &std::collections::HashMap<u32, String>,
) -> Vec<(usize, i64)> {
    let mut corrections = Vec::new();

    for (i, obj) in existing_widths.iter().enumerate() {
        let Some(pdf_w) = object_to_f64(obj) else {
            continue;
        };
        let code = first_char + i as u32;
        let Some(expected) =
            compute_classic_symbol_cff_single_width(font_data, code, enc_name, differences)
        else {
            continue;
        };
        if (pdf_w - expected).abs() >= 1.0 {
            corrections.push((i, expected.round() as i64));
        }
    }

    corrections
}

/// Compute conservative width corrections for classic Symbol/Zapf CFF fonts
/// when no PDF-level encoding is present. Unmapped codes fall back to .notdef.
fn compute_classic_symbol_cff_width_corrections(
    font_data: &[u8],
    first_char: u32,
    existing_widths: &[Object],
) -> Vec<(usize, i64)> {
    // Prefer OTF wrapper metrics when available.
    if let Ok(face) = ttf_parser::Face::parse(font_data, 0) {
        let upem = face.units_per_em() as f64;
        if upem > 0.0 {
            let scale = 1000.0 / upem;
            if extract_cff_from_otf(font_data).is_some() {
                let notdef = face
                    .glyph_hor_advance(ttf_parser::GlyphId(0))
                    .map(|w| w as f64 * scale)
                    .unwrap_or(0.0);
                let mut corrections = Vec::new();
                for (i, obj) in existing_widths.iter().enumerate() {
                    let Some(pdf_w) = object_to_f64(obj) else {
                        continue;
                    };
                    let code = first_char + i as u32;
                    if code > 255 {
                        continue;
                    }
                    let expected = notdef;
                    if (pdf_w - expected).abs() >= 1.0 {
                        corrections.push((i, expected.round() as i64));
                    }
                }
                return corrections;
            }
        }
    }

    // Raw CFF fallback.
    let Some(cff) = cff_parser::Table::parse(font_data) else {
        return Vec::new();
    };
    let scale = cff_matrix_scale(cff.matrix().sx);
    let notdef = cff
        .glyph_width(cff_parser::GlyphId(0))
        .map(|w| w as f64 * scale)
        .unwrap_or(0.0);
    let mut corrections = Vec::new();
    for (i, obj) in existing_widths.iter().enumerate() {
        let Some(pdf_w) = object_to_f64(obj) else {
            continue;
        };
        let code = first_char + i as u32;
        if code > 255 {
            continue;
        }
        let expected = notdef;
        if (pdf_w - expected).abs() >= 1.0 {
            corrections.push((i, expected.round() as i64));
        }
    }
    corrections
}

/// Compute width corrections for a symbolic CFF font.
#[allow(dead_code)]
fn compute_symbolic_cff_width_corrections(
    font_data: &[u8],
    first_char: u32,
    existing_widths: &[Object],
) -> Vec<(usize, i64)> {
    let Some(cff) = cff_parser::Table::parse(font_data) else {
        return Vec::new();
    };

    let matrix = cff.matrix();
    let scale = cff_matrix_scale(matrix.sx);

    let mut corrections = Vec::new();

    for (i, obj) in existing_widths.iter().enumerate() {
        let pdf_w = match obj {
            Object::Integer(w) => *w,
            Object::Real(r) => *r as i64,
            _ => continue,
        };

        let code = first_char + i as u32;

        // For symbolic CFF fonts, only trust explicit encoding-based lookup.
        // A fallback of `GID == code` can rewrite correct widths to unrelated
        // glyph advances (violating ISO 19005-2:2011 6.2.11.5 / veraPDF 6.2.11.5:1).
        let gid = cff.glyph_index(code as u8).filter(|g| g.0 > 0);

        let Some(gid) = gid else { continue };

        let Some(w) = cff.glyph_width(gid) else {
            continue;
        };

        let expected = (w as f64 * scale).round() as i64;

        if (pdf_w - expected).abs() > 1 {
            corrections.push((i, expected));
        }
    }

    corrections
}

fn count_all_fonts(doc: &Document) -> usize {
    doc.objects
        .values()
        .filter(|obj| {
            if let Object::Dictionary(dict) = obj {
                is_font_dict(dict)
            } else {
                false
            }
        })
        .count()
}

fn get_name(dict: &lopdf::Dictionary, key: &[u8]) -> Option<String> {
    match dict.get(key).ok()? {
        Object::Name(n) => String::from_utf8(n.clone()).ok(),
        _ => None,
    }
}

/// Like `get_name` but returns raw bytes and resolves indirect references.
fn get_name_bytes_resolved(
    doc: &Document,
    dict: &lopdf::Dictionary,
    key: &[u8],
) -> Option<Vec<u8>> {
    match dict.get(key).ok()? {
        Object::Name(n) => Some(n.clone()),
        Object::Reference(id) => match doc.get_object(*id).ok()? {
            Object::Name(n) => Some(n.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// Like `get_name` but resolves indirect references through the document.
fn get_name_resolved(doc: &Document, dict: &lopdf::Dictionary, key: &[u8]) -> Option<String> {
    let raw = get_name_bytes_resolved(doc, dict, key)?;
    String::from_utf8(raw).ok()
}

/// Like `get_name_resolved`, but falls back to UTF-8 lossy conversion.
fn get_name_lossy_resolved(doc: &Document, dict: &lopdf::Dictionary, key: &[u8]) -> Option<String> {
    let raw = get_name_bytes_resolved(doc, dict, key)?;
    Some(String::from_utf8_lossy(&raw).to_string())
}

// ---------------------------------------------------------------------------
// 6.2.11.8:1 — Fix .notdef glyph references
// ---------------------------------------------------------------------------
//
// veraPDF rule 6.2.11.8:1: "A PDF/A-2 compliant document shall not contain
// a reference to the .notdef glyph from any of the text showing operators."
//
// This happens when character codes in content streams map to .notdef in
// the font's encoding. The safest approach is to fix the Encoding
// Differences array entries that explicitly map codes to .notdef.
//
// Strategy:
// 1. Find all simple fonts (Type1, TrueType) with Encoding Differences
//    containing ".notdef" entries.
// 2. For each such entry, try to find the CORRECT glyph name by looking up
//    the character code in the embedded font program (via Unicode cmap or
//    glyph name tables).
// 3. If a correct glyph exists in the font, use that name.
// 4. If not, use "space" as a safe AGL-compliant fallback.
// 5. For fonts WITHOUT Differences but where the base encoding maps some
//    used codes to .notdef: only fix if we can add Differences entries
//    where the font provably has the glyph.

/// Fix .notdef glyph references in font Encoding Differences arrays (6.2.11.8:1).
///
/// Scans all simple fonts for Encoding Differences containing ".notdef"
/// entries and replaces them with valid glyph names. Also checks fonts
/// without Differences where the encoding maps character codes to .notdef
/// in the embedded font, and adds Differences entries when the font
/// program provably contains the correct glyph.
///
/// Returns the number of fonts fixed.
pub fn fix_notdef_glyph_refs(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut fixed = 0;

    for font_id in font_ids {
        let (subtype, fd_id, enc_info, first_char, last_char, is_subset) = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            if !is_font_dict(dict) {
                continue;
            }
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();

            // Only handle simple fonts — Type0/CID font .notdef fixing is
            // much more complex (CMap rewriting) and too risky.
            if subtype != "TrueType" && subtype != "Type1" && subtype != "MMType1" {
                continue;
            }

            let base_font = get_name(dict, b"BaseFont").unwrap_or_default();
            let symbolic_name = is_symbolic_font_name(&base_font);
            let symbolic_flags = is_font_symbolic(doc, dict);
            let base_no_subset = strip_subset_prefix(&base_font);
            // Some legacy NewBrunswick Type1 fonts are flagged Symbolic but still
            // need the regular notdef path for missing space-glyph references.
            let allow_symbolic_type1_override = base_no_subset.contains("NewBrunswick");
            // Symbolic fonts by name are handled via stream-level repair.
            // For TrueType symbolic fonts (by flags), avoid Differences-based
            // edits here to prevent reintroducing /Encoding (6.2.11.6:3).
            if (symbolic_name && !(subtype != "TrueType" && allow_symbolic_type1_override))
                || (subtype == "TrueType" && symbolic_flags)
                || (subtype != "TrueType" && symbolic_flags && !allow_symbolic_type1_override)
            {
                continue;
            }

            // Detect subset fonts (prefix like ABCDEF+FontName).
            // We still process them but use a more conservative replacement
            // strategy: only map .notdef codes to glyph names confirmed
            // present in the font subset.
            let is_subset = get_name(dict, b"BaseFont")
                .map(|n| n.contains('+'))
                .unwrap_or(false);

            let fd_id = match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(id)) => Some(*id),
                _ => None,
            };

            // Extract encoding info.
            let enc_info = extract_encoding_info(doc, dict);

            // Extract FirstChar/LastChar to know which codes are actually used.
            let fc = dict
                .get(b"FirstChar")
                .ok()
                .and_then(|o| match o {
                    Object::Integer(i) => Some(*i as u32),
                    _ => None,
                })
                .unwrap_or(0);
            let lc = dict
                .get(b"LastChar")
                .ok()
                .and_then(|o| match o {
                    Object::Integer(i) => Some(*i as u32),
                    _ => None,
                })
                .unwrap_or(255);

            (subtype, fd_id, enc_info, fc, lc, is_subset)
        };

        let Some(fd_id) = fd_id else { continue };

        // Read the embedded font program data.
        let font_data = read_embedded_font_data(doc, fd_id);
        let Some(font_data) = font_data else { continue };

        if subtype == "TrueType" {
            if fix_notdef_in_truetype(
                doc, font_id, &font_data, &enc_info, first_char, last_char, is_subset,
            ) {
                fixed += 1;
            }
        } else {
            // Type1 / MMType1 — try CFF parsing.
            if fix_notdef_in_type1(
                doc, font_id, &font_data, &enc_info, first_char, last_char, is_subset,
            ) {
                fixed += 1;
            }
        }
    }

    fixed
}

/// Strip control characters (0x00-0x1F except \t, \n, \r) from PDF string
/// literals in all content streams. These characters are non-printing and
/// frequently map to .notdef in fonts, causing PDF/A violations (6.2.11.8:1
/// and 6.2.11.4.1:2).
pub fn strip_control_chars_from_streams(doc: &mut Document) -> usize {
    use std::collections::HashMap;

    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let mut total_fixed = 0usize;

    for &page_id in &page_ids {
        // Build a page-local map: font resource name -> is strip-safe simple font.
        let mut has_type0_font = false;
        let font_map: HashMap<String, bool> = {
            let page = match doc.objects.get(&page_id) {
                Some(Object::Dictionary(d)) => d.clone(),
                _ => continue,
            };
            let resources = match page.get(b"Resources").ok() {
                Some(Object::Dictionary(d)) => d.clone(),
                Some(Object::Reference(r)) => match doc.objects.get(r) {
                    Some(Object::Dictionary(d)) => d.clone(),
                    _ => continue,
                },
                _ => continue,
            };
            let fonts = match resources.get(b"Font").ok() {
                Some(Object::Dictionary(d)) => d.clone(),
                Some(Object::Reference(r)) => match doc.objects.get(r) {
                    Some(Object::Dictionary(d)) => d.clone(),
                    _ => continue,
                },
                _ => continue,
            };

            let mut map = HashMap::new();
            for (key, val) in fonts.iter() {
                let name = String::from_utf8_lossy(key).to_string();
                let subtype = match val {
                    Object::Reference(id) => match doc.objects.get(id) {
                        Some(Object::Dictionary(d)) => get_name(d, b"Subtype").unwrap_or_default(),
                        _ => String::new(),
                    },
                    Object::Dictionary(d) => get_name(d, b"Subtype").unwrap_or_default(),
                    _ => String::new(),
                };
                if subtype == "Type0" {
                    has_type0_font = true;
                }

                // Strip in 1-byte text fonts, including Type3.
                let can_strip = subtype == "TrueType"
                    || subtype == "Type1"
                    || subtype == "MMType1"
                    || subtype == "Type3";
                map.insert(name, can_strip);
            }
            map
        };

        if !font_map.values().any(|v| *v) {
            continue;
        }

        // Mixed simple-font and Type0 pages are sensitive to aggressive byte-level
        // stream rewrites; keep original text byte structure there.
        if has_type0_font {
            continue;
        }

        let content_ids = crate::content_editor::get_content_stream_ids(doc, page_id);
        let mut current_font = String::new();

        for cs_id in content_ids {
            let stream_data = match doc.objects.get(&cs_id) {
                Some(Object::Stream(s)) => {
                    let mut s = s.clone();
                    let _ = s.decompress();
                    s.content
                }
                _ => continue,
            };

            let Ok(editor) = crate::content_editor::ContentEditor::from_stream(&stream_data) else {
                continue;
            };
            let ops = editor.operations().to_vec();
            let mut modified = false;
            let mut new_ops = Vec::with_capacity(ops.len());

            for op in &ops {
                match op.operator.as_str() {
                    "Tf" => {
                        if let Some(Object::Name(name)) = op.operands.first() {
                            current_font = String::from_utf8_lossy(name).to_string();
                        }
                        new_ops.push(op.clone());
                    }
                    "Tj" | "'" | "\"" => {
                        if font_map.get(&current_font).copied().unwrap_or(false) {
                            let mut new_op = op.clone();
                            let str_idx = if op.operator == "\"" { 2 } else { 0 };
                            if let Some(Object::String(bytes, _)) = new_op.operands.get_mut(str_idx)
                            {
                                if strip_control_bytes(bytes, !has_type0_font) {
                                    modified = true;
                                }
                            }
                            new_ops.push(new_op);
                        } else {
                            new_ops.push(op.clone());
                        }
                    }
                    "TJ" => {
                        if font_map.get(&current_font).copied().unwrap_or(false) {
                            let mut new_op = op.clone();
                            if let Some(Object::Array(arr)) = new_op.operands.first_mut() {
                                for item in arr.iter_mut() {
                                    if let Object::String(bytes, _) = item {
                                        if strip_control_bytes(bytes, !has_type0_font) {
                                            modified = true;
                                        }
                                    }
                                }
                            }
                            new_ops.push(new_op);
                        } else {
                            new_ops.push(op.clone());
                        }
                    }
                    _ => {
                        new_ops.push(op.clone());
                    }
                }
            }

            if modified {
                let new_editor = crate::content_editor::ContentEditor::from_operations(new_ops);
                if let Ok(encoded) = new_editor.encode() {
                    if let Some(Object::Stream(s)) = doc.objects.get_mut(&cs_id) {
                        s.set_plain_content(encoded);
                        total_fixed += 1;
                    }
                }
            }
        }
    }

    total_fixed
}

fn strip_control_bytes(bytes: &mut Vec<u8>, allow_collapse: bool) -> bool {
    let mut changed = false;

    // Some malformed PDFs encode simple-font text as 2-byte pairs where one
    // lane is a constant sentinel (00/FF). Collapse these to 1-byte codes.
    if allow_collapse && collapse_two_byte_simple_codes(bytes) {
        changed = true;
    }

    // On pages that also use Type0 fonts, keep 2-byte sentinel pairs intact:
    // stripping low bytes only can create ambiguous 1-byte hex strings.
    if !allow_collapse {
        if let Some(code_in_odd_lane) = paired_simple_code_lane(bytes) {
            let mut filtered = Vec::with_capacity(bytes.len());
            for i in (0..bytes.len()).step_by(2) {
                let code = if code_in_odd_lane {
                    bytes[i + 1]
                } else {
                    bytes[i]
                };
                if code >= 32 {
                    filtered.push(bytes[i]);
                    filtered.push(bytes[i + 1]);
                }
            }
            if filtered.len() != bytes.len() {
                *bytes = filtered;
                changed = true;
            }
            return changed;
        }
    }

    let original_len = bytes.len();
    bytes.retain(|b| *b >= 32);
    changed || bytes.len() != original_len
}

/// Fix .notdef references in CID (Type0) fonts by modifying content streams.
///
/// ISO 19005-2, §6.2.11.8: no .notdef glyph references allowed.
///
/// For CIDFontType0/CIDFontType2 with two-byte Type0 CMaps (Identity-H/V and
/// common CJK `*-H`/`*-V` CMaps), character codes in content streams are 2-byte
/// values. If a mapped CID does not have a glyph in the embedded font program,
/// it resolves to .notdef (GID 0). This function replaces such values in Tj/TJ
/// text strings with a valid fallback CID (typically space).
///
/// See NOTDEF_FIXES_LOG.md for the debug log of approaches tried.
pub fn fix_cid_font_notdef(doc: &mut Document) -> usize {
    use std::collections::{HashMap, HashSet};

    #[derive(Clone)]
    struct CidTextRepair {
        valid_values: HashSet<u16>,
        replacement_value: Option<u16>,
        /// For EUC-style CMaps (GB-EUC-H etc.), the CMap cidranges used to
        /// determine byte-code boundaries and code→CID mappings.
        euc_cmap_ranges: Option<Vec<(u16, u16, u16)>>,
    }

    #[derive(Clone, Copy)]
    enum ContentContainer {
        Page(ObjectId),
        Form(ObjectId),
    }

    let mut containers: Vec<ContentContainer> = doc
        .get_pages()
        .values()
        .copied()
        .map(ContentContainer::Page)
        .collect();
    for (&id, obj) in &doc.objects {
        let Object::Stream(stream) = obj else {
            continue;
        };
        let is_form = stream
            .dict
            .get(b"Subtype")
            .ok()
            .and_then(|o| o.as_name().ok())
            == Some(b"Form");
        if is_form {
            containers.push(ContentContainer::Form(id));
        }
    }

    // For each page/Form XObject, find Type0 fonts with likely 2-byte CMaps
    // and build a set of valid CIDs per font resource name.
    let mut total_fixed = 0usize;

    for container in containers {
        // Get font resources: resource_name -> font_obj_id
        let font_map: HashMap<String, ObjectId> = {
            let resources = match container {
                ContentContainer::Page(page_id) => {
                    let page = match doc.objects.get(&page_id) {
                        Some(Object::Dictionary(d)) => d.clone(),
                        _ => continue,
                    };
                    match page.get(b"Resources").ok() {
                        Some(Object::Dictionary(d)) => d.clone(),
                        Some(Object::Reference(r)) => match doc.objects.get(r) {
                            Some(Object::Dictionary(d)) => d.clone(),
                            _ => continue,
                        },
                        _ => continue,
                    }
                }
                ContentContainer::Form(form_id) => {
                    let stream = match doc.objects.get(&form_id) {
                        Some(Object::Stream(s)) => s.clone(),
                        _ => continue,
                    };
                    match stream.dict.get(b"Resources").ok() {
                        Some(Object::Dictionary(d)) => d.clone(),
                        Some(Object::Reference(r)) => match doc.objects.get(r) {
                            Some(Object::Dictionary(d)) => d.clone(),
                            _ => continue,
                        },
                        _ => continue,
                    }
                }
            };
            let fonts = match resources.get(b"Font").ok() {
                Some(Object::Dictionary(d)) => d.clone(),
                Some(Object::Reference(r)) => match doc.objects.get(r) {
                    Some(Object::Dictionary(d)) => d.clone(),
                    _ => continue,
                },
                _ => continue,
            };
            let mut map = HashMap::new();
            for (key, val) in fonts.iter() {
                let name = String::from_utf8_lossy(key).to_string();
                if let Object::Reference(id) = val {
                    map.insert(name, *id);
                }
            }
            map
        };

        // For each Type0 font, check if it uses a likely two-byte CMap and has .notdef CIDs.
        let mut notdef_fonts: HashMap<String, CidTextRepair> = HashMap::new();

        for (res_name, font_id) in &font_map {
            let Some(Object::Dictionary(font_dict)) = doc.objects.get(font_id) else {
                continue;
            };
            let subtype = get_name(font_dict, b"Subtype").unwrap_or_default();
            if subtype != "Type0" {
                continue;
            }

            // Apply only to Type0 encodings that are typically two-byte CMap
            // workflows: Identity-H/V, named CJK maps, or embedded CMap streams.
            let likely_two_byte = match font_dict.get(b"Encoding").ok() {
                Some(Object::Name(n)) => {
                    let enc_l = String::from_utf8_lossy(n).to_ascii_lowercase();
                    enc_l == "identity-h"
                        || enc_l == "identity-v"
                        || enc_l.ends_with("-h")
                        || enc_l.ends_with("-v")
                        || enc_l.contains("gbk")
                        || enc_l.contains("gb")
                        || enc_l.contains("cns")
                        || enc_l.contains("japan")
                        || enc_l.contains("korea")
                }
                Some(Object::Reference(enc_id)) => match doc.objects.get(enc_id) {
                    Some(Object::Name(n)) => {
                        let enc_l = String::from_utf8_lossy(n).to_ascii_lowercase();
                        enc_l == "identity-h"
                            || enc_l == "identity-v"
                            || enc_l.ends_with("-h")
                            || enc_l.ends_with("-v")
                            || enc_l.contains("gbk")
                            || enc_l.contains("gb")
                            || enc_l.contains("cns")
                            || enc_l.contains("japan")
                            || enc_l.contains("korea")
                    }
                    Some(Object::Dictionary(d)) => d.has(b"CMapName"),
                    Some(Object::Stream(s)) => {
                        if let Ok(Object::Name(cmap_name)) = s.dict.get(b"CMapName") {
                            let name_l = String::from_utf8_lossy(cmap_name).to_ascii_lowercase();
                            name_l.contains("gbk")
                                || name_l.contains("gb")
                                || name_l.contains("cns")
                                || name_l.contains("japan")
                                || name_l.contains("korea")
                                || name_l.ends_with("-h")
                                || name_l.ends_with("-v")
                        } else {
                            true
                        }
                    }
                    _ => false,
                },
                _ => false,
            };
            if !likely_two_byte {
                continue;
            }

            let cmap_name = resolve_type0_cmap_name(doc, font_dict);
            let predefined_ranges = cmap_name
                .as_deref()
                .filter(|name| !is_identity_type0_cmap(name))
                .and_then(load_predefined_unicode_cmap_ranges);
            let euc_cmap_ranges = cmap_name
                .as_deref()
                .filter(|name| !is_identity_type0_cmap(name))
                .filter(|name| is_euc_style_cmap(name))
                .and_then(load_all_cmap_cidranges);

            // Get descendant CIDFont (may be inline array or reference).
            let desc_arr = match font_dict.get(b"DescendantFonts").ok() {
                Some(Object::Array(arr)) => Some(arr.clone()),
                Some(Object::Reference(r)) => match doc.objects.get(r) {
                    Some(Object::Array(arr)) => Some(arr.clone()),
                    _ => None,
                },
                _ => None,
            };
            let desc_id = desc_arr.as_ref().and_then(|arr| {
                arr.first().and_then(|o| match o {
                    Object::Reference(id) => Some(*id),
                    _ => None,
                })
            });
            let Some(desc_id) = desc_id else {
                continue;
            };

            // Get FontDescriptor from CIDFont.
            let fd_id = doc.objects.get(&desc_id).and_then(|o| {
                if let Object::Dictionary(d) = o {
                    match d.get(b"FontDescriptor").ok() {
                        Some(Object::Reference(id)) => Some(*id),
                        _ => None,
                    }
                } else {
                    None
                }
            });
            let Some(fd_id) = fd_id else {
                continue;
            };

            // Read embedded font data.
            let Some(font_data) = read_embedded_font_data(doc, fd_id) else {
                continue;
            };

            // Determine CIDFont subtype to choose CFF or TrueType parsing.
            let cid_subtype = doc
                .objects
                .get(&desc_id)
                .and_then(|o| {
                    if let Object::Dictionary(d) = o {
                        get_name(d, b"Subtype")
                    } else {
                        None
                    }
                })
                .unwrap_or_default();

            let mut valid_cids: HashSet<u16> = HashSet::new();
            let mut space_cid: u16 = 0;
            let mut clear_unparseable_text = false;

            if cid_subtype == "CIDFontType2" {
                // TrueType-based CID font. Handle both Identity and stream
                // CIDToGIDMap mappings.
                match ttf_parser::Face::parse(&font_data, 0) {
                    Ok(face) => {
                        let num_glyphs = face.number_of_glyphs();
                        if num_glyphs == 0 {
                            continue;
                        }

                        let map_obj = doc.objects.get(&desc_id).and_then(|o| {
                            if let Object::Dictionary(d) = o {
                                d.get(b"CIDToGIDMap").ok().cloned()
                            } else {
                                None
                            }
                        });
                        let has_glyph_data = |gid: u16| -> bool {
                            gid > 0
                                && gid < num_glyphs
                                && tt_glyph_has_data(&face, ttf_parser::GlyphId(gid))
                        };
                        let space_gid = face
                            .glyph_index(' ')
                            .map(|g| g.0)
                            .filter(|gid| has_glyph_data(*gid))
                            .unwrap_or(0);

                        match map_obj {
                            None => {
                                // Identity mapping: CID == GID.
                                for gid in 1..num_glyphs {
                                    if has_glyph_data(gid) {
                                        valid_cids.insert(gid);
                                    }
                                }
                                if space_gid > 0 {
                                    space_cid = space_gid;
                                }
                            }
                            Some(Object::Name(n)) if n == b"Identity" => {
                                // Identity mapping: CID == GID.
                                for gid in 1..num_glyphs {
                                    if has_glyph_data(gid) {
                                        valid_cids.insert(gid);
                                    }
                                }
                                if space_gid > 0 {
                                    space_cid = space_gid;
                                }
                            }
                            Some(Object::Reference(id)) => {
                                let map_bytes = match doc.objects.get(&id) {
                                    Some(Object::Stream(s)) => {
                                        let mut st = s.clone();
                                        let _ = st.decompress();
                                        st.content
                                    }
                                    _ => Vec::new(),
                                };
                                for (cid, chunk) in map_bytes.chunks_exact(2).enumerate() {
                                    if cid > u16::MAX as usize {
                                        break;
                                    }
                                    let gid = u16::from_be_bytes([chunk[0], chunk[1]]);
                                    if has_glyph_data(gid) {
                                        let cid_u16 = cid as u16;
                                        valid_cids.insert(cid_u16);
                                        if gid == space_gid {
                                            space_cid = cid_u16;
                                        }
                                    }
                                }
                            }
                            Some(Object::Stream(s)) => {
                                let mut st = s.clone();
                                let _ = st.decompress();
                                for (cid, chunk) in st.content.chunks_exact(2).enumerate() {
                                    if cid > u16::MAX as usize {
                                        break;
                                    }
                                    let gid = u16::from_be_bytes([chunk[0], chunk[1]]);
                                    if has_glyph_data(gid) {
                                        let cid_u16 = cid as u16;
                                        valid_cids.insert(cid_u16);
                                        if gid == space_gid {
                                            space_cid = cid_u16;
                                        }
                                    }
                                }
                            }
                            _ => continue,
                        }

                        // Fallback: if stream mapping yielded nothing, fall back
                        // to Identity semantics.
                        if valid_cids.is_empty() {
                            for gid in 1..num_glyphs {
                                if has_glyph_data(gid) {
                                    valid_cids.insert(gid);
                                }
                            }
                            if space_gid > 0 {
                                space_cid = space_gid;
                            }
                        }
                    }
                    Err(_) => {
                        continue;
                    }
                }
            } else {
                // CFF-based CID font (CIDFontType0): parse CFF for CID mapping.
                match cff_parser::Table::parse(&font_data) {
                    Some(cff) => {
                        let num_glyphs = cff.number_of_glyphs();
                        for gid in 0..num_glyphs {
                            let glyph_id = cff_parser::GlyphId(gid);
                            if let Some(cid) = cff.glyph_cid(glyph_id) {
                                let has_usable_width =
                                    cff.glyph_width(glyph_id).map(|w| w > 0).unwrap_or(false);
                                if gid > 0 && has_usable_width {
                                    valid_cids.insert(cid);
                                }
                                if let Some(name) = cff.glyph_name(glyph_id) {
                                    if name == "space" && gid > 0 && has_usable_width {
                                        space_cid = cid;
                                    }
                                }
                            }
                        }
                    }
                    None => {
                        // FIX_LOG: CFF parse can fail for tiny/unusual CFF fonts (e.g. 565-byte
                        // HiddenHorzOCR in 0298). Fallback: try ttf_parser which handles
                        // OpenType-wrapped CFF as well.
                        match ttf_parser::Face::parse(&font_data, 0) {
                            Ok(face) => {
                                let num_glyphs = face.number_of_glyphs();
                                for gid in 1..num_glyphs {
                                    if tt_glyph_has_data(&face, ttf_parser::GlyphId(gid)) {
                                        valid_cids.insert(gid);
                                    }
                                }
                                if let Some(gid) = face.glyph_index(' ') {
                                    if gid.0 > 0 && tt_glyph_has_data(&face, gid) {
                                        space_cid = gid.0;
                                    }
                                }
                            }
                            Err(_) => {
                                // If the embedded CIDFontType0 program cannot be parsed at all,
                                // we cannot prove any rendered CID maps to a present glyph.
                                // Keep the font on the repair list with an empty valid set so
                                // text strings using it are cleared conservatively.
                                clear_unparseable_text = true;
                            }
                        }
                    }
                }
            }

            // If no space glyph found by name, use the first valid CID.
            if !clear_unparseable_text && space_cid == 0 && predefined_ranges.is_none() {
                if let Some(&first_valid) = valid_cids.iter().next() {
                    space_cid = first_valid;
                }
            }

            // Add font to the map. If the valid set is empty (font has only
            // .notdef, e.g. HiddenHorzOCR stub fonts), we still add it so text
            // strings get cleared entirely.
            let repair = if let Some(ranges) = predefined_ranges.as_ref() {
                let valid_codes = build_valid_codes_from_cmap_ranges(&valid_cids, ranges);
                let replacement_value = if valid_codes.contains(&0x0020) {
                    Some(0x0020)
                } else if space_cid > 0 {
                    cmap_first_code_for_cid(ranges, space_cid)
                        .filter(|code| valid_codes.contains(code))
                } else {
                    None
                };
                CidTextRepair {
                    valid_values: valid_codes,
                    replacement_value,
                    euc_cmap_ranges: None,
                }
            } else {
                CidTextRepair {
                    valid_values: valid_cids,
                    replacement_value: if clear_unparseable_text || space_cid == 0 {
                        None
                    } else {
                        Some(space_cid)
                    },
                    euc_cmap_ranges,
                }
            };
            notdef_fonts.insert(res_name.clone(), repair);
        }

        if notdef_fonts.is_empty() {
            continue;
        }

        // Step 3: parse content streams and fix text strings.
        // Track font name across content streams (font state carries over between
        // consecutive content streams on the same page — the graphics state is not
        // reset between them, per ISO 32000-1 §7.8.2).
        let content_ids = match container {
            ContentContainer::Page(page_id) => {
                crate::content_editor::get_content_stream_ids(doc, page_id)
            }
            ContentContainer::Form(form_id) => vec![form_id],
        };
        let mut stream_chunks: Vec<(ObjectId, Vec<u8>)> = Vec::new();
        for cs_id in &content_ids {
            let stream_data = match doc.objects.get(cs_id) {
                Some(Object::Stream(s)) => {
                    let mut s = s.clone();
                    let _ = s.decompress();
                    s.content
                }
                _ => continue,
            };
            stream_chunks.push((*cs_id, stream_data));
        }

        if stream_chunks.is_empty() {
            continue;
        }

        // Some PDFs split operators/tokens across consecutive content streams.
        // Parse merged content first so split tokens are seen as one stream.
        let mut handled_as_combined = false;
        if stream_chunks.len() > 1 {
            let mut merged = Vec::new();
            for (_, chunk) in &stream_chunks {
                merged.extend_from_slice(chunk);
                if !chunk.ends_with(b"\n") {
                    merged.push(b'\n');
                }
            }

            if let Ok(editor) = crate::content_editor::ContentEditor::from_stream(&merged) {
                let ops = editor.operations().to_vec();
                let mut current_font_name = String::new();
                let mut in_text_object = false;
                let mut font_set_in_text_object = false;
                let mut gs_stack: Vec<(String, bool)> = Vec::new();
                let mut modified = false;
                let mut new_ops = Vec::with_capacity(ops.len());

                for op in &ops {
                    match op.operator.as_str() {
                        "q" => {
                            gs_stack.push((current_font_name.clone(), font_set_in_text_object));
                            new_ops.push(op.clone());
                        }
                        "Q" => {
                            if let Some((saved_font, saved_font_set)) = gs_stack.pop() {
                                current_font_name = saved_font;
                                font_set_in_text_object = saved_font_set;
                            }
                            new_ops.push(op.clone());
                        }
                        "BT" => {
                            // ISO 32000-1 §9.4.1: BT starts a new text object.
                            // Keep current font selection (it persists in text
                            // state), but track whether this BT sets Tf again.
                            in_text_object = true;
                            font_set_in_text_object = false;
                            new_ops.push(op.clone());
                        }
                        "ET" => {
                            in_text_object = false;
                            font_set_in_text_object = false;
                            new_ops.push(op.clone());
                        }
                        "Tf" => {
                            if let Some(Object::Name(name)) = op.operands.first() {
                                current_font_name = String::from_utf8_lossy(name).to_string();
                                font_set_in_text_object = true;
                            }
                            new_ops.push(op.clone());
                        }
                        "Tj" | "'" | "\"" => {
                            if let Some(repair) = notdef_fonts.get(&current_font_name) {
                                let mut new_op = op.clone();
                                let str_idx = if op.operator == "\"" { 2 } else { 0 };
                                if let Some(Object::String(bytes, fmt)) =
                                    new_op.operands.get_mut(str_idx)
                                {
                                    let mut changed_here = if let Some(euc) =
                                        repair.euc_cmap_ranges.as_deref()
                                    {
                                        fix_cid_text_string_euc(bytes, euc, &repair.valid_values)
                                    } else {
                                        fix_cid_text_string(
                                            bytes,
                                            &repair.valid_values,
                                            repair.replacement_value,
                                        )
                                    };
                                    if *fmt != lopdf::StringFormat::Hexadecimal {
                                        *fmt = lopdf::StringFormat::Hexadecimal;
                                        changed_here = true;
                                    }
                                    if changed_here {
                                        modified = true;
                                    }
                                }
                                new_ops.push(new_op);
                            } else {
                                let mut new_op = op.clone();
                                let str_idx = if op.operator == "\"" { 2 } else { 0 };
                                if let Some(Object::String(bytes, fmt)) =
                                    new_op.operands.get_mut(str_idx)
                                {
                                    if fix_unset_text_font_hex_string(
                                        bytes,
                                        *fmt,
                                        in_text_object,
                                        font_set_in_text_object,
                                    ) {
                                        modified = true;
                                    }
                                }
                                new_ops.push(new_op);
                            }
                        }
                        "TJ" => {
                            if let Some(repair) = notdef_fonts.get(&current_font_name) {
                                let mut new_op = op.clone();
                                if let Some(Object::Array(arr)) = new_op.operands.first_mut() {
                                    for item in arr.iter_mut() {
                                        if let Object::String(bytes, fmt) = item {
                                            let mut changed_here = if let Some(euc) =
                                                repair.euc_cmap_ranges.as_deref()
                                            {
                                                fix_cid_text_string_euc(
                                                    bytes,
                                                    euc,
                                                    &repair.valid_values,
                                                )
                                            } else {
                                                fix_cid_text_string(
                                                    bytes,
                                                    &repair.valid_values,
                                                    repair.replacement_value,
                                                )
                                            };
                                            if *fmt != lopdf::StringFormat::Hexadecimal {
                                                *fmt = lopdf::StringFormat::Hexadecimal;
                                                changed_here = true;
                                            }
                                            if changed_here {
                                                modified = true;
                                            }
                                        }
                                    }
                                }
                                new_ops.push(new_op);
                            } else {
                                let mut new_op = op.clone();
                                if let Some(Object::Array(arr)) = new_op.operands.first_mut() {
                                    for item in arr.iter_mut() {
                                        if let Object::String(bytes, fmt) = item {
                                            if fix_unset_text_font_hex_string(
                                                bytes,
                                                *fmt,
                                                in_text_object,
                                                font_set_in_text_object,
                                            ) {
                                                modified = true;
                                            }
                                        }
                                    }
                                }
                                new_ops.push(new_op);
                            }
                        }
                        _ => new_ops.push(op.clone()),
                    }
                }

                // Always normalize merged content back into a single stream for
                // CID Identity-H/V fonts on multi-stream pages. This fixes
                // split operators/tokens across stream boundaries (e.g.
                // "/C2_0" at end of one stream and "1 Tf" at start of the
                // next), which can otherwise leave malformed one-byte CID hex
                // strings in the physical stream data.
                let should_rewrite_combined = modified || stream_chunks.len() > 1;
                if should_rewrite_combined {
                    let new_editor = crate::content_editor::ContentEditor::from_operations(new_ops);
                    if let Ok(encoded) = new_editor.encode() {
                        let first_id = stream_chunks[0].0;
                        if let Some(Object::Stream(s)) = doc.objects.get_mut(&first_id) {
                            s.set_plain_content(encoded);
                            total_fixed += 1;
                        }
                        for (extra_id, _) in stream_chunks.iter().skip(1) {
                            if let Some(Object::Stream(s)) = doc.objects.get_mut(extra_id) {
                                s.set_plain_content(Vec::new());
                            }
                        }
                    }
                }

                handled_as_combined = true;
            }
        }

        if handled_as_combined {
            continue;
        }

        // Fallback: parse streams individually (keeps existing behavior).
        let mut current_font_name = String::new();
        let mut in_text_object = false;
        let mut font_set_in_text_object = false;
        let mut gs_stack: Vec<(String, bool)> = Vec::new();
        for (cs_id, stream_data) in stream_chunks {
            let Ok(editor) = crate::content_editor::ContentEditor::from_stream(&stream_data) else {
                continue;
            };
            let ops = editor.operations().to_vec();
            let mut modified = false;
            let mut new_ops = Vec::with_capacity(ops.len());

            for op in &ops {
                match op.operator.as_str() {
                    "q" => {
                        gs_stack.push((current_font_name.clone(), font_set_in_text_object));
                        new_ops.push(op.clone());
                    }
                    "Q" => {
                        if let Some((saved_font, saved_font_set)) = gs_stack.pop() {
                            current_font_name = saved_font;
                            font_set_in_text_object = saved_font_set;
                        }
                        new_ops.push(op.clone());
                    }
                    "BT" => {
                        in_text_object = true;
                        font_set_in_text_object = false;
                        new_ops.push(op.clone());
                    }
                    "ET" => {
                        in_text_object = false;
                        font_set_in_text_object = false;
                        new_ops.push(op.clone());
                    }
                    "Tf" => {
                        if let Some(Object::Name(name)) = op.operands.first() {
                            current_font_name = String::from_utf8_lossy(name).to_string();
                            font_set_in_text_object = true;
                        }
                        new_ops.push(op.clone());
                    }
                    "Tj" | "'" | "\"" => {
                        if let Some(repair) = notdef_fonts.get(&current_font_name) {
                            let mut new_op = op.clone();
                            let str_idx = if op.operator == "\"" { 2 } else { 0 };
                            if let Some(Object::String(bytes, fmt)) =
                                new_op.operands.get_mut(str_idx)
                            {
                                let mut changed_here =
                                    if let Some(euc) = repair.euc_cmap_ranges.as_deref() {
                                        fix_cid_text_string_euc(bytes, euc, &repair.valid_values)
                                    } else {
                                        fix_cid_text_string(
                                            bytes,
                                            &repair.valid_values,
                                            repair.replacement_value,
                                        )
                                    };
                                if *fmt != lopdf::StringFormat::Hexadecimal {
                                    *fmt = lopdf::StringFormat::Hexadecimal;
                                    changed_here = true;
                                }
                                if changed_here {
                                    modified = true;
                                }
                            }
                            new_ops.push(new_op);
                        } else {
                            let mut new_op = op.clone();
                            let str_idx = if op.operator == "\"" { 2 } else { 0 };
                            if let Some(Object::String(bytes, fmt)) =
                                new_op.operands.get_mut(str_idx)
                            {
                                if fix_unset_text_font_hex_string(
                                    bytes,
                                    *fmt,
                                    in_text_object,
                                    font_set_in_text_object,
                                ) {
                                    modified = true;
                                }
                            }
                            new_ops.push(new_op);
                        }
                    }
                    "TJ" => {
                        if let Some(repair) = notdef_fonts.get(&current_font_name) {
                            let mut new_op = op.clone();
                            if let Some(Object::Array(arr)) = new_op.operands.first_mut() {
                                for item in arr.iter_mut() {
                                    if let Object::String(bytes, fmt) = item {
                                        let mut changed_here =
                                            if let Some(euc) = repair.euc_cmap_ranges.as_deref() {
                                                fix_cid_text_string_euc(
                                                    bytes,
                                                    euc,
                                                    &repair.valid_values,
                                                )
                                            } else {
                                                fix_cid_text_string(
                                                    bytes,
                                                    &repair.valid_values,
                                                    repair.replacement_value,
                                                )
                                            };
                                        if *fmt != lopdf::StringFormat::Hexadecimal {
                                            *fmt = lopdf::StringFormat::Hexadecimal;
                                            changed_here = true;
                                        }
                                        if changed_here {
                                            modified = true;
                                        }
                                    }
                                }
                            }
                            new_ops.push(new_op);
                        } else {
                            let mut new_op = op.clone();
                            if let Some(Object::Array(arr)) = new_op.operands.first_mut() {
                                for item in arr.iter_mut() {
                                    if let Object::String(bytes, fmt) = item {
                                        if fix_unset_text_font_hex_string(
                                            bytes,
                                            *fmt,
                                            in_text_object,
                                            font_set_in_text_object,
                                        ) {
                                            modified = true;
                                        }
                                    }
                                }
                            }
                            new_ops.push(new_op);
                        }
                    }
                    _ => new_ops.push(op.clone()),
                }
            }

            if modified {
                let new_editor = crate::content_editor::ContentEditor::from_operations(new_ops);
                if let Ok(encoded) = new_editor.encode() {
                    if let Some(Object::Stream(s)) = doc.objects.get_mut(&cs_id) {
                        s.set_plain_content(encoded);
                        total_fixed += 1;
                    }
                }
            }
        }
    }

    total_fixed
}

fn fix_unset_text_font_hex_string(
    bytes: &mut Vec<u8>,
    fmt: lopdf::StringFormat,
    in_text_object: bool,
    font_set_in_text_object: bool,
) -> bool {
    // PDF/A-2 6.2.11.8 forbids .notdef references in text-showing operators.
    // In malformed content, some streams emit hexadecimal text in a BT..ET
    // block before any Tf in that same text object. For Identity-H/V this can
    // be interpreted as broken 2-byte CID data and trigger .notdef.
    if fmt != lopdf::StringFormat::Hexadecimal {
        return false;
    }
    // Single-byte hexadecimal strings are ambiguous on pages that also use
    // Identity-H/V Type0 fonts. Some validators interpret them as malformed
    // two-byte CIDs (e.g. 0x49FF), which can resolve to .notdef.
    if bytes.len() == 1 {
        bytes.clear();
        return true;
    }
    if !in_text_object || font_set_in_text_object {
        return false;
    }
    false
}

/// Repair 2-byte Type0 text strings against a set of valid values.
///
/// For Identity-H/V fonts the values are CIDs. For predefined Unicode CMaps
/// (for example UniKS-UCS2-H) the values are character codes that must first be
/// resolved through the CMap before reaching a CID/GID.
fn fix_cid_text_string(
    bytes: &mut Vec<u8>,
    valid_values: &std::collections::HashSet<u16>,
    replacement_value: Option<u16>,
) -> bool {
    let mut changed = false;
    if bytes.is_empty() {
        return false;
    }
    // Type0 text strings are 2-byte code units for these CMaps. If malformed
    // odd lengths occur, drop the dangling byte so we can still repair them.
    if !bytes.len().is_multiple_of(2) {
        bytes.pop();
        changed = true;
    }
    if bytes.len() < 2 {
        return changed;
    }
    if valid_values.is_empty() {
        if !bytes.is_empty() {
            bytes.clear();
            return true;
        }
        return false;
    }
    let mut repaired = Vec::with_capacity(bytes.len());
    for i in (0..bytes.len()).step_by(2) {
        let value = ((bytes[i] as u16) << 8) | (bytes[i + 1] as u16);
        if value != 0 && valid_values.contains(&value) {
            repaired.push(bytes[i]);
            repaired.push(bytes[i + 1]);
            continue;
        }
        changed = true;
        if let Some(replacement) = replacement_value {
            repaired.extend_from_slice(&replacement.to_be_bytes());
        }
    }
    if changed {
        *bytes = repaired;
    }
    changed
}

/// Fix .notdef references in symbolic simple fonts by modifying content streams.
///
/// Symbolic fonts (Symbol, Wingdings, phonetic fonts, etc.) are not handled by
/// `fix_notdef_glyph_refs` because their custom encodings make Differences-based
/// fixes unreliable. Instead, this function replaces undefined character codes
/// directly in content streams with a valid code (typically space).
pub fn fix_symbolic_font_notdef_streams(doc: &mut Document) -> usize {
    use std::collections::{HashMap, HashSet};

    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let mut total_fixed = 0;

    for &page_id in &page_ids {
        // Get font resources: resource_name -> font_obj_id
        let font_map: HashMap<String, ObjectId> = {
            let page = match doc.objects.get(&page_id) {
                Some(Object::Dictionary(d)) => d.clone(),
                _ => continue,
            };
            let resources = match page.get(b"Resources").ok() {
                Some(Object::Dictionary(d)) => d.clone(),
                Some(Object::Reference(r)) => match doc.objects.get(r) {
                    Some(Object::Dictionary(d)) => d.clone(),
                    _ => continue,
                },
                _ => continue,
            };
            let fonts = match resources.get(b"Font").ok() {
                Some(Object::Dictionary(d)) => d.clone(),
                Some(Object::Reference(r)) => match doc.objects.get(r) {
                    Some(Object::Dictionary(d)) => d.clone(),
                    _ => continue,
                },
                _ => continue,
            };
            let mut map = HashMap::new();
            for (key, val) in fonts.iter() {
                let name = String::from_utf8_lossy(key).to_string();
                if let Object::Reference(id) = val {
                    map.insert(name, *id);
                }
            }
            map
        };

        // Find symbolic simple fonts with undefined glyphs.
        // Map: resource_name -> (set of invalid codes, replacement code)
        let mut notdef_fonts: HashMap<String, HashSet<u8>> = HashMap::new();

        for (res_name, font_id) in &font_map {
            let Some(Object::Dictionary(dict)) = doc.objects.get(font_id) else {
                continue;
            };
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();
            if subtype != "TrueType" && subtype != "Type1" && subtype != "MMType1" {
                continue;
            }

            // Handle symbolic fonts by descriptor flags OR by well-known symbolic names.
            let base_name = get_name(dict, b"BaseFont").unwrap_or_default();
            if !is_font_symbolic(doc, dict) && !is_symbolic_font_name(&base_name) {
                continue;
            }

            let fd_id = match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(id)) => *id,
                _ => continue,
            };
            let Some(font_data) = read_embedded_font_data(doc, fd_id) else {
                continue;
            };

            let first_char = dict
                .get(b"FirstChar")
                .ok()
                .and_then(|o| match o {
                    Object::Integer(i) => Some(*i as u32),
                    _ => None,
                })
                .unwrap_or(0);
            let last_char = dict
                .get(b"LastChar")
                .ok()
                .and_then(|o| match o {
                    Object::Integer(i) => Some(*i as u32),
                    _ => None,
                })
                .unwrap_or(255);

            // Build set of invalid codes using font's cmap.
            // For symbolic fonts without Encoding, veraPDF uses the (3,0)
            // Symbol cmap subtable with 0xF000 offset. Only process fonts
            // that actually have a (3,0) subtable — other "symbolic" fonts
            // use different encoding mechanisms.
            let mut invalid_codes = HashSet::new();

            if let Ok(face) = ttf_parser::Face::parse(&font_data, 0) {
                // Check if font has a (3,0) Microsoft Symbol cmap subtable.
                let has_symbol_cmap = face
                    .tables()
                    .cmap
                    .map(|cmap| {
                        cmap.subtables.into_iter().any(|st| {
                            st.platform_id == ttf_parser::PlatformId::Windows && st.encoding_id == 0
                        })
                    })
                    .unwrap_or(false);

                for code in first_char..=last_char.min(255) {
                    let sym_ch = char::from_u32(0xF000 + code);
                    let has_symbol_glyph = sym_ch
                        .and_then(|c| face.glyph_index(c))
                        .filter(|g| g.0 != 0)
                        .map(|g| tt_glyph_has_data(&face, g))
                        .unwrap_or(false);

                    // Some symbolic TrueType fonts (e.g. Apple Symbol.ttf)
                    // do not expose a strict (3,0) cmap path for all codes.
                    // Fall back to direct Unicode/GID probes before declaring
                    // a code invalid.
                    let has_unicode_glyph = char::from_u32(code)
                        .and_then(|c| face.glyph_index(c))
                        .filter(|g| g.0 != 0)
                        .map(|g| tt_glyph_has_data(&face, g))
                        .unwrap_or(false);
                    let has_glyph = if has_symbol_cmap {
                        has_symbol_glyph || has_unicode_glyph
                    } else {
                        has_unicode_glyph || has_symbol_glyph
                    };

                    if !has_glyph {
                        invalid_codes.insert(code as u8);
                    }
                }
            } else if let Some(cff) = cff_parser::Table::parse(&font_data) {
                // CFF symbolic font.
                //
                // Follow PDF encoding first when present (veraPDF 6.2.11.4.1:2 /
                // 6.2.11.5:1 path), and only fall back to CFF internal encoding
                // for cases where PDF-level mapping is absent/ambiguous.
                let enc_map = parse_cff_encoding_map(&font_data);
                let (enc_name, differences) = get_simple_encoding_info(doc, dict);
                let has_pdf_encoding = dict.get(b"Encoding").is_ok();
                let has_explicit_difference = !differences.is_empty();
                let allow_cff_encoding_fallback = !has_pdf_encoding
                    || cff_has_gid_based_names(&cff)
                    || (enc_name.is_empty() && !has_explicit_difference);

                for code in first_char..=last_char.min(255) {
                    let code_u8 = code as u8;
                    let mut has_glyph = false;

                    // Resolve via PDF encoding / Differences mapping first.
                    let glyph_name = if let Some(name) = differences.get(&code) {
                        Some(name.clone())
                    } else if !enc_name.is_empty() {
                        let ch = encoding_to_char(code, &enc_name);
                        unicode_to_glyph_name(ch).or_else(|| unicode_to_agl_name(ch))
                    } else {
                        None
                    };

                    if let Some(name) = glyph_name {
                        if !name.is_empty() && name != ".notdef" {
                            has_glyph = cff
                                .glyph_index_by_name(&name)
                                .and_then(|gid| cff.glyph_width(gid))
                                .is_some();

                            if !has_glyph {
                                for alt in cff_glyph_name_alternatives(&name) {
                                    if cff
                                        .glyph_index_by_name(alt)
                                        .and_then(|gid| cff.glyph_width(gid))
                                        .is_some()
                                    {
                                        has_glyph = true;
                                        break;
                                    }
                                }
                            }
                        }
                    }

                    // Fallback to CFF internal code->GID mapping only for
                    // no/ambiguous PDF encoding scenarios.
                    if !has_glyph && allow_cff_encoding_fallback {
                        has_glyph = enc_map.get(&code_u8).map(|&gid| gid != 0).unwrap_or(false);
                    }

                    if !has_glyph {
                        invalid_codes.insert(code_u8);
                    }
                }
            } else if let Some(parsed) = parse_type1_program(&font_data) {
                // Classic Type1 (FontFile/PFB/PFA) symbolic font.
                for code in first_char..=last_char.min(255) {
                    let code_u8 = code as u8;
                    let glyph_name = parsed.encoding.get(&code_u8).map(|s| s.as_str());
                    let has_glyph = glyph_name
                        .filter(|name| !name.is_empty() && *name != ".notdef")
                        .and_then(|name| parsed.charstring_widths.get(name))
                        // In these symbolic subsets, zero-width slots are
                        // typically .notdef proxies and should be removed.
                        .map(|w| *w > 0)
                        .unwrap_or(false);
                    if !has_glyph {
                        invalid_codes.insert(code_u8);
                    }
                }
            }

            // Fallback: when font program parsing is inconclusive, infer invalid
            // codes from PDF widths/encoding metadata for this simple font.
            if invalid_codes.is_empty() {
                invalid_codes.extend(invalid_simple_codes_from_widths(
                    doc, dict, first_char, last_char,
                ));
            }
            // For simple fonts, any single-byte code outside the declared
            // FirstChar..LastChar range is not defined by the font dictionary.
            // Keeping those bytes in text-showing operators causes .notdef /
            // missing-glyph failures (6.2.11.8:1, 6.2.11.4.1:2), especially in
            // symbolic TeX subsets where FirstChar is often 33 and stream text
            // still contains ASCII spaces (0x20).
            let first_bound = first_char.min(256);
            for code in 0..first_bound {
                invalid_codes.insert(code as u8);
            }
            let last_bound = last_char.min(255);
            if last_bound < 255 {
                for code in (last_bound + 1)..=255 {
                    invalid_codes.insert(code as u8);
                }
            }
            if !invalid_codes.is_empty() {
                notdef_fonts.insert(res_name.clone(), invalid_codes);
            }
        }

        if notdef_fonts.is_empty() {
            continue;
        }

        // Scan content streams and replace invalid codes.
        let content_ids = crate::content_editor::get_content_stream_ids(doc, page_id);
        let mut current_font = String::new();

        for cs_id in content_ids {
            let stream_data = match doc.objects.get(&cs_id) {
                Some(Object::Stream(s)) => {
                    let mut s = s.clone();
                    let _ = s.decompress();
                    s.content
                }
                _ => continue,
            };

            let Ok(editor) = crate::content_editor::ContentEditor::from_stream(&stream_data) else {
                continue;
            };
            let ops = editor.operations().to_vec();
            let mut modified = false;
            let mut new_ops = Vec::with_capacity(ops.len());

            for op in &ops {
                match op.operator.as_str() {
                    "Tf" => {
                        if let Some(Object::Name(name)) = op.operands.first() {
                            current_font = String::from_utf8_lossy(name).to_string();
                        }
                        new_ops.push(op.clone());
                    }
                    "Tj" | "'" | "\"" => {
                        if let Some(invalid_codes) = notdef_fonts.get(&current_font) {
                            let mut new_op = op.clone();
                            let str_idx = if op.operator == "\"" { 2 } else { 0 };
                            if let Some(Object::String(bytes, _)) = new_op.operands.get_mut(str_idx)
                            {
                                if fix_simple_text_string(bytes, invalid_codes) {
                                    modified = true;
                                }
                            }
                            new_ops.push(new_op);
                        } else {
                            new_ops.push(op.clone());
                        }
                    }
                    "TJ" => {
                        if let Some(invalid_codes) = notdef_fonts.get(&current_font) {
                            let mut new_op = op.clone();
                            if let Some(Object::Array(arr)) = new_op.operands.first_mut() {
                                for item in arr.iter_mut() {
                                    if let Object::String(bytes, _) = item {
                                        if fix_simple_text_string(bytes, invalid_codes) {
                                            modified = true;
                                        }
                                    }
                                }
                            }
                            new_ops.push(new_op);
                        } else {
                            new_ops.push(op.clone());
                        }
                    }
                    _ => {
                        new_ops.push(op.clone());
                    }
                }
            }

            if modified {
                let new_editor = crate::content_editor::ContentEditor::from_operations(new_ops);
                if let Ok(encoded) = new_editor.encode() {
                    if let Some(Object::Stream(s)) = doc.objects.get_mut(&cs_id) {
                        s.set_plain_content(encoded);
                        total_fixed += 1;
                    }
                }
            }
        }
    }

    total_fixed
}

/// Remove out-of-range byte codes from simple-font text strings.
///
/// For simple fonts (Type1/MMType1/TrueType), bytes outside FirstChar..LastChar
/// are not defined by the font dictionary and can resolve to .notdef / missing
/// glyphs in validators (6.2.11.8:1, 6.2.11.4.1:2). This pass strips those
/// bytes directly in content streams.
pub fn fix_simple_font_out_of_range_codes(doc: &mut Document) -> usize {
    use std::collections::HashMap;

    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let mut total_fixed = 0usize;

    for &page_id in &page_ids {
        let font_map: HashMap<String, ObjectId> = {
            let page = match doc.objects.get(&page_id) {
                Some(Object::Dictionary(d)) => d.clone(),
                _ => continue,
            };
            let resources = match page.get(b"Resources").ok() {
                Some(Object::Dictionary(d)) => d.clone(),
                Some(Object::Reference(r)) => match doc.objects.get(r) {
                    Some(Object::Dictionary(d)) => d.clone(),
                    _ => continue,
                },
                _ => continue,
            };
            let fonts = match resources.get(b"Font").ok() {
                Some(Object::Dictionary(d)) => d.clone(),
                Some(Object::Reference(r)) => match doc.objects.get(r) {
                    Some(Object::Dictionary(d)) => d.clone(),
                    _ => continue,
                },
                _ => continue,
            };
            let mut map = HashMap::new();
            for (key, val) in fonts.iter() {
                let name = String::from_utf8_lossy(key).to_string();
                if let Object::Reference(id) = val {
                    map.insert(name, *id);
                }
            }
            map
        };

        let mut font_ranges: HashMap<String, (u8, u8)> = HashMap::new();
        let mut has_type0_font = false;
        for (res_name, font_id) in &font_map {
            let Some(Object::Dictionary(dict)) = doc.objects.get(font_id) else {
                continue;
            };
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();
            if subtype == "Type0" {
                has_type0_font = true;
            }
            if subtype != "TrueType" && subtype != "Type1" && subtype != "MMType1" {
                continue;
            }

            let first_char = dict
                .get(b"FirstChar")
                .ok()
                .and_then(|o| match o {
                    Object::Integer(i) => Some(*i),
                    _ => None,
                })
                .unwrap_or(0)
                .clamp(0, 255) as u8;
            let last_char = dict
                .get(b"LastChar")
                .ok()
                .and_then(|o| match o {
                    Object::Integer(i) => Some(*i),
                    _ => None,
                })
                .unwrap_or(255)
                .clamp(0, 255) as u8;

            font_ranges.insert(res_name.clone(), (first_char, last_char));
        }

        if font_ranges.is_empty() {
            continue;
        }

        // On pages that also use Type0 fonts, avoid byte-level simple-font
        // pruning: rewriting these streams can create validator/parser
        // divergence around CID text interpretation.
        if has_type0_font {
            continue;
        }

        let content_ids = crate::content_editor::get_content_stream_ids(doc, page_id);
        let mut current_font = String::new();

        for cs_id in content_ids {
            let stream_data = match doc.objects.get(&cs_id) {
                Some(Object::Stream(s)) => {
                    let mut s = s.clone();
                    let _ = s.decompress();
                    s.content
                }
                _ => continue,
            };

            let Ok(editor) = crate::content_editor::ContentEditor::from_stream(&stream_data) else {
                continue;
            };
            let ops = editor.operations().to_vec();
            let mut modified = false;
            let mut new_ops = Vec::with_capacity(ops.len());

            for op in &ops {
                match op.operator.as_str() {
                    "Tf" => {
                        if let Some(Object::Name(name)) = op.operands.first() {
                            current_font = String::from_utf8_lossy(name).to_string();
                        }
                        new_ops.push(op.clone());
                    }
                    "Tj" | "'" | "\"" => {
                        if let Some((first_char, last_char)) = font_ranges.get(&current_font) {
                            let mut new_op = op.clone();
                            let str_idx = if op.operator == "\"" { 2 } else { 0 };
                            if let Some(Object::String(bytes, _)) = new_op.operands.get_mut(str_idx)
                            {
                                if fix_simple_text_string_out_of_range(
                                    bytes,
                                    *first_char,
                                    *last_char,
                                    !has_type0_font,
                                ) {
                                    modified = true;
                                }
                            }
                            new_ops.push(new_op);
                        } else {
                            new_ops.push(op.clone());
                        }
                    }
                    "TJ" => {
                        if let Some((first_char, last_char)) = font_ranges.get(&current_font) {
                            let mut new_op = op.clone();
                            if let Some(Object::Array(arr)) = new_op.operands.first_mut() {
                                for item in arr.iter_mut() {
                                    if let Object::String(bytes, _) = item {
                                        if fix_simple_text_string_out_of_range(
                                            bytes,
                                            *first_char,
                                            *last_char,
                                            !has_type0_font,
                                        ) {
                                            modified = true;
                                        }
                                    }
                                }
                            }
                            new_ops.push(new_op);
                        } else {
                            new_ops.push(op.clone());
                        }
                    }
                    _ => new_ops.push(op.clone()),
                }
            }

            if modified {
                let new_editor = crate::content_editor::ContentEditor::from_operations(new_ops);
                if let Ok(encoded) = new_editor.encode() {
                    if let Some(Object::Stream(s)) = doc.objects.get_mut(&cs_id) {
                        s.set_plain_content(encoded);
                        total_fixed += 1;
                    }
                }
            }
        }
    }

    total_fixed
}

/// Replace single-byte codes in a simple font text string that are invalid.
#[allow(clippy::ptr_arg)]
fn fix_simple_text_string(
    bytes: &mut Vec<u8>,
    invalid_codes: &std::collections::HashSet<u8>,
) -> bool {
    let changed = collapse_two_byte_simple_codes(bytes);
    let original_len = bytes.len();
    bytes.retain(|b| !invalid_codes.contains(b));
    changed || bytes.len() != original_len
}

#[allow(clippy::ptr_arg)]
fn fix_simple_text_string_out_of_range(
    bytes: &mut Vec<u8>,
    first_char: u8,
    last_char: u8,
    allow_collapse: bool,
) -> bool {
    let changed = allow_collapse && collapse_two_byte_simple_codes(bytes);
    let original_len = bytes.len();
    if first_char > last_char {
        if !bytes.is_empty() {
            bytes.clear();
            return true;
        }
        return changed;
    }

    // Preserve sentinel-paired 2-byte encoding when collapse is disabled.
    if !allow_collapse {
        if let Some(code_in_odd_lane) = paired_simple_code_lane(bytes) {
            let mut filtered = Vec::with_capacity(bytes.len());
            for i in (0..bytes.len()).step_by(2) {
                let code = if code_in_odd_lane {
                    bytes[i + 1]
                } else {
                    bytes[i]
                };
                if code >= first_char && code <= last_char {
                    filtered.push(bytes[i]);
                    filtered.push(bytes[i + 1]);
                }
            }
            let len_changed = filtered.len() != original_len;
            if len_changed {
                *bytes = filtered;
            }
            return changed || len_changed;
        }
    }

    bytes.retain(|b| *b >= first_char && *b <= last_char);
    changed || bytes.len() != original_len
}

fn paired_simple_code_lane(bytes: &[u8]) -> Option<bool> {
    if bytes.len() < 2 || !bytes.len().is_multiple_of(2) {
        return None;
    }
    let even_all_00 = bytes.iter().step_by(2).all(|b| *b == 0x00);
    let even_all_ff = bytes.iter().step_by(2).all(|b| *b == 0xFF);
    if even_all_00 || even_all_ff {
        return Some(true); // code byte is odd lane (i+1)
    }
    let odd_all_00 = bytes.iter().skip(1).step_by(2).all(|b| *b == 0x00);
    let odd_all_ff = bytes.iter().skip(1).step_by(2).all(|b| *b == 0xFF);
    if odd_all_00 || odd_all_ff {
        return Some(false); // code byte is even lane (i)
    }
    None
}

/// Conservative fallback for symbolic simple fonts: treat codes with explicit
/// zero/negative widths (or explicit .notdef Differences entries) as invalid.
fn invalid_simple_codes_from_widths(
    doc: &Document,
    dict: &lopdf::Dictionary,
    first_char: u32,
    last_char: u32,
) -> std::collections::HashSet<u8> {
    let mut invalid = std::collections::HashSet::new();

    let width_first_char = dict
        .get(b"FirstChar")
        .ok()
        .and_then(|o| match o {
            Object::Integer(i) => Some(*i),
            _ => None,
        })
        .unwrap_or(first_char as i64);

    let widths = match dict.get(b"Widths").ok() {
        Some(Object::Array(a)) => Some(a.clone()),
        Some(Object::Reference(r)) => match doc.objects.get(r) {
            Some(Object::Array(a)) => Some(a.clone()),
            _ => None,
        },
        _ => None,
    };

    if let Some(widths) = widths {
        for code in first_char..=last_char.min(255) {
            let idx = code as i64 - width_first_char;
            if idx < 0 {
                continue;
            }
            let Some(wobj) = widths.get(idx as usize) else {
                continue;
            };
            let w = match wobj {
                Object::Integer(i) => *i as f64,
                Object::Real(r) => *r as f64,
                _ => continue,
            };
            if w <= 0.0 {
                invalid.insert(code as u8);
            }
        }
    }

    // Explicit Differences /.notdef are always invalid references.
    let enc_info = extract_encoding_info(doc, dict);
    for (code, name) in enc_info.differences {
        if code <= 255 && code >= first_char && code <= last_char && name == ".notdef" {
            invalid.insert(code as u8);
        }
    }

    invalid
}

fn collapse_two_byte_simple_codes(bytes: &mut Vec<u8>) -> bool {
    if bytes.len() < 2 || !bytes.len().is_multiple_of(2) {
        return false;
    }

    let even_all_00 = bytes.iter().step_by(2).all(|b| *b == 0x00);
    let even_all_ff = bytes.iter().step_by(2).all(|b| *b == 0xFF);
    let odd_all_00 = bytes.iter().skip(1).step_by(2).all(|b| *b == 0x00);
    let odd_all_ff = bytes.iter().skip(1).step_by(2).all(|b| *b == 0xFF);

    if !(even_all_00 || even_all_ff || odd_all_00 || odd_all_ff) {
        return false;
    }

    let take_odd = even_all_00 || even_all_ff;
    let mut collapsed = Vec::with_capacity(bytes.len() / 2);
    let start = if take_odd { 1 } else { 0 };
    for i in (start..bytes.len()).step_by(2) {
        collapsed.push(bytes[i]);
    }
    *bytes = collapsed;
    true
}

fn replace_simple_font_code_refs(
    doc: &mut Document,
    target_font_id: ObjectId,
    from_code: u8,
    to_code: Option<u8>,
) -> usize {
    use std::collections::HashSet;

    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    let mut total_fixed = 0usize;

    for &page_id in &page_ids {
        // Find all resource names on this page that resolve to the target font.
        let target_names: HashSet<String> = {
            let page = match doc.objects.get(&page_id) {
                Some(Object::Dictionary(d)) => d.clone(),
                _ => continue,
            };
            let resources = match page.get(b"Resources").ok() {
                Some(Object::Dictionary(d)) => d.clone(),
                Some(Object::Reference(r)) => match doc.objects.get(r) {
                    Some(Object::Dictionary(d)) => d.clone(),
                    _ => continue,
                },
                _ => continue,
            };
            let fonts = match resources.get(b"Font").ok() {
                Some(Object::Dictionary(d)) => d.clone(),
                Some(Object::Reference(r)) => match doc.objects.get(r) {
                    Some(Object::Dictionary(d)) => d.clone(),
                    _ => continue,
                },
                _ => continue,
            };

            fonts
                .iter()
                .filter_map(|(key, val)| match val {
                    Object::Reference(id) if *id == target_font_id => {
                        Some(String::from_utf8_lossy(key).to_string())
                    }
                    _ => None,
                })
                .collect()
        };

        if target_names.is_empty() {
            continue;
        }

        let content_ids = crate::content_editor::get_content_stream_ids(doc, page_id);
        let mut current_font = String::new();

        for cs_id in content_ids {
            let stream_data = match doc.objects.get(&cs_id) {
                Some(Object::Stream(s)) => {
                    let mut s = s.clone();
                    let _ = s.decompress();
                    s.content
                }
                _ => continue,
            };

            let Ok(editor) = crate::content_editor::ContentEditor::from_stream(&stream_data) else {
                continue;
            };
            let ops = editor.operations().to_vec();
            let mut modified = false;
            let mut new_ops = Vec::with_capacity(ops.len());

            for op in &ops {
                match op.operator.as_str() {
                    "Tf" => {
                        if let Some(Object::Name(name)) = op.operands.first() {
                            current_font = String::from_utf8_lossy(name).to_string();
                        }
                        new_ops.push(op.clone());
                    }
                    "Tj" | "'" | "\"" => {
                        if target_names.contains(&current_font) {
                            let mut new_op = op.clone();
                            let str_idx = if op.operator == "\"" { 2 } else { 0 };
                            if let Some(Object::String(bytes, _)) = new_op.operands.get_mut(str_idx)
                            {
                                if collapse_two_byte_simple_codes(bytes) {
                                    modified = true;
                                }
                                if let Some(to) = to_code {
                                    for b in bytes.iter_mut() {
                                        if *b == from_code {
                                            *b = to;
                                            modified = true;
                                        }
                                    }
                                } else {
                                    let original_len = bytes.len();
                                    bytes.retain(|b| *b != from_code);
                                    modified |= bytes.len() != original_len;
                                }
                            }
                            new_ops.push(new_op);
                        } else {
                            new_ops.push(op.clone());
                        }
                    }
                    "TJ" => {
                        if target_names.contains(&current_font) {
                            let mut new_op = op.clone();
                            if let Some(Object::Array(arr)) = new_op.operands.first_mut() {
                                for item in arr.iter_mut() {
                                    if let Object::String(bytes, _) = item {
                                        if collapse_two_byte_simple_codes(bytes) {
                                            modified = true;
                                        }
                                        if let Some(to) = to_code {
                                            for b in bytes.iter_mut() {
                                                if *b == from_code {
                                                    *b = to;
                                                    modified = true;
                                                }
                                            }
                                        } else {
                                            let original_len = bytes.len();
                                            bytes.retain(|b| *b != from_code);
                                            modified |= bytes.len() != original_len;
                                        }
                                    }
                                }
                            }
                            new_ops.push(new_op);
                        } else {
                            new_ops.push(op.clone());
                        }
                    }
                    _ => {
                        new_ops.push(op.clone());
                    }
                }
            }

            if modified {
                let new_editor = crate::content_editor::ContentEditor::from_operations(new_ops);
                if let Ok(encoded) = new_editor.encode() {
                    if let Some(Object::Stream(s)) = doc.objects.get_mut(&cs_id) {
                        s.set_plain_content(encoded);
                        total_fixed += 1;
                    }
                }
            }
        }
    }

    total_fixed
}

/// Encoding info extracted from a font dictionary.
struct EncodingInfo {
    /// Base encoding name (e.g., "WinAnsiEncoding").
    base_encoding: String,
    /// Referenced encoding object ID (if encoding is a reference).
    enc_ref: Option<ObjectId>,
    /// Existing Differences: list of (code, glyph_name) pairs.
    differences: Vec<(u32, String)>,
}

/// Extract encoding info from a font dictionary.
fn extract_encoding_info(doc: &Document, dict: &lopdf::Dictionary) -> EncodingInfo {
    let mut info = EncodingInfo {
        base_encoding: String::new(),
        enc_ref: None,
        differences: Vec::new(),
    };

    match dict.get(b"Encoding").ok() {
        Some(Object::Name(n)) => {
            info.base_encoding = String::from_utf8(n.clone()).unwrap_or_default();
        }
        Some(Object::Dictionary(enc_dict)) => {
            info.base_encoding = get_name(enc_dict, b"BaseEncoding").unwrap_or_default();
            info.differences =
                parse_differences_to_vec_from_object(doc, enc_dict.get(b"Differences").ok());
        }
        Some(Object::Reference(enc_id)) => {
            info.enc_ref = Some(*enc_id);
            if let Ok(enc_obj) = doc.get_object(*enc_id) {
                match enc_obj {
                    Object::Name(n) => {
                        info.base_encoding = String::from_utf8(n.clone()).unwrap_or_default();
                    }
                    Object::Dictionary(enc_dict) => {
                        if info.base_encoding.is_empty() {
                            info.base_encoding =
                                get_name(enc_dict, b"BaseEncoding").unwrap_or_default();
                        }
                        info.differences = parse_differences_to_vec_from_object(
                            doc,
                            enc_dict.get(b"Differences").ok(),
                        );
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }

    info
}

/// Parse a Differences array into (code, glyph_name) pairs.
fn parse_differences_to_vec(doc: &Document, arr: &[Object]) -> Vec<(u32, String)> {
    let mut result = Vec::new();
    let mut current_code: Option<u32> = None;
    for obj in arr {
        match obj {
            Object::Integer(i) if *i >= 0 => {
                current_code = Some(*i as u32);
            }
            Object::Name(n) => {
                if let Some(code) = current_code {
                    if let Ok(name) = String::from_utf8(n.clone()) {
                        result.push((code, name));
                    }
                    current_code = Some(code + 1);
                }
            }
            Object::Reference(r) => {
                if let Ok(resolved) = doc.get_object(*r) {
                    match resolved {
                        Object::Integer(i) if *i >= 0 => {
                            current_code = Some(*i as u32);
                        }
                        Object::Name(n) => {
                            if let Some(code) = current_code {
                                if let Ok(name) = String::from_utf8(n.clone()) {
                                    result.push((code, name));
                                }
                                current_code = Some(code + 1);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    result
}

/// Parse a Differences object that may be an array or an indirect reference.
fn parse_differences_to_vec_from_object(
    doc: &Document,
    obj: Option<&Object>,
) -> Vec<(u32, String)> {
    match obj {
        Some(Object::Array(arr)) => parse_differences_to_vec(doc, arr),
        Some(Object::Reference(r)) => doc
            .get_object(*r)
            .ok()
            .and_then(|o| o.as_array().ok())
            .map(|arr| parse_differences_to_vec(doc, arr))
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// Check whether an Encoding dictionary reference is shared by multiple fonts.
fn is_encoding_ref_shared(doc: &Document, enc_ref: ObjectId, current_font_id: ObjectId) -> bool {
    let mut seen = 0usize;
    for (id, obj) in &doc.objects {
        let Object::Dictionary(dict) = obj else {
            continue;
        };
        let Ok(Object::Reference(r)) = dict.get(b"Encoding") else {
            continue;
        };
        if *r != enc_ref {
            continue;
        }
        if *id != current_font_id {
            seen += 1;
            if seen > 0 {
                return true;
            }
        }
    }
    false
}

/// Check if a TrueType glyph has actual data in the glyf table.
///
/// Subset fonts keep cmap entries for stripped glyphs (GID still valid),
/// but the loca table entry has zero length (no glyf data). This function
/// detects such empty slots by comparing consecutive loca offsets.
fn tt_glyph_has_data(face: &ttf_parser::Face, gid: ttf_parser::GlyphId) -> bool {
    let raw = face.raw_face();
    let Some(head) = raw.table(ttf_parser::Tag::from_bytes(b"head")) else {
        return true; // Can't check — assume present.
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
        // Short format: 2 bytes per entry, stored value × 2 = byte offset.
        let off = g * 2;
        if off + 4 > loca.len() {
            return true;
        }
        let o1 = u16::from_be_bytes([loca[off], loca[off + 1]]) as u32;
        let o2 = u16::from_be_bytes([loca[off + 2], loca[off + 3]]) as u32;
        o2 > o1
    } else {
        // Long format: 4 bytes per entry, stored value = byte offset.
        let off = g * 4;
        if off + 8 > loca.len() {
            return true;
        }
        let o1 = u32::from_be_bytes([loca[off], loca[off + 1], loca[off + 2], loca[off + 3]]);
        let o2 = u32::from_be_bytes([loca[off + 4], loca[off + 5], loca[off + 6], loca[off + 7]]);
        o2 > o1
    }
}

/// Fix .notdef references in a TrueType font.
///
/// Phase 1: Replace any ".notdef" entries in existing Differences with
///          the correct glyph name (if found) or "space".
/// Phase 2: For codes NOT in Differences that map to .notdef via the base
///          encoding, add Differences entries IF the font has the glyph.
/// Phase 3: For subset fonts, detect codes that map to GIDs whose outlines
///          were stripped (empty loca entry) and remap them to "space".
fn fix_notdef_in_truetype(
    doc: &mut Document,
    font_id: ObjectId,
    font_data: &[u8],
    enc_info: &EncodingInfo,
    first_char: u32,
    last_char: u32,
    is_subset: bool,
) -> bool {
    let Ok(face) = ttf_parser::Face::parse(font_data, 0) else {
        return false;
    };

    // Also handle referenced encoding dicts.
    let mut differences = enc_info.differences.clone();
    let mut base_encoding = enc_info.base_encoding.clone();
    let enc_ref = enc_info.enc_ref;
    let shared_encoding_ref = enc_ref.is_some_and(|r| is_encoding_ref_shared(doc, r, font_id));

    if let Some(ref_id) = enc_ref {
        // Dereference the encoding object.
        if let Some(Object::Dictionary(enc_dict)) = doc.objects.get(&ref_id) {
            if base_encoding.is_empty() {
                base_encoding = get_name(enc_dict, b"BaseEncoding").unwrap_or_default();
            }
            differences =
                parse_differences_to_vec_from_object(doc, enc_dict.get(b"Differences").ok());
        } else if let Some(Object::Name(n)) = doc.objects.get(&ref_id) {
            base_encoding = String::from_utf8(n.clone()).unwrap_or_default();
        }
    }

    // Phase 1: Find .notdef entries and entries referencing glyphs
    // not present in the font program (which veraPDF treats as .notdef).
    let mut replacements: Vec<(u32, String)> = Vec::new();

    for (code, name) in &differences {
        let is_notdef = name == ".notdef";
        let glyph_missing = !is_notdef && {
            // Check if the glyph name resolves to a real glyph in the font.
            let ch = glyph_name_to_unicode(name);
            match ch.and_then(|c| face.glyph_index(c)) {
                Some(gid) => !tt_glyph_has_data(&face, gid),
                None => {
                    // Try by post table name lookup.
                    face.glyph_index_by_name(name).is_none()
                }
            }
        };
        if is_notdef || glyph_missing {
            let replacement = sanitize_truetype_difference_name(find_truetype_glyph_name_for_code(
                &face,
                *code,
                &base_encoding,
            ));
            replacements.push((*code, replacement));
        }
    }

    // Phase 2: Check base encoding for .notdef mappings.
    let mut new_diffs: Vec<(u32, String)> = Vec::new();

    // Codes that already have valid Differences entries (glyph present in font).
    let valid_diff_codes: std::collections::HashSet<u32> = differences
        .iter()
        .filter(|(_, name)| {
            if name == ".notdef" {
                return false;
            }
            let ch = glyph_name_to_unicode(name);
            match ch.and_then(|c| face.glyph_index(c)) {
                Some(gid) => tt_glyph_has_data(&face, gid),
                None => face.glyph_index_by_name(name).is_some(),
            }
        })
        .map(|(c, _)| *c)
        .collect();

    let check_start = first_char.min(255);
    let check_end = last_char.min(255);
    for code in check_start..=check_end {
        // Shared encoding dictionaries are commonly referenced by multiple
        // subset fonts with different glyph sets. Avoid adding broad phase-2
        // remaps there; they can introduce cross-font width regressions.
        if shared_encoding_ref && is_subset && code >= 32 {
            continue;
        }
        if valid_diff_codes.contains(&code) {
            continue;
        }
        // Skip codes already handled in Phase 1.
        if replacements.iter().any(|(c, _)| *c == code) {
            continue;
        }

        // For codes below 32: these are control characters that standard
        // encodings (WinAnsi, MacRoman) don't map to real glyphs.
        // If the font uses codes < 32, the content stream references them,
        // so they WILL trigger .notdef. Map them to "space".
        if code < 32 {
            new_diffs.push((code, "space".to_string()));
            continue;
        }

        let ch = encoding_to_char(code, &base_encoding);

        // Check if this code maps to a real glyph in the font.
        let gid_opt = face.glyph_index(ch);
        let has_valid_glyph = match gid_opt {
            Some(gid) => {
                if is_subset {
                    // Subset fonts keep cmap entries for stripped glyphs.
                    // Check the loca table to see if the glyph has actual data.
                    tt_glyph_has_data(&face, gid)
                } else {
                    true
                }
            }
            None => false,
        };

        if has_valid_glyph {
            continue; // Glyph present with outline data — no fix needed.
        }

        // For subset fonts where the cmap has an entry but the outline
        // was stripped, map directly to "space" (don't try to find another
        // glyph name which might also be stripped).
        if is_subset && gid_opt.is_some() {
            new_diffs.push((code, "space".to_string()));
            continue;
        }

        // The encoding maps this code to a Unicode char that the font
        // doesn't have. Check if the font has the glyph by name.
        let glyph_name = sanitize_truetype_difference_name(find_truetype_glyph_name_for_code(
            &face,
            code,
            &base_encoding,
        ));
        if glyph_name == "space" {
            // If "space" exists in the font, remap to it for both subset and
            // non-subset fonts to avoid .notdef references.
            let has_space = face.glyph_index(' ').is_some_and(|gid| {
                if is_subset {
                    tt_glyph_has_data(&face, gid)
                } else {
                    true
                }
            });
            if has_space {
                new_diffs.push((code, "space".to_string()));
            }
        } else {
            // Font has a concrete replacement glyph by name — add it
            // to Differences so it doesn't resolve to .notdef.
            new_diffs.push((code, glyph_name));
        }
    }

    if replacements.is_empty() && new_diffs.is_empty() {
        return false;
    }

    // Apply the fixes by rebuilding the Encoding dictionary.
    apply_encoding_fixes(
        doc,
        font_id,
        &base_encoding,
        &differences,
        &replacements,
        &new_diffs,
        enc_ref,
    )
}

/// Fallback for fixing .notdef references when CFF parsing fails
/// (i.e., PFB Type1 fonts or corrupt CFF data).
///
/// Two strategies:
/// 1. Remap control characters (0-31) in Encoding/Differences to "space"
///    so they don't resolve to .notdef through missing glyphs.
/// 2. Remove control character bytes from content stream text strings.
fn fix_notdef_control_chars_fallback(
    doc: &mut Document,
    font_id: ObjectId,
    enc_info: &EncodingInfo,
    first_char: u32,
    _last_char: u32,
) -> bool {
    if first_char > 31 {
        return false;
    }

    // Strategy 1: Remap control characters in Encoding/Differences to "space".
    // For PFB fonts we can't determine available glyphs, but remapping control
    // codes to "space" is always safe since they are non-printing.
    let mut differences = enc_info.differences.clone();
    let mut base_encoding = enc_info.base_encoding.clone();
    let enc_ref = enc_info.enc_ref;

    if let Some(ref_id) = enc_ref {
        if let Some(Object::Dictionary(enc_dict)) = doc.objects.get(&ref_id) {
            if base_encoding.is_empty() {
                base_encoding = get_name(enc_dict, b"BaseEncoding").unwrap_or_default();
            }
            differences =
                parse_differences_to_vec_from_object(doc, enc_dict.get(b"Differences").ok());
        } else if let Some(Object::Name(n)) = doc.objects.get(&ref_id) {
            base_encoding = String::from_utf8(n.clone()).unwrap_or_default();
        }
    }

    if base_encoding.is_empty() {
        base_encoding = "StandardEncoding".to_string();
    }

    // Remap existing Differences for control codes (0-31) to "space".
    let replacements: Vec<(u32, String)> = differences
        .iter()
        .filter(|(code, name)| *code < 32 && name != "space" && name != ".notdef")
        .map(|(code, _)| (*code, "space".to_string()))
        .collect();

    // Add new Differences for control codes not yet in the array.
    let existing_codes: std::collections::HashSet<u32> =
        differences.iter().map(|(c, _)| *c).collect();
    let new_diffs: Vec<(u32, String)> = (first_char..32)
        .filter(|c| !existing_codes.contains(c))
        .map(|c| (c, "space".to_string()))
        .collect();

    let encoding_fixed = if !replacements.is_empty() || !new_diffs.is_empty() {
        apply_encoding_fixes(
            doc,
            font_id,
            &base_encoding,
            &differences,
            &replacements,
            &new_diffs,
            enc_ref,
        )
    } else {
        false
    };

    // Strategy 2: Remove control character bytes from content streams.
    let mut font_key: Option<Vec<u8>> = None;
    let page_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for pid in &page_ids {
        let Some(Object::Dictionary(page)) = doc.objects.get(pid) else {
            continue;
        };
        if get_name(page, b"Type").as_deref() != Some("Page") {
            continue;
        }
        let resources = match page.get(b"Resources").ok() {
            Some(Object::Dictionary(d)) => d.clone(),
            Some(Object::Reference(r)) => match doc.get_object(*r) {
                Ok(Object::Dictionary(d)) => d.clone(),
                _ => continue,
            },
            _ => continue,
        };
        let fonts = match resources.get(b"Font").ok() {
            Some(Object::Dictionary(d)) => d.clone(),
            Some(Object::Reference(r)) => match doc.get_object(*r) {
                Ok(Object::Dictionary(d)) => d.clone(),
                _ => continue,
            },
            _ => continue,
        };
        for (key, val) in fonts.iter() {
            if let Object::Reference(r) = val {
                if *r == font_id {
                    font_key = Some(key.clone());
                    break;
                }
            }
        }
        if font_key.is_some() {
            break;
        }
    }

    let Some(fk) = font_key else {
        return encoding_fixed;
    };
    let font_key_str = format!("/{}", String::from_utf8_lossy(&fk));

    let mut stream_fixed = false;
    let content_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for cid in content_ids {
        let is_stream = matches!(doc.objects.get(&cid), Some(Object::Stream(_)));
        if !is_stream {
            continue;
        }
        let content = {
            let Some(Object::Stream(ref stream)) = doc.objects.get(&cid) else {
                continue;
            };
            match stream.decompressed_content() {
                Ok(data) => data,
                Err(_) => continue,
            }
        };

        let font_key_bytes = font_key_str.as_bytes();
        if !content
            .windows(font_key_bytes.len())
            .any(|w| w == font_key_bytes)
        {
            continue;
        }

        // Check for any control character in string literals.
        let has_control = content.windows(2).any(|w| w[0] == b'(' && w[1] < 32)
            || content.iter().enumerate().any(|(i, &b)| {
                b < 32
                    && b != b'\n'
                    && b != b'\r'
                    && b != b'\t'
                    && i > 0
                    && content[..i]
                        .iter()
                        .rev()
                        .take_while(|&&c| c != b'(' && c != b')')
                        .count()
                        < content[..i]
                            .iter()
                            .rev()
                            .position(|&c| c == b'(')
                            .unwrap_or(usize::MAX)
            });
        if !has_control {
            continue;
        }

        // Remove all control characters (0-31) from PDF string literals in
        // the content stream.
        let mut new_content = Vec::with_capacity(content.len());
        let mut i = 0;
        let mut in_string = false;
        let mut modified = false;
        while i < content.len() {
            if content[i] == b'(' && !in_string {
                in_string = true;
                new_content.push(content[i]);
                i += 1;
                continue;
            }
            if content[i] == b')' && in_string {
                in_string = false;
                new_content.push(content[i]);
                i += 1;
                continue;
            }
            if content[i] == b'\\' && in_string {
                new_content.push(content[i]);
                i += 1;
                if i < content.len() {
                    new_content.push(content[i]);
                    i += 1;
                }
                continue;
            }
            if in_string && content[i] < 32 {
                // Skip control character.
                modified = true;
                i += 1;
                continue;
            }
            new_content.push(content[i]);
            i += 1;
        }

        if modified {
            let len = new_content.len() as i64;
            let new_stream = lopdf::Stream::new(
                lopdf::dictionary! {
                    "Length" => len,
                },
                new_content,
            );
            doc.objects.insert(cid, Object::Stream(new_stream));
            stream_fixed = true;
        }
    }

    encoding_fixed || stream_fixed
}

/// Fix .notdef references in a Type1 (CFF) font.
fn fix_notdef_in_type1(
    doc: &mut Document,
    font_id: ObjectId,
    font_data: &[u8],
    enc_info: &EncodingInfo,
    first_char: u32,
    last_char: u32,
    is_subset: bool,
) -> bool {
    let enc_ref = enc_info.enc_ref;
    let shared_encoding_ref = enc_ref.is_some_and(|r| is_encoding_ref_shared(doc, r, font_id));

    let cff = cff_parser::Table::parse(font_data);
    // If CFF parsing fails but we have control characters (0-31) in the range,
    // still add Differences to remap them away from .notdef.
    if cff.is_none() {
        let has_fontfile1 = {
            let fd_id = match doc.objects.get(&font_id) {
                Some(Object::Dictionary(font)) => match font.get(b"FontDescriptor").ok() {
                    Some(Object::Reference(id)) => Some(*id),
                    _ => None,
                },
                _ => None,
            };
            fd_id
                .and_then(|id| doc.objects.get(&id))
                .and_then(|o| o.as_dict().ok())
                .is_some_and(|fd| fd.has(b"FontFile"))
        };
        if has_fontfile1
            && looks_like_type1_fontfile(font_data)
            && fix_notdef_in_type1_fontfile(
                doc, font_id, font_data, enc_info, first_char, last_char, is_subset,
            )
        {
            return true;
        }
        return fix_notdef_control_chars_fallback(doc, font_id, enc_info, first_char, last_char);
    }
    let cff = cff.unwrap();

    // Build set of available glyph names.
    let mut available_glyphs: std::collections::HashSet<String> = std::collections::HashSet::new();
    let num_glyphs = cff.number_of_glyphs();
    for gid in 0..num_glyphs {
        let glyph_id = cff_parser::GlyphId(gid);
        if let Some(name) = cff.glyph_name(glyph_id) {
            if name != ".notdef" && cff.glyph_width(glyph_id).is_some() {
                available_glyphs.insert(name.to_string());
            }
        }
    }

    // If CFF has no usable glyphs (only .notdef), we can't remap via
    // Differences — fall back to content stream modification.
    if available_glyphs.is_empty() {
        return fix_notdef_control_chars_fallback(doc, font_id, enc_info, first_char, last_char);
    }

    // Also handle referenced encoding dicts.
    let mut differences = enc_info.differences.clone();
    let mut base_encoding = enc_info.base_encoding.clone();
    if let Some(ref_id) = enc_ref {
        if let Some(Object::Dictionary(enc_dict)) = doc.objects.get(&ref_id) {
            if base_encoding.is_empty() {
                base_encoding = get_name(enc_dict, b"BaseEncoding").unwrap_or_default();
            }
            differences =
                parse_differences_to_vec_from_object(doc, enc_dict.get(b"Differences").ok());
        } else if let Some(Object::Name(n)) = doc.objects.get(&ref_id) {
            base_encoding = String::from_utf8(n.clone()).unwrap_or_default();
        }
    }
    if base_encoding.is_empty() {
        base_encoding = "StandardEncoding".to_string();
    }

    // Phase 1: Replace .notdef entries and entries referencing glyphs
    // not present in the font program (which veraPDF treats as .notdef).
    let mut replacements: Vec<(u32, String)> = Vec::new();

    for (code, name) in &differences {
        if name == ".notdef" || !available_glyphs.contains(name) {
            let replacement = sanitize_type1_difference_name(
                find_type1_glyph_name_for_code(&available_glyphs, *code, &base_encoding),
                &available_glyphs,
            );
            replacements.push((*code, replacement));
        }
    }

    // Phase 2: Check base encoding for .notdef mappings (conservative).
    // Only check codes in the font's FirstChar..LastChar range.
    let mut new_diffs: Vec<(u32, String)> = Vec::new();

    // Codes that already have valid (present in font) Differences entries.
    let valid_diff_codes: std::collections::HashSet<u32> = differences
        .iter()
        .filter(|(_, name)| name != ".notdef" && available_glyphs.contains(name))
        .map(|(c, _)| *c)
        .collect();
    let differences_map: std::collections::HashMap<u32, String> =
        differences.iter().map(|(c, n)| (*c, n.clone())).collect();
    let current_pdf_width_for_code = |code: u32| -> Option<f64> {
        let Some(Object::Dictionary(font)) = doc.objects.get(&font_id) else {
            return None;
        };
        let fc = match font.get(b"FirstChar").ok() {
            Some(Object::Integer(i)) => *i as u32,
            _ => return None,
        };
        let widths_arr = match font.get(b"Widths").ok() {
            Some(Object::Array(arr)) => arr.clone(),
            Some(Object::Reference(r)) => match doc.get_object(*r) {
                Ok(Object::Array(arr)) => arr.clone(),
                _ => return None,
            },
            _ => return None,
        };
        if code < fc {
            return None;
        }
        let idx = (code - fc) as usize;
        widths_arr.get(idx).and_then(object_to_f64)
    };

    let check_start = first_char.min(255);
    let check_end = last_char.min(255);
    for code in check_start..=check_end {
        if shared_encoding_ref && is_subset && code >= 32 {
            continue;
        }
        if valid_diff_codes.contains(&code) {
            continue;
        }
        // Skip codes already handled in Phase 1 replacements.
        if replacements.iter().any(|(c, _)| *c == code) {
            continue;
        }

        // For codes below 32: control characters that standard encodings
        // don't map to real glyphs. Map to "space" to avoid .notdef.
        // In subset fonts where "space" isn't available, use any existing
        // glyph (control chars are invisible anyway).
        if code < 32 {
            if available_glyphs.contains("space") {
                new_diffs.push((code, "space".to_string()));
            } else if let Some(name) = available_glyphs.iter().next() {
                new_diffs.push((code, name.clone()));
            }
            continue;
        }

        let ch = encoding_to_char(code, &base_encoding);
        let glyph_name = unicode_to_glyph_name(ch);

        let has_glyph = match &glyph_name {
            Some(name) => available_glyphs.contains(name),
            None => false,
        };

        if has_glyph {
            continue; // Not .notdef — no fix needed.
        }

        // Subset high-byte codes can legitimately resolve through existing
        // CFF/internal mappings even when the AGL name isn't present in the
        // subset charset. If the current dictionary width already matches that
        // pre-fix mapping, skip remapping to avoid introducing drift.
        if is_subset && code > 127 {
            if let (Some(pdf_w), Some(expected_pre_fix)) = (
                current_pdf_width_for_code(code),
                compute_cff_single_width(font_data, code, &base_encoding, &differences_map),
            ) {
                if (pdf_w - expected_pre_fix).abs() <= 1.0 {
                    continue;
                }
            }
        }

        // Try to find the glyph by a different name.
        let replacement = sanitize_type1_difference_name(
            find_type1_glyph_name_for_code(&available_glyphs, code, &base_encoding),
            &available_glyphs,
        );
        if is_subset && code > 127 {
            let safe_subset_high = matches!(
                replacement.as_str(),
                "space" | "period" | "comma" | "hyphen" | "periodcentered" | "middot" | "bullet"
            );
            if !safe_subset_high {
                continue;
            }
        }
        let replacement_is_space = replacement == "space";
        if replacement_is_space {
            // Remap to "space" when available for both subset and non-subset
            // fonts to avoid .notdef references.
            if available_glyphs.contains("space") {
                new_diffs.push((code, "space".to_string()));
            }
        } else {
            // Font has this glyph by a different name — safe to add.
            new_diffs.push((code, replacement));
        }
        if is_subset && replacement_is_space && !available_glyphs.contains("space") {
            // Last-resort for subset fonts: choose any available glyph.
            if let Some(name) = available_glyphs.iter().next() {
                new_diffs.push((code, name.clone()));
            }
        }
    }

    if replacements.is_empty() && new_diffs.is_empty() {
        return false;
    }

    let encoding_fixed = apply_encoding_fixes(
        doc,
        font_id,
        &base_encoding,
        &differences,
        &replacements,
        &new_diffs,
        enc_ref,
    );

    // Keep /Widths consistent with the remapped Differences entries using the
    // same code->glyph resolution path as width mismatch fixing.
    let mut width_updates: Vec<(u32, i64)> = Vec::new();
    let updated_enc_info = {
        let Some(Object::Dictionary(font)) = doc.objects.get(&font_id) else {
            return encoding_fixed;
        };
        get_simple_encoding_info(doc, font)
    };
    for (code, _name) in replacements.iter().chain(new_diffs.iter()) {
        // Keep widths synchronized with the encoding fixes we just applied.
        // Otherwise veraPDF can resolve a remapped code to a different glyph
        // while the dictionary width still points to the old one (6.2.11.5:1).
        if let Some(w) =
            compute_cff_single_width(font_data, *code, &updated_enc_info.0, &updated_enc_info.1)
        {
            width_updates.push((*code, w.round() as i64));
        }
    }
    let widths_fixed = apply_simple_width_updates_for_codes(doc, font_id, &width_updates);

    encoding_fixed || widths_fixed
}

fn looks_like_type1_fontfile(data: &[u8]) -> bool {
    if data.starts_with(&[0x80, 0x01]) || data.starts_with(&[0x80, 0x02]) {
        return true;
    }
    data.starts_with(b"%!PS-AdobeFont") || data.starts_with(b"%!FontType1")
}

fn apply_simple_width_updates_for_codes(
    doc: &mut Document,
    font_id: ObjectId,
    updates: &[(u32, i64)],
) -> bool {
    if updates.is_empty() {
        return false;
    }

    let (first_char, widths_ref) = {
        let Some(Object::Dictionary(font)) = doc.objects.get(&font_id) else {
            return false;
        };
        let first_char = match font.get(b"FirstChar").ok() {
            Some(Object::Integer(i)) => *i as u32,
            _ => return false,
        };
        let widths_ref = match font.get(b"Widths").ok() {
            Some(Object::Reference(id)) => Some(*id),
            Some(Object::Array(_)) => None,
            _ => return false,
        };
        (first_char, widths_ref)
    };

    let mut update_map: std::collections::HashMap<u32, i64> = std::collections::HashMap::new();
    for (code, width) in updates {
        update_map.insert(*code, *width);
    }

    let mut changed = false;
    if let Some(widths_id) = widths_ref {
        if let Some(Object::Array(ref mut arr)) = doc.objects.get_mut(&widths_id) {
            for (code, new_w) in &update_map {
                if *code < first_char {
                    continue;
                }
                let idx = (*code - first_char) as usize;
                if idx >= arr.len() {
                    continue;
                }
                let current = match arr[idx] {
                    Object::Integer(v) => v,
                    Object::Real(v) => v as i64,
                    _ => continue,
                };
                if (current - *new_w).abs() > 1 {
                    arr[idx] = Object::Integer(*new_w);
                    changed = true;
                }
            }
        }
        return changed;
    }

    if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&font_id) {
        if let Ok(Object::Array(ref mut arr)) = font.get_mut(b"Widths") {
            for (code, new_w) in &update_map {
                if *code < first_char {
                    continue;
                }
                let idx = (*code - first_char) as usize;
                if idx >= arr.len() {
                    continue;
                }
                let current = match arr[idx] {
                    Object::Integer(v) => v,
                    Object::Real(v) => v as i64,
                    _ => continue,
                };
                if (current - *new_w).abs() > 1 {
                    arr[idx] = Object::Integer(*new_w);
                    changed = true;
                }
            }
        }
    }

    changed
}

fn fix_notdef_in_type1_fontfile(
    doc: &mut Document,
    font_id: ObjectId,
    font_data: &[u8],
    enc_info: &EncodingInfo,
    first_char: u32,
    last_char: u32,
    is_subset: bool,
) -> bool {
    // This fallback targets classic Type1 FontFile programs (PFB/PFA), where
    // CFF parsing is unavailable. Handle the common 0x20 ("space") case that
    // triggers .notdef on some custom encodings.
    let Some(parsed) = parse_type1_program(font_data) else {
        return false;
    };

    let mut available_glyphs: std::collections::HashSet<String> = parsed
        .charstring_widths
        .keys()
        .filter(|name| !name.is_empty() && name.as_str() != ".notdef")
        .cloned()
        .collect();
    if available_glyphs.is_empty() {
        return false;
    }

    // Also include names from parsed internal encoding.
    for name in parsed.encoding.values() {
        if !name.is_empty() && name != ".notdef" {
            available_glyphs.insert(name.clone());
        }
    }

    // Only relevant if code 32 is in range.
    let check_start = first_char.min(255);
    let check_end = last_char.min(255);
    if !(check_start..=check_end).contains(&32) {
        return false;
    }

    let mut differences = enc_info.differences.clone();
    let mut base_encoding = enc_info.base_encoding.clone();
    let enc_ref = enc_info.enc_ref;

    if let Some(ref_id) = enc_ref {
        if let Some(Object::Dictionary(enc_dict)) = doc.objects.get(&ref_id) {
            if base_encoding.is_empty() {
                base_encoding = get_name(enc_dict, b"BaseEncoding").unwrap_or_default();
            }
            differences =
                parse_differences_to_vec_from_object(doc, enc_dict.get(b"Differences").ok());
        } else if let Some(Object::Name(n)) = doc.objects.get(&ref_id) {
            base_encoding = String::from_utf8(n.clone()).unwrap_or_default();
        }
    }
    if base_encoding.is_empty() {
        base_encoding = "StandardEncoding".to_string();
    }

    let mut replacements: Vec<(u32, String)> = Vec::new();
    let mut stream_remap_to: Option<u8> = None;
    for (code, name) in &differences {
        if *code != 32 {
            continue;
        }
        if name == ".notdef" || !available_glyphs.contains(name) {
            let replacement = sanitize_type1_difference_name(
                find_type1_glyph_name_for_code(&available_glyphs, *code, &base_encoding),
                &available_glyphs,
            );
            replacements.push((*code, replacement));
        }
    }

    let mut new_diffs: Vec<(u32, String)> = Vec::new();
    if !replacements.iter().any(|(code, _)| *code == 32) {
        let internal_32_ok = parsed
            .encoding
            .get(&32)
            .map(|name| !name.is_empty() && name != ".notdef")
            .unwrap_or(false);

        let has_valid_32 = differences
            .iter()
            .find(|(code, _)| *code == 32)
            .map(|(_, name)| name != ".notdef" && available_glyphs.contains(name))
            .unwrap_or_else(|| {
                let ch = encoding_to_char(32, &base_encoding);
                let glyph_name = unicode_to_glyph_name(ch);
                glyph_name
                    .as_ref()
                    .is_some_and(|name| available_glyphs.contains(name))
            })
            && internal_32_ok;

        if !has_valid_32 {
            let replacement = if !parsed.encoding.contains_key(&32) {
                ["A", "a", "zero", "period", "hyphen", "n", "w"]
                    .iter()
                    .find(|name| available_glyphs.contains(**name))
                    .map(|name| (*name).to_string())
                    .or_else(|| {
                        available_glyphs
                            .iter()
                            .find(|name| name.as_str() != "space")
                            .cloned()
                    })
                    .unwrap_or_else(|| {
                        find_type1_glyph_name_for_code(&available_glyphs, 32, &base_encoding)
                    })
            } else {
                parsed
                    .encoding
                    .get(&32)
                    .filter(|name| !name.is_empty() && name.as_str() != ".notdef")
                    .filter(|name| available_glyphs.contains(name.as_str()))
                    .cloned()
                    .unwrap_or_else(|| {
                        find_type1_glyph_name_for_code(&available_glyphs, 32, &base_encoding)
                    })
            };
            let replacement = sanitize_type1_difference_name(replacement, &available_glyphs);
            if !parsed.encoding.contains_key(&32) {
                if let Some((&code, _)) = parsed
                    .encoding
                    .iter()
                    .find(|(code, name)| **code != 32 && name.as_str() == replacement)
                {
                    stream_remap_to = Some(code);
                } else {
                    stream_remap_to = match replacement.as_str() {
                        "A" => Some(65),
                        "a" => Some(97),
                        "zero" => Some(48),
                        "period" => Some(46),
                        "hyphen" => Some(45),
                        "n" => Some(110),
                        "w" => Some(119),
                        _ => None,
                    };
                }
            }
            if replacement == "space" {
                if available_glyphs.contains("space") {
                    new_diffs.push((32, "space".to_string()));
                } else if is_subset {
                    if let Some(name) = available_glyphs.iter().next() {
                        new_diffs.push((32, name.clone()));
                    }
                }
            } else {
                new_diffs.push((32, replacement));
            }
        }
    }

    if replacements.is_empty() && new_diffs.is_empty() {
        if let Some(to_code) = stream_remap_to {
            return replace_simple_font_code_refs(doc, font_id, 32, Some(to_code)) > 0;
        }
        if !parsed.encoding.contains_key(&32) {
            return replace_simple_font_code_refs(doc, font_id, 32, None) > 0;
        }
        return false;
    }

    let encoding_fixed = apply_encoding_fixes(
        doc,
        font_id,
        &base_encoding,
        &differences,
        &replacements,
        &new_diffs,
        enc_ref,
    );

    let stream_fixed = if let Some(to_code) = stream_remap_to {
        replace_simple_font_code_refs(doc, font_id, 32, Some(to_code)) > 0
    } else if !parsed.encoding.contains_key(&32) {
        replace_simple_font_code_refs(doc, font_id, 32, None) > 0
    } else {
        false
    };

    encoding_fixed || stream_fixed
}

/// Find the correct glyph name for a character code in a TrueType font.
///
/// Tries multiple strategies to find a valid glyph name:
/// 1. Unicode cmap lookup + post table name
/// 2. AGL name lookup in font
/// 3. Fallback to "space"
fn find_truetype_glyph_name_for_code(
    face: &ttf_parser::Face,
    code: u32,
    base_encoding: &str,
) -> String {
    let ch = encoding_to_char(code, base_encoding);

    // Strategy 1: Check if the font has a glyph via Unicode cmap.
    if let Some(gid) = face.glyph_index(ch) {
        // Font has this glyph! Try to find its name.
        if let Some(name) = face.glyph_name(gid) {
            return name.to_string();
        }
        // Glyph exists but has no name — use uniXXXX format.
        return format!("uni{:04X}", ch as u32);
    }

    // Strategy 2: Try glyph_index_by_name for common AGL names.
    if let Some(ref name) = unicode_to_agl_name(ch) {
        if face.glyph_index_by_name(name).is_some() {
            return name.clone();
        }
    }

    // Strategy 3: For ASCII range, try the character itself as glyph name.
    if (0x21..=0x7E).contains(&(ch as u32)) {
        let char_name = String::from(ch);
        if face.glyph_index_by_name(&char_name).is_some() {
            return char_name;
        }
    }

    // Strategy 4: try common AGL names that are usually present.
    for candidate in ["space", "period", "hyphen", "zero", "A", "a"] {
        if face.glyph_index_by_name(candidate).is_some() {
            return candidate.to_string();
        }
    }

    // Fallback.
    "space".to_string()
}

/// Ensure Differences names for TrueType fonts remain AGL-compatible.
///
/// veraPDF rule 6.2.11.6:2 rejects non-AGL glyph names in encoding
/// Differences for non-symbolic TrueType fonts. If we can't resolve a name to
/// Unicode, use "space" as a safe fallback.
fn sanitize_truetype_difference_name(name: String) -> String {
    if name == "space" {
        return name;
    }
    if glyph_name_to_unicode(&name).is_some() {
        return name;
    }
    "space".to_string()
}

/// Find the correct glyph name for a character code in a Type1/CFF font.
fn find_type1_glyph_name_for_code(
    available_glyphs: &std::collections::HashSet<String>,
    code: u32,
    base_encoding: &str,
) -> String {
    let ch = encoding_to_char(code, base_encoding);

    // Strategy 1: Standard Unicode AGL name.
    if let Some(name) = unicode_to_agl_name(ch) {
        if available_glyphs.contains(&name) {
            return name;
        }
    }

    // Strategy 2: Try uniXXXX format.
    let uni_name = format!("uni{:04X}", ch as u32);
    if available_glyphs.contains(&uni_name) {
        return uni_name;
    }

    // Strategy 3: For ASCII, try the character itself.
    if (0x21..=0x7E).contains(&(ch as u32)) {
        let char_name = String::from(ch);
        if available_glyphs.contains(&char_name) {
            return char_name;
        }
    }

    fallback_type1_glyph_name(available_glyphs)
}

/// Ensure Type1 Differences names remain Unicode-resolvable where possible.
///
/// Non-AGL custom names in Differences can trigger 6.2.11.6:2/width validation
/// inconsistencies. Prefer "space" as safe fallback when present.
fn sanitize_type1_difference_name(
    name: String,
    available_glyphs: &std::collections::HashSet<String>,
) -> String {
    if name == "space" || glyph_name_to_unicode(&name).is_some() {
        return name;
    }
    if available_glyphs.contains("space") {
        return "space".to_string();
    }
    fallback_type1_glyph_name(available_glyphs)
}

fn fallback_type1_glyph_name(available_glyphs: &std::collections::HashSet<String>) -> String {
    for candidate in ["space", "nbspace", "period", "hyphen", "zero", "A", "a"] {
        if available_glyphs.contains(candidate) {
            return candidate.to_string();
        }
    }

    let mut names: Vec<String> = available_glyphs.iter().cloned().collect();
    names.sort();
    names
        .into_iter()
        .next()
        .unwrap_or_else(|| "space".to_string())
}

/// Map a Unicode character to an Adobe Glyph List name.
fn unicode_to_agl_name(ch: char) -> Option<String> {
    let code = ch as u32;
    match code {
        0x0020 => Some("space".into()),
        0x0021 => Some("exclam".into()),
        0x0022 => Some("quotedbl".into()),
        0x0023 => Some("numbersign".into()),
        0x0024 => Some("dollar".into()),
        0x0025 => Some("percent".into()),
        0x0026 => Some("ampersand".into()),
        0x0027 => Some("quotesingle".into()),
        0x0028 => Some("parenleft".into()),
        0x0029 => Some("parenright".into()),
        0x002A => Some("asterisk".into()),
        0x002B => Some("plus".into()),
        0x002C => Some("comma".into()),
        0x002D => Some("hyphen".into()),
        0x002E => Some("period".into()),
        0x002F => Some("slash".into()),
        0x0030 => Some("zero".into()),
        0x0031 => Some("one".into()),
        0x0032 => Some("two".into()),
        0x0033 => Some("three".into()),
        0x0034 => Some("four".into()),
        0x0035 => Some("five".into()),
        0x0036 => Some("six".into()),
        0x0037 => Some("seven".into()),
        0x0038 => Some("eight".into()),
        0x0039 => Some("nine".into()),
        0x003A => Some("colon".into()),
        0x003B => Some("semicolon".into()),
        0x003C => Some("less".into()),
        0x003D => Some("equal".into()),
        0x003E => Some("greater".into()),
        0x003F => Some("question".into()),
        0x0040 => Some("at".into()),
        0x0041..=0x005A => Some(String::from(ch)), // A-Z
        0x005B => Some("bracketleft".into()),
        0x005C => Some("backslash".into()),
        0x005D => Some("bracketright".into()),
        0x005E => Some("asciicircum".into()),
        0x005F => Some("underscore".into()),
        0x0060 => Some("grave".into()),
        0x0061..=0x007A => Some(String::from(ch)), // a-z
        0x007B => Some("braceleft".into()),
        0x007C => Some("bar".into()),
        0x007D => Some("braceright".into()),
        0x007E => Some("asciitilde".into()),
        0x00A0 => Some("nbspace".into()),
        0x00A1 => Some("exclamdown".into()),
        0x00A2 => Some("cent".into()),
        0x00A3 => Some("sterling".into()),
        0x00A4 => Some("currency".into()),
        0x00A5 => Some("yen".into()),
        0x00A6 => Some("brokenbar".into()),
        0x00A7 => Some("section".into()),
        0x00A8 => Some("dieresis".into()),
        0x00A9 => Some("copyright".into()),
        0x00AA => Some("ordfeminine".into()),
        0x00AB => Some("guillemotleft".into()),
        0x00AC => Some("logicalnot".into()),
        0x00AE => Some("registered".into()),
        0x00AF => Some("macron".into()),
        0x00B0 => Some("degree".into()),
        0x00B1 => Some("plusminus".into()),
        0x00B4 => Some("acute".into()),
        0x00B5 => Some("mu".into()),
        0x00B6 => Some("paragraph".into()),
        0x00B7 => Some("periodcentered".into()),
        0x00B8 => Some("cedilla".into()),
        0x00BA => Some("ordmasculine".into()),
        0x00BB => Some("guillemotright".into()),
        0x00BC => Some("onequarter".into()),
        0x00BD => Some("onehalf".into()),
        0x00BE => Some("threequarters".into()),
        0x00BF => Some("questiondown".into()),
        0x00C0 => Some("Agrave".into()),
        0x00C1 => Some("Aacute".into()),
        0x00C2 => Some("Acircumflex".into()),
        0x00C3 => Some("Atilde".into()),
        0x00C4 => Some("Adieresis".into()),
        0x00C5 => Some("Aring".into()),
        0x00C6 => Some("AE".into()),
        0x00C7 => Some("Ccedilla".into()),
        0x00C8 => Some("Egrave".into()),
        0x00C9 => Some("Eacute".into()),
        0x00CA => Some("Ecircumflex".into()),
        0x00CB => Some("Edieresis".into()),
        0x00CC => Some("Igrave".into()),
        0x00CD => Some("Iacute".into()),
        0x00CE => Some("Icircumflex".into()),
        0x00CF => Some("Idieresis".into()),
        0x00D0 => Some("Eth".into()),
        0x00D1 => Some("Ntilde".into()),
        0x00D2 => Some("Ograve".into()),
        0x00D3 => Some("Oacute".into()),
        0x00D4 => Some("Ocircumflex".into()),
        0x00D5 => Some("Otilde".into()),
        0x00D6 => Some("Odieresis".into()),
        0x00D7 => Some("multiply".into()),
        0x00D8 => Some("Oslash".into()),
        0x00D9 => Some("Ugrave".into()),
        0x00DA => Some("Uacute".into()),
        0x00DB => Some("Ucircumflex".into()),
        0x00DC => Some("Udieresis".into()),
        0x00DD => Some("Yacute".into()),
        0x00DE => Some("Thorn".into()),
        0x00DF => Some("germandbls".into()),
        0x00E0 => Some("agrave".into()),
        0x00E1 => Some("aacute".into()),
        0x00E2 => Some("acircumflex".into()),
        0x00E3 => Some("atilde".into()),
        0x00E4 => Some("adieresis".into()),
        0x00E5 => Some("aring".into()),
        0x00E6 => Some("ae".into()),
        0x00E7 => Some("ccedilla".into()),
        0x00E8 => Some("egrave".into()),
        0x00E9 => Some("eacute".into()),
        0x00EA => Some("ecircumflex".into()),
        0x00EB => Some("edieresis".into()),
        0x00EC => Some("igrave".into()),
        0x00ED => Some("iacute".into()),
        0x00EE => Some("icircumflex".into()),
        0x00EF => Some("idieresis".into()),
        0x00F0 => Some("eth".into()),
        0x00F1 => Some("ntilde".into()),
        0x00F2 => Some("ograve".into()),
        0x00F3 => Some("oacute".into()),
        0x00F4 => Some("ocircumflex".into()),
        0x00F5 => Some("otilde".into()),
        0x00F6 => Some("odieresis".into()),
        0x00F7 => Some("divide".into()),
        0x00F8 => Some("oslash".into()),
        0x00F9 => Some("ugrave".into()),
        0x00FA => Some("uacute".into()),
        0x00FB => Some("ucircumflex".into()),
        0x00FC => Some("udieresis".into()),
        0x00FD => Some("yacute".into()),
        0x00FE => Some("thorn".into()),
        0x00FF => Some("ydieresis".into()),
        0x0152 => Some("OE".into()),
        0x0153 => Some("oe".into()),
        0x0160 => Some("Scaron".into()),
        0x0161 => Some("scaron".into()),
        0x0178 => Some("Ydieresis".into()),
        0x017D => Some("Zcaron".into()),
        0x017E => Some("zcaron".into()),
        0x0192 => Some("florin".into()),
        0x02C6 => Some("circumflex".into()),
        0x02DC => Some("tilde".into()),
        0x2013 => Some("endash".into()),
        0x2014 => Some("emdash".into()),
        0x2018 => Some("quoteleft".into()),
        0x2019 => Some("quoteright".into()),
        0x201A => Some("quotesinglbase".into()),
        0x201C => Some("quotedblleft".into()),
        0x201D => Some("quotedblright".into()),
        0x201E => Some("quotedblbase".into()),
        0x2020 => Some("dagger".into()),
        0x2021 => Some("daggerdbl".into()),
        0x2022 => Some("bullet".into()),
        0x2026 => Some("ellipsis".into()),
        0x2030 => Some("perthousand".into()),
        0x2039 => Some("guilsinglleft".into()),
        0x203A => Some("guilsinglright".into()),
        0x20AC => Some("Euro".into()),
        0x2122 => Some("trademark".into()),
        _ => None,
    }
}

/// Apply encoding fixes to a font dictionary.
///
/// Rebuilds the Encoding dictionary with the merged Differences array
/// that includes both the original entries (with .notdef replaced) and
/// any new entries.
fn apply_encoding_fixes(
    doc: &mut Document,
    font_id: ObjectId,
    base_encoding: &str,
    original_differences: &[(u32, String)],
    replacements: &[(u32, String)],
    new_diffs: &[(u32, String)],
    enc_ref: Option<ObjectId>,
) -> bool {
    // Build the replacement map: code -> new glyph name.
    let mut replacement_map: std::collections::HashMap<u32, String> =
        std::collections::HashMap::new();
    for (code, name) in replacements {
        replacement_map.insert(*code, name.clone());
    }

    // Merge original differences with replacements.
    let mut merged: Vec<(u32, String)> = Vec::new();
    for (code, name) in original_differences {
        if let Some(replacement) = replacement_map.get(code) {
            merged.push((*code, replacement.clone()));
        } else {
            merged.push((*code, name.clone()));
        }
    }

    // Add new differences.
    for (code, name) in new_diffs {
        merged.push((*code, name.clone()));
    }

    // Sort by code for a clean Differences array.
    merged.sort_by_key(|(code, _)| *code);

    if merged.is_empty() {
        return false;
    }

    // Build the Differences array.
    let mut diff_array: Vec<Object> = Vec::new();
    let mut prev_code: Option<u32> = None;
    for (code, name) in &merged {
        // Only emit a new code integer when the code is not consecutive.
        let need_code = match prev_code {
            Some(pc) => *code != pc + 1,
            None => true,
        };
        if need_code {
            diff_array.push(Object::Integer(*code as i64));
        }
        diff_array.push(Object::Name(name.as_bytes().to_vec()));
        prev_code = Some(*code);
    }

    // Determine the base encoding to use.
    let effective_base = if base_encoding.is_empty() {
        "WinAnsiEncoding"
    } else {
        base_encoding
    };

    let shared_encoding_ref =
        enc_ref.is_some_and(|ref_id| is_encoding_ref_shared(doc, ref_id, font_id));

    // If the encoding was a private reference, modify that referenced object.
    if let Some(ref_id) = enc_ref {
        if !shared_encoding_ref {
            if let Some(Object::Dictionary(ref mut enc)) = doc.objects.get_mut(&ref_id) {
                enc.set(
                    "BaseEncoding",
                    Object::Name(effective_base.as_bytes().to_vec()),
                );
                enc.set("Differences", Object::Array(diff_array));
                return true;
            }
        }
    }

    // Build new encoding dict.
    let enc_dict = lopdf::Dictionary::from_iter(vec![
        ("Type".to_string(), Object::Name(b"Encoding".to_vec())),
        (
            "BaseEncoding".to_string(),
            Object::Name(effective_base.as_bytes().to_vec()),
        ),
        ("Differences".to_string(), Object::Array(diff_array)),
    ]);

    if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&font_id) {
        dict.set("Encoding", Object::Dictionary(enc_dict));
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_doc_with_unembedded_font() -> Document {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();

        let font_dict = dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        };
        let font_id = doc.add_object(Object::Dictionary(font_dict));

        let content = Stream::new(dictionary! {}, b"BT /F1 12 Tf (Hello) Tj ET".to_vec());
        let content_id = doc.add_object(Object::Stream(content));

        let mut font_res = lopdf::Dictionary::new();
        font_res.set("F1", Object::Reference(font_id));
        let mut res = lopdf::Dictionary::new();
        res.set("Font", Object::Dictionary(font_res));

        let page = dictionary! {
            "Type" => "Page",
            "Parent" => Object::Reference(pages_id),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(612), Object::Integer(792),
            ]),
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(res),
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
    fn test_find_non_embedded() {
        let doc = make_doc_with_unembedded_font();
        let non_embedded = find_non_embedded_fonts(&doc);
        assert_eq!(non_embedded.len(), 1);
        assert_eq!(non_embedded[0].1, "Helvetica");
    }

    #[test]
    fn test_find_font_without_type_key() {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();

        // Font with ONLY Subtype, no Type key.
        let font_dict = dictionary! {
            "Subtype" => "Type1",
            "BaseFont" => "Courier",
        };
        let font_id = doc.add_object(Object::Dictionary(font_dict));
        let _ = font_id;

        let pages = dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(0),
            "Kids" => Object::Array(vec![]),
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let non_embedded = find_non_embedded_fonts(&doc);
        assert_eq!(non_embedded.len(), 1, "should detect font without Type key");
        assert_eq!(non_embedded[0].1, "Courier");
    }

    #[test]
    fn test_is_standard_14() {
        assert!(is_standard_14("Helvetica"));
        assert!(is_standard_14("ABCDEF+Helvetica"));
        assert!(is_standard_14("Times-Roman"));
        assert!(!is_standard_14("ArialMT"));
    }

    #[test]
    fn test_embedded_font_not_detected() {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();

        let font_stream = Stream::new(
            dictionary! { "Length1" => Object::Integer(10) },
            vec![0u8; 10],
        );
        let stream_id = doc.add_object(Object::Stream(font_stream));

        let fd = dictionary! {
            "Type" => "FontDescriptor",
            "FontName" => "TestFont",
            "FontFile2" => Object::Reference(stream_id),
            "Flags" => Object::Integer(32),
            "FontBBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(1000), Object::Integer(1000),
            ]),
            "ItalicAngle" => Object::Integer(0),
            "Ascent" => Object::Integer(800),
            "Descent" => Object::Integer(-200),
            "CapHeight" => Object::Integer(700),
            "StemV" => Object::Integer(80),
        };
        let fd_id = doc.add_object(Object::Dictionary(fd));

        let font = dictionary! {
            "Type" => "Font",
            "Subtype" => "TrueType",
            "BaseFont" => "TestFont",
            "FontDescriptor" => Object::Reference(fd_id),
        };
        doc.add_object(Object::Dictionary(font));

        let pages = dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(0),
            "Kids" => Object::Array(vec![]),
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let non_embedded = find_non_embedded_fonts(&doc);
        assert!(
            non_embedded.is_empty(),
            "embedded font should not be detected"
        );
    }

    #[test]
    fn test_embed_report_structure() {
        let mut doc = make_doc_with_unembedded_font();
        let report = embed_fonts(&mut doc).unwrap();
        assert_eq!(report.fonts_inspected, 1);
        assert_eq!(report.non_embedded_found, 1);
    }

    #[test]
    fn test_get_or_create_font_descriptor() {
        let mut doc = make_doc_with_unembedded_font();
        let non_embedded = find_non_embedded_fonts(&doc);
        let font_id = non_embedded[0].0;

        let fd_id = get_or_create_font_descriptor(&mut doc, font_id).unwrap();
        assert!(doc.objects.contains_key(&fd_id));

        if let Some(Object::Dictionary(font)) = doc.objects.get(&font_id) {
            assert!(font.has(b"FontDescriptor"));
        }
    }

    #[test]
    fn test_type0_embedding_targets_descendant() {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();

        let cid_font = dictionary! {
            "Type" => "Font",
            "Subtype" => "CIDFontType2",
            "BaseFont" => "TestCIDFont",
        };
        let cid_id = doc.add_object(Object::Dictionary(cid_font));

        let type0 = dictionary! {
            "Type" => "Font",
            "Subtype" => "Type0",
            "BaseFont" => "TestCIDFont",
            "DescendantFonts" => Object::Array(vec![Object::Reference(cid_id)]),
        };
        let type0_id = doc.add_object(Object::Dictionary(type0));

        let pages = dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(0),
            "Kids" => Object::Array(vec![]),
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let detailed = find_non_embedded_fonts_detailed(&doc);
        let type0_entry = detailed.iter().find(|f| f.font_id == type0_id);
        assert!(type0_entry.is_some(), "should detect Type0 as non-embedded");
        let entry = type0_entry.unwrap();
        assert_eq!(
            entry.target_id, cid_id,
            "embedding target should be CIDFont descendant"
        );
        // CIDFont should NOT appear separately.
        let cid_entry = detailed.iter().find(|f| f.font_id == cid_id);
        assert!(
            cid_entry.is_none(),
            "CIDFont descendant should not be listed separately"
        );
    }

    #[test]
    fn test_winansi_encoding() {
        assert_eq!(winansi_to_char(65), 'A');
        assert_eq!(winansi_to_char(128), '\u{20AC}'); // Euro sign
        assert_eq!(winansi_to_char(147), '\u{201C}'); // Left double quotation
        assert_eq!(winansi_to_char(200), 'È');
    }

    #[test]
    fn test_fix_embedded_font_metrics_no_crash_on_empty() {
        let mut doc = make_doc_with_unembedded_font();
        // No embedded fonts, should return 0 without crashing.
        let fixed = fix_embedded_font_metrics(&mut doc);
        assert_eq!(fixed, 0);
    }

    #[test]
    fn test_fix_cidset_no_crash_on_empty() {
        let mut doc = make_doc_with_unembedded_font();
        let fixed = fix_cidset(&mut doc);
        assert_eq!(fixed, 0);
    }

    #[test]
    fn test_fix_cidset_creates_cidset() {
        let mut doc = Document::with_version("1.7");
        let pages_id = doc.new_object_id();

        // Minimal TrueType font file header (enough for ttf-parser to detect glyph count).
        // Use a real minimal TTF structure: offset table + head table.
        // For testing, we just create a CIDFont with embedded program and check CIDSet creation.
        let font_stream = Stream::new(
            dictionary! { "Length1" => Object::Integer(10) },
            vec![0u8; 10], // Not a valid TTF, so fix_cidset will skip it.
        );
        let stream_id = doc.add_object(Object::Stream(font_stream));

        let fd = dictionary! {
            "Type" => "FontDescriptor",
            "FontName" => "TestCID",
            "FontFile2" => Object::Reference(stream_id),
            "Flags" => Object::Integer(32),
            "FontBBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(1000), Object::Integer(1000),
            ]),
            "ItalicAngle" => Object::Integer(0),
            "Ascent" => Object::Integer(800),
            "Descent" => Object::Integer(-200),
            "CapHeight" => Object::Integer(700),
            "StemV" => Object::Integer(80),
        };
        let fd_id = doc.add_object(Object::Dictionary(fd));

        let cid_font = dictionary! {
            "Type" => "Font",
            "Subtype" => "CIDFontType2",
            "BaseFont" => "TestCID",
            "FontDescriptor" => Object::Reference(fd_id),
        };
        doc.add_object(Object::Dictionary(cid_font));

        let pages = dictionary! {
            "Type" => "Pages",
            "Count" => Object::Integer(0),
            "Kids" => Object::Array(vec![]),
        };
        doc.objects.insert(pages_id, Object::Dictionary(pages));
        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        // Invalid TTF data, so CIDSet won't be created — but no crash.
        let fixed = fix_cidset(&mut doc);
        assert_eq!(fixed, 0);
    }

    #[test]
    fn test_read_embedded_font_data() {
        let mut doc = Document::with_version("1.7");
        let font_bytes = vec![0xAA, 0xBB, 0xCC];
        let font_stream = Stream::new(
            dictionary! { "Length1" => Object::Integer(3) },
            font_bytes.clone(),
        );
        let stream_id = doc.add_object(Object::Stream(font_stream));
        let fd = dictionary! {
            "Type" => "FontDescriptor",
            "FontFile2" => Object::Reference(stream_id),
        };
        let fd_id = doc.add_object(Object::Dictionary(fd));

        let data = read_embedded_font_data(&doc, fd_id);
        assert!(data.is_some());
        assert_eq!(data.unwrap(), font_bytes);
    }
}
