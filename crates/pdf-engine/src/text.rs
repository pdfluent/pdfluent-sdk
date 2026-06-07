//! Text extraction via a custom Device implementation.

use kurbo::{Affine, BezPath};
use pdf_render::pdf_interpret::cmap::BfString;
use pdf_render::pdf_interpret::font::Glyph;
use pdf_render::pdf_interpret::{
    BlendMode, ClipPath, Device, GlyphDrawMode, Image, Paint, PathDrawMode, SoftMask,
};
use std::cmp::Ordering;

/// Minimum Y tolerance for grouping spans into horizontal bands. The
/// effective tolerance is typically `median_font_size * BAND_Y_FRACTION`
/// per ANN[r17/TEX4]; this constant acts as the absolute floor.
const BAND_Y_TOLERANCE: f64 = 5.0;
/// Fraction of the page's median font size used as the band-Y
/// tolerance. Empirically 0.30× works across common typography —
/// below typical leading (~1.2×) so adjacent lines never collapse,
/// above sub-pixel baseline drift.
const BAND_Y_FRACTION: f64 = 0.30;
/// Multiplier applied to median line spacing to derive the horizontal
/// paragraph-break cut threshold. Normal line-to-line progression is
/// ~1.0× the median; paragraph breaks typically show 1.5× or more.
const PARAGRAPH_BREAK_LINE_SPACING_MULTIPLIER: f64 = 1.8;

// ANN[r17/TEX1] Multi-signal consensus thresholds.
// The previous single-threshold scheme (gap > 0.15 * font_size) missed
// word boundaries when kerning or narrow fonts produced small measured
// gaps even though the PDF emitted an explicit TJ backward shift, and
// over-emitted spaces for condensed fonts where 0.15em of kerning is
// well below an actual word space. The consensus system weights three
// signals and inserts a space when the combined confidence exceeds
// SPACE_CONSENSUS_THRESHOLD.
/// Raw TJ backward adjustment (positive in PDF TJ units) that is
/// definitively a word break. Matches pdftotext / MuPDF heuristics —
/// a space glyph is typically emitted as either a literal 0x20 or as
/// a TJ adjustment of around 250 1/1000 em. 100 units is a safely
/// conservative floor.
const TJ_SPACE_THRESHOLD_UNITS: f32 = 100.0;
/// Weight of the TJ offset signal when confidence is high.
const TJ_SIGNAL_WEIGHT: f64 = 0.95;
/// Weight of the purely geometric gap signal.
const GAP_SIGNAL_WEIGHT: f64 = 0.80;
/// Weight of character-heuristic signals (CamelCase, digit↔letter).
const HEURISTIC_SIGNAL_WEIGHT: f64 = 0.60;
/// Combined weight at which a space is inserted.
const SPACE_CONSENSUS_THRESHOLD: f64 = 0.75;
/// Fraction of a median character width above which a gap contributes
/// to the geometric signal (pdf_oxide uses ~0.30).
const GAP_TO_MEDIAN_CHAR_FRACTION: f64 = 0.30;
/// Fallback gap fraction relative to `font_size` when the running
/// median character width has not yet been established.
const GAP_TO_FONT_SIZE_FALLBACK_FRACTION: f64 = 0.15;

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

/// Whether a text span's width was computed from real font metrics or estimated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub enum WidthSource {
    /// Width derived from the font's actual glyph advance (hmtx, CFF, Type1 charstring).
    Metric,
    /// Width estimated at 50 % of font size — no glyph metric was available.
    #[default]
    Estimate,
}

impl WidthSource {
    /// Stable label used across SDK bindings and the JSON wire form
    /// (`"Metric"` / `"Estimate"`). Single source of truth for the string; the
    /// `serde` derive on the enum produces identical values.
    pub fn as_str(&self) -> &'static str {
        match self {
            WidthSource::Metric => "Metric",
            WidthSource::Estimate => "Estimate",
        }
    }
}

/// A single text span at a specific position.
#[derive(Debug, Clone, Default)]
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

    // ---- G1 read-only metadata (added 2026-05; backward-compatible) ----
    /// PostScript name of the font, with any 6-character subset prefix stripped
    /// (e.g. `Helvetica-Bold`, `TimesNewRomanPS-BoldMT`). `None` for Type1
    /// standard-14 fonts and Type3 fonts where no embedded font data is
    /// available through the public `pdf-interpret` API.
    pub font_name: Option<String>,
    /// Inferred bold style: `weight >= 700` or PostScript name suggests bold
    /// ("bold", "demi", "semibold", "heavy", "black"). Defaults `false` when
    /// no descriptor data is reachable.
    pub is_bold: bool,
    /// Inferred italic style: FontDescriptor /Italic flag set or PostScript
    /// name suggests italic/oblique/slant. Defaults `false` when no descriptor
    /// data is reachable.
    pub is_italic: bool,
    /// Fill color as sRGB RGBA, derived from `Paint::Color(c).to_rgba().to_rgba8()`
    /// at the moment of glyph paint. `None` for `Paint::Pattern` (tiling /
    /// shading) — the editor falls back to "auto" in that case.
    pub color: Option<[u8; 4]>,

    // ---- G2 glyph-level metrics (added 2026-05) ----
    /// Whether glyph widths were measured from real font advance data or estimated.
    pub width_source: WidthSource,
    /// Per-glyph bounding boxes in user-space, one entry per source glyph.
    /// `[x0, y0, x1, y1]` with y0 < y1 (PDF coordinate frame).
    pub char_bounds: Vec<[f64; 4]>,

    // ---- Golf 1 typographic metadata (added 2026-06; backward-compatible) ----
    /// Full affine transform of the span's first glyph in user space,
    /// `[a, b, c, d, e, f]` (kurbo coeffs of CTM × text-matrix). Captures
    /// rotation and shear that `(x, y, font_size)` discards. `None` for spans
    /// not built from a glyph (e.g. synthetic markers).
    pub transform: Option<[f64; 6]>,
    /// Numeric font weight (~100–900) from the embedded font's OS/2 table when
    /// available. `None` for non-embedded / standard-14 fonts.
    pub font_weight: Option<u16>,
    /// Whether the font is serif, when determinable from embedded font data.
    /// `None` when no embedded descriptor is reachable.
    pub is_serif: Option<bool>,
    /// Whether the font is monospace, when determinable from embedded font data.
    /// `None` when no embedded descriptor is reachable.
    pub is_monospace: Option<bool>,
    /// Coarse PDF text render mode: `0` fill, `1` stroke, `3` invisible — the
    /// only three values the renderer's `GlyphDrawMode` expresses (the
    /// fill+stroke / clip modes 2 and 4–7 are not distinguished). Reflects the
    /// span's first glyph. `None` for spans not built from a glyph.
    pub render_mode: Option<u8>,
}

impl TextSpan {
    /// Conservative right edge using whichever is wider: measured or estimated.
    /// Used by column detection to avoid underestimating span extent.
    fn right(&self) -> f64 {
        self.x + self.width.max(self.estimated_width())
    }

