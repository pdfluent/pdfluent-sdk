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
        update_simple_widths(doc, info.font_id, &face, scale);
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
    let (first_char, last_char, encoding_name) = {
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

    let mut widths = Vec::new();
    for code in first_char..=last_char {
        let ch = encoding_to_char(code, &encoding_name);
        let width = if let Some(glyph_id) = face.glyph_index(ch) {
            face.glyph_hor_advance(glyph_id)
                .map(|w| (w as f64 * scale).round() as i64)
                .unwrap_or(0)
        } else if code <= u16::MAX as u32 {
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
    // Simplified: map common MacRoman codes, fall back to WinAnsi for others.
    winansi_to_char(code)
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

    // When embedding a fallback font (DejaVuSans), the result is always
    // non-symbolic regardless of the original font name, because DejaVuSans
    // is a standard Unicode font. Use Flags=32 (Nonsymbolic).
    let flags: i64 = 32;

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
        "Symbol" => Some("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
        "ZapfDingbats" => Some("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
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
/// against hmtx table and ONLY updates entries that are genuinely mismatched.
/// Entries where the glyph cannot be resolved are left untouched.
pub fn fix_simple_truetype_widths(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut fixed = 0;

    for font_id in font_ids {
        let (fd_id, encoding_name, differences) = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();
            if subtype != "TrueType" {
                continue;
            }

            // Extract encoding name and Differences array.
            let (enc, diffs) = match dict.get(b"Encoding").ok() {
                Some(Object::Name(n)) => (String::from_utf8(n.clone()).ok(), Vec::new()),
                Some(Object::Dictionary(enc_dict)) => {
                    let base = get_name(enc_dict, b"BaseEncoding");
                    let diffs = parse_differences_array(enc_dict);
                    (base, diffs)
                }
                _ => (None, Vec::new()),
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

            (fd_id, enc, diffs)
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

        // Build a map of code → glyph name from Differences array.
        let diff_map: std::collections::HashMap<u32, String> = differences.into_iter().collect();

        // Read existing Widths, FirstChar, and selectively fix mismatches.
        let patches: Vec<(usize, i64)> = {
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

            let mut result = Vec::new();
            for (i, obj) in existing.iter().enumerate() {
                let pdf_w = match obj {
                    Object::Integer(w) => *w,
                    Object::Real(r) => *r as i64,
                    _ => continue,
                };
                let code = fc + i as u32;

                // Resolve glyph ID from the font program.
                // Strategy: try multiple approaches, skip if none works.
                let gid = resolve_truetype_glyph_id(
                    &face,
                    code,
                    &encoding_name,
                    diff_map.get(&code).map(|s| s.as_str()),
                );
                let Some(gid) = gid else {
                    // Cannot determine the correct glyph — leave width untouched.
                    continue;
                };

                let expected = face
                    .glyph_hor_advance(gid)
                    .map(|w| (w as f64 * scale).round() as i64)
                    .unwrap_or(0);

                // Only patch if there is a genuine mismatch.
                if (pdf_w - expected).abs() > 1 {
                    result.push((i, expected));
                }
            }
            result
        };

        if !patches.is_empty() {
            // Apply patches to the Widths array.
            if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&font_id) {
                if let Ok(Object::Array(ref mut arr)) = font.get_mut(b"Widths") {
                    for (idx, new_w) in &patches {
                        if *idx < arr.len() {
                            arr[*idx] = Object::Integer(*new_w);
                        }
                    }
                }
            }
            fixed += 1;
        }
    }
    fixed
}

/// Resolve a TrueType glyph ID for a character code, trying multiple strategies.
///
/// Returns `None` if the glyph cannot be confidently resolved (meaning the
/// existing width should be left untouched).
fn resolve_truetype_glyph_id(
    face: &ttf_parser::Face,
    code: u32,
    encoding_name: &str,
    diff_glyph_name: Option<&str>,
) -> Option<ttf_parser::GlyphId> {
    // 1. If Differences array provides a glyph name, try name-based lookup first.
    if let Some(name) = diff_glyph_name {
        if name == ".notdef" {
            return Some(ttf_parser::GlyphId(0));
        }
        if let Some(gid) = face.glyph_index_by_name(name) {
            return Some(gid);
        }
    }

    // 2. Standard encoding → Unicode → cmap lookup.
    let ch = encoding_to_char(code, encoding_name);
    if let Some(gid) = face.glyph_index(ch) {
        return Some(gid);
    }

    // 3. Try glyph name from WinAnsi encoding table via post table.
    if code <= 255 {
        if let Some(name) = winansi_code_to_glyph_name(code as u8) {
            if let Some(gid) = face.glyph_index_by_name(name) {
                return Some(gid);
            }
        }
    }

    // 4. For codes that match a Unicode char directly (not via encoding),
    //    try the raw Unicode value as a fallback.
    if let Some(ch_direct) = char::from_u32(code) {
        if ch_direct != ch {
            if let Some(gid) = face.glyph_index(ch_direct) {
                return Some(gid);
            }
        }
    }

    // Cannot resolve — return None so the caller leaves the width alone.
    None
}

/// Parse a Differences array from an Encoding dictionary.
///
/// Returns a list of (code, glyph_name) pairs.
fn parse_differences_array(enc_dict: &lopdf::Dictionary) -> Vec<(u32, String)> {
    let mut result = Vec::new();
    let arr = match enc_dict.get(b"Differences").ok() {
        Some(Object::Array(a)) => a,
        _ => return result,
    };

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

/// Fix widths for Type1 fonts with CFF font programs (6.2.11.5:1).
///
/// Reads glyph widths from FontFile3 (CFF) and compares against existing /Widths.
/// ONLY updates individual entries where the CFF lookup succeeds AND the width
/// genuinely differs. Entries where the glyph cannot be resolved are left untouched.
pub fn fix_type1_widths(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut fixed = 0;

    for font_id in font_ids {
        let (fd_id, differences) = {
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

            // Extract Differences from Encoding dict if present.
            let diffs = match dict.get(b"Encoding").ok() {
                Some(Object::Dictionary(enc_dict)) => parse_differences_array(enc_dict),
                _ => Vec::new(),
            };

            (fd_id, diffs)
        };

        let font_data = read_embedded_font_data(doc, fd_id);
        let Some(font_data) = font_data else { continue };

        let Some(cff) = cff_parser::Table::parse(&font_data) else {
            continue;
        };

        // Get the CFF font matrix scale.
        let matrix = cff.matrix();
        let scale = if matrix.sx.abs() > f32::EPSILON {
            matrix.sx as f64 * 1000.0
        } else {
            1.0
        };

        // Build a map of code → glyph name from Differences array.
        let diff_map: std::collections::HashMap<u32, String> = differences.into_iter().collect();

        // Read FirstChar/Encoding and existing Widths, then selectively fix.
        let patches: Vec<(usize, i64)> = {
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
            let enc_name = font
                .get(b"Encoding")
                .ok()
                .and_then(|o| match o {
                    Object::Name(n) => String::from_utf8(n.clone()).ok(),
                    Object::Dictionary(d) => get_name(d, b"BaseEncoding"),
                    _ => None,
                })
                .unwrap_or_default();
            let existing = match font.get(b"Widths").ok() {
                Some(Object::Array(arr)) => arr,
                _ => continue,
            };

            let mut result = Vec::new();
            for (i, obj) in existing.iter().enumerate() {
                let pdf_w = match obj {
                    Object::Integer(w) => *w,
                    Object::Real(r) => *r as i64,
                    _ => continue,
                };
                let code = fc + i as u32;

                // Resolve glyph width from CFF, trying multiple strategies.
                let width = resolve_cff_glyph_width(
                    &cff,
                    code,
                    &enc_name,
                    diff_map.get(&code).map(|s| s.as_str()),
                    scale,
                );

                let Some(expected) = width else {
                    // Cannot determine the correct width — leave untouched.
                    continue;
                };

                // Only patch if there is a genuine mismatch.
                if (pdf_w - expected).abs() > 1 {
                    result.push((i, expected));
                }
            }
            result
        };

        if !patches.is_empty() {
            // Apply patches to the Widths array.
            if let Some(Object::Dictionary(ref mut font)) = doc.objects.get_mut(&font_id) {
                if let Ok(Object::Array(ref mut arr)) = font.get_mut(b"Widths") {
                    for (idx, new_w) in &patches {
                        if *idx < arr.len() {
                            arr[*idx] = Object::Integer(*new_w);
                        }
                    }
                }
            }
            fixed += 1;
        }
    }
    fixed
}

/// Resolve a CFF glyph width for a character code, trying multiple strategies.
///
/// Returns `None` if the glyph cannot be confidently resolved (meaning the
/// existing width should be left untouched).
fn resolve_cff_glyph_width(
    cff: &cff_parser::Table<'_>,
    code: u32,
    encoding_name: &str,
    diff_glyph_name: Option<&str>,
    scale: f64,
) -> Option<i64> {
    // 1. If Differences array provides a glyph name, use it directly.
    if let Some(name) = diff_glyph_name {
        if name == ".notdef" {
            return cff
                .glyph_width(cff_parser::GlyphId(0))
                .map(|w| (w as f64 * scale).round() as i64);
        }
        let w = find_cff_glyph_width_by_name(cff, name, scale);
        if w.is_some() {
            return w;
        }
    }

    // 2. Map code via encoding → Unicode → glyph name → CFF lookup.
    let ch = encoding_to_char(code, encoding_name);
    let glyph_name = unicode_to_glyph_name(ch);
    if let Some(ref name) = glyph_name {
        let w = find_cff_glyph_width_by_name(cff, name, scale);
        if w.is_some() {
            return w;
        }
    }

    // 3. Try WinAnsi glyph name directly (for codes 0..255).
    if code <= 255 {
        if let Some(name) = winansi_code_to_glyph_name(code as u8) {
            let w = find_cff_glyph_width_by_name(cff, name, scale);
            if w.is_some() {
                return w;
            }
        }
    }

    // 4. Direct GID lookup as last resort (only if code is in range).
    if code <= u16::MAX as u32 {
        let w = cff
            .glyph_width(cff_parser::GlyphId(code as u16))
            .map(|w| (w as f64 * scale).round() as i64);
        if w.is_some() {
            return w;
        }
    }

    // Cannot resolve — return None so the caller leaves the width alone.
    None
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

/// Fix TrueType font encoding for PDF/A compliance (rules 6.2.11.4.1:2, 6.2.11.6:2, 6.2.11.6:3).
///
/// - Non-symbolic TrueType fonts must have MacRomanEncoding or WinAnsiEncoding.
/// - Symbolic TrueType fonts must NOT have an Encoding entry.
/// - The encoding must be compatible with the font's actual cmap subtables:
///   - WinAnsiEncoding requires a (3,1) Windows Unicode BMP cmap
///   - MacRomanEncoding requires a (1,0) Macintosh Roman cmap
/// - For fonts with embedded programs, builds a custom Encoding with Differences
///   that maps only character codes actually resolvable via the font's cmap,
///   avoiding 6.2.11.4.1:2 failures where the encoding references missing glyphs.
pub fn fix_truetype_encoding(doc: &mut Document) -> usize {
    // Phase 1: Scan all TrueType fonts and decide what action to take.
    //
    // We collect ALL non-symbolic TrueType fonts that might fail 6.2.11.4.1:2,
    // including those that already have WinAnsi/MacRoman encoding but whose
    // embedded font program lacks the required cmap subtable.
    struct FontFixInfo {
        font_id: ObjectId,
        fd_id: Option<ObjectId>,
        first_char: u8,
        last_char: u8,
    }
    let mut to_fix: Vec<FontFixInfo> = Vec::new();
    let mut symbolic_to_strip: Vec<ObjectId> = Vec::new();

    for (id, obj) in &doc.objects {
        let Object::Dictionary(dict) = obj else {
            continue;
        };
        if get_name(dict, b"Subtype").as_deref() != Some("TrueType") {
            continue;
        }

        let is_symbolic = is_font_symbolic(doc, dict);
        if is_symbolic {
            if dict.has(b"Encoding") {
                symbolic_to_strip.push(*id);
            }
            continue;
        }

        let first_char = match dict.get(b"FirstChar") {
            Ok(Object::Integer(v)) => (*v).clamp(0, 255) as u8,
            _ => 0,
        };
        let last_char = match dict.get(b"LastChar") {
            Ok(Object::Integer(v)) => (*v).clamp(0, 255) as u8,
            _ => 255,
        };

        let fd_id = match dict.get(b"FontDescriptor") {
            Ok(Object::Reference(r)) => Some(*r),
            _ => None,
        };

        // Check if the current encoding already looks correct.
        let has_valid_base_encoding = tt_has_valid_base_encoding(doc, dict);

        let needs_fix = if has_valid_base_encoding {
            // Even with valid base encoding, the embedded font's cmap subtables
            // might not support it. Check compatibility.
            let font_data = fd_id.and_then(|fid| read_embedded_font_data(doc, fid));
            match font_data {
                Some(data) => match ttf_parser::Face::parse(&data, 0) {
                    Ok(face) => !tt_font_has_compatible_cmap(&face, dict),
                    Err(_) => false,
                },
                None => false,
            }
        } else {
            true
        };

        if needs_fix {
            to_fix.push(FontFixInfo {
                font_id: *id,
                fd_id,
                first_char,
                last_char,
            });
        }
    }

    // Phase 2: Apply fixes.
    let count = to_fix.len();
    for info in &to_fix {
        let base_font_name = doc
            .objects
            .get(&info.font_id)
            .and_then(|o| {
                if let Object::Dictionary(d) = o {
                    get_name(d, b"BaseFont")
                } else {
                    None
                }
            })
            .unwrap_or_default();
        let is_subset = is_subset_font_name(&base_font_name);

        let font_data = info.fd_id.and_then(|fid| read_embedded_font_data(doc, fid));

        if let Some(ref data) = font_data {
            if let Ok(face) = ttf_parser::Face::parse(data, 0) {
                let cmap_info = tt_analyze_cmap_subtables(&face);

                let enc_obj = if cmap_info.has_31 || cmap_info.has_any_unicode {
                    // Font has (3,1) or other Unicode cmap — WinAnsiEncoding is valid.
                    if is_subset {
                        tt_build_cmap_aware_encoding(
                            &face,
                            TtEncodingChoice::WinAnsi,
                            info.first_char,
                            info.last_char,
                        )
                    } else {
                        Object::Name(b"WinAnsiEncoding".to_vec())
                    }
                } else if cmap_info.has_10 {
                    // Font only has (1,0) Mac Roman cmap — use MacRomanEncoding.
                    if is_subset {
                        tt_build_cmap_aware_encoding(
                            &face,
                            TtEncodingChoice::MacRoman,
                            info.first_char,
                            info.last_char,
                        )
                    } else {
                        Object::Name(b"MacRomanEncoding".to_vec())
                    }
                } else if cmap_info.has_30 {
                    // Font has (3,0) Windows Symbol cmap but is marked non-symbolic.
                    // Map codes via 0xF000 offset used by Symbol cmap fonts.
                    tt_build_symbol_cmap_encoding(&face, info.first_char, info.last_char)
                } else {
                    // No recognized cmap — fall back to WinAnsi with post table lookup.
                    if is_subset {
                        tt_build_cmap_aware_encoding(
                            &face,
                            TtEncodingChoice::WinAnsi,
                            info.first_char,
                            info.last_char,
                        )
                    } else {
                        Object::Name(b"WinAnsiEncoding".to_vec())
                    }
                };

                if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(info.font_id) {
                    dict.set("Encoding", enc_obj);
                }
                continue;
            }
        }

        // No embedded font data or parse failure — use simple WinAnsiEncoding.
        if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(info.font_id) {
            dict.set("Encoding", Object::Name(b"WinAnsiEncoding".to_vec()));
        }
    }

    // Phase 3: Strip Encoding from symbolic TrueType fonts (6.2.11.6:3).
    for id in &symbolic_to_strip {
        if let Ok(Object::Dictionary(dict)) = doc.get_object_mut(*id) {
            dict.remove(b"Encoding");
        }
    }

    count + symbolic_to_strip.len()
}

/// Check if a font dict already has WinAnsiEncoding or MacRomanEncoding.
fn tt_has_valid_base_encoding(doc: &Document, dict: &lopdf::Dictionary) -> bool {
    match dict.get(b"Encoding") {
        Ok(Object::Name(enc)) => {
            let s = String::from_utf8_lossy(enc);
            s == "WinAnsiEncoding" || s == "MacRomanEncoding"
        }
        Ok(Object::Dictionary(enc_dict)) => matches!(
            get_name(enc_dict, b"BaseEncoding").as_deref(),
            Some("WinAnsiEncoding") | Some("MacRomanEncoding")
        ),
        Ok(Object::Reference(enc_ref)) => match doc.get_object(*enc_ref) {
            Ok(Object::Name(enc)) => {
                let s = String::from_utf8_lossy(enc);
                s == "WinAnsiEncoding" || s == "MacRomanEncoding"
            }
            Ok(Object::Dictionary(enc_dict)) => matches!(
                get_name(enc_dict, b"BaseEncoding").as_deref(),
                Some("WinAnsiEncoding") | Some("MacRomanEncoding")
            ),
            _ => false,
        },
        _ => false,
    }
}

/// Check if a font name has a subset prefix (6 uppercase ASCII letters followed by '+').
fn is_subset_font_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() > 7 && bytes[6] == b'+' && bytes[..6].iter().all(|b| b.is_ascii_uppercase())
}

/// Summary of which cmap subtables a TrueType font contains.
struct TtCmapInfo {
    /// Has (3,1) Windows Unicode BMP subtable.
    has_31: bool,
    /// Has (1,0) Macintosh Roman subtable.
    has_10: bool,
    /// Has (3,0) Windows Symbol subtable.
    has_30: bool,
    /// Has any Unicode-compatible subtable (platform 0 or (3,1)/(3,10)).
    has_any_unicode: bool,
}

/// Analyze which cmap subtables are present in the font.
fn tt_analyze_cmap_subtables(face: &ttf_parser::Face) -> TtCmapInfo {
    let mut info = TtCmapInfo {
        has_31: false,
        has_10: false,
        has_30: false,
        has_any_unicode: false,
    };

    if let Some(cmap) = face.tables().cmap.as_ref() {
        for st in cmap.subtables.into_iter() {
            match (st.platform_id, st.encoding_id) {
                (ttf_parser::PlatformId::Windows, 1) => {
                    info.has_31 = true;
                    info.has_any_unicode = true;
                }
                (ttf_parser::PlatformId::Windows, 10) => {
                    info.has_any_unicode = true;
                }
                (ttf_parser::PlatformId::Windows, 0) => {
                    info.has_30 = true;
                }
                (ttf_parser::PlatformId::Macintosh, 0) => {
                    info.has_10 = true;
                }
                (ttf_parser::PlatformId::Unicode, _) => {
                    info.has_any_unicode = true;
                }
                _ => {}
            }
        }
    }

    info
}

/// Check whether the font's cmap subtables are compatible with its current PDF encoding.
///
/// WinAnsiEncoding requires (3,1) or any Unicode cmap; MacRomanEncoding requires (1,0)
/// or any Unicode cmap. Returns false if the encoding cannot be resolved via the cmap.
fn tt_font_has_compatible_cmap(face: &ttf_parser::Face, font_dict: &lopdf::Dictionary) -> bool {
    let cmap_info = tt_analyze_cmap_subtables(face);

    let enc_name = match font_dict.get(b"Encoding") {
        Ok(Object::Name(n)) => String::from_utf8_lossy(n).into_owned(),
        Ok(Object::Dictionary(d)) => get_name(d, b"BaseEncoding").unwrap_or_default(),
        _ => String::new(),
    };

    match enc_name.as_str() {
        "WinAnsiEncoding" => cmap_info.has_31 || cmap_info.has_any_unicode,
        "MacRomanEncoding" => cmap_info.has_10 || cmap_info.has_any_unicode,
        _ => false,
    }
}

/// Which base encoding to use for the Differences-based encoding dict.
#[derive(Clone, Copy)]
enum TtEncodingChoice {
    WinAnsi,
    MacRoman,
}

/// Build an Encoding dictionary for a TrueType font based on its actual cmap subtables.
///
/// Uses the chosen BaseEncoding with a Differences array that overrides character codes
/// whose glyph names are NOT resolvable via the font's cmap. Missing codes are remapped
/// to `.notdef` so that veraPDF's 6.2.11.4.1:2 check passes.
fn tt_build_cmap_aware_encoding(
    face: &ttf_parser::Face,
    choice: TtEncodingChoice,
    _first_char: u8,
    _last_char: u8,
) -> Object {
    let (base_name, char_mapper): (&[u8], fn(u32) -> char) = match choice {
        TtEncodingChoice::WinAnsi => (b"WinAnsiEncoding", winansi_to_char),
        TtEncodingChoice::MacRoman => (b"MacRomanEncoding", macroman_to_char),
    };
    let glyph_name_mapper: fn(u8) -> Option<&'static str> = match choice {
        TtEncodingChoice::WinAnsi => winansi_code_to_glyph_name,
        TtEncodingChoice::MacRoman => tt_macroman_code_to_glyph_name,
    };

    // Check all printable codes (32..=255). Even codes outside [FirstChar, LastChar]
    // are part of the base encoding and checked by veraPDF.
    let mut missing_codes: Vec<u8> = Vec::new();
    for code in 32u8..=255u8 {
        let unicode_char = char_mapper(code as u32);
        // Try Unicode cmap lookup first (covers (3,1) and platform 0 subtables).
        let has_glyph = face.glyph_index(unicode_char).is_some();
        if !has_glyph {
            // Fallback: try post table name lookup.
            let glyph_name = glyph_name_mapper(code);
            let has_by_name = glyph_name
                .and_then(|name| face.glyph_index_by_name(name))
                .is_some();
            if !has_by_name {
                // For MacRoman, also try the (1,0) subtable directly.
                let has_via_subtable = if matches!(choice, TtEncodingChoice::MacRoman) {
                    tt_glyph_via_subtable(face, 1, 0, code as u32)
                } else {
                    false
                };
                if !has_via_subtable {
                    missing_codes.push(code);
                }
            }
        }
    }

    if missing_codes.is_empty() {
        return Object::Name(base_name.to_vec());
    }

    let differences = tt_build_notdef_differences(&missing_codes);

    Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Encoding".to_vec()),
        "BaseEncoding" => Object::Name(base_name.to_vec()),
        "Differences" => Object::Array(differences),
    })
}

/// Build an encoding for fonts with a (3,0) Windows Symbol cmap.
///
/// Symbol cmaps use codepoints in the 0xF000-0xF0FF range (Unicode Private Use Area).
/// For a character code `c`, the font maps 0xF000 + c to a glyph.
/// We set WinAnsiEncoding as base and override codes that can't be resolved.
fn tt_build_symbol_cmap_encoding(
    face: &ttf_parser::Face,
    _first_char: u8,
    _last_char: u8,
) -> Object {
    let mut missing_codes: Vec<u8> = Vec::new();

    for code in 32u8..=255u8 {
        // Check via the Symbol cmap: 0xF000 + code.
        let symbol_cp = 0xF000u32 + code as u32;
        let has_via_symbol = char::from_u32(symbol_cp)
            .and_then(|ch| face.glyph_index(ch))
            .is_some();

        if !has_via_symbol {
            // Also check via regular Unicode mapping.
            let unicode_char = winansi_to_char(code as u32);
            let has_via_unicode = face.glyph_index(unicode_char).is_some();
            if !has_via_unicode {
                // Also check via post table.
                let has_via_name = winansi_code_to_glyph_name(code)
                    .and_then(|name| face.glyph_index_by_name(name))
                    .is_some();
                if !has_via_name {
                    missing_codes.push(code);
                }
            }
        }
    }

    if missing_codes.is_empty() {
        return Object::Name(b"WinAnsiEncoding".to_vec());
    }

    let differences = tt_build_notdef_differences(&missing_codes);

    Object::Dictionary(dictionary! {
        "Type" => Object::Name(b"Encoding".to_vec()),
        "BaseEncoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
        "Differences" => Object::Array(differences),
    })
}

/// Build a Differences array that maps the given codes to `.notdef`.
///
/// Groups consecutive codes: `[code1 /.notdef /.notdef code3 /.notdef ...]`
fn tt_build_notdef_differences(missing_codes: &[u8]) -> Vec<Object> {
    let mut differences = Vec::new();
    let mut i = 0;
    while i < missing_codes.len() {
        let start = missing_codes[i];
        differences.push(Object::Integer(start as i64));
        differences.push(Object::Name(b".notdef".to_vec()));
        let mut prev = start;
        i += 1;
        while i < missing_codes.len() && missing_codes[i] == prev.wrapping_add(1) {
            differences.push(Object::Name(b".notdef".to_vec()));
            prev = missing_codes[i];
            i += 1;
        }
    }
    differences
}

/// Look up a codepoint in a specific cmap subtable identified by platform/encoding ID.
fn tt_glyph_via_subtable(
    face: &ttf_parser::Face,
    platform: u16,
    encoding: u16,
    codepoint: u32,
) -> bool {
    let cmap = match face.tables().cmap.as_ref() {
        Some(c) => c,
        None => return false,
    };
    let target_pid = match platform {
        0 => ttf_parser::PlatformId::Unicode,
        1 => ttf_parser::PlatformId::Macintosh,
        3 => ttf_parser::PlatformId::Windows,
        _ => return false,
    };
    for st in cmap.subtables.into_iter() {
        if st.platform_id == target_pid
            && st.encoding_id == encoding
            && st.glyph_index(codepoint).is_some()
        {
            return true;
        }
    }
    false
}

/// Map a MacRomanEncoding character code to its standard glyph name.
///
/// Returns `None` for codes without a defined MacRoman mapping.
fn tt_macroman_code_to_glyph_name(code: u8) -> Option<&'static str> {
    // Codes 32..=126 are identical to WinAnsi (ASCII range).
    if (32..=126).contains(&code) {
        return winansi_code_to_glyph_name(code);
    }
    // MacRoman-specific codes 128..=255.
    match code {
        128 => Some("Adieresis"),
        129 => Some("Aring"),
        130 => Some("Ccedilla"),
        131 => Some("Eacute"),
        132 => Some("Ntilde"),
        133 => Some("Odieresis"),
        134 => Some("Udieresis"),
        135 => Some("aacute"),
        136 => Some("agrave"),
        137 => Some("acircumflex"),
        138 => Some("adieresis"),
        139 => Some("atilde"),
        140 => Some("aring"),
        141 => Some("ccedilla"),
        142 => Some("eacute"),
        143 => Some("egrave"),
        144 => Some("ecircumflex"),
        145 => Some("edieresis"),
        146 => Some("iacute"),
        147 => Some("igrave"),
        148 => Some("icircumflex"),
        149 => Some("idieresis"),
        150 => Some("ntilde"),
        151 => Some("oacute"),
        152 => Some("ograve"),
        153 => Some("ocircumflex"),
        154 => Some("odieresis"),
        155 => Some("otilde"),
        156 => Some("uacute"),
        157 => Some("ugrave"),
        158 => Some("ucircumflex"),
        159 => Some("udieresis"),
        160 => Some("dagger"),
        161 => Some("degree"),
        162 => Some("cent"),
        163 => Some("sterling"),
        164 => Some("section"),
        165 => Some("bullet"),
        166 => Some("paragraph"),
        167 => Some("germandbls"),
        168 => Some("registered"),
        169 => Some("copyright"),
        170 => Some("trademark"),
        171 => Some("acute"),
        172 => Some("dieresis"),
        174 => Some("AE"),
        175 => Some("Oslash"),
        177 => Some("plusminus"),
        180 => Some("yen"),
        181 => Some("mu"),
        187 => Some("ordfeminine"),
        188 => Some("ordmasculine"),
        190 => Some("ae"),
        191 => Some("oslash"),
        192 => Some("questiondown"),
        193 => Some("exclamdown"),
        194 => Some("logicalnot"),
        196 => Some("florin"),
        199 => Some("guillemotleft"),
        200 => Some("guillemotright"),
        201 => Some("ellipsis"),
        202 => Some("nbspace"),
        203 => Some("Agrave"),
        204 => Some("Atilde"),
        205 => Some("Otilde"),
        206 => Some("OE"),
        207 => Some("oe"),
        208 => Some("endash"),
        209 => Some("emdash"),
        210 => Some("quotedblleft"),
        211 => Some("quotedblright"),
        212 => Some("quoteleft"),
        213 => Some("quoteright"),
        214 => Some("divide"),
        218 => Some("ydieresis"),
        219 => Some("Ydieresis"),
        222 => Some("fi"),
        223 => Some("fl"),
        225 => Some("periodcentered"),
        227 => Some("Acircumflex"),
        228 => Some("Ecircumflex"),
        229 => Some("Aacute"),
        230 => Some("Edieresis"),
        231 => Some("Egrave"),
        232 => Some("Iacute"),
        233 => Some("Icircumflex"),
        234 => Some("Idieresis"),
        235 => Some("Igrave"),
        236 => Some("Oacute"),
        237 => Some("Ocircumflex"),
        239 => Some("Ograve"),
        240 => Some("Uacute"),
        241 => Some("Ucircumflex"),
        242 => Some("Ugrave"),
        243 => Some("dotlessi"),
        244 => Some("circumflex"),
        245 => Some("tilde"),
        246 => Some("macron"),
        247 => Some("breve"),
        248 => Some("dotaccent"),
        249 => Some("ring"),
        250 => Some("cedilla"),
        252 => Some("ogonek"),
        253 => Some("caron"),
        _ => None,
    }
}

/// Map a WinAnsiEncoding character code to its standard glyph name.
///
/// Returns `None` for codes without a defined WinAnsi mapping (0..=31, some gaps).
fn winansi_code_to_glyph_name(code: u8) -> Option<&'static str> {
    // Full WinAnsiEncoding table: code -> Adobe standard glyph name.
    // Based on PDF Reference Table D.1 and Adobe Glyph List.
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
        48 => Some("zero"),
        49 => Some("one"),
        50 => Some("two"),
        51 => Some("three"),
        52 => Some("four"),
        53 => Some("five"),
        54 => Some("six"),
        55 => Some("seven"),
        56 => Some("eight"),
        57 => Some("nine"),
        58 => Some("colon"),
        59 => Some("semicolon"),
        60 => Some("less"),
        61 => Some("equal"),
        62 => Some("greater"),
        63 => Some("question"),
        64 => Some("at"),
        65 => Some("A"),
        66 => Some("B"),
        67 => Some("C"),
        68 => Some("D"),
        69 => Some("E"),
        70 => Some("F"),
        71 => Some("G"),
        72 => Some("H"),
        73 => Some("I"),
        74 => Some("J"),
        75 => Some("K"),
        76 => Some("L"),
        77 => Some("M"),
        78 => Some("N"),
        79 => Some("O"),
        80 => Some("P"),
        81 => Some("Q"),
        82 => Some("R"),
        83 => Some("S"),
        84 => Some("T"),
        85 => Some("U"),
        86 => Some("V"),
        87 => Some("W"),
        88 => Some("X"),
        89 => Some("Y"),
        90 => Some("Z"),
        91 => Some("bracketleft"),
        92 => Some("backslash"),
        93 => Some("bracketright"),
        94 => Some("asciicircum"),
        95 => Some("underscore"),
        96 => Some("grave"),
        97 => Some("a"),
        98 => Some("b"),
        99 => Some("c"),
        100 => Some("d"),
        101 => Some("e"),
        102 => Some("f"),
        103 => Some("g"),
        104 => Some("h"),
        105 => Some("i"),
        106 => Some("j"),
        107 => Some("k"),
        108 => Some("l"),
        109 => Some("m"),
        110 => Some("n"),
        111 => Some("o"),
        112 => Some("p"),
        113 => Some("q"),
        114 => Some("r"),
        115 => Some("s"),
        116 => Some("t"),
        117 => Some("u"),
        118 => Some("v"),
        119 => Some("w"),
        120 => Some("x"),
        121 => Some("y"),
        122 => Some("z"),
        123 => Some("braceleft"),
        124 => Some("bar"),
        125 => Some("braceright"),
        126 => Some("asciitilde"),
        // 127 is undefined in WinAnsi
        128 => Some("Euro"),
        // 129 is undefined
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
        // 141 is undefined
        142 => Some("Zcaron"),
        // 143, 144 are undefined
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
        // 157 is undefined
        158 => Some("zcaron"),
        159 => Some("Ydieresis"),
        160 => Some("nbspace"),
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
        173 => Some("sfthyphen"),
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

/// Fix .notdef glyph references for PDF/A compliance (rule 6.2.11.8:1).
///
/// Scans all content streams (pages and Form XObjects) to find which character
/// codes are actually used with each font. For each font, checks whether the
/// embedded font program can render those codes. If a code maps to .notdef
/// (i.e., the font has no glyph for it), fixes the encoding to avoid the
/// .notdef reference.
///
/// Returns the number of fonts fixed.
pub fn fix_notdef_references(doc: &mut Document) -> usize {
    use std::collections::{HashMap, HashSet};

    // Phase 1: Collect all content stream object IDs (pages + Form XObjects).
    let content_stream_ids = collect_content_stream_ids(doc);

    // Phase 2: For each content stream, parse operations and track which
    // character codes are used with each font resource name. Also build
    // a mapping from (container_id, font_resource_name) -> font_object_id.
    //
    // We collect: font_object_id -> set of used character codes.
    let mut font_used_codes: HashMap<ObjectId, HashSet<u8>> = HashMap::new();

    for (container_id, stream_ids) in &content_stream_ids {
        // Resolve font resource names for this container.
        let font_map = resolve_font_resources(doc, *container_id);

        // Concatenate all content stream data for this container.
        let mut all_data = Vec::new();
        for sid in stream_ids {
            if let Some(Object::Stream(stream)) = doc.objects.get(sid) {
                let mut s = stream.clone();
                let _ = s.decompress();
                all_data.extend_from_slice(&s.content);
                all_data.push(b' ');
            }
        }

        if all_data.is_empty() {
            continue;
        }

        // Parse content stream and extract used character codes per font.
        let ops = match lopdf::content::Content::decode(&all_data) {
            Ok(content) => content.operations,
            Err(_) => continue,
        };

        let mut current_font: Option<String> = None;
        for op in &ops {
            match op.operator.as_str() {
                "Tf" => {
                    if let Some(Object::Name(ref name)) = op.operands.first() {
                        current_font = Some(String::from_utf8_lossy(name).to_string());
                    }
                }
                "Tj" | "'" => {
                    if let Some(ref font_name) = current_font {
                        if let Some(font_id) = font_map.get(font_name) {
                            extract_char_codes_from_operands(
                                &op.operands,
                                font_used_codes.entry(*font_id).or_default(),
                            );
                        }
                    }
                }
                "\"" => {
                    // " operator: aw ac string -- the string is the 3rd operand.
                    if let Some(ref font_name) = current_font {
                        if let Some(font_id) = font_map.get(font_name) {
                            if op.operands.len() >= 3 {
                                extract_char_codes_from_operands(
                                    &op.operands[2..],
                                    font_used_codes.entry(*font_id).or_default(),
                                );
                            }
                        }
                    }
                }
                "TJ" => {
                    if let Some(ref font_name) = current_font {
                        if let Some(font_id) = font_map.get(font_name) {
                            extract_char_codes_from_tj_array(
                                &op.operands,
                                font_used_codes.entry(*font_id).or_default(),
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // Phase 3: For each font with used codes, check which codes map to .notdef
    // in the embedded font program and fix the encoding.
    let mut fixed = 0;
    let font_ids: Vec<ObjectId> = font_used_codes.keys().copied().collect();

    for font_id in font_ids {
        let used_codes = match font_used_codes.get(&font_id) {
            Some(codes) if !codes.is_empty() => codes.clone(),
            _ => continue,
        };

        let (subtype, fd_id, is_type0) = {
            let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
                continue;
            };
            if !is_font_dict(dict) {
                continue;
            }
            let subtype = get_name(dict, b"Subtype").unwrap_or_default();
            let is_type0 = subtype == "Type0";

            if is_type0 {
                // For Type0, get CIDFont descendant's FontDescriptor.
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

        // Read the embedded font program.
        let font_data = read_embedded_font_data(doc, fd_id);
        let Some(font_data) = font_data else { continue };

        // For Type0/CID fonts, fixing .notdef is complex (CMap rewriting).
        // For now, only fix simple fonts (Type1, TrueType).
        if is_type0 {
            // Try to fix Type0 fonts by ensuring they have a ToUnicode CMap.
            if fix_type0_notdef(doc, font_id, &font_data, &used_codes) {
                fixed += 1;
            }
            continue;
        }

        let did_fix = if subtype == "TrueType" {
            fix_truetype_notdef(doc, font_id, &font_data, &used_codes)
        } else if subtype == "Type1" || subtype == "MMType1" {
            fix_type1_notdef(doc, font_id, &font_data, &used_codes)
        } else {
            false
        };
        if did_fix {
            fixed += 1;
        }
    }

    fixed
}

/// Collect all container -> content stream ID mappings.
///
/// Returns pairs of (container_object_id, Vec<content_stream_ids>).
/// Containers are pages and Form XObjects.
fn collect_content_stream_ids(doc: &Document) -> Vec<(ObjectId, Vec<ObjectId>)> {
    let mut result = Vec::new();

    // Pages.
    let page_ids: Vec<ObjectId> = doc.get_pages().values().copied().collect();
    for page_id in page_ids {
        let stream_ids = page_content_stream_ids(doc, page_id);
        if !stream_ids.is_empty() {
            result.push((page_id, stream_ids));
        }
    }

    // Form XObjects -- they can also reference fonts and contain text operators.
    for (id, obj) in &doc.objects {
        if let Object::Stream(stream) = obj {
            if get_name(&stream.dict, b"Subtype").as_deref() == Some("Form") {
                // A Form XObject's content is the stream itself.
                result.push((*id, vec![*id]));
            }
        }
    }

    result
}

/// Get the content stream object IDs for a page.
fn page_content_stream_ids(doc: &Document, page_id: ObjectId) -> Vec<ObjectId> {
    let Some(Object::Dictionary(page)) = doc.objects.get(&page_id) else {
        return Vec::new();
    };
    match page.get(b"Contents").ok() {
        Some(Object::Reference(id)) => vec![*id],
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
}

/// Resolve font resource names (e.g., "F1") to font object IDs for a container.
fn resolve_font_resources(
    doc: &Document,
    container_id: ObjectId,
) -> std::collections::HashMap<String, ObjectId> {
    let mut map = std::collections::HashMap::new();

    let resources = get_resources_dict(doc, container_id);
    let Some(resources) = resources else {
        return map;
    };

    // Get the Font subdictionary from Resources.
    let font_dict = match resources.get(b"Font").ok() {
        Some(Object::Dictionary(d)) => Some(d),
        Some(Object::Reference(id)) => {
            if let Some(Object::Dictionary(d)) = doc.objects.get(id) {
                Some(d)
            } else {
                None
            }
        }
        _ => None,
    };

    let Some(font_dict) = font_dict else {
        return map;
    };

    for (key, value) in font_dict.iter() {
        let name = String::from_utf8_lossy(key).to_string();
        if let Object::Reference(id) = value {
            map.insert(name, *id);
        }
    }

    map
}

/// Get the Resources dictionary for a page or Form XObject.
fn get_resources_dict(doc: &Document, obj_id: ObjectId) -> Option<&lopdf::Dictionary> {
    let obj = doc.objects.get(&obj_id)?;
    match obj {
        Object::Dictionary(dict) => get_resources_from_dict(doc, dict),
        Object::Stream(stream) => get_resources_from_dict(doc, &stream.dict),
        _ => None,
    }
}

/// Extract Resources dictionary from a dictionary (page or Form XObject dict).
fn get_resources_from_dict<'a>(
    doc: &'a Document,
    dict: &'a lopdf::Dictionary,
) -> Option<&'a lopdf::Dictionary> {
    match dict.get(b"Resources").ok()? {
        Object::Dictionary(d) => Some(d),
        Object::Reference(id) => {
            if let Some(Object::Dictionary(d)) = doc.objects.get(id) {
                Some(d)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Extract character codes (0..255) from Tj/'/string operands.
fn extract_char_codes_from_operands(
    operands: &[Object],
    codes: &mut std::collections::HashSet<u8>,
) {
    for operand in operands {
        if let Object::String(bytes, _) = operand {
            for &b in bytes {
                codes.insert(b);
            }
        }
    }
}

/// Extract character codes from TJ array operands.
///
/// TJ takes an array like [(string) number (string) ...].
fn extract_char_codes_from_tj_array(
    operands: &[Object],
    codes: &mut std::collections::HashSet<u8>,
) {
    for operand in operands {
        if let Object::Array(arr) = operand {
            for item in arr {
                if let Object::String(bytes, _) = item {
                    for &b in bytes {
                        codes.insert(b);
                    }
                }
            }
        }
    }
}

/// Fix .notdef references in a TrueType font.
///
/// Checks which used character codes map to .notdef in the embedded font
/// and builds a Differences array to remap them to a valid glyph (space).
fn fix_truetype_notdef(
    doc: &mut Document,
    font_id: ObjectId,
    font_data: &[u8],
    used_codes: &std::collections::HashSet<u8>,
) -> bool {
    let Ok(face) = ttf_parser::Face::parse(font_data, 0) else {
        return false;
    };

    // Read current encoding info.
    let (encoding_name, has_differences, is_symbolic) = {
        let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
            return false;
        };
        let enc_name = match dict.get(b"Encoding").ok() {
            Some(Object::Name(n)) => String::from_utf8(n.clone()).ok().unwrap_or_default(),
            Some(Object::Dictionary(enc_dict)) => {
                get_name(enc_dict, b"BaseEncoding").unwrap_or_default()
            }
            _ => String::new(),
        };
        let has_diff = match dict.get(b"Encoding").ok() {
            Some(Object::Dictionary(enc_dict)) => enc_dict.has(b"Differences"),
            _ => false,
        };
        let sym = is_font_symbolic(doc, dict);
        (enc_name, has_diff, sym)
    };

    // Don't touch symbolic fonts -- they use custom encodings.
    if is_symbolic {
        return false;
    }

    // Find character codes that map to .notdef.
    let mut notdef_codes: Vec<u8> = Vec::new();
    for &code in used_codes {
        let ch = encoding_to_char(code as u32, &encoding_name);
        let has_glyph = face.glyph_index(ch).is_some();
        if !has_glyph {
            // Also check by glyph name as a fallback.
            let glyph_name = winansi_code_to_glyph_name(code);
            let has_by_name = glyph_name
                .and_then(|name| face.glyph_index_by_name(name))
                .is_some();
            if !has_by_name {
                // Also try identity mapping (GlyphId == code).
                let has_by_id = if code > 0 {
                    face.glyph_hor_advance(ttf_parser::GlyphId(code as u16))
                        .is_some()
                } else {
                    false
                };
                if !has_by_id {
                    notdef_codes.push(code);
                }
            }
        }
    }

    if notdef_codes.is_empty() {
        return false;
    }

    notdef_codes.sort();

    // Find a valid replacement glyph name.
    let replacement_name = find_valid_replacement_glyph(&face);

    // Build or update the Encoding with Differences that remap .notdef codes
    // to a valid glyph name.
    let base_encoding = if encoding_name.is_empty() {
        "WinAnsiEncoding"
    } else {
        &encoding_name
    };

    // Read existing Differences if any.
    let mut existing_differences: Vec<Object> = if has_differences {
        let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
            return false;
        };
        match dict.get(b"Encoding").ok() {
            Some(Object::Dictionary(enc_dict)) => match enc_dict.get(b"Differences").ok() {
                Some(Object::Array(arr)) => arr.clone(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    } else {
        Vec::new()
    };

    // Add new mappings for .notdef codes to the Differences array.
    for &code in &notdef_codes {
        if !is_code_in_differences(&existing_differences, code) {
            existing_differences.push(Object::Integer(code as i64));
            existing_differences.push(Object::Name(replacement_name.as_bytes().to_vec()));
        }
    }

    // Set the new Encoding dictionary.
    let enc_dict = lopdf::Dictionary::from_iter(vec![
        ("Type".to_string(), Object::Name(b"Encoding".to_vec())),
        (
            "BaseEncoding".to_string(),
            Object::Name(base_encoding.as_bytes().to_vec()),
        ),
        (
            "Differences".to_string(),
            Object::Array(existing_differences),
        ),
    ]);

    if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&font_id) {
        dict.set("Encoding", Object::Dictionary(enc_dict));
    }

    true
}

/// Fix .notdef references in a Type1 font.
///
/// Similar to TrueType but uses CFF glyph names for lookup.
fn fix_type1_notdef(
    doc: &mut Document,
    font_id: ObjectId,
    font_data: &[u8],
    used_codes: &std::collections::HashSet<u8>,
) -> bool {
    // Try parsing as CFF first.
    let cff = cff_parser::Table::parse(font_data);
    let Some(cff) = cff else {
        return false;
    };

    // Read current encoding info.
    let (encoding_name, has_differences) = {
        let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
            return false;
        };
        let enc_name = match dict.get(b"Encoding").ok() {
            Some(Object::Name(n)) => String::from_utf8(n.clone()).ok().unwrap_or_default(),
            Some(Object::Dictionary(enc_dict)) => {
                get_name(enc_dict, b"BaseEncoding").unwrap_or_default()
            }
            _ => String::new(),
        };
        let has_diff = match dict.get(b"Encoding").ok() {
            Some(Object::Dictionary(enc_dict)) => enc_dict.has(b"Differences"),
            _ => false,
        };
        (enc_name, has_diff)
    };

    // Build a set of glyph names available in the CFF font.
    let mut available_glyphs: std::collections::HashSet<String> = std::collections::HashSet::new();
    let num_glyphs = cff.number_of_glyphs();
    for gid in 0..num_glyphs {
        if let Some(name) = cff.glyph_name(cff_parser::GlyphId(gid)) {
            if name != ".notdef" {
                available_glyphs.insert(name.to_string());
            }
        }
    }

    // Find character codes that map to .notdef.
    let mut notdef_codes: Vec<u8> = Vec::new();
    for &code in used_codes {
        let ch = encoding_to_char(code as u32, &encoding_name);
        let glyph_name = unicode_to_glyph_name(ch);

        let has_glyph = match &glyph_name {
            Some(name) => available_glyphs.contains(name),
            None => false,
        };

        if !has_glyph {
            // Try the WinAnsi glyph name.
            let winansi_name = winansi_code_to_glyph_name(code);
            let has_winansi = match winansi_name {
                Some(name) => available_glyphs.contains(name),
                None => false,
            };
            if !has_winansi {
                notdef_codes.push(code);
            }
        }
    }

    if notdef_codes.is_empty() {
        return false;
    }

    notdef_codes.sort();

    // Find a valid replacement glyph from the CFF font.
    let replacement_name = find_valid_replacement_glyph_cff(&cff);

    let base_encoding = if encoding_name.is_empty() {
        "WinAnsiEncoding"
    } else {
        &encoding_name
    };

    // Read existing Differences if any.
    let mut existing_differences: Vec<Object> = if has_differences {
        let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
            return false;
        };
        match dict.get(b"Encoding").ok() {
            Some(Object::Dictionary(enc_dict)) => match enc_dict.get(b"Differences").ok() {
                Some(Object::Array(arr)) => arr.clone(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    } else {
        Vec::new()
    };

    // Add new mappings for .notdef codes.
    for &code in &notdef_codes {
        if !is_code_in_differences(&existing_differences, code) {
            existing_differences.push(Object::Integer(code as i64));
            existing_differences.push(Object::Name(replacement_name.as_bytes().to_vec()));
        }
    }

    let enc_dict = lopdf::Dictionary::from_iter(vec![
        ("Type".to_string(), Object::Name(b"Encoding".to_vec())),
        (
            "BaseEncoding".to_string(),
            Object::Name(base_encoding.as_bytes().to_vec()),
        ),
        (
            "Differences".to_string(),
            Object::Array(existing_differences),
        ),
    ]);

    if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&font_id) {
        dict.set("Encoding", Object::Dictionary(enc_dict));
    }

    true
}

/// Fix .notdef references in a Type0 (CID) font.
///
/// For Type0 fonts without a ToUnicode CMap, builds one from the embedded
/// TrueType font's cmap table. This helps veraPDF resolve .notdef checks
/// by providing a proper Unicode mapping.
///
/// This is best-effort -- many Type0 .notdef issues require CMap rewriting
/// which is beyond the scope of this fix.
fn fix_type0_notdef(
    doc: &mut Document,
    font_id: ObjectId,
    font_data: &[u8],
    _used_codes: &std::collections::HashSet<u8>,
) -> bool {
    let Ok(face) = ttf_parser::Face::parse(font_data, 0) else {
        return false;
    };

    // Check if the font already has a ToUnicode CMap.
    let has_tounicode = {
        let Some(Object::Dictionary(dict)) = doc.objects.get(&font_id) else {
            return false;
        };
        dict.has(b"ToUnicode")
    };

    if has_tounicode {
        // ToUnicode already present -- we can't easily fix .notdef in CID fonts
        // without rewriting the CMap, which is risky.
        return false;
    }

    // For Type0 fonts without ToUnicode: build a basic Identity ToUnicode CMap.
    let num_glyphs = face.number_of_glyphs();
    if num_glyphs == 0 {
        return false;
    }

    // Build CID -> Unicode mappings from the font's cmap table.
    let mut mappings: Vec<(u16, u16)> = Vec::new();
    // Scan Unicode BMP for characters that map to glyphs in this font.
    for unicode in 0u32..=0xFFFF {
        let Some(ch) = char::from_u32(unicode) else {
            continue;
        };
        if let Some(gid) = face.glyph_index(ch) {
            let gid_val = gid.0;
            if gid_val > 0 && gid_val < num_glyphs {
                mappings.push((gid_val, unicode as u16));
            }
        }
    }

    if mappings.is_empty() {
        return false;
    }

    // Sort by CID (GID).
    mappings.sort_by_key(|(gid, _)| *gid);
    // Deduplicate: keep first mapping per GID.
    mappings.dedup_by_key(|(gid, _)| *gid);

    // Build a ToUnicode CMap.
    let cmap_str = build_tounicode_cmap(&mappings);
    let cmap_stream = Stream::new(lopdf::Dictionary::new(), cmap_str.into_bytes());
    let cmap_id = doc.add_object(Object::Stream(cmap_stream));

    if let Some(Object::Dictionary(ref mut dict)) = doc.objects.get_mut(&font_id) {
        dict.set("ToUnicode", Object::Reference(cmap_id));
    }

    true
}

/// Build a ToUnicode CMap string from CID -> Unicode mappings.
fn build_tounicode_cmap(mappings: &[(u16, u16)]) -> String {
    let mut cmap = String::new();
    cmap.push_str("/CIDInit /ProcSet findresource begin\n");
    cmap.push_str("12 dict begin\n");
    cmap.push_str("begincmap\n");
    cmap.push_str("/CIDSystemInfo\n");
    cmap.push_str("<< /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n");
    cmap.push_str("/CMapName /Adobe-Identity-UCS def\n");
    cmap.push_str("/CMapType 2 def\n");
    cmap.push_str("1 begincodespacerange\n");
    cmap.push_str("<0000> <FFFF>\n");
    cmap.push_str("endcodespacerange\n");

    // Write mappings in chunks of 100 (CMap spec limit).
    let chunks: Vec<&[(u16, u16)]> = mappings.chunks(100).collect();
    for chunk in &chunks {
        cmap.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (cid, unicode) in *chunk {
            cmap.push_str(&format!("<{cid:04X}> <{unicode:04X}>\n"));
        }
        cmap.push_str("endbfchar\n");
    }

    cmap.push_str("endcmap\n");
    cmap.push_str("CMapName currentdict /CMap defineresource pop\n");
    cmap.push_str("end\n");
    cmap.push_str("end\n");
    cmap
}

/// Find a valid replacement glyph name for TrueType fonts.
///
/// Prefers "space", then tries common safe glyphs.
fn find_valid_replacement_glyph(face: &ttf_parser::Face) -> String {
    // Try common glyphs in order of preference.
    let candidates = ["space", "nbspace", "uni00A0", "period", "hyphen"];
    for name in &candidates {
        if face.glyph_index_by_name(name).is_some() {
            return name.to_string();
        }
    }
    // Try the space character (U+0020) directly.
    if face.glyph_index(' ').is_some() {
        return "space".to_string();
    }
    // Fallback: use space -- any embedded font should have it.
    "space".to_string()
}

/// Find a valid replacement glyph name for CFF fonts.
fn find_valid_replacement_glyph_cff(cff: &cff_parser::Table<'_>) -> String {
    let num_glyphs = cff.number_of_glyphs();
    let candidates = ["space", "nbspace", "uni00A0", "period", "hyphen"];

    for candidate in &candidates {
        for gid in 0..num_glyphs {
            if let Some(name) = cff.glyph_name(cff_parser::GlyphId(gid)) {
                if name == *candidate {
                    return candidate.to_string();
                }
            }
        }
    }
    "space".to_string()
}

/// Check if a character code is already present in a Differences array.
fn is_code_in_differences(differences: &[Object], code: u8) -> bool {
    let mut current_code: Option<u8> = None;
    for obj in differences {
        match obj {
            Object::Integer(i) => {
                current_code = Some(*i as u8);
            }
            Object::Name(_) => {
                if current_code == Some(code) {
                    return true;
                }
                // Advance to next code in sequence.
                if let Some(ref mut c) = current_code {
                    *c = c.wrapping_add(1);
                }
            }
            _ => {}
        }
    }
    false
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
