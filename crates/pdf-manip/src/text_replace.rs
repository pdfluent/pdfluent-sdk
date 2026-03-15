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

    // Try single-run matching first (fast path).
    let matches = find_matching_runs(&runs, search);
    if !matches.is_empty() {
        let mut new_editor = editor;
        let mut offset: i64 = 0;
        let count = matches.len();

        for m in &matches {
            let run = &runs[m.run_index];
            let adjusted_start = (run.ops_range.start as i64 + offset) as usize;

            let op = match new_editor.operations().get(adjusted_start) {
                Some(op) => op.clone(),
                None => continue,
            };

            let new_ops = build_replacement_ops(&op, search, replacement, &run.font_name, fonts)?;
            let ops_count_diff = new_ops.len() as i64 - 1;

            new_editor.replace_operation(adjusted_start, new_ops);
            offset += ops_count_diff;
        }

        write_editor_to_page(doc, page_num, &new_editor)?;
        return Ok(count);
    }

    // Fall back to cross-run matching for text split across Tj/TJ operators.
    let cross_matches = find_cross_run_matches(&runs, search);
    if cross_matches.is_empty() {
        return Ok(0);
    }

    let mut new_editor = editor;
    let count = cross_matches.len();

    for cm in &cross_matches {
        apply_cross_run_replacement(&mut new_editor, &runs, cm, search, replacement, fonts)?;
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
// Cross-run matching (text split across multiple Tj/TJ operators)
// ---------------------------------------------------------------------------

/// A match that spans multiple consecutive same-font text runs.
struct CrossRunMatch {
    /// First run index (inclusive).
    run_start: usize,
    /// Last run index (exclusive).
    run_end: usize,
}

/// Find matches that span multiple consecutive same-font text runs.
///
/// Groups consecutive runs with the same font name, concatenates their text,
/// and searches for the pattern. Only returns matches that span 2+ runs
/// (single-run matches are handled by `find_matching_runs`).
fn find_cross_run_matches(runs: &[TextRun], search: &str) -> Vec<CrossRunMatch> {
    let mut matches = Vec::new();
    if runs.len() < 2 {
        return matches;
    }

    let mut i = 0;
    while i < runs.len() {
        let font = &runs[i].font_name;
        let group_start = i;
        let mut combined = runs[i].text.clone();
        i += 1;

        while i < runs.len() && runs[i].font_name == *font {
            combined.push_str(&runs[i].text);
            i += 1;
        }

        // Only consider multi-run groups.
        if i - group_start >= 2 && combined.contains(search) {
            matches.push(CrossRunMatch {
                run_start: group_start,
                run_end: i,
            });
        }
    }

    matches
}

/// Apply a cross-run replacement: put full replacement text in the first
/// run's operator and empty out subsequent runs' operators.
fn apply_cross_run_replacement(
    editor: &mut crate::content_editor::ContentEditor,
    runs: &[TextRun],
    cm: &CrossRunMatch,
    search: &str,
    replacement: &str,
    fonts: &FontMap,
) -> Result<()> {
    let group_runs = &runs[cm.run_start..cm.run_end];
    let font_name = &group_runs[0].font_name;

    // Combine text from all runs in the group.
    let combined: String = group_runs.iter().map(|r| r.text.as_str()).collect();
    let new_text = combined.replace(search, replacement);

    // Encode the full replacement text.
    let new_bytes = encode_text_for_font(font_name, &new_text, fonts)?;

    // Put the full text in the first run's text-showing operator.
    let first_op_idx = group_runs[0].ops_range.start;
    if let Some(op) = editor.operations().get(first_op_idx).cloned() {
        let new_op = match op.operator.as_str() {
            "TJ" => {
                // Preserve as TJ with single string element.
                Operation::new(
                    "TJ",
                    vec![Object::Array(vec![Object::String(
                        new_bytes,
                        lopdf::StringFormat::Literal,
                    )])],
                )
            }
            _ => Operation::new(
                "Tj",
                vec![Object::String(new_bytes, lopdf::StringFormat::Literal)],
            ),
        };
        editor.replace_operation(first_op_idx, vec![new_op]);
    }

    // Empty out subsequent runs' text-showing operators.
    for run in &group_runs[1..] {
        let op_idx = run.ops_range.start;
        if let Some(op) = editor.operations().get(op_idx).cloned() {
            let empty_op = make_empty_text_op(&op);
            editor.replace_operation(op_idx, vec![empty_op]);
        }
    }

    Ok(())
}

/// Create an empty version of a text-showing operator (preserves operator type).
fn make_empty_text_op(op: &Operation) -> Operation {
    match op.operator.as_str() {
        "TJ" => Operation::new(
            "TJ",
            vec![Object::Array(vec![Object::String(
                vec![],
                lopdf::StringFormat::Literal,
            )])],
        ),
        "\"" => {
            let mut operands = op.operands.clone();
            if operands.len() >= 3 {
                operands[2] = Object::String(vec![], lopdf::StringFormat::Literal);
            }
            Operation::new("\"", operands)
        }
        // Tj, '
        _ => Operation::new(
            &op.operator,
            vec![Object::String(vec![], lopdf::StringFormat::Literal)],
        ),
    }
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
/// Priority:
/// 1. ToUnicode CMap reverse map (authoritative).
/// 2. Differences/Encoding-based reverse map (for fonts without ToUnicode
///    but with explicit Encoding in the PDF dict, e.g. WinAnsiEncoding).
/// 3. Latin-1 fallback for full (non-subset) fonts only.  Subset fonts
///    (BaseFont prefix like "ABCDEF+") have unknown glyph inventories,
///    so we return an error to avoid silently writing unrenderable bytes.
fn encode_text_for_font(font_name: &str, text: &str, fonts: &FontMap) -> Result<Vec<u8>> {
    if fonts.is_cid_font(font_name) {
        return encode_cid_text(font_name, text, fonts);
    }

    // Reverse map covers both ToUnicode CMap entries and Differences-based
    // encoding entries (built in FontMap::build_reverse_map).
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

    // No reverse map available (font has neither ToUnicode nor an explicit
    // Encoding dict).  Refuse for two classes of fonts where Latin-1 is unsafe:
    //
    // 1. Subset fonts ("ABCDEF+" prefix): only the glyphs used in the source
    //    document are embedded; writing arbitrary bytes may produce unrenderable
    //    glyphs and a FAIL on round-trip verification.
    //
    // 2. Symbolic fonts (FontDescriptor.Flags bit 3): these use the font's
    //    built-in encoding, which may differ arbitrarily from StandardEncoding
    //    (the PDF default for fonts without an Encoding entry).  A Latin-1
    //    encoding of, e.g., '_' as 0x5F would fail round-trip verification if
    //    the font maps 0x5F to a different glyph. Fixes #455.
    if fonts.is_subset_font(font_name) {
        return Err(ManipError::Other(format!(
            "font '{}' is a subset with no known encoding — cannot safely encode replacement text",
            font_name
        )));
    }
    if fonts.is_symbolic_font(font_name) {
        return Err(ManipError::Other(format!(
            "font '{}' is symbolic with no known encoding — Latin-1 fallback unsafe",
            font_name
        )));
    }

    // Latin-1 fallback for full fonts without explicit Encoding.
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

    #[test]
    fn replace_cross_run_split_tj() {
        // Text "January" split across three Tj operators.
        let mut doc = make_doc_with_text(b"BT /F1 12 Tf 100 700 Td (Jan) Tj (u) Tj (ary) Tj ET");
        let fonts = FontMap::from_page(&doc, 1).unwrap();
        let count = replace_text(&mut doc, 1, "January", "Februar", &fonts).unwrap();
        assert_eq!(count, 1);

        // Verify: the combined text should now contain "Februar".
        let editor = editor_for_page(&doc, 1).unwrap();
        let runs = extract_text_runs(&editor, &fonts);
        let combined: String = runs.iter().map(|r| r.text.as_str()).collect();
        assert!(
            combined.contains("Februar"),
            "expected 'Februar' in '{combined}'"
        );
    }

    #[test]
    fn replace_cross_run_with_positioning() {
        // Text "Hello" split across two Tj ops with a Td positioning op in between.
        let mut doc =
            make_doc_with_text(b"BT /F1 12 Tf 100 700 Td (Hel) Tj 0.5 0 Td (lo World) Tj ET");
        let fonts = FontMap::from_page(&doc, 1).unwrap();
        let count = replace_text(&mut doc, 1, "Hello", "Hallo", &fonts).unwrap();
        assert_eq!(count, 1);

        let editor = editor_for_page(&doc, 1).unwrap();
        let runs = extract_text_runs(&editor, &fonts);
        let combined: String = runs.iter().map(|r| r.text.as_str()).collect();
        assert!(
            combined.contains("Hallo"),
            "expected 'Hallo' in '{combined}'"
        );
    }

    #[test]
    fn replace_cross_run_single_run_takes_priority() {
        // When the match is within a single run, use the fast path.
        let mut doc = make_doc_with_text(b"BT /F1 12 Tf 100 700 Td (Hello World) Tj ET");
        let fonts = FontMap::from_page(&doc, 1).unwrap();
        let count = replace_text(&mut doc, 1, "Hello", "Hallo", &fonts).unwrap();
        assert_eq!(count, 1);

        let editor = editor_for_page(&doc, 1).unwrap();
        let runs = extract_text_runs(&editor, &fonts);
        assert_eq!(runs[0].text, "Hallo World");
    }
}
