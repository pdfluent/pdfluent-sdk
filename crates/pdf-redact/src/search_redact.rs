//! Search-and-redact: find text patterns and permanently redact them.
//!
//! Combines text extraction (positioned characters) with content stream
//! surgery to both overlay and remove matched text from the PDF.

use crate::error::{RedactError, Result};
use crate::redact::{RedactionArea, Redactor};
use lopdf::{Document, Object, ObjectId};
use regex::Regex;

/// Options for search-and-redact operations.
#[derive(Debug, Clone)]
pub struct RedactSearchOptions {
    /// Whether the search is case-sensitive (default: true).
    pub case_sensitive: bool,
    /// Whether the pattern is a regex (default: false).
    pub regex: bool,
    /// Fill color [r, g, b] for redaction overlay (default: black).
    pub fill_color: [f64; 3],
    /// Specific pages to search (None = all pages).
    pub pages: Option<Vec<u32>>,
    /// Optional overlay text (e.g., "[REDACTED]").
    pub overlay_text: Option<String>,
}

impl Default for RedactSearchOptions {
    fn default() -> Self {
        Self {
            case_sensitive: true,
            regex: false,
            fill_color: [0.0, 0.0, 0.0],
            pages: None,
            overlay_text: None,
        }
    }
}

impl RedactSearchOptions {
    /// Create options for an exact case-sensitive search.
    pub fn exact(pattern: &str) -> Self {
        let _ = pattern; // used by caller
        Self::default()
    }

    /// Create options for a case-insensitive search.
    pub fn case_insensitive() -> Self {
        Self {
            case_sensitive: false,
            ..Self::default()
        }
    }

    /// Create options for a regex search.
    pub fn with_regex() -> Self {
        Self {
            regex: true,
            ..Self::default()
        }
    }

    /// Set fill color.
    pub fn fill_color(mut self, r: f64, g: f64, b: f64) -> Self {
        self.fill_color = [r, g, b];
        self
    }

    /// Set specific pages.
    pub fn pages(mut self, pages: Vec<u32>) -> Self {
        self.pages = Some(pages);
        self
    }

    /// Set overlay text.
    pub fn overlay_text(mut self, text: impl Into<String>) -> Self {
        self.overlay_text = Some(text.into());
        self
    }
}

/// Report from a search-and-redact operation.
#[derive(Debug, Clone)]
pub struct SearchRedactReport {
    /// Number of text matches found.
    pub matches_found: usize,
    /// Number of redaction areas applied.
    pub areas_redacted: usize,
    /// Number of content operations removed.
    pub operations_removed: usize,
    /// Number of pages affected.
    pub pages_affected: usize,
    /// Whether metadata was cleaned.
    pub metadata_cleaned: bool,
}

