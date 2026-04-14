//! Text extraction via a custom Device implementation.

use kurbo::{Affine, BezPath};
use pdf_render::pdf_interpret::cmap::BfString;
use pdf_render::pdf_interpret::font::Glyph;
use pdf_render::pdf_interpret::{
    BlendMode, ClipPath, Device, GlyphDrawMode, Image, Paint, PathDrawMode, SoftMask,
};
use std::cmp::Ordering;

/// Y tolerance for grouping spans into horizontal bands.
const BAND_Y_TOLERANCE: f64 = 5.0;
/// Minimum horizontal gap treated as a column gutter (adaptive fallback).
const COLUMN_GAP_THRESHOLD_MIN: f64 = 10.0;
/// Maximum adaptive column gap threshold.
const COLUMN_GAP_THRESHOLD_MAX: f64 = 40.0;
/// Multiplier applied to median inter-word gap to derive column threshold.
const COLUMN_GAP_MEDIAN_MULTIPLIER: f64 = 3.0;
/// Fallback column gap threshold when median cannot be computed.
const COLUMN_GAP_THRESHOLD_FALLBACK: f64 = 20.0;
/// Maximum drift allowed when matching gutters across neighboring bands.
const COLUMN_GAP_MATCH_TOLERANCE: f64 = 12.0;
/// Minimum number of gapped bands required before we enable column mode.
const MIN_COLUMN_GAPPED_BANDS: usize = 3;
/// Minimum fraction of bands in a region that must expose the shared gutters.
const MIN_COLUMN_GAP_SUPPORT: f64 = 0.80;
/// Minimum fraction of non-empty column slices that must look like prose.
const MIN_DENSE_SLICE_RATIO: f64 = 0.35;

/// A single text span at a specific position.
#[derive(Debug, Clone)]
pub struct TextSpan {
    /// The extracted text.
    pub text: String,
    /// X position in user space.
    pub x: f64,
    /// Y position in user space.
    pub y: f64,
    /// Approximate bounding-box width in user space.
    pub width: f64,
    /// Approximate bounding-box height in user space.
    pub height: f64,
    /// Font size (approximate, from transform).
    pub font_size: f64,
}

impl TextSpan {
    fn right(&self) -> f64 {
        self.x + self.width.max(self.estimated_width())
    }

    fn estimated_width(&self) -> f64 {
        let char_count = self.text.chars().count() as f64;
        if char_count <= 0.0 {
            self.font_size * 0.5
        } else {
            self.font_size * 0.5 * char_count
        }
    }
}

/// A block of text (grouped by reading order).
#[derive(Debug, Clone)]
pub struct TextBlock {
    /// Spans within this block, sorted by position.
    pub spans: Vec<TextSpan>,
}

impl TextBlock {
    /// Concatenate all spans into a single string.
    ///
    /// Spans that are close together are joined without a separator;
    /// a space is inserted when the gap between spans exceeds half
    /// the average character width.
    pub fn text(&self) -> String {
        if self.spans.is_empty() {
            return String::new();
        }
        let mut result = self.spans[0].text.clone();
        for pair in self.spans.windows(2) {
            let prev = &pair[0];
            let curr = &pair[1];
            let expected_end = prev.right();
            let gap = curr.x - expected_end;
            if gap > prev.font_size * 0.25 {
                result.push(' ');
            }
            result.push_str(&curr.text);
        }
        result
    }
}

#[derive(Debug, Clone)]
struct TextBand {
    y: f64,
    spans: Vec<TextSpan>,
}

impl TextBand {
    fn new(span: TextSpan) -> Self {
        Self {
            y: span.y,
            spans: vec![span],
        }
    }

    fn sort_spans(&mut self) {
        self.spans.sort_by(|a, b| {
            a.x.partial_cmp(&b.x)
                .unwrap_or(Ordering::Equal)
                .then_with(|| b.y.partial_cmp(&a.y).unwrap_or(Ordering::Equal))
        });
    }