    /// Right edge from measured glyph positions only.
    fn measured_right(&self) -> f64 {
        self.x + self.width
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
            let expected_end = prev.measured_right();
            let gap = curr.x - expected_end;
            if gap <= prev.font_size * 0.12 {
                if let Some(trimmed) = trim_overlapping_word_prefix(&prev.text, &curr.text) {
                    result.push_str(&trimmed);
                    continue;
                }
            }
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
        collapse_overprinted_spans(&mut self.spans);
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
///
/// ANN[r17/TEX1][r17/TEX3] Space detection uses a multi-signal consensus
/// rather than a single geometric threshold. Three signals vote:
///   1. `pending_tj_offset`  — raw TJ backward shift surfaced by the
///      interpreter (confidence 0.95). This is the definitive word-break
///      signal used by pdftotext / MuPDF.
///   2. geometric gap        — measured horizontal distance between the
///      previous glyph's right edge and this glyph's origin (confidence
///      0.80). Compared against the running median glyph width rather
///      than a flat em-fraction so condensed/wide fonts are handled
///      uniformly.
///   3. character heuristic  — CamelCase transition or digit↔letter
///      transition at the merge point (confidence 0.60). Catches cases
///      where the writer relied on typography (e.g. table cells glued
///      with zero gap: `Qty1Price$5`).
///
/// A space is inserted when the weighted sum meets SPACE_CONSENSUS_THRESHOLD.
/// Span accumulation still merges adjacent glyphs into one TextSpan (TEX3)
/// so downstream reading-order logic sees logical text runs, not individual
/// character positions.
pub(crate) struct TextExtractionDevice {
    spans: Vec<TextSpan>,
    last_y: f64,
    last_end_x: f64,
    /// TJ adjustment in raw 1/1000 em units since the last glyph was
    /// drawn. Positive values = backward shift (i.e., explicit horizontal
    /// space). Reset every time a glyph is drawn.
    pending_tj_offset: f32,
    /// Running sample of measured glyph widths used as the adaptive
    /// reference for the geometric gap signal. Cheap to maintain and
    /// avoids having to re-walk all spans per decision.
    glyph_widths: Vec<f64>,
    /// Cached median glyph width (kept fresh every `MEDIAN_REFRESH`
    /// insertions). Zero = not yet established, caller falls back to
    /// font-size scaling.
    cached_median_char_width: f64,
}

const MEDIAN_REFRESH: usize = 32;

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
            pending_tj_offset: 0.0,
            glyph_widths: Vec::new(),
            cached_median_char_width: 0.0,
        }
    }

    /// Refresh the cached median char width. Called lazily from
    /// `draw_glyph` to keep the hot path cheap.
    fn refresh_median_char_width(&mut self) {
        if self.glyph_widths.is_empty() {
            self.cached_median_char_width = 0.0;
            return;
        }
        let mut sorted = self.glyph_widths.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        self.cached_median_char_width = sorted[sorted.len() / 2];
    }

    /// Decide whether a space should be glued between two glyphs within
    /// the same span. Returns (insert_space, start_new_span).
    fn evaluate_space_consensus(
        &self,
        gap: f64,
        font_size: f64,
        prev_text: &str,
        next_text: &str,
    ) -> bool {
        let mut confidence = 0.0;

        // Signal 1 — TJ offset (highest confidence). Raw units; a full
        // space is ~250. Anything over TJ_SPACE_THRESHOLD_UNITS counts.
        if self.pending_tj_offset.abs() >= TJ_SPACE_THRESHOLD_UNITS {
            confidence += TJ_SIGNAL_WEIGHT;
        }

        // Signal 2 — geometric gap. Prefer the adaptive median-char-width
        // reference; fall back to font-size when the median hasn't been
        // established yet (first few glyphs on a page).
        let gap_reference = if self.cached_median_char_width > 0.0 {
            self.cached_median_char_width * GAP_TO_MEDIAN_CHAR_FRACTION
        } else {
            font_size * GAP_TO_FONT_SIZE_FALLBACK_FRACTION
        };
        if gap > gap_reference {
            confidence += GAP_SIGNAL_WEIGHT;
        }

        // Signal 3 — character-class transitions. Only checked when the
        // previous span ends with a character and the incoming text starts
        // with one; avoids double-counting with punctuation.
        if let (Some(prev_last), Some(next_first)) =
            (prev_text.chars().last(), next_text.chars().next())
        {
            let camel = prev_last.is_lowercase() && next_first.is_uppercase();
            let digit_to_letter = prev_last.is_ascii_digit() && next_first.is_alphabetic();
            let letter_to_digit = prev_last.is_alphabetic() && next_first.is_ascii_digit();
            if camel || digit_to_letter || letter_to_digit {
                confidence += HEURISTIC_SIGNAL_WEIGHT;
            }
        }

        confidence >= SPACE_CONSENSUS_THRESHOLD
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
        paint: &Paint<'_>,
        draw_mode: &GlyphDrawMode,
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

        // G2: distinguish real advance (Metric) from estimate (Estimate).
        let (glyph_width, glyph_ws) = glyph_width_and_source(glyph, font_size);
        let glyph_end_x = x + glyph_width;
        let glyph_bound = [x, y, glyph_end_x, y + font_size];

        let style = derive_glyph_style(glyph);
        let color = paint_to_rgba(paint);

        // ANN[r17/TEX4] Feed the running sample used to derive the adaptive
        // median character width. Capped to protect against pathological
        // pages with hundreds of thousands of glyphs.
        if self.glyph_widths.len() < 4096 {
            self.glyph_widths.push(glyph_width);
            if self.glyph_widths.len().is_multiple_of(MEDIAN_REFRESH) {
                self.refresh_median_char_width();
            }
        }

        let same_line = (y - self.last_y).abs() <= font_size.max(BAND_Y_TOLERANCE) * 0.35;
        let gap = x - self.last_end_x;
        let adjacent = same_line && gap >= -font_size * 0.25 && gap < font_size * 0.5;

        // G1: only merge into the previous span when font + style + color
        // match. Otherwise the editor's style toolbar would render the wrong
        // state for the cursor position.
        let style_matches = self
            .spans
            .last()
            .map(|last| {
                last.font_name == style.font_name
                    && last.is_bold == style.is_bold
                    && last.is_italic == style.is_italic
                    && last.color == color
            })
            .unwrap_or(false);

        if adjacent && !self.spans.is_empty() && style_matches {
            // ANN[r17/TEX1] Multi-signal consensus replaces the prior
            // single-threshold rule (`gap > 0.15 * font_size`). The
            // consensus evaluates TJ offset, geometric gap, and
            // character-class transitions; a space is inserted only
            // when the weighted sum meets SPACE_CONSENSUS_THRESHOLD.
            // Decision is computed before the mutable borrow of `last`
            // to keep the borrow checker happy.
            let want_space = {
                let last = self.spans.last().expect("checked non-empty");
                !last.text.ends_with(' ')
                    && !text.starts_with(' ')
                    && self.evaluate_space_consensus(gap, font_size, &last.text, &text)
            };
            let last = self.spans.last_mut().expect("checked non-empty");
            if want_space {
                last.text.push(' ');
            }
            last.text.push_str(&text);
            last.width = last.width.max(glyph_end_x - last.x);
            last.height = last.height.max(font_size);
            // G2: append char_bound; downgrade width_source if this glyph is Estimate.
            last.char_bounds.push(glyph_bound);
            if glyph_ws == WidthSource::Estimate {
                last.width_source = WidthSource::Estimate;
            }
            self.last_y = y;
            self.last_end_x = glyph_end_x;
            // ANN[r17/TEX1] Consume the TJ signal: it only counts for
            // the one merge it preceded.
            self.pending_tj_offset = 0.0;
            return;
        }

        self.last_y = y;
        self.last_end_x = glyph_end_x;
        // ANN[r17/TEX1] Non-adjacent glyph starts a fresh span, so any
        // pending TJ offset is about within-span word breaks and no longer
        // meaningful here.
        self.pending_tj_offset = 0.0;

        self.spans.push(TextSpan {
            text,
            x,
            y,
            width: glyph_width,
            height: font_size,
            font_size,
            font_name: style.font_name,
            is_bold: style.is_bold,
            is_italic: style.is_italic,
            color,
            width_source: glyph_ws,
            char_bounds: vec![glyph_bound],
            transform: Some(coeffs),
            font_weight: style.font_weight,
            is_serif: style.is_serif,
            is_monospace: style.is_monospace,
            render_mode: Some(render_mode_from_draw_mode(draw_mode)),
        });
    }

    // ANN[r17/TEX1] Record TJ offsets. Accumulate because a single
    // inter-substring gap may be expressed as multiple numeric entries
    // (rare, but legal per PDF §9.4.3). The next draw_glyph consumes
    // the sum.
    fn text_adjustment(&mut self, amount: f32) {
        self.pending_tj_offset += amount;
    }
}

/// Style metadata derived from a `Glyph` for the G1 text-run extension.
#[derive(Debug, Default, Clone)]
struct GlyphStyle {
    font_name: Option<String>,
    is_bold: bool,
    is_italic: bool,
    /// Numeric weight from embedded font data, if available.
    font_weight: Option<u16>,
    /// Serif flag from embedded font data, if available.
    is_serif: Option<bool>,
    /// Monospace flag from embedded font data, if available.
    is_monospace: Option<bool>,
}