/// Search for text matching a pattern and redact all occurrences.
///
/// This performs two operations:
/// 1. Finds text matches using positioned character extraction
/// 2. Computes bounding rectangles for matches
/// 3. Applies redaction (overlay + content removal) via the Redactor
pub fn search_and_redact(
    doc: &mut Document,
    pattern: &str,
    options: &RedactSearchOptions,
) -> Result<SearchRedactReport> {
    let pages = doc.get_pages();
    let total = pages.len() as u32;

    let page_range: Vec<u32> = match &options.pages {
        Some(ps) => ps.clone(),
        None => (1..=total).collect(),
    };

    // Validate pages.
    for &p in &page_range {
        if p == 0 || p > total {
            return Err(RedactError::PageOutOfRange(p, total));
        }
    }

    // Build the search pattern.
    let matcher = build_matcher(pattern, options)?;

    // Find all matches across pages.
    let mut all_areas: Vec<RedactionArea> = Vec::new();
    let mut total_matches = 0;

    for &page_num in &page_range {
        let chars = match pdf_extract::extract_positioned_chars(doc, page_num) {
            Ok(c) => c,
            Err(_) => continue,
        };

        if chars.is_empty() {
            continue;
        }

        // Build a text string from positioned chars.
        let text: String = chars.iter().map(|c| c.ch).collect();

        // Build a byte-offset-to-char-index map so regex byte offsets can be
        // translated back to indices into `chars`.
        let byte_to_char: Vec<usize> = {
            let mut map = Vec::with_capacity(text.len() + 1);
            for (ci, ch) in text.chars().enumerate() {
                for _ in 0..ch.len_utf8() {
                    map.push(ci);
                }
            }
            map.push(chars.len()); // sentinel for end-of-string
            map
        };

        // Find matches in the text.
        let match_ranges = matcher.find_all(&text);

        for range in &match_ranges {
            total_matches += 1;

            // Convert byte offsets to char indices.
            let char_start = byte_to_char.get(range.start).copied().unwrap_or(0);
            let char_end = byte_to_char.get(range.end).copied().unwrap_or(chars.len());
            if char_start >= chars.len() || char_end > chars.len() || char_start >= char_end {
                continue;
            }

            // Compute bounding rect from the chars in this range.
            let matched_chars = &chars[char_start..char_end];
            if matched_chars.is_empty() {
                continue;
            }

            let bbox = compute_bounding_rect(matched_chars);

            let mut area = RedactionArea::new(page_num, bbox);
            area = area.with_color(
                options.fill_color[0],
                options.fill_color[1],
                options.fill_color[2],
            );
            if let Some(ref overlay) = options.overlay_text {
                area = area.with_overlay(overlay);
            }
            all_areas.push(area);
        }
    }

    if all_areas.is_empty() {
        return Ok(SearchRedactReport {
            matches_found: 0,
            areas_redacted: 0,
            operations_removed: 0,
            pages_affected: 0,
            metadata_cleaned: false,
        });
    }

    // Apply redactions using the existing Redactor.
    let mut redactor = Redactor::new();
    redactor.mark_all(all_areas);
    let report = redactor.apply(doc)?;

    // Additionally, use ContentEditor to surgically remove matching
    // text operations from the content stream.
    let mut extra_ops_removed = 0;
    for &page_num in &page_range {
        let removed = remove_text_ops_for_page(doc, page_num, pattern, options)?;
        extra_ops_removed += removed;
    }

    Ok(SearchRedactReport {
        matches_found: total_matches,
        areas_redacted: report.areas_redacted,
        operations_removed: report.operations_removed + extra_ops_removed,
        pages_affected: report.pages_affected,
        metadata_cleaned: report.metadata_cleaned,
    })
}

// ---------------------------------------------------------------------------
// Pattern matching
// ---------------------------------------------------------------------------

struct TextMatcher {
    regex: Regex,
}

struct MatchRange {
    start: usize,
    end: usize,
}

impl TextMatcher {
    fn find_all(&self, text: &str) -> Vec<MatchRange> {
        self.regex
            .find_iter(text)
            .map(|m| MatchRange {
                start: m.start(),
                end: m.end(),
            })
            .collect()
    }
}

fn build_matcher(pattern: &str, options: &RedactSearchOptions) -> Result<TextMatcher> {
    let regex_pattern = if options.regex {
        if options.case_sensitive {
            pattern.to_string()
        } else {
            format!("(?i){}", pattern)
        }
    } else {
        let escaped = regex::escape(pattern);
        if options.case_sensitive {
            escaped
        } else {
            format!("(?i){}", escaped)
        }
    };

    let regex = Regex::new(&regex_pattern)
        .map_err(|e| RedactError::Other(format!("invalid pattern: {e}")))?;

    Ok(TextMatcher { regex })
}

// ---------------------------------------------------------------------------
// Bounding rectangle computation
// ---------------------------------------------------------------------------

fn compute_bounding_rect(chars: &[pdf_extract::PositionedChar]) -> [f64; 4] {
    let mut x0 = f64::MAX;
    let mut y0 = f64::MAX;
    let mut x1 = f64::MIN;
    let mut y1 = f64::MIN;

    for ch in chars {
        x0 = x0.min(ch.bbox[0]);
        y0 = y0.min(ch.bbox[1]);
        x1 = x1.max(ch.bbox[2]);
        y1 = y1.max(ch.bbox[3]);
    }

    // Add small padding to ensure complete coverage.
    [x0 - 1.0, y0 - 1.0, x1 + 1.0, y1 + 1.0]
}

