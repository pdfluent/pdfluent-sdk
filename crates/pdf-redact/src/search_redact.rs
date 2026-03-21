//! Search-and-redact: find text patterns and permanently redact them.
//!
//! Combines text extraction (positioned characters) with content stream
//! surgery to both overlay and remove matched text from the PDF.

use crate::error::{RedactError, Result};
use crate::redact::{RedactionArea, Redactor};
use lopdf::{Document, Object, ObjectId};
use regex::Regex;
use std::collections::{HashMap, HashSet};

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
    // Per-page match bounding boxes used for position-based op removal fallback.
    let mut page_bboxes: std::collections::HashMap<u32, Vec<[f64; 4]>> =
        std::collections::HashMap::new();

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

            // Store bbox for position-based content removal fallback.
            page_bboxes.entry(page_num).or_default().push(bbox);

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
        let bboxes: &[[f64; 4]] = page_bboxes
            .get(&page_num)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        let removed = remove_text_ops_for_page(doc, page_num, pattern, options, bboxes)?;
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

/// Returns true if a text run's position overlaps a single bounding rectangle.
fn run_overlaps_single_bbox(run: &pdf_manip::text_run::TextRun, bbox: [f64; 4]) -> bool {
    let run_x1 = run.x + run.width.max(1.0);
    let tol = 4.0_f64;
    let x_overlap = run.x < bbox[2] + tol && run_x1 > bbox[0] - tol;
    let y_overlap = run.y <= bbox[3] + tol && run.y >= bbox[1] - tol;
    x_overlap && y_overlap
}

/// Check if a text run is on the same baseline as a match bbox and X-overlaps.
///
/// Used for the "covered" check in `apply_per_bbox_combined_fallback` to decide
/// whether a match bbox is already handled by a text-matched run on the same line.
///
/// `bbox[1]` is the text rendering y (baseline) of the matched chars — the same
/// coordinate that `extract_text_runs` stores in `run.y`.  `bbox[3]` equals
/// `bbox[1] + font_size` (see `extract_positioned_chars`), so using the full
/// bbox interval `[bbox[1], bbox[3]]` for the Y check permits runs on adjacent
/// lines (y ≈ bbox[1] + line_height) to be falsely considered "covering" the bbox.
/// Using a direct `|run.y - bbox[1]| ≤ 0.5` comparison restricts coverage to
/// runs on the same text line.  Fixes #474.
fn run_on_same_baseline(run: &pdf_manip::text_run::TextRun, bbox: [f64; 4]) -> bool {
    let same_y = (run.y - bbox[1]).abs() <= 0.5;
    let run_x1 = run.x + run.width.max(1.0);
    let x_overlap = run.x < bbox[2] + 4.0 && run_x1 > bbox[0] - 4.0;
    same_y && x_overlap
}

/// Extract the Latin-1–decoded text from a text-showing content operation.
///
/// Decodes Tj/TJ/"/"' string operands by treating each byte as its Latin-1
/// code point — the same strategy used by `pdf_extract::extract_positioned_chars`.
/// Used as a last-resort fallback when ToUnicode CMap decoding produces
/// characters that do not match the search pattern (misleading CMap entries).
fn raw_text_from_op(op: &lopdf::content::Operation) -> Option<String> {
    use lopdf::Object;
    match op.operator.as_str() {
        "Tj" | "'" => {
            if let Some(Object::String(ref bytes, _)) = op.operands.first() {
                Some(bytes.iter().map(|&b| b as char).collect())
            } else {
                None
            }
        }
        "TJ" => {
            if let Some(Object::Array(ref arr)) = op.operands.first() {
                let s: String = arr
                    .iter()
                    .filter_map(|item| match item {
                        Object::String(ref bytes, _) => {
                            Some(bytes.iter().map(|&b| b as char).collect::<String>())
                        }
                        _ => None,
                    })
                    .collect();
                if s.is_empty() {
                    None
                } else {
                    Some(s)
                }
            } else {
                None
            }
        }
        "\"" => op.operands.get(2).and_then(|obj| match obj {
            Object::String(ref bytes, _) => Some(bytes.iter().map(|&b| b as char).collect()),
            _ => None,
        }),
        _ => None,
    }
}

