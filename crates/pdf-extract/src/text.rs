//! Text extraction with character-level position tracking.
//!
//! Parses content stream text operators (Tj, TJ, Tm, Td, TD, T*, Tc, Tw, Tz, TL, Ts, ', ")
//! to extract text with positional information.

use crate::error::{ExtractError, Result};
use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, ObjectId};
use std::collections::HashMap;
use std::sync::OnceLock;

/// Approximate character width as a fraction of font size.
const APPROX_CHAR_WIDTH: f64 = 0.5;

/// A block of text extracted from a page.
#[derive(Debug, Clone)]
pub struct TextBlock {
    /// The extracted text content.
    pub text: String,
    /// The page number (1-based).
    pub page: u32,
    /// Bounding box [x0, y0, x1, y1] in PDF coordinates.
    pub bbox: [f64; 4],
    /// Font name used for this text block.
    pub font_name: String,
    /// Font size in points.
    pub font_size: f64,
}

/// A single character with its position on the page.
#[derive(Debug, Clone)]
pub struct PositionedChar {
    /// The character.
    pub ch: char,
    /// The page number (1-based).
    pub page: u32,
    /// Bounding box [x0, y0, x1, y1] in PDF coordinates.
    pub bbox: [f64; 4],
}

/// Internal graphics state for save/restore (q/Q).
#[derive(Debug, Clone)]
struct GraphicsState {
    ctm: [f64; 6],
}

/// Internal text state tracker.
#[derive(Debug, Clone)]
struct TextState {
    /// Text matrix.
    tm: [f64; 6],
    /// Text line matrix.
    tlm: [f64; 6],
    /// Current font name.
    font_name: String,
    /// Current font size.
    font_size: f64,
    /// Character spacing (Tc).
    tc: f64,
    /// Word spacing (Tw).
    tw: f64,
    /// Horizontal scaling (Tz), as a percentage.
    th: f64,
    /// Text leading (TL).
    tl: f64,
    /// Text rise (Ts).
    ts: f64,
    /// Graphics state stack.
    gs_stack: Vec<GraphicsState>,
    /// Current transformation matrix.
    ctm: [f64; 6],
}

impl Default for TextState {
    fn default() -> Self {
        Self {
            tm: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            tlm: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            font_name: String::new(),
            font_size: 12.0,
            tc: 0.0,
            tw: 0.0,
            th: 100.0,
            tl: 0.0,
            ts: 0.0,
            gs_stack: Vec::new(),
            ctm: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        }
    }
}

/// Per-font information for text decoding.
#[derive(Clone)]
struct FontInfo {
    /// True if the font uses 2-byte CID encoding (Identity-H/V or other CID CMaps).
    is_cid: bool,
    /// ToUnicode CMap: maps character code(s) to Unicode string.
    /// For CID fonts the key is a 2-byte big-endian value; for simple fonts it's a 1-byte code.
    to_unicode: HashMap<u32, String>,
    /// Encoding-based code→char map for simple fonts (derived from BaseEncoding + Differences).
    /// Index = byte code (0..255), value = Unicode char if known.
    encoding_map: [Option<char>; 256],
}

/// Build a map from font resource name (e.g. "F1") to FontInfo for a page.
fn build_font_map(doc: &Document, page_id: ObjectId) -> HashMap<String, FontInfo> {
    let mut map = HashMap::new();

    // Get the page's Resources dictionary (may be inherited from parent Pages node).
    let resources = get_page_resources(doc, page_id);
    let font_dict = match resources.and_then(|res| match res.get(b"Font").ok()? {
        Object::Dictionary(d) => Some(d.clone()),
        Object::Reference(r) => match doc.get_object(*r).ok()? {
            Object::Dictionary(d) => Some(d.clone()),
            _ => None,
        },
        _ => None,
    }) {
        Some(d) => d,
        None => return map,
    };

    for (name_bytes, value) in font_dict.iter() {
        let font_name = String::from_utf8_lossy(name_bytes).to_string();

        // Resolve the font dictionary.
        let font = match value {
            Object::Reference(r) => match doc.get_object(*r).ok() {
                Some(Object::Dictionary(d)) => d.clone(),
                _ => continue,
            },
            Object::Dictionary(d) => d.clone(),
            _ => continue,
        };

        let subtype = font
            .get(b"Subtype")
            .ok()
            .and_then(|o| match o {
                Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
                _ => None,
            })
            .unwrap_or_default();

        let is_cid = subtype == "Type0";

        // Parse ToUnicode CMap if present.
        let to_unicode = parse_to_unicode_from_font(doc, &font);

        // For Type0 fonts, also check DescendantFonts for ToUnicode.
        let to_unicode = if to_unicode.is_empty() && is_cid {
            if let Some(Object::Array(descendants)) = font.get(b"DescendantFonts").ok() {
                descendants
                    .iter()
                    .find_map(|d| {
                        let desc_dict = match d {
                            Object::Reference(r) => match doc.get_object(*r).ok()? {
                                Object::Dictionary(d) => d,
                                _ => return None,
                            },
                            Object::Dictionary(d) => d,
                            _ => return None,
                        };
                        let tu = parse_to_unicode_from_font(doc, desc_dict);
                        if tu.is_empty() {
                            None
                        } else {
                            Some(tu)
                        }
                    })
                    .unwrap_or_default()
            } else {
                HashMap::new()
            }
        } else {
            to_unicode
        };

        // Build encoding map for simple fonts (from Encoding + Differences).
        let encoding_map = if !is_cid {
            build_encoding_map(doc, &font)
        } else {
            [None; 256]
        };

        map.insert(
            font_name,
            FontInfo {
                is_cid,
                to_unicode,
                encoding_map,
            },
        );
    }

    map
}

/// Parse the ToUnicode CMap from a font dictionary.
fn parse_to_unicode_from_font(doc: &Document, font: &lopdf::Dictionary) -> HashMap<u32, String> {
    let tu_obj = match font.get(b"ToUnicode").ok() {
        Some(Object::Reference(r)) => doc.get_object(*r).ok(),
        Some(obj) => Some(obj),
        None => return HashMap::new(),
    };

    let stream_bytes = match tu_obj {
        Some(Object::Stream(ref s)) => s
            .decompressed_content()
            .ok()
            .unwrap_or_else(|| s.content.clone()),
        _ => return HashMap::new(),
    };

    parse_to_unicode_cmap(&stream_bytes)
}