// ---------------------------------------------------------------------------
// Content stream surgery
// ---------------------------------------------------------------------------

/// Remove text-showing operations whose decoded text matches the pattern.
///
/// Processes both the page's direct content stream and any Form XObjects
/// referenced in the page's Resources.
fn remove_text_ops_for_page(
    doc: &mut Document,
    page_num: u32,
    pattern: &str,
    options: &RedactSearchOptions,
) -> Result<usize> {
    let editor = match pdf_manip::content_editor::editor_for_page(doc, page_num) {
        Ok(e) => e,
        Err(_) => return Ok(0),
    };

    let fonts = match pdf_manip::text_run::FontMap::from_page(doc, page_num) {
        Ok(f) => f,
        Err(_) => return Ok(0),
    };

    let runs = pdf_manip::text_run::extract_text_runs(&editor, &fonts);
    let matcher = build_matcher(pattern, options)?;

    // Collect op indices to remove (text-showing ops that match).
    let mut indices_to_remove: Vec<usize> = Vec::new();
    for run in &runs {
        if !matcher.find_all(&run.text).is_empty() {
            for idx in run.ops_range.clone() {
                indices_to_remove.push(idx);
            }
        }
    }

    let mut removed = 0;

    if !indices_to_remove.is_empty() {
        indices_to_remove.sort_unstable();
        indices_to_remove.dedup();

        // Remove ops in reverse order to preserve indices.
        let mut new_editor = editor;
        for &idx in indices_to_remove.iter().rev() {
            new_editor.remove_range(idx..idx + 1);
        }

        removed = indices_to_remove.len();
        pdf_manip::content_editor::write_editor_to_page(doc, page_num, &new_editor)
            .map_err(|e| RedactError::Other(format!("write content: {e}")))?;
    }

    // Also process Form XObjects referenced in the page's Resources.
    removed += remove_text_ops_from_xobjects(doc, page_num, &matcher, &fonts)?;

    // Also process annotation appearance streams.
    removed += remove_text_ops_from_annotations(doc, page_num, &matcher, &fonts)?;

    Ok(removed)
}

/// Find and process Form XObjects in the page's Resources/XObject dictionary.
fn remove_text_ops_from_xobjects(
    doc: &mut Document,
    page_num: u32,
    matcher: &TextMatcher,
    fonts: &pdf_manip::text_run::FontMap,
) -> Result<usize> {
    let pages = doc.get_pages();
    let &page_id = match pages.get(&page_num) {
        Some(id) => id,
        None => return Ok(0),
    };

    // Collect Form XObject IDs from the page's Resources/XObject dict.
    let xobject_ids = collect_form_xobject_ids(doc, page_id);
    if xobject_ids.is_empty() {
        return Ok(0);
    }

    let mut total_removed = 0;

    for xobj_id in xobject_ids {
        // Decode the XObject's content stream.
        let content_bytes = match doc.get_object(xobj_id) {
            Ok(Object::Stream(ref s)) => {
                let mut stream = s.clone();
                let _ = stream.decompress();
                stream.content.clone()
            }
            _ => continue,
        };

        let editor = match pdf_manip::content_editor::ContentEditor::from_stream(&content_bytes) {
            Ok(e) => e,
            Err(_) => continue,
        };

        let runs = pdf_manip::text_run::extract_text_runs(&editor, fonts);

        let mut indices_to_remove: Vec<usize> = Vec::new();
        for run in &runs {
            if !matcher.find_all(&run.text).is_empty() {
                for idx in run.ops_range.clone() {
                    indices_to_remove.push(idx);
                }
            }
        }

        if indices_to_remove.is_empty() {
            continue;
        }

        indices_to_remove.sort_unstable();
        indices_to_remove.dedup();

        let mut new_editor = editor;
        for &idx in indices_to_remove.iter().rev() {
            new_editor.remove_range(idx..idx + 1);
        }

        total_removed += indices_to_remove.len();

        // Write back the modified content stream.
        let encoded = new_editor
            .encode()
            .map_err(|e| RedactError::Other(format!("encode xobject: {e}")))?;

        if let Ok(Object::Stream(ref mut s)) = doc.get_object_mut(xobj_id) {
            s.dict.remove(b"Filter");
            s.content = encoded;
            s.dict
                .set("Length", Object::Integer(s.content.len() as i64));
        }
    }

    Ok(total_removed)
}

