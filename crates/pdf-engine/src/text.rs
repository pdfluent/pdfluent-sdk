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
/// Minimum horizontal gap treated as a column gutter.
const COLUMN_GAP_THRESHOLD: f64 = 20.0;
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

    fn gap_midpoints(&self) -> Vec<f64> {
        self.gaps()
            .into_iter()
            .map(|gap| (gap.start + gap.end) * 0.5)
            .collect()
    }

    fn gaps(&self) -> Vec<BandGap> {
        if self.spans.len() < 2 {
            return Vec::new();
        }

        let mut spans = self.spans.clone();
        spans.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(Ordering::Equal));

        let mut gaps = Vec::new();
        let mut prev_right = spans[0].right();
        for span in spans.iter().skip(1) {
            let gap = span.x - prev_right;
            if gap >= COLUMN_GAP_THRESHOLD {
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
        blocks
            .iter()
            .map(|b| b.text())
            .collect::<Vec<_>>()
            .join("\n")
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

/// Group spans into reading-order blocks, using column-aware reordering when
/// a contiguous region repeatedly exposes the same gutters.
fn group_spans_into_blocks(spans: Vec<TextSpan>) -> Vec<TextBlock> {
    let bands = group_spans_into_bands(spans);
    if bands.is_empty() {
        return Vec::new();
    }

    let mut blocks = Vec::new();
    let mut idx = 0;

    while idx < bands.len() {
        let gap_midpoints = bands[idx].gap_midpoints();
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
            let next_gap_midpoints = next_band.gap_midpoints();
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
}