    fn row_block(&self) -> TextBlock {
        let mut spans = self.spans.clone();
        spans.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(Ordering::Equal));
        TextBlock { spans }
    }

    fn left(&self) -> f64 {
        self.spans
            .iter()
            .map(|span| span.x)
            .fold(f64::INFINITY, f64::min)
    }

    fn right(&self) -> f64 {
        self.spans
            .iter()
            .map(TextSpan::right)
            .fold(f64::NEG_INFINITY, f64::max)
    }

    fn width(&self) -> f64 {
        (self.right() - self.left()).max(0.0)
    }

    fn gap_midpoints(&self, column_gap_threshold: f64) -> Vec<f64> {
        self.gaps(column_gap_threshold)
            .into_iter()
            .map(|gap| (gap.start + gap.end) * 0.5)
            .collect()
    }

    fn gaps(&self, column_gap_threshold: f64) -> Vec<BandGap> {
        if self.spans.len() < 2 {
            return Vec::new();
        }

        let mut spans = self.spans.clone();
        spans.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(Ordering::Equal));

        let mut gaps = Vec::new();
        let mut prev_right = spans[0].right();
        for span in spans.iter().skip(1) {
            let gap = span.x - prev_right;
            if gap >= column_gap_threshold {
                gaps.push(BandGap {
                    start: prev_right,
                    end: span.x,
                });
            }
            prev_right = prev_right.max(span.right());
        }

        gaps
    }

    fn split_by_boundaries(&self, boundaries: &[f64]) -> Vec<Vec<TextSpan>> {
        let mut columns = vec![Vec::new(); boundaries.len() + 1];
        for span in &self.spans {
            let center_x = span.x + span.width.max(span.estimated_width()) * 0.5;
            let column_idx = boundaries
                .iter()
                .position(|boundary| center_x < *boundary)
                .unwrap_or(boundaries.len());
            columns[column_idx].push(span.clone());
        }

        for spans in &mut columns {
            spans.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(Ordering::Equal));
        }

        columns
    }

    fn fits_single_column(
        &self,
        boundaries: &[f64],
        region_left: f64,
        region_right: f64,
    ) -> Option<usize> {
        let mut column_idx: Option<usize> = None;
        for span in &self.spans {
            let left = span.x;
            let right = span.right();
            if boundaries
                .iter()
                .any(|boundary| left < *boundary && right > *boundary)
            {
                return None;
            }

            let center_x = left + (right - left) * 0.5;
            let idx = boundaries
                .iter()
                .position(|boundary| center_x < *boundary)
                .unwrap_or(boundaries.len());
            match column_idx {
                Some(existing) if existing != idx => return None,
                Some(_) => {}
                None => column_idx = Some(idx),
            }
        }
        let idx = column_idx?;
        let mut edges = Vec::with_capacity(boundaries.len() + 2);
        edges.push(region_left);
        edges.extend_from_slice(boundaries);
        edges.push(region_right);

        let column_width = (edges[idx + 1] - edges[idx]).max(0.0);
        if column_width <= 0.0 || self.width() > column_width * 0.8 {
            return None;
        }

        Some(idx)
    }
}

#[derive(Debug, Clone, Copy)]
struct BandGap {
    start: f64,
    end: f64,
}

/// A Device implementation that captures text from draw_glyph calls.
pub(crate) struct TextExtractionDevice {
    spans: Vec<TextSpan>,
    last_y: f64,
    last_end_x: f64,
}

impl Default for TextExtractionDevice {
    fn default() -> Self {
        Self::new()
    }
}

impl TextExtractionDevice {
    /// Create a new text extraction device.
    pub fn new() -> Self {
        Self {
            spans: Vec::new(),
            last_y: f64::NEG_INFINITY,
            last_end_x: f64::NEG_INFINITY,
        }
    }

    /// Consume the device and return extracted text as a single string.
    pub fn into_text(self) -> String {
        let blocks = group_spans_into_blocks(self.spans);
        let lines: Vec<String> = blocks.iter().map(|b| b.text()).collect();
        let stitched = stitch_hyphenated_lines(&lines);
        normalize_text_output(&stitched)
    }

    /// Consume the device and return text blocks.
    pub fn into_blocks(self) -> Vec<TextBlock> {
        group_spans_into_blocks(self.spans)
    }

    /// Consume the device and return raw spans.
    #[allow(dead_code)]
    pub(crate) fn into_spans(self) -> Vec<TextSpan> {
        self.spans
    }
}