/// Remove matching text ops from annotation appearance streams on a page.
fn remove_text_ops_from_annotations(
    doc: &mut Document,
    page_num: u32,
    matcher: &TextMatcher,
    fonts: &pdf_manip::text_run::FontMap,
) -> Result<usize> {
    let pages = doc.get_pages();
    let &page_id = match pages.get(&page_num) {
        Some(id) => id,
        None => return Ok(0),
    };

    // Collect appearance stream IDs from annotations.
    let ap_stream_ids = collect_annotation_appearance_ids(doc, page_id);
    if ap_stream_ids.is_empty() {
        return Ok(0);
    }

    let mut total_removed = 0;
    for stream_id in ap_stream_ids {
        total_removed += remove_text_ops_from_stream(doc, stream_id, matcher, fonts)?;
    }

    Ok(total_removed)
}

/// Remove matching text operations from a single stream object.
fn remove_text_ops_from_stream(
    doc: &mut Document,
    stream_id: ObjectId,
    matcher: &TextMatcher,
    fonts: &pdf_manip::text_run::FontMap,
) -> Result<usize> {
    let content_bytes = match doc.get_object(stream_id) {
        Ok(Object::Stream(ref s)) => {
            let mut stream = s.clone();
            let _ = stream.decompress();
            stream.content.clone()
        }
        _ => return Ok(0),
    };

    let editor = match pdf_manip::content_editor::ContentEditor::from_stream(&content_bytes) {
        Ok(e) => e,
        Err(_) => return Ok(0),
    };

    let runs = pdf_manip::text_run::extract_text_runs(&editor, fonts);

    let mut indices_to_remove: Vec<usize> = Vec::new();
    for run in &runs {
        if !matcher.find_all(&run.text).is_empty() {
            for idx in run.ops_range.clone() {
                indices_to_remove.push(idx);
            }
        }
    }

    if indices_to_remove.is_empty() {
        // Check for nested Form XObjects within this stream (e.g., signature appearances).
        let nested_ids = collect_nested_form_xobjects(doc, stream_id);
        let mut nested_removed = 0;
        for nested_id in nested_ids {
            nested_removed += remove_text_ops_from_stream(doc, nested_id, matcher, fonts)?;
        }
        return Ok(nested_removed);
    }

    indices_to_remove.sort_unstable();
    indices_to_remove.dedup();

    let mut new_editor = editor;
    for &idx in indices_to_remove.iter().rev() {
        new_editor.remove_range(idx..idx + 1);
    }

    let removed = indices_to_remove.len();

    let encoded = new_editor
        .encode()
        .map_err(|e| RedactError::Other(format!("encode annotation stream: {e}")))?;

    if let Ok(Object::Stream(ref mut s)) = doc.get_object_mut(stream_id) {
        s.dict.remove(b"Filter");
        s.content = encoded;
        s.dict
            .set("Length", Object::Integer(s.content.len() as i64));
    }

    // Also recurse into nested Form XObjects.
    let nested_ids = collect_nested_form_xobjects(doc, stream_id);
    let mut nested_removed = removed;
    for nested_id in nested_ids {
        nested_removed += remove_text_ops_from_stream(doc, nested_id, matcher, fonts)?;
    }

    Ok(nested_removed)
}