/// Parse a ToUnicode CMap stream into a code→Unicode mapping.
///
/// Handles both `beginbfchar` and `beginbfrange` sections.
fn parse_to_unicode_cmap(data: &[u8]) -> HashMap<u32, String> {
    let text = String::from_utf8_lossy(data);
    let mut map = HashMap::new();

    // Parse beginbfchar sections: <srcCode> <dstString>
    for section in text.split("beginbfchar") {
        let section = match section.split("endbfchar").next() {
            Some(s) => s,
            None => continue,
        };
        let tokens = extract_hex_tokens(section);
        for pair in tokens.chunks(2) {
            if pair.len() == 2 {
                let code = parse_hex_u32(&pair[0]);
                let unicode = hex_to_unicode_string(&pair[1]);
                map.insert(code, unicode);
            }
        }
    }

    // Parse beginbfrange sections: <srcLo> <srcHi> <dstStart> or <srcLo> <srcHi> [<dst1> <dst2> ...]
    for section in text.split("beginbfrange") {
        let section = match section.split("endbfrange").next() {
            Some(s) => s,
            None => continue,
        };

        // Tokenize: extract hex tokens and array brackets
        let mut chars = section.chars().peekable();
        let mut tokens: Vec<String> = Vec::new();
        let mut arrays: Vec<Vec<String>> = Vec::new();
        let mut in_array = false;
        let mut current_array: Vec<String> = Vec::new();

        while let Some(&ch) = chars.peek() {
            if ch == '<' {
                chars.next();
                let hex: String = chars
                    .by_ref()
                    .take_while(|&c| c != '>')
                    .filter(|c| !c.is_whitespace())
                    .collect();
                if in_array {
                    current_array.push(hex);
                } else {
                    tokens.push(hex);
                }
            } else if ch == '[' {
                chars.next();
                in_array = true;
                current_array = Vec::new();
            } else if ch == ']' {
                chars.next();
                in_array = false;
                arrays.push(std::mem::take(&mut current_array));
                tokens.push(String::new()); // placeholder for array position
            } else {
                chars.next();
            }
        }

        // Process range entries: every 3 tokens = (lo, hi, dst_or_array)
        let mut array_idx = 0;
        let mut i = 0;
        while i + 2 < tokens.len() {
            let lo = parse_hex_u32(&tokens[i]);
            let hi = parse_hex_u32(&tokens[i + 1]);

            if tokens[i + 2].is_empty() {
                // Array destination
                if array_idx < arrays.len() {
                    let arr = &arrays[array_idx];
                    for (offset, dst) in arr.iter().enumerate() {
                        let code = lo + offset as u32;
                        if code <= hi {
                            map.insert(code, hex_to_unicode_string(dst));
                        }
                    }
                    array_idx += 1;
                }
            } else {
                // Single start value — increment for each code in range
                let dst_start = parse_hex_u32(&tokens[i + 2]);
                let dst_len = tokens[i + 2].len();
                for code in lo..=hi {
                    let dst_val = dst_start + (code - lo);
                    let s = if dst_len <= 4 {
                        // BMP character
                        char::from_u32(dst_val)
                            .map(|c| c.to_string())
                            .unwrap_or_default()
                    } else {
                        // Multi-byte: treat as UTF-16BE pairs
                        let hex = format!("{:0>width$X}", dst_val, width = dst_len);
                        hex_to_unicode_string(&hex)
                    };
                    map.insert(code, s);
                }
            }
            i += 3;
        }
    }

    map
}

/// Extract hex tokens (contents between < and >) from text.
fn extract_hex_tokens(text: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut in_hex = false;
    let mut current = String::new();
    for ch in text.chars() {
        if ch == '<' {
            in_hex = true;
            current.clear();
        } else if ch == '>' && in_hex {
            in_hex = false;
            tokens.push(current.clone());
        } else if in_hex && !ch.is_whitespace() {
            current.push(ch);
        }
    }
    tokens
}

/// Parse a hex string to a u32 (e.g., "0041" → 65).
fn parse_hex_u32(hex: &str) -> u32 {
    u32::from_str_radix(hex, 16).unwrap_or(0)
}

/// Convert a hex string to a Unicode string (interpreting as UTF-16BE pairs).
fn hex_to_unicode_string(hex: &str) -> String {
    let bytes: Vec<u8> = (0..hex.len())
        .step_by(2)
        .filter_map(|i| u8::from_str_radix(&hex[i..i + 2.min(hex.len() - i)], 16).ok())
        .collect();

    if bytes.len() >= 2 && bytes.len() % 2 == 0 {
        // Interpret as UTF-16BE
        let u16s: Vec<u16> = bytes
            .chunks(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&u16s)
    } else if bytes.len() == 1 {
        char::from_u32(bytes[0] as u32)
            .map(|c| c.to_string())
            .unwrap_or_default()
    } else {
        String::new()
    }
}

/// Get the Resources dictionary for a page (resolves inheritance from parent Pages).
fn get_page_resources(doc: &Document, page_id: ObjectId) -> Option<lopdf::Dictionary> {
    let page = match doc.get_object(page_id).ok()? {
        Object::Dictionary(d) => d.clone(),
        _ => return None,
    };

    // Direct Resources on the page.
    if let Some(res) = resolve_dict(doc, &page, b"Resources") {
        return Some(res);
    }

    // Walk up the parent chain (Pages nodes) for inherited Resources.
    let mut current = page;
    for _ in 0..20 {
        let parent_ref = match current.get(b"Parent").ok()? {
            Object::Reference(r) => *r,
            _ => break,
        };
        let parent = match doc.get_object(parent_ref).ok()? {
            Object::Dictionary(d) => d.clone(),
            _ => break,
        };
        if let Some(res) = resolve_dict(doc, &parent, b"Resources") {
            return Some(res);
        }
        current = parent;
    }

    None
}

/// Resolve a dictionary entry that may be inline or an indirect reference.
fn resolve_dict(doc: &Document, dict: &lopdf::Dictionary, key: &[u8]) -> Option<lopdf::Dictionary> {
    match dict.get(key).ok()? {
        Object::Dictionary(d) => Some(d.clone()),
        Object::Reference(r) => match doc.get_object(*r).ok()? {
            Object::Dictionary(d) => Some(d.clone()),
            _ => None,
        },
        _ => None,
    }
}

/// Build a 256-entry encoding map from a font's /Encoding dictionary.
///
/// Handles:
/// - Named base encodings: WinAnsiEncoding, MacRomanEncoding, MacExpertEncoding
/// - Differences arrays: `[code1 /name1 /name2 ... codeN /nameN ...]`
/// - Glyph name → Unicode via AGL (Adobe Glyph List) lookup
fn build_encoding_map(doc: &Document, font: &lopdf::Dictionary) -> [Option<char>; 256] {
    let mut table = [None::<char>; 256];

    let encoding = match font.get(b"Encoding").ok() {
        Some(obj) => obj,
        None => return table,
    };

    match encoding {
        Object::Name(name) => {
            // Named encoding (e.g. "WinAnsiEncoding").
            let name_str = String::from_utf8_lossy(name);
            apply_base_encoding(&mut table, &name_str);
        }
        Object::Reference(r) => {
            if let Some(Object::Dictionary(enc_dict)) = doc.get_object(*r).ok() {
                parse_encoding_dict(doc, enc_dict, &mut table);
            } else if let Some(Object::Name(name)) = doc.get_object(*r).ok() {
                let name_str = String::from_utf8_lossy(name);
                apply_base_encoding(&mut table, &name_str);
            }
        }
        Object::Dictionary(enc_dict) => {
            parse_encoding_dict(doc, enc_dict, &mut table);
        }
        _ => {}
    }

    table
}

/// Parse an Encoding dictionary with optional BaseEncoding and Differences.
fn parse_encoding_dict(
    doc: &Document,
    enc_dict: &lopdf::Dictionary,
    table: &mut [Option<char>; 256],
) {
    // Apply BaseEncoding first.
    if let Some(Object::Name(base)) = enc_dict.get(b"BaseEncoding").ok() {
        let base_str = String::from_utf8_lossy(base);
        apply_base_encoding(table, &base_str);
    }

    // Apply Differences array: [code /name1 /name2 ... code /name3 ...]
    let diffs = match enc_dict.get(b"Differences").ok() {
        Some(Object::Array(arr)) => arr.clone(),
        Some(Object::Reference(r)) => match doc.get_object(*r).ok() {
            Some(Object::Array(arr)) => arr.clone(),
            _ => return,
        },
        _ => return,
    };

    let mut code: Option<u32> = None;
    for item in &diffs {
        match item {
            Object::Integer(n) => {
                code = Some(*n as u32);
            }
            Object::Name(name) => {
                if let Some(c) = code {
                    if c < 256 {
                        let glyph = String::from_utf8_lossy(name);
                        if let Some(ch) = glyph_name_to_unicode(&glyph) {
                            table[c as usize] = Some(ch);
                        }
                    }
                    code = Some(c + 1);
                }
            }
            Object::Reference(r) => {
                // Indirect name reference (rare).
                if let Some(Object::Name(name)) = doc.get_object(*r).ok() {
                    if let Some(c) = code {
                        if c < 256 {
                            let glyph = String::from_utf8_lossy(name);
                            if let Some(ch) = glyph_name_to_unicode(&glyph) {
                                table[c as usize] = Some(ch);
                            }
                        }
                        code = Some(c + 1);
                    }
                }
            }
            _ => {}
        }
    }
}

/// Apply a named base encoding to the table.
fn apply_base_encoding(table: &mut [Option<char>; 256], name: &str) {
    let source = match name {
        "WinAnsiEncoding" => winansi_encoding(),
        "MacRomanEncoding" => mac_roman_encoding(),
        _ => return,
    };
    for (i, &ch) in source.iter().enumerate() {
        if ch != '\0' {
            table[i] = Some(ch);
        }
    }
}