impl Device<'_> for TextExtractionDevice {
    fn set_soft_mask(&mut self, _: Option<SoftMask<'_>>) {}
    fn set_blend_mode(&mut self, _: BlendMode) {}
    fn draw_path(&mut self, _: &BezPath, _: Affine, _: &Paint<'_>, _: &PathDrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'_>>, _: BlendMode) {}
    fn draw_image(&mut self, _: Image<'_, '_>, _: Affine) {}
    fn pop_clip_path(&mut self) {}
    fn pop_transparency_group(&mut self) {}

    fn draw_glyph(
        &mut self,
        glyph: &Glyph<'_>,
        transform: Affine,
        glyph_transform: Affine,
        _paint: &Paint<'_>,
        _draw_mode: &GlyphDrawMode,
    ) {
        let text = match glyph.as_unicode() {
            Some(BfString::Char(c)) => c.to_string(),
            Some(BfString::String(s)) => s,
            None => return,
        };

        let composed = transform * glyph_transform;
        let coeffs = composed.as_coeffs();
        let x = coeffs[4];
        let y = coeffs[5];
        let glyph_scale = (coeffs[0].powi(2) + coeffs[1].powi(2)).sqrt().abs();
        let font_size = glyph_scale * 1000.0;
        let glyph_width = estimate_glyph_width(glyph, font_size).max(font_size * 0.25);
        let glyph_end_x = x + glyph_width;

        let same_line = (y - self.last_y).abs() <= font_size.max(BAND_Y_TOLERANCE) * 0.35;
        let gap = x - self.last_end_x;
        let adjacent = same_line && gap >= -font_size * 0.25 && gap < font_size * 0.5;

        if adjacent {
            if let Some(last) = self.spans.last_mut() {
                // Inject a space when the horizontal gap between the previous
                // glyph and this one is wide enough to indicate an inter-word
                // break (typical PDFs emit `[(foo) -200 (bar)] TJ` or two
                // separate show-text ops without a literal space glyph). The
                // 0.15 em threshold matches what pdftotext / MuPDF use: well
                // below normal letter spacing but comfortably above intra-word
                // kerning. Skip if either side already ends/starts with space.
                let glue_needed = gap > font_size * 0.15
                    && !last.text.ends_with(' ')
                    && !text.starts_with(' ');
                if glue_needed {
                    last.text.push(' ');
                }
                last.text.push_str(&text);
                last.width = last.width.max(glyph_end_x - last.x);
                last.height = last.height.max(font_size);
                self.last_y = y;
                self.last_end_x = glyph_end_x;
                return;
            }
        }

        self.last_y = y;
        self.last_end_x = glyph_end_x;

        self.spans.push(TextSpan {
            text,
            x,
            y,
            width: glyph_width,
            height: font_size,
            font_size,
        });
    }
}

fn estimate_glyph_width(glyph: &Glyph<'_>, font_size: f64) -> f64 {
    match glyph {
        Glyph::Outline(outline) => outline
            .advance_width()
            .map(|width| width as f64 / 1000.0 * font_size)
            .unwrap_or(font_size * 0.5),
        Glyph::Type3(_) => font_size * 0.5,
    }
}

/// Compute an adaptive column gap threshold from a set of bands.
///
/// Collects all positive inter-span gaps within each band, computes the
/// median, and returns `COLUMN_GAP_MEDIAN_MULTIPLIER × median`, clamped to
/// `[COLUMN_GAP_THRESHOLD_MIN, COLUMN_GAP_THRESHOLD_MAX]`.  Falls back to
/// `COLUMN_GAP_THRESHOLD_FALLBACK` when there are no measurable gaps.
fn compute_adaptive_column_gap(bands: &[TextBand]) -> f64 {
    let mut all_gaps: Vec<f64> = Vec::new();

    for band in bands {
        if band.spans.len() < 2 {
            continue;
        }
        let mut sorted = band.spans.clone();
        sorted.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(Ordering::Equal));
        let mut prev_right = sorted[0].right();
        for span in sorted.iter().skip(1) {
            let gap = span.x - prev_right;
            if gap > 0.0 {
                all_gaps.push(gap);
            }
            prev_right = prev_right.max(span.right());
        }
    }

    if all_gaps.is_empty() {
        return COLUMN_GAP_THRESHOLD_FALLBACK;
    }

    all_gaps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let mid = all_gaps.len() / 2;
    let median = if all_gaps.len() % 2 == 0 {
        (all_gaps[mid - 1] + all_gaps[mid]) * 0.5
    } else {
        all_gaps[mid]
    };

    (median * COLUMN_GAP_MEDIAN_MULTIPLIER)
        .clamp(COLUMN_GAP_THRESHOLD_MIN, COLUMN_GAP_THRESHOLD_MAX)
}