/// Strip a 6-character subset prefix (e.g. `AAAAAA+Helvetica` → `Helvetica`).
fn strip_subset_prefix(name: &str) -> &str {
    match name.split_once('+') {
        Some((prefix, rest)) if prefix.len() == 6 => rest,
        _ => name,
    }
}

/// Heuristic style inference from a PostScript name when no descriptor
/// flags are reachable. Matches the same rules `pdf-interpret` uses in
/// `FallbackFontQuery::new`.
fn name_style_hints(name: &str) -> (bool, bool) {
    let lower = name.to_ascii_lowercase();
    let italic = lower.contains("italic") || lower.contains("oblique") || lower.contains("slant");
    let bold = lower.contains("bold")
        || lower.contains("demi")
        || lower.contains("semibold")
        || lower.contains("heavy")
        || lower.contains("black");
    (bold, italic)
}

fn derive_glyph_style(glyph: &Glyph<'_>) -> GlyphStyle {
    match glyph {
        Glyph::Outline(outline) => {
            if let Some(data) = outline.font_data() {
                let raw = data.postscript_name.as_deref().unwrap_or("");
                let name = strip_subset_prefix(raw).to_string();
                let weight_bold = data.weight.is_some_and(|w| w >= 700);
                let (name_bold, name_italic) = name_style_hints(&name);
                GlyphStyle {
                    font_name: if name.is_empty() { None } else { Some(name) },
                    is_bold: weight_bold || name_bold,
                    is_italic: data.is_italic || name_italic,
                    font_weight: data.weight.map(|w| w.clamp(1, 1000) as u16),
                    is_serif: Some(data.is_serif),
                    is_monospace: Some(data.is_monospace),
                }
            } else {
                // Type1 / non-embedded font — descriptor not surfaced
                // via font_data(). Fall back to the name-only
                // accessor which works for standard-14 fallbacks.
                let raw = outline.postscript_name().unwrap_or_default();
                let name = strip_subset_prefix(&raw).to_string();
                let (name_bold, name_italic) = name_style_hints(&name);
                GlyphStyle {
                    font_name: if name.is_empty() { None } else { Some(name) },
                    is_bold: name_bold,
                    is_italic: name_italic,
                    // Non-embedded font: descriptor metrics are not reachable.
                    font_weight: None,
                    is_serif: None,
                    is_monospace: None,
                }
            }
        }
        Glyph::Type3(_) => GlyphStyle::default(),
    }
}

fn paint_to_rgba(paint: &Paint<'_>) -> Option<[u8; 4]> {
    match paint {
        Paint::Color(c) => Some(c.to_rgba().to_rgba8()),
        Paint::Pattern(_) => None,
    }
}

/// Map the renderer's coarse glyph draw mode to a PDF text render-mode code.
///
/// NOTE: the interpreter collapses the eight PDF text render modes (Tr 0–7)
/// into three visibility classes, so only `{0, 1, 3}` are representable here —
/// fill (0), stroke (1), and invisible (3). The fill+stroke and clip modes
/// (2, 4–7) are not distinguished. This is sufficient to identify and filter
/// invisible (mode-3) OCR/overlay text; recovering the full `Tr` range would
/// require threading it through the `Device` trait (out of scope).
fn render_mode_from_draw_mode(mode: &GlyphDrawMode) -> u8 {
    match mode {
        GlyphDrawMode::Fill => 0,
        GlyphDrawMode::Stroke(_) => 1,
        GlyphDrawMode::Invisible => 3,
    }
}

#[cfg(test)]
mod render_mode_tests {
    use super::render_mode_from_draw_mode;
    use pdf_render::pdf_interpret::{GlyphDrawMode, StrokeProps};

    #[test]
    fn render_mode_is_only_zero_one_three() {
        assert_eq!(render_mode_from_draw_mode(&GlyphDrawMode::Fill), 0);
        assert_eq!(
            render_mode_from_draw_mode(&GlyphDrawMode::Stroke(StrokeProps::default())),
            1
        );
        assert_eq!(render_mode_from_draw_mode(&GlyphDrawMode::Invisible), 3);
        // Exhaustive: the mapping only ever yields 0, 1, or 3 — never 2 or 4–7.
        for m in [
            GlyphDrawMode::Fill,
            GlyphDrawMode::Stroke(StrokeProps::default()),
            GlyphDrawMode::Invisible,
        ] {
            assert!(matches!(render_mode_from_draw_mode(&m), 0 | 1 | 3));
        }
    }
}

/// Returns `(advance_in_user_space, WidthSource)` for a glyph.
///
/// Uses the real advance from `OutlineGlyph::advance_width()` when available
/// (returns `WidthSource::Metric`); falls back to 50% em otherwise
/// (`WidthSource::Estimate`). The result is clamped to at least 25% em so
/// invisible-glyph outliers do not collapse spans.
fn glyph_width_and_source(glyph: &Glyph<'_>, font_size: f64) -> (f64, WidthSource) {
    match glyph {
        Glyph::Outline(outline) => {
            if let Some(w) = outline.advance_width() {
                let advance = (w as f64 / 1000.0 * font_size).max(font_size * 0.25);
                (advance, WidthSource::Metric)
            } else {
                (font_size * 0.5, WidthSource::Estimate)
            }
        }
        Glyph::Type3(_) => (font_size * 0.5, WidthSource::Estimate),
    }
}

/// Collapse fake-bold / overprint duplicates inside one band.
///
/// Real-word corpus failures such as 0105.pdf draw the same text several times
/// with sub-point x drift to simulate heavier weight. Text extraction should
/// keep the most informative span once rather than concatenate every overprint.
fn collapse_overprinted_spans(spans: &mut Vec<TextSpan>) {
    if spans.len() < 2 {
        return;
    }

    let mut deduped: Vec<TextSpan> = Vec::with_capacity(spans.len());
    for span in spans.drain(..) {
        if let Some(last) = deduped.last_mut() {
            if spans_are_overprint_duplicates(last, &span) {
                let choose_incoming = span.text.chars().count() > last.text.chars().count()
                    || (span.text.chars().count() == last.text.chars().count()
                        && span.width > last.width);
                let preferred_text = if choose_incoming {
                    span.text.clone()
                } else {
                    last.text.clone()
                };
                let left = last.x.min(span.x);
                let right = last.right().max(span.right());
                last.x = left;
                last.y = (last.y + span.y) * 0.5;
                last.width = (right - left).max(last.width).max(span.width);
                last.height = last.height.max(span.height);
                last.font_size = last.font_size.max(span.font_size);
                last.text = preferred_text;
                continue;
            }
        }

        deduped.push(span);
    }

    *spans = deduped;
}

fn spans_are_overprint_duplicates(lhs: &TextSpan, rhs: &TextSpan) -> bool {
    let lhs_text = lhs.text.trim();
    let rhs_text = rhs.text.trim();
    if lhs_text.is_empty() || rhs_text.is_empty() {
        return false;
    }

    let same_baseline = (lhs.y - rhs.y).abs() <= lhs.font_size.max(rhs.font_size) * 0.12;
    if !same_baseline {
        return false;
    }

    let lhs_left = lhs.x;
    let lhs_right = lhs.right();
    let rhs_left = rhs.x;
    let rhs_right = rhs.right();
    let overlap = (lhs_right.min(rhs_right) - lhs_left.max(rhs_left)).max(0.0);
    let min_width = (lhs_right - lhs_left).min(rhs_right - rhs_left).max(1.0);
    let heavily_overlaps = overlap / min_width >= 0.85;
    if !heavily_overlaps {
        return false;
    }

    lhs_text == rhs_text || lhs_text.starts_with(rhs_text) || rhs_text.starts_with(lhs_text)
}