/// WinAnsiEncoding table (cp1252).
fn winansi_encoding() -> &'static [char; 256] {
    static TABLE: OnceLock<[char; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t = ['\0'; 256];
        // ASCII range is identity.
        for i in 0x20..=0x7Eu8 {
            t[i as usize] = i as char;
        }
        // Control chars mapped to common usage.
        t[0x09] = '\t';
        t[0x0A] = '\n';
        t[0x0D] = '\r';
        // cp1252 upper range (0x80-0xFF).
        let cp1252: [(u8, char); 27] = [
            (0x80, '\u{20AC}'), // Euro sign
            (0x82, '\u{201A}'), // Single low-9 quotation mark
            (0x83, '\u{0192}'), // Latin small letter f with hook
            (0x84, '\u{201E}'), // Double low-9 quotation mark
            (0x85, '\u{2026}'), // Horizontal ellipsis
            (0x86, '\u{2020}'), // Dagger
            (0x87, '\u{2021}'), // Double dagger
            (0x88, '\u{02C6}'), // Modifier letter circumflex accent
            (0x89, '\u{2030}'), // Per mille sign
            (0x8A, '\u{0160}'), // Latin capital letter S with caron
            (0x8B, '\u{2039}'), // Single left-pointing angle quotation mark
            (0x8C, '\u{0152}'), // Latin capital ligature OE
            (0x8E, '\u{017D}'), // Latin capital letter Z with caron
            (0x91, '\u{2018}'), // Left single quotation mark
            (0x92, '\u{2019}'), // Right single quotation mark
            (0x93, '\u{201C}'), // Left double quotation mark
            (0x94, '\u{201D}'), // Right double quotation mark
            (0x95, '\u{2022}'), // Bullet
            (0x96, '\u{2013}'), // En dash
            (0x97, '\u{2014}'), // Em dash
            (0x98, '\u{02DC}'), // Small tilde
            (0x99, '\u{2122}'), // Trade mark sign
            (0x9A, '\u{0161}'), // Latin small letter s with caron
            (0x9B, '\u{203A}'), // Single right-pointing angle quotation mark
            (0x9C, '\u{0153}'), // Latin small ligature oe
            (0x9E, '\u{017E}'), // Latin small letter z with caron
            (0x9F, '\u{0178}'), // Latin capital letter Y with diaeresis
        ];
        for (code, ch) in cp1252 {
            t[code as usize] = ch;
        }
        // 0xA0-0xFF: same as Unicode Latin-1 supplement.
        for i in 0xA0..=0xFFu16 {
            t[i as usize] = char::from_u32(i as u32).unwrap_or('\0');
        }
        t
    })
}

/// MacRomanEncoding table.
fn mac_roman_encoding() -> &'static [char; 256] {
    static TABLE: OnceLock<[char; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t = ['\0'; 256];
        // ASCII range is identity.
        for i in 0x20..=0x7Eu8 {
            t[i as usize] = i as char;
        }
        t[0x09] = '\t';
        t[0x0A] = '\n';
        t[0x0D] = '\r';
        // Mac Roman 0x80-0xFF mapping to Unicode.
        let mac_upper: [char; 128] = [
            '\u{00C4}', '\u{00C5}', '\u{00C7}', '\u{00C9}', '\u{00D1}', '\u{00D6}', '\u{00DC}',
            '\u{00E1}', '\u{00E0}', '\u{00E2}', '\u{00E4}', '\u{00E3}', '\u{00E5}', '\u{00E7}',
            '\u{00E9}', '\u{00E8}', '\u{00EA}', '\u{00EB}', '\u{00ED}', '\u{00EC}', '\u{00EE}',
            '\u{00EF}', '\u{00F1}', '\u{00F3}', '\u{00F2}', '\u{00F4}', '\u{00F6}', '\u{00F5}',
            '\u{00FA}', '\u{00F9}', '\u{00FB}', '\u{00FC}', '\u{2020}', '\u{00B0}', '\u{00A2}',
            '\u{00A3}', '\u{00A7}', '\u{2022}', '\u{00B6}', '\u{00DF}', '\u{00AE}', '\u{00A9}',
            '\u{2122}', '\u{00B4}', '\u{00A8}', '\u{2260}', '\u{00C6}', '\u{00D8}', '\u{221E}',
            '\u{00B1}', '\u{2264}', '\u{2265}', '\u{00A5}', '\u{00B5}', '\u{2202}', '\u{2211}',
            '\u{220F}', '\u{03C0}', '\u{222B}', '\u{00AA}', '\u{00BA}', '\u{03A9}', '\u{00E6}',
            '\u{00F8}', '\u{00BF}', '\u{00A1}', '\u{00AC}', '\u{221A}', '\u{0192}', '\u{2248}',
            '\u{2206}', '\u{00AB}', '\u{00BB}', '\u{2026}', '\u{00A0}', '\u{00C0}', '\u{00C3}',
            '\u{00D5}', '\u{0152}', '\u{0153}', '\u{2013}', '\u{2014}', '\u{201C}', '\u{201D}',
            '\u{2018}', '\u{2019}', '\u{00F7}', '\u{25CA}', '\u{00FF}', '\u{0178}', '\u{2044}',
            '\u{20AC}', '\u{2039}', '\u{203A}', '\u{FB01}', '\u{FB02}', '\u{2021}', '\u{00B7}',
            '\u{201A}', '\u{201E}', '\u{2030}', '\u{00C2}', '\u{00CA}', '\u{00C1}', '\u{00CB}',
            '\u{00C8}', '\u{00CD}', '\u{00CE}', '\u{00CF}', '\u{00CC}', '\u{00D3}', '\u{00D4}',
            '\u{F8FF}', '\u{00D2}', '\u{00DA}', '\u{00DB}', '\u{00D9}', '\u{0131}', '\u{02C6}',
            '\u{02DC}', '\u{00AF}', '\u{02D8}', '\u{02D9}', '\u{02DA}', '\u{00B8}', '\u{02DD}',
            '\u{02DB}', '\u{02C7}',
        ];
        for (i, &ch) in mac_upper.iter().enumerate() {
            t[0x80 + i] = ch;
        }
        t
    })
}

/// Map an Adobe Glyph List (AGL) name to a Unicode character.
///
/// Handles:
/// - `uniXXXX` names (4+ hex digits after "uni")
/// - `uXXXX` / `uXXXXX` names
/// - Common AGL names (top ~250 entries covering >99% of real-world PDFs)
fn glyph_name_to_unicode(name: &str) -> Option<char> {
    // Handle uniXXXX / uXXXXX patterns.
    if name.starts_with("uni") && name.len() >= 7 {
        return u32::from_str_radix(&name[3..7], 16)
            .ok()
            .and_then(char::from_u32);
    }
    if name.starts_with('u') && name.len() >= 5 && name[1..].chars().all(|c| c.is_ascii_hexdigit())
    {
        return u32::from_str_radix(&name[1..], 16)
            .ok()
            .and_then(char::from_u32);
    }

    // Look up in the built-in AGL table.
    agl_table().get(name).copied()
}

