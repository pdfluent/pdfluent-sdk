//! Text replacement in PDF content streams.
//!
//! Find and replace text with correct spacing by re-encoding replacement
//! strings into the original font encoding.

use crate::content_editor::{as_number, editor_for_page, write_editor_to_page};
use crate::error::{ManipError, Result};
use crate::text_run::{extract_text_runs, FontMap, TextRun};
use lopdf::content::Operation;
use lopdf::{Document, Object};

/// Replace all occurrences of `search` with `replacement` in a page's content stream.
///
/// Returns the number of replacements made. The replacement string is encoded
/// using the same font encoding as the original text. Returns an error if the
/// replacement contains characters that cannot be encoded in the font.
pub fn replace_text(
    doc: &mut Document,
    page_num: u32,
    search: &str,
    replacement: &str,
    fonts: &FontMap,
) -> Result<usize> {
    let editor = editor_for_page(doc, page_num)?;
    let runs = extract_text_runs(&editor, fonts);

    // Find runs that contain the search string.
    let matches = find_matching_runs(&runs, search);
    if matches.is_empty() {
        return Ok(0);
    }

    let mut new_editor = editor;
    let mut offset: i64 = 0;
    let count = matches.len();

    for m in &matches {
        let run = &runs[m.run_index];
        let adjusted_start = (run.ops_range.start as i64 + offset) as usize;

        // Get the original operation.
        let op = match new_editor.operations().get(adjusted_start) {
            Some(op) => op.clone(),
            None => continue,
        };

        // Build replacement operation(s).
        let new_ops = build_replacement_ops(&op, search, replacement, &run.font_name, fonts)?;
        let ops_count_diff = new_ops.len() as i64 - 1;

        new_editor.replace_operation(adjusted_start, new_ops);
        offset += ops_count_diff;
    }

    write_editor_to_page(doc, page_num, &new_editor)?;
    Ok(count)
}

/// Replace text across all pages in a document.
///
/// Pages where the replacement text cannot be encoded in the font are
/// silently skipped (e.g. subset fonts missing glyphs for the replacement).
pub fn replace_text_all_pages(
    doc: &mut Document,
    search: &str,
    replacement: &str,
) -> Result<usize> {
    let page_count = doc.get_pages().len() as u32;
    let mut total = 0;

    for page_num in 1..=page_count {
        let fonts = match FontMap::from_page(doc, page_num) {
            Ok(f) => f,
            Err(_) => continue,
        };
        match replace_text(doc, page_num, search, replacement, &fonts) {
            Ok(n) => total += n,
            Err(_) => continue, // skip pages with encoding issues
        }
    }

    Ok(total)
}

// ---------------------------------------------------------------------------
// Match finding
// ---------------------------------------------------------------------------

struct TextMatch {
    run_index: usize,
}

fn find_matching_runs(runs: &[TextRun], search: &str) -> Vec<TextMatch> {
    let mut matches = Vec::new();
    for (i, run) in runs.iter().enumerate() {
        if run.text.contains(search) {
            matches.push(TextMatch { run_index: i });
        }
    }
    matches
}

// ---------------------------------------------------------------------------
// Replacement operation building
// ---------------------------------------------------------------------------

fn build_replacement_ops(
    original_op: &Operation,
    search: &str,
    replacement: &str,
    font_name: &str,
    fonts: &FontMap,
) -> Result<Vec<Operation>> {
    match original_op.operator.as_str() {
        "Tj" => build_tj_replacement(original_op, search, replacement, font_name, fonts),
        "TJ" => build_tj_array_replacement(original_op, search, replacement, font_name, fonts),
        "'" => build_tj_replacement(original_op, search, replacement, font_name, fonts),
        "\"" => {
            // For " operator, the string is the third operand.
            build_quote_replacement(original_op, search, replacement, font_name, fonts)
        }
        _ => Ok(vec![original_op.clone()]),
    }
}

fn build_tj_replacement(
    op: &Operation,
    search: &str,
    replacement: &str,
    font_name: &str,
    fonts: &FontMap,
) -> Result<Vec<Operation>> {
    let bytes = match op.operands.first() {
        Some(Object::String(ref b, _)) => b,
        _ => return Ok(vec![op.clone()]),
    };

    let decoded = fonts.decode_string(font_name, bytes);
    let new_text = decoded.replace(search, replacement);

    // Re-encode the replacement text.
    let new_bytes = encode_text_for_font(font_name, &new_text, fonts)?;

    Ok(vec![Operation::new(
        &op.operator,
        vec![Object::String(new_bytes, lopdf::StringFormat::Literal)],
    )])
}