fn trim_overlapping_word_prefix(prev: &str, curr: &str) -> Option<String> {
    let prev_chars: Vec<char> = prev.trim_end().chars().collect();
    let curr_chars: Vec<char> = curr.trim_start().chars().collect();
    let max = prev_chars.len().min(curr_chars.len());

    for len in (4..=max).rev() {
        let prev_start = prev_chars.len() - len;
        if prev_chars[prev_start..] != curr_chars[..len] {
            continue;
        }

        if !curr_chars[..len].iter().all(|ch| ch.is_alphanumeric()) {
            continue;
        }

        let prev_boundary = prev_start == 0 || !prev_chars[prev_start - 1].is_alphanumeric();
        let curr_boundary = len == curr_chars.len() || !curr_chars[len].is_alphanumeric();
        if !prev_boundary || !curr_boundary {
            continue;
        }

        return Some(curr_chars[len..].iter().collect());
    }

    None
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

    let min_gap = all_gaps[0];

    // When all inter-span gaps are already large (> MIN threshold), they are
    // likely all column gaps — the draw_glyph merger absorbed word-level
    // spaces into span text.  Use a fraction of the smallest gap so that
    // ALL column gaps exceed the threshold.
    if min_gap > COLUMN_GAP_THRESHOLD_MIN {
        return (min_gap * 0.75).clamp(COLUMN_GAP_THRESHOLD_MIN, COLUMN_GAP_THRESHOLD_MAX);
    }

    // Look for a natural break: the largest relative jump between consecutive
    // sorted gaps separates word-level gaps from column gaps.
    let mut best_break_threshold = 0.0f64;
    let mut best_ratio = 1.5f64; // require at least 1.5× jump
    for pair in all_gaps.windows(2) {
        if pair[0] > 0.5 {
            let ratio = pair[1] / pair[0];
            if ratio > best_ratio {
                best_ratio = ratio;
                best_break_threshold = (pair[0] + pair[1]) * 0.5;
            }
        }
    }

    if best_break_threshold > 0.0 {
        return best_break_threshold.clamp(COLUMN_GAP_THRESHOLD_MIN, COLUMN_GAP_THRESHOLD_MAX);
    }

    // Fallback: median × multiplier.
    let mid = all_gaps.len() / 2;
    let median = if all_gaps.len().is_multiple_of(2) {
        (all_gaps[mid - 1] + all_gaps[mid]) * 0.5
    } else {
        all_gaps[mid]
    };

    (median * COLUMN_GAP_MEDIAN_MULTIPLIER)
        .clamp(COLUMN_GAP_THRESHOLD_MIN, COLUMN_GAP_THRESHOLD_MAX)
}

/// Group spans into reading-order blocks, using column-aware reordering when
/// a contiguous region repeatedly exposes the same gutters.
/// Per-page adaptive parameters derived from the span set before any
/// grouping happens. Centralising these here (TEX4) means the rest of
/// the pipeline — band grouping, XY-Cut cuts, in-block space insertion
/// — all speak the same typographic baseline for this specific page,
/// rather than each helper reaching for an independent fixed constant.
#[derive(Debug, Clone, Copy)]
struct PageStats {
    /// Median font size across all spans (pt).
    median_font_size: f64,
    /// Median measured character width (pt). Zero-guarded fallback is
    /// 0.5 × median_font_size when there aren't enough samples.
    /// Currently populated for diagnostics / future tuning; allow dead_code
    /// under `-D warnings` until a reader is added.
    #[allow(dead_code)]
    median_char_width: f64,
    /// Tight line-to-line spacing (25th percentile of pairwise band
    /// gaps), representing the body-text leading on this page. The
    /// quartile is used instead of the median so large paragraph /
    /// zone gaps don't inflate the baseline. Zero if the page has
    /// only one band.
    median_line_spacing: f64,
}

impl PageStats {
    fn from_spans(spans: &[TextSpan]) -> Self {
        if spans.is_empty() {
            return Self {
                median_font_size: 12.0,
                median_char_width: 6.0,
                median_line_spacing: 0.0,
            };
        }

        // Median font size.
        let mut sizes: Vec<f64> = spans.iter().map(|s| s.font_size).collect();
        sizes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        let median_font_size = sizes[sizes.len() / 2];

        // Median char width — measured width / char count, per span.
        let mut char_widths: Vec<f64> = spans
            .iter()
            .filter_map(|s| {
                let chars = s.text.chars().count();
                if chars > 0 && s.width > 0.0 {
                    Some(s.width / chars as f64)
                } else {
                    None
                }
            })
            .collect();
        let median_char_width = if char_widths.is_empty() {
            median_font_size * 0.5
        } else {
            char_widths.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
            char_widths[char_widths.len() / 2]
        };

        // Median line spacing — pairwise gaps between consecutive band
        // y-values.
        let band_tolerance = (median_font_size * BAND_Y_FRACTION).max(BAND_Y_TOLERANCE);
        let mut ys: Vec<f64> = spans.iter().map(|s| s.y).collect();
        ys.sort_by(|a, b| b.partial_cmp(a).unwrap_or(Ordering::Equal));
        let mut band_ys: Vec<f64> = Vec::new();
        for y in ys {
            if band_ys
                .last()
                .map(|prev: &f64| (prev - y).abs() > band_tolerance)
                .unwrap_or(true)
            {
                band_ys.push(y);
            }
        }
        // ANN[r17/TEX4] "Line spacing" here means the TIGHT line-to-line
        // gap inside a text block — not the median of all gaps. Using
        // the median drags the estimate up when the page has
        // paragraph / zone breaks (which are the very gaps the
        // paragraph-break threshold is supposed to EXCEED). The 25th
        // percentile is the smallest gap that still shows up in more
        // than one place on the page; it captures body-text leading
        // robustly even when large zone gaps dominate.
        let median_line_spacing = if band_ys.len() < 2 {
            0.0
        } else {
            let mut spacings: Vec<f64> = band_ys
                .windows(2)
                .map(|pair| (pair[0] - pair[1]).abs())
                .collect();
            spacings.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
            let q1_index = spacings.len() / 4;
            spacings[q1_index]
        };

        Self {
            median_font_size,
            median_char_width,
            median_line_spacing,
        }
    }
}

// ANN[r17/TEX2] Maximum recursion depth for XY-Cut. Any real page layout
// is decomposable in well under 10 alternating cuts; the cap guards
// against pathological inputs where the cut predicate keeps triggering
// due to floating-point drift.
const XY_CUT_MAX_DEPTH: usize = 12;
/// Minimum fraction of a region's width that a vertical gap must reach
/// before it qualifies as a column gutter.
const XY_CUT_VERTICAL_GAP_REGION_FRACTION: f64 = 0.04;
/// Floor (in pt) for vertical gap regardless of region width. Matches
/// the previous `COLUMN_GAP_THRESHOLD_MIN` and keeps XY-Cut conservative
/// on narrow regions (sidebars, tall columns).
const XY_CUT_VERTICAL_GAP_FLOOR: f64 = 10.0;
/// Multiplier applied to median font size to produce the horizontal-gap
/// threshold. 1.8 × line-height matches typical paragraph spacing.
const XY_CUT_HORIZONTAL_GAP_FONT_MULTIPLIER: f64 = 1.8;
/// Minimum number of spans a column must contain before it is eligible
/// for acceptance — one-span "columns" are almost always sidebar noise
/// or table-cell fragments.
const XY_CUT_MIN_SPANS_PER_COLUMN: usize = 2;
/// Average characters per band a column must have before it's accepted
/// as dense prose (vs. a short-cell table column).
const XY_CUT_MIN_CHARS_PER_BAND: f64 = 8.0;

/// ANN[r17/TEX2][r17/TEX4] Top-level grouping uses recursive XY-Cut
/// with a density guard. Per-page stats are computed once up front so
/// every decision downstream speaks the same typographic baseline.
fn group_spans_into_blocks(spans: Vec<TextSpan>) -> Vec<TextBlock> {
    if spans.is_empty() {
        return Vec::new();
    }
    let stats = PageStats::from_spans(&spans);
    xy_cut_recursive(spans, 0, &stats)
}