/// Built-in Adobe Glyph List table (most common ~250 entries).
fn agl_table() -> &'static HashMap<&'static str, char> {
    static TABLE: OnceLock<HashMap<&'static str, char>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let entries: &[(&str, char)] = &[
            ("space", ' '),
            ("exclam", '!'),
            ("quotedbl", '"'),
            ("numbersign", '#'),
            ("dollar", '$'),
            ("percent", '%'),
            ("ampersand", '&'),
            ("quotesingle", '\''),
            ("parenleft", '('),
            ("parenright", ')'),
            ("asterisk", '*'),
            ("plus", '+'),
            ("comma", ','),
            ("hyphen", '-'),
            ("period", '.'),
            ("slash", '/'),
            ("zero", '0'),
            ("one", '1'),
            ("two", '2'),
            ("three", '3'),
            ("four", '4'),
            ("five", '5'),
            ("six", '6'),
            ("seven", '7'),
            ("eight", '8'),
            ("nine", '9'),
            ("colon", ':'),
            ("semicolon", ';'),
            ("less", '<'),
            ("equal", '='),
            ("greater", '>'),
            ("question", '?'),
            ("at", '@'),
            ("A", 'A'),
            ("B", 'B'),
            ("C", 'C'),
            ("D", 'D'),
            ("E", 'E'),
            ("F", 'F'),
            ("G", 'G'),
            ("H", 'H'),
            ("I", 'I'),
            ("J", 'J'),
            ("K", 'K'),
            ("L", 'L'),
            ("M", 'M'),
            ("N", 'N'),
            ("O", 'O'),
            ("P", 'P'),
            ("Q", 'Q'),
            ("R", 'R'),
            ("S", 'S'),
            ("T", 'T'),
            ("U", 'U'),
            ("V", 'V'),
            ("W", 'W'),
            ("X", 'X'),
            ("Y", 'Y'),
            ("Z", 'Z'),
            ("bracketleft", '['),
            ("backslash", '\\'),
            ("bracketright", ']'),
            ("asciicircum", '^'),
            ("underscore", '_'),
            ("grave", '`'),
            ("a", 'a'),
            ("b", 'b'),
            ("c", 'c'),
            ("d", 'd'),
            ("e", 'e'),
            ("f", 'f'),
            ("g", 'g'),
            ("h", 'h'),
            ("i", 'i'),
            ("j", 'j'),
            ("k", 'k'),
            ("l", 'l'),
            ("m", 'm'),
            ("n", 'n'),
            ("o", 'o'),
            ("p", 'p'),
            ("q", 'q'),
            ("r", 'r'),
            ("s", 's'),
            ("t", 't'),
            ("u", 'u'),
            ("v", 'v'),
            ("w", 'w'),
            ("x", 'x'),
            ("y", 'y'),
            ("z", 'z'),
            ("braceleft", '{'),
            ("bar", '|'),
            ("braceright", '}'),
            ("asciitilde", '~'),
            // Latin extended
            ("Agrave", '\u{00C0}'),
            ("Aacute", '\u{00C1}'),
            ("Acircumflex", '\u{00C2}'),
            ("Atilde", '\u{00C3}'),
            ("Adieresis", '\u{00C4}'),
            ("Aring", '\u{00C5}'),
            ("AE", '\u{00C6}'),
            ("Ccedilla", '\u{00C7}'),
            ("Egrave", '\u{00C8}'),
            ("Eacute", '\u{00C9}'),
            ("Ecircumflex", '\u{00CA}'),
            ("Edieresis", '\u{00CB}'),
            ("Igrave", '\u{00CC}'),
            ("Iacute", '\u{00CD}'),
            ("Icircumflex", '\u{00CE}'),
            ("Idieresis", '\u{00CF}'),
            ("Eth", '\u{00D0}'),
            ("Ntilde", '\u{00D1}'),
            ("Ograve", '\u{00D2}'),
            ("Oacute", '\u{00D3}'),
            ("Ocircumflex", '\u{00D4}'),
            ("Otilde", '\u{00D5}'),
            ("Odieresis", '\u{00D6}'),
            ("Ugrave", '\u{00D9}'),
            ("Uacute", '\u{00DA}'),
            ("Ucircumflex", '\u{00DB}'),
            ("Udieresis", '\u{00DC}'),
            ("Yacute", '\u{00DD}'),
            ("Thorn", '\u{00DE}'),
            ("germandbls", '\u{00DF}'),
            ("agrave", '\u{00E0}'),
            ("aacute", '\u{00E1}'),
            ("acircumflex", '\u{00E2}'),
            ("atilde", '\u{00E3}'),
            ("adieresis", '\u{00E4}'),
            ("aring", '\u{00E5}'),
            ("ae", '\u{00E6}'),
            ("ccedilla", '\u{00E7}'),
            ("egrave", '\u{00E8}'),
            ("eacute", '\u{00E9}'),
            ("ecircumflex", '\u{00EA}'),
            ("edieresis", '\u{00EB}'),
            ("igrave", '\u{00EC}'),
            ("iacute", '\u{00ED}'),
            ("icircumflex", '\u{00EE}'),
            ("idieresis", '\u{00EF}'),
            ("eth", '\u{00F0}'),
            ("ntilde", '\u{00F1}'),
            ("ograve", '\u{00F2}'),
            ("oacute", '\u{00F3}'),
            ("ocircumflex", '\u{00F4}'),
            ("otilde", '\u{00F5}'),
            ("odieresis", '\u{00F6}'),
            ("ugrave", '\u{00F9}'),
            ("uacute", '\u{00FA}'),
            ("ucircumflex", '\u{00FB}'),
            ("udieresis", '\u{00FC}'),
            ("yacute", '\u{00FD}'),
            ("thorn", '\u{00FE}'),
            ("ydieresis", '\u{00FF}'),
            // Ligatures and special
            ("fi", '\u{FB01}'),
            ("fl", '\u{FB02}'),
            ("ff", '\u{FB00}'),
            ("ffi", '\u{FB03}'),
            ("ffl", '\u{FB04}'),
            // Punctuation and symbols
            ("endash", '\u{2013}'),
            ("emdash", '\u{2014}'),
            ("bullet", '\u{2022}'),
            ("ellipsis", '\u{2026}'),
            ("quoteleft", '\u{2018}'),
            ("quoteright", '\u{2019}'),
            ("quotedblleft", '\u{201C}'),
            ("quotedblright", '\u{201D}'),
            ("quotesinglebase", '\u{201A}'),
            ("quotesinglbase", '\u{201A}'),
            ("quotedblbase", '\u{201E}'),
            ("dagger", '\u{2020}'),
            ("daggerdbl", '\u{2021}'),
            ("perthousand", '\u{2030}'),
            ("guilsinglleft", '\u{2039}'),
            ("guilsinglright", '\u{203A}'),
            ("guillemotleft", '\u{00AB}'),
            ("guillemotright", '\u{00BB}'),
            ("trademark", '\u{2122}'),
            ("copyright", '\u{00A9}'),
            ("registered", '\u{00AE}'),
            ("degree", '\u{00B0}'),
            ("plusminus", '\u{00B1}'),
            ("multiply", '\u{00D7}'),
            ("divide", '\u{00F7}'),
            ("fraction", '\u{2044}'),
            ("Euro", '\u{20AC}'),
            ("sterling", '\u{00A3}'),
            ("yen", '\u{00A5}'),
            ("cent", '\u{00A2}'),
            ("currency", '\u{00A4}'),
            ("section", '\u{00A7}'),
            ("paragraph", '\u{00B6}'),
            ("brokenbar", '\u{00A6}'),
            ("ordfeminine", '\u{00AA}'),
            ("ordmasculine", '\u{00BA}'),
            ("exclamdown", '\u{00A1}'),
            ("questiondown", '\u{00BF}'),
            ("logicalnot", '\u{00AC}'),
            ("mu", '\u{00B5}'),
            ("macron", '\u{00AF}'),
            ("acute", '\u{00B4}'),
            ("cedilla", '\u{00B8}'),
            ("dieresis", '\u{00A8}'),
            ("circumflex", '\u{02C6}'),
            ("tilde", '\u{02DC}'),
            ("caron", '\u{02C7}'),
            ("ring", '\u{02DA}'),
            ("breve", '\u{02D8}'),
            ("dotaccent", '\u{02D9}'),
            ("hungarumlaut", '\u{02DD}'),
            ("ogonek", '\u{02DB}'),
            ("nbspace", '\u{00A0}'),
            ("nonbreakingspace", '\u{00A0}'),
            ("softhyphen", '\u{00AD}'),
            ("periodcentered", '\u{00B7}'),
            ("middot", '\u{00B7}'),
            ("florin", '\u{0192}'),
            ("OE", '\u{0152}'),
            ("oe", '\u{0153}'),
            ("Scaron", '\u{0160}'),
            ("scaron", '\u{0161}'),
            ("Zcaron", '\u{017D}'),
            ("zcaron", '\u{017E}'),
            ("Ydieresis", '\u{0178}'),
            ("Lslash", '\u{0141}'),
            ("lslash", '\u{0142}'),
            ("Oslash", '\u{00D8}'),
            ("oslash", '\u{00F8}'),
            ("dotlessi", '\u{0131}'),
            // Superscripts / subscripts
            ("onesuperior", '\u{00B9}'),
            ("twosuperior", '\u{00B2}'),
            ("threesuperior", '\u{00B3}'),
            ("onequarter", '\u{00BC}'),
            ("onehalf", '\u{00BD}'),
            ("threequarters", '\u{00BE}'),
            // Math
            ("minus", '\u{2212}'),
            ("notequal", '\u{2260}'),
            ("lessequal", '\u{2264}'),
            ("greaterequal", '\u{2265}'),
            ("infinity", '\u{221E}'),
            ("partialdiff", '\u{2202}'),
            ("summation", '\u{2211}'),
            ("product", '\u{220F}'),
            ("integral", '\u{222B}'),
            ("radical", '\u{221A}'),
            ("approxequal", '\u{2248}'),
            ("Delta", '\u{0394}'),
            ("lozenge", '\u{25CA}'),
            ("pi", '\u{03C0}'),
            ("Omega", '\u{03A9}'),
        ];
        entries.iter().cloned().collect()
    })
}

