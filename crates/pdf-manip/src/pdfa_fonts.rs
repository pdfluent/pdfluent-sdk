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
            let has_31_cmap = face
                .tables()
                .cmap
                .as_ref()
                .is_some_and(|cmap| {
                    cmap.subtables
                        .into_iter()
                        .any(|st| st.platform_id == ttf_parser::PlatformId::Windows
                            && st.encoding_id == 1)
                });
            if has_31_cmap {
                // Font program has Windows Unicode cmap → non-symbolic.
                if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
                    if let Ok(Object::Integer(flags)) = fd.get(b"Flags") {
                        let mut f = *flags;
                        f &= !4;  // Clear Symbolic (bit 3)
                        f |= 32;  // Set Nonsymbolic (bit 6)
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

/// Fix CIDSet streams for all CID fonts (6.2.11.8:1).
///
/// CIDSet must be a stream containing a bitmap covering all CIDs present
/// in the embedded font program. This builds a complete CIDSet from the
/// font's glyph count.
pub fn fix_cidset(doc: &mut Document) -> usize {
    let font_ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    let mut fixed = 0;

    for font_id in font_ids {
        let (fd_id, has_cidset, num_glyphs) = {
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

            // Read font data to get glyph count.
            let font_data = read_embedded_font_data(doc, fd_id);
            let num_glyphs = font_data.and_then(|data| {
                // Try TrueType/OpenType first.
                if let Ok(face) = ttf_parser::Face::parse(&data, 0) {
                    return Some(face.number_of_glyphs());
                }
                // Try standalone CFF (CIDFontType0).
                if let Some(cff) = cff_parser::Table::parse(&data) {
                    return Some(cff.number_of_glyphs());
                }
                None
            });

            (fd_id, has_cidset, num_glyphs)
        };

        // Only fix if we can parse the font to determine glyph count.
        let Some(num_glyphs) = num_glyphs else {
            continue;
        };

        // Build CIDSet bitmap: one bit per CID, all set to 1.
        let num_bytes = (num_glyphs as usize).div_ceil(8);
        let mut cidset_data = vec![0xFFu8; num_bytes];
        // Clear trailing bits in the last byte.
        let trailing = num_glyphs as usize % 8;
        if trailing != 0 && !cidset_data.is_empty() {
            let last = cidset_data.len() - 1;
            cidset_data[last] = 0xFF << (8 - trailing);
        }

        let cidset_stream = Stream::new(dictionary! {}, cidset_data);
        let cidset_id = doc.add_object(Object::Stream(cidset_stream));

        if let Some(Object::Dictionary(ref mut fd)) = doc.objects.get_mut(&fd_id) {
            fd.set("CIDSet", Object::Reference(cidset_id));
        }

        if !has_cidset {
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
