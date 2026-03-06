//! Text extraction with character-level position tracking.
//!
//! Parses content stream text operators (Tj, TJ, Tm, Td, TD, T*, Tc, Tw, Tz, TL, Ts, ', ")
//! to extract text with positional information.

use crate::error::{ExtractError, Result};
use lopdf::content::{Content, Operation};
use lopdf::{Document, Object, ObjectId};

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

/// Extract text blocks from all pages of a document.
pub fn extract_text(doc: &Document) -> Vec<TextBlock> {
    let pages = doc.get_pages();
    let mut blocks = Vec::new();

    for (&page_num, &page_id) in &pages {
        if let Ok(content_bytes) = get_page_content_bytes(doc, page_id) {
            if let Ok(content) = Content::decode(&content_bytes) {
                let page_blocks = extract_blocks_from_ops(&content.operations, page_num);
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

    let content_bytes = get_page_content_bytes(doc, page_id).unwrap_or_default();
    let content = match Content::decode(&content_bytes) {
        Ok(c) => c,
        Err(_) => return Ok(String::new()),
    };

    let blocks = extract_blocks_from_ops(&content.operations, page_num);
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

    let content_bytes = get_page_content_bytes(doc, page_id).unwrap_or_default();
    let content = match Content::decode(&content_bytes) {
        Ok(c) => c,
        Err(_) => return Ok(Vec::new()),
    };

    let chars = extract_chars_from_ops(&content.operations, page_num);
    Ok(chars)
}

/// Get content stream bytes for a page.
fn get_page_content_bytes(doc: &Document, page_id: ObjectId) -> std::result::Result<Vec<u8>, ()> {
    doc.get_page_content(page_id).map_err(|_| ())
}

/// Extract text blocks from a list of operations.
fn extract_blocks_from_ops(ops: &[Operation], page: u32) -> Vec<TextBlock> {
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
                if let Some(text) = extract_string_operand(&op.operands) {
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

                    for item in arr {
                        match item {
                            Object::String(bytes, _) => {
                                let text = decode_pdf_string(bytes);
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

                if let Some(text) = extract_string_operand(&op.operands) {
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

                    if let Some(text) = extract_string_operand(&op.operands[2..]) {
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
fn extract_chars_from_ops(ops: &[Operation], page: u32) -> Vec<PositionedChar> {
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
                if let Some(text) = extract_string_operand(&op.operands) {
                    let char_w = state.font_size * APPROX_CHAR_WIDTH * (state.th / 100.0);
                    for ch in text.chars() {
                        let x = state.tm[4];
                        let y = state.tm[5];
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
                    for item in arr {
                        match item {
                            Object::String(bytes, _) => {
                                let text = decode_pdf_string(bytes);
                                for ch in text.chars() {
                                    let x = state.tm[4];
                                    let y = state.tm[5];
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

                if let Some(text) = extract_string_operand(&op.operands) {
                    let char_w = state.font_size * APPROX_CHAR_WIDTH * (state.th / 100.0);
                    for ch in text.chars() {
                        let x = state.tm[4];
                        let y = state.tm[5];
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

                    if let Some(text) = extract_string_operand(&op.operands[2..]) {
                        let char_w = state.font_size * APPROX_CHAR_WIDTH * (state.th / 100.0);
                        for ch in text.chars() {
                            let x = state.tm[4];
                            let y = state.tm[5];
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

/// Extract the first string operand from a list of operands.
fn extract_string_operand(operands: &[Object]) -> Option<String> {
    for op in operands {
        if let Object::String(bytes, _) = op {
            return Some(decode_pdf_string(bytes));
        }
    }
    None
}

/// Decode a PDF string to a Rust String.
///
/// Handles both UTF-16BE (BOM-prefixed) and PDFDocEncoding.
fn decode_pdf_string(bytes: &[u8]) -> String {
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
        // PDFDocEncoding: first 127 chars are ASCII, rest are Latin-1.
        bytes.iter().map(|&b| b as char).collect()
    }
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