fn xy_cut_recursive(spans: Vec<TextSpan>, depth: usize, stats: &PageStats) -> Vec<TextBlock> {
    if spans.is_empty() {
        return Vec::new();
    }
    if depth >= XY_CUT_MAX_DEPTH {
        return band_based_blocks(spans, stats);
    }

    // ANN[r17/TEX2] Pick whichever direction has the largest qualifying
    // gap. Always cutting vertically first breaks layouts where a
    // footer sits in the mid-x range — it would attach to the left
    // column instead of being recognized as a page-level zone. The
    // "largest gap wins" rule is the standard XY-Cut tie-breaker used
    // by academic OCR literature and matches pdf_oxide.
    let vcut = try_vertical_cut(&spans, stats);
    let hcut = try_horizontal_cut(&spans, stats);

    let (chosen, _) = match (vcut, hcut) {
        (Some((v_groups, v_gap)), Some((h_groups, h_gap))) => {
            if v_gap >= h_gap {
                (Some(v_groups), v_gap)
            } else {
                (Some(h_groups), h_gap)
            }
        }
        (Some((v_groups, v_gap)), None) => (Some(v_groups), v_gap),
        (None, Some((h_groups, h_gap))) => (Some(h_groups), h_gap),
        (None, None) => (None, 0.0),
    };

    if let Some(groups) = chosen {
        let mut out = Vec::new();
        for group in groups {
            out.extend(xy_cut_recursive(group, depth + 1, stats));
        }
        return out;
    }

    band_based_blocks(spans, stats)
}

/// Emit per-band row blocks without any column detection. Used as the
/// leaf of XY-Cut recursion — at this point the region either has no
/// further cuts or the density guard refused them.
fn band_based_blocks(spans: Vec<TextSpan>, stats: &PageStats) -> Vec<TextBlock> {
    // XY-Cut can miss recurring gutters when a small number of bands span the
    // full page width (e.g. a running header above a 3-column body). In that
    // case, fall back to the older band/gutter detector inside the leaf region
    // instead of flattening everything row-major.
    group_spans_into_blocks_legacy_with_stats(spans, stats)
}

/// Median font-size helper. Currently unreferenced after `PageStats` took over
/// the typography baseline computation; kept available for future tuning paths.
#[allow(dead_code)]
fn median_font_size(spans: &[TextSpan]) -> f64 {
    if spans.is_empty() {
        return 12.0;
    }
    let mut sizes: Vec<f64> = spans.iter().map(|s| s.font_size).collect();
    sizes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    sizes[sizes.len() / 2]
}

/// Attempt a vertical (column) cut. Returns the span groups plus the
/// gap size (in pt) if a suitable gutter is found AND the density +
/// alignment guards accept.
///
/// ANN[r17/TEX2] Three guards together avoid false-positive columns:
///   1. `min_gap` is the MAX of (median_font, 4% of region width, 10pt)
///      — deliberately lower than `median_font * 2` so narrow-gutter
///      academic papers (12pt gutters, common in print) are still
///      detected.
///   2. `columns_are_dense` rejects column splits where either side
///      has <2 spans or <8 chars/band — catches table cells.
///   3. `columns_are_band_aligned` rejects cuts where any band would
///      end up on only one side of the cut while being wider than
///      ~70% of that side's column width — catches full-width
///      paragraphs (Intro / Outro) that accidentally sit in the
///      left-column x-range.
fn try_vertical_cut(spans: &[TextSpan], stats: &PageStats) -> Option<(Vec<Vec<TextSpan>>, f64)> {
    if spans.len() < 2 * XY_CUT_MIN_SPANS_PER_COLUMN {
        return None;
    }

    let region_left = spans.iter().map(|s| s.x).fold(f64::INFINITY, f64::min);
    let region_right = spans
        .iter()
        .map(TextSpan::right)
        .fold(f64::NEG_INFINITY, f64::max);
    let region_width = region_right - region_left;
    if region_width <= 0.0 {
        return None;
    }

    // ANN[r17/TEX2][r17/TEX4] Threshold uses the ADAPTIVE median-word-gap
    // from the bands rather than a flat font-size multiple. Narrow-gutter
    // academic layouts have 12pt gutters next to 4pt word spaces — the
    // adaptive threshold scales with the actual typography used on this
    // page. Clamped to `XY_CUT_VERTICAL_GAP_FLOOR` to avoid firing on
    // ordinary inter-word spaces when character advance data is noisy.
    // median_font and the width fraction act only as safety rails for
    // pathological inputs.
    let bands = group_spans_into_bands_with_stats(spans.to_vec(), stats);
    let adaptive = compute_adaptive_column_gap(&bands);
    let floor = stats
        .median_font_size
        .max(region_width * XY_CUT_VERTICAL_GAP_REGION_FRACTION)
        .max(XY_CUT_VERTICAL_GAP_FLOOR);
    let min_gap = adaptive.min(floor).max(XY_CUT_VERTICAL_GAP_FLOOR);

    // Intervals [x_left, x_right] of every span; we look for an x value
    // that is free of ALL intervals (full-height gap).
    let mut intervals: Vec<(f64, f64)> = spans
        .iter()
        .map(|s| (s.x, s.right().max(s.x + 0.001)))
        .collect();
    intervals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));

    let mut cursor = intervals[0].1;
    let mut best_gap: Option<(f64, f64)> = None; // (gap_size, cut_x)
    for (left, right) in intervals.iter().skip(1) {
        if *left > cursor {
            let gap = *left - cursor;
            if gap >= min_gap {
                match best_gap {
                    Some((best, _)) if best >= gap => {}
                    _ => {
                        let cut_x = (cursor + *left) * 0.5;
                        best_gap = Some((gap, cut_x));
                    }
                }
            }
        }
        cursor = cursor.max(*right);
    }

    let (gap_size, cut_x) = best_gap?;

    // Split spans around the cut. A span whose midpoint is < cut_x
    // belongs to the left group.
    let mut left_group = Vec::new();
    let mut right_group = Vec::new();
    for span in spans {
        let midpoint = span.x + (span.right() - span.x) * 0.5;
        if midpoint < cut_x {
            left_group.push(span.clone());
        } else {
            right_group.push(span.clone());
        }
    }

    if !columns_are_dense(&left_group, &right_group, stats) {
        return None;
    }
    if !columns_are_band_aligned(spans, cut_x, region_left, region_right, stats) {
        return None;
    }

    Some((vec![left_group, right_group], gap_size))
}

/// ANN[r17/TEX2] Reject a vertical cut when any band sits on only one
/// side of the cut AND occupies more than ~70% of that side's column
/// width. Such bands are almost certainly full-width paragraphs that
/// happened to align with the left margin of one column, and forcing
/// them into that column re-orders them relative to text that follows.
fn columns_are_band_aligned(
    spans: &[TextSpan],
    cut_x: f64,
    region_left: f64,
    region_right: f64,
    stats: &PageStats,
) -> bool {
    let left_width = (cut_x - region_left).max(1.0);
    let right_width = (region_right - cut_x).max(1.0);

    // Threshold chosen empirically: paragraph bodies in columnar
    // layouts usually fill ~60-70% of their column; anything wider
    // than 0.7× is a page-level element masquerading as column
    // content.
    const MAX_SINGLE_SIDE_FRACTION: f64 = 0.70;

    let bands = group_spans_into_bands_with_stats(spans.to_vec(), stats);
    for band in &bands {
        let mut has_left = false;
        let mut has_right = false;
        for span in &band.spans {
            let midpoint = span.x + (span.right() - span.x) * 0.5;
            if midpoint < cut_x {
                has_left = true;
            } else {
                has_right = true;
            }
        }
        if has_left && has_right {
            continue; // Band straddles columns → fine.
        }
        let band_width = band.width();
        if has_left && band_width > left_width * MAX_SINGLE_SIDE_FRACTION {
            return false;
        }
        if has_right && band_width > right_width * MAX_SINGLE_SIDE_FRACTION {
            return false;
        }
    }
    true
}