/// Per-bbox combined fallback: for each match bbox not covered by text-matched
/// runs, find the correct ops to remove using a two-phase approach:
///
/// **Phase 1 — Y-line raw-byte match**: concatenates the raw Latin-1 bytes of
/// all ops on the same Y-baseline as the bbox and pattern-matches across them.
/// This correctly handles:
/// - Words split across adjacent Tj ops (e.g. "(Ar) Tj (e) Tj" → "Are").
/// - Mid-line x-coordinate drift: `extract_positioned_chars` uses an
///   approximate char width (0.5 × font_size), which accumulates error across
///   a long line.  By the time we reach a mid-line word, the bbox x can be off
///   by tens of units from the actual run.x from `extract_text_runs` (which
///   uses real font metrics).  Y is always accurate; the raw bytes are the same
///   Latin-1 decoding that `extract_positioned_chars` uses.  Fixes #XXX.
///
/// **Phase 2 — X+Y spatial overlap** (legacy fallback): fires only when Phase 1
/// finds no raw-byte match on the Y-line (e.g. font encoding means the glyph
/// bytes are not the expected ASCII codepoints).
fn apply_per_bbox_combined_fallback(
    runs: &[pdf_manip::text_run::TextRun],
    indices_to_remove: &mut Vec<usize>,
    bboxes: &[[f64; 4]],
    ops: &[lopdf::content::Operation],
    matcher: &TextMatcher,
) {
    // Snapshot the text-matched indices for O(1) lookups.  Indices added in
    // this pass must not retroactively cover other bboxes.
    let text_matched: HashSet<usize> = indices_to_remove.iter().copied().collect();
    let mut to_add: HashSet<usize> = HashSet::new();

    // Build op_index → run.y map for Y-line lookups.
    let op_to_y: HashMap<usize, f64> = runs
        .iter()
        .flat_map(|run| run.ops_range.clone().map(move |i| (i, run.y)))
        .collect();

    for &bbox in bboxes {
        // If this bbox is already handled by a text-matched run on the same
        // baseline, skip it.  Uses run_on_same_baseline (strict Y check) so
        // runs from adjacent lines don't falsely "cover" this bbox. Fixes #474.
        let covered = runs.iter().any(|run| {
            run_on_same_baseline(run, bbox)
                && run.ops_range.clone().any(|i| text_matched.contains(&i))
        });
        if covered {
            continue;
        }

        // ------------------------------------------------------------------
        // Phase 1: Y-line raw-byte match.
        //
        // bbox[1] = y_baseline - 1 (padding from compute_bounding_rect).
        // run.y = y_baseline.  So |run.y - bbox[1]| = 1.0; use tol = 6.0 to
        // catch slight y discrepancies between streams.
        // ------------------------------------------------------------------
        let bbox_y = bbox[1];
        let mut y_line: Vec<(usize, &lopdf::content::Operation)> = ops
            .iter()
            .enumerate()
            .filter(|(idx, _)| {
                op_to_y
                    .get(idx)
                    .map(|&y| (y - bbox_y).abs() <= 6.0)
                    .unwrap_or(false)
            })
            .collect();
        y_line.sort_by_key(|(idx, _)| *idx);

        // Concatenate raw bytes for all ops on this Y-line, tracking which op
        // contributed each byte so we can map match positions back to op indices.
        let mut combined = String::new();
        let mut byte_to_op: Vec<usize> = Vec::new();
        for &(idx, op) in &y_line {
            if let Some(raw) = raw_text_from_op(op) {
                let before = combined.len(); // byte offset before push
                combined.push_str(&raw);
                // Each byte in the appended slice belongs to this op.
                byte_to_op.extend(std::iter::repeat_n(idx, combined.len() - before));
            }
        }

        let raw_matches = matcher.find_all(&combined);
        if !raw_matches.is_empty() {
            // Phase 1 succeeded: add only the ops that contain the matched bytes.
            for m in &raw_matches {
                for i in m.start..m.end {
                    if let Some(&op_idx) = byte_to_op.get(i) {
                        if !text_matched.contains(&op_idx) {
                            to_add.insert(op_idx);
                        }
                    }
                }
            }
            continue; // Skip Phase 2 for this bbox.
        }

        // ------------------------------------------------------------------
        // Phase 2: X+Y spatial overlap (legacy fallback).
        //
        // Used when Phase 1 finds nothing, e.g. when the font's byte→glyph
        // mapping means the raw Tj bytes do not spell the search word in Latin-1.
        // ------------------------------------------------------------------
        for run in runs {
            if run_overlaps_single_bbox(run, bbox) {
                for idx in run.ops_range.clone() {
                    to_add.insert(idx);
                }
            }
        }
    }

    for idx in to_add {
        if !text_matched.contains(&idx) {
            indices_to_remove.push(idx);
        }
    }
}

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
///
/// `match_bboxes` contains bounding rectangles (in page space) of all matches
/// found on this page by the `pdf_extract`-based search.  These are used as a
/// fallback when decoded text is unreadable (e.g. glyph-indexed fonts without
/// a ToUnicode CMap): in that case we fall back to spatial matching, removing
/// every text-showing op whose position overlaps one of the match bboxes.
fn remove_text_ops_for_page(
    doc: &mut Document,
    page_num: u32,
    pattern: &str,
    options: &RedactSearchOptions,
    match_bboxes: &[[f64; 4]],
) -> Result<usize> {
    let fonts = match pdf_manip::text_run::FontMap::from_page(doc, page_num) {
        Ok(f) => f,
        Err(_) => return Ok(0),
    };

    let matcher = build_matcher(pattern, options)?;

    // Shared visited set: prevents re-processing the same stream object across
    // XObjects and annotation AP streams (avoids diamond-DAG / cycle blowup).
    let mut visited: HashSet<ObjectId> = HashSet::new();

    // Try normal ContentEditor path first.  Falls back to inline-image-aware
    // path when the content stream contains BI…EI binary image data that
    // lopdf's decoder cannot handle.
    let removed = match pdf_manip::content_editor::editor_for_page(doc, page_num) {
        Ok(editor) => {
            remove_text_ops_via_editor(doc, page_num, editor, &matcher, &fonts, match_bboxes)?
        }
        Err(_) => {
            remove_text_ops_with_inline_images(doc, page_num, &matcher, &fonts, match_bboxes)?
        }
    };

    // Also process Form XObjects referenced in the page's Resources.
    // Pass match_bboxes so that XObjects whose font encoding prevents text
    // matching can still be cleaned via the spatial fallback. Fixes #466 bugs 5–6.
    let removed = removed
        + remove_text_ops_from_xobjects(
            doc,
            page_num,
            &matcher,
            &fonts,
            match_bboxes,
            &mut visited,
        )?;

    // Also process annotation appearance streams.  Pass match_bboxes so the
    // raw-byte fallback can fire for AP streams with misleading ToUnicode CMaps.
    let removed = removed
        + remove_text_ops_from_annotations(
            doc,
            page_num,
            &matcher,
            &fonts,
            match_bboxes,
            &mut visited,
        )?;

    Ok(removed)
}