/// Decode a PDF string using the font's ToUnicode CMap (if available).
fn decode_pdf_string_with_font(bytes: &[u8], font_info: Option<&FontInfo>) -> String {
    // Check for UTF-16BE BOM first — always takes priority.
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let chars: Vec<u16> = bytes[2..]
            .chunks(2)
            .filter_map(|chunk| {
                if chunk.len() == 2 {
                    Some(u16::from_be_bytes([chunk[0], chunk[1]]))
                } else {
                    None
                }
            })
            .collect();
        return String::from_utf16_lossy(&chars);
    }

    if let Some(info) = font_info {
        if info.is_cid && !info.to_unicode.is_empty() {
            // CID font: decode 2-byte codes via ToUnicode.
            let mut result = String::new();
            let mut i = 0;
            while i + 1 < bytes.len() {
                let code = u16::from_be_bytes([bytes[i], bytes[i + 1]]) as u32;
                if let Some(s) = info.to_unicode.get(&code) {
                    result.push_str(s);
                } else {
                    // Fallback: try direct Unicode interpretation.
                    if let Some(ch) = char::from_u32(code) {
                        if !ch.is_control() || ch == ' ' || ch == '\t' || ch == '\n' {
                            result.push(ch);
                        }
                    }
                }
                i += 2;
            }
            return result;
        }

        if !info.is_cid && !info.to_unicode.is_empty() {
            // Simple font with ToUnicode: decode 1-byte codes.
            let mut result = String::new();
            for &b in bytes {
                if let Some(s) = info.to_unicode.get(&(b as u32)) {
                    result.push_str(s);
                } else if let Some(ch) = info.encoding_map[b as usize] {
                    result.push(ch);
                } else {
                    let ch = b as char;
                    if is_printable_or_space(ch) {
                        result.push(ch);
                    }
                }
            }
            return result;
        }

        // Simple font with encoding map but no ToUnicode.
        if !info.is_cid && info.encoding_map.iter().any(|c| c.is_some()) {
            let mut result = String::new();
            for &b in bytes {
                if let Some(ch) = info.encoding_map[b as usize] {
                    result.push(ch);
                } else {
                    let ch = b as char;
                    if is_printable_or_space(ch) {
                        result.push(ch);
                    }
                }
            }
            return result;
        }
    }

    // Fallback: PDFDocEncoding (ASCII + Latin-1), skipping control chars.
    bytes
        .iter()
        .filter_map(|&b| {
            let ch = b as char;
            if is_printable_or_space(ch) {
                Some(ch)
            } else {
                None
            }
        })
        .collect()
}

/// Returns true if a character is printable or common whitespace (tab, newline, CR, space).
/// Filters out NUL, BEL, and other control characters that corrupt XML output.
fn is_printable_or_space(ch: char) -> bool {
    let cp = ch as u32;
    cp >= 0x20 || cp == 0x09 || cp == 0x0A || cp == 0x0D
}

/// Extract text blocks from a specific page.
pub fn extract_page_blocks(doc: &Document, page_num: u32) -> Vec<TextBlock> {
    let pages = doc.get_pages();
    let Some(&page_id) = pages.get(&page_num) else {
        return Vec::new();
    };

    let font_map = build_font_map(doc, page_id);
    let resources = get_page_resources(doc, page_id).unwrap_or_default();
    if let Ok(content_bytes) = get_page_content_bytes(doc, page_id) {
        if let Ok(content) = Content::decode(&content_bytes) {
            return extract_blocks_from_ops_inner(
                &content.operations,
                page_num,
                &font_map,
                Some((doc, &resources)),
                0,
            );
        }
    }

    Vec::new()
}

/// Extract text blocks from a page identified by its object ID.
///
/// Unlike [`extract_page_blocks`], this function does not call `get_pages()`
/// internally. Use this when the caller already holds a page map from a
/// prior `doc.get_pages()` call to avoid redundant page-tree traversals.
pub fn extract_blocks_from_page_id(
    doc: &Document,
    page_id: lopdf::ObjectId,
    page_num: u32,
) -> Vec<TextBlock> {
    let font_map = build_font_map(doc, page_id);
    let resources = get_page_resources(doc, page_id).unwrap_or_default();
    if let Ok(content_bytes) = get_page_content_bytes(doc, page_id) {
        if let Ok(content) = Content::decode(&content_bytes) {
            return extract_blocks_from_ops_inner(
                &content.operations,
                page_num,
                &font_map,
                Some((doc, &resources)),
                0,
            );
        }
    }
    Vec::new()
}

/// Extract text blocks from all pages of a document.
pub fn extract_text(doc: &Document) -> Vec<TextBlock> {
    let pages = doc.get_pages();
    let mut blocks = Vec::new();

    for (&page_num, &page_id) in &pages {
        let font_map = build_font_map(doc, page_id);
        let resources = get_page_resources(doc, page_id).unwrap_or_default();
        if let Ok(content_bytes) = get_page_content_bytes(doc, page_id) {
            if let Ok(content) = Content::decode(&content_bytes) {
                let page_blocks = extract_blocks_from_ops_inner(
                    &content.operations,
                    page_num,
                    &font_map,
                    Some((doc, &resources)),
                    0,
                );
                blocks.extend(page_blocks);
            }
        }
    }

    blocks
}

/// Extract text from a specific page as a plain string.
pub fn extract_page_text(doc: &Document, page_num: u32) -> Result<String> {
    let pages = doc.get_pages();
    let total = pages.len() as u32;

    if page_num == 0 || page_num > total {
        return Err(ExtractError::PageOutOfRange(page_num, total));
    }

    let page_id = *pages
        .get(&page_num)
        .ok_or(ExtractError::PageOutOfRange(page_num, total))?;

    let font_map = build_font_map(doc, page_id);
    let resources = get_page_resources(doc, page_id).unwrap_or_default();
    let content_bytes = get_page_content_bytes(doc, page_id).unwrap_or_default();
    let content = match Content::decode(&content_bytes) {
        Ok(c) => c,
        Err(_) => return Ok(String::new()),
    };

    let blocks = extract_blocks_from_ops_inner(
        &content.operations,
        page_num,
        &font_map,
        Some((doc, &resources)),
        0,
    );
    let text = blocks
        .iter()
        .map(|b| b.text.as_str())
        .collect::<Vec<_>>()
        .join("");

    Ok(text)
}

/// Extract positioned characters from a specific page.
pub fn extract_positioned_chars(doc: &Document, page_num: u32) -> Result<Vec<PositionedChar>> {
    let pages = doc.get_pages();
    let total = pages.len() as u32;

    if page_num == 0 || page_num > total {
        return Err(ExtractError::PageOutOfRange(page_num, total));
    }

    let page_id = *pages
        .get(&page_num)
        .ok_or(ExtractError::PageOutOfRange(page_num, total))?;

    let font_map = build_font_map(doc, page_id);
    let content_bytes = get_page_content_bytes(doc, page_id).unwrap_or_default();
    let content = match Content::decode(&content_bytes) {
        Ok(c) => c,
        Err(_) => return Ok(Vec::new()),
    };

    let chars = extract_chars_from_ops(&content.operations, page_num, &font_map);
    Ok(chars)
}