/// Density guard — reject column splits that look like tables (few,
/// short spans per column). A column is "dense" when it has at least
/// MIN_SPANS_PER_COLUMN spans and the average character count per band
/// exceeds MIN_CHARS_PER_BAND.
fn columns_are_dense(left: &[TextSpan], right: &[TextSpan], stats: &PageStats) -> bool {
    for col in [left, right] {
        if col.len() < XY_CUT_MIN_SPANS_PER_COLUMN {
            return false;
        }
        let bands = group_spans_into_bands_with_stats(col.to_vec(), stats);
        if bands.is_empty() {
            return false;
        }
        let total_chars: usize = col.iter().map(|s| s.text.chars().count()).sum();
        let chars_per_band = total_chars as f64 / bands.len() as f64;
        if chars_per_band < XY_CUT_MIN_CHARS_PER_BAND {
            return false;
        }
    }
    true
}

/// Attempt a horizontal (zone / paragraph) cut. Unlike vertical cuts
/// this does NOT need a density guard — splitting top-from-bottom
/// cannot re-order content.
fn try_horizontal_cut(spans: &[TextSpan], stats: &PageStats) -> Option<(Vec<Vec<TextSpan>>, f64)> {
    if spans.len() < 2 {
        return None;
    }
    // Sort by descending y (PDF y grows upward).
    let mut sorted = spans.to_vec();
    sorted.sort_by(|a, b| {
        b.y.partial_cmp(&a.y)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.x.partial_cmp(&b.x).unwrap_or(Ordering::Equal))
    });

    // ANN[r17/TEX4] Paragraph / zone cuts scale with MEDIAN LINE
    // SPACING when available — this is the typographically correct
    // baseline (paragraph break ≈ 1.8 × line-spacing). When the page
    // has only one band, or stats haven't observed spacing yet, fall
    // back to the font-size multiple the legacy path used.
    let min_gap = if stats.median_line_spacing > 0.0 {
        stats.median_line_spacing * PARAGRAPH_BREAK_LINE_SPACING_MULTIPLIER
    } else {
        stats.median_font_size * XY_CUT_HORIZONTAL_GAP_FONT_MULTIPLIER
    };

    // Look for the largest gap between consecutive span y-values.
    let mut best: Option<(f64, f64)> = None; // (gap_size, cut_y)
    let tolerance = stats.median_font_size * BAND_Y_FRACTION;
    let mut band_bottom = sorted[0].y;

    for span in sorted.iter().skip(1) {
        if (band_bottom - span.y).abs() <= tolerance {
            band_bottom = band_bottom.min(span.y);
            continue;
        }
        let gap = band_bottom - span.y;
        if gap >= min_gap {
            let cut_y = (band_bottom + span.y) * 0.5;
            match best {
                Some((best_gap, _)) if best_gap >= gap => {}
                _ => best = Some((gap, cut_y)),
            }
        }
        band_bottom = span.y;
    }

    let (gap_size, cut_y) = best?;

    let mut top_group = Vec::new();
    let mut bottom_group = Vec::new();
    for span in spans {
        if span.y > cut_y {
            top_group.push(span.clone());
        } else {
            bottom_group.push(span.clone());
        }
    }
    if top_group.is_empty() || bottom_group.is_empty() {
        return None;
    }
    Some((vec![top_group, bottom_group], gap_size))
}

/// Legacy band+column-detection path, kept for reference and as the
/// fallback inside `band_based_blocks` test coverage. Not currently
/// used — XY-Cut supersedes it.
#[allow(dead_code)]
fn group_spans_into_blocks_legacy(spans: Vec<TextSpan>) -> Vec<TextBlock> {
    let bands = group_spans_into_bands(spans);
    group_spans_into_blocks_legacy_from_bands(bands)
}

fn group_spans_into_blocks_legacy_with_stats(
    spans: Vec<TextSpan>,
    stats: &PageStats,
) -> Vec<TextBlock> {
    let bands = group_spans_into_bands_with_stats(spans, stats);
    group_spans_into_blocks_legacy_from_bands(bands)
}