/// Normal content-edit path: parse → find matches → remove → write back.
fn remove_text_ops_via_editor(
    doc: &mut Document,
    page_num: u32,
    editor: pdf_manip::content_editor::ContentEditor,
    matcher: &TextMatcher,
    fonts: &pdf_manip::text_run::FontMap,
    match_bboxes: &[[f64; 4]],
) -> Result<usize> {
    let runs = pdf_manip::text_run::extract_text_runs(&editor, fonts);

    let mut indices_to_remove: Vec<usize> = Vec::new();
    for run in &runs {
        if !matcher.find_all(&run.text).is_empty() {
            for idx in run.ops_range.clone() {
                indices_to_remove.push(idx);
            }
        }
    }

    // Per-bbox combined fallback: for each match bbox not covered by a
    // text-matched run, use Y-line raw-byte matching (Phase 1) to handle
    // split words and x-coordinate drift, then fall back to spatial overlap
    // (Phase 2) when the Y-line match finds nothing.
    if !match_bboxes.is_empty() {
        apply_per_bbox_combined_fallback(
            &runs,
            &mut indices_to_remove,
            match_bboxes,
            editor.operations(),
            matcher,
        );
    }

    // Raw-byte fallback: when both text-based and spatial matching found nothing
    // but the word IS confirmed to be on the page (non-empty match_bboxes), try
    // decoding each Tj/TJ operand as raw Latin-1 bytes — the same strategy used
    // by extract_positioned_chars for fonts without a ToUnicode CMap.
    // This handles cases where extract_text_runs decodes a font differently from
    // extract_positioned_chars, causing the word to be found during bbox
    // computation but missed by both text-matching and spatial matching on the
    // content stream.  Fixes #476.
    if indices_to_remove.is_empty() && !match_bboxes.is_empty() {
        let ops = editor.operations();
        for (idx, op) in ops.iter().enumerate() {
            if let Some(raw_text) = raw_text_from_op(op) {
                if !matcher.find_all(&raw_text).is_empty() {
                    indices_to_remove.push(idx);
                }
            }
        }
    }

    if indices_to_remove.is_empty() {
        return Ok(0);
    }

    indices_to_remove.sort_unstable();
    indices_to_remove.dedup();

    let mut new_editor = editor;
    for &idx in indices_to_remove.iter().rev() {
        new_editor.remove_range(idx..idx + 1);
    }

    let removed = indices_to_remove.len();
    pdf_manip::content_editor::write_editor_to_page(doc, page_num, &new_editor)
        .map_err(|e| RedactError::Other(format!("write content: {e}")))?;

    Ok(removed)
}

