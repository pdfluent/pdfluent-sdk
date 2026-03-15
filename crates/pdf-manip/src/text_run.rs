//! Text run identification from content streams with CMap-aware decoding.
//!
//! Walks content stream operations, tracks font/text state, and decodes
//! text bytes to Unicode using ToUnicode CMaps extracted from font resources.

use crate::content_editor::{as_number, multiply_matrix, ContentEditor};
use crate::encoding_utils::build_font_encoding;
use crate::error::{ManipError, Result};
use lopdf::content::Operation;
use lopdf::{Document, Object, ObjectId};
use pdf_font::cmap::{BfString, CMap};
use std::collections::HashMap;
use std::ops::Range;

const IDENTITY: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// A contiguous run of text extracted from a content stream.
#[derive(Debug, Clone)]
pub struct TextRun {
    /// Decoded Unicode text.
    pub text: String,
    /// Range of operation indices in the content stream that produced this run.
    pub ops_range: Range<usize>,
    /// Font resource name (e.g. "F1").
    pub font_name: String,
    /// Font size in points.
    pub font_size: f64,
    /// X position in user space.
    pub x: f64,
    /// Y position in user space.
    pub y: f64,
    /// Estimated width of the text run in user space units.
    pub width: f64,
}

/// Font encoding information extracted from a PDF document.
///
/// Maps font resource names to their ToUnicode CMap (if available),
/// encoding type, and glyph widths.
#[derive(Debug, Clone)]
pub struct FontMap {
    pub(crate) fonts: HashMap<String, FontInfo>,
}

#[derive(Debug, Clone)]
pub(crate) struct FontInfo {
    pub(crate) to_unicode: Option<CMap>,
    pub(crate) encoding: FontEncoding,
    pub(crate) widths: GlyphWidths,
    /// Differences-based code→Unicode map for fonts without ToUnicode.
    /// Empty when no Encoding/Differences information is available.
    pub(crate) differences_encoding: HashMap<u8, char>,
    /// True when the BaseFont name has a subset prefix (e.g. "ABCDEF+FontName").
    /// Subset fonts only contain the glyphs used in the document; encoding
    /// replacement text with arbitrary characters is unsafe.
    pub(crate) is_subset: bool,
}

/// Font encoding type.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FontEncoding {
    /// Standard built-in encoding or Differences-based.
    Builtin,
    /// Identity-H or Identity-V (2-byte CID).
    IdentityH,
    /// Custom CMap encoding.
    CustomCMap,
}

/// Glyph width information for estimating text run widths.
#[derive(Debug, Clone)]
pub(crate) struct GlyphWidths {
    first_char: u32,
    widths: Vec<f64>,
    default_width: f64,
}

impl GlyphWidths {
    fn empty() -> Self {
        Self {
            first_char: 0,
            widths: Vec::new(),
            default_width: 1000.0,
        }
    }

    /// Get the width of a character code in font units (typically 1/1000 em).
    fn width_for_code(&self, code: u32) -> f64 {
        let idx = code.checked_sub(self.first_char).and_then(|i| {
            let i = i as usize;
            if i < self.widths.len() {
                Some(i)
            } else {
                None
            }
        });
        match idx {
            Some(i) => self.widths[i],
            None => self.default_width,
        }
    }
}

impl FontMap {
    /// Build a font map from a page's font resources in a lopdf Document.
    pub fn from_page(doc: &Document, page_num: u32) -> Result<Self> {
        let pages = doc.get_pages();
        let total = pages.len() as u32;
        let &page_id = pages.get(&page_num).ok_or(ManipError::PageOutOfRange(
            page_num as usize,
            total as usize,
        ))?;

        let mut fonts = HashMap::new();
        let font_dict = get_page_font_dict(doc, page_id);

        for (name, font_ref) in &font_dict {
            let info = build_font_info(doc, font_ref);
            fonts.insert(name.clone(), info);
        }

        Ok(Self { fonts })
    }

    /// Create an empty font map (no CMap decoding — raw byte fallback).
    pub fn empty() -> Self {
        Self {
            fonts: HashMap::new(),
        }
    }