/// Collect appearance stream IDs from page annotations.
fn collect_annotation_appearance_ids(doc: &Document, page_id: ObjectId) -> Vec<ObjectId> {
    let mut result = Vec::new();

    let page_dict = match doc.get_object(page_id) {
        Ok(Object::Dictionary(ref d)) => d.clone(),
        _ => return result,
    };

    let annots = match page_dict.get(b"Annots") {
        Ok(Object::Array(ref arr)) => arr.clone(),
        Ok(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::Array(ref arr)) => arr.clone(),
            _ => return result,
        },
        _ => return result,
    };

    for annot_ref in &annots {
        let annot_id = match annot_ref {
            Object::Reference(id) => *id,
            _ => continue,
        };

        let annot_dict = match doc.get_object(annot_id) {
            Ok(Object::Dictionary(ref d)) => d.clone(),
            _ => continue,
        };

        // Get the AP (appearance) dictionary.
        let ap_dict = match annot_dict.get(b"AP") {
            Ok(Object::Dictionary(ref d)) => d.clone(),
            Ok(Object::Reference(id)) => match doc.get_object(*id) {
                Ok(Object::Dictionary(ref d)) => d.clone(),
                _ => continue,
            },
            _ => continue,
        };

        // Get the N (normal appearance) stream.
        match ap_dict.get(b"N") {
            Ok(Object::Reference(id)) => {
                result.push(*id);
            }
            Ok(Object::Dictionary(ref d)) => {
                // Some annotations have a dict of appearance states.
                for (_key, val) in d.iter() {
                    if let Object::Reference(id) = val {
                        result.push(*id);
                    }
                }
            }
            _ => {}
        }
    }

    result
}

/// Collect Form XObject IDs referenced within a stream's Resources or content.
fn collect_nested_form_xobjects(doc: &Document, stream_id: ObjectId) -> Vec<ObjectId> {
    let mut result = Vec::new();

    let stream_dict = match doc.get_object(stream_id) {
        Ok(Object::Stream(ref s)) => s.dict.clone(),
        _ => return result,
    };

    // Check the stream's own Resources/XObject dict.
    let resources = match stream_dict.get(b"Resources") {
        Ok(Object::Dictionary(ref d)) => d.clone(),
        Ok(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::Dictionary(ref d)) => d.clone(),
            _ => return result,
        },
        _ => return result,
    };

    let xobject_dict = match resources.get(b"XObject") {
        Ok(Object::Dictionary(ref d)) => d.clone(),
        Ok(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::Dictionary(ref d)) => d.clone(),
            _ => return result,
        },
        _ => return result,
    };

    for (_key, value) in xobject_dict.iter() {
        let obj_id = match value {
            Object::Reference(id) => *id,
            _ => continue,
        };
        // Only include Form XObjects, not images.
        if let Ok(Object::Stream(ref s)) = doc.get_object(obj_id) {
            let is_form = s
                .dict
                .get(b"Subtype")
                .ok()
                .and_then(|v| match v {
                    Object::Name(ref n) => Some(n.as_slice()),
                    _ => None,
                })
                .map(|n| n == b"Form")
                .unwrap_or(false);
            if is_form {
                result.push(obj_id);
            }
        }
    }

    result
}