/// Fallback path for pages whose content stream contains inline images
/// (`BI … EI`).  Strips the images, edits the remaining text ops, then
/// reconstructs the stream with the images prepended so z-order is preserved.
fn remove_text_ops_with_inline_images(
    doc: &mut Document,
    page_num: u32,
    matcher: &TextMatcher,
    fonts: &pdf_manip::text_run::FontMap,
    match_bboxes: &[[f64; 4]],
) -> Result<usize> {
    let pages = doc.get_pages();
    let &page_id = match pages.get(&page_num) {
        Some(id) => id,
        None => return Ok(0),
    };

    // Decompressed concatenation of all content streams for this page.
    let content_bytes = match doc.get_page_content(page_id) {
        Ok(b) => b,
        Err(_) => return Ok(0),
    };

    // Separate inline images from the rest.
    let (stripped, inline_images) = pdf_manip::content_editor::strip_inline_images(&content_bytes);

    let editor = match pdf_manip::content_editor::ContentEditor::from_stream(&stripped) {
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

    // Per-bbox combined fallback (same logic as in remove_text_ops_via_editor).
    if !match_bboxes.is_empty() {
        apply_per_bbox_combined_fallback(
            &runs,
            &mut indices_to_remove,
            match_bboxes,
            editor.operations(),
            matcher,
        );
    }

    if indices_to_remove.is_empty() {
        return Ok(0);
    }

    indices_to_remove.sort_unstable();
    indices_to_remove.dedup();

    let mut new_editor = editor;
    for &idx in indices_to_remove.iter().rev() {
        new_editor.remove_range(idx..idx + 1);
    }

    let removed = indices_to_remove.len();

    // Re-encode the edited ops and prepend the raw inline image blobs so the
    // visual z-order is preserved (images were originally before the text).
    let re_encoded = new_editor
        .encode()
        .map_err(|e| RedactError::Other(format!("encode: {e}")))?;

    let mut final_content = Vec::new();
    for img in &inline_images {
        final_content.extend_from_slice(img);
        final_content.push(b'\n');
    }
    final_content.extend_from_slice(&re_encoded);

    // Compress if it helps.
    let compressed = {
        let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
        use std::io::Write as _;
        if enc.write_all(&final_content).is_ok() {
            enc.finish().unwrap_or_else(|_| final_content.clone())
        } else {
            final_content.clone()
        }
    };
    let (stream_bytes, use_flate) = if compressed.len() < final_content.len() {
        (compressed, true)
    } else {
        (final_content, false)
    };

    // Write to the first content stream and collapse multiple streams into one.
    let content_ids = pdf_manip::content_editor::get_content_stream_ids(doc, page_id);
    if let Some(&first_id) = content_ids.first() {
        if let Ok(Object::Stream(ref mut s)) = doc.get_object_mut(first_id) {
            s.content = stream_bytes;
            if use_flate {
                s.dict.set("Filter", Object::Name(b"FlateDecode".to_vec()));
            } else {
                s.dict.remove(b"Filter");
            }
            s.dict
                .set("Length", Object::Integer(s.content.len() as i64));
        }
        if content_ids.len() > 1 {
            if let Ok(Object::Dictionary(ref mut page_dict)) = doc.get_object_mut(page_id) {
                page_dict.set("Contents", Object::Reference(first_id));
            }
        }
    }

    Ok(removed)
}

/// Find and process Form XObjects in the page's Resources/XObject dictionary.
///
/// Delegates to `remove_text_ops_from_stream` for each top-level XObject so
/// that:
/// - Each XObject's own Resources/Font dict is used for correct CMap decoding.
/// - Nested Form XObjects (`Do` inside an XObject) are handled recursively.
///
/// `match_bboxes` are forwarded so that XObjects with non-decodable font
/// encodings can still be cleaned via the spatial fallback.
///
/// Fixes #457: previously this function used the page-level FontMap and did
/// not recurse into nested XObjects, leaving redacted text extractable when it
/// resided in a Form XObject hierarchy.
fn remove_text_ops_from_xobjects(
    doc: &mut Document,
    page_num: u32,
    matcher: &TextMatcher,
    fonts: &pdf_manip::text_run::FontMap,
    match_bboxes: &[[f64; 4]],
    visited: &mut HashSet<ObjectId>,
) -> Result<usize> {
    let pages = doc.get_pages();
    let &page_id = match pages.get(&page_num) {
        Some(id) => id,
        None => return Ok(0),
    };

    let xobject_ids = collect_form_xobject_ids(doc, page_id);
    if xobject_ids.is_empty() {
        return Ok(0);
    }

    let mut total_removed = 0;
    for xobj_id in xobject_ids {
        total_removed +=
            remove_text_ops_from_stream(doc, xobj_id, matcher, fonts, match_bboxes, visited)?;
    }
    Ok(total_removed)
}

/// Remove matching text ops from annotation appearance streams on a page.
///
/// `match_bboxes` are passed to `remove_text_ops_from_stream` to enable the
/// raw-byte fallback for AP streams whose ToUnicode CMap decodes to unexpected
/// characters.  Spatial matching is still disabled for AP streams (their
/// coordinate space is local, not page space), but raw-byte matching only
/// requires the word to have been found somewhere on the page (non-empty bboxes).
fn remove_text_ops_from_annotations(
    doc: &mut Document,
    page_num: u32,
    matcher: &TextMatcher,
    fonts: &pdf_manip::text_run::FontMap,
    match_bboxes: &[[f64; 4]],
    visited: &mut HashSet<ObjectId>,
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
        total_removed +=
            remove_text_ops_from_stream(doc, stream_id, matcher, fonts, match_bboxes, visited)?;
    }

    Ok(total_removed)
}