/// Get content stream bytes for a page.
fn get_page_content_bytes(doc: &Document, page_id: ObjectId) -> std::result::Result<Vec<u8>, ()> {
    doc.get_page_content(page_id).map_err(|_| ())
}

/// Extract text blocks from a list of operations, handling Form XObject recursion via `Do`.
fn extract_blocks_from_ops_inner(
    ops: &[Operation],
    page: u32,
    font_map: &HashMap<String, FontInfo>,
    doc_and_resources: Option<(&Document, &lopdf::Dictionary)>,
    depth: u32,
) -> Vec<TextBlock> {
    let mut state = TextState::default();
    let mut blocks = Vec::new();

    for op in ops {
        match op.operator.as_str() {
            "q" => {
                state.gs_stack.push(GraphicsState { ctm: state.ctm });
            }
            "Q" => {
                if let Some(gs) = state.gs_stack.pop() {
                    state.ctm = gs.ctm;
                }
            }
            "cm" => {
                if let Some(m) = extract_matrix(&op.operands) {
                    state.ctm = multiply_matrix(&state.ctm, &m);
                }
            }
            "BT" => {
                state.tm = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
                state.tlm = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
            }
            "Tf" => {
                if op.operands.len() >= 2 {
                    if let Object::Name(ref name) = op.operands[0] {
                        state.font_name = String::from_utf8_lossy(name).to_string();
                    }
                    if let Some(size) = as_number(&op.operands[1]) {
                        state.font_size = size;
                    }
                }
            }
            "Tc" => {
                if let Some(v) = op.operands.first().and_then(as_number) {
                    state.tc = v;
                }
            }
            "Tw" => {
                if let Some(v) = op.operands.first().and_then(as_number) {
                    state.tw = v;
                }
            }
            "Tz" => {
                if let Some(v) = op.operands.first().and_then(as_number) {
                    state.th = v;
                }
            }
            "TL" => {
                if let Some(v) = op.operands.first().and_then(as_number) {
                    state.tl = v;
                }
            }
            "Ts" => {
                if let Some(v) = op.operands.first().and_then(as_number) {
                    state.ts = v;
                }
            }
            "Td" => {
                if op.operands.len() >= 2 {
                    let tx = as_number(&op.operands[0]).unwrap_or(0.0);
                    let ty = as_number(&op.operands[1]).unwrap_or(0.0);
                    let new_tlm = multiply_matrix(&state.tlm, &[1.0, 0.0, 0.0, 1.0, tx, ty]);
                    state.tlm = new_tlm;
                    state.tm = new_tlm;
                }
            }
            "TD" => {
                if op.operands.len() >= 2 {
                    let tx = as_number(&op.operands[0]).unwrap_or(0.0);
                    let ty = as_number(&op.operands[1]).unwrap_or(0.0);
                    state.tl = -ty;
                    let new_tlm = multiply_matrix(&state.tlm, &[1.0, 0.0, 0.0, 1.0, tx, ty]);
                    state.tlm = new_tlm;
                    state.tm = new_tlm;
                }
            }
            "Tm" => {
                if let Some(m) = extract_matrix(&op.operands) {
                    state.tm = m;
                    state.tlm = m;
                }
            }
            "T*" => {
                let new_tlm = multiply_matrix(&state.tlm, &[1.0, 0.0, 0.0, 1.0, 0.0, -state.tl]);
                state.tlm = new_tlm;
                state.tm = new_tlm;
            }
            "Tj" => {
                let fi = font_map.get(&state.font_name);
                if let Some(text) = extract_string_operand_with_font(&op.operands, fi) {
                    let char_w = state.font_size * APPROX_CHAR_WIDTH * (state.th / 100.0);

                    // Skip empty text blocks (consistent with TJ handler) — empty
                    // blocks carry position but no data, causing phantom table
                    // detection with all-empty cells (#651).
                    if !text.is_empty() {
                        let x = state.tm[4];
                        let y = state.tm[5];
                        let text_width = text.len() as f64 * char_w;
                        blocks.push(TextBlock {
                            text: text.clone(),
                            page,
                            bbox: [x, y, x + text_width, y + state.font_size],
                            font_name: state.font_name.clone(),
                            font_size: state.font_size,
                        });
                    }

                    // Advance text position.
                    for _ in text.chars() {
                        state.tm[4] += char_w + state.tc;
                    }
                }
            }
            "TJ" => {
                if let Some(Object::Array(ref arr)) = op.operands.first() {
                    let x_start = state.tm[4];
                    let y = state.tm[5];
                    let char_w = state.font_size * APPROX_CHAR_WIDTH * (state.th / 100.0);
                    let mut combined_text = String::new();
                    let fi = font_map.get(&state.font_name);

                    for item in arr {
                        match item {
                            Object::String(bytes, _) => {
                                let text = decode_pdf_string_with_font(bytes, fi);
                                for _ in text.chars() {
                                    state.tm[4] += char_w + state.tc;
                                }
                                combined_text.push_str(&text);
                            }
                            _ => {
                                if let Some(adj) = as_number(item) {
                                    // Negative values move right, positive move left.
                                    state.tm[4] -= adj / 1000.0 * state.font_size;
                                }
                            }
                        }
                    }

                    if !combined_text.is_empty() {
                        let x_end = state.tm[4];
                        blocks.push(TextBlock {
                            text: combined_text,
                            page,
                            bbox: [x_start, y, x_end, y + state.font_size],
                            font_name: state.font_name.clone(),
                            font_size: state.font_size,
                        });
                    }
                }
            }
            "'" => {
                // Move to next line and show text.
                let new_tlm =
                    multiply_matrix(&state.tlm, &[1.0, 0.0, 0.0, 1.0, 0.0, -state.tl]);
                state.tlm = new_tlm;
                state.tm = new_tlm;

                let fi = font_map.get(&state.font_name);
                if let Some(text) = extract_string_operand_with_font(&op.operands, fi) {
                    let char_w = state.font_size * APPROX_CHAR_WIDTH * (state.th / 100.0);

                    if !text.is_empty() {
                        let x = state.tm[4];
                        let y = state.tm[5];
                        let text_width = text.len() as f64 * char_w;
                        blocks.push(TextBlock {
                            text: text.clone(),
                            page,
                            bbox: [x, y, x + text_width, y + state.font_size],
                            font_name: state.font_name.clone(),
                            font_size: state.font_size,
                        });
                    }

                    for _ in text.chars() {
                        state.tm[4] += char_w + state.tc;
                    }
                }
            }
            "\"" => {
                // Set word/char spacing, move to next line, show text.
                if op.operands.len() >= 3 {
                    if let Some(tw) = as_number(&op.operands[0]) {
                        state.tw = tw;
                    }
                    if let Some(tc) = as_number(&op.operands[1]) {
                        state.tc = tc;
                    }

                    let new_tlm =
                        multiply_matrix(&state.tlm, &[1.0, 0.0, 0.0, 1.0, 0.0, -state.tl]);
                    state.tlm = new_tlm;
                    state.tm = new_tlm;

                    let fi =
                        font_map.get(&state.font_name);
                    if let Some(text) =
                        extract_string_operand_with_font(&op.operands[2..], fi)
                    {
                        let char_w =
                            state.font_size * APPROX_CHAR_WIDTH * (state.th / 100.0);

                        if !text.is_empty() {
                            let x = state.tm[4];
                            let y = state.tm[5];
                            let text_width = text.len() as f64 * char_w;
                            blocks.push(TextBlock {
                                text: text.clone(),
                                page,
                                bbox: [x, y, x + text_width, y + state.font_size],
                                font_name: state.font_name.clone(),
                                font_size: state.font_size,
                            });
                        }

                        for _ in text.chars() {
                            state.tm[4] += char_w + state.tc;
                        }
                    }
                }
            }
            "Do" => {
                // Invoke Form XObject — recurse into its content stream.
                if depth < 5 {
                    if let Some((doc, resources)) = doc_and_resources {
                        if let Some(Object::Name(ref xobj_name)) = op.operands.first() {
                            let xobj_name_str = String::from_utf8_lossy(xobj_name);
                            if let Some(xobj_blocks) = extract_form_xobject_text(
                                doc,
                                resources,
                                &xobj_name_str,
                                page,
                                font_map,
                                depth,
                            ) {
                                blocks.extend(xobj_blocks);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    blocks
}

/// Extract text from a Form XObject referenced by name in the page's Resources/XObject dict.
fn extract_form_xobject_text(
    doc: &Document,
    resources: &lopdf::Dictionary,
    name: &str,
    page: u32,
    font_map: &HashMap<String, FontInfo>,
    depth: u32,
) -> Option<Vec<TextBlock>> {
    // Look up the XObject in the Resources dictionary.
    let xobj_dict = match resources.get(b"XObject").ok()? {
        Object::Dictionary(d) => d.clone(),
        Object::Reference(r) => match doc.get_object(*r).ok()? {
            Object::Dictionary(d) => d.clone(),
            _ => return None,
        },
        _ => return None,
    };

    let xobj_ref = match xobj_dict.get(name.as_bytes()).ok()? {
        Object::Reference(r) => *r,
        _ => return None,
    };

    let stream = match doc.get_object(xobj_ref).ok()? {
        Object::Stream(s) => s.clone(),
        _ => return None,
    };

    // Verify it's a Form XObject (not Image).
    let subtype = stream
        .dict
        .get(b"Subtype")
        .ok()
        .and_then(|o| match o {
            Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
            _ => None,
        })
        .unwrap_or_default();
    if subtype != "Form" {
        return None;
    }

    // Decode the content stream.
    let content_bytes = stream
        .decompressed_content()
        .ok()
        .unwrap_or_else(|| stream.content.clone());
    let content = Content::decode(&content_bytes).ok()?;

    // Build font map: XObject's own fonts take priority over inherited page fonts.
    // A Form XObject may reuse font names (e.g. /F1) with different encodings.
    let mut xobj_font_map = font_map.clone();
    if let Some(xobj_resources) = resolve_dict(doc, &stream.dict, b"Resources") {
        if let Some(xobj_fonts) = resolve_dict(doc, &xobj_resources, b"Font") {
            for (name_bytes, value) in xobj_fonts.iter() {
                let fname = String::from_utf8_lossy(name_bytes).to_string();
                if let Some(fi) = build_font_info_from_value(doc, value) {
                    xobj_font_map.insert(fname, fi);
                }
            }
        }
    }

    // Use the XObject's own Resources dict for recursive Do lookups.
    let xobj_resources =
        resolve_dict(doc, &stream.dict, b"Resources").unwrap_or_else(|| resources.clone());

    Some(extract_blocks_from_ops_inner(
        &content.operations,
        page,
        &xobj_font_map,
        Some((doc, &xobj_resources)),
        depth + 1,
    ))
}

/// Build a FontInfo from a font dictionary value (used for XObject font resolution).
fn build_font_info_from_value(doc: &Document, value: &Object) -> Option<FontInfo> {
    let font = match value {
        Object::Reference(r) => match doc.get_object(*r).ok()? {
            Object::Dictionary(d) => d.clone(),
            _ => return None,
        },
        Object::Dictionary(d) => d.clone(),
        _ => return None,
    };

    let subtype = font
        .get(b"Subtype")
        .ok()
        .and_then(|o| match o {
            Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
            _ => None,
        })
        .unwrap_or_default();

    let is_cid = subtype == "Type0";
    let mut to_unicode = parse_to_unicode_from_font(doc, &font);

    if to_unicode.is_empty() && is_cid {
        if let Some(Object::Array(descendants)) = font.get(b"DescendantFonts").ok() {
            for d in descendants {
                let desc_dict = match d {
                    Object::Reference(r) => {
                        // Don't use `?` here — a bad reference in one descendant
                        // should not abort the entire font info construction.
                        let Some(Object::Dictionary(d)) = doc.get_object(*r).ok()
                        else {
                            continue;
                        };
                        d
                    }
                    Object::Dictionary(d) => d,
                    _ => continue,
                };
                let tu = parse_to_unicode_from_font(doc, desc_dict);
                if !tu.is_empty() {
                    to_unicode = tu;
                    break;
                }
            }
        }
    }

    let encoding_map = if !is_cid {
        build_encoding_map(doc, &font)
    } else {
        [None; 256]
    };

    Some(FontInfo {
        is_cid,
        to_unicode,
        encoding_map,
    })
}

/// Extract positioned characters from operations.
fn extract_chars_from_ops(
    ops: &[Operation],
    page: u32,
    font_map: &HashMap<String, FontInfo>,
) -> Vec<PositionedChar> {
    let mut state = TextState::default();
    let mut chars = Vec::new();

    for op in ops {
        match op.operator.as_str() {
            "q" => {
                state.gs_stack.push(GraphicsState { ctm: state.ctm });
            }
            "Q" => {
                if let Some(gs) = state.gs_stack.pop() {
                    state.ctm = gs.ctm;
                }
            }
            "cm" => {
                if let Some(m) = extract_matrix(&op.operands) {
                    state.ctm = multiply_matrix(&state.ctm, &m);
                }
            }
            "BT" => {
                state.tm = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
                state.tlm = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
            }
            "Tf" => {
                if op.operands.len() >= 2 {
                    if let Object::Name(ref name) = op.operands[0] {
                        state.font_name = String::from_utf8_lossy(name).to_string();
                    }
                    if let Some(size) = as_number(&op.operands[1]) {
                        state.font_size = size;
                    }
                }
            }
            "Tc" => {
                if let Some(v) = op.operands.first().and_then(as_number) {
                    state.tc = v;
                }
            }
            "Tw" => {
                if let Some(v) = op.operands.first().and_then(as_number) {
                    state.tw = v;
                }
            }
            "Tz" => {
                if let Some(v) = op.operands.first().and_then(as_number) {
                    state.th = v;
                }
            }
            "TL" => {
                if let Some(v) = op.operands.first().and_then(as_number) {
                    state.tl = v;
                }
            }
            "Ts" => {
                if let Some(v) = op.operands.first().and_then(as_number) {
                    state.ts = v;
                }
            }
            "Td" => {
                if op.operands.len() >= 2 {
                    let tx = as_number(&op.operands[0]).unwrap_or(0.0);
                    let ty = as_number(&op.operands[1]).unwrap_or(0.0);
                    let new_tlm = multiply_matrix(&state.tlm, &[1.0, 0.0, 0.0, 1.0, tx, ty]);
                    state.tlm = new_tlm;
                    state.tm = new_tlm;
                }
            }
            "TD" => {
                if op.operands.len() >= 2 {
                    let tx = as_number(&op.operands[0]).unwrap_or(0.0);
                    let ty = as_number(&op.operands[1]).unwrap_or(0.0);
                    state.tl = -ty;
                    let new_tlm = multiply_matrix(&state.tlm, &[1.0, 0.0, 0.0, 1.0, tx, ty]);
                    state.tlm = new_tlm;
                    state.tm = new_tlm;
                }
            }
            "Tm" => {
                if let Some(m) = extract_matrix(&op.operands) {
                    state.tm = m;
                    state.tlm = m;
                }
            }
            "T*" => {
                let new_tlm = multiply_matrix(&state.tlm, &[1.0, 0.0, 0.0, 1.0, 0.0, -state.tl]);
                state.tlm = new_tlm;
                state.tm = new_tlm;
            }
            "Tj" => {
                let fi = font_map.get(&state.font_name);
                if let Some(text) = extract_string_operand_with_font(&op.operands, fi) {
                    let char_w = state.font_size * APPROX_CHAR_WIDTH * (state.th / 100.0);
                    for ch in text.chars() {
                        let (x, y) = apply_ctm(&state);
                        chars.push(PositionedChar {
                            ch,
                            page,
                            bbox: [x, y, x + char_w, y + state.font_size],
                        });
                        state.tm[4] += char_w + state.tc;
                    }
                }
            }
            "TJ" => {
                if let Some(Object::Array(ref arr)) = op.operands.first() {
                    let char_w = state.font_size * APPROX_CHAR_WIDTH * (state.th / 100.0);
                    let fi = font_map.get(&state.font_name);
                    for item in arr {
                        match item {
                            Object::String(bytes, _) => {
                                let text = decode_pdf_string_with_font(bytes, fi);
                                for ch in text.chars() {
                                    let (x, y) = apply_ctm(&state);
                                    chars.push(PositionedChar {
                                        ch,
                                        page,
                                        bbox: [x, y, x + char_w, y + state.font_size],
                                    });
                                    state.tm[4] += char_w + state.tc;
                                }
                            }
                            _ => {
                                if let Some(adj) = as_number(item) {
                                    state.tm[4] -= adj / 1000.0 * state.font_size;
                                }
                            }
                        }
                    }
                }
            }
            "'" => {
                let new_tlm = multiply_matrix(&state.tlm, &[1.0, 0.0, 0.0, 1.0, 0.0, -state.tl]);
                state.tlm = new_tlm;
                state.tm = new_tlm;

                let fi = font_map.get(&state.font_name);
                if let Some(text) = extract_string_operand_with_font(&op.operands, fi) {
                    let char_w = state.font_size * APPROX_CHAR_WIDTH * (state.th / 100.0);
                    for ch in text.chars() {
                        let (x, y) = apply_ctm(&state);
                        chars.push(PositionedChar {
                            ch,
                            page,
                            bbox: [x, y, x + char_w, y + state.font_size],
                        });
                        state.tm[4] += char_w + state.tc;
                    }
                }
            }
            "\"" => {
                if op.operands.len() >= 3 {
                    if let Some(tw) = as_number(&op.operands[0]) {
                        state.tw = tw;
                    }
                    if let Some(tc) = as_number(&op.operands[1]) {
                        state.tc = tc;
                    }

                    let new_tlm =
                        multiply_matrix(&state.tlm, &[1.0, 0.0, 0.0, 1.0, 0.0, -state.tl]);
                    state.tlm = new_tlm;
                    state.tm = new_tlm;

                    let fi = font_map.get(&state.font_name);
                    if let Some(text) = extract_string_operand_with_font(&op.operands[2..], fi) {
                        let char_w = state.font_size * APPROX_CHAR_WIDTH * (state.th / 100.0);
                        for ch in text.chars() {
                            let (x, y) = apply_ctm(&state);
                            chars.push(PositionedChar {
                                ch,
                                page,
                                bbox: [x, y, x + char_w, y + state.font_size],
                            });
                            state.tm[4] += char_w + state.tc;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    chars
}

/// Apply the current transformation matrix (CTM) to the text matrix position,
/// returning (x, y) in page/user space.
///
/// Mirrors the `compute_x` / `compute_y` logic in pdf-manip's `text_run.rs` so
/// that character positions emitted by `extract_chars_from_ops` use the same
/// coordinate space as `TextRun.x` / `TextRun.y` from `extract_text_runs`.
/// Without this, PDFs whose content streams set a non-identity CTM via `cm`
/// produce mismatched coordinates, breaking the spatial redaction fallback.
#[inline]
fn apply_ctm(state: &TextState) -> (f64, f64) {
    let x = state.ctm[0] * state.tm[4] + state.ctm[2] * state.tm[5] + state.ctm[4];
    let y = state.ctm[1] * state.tm[4] + state.ctm[3] * state.tm[5] + state.ctm[5];
    (x, y)
}

/// Extract the first string operand, decoding via font's ToUnicode CMap if available.
fn extract_string_operand_with_font(
    operands: &[Object],
    font_info: Option<&FontInfo>,
) -> Option<String> {
    for op in operands {
        if let Object::String(bytes, _) = op {
            return Some(decode_pdf_string_with_font(bytes, font_info));
        }
    }
    None
}

/// Convert a PDF object to a number (f64).
fn as_number(obj: &Object) -> Option<f64> {
    match obj {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(f) => Some(*f as f64),
        _ => None,
    }
}

/// Extract a 6-element transformation matrix from operands.
fn extract_matrix(operands: &[Object]) -> Option<[f64; 6]> {
    if operands.len() < 6 {
        return None;
    }
    let a = as_number(&operands[0])?;
    let b = as_number(&operands[1])?;
    let c = as_number(&operands[2])?;
    let d = as_number(&operands[3])?;
    let e = as_number(&operands[4])?;
    let f = as_number(&operands[5])?;
    Some([a, b, c, d, e, f])
}

/// Multiply two 3x3 transformation matrices (stored as [a, b, c, d, e, f]).
fn multiply_matrix(m1: &[f64; 6], m2: &[f64; 6]) -> [f64; 6] {
    [
        m1[0] * m2[0] + m1[1] * m2[2],
        m1[0] * m2[1] + m1[1] * m2[3],
        m1[2] * m2[0] + m1[3] * m2[2],
        m1[2] * m2[1] + m1[3] * m2[3],
        m1[4] * m2[0] + m1[5] * m2[2] + m2[4],
        m1[4] * m2[1] + m1[5] * m2[3] + m2[5],
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Document, Object, Stream};

    /// Helper: create a minimal doc with text content.
    fn make_doc_with_text(content: &[u8]) -> Document {
        let mut doc = Document::with_version("1.7");

        let content_stream = Stream::new(dictionary! {}, content.to_vec());
        let content_id = doc.add_object(Object::Stream(content_stream));

        let page_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content_id),
        };
        let page_id = doc.add_object(Object::Dictionary(page_dict));

        let pages_dict = dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        };
        let pages_id = doc.add_object(Object::Dictionary(pages_dict));

        if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
            d.set("Parent", Object::Reference(pages_id));
        }

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        doc
    }

    #[test]
    fn extract_simple_text() {
        let doc = make_doc_with_text(b"BT /F1 12 Tf (Hello World) Tj ET");
        let blocks = extract_text(&doc);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].text, "Hello World");
        assert_eq!(blocks[0].page, 1);
        assert_eq!(blocks[0].font_size, 12.0);
    }

    #[test]
    fn extract_page_text_single() {
        let doc = make_doc_with_text(b"BT /F1 12 Tf (Hello) Tj ET");
        let text = extract_page_text(&doc, 1).unwrap();
        assert_eq!(text, "Hello");
    }

    #[test]
    fn extract_page_text_out_of_range() {
        let doc = make_doc_with_text(b"BT /F1 12 Tf (Hello) Tj ET");
        let result = extract_page_text(&doc, 5);
        assert!(result.is_err());
    }

    #[test]
    fn extract_positioned_chars_basic() {
        let doc = make_doc_with_text(b"BT /F1 12 Tf (AB) Tj ET");
        let chars = extract_positioned_chars(&doc, 1).unwrap();
        assert_eq!(chars.len(), 2);
        assert_eq!(chars[0].ch, 'A');
        assert_eq!(chars[1].ch, 'B');
        assert_eq!(chars[0].page, 1);
        // Second char should be positioned after the first.
        assert!(chars[1].bbox[0] > chars[0].bbox[0]);
    }

    #[test]
    fn extract_tj_array() {
        let doc = make_doc_with_text(b"BT /F1 12 Tf [(He) -100 (llo)] TJ ET");
        let blocks = extract_text(&doc);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].text, "Hello");
    }

    #[test]
    fn empty_page_extracts_no_text() {
        let doc = make_doc_with_text(b"q Q");
        let blocks = extract_text(&doc);
        assert!(blocks.is_empty());
    }

    #[test]
    fn multiline_text_extraction() {
        let doc = make_doc_with_text(b"BT /F1 12 Tf 12 TL (Line1) Tj T* (Line2) Tj ET");
        let blocks = extract_text(&doc);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].text, "Line1");
        assert_eq!(blocks[1].text, "Line2");
    }
}
