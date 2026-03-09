//! Search-and-redact: find text patterns and permanently redact them.
//!
//! Combines text extraction (positioned characters) with content stream
//! surgery to both overlay and remove matched text from the PDF.

use crate::error::{RedactError, Result};
use crate::redact::{RedactionArea, Redactor};
use lopdf::Document;
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

    if indices_to_remove.is_empty() {
        return Ok(0);
    }

    indices_to_remove.sort_unstable();
    indices_to_remove.dedup();

    // Remove ops in reverse order to preserve indices.
    let mut new_editor = editor;
    for &idx in indices_to_remove.iter().rev() {
        new_editor.remove_range(idx..idx + 1);
    }

    let removed = indices_to_remove.len();
    pdf_manip::content_editor::write_editor_to_page(doc, page_num, &new_editor)
        .map_err(|e| RedactError::Other(format!("write content: {e}")))?;

    Ok(removed)
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