fn build_tj_array_replacement(
    op: &Operation,
    search: &str,
    replacement: &str,
    font_name: &str,
    fonts: &FontMap,
) -> Result<Vec<Operation>> {
    let arr = match op.operands.first() {
        Some(Object::Array(ref a)) => a,
        _ => return Ok(vec![op.clone()]),
    };

    // Decode the entire TJ array into a single string, tracking segments.
    let mut full_text = String::new();
    let mut segments: Vec<TjSegment> = Vec::new();

    for item in arr {
        match item {
            Object::String(ref bytes, _) => {
                let text = fonts.decode_string(font_name, bytes);
                let start = full_text.len();
                full_text.push_str(&text);
                segments.push(TjSegment::Text {
                    start,
                    end: full_text.len(),
                    original_bytes: bytes.clone(),
                });
            }
            _ => {
                if let Some(adj) = as_number(item) {
                    segments.push(TjSegment::Spacing(adj));
                }
            }
        }
    }

    if !full_text.contains(search) {
        return Ok(vec![op.clone()]);
    }

    // Replace in the combined text.
    let new_text = full_text.replace(search, replacement);

    // Simple approach: encode entire new text as a single Tj.
    // This loses inter-character spacing adjustments but is correct.
    let new_bytes = encode_text_for_font(font_name, &new_text, fonts)?;

    // If the original had spacing adjustments, we try to preserve structure.
    // For simplicity, if lengths match exactly we preserve segments.
    if new_text.len() == full_text.len() && search.len() == replacement.len() {
        // Lengths match — can preserve TJ array structure.
        let new_arr =
            rebuild_tj_array_same_length(&segments, &full_text, &new_text, font_name, fonts)?;
        return Ok(vec![Operation::new("TJ", vec![Object::Array(new_arr)])]);
    }

    // Different lengths — emit as single Tj string.
    Ok(vec![Operation::new(
        "Tj",
        vec![Object::String(new_bytes, lopdf::StringFormat::Literal)],
    )])
}

fn build_quote_replacement(
    op: &Operation,
    search: &str,
    replacement: &str,
    font_name: &str,
    fonts: &FontMap,
) -> Result<Vec<Operation>> {
    if op.operands.len() < 3 {
        return Ok(vec![op.clone()]);
    }

    let bytes = match &op.operands[2] {
        Object::String(ref b, _) => b,
        _ => return Ok(vec![op.clone()]),
    };

    let decoded = fonts.decode_string(font_name, bytes);
    let new_text = decoded.replace(search, replacement);
    let new_bytes = encode_text_for_font(font_name, &new_text, fonts)?;

    Ok(vec![Operation::new(
        "\"",
        vec![
            op.operands[0].clone(),
            op.operands[1].clone(),
            Object::String(new_bytes, lopdf::StringFormat::Literal),
        ],
    )])
}

// ---------------------------------------------------------------------------
// TJ array helpers
// ---------------------------------------------------------------------------

enum TjSegment {
    Text {
        start: usize,
        end: usize,
        #[allow(dead_code)]
        original_bytes: Vec<u8>,
    },
    Spacing(f64),
}