    /// Build a FontMap from a Form XObject's (or AP stream's) own Resources/Font
    /// dictionary.  The XObject's fonts take priority over `page_fonts` so that
    /// character-code decoding uses the correct CMap for each stream.
    ///
    /// Falls back to `page_fonts` for any name not found in the stream's own
    /// Resources, and uses `page_fonts` as-is when the stream has no Resources.
    pub fn from_xobject_stream(doc: &Document, stream_id: ObjectId, page_fonts: &FontMap) -> Self {
        // Start with page-level fonts as the base (fallback for any missing entries).
        let mut fonts = page_fonts.fonts.clone();

        let stream_dict = match doc.get_object(stream_id) {
            Ok(Object::Stream(ref s)) => s.dict.clone(),
            _ => return Self { fonts },
        };

        let resources = match stream_dict.get(b"Resources") {
            Ok(Object::Dictionary(ref d)) => d.clone(),
            Ok(Object::Reference(id)) => match doc.get_object(*id) {
                Ok(Object::Dictionary(ref d)) => d.clone(),
                _ => return Self { fonts },
            },
            _ => return Self { fonts },
        };

        let font_dict = match resources.get(b"Font") {
            Ok(Object::Dictionary(ref d)) => d.clone(),
            Ok(Object::Reference(id)) => match doc.get_object(*id) {
                Ok(Object::Dictionary(ref d)) => d.clone(),
                _ => return Self { fonts },
            },
            _ => return Self { fonts },
        };

        for (key, value) in font_dict.iter() {
            let name = String::from_utf8_lossy(key).to_string();
            if let Object::Reference(id) = value {
                // XObject's own font definition overrides any page-level entry
                // with the same name, ensuring correct CMap decoding.
                let info = build_font_info(doc, id);
                fonts.insert(name, info);
            }
        }

        Self { fonts }
    }

    /// Decode a PDF string from a Tj/TJ operand using the font's ToUnicode CMap.
    pub fn decode_string(&self, font_name: &str, bytes: &[u8]) -> String {
        let info = match self.fonts.get(font_name) {
            Some(info) => info,
            None => return decode_pdf_string_fallback(bytes),
        };

        if let Some(ref cmap) = info.to_unicode {
            return decode_with_cmap(bytes, cmap, &info.encoding);
        }

        decode_pdf_string_fallback(bytes)
    }

    /// Get the width of a character code in font units.
    fn char_width(&self, font_name: &str, code: u32) -> f64 {
        self.fonts
            .get(font_name)
            .map(|info| info.widths.width_for_code(code))
            .unwrap_or(1000.0)
    }

    /// Check if a font uses 2-byte CID encoding.
    pub(crate) fn is_cid_font(&self, font_name: &str) -> bool {
        self.fonts
            .get(font_name)
            .map(|info| {
                matches!(
                    info.encoding,
                    FontEncoding::IdentityH | FontEncoding::CustomCMap
                )
            })
            .unwrap_or(false)
    }

    /// Returns true when the font is a subset (BaseFont has an "ABCDEF+" prefix).
    ///
    /// Subset fonts only contain the glyphs used in the source document.
    /// Encoding arbitrary replacement text is unsafe because the required
    /// glyphs may not be present.
    pub(crate) fn is_subset_font(&self, font_name: &str) -> bool {
        self.fonts
            .get(font_name)
            .map(|info| info.is_subset)
            .unwrap_or(false)
    }

    /// Build a reverse map (Unicode char → character code) for a font.
    ///
    /// Priority:
    /// 1. ToUnicode CMap reverse map (most authoritative).
    /// 2. Differences/Encoding-based reverse map (for fonts without ToUnicode
    ///    but with explicit Encoding/Differences, e.g. TeX OT1 fonts). Only
    ///    populated for single-byte (Builtin) fonts.
    /// 3. Empty map — caller falls back to Latin-1.
    pub fn build_reverse_map(&self, font_name: &str) -> std::collections::HashMap<char, u32> {
        let mut reverse = std::collections::HashMap::new();
        let info = match self.fonts.get(font_name) {
            Some(info) => info,
            None => return reverse,
        };

        if let Some(ref cmap) = info.to_unicode {
            let max_code: u32 = if matches!(info.encoding, FontEncoding::Builtin) {
                0xFF
            } else {
                0xFFFF
            };
            for code in 0..=max_code {
                if let Some(BfString::Char(ch)) = cmap.lookup_bf_string(code) {
                    reverse.entry(ch).or_insert(code);
                }
            }
            return reverse;
        }

        // No ToUnicode: use Differences-based encoding if available.
        // Build reverse from differences_encoding (char → code).
        if !info.differences_encoding.is_empty() && matches!(info.encoding, FontEncoding::Builtin) {
            for (&code, &ch) in &info.differences_encoding {
                reverse.entry(ch).or_insert(u32::from(code));
            }
        }

        reverse
    }
}