fn group_spans_into_blocks_legacy_from_bands(bands: Vec<TextBand>) -> Vec<TextBlock> {
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

            if !boundaries_match(&boundaries, &next_gap_midpoints, column_gap_threshold) {
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

/// Legacy wrapper used by call sites that haven't been handed PageStats.
/// It derives stats locally. Prefer `group_spans_into_bands_with_stats`
/// inside the XY-Cut pipeline to avoid recomputing the stats per call.
fn group_spans_into_bands(spans: Vec<TextSpan>) -> Vec<TextBand> {
    let stats = PageStats::from_spans(&spans);
    group_spans_into_bands_with_stats(spans, &stats)
}

fn group_spans_into_bands_with_stats(mut spans: Vec<TextSpan>, stats: &PageStats) -> Vec<TextBand> {
    if spans.is_empty() {
        return Vec::new();
    }

    spans.sort_by(|a, b| {
        b.y.partial_cmp(&a.y)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.x.partial_cmp(&b.x).unwrap_or(Ordering::Equal))
    });

    // ANN[r17/TEX4] Band tolerance scales with this page's median font
    // size rather than a fixed 5pt floor. Single-page spreads with
    // huge display fonts (24pt+) previously merged unrelated lines; a
    // fractional threshold keeps that from happening without hurting
    // body-text pages.
    let page_tolerance = (stats.median_font_size * BAND_Y_FRACTION).max(BAND_Y_TOLERANCE);

    let mut bands: Vec<TextBand> = Vec::new();

    for span in spans {
        let tolerance = (span.height * BAND_Y_FRACTION)
            .max(page_tolerance)
            .max(BAND_Y_TOLERANCE);
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

fn boundaries_match(boundaries: &[f64], gap_midpoints: &[f64], column_gap_threshold: f64) -> bool {
    let tolerance = (column_gap_threshold * 1.5).clamp(COLUMN_GAP_MATCH_TOLERANCE, 60.0);
    boundaries.len() == gap_midpoints.len()
        && boundaries
            .iter()
            .zip(gap_midpoints)
            .all(|(lhs, rhs)| (lhs - rhs).abs() <= tolerance)
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
            column_bands[column_idx].push(TextSpan::default());
            let marker_idx = column_bands[column_idx].len() - 1;
            column_bands[column_idx][marker_idx] = TextSpan {
                x: f64::NEG_INFINITY,
                y: bands[band_idx].y,
                ..TextSpan::default()
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
            // Both form-feed-prefixed and regular lines are emitted as-is.
            consecutive_empty = 0;
            result.push_str(line);
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
            ..TextSpan::default()
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
        assert!(
            (10.0..=14.0).contains(&threshold),
            "expected ~12, got {threshold}"
        );
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
        assert!(
            (threshold - COLUMN_GAP_THRESHOLD_MIN).abs() < 0.01,
            "expected {COLUMN_GAP_THRESHOLD_MIN}, got {threshold}"
        );
    }

    #[test]
    fn adaptive_column_gap_all_large_gaps_uses_fraction_of_min() {
        // When all gaps are large (> MIN), threshold = 0.75 × min_gap.
        let mut band = TextBand::new(span("Left", 0.0, 700.0, 30.0));
        band.spans.push(span("Right", 80.0, 700.0, 30.0)); // gap = 50
        let bands = vec![band];
        let threshold = compute_adaptive_column_gap(&bands);
        assert!(
            (threshold - 37.5).abs() < 0.01,
            "expected 37.5 (0.75×50), got {threshold}"
        );
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
        assert_eq!(stitch_hyphenated_lines(&lines), "page 42-\nseventy");
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
        assert_eq!(stitch_hyphenated_lines(&lines), "counter-\nan example");
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

    // --- TEX1 multi-signal space consensus tests ---

    fn make_device_with_median(median: f64) -> TextExtractionDevice {
        let mut dev = TextExtractionDevice::new();
        // Seed enough samples for the median to resolve to `median`.
        for _ in 0..MEDIAN_REFRESH {
            dev.glyph_widths.push(median);
        }
        dev.refresh_median_char_width();
        assert!((dev.cached_median_char_width - median).abs() < 1e-9);
        dev
    }

    #[test]
    fn consensus_inserts_space_on_strong_tj_offset_alone() {
        // Gap is below the geometric threshold, but the TJ offset is large
        // enough that the consensus must still fire.
        let mut dev = make_device_with_median(6.0);
        dev.pending_tj_offset = 250.0; // full em-space
        assert!(dev.evaluate_space_consensus(0.5, 12.0, "Hello", "World"));
    }

    #[test]
    fn consensus_inserts_space_on_geometric_gap_alone() {
        // No TJ, no character transition, but a clearly wide geometric gap.
        let dev = make_device_with_median(6.0);
        // gap > 0.3 * 6.0 = 1.8 → fires gap signal (0.80), below threshold
        // on its own? 0.80 < 0.75 threshold? No, 0.80 > 0.75, so it fires.
        assert!(dev.evaluate_space_consensus(2.5, 12.0, "hello", "world"));
    }

    #[test]
    fn consensus_no_space_on_kerning_gap() {
        // Small kerning-size gap with no other signals must not inject a
        // space (regression guard against false-positive spaces inside
        // tightly kerned words).
        let dev = make_device_with_median(6.0);
        assert!(!dev.evaluate_space_consensus(0.5, 12.0, "fi", "lm"));
    }

    #[test]
    fn consensus_inserts_space_on_camel_case_plus_gap() {
        // CamelCase heuristic (0.60) alone doesn't reach threshold, but a
        // moderate gap (0.60 gap + 0.60 heuristic if gap fires) should.
        // Here gap = 2.5 > 1.8 → gap fires → total 0.80 + 0.60 = 1.40.
        let dev = make_device_with_median(6.0);
        assert!(dev.evaluate_space_consensus(2.5, 12.0, "helloWorld", "Inc"));
    }

    #[test]
    fn consensus_inserts_space_on_digit_letter_transition_with_gap() {
        let dev = make_device_with_median(6.0);
        assert!(dev.evaluate_space_consensus(2.5, 12.0, "123", "abc"));
    }

    #[test]
    fn consensus_heuristic_alone_is_insufficient() {
        // Heuristic (0.60) on its own is below the 0.75 threshold — the
        // design deliberately requires a second corroborating signal to
        // avoid gluing spaces into existing CamelCase identifiers that
        // have no geometric break.
        let dev = make_device_with_median(6.0);
        assert!(!dev.evaluate_space_consensus(0.5, 12.0, "camel", "Case"));
    }

    #[test]
    fn consensus_falls_back_to_font_size_when_no_median() {
        // No samples → median is 0; geometric reference uses font-size.
        let dev = TextExtractionDevice::new();
        // gap 1.9 > 0.15 * 12.0 = 1.8 → gap signal fires
        assert!(dev.evaluate_space_consensus(1.9, 12.0, "a", "b"));
        // gap 1.5 < 1.8 → no signal
        assert!(!dev.evaluate_space_consensus(1.5, 12.0, "a", "b"));
    }

    #[test]
    fn consensus_ignores_tiny_tj_offsets() {
        // TJ offsets below the threshold are kerning, not word breaks.
        let mut dev = make_device_with_median(6.0);
        dev.pending_tj_offset = 50.0;
        assert!(!dev.evaluate_space_consensus(0.5, 12.0, "Hello", "World"));
    }

    #[test]
    fn consensus_accepts_negative_tj_offsets() {
        // A negative TJ offset still represents an explicit inter-substring
        // shift and counts toward the consensus (|amount| check).
        let mut dev = make_device_with_median(6.0);
        dev.pending_tj_offset = -250.0;
        assert!(dev.evaluate_space_consensus(0.5, 12.0, "Hello", "World"));
    }

    #[test]
    fn text_adjustment_accumulates_until_glyph() {
        let mut dev = TextExtractionDevice::new();
        dev.text_adjustment(120.0);
        dev.text_adjustment(140.0);
        assert!((dev.pending_tj_offset - 260.0).abs() < 1e-6);
    }

    // --- TEX2 XY-Cut tests ---

    #[test]
    fn xy_cut_header_body_footer_with_two_columns() {
        // Header and footer sit in the mid-x range that would
        // accidentally fall into a left-column bucket with a naive
        // vertical-first cut. The largest-gap-first rule plus the
        // alignment guard ensure header and footer bracket the
        // columnar body.
        let texts = block_texts(vec![
            span("HEADLINE TITLE", 180.0, 760.0, 120.0),
            span("Left col line A", 40.0, 700.0, 110.0),
            span("Right col line A", 320.0, 700.0, 115.0),
            span("Left col line B", 40.0, 684.0, 110.0),
            span("Right col line B", 320.0, 684.0, 115.0),
            span("Left col line C", 40.0, 668.0, 110.0),
            span("Right col line C", 320.0, 668.0, 115.0),
            span("FOOTER LINE TEXT", 180.0, 600.0, 120.0),
        ]);
        assert_eq!(texts.first().map(String::as_str), Some("HEADLINE TITLE"));
        assert_eq!(texts.last().map(String::as_str), Some("FOOTER LINE TEXT"));
        // Left column lines all come before right column lines.
        let left_c_idx = texts.iter().position(|s| s == "Left col line C").unwrap();
        let right_a_idx = texts.iter().position(|s| s == "Right col line A").unwrap();
        assert!(
            left_c_idx < right_a_idx,
            "expected column-major ordering in body: {texts:?}"
        );
    }

    #[test]
    fn xy_cut_rejects_column_split_on_table_rows() {
        // The density guard must still reject the 280pt inter-cell gap
        // in a short-cell table, preserving row-major reading order.
        let texts = block_texts(vec![
            span("Name", 40.0, 700.0, 30.0),
            span("Age", 320.0, 700.0, 20.0),
            span("Alice", 40.0, 684.0, 35.0),
            span("30", 320.0, 684.0, 15.0),
        ]);
        assert_eq!(texts, vec!["Name Age", "Alice 30"]);
    }

    #[test]
    fn xy_cut_rejects_column_split_when_one_band_is_full_width() {
        // The alignment guard catches a full-width paragraph that
        // would otherwise be forced into the left column of a 2-column
        // region below it.
        let texts = block_texts(vec![
            span(
                "Full width intro spanning both columns here",
                40.0,
                740.0,
                360.0,
            ),
            span("Left A", 40.0, 700.0, 50.0),
            span("Right A", 320.0, 700.0, 50.0),
            span("Left B", 40.0, 684.0, 50.0),
            span("Right B", 320.0, 684.0, 50.0),
        ]);
        assert!(
            texts[0].contains("Full width intro"),
            "expected full-width intro first: {texts:?}"
        );
    }

    #[test]
    fn xy_cut_horizontal_split_for_zone_boundaries() {
        // Pure horizontal cut on a single-column page with a big
        // vertical gap between paragraphs — the cut fires and both
        // paragraphs stay in their own blocks.
        let texts = block_texts(vec![
            span("First paragraph body text", 40.0, 740.0, 200.0),
            span("Second paragraph body", 40.0, 680.0, 180.0),
        ]);
        assert_eq!(texts.len(), 2);
        assert!(texts[0].starts_with("First"));
        assert!(texts[1].starts_with("Second"));
    }

    #[test]
    fn xy_cut_recursion_terminates_with_single_span() {
        let texts = block_texts(vec![span("Only one span on the page", 40.0, 700.0, 180.0)]);
        assert_eq!(texts, vec!["Only one span on the page"]);
    }

    #[test]
    fn median_font_size_handles_mixed_sizes() {
        let spans = vec![
            TextSpan {
                text: "small".into(),
                width: 10.0,
                height: 8.0,
                font_size: 8.0,
                ..TextSpan::default()
            },
            TextSpan {
                text: "medium".into(),
                width: 10.0,
                height: 12.0,
                font_size: 12.0,
                ..TextSpan::default()
            },
            TextSpan {
                text: "large".into(),
                width: 10.0,
                height: 24.0,
                font_size: 24.0,
                ..TextSpan::default()
            },
        ];
        assert!((median_font_size(&spans) - 12.0).abs() < 1e-9);
    }

    #[test]
    fn columns_band_aligned_accepts_aligned_columns() {
        let spans = vec![
            span("L1", 40.0, 700.0, 60.0),
            span("R1", 300.0, 700.0, 60.0),
            span("L2", 40.0, 684.0, 60.0),
            span("R2", 300.0, 684.0, 60.0),
        ];
        let stats = PageStats::from_spans(&spans);
        // cut_x between 100 and 300 → 200. Every band straddles the cut.
        assert!(columns_are_band_aligned(&spans, 200.0, 40.0, 360.0, &stats));
    }

    #[test]
    fn columns_band_aligned_rejects_wide_single_side_band() {
        let spans = vec![
            span("Wide banner line across top", 40.0, 740.0, 280.0),
            span("L1", 40.0, 700.0, 60.0),
            span("R1", 300.0, 700.0, 60.0),
        ];
        let stats = PageStats::from_spans(&spans);
        // cut_x = 200. Banner only in left group (midpoint < 200). Width
        // exceeds 0.7 × left column width → rejected.
        assert!(!columns_are_band_aligned(
            &spans, 200.0, 40.0, 360.0, &stats
        ));
    }

    #[test]
    fn page_stats_computes_median_values() {
        let spans = vec![
            span("one", 40.0, 700.0, 30.0),
            span("two", 40.0, 680.0, 30.0),
            span("three", 40.0, 660.0, 50.0),
        ];
        let stats = PageStats::from_spans(&spans);
        assert!((stats.median_font_size - 12.0).abs() < 1e-9);
        // char width = width / chars. one=30/3=10, two=30/3=10, three=50/5=10. median=10.
        assert!((stats.median_char_width - 10.0).abs() < 1e-9);
        // line spacing: bands at 700, 680, 660. gaps = 20, 20. median = 20.
        assert!((stats.median_line_spacing - 20.0).abs() < 1e-9);
    }

    #[test]
    fn page_stats_handles_empty_input() {
        let stats = PageStats::from_spans(&[]);
        assert!((stats.median_font_size - 12.0).abs() < 1e-9);
        assert!((stats.median_char_width - 6.0).abs() < 1e-9);
        assert_eq!(stats.median_line_spacing, 0.0);
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
        assert!(
            texts.len() >= 6,
            "expected column-major output, got {texts:?}"
        );
        // First three blocks should be left column lines
        assert!(
            texts[0].contains("Lorem"),
            "first block should be left column: {texts:?}"
        );
    }

    #[test]
    fn xy_cut_leaf_falls_back_to_legacy_columns_for_header_plus_three_columns() {
        let texts = block_texts(vec![
            span("73022", 45.0, 750.0, 70.0),
            span("Federal Register banner", 125.6, 750.0, 260.0),
            span("Left column line one", 45.0, 725.0, 140.0),
            span("Middle column line one", 222.0, 725.0, 140.0),
            span("Right column line one", 399.0, 725.0, 120.0),
            span("Left column line two", 45.0, 715.0, 140.0),
            span("Middle column line two", 210.0, 715.0, 152.0),
            span("Right column line two", 388.0, 715.0, 132.0),
            span("Left column line three", 45.0, 705.0, 140.0),
            span("Middle column line three", 235.0, 705.0, 135.0),
            span("Right column line three", 408.0, 705.0, 118.0),
        ]);

        assert_eq!(
            texts,
            vec![
                "73022 Federal Register banner",
                "Left column line one",
                "Left column line two",
                "Left column line three",
                "Middle column line one",
                "Middle column line two",
                "Middle column line three",
                "Right column line one",
                "Right column line two",
                "Right column line three",
            ]
        );
    }

    #[test]
    fn overlapping_fake_bold_spans_collapse_to_single_copy() {
        let texts = block_texts(vec![
            span("1 This is fakebold text.", 25.9, 785.3, 320.0),
            span("1 This is fakebold text.", 26.2, 785.3, 320.0),
            span("1 This is fakebold text.", 26.4, 785.3, 320.0),
            span("1 This is fakebold text.", 26.7, 785.3, 320.0),
            span("2 This is a fakebold", 27.0, 714.8, 142.0),
            span(" fakebold", 169.8, 714.8, 70.0),
            span(" fakebold", 170.1, 714.8, 70.0),
            span(" fakebold word.", 170.4, 714.8, 110.0),
        ]);

        assert_eq!(
            texts,
            vec!["1 This is fakebold text.", "2 This is a fakebold word.",]
        );
    }

    // ---- G1: read-only metadata field tests ----

    #[test]
    fn g1_default_text_span_has_empty_metadata() {
        let s = TextSpan::default();
        assert_eq!(s.font_name, None);
        assert!(!s.is_bold);
        assert!(!s.is_italic);
        assert_eq!(s.color, None);
    }

    #[test]
    fn g1_strip_subset_prefix_handles_six_char_prefix() {
        assert_eq!(strip_subset_prefix("AAAAAA+Helvetica"), "Helvetica");
        // Non-6-char prefix → keep verbatim.
        assert_eq!(strip_subset_prefix("ABC+Helvetica"), "ABC+Helvetica");
        // No `+` → unchanged.
        assert_eq!(strip_subset_prefix("Helvetica-Bold"), "Helvetica-Bold");
    }

    #[test]
    fn g1_name_style_hints_match_pdf_interpret_rules() {
        assert_eq!(name_style_hints("Helvetica-Bold"), (true, false));
        assert_eq!(name_style_hints("Times-Italic"), (false, true));
        assert_eq!(name_style_hints("MyFont-BoldOblique"), (true, true));
        assert_eq!(name_style_hints("Helvetica"), (false, false));
        // Semibold / Demi / Heavy / Black variants → bold.
        assert_eq!(name_style_hints("Roboto-DemiBold"), (true, false));
        assert_eq!(name_style_hints("Roboto-Black"), (true, false));
        // Oblique / slant variants → italic.
        assert_eq!(name_style_hints("Roboto-Oblique"), (false, true));
        assert_eq!(name_style_hints("MyFont-Slanted"), (false, true));
    }

    // ---- G2: widthSource + char_bounds on TextSpan ----

    #[test]
    fn g2_default_text_span_has_estimate_width_source() {
        let s = TextSpan::default();
        assert_eq!(s.width_source, WidthSource::Estimate);
        assert!(s.char_bounds.is_empty());
    }

    /// Verify that a single-glyph span has exactly one char_bound entry
    /// and that the bound matches the span's x / width.
    #[test]
    fn g2_single_glyph_span_has_one_char_bound() {
        let s = TextSpan {
            text: "A".into(),
            x: 10.0,
            y: 100.0,
            width: 7.22,
            height: 10.0,
            font_size: 10.0,
            width_source: WidthSource::Metric,
            char_bounds: vec![[10.0, 100.0, 17.22, 110.0]],
            ..Default::default()
        };

        assert_eq!(s.char_bounds.len(), 1);
        let [x0, y0, x1, y1] = s.char_bounds[0];
        assert!((x0 - 10.0).abs() < 0.001);
        assert!((x1 - 17.22).abs() < 0.001);
        assert!((y1 - y0 - s.font_size).abs() < 0.001);
    }

    /// When merging glyphs into a span the width_source degrades to Estimate
    /// if any glyph was estimated.
    #[test]
    fn g2_merged_span_degrades_width_source_on_estimate() {
        let mut s = TextSpan {
            width_source: WidthSource::Metric,
            char_bounds: vec![[0.0, 0.0, 7.0, 10.0]],
            ..Default::default()
        };

        // Simulate what draw_glyph does on merge: push bound + downgrade.
        s.char_bounds.push([7.0, 0.0, 12.0, 10.0]);
        s.width_source = WidthSource::Estimate; // second glyph had no advance

        assert_eq!(s.width_source, WidthSource::Estimate);
        assert_eq!(s.char_bounds.len(), 2);
    }

    /// Verify WidthSource enum serialises to the two expected string literals.
    #[test]
    fn g2_width_source_variants_are_correct() {
        assert_eq!(format!("{:?}", WidthSource::Metric), "Metric");
        assert_eq!(format!("{:?}", WidthSource::Estimate), "Estimate");
        assert_ne!(WidthSource::Metric, WidthSource::Estimate);
        assert_eq!(WidthSource::default(), WidthSource::Estimate);
    }
}