/// Collect ObjectIds of Form XObjects from a page's Resources/XObject dictionary.
fn collect_form_xobject_ids(doc: &Document, page_id: ObjectId) -> Vec<ObjectId> {
    let mut result = Vec::new();

    let page_dict = match doc.get_object(page_id) {
        Ok(Object::Dictionary(ref d)) => d.clone(),
        _ => return result,
    };

    let resources = match page_dict.get(b"Resources") {
        Ok(Object::Dictionary(ref d)) => d.clone(),
        Ok(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::Dictionary(ref d)) => d.clone(),
            _ => return result,
        },
        _ => return result,
    };

    let xobject_dict = match resources.get(b"XObject") {
        Ok(Object::Dictionary(ref d)) => d.clone(),
        Ok(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::Dictionary(ref d)) => d.clone(),
            _ => return result,
        },
        _ => return result,
    };

    for (_key, value) in xobject_dict.iter() {
        let obj_id = match value {
            Object::Reference(id) => *id,
            _ => continue,
        };

        // Check if it's a Form XObject (Subtype == Form).
        if let Ok(Object::Stream(ref s)) = doc.get_object(obj_id) {
            let is_form = s
                .dict
                .get(b"Subtype")
                .ok()
                .and_then(|v| match v {
                    Object::Name(ref n) => Some(n.as_slice()),
                    _ => None,
                })
                .map(|n| n == b"Form")
                .unwrap_or(false);
            if is_form {
                result.push(obj_id);
            }
        }
    }

    result
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

        let info = dictionary! {
            "Title" => Object::String(b"Test".to_vec(), lopdf::StringFormat::Literal),
        };
        let info_id = doc.add_object(Object::Dictionary(info));
        doc.trailer.set("Info", Object::Reference(info_id));

        let catalog = dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        doc
    }

    #[test]
    fn search_and_redact_exact_match() {
        let mut doc = make_doc_with_text(b"BT /F1 12 Tf 100 700 Td (Secret Data) Tj ET");
        let opts = RedactSearchOptions::default();
        let report = search_and_redact(&mut doc, "Secret", &opts).unwrap();
        assert!(report.matches_found >= 1);
        assert!(report.areas_redacted >= 1);
    }

    #[test]
    fn search_and_redact_no_match() {
        let mut doc = make_doc_with_text(b"BT /F1 12 Tf 100 700 Td (Hello World) Tj ET");
        let opts = RedactSearchOptions::default();
        let report = search_and_redact(&mut doc, "Missing", &opts).unwrap();
        assert_eq!(report.matches_found, 0);
        assert_eq!(report.areas_redacted, 0);
    }

    #[test]
    fn search_and_redact_case_insensitive() {
        let mut doc = make_doc_with_text(b"BT /F1 12 Tf 100 700 Td (Secret Data) Tj ET");
        let opts = RedactSearchOptions::case_insensitive();
        let report = search_and_redact(&mut doc, "secret", &opts).unwrap();
        assert!(report.matches_found >= 1);
    }

    #[test]
    fn search_and_redact_regex() {
        let mut doc = make_doc_with_text(b"BT /F1 12 Tf 100 700 Td (SSN 123-45-6789) Tj ET");
        let opts = RedactSearchOptions::with_regex();
        let report = search_and_redact(&mut doc, r"\d{3}-\d{2}-\d{4}", &opts).unwrap();
        assert!(report.matches_found >= 1);
    }

    #[test]
    fn search_and_redact_with_overlay() {
        let mut doc = make_doc_with_text(b"BT /F1 12 Tf 100 700 Td (Confidential) Tj ET");
        let opts = RedactSearchOptions::default().overlay_text("[REDACTED]");
        let report = search_and_redact(&mut doc, "Confidential", &opts).unwrap();
        assert!(report.matches_found >= 1);
    }

    #[test]
    fn search_and_redact_specific_pages() {
        let mut doc = make_doc_with_text(b"BT /F1 12 Tf 100 700 Td (Secret) Tj ET");
        let opts = RedactSearchOptions::default().pages(vec![1]);
        let report = search_and_redact(&mut doc, "Secret", &opts).unwrap();
        assert!(report.matches_found >= 1);
    }

    #[test]
    fn search_and_redact_page_out_of_range() {
        let mut doc = make_doc_with_text(b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET");
        let opts = RedactSearchOptions::default().pages(vec![5]);
        let result = search_and_redact(&mut doc, "Hello", &opts);
        assert!(result.is_err());
    }

    #[test]
    fn search_and_redact_cleans_metadata() {
        let mut doc = make_doc_with_text(b"BT /F1 12 Tf 100 700 Td (Secret) Tj ET");
        let opts = RedactSearchOptions::default();
        let report = search_and_redact(&mut doc, "Secret", &opts).unwrap();
        assert!(report.metadata_cleaned);
        assert!(doc.trailer.get(b"Info").is_err());
    }

    #[test]
    fn search_and_redact_custom_color() {
        let mut doc = make_doc_with_text(b"BT /F1 12 Tf 100 700 Td (Secret) Tj ET");
        let opts = RedactSearchOptions::default().fill_color(1.0, 0.0, 0.0);
        let report = search_and_redact(&mut doc, "Secret", &opts).unwrap();
        assert!(report.matches_found >= 1);
    }
}
