//! Text extraction with character-level position tracking.
//!
//! Parses content stream text operators (Tj, TJ, Tm, Td, TD, T*, Tc, Tw, Tz, TL, Ts, ', ")
//! to extract text with positional information.

use crate::error::{ExtractError, Result};
use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, ObjectId};
use std::collections::HashMap;

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
struct FontInfo {
    /// True if the font uses 2-byte CID encoding (Identity-H/V or other CID CMaps).
    is_cid: bool,
    /// ToUnicode CMap: maps character code(s) to Unicode string.
    /// For CID fonts the key is a 2-byte big-endian value; for simple fonts it's a 1-byte code.
    to_unicode: HashMap<u32, String>,
}

/// Build a map from font resource name (e.g. "F1") to FontInfo for a page.
fn build_font_map(doc: &Document, page_id: ObjectId) -> HashMap<String, FontInfo> {
    let mut map = HashMap::new();

    // Get the page's Resources dictionary (may be inherited from parent Pages node).
    let resources = get_page_resources(doc, page_id);
    let font_dict = match resources.and_then(|res| {
        match res.get(b"Font").ok()? {
            Object::Dictionary(d) => Some(d.clone()),
            Object::Reference(r) => match doc.get_object(*r).ok()? {
                Object::Dictionary(d) => Some(d.clone()),
                _ => None,
            },
            _ => None,
        }
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

        map.insert(font_name, FontInfo { is_cid, to_unicode });
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
        Some(Object::Stream(ref s)) => {
            s.decompressed_content().ok().unwrap_or_else(|| s.content.clone())
        }
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
                } else {
                    result.push(b as char);
                }
            }
            return result;
        }
    }

    // Fallback: PDFDocEncoding (ASCII + Latin-1).
    bytes.iter().map(|&b| b as char).collect()
}

/// Extract text blocks from a specific page.
pub fn extract_page_blocks(doc: &Document, page_num: u32) -> Vec<TextBlock> {
    let pages = doc.get_pages();
    let Some(&page_id) = pages.get(&page_num) else {
        return Vec::new();
    };

    let font_map = build_font_map(doc, page_id);
    if let Ok(content_bytes) = get_page_content_bytes(doc, page_id) {
        if let Ok(content) = Content::decode(&content_bytes) {
            return extract_blocks_from_ops(&content.operations, page_num, &font_map);
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
        if let Ok(content_bytes) = get_page_content_bytes(doc, page_id) {
            if let Ok(content) = Content::decode(&content_bytes) {
                let page_blocks =
                    extract_blocks_from_ops(&content.operations, page_num, &font_map);
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
    let content_bytes = get_page_content_bytes(doc, page_id).unwrap_or_default();
    let content = match Content::decode(&content_bytes) {
        Ok(c) => c,
        Err(_) => return Ok(String::new()),
    };

    let blocks = extract_blocks_from_ops(&content.operations, page_num, &font_map);
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

/// Extract text blocks from a list of operations.
fn extract_blocks_from_ops(
    ops: &[Operation],
    page: u32,
    font_map: &HashMap<String, FontInfo>,
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
                    let x = state.tm[4];
                    let y = state.tm[5];
                    let char_w = state.font_size * APPROX_CHAR_WIDTH * (state.th / 100.0);
                    let text_width = text.len() as f64 * char_w;

                    blocks.push(TextBlock {
                        text: text.clone(),
                        page,
                        bbox: [x, y, x + text_width, y + state.font_size],
                        font_name: state.font_name.clone(),
                        font_size: state.font_size,
                    });

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
                let new_tlm = multiply_matrix(&state.tlm, &[1.0, 0.0, 0.0, 1.0, 0.0, -state.tl]);
                state.tlm = new_tlm;
                state.tm = new_tlm;

                let fi = font_map.get(&state.font_name);
                if let Some(text) = extract_string_operand_with_font(&op.operands, fi) {
                    let x = state.tm[4];
                    let y = state.tm[5];
                    let char_w = state.font_size * APPROX_CHAR_WIDTH * (state.th / 100.0);
                    let text_width = text.len() as f64 * char_w;

                    blocks.push(TextBlock {
                        text: text.clone(),
                        page,
                        bbox: [x, y, x + text_width, y + state.font_size],
                        font_name: state.font_name.clone(),
                        font_size: state.font_size,
                    });

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

                    let fi = font_map.get(&state.font_name);
                    if let Some(text) = extract_string_operand_with_font(&op.operands[2..], fi) {
                        let x = state.tm[4];
                        let y = state.tm[5];
                        let char_w = state.font_size * APPROX_CHAR_WIDTH * (state.th / 100.0);
                        let text_width = text.len() as f64 * char_w;

                        blocks.push(TextBlock {
                            text: text.clone(),
                            page,
                            bbox: [x, y, x + text_width, y + state.font_size],
                            font_name: state.font_name.clone(),
                            font_size: state.font_size,
                        });

                        for _ in text.chars() {
                            state.tm[4] += char_w + state.tc;
                        }
                    }
                }
            }
            _ => {}
        }
    }

    blocks
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