/// Extract text runs from a content stream using font encoding information.
pub fn extract_text_runs(editor: &ContentEditor, fonts: &FontMap) -> Vec<TextRun> {
    let ops = editor.operations();
    let mut runs = Vec::new();
    let mut state = TextState::default();

    for (idx, op) in ops.iter().enumerate() {
        match op.operator.as_str() {
            "q" => {
                state.gs_stack.push(SavedGS { ctm: state.ctm });
            }
            "Q" => {
                if let Some(saved) = state.gs_stack.pop() {
                    state.ctm = saved.ctm;
                }
            }
            "cm" => {
                if let Some(m) = extract_matrix(&op.operands) {
                    state.ctm = multiply_matrix(&state.ctm, &m);
                }
            }
            "BT" => {
                state.in_text = true;
                state.tm = IDENTITY;
                state.tlm = IDENTITY;
            }
            "ET" => {
                state.in_text = false;
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
                if let Some(run) = process_tj(idx, op, &mut state, fonts) {
                    runs.push(run);
                }
            }
            "TJ" => {
                if let Some(run) = process_tj_array(idx, op, &mut state, fonts) {
                    runs.push(run);
                }
            }
            "'" => {
                // Move to next line, then show text.
                let new_tlm = multiply_matrix(&state.tlm, &[1.0, 0.0, 0.0, 1.0, 0.0, -state.tl]);
                state.tlm = new_tlm;
                state.tm = new_tlm;

                if let Some(run) = process_tj(idx, op, &mut state, fonts) {
                    runs.push(run);
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

                    // The string operand is the third one.
                    if let Some(Object::String(ref bytes, _)) = op.operands.get(2) {
                        let text = fonts.decode_string(&state.font_name, bytes);
                        if !text.is_empty() {
                            let x = compute_x(&state);
                            let y = compute_y(&state);
                            let width = compute_text_width(bytes, &state, fonts);
                            advance_text_position(bytes, &mut state, fonts);
                            runs.push(TextRun {
                                text,
                                ops_range: idx..idx + 1,
                                font_name: state.font_name.clone(),
                                font_size: state.font_size,
                                x,
                                y,
                                width,
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }

    runs
}

/// Extract text runs from a page in a document.
pub fn extract_page_text_runs(doc: &Document, page_num: u32) -> Result<Vec<TextRun>> {
    let editor = crate::content_editor::editor_for_page(doc, page_num)?;
    let fonts = FontMap::from_page(doc, page_num)?;
    Ok(extract_text_runs(&editor, &fonts))
}

// ---------------------------------------------------------------------------
// Internal state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct TextState {
    ctm: [f64; 6],
    tm: [f64; 6],
    tlm: [f64; 6],
    font_name: String,
    font_size: f64,
    tc: f64,
    tw: f64,
    th: f64,
    tl: f64,
    ts: f64,
    in_text: bool,
    gs_stack: Vec<SavedGS>,
}

#[derive(Debug, Clone)]
struct SavedGS {
    ctm: [f64; 6],
}

impl Default for TextState {
    fn default() -> Self {
        Self {
            ctm: IDENTITY,
            tm: IDENTITY,
            tlm: IDENTITY,
            font_name: String::new(),
            font_size: 12.0,
            tc: 0.0,
            tw: 0.0,
            th: 100.0,
            tl: 0.0,
            ts: 0.0,
            in_text: false,
            gs_stack: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Text showing operator processors
// ---------------------------------------------------------------------------

fn process_tj(
    idx: usize,
    op: &Operation,
    state: &mut TextState,
    fonts: &FontMap,
) -> Option<TextRun> {
    let bytes = match op.operands.first() {
        Some(Object::String(ref b, _)) => b,
        _ => return None,
    };

    let text = fonts.decode_string(&state.font_name, bytes);
    if text.is_empty() {
        return None;
    }

    let x = compute_x(state);
    let y = compute_y(state);
    let width = compute_text_width(bytes, state, fonts);
    advance_text_position(bytes, state, fonts);

    Some(TextRun {
        text,
        ops_range: idx..idx + 1,
        font_name: state.font_name.clone(),
        font_size: state.font_size,
        x,
        y,
        width,
    })
}

fn process_tj_array(
    idx: usize,
    op: &Operation,
    state: &mut TextState,
    fonts: &FontMap,
) -> Option<TextRun> {
    let arr = match op.operands.first() {
        Some(Object::Array(ref a)) => a,
        _ => return None,
    };

    let x_start = compute_x(state);
    let y = compute_y(state);
    let mut combined_text = String::new();

    for item in arr {
        match item {
            Object::String(ref bytes, _) => {
                let text = fonts.decode_string(&state.font_name, bytes);
                combined_text.push_str(&text);
                advance_text_position(bytes, state, fonts);
            }
            _ => {
                if let Some(adj) = as_number(item) {
                    // TJ spacing: negative = move right, positive = move left.
                    state.tm[4] -= adj / 1000.0 * state.font_size * (state.th / 100.0);
                }
            }
        }
    }

    if combined_text.is_empty() {
        return None;
    }

    let x_end = compute_x(state);
    let width = (x_end - x_start).abs();

    Some(TextRun {
        text: combined_text,
        ops_range: idx..idx + 1,
        font_name: state.font_name.clone(),
        font_size: state.font_size,
        x: x_start,
        y,
        width,
    })
}

// ---------------------------------------------------------------------------
// Position and width computation
// ---------------------------------------------------------------------------

fn compute_x(state: &TextState) -> f64 {
    // Apply CTM to text matrix position.
    state.ctm[0] * state.tm[4] + state.ctm[2] * state.tm[5] + state.ctm[4]
}

fn compute_y(state: &TextState) -> f64 {
    state.ctm[1] * state.tm[4] + state.ctm[3] * state.tm[5] + state.ctm[5]
}

fn compute_text_width(bytes: &[u8], state: &TextState, fonts: &FontMap) -> f64 {
    let scale = state.font_size * (state.th / 100.0);
    if fonts.is_cid_font(&state.font_name) {
        // 2-byte codes.
        let mut width = 0.0;
        let mut i = 0;
        while i + 1 < bytes.len() {
            let code = u32::from(bytes[i]) << 8 | u32::from(bytes[i + 1]);
            let w = fonts.char_width(&state.font_name, code);
            width += w / 1000.0 * scale + state.tc;
            i += 2;
        }
        width
    } else {
        // Single-byte codes.
        let mut width = 0.0;
        for &b in bytes {
            let w = fonts.char_width(&state.font_name, u32::from(b));
            width += w / 1000.0 * scale + state.tc;
            if b == b' ' {
                width += state.tw;
            }
        }
        width
    }
}

fn advance_text_position(bytes: &[u8], state: &mut TextState, fonts: &FontMap) {
    let scale = state.font_size * (state.th / 100.0);
    if fonts.is_cid_font(&state.font_name) {
        let mut i = 0;
        while i + 1 < bytes.len() {
            let code = u32::from(bytes[i]) << 8 | u32::from(bytes[i + 1]);
            let w = fonts.char_width(&state.font_name, code);
            state.tm[4] += w / 1000.0 * scale + state.tc;
            i += 2;
        }
    } else {
        for &b in bytes {
            let w = fonts.char_width(&state.font_name, u32::from(b));
            state.tm[4] += w / 1000.0 * scale + state.tc;
            if b == b' ' {
                state.tm[4] += state.tw;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Font resource extraction
// ---------------------------------------------------------------------------

/// Get font name → font object ID mapping from a page's Resources.
fn get_page_font_dict(doc: &Document, page_id: ObjectId) -> HashMap<String, ObjectId> {
    let mut result = HashMap::new();

    let page_dict = match doc.get_object(page_id) {
        Ok(Object::Dictionary(ref d)) => d.clone(),
        _ => return result,
    };

    // Try Resources directly, or inherited from parent.
    let resources = match page_dict.get(b"Resources") {
        Ok(Object::Dictionary(ref d)) => d.clone(),
        Ok(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::Dictionary(ref d)) => d.clone(),
            _ => return result,
        },
        _ => return result,
    };

    let font_dict = match resources.get(b"Font") {
        Ok(Object::Dictionary(ref d)) => d.clone(),
        Ok(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::Dictionary(ref d)) => d.clone(),
            _ => return result,
        },
        _ => return result,
    };

    for (key, value) in font_dict.iter() {
        let name = String::from_utf8_lossy(key).to_string();
        if let Object::Reference(id) = value {
            result.insert(name, *id);
        }
    }

    result
}

/// Build FontInfo from a font dictionary in the document.
fn build_font_info(doc: &Document, font_id: &ObjectId) -> FontInfo {
    let font_dict = match doc.get_object(*font_id) {
        Ok(Object::Dictionary(ref d)) => d.clone(),
        _ => {
            return FontInfo {
                to_unicode: None,
                encoding: FontEncoding::Builtin,
                widths: GlyphWidths::empty(),
                differences_encoding: HashMap::new(),
                is_subset: false,
            }
        }
    };

    // Parse ToUnicode CMap.
    let to_unicode = extract_to_unicode(doc, &font_dict);

    // Determine encoding type.
    let encoding = determine_encoding(doc, &font_dict);

    // Extract glyph widths.
    let widths = extract_widths(doc, &font_dict);

    // Build Differences-based encoding (used when there is no ToUnicode CMap).
    // Only meaningful for single-byte (Builtin) fonts.
    let differences_encoding = if to_unicode.is_none() && matches!(encoding, FontEncoding::Builtin)
    {
        build_font_encoding(doc, &font_dict)
    } else {
        HashMap::new()
    };

    // Detect subset fonts: BaseFont name starts with 6 uppercase ASCII letters
    // followed by '+', e.g. "ABCDEF+CMR10".  Subset fonts only contain the
    // glyphs actually used in the document, so encoding arbitrary replacement
    // text is unsafe (the glyph might not be present in the subset).
    let is_subset = font_dict
        .get(b"BaseFont")
        .ok()
        .and_then(|o| match o {
            Object::Name(ref n) => Some(n.clone()),
            _ => None,
        })
        .map(|name| {
            name.len() > 7 && name[6] == b'+' && name[..6].iter().all(|b| b.is_ascii_uppercase())
        })
        .unwrap_or(false);

    FontInfo {
        to_unicode,
        encoding,
        widths,
        differences_encoding,
        is_subset,
    }
}

/// Extract and parse the ToUnicode CMap from a font dictionary.
fn extract_to_unicode(doc: &Document, font_dict: &lopdf::Dictionary) -> Option<CMap> {
    let to_unicode_obj = font_dict.get(b"ToUnicode").ok()?;

    let stream_data = match to_unicode_obj {
        Object::Reference(id) => {
            let stream = match doc.get_object(*id) {
                Ok(Object::Stream(ref s)) => s.clone(),
                _ => return None,
            };
            let mut s = stream;
            let _ = s.decompress();
            s.content
        }
        Object::Stream(ref s) => {
            let mut s = s.clone();
            let _ = s.decompress();
            s.content
        }
        _ => return None,
    };

    CMap::parse(&stream_data, pdf_font::cmap::load_embedded)
}

/// Determine the encoding type of a font.
fn determine_encoding(doc: &Document, font_dict: &lopdf::Dictionary) -> FontEncoding {
    // Check if it's a Type 0 (CID) font.
    let subtype = font_dict.get(b"Subtype").ok().and_then(|o| match o {
        Object::Name(ref n) => Some(String::from_utf8_lossy(n).to_string()),
        _ => None,
    });

    if subtype.as_deref() == Some("Type0") {
        // Check Encoding.
        let encoding_name = font_dict.get(b"Encoding").ok().and_then(|o| match o {
            Object::Name(ref n) => Some(String::from_utf8_lossy(n).to_string()),
            Object::Reference(id) => doc.get_object(*id).ok().and_then(|o| match o {
                Object::Name(ref n) => Some(String::from_utf8_lossy(n).to_string()),
                _ => None,
            }),
            _ => None,
        });

        return match encoding_name.as_deref() {
            Some("Identity-H") | Some("Identity-V") => FontEncoding::IdentityH,
            _ => FontEncoding::CustomCMap,
        };
    }

    FontEncoding::Builtin
}

/// Extract glyph widths from a font dictionary.
fn extract_widths(doc: &Document, font_dict: &lopdf::Dictionary) -> GlyphWidths {
    let first_char = font_dict
        .get(b"FirstChar")
        .ok()
        .and_then(|o| match o {
            Object::Integer(i) => Some(*i as u32),
            _ => None,
        })
        .unwrap_or(0);

    let widths_array = font_dict.get(b"Widths").ok().and_then(|o| match o {
        Object::Array(ref a) => Some(a.clone()),
        Object::Reference(id) => match doc.get_object(*id).ok() {
            Some(Object::Array(ref a)) => Some(a.clone()),
            _ => None,
        },
        _ => None,
    });

    let widths = match widths_array {
        Some(arr) => arr.iter().map(|o| as_number(o).unwrap_or(1000.0)).collect(),
        None => {
            // For CID fonts, check DescendantFonts for DW (default width).
            let dw = get_cid_default_width(doc, font_dict);
            return GlyphWidths {
                first_char: 0,
                widths: Vec::new(),
                default_width: dw,
            };
        }
    };

    GlyphWidths {
        first_char,
        widths,
        default_width: 1000.0,
    }
}

/// Get default width from CID font's DescendantFonts.
fn get_cid_default_width(doc: &Document, font_dict: &lopdf::Dictionary) -> f64 {
    let descendants = match font_dict.get(b"DescendantFonts").ok() {
        Some(Object::Array(ref arr)) => arr.clone(),
        Some(Object::Reference(id)) => match doc.get_object(*id).ok() {
            Some(Object::Array(ref arr)) => arr.clone(),
            _ => return 1000.0,
        },
        _ => return 1000.0,
    };

    let desc_id = match descendants.first() {
        Some(Object::Reference(id)) => *id,
        _ => return 1000.0,
    };

    let desc_dict = match doc.get_object(desc_id) {
        Ok(Object::Dictionary(ref d)) => d,
        _ => return 1000.0,
    };

    desc_dict
        .get(b"DW")
        .ok()
        .and_then(as_number)
        .unwrap_or(1000.0)
}

// ---------------------------------------------------------------------------
// CMap decoding
// ---------------------------------------------------------------------------

/// Decode bytes to Unicode using a ToUnicode CMap.
fn decode_with_cmap(bytes: &[u8], cmap: &CMap, encoding: &FontEncoding) -> String {
    let mut result = String::new();

    match encoding {
        FontEncoding::IdentityH | FontEncoding::CustomCMap => {
            // 2-byte codes.
            let mut i = 0;
            while i + 1 < bytes.len() {
                let code = u32::from(bytes[i]) << 8 | u32::from(bytes[i + 1]);
                match cmap.lookup_bf_string(code) {
                    Some(BfString::Char(c)) => result.push(c),
                    Some(BfString::String(s)) => result.push_str(&s),
                    None => result.push(char::REPLACEMENT_CHARACTER),
                }
                i += 2;
            }
        }
        FontEncoding::Builtin => {
            // Single-byte codes.
            for &b in bytes {
                match cmap.lookup_bf_string(u32::from(b)) {
                    Some(BfString::Char(c)) => result.push(c),
                    Some(BfString::String(s)) => result.push_str(&s),
                    None => result.push(b as char),
                }
            }
        }
    }

    result
}

/// Fallback: decode PDF string without CMap.
fn decode_pdf_string_fallback(bytes: &[u8]) -> String {
    // Check for UTF-16BE BOM.
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
        String::from_utf16_lossy(&chars)
    } else {
        bytes.iter().map(|&b| b as char).collect()
    }
}

/// Extract 6-element matrix from operands.
fn extract_matrix(operands: &[Object]) -> Option<[f64; 6]> {
    if operands.len() < 6 {
        return None;
    }
    Some([
        as_number(&operands[0])?,
        as_number(&operands[1])?,
        as_number(&operands[2])?,
        as_number(&operands[3])?,
        as_number(&operands[4])?,
        as_number(&operands[5])?,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Document, Object, Stream};

    fn make_doc_with_font_and_content(content: &[u8], to_unicode_data: Option<&[u8]>) -> Document {
        let mut doc = Document::with_version("1.7");

        // Create font dictionary.
        let mut font = dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        };

        if let Some(tu_data) = to_unicode_data {
            let tu_stream = Stream::new(dictionary! {}, tu_data.to_vec());
            let tu_id = doc.add_object(Object::Stream(tu_stream));
            font.set("ToUnicode", Object::Reference(tu_id));
        }

        let font_id = doc.add_object(Object::Dictionary(font));

        let font_resources = dictionary! {
            "F1" => Object::Reference(font_id),
        };
        let resources = dictionary! {
            "Font" => Object::Dictionary(font_resources),
        };

        let content_stream = Stream::new(dictionary! {}, content.to_vec());
        let content_id = doc.add_object(Object::Stream(content_stream));

        let page_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(resources),
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
    fn extract_simple_text_run() {
        let doc =
            make_doc_with_font_and_content(b"BT /F1 12 Tf 100 700 Td (Hello World) Tj ET", None);
        let runs = extract_page_text_runs(&doc, 1).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "Hello World");
        assert_eq!(runs[0].font_name, "F1");
        assert_eq!(runs[0].font_size, 12.0);
    }

    #[test]
    fn extract_multiple_text_runs() {
        let doc = make_doc_with_font_and_content(
            b"BT /F1 12 Tf 100 700 Td (Hello) Tj 0 -20 Td (World) Tj ET",
            None,
        );
        let runs = extract_page_text_runs(&doc, 1).unwrap();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].text, "Hello");
        assert_eq!(runs[1].text, "World");
    }

    #[test]
    fn extract_tj_array_run() {
        let doc = make_doc_with_font_and_content(
            b"BT /F1 12 Tf 100 700 Td [(He) -100 (llo)] TJ ET",
            None,
        );
        let runs = extract_page_text_runs(&doc, 1).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "Hello");
    }

    #[test]
    fn text_run_position_tracking() {
        let doc =
            make_doc_with_font_and_content(b"BT /F1 12 Tf 1 0 0 1 100 700 Tm (Hello) Tj ET", None);
        let runs = extract_page_text_runs(&doc, 1).unwrap();
        assert_eq!(runs.len(), 1);
        assert!((runs[0].x - 100.0).abs() < 1e-6);
        assert!((runs[0].y - 700.0).abs() < 1e-6);
    }

    #[test]
    fn text_run_with_to_unicode_cmap() {
        // A minimal ToUnicode CMap that maps 0x48→H, 0x69→i.
        let cmap = b"/CIDInit /ProcSet findresource begin
12 dict begin
begincmap
/CIDSystemInfo << /Registry (Test) /Ordering (UCS) /Supplement 0 >> def
/CMapName /Test def
1 begincodespacerange
<00> <FF>
endcodespacerange
2 beginbfchar
<48> <0048>
<69> <0069>
endbfchar
endcmap
CMapName currentdict /CMap defineresource pop
end
end";

        let doc = make_doc_with_font_and_content(b"BT /F1 12 Tf 100 700 Td (Hi) Tj ET", Some(cmap));
        let runs = extract_page_text_runs(&doc, 1).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "Hi");
    }

    #[test]
    fn text_run_ops_range() {
        let doc = make_doc_with_font_and_content(b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET", None);
        let runs = extract_page_text_runs(&doc, 1).unwrap();
        assert_eq!(runs.len(), 1);
        // Ops: BT(0), Tf(1), Td(2), Tj(3), ET(4)
        assert_eq!(runs[0].ops_range, 3..4);
    }

    #[test]
    fn font_map_empty_fallback() {
        let fonts = FontMap::empty();
        let text = fonts.decode_string("F1", b"Hello");
        assert_eq!(text, "Hello");
    }

    #[test]
    fn text_run_font_change() {
        let mut doc = Document::with_version("1.7");

        let font1 = dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        };
        let font1_id = doc.add_object(Object::Dictionary(font1));

        let font2 = dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Courier",
        };
        let font2_id = doc.add_object(Object::Dictionary(font2));

        let font_resources = dictionary! {
            "F1" => Object::Reference(font1_id),
            "F2" => Object::Reference(font2_id),
        };
        let resources = dictionary! {
            "Font" => Object::Dictionary(font_resources),
        };

        let content = b"BT /F1 12 Tf (Hello) Tj /F2 14 Tf (World) Tj ET";
        let content_stream = Stream::new(dictionary! {}, content.to_vec());
        let content_id = doc.add_object(Object::Stream(content_stream));

        let page_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(resources),
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

        let runs = extract_page_text_runs(&doc, 1).unwrap();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].font_name, "F1");
        assert_eq!(runs[0].font_size, 12.0);
        assert_eq!(runs[1].font_name, "F2");
        assert_eq!(runs[1].font_size, 14.0);
    }

    #[test]
    fn empty_content_no_runs() {
        let doc = make_doc_with_font_and_content(b"q Q", None);
        let runs = extract_page_text_runs(&doc, 1).unwrap();
        assert!(runs.is_empty());
    }
}