/// Group spans into reading-order blocks, using column-aware reordering when
/// a contiguous region repeatedly exposes the same gutters.
fn group_spans_into_blocks(spans: Vec<TextSpan>) -> Vec<TextBlock> {
    let bands = group_spans_into_bands(spans);
    if bands.is_empty() {
        return Vec::new();
    }

    let column_gap_threshold = compute_adaptive_column_gap(&bands);

    let mut blocks = Vec::new();
    let mut idx = 0;

    while idx < bands.len() {
        let gap_midpoints = bands[idx].gap_midpoints(column_gap_threshold);
        if gap_midpoints.is_empty() {
            blocks.push(bands[idx].row_block());
            idx += 1;
            continue;
        }

        let mut boundaries = gap_midpoints.clone();
        let mut band_indices = vec![idx];
        let mut gapped_band_count = 1usize;
        let mut region_left = bands[idx].left();
        let mut region_right = bands[idx].right();
        let mut next_idx = idx + 1;

        while next_idx < bands.len() {
            let next_band = &bands[next_idx];
            let next_gap_midpoints = next_band.gap_midpoints(column_gap_threshold);
            if next_gap_midpoints.is_empty() {
                if next_band
                    .fits_single_column(&boundaries, region_left, region_right)
                    .is_some()
                {
                    band_indices.push(next_idx);
                    next_idx += 1;
                    continue;
                }
                break;
            }

            if !boundaries_match(&boundaries, &next_gap_midpoints) {
                break;
            }

            update_boundaries(&mut boundaries, &next_gap_midpoints, gapped_band_count);
            gapped_band_count += 1;
            band_indices.push(next_idx);
            region_left = region_left.min(next_band.left());
            region_right = region_right.max(next_band.right());
            next_idx += 1;
        }

        if region_is_columnar(&bands, &band_indices, &boundaries, gapped_band_count) {
            append_column_region_blocks(&bands, &band_indices, &boundaries, &mut blocks);
            idx = next_idx;
        } else {
            blocks.push(bands[idx].row_block());
            idx += 1;
        }
    }

    blocks
}

fn group_spans_into_bands(mut spans: Vec<TextSpan>) -> Vec<TextBand> {
    if spans.is_empty() {
        return Vec::new();
    }

    spans.sort_by(|a, b| {
        b.y.partial_cmp(&a.y)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.x.partial_cmp(&b.x).unwrap_or(Ordering::Equal))
    });

    let mut bands: Vec<TextBand> = Vec::new();

    for span in spans {
        let tolerance = span.height.max(BAND_Y_TOLERANCE) * 0.5;
        if let Some(band) = bands
            .iter_mut()
            .find(|band| (band.y - span.y).abs() <= tolerance)
        {
            let span_count = band.spans.len() as f64;
            band.y = (band.y * span_count + span.y) / (span_count + 1.0);
            band.spans.push(span);
        } else {
            bands.push(TextBand::new(span));
        }
    }

    for band in &mut bands {
        band.sort_spans();
    }

    bands.sort_by(|a, b| b.y.partial_cmp(&a.y).unwrap_or(Ordering::Equal));
    bands
}

fn boundaries_match(boundaries: &[f64], gap_midpoints: &[f64]) -> bool {
    boundaries.len() == gap_midpoints.len()
        && boundaries
            .iter()
            .zip(gap_midpoints)
            .all(|(lhs, rhs)| (lhs - rhs).abs() <= COLUMN_GAP_MATCH_TOLERANCE)
}

fn update_boundaries(boundaries: &mut [f64], gap_midpoints: &[f64], seen_gapped_bands: usize) {
    for (boundary, midpoint) in boundaries.iter_mut().zip(gap_midpoints) {
        *boundary =
            (*boundary * seen_gapped_bands as f64 + midpoint) / (seen_gapped_bands as f64 + 1.0);
    }
}

fn region_is_columnar(
    bands: &[TextBand],
    band_indices: &[usize],
    boundaries: &[f64],
    gapped_band_count: usize,
) -> bool {
    if boundaries.is_empty()
        || gapped_band_count < MIN_COLUMN_GAPPED_BANDS
        || band_indices.is_empty()
        || (gapped_band_count as f64 / band_indices.len() as f64) < MIN_COLUMN_GAP_SUPPORT
    {
        return false;
    }

    let mut non_empty_slices = 0usize;
    let mut dense_slices = 0usize;
    let mut slices_per_column = vec![0usize; boundaries.len() + 1];

    for &band_idx in band_indices {
        let slices = bands[band_idx].split_by_boundaries(boundaries);
        for (column_idx, slice) in slices.iter().enumerate() {
            if slice.is_empty() {
                continue;
            }

            non_empty_slices += 1;
            slices_per_column[column_idx] += 1;

            let char_count = slice
                .iter()
                .map(|span| span.text.chars().count())
                .sum::<usize>();
            if slice.len() >= 2 || char_count >= 8 {
                dense_slices += 1;
            }
        }
    }

    if non_empty_slices < boundaries.len() + 2 {
        return false;
    }

    if slices_per_column.contains(&0) {
        return false;
    }

    (dense_slices as f64 / non_empty_slices as f64) >= MIN_DENSE_SLICE_RATIO
}