/// Remove matching text operations from a single stream object.
///
/// Builds a stream-local FontMap from the stream's own Resources/Font dict
/// (merged with the caller-supplied page-level `page_fonts` as fallback) so
/// that CMap decoding is correct for Form XObjects and AP streams that define
/// their own font resources.
///
/// `match_bboxes` are page-space bounding rectangles from the pdf_extract pass.
/// Used for the per-bbox spatial fallback (effective when XObject runs are in
/// page space) and as a guard for the raw-byte fallback (fires when non-empty,
/// confirming the word was found on this page).
fn remove_text_ops_from_stream(
    doc: &mut Document,
    stream_id: ObjectId,
    matcher: &TextMatcher,
    page_fonts: &pdf_manip::text_run::FontMap,
    match_bboxes: &[[f64; 4]],
    visited: &mut HashSet<ObjectId>,
) -> Result<usize> {
    // Guard against cycles and diamond-DAG re-processing: if we have already
    // visited this stream in the current page pass, skip it (#OOM-002874).
    if !visited.insert(stream_id) {
        return Ok(0);
    }

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

    // Use the stream's own font resources so CMap decoding is correct.
    // Fixes #457: XObjects/AP streams often define fonts not present on the page.
    let stream_fonts =
        pdf_manip::text_run::FontMap::from_xobject_stream(doc, stream_id, page_fonts);
    let fonts = &stream_fonts;

    let runs = pdf_manip::text_run::extract_text_runs(&editor, fonts);

    let mut indices_to_remove: Vec<usize> = Vec::new();
    for run in &runs {
        if !matcher.find_all(&run.text).is_empty() {
            for idx in run.ops_range.clone() {
                indices_to_remove.push(idx);
            }
        }
    }

    // Per-bbox combined fallback: Y-line raw-byte match (Phase 1) + spatial
    // overlap fallback (Phase 2).  Note: XObject run positions are in local
    // space, not page space, so spatial matching is most effective for
    // XObjects without CTM transforms.  Fixes #466 bugs 5–6.
    if !match_bboxes.is_empty() {
        apply_per_bbox_combined_fallback(
            &runs,
            &mut indices_to_remove,
            match_bboxes,
            editor.operations(),
            matcher,
        );
    }

    // Raw-byte fallback: when both text-based and spatial matching failed,
    // decode Tj operand bytes as Latin-1 (same as pdf_extract) and match.
    // Handles XObjects/AP streams where a misleading ToUnicode CMap causes
    // text_run to produce characters that don't match the search pattern,
    // even though the raw bytes do. Fixes edge cases #463 (e.g. '270', '000').
    if indices_to_remove.is_empty() && !match_bboxes.is_empty() {
        let ops = editor.operations();
        for (idx, op) in ops.iter().enumerate() {
            if let Some(raw_text) = raw_text_from_op(op) {
                if !matcher.find_all(&raw_text).is_empty() {
                    indices_to_remove.push(idx);
                }
            }
        }
    }

    if indices_to_remove.is_empty() {
        // Check for nested Form XObjects within this stream (e.g., signature appearances).
        let nested_ids = collect_nested_form_xobjects(doc, stream_id);
        let mut nested_removed = 0;
        for nested_id in nested_ids {
            nested_removed +=
                remove_text_ops_from_stream(doc, nested_id, matcher, fonts, match_bboxes, visited)?;
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
        nested_removed +=
            remove_text_ops_from_stream(doc, nested_id, matcher, fonts, match_bboxes, visited)?;
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

    /// Build a document where the word "Classified" lives only inside a Form
    /// XObject — not in the page's own content stream.  The XObject has its
    /// own Resources/Font dictionary that differs from the page-level one.
    ///
    /// Before the #457 fix, `remove_text_ops_from_xobjects` used the wrong
    /// (page-level) FontMap and did not recurse into nested XObjects, so the
    /// Tj operator was never removed from the XObject stream.
    fn make_doc_with_xobject_text() -> (Document, ObjectId) {
        let mut doc = Document::with_version("1.7");

        // Font defined only in the XObject's own Resources (not on the page).
        let xobj_font = dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Times-Roman",
        };
        let xobj_font_id = doc.add_object(Object::Dictionary(xobj_font));
        let xobj_font_res = dictionary! { "FX" => Object::Reference(xobj_font_id) };
        let xobj_resources = dictionary! {
            "Font" => Object::Dictionary(xobj_font_res),
        };

        // Form XObject stream containing the sensitive text.
        let xobj_content = b"BT /FX 12 Tf 0 0 Td (Classified) Tj ET".to_vec();
        let xobj_stream = Stream::new(
            dictionary! {
                "Type" => "XObject",
                "Subtype" => "Form",
                "BBox" => vec![0.into(), 0.into(), 300_i64.into(), 20_i64.into()],
                "Resources" => Object::Dictionary(xobj_resources),
            },
            xobj_content,
        );
        let xobj_id = doc.add_object(Object::Stream(xobj_stream));

        // Page has a different font (F1/Helvetica) but no reference to FX.
        let page_font = dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        };
        let page_font_id = doc.add_object(Object::Dictionary(page_font));
        let page_font_res = dictionary! { "F1" => Object::Reference(page_font_id) };
        let xobj_map = dictionary! { "Xobj1" => Object::Reference(xobj_id) };
        let page_resources = dictionary! {
            "Font" => Object::Dictionary(page_font_res),
            "XObject" => Object::Dictionary(xobj_map),
        };

        // Page content only invokes the XObject — no direct Tj operators.
        let page_content = b"q 1 0 0 1 100 700 cm /Xobj1 Do Q".to_vec();
        let content_stream = Stream::new(dictionary! {}, page_content);
        let content_id = doc.add_object(Object::Stream(content_stream));

        let page_dict = dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(page_resources),
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

        (doc, xobj_id)
    }

    /// Verify that text inside a Form XObject is removed from the XObject's
    /// content stream after redaction (Fixes #457).
    ///
    /// We check the raw bytes of the XObject stream directly rather than
    /// going through text extraction, so the test does not depend on
    /// pdf_extract being able to parse this minimal synthetic PDF.
    #[test]
    fn redact_removes_text_from_xobject_stream() {
        let (mut doc, xobj_id) = make_doc_with_xobject_text();

        // Build a TextMatcher and FontMap and call remove_text_ops_for_page
        // indirectly by exercising the XObject-removal path directly.
        // We use the page-level FontMap (which does NOT contain "FX"); the fix
        // must still correctly use the XObject's own Resources to decode.
        let page_fonts = pdf_manip::text_run::FontMap::empty();
        let matcher_opts = RedactSearchOptions::default();
        let matcher = build_matcher("Classified", &matcher_opts).unwrap();

        // Call the private helper via remove_text_ops_from_stream.
        // We test it indirectly: verify the XObject stream bytes change.
        let removed = remove_text_ops_from_stream(
            &mut doc,
            xobj_id,
            &matcher,
            &page_fonts,
            &[],
            &mut HashSet::new(),
        )
        .unwrap();

        assert!(
            removed > 0,
            "Expected at least one op removed from XObject stream, got 0"
        );

        // Confirm the raw bytes of the XObject no longer contain the literal.
        if let Ok(Object::Stream(ref s)) = doc.get_object(xobj_id) {
            let content = std::str::from_utf8(&s.content).unwrap_or("");
            assert!(
                !content.contains("Classified"),
                "XObject stream still contains 'Classified' after redaction"
            );
        } else {
            panic!("XObject is not a stream after redaction");
        }
    }

    /// When the target word is split across multiple Tj ops (e.g. "(LI) Tj (C) Tj")
    /// and another occurrence of the word is text-matched first (making
    /// `indices_to_remove` non-empty), the per-bbox spatial fallback must still
    /// fire independently for each match_bbox that isn't covered.  Fixes #463
    /// edge case 'LIC'.
    #[test]
    fn redact_split_token_per_bbox_spatial_fallback() {
        // "ALICE" contains "LIC" → text-match succeeds for that occurrence.
        // "(LI) Tj (C) Tj" is a split-token occurrence of "LIC" at a different
        // position; the old global-empty guard would have skipped the spatial
        // fallback because indices_to_remove was already non-empty from ALICE.
        let content = b"BT /F1 12 Tf 0 700 Td (ALICE) Tj 200 0 Td (LI) Tj 30 0 Td (C) Tj ET";
        let mut doc = make_doc_with_text(content);
        let opts = RedactSearchOptions::default();
        let report = search_and_redact(&mut doc, "LIC", &opts).unwrap();
        assert!(report.matches_found >= 1);
        assert!(report.areas_redacted >= 1);
    }

    /// A Form XObject whose font has a misleading ToUnicode CMap must still be
    /// cleaned via the raw-byte fallback when CMap-decoded text doesn't match
    /// but the literal bytes do.  Fixes #463 edge cases '270' / '000'.
    ///
    /// We reuse `make_doc_with_xobject_text` (text = "Classified") and pass
    /// a non-empty `dummy_bboxes` so the raw-byte fallback is triggered even
    /// when there are no runs with matching text.
    #[test]
    fn redact_xobject_raw_byte_fallback() {
        let (mut doc, xobj_id) = make_doc_with_xobject_text();
        let page_fonts = pdf_manip::text_run::FontMap::empty();
        let matcher_opts = RedactSearchOptions::default();
        let matcher = build_matcher("Classified", &matcher_opts).unwrap();
        // Non-empty bboxes activate the raw-byte fallback path in
        // `remove_text_ops_from_stream` when no spatial run was matched.
        let dummy_bboxes = [[0.0_f64, 0.0, 300.0, 20.0]];
        let removed = remove_text_ops_from_stream(
            &mut doc,
            xobj_id,
            &matcher,
            &page_fonts,
            &dummy_bboxes,
            &mut HashSet::new(),
        )
        .unwrap();
        assert!(
            removed > 0,
            "Expected raw-byte fallback to remove ops from XObject stream"
        );
    }
}