fn rebuild_tj_array_same_length(
    segments: &[TjSegment],
    _old_text: &str,
    new_text: &str,
    font_name: &str,
    fonts: &FontMap,
) -> Result<Vec<Object>> {
    let mut result = Vec::new();

    for seg in segments {
        match seg {
            TjSegment::Text { start, end, .. } => {
                let new_segment = &new_text[*start..*end];
                let new_bytes = encode_text_for_font(font_name, new_segment, fonts)?;
                result.push(Object::String(new_bytes, lopdf::StringFormat::Literal));
            }
            TjSegment::Spacing(adj) => {
                result.push(Object::Real(*adj as f32));
            }
        }
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Text encoding
// ---------------------------------------------------------------------------

/// Encode Unicode text back to PDF string bytes for the given font.
///
/// For builtin (single-byte) fonts without a ToUnicode CMap, we encode
/// as Latin-1 (ISO 8859-1). For fonts with a reverse CMap, we attempt
/// the reverse lookup. Returns an error if characters can't be encoded.
fn encode_text_for_font(font_name: &str, text: &str, fonts: &FontMap) -> Result<Vec<u8>> {
    if fonts.is_cid_font(font_name) {
        return encode_cid_text(font_name, text, fonts);
    }

    // For fonts with a ToUnicode CMap, use the reverse CMap to encode.
    // This respects font-specific Encoding/Differences and subset glyphs.
    let reverse = fonts.build_reverse_map(font_name);
    if !reverse.is_empty() {
        let mut bytes = Vec::with_capacity(text.len());
        for ch in text.chars() {
            if let Some(&code) = reverse.get(&ch) {
                if code <= 0xFF {
                    bytes.push(code as u8);
                } else {
                    return Err(ManipError::Other(format!(
                        "character '{}' (U+{:04X}) maps to code {} which exceeds single-byte range in font '{}'",
                        ch, ch as u32, code, font_name
                    )));
                }
            } else {
                return Err(ManipError::Other(format!(
                    "character '{}' (U+{:04X}) not available in font '{}' (subset or custom encoding)",
                    ch, ch as u32, font_name
                )));
            }
        }
        return Ok(bytes);
    }

    // Fallback: Latin-1 encoding for fonts without a ToUnicode CMap
    // (standard fonts with standard encoding).
    let mut bytes = Vec::with_capacity(text.len());
    for ch in text.chars() {
        let code = ch as u32;
        if code <= 0xFF {
            bytes.push(code as u8);
        } else {
            return Err(ManipError::Other(format!(
                "character '{}' (U+{:04X}) cannot be encoded in font '{}'",
                ch, code, font_name
            )));
        }
    }
    Ok(bytes)
}

fn encode_cid_text(font_name: &str, text: &str, fonts: &FontMap) -> Result<Vec<u8>> {
    // For CID fonts with Identity-H encoding, try to build reverse map
    // from the ToUnicode CMap. If no reverse map is available, attempt
    // direct Unicode → CID mapping.
    let reverse = fonts.build_reverse_map(font_name);
    let mut bytes = Vec::with_capacity(text.len() * 2);

    for ch in text.chars() {
        if let Some(code) = reverse.get(&ch) {
            bytes.push((*code >> 8) as u8);
            bytes.push((*code & 0xFF) as u8);
        } else {
            return Err(ManipError::Other(format!(
                "character '{}' (U+{:04X}) not in font '{}' CMap",
                ch, ch as u32, font_name
            )));
        }
    }

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Document, Object, Stream};

    fn make_doc_with_text(content: &[u8]) -> Document {
        let mut doc = Document::with_version("1.7");

        let font = dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        };
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
    fn replace_simple_same_length() {
        let mut doc = make_doc_with_text(b"BT /F1 12 Tf 100 700 Td (Hello World) Tj ET");
        let fonts = FontMap::from_page(&doc, 1).unwrap();
        let count = replace_text(&mut doc, 1, "Hello", "Hallo", &fonts).unwrap();
        assert_eq!(count, 1);

        // Verify the replacement.
        let editor = editor_for_page(&doc, 1).unwrap();
        let runs = extract_text_runs(&editor, &fonts);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "Hallo World");
    }

    #[test]
    fn replace_different_length() {
        let mut doc = make_doc_with_text(b"BT /F1 12 Tf 100 700 Td (Hello World) Tj ET");
        let fonts = FontMap::from_page(&doc, 1).unwrap();
        let count = replace_text(&mut doc, 1, "World", "Earth!", &fonts).unwrap();
        assert_eq!(count, 1);

        let editor = editor_for_page(&doc, 1).unwrap();
        let runs = extract_text_runs(&editor, &fonts);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "Hello Earth!");
    }

    #[test]
    fn replace_no_match() {
        let mut doc = make_doc_with_text(b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET");
        let fonts = FontMap::from_page(&doc, 1).unwrap();
        let count = replace_text(&mut doc, 1, "Missing", "Replacement", &fonts).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn replace_in_tj_array() {
        let mut doc = make_doc_with_text(b"BT /F1 12 Tf 100 700 Td [(Hel) -100 (lo)] TJ ET");
        let fonts = FontMap::from_page(&doc, 1).unwrap();
        let count = replace_text(&mut doc, 1, "Hello", "Hallo", &fonts).unwrap();
        assert_eq!(count, 1);

        let editor = editor_for_page(&doc, 1).unwrap();
        let runs = extract_text_runs(&editor, &fonts);
        assert_eq!(runs[0].text, "Hallo");
    }

    #[test]
    fn replace_error_on_unencodable_char() {
        let mut doc = make_doc_with_text(b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET");
        let fonts = FontMap::from_page(&doc, 1).unwrap();
        // Try to replace with a character outside Latin-1.
        let result = replace_text(&mut doc, 1, "Hello", "\u{4e16}\u{754c}", &fonts);
        assert!(result.is_err());
    }

    #[test]
    fn replace_all_pages() {
        let mut doc = Document::with_version("1.7");

        let font = dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        };
        let font_id = doc.add_object(Object::Dictionary(font));

        let font_resources = dictionary! {
            "F1" => Object::Reference(font_id),
        };
        let resources = dictionary! {
            "Font" => Object::Dictionary(font_resources),
        };

        // Two pages with same text.
        let mut page_ids = Vec::new();
        for _ in 0..2 {
            let content = b"BT /F1 12 Tf (Hello) Tj ET";
            let content_stream = Stream::new(dictionary! {}, content.to_vec());
            let content_id = doc.add_object(Object::Stream(content_stream));

            let page_dict = dictionary! {
                "Type" => "Page",
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                "Contents" => Object::Reference(content_id),
                "Resources" => Object::Dictionary(resources.clone()),
            };
            let page_id = doc.add_object(Object::Dictionary(page_dict));
            page_ids.push(page_id);
        }

        let pages_dict = dictionary! {
            "Type" => "Pages",
            "Kids" => page_ids.iter().map(|id| Object::Reference(*id)).collect::<Vec<_>>(),
            "Count" => page_ids.len() as i64,
        };
        let pages_id = doc.add_object(Object::Dictionary(pages_dict));

        for &page_id in &page_ids {
            if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
                d.set("Parent", Object::Reference(pages_id));
            }
        }

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        let count = replace_text_all_pages(&mut doc, "Hello", "Hallo").unwrap();
        assert_eq!(count, 2);
    }
}