fn append_column_region_blocks(
    bands: &[TextBand],
    band_indices: &[usize],
    boundaries: &[f64],
    blocks: &mut Vec<TextBlock>,
) {
    let column_count = boundaries.len() + 1;
    let mut column_bands = vec![Vec::<TextSpan>::new(); column_count];

    for &band_idx in band_indices {
        let slices = bands[band_idx].split_by_boundaries(boundaries);
        for (column_idx, slice) in slices.into_iter().enumerate() {
            if slice.is_empty() {
                continue;
            }
            column_bands[column_idx].push(TextSpan {
                text: String::new(),
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
                font_size: 0.0,
            });
            let marker_idx = column_bands[column_idx].len() - 1;
            column_bands[column_idx][marker_idx] = TextSpan {
                text: String::new(),
                x: f64::NEG_INFINITY,
                y: bands[band_idx].y,
                width: 0.0,
                height: 0.0,
                font_size: 0.0,
            };
            column_bands[column_idx].extend(slice);
        }
    }

    for spans in column_bands {
        let mut current: Vec<TextSpan> = Vec::new();
        for span in spans {
            if span.x == f64::NEG_INFINITY {
                if !current.is_empty() {
                    current.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(Ordering::Equal));
                    blocks.push(TextBlock {
                        spans: std::mem::take(&mut current),
                    });
                }
                continue;
            }
            current.push(span);
        }
        if !current.is_empty() {
            current.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(Ordering::Equal));
            blocks.push(TextBlock { spans: current });
        }
    }
}

/// Join per-block lines, stitching end-of-line hyphenated word-wraps the
/// way pdftotext / MuPDF / PDFBox do.
///
/// Trigger conditions (all must hold):
/// 1. Previous line ends with `-` preceded by an alphabetic character.
/// 2. The alphabetic suffix before the `-` has >= 3 characters.
/// 3. The next line (trimmed) starts with an ASCII lowercase letter.
/// 4. The lowercase prefix of the next line has >= 3 characters.
///
/// When triggered, the trailing `-` is removed and the two halves are
/// concatenated without a space or newline.
///
/// This avoids false positives on compound words ("real-time"), bullet
/// lists, numeric ranges ("42-"), and short fragments.
fn stitch_hyphenated_lines(lines: &[String]) -> String {
    let mut out = String::new();
    for (idx, line) in lines.iter().enumerate() {
        if idx == 0 {
            out.push_str(line);
            continue;
        }

        let next_trimmed = line.trim_start();

        // Check the accumulated output for end-of-line hyphen pattern
        let should_merge = is_hyphen_wrap_candidate(&out, next_trimmed);

        if should_merge {
            out.pop(); // drop the trailing '-'
            out.push_str(next_trimmed);
        } else {
            out.push('\n');
            out.push_str(line);
        }
    }
    out
}

/// Check if the accumulated text ends with a hyphen-wrap pattern and the
/// continuation is a valid merge target.
fn is_hyphen_wrap_candidate(accumulated: &str, next_trimmed: &str) -> bool {
    // Must end with '-'
    if !accumulated.ends_with('-') {
        return false;
    }

    // Character before '-' must be alphabetic
    let before_hyphen = accumulated.chars().rev().nth(1);
    if !before_hyphen.is_some_and(|c| c.is_alphabetic()) {
        return false;
    }

    // Count consecutive alphabetic chars before the '-' (the word fragment)
    let alpha_prefix_len = accumulated
        .chars()
        .rev()
        .skip(1) // skip the '-'
        .take_while(|c| c.is_alphabetic())
        .count();
    if alpha_prefix_len < 3 {
        return false;
    }

    // Next line must start with lowercase ASCII
    let first_next = next_trimmed.chars().next();
    if !first_next.is_some_and(|c| c.is_ascii_lowercase()) {
        return false;
    }

    // Count consecutive lowercase chars at start of next line
    let next_alpha_len = next_trimmed
        .chars()
        .take_while(|c| c.is_ascii_lowercase())
        .count();
    if next_alpha_len < 3 {
        return false;
    }

    true
}

