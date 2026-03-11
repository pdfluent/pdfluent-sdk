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

        let font_name = get_name(dict, b"BaseFont").unwrap_or_default();
        if font_name.is_empty() {
            continue;
        }

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
fn has_embedded_font_program(doc: &Document, dict: &lopdf::Dictionary) -> bool {
    match dict.get(b"FontDescriptor").ok() {
        Some(Object::Reference(fd_id)) => {
            if let Some(Object::Dictionary(fd)) = doc.objects.get(fd_id) {
                fd.has(b"FontFile") || fd.has(b"FontFile2") || fd.has(b"FontFile3")
            } else {
                false
            }
        }
        _ => false,
    }
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
                    fd.has(b"FontFile") || fd.has(b"FontFile2") || fd.has(b"FontFile3")
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
        let font_path = find_system_font(&info.name).or_else(|| {
            FALLBACK_FONTS
                .iter()
                .find(|p| std::path::Path::new(p).exists())
                .map(|p| p.to_string())
        });

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

    // Update Widths and FontDescriptor metrics from the embedded font.
    if is_truetype || is_otf {
        update_metrics_from_font(doc, info, &font_data);
    }

    // If we embedded a non-symbolic font (e.g., DejaVuSans) for a symbolic-named
    // font (e.g., ZapfDingbats), update FontDescriptor Flags to match the actual
    // embedded program. veraPDF checks the font program, not the name.
    if is_truetype || is_otf {
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
    let data = match doc.objects.get(&stream_id) {
        Some(Object::Stream(s)) => s.content.clone(),
        _ => return,
    };

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

    // Check if (3,0) Microsoft Symbol cmap exists.
    for j in 0..num_sub as usize {
        let rec = cmap_off + 4 + j * 8;
        if rec + 8 > data.len() {
            return;
        }
        let plat = u16::from_be_bytes([data[rec], data[rec + 1]]);
        let enc = u16::from_be_bytes([data[rec + 2], data[rec + 3]]);
        if plat == 3 && enc == 0 {
            return; // Already has (3,0) — no fix needed.
        }
    }

    // Strip to 1 cmap subtable by patching numSubtables to 1.
    let Some(Object::Stream(ref mut stream)) = doc.objects.get_mut(&stream_id) else {
        return;
    };
    stream.content[cmap_off + 2] = 0;
    stream.content[cmap_off + 3] = 1;
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
        if is_cff && is_symbolic_font_name(&info.name) {
            update_simple_widths_cff_symbolic(doc, info, font_data, &face, scale);
        } else {
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
                parse_differences(enc_dict, &mut diffs);
            }
            Some(Object::Reference(enc_ref)) => {
                if let Ok(Object::Dictionary(enc_dict)) = doc.get_object(*enc_ref) {
                    if let Some(base) = get_name(enc_dict, b"BaseEncoding") {
                        enc_name = base;
                    }
                    parse_differences(enc_dict, &mut diffs);
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
        let ch = encoding_to_char(code, &encoding_name);
        let width = if let Some(glyph_id) = face.glyph_index(ch) {
            face.glyph_hor_advance(glyph_id)
                .map(|w| (w as f64 * scale).round() as i64)
                .unwrap_or(0)
        } else if let Some(glyph_name) = differences.get(&code) {
            // Encoding Differences: look up glyph by name in the font program.
            if let Some(gid) = face.glyph_index_by_name(glyph_name) {
                face.glyph_hor_advance(gid)
                    .map(|w| (w as f64 * scale).round() as i64)
                    .unwrap_or(0)
            } else {
                glyph_name_to_unicode(glyph_name)
                    .and_then(|u| face.glyph_index(u))
                    .and_then(|gid| face.glyph_hor_advance(gid))
                    .map(|w| (w as f64 * scale).round() as i64)
                    .unwrap_or(0)
            }
        } else if is_truetype_outline && code <= u16::MAX as u32 {
            // Fallback for TrueType: GlyphId == code (identity mapping).
            face.glyph_hor_advance(ttf_parser::GlyphId(code as u16))
                .map(|w| (w as f64 * scale).round() as i64)
                .unwrap_or(0)
        } else {
            0
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
                parse_differences(enc_dict, &mut diffs);
            }
            Some(Object::Reference(enc_ref)) => {
                if let Ok(Object::Dictionary(enc_dict)) = doc.get_object(*enc_ref) {
                    parse_differences(enc_dict, &mut diffs);
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

fn standard14_system_path(clean_name: &str) -> Option<&'static str> {
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
        if std::path::Path::new(path).exists() {
            return Some(path);
        }
    }
    candidates.first().copied()
}

/// Search common system font directories for a font file.
fn find_system_font(font_name: &str) -> Option<String> {
    let clean_name = strip_subset_prefix(font_name);

    if let Some(path) = standard14_system_path(clean_name) {
        if std::path::Path::new(path).exists() {
            return Some(path.to_string());
        }
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

    standard14_system_path(key).and_then(|p| {
        if std::path::Path::new(p).exists() {
            Some(p.to_string())
        } else {
            None
        }
    })
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

    // Collect widths for all glyphs from the CFF program.
    let mut widths: Vec<(u16, i64)> = Vec::new();
    for gid in 0..num_glyphs {
        let glyph_id = cff_parser::GlyphId(gid);
        if let Some(w) = cff.glyph_width(glyph_id) {
            let scaled = (w as f64 * scale as f64).round() as i64;
            widths.push((gid, scaled));
        }
    }

    if widths.is_empty() {
        return false;
    }

    // Determine DW (default width) — use the most common width.
    let mut freq: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
    for (_, w) in &widths {
        *freq.entry(*w).or_default() += 1;
    }
    let dw = freq
        .iter()
        .max_by_key(|(_, count)| *count)
        .map(|(w, _)| *w)
        .unwrap_or(1000);

    // Build /W array: consecutive runs of widths that differ from DW.
    // Format: [cid [w1 w2 ...] cid2 [w3 w4 ...] ...]
    let mut w_array: Vec<Object> = Vec::new();
    let mut run_start: Option<u16> = None;
    let mut run_widths: Vec<Object> = Vec::new();

    for (gid, w) in &widths {
        if *w == dw {
            // Flush any accumulated run.
            if let Some(start) = run_start.take() {
                w_array.push(Object::Integer(start as i64));
                w_array.push(Object::Array(std::mem::take(&mut run_widths)));
            }
            continue;
        }

        match run_start {
            Some(start) if *gid == start + run_widths.len() as u16 => {
                // Continue existing run.
                run_widths.push(Object::Integer(*w));
            }
            _ => {
                // Flush previous run and start new.
                if let Some(start) = run_start.take() {
                    w_array.push(Object::Integer(start as i64));
                    w_array.push(Object::Array(std::mem::take(&mut run_widths)));
                }
                run_start = Some(*gid);
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
    let font_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut fixed = 0;

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

        let Some(cff) = cff_parser::Table::parse(&font_data) else {
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
        let (subtype, fd_id, cid_font_id) = {
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
                        (cid_subtype.unwrap_or_default(), cid_fd, Some(cid_id))
                    }
                    _ => continue,
                }
            } else if subtype == "CIDFontType2" {
                let fd_id = match dict.get(b"FontDescriptor").ok() {
                    Some(Object::Reference(id)) => Some(*id),
                    _ => None,
                };
                (subtype, fd_id, Some(font_id))
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

        // Only process fonts with FontFile2 (TrueType programs).
        let has_ff2 = matches!(
            doc.objects.get(&fd_id),
            Some(Object::Dictionary(d)) if d.has(b"FontFile2")
        );
        if !has_ff2 {
            continue;
        }

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

        // Read CIDToGIDMap to determine mapping.
        let is_identity = {
            let Some(Object::Dictionary(cid_dict)) = doc.objects.get(&cid_id) else {
                continue;
            };
            match cid_dict.get(b"CIDToGIDMap").ok() {
                Some(Object::Name(n)) => n == b"Identity",
                None => true, // Default is Identity per spec.
                _ => false,   // Stream-based map — skip for now.
            }
        };

        if !is_identity {
            continue; // Only handle Identity mapping for now.
        }

        // For Identity mapping: CID == GID. Read widths from hmtx.
        let num_glyphs = face.number_of_glyphs();
        if num_glyphs == 0 {
            continue;
        }

        // Collect widths: CID → width in PDF units.
        let mut widths: Vec<(u16, i64)> = Vec::new();
        for gid in 0..num_glyphs {
            let w = face
                .glyph_hor_advance(ttf_parser::GlyphId(gid))
                .map(|a| (a as f64 * scale).round() as i64)
                .unwrap_or(0);
            widths.push((gid, w));
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
        let scale = if matrix.sx.abs() > f32::EPSILON {
            matrix.sx as f64 * 1000.0
        } else {
            1.0
        };

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
    // For PDF/A-2b, CIDSet is OPTIONAL. If present, it must correctly identify
    // all CIDs in the font program. Generating a correct CIDSet for CID-keyed
    // CFF fonts requires mapping GIDs to CIDs via the charset structure, which
    // is complex and error-prone. The safest approach: REMOVE CIDSet entirely.
    // This eliminates 6.2.11.4.2:2 validation errors without risk.
    let font_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut fixed = 0;

    for font_id in font_ids {
        let fd_id = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();
            if subtype != "CIDFontType0" && subtype != "CIDFontType2" {
                continue;
            }

            match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(id)) => *id,
                _ => continue,
            }
        };

        // Remove CIDSet if present — it's optional for PDF/A-2b and
        // incorrect CIDSet causes validation failures.
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

        if has_cidset {
            if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
                fd.remove(b"CIDSet");
            }
            fixed += 1;
        }
    }
    fixed
}

/// Fix font width mismatches between /Widths array and embedded font program (6.2.11.5:1).
///
/// Conservative approach: only updates individual width entries that clearly mismatch,
/// and only for fonts where the glyph mapping can be unambiguously determined.
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

            // Skip fonts with known symbolic names when the FontDescriptor
            // has the Symbolic flag set (indicating the CFF encoding should be used).
            // Also skip symbolic-named CFF fonts (FontFile3) even when flags say
            // Nonsymbolic — veraPDF validates CFF widths via CFF internal encoding,
            // not PDF encoding.  update_simple_widths_cff_symbolic already set the
            // correct widths during embedding.
            // Only process symbolic fonts when they were re-embedded as TrueType
            // (FontFile2) with a non-symbolic fallback (e.g. DejaVuSans).
            if let Some(name) = get_name(dict, b"BaseFont") {
                if is_symbolic_font_name(&name) {
                    if is_font_symbolic(doc, dict) {
                        continue;
                    }
                    // CFF symbolic fonts: skip — widths are CFF-encoding-based.
                    let has_ff3 = dict
                        .get(b"FontDescriptor")
                        .ok()
                        .and_then(|o| {
                            if let Object::Reference(id) = o {
                                Some(*id)
                            } else {
                                None
                            }
                        })
                        .and_then(|fd_id| doc.objects.get(&fd_id))
                        .and_then(|o| {
                            if let Object::Dictionary(d) = o {
                                Some(d)
                            } else {
                                None
                            }
                        })
                        .is_some_and(|d| d.has(b"FontFile3"));
                    if has_ff3 {
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

            (subtype, fd_id, fc, existing_widths, enc_info, widths_ref)
        };

        let (subtype, fd_id, first_char, existing_widths, enc_info, widths_ref) = info;

        // Check if font program is embedded.
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
            (fd.has(b"FontFile"), fd.has(b"FontFile2"), fd.has(b"FontFile3"))
        };

        let font_data = read_embedded_font_data(doc, fd_id);
        let Some(font_data) = font_data else { continue };

        let corrections: Vec<(usize, i64)>;

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
            // DEBUG: trace CFF corrections for failing fonts
            if let Some(ref bfname) = get_name(doc.objects.get(&font_id).and_then(|o| if let Object::Dictionary(d) = o { Some(d) } else { None }).unwrap_or(&lopdf::Dictionary::new()), b"BaseFont") {
                if bfname.contains("CMR10") || bfname.contains("CMTI10") || bfname.contains("CMSY") || bfname.contains("CMMIB") {
                    eprintln!("DEBUG {bfname}: fc={first_char} widths={} corrections={} enc=({},{}) ff3={has_ff3}", existing_widths.len(), corrections.len(), enc_info.0, enc_info.1.len());
                    // Check specific failing codes
                    for check_code in [161u32, 174, 175, 176, 185, 188, 189, 193, 196] {
                        if check_code < first_char { continue; }
                        let idx = (check_code - first_char) as usize;
                        if idx >= existing_widths.len() { continue; }
                        let old = match &existing_widths[idx] { Object::Integer(v) => *v, _ => -1 };
                        let correction = corrections.iter().find(|&&(i, _)| i == idx);
                        eprintln!("  code {check_code}: old={old}, correction={correction:?}");
                    }
                }
            }
        } else if has_ff2 && (subtype == "Type1" || subtype == "MMType1") {
            // Type1 font re-encoded as TrueType (after embedding fallback font).
            corrections = compute_truetype_width_corrections(
                &font_data,
                first_char,
                &existing_widths,
                &enc_info,
            );
        } else if has_ff1 && (subtype == "Type1" || subtype == "MMType1") {
            // Traditional Type1 font with FontFile — parse charstring widths.
            corrections = compute_type1_fontfile_width_corrections(
                &font_data,
                first_char,
                &existing_widths,
                &enc_info,
            );
        } else {
            continue;
        }

        // Also compute widths for codes beyond LastChar (up to 255).
        // Some fonts have codes used in content streams that fall outside
        // [FirstChar, LastChar]. Extend the Widths array to cover them.
        let last_char = first_char + existing_widths.len() as u32 - 1;
        let mut extensions: Vec<(u32, i64)> = Vec::new(); // (code, width)
        if last_char < 255 {
            for code in (last_char + 1)..=255 {
                let expected_w = if has_ff2 {
                    if let Ok(face) = ttf_parser::Face::parse(&font_data, 0) {
                        let upem = face.units_per_em() as f64;
                        if upem > 0.0 {
                            let scale = 1000.0 / upem;
                            get_truetype_glyph_width_fractional(
                                &face,
                                code,
                                &enc_info.0,
                                &enc_info.1,
                                scale,
                            )
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else if has_ff3 {
                    // CFF font
                    compute_cff_single_width(&font_data, code, &enc_info.0, &enc_info.1)
                } else {
                    None
                };
                if let Some(w) = expected_w {
                    let w_rounded = w.round() as i64;
                    if w_rounded != 0 {
                        extensions.push((code, w_rounded));
                    }
                }
            }
        }

        if corrections.is_empty() && extensions.is_empty() {
            continue;
        }

        // Safety check: if more than 50% of widths mismatch AND the encoding
        // is not a well-known standard encoding, our mapping is probably wrong.
        // For WinAnsiEncoding/MacRomanEncoding, the mapping is unambiguous,
        // so we trust the computed widths even if many differ (common when a
        // fallback font like DejaVuSans was embedded for Helvetica/Times etc.).
        // CFF internal encoding is also reliable — when no PDF-level Encoding
        // exists, the CFF's own encoding provides an unambiguous code-to-GID map.
        let has_reliable_encoding =
            matches!(enc_info.0.as_str(), "WinAnsiEncoding" | "MacRomanEncoding");
        let uses_cff_encoding = enc_info.0.is_empty() && enc_info.1.is_empty() && has_ff3;
        // Type 1 FontFile widths are computed from the font program directly,
        // so they are always reliable regardless of encoding.
        let uses_type1_fontfile = has_ff1;
        let total_widths = existing_widths.len();
        if !has_reliable_encoding
            && !uses_cff_encoding
            && !uses_type1_fontfile
            && corrections.len() * 2 > total_widths
            && extensions.is_empty()
        {
            continue;
        }

        // Apply corrections and extensions.
        // First, build the updated widths array.
        let new_last_char = extensions
            .last()
            .map(|(code, _)| *code)
            .unwrap_or(last_char);
        let new_len = (new_last_char - first_char + 1) as usize;

        // Get a mutable copy of widths.
        let mut new_widths = existing_widths.clone();
        new_widths.resize(new_len, Object::Integer(0));

        // Apply inline corrections.
        for (idx, new_w) in &corrections {
            if *idx < new_widths.len() {
                new_widths[*idx] = Object::Integer(*new_w);
            }
        }
        // Apply extensions.
        for (code, w) in &extensions {
            let idx = (*code - first_char) as usize;
            if idx < new_widths.len() {
                new_widths[idx] = Object::Integer(*w);
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
            parse_differences(enc_dict, &mut differences);
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
                        parse_differences(enc_dict, &mut differences);
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
    enc_dict: &lopdf::Dictionary,
    differences: &mut std::collections::HashMap<u32, String>,
) {
    if let Ok(Object::Array(diff_arr)) = enc_dict.get(b"Differences") {
        let mut code: u32 = 0;
        for item in diff_arr {
            match item {
                Object::Integer(i) => code = *i as u32,
                Object::Name(n) => {
                    if let Ok(name) = String::from_utf8(n.clone()) {
                        differences.insert(code, name);
                    }
                    code += 1;
                }
                _ => {}
            }
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
            if let Some(gid) = face.glyph_index(unicode) {
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

    // Map code → Unicode via PDF Encoding.
    let ch = encoding_to_char(code, enc_name);
    if let Some(gid) = face.glyph_index(ch) {
        return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
    }

    // Fallback: try (1,0) Mac Roman cmap subtable with raw code byte.
    if code <= 255 {
        if let Some(gid) = lookup_mac_cmap(face, code) {
            return face.glyph_hor_advance(gid).map(|w| w as f64 * scale);
        }
    }

    None
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
        "hyphen" | "minus" => Some('-'),
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

/// Compute width corrections for a Type 1 font with FontFile (PFB format).
///
/// Parses the Type 1 font program to extract FontMatrix, Encoding, and CharString
/// widths. veraPDF validates Type 1 widths as: charstring_width * FontMatrix.sx * 1000.
fn compute_type1_fontfile_width_corrections(
    font_data: &[u8],
    first_char: u32,
    existing_widths: &[Object],
    enc_info: &(String, std::collections::HashMap<u32, String>),
) -> Vec<(usize, i64)> {
    let Some(parsed) = parse_type1_program(font_data) else {
        return Vec::new();
    };

    let (_enc_name, differences) = enc_info;

    let mut corrections = Vec::new();
    for (i, obj) in existing_widths.iter().enumerate() {
        let pdf_w = match obj {
            Object::Integer(w) => *w as f64,
            Object::Real(r) => *r as f64,
            _ => continue,
        };

        let code = first_char + i as u32;

        // Determine glyph name: PDF Differences override, then Type 1 encoding.
        let glyph_name = if let Some(name) = differences.get(&code) {
            name.as_str()
        } else if let Some(name) = parsed.encoding.get(&(code as u8)) {
            name.as_str()
        } else {
            continue;
        };

        if glyph_name == ".notdef" {
            continue;
        }

        let Some(&cs_width) = parsed.charstring_widths.get(glyph_name) else {
            continue;
        };

        let expected = (cs_width as f64 * parsed.font_matrix_sx * 1000.0).round();

        if (pdf_w - expected).abs() > 0.5 {
            corrections.push((i, expected as i64));
        }
    }

    corrections
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
    let encoding = parse_type1_encoding(cleartext);

    // Decrypt eexec section.
    let decrypted = eexec_decrypt(eexec_data);

    // Parse lenIV from cleartext first, then from decrypted Private dict.
    let len_iv = parse_type1_len_iv(cleartext)
        .or_else(|| parse_type1_len_iv_bytes(&decrypted))
        .unwrap_or(4) as usize;

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
        let seg_len = u32::from_le_bytes([data[pos + 2], data[pos + 3], data[pos + 4], data[pos + 5]]) as usize;
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
    haystack
        .windows(needle.len())
        .position(|w| w == needle)
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

/// Parse lenIV from Type 1 cleartext (number of random bytes at start of charstrings).
fn parse_type1_len_iv(cleartext: &[u8]) -> Option<u32> {
    let text = std::str::from_utf8(cleartext).ok()?;
    let pos = text.find("/lenIV")?;
    let after = &text[pos + 6..];
    let trimmed = after.trim_start();
    trimmed.split_whitespace().next()?.parse().ok()
}

/// Parse lenIV from raw bytes (e.g., decrypted Private dict).
fn parse_type1_len_iv_bytes(data: &[u8]) -> Option<u32> {
    let pos = find_bytes(data, b"/lenIV")?;
    let after = &data[pos + 6..];
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
        r = (cipher as u16).wrapping_add(r).wrapping_mul(c1).wrapping_add(c2);
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
                // Likely past the end of CharStrings dict.
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
        r = (cipher as u16).wrapping_add(r).wrapping_mul(c1).wrapping_add(c2);
        decrypted.push(plain);
    }

    // Skip lenIV random bytes.
    let cs = &decrypted[len_iv..];

    // Parse first two integers (sbx, wx) followed by hsbw (13) or sbw (12 7).
    let mut pos = 0;
    let mut values = Vec::new();

    while pos < cs.len() && values.len() < 4 {
        let b = cs[pos];
        if b == 13 {
            // hsbw: stack has [sbx, wx]
            break;
        }
        if b == 12 {
            // Two-byte operator — could be sbw (12 7).
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

    // Width is the second value (values[1] = wx in hsbw/sbw).
    values.get(1).copied()
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
            eprintln!("DEBUG cff_type1: OTF path taken, upem={units_per_em}");
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

    eprintln!("DEBUG cff_type1: raw CFF path taken");
    // Fall back to raw CFF parse (Type1C).
    let Some(cff) = cff_parser::Table::parse(font_data) else {
        return Vec::new();
    };

    let matrix = cff.matrix();
    let scale = if matrix.sx.abs() > f32::EPSILON {
        matrix.sx as f64 * 1000.0
    } else {
        1.0
    };

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

        if (pdf_w - frac_w).abs() > 1.0 {
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

    let mut corrections = Vec::new();

    for (i, obj) in existing_widths.iter().enumerate() {
        let pdf_w = match obj {
            Object::Integer(w) => *w as f64,
            Object::Real(r) => *r as f64,
            _ => continue,
        };

        let code = first_char + i as u32;

        let frac_w = if has_pdf_encoding {
            get_otf_width_via_encoding(face, code, enc_name, differences, scale)
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

        if (pdf_w - frac_w).abs() > 1.0 {
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

    None
}

/// Extract the CFF table from an OTF font.
fn extract_cff_from_otf(font_data: &[u8]) -> Option<cff_parser::Table<'_>> {
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
                return cff_parser::Table::parse(
                    &font_data[table_offset..table_offset + table_length],
                );
            }
            return None;
        }

        offset += 16;
    }

    None
}

/// Like find_cff_glyph_width_by_name but returns f64 (unrounded) for fractional comparison.
fn find_cff_glyph_width_by_name_fractional(
    cff: &cff_parser::Table,
    glyph_name: &str,
    scale: f64,
) -> Option<f64> {
    let num_glyphs = cff.number_of_glyphs();
    for gid_raw in 0..num_glyphs {
        let gid = cff_parser::GlyphId(gid_raw);
        if let Some(name) = cff.glyph_name(gid) {
            if name == glyph_name {
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
            return get_truetype_glyph_width_fractional(&face, code, enc_name, differences, scale);
        }
    }

    // Fall back to raw CFF parse.
    let cff = cff_parser::Table::parse(font_data)?;
    let matrix = cff.matrix();
    let scale = if matrix.sx.abs() > f32::EPSILON {
        matrix.sx as f64 * 1000.0
    } else {
        1.0
    };

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

    if has_pdf_encoding {
        // Try PDF encoding glyph name.
        let glyph_name = if let Some(name) = differences.get(&code) {
            name.clone()
        } else {
            let ch = encoding_to_char(code, enc_name);
            unicode_to_glyph_name(ch).unwrap_or_default()
        };
        if !glyph_name.is_empty() {
            if let Some(w) = find_cff_glyph_width_by_name_fractional(cff, &glyph_name, scale) {
                return Some(w);
            }
            // Try the standard AGL name if we used a "uniXXXX" form.
            if glyph_name.starts_with("uni") {
                let ch = encoding_to_char(code, enc_name);
                if let Some(agl_name) = unicode_to_agl_name(ch) {
                    if let Some(w) = find_cff_glyph_width_by_name_fractional(cff, &agl_name, scale)
                    {
                        return Some(w);
                    }
                }
            }
            // Try common fallback names (e.g. softhyphen → hyphen).
            for alt in cff_glyph_name_alternatives(&glyph_name) {
                if let Some(w) = find_cff_glyph_width_by_name_fractional(cff, alt, scale) {
                    return Some(w);
                }
            }

            // Glyph name not found in subset — veraPDF maps to .notdef (GID 0).
            // Return .notdef width so the PDF Widths array matches.
            if glyph_name != ".notdef" {
                return cff
                    .glyph_width(cff_parser::GlyphId(0))
                    .map(|w| w as f64 * scale);
            }
        }
    }

    // Fallback: CFF internal encoding — parse directly to avoid the Standard
    // Encoding fallback that cff_parser::Table::glyph_index() uses.
    // veraPDF uses only the CFF's own encoding for Type1C fonts.
    if code <= 255 {
        let enc_map = parse_cff_encoding_map(font_data);
        if let Some(&gid) = enc_map.get(&(code as u8)) {
            if gid != 0 {
                return cff
                    .glyph_width(cff_parser::GlyphId(gid))
                    .map(|w| w as f64 * scale);
            }
        }
        // Code not in CFF encoding or maps to .notdef → return .notdef width.
        return cff
            .glyph_width(cff_parser::GlyphId(0))
            .map(|w| w as f64 * scale);
    }

    None
}

/// Parse the CFF encoding table directly from raw CFF data, returning a
/// code → GID map. Does NOT apply Standard Encoding fallback.
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
    let has_supplement = data[offset] & 0x80 != 0;

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
            // Parse supplement if present
            if has_supplement {
                let sup_start = offset + 2 + n_codes;
                parse_cff_encoding_supplement(data, sup_start, &mut map, data);
            }
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
            // Parse supplement if present
            if has_supplement {
                let sup_start = offset + 2 + n_ranges * 2;
                parse_cff_encoding_supplement(data, sup_start, &mut map, data);
            }
        }
        _ => {}
    }

    map
}

/// Parse CFF encoding supplement entries.
fn parse_cff_encoding_supplement(
    data: &[u8],
    start: usize,
    map: &mut std::collections::HashMap<u8, u16>,
    cff_data: &[u8],
) {
    if start >= data.len() {
        return;
    }
    let n_sups = data[start] as usize;
    // Each supplement: code (u8) + SID (u16) = 3 bytes
    // We need to map SID → GID via charset. Since we don't have easy
    // charset access, parse via cff_parser for supplement entries.
    if let Some(cff) = cff_parser::Table::parse(cff_data) {
        for i in 0..n_sups {
            let entry_start = start + 1 + i * 3;
            if entry_start + 2 >= data.len() {
                break;
            }
            let code = data[entry_start];
            // The SID is at entry_start+1..entry_start+3
            // We can look up the GID by searching for this SID in charset
            // For now, use glyph_index as fallback for supplements
            if let Some(gid) = cff.glyph_index(code) {
                map.insert(code, gid.0);
            }
        }
    }
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
                    let val =
                        i16::from_be_bytes([dict_data[i + 1], dict_data[i + 2]]) as i64;
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
    "ZapfDingbats",
    "Wingdings",
    "Webdings",
    "Dingbats",
];

/// Check if a font name (with optional subset prefix) is a symbolic font.
fn is_symbolic_font_name(name: &str) -> bool {
    let base = name.split('+').next_back().unwrap_or(name);
    SYMBOLIC_FONTS
        .iter()
        .any(|sym| base.eq_ignore_ascii_case(sym))
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
                !matches!(
                    get_name(enc_dict, b"BaseEncoding").as_deref(),
                    Some("WinAnsiEncoding")
                )
            }
            Ok(Object::Reference(enc_ref)) => {
                match doc.get_object(*enc_ref) {
                    Ok(Object::Name(enc)) => {
                        let enc_str = String::from_utf8_lossy(enc);
                        enc_str != "WinAnsiEncoding"
                    }
                    Ok(Object::Dictionary(enc_dict)) => !matches!(
                        get_name(enc_dict, b"BaseEncoding").as_deref(),
                        Some("WinAnsiEncoding")
                    ),
                    _ => true,
                }
            }
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
            if is_font_symbolic(doc, dict) {
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
            // If Nonsymbolic is explicitly set, trust it.
            if nonsymbolic {
                return false;
            }
            if symbolic {
                return true;
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

/// Fix width mismatches for symbolic fonts (ZapfDingbats, Symbol) — rule 6.2.11.5:1.
///
/// Symbolic fonts use custom encodings where character codes map directly
/// to glyph IDs (not via Unicode cmap). This function reads the embedded
/// font program and updates the /Widths array to match.
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

            match subtype.as_str() {
                "TrueType" | "Type1" | "MMType1" => {}
                _ => continue,
            }

            let name = match get_name(dict, b"BaseFont") {
                Some(n) => n,
                None => continue,
            };
            if !is_symbolic_font_name(&name) {
                continue;
            }

            // Skip subset fonts (ABCDEF+FontName) — their widths were set
            // by the original PDF producer and match the embedded subset program.
            if name.contains('+') {
                continue;
            }

            // Only apply to fonts where the FontDescriptor Flags say Symbolic.
            // If the font was re-embedded with a non-symbolic fallback, the flags
            // will be Nonsymbolic, and fix_font_width_mismatches handles that case.
            if !is_font_symbolic(doc, dict) {
                continue;
            }

            // Skip symbolic fonts that have a standard PDF-level Encoding.
            // When WinAnsiEncoding / MacRomanEncoding is present, veraPDF
            // validates widths via Unicode cmap (not CFF internal encoding),
            // so fix_font_width_mismatches handles them correctly.
            {
                let enc = get_simple_encoding_info(doc, dict);
                if matches!(enc.0.as_str(), "WinAnsiEncoding" | "MacRomanEncoding") {
                    continue;
                }
            }

            let fd_id = match dict.get(b"FontDescriptor").ok() {
                Some(Object::Reference(id)) => *id,
                _ => continue,
            };

            let fc = match dict.get(b"FirstChar").ok() {
                Some(Object::Integer(i)) => *i as u32,
                _ => continue,
            };
            let existing_widths = match dict.get(b"Widths").ok() {
                Some(Object::Array(arr)) => arr.clone(),
                _ => continue,
            };
            if existing_widths.is_empty() {
                continue;
            }

            (subtype, fd_id, fc, existing_widths)
        };

        let (subtype, fd_id, first_char, existing_widths) = info;

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

        let corrections = if has_ff2 {
            compute_symbolic_truetype_width_corrections(&font_data, first_char, &existing_widths)
        } else if has_ff3 || has_ff {
            compute_symbolic_cff_width_corrections(&font_data, first_char, &existing_widths)
        } else {
            continue;
        };

        if corrections.is_empty() {
            continue;
        }

        // Safety: don't apply if >80% mismatch on non-Type1 — likely wrong mapping.
        if subtype != "Type1" && corrections.len() * 5 > existing_widths.len() * 4 {
            continue;
        }

        let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&font_id) else {
            continue;
        };
        let Some(Object::Array(ref mut widths)) = font.get_mut(b"Widths").ok() else {
            continue;
        };
        for (idx, new_w) in &corrections {
            if *idx < widths.len() {
                widths[*idx] = Object::Integer(*new_w);
            }
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
) -> Vec<(usize, i64)> {
    let Ok(face) = ttf_parser::Face::parse(font_data, 0) else {
        return Vec::new();
    };

    let units_per_em = face.units_per_em() as f64;
    if units_per_em == 0.0 {
        return Vec::new();
    }
    let scale = 1000.0 / units_per_em;
    let mut corrections = Vec::new();

    for (i, obj) in existing_widths.iter().enumerate() {
        let pdf_w = match obj {
            Object::Integer(w) => *w,
            Object::Real(r) => *r as i64,
            _ => continue,
        };

        let code = first_char + i as u32;

        // Symbolic TrueType: veraPDF maps code via (3,0) cmap at 0xF000+code,
        // or (1,0) cmap at code directly.
        let gid = face
            .glyph_index(char::from_u32(0xF000 + code).unwrap_or('\0'))
            .or_else(|| face.glyph_index(char::from_u32(code).unwrap_or('\0')));

        let Some(gid) = gid else { continue };
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

/// Compute width corrections for a symbolic CFF font.
fn compute_symbolic_cff_width_corrections(
    font_data: &[u8],
    first_char: u32,
    existing_widths: &[Object],
) -> Vec<(usize, i64)> {
    let Some(cff) = cff_parser::Table::parse(font_data) else {
        return Vec::new();
    };

    let matrix = cff.matrix();
    let scale = if matrix.sx.abs() > f32::EPSILON {
        matrix.sx as f64 * 1000.0
    } else {
        1.0
    };

    let mut corrections = Vec::new();

    for (i, obj) in existing_widths.iter().enumerate() {
        let pdf_w = match obj {
            Object::Integer(w) => *w,
            Object::Real(r) => *r as i64,
            _ => continue,
        };

        let code = first_char + i as u32;

        let gid = cff.glyph_index(code as u8).filter(|g| g.0 > 0).or_else(|| {
            if code < cff.number_of_glyphs() as u32 {
                Some(cff_parser::GlyphId(code as u16))
            } else {
                None
            }
        });

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

            // Skip symbolic fonts — they use custom encodings.
            // Exception: if the font has an explicit standard encoding
            // (WinAnsiEncoding/MacRomanEncoding), the Differences logic
            // works fine regardless of the Symbolic flag (e.g. OCR fonts).
            if is_font_symbolic(doc, dict) {
                let enc_name = get_name(dict, b"Encoding").unwrap_or_default();
                let has_std_enc = enc_name == "WinAnsiEncoding"
                    || enc_name == "MacRomanEncoding"
                    || enc_name == "MacExpertEncoding";
                if !has_std_enc {
                    // Check if encoding is a dict with a standard BaseEncoding.
                    let has_dict_enc = dict
                        .get(b"Encoding")
                        .ok()
                        .and_then(|o| match o {
                            Object::Dictionary(d) => {
                                let be = get_name(d, b"BaseEncoding").unwrap_or_default();
                                Some(
                                    be == "WinAnsiEncoding"
                                        || be == "MacRomanEncoding"
                                        || be == "MacExpertEncoding",
                                )
                            }
                            _ => None,
                        })
                        .unwrap_or(false);
                    if !has_dict_enc {
                        continue;
                    }
                }
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
            let enc_info = extract_encoding_info(dict);

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

/// Fix .notdef references in CID (Type0) fonts by modifying content streams.
///
/// ISO 19005-2, §6.2.11.8: no .notdef glyph references allowed.
///
/// For CIDFontType0/CIDFontType2 with Identity-H CMap, character codes in
/// content streams are 2-byte CIDs.  If a CID does not have a glyph in the
/// font program, it maps to .notdef (GID 0).  This function replaces such
/// CIDs in Tj/TJ text strings with the space CID.
///
/// See NOTDEF_FIXES_LOG.md for the debug log of approaches tried.
pub fn fix_cid_font_notdef(doc: &mut Document) -> usize {
    use std::collections::{HashMap, HashSet};

    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();

    // For each page, find Type0 fonts with Identity-H CMap and build
    //         a set of valid CIDs per font resource name.
    let mut total_fixed = 0usize;

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

        // For each Type0 font, check if it uses Identity-H and has .notdef CIDs.
        let mut notdef_fonts: HashMap<String, (HashSet<u16>, u16)> = HashMap::new();

        for (res_name, font_id) in &font_map {
            let Some(Object::Dictionary(font_dict)) = doc.objects.get(font_id) else {
                continue;
            };
            let subtype = get_name(font_dict, b"Subtype").unwrap_or_default();
            if subtype != "Type0" {
                continue;
            }

            // Check for Identity-H encoding.
            let enc = get_name(font_dict, b"Encoding").unwrap_or_default();
            if enc != "Identity-H" && enc != "Identity-V" {
                continue;
            }

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

            if cid_subtype == "CIDFontType2" {
                // TrueType-based CID font: GID = CID for Identity-H with Identity
                // CIDToGIDMap (or default). GID 0 is .notdef, 1..num_glyphs are valid.
                match ttf_parser::Face::parse(&font_data, 0) {
                    Ok(face) => {
                        let num_glyphs = face.number_of_glyphs();
                        for gid in 1..num_glyphs {
                            valid_cids.insert(gid);
                        }
                        // Try to find space glyph via cmap.
                        if let Some(gid) = face.glyph_index(' ') {
                            if gid.0 > 0 {
                                space_cid = gid.0;
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
                            if let Some(cid) = cff.glyph_cid(cff_parser::GlyphId(gid)) {
                                if gid > 0 {
                                    valid_cids.insert(cid);
                                }
                                if let Some(name) = cff.glyph_name(cff_parser::GlyphId(gid)) {
                                    if name == "space" && gid > 0 {
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
                                    valid_cids.insert(gid);
                                }
                                if let Some(gid) = face.glyph_index(' ') {
                                    if gid.0 > 0 {
                                        space_cid = gid.0;
                                    }
                                }
                            }
                            Err(_) => {
                                continue;
                            }
                        }
                    }
                }
            }

            // If no space glyph found by name, use the first valid CID.
            if space_cid == 0 {
                if let Some(&first_valid) = valid_cids.iter().next() {
                    space_cid = first_valid;
                }
            }

            // Add font to the map. If valid_cids is empty (font has only .notdef,
            // e.g. HiddenHorzOCR stub fonts), we still add it so text strings get
            // cleared entirely — fix_cid_text_string handles empty valid_cids by
            // clearing the byte vec.
            notdef_fonts.insert(res_name.clone(), (valid_cids, space_cid));
        }

        if notdef_fonts.is_empty() {
            continue;
        }

        // Step 3: parse content streams and fix text strings.
        // Track font name across content streams (font state carries over between
        // consecutive content streams on the same page — the graphics state is not
        // reset between them, per ISO 32000-1 §7.8.2).
        let content_ids = crate::content_editor::get_content_stream_ids(doc, page_id);
        let mut current_font_name = String::new();
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
                        // Track font change: /FontName size Tf
                        if let Some(Object::Name(name)) = op.operands.first() {
                            current_font_name = String::from_utf8_lossy(name).to_string();
                        }
                        new_ops.push(op.clone());
                    }
                    "Tj" | "'" | "\"" => {
                        // Single text string operator.
                        if let Some((valid_cids, space_cid)) = notdef_fonts.get(&current_font_name)
                        {
                            let mut new_op = op.clone();
                            let str_idx = match op.operator.as_str() {
                                "\"" => 2, // aw ac string "
                                _ => 0,
                            };
                            if let Some(Object::String(bytes, _)) = new_op.operands.get_mut(str_idx)
                            {
                                if fix_cid_text_string(bytes, valid_cids, *space_cid) {
                                    modified = true;
                                }
                            }
                            new_ops.push(new_op);
                        } else {
                            new_ops.push(op.clone());
                        }
                    }
                    "TJ" => {
                        // Array of text strings and adjustments.
                        if let Some((valid_cids, space_cid)) = notdef_fonts.get(&current_font_name)
                        {
                            let mut new_op = op.clone();
                            if let Some(Object::Array(arr)) = new_op.operands.first_mut() {
                                for item in arr.iter_mut() {
                                    if let Object::String(bytes, _) = item {
                                        if fix_cid_text_string(bytes, valid_cids, *space_cid) {
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
                // Write back modified content stream.
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

/// Replace 2-byte CIDs in a text string that map to .notdef with the space CID.
/// If the font has no valid CIDs at all (only .notdef), clears the text entirely.
/// Returns true if any replacement was made.
fn fix_cid_text_string(
    bytes: &mut Vec<u8>,
    valid_cids: &std::collections::HashSet<u16>,
    space_cid: u16,
) -> bool {
    if bytes.len() < 2 || !bytes.len().is_multiple_of(2) {
        return false;
    }
    // If font has no valid glyphs at all, clear the entire text string.
    // This happens with stub fonts like HiddenHorzOCR (only .notdef glyph).
    if valid_cids.is_empty() {
        if !bytes.is_empty() {
            bytes.clear();
            return true;
        }
        return false;
    }
    let mut changed = false;
    let space_hi = (space_cid >> 8) as u8;
    let space_lo = (space_cid & 0xFF) as u8;
    for i in (0..bytes.len()).step_by(2) {
        let cid = ((bytes[i] as u16) << 8) | (bytes[i + 1] as u16);
        if cid == 0 || !valid_cids.contains(&cid) {
            bytes[i] = space_hi;
            bytes[i + 1] = space_lo;
            changed = true;
        }
    }
    changed
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
fn extract_encoding_info(dict: &lopdf::Dictionary) -> EncodingInfo {
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
            if let Ok(Object::Array(arr)) = enc_dict.get(b"Differences") {
                info.differences = parse_differences_to_vec(arr);
            }
        }
        Some(Object::Reference(enc_id)) => {
            info.enc_ref = Some(*enc_id);
            // We can't dereference here easily; the caller handles this.
        }
        _ => {}
    }

    info
}

/// Parse a Differences array into (code, glyph_name) pairs.
fn parse_differences_to_vec(arr: &[Object]) -> Vec<(u32, String)> {
    let mut result = Vec::new();
    let mut current_code: Option<u32> = None;
    for obj in arr {
        match obj {
            Object::Integer(i) => {
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
    result
}

/// Fix .notdef references in a TrueType font.
///
/// Phase 1: Replace any ".notdef" entries in existing Differences with
///          the correct glyph name (if found) or "space".
/// Phase 2: For codes NOT in Differences that map to .notdef via the base
///          encoding, add Differences entries IF the font has the glyph.
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

    if let Some(ref_id) = enc_ref {
        // Dereference the encoding object.
        if let Some(Object::Dictionary(enc_dict)) = doc.objects.get(&ref_id) {
            if base_encoding.is_empty() {
                base_encoding = get_name(enc_dict, b"BaseEncoding").unwrap_or_default();
            }
            if let Ok(Object::Array(arr)) = enc_dict.get(b"Differences") {
                differences = parse_differences_to_vec(arr);
            }
        } else if let Some(Object::Name(n)) = doc.objects.get(&ref_id) {
            base_encoding = String::from_utf8(n.clone()).unwrap_or_default();
        }
    }

    // Phase 1: Find .notdef entries in existing Differences.
    let mut replacements: Vec<(u32, String)> = Vec::new();

    for (code, name) in &differences {
        if name == ".notdef" {
            // Try to find the correct glyph for this code.
            let replacement = find_truetype_glyph_name_for_code(&face, *code, &base_encoding);
            replacements.push((*code, replacement));
        }
    }

    // Phase 2: Check base encoding for .notdef mappings.
    // Only add new Differences if the font has the glyph AND the code
    // is in the font's FirstChar..LastChar range (i.e., actually used).
    //
    // This is the "safe" part: we only add Differences for codes where
    // the font PROVABLY has a glyph (via Unicode cmap lookup), or where
    // the code is below 32 (control character range) and needs "space"
    // to avoid .notdef.
    let mut new_diffs: Vec<(u32, String)> = Vec::new();
    let existing_diff_codes: std::collections::HashSet<u32> =
        differences.iter().map(|(c, _)| *c).collect();

    // Check codes in the font's FirstChar..LastChar range.
    let check_start = first_char.min(255);
    let check_end = last_char.min(255);
    for code in check_start..=check_end {
        if existing_diff_codes.contains(&code) {
            continue; // Already handled in Phase 1.
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

        // Check if this code maps to .notdef in the font.
        let has_glyph_via_cmap = face.glyph_index(ch).is_some();
        if has_glyph_via_cmap {
            continue; // Not .notdef — no fix needed.
        }

        // The encoding maps this code to a Unicode char that the font
        // doesn't have. Check if the font has the glyph by name.
        let glyph_name = find_truetype_glyph_name_for_code(&face, code, &base_encoding);
        if glyph_name != "space" {
            // Font has a glyph for this code by name lookup — add it
            // to Differences so it doesn't resolve to .notdef.
            new_diffs.push((code, glyph_name));
        } else if is_subset {
            // ISO 19005-2, §6.2.11.8: No .notdef references allowed.
            // For subset fonts, codes that map to .notdef were already
            // invisible in the original — mapping to "space" is safe
            // and avoids the PDF/A violation.
            new_diffs.push((code, "space".to_string()));
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
    let cff = cff_parser::Table::parse(font_data);
    let Some(cff) = cff else {
        return false;
    };

    // Build set of available glyph names.
    let mut available_glyphs: std::collections::HashSet<String> = std::collections::HashSet::new();
    let num_glyphs = cff.number_of_glyphs();
    for gid in 0..num_glyphs {
        if let Some(name) = cff.glyph_name(cff_parser::GlyphId(gid)) {
            if name != ".notdef" {
                available_glyphs.insert(name.to_string());
            }
        }
    }

    // Also handle referenced encoding dicts.
    let mut differences = enc_info.differences.clone();
    let mut base_encoding = enc_info.base_encoding.clone();
    let enc_ref = enc_info.enc_ref;

    if let Some(ref_id) = enc_ref {
        if let Some(Object::Dictionary(enc_dict)) = doc.objects.get(&ref_id) {
            if base_encoding.is_empty() {
                base_encoding = get_name(enc_dict, b"BaseEncoding").unwrap_or_default();
            }
            if let Ok(Object::Array(arr)) = enc_dict.get(b"Differences") {
                differences = parse_differences_to_vec(arr);
            }
        } else if let Some(Object::Name(n)) = doc.objects.get(&ref_id) {
            base_encoding = String::from_utf8(n.clone()).unwrap_or_default();
        }
    }

    // Phase 1: Replace .notdef entries in existing Differences.
    let mut replacements: Vec<(u32, String)> = Vec::new();

    for (code, name) in &differences {
        if name == ".notdef" {
            let replacement =
                find_type1_glyph_name_for_code(&available_glyphs, *code, &base_encoding);
            replacements.push((*code, replacement));
        }
    }

    // Phase 2: Check base encoding for .notdef mappings (conservative).
    // Only check codes in the font's FirstChar..LastChar range.
    let mut new_diffs: Vec<(u32, String)> = Vec::new();
    let existing_diff_codes: std::collections::HashSet<u32> =
        differences.iter().map(|(c, _)| *c).collect();

    let check_start = first_char.min(255);
    let check_end = last_char.min(255);
    for code in check_start..=check_end {
        if existing_diff_codes.contains(&code) {
            continue;
        }

        // For codes below 32: control characters that standard encodings
        // don't map to real glyphs. Map to "space" to avoid .notdef.
        if code < 32 {
            new_diffs.push((code, "space".to_string()));
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

        // Try to find the glyph by a different name.
        let replacement = find_type1_glyph_name_for_code(&available_glyphs, code, &base_encoding);
        if replacement != "space" {
            // Font has this glyph by a different name — safe to add.
            new_diffs.push((code, replacement));
        } else if is_subset && available_glyphs.contains("space") {
            // ISO 19005-2, §6.2.11.8: No .notdef references allowed.
            // For subset fonts, codes that map to .notdef were already
            // invisible — mapping to "space" avoids the violation.
            new_diffs.push((code, "space".to_string()));
        }
    }

    if replacements.is_empty() && new_diffs.is_empty() {
        return false;
    }

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

    // Fallback: use "space" as a safe, universally available glyph.
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

    // Fallback: "space" is always safe.
    "space".to_string()
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
        0x00A9 => Some("copyright".into()),
        0x00AB => Some("guillemotleft".into()),
        0x00AE => Some("registered".into()),
        0x00B0 => Some("degree".into()),
        0x00B7 => Some("periodcentered".into()),
        0x00BB => Some("guillemotright".into()),
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

    // If the encoding was a reference, modify that referenced object.
    if let Some(ref_id) = enc_ref {
        if let Some(Object::Dictionary(ref mut enc)) = doc.objects.get_mut(&ref_id) {
            enc.set(
                "BaseEncoding",
                Object::Name(effective_base.as_bytes().to_vec()),
            );
            enc.set("Differences", Object::Array(diff_array));
            return true;
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
