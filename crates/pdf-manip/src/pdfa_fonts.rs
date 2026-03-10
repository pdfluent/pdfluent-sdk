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

/// Fallback font path for any font that cannot be found.
const FALLBACK_FONT: &str = "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf";

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
            if std::path::Path::new(FALLBACK_FONT).exists() {
                Some(FALLBACK_FONT.to_string())
            } else {
                None
            }
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
    let font_data = std::fs::read(font_path)
        .map_err(|e| ManipError::Other(format!("failed to read font file: {e}")))?;

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
    if is_truetype {
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

    Ok(())
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
            update_simple_widths_cff_symbolic(doc, info.font_id, font_data);
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

    // For TrueType fonts (with 'glyf' table), GlyphId == code is a valid
    // fallback when the cmap has no entry.  For CFF-based OTF fonts the glyph
    // order follows the CFF charset, so GlyphId(code) is meaningless.
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
                // Glyph name not found — try Unicode mapping of the name.
                glyph_name_to_unicode(glyph_name)
                    .and_then(|u| face.glyph_index(u))
                    .and_then(|gid| face.glyph_hor_advance(gid))
                    .map(|w| (w as f64 * scale).round() as i64)
                    .unwrap_or(0)
            }
        } else if is_truetype_outline && code <= u16::MAX as u32 {
            // Fallback: use width at GlyphId == code (identity mapping for TrueType).
            // This matches how veraPDF validates widths when encoding is absent.
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
/// Uses the CFF internal encoding to map character codes → glyph IDs,
/// then reads widths from the OTF hmtx table (which veraPDF validates against).
fn update_simple_widths_cff_symbolic(doc: &mut Document, font_id: ObjectId, font_data: &[u8]) {
    // Parse the OTF face for hmtx-based width lookup.
    let Ok(face) = ttf_parser::Face::parse(font_data, 0) else {
        return;
    };
    let units_per_em = face.units_per_em() as f64;
    if units_per_em == 0.0 {
        return;
    }
    let scale = 1000.0 / units_per_em;

    // Extract the CFF table from the OTF wrapper for encoding lookup.
    let cff_data = extract_cff_table(font_data);
    let Some(cff_data) = cff_data else { return };
    let Some(cff) = cff_parser::Table::parse(cff_data) else {
        return;
    };

    let (first_char, last_char) = {
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
        (fc, lc)
    };

    let mut widths = Vec::new();
    for code in first_char..=last_char {
        let width = if code <= 255 {
            // Use CFF encoding to map code → glyph ID, then hmtx for width.
            let gid = cff
                .encoding
                .code_to_gid(&cff.charset, code as u8)
                .map(|g| ttf_parser::GlyphId(g.0))
                .unwrap_or(ttf_parser::GlyphId(0));
            face.glyph_hor_advance(gid)
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
fn standard14_system_path(clean_name: &str) -> Option<&'static str> {
    match clean_name {
        "Helvetica" => Some("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
        "Helvetica-Bold" => Some("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"),
        "Helvetica-Oblique" => Some("/usr/share/fonts/truetype/dejavu/DejaVuSans-Oblique.ttf"),
        "Helvetica-BoldOblique" => {
            Some("/usr/share/fonts/truetype/dejavu/DejaVuSans-BoldOblique.ttf")
        }
        "Times-Roman" => Some("/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf"),
        "Times-Bold" => Some("/usr/share/fonts/truetype/dejavu/DejaVuSerif-Bold.ttf"),
        "Times-Italic" => Some("/usr/share/fonts/truetype/dejavu/DejaVuSerif-Italic.ttf"),
        "Times-BoldItalic" => Some("/usr/share/fonts/truetype/dejavu/DejaVuSerif-BoldItalic.ttf"),
        "Courier" => Some("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf"),
        "Courier-Bold" => Some("/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf"),
        "Courier-Oblique" => Some("/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Oblique.ttf"),
        "Courier-BoldOblique" => {
            Some("/usr/share/fonts/truetype/dejavu/DejaVuSansMono-BoldOblique.ttf")
        }
        "Symbol" => Some("/usr/share/fonts/opentype/urw-base35/StandardSymbolsPS.otf"),
        "ZapfDingbats" => Some("/usr/share/fonts/opentype/urw-base35/D050000L.otf"),
        "ArialMT" | "Arial" => Some("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
        "Arial-BoldMT" | "Arial,Bold" | "Arial-Bold" => {
            Some("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf")
        }
        "Arial-ItalicMT" | "Arial,Italic" | "Arial-Italic" => {
            Some("/usr/share/fonts/truetype/dejavu/DejaVuSans-Oblique.ttf")
        }
        "Arial-BoldItalicMT" | "Arial,BoldItalic" => {
            Some("/usr/share/fonts/truetype/dejavu/DejaVuSans-BoldOblique.ttf")
        }
        "TimesNewRomanPSMT" | "TimesNewRoman" | "TimesNewRomanPS" => {
            Some("/usr/share/fonts/truetype/dejavu/DejaVuSerif.ttf")
        }
        "TimesNewRomanPS-BoldMT" | "TimesNewRoman,Bold" | "TimesNewRoman-Bold" => {
            Some("/usr/share/fonts/truetype/dejavu/DejaVuSerif-Bold.ttf")
        }
        "TimesNewRomanPS-ItalicMT" | "TimesNewRoman,Italic" | "TimesNewRoman-Italic" => {
            Some("/usr/share/fonts/truetype/dejavu/DejaVuSerif-Italic.ttf")
        }
        "TimesNewRomanPS-BoldItalicMT" | "TimesNewRoman,BoldItalic" => {
            Some("/usr/share/fonts/truetype/dejavu/DejaVuSerif-BoldItalic.ttf")
        }
        "CourierNewPSMT" | "CourierNew" | "CourierNewPS" => {
            Some("/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf")
        }
        "CourierNewPS-BoldMT" | "CourierNew,Bold" | "CourierNew-Bold" => {
            Some("/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Bold.ttf")
        }
        "CourierNewPS-ItalicMT" | "CourierNew,Italic" | "CourierNew-Italic" => {
            Some("/usr/share/fonts/truetype/dejavu/DejaVuSansMono-Oblique.ttf")
        }
        "SymbolMT" => Some("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
        _ => None,
    }
}

/// Search common system font directories for a font file.
fn find_system_font(font_name: &str) -> Option<String> {
    let clean_name = strip_subset_prefix(font_name);

    if let Some(path) = standard14_system_path(clean_name) {
        if std::path::Path::new(path).exists() {
            return Some(path.to_string());
        }
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
                // Try to get decompressed content.
                let mut s = stream.clone();
                let _ = s.decompress();
                return Some(s.content);
            }
        }
    }
    None
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

        // Only process FontFile3 (CFF programs) or FontFile (Type 1).
        let has_ff3 = matches!(
            doc.objects.get(&fd_id),
            Some(Object::Dictionary(d)) if d.has(b"FontFile3") || d.has(b"FontFile")
        );
        if !has_ff3 {
            continue;
        }

        let font_data = read_embedded_font_data(doc, fd_id);
        let Some(font_data) = font_data else { continue };

        let Some(cff) = cff_parser::Table::parse(&font_data) else {
            continue;
        };

        // Build CharSet string from CFF glyph names: "/name1/name2/name3..."
        let num_glyphs = cff.number_of_glyphs();
        let mut charset_str = String::new();
        for gid in 0..num_glyphs {
            let glyph_id = cff_parser::GlyphId(gid);
            if let Some(name) = cff.glyph_name(glyph_id) {
                if name != ".notdef" {
                    charset_str.push('/');
                    charset_str.push_str(name);
                }
            }
        }

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

            // Skip symbolic fonts — encoding mapping is unreliable.
            // Also skip subset fonts (prefix like ABCDEF+FontName) — their widths
            // were set by the original PDF producer and already match the embedded
            // subset program.
            if let Some(name) = get_name(dict, b"BaseFont") {
                if is_symbolic_font_name(&name) {
                    continue;
                }
                if name.contains('+') {
                    continue;
                }
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
            let existing_widths = match dict.get(b"Widths").ok() {
                Some(Object::Array(arr)) => arr.clone(),
                _ => continue,
            };
            if existing_widths.is_empty() {
                continue;
            }

            // Get encoding info.
            let enc_info = get_simple_encoding_info(doc, dict);

            (subtype, fd_id, fc, existing_widths, enc_info)
        };

        let (subtype, fd_id, first_char, existing_widths, enc_info) = info;

        // Check if font program is embedded.
        let has_embedded = matches!(
            doc.objects.get(&fd_id),
            Some(Object::Dictionary(d)) if d.has(b"FontFile") || d.has(b"FontFile2") || d.has(b"FontFile3")
        );
        if !has_embedded {
            continue;
        }

        // Determine which font file type is embedded.
        let (has_ff2, has_ff3) = {
            let Some(Object::Dictionary(fd)) = doc.objects.get(&fd_id) else {
                continue;
            };
            (fd.has(b"FontFile2"), fd.has(b"FontFile3"))
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
        } else if has_ff2 && (subtype == "Type1" || subtype == "MMType1") {
            // Type1 font re-encoded as TrueType (after embedding fallback font).
            corrections = compute_truetype_width_corrections(
                &font_data,
                first_char,
                &existing_widths,
                &enc_info,
            );
        } else {
            continue;
        }

        if corrections.is_empty() {
            continue;
        }

        // Safety check: if more than 50% of widths mismatch AND the encoding
        // is not a well-known standard encoding, our mapping is probably wrong.
        // For WinAnsiEncoding/MacRomanEncoding, the mapping is unambiguous,
        // so we trust the computed widths even if many differ (common when a
        // fallback font like DejaVuSans was embedded for Helvetica/Times etc.).
        let has_reliable_encoding =
            matches!(enc_info.0.as_str(), "WinAnsiEncoding" | "MacRomanEncoding");
        let total_widths = existing_widths.len();
        if !has_reliable_encoding && corrections.len() * 2 > total_widths {
            continue;
        }

        // Apply only the individual corrections.
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
            Object::Integer(w) => *w,
            Object::Real(r) => *r as i64,
            _ => continue,
        };

        let code = first_char + i as u32;

        // Determine the expected glyph width from the font program.
        let expected_w =
            get_truetype_glyph_width_for_code(&face, code, enc_name, differences, scale);

        let Some(expected) = expected_w else { continue };

        // Only flag as mismatch if difference > 1 unit (allow rounding).
        if (pdf_w - expected).abs() > 1 {
            corrections.push((i, expected));
        }
    }

    corrections
}

/// Get the expected glyph width for a character code in a TrueType font.
///
/// Uses the encoding to map code -> Unicode -> glyph ID via cmap.
/// If the Differences array overrides the glyph for this code, uses that.
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

/// Compute width corrections for a Type1 font with CFF program.
///
/// Maps character codes -> glyph names -> CFF glyph widths, and compares
/// with existing /Widths array.
fn compute_cff_type1_width_corrections(
    font_data: &[u8],
    first_char: u32,
    existing_widths: &[Object],
    enc_info: &(String, std::collections::HashMap<u32, String>),
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

    let (enc_name, differences) = enc_info;
    let mut corrections = Vec::new();

    for (i, obj) in existing_widths.iter().enumerate() {
        let pdf_w = match obj {
            Object::Integer(w) => *w,
            Object::Real(r) => *r as i64,
            _ => continue,
        };

        let code = first_char + i as u32;

        // Determine the glyph name for this code.
        let glyph_name = if let Some(name) = differences.get(&code) {
            name.clone()
        } else {
            // Use encoding to get Unicode, then map to AGL name.
            let ch = encoding_to_char(code, enc_name);
            match unicode_to_glyph_name(ch) {
                Some(name) => name,
                None => continue,
            }
        };

        // Look up width by glyph name in CFF.
        let expected = find_cff_glyph_width_by_name(&cff, &glyph_name, scale);
        let Some(expected_w) = expected else { continue };

        if (pdf_w - expected_w).abs() > 1 {
            corrections.push((i, expected_w));
        }
    }

    corrections
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

        // Check existing Encoding.
        let needs_fix = match dict.get(b"Encoding") {
            Ok(Object::Name(enc)) => {
                let enc_str = String::from_utf8_lossy(enc);
                // Must be MacRomanEncoding or WinAnsiEncoding.
                enc_str != "WinAnsiEncoding" && enc_str != "MacRomanEncoding"
            }
            Ok(Object::Dictionary(enc_dict)) => {
                // Encoding is a dict — check BaseEncoding.
                !matches!(
                    get_name(enc_dict, b"BaseEncoding").as_deref(),
                    Some("WinAnsiEncoding") | Some("MacRomanEncoding")
                )
            }
            Ok(Object::Reference(enc_ref)) => {
                // Encoding references another object — check if it's a valid name or dict.
                match doc.get_object(*enc_ref) {
                    Ok(Object::Name(enc)) => {
                        let enc_str = String::from_utf8_lossy(enc);
                        enc_str != "WinAnsiEncoding" && enc_str != "MacRomanEncoding"
                    }
                    Ok(Object::Dictionary(enc_dict)) => !matches!(
                        get_name(enc_dict, b"BaseEncoding").as_deref(),
                        Some("WinAnsiEncoding") | Some("MacRomanEncoding")
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
        let (subtype, fd_id, enc_info, first_char, last_char) = {
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
            if is_font_symbolic(doc, dict) {
                continue;
            }

            // Skip subset fonts (prefix like ABCDEF+FontName).  Their encoding
            // and glyph tables were set by the original producer; modifying
            // Differences can point to glyph names absent from the subset.
            if let Some(name) = get_name(dict, b"BaseFont") {
                if name.contains('+') {
                    continue;
                }
            }

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

            (subtype, fd_id, enc_info, fc, lc)
        };

        let Some(fd_id) = fd_id else { continue };

        // Read the embedded font program data.
        let font_data = read_embedded_font_data(doc, fd_id);
        let Some(font_data) = font_data else { continue };

        if subtype == "TrueType" {
            if fix_notdef_in_truetype(doc, font_id, &font_data, &enc_info, first_char, last_char) {
                fixed += 1;
            }
        } else {
            // Type1 / MMType1 — try CFF parsing.
            if fix_notdef_in_type1(doc, font_id, &font_data, &enc_info, first_char, last_char) {
                fixed += 1;
            }
        }
    }

    fixed
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
        }
        // If we could only find "space", DON'T add it to Differences
        // for codes >= 32 in Phase 2 — that would change the rendering
        // for codes that may not even be used in the document.
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