/// Normalize extracted text to match pdftotext conventions.
///
/// 1. Trim trailing whitespace from each line.
/// 2. Collapse runs of more than two consecutive newlines into exactly two.
/// 3. Preserve form-feed characters (`\x0C`) as page separators.
/// 4. End with a single trailing newline (or empty for empty input).
pub(crate) fn normalize_text_output(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }

    let mut lines: Vec<&str> = Vec::new();
    for line in text.split('\n') {
        lines.push(line.trim_end());
    }

    // Remove trailing empty lines (we'll add exactly one \n at the end)
    while lines.last() == Some(&"") {
        lines.pop();
    }

    if lines.is_empty() {
        return String::new();
    }

    let mut result = String::with_capacity(text.len());
    let mut consecutive_empty = 0u32;

    for (i, line) in lines.iter().enumerate() {
        if line.is_empty() || *line == "\x0C" {
            if line.is_empty() {
                consecutive_empty += 1;
                // Collapse >2 consecutive blank lines to 2
                if consecutive_empty <= 2 {
                    result.push('\n');
                }
            } else {
                // Bare form-feed line
                consecutive_empty = 0;
                result.push_str(line);
                if i + 1 < lines.len() {
                    result.push('\n');
                }
            }
        } else {
            // Check if line starts with form-feed
            if line.starts_with('\x0C') {
                consecutive_empty = 0;
                result.push_str(line);
            } else {
                consecutive_empty = 0;
                result.push_str(line);
            }
            if i + 1 < lines.len() {
                result.push('\n');
            }
        }
    }

    // Ensure single trailing newline
    if !result.is_empty() && !result.ends_with('\n') {
        result.push('\n');
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(text: &str, x: f64, y: f64, width: f64) -> TextSpan {
        TextSpan {
            text: text.into(),
            x,
            y,
            width,
            height: 12.0,
            font_size: 12.0,
        }
    }

    fn block_texts(spans: Vec<TextSpan>) -> Vec<String> {
        group_spans_into_blocks(spans)
            .into_iter()
            .map(|block| block.text())
            .collect()
    }

    #[test]
    fn empty_device_produces_empty_text() {
        let dev = TextExtractionDevice::new();
        assert!(dev.into_text().is_empty());
    }

    #[test]
    fn single_column_stays_row_major() {
        let texts = block_texts(vec![
            span("Single Column Line 1", 40.0, 700.0, 140.0),
            span("Single Column Line 2", 40.0, 684.0, 140.0),
            span("Single Column Line 3", 40.0, 668.0, 140.0),
        ]);

        assert_eq!(
            texts,
            vec![
                "Single Column Line 1",
                "Single Column Line 2",
                "Single Column Line 3",
            ]
        );
    }

    #[test]
    fn two_column_region_reads_column_major() {
        let texts = block_texts(vec![
            span("Header", 200.0, 740.0, 80.0),
            span("Left column line one", 40.0, 700.0, 115.0),
            span("Right column line one", 320.0, 700.0, 120.0),
            span("Left column line two", 40.0, 684.0, 115.0),
            span("Right column line two", 320.0, 684.0, 120.0),
            span("Left column line three", 40.0, 668.0, 125.0),
            span("Right column line three", 320.0, 668.0, 130.0),
            span("Footer", 200.0, 620.0, 80.0),
        ]);

        assert_eq!(
            texts,
            vec![
                "Header",
                "Left column line one",
                "Left column line two",
                "Left column line three",
                "Right column line one",
                "Right column line two",
                "Right column line three",
                "Footer",
            ]
        );
    }

    #[test]
    fn mixed_single_and_multi_column_regions_preserve_shared_bands() {
        let texts = block_texts(vec![
            span("Intro paragraph", 40.0, 740.0, 180.0),
            span("L1 words here", 40.0, 700.0, 110.0),
            span("R1 words here", 320.0, 700.0, 110.0),
            span("L2 words here", 40.0, 684.0, 110.0),
            span("R2 words here", 320.0, 684.0, 110.0),
            span("L3 words here", 40.0, 668.0, 110.0),
            span("R3 words here", 320.0, 668.0, 110.0),
            span("Outro paragraph", 40.0, 620.0, 180.0),
        ]);

        assert_eq!(
            texts,
            vec![
                "Intro paragraph",
                "L1 words here",
                "L2 words here",
                "L3 words here",
                "R1 words here",
                "R2 words here",
                "R3 words here",
                "Outro paragraph",
            ]
        );
    }

    #[test]
    fn short_table_like_rows_fall_back_to_row_major() {
        let texts = block_texts(vec![
            span("Name", 40.0, 700.0, 30.0),
            span("Age", 320.0, 700.0, 20.0),
            span("Alice", 40.0, 684.0, 35.0),
            span("30", 320.0, 684.0, 15.0),
            span("Bob", 40.0, 668.0, 24.0),
            span("25", 320.0, 668.0, 15.0),
        ]);

        assert_eq!(texts, vec!["Name Age", "Alice 30", "Bob 25"]);
    }

    #[test]
    fn three_column_regions_are_supported() {
        let texts = block_texts(vec![
            span("Column one line one", 40.0, 700.0, 105.0),
            span("Column two line one", 220.0, 700.0, 105.0),
            span("Column three line one", 400.0, 700.0, 120.0),
            span("Column one line two", 40.0, 684.0, 105.0),
            span("Column two line two", 220.0, 684.0, 105.0),
            span("Column three line two", 400.0, 684.0, 120.0),
            span("Column one line three", 40.0, 668.0, 120.0),
            span("Column two line three", 220.0, 668.0, 120.0),
            span("Column three line three", 400.0, 668.0, 135.0),
        ]);

        assert_eq!(
            texts,
            vec![
                "Column one line one",
                "Column one line two",
                "Column one line three",
                "Column two line one",
                "Column two line two",
                "Column two line three",
                "Column three line one",
                "Column three line two",
                "Column three line three",
            ]
        );
    }

    #[test]
    fn text_block_concatenation_spaced() {
        let block = TextBlock {
            spans: vec![span("A", 0.0, 0.0, 6.0), span("B", 20.0, 0.0, 6.0)],
        };
        assert_eq!(block.text(), "A B");
    }

    #[test]
    fn adaptive_column_gap_fallback_for_no_gaps() {
        // Single-span bands produce no measurable gaps → fallback
        let bands = vec![
            TextBand::new(span("Hello", 40.0, 700.0, 80.0)),
            TextBand::new(span("World", 40.0, 684.0, 80.0)),
        ];
        let threshold = compute_adaptive_column_gap(&bands);
        assert!((threshold - COLUMN_GAP_THRESHOLD_FALLBACK).abs() < 0.01);
    }

    #[test]
    fn adaptive_column_gap_uses_median() {
        // Three bands with word gaps of ~4pt each → median ≈ 4, threshold = 12
        let mut bands = Vec::new();
        for y in [700.0, 684.0, 668.0] {
            let mut band = TextBand::new(span("word1", 40.0, y, 30.0));
            band.spans.push(span("word2", 74.0, y, 30.0)); // gap = 4
            band.spans.push(span("word3", 108.0, y, 30.0)); // gap = 4
            bands.push(band);
        }
        let threshold = compute_adaptive_column_gap(&bands);
        // median gap = 4, × 3 = 12, clamped to [10, 40] → 12
        assert!(threshold >= 10.0 && threshold <= 14.0,
            "expected ~12, got {threshold}");
    }

    #[test]
    fn adaptive_column_gap_clamps_to_min() {
        // Tight gaps (2pt) across many bands → median = 2, 3×2 = 6 → clamped to 10
        let mut bands = Vec::new();
        for y in [700.0, 684.0, 668.0, 652.0] {
            let mut band = TextBand::new(span("abc", 0.0, y, 18.0));
            // right of "abc" = max(18, 12*0.5*3=18) = 18; gap = 20-18 = 2
            band.spans.push(span("def", 20.0, y, 18.0));
            bands.push(band);
        }
        let threshold = compute_adaptive_column_gap(&bands);
        assert!((threshold - COLUMN_GAP_THRESHOLD_MIN).abs() < 0.01,
            "expected {COLUMN_GAP_THRESHOLD_MIN}, got {threshold}");
    }

    #[test]
    fn adaptive_column_gap_clamps_to_max() {
        // Very wide gaps (50pt) → 3×50 = 150 → clamped to 40
        let mut band = TextBand::new(span("Left", 0.0, 700.0, 30.0));
        band.spans.push(span("Right", 80.0, 700.0, 30.0)); // gap = 50
        let bands = vec![band];
        let threshold = compute_adaptive_column_gap(&bands);
        assert!((threshold - COLUMN_GAP_THRESHOLD_MAX).abs() < 0.01,
            "expected {COLUMN_GAP_THRESHOLD_MAX}, got {threshold}");
    }

    #[test]
    fn normalize_trims_trailing_whitespace_per_line() {
        assert_eq!(
            normalize_text_output("hello   \nworld  \n"),
            "hello\nworld\n"
        );
    }

    #[test]
    fn normalize_collapses_excess_newlines() {
        // >2 blank lines collapse to 2 (meaning 3 \n in a row: line, blank, blank)
        assert_eq!(
            normalize_text_output("hello\n\n\n\n\nworld\n"),
            "hello\n\n\nworld\n"
        );
    }

    #[test]
    fn normalize_preserves_double_newline() {
        assert_eq!(
            normalize_text_output("paragraph one\n\nparagraph two\n"),
            "paragraph one\n\nparagraph two\n"
        );
    }

    #[test]
    fn normalize_preserves_form_feed() {
        assert_eq!(
            normalize_text_output("page1\n\n\x0Cpage2\n"),
            "page1\n\n\x0Cpage2\n"
        );
    }

    #[test]
    fn normalize_adds_trailing_newline() {
        assert_eq!(normalize_text_output("hello"), "hello\n");
    }

    #[test]
    fn normalize_empty_input() {
        assert_eq!(normalize_text_output(""), "");
    }

    #[test]
    fn normalize_only_whitespace() {
        assert_eq!(normalize_text_output("   \n  \n"), "");
    }

    // --- Hyphen stitching tests ---

    #[test]
    fn hyphen_stitch_joins_wrapped_word() {
        let lines = vec!["the aver-".into(), "age rainfall".into()];
        assert_eq!(stitch_hyphenated_lines(&lines), "the average rainfall");
    }

    #[test]
    fn hyphen_stitch_handles_leading_whitespace() {
        let lines = vec!["pre-".into(), "   dict the outcome".into()];
        // "pre" is only 3 chars → meets >= 3 guard
        assert_eq!(stitch_hyphenated_lines(&lines), "predict the outcome");
    }

    #[test]
    fn hyphen_stitch_capital_continuation_not_stitched() {
        let lines = vec!["Section three-".into(), "Summary here".into()];
        assert_eq!(
            stitch_hyphenated_lines(&lines),
            "Section three-\nSummary here"
        );
    }

    #[test]
    fn hyphen_stitch_bullet_dash_not_stitched() {
        // "-" alone: char before hyphen is not alphabetic
        let lines = vec!["Items:".into(), "-".into(), "milk".into()];
        assert_eq!(stitch_hyphenated_lines(&lines), "Items:\n-\nmilk");
    }

    #[test]
    fn hyphen_stitch_numeric_range_not_stitched() {
        // "42-" — char before hyphen is digit, not alphabetic
        let lines = vec!["page 42-".into(), "seventy".into()];
        assert_eq!(
            stitch_hyphenated_lines(&lines),
            "page 42-\nseventy"
        );
    }

    #[test]
    fn hyphen_stitch_short_prefix_not_stitched() {
        // "re-" only 2 alpha chars before hyphen → below 3-char guard
        let lines = vec!["re-".into(), "organize".into()];
        assert_eq!(stitch_hyphenated_lines(&lines), "re-\norganize");
    }

    #[test]
    fn hyphen_stitch_short_continuation_not_stitched() {
        // Next line starts with "an" (2 chars) → below 3-char guard
        let lines = vec!["counter-".into(), "an example".into()];
        assert_eq!(
            stitch_hyphenated_lines(&lines),
            "counter-\nan example"
        );
    }

    #[test]
    fn hyphen_stitch_compound_word_midline_preserved() {
        // "real-time" is mid-line, not end-of-line — no stitching applies
        // because stitch only operates on line boundaries
        let lines = vec!["real-time system".into()];
        assert_eq!(stitch_hyphenated_lines(&lines), "real-time system");
    }

    #[test]
    fn hyphen_stitch_single_line_unchanged() {
        let lines = vec!["only line".into()];
        assert_eq!(stitch_hyphenated_lines(&lines), "only line");
    }

    #[test]
    fn hyphen_stitch_empty_input() {
        let lines: Vec<String> = vec![];
        assert_eq!(stitch_hyphenated_lines(&lines), "");
    }

    #[test]
    fn narrow_gutter_detected_with_adaptive_threshold() {
        // Academic paper layout: 12pt gutter between columns.
        // With old fixed 20pt threshold, this was not detected as columnar.
        // With adaptive: median word gap ~4pt, threshold = 12pt → detects 12pt gutter.
        let mut spans = Vec::new();
        for y in [700.0, 684.0, 668.0] {
            // Left column: two words with 4pt gap, ending at x=145
            spans.push(span("Lorem ipsum", 40.0, y, 100.0));
            spans.push(span("dolor sit", 144.0, y, 80.0));
            // Right column starts at 236 (gap = 12pt from 224)
            spans.push(span("amet consec", 236.0, y, 100.0));
            spans.push(span("tetur adipi", 340.0, y, 80.0));
        }
        let texts = block_texts(spans);
        // Should detect 2-column layout and read column-major
        assert!(texts.len() >= 6, "expected column-major output, got {texts:?}");
        // First three blocks should be left column lines
        assert!(texts[0].contains("Lorem"), "first block should be left column: {texts:?}");
    }
}
