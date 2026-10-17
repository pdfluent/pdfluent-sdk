//! Text extraction via a custom Device implementation.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use kurbo::{Affine, BezPath, PathEl, Point, Shape};
use pdf_render::pdf_interpret::cmap::BfString;
use pdf_render::pdf_interpret::font::{Glyph, GlyphPlace, OutlineGlyph};
use pdf_render::pdf_interpret::{
    BlendMode, ClipPath, Device, GlyphDrawMode, Image, Paint, PathDrawMode, SoftMask,
};
use std::borrow::Cow;
use std::cell::Cell;
use std::cmp::Ordering;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

use crate::blocks::{Block, BlockKind, TableStrategy};
#[cfg(test)]
use crate::table_trace::TableOrigin;

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
/// How far a continuation must sit in from its marker line's left edge to
/// read as hung under the marker's text (#574). Past a third of an em the
/// indent is the list's own hang and not the page's drift.
const MARKER_HANG_MIN_EM: f64 = 0.3;
/// How far in it may sit and still hang from that line (#574). Six ems is
/// the widest marker-plus-gap on the measured pages; past it the line is a
/// new indent, not a continuation.
const MARKER_HANG_MAX_EM: f64 = 6.0;
/// How close a hung line must start to the marker line's own text column
/// where the page drew the marker in a span of its own (#574). Half an em
/// either way; the column is exact, so the tolerance is the drift.
const MARKER_TEXT_LEFT_TOL_EM: f64 = 0.5;
/// How far short of the wider line's right edge a wrapping marker line may
/// stop (#574). A line that ended an em and a half early turned back; one
/// that ran to the measure wrapped.
const MARKER_WRAP_SLACK_EM: f64 = 1.5;
/// The most white a hung continuation may leave above itself (#574). Half
/// an em over the glyphs is normal leading; more is a paragraph break.
const MARKER_GAP_MAX_EM: f64 = 0.5;

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

/// How many EARLIER gaps have to agree with the gap being judged before a run
/// reads as tracked.
///
/// Four earlier ones, so five agreeing gaps in all and a run of at least six
/// glyphs. Two letters set the same distance apart is a coincidence of two
/// sidebearings; five in a row is the typesetter holding a key down.
///
/// THE NAME USED TO SAY `PAIRS` AND THE COUNT WAS READ AS FOUR, which is the
/// gap the review found (#582): the current gap is recorded AFTER it is
/// judged, so a five-glyph word offers only three earlier gaps at its last
/// pair and never reaches this. `TITLE` and `NOTES` therefore come back as
/// `T I T L E` and `N O T E S` while `REGION` is joined -- at the glyph.
///
/// It is left at four rather than lowered to three: while a glyph is being
/// read the rest of its span is not known. The short words are reached
/// instead once the span is finished -- `retrack_a_finished_span` asks the
/// same question of all its gaps at once -- which #1973 measured on the 6,446
/// DocLayNet validation pages: nine pages change, each a tracked word read
/// right, none a line of single-letter words. That is the measurement #582
/// asked for, and `a_five_letter_tracked_word_is_read_once_its_span_is_finished`
/// pins where the boundary now is.
const TRACKING_MIN_PAIRS: usize = 4;

/// How far the gaps of a tracked run may differ from one another, as a
/// fraction of the gap being judged.
///
/// A quarter. Tracking is a constant added to every advance, so what varies
/// inside a tracked run is only the kerning of the pair -- which is why
/// `D I R E C TO R S` keeps `TO` and `RS` together: those two pairs kern
/// closed far enough to fall back under the threshold on their own.
const TRACKING_TOLERANCE: f64 = 0.25;

/// The smallest median gap between a span's glyphs, as a share of its size,
/// that is letters set apart rather than set solid (#1973). `P E N N` at 6.2
/// points stands 0.145 em apart, a 60-point display line 0.05, and prose 0 --
/// its boxes are advances and touch. 0.04 is under the lightest tracking seen
/// by a margin and far over prose.
const TRACKED_GAP_MIN_EM: f64 = 0.04;
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
/// How much of its own column the typical slice of a region may cover before
/// the region reads as prose set in columns rather than as a table (#570).
/// Under two thirds is a cell with white beside it; a line of prose fills its
/// measure.
const GRID_SLICE_FILL: f64 = 0.6;
/// How many of a region's rows a column has to appear in before the region can
/// be a grid. Three quarters, so that a table with an empty cell here and there
/// is still a table and a sidebar beside one column is not.
const GRID_COLUMN_SUPPORT: f64 = 0.75;
/// The fewest rows a grid has. Three, because two rows of two short slices is
/// the shape of a heading beside a date.
const GRID_MIN_ROWS: usize = 3;
/// The fewest labels a column of labels has. Three: two names beside two
/// paragraphs is a heading over a note as often as it is a table.
const LABEL_MIN_ROWS: usize = 3;
/// The fewest bands a side of a proposed column cut has to have ink on before
/// the gaps inside it say anything. Three, the same evidence a grid is asked
/// for: two bands that both break once are a heading beside a date.
const BANK_MIN_ROWS: usize = 3;
/// How many of a side's own bands have to break at a gap as wide as the cut's
/// before the cut is read as one gutter of a table rather than the gutter
/// between two columns. More than half: a table breaks every row of both
/// banks, and a paper set in columns with ONE table in one of its columns
/// breaks a minority of that column's lines -- which is a page whose column
/// cut is right and has to stay.
const BANK_BREAK_SHARE: f64 = 0.5;
/// How much wider than the wide side's leading a step has to be before it is
/// the start of something rather than the next line of it. The same question
/// `blocks::wrapped_leading` asks of a table's rows, and the same answer.
const LABEL_PITCH_JUMP: f64 = 1.4;

/// Two span starts this far apart are the same cell edge (#434). A cell's
/// wrapped line is set from the cell's own left edge, so the two lines start on
/// the same coordinate up to the rounding of the text matrix.
const CELL_EDGE_TOLERANCE: f64 = 2.0;
/// How far below its row a line may sit and still be that row's continuation,
/// as a multiple of the row's tallest glyph. One line of leading is ~1.2; the
/// next row of a table sits further down than this.
const WRAPPED_CELL_LINE_GAP_FACTOR: f64 = 1.8;
/// How far above the row the band that corroborates the grid may sit, counted
/// in bands. The lines between are the wrapped cells of the row above, so a
/// couple of them is all a row can put in the way.
const TABLE_GRID_MAX_LOOKBACK: usize = 4;
/// The share of the grid's own row pitch a continuation may sit below the line
/// it continues. A row's pitch is one line plus the cell's padding and a
/// wrapped line is one line, so a line at the full pitch is the next row.
const WRAPPED_CELL_MAX_PITCH_FRACTION: f64 = 0.9;
/// A grid is a claim about columns, so the band that corroborates it has to
/// carry at least this many of them. Two, because a row of the table is
/// allowed an empty cell.
const TABLE_GRID_MIN_COLUMNS: usize = 2;
/// A drawn path this flat is a line and not a box (#545). The ruling lines of
/// the Isartor manual's appendix are 0.18 pt of ink; a hairline stroke is a
/// path of no height at all, and a table's cell shading is eighteen points.
const RULE_MAX_THICKNESS: f64 = 3.0;
/// A line shorter than this separates nothing: it is a tick, a bullet or the
/// underline of a word.
const RULE_MIN_LENGTH: f64 = 24.0;
/// How many path boxes are kept per page before they are clustered: a map
/// drawn from ten thousand paths is one figure, and past this the rest add
/// nothing to where it is.
const MAX_RECORDED_PATH_BOXES: usize = 4000;

/// How far apart two path boxes may be, in points, and still be one figure.
const GRAPHIC_GAP: f64 = 2.0;

fn touches(a: &[f64; 4], b: &[f64; 4], gap: f64) -> bool {
    a[0] <= b[2] + gap && b[0] <= a[2] + gap && a[1] <= b[3] + gap && b[1] <= a[3] + gap
}

fn union(a: &[f64; 4], b: &[f64; 4]) -> [f64; 4] {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}

/// How many image boxes are kept per page: a page tiled from hundreds of
/// image strips is a scan, not a page of pictures to caption (#636).
const MAX_RECORDED_IMAGES: usize = 256;

/// How many rules are kept. A chart draws thousands of short flat segments and
/// none of them rules a row; the cap bounds the memory a pathological page can
/// ask for without changing what any real table looks like.
const MAX_RECORDED_RULES: usize = 512;
/// How many lattice rulings are kept (#574). A timetable draws its cells as
/// several hundred boxes, four edges each; a chart can draw tens of thousands
/// of segments, and past this the page is not a grid a lattice should read.
const MAX_RECORDED_RULINGS: usize = 8192;
/// Grey at or above this (0..255) is paper, not shading (#574).
const FILL_WHITE_ABOVE: u32 = 247;
/// The smallest fill, in points, that can stand behind a line of text.
const FILL_MIN_WIDTH: f64 = 10.0;
/// The smallest fill, in points, that can stand behind a line of text.
const FILL_MIN_HEIGHT: f64 = 4.0;
/// A lattice edge shorter than this is a tick, not a cell border.
const RULING_MIN_LENGTH: f64 = 3.0;
/// The share of a region's width a rule has to cross before it counts as
/// separating that region's rows. A rule between two rows runs the whole table;
/// a rule under one cell does not.
const RULE_SPAN_FRACTION: f64 = 0.9;
/// How many of a region's band gaps have to carry a rule before the region is
/// a ruled table. Two, so that a single horizontal line -- a header rule, the
/// line above a footnote -- never speaks for the page, while a region the
/// recursion has already cut down to three rows can still be recognised as the
/// table it is. Each of the two has to cross the whole region and lie between
/// two bands that follow each other, which one rule under a headline does not.
const MIN_RULED_ROW_SEPARATORS: usize = 2;
/// The share of a region's band gaps that have to carry such a rule. Half,
/// because a row whose cell wraps puts a band inside itself, and that gap has
/// no rule in it.
const MIN_RULED_GAP_SUPPORT: f64 = 0.5;

/// How many cells the row itself has to fill before a line under it is read as
/// one of its cells running on.
///
/// Three, because two is what a page set in columns looks like: its lines
/// start on the same two edges as a two-cell row does, one of its columns ends
/// before the other, and the line left over then reads exactly like a wrapped
/// cell. On `tracemonkey` p13 it is one -- and the page is two columns of
/// prose, where moving a line into "its cell" is the column cut of #389 class
/// O-B done on one band instead of the region. A two-column table pays for
/// that: its wrapped cells are the other half of #545.
const TABLE_ROW_MIN_CELLS: usize = 3;

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

/// Controls the richness of per-glyph geometry extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GeometryMode {
    /// Advance-based bounds only (current default).
    #[default]
    Basic,
    /// Full tight glyph bounds from outline paths.
    RichGeometry,
}

/// Provenance of per-glyph bounding boxes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum BoundsSource {
    /// Advance bounds.
    #[default]
    Advance,
    /// Path-derived tight bounds.
    Tight,
    /// Fallback (no outline available).
    Estimate,
}

/// Vertical font metrics in /1000 em (1000 units-per-em), sourced from the
/// embedded font's OS/2 / hhea tables via skrifa. `ascent` is positive (above
/// the baseline); `descent` is negative (below it). The whole struct is `None`
/// on a span when no embedded font binary exposes metrics (non-embedded
/// standard-14, CFF, Type1, Type3).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct FontMetrics {
    /// Ascent above the baseline, in /1000 em.
    pub ascent: f64,
    /// Descent below the baseline (negative), in /1000 em.
    pub descent: f64,
    /// Cap height in /1000 em, when present in the font.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "capHeight", skip_serializing_if = "Option::is_none")
    )]
    pub cap_height: Option<f64>,
    /// x-height in /1000 em, when present in the font.
    #[cfg_attr(
        feature = "serde",
        serde(rename = "xHeight", skip_serializing_if = "Option::is_none")
    )]
    pub x_height: Option<f64>,
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
    /// Whether a content run ends after this span: a consumer joining
    /// same-style spans must not merge past it (#643). Only set on the raw
    /// path; the default path leaves every span open, as before.
    pub complete_run: bool,

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

    // ---- M3 rich geometry (added 2026-06; backward-compatible) ----
    #[doc(hidden)]
    pub geometry_mode: GeometryMode,
    #[doc(hidden)]
    pub bounds_source: BoundsSource,
    #[doc(hidden)]
    pub tight_char_bounds: Vec<[f64; 4]>,
    #[doc(hidden)]
    pub glyph_advances: Vec<f64>,
    #[doc(hidden)]
    pub glyph_bounds_sources: Vec<BoundsSource>,

    // ---- Golf 1 typographic metadata (added 2026-06; backward-compatible) ----
    /// Full affine transform of the span's first glyph in user space,
    /// `[a, b, c, d, e, f]` (kurbo coeffs of CTM × text-matrix). Captures
    /// rotation and shear that `(x, y, font_size)` discards. Text placed by a
    /// rectangle rather than drawn by a glyph (a form field's value with no
    /// appearance stream, #549) carries the placement in the same form.
    /// `None` for spans built from neither (e.g. synthetic markers).
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
    /// span's first glyph, and `0` for text placed by a rectangle rather than
    /// drawn (#549). `None` for spans built from neither.
    pub render_mode: Option<u8>,
    /// Whether the span was set in the engine's own OCR layer resource
    /// ([`pdf_ocr::pipeline::OCR_FONT_NAME`]). The hidden-text drop keeps
    /// these spans while author-hidden text goes, because the layer is the
    /// engine's own voice, not ink the author hid.
    pub is_ocr_layer: bool,

    // ---- Golf 2 font metrics (added 2026-06; backward-compatible) ----
    /// Vertical font metrics (ascent/descent, optional cap/x-height) in /1000
    /// em from the embedded font binary. `None` for non-embedded fonts.
    pub font_metrics: Option<FontMetrics>,
}

impl TextSpan {
    /// Whether this span is text the page hides: drawn with render mode 3
    /// (invisible) or painted with zero alpha (lane L1).
    ///
    /// The render mode comes from the span's first glyph and the alpha from
    /// its fill paint; a span with no paint information counts as visible.
    pub fn is_hidden(&self) -> bool {
        if self.render_mode == Some(3) {
            return true;
        }
        if let Some(color) = self.color {
            if color[3] == 0 {
                return true;
            }
        }
        false
    }

    /// Conservative right edge using whichever is wider: measured or estimated.
    /// Used by column detection to avoid underestimating span extent.
    ///
    /// Measured only for a quarter-turned span (#574): the estimate counts
    /// characters times a nominal advance, which runs ALONG the baseline, and
    /// for a baseline that stands up the page that length is not an x extent
    /// at all. Widening x by it lays a 29-glyph column head flat across the
    /// next column and closes whatever gutter it lands on. A free-angle span
    /// keeps the estimate, as before: its baseline still advances in x.
    fn right(&self) -> f64 {
        if span_is_quarter_turned(self) {
            return self.x + self.width;
        }
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
    /// Concatenate all spans into a single string, in reading order.
    ///
    /// Spans that are close together are joined without a separator;
    /// a space is inserted when the gap between spans exceeds half
    /// the average character width.
    ///
    /// ANN[r17/TEX5] The spans are in the order the page draws them, left to
    /// right, and for Arabic or Hebrew that is the reverse of the order they
    /// are read in (#436, class R-H of #389). The assembled line therefore goes
    /// through [`crate::bidi::logical_order`] before it is returned: a line
    /// with no right-to-left character comes back untouched, and one that has
    /// them comes back with each of its runs the way a reader would type it.
    /// The spans themselves are left alone -- they carry the geometry, and the
    /// geometry is visual.
    pub fn text(&self) -> String {
        crate::bidi::logical_order(&self.visual_text()).into_owned()
    }

    /// [`text`](Self::text) with the cell-path word rules where
    /// `cell_path_words` is true: a word space set at exactly a quarter em
    /// reads as a space, exactly as without the option otherwise.
    pub(crate) fn text_with_cell_path_words(&self, cell_path_words: bool) -> String {
        crate::bidi::logical_order(&self.visual_text_with(cell_path_words)).into_owned()
    }

    /// The line as the page draws it, left to right. The order every geometric
    /// decision above this point was made in.
    fn visual_text(&self) -> String {
        self.visual_text_with(false)
    }

    /// [`visual_text`](Self::visual_text) with the cell-path word rules where
    /// `cell_path_words` is true.
    fn visual_text_with(&self, cell_path_words: bool) -> String {
        if self.spans.is_empty() {
            return String::new();
        }
        let spans: &[TextSpan] = &self.spans;
        // What goes between each pair, decided pair by pair first and then
        // looked at as a row: letter-spacing is a property of the row.
        enum Join {
            Overlap(String),
            Gap(f64, bool),
        }
        let mut joins: Vec<Join> = Vec::with_capacity(spans.len());
        // ANN[r17/TEX5] Where the line got to, which is not always the span
        // before this one: a combining mark is drawn on the glyph before it and
        // moves the pen nowhere, so its right edge sits an advance short and
        // the letter after it looks a word apart. A Hebrew word with points
        // came out `שָׁל וֹם` -- one word, two "words" (#436). A span that is
        // nothing but marks does not say where the pen is, so the last one that
        // does answers instead.
        let mut anchor = &spans[0];
        for pair in spans.windows(2) {
            let prev = &pair[0];
            let curr = &pair[1];
            if !crate::bidi::is_only_marks(&prev.text) {
                anchor = prev;
            }
            // The gap is measured along the baseline the anchor was drawn on:
            // for upright text that is `curr.x - anchor.measured_right()` to
            // the last bit, for a rotated run it is the distance the reader
            // sees between the two (#435).
            let direction = span_direction(anchor);
            let (curr_along, _) = baseline_coordinates(curr, direction);
            let gap = curr_along - baseline_end(anchor, direction);
            // W1 (#663): the text join beside the shared word-join rule. A gap
            // under 0.15 em is under 0.25 em, so `pieces_join` implies the rule
            // below by arithmetic: this asserts without changing a byte, and
            // shows the rewrite where the shared call goes.
            debug_assert!(
                !crate::words_join::pieces_join(
                    gap,
                    (baseline_coordinates(curr, direction).1
                        - baseline_coordinates(anchor, direction).1)
                        .abs(),
                    prev.font_size
                ) || gap <= prev.font_size * 0.25,
                "the shared join implies the text join"
            );
            if gap <= prev.font_size * 0.12 && second_run_carries_on_from_the_first(prev, curr) {
                if let Some(trimmed) = trim_overlapping_word_prefix(&prev.text, &curr.text) {
                    joins.push(Join::Overlap(trimmed));
                    continue;
                }
            }
            if space_drawn_over_previous_run(prev, curr, gap) {
                joins.push(Join::Overlap(curr.text.trim_start_matches(' ').to_string()));
                continue;
            }
            let space = if cell_path_words {
                // At the boundary, a space: a word space set at exactly a
                // quarter em is still a word space (`financial officer`).
                gap >= prev.font_size * 0.25
            } else {
                gap > prev.font_size * 0.25
            };
            joins.push(Join::Gap(gap, space));
        }
        untrack_spaced_spans(spans, &mut joins, |j| match j {
            Join::Gap(gap, space) => Some((*gap, space)),
            Join::Overlap(_) => None,
        });
        let mut result = spans[0].text.clone();
        for (curr, join) in spans[1..].iter().zip(joins) {
            match join {
                Join::Overlap(trimmed) => result.push_str(&trimmed),
                Join::Gap(_, space) => {
                    if space {
                        result.push(' ');
                    }
                    result.push_str(&curr.text);
                }
            }
        }
        result
    }
}

/// Letter-spacing drawn as a row of one-letter spans (#1973).
///
/// #582 reads tracking inside a span. When the tracking is wide -- a space
/// glyph after every capital, as in `P O W E L L  I N D U S T R I E S` set at
/// five points -- every letter is a span of its own, the space glyphs never
/// reach the text, and each gap between the spans is over the word-space
/// threshold: the line came out one letter per word. On the evidence of the
/// 24-09 round that is the largest class of text we write that the page does
/// not hold (0.74 % of our words on DocLayNet, against Docling's 0.07 %).
///
/// A run of four one-letter spans or more (a kerned pair such as `AT` drawn as
/// one does not end it) is read as letter-spacing only when THE PAGE SAYS
/// WHERE ITS WORDS PART: a space glyph drawn as a span of its own between them
/// (`P O W E L L` + a space span + `I N D U S T R I E S`), or gaps that fall in
/// two -- the letters of a word one distance apart and the words clearly
/// further, the widest gap at least 1.6 times the narrowest. The letters of a
/// word then lose their spaces and the words keep one. A row whose gaps all agree
/// says nothing about where its words part, and is left as it is: letters set
/// apart on purpose and letters tracked apart are the same page, and that is
/// the choice `a_line_of_single_letter_words_reads_as_tracking_and_that_is_the_choice`
/// already pins inside a span.
fn untrack_spaced_spans<J>(
    spans: &[TextSpan],
    joins: &mut [J],
    mut gap_of: impl FnMut(&mut J) -> Option<(f64, &mut bool)>,
) {
    let blank = |span: &TextSpan| span.text.trim().is_empty();
    // Capitals, and the punctuation a line of them carries: letter-spaced
    // display type is set in capitals, and a row of lower-case letters,
    // digits or symbols one apart is a formula or a listing (`NK(S) ∩ T`,
    // `# 0 1 2 3`), which this must not touch.
    let small = |span: &TextSpan| {
        let t = span.text.trim();
        let n = t.chars().count();
        (n == 1 || n == 2)
            && t.chars()
                .all(|c| c.is_uppercase() || matches!(c, ',' | '.' | '\'' | '’' | '&' | '-'))
            && (n == 1 || t.chars().any(char::is_uppercase))
    };
    let mut start = 0;
    while start < spans.len() {
        if !small(&spans[start]) {
            start += 1;
            continue;
        }
        let mut end = start + 1;
        while end < spans.len()
            && (small(&spans[end]) || blank(&spans[end]))
            && gap_of(&mut joins[end - 1]).is_some()
        {
            end += 1;
        }
        // A space drawn at the end of the run is not inside it, and neither
        // is a last "word" of one letter after it: `F C F C` + a space + `S a
        // m p l e` is a figure's label beside a word, not two tracked words.
        loop {
            while end > start + 1 && blank(&spans[end - 1]) {
                end -= 1;
            }
            let tail = spans[start..end].iter().rposition(blank);
            match tail {
                Some(at)
                    if spans[start + at + 1..end]
                        .iter()
                        .filter(|s| s.text.trim().chars().any(char::is_uppercase))
                        .count()
                        < 2 =>
                {
                    end = start + at;
                }
                _ => break,
            }
        }
        let singles = spans[start..end]
            .iter()
            .filter(|s| {
                let t = s.text.trim();
                t.chars().count() == 1 && t.chars().all(char::is_uppercase)
            })
            .count();
        // THE PAGE DREW ITS WORD SPACES: a space glyph set as a span of its
        // own where the words part, which is the boundary. The gaps between the
        // letters are tracking and lose their space; a gap much wider than the
        // run's tracking keeps it -- the end of a line, or a word the page
        // parted without a drawn space. Without a drawn space the row is left
        // as it is: gaps that fall in two were tried as the boundary and read
        // figure labels and OCR noise (`B C B C`, `I I I`) as words.
        if singles >= 4 && spans[start..end].iter().any(blank) {
            let mut tracking: Vec<f64> = (start..end - 1)
                .filter(|&k| !blank(&spans[k]) && !blank(&spans[k + 1]))
                .filter_map(|k| gap_of(&mut joins[k]).map(|(g, _)| g))
                .collect();
            tracking.sort_by(|a, b| a.total_cmp(b));
            let median = tracking.get(tracking.len() / 2).copied().unwrap_or(0.0);
            for k in start..end - 1 {
                let beside_a_drawn_space = blank(&spans[k]) || blank(&spans[k + 1]);
                if let Some((gap, space)) = gap_of(&mut joins[k]) {
                    if beside_a_drawn_space || gap <= 1.5 * median.max(0.0) {
                        *space = false;
                    }
                }
            }
        }
        // Never stand still: a run that shrank to nothing moves on by one.
        start = end.max(start + 1);
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
        split_spans_covering_siblings(&mut self.spans);
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
            // Through `right()` rather than the estimate inline, so a
            // quarter-turned span is centred on the x it covers and not on the
            // length of the run stood up on it (#574). The same value for
            // upright text.
            let center_x = (span.x + span.right()) * 0.5;
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

/// The points a path segment is built from, in the order the segment draws
/// them. Control points included: a cubic lies inside the hull of its four
/// points, so a bound taken over all of them is never too small.
fn element_points(element: &PathEl) -> impl Iterator<Item = Point> {
    let mut points = [Point::ZERO; 3];
    let count = match *element {
        PathEl::MoveTo(p) | PathEl::LineTo(p) => {
            points[0] = p;
            1
        }
        PathEl::QuadTo(a, b) => {
            points[0] = a;
            points[1] = b;
            2
        }
        PathEl::CurveTo(a, b, c) => {
            points[0] = a;
            points[1] = b;
            points[2] = c;
            3
        }
        PathEl::ClosePath => 0,
    };
    points.into_iter().take(count)
}

/// A horizontal rule the page draws, in the page space the spans are measured
/// in (#545).
///
/// This is the one thing on the page that says "table" without guessing at it
/// from the text's own geometry. A width heuristic cannot tell a table from a
/// paper set in two columns -- both are slices of a band with a gutter between
/// them -- and the discriminator tried on slice width fixed the Isartor table
/// by breaking `tracemonkey`. The ruling lines are drawn by the document
/// itself, and a page of prose does not draw one between every pair of lines.
#[derive(Clone, Copy, Debug)]
#[doc(hidden)]
pub struct HorizontalRule {
    pub left: f64,
    pub right: f64,
    pub y: f64,
}

/// One straight, axis-aligned stroke of a drawn grid, in page space (#574):
/// a thin line in either direction, or one edge of a drawn rectangle.
///
/// KEPT APART FROM [`HorizontalRule`] on purpose. Every heuristic that reads
/// the horizontal rules was tuned on what that list holds; a table's cell
/// borders and shading boxes would flood it with short segments none of them
/// was measured against. This list is read by the lattice alone.
#[derive(Clone, Copy, Debug, PartialEq)]
#[doc(hidden)]
pub struct Ruling {
    /// Left or only x.
    pub x0: f64,
    /// Bottom or only y.
    pub y0: f64,
    /// Right x; equal to `x0` for a vertical ruling.
    pub x1: f64,
    /// Top y; equal to `y0` for a horizontal ruling.
    pub y1: f64,
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
///
/// One entry on the marked-content stack for `/ActualText`. Sequences nest,
/// so each `BDC`/`BMC` pushes and `EMC` pops; the innermost entry carrying
/// a replacement wins for a glyph drawn inside it. The replacement is
/// emitted once, on the first glyph of its sequence, and later glyphs of
/// the same sequence are suppressed. A sequence that closes without a glyph
/// having carried it reads once at its close instead, at the marks it drew.
struct ActualTextEntry {
    actual_text: Option<String>,
    consumed: bool,
    /// A glyph that reads as drawn text was painted while this sequence was
    /// open, so the sequence already stands in the text and its close must
    /// not read the replacement as well.
    saw_glyph: bool,
    /// A sequence nested inside this one already answered for these marks --
    /// its replacement read on a glyph or at its close -- so this close
    /// stays quiet, and a replacement inside a replacement reads once.
    child_resolved: bool,
    /// The marks painted while this sequence was open, in page space: glyph
    /// boxes, path boxes and image boxes united. Where the replacement reads
    /// when no glyph carried it.
    extent: Option<[f64; 4]>,
}

#[doc(hidden)]
pub struct TextExtractionDevice {
    spans: Vec<TextSpan>,
    last_y: f64,
    last_end_x: f64,
    /// The gaps between the last few glyph pairs of the span being built, so
    /// that a run the typesetter TRACKED can be told from a run with word
    /// spaces in it (#582). See `a_tracked_run`.
    recent_gaps: Vec<f64>,
    /// The last glyph drawn with a fill: where, and what it reads. Text in
    /// render mode 2 or 6 reaches the device twice per glyph, filled and then
    /// stroked, and the stroke is the same character on the same spot.
    last_fill: Option<([f64; 6], Option<BfString>)>,
    /// How far the last glyph's box was widened past its declared advance by
    /// the quarter-em floor of `glyph_width_and_source`; the gap after it is
    /// that much short of the one the page drew.
    last_widened: f64,
    /// The face the last span is MERGED on: the program's own name, which is
    /// what `font_name` was before it learned /BaseFont (#574). A span that
    /// carries the page's name for a face the program does not name keeps
    /// merging as it did -- a formula's CMMI into its CMR -- because a merge
    /// across a face change is where a word space is judged, and splitting
    /// there lost `where µ measures` on the arXiv pages.
    last_merge_face: Option<String>,
    /// Where in the text of the span being built the space consensus put a
    /// space, and the gap it judged: a drawn space is not in here.
    inserted_spaces: Vec<(usize, f64)>,
    /// Where in that text the current run of regular gaps began.
    run_start: usize,
    /// The last gap between two letters of the span being built.
    last_letter_gap: Option<f64>,
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
    /// Geometry extraction mode — Basic (advance bounds) or RichGeometry (tight bounds).
    geometry_mode: GeometryMode,
    /// Per-glyph data recorded during extraction for deferred tight-bounds
    /// computation. Only populated in RichGeometry mode. The index into this
    /// vec corresponds 1:1 with the glyph order across all spans.
    deferred_rich_glyphs: Vec<DeferredGlyph>,
    /// Hand back the ligature code points a font declared instead of the
    /// letters they stand for. Off, because a search box compares characters
    /// and U+FB03 is not `ffi` (#437); see [`crate::ligatures`].
    keep_ligatures: bool,
    /// Report the document's own font names and the descriptor's metrics on
    /// spans. Off, because the default output is what it was before the E34
    /// truth existed: substitute-face names and AFM metrics (#643).
    document_font_names: bool,
    /// The horizontal rules the page draws, in page space (#545).
    rules: Vec<HorizontalRule>,
    /// Every thin line and rectangle edge the page draws, for the lattice
    /// (#574).
    rulings: Vec<Ruling>,
    /// Every solid, non-white, axis-aligned rectangle the page fills, in page
    /// space and clipped: the shading behind a heading, a highlighted row, a
    /// box a note is set in (#574, the line network's vector inputs).
    fills: Vec<[f64; 4]>,
    /// Where the page places its images, in page space: the unit square each
    /// image XObject is drawn into, under its transform and clipped to what is
    /// visible (#636). A caption sits beside a picture, and the picture is
    /// nowhere else in what this device reads.
    images: Vec<[f64; 4]>,
    /// The box of every path the page fills or strokes that is not a thin
    /// line, in page space and clipped (#636): what a chart or a drawn figure
    /// is made of. Clustered into figures by [`Self::take_graphics`].
    path_boxes: Vec<[f64; 4]>,
    /// The clip boxes in force, innermost last, in page space (#545). Each
    /// entry is already intersected with the one below it, so the last is the
    /// box a path drawn now can reach.
    clip_stack: Vec<kurbo::Rect>,
    /// Whether glyphs merge into the runs the page draws, for the raw-order
    /// path (`ExtractOptions::raw_order`, #643). Off: every glyph stands on
    /// its own span, exactly as without the option. On: the gap is judged on
    /// the pen the last glyph left, so runs merge as drawn, and a run ends
    /// where the marked-content segment does.
    raw_runs: bool,
    /// The marked-content segment each span was drawn in, in span order; only
    /// filled when [`Self::raw_runs`] is on. `None` outside any segment (an
    /// untagged page never opens one).
    raw_bdc: Vec<Option<u32>>,
    /// The open marked-content segments, innermost last; only tracked when
    /// [`Self::raw_runs`] is on.
    raw_bdc_stack: Vec<u32>,
    /// The next marked-content segment number; only used when
    /// [`Self::raw_runs`] is on.
    raw_bdc_next: u32,
    /// Whether a segment opened or closed since the last glyph was drawn;
    /// only set when [`Self::raw_runs`] is on. The next glyph starts a fresh
    /// span, so a run never crosses the segment it was drawn in.
    raw_bdc_changed: bool,
    /// The page's declared name of the face the run being built is set in,
    /// subset prefix stripped; only tracked when [`Self::raw_runs`] is on.
    /// The program substitutes one face for several of the page's --
    /// `HelveticaNeueLTStd-BdOu` and `-Blk` both reach the spans as
    /// `Helvetica` -- so the merge face cannot tell `20` from `25`, and the
    /// page's own name is what parts them.
    last_raw_declared: Option<String>,
    /// Where the last glyph drawn ended, and the unit advance direction it
    /// read in; only tracked when [`Self::raw_runs`] is on. A turned glyph
    /// joins its run along the way it reads, so the pen is a point here,
    /// not the x the default path keeps.
    raw_pen: [f64; 2],
    raw_dir: [f64; 2],
    /// Whether `raw_pen` holds a glyph; only tracked when [`Self::raw_runs`]
    /// is on.
    raw_pen_valid: bool,
    /// The open marked-content sequences, innermost last. Tracked only while
    /// [`Self::actual_text`] applies replacements; ignored otherwise, so the
    /// default output is what it was before the replacements were read. The
    /// stack survives `Do` into a Form XObject because the same device reads
    /// the nested stream.
    actual_text_stack: Vec<ActualTextEntry>,
    /// Whether `/ActualText` replacements apply. Off, because the default
    /// output is what it was before the replacements were read. Set this
    /// when the caller asked for the replacements through
    /// `ExtractOptions::actual_text` (#663).
    actual_text: bool,
    /// The annotation appearances being drawn, innermost last, in page
    /// space: the rectangle each annotation shows its appearance in. A
    /// glyphless replacement inside one is the value the widget shows --
    /// the only place that value is painted -- so it reads even where
    /// replacements on the page stay off.
    annotation_appearances: Vec<kurbo::Rect>,
    /// Where the last glyph's own text went, so that it can be read again
    /// when the next glyph arrives. See [`LastGlyphText`].
    last_glyph_text: Option<LastGlyphText>,
    /// This device's number, which names its entry in [`LAST_GLYPH`].
    id: u64,
    /// Whether [`LAST_GLYPH`] names this device, so that it is written once
    /// rather than at every glyph (spec, "Performance").
    holds_last_glyph: bool,
    /// Clears this device's entry in [`LAST_GLYPH`] when the device goes,
    /// unwinding included.
    last_glyph_guard: ClearLastGlyph,
}

/// The number the next [`TextExtractionDevice`] gets.
static NEXT_DEVICE: AtomicU64 = AtomicU64::new(0);

/// Empties [`LAST_GLYPH`] when dropped if the entry is the device's whose
/// number it holds, so no glyph -- and no interpreter font behind it --
/// outlives the device that drew it, even when extraction panics (spec
/// review 14-10 02:00 (i)).
struct ClearLastGlyph(u64);

impl ClearLastGlyph {
    /// Empties [`LAST_GLYPH`] if it names this device.
    fn clear(&self) {
        let _ = LAST_GLYPH.try_with(|slot| {
            if slot.get() == Some(self.0) {
                slot.set(None);
            }
        });
    }
}

impl Drop for ClearLastGlyph {
    fn drop(&mut self) {
        self.clear();
    }
}

/// Whether [`LAST_GLYPH`] holds a glyph on this thread.
#[cfg(test)]
pub(crate) fn last_glyph_is_held() -> bool {
    LAST_GLYPH.with(|slot| slot.get().is_some())
}

thread_local! {
    /// The number of the device on this thread that holds a last glyph.
    ///
    /// The glyph itself is not kept: the device keeps its place on the page
    /// (a plain value, so the device stays `Send`), and the next glyph says
    /// what the glyph at that place reads as now when drawing it changed
    /// that (`OutlineGlyph::reread_previous`).
    static LAST_GLYPH: Cell<Option<u64>> = const { Cell::new(None) };
}

/// Where the last glyph's text went, so that it can be read again.
///
/// The interpreter offers a combining mark from the AMS name rows (spec
/// `specs/text/circlecopyrt.md`, C4 and C5a) to the glyph on the other side
/// of it once it has seen both, and the glyph already handed over may then
/// read differently: a `c` drawn before its circle reads `c⃝`, and a slash
/// drawn before the `=` it strikes reads as nothing, the `=` then reading
/// `≠`. The device reads the last glyph again at the next one, but only when
/// its text still stands last in the last span, so that what it reads now
/// can go in place of what it read then; reading it is what takes the offer,
/// and a glyph that cannot be read again leaves the offer to be refused.
/// Only the text changes: the glyph's box stays, and the pen stays where the
/// first reading left it, except that a glyph that now reads as nothing
/// gives its box back.
struct LastGlyphText {
    /// Where the glyph stands on its page, set when it is remembered.
    place: Option<GlyphPlace>,
    /// The span the text went into, and where in its text it starts.
    span: usize,
    offset: usize,
    /// How long the text was, in bytes.
    len: usize,
    /// Whether the glyph read as nothing and so wrote no text and no box.
    boxless: bool,
    /// Whether the glyph opened the span.
    opened: bool,
    /// What the span held before this glyph: glyph boxes, width, and (for a
    /// glyph that opened it) the previous span's run end under raw runs.
    char_bounds: usize,
    width: f64,
    previous_complete_run: Option<bool>,
    /// The rich-geometry records before this glyph's.
    deferred: usize,
    /// How many spans there were after this glyph, and the last one's text
    /// length and box count: the glyph is still last while these hold.
    span_count: usize,
    tail_text: usize,
    tail_boxes: usize,
}

/// Lightweight glyph data recorded during extraction for deferred tight-bounds
/// computation. Avoids computing bounds on the hot path.
#[derive(Clone)]
struct DeferredGlyph {
    /// Transform coefficients [a,b,c,d,e,f] from composed (CTM * glyph_transform)
    coeffs: [f64; 6],
    /// Font size in points
    font_size: f64,
    /// Advance width in page-space points
    glyph_width: f64,
    /// Whether this glyph needs transform-then-bound (rotation/shear)
    needs_exact: bool,
    /// The glyph outline path in raw glyph space. Retained ONLY for
    /// rotated/sheared glyphs (`needs_exact`), where a tight bound requires
    /// bounding the transformed path; `None` for axis-aligned glyphs (the
    /// `font_bbox` suffices) and for ink-less glyphs (spaces, Type3).
    outline: Option<BezPath>,
    /// Pre-computed bounding box of the outline in raw glyph space. `None` when
    /// the glyph has no ink (space, Type3, or empty outline) → Estimate.
    font_bbox: Option<kurbo::Rect>,
}

const MEDIAN_REFRESH: usize = 32;

/// How far apart two glyphs' type sizes may stand and still merge into one
/// raw run, as a fraction of the larger (#643). Within a run the size is one
/// Tm and never varies; across runs the page changes it wholesale (`MM` at
/// 7pt against its 4.95pt space is twenty-nine per cent).
const RAW_SIZE_TOLERANCE: f64 = 0.01;

impl Default for TextExtractionDevice {
    fn default() -> Self {
        Self::new()
    }
}

impl TextExtractionDevice {
    /// Create a new text extraction device with Basic geometry (default).
    pub fn new() -> Self {
        Self::with_mode(GeometryMode::Basic)
    }

    /// Create a new text extraction device with the given geometry mode.
    pub fn with_mode(geometry_mode: GeometryMode) -> Self {
        let id = NEXT_DEVICE.fetch_add(1, AtomicOrdering::Relaxed);
        Self {
            spans: Vec::new(),
            last_y: f64::NEG_INFINITY,
            last_end_x: f64::NEG_INFINITY,
            recent_gaps: Vec::new(),
            last_fill: None,
            last_widened: 0.0,
            last_merge_face: None,
            inserted_spaces: Vec::new(),
            run_start: 0,
            last_letter_gap: None,
            pending_tj_offset: 0.0,
            glyph_widths: Vec::new(),
            cached_median_char_width: 0.0,
            geometry_mode,
            deferred_rich_glyphs: Vec::new(),
            keep_ligatures: false,
            document_font_names: false,
            rules: Vec::new(),
            rulings: Vec::new(),
            fills: Vec::new(),
            images: Vec::new(),
            path_boxes: Vec::new(),
            clip_stack: Vec::new(),
            raw_runs: false,
            raw_bdc: Vec::new(),
            raw_bdc_stack: Vec::new(),
            raw_bdc_next: 0,
            raw_bdc_changed: false,
            last_raw_declared: None,
            raw_pen: [0.0, 0.0],
            raw_dir: [1.0, 0.0],
            raw_pen_valid: false,
            actual_text_stack: Vec::new(),
            actual_text: false,
            annotation_appearances: Vec::new(),
            last_glyph_text: None,
            id,
            holds_last_glyph: false,
            last_glyph_guard: ClearLastGlyph(id),
        }
    }

    fn push_placed_span(&mut self, text: &str, rect: kurbo::Rect) {
        if text.is_empty() {
            return;
        }

        let height = if rect.height().is_finite() && rect.height() > 0.0 {
            rect.height()
        } else {
            0.0
        };
        let width = if rect.width().is_finite() && rect.width() > 0.0 {
            rect.width()
        } else {
            0.0
        };

        // A NEW SPAN IS A NEW RUN, so what the last one was tracked at says
        // nothing about this one (#582).
        self.recent_gaps.clear();
        self.inserted_spaces.clear();
        self.run_start = 0;
        self.last_letter_gap = None;
        self.last_merge_face = None;
        self.spans.push(TextSpan {
            text: text.to_string(),
            x: rect.x0,
            y: rect.y0,
            width,
            height,
            font_size: height,
            // Placed text is one synthetic span, never a content run (#643).
            complete_run: false,
            font_name: None,
            is_bold: false,
            is_italic: false,
            color: None,
            width_source: WidthSource::Estimate,
            char_bounds: Vec::new(),
            // The placement is a real statement about where these characters
            // are, in the same form a glyph's is -- scale on the diagonal,
            // position in the last pair -- so a caller that lays the text back
            // out has what it needs, and the coverage gate over these two
            // fields stays a gate rather than acquiring an exception.
            transform: Some([height / 1000.0, 0.0, 0.0, height / 1000.0, rect.x0, rect.y0]),
            font_weight: None,
            is_serif: None,
            is_monospace: None,
            // Filled: a field value is painted, not stroked and not hidden.
            render_mode: Some(0),
            // Placed, not recognized: no OCR layer wrote this.
            is_ocr_layer: false,
            font_metrics: None,
            geometry_mode: self.geometry_mode,
            bounds_source: BoundsSource::Estimate,
            tight_char_bounds: Vec::new(),
            glyph_advances: Vec::new(),
            glyph_bounds_sources: Vec::new(),
        });

        // The pen follows, so that anything drawn after this reads its gap
        // against where the placed text ended rather than against a glyph
        // three widgets back.
        self.last_y = rect.y0;
        self.last_end_x = rect.x1;
        self.last_widened = 0.0;
        self.pending_tj_offset = 0.0;
    }

    /// Hand back the ligature code points the font declared -- `ﬃ` rather than
    /// `ffi` -- from the TEXT outputs ([`Self::into_text`], [`Self::into_blocks`]).
    ///
    /// The default folds, because the extracted string is read by a search
    /// index, a RAG chunker or a person pasting a word into a search box, and
    /// none of them will match a ligature code point against the letters
    /// somebody types. Set this when the caller wants the file's own
    /// characters back: a typesetting tool, or a comparison against the
    /// content stream. The paragraph blocks ([`Self::into_paragraph_blocks`])
    /// are unfolded either way: they feed the decisions, and the fold happens
    /// after them (see [`crate::ligatures`]).
    #[must_use]
    pub fn keep_ligatures(mut self, keep: bool) -> Self {
        self.keep_ligatures = keep;
        self
    }

    /// Merge glyphs into the runs the page draws, for the raw-order path
    /// (`ExtractOptions::raw_order`, #643).
    pub fn raw_runs(mut self, raw: bool) -> Self {
        self.raw_runs = raw;
        self
    }

    /// Report the document's own font names and the descriptor's metrics on
    /// spans: the page's /BaseFont and /Ascent//Descent instead of the
    /// substitute face's. Off, because the default output is what it was
    /// before the E34 truth existed. Set this when the caller asked for the
    /// document truth through `ExtractOptions::document_font_names` (#643).
    #[must_use]
    pub fn document_font_names(mut self, report: bool) -> Self {
        self.document_font_names = report;
        self
    }

    /// Apply `/ActualText` replacements to the spans. Off, because the
    /// default output is what it was before the replacements were read. Set
    /// this when the caller asked for the replacements through
    /// `ExtractOptions::actual_text` (#663).
    #[must_use]
    pub fn actual_text(mut self, apply: bool) -> Self {
        self.actual_text = apply;
        self
    }

    /// Whether the device is inside an annotation appearance stream: a
    /// replacement there is the annotation's own statement about what it
    /// shows, so the marked-content stack is tracked even where replacements
    /// on the page stay off.
    fn in_annotation_appearance(&self) -> bool {
        !self.annotation_appearances.is_empty()
    }

    /// Fold a painted box, in page space, into every open marked-content
    /// sequence: the marks a glyphless replacement reads at when it closes.
    fn note_drawn_box(&mut self, drawn: [f64; 4]) {
        for open in self.actual_text_stack.iter_mut() {
            open.extent = Some(match open.extent {
                Some(before) => union(&before, &drawn),
                None => drawn,
            });
        }
    }

    /// Where a glyphless replacement reads: the marks its sequence drew, or
    /// the annotation showing the sequence when it drew nothing placeable.
    fn glyphless_placement(&self, extent: Option<[f64; 4]>) -> Option<kurbo::Rect> {
        if let Some([x0, y0, x1, y1]) = extent {
            if x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite() {
                return Some(kurbo::Rect::new(x0, y0, x1, y1));
            }
        }
        self.annotation_appearances.last().copied()
    }

    /// Read the replacement of a marked-content sequence that closes without
    /// a glyph having carried it: once, at the marks it drew. A sequence
    /// that already answered -- on a glyph, or through a nested sequence
    /// that did -- stays quiet, so a replacement inside a replacement reads
    /// once and the innermost answers.
    fn emit_glyphless_replacement(&mut self, entry: ActualTextEntry) {
        let answered = entry.consumed || entry.child_resolved;
        if answered || entry.saw_glyph || entry.actual_text.is_none() {
            // The sequence already stands in the text -- its replacement
            // read on a glyph, a nested sequence answered for it, or its
            // glyphs read as drawn -- or it carries no replacement. A nested
            // answer still quiets the sequence around it.
            if answered {
                if let Some(parent) = self.actual_text_stack.last_mut() {
                    parent.child_resolved = true;
                }
            }
            return;
        }
        let replacement = entry.actual_text.as_deref().unwrap_or("");
        if let Some(at) = self.glyphless_placement(entry.extent) {
            if !replacement.is_empty() {
                self.push_placed_span(replacement, at);
            }
        }
        if let Some(parent) = self.actual_text_stack.last_mut() {
            parent.child_resolved = true;
        }
    }

    /// Drop the spans whose centre falls outside `rect` (`[x0, y0, x1, y1]`
    /// in user space, y up), before segmentation turns the rest into blocks.
    /// Called with the rectangle `ExtractOptions::clip_margins` leaves on the
    /// page: content the margins cover is never read.
    pub(crate) fn clip_spans(&mut self, rect: [f64; 4]) {
        self.spans.retain(|span| {
            let centre = (span.x + span.width * 0.5, span.y + span.height * 0.5);
            centre.0 >= rect[0] && centre.0 <= rect[2] && centre.1 >= rect[1] && centre.1 <= rect[3]
        });
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

    /// Move the pen to the end of the glyph just drawn.
    ///
    /// ANN[r17/TEX5] A combining mark -- a Hebrew point, an Arabic harakat --
    /// is drawn on the glyph before it and leaves the pen where it was. Letting
    /// it move the pen back to its own origin makes the next letter look an
    /// advance away, and that gap then reads as a word break: `שָׁלוֹם` came
    /// apart into `שָׁל` and `וֹם` (#436). The declared advance cannot answer
    /// this, because `glyph_width_and_source` clamps a zero one up to a quarter
    /// of the font size before anything sees it.
    fn advance_pen(&mut self, text: &str, glyph_end_x: f64, widened: f64) {
        if !crate::bidi::is_only_marks(text) {
            self.last_end_x = glyph_end_x;
            self.last_widened = widened;
        }
    }

    /// Move the raw pen past a glyph, for the turned-run judgment (#643).
    /// A no-op unless [`Self::raw_runs`] is on; marks never move it, exactly
    /// like [`Self::advance_pen`].
    fn advance_raw_pen(
        &mut self,
        text: &str,
        x: f64,
        y: f64,
        adv_x: f64,
        adv_y: f64,
        glyph_width: f64,
    ) {
        if !self.raw_runs || crate::bidi::is_only_marks(text) {
            return;
        }
        self.raw_pen = [x + adv_x * glyph_width, y + adv_y * glyph_width];
        self.raw_dir = [adv_x, adv_y];
        self.raw_pen_valid = true;
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
        //
        // ONLY A NEGATIVE ONE, because only a negative TJ moves the next glyph
        // right, away from the last. A positive one is a pair kern pulling it
        // back: `[(FINANCIAL RESUL)134.6(TS)] TJ` in NYSE_MS_2001 overlaps
        // its `L` and `T` by a point, and counted by its size it wrote
        // `FINANCIAL RESUL TS` and `MANAGEMENT'S DISCUSSION AND ANAL YSIS`,
        // two headings the ground truth could not match (#638 rank 4).
        if self.pending_tj_offset <= -TJ_SPACE_THRESHOLD_UNITS {
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

    /// Whether this gap is the typesetter's TRACKING rather than a word space
    /// (#582).
    ///
    /// A display line set in letter-spaced capitals puts the same distance
    /// between every pair of letters, and that distance is over every
    /// threshold a word space has: the TJ adjustment is a full one and the
    /// geometric gap is far past a third of a character. So the consensus
    /// fires on EVERY pair and `DIRECTORS' REPORT` comes out as
    /// `D I R E C TO R S ' R E P O R T` -- which is 47 of the 200 pages of
    /// run `structured-doclaynet-20260922T081244Z` writing text the ground
    /// truth does not have, and, on the heading axis, 65 of 154 missed
    /// headings (#583).
    ///
    /// WHAT SEPARATES THEM IS NOT THE LETTERS BUT THE REGULARITY. Tracking is
    /// a constant added to every advance, so a tracked run's gaps agree with
    /// one another; prose has word spaces at the ends of words and kerning
    /// everywhere else, and four consecutive gaps that agree within a quarter
    /// while every one of them is over the word-space threshold is not a
    /// sentence. Nothing here reads the text, so it holds for a language whose
    /// capitals this file has never seen.
    ///
    /// AND THE REAL WORD SPACE IS NOT LOST, because a tracked line still draws
    /// its word space as a space GLYPH: `DIRECTORS' REPORT` carries a 0x20 in
    /// the string the page shows, and that arrives as text rather than as a
    /// gap for this to judge.
    ///
    /// WHERE IT PAYS, AND WHY THAT IS A CHOICE AND NOT AN OVERSIGHT. A page
    /// that draws `A B C D E F` as separate glyphs with equal gaps and no
    /// space glyphs is, byte for byte, the same page as a letter-spaced
    /// display line: both are single characters at one constant advance, and
    /// nothing on the page says which the author meant. This takes the reading
    /// that is far more common and that the corpus measured -- letter-spacing,
    /// 47 of 200 pages -- and pays for it on a line of isolated single-letter
    /// words, which is rare. The price is pinned by
    /// `a_line_of_single_letter_words_reads_as_tracking_and_that_is_the_choice`
    /// so that a later reader argues with the note rather than with a silence.
    fn a_tracked_run(&self, gap: f64) -> bool {
        if gap <= 0.0 || self.recent_gaps.len() < TRACKING_MIN_PAIRS {
            return false;
        }
        self.recent_gaps
            .iter()
            .rev()
            .take(TRACKING_MIN_PAIRS)
            .all(|before| (before - gap).abs() <= TRACKING_TOLERANCE * gap)
    }

    /// Take back the spaces already written into a run now recognised as
    /// tracked.
    ///
    /// The regularity needs [`TRACKING_MIN_PAIRS`] earlier gaps before it can be
    /// seen, so the first letters of the run have had their spaces inserted
    /// already: without this, `DIRECTORS` comes back as `D I R E CTORS`.
    ///
    /// ONLY THE SPACES THE CONSENSUS PUT IN, AT THE TRACKING (#623). The join
    /// used to take back every space between single characters at the end of
    /// the text, and a space the page DREW looks the same there: `IT ALL ADDS
    /// UP` (NYSE_WAL_2014) draws its word spaces as glyphs, and the run came out
    /// `ITALL`. It also stopped at the first piece longer than one letter, so a
    /// narrow `I` that took no space left `D IR ECTORS`. A space goes when the
    /// consensus inserted it at a gap the tracking explains, inside the run
    /// the tracking was seen in, between two letters; a word space set wider
    /// than the tracking, drawn or not, stays -- and so does the space of
    /// `suppose that`, which a formula's regular gaps further on matched
    /// (1201.4397).
    fn take_back_tracked_spaces(
        text: &mut String,
        inserted: &mut Vec<(usize, f64)>,
        run_start: usize,
        tracking: f64,
    ) {
        let between_letters = |at: usize| {
            text[..at]
                .chars()
                .next_back()
                .is_some_and(char::is_alphabetic)
                && text[at + 1..]
                    .chars()
                    .next()
                    .is_some_and(char::is_alphabetic)
        };
        let goes = |at: usize, gap: f64| {
            at >= run_start
                && at < text.len()
                && text.as_bytes()[at] == b' '
                && (gap - tracking).abs() <= TRACKING_TOLERANCE * tracking
                && between_letters(at)
        };
        if !inserted.iter().any(|&(at, gap)| goes(at, gap)) {
            return;
        }
        let goes: Vec<bool> = inserted.iter().map(|&(at, gap)| goes(at, gap)).collect();
        let bytes = text.as_bytes();
        let mut kept = String::with_capacity(text.len());
        let mut still: Vec<(usize, f64)> = Vec::with_capacity(inserted.len());
        let mut from = 0;
        for (&(at, gap), &goes) in inserted.iter().zip(&goes) {
            if at < from || at >= text.len() || bytes[at] != b' ' {
                continue;
            }
            kept.push_str(&text[from..at]);
            if goes {
                from = at + 1;
            } else {
                still.push((kept.len(), gap));
                kept.push(' ');
                from = at + 1;
            }
        }
        kept.push_str(&text[from..]);
        *text = kept;
        *inserted = still;
    }

    /// Keep a drawn path when it is a horizontal rule (#545).
    ///
    /// The whole of the page's graphics arrives here and almost none of it is
    /// kept: what is, is ink that is a line -- flat enough not to be a box and
    /// long enough to separate two rows -- recorded in the page space the
    /// spans already live in, so the two can be compared without a second
    /// coordinate system to keep in step. The box is the transformed bounding
    /// box, which for a rotated path is the box around it and therefore too
    /// tall to be a rule: a rule on a turned page is not read as one, and that
    /// is the safe way round.
    ///
    /// A stroke is measured on its centreline, since that is the path the
    /// device is given and the weight is a property of the pen. A heavy rule is
    /// still a rule, and it still lies where its centreline lies.
    fn record_rule(&mut self, path: &BezPath, transform: Affine) {
        // The cap is asked at the door AND at every push. One path may hold
        // hundreds of subpaths, so a check that runs once per call is not a
        // bound on anything: it lets a single compound path put thousands of
        // entries in a vector `region_is_ruled` then walks per band.
        if self.rules.len() >= MAX_RECORDED_RULES {
            return;
        }
        // SUBPATH BY SUBPATH, because a producer is free to draw all of a
        // table's rules in one path, and a box around the lot of them is a box
        // and not a line. The bound is taken over the points a segment is made
        // of, curve controls included -- a curve lies inside its own hull, so
        // the box is never too small, and the arithmetic is a comparison per
        // point rather than solving each curve for its extremes. On the slowest
        // document of the gate set, which draws a figure of several thousand
        // segments, exact curve bounds cost 7% of its 3.3 s; this costs 1.6%.
        // WHAT IS DRAWN IS NOT WHAT IS SEEN. A Form XObject's `/BBox` reaches
        // this device as a clip and the geometry inside it unclipped, so a line
        // ruled across one cell of a form can arrive as a line across the page.
        // Two of those are enough for `region_is_ruled` to refuse a column cut
        // that should have been taken, so the extent recorded is the visible
        // one.
        let clip = self.clip_stack.last().copied();
        let mut lo = (f64::INFINITY, f64::INFINITY);
        let mut hi = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        let flush = |lo: &mut (f64, f64), hi: &mut (f64, f64), rules: &mut Vec<HorizontalRule>| {
            let (mut left, mut right) = (lo.0, hi.0);
            let (mut bottom, mut top) = (lo.1, hi.1);
            if let Some(clip) = clip {
                left = left.max(clip.x0);
                right = right.min(clip.x1);
                bottom = bottom.max(clip.y0);
                top = top.min(clip.y1);
            }
            if lo.0.is_finite()
                && rules.len() < MAX_RECORDED_RULES
                && bottom <= top
                && hi.1 - lo.1 <= RULE_MAX_THICKNESS
                && right - left >= RULE_MIN_LENGTH
            {
                rules.push(HorizontalRule {
                    left,
                    right,
                    y: (lo.1 + hi.1) * 0.5,
                });
            }
            *lo = (f64::INFINITY, f64::INFINITY);
            *hi = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        };

        for element in path.elements() {
            if matches!(element, PathEl::MoveTo(_) | PathEl::ClosePath) {
                flush(&mut lo, &mut hi, &mut self.rules);
            }
            for point in element_points(element) {
                let p = transform * point;
                lo = (lo.0.min(p.x), lo.1.min(p.y));
                hi = (hi.0.max(p.x), hi.1.max(p.y));
            }
        }
        flush(&mut lo, &mut hi, &mut self.rules);
    }

    /// Keep a drawn path's straight axis-aligned strokes for the lattice
    /// (#574): each subpath that is a thin line, in either direction, and the
    /// four edges of each subpath that is an axis-aligned rectangle -- the way
    /// a timetable draws its cells. Anything else, a curve or a slanted line,
    /// is not a grid and is not kept.
    fn record_rulings(&mut self, path: &BezPath, transform: Affine) {
        if self.rulings.len() >= MAX_RECORDED_RULINGS {
            return;
        }
        let clip = self.clip_stack.last().copied();
        let mut points: Vec<Point> = Vec::new();
        let mut curved = false;
        let mut subpaths: Vec<(Vec<Point>, bool)> = Vec::new();
        for element in path.elements() {
            if matches!(element, PathEl::MoveTo(_)) && !points.is_empty() {
                subpaths.push((std::mem::take(&mut points), curved));
                curved = false;
            }
            if matches!(element, PathEl::QuadTo(..) | PathEl::CurveTo(..)) {
                curved = true;
            }
            for point in element_points(element) {
                points.push(transform * point);
            }
        }
        if !points.is_empty() {
            subpaths.push((points, curved));
        }
        for (points, curved) in subpaths {
            if curved || points.len() < 2 {
                continue;
            }
            let (mut x0, mut y0) = (f64::INFINITY, f64::INFINITY);
            let (mut x1, mut y1) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
            for p in &points {
                x0 = x0.min(p.x);
                y0 = y0.min(p.y);
                x1 = x1.max(p.x);
                y1 = y1.max(p.y);
            }
            let clipped = |r: Ruling| -> Option<Ruling> {
                let Some(c) = clip else {
                    return Some(r);
                };
                let r = Ruling {
                    x0: r.x0.max(c.x0),
                    y0: r.y0.max(c.y0),
                    x1: r.x1.min(c.x1),
                    y1: r.y1.min(c.y1),
                };
                (r.x0 <= r.x1 && r.y0 <= r.y1).then_some(r)
            };
            let (w, h) = (x1 - x0, y1 - y0);
            let mut found: Vec<Ruling> = Vec::new();
            if h <= RULE_MAX_THICKNESS && w >= RULING_MIN_LENGTH {
                let y = (y0 + y1) * 0.5;
                found.push(Ruling {
                    x0,
                    y0: y,
                    x1,
                    y1: y,
                });
            } else if w <= RULE_MAX_THICKNESS && h >= RULING_MIN_LENGTH {
                let x = (x0 + x1) * 0.5;
                found.push(Ruling {
                    x0: x,
                    y0,
                    x1: x,
                    y1,
                });
            } else if w > RULE_MAX_THICKNESS && h > RULE_MAX_THICKNESS {
                // A rectangle, and only if every point is one of its corners.
                let corner = |p: &Point| {
                    ((p.x - x0).abs() < 0.5 || (p.x - x1).abs() < 0.5)
                        && ((p.y - y0).abs() < 0.5 || (p.y - y1).abs() < 0.5)
                };
                if points.iter().all(corner) && points.len() <= 6 {
                    found.push(Ruling { x0, y0, x1, y1: y0 });
                    found.push(Ruling { x0, y0: y1, x1, y1 });
                    found.push(Ruling { x0, y0, x1: x0, y1 });
                    found.push(Ruling { x0: x1, y0, x1, y1 });
                }
            }
            for r in found.into_iter().filter_map(clipped) {
                if self.rulings.len() >= MAX_RECORDED_RULINGS {
                    return;
                }
                self.rulings.push(r);
            }
        }
    }

    /// The lattice rulings the page drew so far, taken out of the device
    /// (#574). Called before the device is consumed into its blocks.
    #[doc(hidden)]
    /// Keep a filled path that is a solid, non-white, axis-aligned rectangle
    /// big enough to stand behind a line of text (#574): shading, a
    /// highlighted row, a note's box. Anything curved or patterned, anything
    /// near white -- a page's own background -- and anything past the cap is
    /// left out.
    fn record_fill(&mut self, path: &BezPath, transform: Affine, paint: &Paint<'_>) {
        if self.fills.len() >= MAX_RECORDED_RULINGS {
            return;
        }
        let Paint::Color(color) = paint else {
            return;
        };
        let rgba = color.to_rgba().to_rgba8();
        let [r, g, b, a] = rgba;
        let luminance = (299 * u32::from(r) + 587 * u32::from(g) + 114 * u32::from(b)) / 1000;
        if a < 26 || luminance >= FILL_WHITE_ABOVE {
            return;
        }
        let mut points: Vec<Point> = Vec::new();
        for element in path.elements() {
            match element {
                PathEl::QuadTo(..) | PathEl::CurveTo(..) => return,
                PathEl::MoveTo(_) if !points.is_empty() => return,
                _ => points.extend(element_points(element).map(|p| transform * p)),
            }
        }
        if points.len() < 4 {
            return;
        }
        let rect = transform.transform_rect_bbox(path.bounding_box());
        // Axis-aligned: every corner on the box's edges.
        let on_edge = |p: &Point| {
            ((p.x - rect.x0).abs() < 0.5 || (p.x - rect.x1).abs() < 0.5)
                && ((p.y - rect.y0).abs() < 0.5 || (p.y - rect.y1).abs() < 0.5)
        };
        if !points.iter().all(on_edge) {
            return;
        }
        let mut rect = rect;
        if let Some(clip) = self.clip_stack.last() {
            rect = rect.intersect(*clip);
        }
        if rect.width() >= FILL_MIN_WIDTH && rect.height() >= FILL_MIN_HEIGHT {
            self.fills.push([rect.x0, rect.y0, rect.x1, rect.y1]);
        }
    }

    /// The fills [`Self::record_fill`] kept, in page space.
    pub fn take_fills(&mut self) -> Vec<[f64; 4]> {
        std::mem::take(&mut self.fills)
    }

    pub fn take_rulings(&mut self) -> Vec<Ruling> {
        std::mem::take(&mut self.rulings)
    }

    /// The boxes the page drew its images into, `[left, bottom, right, top]`
    /// in page space, in drawing order (#636).
    #[doc(hidden)]
    pub fn take_images(&mut self) -> Vec<[f64; 4]> {
        std::mem::take(&mut self.images)
    }

    /// The figures the page DRAWS rather than places as images, as boxes in
    /// page space (#636): the path boxes that touch or overlap, merged; a
    /// cluster counts when it holds at least three paths and is at least 20
    /// points each way, and not when it covers more than half the page -- a
    /// page's background or frame is not a figure anyone captions.
    #[doc(hidden)]
    pub fn take_graphics(&mut self, width: f64, height: f64) -> Vec<[f64; 4]> {
        let boxes = std::mem::take(&mut self.path_boxes);
        let mut clusters: Vec<([f64; 4], usize)> = Vec::new();
        for b in boxes {
            let mut merged = (b, 1usize);
            let mut rest = Vec::with_capacity(clusters.len());
            for c in clusters {
                if touches(&merged.0, &c.0, GRAPHIC_GAP) {
                    merged = (union(&merged.0, &c.0), merged.1 + c.1);
                } else {
                    rest.push(c);
                }
            }
            rest.push(merged);
            clusters = rest;
        }
        let page = (width * height).max(1.0);
        clusters
            .into_iter()
            .filter(|(b, n)| {
                let (w, h) = (b[2] - b[0], b[3] - b[1]);
                *n >= 3 && w >= 20.0 && h >= 20.0 && w * h <= 0.5 * page
            })
            .map(|(b, _)| b)
            .collect()
    }

    fn record_path_box(&mut self, path: &BezPath, transform: Affine) {
        if self.path_boxes.len() >= MAX_RECORDED_PATH_BOXES {
            return;
        }
        let mut rect = transform.transform_rect_bbox(path.bounding_box());
        if let Some(clip) = self.clip_stack.last() {
            rect = rect.intersect(*clip);
        }
        // A THIN LINE IS A RULE, a ruling or an underline -- the table and
        // paragraph passes read those -- and not a part of a figure.
        if rect.width().min(rect.height()) >= 1.5 {
            self.path_boxes.push([rect.x0, rect.y0, rect.x1, rect.y1]);
        }
    }

    /// Record where an image lands: its PIXEL rectangle under the transform
    /// the interpreter hands over -- which maps `0..width, 0..height`, not the
    /// unit square of the content stream's `cm` -- clipped to the visible box. A sliver under a point wide is not a picture
    /// anyone would caption -- it is a rule or a dot drawn as an image -- and
    /// is left out, and so is anything past the cap.
    fn record_image(&mut self, width: u32, height: u32, transform: Affine) {
        if self.images.len() >= MAX_RECORDED_IMAGES {
            return;
        }
        let (w, h) = (f64::from(width.max(1)), f64::from(height.max(1)));
        let mut rect = transform.transform_rect_bbox(kurbo::Rect::new(0.0, 0.0, w, h));
        if let Some(clip) = self.clip_stack.last() {
            rect = rect.intersect(*clip);
        }
        if rect.width() >= 1.0 && rect.height() >= 1.0 {
            self.images.push([rect.x0, rect.y0, rect.x1, rect.y1]);
        }
    }

    /// Consume the device and return extracted text as a single string.
    pub fn into_text(mut self) -> String {
        self.end_last_glyph();
        if self.geometry_mode == GeometryMode::RichGeometry {
            self.compute_tight_bounds();
        }
        self.spans.iter_mut().for_each(untrack_spaced_letters);
        let keep_ligatures = self.keep_ligatures;
        let rules = std::mem::take(&mut self.rules);
        let mut blocks = group_spans_into_blocks(self.spans, &rules);
        if !keep_ligatures {
            // Folded on the spans, which the lines assemble from: the one
            // fold every TEXT output shares (see `crate::ligatures`).
            for block in &mut blocks {
                crate::ligatures::fold_text_block(block);
            }
        }
        let lines: Vec<String> = blocks.iter().map(|b| b.text()).collect();
        let stitched = stitch_hyphenated_lines(&lines);
        normalize_text_output(&stitched)
    }

    /// Consume the device and return text blocks.
    pub fn into_blocks(mut self) -> Vec<TextBlock> {
        self.end_last_glyph();
        if self.geometry_mode == GeometryMode::RichGeometry {
            self.compute_tight_bounds();
        }
        self.spans.iter_mut().for_each(untrack_spaced_letters);
        let keep_ligatures = self.keep_ligatures;
        let rules = std::mem::take(&mut self.rules);
        let mut blocks = group_spans_into_blocks(self.spans, &rules);
        if !keep_ligatures {
            for block in &mut blocks {
                crate::ligatures::fold_text_block(block);
            }
        }
        blocks
    }

    /// Consume the device and return the page's paragraphs (#567).
    ///
    /// `page` is the 0-based page index the blocks record, and `first_index`
    /// where this page's blocks start in the document's reading order, so a
    /// caller walking a document hands the count of what it has emitted so far.
    pub fn into_paragraph_blocks(mut self, page: usize, first_index: usize) -> Vec<Block> {
        self.end_last_glyph();
        if self.geometry_mode == GeometryMode::RichGeometry {
            self.compute_tight_bounds();
        }
        self.spans.iter_mut().for_each(untrack_spaced_letters);
        let rules = std::mem::take(&mut self.rules);
        // Unfolded: these blocks feed the layout and model decisions and the
        // training dump, which all read the characters the glyphs declared.
        // The fold happens on the way out of `PdfDocument`, after the
        // decisions (see `crate::ligatures`). Every table stands: the
        // single-page path takes no options, so no strategy steers it.
        paragraph_blocks(self.spans, &rules, page, first_index, TableStrategy::Both)
    }

    /// The paragraphs AND the rules the page drew, in one pass (#600).
    ///
    /// The blocks are what a reader wants and the rules are what a model
    /// wants: a cell boundary in a ruled table is a drawn line, and a learned
    /// table structure that cannot see the lines is guessing at the strongest
    /// evidence the page offers. They come back together because the
    /// alternative is interpreting the page twice, and two passes over one
    /// page is two chances for them to disagree.
    ///
    /// Hidden rather than published: this is what
    /// `crates/pdf-learned`'s feature dump reads, and the shape of it follows
    /// whatever that needs rather than a promise to anyone else.
    #[doc(hidden)]
    pub fn into_paragraph_blocks_and_rules(
        self,
        page: usize,
        first_index: usize,
    ) -> (Vec<Block>, Vec<HorizontalRule>) {
        self.into_paragraph_blocks_and_rules_with_strategy(
            page,
            first_index,
            TableStrategy::Both,
            false,
        )
    }

    /// [`Self::into_paragraph_blocks_and_rules`], with the table-detection
    /// strategy given: under [`TableStrategy::Lines`] a grid no drawn rule
    /// crosses goes back to the paragraph merge (#659).
    ///
    /// `cell_path_words` is
    /// [`ExtractOptions::cell_path_words`](crate::ExtractOptions::cell_path_words):
    /// the leader fold and the line and cell texts read with the cell-path
    /// word rules where true.
    ///
    /// Hidden like the twin: this is what `extract_blocks` reads, and the
    /// shape of it follows whatever that needs rather than a promise to
    /// anyone else.
    #[doc(hidden)]
    pub fn into_paragraph_blocks_and_rules_with_strategy(
        mut self,
        page: usize,
        first_index: usize,
        table_strategy: TableStrategy,
        cell_path_words: bool,
    ) -> (Vec<Block>, Vec<HorizontalRule>) {
        self.end_last_glyph();
        if self.geometry_mode == GeometryMode::RichGeometry {
            self.compute_tight_bounds();
        }
        self.spans.iter_mut().for_each(untrack_spaced_letters);
        let rules = std::mem::take(&mut self.rules);
        let blocks = paragraph_blocks_with_cell_path_words(
            self.spans,
            &rules,
            page,
            first_index,
            table_strategy,
            cell_path_words,
        );
        (blocks, rules)
    }

    /// Consume the device and return the page's blocks in raw content-stream
    /// order (#643).
    ///
    /// Requires [`Self::raw_runs`]: the spans must be the runs the page
    /// draws, in the order it draws them. Blocks follow draw order with no
    /// reading-order sort; the frames turn with the page exactly as the
    /// default path's do.
    pub fn into_raw_blocks(mut self, page: usize, first_index: usize) -> Vec<Block> {
        self.end_last_glyph();
        debug_assert!(self.raw_runs, "raw blocks need raw runs");
        if self.geometry_mode == GeometryMode::RichGeometry {
            self.compute_tight_bounds();
        }
        self.spans.iter_mut().for_each(untrack_spaced_letters);
        // The page ends every run it drew (#643).
        if let Some(last) = self.spans.last_mut() {
            last.complete_run = true;
        }
        let bdc = std::mem::take(&mut self.raw_bdc);
        debug_assert_eq!(bdc.len(), self.spans.len());
        raw_blocks(self.spans, &bdc, page, first_index)
    }

    /// Consume the device and return raw spans.
    #[allow(dead_code)]
    pub(crate) fn into_spans(mut self) -> Vec<TextSpan> {
        self.end_last_glyph();
        if self.geometry_mode == GeometryMode::RichGeometry {
            self.compute_tight_bounds();
        }
        self.spans.iter_mut().for_each(untrack_spaced_letters);
        self.spans
    }

    /// Forget the last glyph where the device ends: no glyph follows, so its
    /// place is never read again and [`LAST_GLYPH`] no longer names this
    /// device.
    fn end_last_glyph(&mut self) {
        self.last_glyph_text = None;
        self.holds_last_glyph = false;
        self.last_glyph_guard.clear();
    }

    /// Remember `glyph` as the last glyph and `record` as where its text went.
    ///
    /// A glyph the page hides (`hidden`), or one whose text lands in a span
    /// the page hides, is not remembered: hidden text can be dropped from the
    /// output after the fact, and a mark that went into it would go with it.
    /// Not read again, its offer is refused and the mark reads as before its
    /// row (spec review 14-10 02:00 (ii)).
    fn remember_last_glyph(
        &mut self,
        glyph: &OutlineGlyph,
        mut record: LastGlyphText,
        hidden: bool,
    ) {
        let tail = self.spans.last();
        if hidden || tail.is_some_and(TextSpan::is_hidden) {
            self.last_glyph_text = None;
            return;
        }
        record.span_count = self.spans.len();
        record.tail_text = tail.map_or(0, |span| span.text.len());
        record.tail_boxes = tail.map_or(0, |span| span.char_bounds.len());
        record.place = glyph.place();
        if record.place.is_none() {
            self.last_glyph_text = None;
            return;
        }
        if !self.holds_last_glyph {
            let id = self.id;
            self.holds_last_glyph = LAST_GLYPH.try_with(|slot| slot.set(Some(id))).is_ok();
        }
        self.last_glyph_text = self.holds_last_glyph.then_some(record);
    }

    /// Whether the last glyph's text still stands last in the last span.
    fn last_glyph_still_last(&self, last: &LastGlyphText) -> bool {
        let tail = self.spans.last();
        self.spans.len() == last.span_count
            && tail.map_or(0, |span| span.text.len()) == last.tail_text
            && tail.map_or(0, |span| span.char_bounds.len()) == last.tail_boxes
    }

    /// Read the last glyph's text again and put what it reads now in place of
    /// what it read then (see [`LastGlyphText`]). The glyph is read only while
    /// its text still stands last, so that the read is never one this device
    /// cannot act on; nothing changes when the text is unchanged.
    fn reread_last_glyph(&mut self, glyph: &Glyph<'_>) {
        let Some(last) = self.last_glyph_text.as_ref() else {
            return;
        };
        if !self.last_glyph_still_last(last) {
            self.last_glyph_text = None;
            return;
        }
        // Only the glyph arriving now can have changed the last one, and it
        // says so; on a page without a row mark it never has.
        let read = match (glyph, last.place) {
            (Glyph::Outline(outline), Some(place)) => outline.reread_previous(place),
            _ => None,
        };
        let Some(read) = read else {
            return;
        };
        let last = self.last_glyph_text.as_mut().expect("checked above");
        if last.boxless {
            // It wrote nothing, and a glyph that read as nothing is a mark
            // whose read gave its offer up; there is nothing to put back.
            return;
        }
        let span = &mut self.spans[last.span];
        // Compared in place: on a page without mark rows the text never
        // changes, and the comparison then costs no allocation.
        let written = &span.text[last.offset..];
        let unchanged = match &read {
            Some(BfString::Char(c)) => written == c.encode_utf8(&mut [0; 4]),
            Some(BfString::String(s)) => written == s,
            None => written.is_empty(),
        };
        if unchanged {
            return;
        }
        let now = match read {
            Some(BfString::Char(c)) => c.to_string(),
            Some(BfString::String(s)) => s,
            None => String::new(),
        };
        span.text.truncate(last.offset);
        span.text.push_str(&now);
        last.len = now.len();
        last.tail_text = span.text.len();
        if !now.is_empty() {
            return;
        }
        // Read as nothing now: the glyph gives its box back, and a span it
        // opened goes with it.
        let last = self.last_glyph_text.take().expect("checked above");
        self.deferred_rich_glyphs.truncate(last.deferred);
        if last.opened {
            self.spans.pop();
            if self.raw_runs {
                self.raw_bdc.pop();
                if let (Some(prev), Some(was)) = (self.spans.last_mut(), last.previous_complete_run)
                {
                    prev.complete_run = was;
                }
            }
        } else {
            let span = &mut self.spans[last.span];
            span.char_bounds.truncate(last.char_bounds);
            span.width = last.width;
        }
    }

    /// Record glyph-level data for deferred tight-bounds computation.
    /// Called from draw_glyph in RichGeometry mode only.
    fn record_deferred_glyph(
        &mut self,
        glyph: &Glyph<'_>,
        composed: &Affine,
        font_size: f64,
        glyph_width: f64,
    ) {
        let coeffs = composed.as_coeffs();
        let is_rotated = coeffs[1].abs() > ROTATION_EPSILON || coeffs[2].abs() > ROTATION_EPSILON;
        let is_sheared = (coeffs[0] - coeffs[3]).abs() > SHEAR_EPSILON;
        let needs_exact = is_rotated || is_sheared;

        // Capture the glyph's ink bbox in raw glyph space. The full outline path
        // is retained only when `needs_exact` (rotated/sheared), so a tight bound
        // can bound the transformed path; axis-aligned glyphs keep only the bbox
        // (enveloping its transformed corners is exact) to bound memory on large
        // pages. Ink-less glyphs (spaces, Type3) carry no bbox → Estimate later.
        let (outline, font_bbox) = match glyph {
            Glyph::Outline(o) => {
                let path = o.outline();
                let bb = path.bounding_box();
                if bb.width() <= 0.0 && bb.height() <= 0.0 {
                    (None, None)
                } else if needs_exact {
                    (Some(path), Some(bb))
                } else {
                    (None, Some(bb))
                }
            }
            Glyph::Type3(_) => (None, None),
        };

        self.deferred_rich_glyphs.push(DeferredGlyph {
            coeffs,
            font_size,
            glyph_width,
            needs_exact,
            outline,
            font_bbox,
        });
    }

    /// Compute tight bounds for all recorded glyphs and populate the spans.
    /// Called once after all glyphs have been extracted.
    fn compute_tight_bounds(&mut self) {
        let deferred = std::mem::take(&mut self.deferred_rich_glyphs);
        if deferred.is_empty() {
            return;
        }

        let mut idx = 0usize;

        for span in &mut self.spans {
            let n = span.char_bounds.len();
            if n == 0 {
                continue;
            }

            for _gi in 0..n {
                if idx >= deferred.len() {
                    break;
                }
                let dg = &deferred[idx];
                idx += 1;

                let composed = Affine::new(dg.coeffs);
                let (tight_bound, source) = if let Some(font_bbox) = dg.font_bbox {
                    // Glyph has ink. The outline/bbox is in raw glyph space and
                    // `composed` (CTM * glyph_transform) maps it straight to page
                    // space — exactly as the renderer fills the raw outline, with
                    // NO upem pre-scaling. Applying any extra font_size/1000 here
                    // would double-scale the bound.
                    if dg.needs_exact {
                        // Rotated/sheared: bound the transformed outline path so
                        // the box stays tight (the transformed bbox would be
                        // looser). Falls back to enveloping the bbox corners if
                        // the path was not retained.
                        let page_bbox = match dg.outline {
                            Some(ref path) => (composed * path.clone()).bounding_box(),
                            None => {
                                let raw = [font_bbox.x0, font_bbox.y0, font_bbox.x1, font_bbox.y1];
                                let b = transform_bbox_corners(&raw, &composed);
                                kurbo::Rect::new(b[0], b[1], b[2], b[3])
                            }
                        };
                        (
                            [page_bbox.x0, page_bbox.y0, page_bbox.x1, page_bbox.y1],
                            BoundsSource::Tight,
                        )
                    } else {
                        // Axis-aligned scale + translate: enveloping the
                        // transformed bbox corners is exact.
                        let raw = [font_bbox.x0, font_bbox.y0, font_bbox.x1, font_bbox.y1];
                        (transform_bbox_corners(&raw, &composed), BoundsSource::Tight)
                    }
                } else {
                    // No ink (space, Type3, missing outline): fall back to the
                    // advance box in page space, in the same frame as the span's
                    // char_bounds ([x, y, x + advance, y + font_size]).
                    let x = dg.coeffs[4];
                    let y = dg.coeffs[5];
                    (
                        [x, y, x + dg.glyph_width, y + dg.font_size],
                        BoundsSource::Estimate,
                    )
                };

                span.tight_char_bounds.push(tight_bound);
                span.glyph_advances.push(dg.glyph_width);
                span.glyph_bounds_sources.push(source);
            }

            span.bounds_source = aggregate_bounds_source(&span.glyph_bounds_sources);
        }
    }
}

impl Device<'_> for TextExtractionDevice {
    fn set_soft_mask(&mut self, _: Option<SoftMask<'_>>) {}
    fn set_blend_mode(&mut self, _: BlendMode) {}
    fn begin_marked_content(&mut self, _tag: &[u8], _mcid: Option<i32>, actual_text: Option<&str>) {
        if self.actual_text || self.in_annotation_appearance() {
            self.actual_text_stack.push(ActualTextEntry {
                actual_text: actual_text.map(str::to_string),
                consumed: false,
                saw_glyph: false,
                child_resolved: false,
                extent: None,
            });
        }
        if !self.raw_runs {
            return;
        }
        let id = self.raw_bdc_next;
        self.raw_bdc_next += 1;
        self.raw_bdc_stack.push(id);
        self.raw_bdc_changed = true;
    }
    fn end_marked_content(&mut self) {
        if let Some(entry) = self.actual_text_stack.pop() {
            self.emit_glyphless_replacement(entry);
        }
        if !self.raw_runs {
            return;
        }
        self.raw_bdc_stack.pop();
        self.raw_bdc_changed = true;
    }
    fn draw_path(
        &mut self,
        path: &BezPath,
        transform: Affine,
        paint: &Paint<'_>,
        mode: &PathDrawMode,
    ) {
        self.record_rule(path, transform);
        self.record_rulings(path, transform);
        self.record_path_box(path, transform);
        if matches!(mode, PathDrawMode::Fill(_)) {
            self.record_fill(path, transform, paint);
        }
        if !self.actual_text_stack.is_empty() {
            let mut drawn = transform.transform_rect_bbox(path.bounding_box());
            if let Some(clip) = self.clip_stack.last() {
                drawn = drawn.intersect(*clip);
            }
            self.note_drawn_box([drawn.x0, drawn.y0, drawn.x1, drawn.y1]);
        }
    }
    fn begin_annotation_appearance(&mut self, rect: kurbo::Rect) {
        self.annotation_appearances.push(rect);
    }
    fn end_annotation_appearance(&mut self) {
        self.annotation_appearances.pop();
    }
    fn push_clip_path(&mut self, clip_path: &ClipPath) {
        // The clip arrives in page space already -- `render.rs` applies it
        // under the identity transform -- so its box needs no transform here.
        // The box and not the region: a clip shaped like a star still reaches
        // only its own bounding box, so this over-states what is visible and
        // never under-states it, which is the direction that cannot invent a
        // rule.
        let box_of_clip = clip_path.path.bounding_box();
        let effective = match self.clip_stack.last() {
            Some(outer) => outer.intersect(box_of_clip),
            None => box_of_clip,
        };
        self.clip_stack.push(effective);
    }
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'_>>, _: BlendMode) {}
    fn draw_image(&mut self, image: Image<'_, '_>, transform: Affine) {
        self.record_image(image.width(), image.height(), transform);
        if !self.actual_text_stack.is_empty() {
            let (w, h) = (
                f64::from(image.width().max(1)),
                f64::from(image.height().max(1)),
            );
            let mut drawn = transform.transform_rect_bbox(kurbo::Rect::new(0.0, 0.0, w, h));
            if let Some(clip) = self.clip_stack.last() {
                drawn = drawn.intersect(*clip);
            }
            self.note_drawn_box([drawn.x0, drawn.y0, drawn.x1, drawn.y1]);
        }
    }
    fn pop_clip_path(&mut self) {
        self.clip_stack.pop();
    }
    fn pop_transparency_group(&mut self) {}
    fn draw_placed_text(&mut self, text: &str, rect: kurbo::Rect) {
        self.push_placed_span(text, rect);
    }

    fn draw_glyph(
        &mut self,
        glyph: &Glyph<'_>,
        transform: Affine,
        glyph_transform: Affine,
        paint: &Paint<'_>,
        draw_mode: &GlyphDrawMode,
    ) {
        // A glyph whose text an `/ActualText` replacement stands in for, or
        // that the page hides and the output may drop, cannot carry a mark
        // offered across it, so the glyph before it is not read again: the
        // offer is then refused and both read as before (spec review (b),
        // review 14-10 02:00 (ii)).
        let color = paint_to_rgba(paint);
        let glyph_hidden = matches!(draw_mode, GlyphDrawMode::Invisible)
            || color.is_some_and(|color| color[3] == 0);
        let replaced = self.actual_text
            && self
                .actual_text_stack
                .iter()
                .any(|entry| entry.actual_text.is_some());
        if replaced || glyph_hidden {
            self.last_glyph_text = None;
        } else {
            self.reread_last_glyph(glyph);
        }
        // Whatever this glyph does, the glyph before it is no longer the last
        // one, unless this is the stroke that repeats its fill.
        let before = self.last_glyph_text.take();
        // The glyph is read once: a read decides a placement offered to it
        // and every later read answers the same, so reading it again here
        // would only repeat the font's lookup.
        let unicode = glyph.as_unicode();

        // FILL AND STROKE IS ONE GLYPH (#623). Render mode 2 draws each glyph
        // filled and then stroked -- how a heading is made heavier without a
        // bold face -- and the interpreter hands both to the device, since a
        // renderer needs both. Read twice, `1.0 BACKGROUND` came out as
        // `BBAACCKKGGRROOUUNNDD` (GAMOBUDU LANDFILL, DocLayNet validation) and
        // the heading matched nothing. The stroke that repeats the fill just
        // before it, at the same transform, is the same character.
        let placed = (transform * glyph_transform).as_coeffs();
        let repeats_fill = matches!(draw_mode, GlyphDrawMode::Stroke(_))
            && self
                .last_fill
                .as_ref()
                .is_some_and(|(at, filled)| *at == placed && *filled == unicode);
        self.last_fill = match draw_mode {
            GlyphDrawMode::Fill => Some((placed, unicode.clone())),
            _ => None,
        };
        if repeats_fill {
            self.last_glyph_text = before;
            return;
        }

        let reads_as_nothing = unicode.is_none();
        let glyph_text = match unicode {
            Some(BfString::Char(c)) => c.to_string(),
            Some(BfString::String(s)) => s,
            None => String::new(),
        };

        let composed = transform * glyph_transform;
        let coeffs = composed.as_coeffs();
        let x = coeffs[4];
        let y = coeffs[5];
        let glyph_scale = (coeffs[0].powi(2) + coeffs[1].powi(2)).sqrt().abs();
        let draw_size = glyph_scale * 1000.0;
        // Under raw runs the size is the area scale of the text matrix, so a
        // squashed line reports what it is set in (#643): `Deceased` reads
        // 6.9296, the geometric mean of its 6.86 by 7 matrix, where the first
        // column alone gives 6.86. Widths keep the drawn scale, so boxes and
        // gaps never move; the default path reports the drawn scale, as
        // before.
        let font_size = if self.raw_runs {
            (coeffs[0] * coeffs[3] - coeffs[1] * coeffs[2]).abs().sqrt() * 1000.0
        } else {
            draw_size
        };

        // G2: distinguish real advance (Metric) from estimate (Estimate).
        let (glyph_width, glyph_ws) = glyph_width_and_source(glyph, draw_size);
        let widened = match glyph {
            Glyph::Outline(outline) => outline.advance_width().map_or(0.0, |w| {
                (glyph_width - f64::from(w) / 1000.0 * font_size).max(0.0)
            }),
            Glyph::Type3(_) => 0.0,
        };
        let glyph_end_x = x + glyph_width;
        let glyph_bound = [x, y, glyph_end_x, y + font_size];

        let style = derive_glyph_style(glyph, self.document_font_names);

        // An `/ActualText` replacement reads instead of the glyphs of its
        // marked run, once, on the run's first glyph; the box of that glyph
        // stays whole for however many letters the replacement holds. The
        // innermost open sequence carrying one wins, and a run replaced by
        // nothing reads as nothing. Later glyphs of the same run add no
        // text and no box, but the pen still moves past what was drawn, so
        // the gap after the run is measured from where the run ended.
        let actual_idx = if self.actual_text {
            self.actual_text_stack
                .iter()
                .rposition(|e| e.actual_text.is_some())
        } else {
            None
        };
        let text = if let Some(idx) = actual_idx {
            let replacement = if self.actual_text_stack[idx].consumed {
                String::new()
            } else {
                let found = self.actual_text_stack[idx]
                    .actual_text
                    .clone()
                    .unwrap_or_default();
                self.actual_text_stack[idx].consumed = true;
                found
            };
            if replacement.is_empty() {
                self.last_y = y;
                self.advance_pen(&glyph_text, glyph_end_x, widened);
                let adv_len = (coeffs[0].powi(2) + coeffs[1].powi(2)).sqrt();
                let (adv_x, adv_y) = if adv_len > 0.0 {
                    (coeffs[0] / adv_len, coeffs[1] / adv_len)
                } else {
                    (1.0, 0.0)
                };
                self.advance_raw_pen(&glyph_text, x, y, adv_x, adv_y, glyph_width);
                self.pending_tj_offset = 0.0;
                return;
            }
            replacement
        } else {
            if reads_as_nothing {
                // Read again at the next glyph all the same: a mark drawn
                // before its base gives its offer up by being read.
                if let Glyph::Outline(outline) = glyph {
                    let record = LastGlyphText {
                        place: None,
                        span: self.spans.len().saturating_sub(1),
                        offset: 0,
                        len: 0,
                        boxless: true,
                        opened: false,
                        char_bounds: 0,
                        width: 0.0,
                        previous_complete_run: None,
                        deferred: self.deferred_rich_glyphs.len(),
                        span_count: 0,
                        tail_text: 0,
                        tail_boxes: 0,
                    };
                    self.remember_last_glyph(outline, record, glyph_hidden);
                }
                return;
            }
            // A glyph that reads as drawn: the open sequences already stand
            // in the text, so their close must not read a replacement too.
            // Its box is still a mark a glyphless close could read at.
            if !self.actual_text_stack.is_empty() {
                for open in self.actual_text_stack.iter_mut() {
                    open.saw_glyph = true;
                }
                self.note_drawn_box(glyph_bound);
            }
            glyph_text
        };
        // The glyph's own text, which the next glyph may make it read
        // differently (see `LastGlyphText`); a replacement is not.
        let own_glyph = match glyph {
            Glyph::Outline(outline) if actual_idx.is_none() => Some(outline),
            _ => None,
        };

        // M3: record glyph data for deferred tight-bounds computation.
        let deferred_before = self.deferred_rich_glyphs.len();
        if self.geometry_mode == GeometryMode::RichGeometry {
            self.record_deferred_glyph(glyph, &composed, font_size, glyph_width);
        }

        // ANN[r17/TEX4] Feed the running sample used to derive the adaptive
        // median character width. Capped to protect against pathological
        // pages with hundreds of thousands of glyphs.
        if self.glyph_widths.len() < 4096 {
            self.glyph_widths.push(glyph_width);
            if self.glyph_widths.len().is_multiple_of(MEDIAN_REFRESH) {
                self.refresh_median_char_width();
            }
        }

        // The gap is what the page drew between this glyph and the last one,
        // judged on the pen the last glyph left. The pen moves past this glyph
        // only after the merge decision -- `advance_pen` on every path below --
        // so the gap reads the previous end, and `last_widened` still carries
        // the previous glyph's floor for the tracking read. Advancing here left
        // every glyph a span of its own and the tracking dead (#623 family).
        let gap = x - self.last_end_x;
        let same_line = (y - self.last_y).abs() <= font_size.max(BAND_Y_TOLERANCE) * 0.35;
        // Under raw runs a turned glyph joins its run along the way it reads
        // (#643): `Form` is one show of four glyphs reading down, and judged
        // on x and y each glyph stands alone. The gap runs along the way this
        // glyph reads and the same-line band across it, off the pen the last
        // glyph left. Two horizontal glyphs keep the judgment above, exactly
        // as before.
        let adv_len = (coeffs[0].powi(2) + coeffs[1].powi(2)).sqrt();
        let (adv_x, adv_y) = if adv_len > 0.0 {
            (coeffs[0] / adv_len, coeffs[1] / adv_len)
        } else {
            (1.0, 0.0)
        };
        let vector = self.raw_runs
            && self.raw_pen_valid
            && (adv_y.abs() > adv_x.abs() || self.raw_dir[1].abs() > self.raw_dir[0].abs());
        let (same_line, gap, dir_changed) = if vector {
            let along = (x - self.raw_pen[0]) * adv_x + (y - self.raw_pen[1]) * adv_y;
            let across_x = (x - self.raw_pen[0]) - along * adv_x;
            let across_y = (y - self.raw_pen[1]) - along * adv_y;
            let across = (across_x.powi(2) + across_y.powi(2)).sqrt();
            let aligned = adv_x * self.raw_dir[0] + adv_y * self.raw_dir[1] > 0.999;
            (aligned && across <= font_size * 0.35, along, !aligned)
        } else {
            (same_line, gap, false)
        };
        let adjacent = same_line && gap >= -font_size * 0.25 && gap < font_size * 0.5;

        // G1: only merge into the previous span when font + style + color
        // match. Otherwise the editor's style toolbar would render the wrong
        // state for the cursor position.
        //
        // A span is one font at one size and style: where the caller asks
        // for the document's names, the reported name parts a span even
        // where the merge face does not. Two document fonts can stand on one
        // face, and merged the second font's name and descriptor metrics are
        // lost with it (#636). Words still join across the split at word
        // level. The default path keeps merging on the face alone, as it
        // did: a program that names no face still reads under the page's
        // name (#574).
        let same_font = !self.document_font_names
            || self
                .spans
                .last()
                .is_some_and(|last| last.font_name == style.font_name);
        let style_matches = self
            .spans
            .last()
            .map(|last| {
                self.last_merge_face == style.merge_face
                    && same_font
                    && last.is_bold == style.is_bold
                    && last.is_italic == style.is_italic
                    && last.color == color
            })
            .unwrap_or(false);
        // Under raw runs a run ends where the type size changes: `MM` at 7pt
        // and the space at 4.95pt the page sets between it and `DD` are one
        // line but three runs (#643). The default path merges across sizes, as
        // before.
        let same_size = !self.raw_runs
            || self.spans.last().is_some_and(|last| {
                let big = last.font_size.max(font_size).max(1.0);
                (last.font_size - font_size).abs() <= RAW_SIZE_TOLERANCE * big
            });
        // A segment that opened or closed since the last glyph ends the run,
        // so a run never crosses the segment it was drawn in (#643). Only
        // ever set under raw runs; read and cleared here, on every glyph.
        let bdc_changed = std::mem::replace(&mut self.raw_bdc_changed, false);
        // Under raw runs a run ends where the page's declared face changes,
        // for the reason `last_raw_declared` gives (#643). A glyph the page
        // named nothing for never parts a run on its own.
        let declared = if self.raw_runs {
            match glyph {
                Glyph::Outline(outline) => outline
                    .declared_name()
                    .map(|d| strip_subset_prefix(&d).to_string()),
                Glyph::Type3(_) => None,
            }
        } else {
            None
        };
        let same_declared = !self.raw_runs
            || match (&self.last_raw_declared, &declared) {
                (Some(before), Some(now)) => before == now,
                _ => true,
            };

        if adjacent
            && !self.spans.is_empty()
            && style_matches
            && same_size
            && same_declared
            && !bdc_changed
            && !dir_changed
        {
            // ANN[r17/TEX1] Multi-signal consensus replaces the prior
            // single-threshold rule (`gap > 0.15 * font_size`). The
            // consensus evaluates TJ offset, geometric gap, and
            // character-class transitions; a space is inserted only
            // when the weighted sum meets SPACE_CONSENSUS_THRESHOLD.
            // Decision is computed before the mutable borrow of `last`
            // to keep the borrow checker happy.
            // AND THE TYPESETTER'S TRACKING IS NOT A WORD SPACE (#582). The
            // consensus fires on every pair of a letter-spaced display line,
            // so the regularity of the run is asked about before its answer is
            // believed.
            //
            // BETWEEN TWO LETTERS THE RUN IS JUDGED ON THE GAP THE PAGE DREW
            // (#623). Two things make a tracked run's gaps disagree that are not
            // the tracking: the quarter-em floor of `glyph_width_and_source`
            // widens a narrow glyph's box, so the gap after every `I` of a
            // tracked `DIRECTORS’ DECLARATION` (OTC_STKAF_2002) read half the
            // tracking; and a pair kern in the TJ -- `[(ORPORA) 74 (TION)] TJ`
            // in NYSE_PVA_2004 -- takes 40 per cent off one gap. Either broke
            // the regularity for the next four pairs, and the run ended
            // `CORPORAT I O N`. Both are put back. A kern is a POSITIVE TJ, of
            // any size, since it moves the next glyph left: `Twenty-five`
            // tightens its `Tw` by 1.3 points. A small negative one is the
            // tracking itself -- NASDAQ_UCBI_2014 letter-spaces `CONSOLIDATED`
            // with `-80` between every pair -- and stays in the gap. Only
            // between letters and drawn spaces: a formula's commas and dots are
            // floored too, and put back there they make `{1, ..., n}` as
            // regular as tracking (1201.4397).
            let prev_char = self.spans.last().and_then(|last| last.text.chars().last());
            let next_char = text.chars().next();
            let letters = prev_char.is_some_and(char::is_alphabetic)
                && next_char.is_some_and(char::is_alphabetic);
            // A drawn space in a tracked line is floored and kerned like its
            // letters: `BACK TO` (NYSE_WAL_2014) kerns the `T` after its space
            // by 25, and left raw that one gap kept the run from being seen.
            let letter_or_space =
                |c: Option<char>| c.is_some_and(|c| c.is_alphabetic() || c == ' ');
            let drawn_gap = if letter_or_space(prev_char) && letter_or_space(next_char) {
                let kern = f64::from(self.pending_tj_offset.max(0.0)) / 1000.0 * font_size;
                gap + self.last_widened + kern
            } else {
                gap
            };
            let tracked = self.a_tracked_run(drawn_gap);
            let want_space = {
                let last = self.spans.last().expect("checked non-empty");
                !last.text.ends_with(' ')
                    && !text.starts_with(' ')
                    && !tracked
                    && self.evaluate_space_consensus(gap, font_size, &last.text, &text)
            };
            // EVERY GAP, and not only the ones wide enough to be a space.
            // Filtering to the wide ones leaves a run of word spaces, which
            // agree with one another as neatly as tracking does -- that is how
            // `in a healthy` came back as `in ahealthy`. What tells the two
            // apart is the gaps INSIDE the words: tracking adds its constant
            // to those as well, and prose does not.
            //
            // A gap that breaks the regularity ends one run and the next
            // starts after it, and a take-back never reaches back past that
            // start: the gap itself is the boundary, so the word space before
            // a letter-spaced `C O M B I N E` stays (NYSE_GLW_2004). A span's
            // first gap has nothing to break, and it is the run's own. Only a
            // gap between two letters breaks a run, measured against the last
            // gap between two letters: a drawn space glyph sits a little off
            // the tracking on both sides, and a break there left `I T ALL`
            // (NYSE_WAL_2014) and `D ECLARATION` (OTC_STKAF_2002).
            if letters {
                if self.last_letter_gap.is_some_and(|before| {
                    (before - drawn_gap).abs() > TRACKING_TOLERANCE * drawn_gap
                }) {
                    self.run_start = self.spans.last().map_or(0, |last| last.text.len() + 1);
                }
                self.last_letter_gap = Some(drawn_gap);
            }
            self.recent_gaps.push(drawn_gap);
            if self.recent_gaps.len() > TRACKING_MIN_PAIRS * 2 {
                self.recent_gaps.remove(0);
            }
            let span_index = self.spans.len() - 1;
            let last = self.spans.last_mut().expect("checked non-empty");
            if tracked {
                Self::take_back_tracked_spaces(
                    &mut last.text,
                    &mut self.inserted_spaces,
                    self.run_start,
                    drawn_gap,
                );
            }
            if want_space {
                self.inserted_spaces.push((last.text.len(), drawn_gap));
                last.text.push(' ');
            }
            let offset = last.text.len();
            let char_bounds = last.char_bounds.len();
            let width = last.width;
            last.text.push_str(&text);
            last.width = last.width.max(glyph_end_x - last.x);
            last.height = last.height.max(font_size);
            // G2: append char_bound; downgrade width_source if this glyph is Estimate.
            last.char_bounds.push(glyph_bound);
            if glyph_ws == WidthSource::Estimate {
                last.width_source = WidthSource::Estimate;
            }
            if let Some(outline) = own_glyph {
                let record = LastGlyphText {
                    place: None,
                    span: span_index,
                    offset,
                    len: text.len(),
                    boxless: false,
                    opened: false,
                    char_bounds,
                    width,
                    previous_complete_run: None,
                    deferred: deferred_before,
                    span_count: 0,
                    tail_text: 0,
                    tail_boxes: 0,
                };
                self.remember_last_glyph(outline, record, glyph_hidden);
            }
            self.last_y = y;
            self.advance_pen(&text, glyph_end_x, widened);
            self.advance_raw_pen(&text, x, y, adv_x, adv_y, glyph_width);
            // ANN[r17/TEX1] Consume the TJ signal: it only counts for
            // the one merge it preceded.
            self.pending_tj_offset = 0.0;
            return;
        }

        self.last_y = y;
        self.advance_pen(&text, glyph_end_x, widened);
        self.advance_raw_pen(&text, x, y, adv_x, adv_y, glyph_width);
        // ANN[r17/TEX1] Non-adjacent glyph starts a fresh span, so any
        // pending TJ offset is about within-span word breaks and no longer
        // meaningful here.
        self.pending_tj_offset = 0.0;
        // AND A NEW SPAN IS A NEW RUN (#582). The gaps belong to the run they
        // were measured in; carried across, the first gap of a new line is
        // judged against the last line's tracking, and a line that opens with
        // a one-letter word loses its space -- `A report` as `Areport`. The
        // placed-text path clears this for the same reason.
        self.recent_gaps.clear();
        self.inserted_spaces.clear();
        self.run_start = 0;
        self.last_letter_gap = None;

        let span = TextSpan {
            text,
            x,
            y,
            width: glyph_width,
            height: font_size,
            font_size,
            font_name: style.font_name.clone(),
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
            is_ocr_layer: style.is_ocr_layer,
            font_metrics: style.font_metrics,
            geometry_mode: self.geometry_mode,
            bounds_source: BoundsSource::Advance,
            tight_char_bounds: Vec::new(),
            glyph_advances: Vec::new(),
            glyph_bounds_sources: Vec::new(),
            complete_run: false,
        };

        self.last_merge_face = style.merge_face;
        let previous_complete_run = self.spans.last().map(|prev| prev.complete_run);
        if self.raw_runs {
            // A run ends where the style, the size, the declared face or the
            // reading direction does; a segment or a text object alone never
            // ends one (`4a` reads as one span across two segments, and the
            // foreign-address fields merge across text objects), and neither
            // does a purely geometric part -- a gap or a backward step inside
            // one show -- so a consumer joining same-style spans may still
            // merge there (#643).
            let run_end = !style_matches || !same_size || !same_declared || dir_changed;
            if let Some(prev) = self.spans.last_mut() {
                prev.complete_run = run_end;
            }
            self.raw_bdc.push(self.raw_bdc_stack.last().copied());
            self.last_raw_declared = declared;
        }
        let record = own_glyph.map(|outline| {
            let record = LastGlyphText {
                place: None,
                span: self.spans.len(),
                offset: 0,
                len: span.text.len(),
                boxless: false,
                opened: true,
                char_bounds: 0,
                width: 0.0,
                previous_complete_run,
                deferred: deferred_before,
                span_count: 0,
                tail_text: 0,
                tail_boxes: 0,
            };
            (outline, record)
        });
        self.spans.push(span);
        if let Some((outline, record)) = record {
            self.remember_last_glyph(outline, record, glyph_hidden);
        }
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
    /// The face spans merge on: `font_name` as the program names it, without
    /// the /BaseFont fallback (see `last_merge_face`).
    merge_face: Option<String>,
    is_bold: bool,
    is_italic: bool,
    /// Numeric weight from embedded font data, if available.
    font_weight: Option<u16>,
    /// Serif flag from embedded font data, if available.
    is_serif: Option<bool>,
    /// Monospace flag from embedded font data, if available.
    is_monospace: Option<bool>,
    /// Whether the glyph was set in the engine's own OCR layer resource.
    is_ocr_layer: bool,
    /// Vertical font metrics from embedded font data, if available.
    font_metrics: Option<FontMetrics>,
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

/// Whether a font name says monospace, for faces whose program is out of
/// reach (standard-14 and other non-embedded fonts).
///
/// Lowercased, spaceless substring match over known monospace families, the
/// way [`name_style_hints`] reads bold and italic. `Some(true)` on a hit,
/// `None` otherwise: a name that says nothing keeps the unknown it had. A
/// bare "mono" is never matched: decorative faces carry it too.
fn name_monospace_hint(name: &str) -> Option<bool> {
    let squashed: String = name
        .to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    const MONOSPACE_FAMILIES: &[&str] = &[
        "courier",
        "monaco",
        "menlo",
        "consolas",
        "liberationmono",
        "dejavusansmono",
        "notosansmono",
        "sfmono",
        "ubuntumono",
        "robotomono",
        "ptmono",
        "droidsansmono",
        "cascadiacode",
        "cascadiamono",
        "firacode",
        "firamono",
        "inconsolata",
        "jetbrainsmono",
        "sourcecodepro",
        "lucidaconsole",
        "nimbusmono",
        "texgyrecursor",
        "andalemono",
        "lettergothic",
        "prestigeelite",
        "anonymouspro",
        "oxygenmono",
        "overpassmono",
        "plexmono",
        "spacemono",
        "sharetechmono",
        "cutivemono",
        "novamono",
        "fixedsys",
        "typewriter",
        "monospace",
    ];
    if MONOSPACE_FAMILIES
        .iter()
        .any(|family| squashed.contains(family))
    {
        Some(true)
    } else {
        None
    }
}

fn derive_glyph_style(glyph: &Glyph<'_>, document_font_names: bool) -> GlyphStyle {
    let mut style = glyph_style_from_font(glyph, document_font_names);
    // THE ENGINE'S OWN VOICE. Text set in the OCR layer resource is the
    // layer the engine itself wrote, not ink the author hid: the hidden-text
    // drop keeps it. The resource name is the only mark that survives, since
    // the layer stands on Helvetica like any author text.
    style.is_ocr_layer = glyph
        .font_resource_name()
        .is_some_and(|name| name.as_bytes() == pdf_ocr::pipeline::OCR_FONT_NAME);
    // THE PAGE'S NAME, BESIDE THE FACE'S (#623). A font that stands on a
    // standard face -- an embedded CFF or Type1 whose program the renderer
    // replaced, a named face that is not embedded -- reaches here as that
    // face: `RotisSansSerif-Light`, its regular and its `-Bold` were all
    // `Helvetica`, and a `-Bold` or `-Italic` the page asked for was lost.
    // `font_name` stays the face's, because span merging compares it and a
    // formula's faces must keep merging as they did; bold and italic read
    // the page's name as well.
    if let Glyph::Outline(outline) = glyph {
        if let Some(raw) = outline.declared_name() {
            let declared = strip_subset_prefix(&raw).to_string();
            if !declared.is_empty() && style.font_name.as_deref() != Some(declared.as_str()) {
                let (bold, italic) = name_style_hints(&declared);
                style.is_bold |= bold;
                style.is_italic |= italic;
                style.is_monospace = style.is_monospace.or(name_monospace_hint(&declared));
            }
        }
    }
    style
}

fn glyph_style_from_font(glyph: &Glyph<'_>, document_font_names: bool) -> GlyphStyle {
    match glyph {
        Glyph::Outline(outline) => {
            if let Some(data) = outline.font_data() {
                let raw = data.postscript_name.as_deref().unwrap_or("");
                let name = strip_subset_prefix(raw).to_string();
                let weight_bold = data.weight.is_some_and(|w| w >= 700);
                let (name_bold, name_italic) = name_style_hints(&name);
                let font_name = if name.is_empty() { None } else { Some(name) };
                GlyphStyle {
                    merge_face: font_name.clone(),
                    font_name,
                    is_bold: weight_bold || name_bold,
                    is_italic: data.is_italic || name_italic,
                    font_weight: data.weight.map(|w| w.clamp(1, 1000) as u16),
                    is_serif: Some(data.is_serif),
                    is_monospace: Some(data.is_monospace),
                    // Read back in `derive_glyph_style`, the only caller.
                    is_ocr_layer: false,
                    font_metrics: match (data.ascent, data.descent) {
                        (Some(ascent), Some(descent)) => Some(FontMetrics {
                            ascent,
                            descent,
                            cap_height: data.cap_height,
                            x_height: data.x_height,
                        }),
                        _ => None,
                    },
                }
            } else if document_font_names {
                // Type1 / non-embedded font — descriptor not surfaced
                // via font_data(). Fall back to the name-only
                // accessor which works for standard-14 fallbacks,
                // and use the canonical Standard-14 AFM metrics
                // when the outline font resolves to a known face.
                //
                // THE MERGE KEY STAYS THE FACE'S, THE REPORTED NAME IS THE
                // PAGE'S (#643 E3). Spans merge on the face the program
                // names (or the substitute it stands on), exactly as they
                // did (#574); `font_name` is the name the page set its
                // text in. Style reads both, as it did (#623): a `Bd` the
                // page names is bold because the face says `Bold`, and a
                // `Bliss-LightItalic` no face names is italic because the
                // page says so.
                let face =
                    strip_subset_prefix(&outline.postscript_name().unwrap_or_default()).to_string();
                let merge_face = if face.is_empty() {
                    None
                } else {
                    Some(face.clone())
                };
                let raw = outline.document_name().unwrap_or_default();
                let name = strip_subset_prefix(&raw).to_string();
                // AND WHERE THE PROGRAM NAMES NOTHING, THE PAGE'S NAME (#574).
                // An embedded Type1 or CFF reaches here with no descriptor and,
                // unless it stands on a standard face, no PostScript name: every
                // span of `DIN-Medium`, `CMR12`, `MetalworkBold` had no face at
                // all -- five of the rig-like proxy's 36 documents, 105 of its
                // 389 pages, on which `font_is_the_body_face` could say nothing.
                // A Type1 always declares a /BaseFont. Spans still merge on the
                // program's name (`merge_face`).
                let name = if name.is_empty() {
                    outline
                        .declared_name()
                        .map(|d| strip_subset_prefix(&d).to_string())
                        .unwrap_or_default()
                } else {
                    name
                };
                let (name_bold, name_italic) = name_style_hints(&name);
                let (face_bold, face_italic) = name_style_hints(&face);
                let metrics = outline.font_metrics().map(|(a, d, c, x)| FontMetrics {
                    ascent: a,
                    descent: d,
                    cap_height: c,
                    x_height: x,
                });
                GlyphStyle {
                    font_name: if name.is_empty() { None } else { Some(name) },
                    merge_face,
                    is_bold: name_bold || face_bold,
                    is_italic: name_italic || face_italic,
                    // Non-embedded font: descriptor metrics are not reachable.
                    font_weight: None,
                    is_serif: None,
                    is_monospace: None,
                    // Read back in `derive_glyph_style`, the only caller.
                    is_ocr_layer: false,
                    font_metrics: metrics,
                }
            } else {
                // THE DEFAULT ANSWERS AS cc2f37b DID (#643 follow-up). The
                // option is off: the substitute face's name and AFM pair,
                // style hints from that name alone -- the merge key and
                // `font_is_the_body_face` see exactly what they saw before.
                // `substitute_metrics` is the pre-E4 metrics source under
                // its new name; the rest is verbatim.
                let raw = outline.postscript_name().unwrap_or_default();
                let name = strip_subset_prefix(&raw).to_string();
                let merge_face = if name.is_empty() {
                    None
                } else {
                    Some(name.clone())
                };
                // AND WHERE THE PROGRAM NAMES NOTHING, THE PAGE'S NAME (#574).
                // An embedded Type1 or CFF reaches here with no descriptor and,
                // unless it stands on a standard face, no PostScript name: every
                // span of `DIN-Medium`, `CMR12`, `MetalworkBold` had no face at
                // all -- five of the rig-like proxy's 36 documents, 105 of its
                // 389 pages, on which `font_is_the_body_face` could say nothing.
                // A Type1 always declares a /BaseFont. Spans still merge on the
                // program's name (`merge_face`).
                let name = if name.is_empty() {
                    outline
                        .declared_name()
                        .map(|d| strip_subset_prefix(&d).to_string())
                        .unwrap_or_default()
                } else {
                    name
                };
                let (name_bold, name_italic) = name_style_hints(&name);
                let metrics = outline
                    .substitute_metrics()
                    .map(|(a, d, c, x)| FontMetrics {
                        ascent: a,
                        descent: d,
                        cap_height: c,
                        x_height: x,
                    });
                let monospace = name_monospace_hint(&name);
                GlyphStyle {
                    font_name: if name.is_empty() { None } else { Some(name) },
                    merge_face,
                    is_bold: name_bold,
                    is_italic: name_italic,
                    // Non-embedded font: descriptor metrics are not reachable.
                    font_weight: None,
                    is_serif: None,
                    is_monospace: monospace,
                    // Read back in `derive_glyph_style`, the only caller.
                    is_ocr_layer: false,
                    font_metrics: metrics,
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

/// Two baselines run in the same direction when they are within half a degree
/// of each other. A page draws a run of text with one text matrix, so the
/// glyphs of one run agree to the last bit; the tolerance is there for the
/// rounding a document writes its matrix with (`0.7071 -0.7071`), not for
/// telling two angles apart.
const DIRECTION_TOLERANCE_RAD: f64 = 0.008_727; // 0.5 degrees

/// How far a baseline may stand from a quarter turn and still read as one:
/// ten degrees (#574). The defect's runs stand at a quarter or half turn; a
/// typeset line that merely drifts -- the 3-degree skew the guard test pins --
/// stays well inside this, and so does the rounding a document writes its
/// matrix with.
const QUARTER_TURN_TOLERANCE_RAD: f64 = 0.174_533; // 10 degrees

/// Whether the span's baseline stands at a quarter turn to the page: within
/// ten degrees of 90, 180 or 270 degrees (#574). The defect is a
/// quarter-turned run read in page space -- six reversed runs on the tuning
/// pages, all at 90 degrees -- and only such a run is ever re-grouped. A
/// free-angle region already reads forwards and must not be: merging a
/// 60-degree region re-groups text that was never broken (OTC_STKAF p-c3c8:
/// spans 197 -> 31, text -0.107). Upright text and small skews are not
/// quarter-turned, and neither is anything on a majority-turned page, which
/// [`PageTurn::into_frame`] lays upright before this is ever asked.
fn span_is_quarter_turned(span: &TextSpan) -> bool {
    let direction = span_direction(span);
    direction_gap(direction, (0.0, 1.0)) <= QUARTER_TURN_TOLERANCE_RAD
        || direction_gap(direction, (-1.0, 0.0)) <= QUARTER_TURN_TOLERANCE_RAD
        || direction_gap(direction, (0.0, -1.0)) <= QUARTER_TURN_TOLERANCE_RAD
}

/// A baseline direction: a unit vector in page space.
type Direction = (f64, f64);

/// A span taken out of the page's blocks, carrying the position in the page's
/// order that the block it came from held.
type PlacedSpan = (usize, TextSpan);

/// The direction the baseline of a glyph runs in, as a unit vector in page
/// space: the first column of its composed transform, normalised.
///
/// This is what the text matrix rotates. `(1, 0)` for upright text, `(0, 1)`
/// for a page's 90-degree marginalia, `(0.7071, -0.7071)` for the diagonal
/// watermark of pdfjs issue17069 -- and `(1, 0)` again for a degenerate
/// transform, which has no direction to speak of and may as well read as the
/// page's own (#435).
fn baseline_direction(coeffs: &[f64; 6]) -> Direction {
    let (a, b) = (coeffs[0], coeffs[1]);
    let length = (a * a + b * b).sqrt();
    if length <= f64::EPSILON {
        (1.0, 0.0)
    } else {
        (a / length, b / length)
    }
}

/// The direction a span's baseline runs in. A span not built from a glyph
/// carries no transform and reads as upright.
fn span_direction(span: &TextSpan) -> Direction {
    span.transform
        .as_ref()
        .map(baseline_direction)
        .unwrap_or((1.0, 0.0))
}

/// The angle between two unit vectors, in radians, without a sign.
fn direction_gap(lhs: Direction, rhs: Direction) -> f64 {
    let cross = lhs.0 * rhs.1 - lhs.1 * rhs.0;
    let dot = lhs.0 * rhs.0 + lhs.1 * rhs.1;
    cross.atan2(dot).abs()
}

fn directions_agree(lhs: Direction, rhs: Direction) -> bool {
    direction_gap(lhs, rhs) <= DIRECTION_TOLERANCE_RAD
}

/// How far along its own baseline, and how far across it, a span's origin sits.
///
/// The pair is the page's `(x, y)` seen from the baseline: `along` grows in
/// reading direction, `across` grows to the left of it, so a line of rotated
/// text has one `across` and a range of `along` exactly as an upright line has
/// one `y` and a range of `x`.
fn baseline_coordinates(span: &TextSpan, direction: Direction) -> (f64, f64) {
    (
        span.x * direction.0 + span.y * direction.1,
        -span.x * direction.1 + span.y * direction.0,
    )
}

/// Where a span stops along its own baseline.
///
/// `width` is the widest a glyph of the span reached in PAGE x, which is the
/// along-baseline extent only while the text is upright: a run drawn at thirty
/// degrees advances `cos 30` of its length in x, and reading that as the length
/// leaves an eighth of the run behind it as phantom gap -- enough to put a
/// space in the middle of a word where a style change ends one span and starts
/// the next. So the last glyph's own bound says where the run stopped, and
/// `width` answers only when there are no bounds to ask (every hand-built span)
/// or when it reaches further, which is what an overprinted or backwards-drawn
/// run does and what the seam allowance of #432 is measured against (#435).
fn baseline_end(span: &TextSpan, direction: Direction) -> f64 {
    let start = baseline_coordinates(span, direction).0;
    let from_width = start + span.width;
    match span.char_bounds.last() {
        Some(bound) => {
            let last = bound[0] * direction.0 + bound[1] * direction.1;
            let advance = bound[2] - bound[0];
            (last + advance).max(from_width)
        }
        None => from_width,
    }
}

/// Threshold for detecting rotation or shear in the composed transform.
/// Any |b| or |c| above this is treated as rotation; any difference between
/// a and d above this is treated as non-uniform scale (= shear potential).
const ROTATION_EPSILON: f64 = 1e-6;
const SHEAR_EPSILON: f64 = 1e-3;

/// Transform a bounding-box `[x0,y0,x1,y1]` (in local space) to page space
/// by applying the affine to its four corners and taking the envelope.
fn transform_bbox_corners(local_bbox: &[f64; 4], affine: &Affine) -> [f64; 4] {
    use kurbo::Point;
    let corners = [
        *affine * Point::new(local_bbox[0], local_bbox[1]),
        *affine * Point::new(local_bbox[2], local_bbox[1]),
        *affine * Point::new(local_bbox[2], local_bbox[3]),
        *affine * Point::new(local_bbox[0], local_bbox[3]),
    ];
    let x0 = corners.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
    let y0 = corners.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
    let x1 = corners
        .iter()
        .map(|p| p.x)
        .fold(f64::NEG_INFINITY, f64::max);
    let y1 = corners
        .iter()
        .map(|p| p.y)
        .fold(f64::NEG_INFINITY, f64::max);
    [x0, y0, x1, y1]
}

/// Collapse fake-bold / overprint duplicates inside one band.
///
/// Real-word corpus failures such as 0105.pdf draw the same text several times
/// with sub-point x drift to simulate heavier weight. Text extraction should
/// keep the most informative span once rather than concatenate every overprint.
/// A span the page drew over split around the narrower span it covers.
///
/// Merged spans join a glyph run with the spaces between its glyphs, so one
/// span can cover the x of a span the page drew between them: the form's
/// `MM      DD      YYYY` field span covers the `/` separators set between
/// its groups, and left whole no x-order of spans interleaves them again --
/// the line reads `MM DD YYYY/ /` (#636). Where a narrow sibling sits fully
/// inside the span on the same baseline and the span maps its characters to
/// its glyphs one to one, the span is cut at the whitespace gap holding the
/// sibling, so the reading order puts the separator back between its groups.
/// A sibling over ink rather than white never cuts: the split point must sit
/// under the sibling at a whitespace character.
fn split_spans_covering_siblings(spans: &mut Vec<TextSpan>) {
    let mut changed = true;
    while changed {
        changed = false;
        let mut at = 0usize;
        while at < spans.len() {
            if let Some((left, right)) = split_cover_at_sibling(spans, at) {
                spans[at] = left;
                spans.insert(at + 1, right);
                changed = true;
                at += 2;
            } else {
                at += 1;
            }
            if spans.len() > 4096 {
                return;
            }
        }
    }
}

/// The two halves of `spans[cover]` cut around one span it covers, or `None`.
///
/// `SIBLING_WIDTH_EM` and `COVER_MARGIN` are above; the y check keeps the cut
/// to the band the sort already grouped, since a tall span can reach into the
/// band over or under its own.
fn split_cover_at_sibling(spans: &[TextSpan], cover: usize) -> Option<(TextSpan, TextSpan)> {
    const SIBLING_WIDTH_EM: f64 = 0.6;
    const COVER_MARGIN: f64 = 1.0;
    let (cover_left, cover_right) = {
        let span = &spans[cover];
        (span.x, span.measured_right())
    };
    if cover_right <= cover_left {
        return None;
    }
    let chars: Vec<char> = spans[cover].text.chars().collect();
    if chars.len() < 3 || chars.len() != spans[cover].char_bounds.len() {
        return None;
    }
    for (sib, sibling) in spans.iter().enumerate() {
        if sib == cover {
            continue;
        }
        let em = sibling.font_size.max(1.0);
        if sibling.width > SIBLING_WIDTH_EM * em {
            continue;
        }
        if (sibling.y - spans[cover].y).abs() > 0.35 * spans[cover].font_size.max(em) {
            continue;
        }
        let (sib_left, sib_right) = (sibling.x, sibling.measured_right());
        if !(sib_left > cover_left + COVER_MARGIN && sib_right < cover_right - COVER_MARGIN) {
            continue;
        }
        // The whitespace gap under the sibling: the char boundary closest to
        // the sibling's centre whose glyphs stand on either side of it.
        let centre = (sib_left + sib_right) * 0.5;
        let boxes = &spans[cover].char_bounds;
        let mut cut: Option<usize> = None;
        let mut best = f64::INFINITY;
        for i in 1..chars.len() {
            if !(chars[i - 1].is_whitespace() || chars[i].is_whitespace()) {
                continue;
            }
            let edge = (boxes[i - 1][2] + boxes[i][0]) * 0.5;
            if edge < sib_left || edge > sib_right {
                continue;
            }
            let distance = (edge - centre).abs();
            if distance < best {
                best = distance;
                cut = Some(i);
            }
        }
        let Some(i) = cut else { continue };
        let mut left = spans[cover].clone();
        let mut right = spans[cover].clone();
        left.text = chars[..i].iter().collect();
        right.text = chars[i..].iter().collect();
        left.char_bounds = boxes[..i].to_vec();
        right.char_bounds = boxes[i..].to_vec();
        split_parallel_glyph_vecs(&mut left, &mut right, i, chars.len());
        left.width = (boxes[i - 1][2] - left.x).max(0.0);
        right.x = boxes[i][0];
        right.width = (cover_right - right.x).max(0.0);
        return Some((left, right));
    }
    None
}

/// Split the per-glyph vectors alongside [`split_cover_at_sibling`]'s cut.
///
/// A vector that does not run with `char_bounds` (nothing names such a case
/// today) is dropped on both halves rather than cut at the wrong place.
fn split_parallel_glyph_vecs(left: &mut TextSpan, right: &mut TextSpan, at: usize, len: usize) {
    if left.tight_char_bounds.len() == len {
        right.tight_char_bounds = left.tight_char_bounds[at..].to_vec();
        left.tight_char_bounds.truncate(at);
    } else {
        left.tight_char_bounds.clear();
        right.tight_char_bounds.clear();
    }
    if left.glyph_advances.len() == len {
        right.glyph_advances = left.glyph_advances[at..].to_vec();
        left.glyph_advances.truncate(at);
    } else {
        left.glyph_advances.clear();
        right.glyph_advances.clear();
    }
    if left.glyph_bounds_sources.len() == len {
        right.glyph_bounds_sources = left.glyph_bounds_sources[at..].to_vec();
        left.glyph_bounds_sources.truncate(at);
    } else {
        left.glyph_bounds_sources.clear();
        right.glyph_bounds_sources.clear();
    }
}

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

    let size = lhs.font_size.max(rhs.font_size);
    let drift = (lhs.y - rhs.y).abs();
    // Ink over ink: `right()` widens a short span to half an em per character
    // for layout, and through that estimate two spans standing side by side
    // count as mostly overlapping -- `t ` beside `t`, `oll` beside `o` -- so
    // the second reads as a fake-bold overprint and a mid-word letter is lost
    // (`follws`, `heir`). A strike over the first sits on the same ink, which
    // the measured edges still overlap; neighbours only touch there.
    // Quarter-turned spans already measure this way: their `right()` is the
    // measured edge.
    let lhs_left = lhs.x;
    let lhs_right = lhs.measured_right();
    let rhs_left = rhs.x;
    let rhs_right = rhs.measured_right();
    let overlap = (lhs_right.min(rhs_right) - lhs_left.max(rhs_left)).max(0.0);
    let min_width = (lhs_right - lhs_left).min(rhs_right - rhs_left).max(1.0);
    let share = overlap / min_width;

    // A SHADOW IS THE SAME TEXT A LITTLE OFF (#1973). A label drawn twice --
    // a halo or drop shadow under it, sometimes a size larger -- sits up to a
    // quarter of an em off the first copy's baseline and overlaps it by less
    // than a fake-bold overprint does: `KFTC` 0.85 point lower at 4.9 points,
    // `NRST` 5.52 over 4.90. Concatenated, it read `KFTCKFTC`. Only for the SAME
    // text, and still only where the two lie mostly over one another, so two
    // cells or two words that happen to agree never meet here. And of two
    // characters or more: one character's shadow and its neighbour are the
    // same picture -- the two `1`s of a matrix, the `ll` of `Ersatzteilliste`
    // -- and a letter lost there is worse than a letter doubled.
    //
    // HALF THE WIDTH IS ENOUGH FROM THREE CHARACTERS ON (#623). NASDAQ_SHEN_2003
    // (DocLayNet) sets its current-year column head twice, the second copy 8.0
    // points right and 0.86 higher at 10.08 points: the two `2003`s overlap by
    // 60 per cent, and the page read `20032003` in ten of its tables where the
    // annotation reads `2003`. Two different cells cannot lie half over one
    // another, and a word of three characters or more drawn twice at that
    // distance is a copy; two characters keep the stricter share, for the
    // reason one character is left alone.
    let marks = lhs_text.chars().filter(|c| !c.is_whitespace()).count();
    let needed = if marks >= 3 { 0.5 } else { 0.7 };
    if lhs_text == rhs_text && marks >= 2 && drift <= size * 0.25 && share >= needed {
        return true;
    }

    // Fake bold: the same line struck again with sub-point drift.
    if drift > size * 0.12 || share < 0.85 {
        return false;
    }
    lhs_text == rhs_text || lhs_text.starts_with(rhs_text) || rhs_text.starts_with(lhs_text)
}

/// Does `curr` carry on from `prev`, or does it lie under it?
///
/// The join in `TextBlock::text` drops a word `curr` repeats from the end of
/// `prev`, because a document that draws that word twice at the seam between
/// two runs holds it once (0105.pdf, #1014). That reading needs `curr` to
/// begin where `prev` stopped. A run that begins a whole character *inside*
/// its predecessor is not the continuation of it: a text field draws its value
/// with the advance the font asks for, which is regularly wider than the field
/// box, so each of three fields on one row of a form reaches tens of points
/// into the next field's column. With the same value in all three -- a
/// repeated date, a column of `N/A`, or Acroform-PDFBOX-2333's own value -- the
/// second and the third read as the first one's tail said again, and dropping
/// them loses two filled fields. On a form that is the loss of entered data
/// (#432, class R-E of #389).
///
/// One glyph of `curr`'s own mean advance is the allowance, and it is there
/// for the seam itself: a measured width carries the last glyph's side bearing
/// and a rounding, never a column of another field.
///
/// The mean is per GLYPH, not per character, and `char_bounds` holds one entry
/// per glyph drawn. A glyph that decodes to several characters -- a `fi`
/// ligature, any `bfrange` that names a string -- would otherwise divide one
/// advance by two or three characters and shrink the allowance below the seam
/// it exists for. The character count answers only for a span that carries no
/// bounds, which is every hand-built one.
fn second_run_carries_on_from_the_first(prev: &TextSpan, curr: &TextSpan) -> bool {
    let glyphs = if curr.char_bounds.is_empty() {
        curr.text.chars().count()
    } else {
        curr.char_bounds.len()
    };
    if glyphs == 0 {
        return false;
    }

    let mean_advance = curr.width / glyphs as f64;
    // Along the previous run's baseline, so that a rotated seam is measured in
    // the direction the two runs actually overlap in (#435).
    let direction = span_direction(prev);
    let (curr_along, _) = baseline_coordinates(curr, direction);
    baseline_end(prev, direction) - curr_along <= mean_advance
}

/// A space drawn over the letter before it is no word space (#574).
///
/// The page draws the word as two runs, and the second run starts with a
/// SPACE character placed LEFT of where the first run ends: after a ligature
/// glyph the next run starts ` cally` 0.3 em back, after an initial capital
/// ` ransition` 0.6 em back. A real word space never starts left of the
/// letter before it, so where the next run overlaps the previous by more
/// than a tenth of an em and opens with a space and a lower-case letter,
/// the join carries no space and the leading space is dropped.
///
/// Lower-case only, and that is the whole of the guard: leading space plus
/// overlap alone fires a thousand times across formulas and tables (`v 2`,
/// `0 N`), and some of those are real gaps. A capital or a digit after the
/// space keeps its space.
///
/// `gap` is the join's own gap, measured along the baseline the same way the
/// 0.25 em rule measures it. Where this can fire the previous run ends in a
/// letter, so it is never a marks-only span and the anchor the gap was
/// measured from is the previous run itself.
///
/// This runs before the ligature fold, which reads the assembled text: the
/// previous run still ends in the ligature code point here.
fn space_drawn_over_previous_run(prev: &TextSpan, curr: &TextSpan, gap: f64) -> bool {
    if gap >= -0.1 * prev.font_size {
        return false;
    }
    let mut next = curr.text.chars();
    if next.next() != Some(' ') || !next.next().is_some_and(|c| c.is_lowercase()) {
        return false;
    }
    prev.text
        .chars()
        .last()
        .is_some_and(|c| c.is_alphabetic() || ('\u{FB00}'..='\u{FB06}').contains(&c))
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
// THE GUTTER IS MEASURED AGAINST THE TYPE, NOT AGAINST THE PAGE. There was a
// third rail here -- a vertical gap had to reach 4% of the region's width --
// and on the page it was written for it is the only rail that ever binds. A
// two-column paper is set in 9pt type inside a 500pt text block, so 4% asks for
// 20pt of white where the typographer left 13pt: wider than the gutter of every
// journal that ever printed two columns, which is what "narrow-gutter academic
// layouts are still detected" in the comment below was meant to allow and did
// not. It refused the cut on tracemonkey's pages 2 and 13 and on the same pages
// of highlights and tracemonkey_with_editable_annotations -- six of the eleven
// pages that were left under 95% on the gate set (#389).
//
// What separates a gutter from a word space is the type, and that is what the
// two remaining rails say: the median font size (a word space is a quarter of
// an em, a gutter is more than a whole one) and a 10pt absolute floor. A page
// four times as wide does not get four times the word spaces, so scaling the
// question with the page was never measuring the thing it asked about.
/// Floor (in pt) for vertical gap regardless of region width. Matches
/// the previous `COLUMN_GAP_THRESHOLD_MIN` and keeps XY-Cut conservative
/// on narrow regions (sidebars, tall columns).
const XY_CUT_VERTICAL_GAP_FLOOR: f64 = 10.0;
/// Multiplier applied to median font size to produce the horizontal-gap
/// threshold. 1.8 × line-height matches typical paragraph spacing.
const XY_CUT_HORIZONTAL_GAP_FONT_MULTIPLIER: f64 = 1.8;

/// Whether the horizontal cut at `cut_y` falls INSIDE one grid (#596).
///
/// The defect this refuses is the largest single item on the tables axis, and it
/// is not in `tables.rs` at all. A table whose rows are grouped under labels --
/// `Study Design`, `Gender studied`, `Region` -- leaves a little more white
/// between the groups than between the rows of a group, and
/// `try_horizontal_cut`'s threshold is a multiple of the ROW pitch. On
/// `PMC3037919-p0007` the rows sit 12.0 points apart and the groups 23.8, against
/// a threshold of 12 * 1.8 = 21.6: so the cut fires between every group, each
/// group becomes its own region, and `blocks::paragraphs` -- whose stretches are
/// bounded per region -- hands `tables::find` one group at a time. The page comes
/// back as twelve tables where the document holds one, and `find` is blameless:
/// it is called twelve times with between one and six rows and does the right
/// thing with each of them. Measured over both sets, that shape is 34 pages of
/// DocLayNet and 29 of PubTables-1M, and 0.3378 of the tables axis's 0.5442.
///
/// WHAT SAYS "ONE GRID" IS THE COLUMN EDGES, and the measurement already exists:
/// `TextBand::gap_midpoints` gives the x where a band's runs are separated, and
/// `boundaries_match` asks whether two bands are separated at the same places.
/// `region_is_a_grid` has asked exactly this question since #429 -- and is
/// reachable only from `group_spans_into_blocks_legacy_from_bands`, the path whose
/// own comment says XY-Cut supersedes it. The vertical cut carries two guards of
/// this kind (`region_is_ruled` and `the_cut_is_a_column_of_a_table`); the
/// horizontal cut carried none, so a guard that would have refused the dominant
/// table defect was left on the dead side of a replacement.
///
/// BOTH SIDES, and at least two bands each. One band above a cut agreeing with
/// one below is two lines of prose that happen to break at the same place; four
/// bands agreeing across the cut is a grid. The bands are taken adjacent to the
/// cut rather than over the whole region, because a table's own caption and the
/// prose above it break nowhere and would refuse every table.
fn the_cut_is_inside_a_grid(bands: &[TextBand], cut_y: f64, gap: f64) -> bool {
    let threshold = compute_adaptive_column_gap(bands);
    // `TextBand` has no y accessor of its own; a band's y is its spans' and they
    // are within the band tolerance of each other, so the maximum will do.
    let band_y = |band: &TextBand| {
        band.spans
            .iter()
            .map(|span| span.y)
            .fold(f64::NEG_INFINITY, f64::max)
    };
    let mut above: Vec<&TextBand> = bands.iter().filter(|band| band_y(band) > cut_y).collect();
    let mut below: Vec<&TextBand> = bands.iter().filter(|band| band_y(band) <= cut_y).collect();
    // Nearest the cut first on each side: ascending y above it, descending below.
    above.sort_by(|a, b| band_y(a).partial_cmp(&band_y(b)).unwrap_or(Ordering::Equal));
    below.sort_by(|a, b| band_y(b).partial_cmp(&band_y(a)).unwrap_or(Ordering::Equal));
    if above.len() < GRID_SIDE_ROWS || below.len() < GRID_SIDE_ROWS {
        return false;
    }
    // The bands touching the cut, nearest first on each side.
    let touching: Vec<&TextBand> = above
        .iter()
        .take(GRID_SIDE_ROWS)
        .chain(below.iter().take(GRID_SIDE_ROWS))
        .copied()
        .collect();
    // AND THE GAP HAS TO BE A GROUP'S GAP AND NOT A ZONE'S. Refusing every cut
    // whose two sides share column edges refuses the cut that takes the prose off
    // the top of the table as well -- and then the region handed to
    // `tables::find` opens with more prose than its `MAX_TRIMMED_ROWS` will trim,
    // so the grid is refused outright and the page comes back with NO table.
    // Measured: `PMC1277013-p0003` went from six tables to none that way, which is
    // worse than the six. The white between the label groups of one table is a
    // small multiple of its row pitch; the white above a table is not.
    let mut steps: Vec<f64> = touching
        .windows(2)
        .map(|pair| (band_y(pair[0]) - band_y(pair[1])).abs())
        .filter(|step| *step > 0.0)
        .collect();
    steps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let pitch = match steps.first() {
        Some(pitch) => *pitch,
        None => return false,
    };
    if gap > GRID_GROUP_GAP_PITCHES * pitch {
        return false;
    }

    // A ROW WITH AN EMPTY CELL IS STILL A ROW OF THE GRID, and this is the whole
    // difficulty. `boundaries_match` asks for the same NUMBER of separations on
    // both bands, which is right for two lines of a column cut and wrong here: the
    // rows that make a table fragment are exactly the ones whose label cell is
    // empty, so they hold one gap fewer than the row above them. A guard built on
    // equal counts would refuse the grid precisely where the grid has the empty
    // cells that caused the split -- the same way round as a detector I wrote for
    // the interleave class, which could not fire on the pages it was for.
    //
    // So the widest band sets the edges and every other band has to agree with a
    // SUBSET of them: each of its separations falls on one of the edges, and no
    // separation of its own sits where the widest band has ink.
    let reference = touching
        .iter()
        .map(|band| band.gap_midpoints(threshold))
        .max_by_key(Vec::len)
        .unwrap_or_default();
    if reference.len() < GRID_MIN_BOUNDARIES {
        return false;
    }
    let tolerance = (threshold * 1.5).clamp(COLUMN_GAP_MATCH_TOLERANCE, 60.0);
    // A BAND WITH ONE RUN OF INK IS A ROW OF THE TABLE TOO, and refusing on it
    // would refuse exactly the tables this guard is for: the section heads set
    // across a grid -- `Region`, `Health status` -- are what the label groups are
    // divided by, so they touch almost every cut worth refusing. `tables::find`
    // already reads such a band as a row of the table when the grid goes on under
    // it (#576); this is the same judgement one layer up. One is allowed and not
    // two, because two in a row is prose between two tables rather than a head
    // inside one.
    let mut unseparated = 0usize;
    for band in &touching {
        let midpoints = band.gap_midpoints(threshold);
        if midpoints.is_empty() {
            unseparated += 1;
            if unseparated > GRID_MAX_UNSEPARATED {
                return false;
            }
            continue;
        }
        if !midpoints
            .iter()
            .all(|mid| reference.iter().any(|edge| (edge - mid).abs() <= tolerance))
        {
            return false;
        }
    }
    true
}

/// How many bands on EACH side of a horizontal cut must agree on the column
/// edges before the cut is inside one grid (#596). Two: one band against one is
/// a coincidence two lines of prose can produce.
const GRID_SIDE_ROWS: usize = 2;
/// How many row pitches of white a gap may hold and still be a gap BETWEEN THE
/// GROUPS of one table rather than the white above the table (#596).
const GRID_GROUP_GAP_PITCHES: f64 = 5.0;
/// How many column separations the widest band across a cut must show before the
/// bands can be one grid (#596). Two, so three columns: a single separation is
/// what every line of prose with one wide word space has.
const GRID_MIN_BOUNDARIES: usize = 2;
/// How many of the bands touching a cut may hold a single run of ink and still
/// leave the rest a grid (#596): a section head set across the table.
const GRID_MAX_UNSEPARATED: usize = 1;

/// A table's extent on the page, found before the XY-cut runs (#574).
#[derive(Debug, Clone, Copy, PartialEq)]
struct TableRegion {
    left: f64,
    right: f64,
    /// The highest baseline of the region's first row.
    top: f64,
    /// The lowest baseline of its last row.
    bottom: f64,
    /// Whether the region was found by something stronger than aligned text --
    /// rules above and below, two columns of keys, a leader -- so that a column
    /// cut through it is the cut dividing a table and not a page.
    ruled: bool,
    /// How many rows the detector counted in it.
    rows: usize,
}

impl TableRegion {
    fn holds(&self, span: &TextSpan) -> bool {
        let centre = span.x + (span.right() - span.x) * 0.5;
        span.y <= self.top && span.y >= self.bottom && centre >= self.left && centre <= self.right
    }
}

/// One run of ink inside a band: its left and right edge and how many
/// characters it holds.
#[derive(Debug, Clone)]
struct InkRun {
    left: f64,
    right: f64,
    chars: usize,
    /// Whether the run opens with a bullet glyph and goes on in text.
    bulleted: bool,
    /// The run's text, its spans joined by a space.
    text: String,
}

/// Whether a span is nothing but a dot leader's dots: the filler a table of
/// contents or a statement runs from a label out to its figure. It is white to
/// the eye that reads the row, and to the table found in it (#574).
fn is_leader(span: &TextSpan) -> bool {
    let text = span.text.trim();
    !text.is_empty()
        && text
            .chars()
            .all(|c| matches!(c, '.' | '\u{00B7}' | '\u{2026}' | ' '))
}

/// Whether a run's first span opens with a bullet -- one that is not also a
/// dash, since a dash alone is the nil of a financial table and a minus opens a
/// figure. The bullet is often a span of its own, so whether text follows it is
/// asked of the whole run (`chars`).
fn opens_with_bullet(text: &str) -> bool {
    matches!(text.chars().find(|c| !c.is_whitespace()),
        Some(c) if crate::lists::BULLET_CHARS.contains(&c)
            && !matches!(c, '-' | '\u{2013}' | '\u{2014}'))
}

fn ink_runs(band: &TextBand, gap: f64) -> Vec<InkRun> {
    let mut runs: Vec<InkRun> = Vec::new();
    for span in &band.spans {
        let chars = span.text.chars().filter(|c| !c.is_whitespace()).count();
        if chars == 0 || is_leader(span) {
            continue;
        }
        // The glyphs' own advance where the page gave one: the estimate is
        // deliberately wide, and a long label measured by it runs into the
        // value column it stops well short of.
        let right = if span.width > 0.0 {
            span.measured_right()
        } else {
            span.right()
        };
        match runs.last_mut() {
            Some(run) if span.x - run.right < gap => {
                run.right = run.right.max(right);
                run.chars += chars;
                run.text.push(' ');
                run.text.push_str(span.text.trim());
            }
            _ => runs.push(InkRun {
                left: span.x,
                right,
                chars,
                bulleted: opens_with_bullet(&span.text),
                text: span.text.trim().to_string(),
            }),
        }
    }
    runs
}

/// The white strips between a band's runs, left edge and right edge.
fn run_gaps(runs: &[InkRun]) -> Vec<(f64, f64)> {
    runs.windows(2)
        .map(|pair| (pair[0].right, pair[1].left))
        .collect()
}

/// Whether a row repeats the column heads of the row that opened the table:
/// the same text in every value column, three columns or more.
fn heads_again(first: &[InkRun], here: &[InkRun]) -> bool {
    first.len() >= 3
        && here.len() == first.len()
        && first
            .iter()
            .zip(here)
            .skip(1)
            .all(|(a, b)| a.text == b.text && a.text.chars().any(char::is_alphabetic))
}

/// Which of the table's strips this gap falls in, if any. A strip is the white
/// every row so far leaves between two columns, so a gap belongs to it when the
/// two overlap: right-aligned figures move their left edges row by row and a
/// long label moves its right edge, and neither moves the strip.
fn strip_of(strips: &[(f64, f64)], gap: (f64, f64), slack: f64) -> Option<usize> {
    strips
        .iter()
        .position(|strip| gap.1.min(strip.1) - gap.0.max(strip.0) > -slack)
}

/// The tables of a page, found on the WHOLE page before the XY-cut (#574).
///
/// The XY-cut decides the regions and `tables::find` only ever sees one region
/// at a time, so a table the cut runs through comes back as several -- or as a
/// table, a few loose paragraphs and another table. That is the largest single
/// item on the DocLayNet tables axis: the annotated table drawn as several
/// (33 of 64 pages of the round of 24-09, 58 % of the gap), and of the text the
/// annotation puts in a table a third goes to prose (T4, 27-09). Grouping the
/// cells we already produce by the annotation's own region takes detection from
/// 0.300 to 0.849 on DocLayNet's train split (T2-577), so the cells are right
/// and the region is the defect.
///
/// So the region is decided first, from the page's own bands. A ROW is a band
/// that breaks into three runs or more at a gap of two ems, at least two of them
/// short (a value, a year, a `$`). A TABLE is a stack of at least
/// `TABLE_REGION_MIN_ROWS` such rows that break at the SAME places, with room
/// between them for what a financial statement puts there: a group label in the
/// stub column (`Leasehold buildings and improvements`), a row with one value,
/// and the white between groups -- up to `TABLE_REGION_MAX_STEP_PITCHES` of the
/// page's line pitch. The cut then runs round each region and never through it.
fn table_regions(
    spans: &[TextSpan],
    stats: &PageStats,
    rules: &[HorizontalRule],
) -> Vec<TableRegion> {
    table_regions_seen(spans, stats, rules, None)
}

/// [`table_regions`], and when `seen` is given, every region each detector
/// accepted before one gave way to another -- the proposals a judge chooses
/// between (#574, T4's judge).
fn table_regions_seen(
    spans: &[TextSpan],
    stats: &PageStats,
    rules: &[HorizontalRule],
    seen: Option<&mut Vec<(TableRegion, &'static str)>>,
) -> Vec<TableRegion> {
    if spans.len() < TABLE_REGION_MIN_ROWS * 2 {
        return Vec::new();
    }
    let bands = group_spans_into_bands_with_stats(spans.to_vec(), stats);
    let em = stats.median_font_size.max(1.0);
    let gap = TABLE_REGION_GAP_EM * em;
    let tolerance = TABLE_REGION_EDGE_EM * em;
    // Never under a line of the body type: a page with superscripts or a
    // footnote block measures a tighter spacing than any of its rows keep,
    // and every step between two rows would then read as a break.
    let pitch = stats.median_line_spacing.max(em * 1.2);
    let runs: Vec<Vec<InkRun>> = bands.iter().map(|band| ink_runs(band, gap)).collect();
    // A key and its description may stand closer than two cells of a grid:
    // the second column of a two-column table starts where the longest key
    // ends, and the white between them can be a single em.
    let pair_runs: Vec<Vec<InkRun>> = bands
        .iter()
        .map(|band| ink_runs(band, TABLE_REGION_PAIR_GAP_EM * em))
        .collect();
    // Columns of bulleted items side by side line up like a table and are
    // lists (a brochure's `• Power Plants  • Marine Propulsion  • Oil and Gas`).
    let is_row = |runs: &[InkRun]| {
        runs.len() >= 3
            && runs
                .iter()
                .filter(|run| run.bulleted && run.chars > 1)
                .count()
                < 2
            && runs
                .iter()
                .filter(|run| run.chars <= TABLE_REGION_SHORT_CELL)
                .count()
                >= 2
            && runs
                .iter()
                .skip(1)
                .all(|run| run.chars <= TABLE_REGION_LONG_CELL)
    };

    // Asked for what the detectors considered, the loop keeps the stacks of
    // two rows as well; they are proposals the judge may rescue, and they are
    // left out of what the page itself uses below.
    let min_rows = if seen.is_some() {
        2
    } else {
        TABLE_REGION_MIN_ROWS
    };
    let mut regions = Vec::new();
    let mut i = 0;
    while i < bands.len() {
        if !is_row(&runs[i]) {
            i += 1;
            continue;
        }
        let mut strips = run_gaps(&runs[i]);
        // Where the first value column starts: a label may run up to it.
        let mut values_left = runs[i]
            .iter()
            .skip(1)
            .map(|run| run.left)
            .fold(f64::INFINITY, f64::min);
        let mut last_row = i;
        let mut rows = 1;
        let mut loose = 0;
        let mut above = bands[i].y;
        let mut j = i + 1;
        while j < bands.len() {
            let here = &runs[j];
            // A band of nothing but blank glyphs is white, not a row.
            if here.is_empty() {
                j += 1;
                continue;
            }
            if above - bands[j].y > TABLE_REGION_MAX_STEP_PITCHES * pitch {
                break;
            }
            above = bands[j].y;
            let gaps = run_gaps(here);
            let found: Vec<Option<usize>> = gaps
                .iter()
                .map(|gap| strip_of(&strips, *gap, tolerance))
                .collect();
            // THE HEAD AGAIN IS A SECOND TABLE: a page that sets two years'
            // statements one under the other repeats the column heads over
            // the second, and the annotation draws two tables there.
            if heads_again(&runs[i], here) {
                break;
            }
            // A CAPTION IS WHERE THE NEXT TABLE STARTS, however short and
            // wherever it stands: `Table 2 The same by subtype` between two
            // banks is two tables.
            let line: Vec<&str> = here.iter().map(|run| run.text.as_str()).collect();
            if crate::tables::announces_a_table(&line.join(" ")) {
                break;
            }
            if is_row(here) {
                // Most of this row's gaps must fall in strips the table already
                // has; a row with one gap more (a column the header did not
                // fill) adds it.
                let known = found.iter().flatten().count();
                if known < 2.min(strips.len()) || known * 2 < gaps.len() {
                    break;
                }
                for (gap, at) in gaps.iter().zip(&found) {
                    match at {
                        Some(k) => {
                            let (lo, hi) = (strips[*k].0.max(gap.0), strips[*k].1.min(gap.1));
                            if hi - lo >= TABLE_REGION_MIN_STRIP {
                                strips[*k] = (lo, hi);
                            }
                        }
                        None => strips.push(*gap),
                    }
                }
                values_left = here
                    .iter()
                    .skip(1)
                    .map(|run| run.left)
                    .fold(values_left, f64::min);
                last_row = j;
                rows += 1;
                loose = 0;
            } else {
                // A label in the stub column, or a row with fewer cells filled
                // whose breaks fall on the table's own edges.
                let first_edge = strips.iter().map(|s| s.1).fold(f64::INFINITY, f64::min);
                // A label stops short of the figures; a section head set
                // across the table may run over them, and is short.
                let label = here.len() == 1
                    && here[0].left < first_edge
                    && (here[0].right < values_left - tolerance
                        || here[0].chars <= TABLE_REGION_LONG_CELL);
                let sparse = here.len() == 2 && found.iter().all(Option::is_some);
                // A cell's second line: one run that closes none of the strips
                // -- it may be longer than the line above it, but it does not
                // reach across into the next column.
                let wrapped = here.len() == 1
                    && strips
                        .iter()
                        .all(|strip| here[0].left > strip.0 || here[0].right < strip.1);
                if !(label || sparse || wrapped) {
                    break;
                }
                // A row with a cell left empty is a row of the table even as
                // its last line; a label is only a row of it with a row under it.
                if sparse {
                    last_row = j;
                }
                loose += 1;
                if loose > TABLE_REGION_MAX_LOOSE {
                    break;
                }
            }
            j += 1;
        }
        // THE HEAD OF THE TABLE, above its first row: a line of years or units
        // set over the value columns with nothing in the stub, or a head that
        // spans two of them, is not a row by the test above, and it is the
        // first thing the annotation's box holds.
        let mut first_row = i;
        let mut k = i;
        let mut heads = 0;
        while k > 0 && heads < TABLE_REGION_MAX_HEADS {
            k -= 1;
            let here = &runs[k];
            if here.is_empty() {
                continue;
            }
            if bands[k].y - bands[first_row].y > TABLE_REGION_MAX_STEP_PITCHES * pitch {
                break;
            }
            // Over the value columns only, or a row of heads whose breaks fall
            // in the table's strips -- a head spanning two columns among them.
            let over_values = here.iter().all(|run| run.left >= values_left - tolerance);
            let gaps = run_gaps(here);
            if (!over_values && gaps.is_empty())
                || !gaps
                    .iter()
                    .all(|gap| strip_of(&strips, *gap, tolerance).is_some())
            {
                break;
            }
            first_row = k;
            heads += 1;
        }
        if rows >= min_rows && strips.len() >= 2 {
            let chosen = &bands[first_row..=last_row];
            let mut left = chosen
                .iter()
                .map(TextBand::left)
                .fold(f64::INFINITY, f64::min);
            // A STUB THAT IS A COLUMN OF PROSE is the page's other column: on a
            // paper set in two columns a table's rows share their bands with
            // the lines of the paragraph beside it, and each such line reads as
            // the row's first cell. Labels are ragged on the right; justified
            // prose is not. Where most rows open with a long run and those runs
            // end flush, the region starts where the rows' second run does.
            if let Some(cut) = prose_stub_edge(&runs[first_row..=last_row], em) {
                left = cut;
            }
            let right = chosen
                .iter()
                .map(TextBand::right)
                .fold(f64::NEG_INFINITY, f64::max);
            let top = bands[first_row]
                .spans
                .iter()
                .map(|span| span.y)
                .fold(f64::NEG_INFINITY, f64::max);
            let bottom = bands[last_row]
                .spans
                .iter()
                .map(|span| span.y)
                .fold(f64::INFINITY, f64::min);
            regions.push(TableRegion {
                left,
                right,
                top,
                bottom,
                ruled: false,
                rows,
            });
        }
        i = last_row + 1;
    }
    if let Some(seen) = seen {
        seen.extend(regions.iter().map(|r| (*r, "aligned")));
        regions.retain(|r| r.rows >= TABLE_REGION_MIN_ROWS);
        seen.extend(
            booktabs_regions(&bands, &runs, rules, pitch, em)
                .into_iter()
                .map(|r| (r, "booktabs")),
        );
        seen.extend(
            pair_regions(&bands, &pair_runs, pitch, tolerance, &[], 2)
                .into_iter()
                .map(|r| (r, "pair")),
        );
        seen.extend(
            leader_regions(&bands, &runs, pitch, &[], 2)
                .into_iter()
                .map(|r| (r, "leader")),
        );
    }
    // A table the document ruled above and below is the table the rules
    // enclose, head and all: what the alignment found inside it gives way.
    for ruled in booktabs_regions(&bands, &runs, rules, pitch, em) {
        regions.retain(|r| !(r.top <= ruled.top + em && r.bottom >= ruled.bottom - em));
        if !regions
            .iter()
            .any(|r| r.bottom <= ruled.top && r.top >= ruled.bottom)
        {
            regions.push(ruled);
        }
    }
    let taken: Vec<(f64, f64)> = regions.iter().map(|r| (r.bottom, r.top)).collect();
    regions.extend(pair_regions(
        &bands,
        &pair_runs,
        pitch,
        tolerance,
        &taken,
        TABLE_REGION_MIN_ROWS,
    ));
    let taken: Vec<(f64, f64)> = regions.iter().map(|r| (r.bottom, r.top)).collect();
    regions.extend(leader_regions(
        &bands,
        &runs,
        pitch,
        &taken,
        TABLE_REGION_MIN_ROWS,
    ));
    regions
}

/// Tables the document drew a rule above and below: the booktabs style of a
/// paper or a report, with no vertical lines and no rules between the rows
/// (#574, the idea from LiteParse's detector, #632).
///
/// A top rule and a rule below it of the same extent, at least
/// `TABLE_REGION_MIN_ROWS` bands between them all inside that extent, at least
/// two of them broken into cells, and no more white between two of them than a
/// table holds. The rule farthest down that still encloses such a stack is the
/// foot, so a mid-rule under the head is inside the table and not its end.
fn booktabs_regions(
    bands: &[TextBand],
    runs: &[Vec<InkRun>],
    rules: &[HorizontalRule],
    pitch: f64,
    em: f64,
) -> Vec<TableRegion> {
    // A GRID DRAWS ITS RULES CELL BY CELL: the rule under a row is often one
    // segment per cell, end to end, and no one of them spans the table. They are
    // one rule for this purpose, so segments at one height that meet are joined
    // first (EN tender annexes on DocLayNet: four segments to every rule).
    let mut joined: Vec<HorizontalRule> = rules.to_vec();
    joined.sort_by(|a, b| {
        b.y.partial_cmp(&a.y)
            .unwrap_or(Ordering::Equal)
            .then(a.left.partial_cmp(&b.left).unwrap_or(Ordering::Equal))
    });
    let mut merged: Vec<HorizontalRule> = Vec::with_capacity(joined.len());
    for rule in joined {
        match merged.last_mut() {
            Some(last)
                if (last.y - rule.y).abs() <= RULE_JOIN_TOLERANCE
                    && rule.left - last.right <= RULE_JOIN_TOLERANCE =>
            {
                last.right = last.right.max(rule.right);
            }
            _ => merged.push(rule),
        }
    }
    let mut rules: Vec<&HorizontalRule> = merged
        .iter()
        .filter(|rule| rule.right - rule.left >= TABLE_REGION_RULE_MIN_EM * em)
        .collect();
    rules.sort_by(|a, b| b.y.partial_cmp(&a.y).unwrap_or(Ordering::Equal));
    let same_extent = |a: &HorizontalRule, b: &HorizontalRule| {
        let overlap = a.right.min(b.right) - a.left.max(b.left);
        let narrower = (a.right - a.left).min(b.right - b.left);
        overlap >= 0.9 * narrower && narrower >= 0.8 * (a.right - a.left).max(b.right - b.left)
    };
    let mut regions: Vec<TableRegion> = Vec::new();
    let mut t = 0;
    while t < rules.len() {
        let top = rules[t];
        let mut found: Option<(usize, TableRegion)> = None;
        for (f, foot) in rules.iter().enumerate().skip(t + 1) {
            if !same_extent(top, foot) {
                continue;
            }
            let inside: Vec<usize> = (0..bands.len())
                .filter(|&k| bands[k].y < top.y && bands[k].y > foot.y && !runs[k].is_empty())
                .collect();
            // The ink's own extent, from the runs: a band's `right()` is the
            // estimate's, deliberately wide, and a long line of a description
            // column measured by it runs out past the rule it stops short of.
            // A mark in the margin -- a manual's change bar `|`, a footnote
            // dagger -- stands outside the rules and is no ink of the table's:
            // runs of at most two characters are not asked.
            let within = |k: usize| {
                runs[k]
                    .iter()
                    .filter(|r| r.chars > TABLE_REGION_MARGIN_MARK)
                    .all(|r| r.left >= top.left - em && r.right <= top.right + em)
            };
            let contained = inside.iter().all(|&k| within(k));
            let celled = inside.iter().filter(|&&k| runs[k].len() >= 2).count();
            let tight = inside
                .windows(2)
                .all(|w| bands[w[0]].y - bands[w[1]].y <= TABLE_REGION_MAX_STEP_PITCHES * pitch);
            if !contained || !tight {
                break;
            }
            if inside.len() >= TABLE_REGION_MIN_ROWS && celled >= 2 {
                let top_y = inside
                    .first()
                    .map(|&k| {
                        bands[k]
                            .spans
                            .iter()
                            .map(|s| s.y)
                            .fold(f64::NEG_INFINITY, f64::max)
                    })
                    .unwrap_or(top.y);
                // A table ruled ABOVE each row has its last row under the last
                // rule: rows directly below it, in the extent and broken into
                // cells, are still the table's.
                let mut last = *inside.last().unwrap_or(&0);
                for k in last + 1..bands.len() {
                    if runs[k].is_empty() {
                        continue;
                    }
                    let in_extent = within(k);
                    if runs[k].len() < 2
                        || !in_extent
                        || bands[last].y - bands[k].y > TABLE_REGION_PAIR_STEP_PITCHES * pitch
                        || foot.y - bands[k].y > TABLE_REGION_PAIR_STEP_PITCHES * pitch
                    {
                        break;
                    }
                    last = k;
                }
                let bottom_y = bands[last]
                    .spans
                    .iter()
                    .map(|s| s.y)
                    .fold(f64::INFINITY, f64::min);
                found = Some((
                    f,
                    TableRegion {
                        left: top.left.min(foot.left),
                        right: top.right.max(foot.right),
                        top: top_y,
                        bottom: bottom_y,
                        ruled: true,
                        rows: inside.len(),
                    },
                ));
            }
        }
        match found {
            Some((f, region)) => {
                regions.push(region);
                t = f + 1;
            }
            None => t += 1,
        }
    }
    regions
}

/// Where a region's rows start when their first runs are a column of justified
/// prose rather than labels: just left of the leftmost second run. `None` when
/// the first runs are ragged, short, or too few (#574).
fn prose_stub_edge(rows: &[Vec<InkRun>], em: f64) -> Option<f64> {
    let firsts: Vec<&InkRun> = rows
        .iter()
        .filter(|r| r.len() >= 2)
        .map(|r| &r[0])
        .collect();
    if firsts.len() < TABLE_REGION_MIN_ROWS {
        return None;
    }
    // The long runs, since a paragraph's last line and a short one of a new
    // paragraph are neither long nor flush; half the rows at least.
    let long: Vec<&&InkRun> = firsts
        .iter()
        .filter(|r| r.chars > TABLE_REGION_LONG_CELL)
        .collect();
    if long.len() * 2 < firsts.len() {
        return None;
    }
    let mut rights: Vec<f64> = long.iter().map(|r| r.right).collect();
    rights.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let median = rights[rights.len() / 2];
    let flush = rights
        .iter()
        .filter(|r| (*r - median).abs() <= TABLE_REGION_JUSTIFIED_EM * em)
        .count();
    if flush * 3 < rights.len() * 2 {
        return None;
    }
    rows.iter()
        .filter(|r| r.len() >= 2)
        .map(|r| r[1].left)
        .min_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal))
        .map(|x| x - 0.5 * em)
}

/// How far apart, in ems, the right edges of a column's lines may be and still
/// be one justified edge.
const TABLE_REGION_JUSTIFIED_EM: f64 = 0.5;

/// Tables of two columns: a short key and what it stands for (#574).
///
/// `Option | Function`, `F3 | Exit`, `11 | Work with PFD definitions` -- a
/// manual sets its menus and its function keys this way, and DocLayNet calls
/// every one a table. A row of two runs is also what a page set in two columns
/// is, which is why `table_regions` asks for three; here the two columns are
/// told from a page's by what they hold and how they stand: the first cell is
/// short and no list marker, the second is no bullet, and both start at the
/// same place on at least `TABLE_REGION_MIN_ROWS` rows running. The second
/// cell may wrap onto a line of its own under itself.
fn pair_regions(
    bands: &[TextBand],
    runs: &[Vec<InkRun>],
    pitch: f64,
    tolerance: f64,
    taken: &[(f64, f64)],
    min_rows: usize,
) -> Vec<TableRegion> {
    let free = |y: f64| !taken.iter().any(|(bottom, top)| y >= *bottom && y <= *top);
    let is_pair = |runs: &[InkRun]| {
        runs.len() == 2
            && runs[0].chars <= TABLE_REGION_SHORT_CELL
            && !runs[0].bulleted
            && !runs[1].bulleted
            && !is_list_marker(&runs[0].text)
    };
    let mut regions = Vec::new();
    let mut i = 0;
    while i < bands.len() {
        if !is_pair(&runs[i]) || !free(bands[i].y) {
            i += 1;
            continue;
        }
        let (key, value) = (runs[i][0].left, runs[i][1].left);
        let (mut last_row, mut rows, mut loose) = (i, 1, 0);
        let mut above = bands[i].y;
        let mut j = i + 1;
        while j < bands.len() {
            let here = &runs[j];
            if here.is_empty() {
                j += 1;
                continue;
            }
            if above - bands[j].y > TABLE_REGION_PAIR_STEP_PITCHES * pitch || !free(bands[j].y) {
                break;
            }
            above = bands[j].y;
            if is_pair(here)
                && (here[0].left - key).abs() <= tolerance
                && (here[1].left - value).abs() <= tolerance
            {
                last_row = j;
                rows += 1;
                loose = 0;
            } else if here.len() == 1 && (here[0].left - value).abs() <= tolerance {
                loose += 1;
                if loose > TABLE_REGION_MAX_LOOSE {
                    break;
                }
            } else {
                break;
            }
            j += 1;
        }
        if rows >= min_rows {
            let chosen = &bands[i..=last_row];
            regions.push(TableRegion {
                left: chosen
                    .iter()
                    .map(TextBand::left)
                    .fold(f64::INFINITY, f64::min),
                right: chosen
                    .iter()
                    .map(TextBand::right)
                    .fold(f64::NEG_INFINITY, f64::max),
                top: bands[i]
                    .spans
                    .iter()
                    .map(|span| span.y)
                    .fold(f64::NEG_INFINITY, f64::max),
                bottom: bands[last_row]
                    .spans
                    .iter()
                    .map(|span| span.y)
                    .fold(f64::INFINITY, f64::min),
                ruled: true,
                rows,
            });
        }
        i = last_row + 1;
    }
    regions
}

/// A span that holds its own leader -- `Characters per line ........ 10` drawn in
/// one string -- as the label with its dots and, apart, the figure after them
/// (#574). The label's box stops where its words do, so the white the dots
/// stand in is a gutter to `table_regions` and `tables::find` alike; the dots
/// stay in the label's text, since the reader keeps them. A span with no leader
/// of at least `TABLE_REGION_LEADER_DOTS` dots, or with more than a short
/// figure after it, comes back as it went in.
fn split_at_leader(span: TextSpan) -> Vec<TextSpan> {
    let chars: Vec<char> = span.text.chars().collect();
    // The last run of at least `TABLE_REGION_LEADER_DOTS` dots, single spaces
    // allowed between them: the figure after it may hold a point of its own.
    let (mut start, mut end, mut dots) = (0usize, 0usize, 0usize);
    let mut k = 0;
    while k < chars.len() {
        if chars[k] != '.' {
            k += 1;
            continue;
        }
        let (from, mut count, mut to) = (k, 0usize, k);
        while to < chars.len()
            && (chars[to] == '.'
                || (chars[to] == ' ' && to + 1 < chars.len() && chars[to + 1] == '.'))
        {
            if chars[to] == '.' {
                count += 1;
            }
            to += 1;
        }
        if count >= TABLE_REGION_LEADER_DOTS {
            (start, end, dots) = (from, to, count);
        }
        k = to.max(k + 1);
    }
    let label: String = chars[..start].iter().collect();
    let tail: String = chars[end..].iter().collect::<String>().trim().to_string();
    if dots < TABLE_REGION_LEADER_DOTS
        || label.trim().is_empty()
        || tail.chars().count() > TABLE_REGION_SHORT_CELL
    {
        return vec![span];
    }
    let n = chars.len().max(1) as f64;
    let exact = span.char_bounds.len() == chars.len();
    let at = |i: usize, right: bool| -> f64 {
        if exact {
            span.char_bounds[i][if right { 2 } else { 0 }]
        } else {
            span.x + span.width * ((i + usize::from(right)) as f64 / n)
        }
    };
    let label_end = label.trim_end().chars().count().max(1) - 1;
    let label_right = at(label_end, true);
    let mut out = Vec::with_capacity(2);
    let mut head = span.clone();
    head.text = chars[..end].iter().collect();
    head.width = (label_right - span.x).max(0.0);
    head.char_bounds.clear();
    head.tight_char_bounds.clear();
    head.glyph_advances.clear();
    head.glyph_bounds_sources.clear();
    out.push(head);
    if !tail.is_empty() {
        let first = chars[end..]
            .iter()
            .position(|c| !c.is_whitespace())
            .map_or(end, |k| end + k);
        let mut figure = span.clone();
        figure.text = tail;
        figure.x = at(first, false);
        figure.width = (span.x + span.width - figure.x).max(0.0);
        figure.char_bounds.clear();
        figure.tight_char_bounds.clear();
        figure.glyph_advances.clear();
        figure.glyph_bounds_sources.clear();
        out.push(figure);
    }
    out
}

/// Each leader span's dots appended to the text of the span before it, and the
/// leader span removed; `spans` is in reading order along the band.
///
/// The dots' glyphs ride along: their boxes are appended to the label's, so a
/// word the fold glued keeps one box per glyph. Dropped, a folded dot left its
/// word boxless and the word fell back to the label's whole box -- a decimal
/// dot read on top of its digit, and no join rule could put `1.1.1` together
/// again.
///
/// Where the dots abut the label on its own line -- a decimal dot, not a
/// leader run across white -- they join it as text and box: `2` and `.` read
/// `2.`, and the label's box grows to cover them, because the box claims the
/// text and a gap measured past a stale edge reads white the ink fills. Dots
/// standing off across white keep their space and leave the box alone, so
/// the gutter past them still reads as one.
///
/// The abutting join is on both paths (#655): a decimal dot is a decimal
/// dot whatever the option says. What `cell_path_words` still gates is the
/// standoff fold: with it the dots' boxes ride along, without it the dots
/// join with a space and leave the glyph boxes behind, exactly as before.
fn fold_leaders(spans: &mut Vec<TextSpan>, cell_path_words: bool) {
    let mut kept: Vec<TextSpan> = Vec::with_capacity(spans.len());
    for span in spans.drain(..) {
        match kept.last_mut() {
            Some(label) if is_leader(&span) => {
                // Abutting on the label's own line: no daylight between the
                // label's right edge and the dots' left, and their heights
                // overlap. Anything past a hair is a leader across white.
                const ABUT_EPS: f64 = 1e-6;
                let dots_left = span
                    .char_bounds
                    .iter()
                    .map(|b| b[0])
                    .fold(f64::INFINITY, f64::min);
                let dots_right = span
                    .char_bounds
                    .iter()
                    .map(|b| b[2])
                    .fold(f64::NEG_INFINITY, f64::max);
                let dots_top = span
                    .char_bounds
                    .iter()
                    .map(|b| b[3])
                    .fold(f64::NEG_INFINITY, f64::max);
                let dots_bottom = span
                    .char_bounds
                    .iter()
                    .map(|b| b[1])
                    .fold(f64::INFINITY, f64::min);
                let abuts = dots_left <= label.x + label.width + ABUT_EPS
                    && dots_top > label.y
                    && dots_bottom < label.y + label.height;
                if abuts {
                    label.text.push_str(span.text.trim());
                    label.char_bounds.extend(span.char_bounds.iter().copied());
                    label
                        .tight_char_bounds
                        .extend(span.tight_char_bounds.iter().copied());
                    label
                        .glyph_advances
                        .extend(span.glyph_advances.iter().copied());
                    label
                        .glyph_bounds_sources
                        .extend(span.glyph_bounds_sources.iter().copied());
                    label.width = (dots_right - label.x).max(label.width).max(0.0);
                    continue;
                }
                if !cell_path_words {
                    label.text.push(' ');
                    label.text.push_str(span.text.trim());
                    continue;
                }
                label.text.push(' ');
                label.text.push_str(span.text.trim());
                label.char_bounds.extend(span.char_bounds.iter().copied());
                label
                    .tight_char_bounds
                    .extend(span.tight_char_bounds.iter().copied());
                label
                    .glyph_advances
                    .extend(span.glyph_advances.iter().copied());
                label
                    .glyph_bounds_sources
                    .extend(span.glyph_bounds_sources.iter().copied());
            }
            _ => kept.push(span),
        }
    }
    *spans = kept;
}

/// Rows joined to their figures by a dot leader: a table of contents, a list of
/// tables, a statement set with leaders (#574). The leader is dropped by
/// `ink_runs`, so a row is a label and a short figure with white between them
/// where the dots were; `leader_rows` counts the dots, since a label and a page
/// number with white between them and no leader is a line of prose that ends
/// in a number. At least `TABLE_REGION_MIN_ROWS` such rows running, a label
/// that wraps onto a line of its own allowed between them.
fn leader_regions(
    bands: &[TextBand],
    runs: &[Vec<InkRun>],
    pitch: f64,
    taken: &[(f64, f64)],
    min_rows: usize,
) -> Vec<TableRegion> {
    let free = |y: f64| !taken.iter().any(|(bottom, top)| y >= *bottom && y <= *top);
    let is_leader_row = |k: usize| {
        // The dots, whether each is a span of its own or they run on inside
        // the label's string (`split_at_leader` has cut the figure off).
        let dots: usize = bands[k]
            .spans
            .iter()
            .map(|span| {
                if is_leader(span) {
                    span.text.chars().filter(|c| !c.is_whitespace()).count()
                } else {
                    span.text
                        .trim_end()
                        .chars()
                        .rev()
                        .take_while(|c| *c == '.' || *c == ' ')
                        .filter(|c| *c == '.')
                        .count()
                }
            })
            .sum();
        dots >= TABLE_REGION_LEADER_DOTS
            && runs[k].len() >= 2
            && runs[k]
                .last()
                .is_some_and(|run| run.chars <= TABLE_REGION_SHORT_CELL)
    };
    let mut regions = Vec::new();
    let mut i = 0;
    while i < bands.len() {
        if !is_leader_row(i) || !free(bands[i].y) {
            i += 1;
            continue;
        }
        let (mut last_row, mut rows, mut loose) = (i, 1, 0);
        let mut above = bands[i].y;
        let mut j = i + 1;
        while j < bands.len() {
            if runs[j].is_empty() {
                j += 1;
                continue;
            }
            if above - bands[j].y > TABLE_REGION_PAIR_STEP_PITCHES * pitch || !free(bands[j].y) {
                break;
            }
            above = bands[j].y;
            if is_leader_row(j) {
                last_row = j;
                rows += 1;
                loose = 0;
            } else if runs[j].len() == 1 {
                loose += 1;
                if loose > 2 {
                    break;
                }
            } else {
                break;
            }
            j += 1;
        }
        if rows >= min_rows {
            let chosen = &bands[i..=last_row];
            regions.push(TableRegion {
                left: chosen
                    .iter()
                    .map(TextBand::left)
                    .fold(f64::INFINITY, f64::min),
                right: chosen
                    .iter()
                    .map(TextBand::right)
                    .fold(f64::NEG_INFINITY, f64::max),
                top: bands[i]
                    .spans
                    .iter()
                    .map(|span| span.y)
                    .fold(f64::NEG_INFINITY, f64::max),
                bottom: bands[last_row]
                    .spans
                    .iter()
                    .map(|span| span.y)
                    .fold(f64::INFINITY, f64::min),
                ruled: true,
                rows,
            });
        }
        i = last_row + 1;
    }
    regions
}

/// Whether a short run is a list's marker rather than a key: `1.`, `a)`, `(iv)`.
fn is_list_marker(text: &str) -> bool {
    let text = text.trim();
    text.chars().count() <= 5
        && (text.ends_with('.') || text.ends_with(')') || text.starts_with('('))
}

/// A table region one of the page's detectors proposed (#574): what a judge
/// chooses between. `hull` is `[left, bottom, right, top]` in page space, the
/// extent of the rows' ink, y up.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub struct TableProposal {
    /// `aligned`, `booktabs`, `pair` or `leader`.
    pub detector: &'static str,
    /// Whether the detector would keep it: `false` for a stack of two rows,
    /// which is a proposal for the judge and not a table for the page.
    pub accepted: bool,
    /// The region's extent.
    pub hull: [f64; 4],
}

/// Every table region the detectors accept on a page, before any gives way to
/// another, from the same spans and rules the page's blocks are built from.
///
/// A PAGE THAT IS ITSELF TURNED proposes nothing here: the regions are found in
/// the frame the page was written in, and until a proposal can be turned back
/// with it exactly, an empty list is the honest answer. Hidden: this is the
/// feature dump's and the table judge's, not a promise.
#[doc(hidden)]
pub fn table_proposals(spans: &[TextSpan], rules: &[HorizontalRule]) -> Vec<TableProposal> {
    if spans.is_empty() || PageTurn::of(spans) != PageTurn::None {
        return Vec::new();
    }
    let stats = PageStats::from_spans(spans);
    let spans: Vec<TextSpan> = spans.iter().cloned().flat_map(split_at_leader).collect();
    let mut seen = Vec::new();
    table_regions_seen(&spans, &stats, rules, Some(&mut seen));
    // And each column of a page set in columns, as a page of its own: a table
    // in one column shares its bands with the other column's prose, which no
    // detector reads as rows on the whole page (see `xy_cut_round_tables`).
    if let Some((columns, _)) = try_vertical_cut(&spans, &stats, rules) {
        for column in &columns {
            let mut more = Vec::new();
            table_regions_seen(column, &stats, rules, Some(&mut more));
            for found in more {
                if !seen.contains(&found) {
                    seen.push(found);
                }
            }
        }
    }
    seen.into_iter()
        .map(|(r, detector)| {
            let top = r.top
                + spans
                    .iter()
                    .filter(|s| r.holds(s))
                    .map(|s| s.height)
                    .fold(0.0, f64::max);
            TableProposal {
                detector,
                accepted: r.rows >= TABLE_REGION_MIN_ROWS,
                hull: [r.left, r.bottom, r.right, top],
            }
        })
        .collect()
}

/// The page cut round its tables (#574): each table region is one leaf, the
/// rest goes through the XY-cut, and each table is put back in front of the
/// first region below it that it shares any width with. Each region comes back
/// with whether it is one of those tables, which `blocks::paragraphs` reads as
/// "one grid if it makes one" (`tables::find_whole`).
fn xy_cut_round_tables(
    spans: Vec<TextSpan>,
    stats: &PageStats,
    rules: &[HorizontalRule],
    merge_turned_runs: bool,
    cell_path_words: bool,
) -> Vec<(Vec<TextBlock>, bool)> {
    let tables = table_regions(&spans, stats, rules);
    // Each quarter-turned run the layout would break as one span for the
    // layout below -- the cut, the bands, the grid -- while the detectors
    // above read the spans as drawn (#574). A marginal run merged first
    // stops being the bands, steps and short marks the detectors count: on
    // one proxy page thirteen one-glyph bands veto an aligned region that
    // blocks a ruled one, and merged to one span the veto count drops under
    // the threshold, the page comes back one ruled region, and two columns
    // of prose are gridded as a table.
    //
    // On an upright page only: a turned page is already in the frame it was
    // written in here, where its own runs read upright and its MINORITY --
    // the upright furniture of a landscape table -- reads turned, and merging
    // that minority re-decides the table around it (#574: a landscape segment
    // report comes back more duplicated and fewer of its words match).
    let spans = if merge_turned_runs {
        merge_rotated_runs(spans)
    } else {
        spans
    };
    if tables.is_empty() {
        return xy_cut_recursive(spans, 0, stats, rules)
            .into_iter()
            .map(|region| (region, false))
            .collect();
    }
    // A PAGE SET IN COLUMNS IS LOOKED AT ONE COLUMN AT A TIME. Across the
    // gutter a line of prose and a row of a table share a band, and the prose
    // reads as the row's stub: the table's region took the column beside it
    // with it. Where the cut would divide the page into columns anyway, each
    // column is its own page for this.
    // Unless the cut would divide a table found by more than its alignment --
    // ruled above and below, keys and what they stand for, a leader: the cut
    // would take the labels from what they label.
    let ruled_across = |columns: &[Vec<TextSpan>]| {
        tables.iter().filter(|table| table.ruled).any(|table| {
            columns
                .iter()
                .filter(|column| column.iter().any(|span| table.holds(span)))
                .count()
                > 1
        })
    };
    if let Some((columns, _)) =
        try_vertical_cut(&spans, stats, rules).filter(|(columns, _)| !ruled_across(columns))
    {
        return columns
            .into_iter()
            .flat_map(|column| {
                xy_cut_round_tables(column, stats, rules, merge_turned_runs, cell_path_words)
            })
            .collect();
    }
    let mut held: Vec<Vec<TextSpan>> = vec![Vec::new(); tables.len()];
    let mut rest = Vec::new();
    for span in spans {
        match tables.iter().position(|table| table.holds(&span)) {
            Some(at) => held[at].push(span),
            None => rest.push(span),
        }
    }
    let mut regions: Vec<(Vec<TextBlock>, bool)> = xy_cut_recursive(rest, 0, stats, rules)
        .into_iter()
        .map(|region| (region, false))
        .collect();
    for (table, spans) in tables.iter().zip(held) {
        if spans.is_empty() {
            continue;
        }
        let at = regions
            .iter()
            .position(|(region, _)| {
                let spans = region.iter().flat_map(|line| line.spans.iter());
                let (mut top, mut left, mut right) =
                    (f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
                for span in spans {
                    top = top.max(span.y);
                    left = left.min(span.x);
                    right = right.max(span.right());
                }
                top < table.top && left < table.right && right > table.left
            })
            .unwrap_or(regions.len());
        // One line per row, every cell of the row in it and a wrapped cell's
        // lines kept with it (`push_row`): what `tables::find` reads a row as.
        // The leaf grouping the cut uses would look for columns of prose in it
        // and find the table's.
        // A leader's dots go into the text of the label they run out from and
        // not into its box: the reader keeps them, and `tables::find` sees the
        // gutter between the label and its figure that the dots filled.
        let mut bands = group_spans_into_bands_with_stats(spans, stats);
        for band in &mut bands {
            fold_leaders(&mut band.spans, cell_path_words);
        }
        let mut rows = Vec::with_capacity(bands.len());
        let mut k = 0;
        while k < bands.len() {
            k = push_row(&bands, k, &mut rows);
        }
        regions.insert(at, (rows, true));
    }
    regions
}

/// The gap, in ems of the page's body size, that separates two cells of a
/// table row when its region is looked for on the whole page (#574).
const TABLE_REGION_GAP_EM: f64 = 2.0;
/// The narrowest a strip of white between two columns may become, in points,
/// before a row stops narrowing it.
const TABLE_REGION_MIN_STRIP: f64 = 2.0;
/// How far, in ems, a row's gap may miss a strip and still be in it.
const TABLE_REGION_EDGE_EM: f64 = 1.5;
/// A cell this short (non-blank characters) is a value, a year or a unit.
const TABLE_REGION_SHORT_CELL: usize = 16;
/// No cell but the stub may be longer than this: a run of prose beside a table
/// is a column of the page, not a cell of the table.
const TABLE_REGION_LONG_CELL: usize = 40;
/// The fewest rows that make a table region.
const TABLE_REGION_MIN_ROWS: usize = 3;
/// How many bands in a row may be labels or sparse rows inside a table.
const TABLE_REGION_MAX_LOOSE: usize = 5;
/// How many head lines over the value columns a table may carry above its
/// first row.
const TABLE_REGION_MAX_HEADS: usize = 2;
/// The most characters a run outside a ruled table's extent may hold and still
/// be a mark in the margin rather than text beside the table.
const TABLE_REGION_MARGIN_MARK: usize = 2;
/// How far apart, in points, two rule segments at one height may be and still
/// be one rule.
const RULE_JOIN_TOLERANCE: f64 = 2.0;
/// How long a rule must be, in ems, to open or close a table.
const TABLE_REGION_RULE_MIN_EM: f64 = 10.0;
/// The gap, in ems, that separates a key from its description.
const TABLE_REGION_PAIR_GAP_EM: f64 = 1.0;
/// How many leader dots make a row a leader row.
const TABLE_REGION_LEADER_DOTS: usize = 4;
/// How much white between two rows of a two-column table, in line pitches: less
/// than a statement's, since such a table has no groups to set apart.
const TABLE_REGION_PAIR_STEP_PITCHES: f64 = 2.5;
/// How much white between two bands, in line pitches, still lies inside one
/// table: the space between a statement's groups.
const TABLE_REGION_MAX_STEP_PITCHES: f64 = 4.0;

/// The leading the body of THIS SET OF SPANS is set on: the commonest step
/// between its bands (#580).
///
/// PER REGION AND NOT PER PAGE, which is the whole of what makes this safe. A
/// page-wide leading is the leading of whatever the page has most of, and a
/// document that mixes dense form boxes with columns of prose has two: on
/// `pdfjs__test_pdfs_issue6127.pdf` the prose sets it to 12 points while the
/// form's own zone gaps are under that, so a page-wide floor suppressed a cut
/// the form needed and glued a heading to the marginal notes beside it
/// (`order_agreement` 97.8 -> 81.1 over the gate set). The region being cut
/// answers about itself.
///
/// Bounded on both sides, and both bounds earn their place:
///   * at least 0.8 of the line height, because two bands closer than that are
///     one line's sub- and superscripts, a split run or a rule -- the very
///     steps that drag `median_line_spacing` down. That bound is what tells
///     6.0pt of table pitch from 12.0pt of leading on the pages this fixes. It
///     is on the HEIGHT and not on the em, because a leading can sit below the
///     em -- a 10-on-12 setting does, and at 0.9 this refused
///     `xy_cut_leaf_falls_back_to_legacy_columns_for_header_plus_three_columns`.
///   * at most three ems, because a wider step is a paragraph break, a zone
///     break or the white above a heading. Without the ceiling a region that is
///     mostly table reports its one big gap as the leading, 57.6pt on an 8pt
///     page.
///
/// Zero when the region has no step that could be a leading at all, which says
/// "do not floor anything" rather than "the leading is nothing".
fn leading_of_a_region(spans: &[TextSpan], stats: &PageStats) -> f64 {
    let tolerance = (stats.median_font_size * BAND_Y_FRACTION).max(BAND_Y_TOLERANCE);
    let mut ordered: Vec<&TextSpan> = spans.iter().collect();
    ordered.sort_by(|a, b| b.y.partial_cmp(&a.y).unwrap_or(Ordering::Equal));
    // Each band keeps the leftmost ink on it, because whether two bands are
    // lines of one paragraph is a question about their left edge.
    let mut bands: Vec<(f64, f64)> = Vec::new();
    for span in ordered {
        match bands.last_mut() {
            Some(band) if (band.0 - span.y).abs() <= tolerance => {
                band.1 = band.1.min(span.x);
            }
            _ => bands.push((span.y, span.x)),
        }
    }
    let aligned = LEADING_ALIGNED_EM * stats.median_font_size;
    let mut steps: Vec<f64> = bands
        .windows(2)
        // CONTINUOUS TEXT, and not merely a step of the right size (codex, #580
        // round two). Two bands are lines of one paragraph when they start at the
        // same left edge; a masthead over a title over a byline is three bands
        // that may be equally spaced by coincidence but are not flush with each
        // other. Without this, three equidistant one-line zones read as a leading
        // and the floor suppresses both of the region's cuts -- and a rule
        // counting occurrences alone cannot tell the two apart, because two equal
        // gaps is every gap such a region has.
        .filter(|pair| (pair[0].1 - pair[1].1).abs() <= aligned)
        .map(|pair| (pair[0].0 - pair[1].0).abs())
        .filter(|step| {
            *step >= LEADING_MIN_EM * stats.median_font_size
                && *step <= LEADING_MAX_EM * stats.median_font_size
        })
        .map(|step| (step * 10.0).round() / 10.0)
        .collect();
    steps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let mut best = (0usize, 0.0f64);
    let mut i = 0;
    while i < steps.len() {
        let mut j = i;
        while j < steps.len() && steps[j] == steps[i] {
            j += 1;
        }
        if j - i > best.0 {
            best = (j - i, steps[i]);
        }
        i = j;
    }
    // A LEADING IS A REPETITION THE REGION DOES NOT EXHAUST (codex, #580, both
    // rounds). One step is not a repetition: a region of exactly two bands has a
    // single gap, and that gap would win this count with a tally of one, so a
    // header over a body would be read as the leading of a body it is not part
    // of. The floor would then refuse the only horizontal cut the region has,
    // leave the full-width band in place, and through that block the column split
    // underneath it -- the interleave this change exists to avoid, reintroduced
    // by the change itself.
    //
    // AND A COUNT THAT EVERY GAP IN THE REGION SATISFIES IS NOT EVIDENCE EITHER,
    // which is the second round's point: three bands set equally apart have two
    // equal gaps whatever they are, so raising this count to three only moves the
    // same hole from two bands to three. What answers it is the filter above --
    // the gaps have to be between bands that are FLUSH with each other, which is
    // what tells three lines of a paragraph from a masthead over a centred title
    // over an indented byline. The count then only has to say "more than one".
    //
    // Raising the count to three was measured too and rejected: it changed ten of
    // the seventy-five pages this machine may hold and left four of the
    // twenty-one the floor had fixed unfixed, for a hole the alignment filter
    // closes without giving anything up.
    if best.0 < LEADING_MIN_OCCURRENCES {
        return 0.0;
    }
    best.1
}

/// How many gaps of one size, between bands flush with each other, a region must
/// show before that size counts as its leading (#580). Two: three lines set
/// together on one leading is the least a body can be and still have one, and
/// what keeps a small region from inventing a leading out of its own two gaps is
/// that those gaps have to be between left-aligned bands, not that there are
/// more of them.
const LEADING_MIN_OCCURRENCES: usize = 2;

/// The narrowest step that can be a line of text rather than one line's own
/// parts, as a share of the line height (#580).
const LEADING_MIN_EM: f64 = 0.8;
/// The widest step that can still be a leading rather than a break (#580).
const LEADING_MAX_EM: f64 = 3.0;
/// How far two bands' left edges may differ and still be lines of one paragraph
/// (#580). Half an em: flush enough to be set together, loose enough that the
/// measured left edge of a line opening with a narrow glyph still counts.
const LEADING_ALIGNED_EM: f64 = 0.5;

/// How far above the body's leading the paragraph-break threshold has to stay
/// (#580). Just above, and deliberately so: the threshold also has to stay
/// BELOW the page's paragraph breaks or the zone cuts stop firing, and a page
/// whose paragraph break is only a fifth wider than its leading exists.
const HORIZONTAL_CUT_LEADING_FLOOR: f64 = 1.05;
/// Minimum number of spans a column must contain before it is eligible
/// for acceptance — one-span "columns" are almost always sidebar noise
/// or table-cell fragments.
/// The white, in ems of its own size, after a single glyph that sets it apart
/// as a marker hung into a gutter rather than a word of the line (#574).
const HUNG_MARKER_GAP_EM: f64 = 0.6;
/// The letters, not counting figures or signs, the median line of each side
/// of a gutter found past hung markers must hold to be a column of text.
const RUNNING_LETTERS: usize = 30;
const XY_CUT_MIN_SPANS_PER_COLUMN: usize = 2;
/// Average characters per band a column must have before it's accepted
/// as dense prose (vs. a short-cell table column).
const XY_CUT_MIN_CHARS_PER_BAND: f64 = 8.0;

/// The page's spans as blocks, in reading order, given the horizontal rules the
/// page drew (#545): a region the document itself rules into rows is a table,
/// and a table is not read column by column.
///
/// ANN[r17/TEX2][r17/TEX4] Top-level grouping uses recursive XY-Cut
/// with a density guard. Per-page stats are computed once up front so
/// every decision downstream speaks the same typographic baseline.
fn group_spans_into_blocks(spans: Vec<TextSpan>, rules: &[HorizontalRule]) -> Vec<TextBlock> {
    if spans.is_empty() {
        return Vec::new();
    }
    // A page that is itself turned is read in the frame it was written in, and
    // an upright page is turned by nothing at all (#546).
    let turn = PageTurn::of(&spans);
    let spans = turn.into_frame(spans);
    let stats = PageStats::from_spans(&spans);
    let rules = turn.rules_in_frame(rules);
    // Each quarter-turned run as one span, so the cut cannot part it at a
    // word gap (#574). After the stats, which describe the spans as drawn. On
    // an upright page only: in the frame of a turned page the minority reads
    // turned, and it is not this merge's to touch.
    let spans = if turn == PageTurn::None {
        merge_rotated_runs(spans)
    } else {
        spans
    };
    let blocks = xy_cut_recursive(spans, 0, &stats, &rules)
        .into_iter()
        .flatten()
        .collect();
    let blocks = reassemble_cross_direction_runs(blocks);
    turn.back(blocks)
}

/// The page's spans as paragraphs, in reading order (#567).
///
/// The same layout as [`group_spans_into_blocks`], up to the point where it
/// hands back lines: the page is turned into the frame it was written in, cut,
/// and its cross-direction runs reassembled. What is added is the merge, and it
/// runs in that same frame -- "the next line down" and "the left margin" are
/// statements about the frame the type was set in, not about page space. The
/// spans a caller receives are in page space, exactly as `into_blocks` leaves
/// them.
/// The reading direction of a run, quantized to quarter turns (#643): 0 is
/// upright or unknown, then 90, 180 and 270 degrees counter-clockwise. Only
/// equality matters: runs that read in different directions never share a
/// raw block.
fn raw_direction(span: &TextSpan) -> u8 {
    let Some(t) = span.transform else {
        return 0;
    };
    let (a, b) = (t[0], t[1]);
    if a.abs() >= b.abs() {
        if a >= 0.0 {
            0
        } else {
            2
        }
    } else if b >= 0.0 {
        1
    } else {
        3
    }
}

/// Whether the face is one of the fourteen the viewer always has, by the
/// name the spans carry (#643).
fn raw_is_base14(font_name: Option<&str>) -> bool {
    font_name.is_some_and(|name| {
        name.starts_with("Helvetica")
            || name.starts_with("Times")
            || name.starts_with("Courier")
            || name.starts_with("Symbol")
            || name.starts_with("ZapfDingbats")
    })
}

/// A run's vertical metrics in ems, for the tall boxes the raw grouping
/// overlaps (#643): the recorded base-14 pair when the face is one, else the
/// embedded font's ascent and descent, else Helvetica's recorded pair for a
/// base-14 face with no binary, else the engine's embedded fallback. A face
/// the metrics lookup missed is an embedded face, not base-14: the caption
/// over the form's title gaps it by 3pt in embedded metrics and would drown
/// in Helvetica's.
fn raw_metrics(span: &TextSpan) -> (f64, f64) {
    match span.font_name.as_deref() {
        Some("Helvetica") => (f64::from(1.075f32), f64::from(-0.299f32)),
        Some("Helvetica-Bold") => (1.07, -0.307),
        _ => span
            .font_metrics
            .map(|m| (m.ascent / 1000.0, m.descent / 1000.0))
            .unwrap_or_else(|| {
                if raw_is_base14(span.font_name.as_deref()) {
                    (1.075, -0.299)
                } else {
                    (0.718, -0.207)
                }
            }),
    }
}

/// A run's tall box: the advance width with the ascender above the baseline
/// and the descender below (#643). Bullet rows set 12pt apart at 10pt type
/// overlap in these boxes and join one block; in advance boxes they would
/// stand 2pt apart and part.
fn raw_tall_box(span: &TextSpan) -> [f64; 4] {
    let (ascender, descender) = raw_metrics(span);
    [
        span.x,
        span.y + descender * span.font_size,
        span.x + span.width,
        span.y + ascender * span.font_size,
    ]
}

/// How far apart two runs may stand along the line and still share it, as a
/// multiple of the larger type size (#643). The recordings join the 2.65pt
/// gap at 7pt type (0.38em, `Deceased` to `MM`) and part the 6.5pt gap at 8pt
/// type (0.81em, the dot leaders) and the 10.5pt gap at 10pt type (1.05em,
/// the bullets from their text).
const RAW_LINE_GAP_EM: f64 = 0.6;

/// How far back past the line's end a run may start and still join it, as a
/// multiple of the larger type size (#643). The `1a` line number reads back
/// 2.2pt at 8pt type (0.28em) into its own line, while a new row reads back
/// whole ems (`/` 46pt at 7pt type past `YYYY`). Shares the forward gap's
/// threshold; the back side is untested between 0.28em and 6.6em.
const RAW_LINE_BACK_EM: f64 = 0.6;

/// Cut a block's runs into lines: the x-runs (#643).
///
/// A run joins the open line while it reads in the line's direction, its
/// tall box overlaps the run drawn just before it, and it starts within
/// sixtenths of an em of the line's end on either side. Anything else --
/// the bullets past their 10.5pt gap, `25` back left of `U.S.`, the caption
/// under the title -- opens the next line. Returns the line each run lands
/// on, in run order.
fn raw_line_of(spans: &[TextSpan]) -> Vec<usize> {
    let mut lines = Vec::with_capacity(spans.len());
    let mut line = 0;
    let mut end = f64::NEG_INFINITY;
    let mut low = f64::NEG_INFINITY;
    let mut high = f64::NEG_INFINITY;
    let mut direction = 0;
    let mut size = 0.0f64;
    for span in spans {
        let dir = raw_direction(span);
        let tall = raw_tall_box(span);
        let overlap = tall[1] <= high && tall[3] >= low;
        let gap = span.x - end;
        let em = size.max(span.font_size);
        if !lines.is_empty()
            && (dir != direction
                || !overlap
                || span.x < end - RAW_LINE_BACK_EM * em
                || gap > RAW_LINE_GAP_EM * em)
        {
            line += 1;
        }
        direction = dir;
        end = span.x + span.width;
        low = tall[1];
        high = tall[3];
        size = span.font_size;
        lines.push(line);
    }
    lines
}

/// Rebuild a raw block's text and box from its spans (#643).
///
/// Lines join with a newline, runs within a line concatenate as drawn -- the
/// spaces are inside the runs, and the fillers between them arrive with E2c.
/// Shared with the hidden-text drop, so a raw block that loses spans keeps
/// raw text.
pub(crate) fn raw_text_and_box(spans: &[TextSpan]) -> (String, [f64; 4]) {
    let mut text = String::new();
    let lines = raw_line_of(spans);
    for (i, span) in spans.iter().enumerate() {
        if i > 0 && lines[i] != lines[i - 1] {
            text.push('\n');
        }
        text.push_str(&span.text);
    }
    let mut bbox = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for span in spans {
        bbox[0] = bbox[0].min(span.x);
        bbox[1] = bbox[1].min(span.y);
        bbox[2] = bbox[2].max(span.x + span.width);
        bbox[3] = bbox[3].max(span.y + span.height);
    }
    (text, bbox)
}

/// Seal the runs drawn so far into one raw paragraph block.
fn push_raw_block(blocks: &mut Vec<Block>, spans: Vec<TextSpan>, page: usize, index: usize) {
    if spans.is_empty() {
        return;
    }
    let (text, bbox) = raw_text_and_box(&spans);
    let mut block = Block::new(BlockKind::Paragraph, page, bbox, text, index);
    block.spans = spans;
    blocks.push(block);
}

/// How much of the previous line's tall reach an overlapping row must cover
/// to join its block when it reads forward (#643). The recordings join from
/// 0.69 up (`Digital Assets` to its row) and part at 0.375 (the staggered
/// checkbox rows) and 0.10 (into `2a`).
const RAW_FWD_OVERLAP_FRAC: f64 = 0.5;

/// How far a gapped row may stand past the previous line and still read on,
/// in ems of the larger type size (#643). Past a gap the recordings join
/// to 0.125 and part from 0.186; the size-up joins reach 0.15 through the
/// size rule below, not through this gap.
const RAW_BLOCK_GAP_EM: f64 = 0.15;

/// How wide a gapped line may be and still read on, in points (#643). The
/// recordings join to 128pt at the same size and part from 178pt (the
/// `Married filing jointly` row) and 348pt; the two wider joins (199pt,
/// 224pt) read on as larger type through the size rule below.
const RAW_BLOCK_WIDTH_PT: f64 = 150.0;

/// How much larger a gapped row may be and always read on, as a multiple of
/// the previous line's size (#643). The recordings join size-ups from 1.25
/// (section heads after wrapped lines; the two wider ones only through this
/// rule). Same-size rows part through the gap and width rules; nothing
/// between 1.0 and 1.25 tests the threshold from below.
const RAW_SIZE_UP_RATIO: f64 = 1.2;

/// One cut line with the features the block rule reads (#643).
struct RawLine {
    spans: Vec<TextSpan>,
    bdc_first: Option<u32>,
    bdc_last: Option<u32>,
    x0: f64,
    x1: f64,
    low: f64,
    high: f64,
    direction: u8,
    size: f64,
}

impl RawLine {
    fn width(&self) -> f64 {
        self.x1 - self.x0
    }

    fn height(&self) -> f64 {
        self.high - self.low
    }
}

/// Whether the next line opens a new block after the previous one (#643).
///
/// One marked-content segment never parts; a turn of the reading direction
/// always does. Between rows that overlap, a row that reads back joins, and
/// a row that reads forward joins while it covers half the previous reach.
/// Past a gap, a row that reads forward parts, and a row that reads back
/// joins when it opens larger type or reads on: same size or larger, a
/// short line, a small gap.
fn raw_block_split(before: &RawLine, after: &RawLine) -> bool {
    if after.direction != before.direction {
        return true;
    }
    if let (Some(a), Some(b)) = (before.bdc_last, after.bdc_first) {
        if a == b {
            return false;
        }
    }
    let overlap = before.high.min(after.high) - before.low.max(after.low);
    let motion = after.x0 - before.x1;
    if overlap > 0.0 {
        if motion <= 0.5 {
            return false;
        }
        return overlap / before.height() < RAW_FWD_OVERLAP_FRAC;
    }
    if motion > 0.5 {
        return true;
    }
    let ratio = after.size / before.size;
    if ratio >= RAW_SIZE_UP_RATIO {
        return false;
    }
    let gap_em = -overlap / before.size.max(after.size);
    !(ratio >= 1.0 && gap_em <= RAW_BLOCK_GAP_EM && before.width() <= RAW_BLOCK_WIDTH_PT)
}

/// Group the page's runs into blocks in draw order (#643).
///
/// Lines cut first, over the whole page, and blocks group whole lines: a
/// line never spans a block boundary. The frames turn with the page exactly
/// as the default path's do; `bdc` carries each run's marked-content
/// segment, in run order.
fn raw_blocks(
    spans: Vec<TextSpan>,
    bdc: &[Option<u32>],
    page: usize,
    first_index: usize,
) -> Vec<Block> {
    if spans.is_empty() {
        return Vec::new();
    }
    debug_assert_eq!(bdc.len(), spans.len());
    let turn = PageTurn::of(&spans);
    let spans = turn.into_frame(spans);
    let line_of = raw_line_of(&spans);
    let mut lines: Vec<RawLine> = Vec::new();
    for (i, span) in spans.into_iter().enumerate() {
        let tall = raw_tall_box(&span);
        let dir = raw_direction(&span);
        let fresh = lines.is_empty() || line_of[i] != line_of[i - 1];
        if fresh {
            lines.push(RawLine {
                spans: Vec::new(),
                bdc_first: bdc[i],
                bdc_last: bdc[i],
                x0: span.x,
                x1: span.x + span.width,
                low: tall[1],
                high: tall[3],
                direction: dir,
                size: span.font_size,
            });
        }
        if let Some(line) = lines.last_mut() {
            line.bdc_last = bdc[i];
            line.x0 = line.x0.min(span.x);
            line.x1 = line.x1.max(span.x + span.width);
            line.low = line.low.min(tall[1]);
            line.high = line.high.max(tall[3]);
            line.spans.push(span);
        }
    }
    let mut blocks = Vec::new();
    let mut current: Vec<TextSpan> = Vec::new();
    for i in 0..lines.len() {
        let (head, tail) = lines.split_at_mut(i);
        let line = &mut tail[0];
        let cut = i > 0 && raw_block_split(&head[i - 1], line);
        if cut {
            let index = first_index + blocks.len();
            push_raw_block(&mut blocks, std::mem::take(&mut current), page, index);
        }
        current.append(&mut line.spans);
    }
    if !current.is_empty() {
        let index = first_index + blocks.len();
        push_raw_block(&mut blocks, current, page, index);
    }
    turn.back_blocks(blocks)
}

fn paragraph_blocks(
    spans: Vec<TextSpan>,
    rules: &[HorizontalRule],
    page: usize,
    first_index: usize,
    table_strategy: TableStrategy,
) -> Vec<Block> {
    paragraph_blocks_with_cell_path_words(spans, rules, page, first_index, table_strategy, false)
}

/// [`paragraph_blocks`] with the cell-path word rules where `cell_path_words`
/// is true: the leader fold joins abutting dots, and line and cell texts read
/// the quarter-em boundary as a space.
fn paragraph_blocks_with_cell_path_words(
    spans: Vec<TextSpan>,
    rules: &[HorizontalRule],
    page: usize,
    first_index: usize,
    table_strategy: TableStrategy,
    cell_path_words: bool,
) -> Vec<Block> {
    if spans.is_empty() {
        return Vec::new();
    }
    let turn = PageTurn::of(&spans);
    let spans = turn.into_frame(spans);
    let stats = PageStats::from_spans(&spans);
    let rules = turn.rules_in_frame(rules);
    // Once, before anything is cut: a leader drawn inside its label's string
    // is where the label ends and the figure begins.
    let spans: Vec<TextSpan> = spans.into_iter().flat_map(split_at_leader).collect();
    let (regions, whole): (Vec<Vec<TextBlock>>, Vec<bool>) = xy_cut_round_tables(
        spans,
        &stats,
        &rules,
        turn == PageTurn::None,
        cell_path_words,
    )
    .into_iter()
    .unzip();
    let direction = page_direction(&regions.iter().flatten().cloned().collect::<Vec<_>>());
    let regions: Vec<Vec<TextBlock>> = regions
        .into_iter()
        .map(|region| match direction {
            Some(direction) => reassemble_with_direction(direction, region),
            None => region,
        })
        .collect();
    let blocks = crate::blocks::paragraphs_with_cell_path_words(
        page,
        first_index,
        &regions,
        &whole,
        &rules,
        stats.median_line_spacing,
        table_strategy,
        cell_path_words,
    );
    // After the merge and before the heading classifier, so the joined block
    // is classified like any other (#574).
    let blocks = join_marker_continuations(blocks, first_index);
    turn.back_blocks(blocks)
}

/// Join the lines of a double-spaced paragraph back into one block (#574).
///
/// A passage set on wider leading than the rest of the page clears the
/// paragraph-break threshold at every line, so each of its lines arrives here
/// as a block of its own: the double-spaced abstract of an arXiv preprint, a
/// double-spaced patent description. This pass joins runs of such one-line
/// paragraph blocks, guarded so lists, addresses, labels and contents lines
/// never join. The rule is a port of the simulated one (`dsim.py`); the guards
/// are drawn from the false merges the simulation found.
///
/// AFTER the heading classifier, and only over [`BlockKind::Paragraph`]:
/// stacked headings arrive on the same wide pitch as a double-spaced run, and
/// joining before they are marked turns two headings into one paragraph
/// (#636).
pub(crate) fn join_double_spaced_runs(blocks: Vec<Block>, first_index: usize) -> Vec<Block> {
    let sizes: Vec<f64> = blocks.iter().map(block_size).collect();
    let single = |i: usize| {
        matches!(blocks[i].kind, BlockKind::Paragraph) && block_line_count(&blocks[i]) == 1
    };
    // The white between a block and the one above it, in ems of the upper.
    let gap_above = |i: usize| {
        let em = sizes[i - 1];
        let em = if em > 0.0 { em } else { 10.0 };
        (blocks[i - 1].bbox[1] - blocks[i].bbox[3]) / em
    };
    let mut out: Vec<Block> = Vec::with_capacity(blocks.len());
    let mut i = 0;
    while i < blocks.len() {
        if !single(i) {
            out.push(blocks[i].clone());
            i += 1;
            continue;
        }
        let mut run = vec![i];
        let mut j = i + 1;
        while j < blocks.len() {
            let a = run[run.len() - 1];
            if blocks[j].page != blocks[a].page || !single(j) {
                break;
            }
            let em = if sizes[a] > 0.0 { sizes[a] } else { 10.0 };
            if (sizes[j] - sizes[a]).abs() > DOUBLE_SPACED_SIZE_TOLERANCE_PT {
                break;
            }
            if (blocks[j].bbox[0] - blocks[run[0]].bbox[0]).abs() > DOUBLE_SPACED_LEFT_EM * em {
                break;
            }
            let gap = gap_above(j);
            if !(DOUBLE_SPACED_GAP_MIN_EM..=DOUBLE_SPACED_GAP_MAX_EM).contains(&gap) {
                break;
            }
            run.push(j);
            j += 1;
        }
        if run.len() < 2 {
            out.push(blocks[i].clone());
            i += 1;
            continue;
        }
        let gaps: Vec<f64> = run.iter().skip(1).map(|&k| gap_above(k)).collect();
        let median = median_gap(gaps);
        let right = run
            .iter()
            .map(|&k| blocks[k].bbox[2])
            .fold(f64::NEG_INFINITY, f64::max);
        let mut current = blocks[run[0]].clone();
        for pair in run.windows(2) {
            let (a, c) = (pair[0], pair[1]);
            let em = if sizes[a] > 0.0 { sizes[a] } else { 10.0 };
            let upper = blocks[a].text.trim();
            let lower = blocks[c].text.trim();
            let fills_the_measure = blocks[a].bbox[2] >= right - DOUBLE_SPACED_SHORT_EM * em;
            let join = fills_the_measure
                && gap_above(c) <= DOUBLE_SPACED_GAP_RATIO * median
                && !starts_with_marker(lower)
                && !double_spaced_guard(upper, lower);
            if join {
                if is_hyphen_wrap_candidate(&current.text, lower) {
                    current.text.pop();
                    current.text.push_str(lower);
                } else {
                    current.text.push(' ');
                    current.text.push_str(lower);
                }
                current.spans.extend(blocks[c].spans.iter().cloned());
                current.bbox = [
                    current.bbox[0].min(blocks[c].bbox[0]),
                    current.bbox[1].min(blocks[c].bbox[1]),
                    current.bbox[2].max(blocks[c].bbox[2]),
                    current.bbox[3].max(blocks[c].bbox[3]),
                ];
            } else {
                out.push(current);
                current = blocks[c].clone();
            }
        }
        out.push(current);
        i = j;
    }
    for (n, block) in out.iter_mut().enumerate() {
        block.index = first_index + n;
    }
    out
}

/// Join a marker line back to its hanging continuation (#574).
///
/// The merge cuts a marker-led line off its continuation at a change of
/// weight -- `(74) Agent: ...` over `Road, Kennett Square ...`, bold over
/// regular -- and the heading classifier then reads the marker line as a
/// heading. Where the continuation hangs under the marker line's text at
/// normal leading, the cut is undone here, before classification, so the
/// joined block is classified like any other. Indices are renumbered from
/// `first_index`; nothing else moves.
fn join_marker_continuations(blocks: Vec<Block>, first_index: usize) -> Vec<Block> {
    let mut out: Vec<Block> = Vec::with_capacity(blocks.len());
    for block in blocks {
        let join = match out.last() {
            Some(prev) if prev.page == block.page => {
                let prev_lines = lines_of_block(prev);
                let next_lines = lines_of_block(&block);
                match (prev_lines.first(), prev_lines.last(), next_lines.first()) {
                    (Some(first), Some(last), Some(c)) => {
                        marker_line_hangs_onto(prev, first, last, &block, c)
                    }
                    _ => false,
                }
            }
            _ => false,
        };
        if join {
            let prev = out.last_mut().expect("a previous block was checked");
            let text = crate::blocks::join_lines(&[prev.text.clone(), block.text.clone()]);
            prev.spans.extend(block.spans);
            prev.text = text;
            crate::blocks::recompute_bbox(prev);
        } else {
            out.push(block);
        }
    }
    for (n, block) in out.iter_mut().enumerate() {
        block.index = first_index + n;
    }
    out
}

/// The white a line of a double-spaced run stands under, in ems of the line
/// above it (#574). Under six tenths the lines were never split; over three
/// the white is a zone break however regularly it recurs.
const DOUBLE_SPACED_GAP_MIN_EM: f64 = 0.6;
/// See [`DOUBLE_SPACED_GAP_MIN_EM`].
const DOUBLE_SPACED_GAP_MAX_EM: f64 = 3.0;

/// How far the type size may differ between two lines of one run, in points.
const DOUBLE_SPACED_SIZE_TOLERANCE_PT: f64 = 0.5;

/// How far a line's left edge may stand from the run's first, in ems.
const DOUBLE_SPACED_LEFT_EM: f64 = 0.5;

/// How far short of the run's measure a line may fall and still be read as
/// filling it, in ems: a justified line does, an address line does not.
const DOUBLE_SPACED_SHORT_EM: f64 = 1.5;

/// How far a line's gap may stand above the run's own median and still join.
const DOUBLE_SPACED_GAP_RATIO: f64 = 1.5;

/// The type size most of the block's glyphs are set in, as the feature dump
/// measures it: one vote per glyph box, one per span without boxes.
fn block_size(block: &Block) -> f64 {
    let mut sizes: Vec<f64> = block
        .spans
        .iter()
        .flat_map(|span| {
            let votes = span.char_bounds.len().max(1);
            std::iter::repeat_n(span.font_size, votes)
        })
        .collect();
    if sizes.is_empty() {
        return 0.0;
    }
    sizes.sort_by(|a, b| a.total_cmp(b));
    sizes[sizes.len() / 2]
}

/// How many lines the block holds, as the feature dump counts them: the
/// distinct baselines, rounded to half a point.
fn block_line_count(block: &Block) -> usize {
    let mut baselines: Vec<f64> = block
        .spans
        .iter()
        .map(|span| (span.y * 2.0).round() / 2.0)
        .collect();
    baselines.sort_by(|a, b| a.total_cmp(b));
    baselines.dedup();
    baselines.len()
}

/// The middle of the run's own gaps, averaged where the run has an even
/// number of them, as `statistics.median` answers.
fn median_gap(mut gaps: Vec<f64>) -> f64 {
    gaps.sort_by(|a, b| a.total_cmp(b));
    match gaps.len() {
        0 => 0.0,
        len if len % 2 == 1 => gaps[len / 2],
        len => (gaps[len / 2 - 1] + gaps[len / 2]) / 2.0,
    }
}

/// Whether a line opens with a list marker: `[0012]`, `1.` / `1)` / `(1)`,
/// `a)` / `(a)`, a Roman `IV. `, a bullet, or a section marker (`§`, `Art.`).
fn starts_with_marker(text: &str) -> bool {
    starts_with_bracket_number(text)
        || starts_with_numbered_marker(text)
        || starts_with_letter_marker(text)
        || starts_with_roman_marker(text)
        || starts_with_bullet(text)
        || starts_with_section_marker(text)
}

/// `§ 2 ...`, `Art. 3 ...`: a section line never joins the one above it
/// (#636).
fn starts_with_section_marker(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with('§') || trimmed.starts_with("Art.")
}

/// `[0012]`: an opening bracket, three to five digits, a closing bracket.
fn starts_with_bracket_number(text: &str) -> bool {
    let Some(rest) = text.trim_start().strip_prefix('[') else {
        return false;
    };
    let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    (3..=5).contains(&digits) && rest.chars().nth(digits) == Some(']')
}

/// `1.` / `1)` / `(1)`: an optional opening bracket, one to three digits, a
/// stop or a closing bracket.
fn starts_with_numbered_marker(text: &str) -> bool {
    let trimmed = text.trim_start();
    let rest = trimmed.strip_prefix('(').unwrap_or(trimmed);
    let digits = rest.chars().take_while(|c| c.is_ascii_digit()).count();
    (1..=3).contains(&digits) && matches!(rest.chars().nth(digits), Some('.') | Some(')'))
}

/// `a)` / `(a)`: an optional opening bracket, a lowercase letter, a stop or a
/// closing bracket, and the space after it.
fn starts_with_letter_marker(text: &str) -> bool {
    let trimmed = text.trim_start();
    let rest = trimmed.strip_prefix('(').unwrap_or(trimmed);
    let mut chars = rest.chars();
    matches!(
        (chars.next(), chars.next(), chars.next()),
        (Some(letter), Some(stop), Some(space))
            if letter.is_ascii_lowercase()
                && matches!(stop, '.' | ')')
                && space.is_whitespace()
    )
}

/// `IV. `: one to four Roman capitals, a stop, and the space after it.
fn starts_with_roman_marker(text: &str) -> bool {
    let trimmed = text.trim_start();
    let numerals = trimmed
        .chars()
        .take_while(|c| matches!(c, 'I' | 'V' | 'X'))
        .count();
    if !(1..=4).contains(&numerals) {
        return false;
    }
    let mut chars = trimmed.chars().skip(numerals);
    matches!((chars.next(), chars.next()), (Some('.'), Some(space)) if space.is_whitespace())
}

/// `• item`: a bullet and the space after it.
fn starts_with_bullet(text: &str) -> bool {
    let mut chars = text.trim_start().chars();
    matches!(
        (chars.next(), chars.next()),
        (Some(bullet), Some(space))
            if matches!(bullet, '•' | '●' | '▪' | '■' | '–' | '—' | '-' | '*')
                && space.is_whitespace()
    )
}

/// Whether the pair must stay two blocks: every guard is drawn from a false
/// merge the simulation found (#574).
fn double_spaced_guard(upper: &str, lower: &str) -> bool {
    starts_with_number_then_capital(lower)
        || ends_in_digit(upper)
        || holds_key_value(upper)
        || lower.ends_with(':')
        || reads_as_heading(lower)
        || ends_sentence_then_capital(upper, lower)
}

/// `26 Consolidated Statements`: a contents line, a number and a capital word.
fn starts_with_number_then_capital(text: &str) -> bool {
    let digits = text.chars().take_while(|c| c.is_ascii_digit()).count();
    if !(1..=4).contains(&digits) {
        return false;
    }
    let mut chars = text.chars().skip(digits);
    let mut spaces = 0;
    for c in chars.by_ref() {
        if c.is_whitespace() {
            spaces += 1;
        } else {
            return spaces > 0 && c.is_ascii_uppercase();
        }
    }
    false
}

/// `Ausfertigungsdatum: 20.12.1963`: a contents line ends in a digit, whatever
/// punctuation follows it.
fn ends_in_digit(text: &str) -> bool {
    text.trim_end()
        .trim_end_matches(|c: char| !(c.is_alphanumeric() || c == '_'))
        .chars()
        .next_back()
        .is_some_and(|c| c.is_ascii_digit())
}

/// `Key: value`: a line holding a colon followed by text is a label and its
/// value, not the start of a sentence.
fn holds_key_value(text: &str) -> bool {
    text.match_indices(':')
        .any(|(at, _)| !text[at + 1..].trim_start().is_empty())
}

/// `Leases`: three words or fewer with no closing punctuation is a heading.
fn reads_as_heading(text: &str) -> bool {
    text.split_whitespace().count() <= 3 && !text.ends_with(['.', '!', '?', ';', ','])
}

/// A full stop and a capital on the same pitch: two paragraphs, not one.
fn ends_sentence_then_capital(upper: &str, lower: &str) -> bool {
    upper.ends_with(['.', '!', '?']) && lower.chars().next().is_some_and(|c| c.is_uppercase())
}

/// Characters a marker-led line opens with as a bullet (#574).
///
/// The dozen the patent front pages and the annual reports use. Both
/// triangular bullets are here because the two codepoints render as one
/// glyph and either may be what the page drew.
const MARKER_BULLETS: [char; 13] = [
    '\u{2022}', // bullet
    '\u{25CF}', // black circle
    '\u{25AA}', // black small square
    '\u{25E6}', // white bullet
    '\u{2023}', // triangular bullet
    '\u{2043}', // hyphen bullet
    '\u{2219}', // bullet operator
    '\u{00B7}', // middle dot
    '\u{2013}', // en dash
    '\u{2014}', // em dash
    '-',        // hyphen-minus
    '*',        // asterisk
    '>',        // greater-than
];

/// Whether a letter is a roman digit, either case (#574).
fn is_roman_digit(c: char) -> bool {
    matches!(
        c.to_ascii_lowercase(),
        'i' | 'v' | 'x' | 'l' | 'c' | 'd' | 'm'
    )
}

/// The marker a line opens with, if it opens with one (#574).
///
/// A bullet, `(n)` or `[n]` of one to three digits, `n.` or `n)`, `a.`,
/// `a)` or `(a)`, or a roman numeral `(i)` or `iv.`. Ordered markers want
/// white space and text after the delimiter, so `3.14` and a name's `A.`
/// stay prose; a bullet wants text after it. Answers the marker's length
/// in bytes without the trailing space, and its inked characters, which is
/// what maps the marker onto the spans that drew it.
fn marker_prefix(text: &str) -> Option<(usize, usize)> {
    let trimmed = text.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    let first = trimmed.chars().next()?;
    if MARKER_BULLETS.contains(&first) {
        let rest = &trimmed[first.len_utf8()..];
        if rest.trim().is_empty() {
            return None;
        }
        return Some((first.len_utf8(), 1));
    }
    if first == '(' || first == '[' {
        let closing = if first == '(' { ')' } else { ']' };
        let body = &trimmed[first.len_utf8()..];
        let digits = body.chars().take_while(|c| c.is_ascii_digit()).count();
        if (1..=3).contains(&digits) {
            let mut rest = &body[digits..];
            if rest.starts_with(closing) {
                rest = &rest[closing.len_utf8()..];
                if rest.starts_with(char::is_whitespace) && !rest.trim().is_empty() {
                    return Some((first.len_utf8() + digits + closing.len_utf8(), 2 + digits));
                }
            }
        }
        if first == '(' {
            let letters = body.chars().take_while(|c| c.is_ascii_alphabetic()).count();
            if (1..=6).contains(&letters) {
                let word = &body[..letters];
                if letters == 1 || word.chars().all(is_roman_digit) {
                    let mut rest = &body[letters..];
                    if rest.starts_with(')') {
                        rest = &rest[1..];
                        if rest.starts_with(char::is_whitespace) && !rest.trim().is_empty() {
                            return Some((1 + letters + 1, 2 + letters));
                        }
                    }
                }
            }
        }
        return None;
    }
    if first.is_ascii_digit() {
        let digits = trimmed.chars().take_while(|c| c.is_ascii_digit()).count();
        if (1..=3).contains(&digits) {
            let rest = &trimmed[digits..];
            if matches!(rest.chars().next(), Some('.') | Some(')')) {
                let after = &rest[1..];
                if after.starts_with(char::is_whitespace) && !after.trim().is_empty() {
                    return Some((digits + 1, digits + 1));
                }
            }
        }
        return None;
    }
    if first.is_ascii_alphabetic() {
        let letters = trimmed
            .chars()
            .take_while(|c| c.is_ascii_alphabetic())
            .count();
        if letters == 1 {
            let rest = &trimmed[1..];
            if matches!(rest.chars().next(), Some('.') | Some(')')) {
                let after = &rest[1..];
                if after.starts_with(char::is_whitespace) && !after.trim().is_empty() {
                    return Some((2, 2));
                }
            }
            return None;
        }
        if (2..=6).contains(&letters) {
            let word = &trimmed[..letters];
            if word.chars().all(is_roman_digit) {
                let rest = &trimmed[letters..];
                if matches!(rest.chars().next(), Some('.') | Some(')')) {
                    let after = &rest[1..];
                    if after.starts_with(char::is_whitespace) && !after.trim().is_empty() {
                        return Some((letters + 1, letters + 1));
                    }
                }
            }
        }
        return None;
    }
    None
}

/// The vertical extent of a span, from its glyph boxes where they exist and
/// from the type size where they do not.
fn span_bottom_top(span: &TextSpan) -> (f64, f64) {
    let mut bottom = f64::INFINITY;
    let mut top = f64::NEG_INFINITY;
    for bounds in &span.char_bounds {
        bottom = bottom.min(bounds[1]);
        top = top.max(bounds[3]);
    }
    if bottom.is_finite() && top.is_finite() && top > bottom {
        return (bottom, top);
    }
    (span.y - span.font_size * 0.2, span.y + span.font_size * 0.8)
}

/// Characters that are not white space: what a marker is counted in, because
/// the line's text joins its spans with spaces the spans never drew.
fn inked_chars(text: &str) -> usize {
    text.chars().filter(|c| !c.is_whitespace()).count()
}

/// Where the text after a marker line's marker begins: the left edge of the
/// first span the marker does not reach into (#574).
///
/// `None` where the marker is glued inside its line's first span and has no
/// column of its own to measure. A run of white space after the marker is
/// the spacing the page set it apart with, not where its text begins.
fn text_left_after_marker(block: &Block, line: &[usize], marker_inked: usize) -> Option<f64> {
    if marker_inked == 0 {
        return None;
    }
    let mut seen = 0usize;
    for i in line {
        let span = &block.spans[*i];
        let count = inked_chars(&span.text);
        if seen == marker_inked {
            if count == 0 {
                continue;
            }
            return Some(span.x);
        }
        if seen > marker_inked {
            return None;
        }
        seen += count;
    }
    None
}

/// Whether the line c continues the marker block a: all six clauses of the
/// marker-hang rule (#574).
///
/// a's FIRST line opens with a marker and its LAST line does not end in a
/// colon, c hangs under the first line's text at normal leading without
/// opening a marker of its own, and the last line ran to the measure rather
/// than turning back. The hang is read off the first line's own text column
/// where the page drew the marker in a span of its own, and off its left
/// edge where it did not.
///
/// Two lines of a rather than one, because the marker item may wrap over
/// several lines before the weight cut parts it from its continuation: the
/// marker is on the first line and the wrap on the last, and reading both
/// off the last line leaves a wrapped item uncovered.
fn marker_line_hangs_onto(
    a_block: &Block,
    a_first: &[usize],
    a_last: &[usize],
    c_block: &Block,
    c_line: &[usize],
) -> bool {
    let a_text = line_text(a_block, a_first);
    let c_text = line_text(c_block, c_line);
    let Some((_, a_inked)) = marker_prefix(&a_text) else {
        return false;
    };
    if marker_prefix(&c_text).is_some() {
        return false;
    }
    if line_text(a_block, a_last).trim_end().ends_with(':') {
        return false;
    }
    let anchor_of = |block: &Block, line: &[usize]| -> Option<usize> {
        line.iter()
            .copied()
            .max_by_key(|i| inked_chars(&block.spans[*i].text))
    };
    let (Some(a_at), Some(c_at)) = (anchor_of(a_block, a_last), anchor_of(c_block, c_line)) else {
        return false;
    };
    let a_anchor = &a_block.spans[a_at];
    let c_anchor = &c_block.spans[c_at];
    let em = a_anchor.font_size;
    if !em.is_finite() || em <= 0.0 {
        return false;
    }
    if a_anchor.y <= c_anchor.y {
        return false;
    }
    let left_of = |block: &Block, line: &[usize]| {
        line.iter()
            .map(|i| block.spans[*i].x)
            .fold(f64::INFINITY, f64::min)
    };
    let right_of = |block: &Block, line: &[usize]| {
        line.iter()
            .map(|i| crate::blocks::span_right(&block.spans[*i]))
            .fold(f64::NEG_INFINITY, f64::max)
    };
    let a_left = left_of(a_block, a_first);
    let a_right = right_of(a_block, a_last);
    let c_left = left_of(c_block, c_line);
    let c_right = right_of(c_block, c_line);
    if !(a_left.is_finite() && a_right.is_finite() && c_left.is_finite() && c_right.is_finite()) {
        return false;
    }
    if let Some(text_left) = text_left_after_marker(a_block, a_first, a_inked) {
        if (c_left - text_left).abs() > MARKER_TEXT_LEFT_TOL_EM * em {
            return false;
        }
    } else if !(c_left > a_left + MARKER_HANG_MIN_EM * em
        && c_left < a_left + MARKER_HANG_MAX_EM * em)
    {
        return false;
    }
    if a_right < a_right.max(c_right) - MARKER_WRAP_SLACK_EM * em {
        return false;
    }
    let a_bottom = a_last
        .iter()
        .map(|i| span_bottom_top(&a_block.spans[*i]).0)
        .fold(f64::INFINITY, f64::min);
    let c_top = c_line
        .iter()
        .map(|i| span_bottom_top(&c_block.spans[*i]).1)
        .fold(f64::NEG_INFINITY, f64::max);
    if !(a_bottom.is_finite() && c_top.is_finite()) {
        return false;
    }
    if ((a_bottom - c_top) / em) >= MARKER_GAP_MAX_EM {
        return false;
    }
    true
}

/// The spans of a block grouped into lines, top first, each line left to
/// right. The same grouping the feature dump reads words by: sorted by
/// baseline, then by x, a new line where the baseline steps by half an em.
///
/// `pub(crate)` for the markdown serializer, which reads an all-monospace
/// paragraph's lines back out of it as a fenced block (#636).
pub(crate) fn lines_of_block(block: &Block) -> Vec<Vec<usize>> {
    let mut order: Vec<usize> = (0..block.spans.len()).collect();
    order.sort_by(|a, b| {
        block.spans[*b]
            .y
            .partial_cmp(&block.spans[*a].y)
            .unwrap_or(Ordering::Equal)
            .then_with(|| {
                block.spans[*a]
                    .x
                    .partial_cmp(&block.spans[*b].x)
                    .unwrap_or(Ordering::Equal)
            })
    });
    let mut lines: Vec<Vec<usize>> = Vec::new();
    let mut tops: Vec<f64> = Vec::new();
    for idx in order {
        let size = block.spans[idx].font_size;
        let tolerance = if size > 0.0 { 0.5 * size } else { 1e-6 };
        if let (Some(last), Some(top)) = (lines.last_mut(), tops.last().copied()) {
            if (top - block.spans[idx].y).abs() < tolerance {
                last.push(idx);
                continue;
            }
        }
        lines.push(vec![idx]);
        tops.push(block.spans[idx].y);
    }
    for line in &mut lines {
        line.sort_by(|a, b| {
            block.spans[*a]
                .x
                .partial_cmp(&block.spans[*b].x)
                .unwrap_or(Ordering::Equal)
        });
    }
    lines
}

/// One line as it reads: its spans left to right through the same assembly
/// the merge built the block's text from.
///
/// `pub(crate)` beside [`lines_of_block`], for the same fenced block (#636).
pub(crate) fn line_text(block: &Block, line: &[usize]) -> String {
    let spans: Vec<TextSpan> = line.iter().map(|i| block.spans[*i].clone()).collect();
    TextBlock { spans }.text()
}

/// The quarter turn that puts a page which is itself turned back in the frame
/// it was written in (#546).
///
/// A page carries a `/Rotate`, of its own or inherited from the `Pages` tree,
/// and `Page::initial_transform` turns every glyph on it with the page: the
/// text device sees a page of lines running up, down or backwards. Bands are
/// horizontal and they read one direction, so on such a page they lie ACROSS
/// its lines: each glyph of a line lands in a band of its own and the page
/// reads as letter salad -- `ro t a ti o n  9 0` where pdfbox `PDFBOX-6049`
/// says `rotation 90`, and `081 noitator` on its half-turned page. This is the
/// same failure #435 fixed for a run drawn against the direction of its page,
/// and it stayed on the pages where the turned direction is the MAJORITY: what
/// #435 assembles there is the upright text on a turned page.
///
/// So the layout runs in the frame the page was written in. Every span is
/// turned into the direction most of the page's glyphs are drawn in before
/// `PageStats`, the cut and the bands, and the blocks are turned back before
/// they are returned, which leaves the spans the caller gets in page space to
/// the bit.
///
/// QUARTER TURNS ONLY, and the reason is exactness rather than taste: the sine
/// and cosine of a quarter turn are 0 and ±1, so the turn and its inverse move
/// no coordinate by a single bit, and a page this pass declines to turn comes
/// out of it byte for byte as it went in. A page `/Rotate` is a multiple of 90
/// by §7.7.3.3, which is the whole of what this is about; a page drawn entirely
/// at a free angle keeps its own bands, exactly as it did before (they read it:
/// `a_page_written_entirely_at_an_angle_keeps_its_own_bands`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PageTurn {
    /// The page reads left to right already, or in no direction this undoes.
    None,
    /// Its lines run up the page: `/Rotate 90`.
    Quarter,
    /// Its lines run right to left: `/Rotate 180`.
    Half,
    /// Its lines run down the page: `/Rotate 270`.
    ThreeQuarter,
}

impl PageTurn {
    /// The turn that makes the direction the page is WRITTEN in upright.
    ///
    /// Written in, and not merely the heaviest of: this turns the layout of a
    /// whole page, so the direction has to carry more than half its glyphs.
    /// The plurality rule #435 picks its cross-direction runs by is the right
    /// one there -- whatever is not the page's direction is a run on it -- and
    /// the wrong one here, because a page with no majority direction has no
    /// frame to be read in and turning it by a 26% plurality moves every block
    /// on it. pdfbox `AcroFormsRotation` is that page: four blocks of form
    /// fields at the four rotations, a quarter of the glyphs each, and the
    /// plurality rule turned its UPRIGHT page and put its fields in an order
    /// neither mutool nor poppler reads them in (order agreement 100 -> 50 on
    /// the gate set, where nothing else on 500 documents moved at all).
    /// A REFLECTION IS NOT A TURN, and only the determinant says which it is
    /// (codex, review round 2). `heaviest_direction` reads the baseline vector,
    /// and a horizontal reflection such as `[-1, 0, 0, 1, e, f]` runs its
    /// baseline in `(-1, 0)` exactly as a half turn does. Classifying that page
    /// as `Half` makes [`Self::into_frame`] negate BOTH coordinates, so a
    /// mirrored block whose lines descend the page comes back bottom to top --
    /// a reading order this pass invented out of a page nobody turned.
    ///
    /// Reflected glyphs are therefore counted in the page's total and in no
    /// direction's share. A page mostly drawn mirrored cannot reach a majority
    /// and comes out of this pass byte for byte as it went in, which is what
    /// `Self::None` means here.
    fn of(spans: &[TextSpan]) -> Self {
        let page_total: usize = spans.iter().map(glyph_weight).sum();
        if page_total == 0 {
            return Self::None;
        }
        let turned: Vec<&TextSpan> = spans
            .iter()
            .filter(|span| is_a_proper_rotation(span))
            .collect();
        let turned_total: usize = turned.iter().copied().map(glyph_weight).sum();
        let Some((page, share_of_turned)) = heaviest_direction(turned.into_iter()) else {
            return Self::None;
        };
        // The share is of the WHOLE page, mirrored glyphs included: measured
        // against the rotations alone, one upright line on a mirrored page
        // would carry 100 % of the vote.
        let share = share_of_turned * turned_total as f64 / page_total as f64;
        if share <= 0.5 {
            return Self::None;
        }
        [Self::Quarter, Self::Half, Self::ThreeQuarter]
            .into_iter()
            .find(|turn| directions_agree(page, turn.direction()))
            .unwrap_or(Self::None)
    }

    /// The direction a page this turn undoes is written in.
    fn direction(self) -> Direction {
        match self {
            Self::None => (1.0, 0.0),
            Self::Quarter => (0.0, 1.0),
            Self::Half => (-1.0, 0.0),
            Self::ThreeQuarter => (0.0, -1.0),
        }
    }

    /// The `(cos, sin)` of the rotation into the frame: the one that takes
    /// [`Self::direction`] to `(1, 0)`.
    fn rotation(self) -> (f64, f64) {
        match self {
            Self::None => (1.0, 0.0),
            Self::Quarter => (0.0, -1.0),
            Self::Half => (-1.0, 0.0),
            Self::ThreeQuarter => (0.0, 1.0),
        }
    }

    /// Every span in the frame the page was written in.
    fn into_frame(self, mut spans: Vec<TextSpan>) -> Vec<TextSpan> {
        if self == Self::None {
            return spans;
        }
        let rotation = self.rotation();
        for span in &mut spans {
            // In its own frame a run is as wide as it advances along its
            // baseline, and that is not what `draw_glyph` accumulated: `width`
            // is the furthest a glyph reached in PAGE x, which for a run
            // drawn up the page is one advance however long the run is. Every
            // reader of `width` above this point -- the column gutters, the
            // band gaps, the cell edges -- would take a line of a turned page
            // for a single word.
            let along = baseline_extent(span);
            turn_span(span, rotation);
            span.width = along;
        }
        spans
    }

    /// The ruling lines of #545 in the frame the page is read in.
    ///
    /// The device records horizontal rules and no others, and only the half
    /// turn maps a horizontal line to a horizontal line -- mirrored, which is
    /// why the two ends swap. A quarter turn maps one to a vertical line:
    /// what the device caught on such a page are its column separators, and
    /// its row rules were never horizontal to begin with, so they were never
    /// recorded at all. Those pages are cut without rules, exactly as every
    /// page was before #545, rather than with rules that mean the other thing
    /// (#546, review round 2).
    fn rules_in_frame(self, rules: &[HorizontalRule]) -> Cow<'_, [HorizontalRule]> {
        let rotation = self.rotation();
        match self {
            Self::None => Cow::Borrowed(rules),
            Self::Half => Cow::Owned(
                rules
                    .iter()
                    .map(|rule| {
                        let (left, y) = turn_point(rule.left, rule.y, rotation);
                        let (right, _) = turn_point(rule.right, rule.y, rotation);
                        HorizontalRule {
                            left: left.min(right),
                            right: left.max(right),
                            y,
                        }
                    })
                    .collect(),
            ),
            Self::Quarter | Self::ThreeQuarter => Cow::Owned(Vec::new()),
        }
    }

    /// The blocks back in page space, with every span's geometry the caller's
    /// own again.
    fn back(self, mut blocks: Vec<TextBlock>) -> Vec<TextBlock> {
        if self == Self::None {
            return blocks;
        }
        let (cos, sin) = self.rotation();
        for span in blocks.iter_mut().flat_map(|block| block.spans.iter_mut()) {
            turn_span(span, (cos, -sin));
            span.width = page_space_width(span);
        }
        blocks
    }

    /// The paragraphs back in page space (#567).
    ///
    /// The same restoration [`Self::back`] performs on lines, and then the
    /// block's own box again: a box computed in the frame the page was written
    /// in describes a rectangle nobody on the caller's side can find.
    fn back_blocks(self, mut blocks: Vec<Block>) -> Vec<Block> {
        if self == Self::None {
            return blocks;
        }
        let (cos, sin) = self.rotation();
        for block in blocks.iter_mut() {
            for span in block.spans.iter_mut() {
                turn_span(span, (cos, -sin));
                span.width = page_space_width(span);
            }
            crate::blocks::recompute_bbox(block);
        }
        blocks
    }
}

/// How far a span reaches along its own baseline, from its origin.
fn baseline_extent(span: &TextSpan) -> f64 {
    let direction = span_direction(span);
    baseline_end(span, direction) - baseline_coordinates(span, direction).0
}

/// The width `draw_glyph` measures: the furthest any of the span's glyphs
/// reaches in page x, from the span's origin.
///
/// This is what [`PageTurn::back`] restores, so that a caller of
/// `extract_text_blocks` on a turned page reads the same `width` off a span as
/// one on an upright page does. It is derived from the bounds rather than
/// remembered because the bounds are the same measurements `draw_glyph`
/// accumulated it from; a span with no bounds -- every hand-built one -- keeps
/// the width it came with.
fn page_space_width(span: &TextSpan) -> f64 {
    span.char_bounds
        .iter()
        .map(|bound| bound[2] - span.x)
        .reduce(f64::max)
        .unwrap_or(span.width)
}

/// Turn one span's geometry by `(cos, sin)`.
///
/// What turns is what the layout reads as a position: the span's origin, the
/// baseline its transform names, and the origin of each glyph bound.
///
/// What does not turn is what is measured along that baseline and means the
/// same in either frame. A `char_bound` is one of those: `draw_glyph` writes
/// it as `[x, y, x + advance, y + font size]` around a PAGE-space origin, so
/// it is an advance and a font size hung off a point rather than the rectangle
/// a turned glyph covers -- which is what `baseline_end` reads back out of it.
/// Turning the two numbers as if the entry were a rectangle would turn a
/// measurement that was never in page space to begin with.
///
/// `tight_char_bounds` is deliberately left alone. That one IS a page-space
/// rectangle, and it is ink rather than layout: nothing between here and the
/// blocks reads it, so leaving it where it is keeps it exact for the caller who
/// does (`GeometryMode::RichGeometry`).
fn turn_span(span: &mut TextSpan, rotation: (f64, f64)) {
    let (x, y) = turn_point(span.x, span.y, rotation);
    span.x = x;
    span.y = y;
    if let Some(coeffs) = span.transform.as_mut() {
        let (a, b) = turn_point(coeffs[0], coeffs[1], rotation);
        let (c, d) = turn_point(coeffs[2], coeffs[3], rotation);
        let (e, f) = turn_point(coeffs[4], coeffs[5], rotation);
        *coeffs = [a, b, c, d, e, f];
    }
    for bound in &mut span.char_bounds {
        let (advance, size) = (bound[2] - bound[0], bound[3] - bound[1]);
        let (x0, y0) = turn_point(bound[0], bound[1], rotation);
        *bound = [x0, y0, x0 + advance, y0 + size];
    }
}

/// A point turned by `(cos, sin)`.
fn turn_point(x: f64, y: f64, (cos, sin): (f64, f64)) -> (f64, f64) {
    (x * cos - y * sin, x * sin + y * cos)
}

/// Take the runs drawn in another direction than the page's out of the blocks
/// the page's own layout put them in, and give each one back as the line it is
/// (#435).
///
/// Bands are horizontal, and they read one direction. The letters of a diagonal
/// watermark, of a rotated table header, of the marginalia down the side of a
/// page each land in a band of their own, interleaved with whatever upright text
/// shares that band, and the reader gets `rk Xa Orm test value aBte D df wN t pA
/// sS te` where pdfjs issue17069 says `test pdf watermark` and `SANDBOX`.
///
/// WHERE THE PAGE'S OWN DIRECTION COMES FROM. The direction most of the page's
/// glyphs are drawn in keeps the page's bands, and on a page that is ITSELF
/// turned -- a `/Rotate 90` page, whose every glyph reaches the device turned
/// with it -- those bands would lie across its lines and read it as badly as a
/// watermark read before this pass (pdfbox `PDFBOX-6049`, `AcroFormsRotation`).
/// Such a page is turned back into the frame it was written in before the cut
/// and the bands ever see it ([`PageTurn`], #546), so by the time this pass
/// runs the page's own direction is upright again and what is left for it is
/// what it is for: the run drawn against the direction of the page it is on.
///
/// This runs AFTER the page's cut and takes nothing away from it. Where the
/// page's columns and zones are is decided by every glyph on it, rotated ones
/// included -- a run of marginalia down the left edge is part of why a page
/// splits where it does, and a version of this that cut the page without them
/// moved the reading order of `issue6127` by sixteen points. So the layout is
/// the layout, and what changes here is only which line each rotated glyph is
/// read as part of.
fn reassemble_cross_direction_runs(blocks: Vec<TextBlock>) -> Vec<TextBlock> {
    let Some(page) = page_direction(&blocks) else {
        return blocks;
    };
    reassemble_with_direction(page, blocks)
}

/// The same pass, told which direction the page reads in.
///
/// Split out for [`paragraph_blocks`], which runs it once per region of the cut
/// rather than once over the page: a run drawn across the type is read as part
/// of the region it was drawn in, and the page's own direction is still the
/// page's -- decided over every block on it, which is why it is an argument
/// here and not something each region votes on for itself.
fn reassemble_with_direction(page: Direction, blocks: Vec<TextBlock>) -> Vec<TextBlock> {
    let mut kept: Vec<TextBlock> = Vec::with_capacity(blocks.len());
    // Each cross-direction span, with the position in the page's order that the
    // block it was taken from held.
    let mut cross: Vec<PlacedSpan> = Vec::new();
    for block in blocks {
        let at = kept.len();
        let mut page_spans = Vec::with_capacity(block.spans.len());
        for span in block.spans {
            if directions_agree(page, span_direction(&span)) {
                page_spans.push(span);
            } else {
                cross.push((at, span));
            }
        }
        if !page_spans.is_empty() {
            kept.push(TextBlock { spans: page_spans });
        }
    }

    if cross.is_empty() {
        return kept;
    }

    // `lines_along_their_own_baselines` returns the runs in the order they are
    // to be read. They are inserted back to front -- so that inserting one
    // never moves the position another was measured at -- and two runs that
    // share a position are inserted in the reverse of their order, because each
    // insertion at the same index pushes the last one down (#435, review round
    // 2).
    let runs = lines_along_their_own_baselines(cross);
    for (at, run) in runs.into_iter().rev() {
        let at = at.min(kept.len());
        kept.insert(at, run);
    }
    kept
}

/// The direction most of a page's glyphs are drawn in, or `None` for a page
/// with no text at all.
fn page_direction(blocks: &[TextBlock]) -> Option<Direction> {
    dominant_direction(blocks.iter().flat_map(|block| block.spans.iter()))
}

/// A span's provenance from its glyphs': an estimate if any glyph was
/// estimated, tight if any was measured tight and none estimated, the advance
/// otherwise. One definition, for the span as `compute_tight_bounds` makes it
/// and for a span that loses glyphs afterwards (codex, #1974).
pub(crate) fn aggregate_bounds_source(sources: &[BoundsSource]) -> BoundsSource {
    if sources.contains(&BoundsSource::Estimate) {
        BoundsSource::Estimate
    } else if sources.contains(&BoundsSource::Tight) {
        BoundsSource::Tight
    } else {
        BoundsSource::Advance
    }
}

/// #582's reading of tracking, asked again of a FINISHED span (#1973).
///
/// #582 believes a run is tracked once four gaps in a row agree, deciding at
/// each glyph from the gaps before it. A word of four letters has three --
/// `P E N N`, `P A G E` -- and a word space set wider than the tracking starts a
/// new span, so on a display line of short words the count never reaches four
/// and every letter keeps the space the consensus gave it. Once the span is
/// finished all its gaps are known, and the same question has an answer: the
/// span's tracking is its median gap, a consensus space at a gap within
/// [`TRACKING_TOLERANCE`]-and-a-bit of it -- or tighter, a kerned pair such as
/// `TA` -- is tracking and goes, and one clearly wider is a word boundary and
/// stays. Which is #582's choice about letters at one constant distance,
/// pinned there by `a_line_of_single_letter_words_reads_as_tracking_and_that_is_the_choice`.
///
/// Asked only of a span of capitals that shows the symptom (four single
/// letters between spaces), and only where its glyphs are its letters one for
/// one: every space in it is then the consensus's own and none is a glyph the
/// page drew. A span with a drawn space, a ligature or lower case is left to
/// the rules after this.
fn retrack_a_finished_span(span: &mut TextSpan, chars: &[char]) -> bool {
    let letters: Vec<char> = chars.iter().copied().filter(|c| *c != ' ').collect();
    if letters.len() != span.char_bounds.len() || letters.len() < 4 {
        return false;
    }
    if letters.iter().any(|c| c.is_lowercase()) || !letters.iter().any(|c| c.is_uppercase()) {
        return false;
    }
    let singles = span
        .text
        .split(' ')
        .filter(|w| w.chars().count() == 1 && w.chars().all(char::is_uppercase))
        .count();
    if singles < 4 {
        return false;
    }
    let gaps: Vec<f64> = span
        .char_bounds
        .windows(2)
        .map(|p| p[1][0] - p[0][2])
        .collect();
    let mut sorted = gaps.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    // THE LOWER MEDIAN (codex, #1980). A short phrase can have as many word
    // gaps as letter gaps -- `I AM OK`: 1, 1, 3, 3 -- and the upper median then
    // takes the word space for the tracking and removes every space. The lower
    // one stays with the letters, and a kerned pair below them (`TA`) is too
    // rare in a word to pull it down.
    let tracking = sorted[(sorted.len() - 1) / 2];
    // Real tracking: the letters stand apart. In prose the boxes of a word's
    // letters touch -- they are advances -- so its median gap is nothing, and
    // the consensus's spaces there are word spaces, which this must not take.
    if tracking <= TRACKED_GAP_MIN_EM * span.font_size {
        return false;
    }
    let inside_a_word = tracking * (1.0 + TRACKING_TOLERANCE + 0.1);
    let mut spaced_before = vec![false; letters.len()];
    let mut k = 0;
    for c in chars {
        if *c == ' ' {
            if k < letters.len() {
                spaced_before[k] = true;
            }
        } else {
            k += 1;
        }
    }
    let mut text = String::with_capacity(chars.len());
    for (k, c) in letters.iter().enumerate() {
        if k > 0 && spaced_before[k] && gaps[k - 1] > inside_a_word {
            text.push(' ');
        }
        text.push(*c);
    }
    if chars.last() == Some(&' ') {
        text.push(' ');
    }
    let changed = text != span.text;
    span.text = text;
    changed
}

/// Letter-spacing done with SPACE CHARACTERS, taken back out (#1973).
///
/// #582 reads tracking that the page draws as GAPS. A good share of display
/// lines draw it with space glyphs instead -- `P O W E L L  I N D U S T R I E S`,
/// a space after every capital and two where the words part -- and those
/// arrive as text, so no gap is ever there to judge: on the evidence of the
/// 24-09 round it is the largest class of text we write that the page does not
/// hold (0.74 % of our words on DocLayNet, against Docling's 0.07 %).
///
/// A run of four single capitals or more with one space between each is taken
/// for letter-spacing only when THE PAGE MARKS ITS WORD BOUNDARIES with a
/// double space (`P O W E L L  I N D U S T R I E S`). The single spaces then go
/// and each double one becomes one. With no such mark, `A B C D` stays as it
/// is: letters set apart on purpose and letters tracked by spaces are the same
/// bytes, and the boundary is the only thing that tells them apart. A pair of
/// capitals inside the run (`O P E R AT I O N S`, a kerned pair drawn as one)
/// does not end it.
fn untrack_spaced_letters(span: &mut TextSpan) {
    let chars: Vec<char> = span.text.chars().collect();
    if chars.len() < 7 || !chars.contains(&' ') {
        return;
    }
    // The finished span first; what it leaves is asked of the rules below.
    let chars: Vec<char> = if retrack_a_finished_span(span, &chars) {
        span.text.chars().collect()
    } else {
        chars
    };
    let aligned = span.char_bounds.len() == chars.len();
    let tight = span.tight_char_bounds.len() == chars.len();
    // Words and the separators between them, as char ranges.
    let mut words: Vec<(usize, usize)> = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        if chars[at] == ' ' {
            at += 1;
            continue;
        }
        let start = at;
        while at < chars.len() && chars[at] != ' ' {
            at += 1;
        }
        words.push((start, at));
    }
    // Capitals and their punctuation only, as in `untrack_spaced_spans`.
    let small = |(a, b): (usize, usize)| {
        (b - a == 1 || b - a == 2)
            && chars[a..b]
                .iter()
                .all(|c| c.is_uppercase() || matches!(c, ',' | '.' | '\'' | '’' | '&' | '-'))
            && (b - a == 1 || chars[a..b].iter().any(|c| c.is_uppercase()))
    };
    let mut drop = vec![false; chars.len()];
    let mut w = 0;
    while w < words.len() {
        if !small(words[w]) {
            w += 1;
            continue;
        }
        // The run: small words, one or two spaces apart.
        let mut end = w + 1;
        while end < words.len()
            && small(words[end])
            && (1..=2).contains(&(words[end].0 - words[end - 1].1))
        {
            end += 1;
        }
        // A last "word" of one capital after a double space is not a tracked
        // word: `F C F C  S` is a label beside a letter, as the row rule
        // reads it (codex, #1974).
        while end > w + 1 {
            let last_double = (w + 1..end)
                .rev()
                .find(|&i| words[i].0 - words[i - 1].1 >= 2);
            match last_double {
                Some(at)
                    if words[at..end]
                        .iter()
                        .map(|&(a, b)| chars[a..b].iter().filter(|c| c.is_uppercase()).count())
                        .sum::<usize>()
                        < 2 =>
                {
                    end = at;
                }
                _ => break,
            }
        }
        let run = &words[w..end];
        let singles = run
            .iter()
            .filter(|&&(a, b)| b - a == 1 && chars[a].is_uppercase())
            .count();
        if singles >= 4 && run.len() >= 4 {
            let seps: Vec<(usize, usize)> = run.windows(2).map(|p| (p[0].1, p[1].0)).collect();
            // Only where the page marks its word boundaries with a double
            // space. Widths and gaps were tried and read figure labels and
            // OCR noise (`B C B C`, `I I I`) as tracked words.
            let boundary: Option<Vec<bool>> = seps
                .iter()
                .any(|(a, b)| b - a >= 2)
                .then(|| seps.iter().map(|(a, b)| b - a >= 2).collect());
            if let Some(boundary) = boundary {
                for (&(a, b), keep) in seps.iter().zip(boundary) {
                    // A boundary keeps its first space; everything else goes.
                    let from = if keep { a + 1 } else { a };
                    for d in drop.iter_mut().take(b).skip(from) {
                        *d = true;
                    }
                }
            }
        }
        w = end;
    }
    if !drop.contains(&true) {
        return;
    }
    span.text = chars
        .iter()
        .zip(&drop)
        .filter(|(_, d)| !**d)
        .map(|(c, _)| *c)
        .collect();
    if aligned {
        let mut k = 0;
        span.char_bounds.retain(|_| {
            k += 1;
            !drop[k - 1]
        });
    }
    if tight {
        let mut k = 0;
        span.tight_char_bounds.retain(|_| {
            k += 1;
            !drop[k - 1]
        });
    }
    // The rich-geometry vectors run beside the tight boxes, glyph for glyph.
    if span.glyph_advances.len() == chars.len() {
        let mut k = 0;
        span.glyph_advances.retain(|_| {
            k += 1;
            !drop[k - 1]
        });
    }
    if span.glyph_bounds_sources.len() == chars.len() {
        let mut k = 0;
        span.glyph_bounds_sources.retain(|_| {
            k += 1;
            !drop[k - 1]
        });
        // A space has no outline and marks its span an estimate; with the
        // spaces gone the span is what its letters are (codex, #1974).
        span.bounds_source = aggregate_bounds_source(&span.glyph_bounds_sources);
    }
}

/// The direction most of a set of spans' glyphs are drawn in, or `None` for no
/// spans at all.
///
/// Weighted by glyphs and not by spans: one word of upright text is not the
/// page's direction because a rotated run happened to arrive as three spans.
/// And by GLYPHS and not by characters, which are not the same count: a
/// ligature decodes to two or three of them, and the spaces the space consensus
/// puts between two glyphs are characters that were never drawn. Either would
/// let a minority direction outvote the page's own on a page where the two are
/// close. `char_bounds` holds one entry per glyph drawn; the character count
/// answers only for a span that carries no bounds, which is every hand-built
/// one (#435, review round 2).
fn dominant_direction<'a>(spans: impl Iterator<Item = &'a TextSpan>) -> Option<Direction> {
    heaviest_direction(spans).map(|(direction, _)| direction)
}

/// What one span weighs in a vote about direction: the glyphs it drew, or its
/// characters when it carries no bounds. One definition, because two of them
/// would let the same page have two majorities.
fn glyph_weight(span: &TextSpan) -> usize {
    if span.char_bounds.is_empty() {
        span.text.chars().count().max(1)
    } else {
        span.char_bounds.len()
    }
}

/// Whether a span was drawn by a proper rotation rather than a reflection.
///
/// A baseline vector cannot tell the two apart. `[-1, 0, 0, 1, e, f]` mirrors
/// the page about its vertical axis and leaves the baseline running in `(-1,
/// 0)` -- the same direction a half turn produces -- while the page's vertical
/// axis stays upright and nothing has turned. The determinant is what separates
/// them: it is positive for a rotation and negative for a reflection.
///
/// A span with no transform is hand-built and reads upright, which is a proper
/// rotation by the identity.
fn is_a_proper_rotation(span: &TextSpan) -> bool {
    match span.transform {
        None => true,
        Some([a, b, c, d, _, _]) => a * d - b * c > 0.0,
    }
}

/// The heaviest direction and the share of the glyphs drawn in it, weighed by
/// the rule [`dominant_direction`] documents.
fn heaviest_direction<'a>(spans: impl Iterator<Item = &'a TextSpan>) -> Option<(Direction, f64)> {
    let mut directions: Vec<(Direction, usize)> = Vec::new();
    for span in spans {
        let direction = span_direction(span);
        let weight = glyph_weight(span);
        match directions
            .iter_mut()
            .find(|(known, _)| directions_agree(*known, direction))
        {
            Some(entry) => entry.1 += weight,
            None => directions.push((direction, weight)),
        }
    }
    let total: usize = directions.iter().map(|(_, weight)| *weight).sum();
    // The first of the heaviest, so that a page whose two directions carry the
    // same number of glyphs reads in the one it was drawn in first rather than
    // in whichever the iterator saw last.
    let mut winner: Option<(Direction, usize)> = None;
    for (direction, weight) in directions {
        if winner.map(|(_, best)| weight > best).unwrap_or(true) {
            winner = Some((direction, weight));
        }
    }
    winner.map(|(direction, weight)| (direction, weight as f64 / total as f64))
}

/// Assemble cross-direction spans into lines along their own baselines.
///
/// The rule the page's bands follow, in each run's own frame: spans within the
/// band tolerance of each other across the baseline are one line, ordered along
/// it. A line is placed where the page's order held the first of its spans, so
/// a watermark drawn across the middle of a page is read there and not after
/// the footer.
fn lines_along_their_own_baselines(spans: Vec<PlacedSpan>) -> Vec<(usize, TextBlock)> {
    // One group per direction: two runs at two angles never join, however close
    // their glyphs land.
    let mut groups: Vec<(Direction, Vec<PlacedSpan>)> = Vec::new();
    for (at, span) in spans {
        let direction = span_direction(&span);
        match groups
            .iter_mut()
            .find(|(known, _)| directions_agree(*known, direction))
        {
            Some((_, group)) => group.push((at, span)),
            None => groups.push((direction, vec![(at, span)])),
        }
    }

    let mut lines: Vec<(usize, TextBlock)> = Vec::new();
    for (direction, group) in groups {
        let mut rows: Vec<(f64, usize, Vec<TextSpan>)> = Vec::new();
        for (at, span) in group {
            let across = baseline_coordinates(&span, direction).1;
            let tolerance = (span.height * BAND_Y_FRACTION).max(BAND_Y_TOLERANCE);
            match rows
                .iter_mut()
                .find(|(row_across, _, _)| (row_across - across).abs() <= tolerance)
            {
                Some((_, first, row)) => {
                    *first = (*first).min(at);
                    row.push(span);
                }
                None => rows.push((across, at, vec![span])),
            }
        }

        // Read in the order the page's own order first held them, and where
        // two lines start at the same place -- which is what a block of
        // rotated text does -- from the top of the run's own frame down.
        rows.sort_by(|a, b| {
            a.1.cmp(&b.1)
                .then_with(|| b.0.partial_cmp(&a.0).unwrap_or(Ordering::Equal))
        });
        for (_, at, mut row) in rows {
            row.sort_by(|a, b| {
                baseline_coordinates(a, direction)
                    .0
                    .partial_cmp(&baseline_coordinates(b, direction).0)
                    .unwrap_or(Ordering::Equal)
            });
            lines.push((at, TextBlock { spans: row }));
        }
    }
    // Across directions too: the page's order decides, and a stable sort keeps
    // each direction's own line order inside it.
    lines.sort_by(|a, b| a.0.cmp(&b.0));
    lines
}

/// How far apart, in ems of the previous span's size, two quarter-turned
/// spans may stand along their shared baseline and still be glyphs of one
/// run (#574).
///
/// A word space is a fraction of an em; a tracked display line, and a word gap
/// the producer positioned rather than drew, run wider, to about an em. Two
/// ems is past every within-run gap the tuning pages show and short of the
/// distance between two runs that share a baseline -- the axis labels of a
/// chart, the row labels of a stub -- which stand several ems apart or they
/// would read as one line.
const ROTATED_RUN_MAX_GAP_EM: f64 = 2.0;

/// How wide a gap along a turned baseline must be before the run it sits in
/// is merged for the cut's sake (#574).
///
/// The cut parts a run at a gap on the paragraph-break scale: two words
/// 1.2 em apart come back two blocks, reversed for a run that climbs the
/// page. A gap of an em or less never does -- the marginal runs that stand
/// alone in a margin are drawn one text matrix with uniform advances, a word
/// gap the width of a glyph -- so a run whose widest gap is an em, standing
/// alone outside any table, is left as drawn: #435 already reads it forwards,
/// and merging it buys nothing downstream can use.
const LONE_RUN_MAX_GAP_EM: f64 = 1.0;

/// One minority run drawn turned on an upright page, as one span, before the
/// cut ever sees it (#574).
///
/// The producers of the defect's pages draw a turned run one glyph per show
/// operation, so the device keeps every glyph a span of its own: the page
/// space adjacency it merges on never holds across a baseline that stands up
/// the page. What #435 assembles afterwards is then unmade twice. The cut
/// reads a word gap of the run as a paragraph break and parts the run into
/// regions, which the page's top-to-bottom order reads backwards for a run
/// that climbs it (`rate interest average Weighted`); and the table grid reads
/// a stack of turned heads as rows of one-glyph cells, which the stacked split
/// then writes out a letter at a time (`T R O P E R L A U N N A`).
///
/// So each quarter-turned run is one text item before any grid is built:
/// consecutive quarter-turned spans that share an orientation and a baseline,
/// within a word gap of each other along it, ordered by their coordinate along
/// the baseline. One span cannot be cut in two, banded apart, or gridded a
/// glyph at a time, and what the grid and the cells then read is the run whole
/// and forwards.
///
/// The text is assembled by the same code that assembles a line of prose --
/// the word spacing, the overprint dedupe, the tracking -- so a run this joins
/// reads byte for byte as #435 reads it where the cut leaves it alone. Each
/// guard below names what it keeps unchanged:
///
/// * only quarter-turned spans are candidates ([`span_is_quarter_turned`]:
///   within ten degrees of 90, 180 or 270 degrees), so upright text, small
///   skews and free-angle regions come back untouched -- the defect class is
///   quarter turns only (six reversed runs, all at 90 degrees), and a
///   free-angle region already reads forwards (OTC_STKAF p-c3c8);
/// * its callers pass only upright pages: in the frame of a turned page the
///   minority reads turned, and merging it re-decides a landscape table;
/// * a style change still ends the run, the way it ends a span in the device,
///   so a bold word inside a quarter-turned run keeps its face;
/// * a gap wider than any word gap ends the run, so two runs that share a
///   baseline stay two;
/// * a gap another quarter-turned run's glyph starts in ends the run too, so
///   an axis label's capitals never join across the small digits set between
///   them;
/// * a quarter-turned span with no partner on its baseline is left exactly as
///   it was.
///
/// A group merges only when the layout would otherwise break it: a gap wide
/// enough to part it at the cut, or a sibling quarter-turned run on its row
/// the grid would read it with. A run standing lone and tight already reads
/// forwards through #435, and merging it only changes the shape downstream
/// counts -- thirteen glyph spans read as thirteen lines, one span as one,
/// which on one proxy page carries a marginal run to heading level and
/// re-decides the page's tables around it. Lone and tight stays as drawn,
/// everywhere, including a run held inside a table region's rectangle that
/// is margin furniture and not table ink.
/// A lone run inside a table region is the accepted residual: the grid slices
/// only rows of two or more, so a single turned line beside upright text is
/// thin and refused, as it is without this merge; no tuning page shows
/// otherwise.
fn merge_rotated_runs(spans: Vec<TextSpan>) -> Vec<TextSpan> {
    if !spans.iter().any(span_is_quarter_turned) {
        return spans;
    }
    /// A baseline of quarter-turned text taking shape: the direction its
    /// members share, the across-coordinate of its first span, and its members
    /// in draw order.
    struct Baseline {
        direction: Direction,
        across: f64,
        members: Vec<usize>,
    }
    let mut baselines: Vec<Baseline> = Vec::new();
    'spans: for (index, span) in spans.iter().enumerate() {
        if !span_is_quarter_turned(span) {
            continue;
        }
        let direction = span_direction(span);
        let across = baseline_coordinates(span, direction).1;
        // The band tolerance #435 groups a run's own lines by, asked of the
        // span joining: sub-point drift in the matrices still joins, and the
        // next column's head never does.
        let tolerance = (span.height * BAND_Y_FRACTION).max(BAND_Y_TOLERANCE);
        for baseline in &mut baselines {
            if !directions_agree(baseline.direction, direction)
                || (baseline.across - across).abs() > tolerance
            {
                continue;
            }
            let first = &spans[baseline.members[0]];
            if first.font_name != span.font_name
                || first.is_bold != span.is_bold
                || first.is_italic != span.is_italic
                || first.color != span.color
            {
                continue;
            }
            baseline.members.push(index);
            continue 'spans;
        }
        baselines.push(Baseline {
            direction,
            across,
            members: vec![index],
        });
    }

    // A baseline in pieces where another quarter-turned run stands inside
    // it: the members of one run are consecutive along it, and a gap another
    // run's glyph starts in is not a word gap of this one. A chart's axis label
    // reads `W`, `1`, `0`, `Omega`, `Delta` down one baseline with the two
    // small digits set in another face between the capitals; joining the
    // capitals across the digits parts the label where nothing parts it (#574:
    // the label comes back `01` in one block and `Delta Omega W` in another).
    // The label's own tight pairs stay whole -- intervention only ever splits.
    let mut split: Vec<Baseline> = Vec::with_capacity(baselines.len());
    for baseline in baselines {
        let mut members = baseline.members;
        members.sort_by(|a, b| {
            baseline_coordinates(&spans[*a], baseline.direction)
                .0
                .partial_cmp(&baseline_coordinates(&spans[*b], baseline.direction).0)
                .unwrap_or(Ordering::Equal)
        });
        let mut piece = vec![members[0]];
        for pair in members.windows(2) {
            let (prev, curr) = (&spans[pair[0]], &spans[pair[1]]);
            let end = baseline_end(prev, baseline.direction);
            let start = baseline_coordinates(curr, baseline.direction).0;
            let intervened = (start > end)
                && spans.iter().enumerate().any(|(index, span)| {
                    !members.contains(&index)
                        && span_is_quarter_turned(span)
                        && directions_agree(span_direction(span), baseline.direction)
                        && {
                            let (along, across) = baseline_coordinates(span, baseline.direction);
                            let tolerance = (span.height * BAND_Y_FRACTION).max(BAND_Y_TOLERANCE);
                            along > end
                                && along < start
                                && (baseline.across - across).abs() <= tolerance
                        }
                });
            let size = prev.font_size.max(f64::MIN_POSITIVE);
            if start - end > ROTATED_RUN_MAX_GAP_EM * size || intervened {
                split.push(Baseline {
                    direction: baseline.direction,
                    across: baseline.across,
                    members: std::mem::take(&mut piece),
                });
            }
            piece.push(pair[1]);
        }
        split.push(Baseline {
            direction: baseline.direction,
            across: baseline.across,
            members: piece,
        });
    }
    let mut baselines = split;

    // Keep the groups the layout would break and let the rest through as
    // drawn.
    {
        // Each group's reading-first baseline and size, for the sibling test
        // below. Reading-first is where the grid would row the run: the
        // bottom of a run that climbs the page, the top of one that falls.
        struct Seat {
            baseline: f64,
            size: f64,
        }
        let seats: Vec<Seat> = baselines
            .iter()
            .map(|baseline| {
                let first = baseline
                    .members
                    .iter()
                    .min_by(|a, b| {
                        baseline_coordinates(&spans[**a], baseline.direction)
                            .0
                            .partial_cmp(&baseline_coordinates(&spans[**b], baseline.direction).0)
                            .unwrap_or(Ordering::Equal)
                    })
                    .copied()
                    .unwrap_or(0);
                Seat {
                    baseline: spans[first].y,
                    size: baseline
                        .members
                        .iter()
                        .map(|i| spans[*i].font_size)
                        .fold(0.0, f64::max),
                }
            })
            .collect();
        let mut keep = vec![false; baselines.len()];
        for (ordinal, baseline) in baselines.iter().enumerate() {
            // A gap on the paragraph-break scale parts the run at the cut,
            // and the pieces come back in page order, backwards for a run
            // that climbs the page. Past two ems the gap splits the baseline
            // into runs below instead, so it is not this trigger: stacked
            // labels part where they should.
            let mut members = baseline.members.clone();
            members.sort_by(|a, b| {
                baseline_coordinates(&spans[*a], baseline.direction)
                    .0
                    .partial_cmp(&baseline_coordinates(&spans[*b], baseline.direction).0)
                    .unwrap_or(Ordering::Equal)
            });
            let mut wide = false;
            for pair in members.windows(2) {
                let (prev, curr) = (&spans[pair[0]], &spans[pair[1]]);
                let gap = baseline_coordinates(curr, baseline.direction).0
                    - baseline_end(prev, baseline.direction);
                let size = prev.font_size.max(f64::MIN_POSITIVE);
                if gap > LONE_RUN_MAX_GAP_EM * size && gap <= ROTATED_RUN_MAX_GAP_EM * size {
                    wide = true;
                    break;
                }
            }
            if wide {
                keep[ordinal] = true;
                continue;
            }
            // A sibling quarter-turned run on the same row is the row the grid
            // would read them as, one-glyph cells across: side-by-side heads
            // share a reading-first baseline to the point. Parts of one run split
            // by a style change never do -- consecutive parts start at least
            // an advance apart, past the half em below -- nor do stacked
            // labels or runs that read opposite ways, so the lone and the
            // stacked stay as drawn.
            keep[ordinal] = seats.iter().enumerate().any(|(other, seat)| {
                other != ordinal
                    && (seats[ordinal].baseline - seat.baseline).abs()
                        <= 0.5 * seats[ordinal].size.max(seat.size)
            });
        }
        let mut kept: Vec<Baseline> = Vec::with_capacity(baselines.len());
        for (baseline, keep) in baselines.into_iter().zip(keep) {
            if keep {
                kept.push(baseline);
            }
        }
        baselines = kept;
        if baselines.is_empty() {
            return spans;
        }
    }

    // Each baseline along itself, in reading order, split into runs where the
    // white is wider than any word gap.
    let mut runs: Vec<Vec<usize>> = Vec::new();
    for baseline in &baselines {
        let mut members = baseline.members.clone();
        members.sort_by(|a, b| {
            baseline_coordinates(&spans[*a], baseline.direction)
                .0
                .partial_cmp(&baseline_coordinates(&spans[*b], baseline.direction).0)
                .unwrap_or(Ordering::Equal)
        });
        let mut run = vec![members[0]];
        for pair in members.windows(2) {
            let (prev, curr) = (&spans[pair[0]], &spans[pair[1]]);
            let gap = baseline_coordinates(curr, baseline.direction).0
                - baseline_end(prev, baseline.direction);
            if gap > ROTATED_RUN_MAX_GAP_EM * prev.font_size.max(f64::MIN_POSITIVE) {
                runs.push(std::mem::take(&mut run));
            }
            run.push(pair[1]);
        }
        runs.push(run);
    }

    // Each run of two or more as one span, emitted where its first-drawn
    // member stood; a quarter-turned span with no partner keeps its own place.
    let mut replacement: Vec<Option<TextSpan>> = (0..spans.len()).map(|_| None).collect();
    let mut folded = vec![false; spans.len()];
    for run in runs.iter().filter(|run| run.len() >= 2) {
        let members: Vec<TextSpan> = run.iter().map(|i| spans[*i].clone()).collect();
        // The run as #435 would assemble it: its spans in reading order
        // through the line assembly, so the string is the one the page holds.
        // The bounds below follow the same order the text was joined in.
        let text = TextBlock {
            spans: members.clone(),
        }
        .visual_text();
        let reading_first = &members[0];
        let x = members
            .iter()
            .map(|span| span.x)
            .fold(f64::INFINITY, f64::min);
        let y = members
            .iter()
            .map(|span| span.y)
            .fold(f64::INFINITY, f64::min);
        let right = members
            .iter()
            .map(|span| span.measured_right())
            .fold(f64::NEG_INFINITY, f64::max);
        // Glyph-scale, and deliberately not the envelope: every reader of
        // `height` -- the band tolerance, the row pitch -- expects one line's
        // worth, and the envelope lives in the bounds, where the boxes are
        // actually read from.
        let height = members.iter().map(|span| span.height).fold(0.0, f64::max);
        let font_size = members
            .iter()
            .map(|span| span.font_size)
            .fold(0.0, f64::max);
        let mut char_bounds = Vec::new();
        let mut tight_char_bounds = Vec::new();
        let mut glyph_advances = Vec::new();
        let mut glyph_bounds_sources = Vec::new();
        for span in &members {
            char_bounds.extend(span.char_bounds.iter().copied());
            tight_char_bounds.extend(span.tight_char_bounds.iter().copied());
            glyph_advances.extend(span.glyph_advances.iter().copied());
            glyph_bounds_sources.extend(span.glyph_bounds_sources.iter().copied());
        }
        let merged = TextSpan {
            text,
            x,
            y,
            width: (right - x).max(0.0),
            height,
            font_size,
            font_name: reading_first.font_name.clone(),
            is_bold: reading_first.is_bold,
            is_italic: reading_first.is_italic,
            color: reading_first.color,
            // An estimate anywhere in the run, like the device's own merge.
            width_source: if members
                .iter()
                .any(|span| span.width_source == WidthSource::Estimate)
            {
                WidthSource::Estimate
            } else {
                WidthSource::Metric
            },
            char_bounds,
            geometry_mode: reading_first.geometry_mode,
            bounds_source: aggregate_bounds_source(&glyph_bounds_sources),
            tight_char_bounds,
            glyph_advances,
            glyph_bounds_sources,
            transform: reading_first.transform,
            font_weight: reading_first.font_weight,
            is_serif: reading_first.is_serif,
            is_ocr_layer: members.iter().any(|span| span.is_ocr_layer),
            is_monospace: reading_first.is_monospace,
            render_mode: reading_first.render_mode,
            font_metrics: reading_first.font_metrics,
            // The merged span ends where its last-drawn member does (#643).
            complete_run: members.last().is_some_and(|span| span.complete_run),
        };
        let first_drawn = run.iter().copied().min().unwrap_or(0);
        for index in run {
            folded[*index] = *index != first_drawn;
        }
        replacement[first_drawn] = Some(merged);
    }

    let mut out: Vec<TextSpan> = Vec::with_capacity(spans.len());
    for (index, span) in spans.into_iter().enumerate() {
        if folded[index] {
            continue;
        }
        match replacement[index].take() {
            Some(merged) => out.push(merged),
            None => out.push(span),
        }
    }
    out
}

fn xy_cut_recursive(
    spans: Vec<TextSpan>,
    depth: usize,
    stats: &PageStats,
    rules: &[HorizontalRule],
) -> Vec<Vec<TextBlock>> {
    if spans.is_empty() {
        return Vec::new();
    }
    if depth >= XY_CUT_MAX_DEPTH {
        return vec![band_based_blocks(spans, stats, rules)];
    }

    // ANN[r17/TEX2] Pick whichever direction has the largest qualifying
    // gap. Always cutting vertically first breaks layouts where a
    // footer sits in the mid-x range — it would attach to the left
    // column instead of being recognized as a page-level zone. The
    // "largest gap wins" rule is the standard XY-Cut tie-breaker used
    // by academic OCR literature and matches pdf_oxide.
    let vcut = try_vertical_cut(&spans, stats, rules);
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
            out.extend(xy_cut_recursive(group, depth + 1, stats, rules));
        }
        return out;
    }

    vec![band_based_blocks(spans, stats, rules)]
}

/// Emit per-band row blocks without any column detection. Used as the
/// leaf of XY-Cut recursion — at this point the region either has no
/// further cuts or the density guard refused them.
fn band_based_blocks(
    spans: Vec<TextSpan>,
    stats: &PageStats,
    rules: &[HorizontalRule],
) -> Vec<TextBlock> {
    // XY-Cut can miss recurring gutters when a small number of bands span the
    // full page width (e.g. a running header above a 3-column body). In that
    // case, fall back to the older band/gutter detector inside the leaf region
    // instead of flattening everything row-major.
    group_spans_into_blocks_legacy_with_stats(spans, stats, rules)
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

/// Whether span `at` of a band stands in a gutter without being its ink: a
/// marker hung into it (one glyph set apart, `hung`), or a word space drawn as
/// a run of its own (#574).
///
/// NYSE_MSA outdents its `t` bullets 10 pt past the left column's measure and
/// draws the space after each as a run of its own, so no x on the page is free
/// of every span and the two columns were read across, line by line. A run of
/// spaces wider than an em is not looked past: a table that fills its row with
/// spaces from cell to cell has said the row is one.
fn is_marker_or_space(hung: &HashSet<usize>, at: usize, span: &TextSpan) -> bool {
    hung.contains(&at)
        || (!span.text.chars().any(|c| !c.is_whitespace())
            && span.measured_right() - span.x <= span.font_size.max(1.0))
}

/// Whether both sides of `cut_x` are running text: the median line on each
/// side holds more than `RUNNING_LETTERS` letters.
///
/// Asked of a gutter found only by looking past markers and spaces, because
/// the cell column of a table is also a strip of white with a few spans in it
/// -- NYSE_JWN's exhibit index, `Filed herewith electronically` down the right
/// -- and a cell is short where a column's line runs on.
fn both_sides_run_on(bands: &[TextBand], cut_x: f64) -> bool {
    let mut left: Vec<usize> = Vec::new();
    let mut right: Vec<usize> = Vec::new();
    for band in bands {
        // A band with ink on a side counts on that side even when its ink
        // is all figures: a row of amounts is a row with no letters, and
        // left out it would let one long header stand for the whole side
        // (codex, #2045).
        let (mut l, mut r) = (0usize, 0usize);
        let (mut l_ink, mut r_ink) = (false, false);
        for span in &band.spans {
            let inked = span.text.chars().any(|c| !c.is_whitespace());
            // Letters, because a row of amounts is as long as a line of text
            // and says nothing a reader would call running on.
            let chars = span.text.chars().filter(|c| c.is_alphabetic()).count();
            if span.x + (span.measured_right() - span.x) * 0.5 < cut_x {
                l += chars;
                l_ink |= inked;
            } else {
                r += chars;
                r_ink |= inked;
            }
        }
        if l_ink {
            left.push(l);
        }
        if r_ink {
            right.push(r);
        }
    }
    let median = |v: &mut Vec<usize>| {
        v.sort_unstable();
        v.get(v.len() / 2).copied().unwrap_or(0)
    };
    median(&mut left) > RUNNING_LETTERS && median(&mut right) > RUNNING_LETTERS
}

/// Attempt a vertical (column) cut. Returns the span groups plus the
/// gap size (in pt) if a suitable gutter is found AND the density +
/// alignment guards accept.
///
/// ANN[r17/TEX2] Three guards together avoid false-positive columns:
///   1. `min_gap` is the MAX of (median_font, 10pt) — deliberately
///      lower than `median_font * 2` so narrow-gutter academic papers
///      (12pt gutters, common in print) are still detected. It does not
///      scale with the region: see the note on the constants above.
///   2. `columns_are_dense` rejects column splits where either side
///      has <2 spans or <8 chars/band — catches table cells.
///   3. `columns_are_band_aligned` rejects cuts where any band would
///      end up on only one side of the cut while being wider than
///      ~70% of that side's column width — catches full-width
///      paragraphs (Intro / Outro) that accidentally sit in the
///      left-column x-range.
fn try_vertical_cut(
    spans: &[TextSpan],
    stats: &PageStats,
    rules: &[HorizontalRule],
) -> Option<(Vec<Vec<TextSpan>>, f64)> {
    if spans.len() < 2 * XY_CUT_MIN_SPANS_PER_COLUMN {
        return None;
    }
    // A LINE NUMBER DRAWN INTO ITS LINE IS PEELED OFF FIRST (#1973), so that
    // the margin scan below can see it for what it is.
    let peeled = peel_line_numbers(spans);
    let spans: &[TextSpan] = &peeled;

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
    // THE PAGE'S OWN RULING LINES COME FIRST. A region the document rules into
    // rows is a table, and a table read down its columns is the defect this
    // guard exists for (#545). Asked here as well as in the leaf below because
    // a table with a wide gutter is taken by this cut and never reaches the
    // leaf at all.
    let all_bands: Vec<usize> = (0..bands.len()).collect();
    if region_is_ruled(&bands, &all_bands, rules) {
        return None;
    }
    let adaptive = compute_adaptive_column_gap(&bands);
    let floor = stats.median_font_size.max(XY_CUT_VERTICAL_GAP_FLOOR);
    let min_gap = adaptive.min(floor).max(XY_CUT_VERTICAL_GAP_FLOOR);

    // Intervals [x_left, x_right] of every span; we look for an x value
    // that is free of ALL intervals (full-height gap).
    // MEASURED EDGES ONLY. `right()` returns x + width.max(estimated_width()),
    // and the estimate is a character count times a nominal advance -- on
    // tracemonkey's abstract it overshoots the measured edge by 61pt, which is
    // wider than the gutter it then covers. A gap scan that asks "is any x free
    // of every span" cannot use an edge that is deliberately generous.
    // Unmeasured spans (width 0) keep the estimate, since a point-wide interval
    // would open a gap where there is text.
    //
    // AND THE MARGIN COLUMN IS NOT INK FOR THIS PURPOSE (#582). A statute and a
    // US patent print their line numbers down the middle of the page, INSIDE
    // the gutter, so no x is free of every span and the cut is unfindable at
    // any depth -- the page comes back read line by line across both columns.
    // `a_margin_column` is what those figures are and a word is not. The
    // intervals are taken from the bands rather than from the region's own
    // slice, because the set that names the spans to look past is indexed on
    // each band's spans and two numberings of one set of spans is how an index
    // goes wrong.
    let word_space = (adaptive / COLUMN_GAP_MEDIAN_MULTIPLIER).max(0.5);
    let (mut skipped, figures) = a_margin_column(&bands, region_width, min_gap, word_space);
    // A BULLET HUNG INTO THE GUTTER IS NOT THE GUTTER'S INK (#574). A column
    // of items outdents its markers to the left of its text, and on a page
    // set with a narrow gutter they stand in it: NYSE_MSA's `t` bullets sit
    // 10 pt from the left column's measure where the columns' text is 18 pt
    // apart, and the page was read across both columns line by line. A glyph
    // set apart from the rest of its band -- one character, then more white
    // than a word space -- is looked past here and still goes with the column
    // its midpoint falls in.
    let hung: Vec<HashSet<usize>> = bands
        .iter()
        .map(|row| {
            (0..row.spans.len())
                .filter(|&at| {
                    let span = &row.spans[at];
                    // LEADING, with white before it as well as after: the
                    // last glyph of a left-column line, `)` in a run of its
                    // own, has its line's text right against it (codex,
                    // #2041).
                    let apart_before = row.spans[..at]
                        .iter()
                        .rev()
                        .find(|prev| prev.text.chars().any(|c| !c.is_whitespace()))
                        .is_none_or(|prev| {
                            span.x - prev.measured_right()
                                >= HUNG_MARKER_GAP_EM * span.font_size.max(1.0)
                        });
                    span.text.chars().filter(|c| !c.is_whitespace()).count() == 1
                        && apart_before
                        && row.spans[at + 1..]
                            .iter()
                            .find(|next| next.text.chars().any(|c| !c.is_whitespace()))
                            .is_some_and(|next| {
                                next.x - span.measured_right()
                                    >= HUNG_MARKER_GAP_EM * span.font_size.max(1.0)
                            })
                })
                .collect()
        })
        .collect();
    let scan =
        |skipped: &[HashSet<usize>], from: usize, past_markers: bool| -> Option<(f64, f64)> {
            let mut intervals: Vec<(f64, f64)> = bands
                .iter()
                .enumerate()
                .skip(from)
                .flat_map(|(band, row)| {
                    let skipped = &skipped[band];
                    let hung = &hung[band];
                    row.spans
                        .iter()
                        .enumerate()
                        .filter(move |(at, s)| {
                            !skipped.contains(at)
                                && (!past_markers || !is_marker_or_space(hung, *at, s))
                        })
                        .map(|(_, s)| {
                            let r = if s.width > 0.0 {
                                s.measured_right()
                            } else {
                                s.right()
                            };
                            (s.x, r.max(s.x + 0.001))
                        })
                })
                .collect();
            intervals.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));
            let mut cursor = intervals.first()?.1;
            let mut best: Option<(f64, f64)> = None; // (gap_size, cut_x)
            for (left, right) in intervals.iter().skip(1) {
                if *left > cursor {
                    let gap = *left - cursor;
                    if gap >= min_gap && !matches!(best, Some((b, _)) if b >= gap) {
                        best = Some((gap, (cursor + *left) * 0.5));
                    }
                }
                cursor = cursor.max(*right);
            }
            best
        };
    // A RUNNING HEAD ACROSS THE GUTTER IS NOT THE PAGE (#1973). A US patent
    // centres `US 7,268,176 B2` over the gutter and sets its column numbers a
    // line under it, close enough to the body that no horizontal cut takes them
    // off first -- so the head alone leaves no x free, and the page was read
    // across both columns. Where the gutter is found once the top band or two
    // are left out, and those bands are a line of a few runs each, they are a
    // group of their own and read first.
    //
    // ONLY ON A PAGE THAT NUMBERS ITS LINES IN THE GUTTER, which is the page
    // this is for. Anywhere else a centred title over a table is the same
    // shape -- `SCHEDULE 3` over the two columns of a statute's schedule, a
    // heading over a reconciliation table -- and setting it aside handed the
    // table to the column cut and read it down its columns (validation,
    // uksi_20200121 0.939 -> 0.627, OTC_SDXHF 0.774 -> 0.455).
    let mut head = 0;
    let mut best_gap = scan(&skipped, 0, false);
    // Numbered BY A COLUMN OF FIGURES, as the margin scan found it: every
    // member it confirmed on its own band is a figure. The scan also looks past
    // a column of tab runs down a financial table (a 10-K, 0.641 -> 0.535) and
    // a list's markers, and one digit among `i`, `ii` is not a numbered page
    // (codex, #1991).
    let numbered = figures && skipped.iter().any(|band| !band.is_empty());
    if best_gap.is_none() && numbered {
        for drop in 1..=HEAD_BANDS_MAX.min(bands.len().saturating_sub(2)) {
            if bands[drop - 1].spans.len() > HEAD_BAND_MAX_SPANS {
                break;
            }
            if let Some(found) = scan(&skipped, drop, false) {
                best_gap = Some(found);
                head = drop;
                break;
            }
        }
    }

    // ONLY WHERE NOTHING ELSE OPENS A GUTTER is it looked for past the
    // markers and word spaces standing in it, and what it then separates has
    // to be running text on both sides -- see `is_marker_or_space`.
    let mut past_markers = false;
    if best_gap.is_none() {
        best_gap = scan(&skipped, 0, true);
        past_markers = best_gap.is_some();
    }
    let (gap_size, cut_x) = best_gap?;
    // A marker looked past belongs to the column it hangs before, so the cut
    // goes just left of the first one standing in the gap.
    let cut_x = if past_markers {
        bands
            .iter()
            .zip(&hung)
            .flat_map(|(row, hung)| {
                row.spans
                    .iter()
                    .enumerate()
                    .filter(|(at, s)| is_marker_or_space(hung, *at, s))
                    .map(|(_, s)| s.x)
                    .collect::<Vec<_>>()
            })
            .filter(|x| (x - cut_x).abs() < gap_size * 0.5)
            .fold(cut_x, f64::min)
            - 0.01
    } else {
        cut_x
    };

    // AND THE SKIP HAS TO HAVE BEEN WHAT WAS IN THE WAY. A thin column
    // somewhere ELSE on the page -- the hanging figures of a bibliography at
    // the left edge of each column, which are narrow, short and at one x on
    // every band -- is not a reason to believe in a gutter it does not stand
    // in. Where the column the scan looked past is not inside the gap that was
    // found, the scan is run again with nothing skipped and THAT cut is the
    // answer: refusing instead would take the columns off a page that has a
    // perfectly good gutter, which cost `tracemonkey.pdf` page 14 its reading
    // order, 100.0 to 74.5, where both foreign readers agree at 100.
    let margin_of = |sets: &[HashSet<usize>]| -> Vec<TextSpan> {
        bands
            .iter()
            .enumerate()
            .flat_map(|(band, row)| {
                let set = &sets[band];
                row.spans
                    .iter()
                    .enumerate()
                    .filter(move |(at, _)| set.contains(at))
                    .map(|(_, s)| s.clone())
            })
            .collect()
    };
    let mut margin = margin_of(&skipped);
    let inside = |spans: &[TextSpan], cut_x: f64, gap: f64| {
        spans
            .iter()
            .all(|s| s.x >= cut_x - gap && s.measured_right() <= cut_x + gap)
    };
    let (gap_size, cut_x) = if margin.is_empty() || inside(&margin, cut_x, gap_size) {
        (gap_size, cut_x)
    } else {
        skipped = vec![HashSet::new(); bands.len()];
        margin.clear();
        scan(&skipped, head, past_markers)?
    };
    let head_spans: Vec<TextSpan> = bands[..head].iter().flat_map(|b| b.spans.clone()).collect();
    let in_margin: HashSet<(u64, u64)> = margin
        .iter()
        .chain(&head_spans)
        .map(|s| (s.x.to_bits(), s.y.to_bits()))
        .collect();

    // Split spans around the cut. A span whose midpoint is < cut_x belongs to
    // the left group; the margin column is a group of its OWN, so the line
    // numbers come out as their own block instead of inside the running text
    // of whichever column they happened to fall on.
    let mut left_group = Vec::new();
    let mut right_group = Vec::new();
    for span in spans {
        if in_margin.contains(&(span.x.to_bits(), span.y.to_bits())) {
            continue;
        }
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
    // WITHOUT THE MARGIN COLUMN, for the reason the gap scan is run without it:
    // a band that carries a line number would otherwise reach across the cut
    // by the width of two figures and be read as a band sitting on one side.
    let aligned: Vec<TextSpan> = if in_margin.is_empty() {
        spans.to_vec()
    } else {
        spans
            .iter()
            .filter(|s| !in_margin.contains(&(s.x.to_bits(), s.y.to_bits())))
            .cloned()
            .collect()
    };
    let aligned: Vec<TextSpan> = if past_markers {
        aligned
            .into_iter()
            .filter(|s| s.text.chars().filter(|c| !c.is_whitespace()).count() > 1)
            .collect()
    } else {
        aligned
    };
    if past_markers && !both_sides_run_on(&bands[head..], cut_x) {
        return None;
    }
    if !columns_are_band_aligned(&aligned, cut_x, region_left, region_right, stats) {
        return None;
    }
    if a_column_of_labels(&bands[head..], cut_x, region_left, region_right, min_gap) {
        return None;
    }
    // THE SPANS THE SCAN SKIPPED REACH THE GUARD, and this is the caller the
    // parameter was added for. Counted as ink, a column of line numbers beside
    // the gutter puts a wide strip of white between itself and the rest of its
    // side on nearly every band, which reads as a bank's own cell boundary --
    // and the guard would refuse exactly the cut that skipping them was meant
    // to allow (#582, on the parameter #580 landed).
    if the_cut_is_a_column_of_a_table(&bands[head..], cut_x, gap_size, &skipped[head..]) {
        return None;
    }

    let mut groups = Vec::new();
    if !head_spans.is_empty() {
        groups.push(head_spans);
    }
    groups.push(left_group);
    groups.push(right_group);
    if !margin.is_empty() {
        groups.push(margin);
    }
    Some((groups, gap_size))
}

/// A line number drawn into the line beside it, split off as a span of its own
/// (#1973).
///
/// `a_margin_column` looks past a span that IS a line number. An OCR text layer
/// -- every US patent of DocLayNet's validation half set in two columns, 45 of
/// their 65 pages -- draws each right-column line as one object that opens in
/// the gutter with the line's number, so `10 example, aluminum and steel` is
/// one span from the gutter to the margin: a number and a line, which that
/// scan cannot look past, and the page was read across both columns. On the
/// rig that was one document, 9840656_AdobeOCR, and the whole of the text gap
/// to the reference: 0.340 against 0.886, 0.050 of the mean over eleven.
///
/// THE GEOMETRY DECIDES, as it does for the margin column: the head of the
/// span is one to three figures, then white, and the rest of the span starts
/// on an edge at least [`MARGIN_COLUMN_MIN_BANDS`] other spans start on --
/// the column's own left edge -- while the figures stand left of it. A line
/// that opens with a number at the column's edge (`10 people attended`) does
/// not stick out and is left whole; a figure that sticks out of a column
/// nothing else starts on is not peeled either. Whether the peeled figure is
/// then a margin column is `a_margin_column`'s question, asked as before.
fn peel_line_numbers(spans: &[TextSpan]) -> Vec<TextSpan> {
    let mut out = Vec::with_capacity(spans.len());
    for (at, span) in spans.iter().enumerate() {
        match peel_one(span, at, spans) {
            Some((figure, rest)) => {
                out.push(figure);
                out.push(rest);
            }
            None => match peel_trailing(span, at, spans) {
                Some((rest, figure)) => {
                    out.push(rest);
                    out.push(figure);
                }
                None => out.push(span.clone()),
            },
        }
    }
    out
}

fn peel_one(span: &TextSpan, at: usize, spans: &[TextSpan]) -> Option<(TextSpan, TextSpan)> {
    let digits = span.text.chars().take_while(char::is_ascii_digit).count();
    if !(1..=3).contains(&digits) || !span.text[digits..].starts_with(' ') {
        return None;
    }
    let bounds = &span.char_bounds;
    if bounds.len() <= digits {
        return None;
    }
    let size = span.font_size.max(1.0);
    let figure_right = bounds[digits - 1][2];
    let clear = size * MARGIN_COLUMN_MIN_CLEARANCE_EM;
    // The rest opens at the first glyph clear of the figures: a drawn space
    // between them is a glyph of its own and belongs to neither.
    let first = (digits..bounds.len()).find(|&k| bounds[k][0] >= figure_right + clear)?;
    let rest_x = bounds[first][0];
    let tolerance = size * PEEL_EDGE_EM;
    if span.x >= rest_x - tolerance {
        return None;
    }
    let on_edge = spans
        .iter()
        .enumerate()
        .filter(|(other, s)| *other != at && (s.x - rest_x).abs() <= tolerance)
        .count();
    if on_edge < MARGIN_COLUMN_MIN_BANDS {
        return None;
    }
    let rest_text = span.text[digits..].trim_start();
    if rest_text.is_empty() {
        return None;
    }
    let cut = |v: &[[f64; 4]], lo: usize, hi: usize| {
        if v.len() == bounds.len() {
            v[lo..hi].to_vec()
        } else {
            Vec::new()
        }
    };
    let right = span.measured_right();
    let mut figure = span.clone();
    figure.text = span.text[..digits].to_string();
    figure.width = figure_right - span.x;
    figure.char_bounds = bounds[..digits].to_vec();
    figure.tight_char_bounds = cut(&span.tight_char_bounds, 0, digits);
    let mut rest = span.clone();
    rest.text = rest_text.to_string();
    rest.x = rest_x;
    rest.width = (right - rest_x).max(0.0);
    rest.char_bounds = bounds[first..].to_vec();
    // THE ORIGIN MOVES WITH THE SPAN (codex, #1991): `transform` locates the
    // span's first glyph, and the rest's first glyph is `first`, not the
    // figure it was drawn after.
    if let Some(t) = rest.transform.as_mut() {
        t[4] += bounds[first][0] - bounds[0][0];
        t[5] += bounds[first][1] - bounds[0][1];
    }
    rest.tight_char_bounds = cut(&span.tight_char_bounds, first, bounds.len());
    // The rich-geometry vectors run glyph for glyph beside the tight boxes;
    // split where they line up, dropped where they do not.
    if span.glyph_advances.len() == bounds.len() {
        figure.glyph_advances = span.glyph_advances[..digits].to_vec();
        rest.glyph_advances = span.glyph_advances[first..].to_vec();
    } else {
        figure.glyph_advances.clear();
        rest.glyph_advances.clear();
    }
    if span.glyph_bounds_sources.len() == bounds.len() {
        figure.glyph_bounds_sources = span.glyph_bounds_sources[..digits].to_vec();
        rest.glyph_bounds_sources = span.glyph_bounds_sources[first..].to_vec();
    } else {
        figure.glyph_bounds_sources.clear();
        rest.glyph_bounds_sources.clear();
    }
    Some((figure, rest))
}

/// A line number drawn at the END of the line before it, the mirror of
/// [`peel_one`] (#574). A US patent prints its line numbers in the gutter, and
/// where a left-column line runs close to them the OCR layer's words join the
/// figure into the line's run: `market, citric acid and/or 35` (8287919)
/// ended nine points into a gutter of fourteen, and the page was read across
/// both columns line by line. Peeled the same way: one to three figures at the
/// end, white before them, and the rest ending on an edge at least
/// [`MARGIN_COLUMN_MIN_BANDS`] other spans end on -- the column's own right
/// edge -- with the figures standing right of it.
///
/// Counted from the END, because that is where the glyphs and the text agree:
/// a space the device inserts between two glyphs has no box, so the boxes and
/// the characters are out of step from the first inserted space on, but a
/// space is only ever inserted BEFORE a glyph, so whatever ends the text was
/// drawn.
fn peel_trailing(span: &TextSpan, at: usize, spans: &[TextSpan]) -> Option<(TextSpan, TextSpan)> {
    let body = span.text.trim_end();
    let tail_ws = span.text[body.len()..].chars().count();
    let digits = body.chars().rev().take_while(char::is_ascii_digit).count();
    if !(1..=3).contains(&digits) || !body[..body.len() - digits].ends_with(' ') {
        return None;
    }
    let rest_text = body[..body.len() - digits].trim_end();
    if rest_text.is_empty() {
        return None;
    }
    let bounds = &span.char_bounds;
    if bounds.len() <= tail_ws + digits {
        return None;
    }
    let figure_end = bounds.len() - tail_ws;
    let figure_start = figure_end - digits;
    let size = span.font_size.max(1.0);
    let figure_left = bounds[figure_start][0];
    let clear = size * MARGIN_COLUMN_MIN_CLEARANCE_EM;
    // The rest ends at the last glyph clear of the figures: a drawn space
    // between them reaches up to the figure and belongs to neither.
    let last = (0..figure_start)
        .rev()
        .find(|&k| bounds[k][2] <= figure_left - clear)?;
    let rest_right = bounds[last][2];
    let tolerance = size * PEEL_EDGE_EM;
    let figure_right = bounds[figure_end - 1][2];
    if figure_right <= rest_right + tolerance {
        return None;
    }
    let on_edge = spans
        .iter()
        .enumerate()
        .filter(|(other, s)| *other != at && (s.measured_right() - rest_right).abs() <= tolerance)
        .count();
    if on_edge < MARGIN_COLUMN_MIN_BANDS {
        return None;
    }
    let cut = |v: &[[f64; 4]], lo: usize, hi: usize| {
        if v.len() == bounds.len() {
            v[lo..hi].to_vec()
        } else {
            Vec::new()
        }
    };
    let mut rest = span.clone();
    rest.text = rest_text.to_string();
    rest.width = (rest_right - span.x).max(0.0);
    rest.char_bounds = bounds[..=last].to_vec();
    rest.tight_char_bounds = cut(&span.tight_char_bounds, 0, last + 1);
    let mut figure = span.clone();
    figure.text = body[body.len() - digits..].to_string();
    figure.x = figure_left;
    figure.width = (figure_right - figure_left).max(0.0);
    figure.char_bounds = bounds[figure_start..figure_end].to_vec();
    figure.tight_char_bounds = cut(&span.tight_char_bounds, figure_start, figure_end);
    // The origin moves with the figure, as it does with a peeled rest.
    if let Some(t) = figure.transform.as_mut() {
        t[4] += bounds[figure_start][0] - bounds[0][0];
        t[5] += bounds[figure_start][1] - bounds[0][1];
    }
    if span.glyph_advances.len() == bounds.len() {
        rest.glyph_advances = span.glyph_advances[..=last].to_vec();
        figure.glyph_advances = span.glyph_advances[figure_start..figure_end].to_vec();
    } else {
        rest.glyph_advances.clear();
        figure.glyph_advances.clear();
    }
    if span.glyph_bounds_sources.len() == bounds.len() {
        rest.glyph_bounds_sources = span.glyph_bounds_sources[..=last].to_vec();
        figure.glyph_bounds_sources = span.glyph_bounds_sources[figure_start..figure_end].to_vec();
    } else {
        rest.glyph_bounds_sources.clear();
        figure.glyph_bounds_sources.clear();
    }
    Some((rest, figure))
}

/// Whether a column of candidates is a count of the page's lines: every member
/// a whole number, rising strictly from the top band down, all of them multiples
/// of one step of at least two -- `5`, `10`, `15`, with the members the scan did
/// not confirm simply missing (#574).
fn counts_the_lines(column: &[(usize, usize, f64)], bands: &[TextBand]) -> bool {
    let mut members: Vec<(usize, u32)> = Vec::with_capacity(column.len());
    for &(band, at, _) in column {
        // One to three figures, as a printed line number is. Nothing earlier
        // limits a candidate's length, and a column of years rises by a common
        // step as well: `2018`, `2020`, `2022` (codex, #2056).
        let text = bands[band].spans[at].text.trim();
        if text.chars().count() > MARGIN_FIGURE_MAX_CHARS {
            return false;
        }
        match text.parse::<u32>() {
            Ok(value) => members.push((band, value)),
            Err(_) => return false,
        }
    }
    if members.len() < MARGIN_COLUMN_MIN_BANDS {
        return false;
    }
    members.sort_unstable();
    let rising = members
        .windows(2)
        .all(|pair| pair[0].0 < pair[1].0 && pair[0].1 < pair[1].1);
    let step = members.iter().fold(0, |g, &(_, v)| gcd(g, v));
    rising && step >= 2
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// How many bands at the top of a region may be set aside as its running head
/// when they are what fills the gutter (#1973): the head and the line of
/// column numbers a patent sets under it.
const HEAD_BANDS_MAX: usize = 2;

/// The most runs a band set aside as a head may hold. A running head is a line
/// or two of a few runs; a band of more is a line of the page's own text, and
/// a region whose own first line crosses the gutter is not two columns.
const HEAD_BAND_MAX_SPANS: usize = 3;

/// How near an edge a span has to start to be on it, in ems of the peeled
/// span's type: half an em, well inside a gutter and well over the jitter of
/// an OCR layer's left edges (a fifth of a point on the pages it was built for).
const PEEL_EDGE_EM: f64 = 0.5;

/// Whether a run is a figure and nothing else, as a printed line number is.
fn only_figures(text: &str) -> bool {
    let t = text.trim();
    !t.is_empty() && t.chars().all(|c| c.is_ascii_digit())
}

/// The most characters a member of a margin column may carry when it is taken
/// in by the column rather than by its own band (#1973): a line number is
/// three figures at most.
const MARGIN_FIGURE_MAX_CHARS: usize = 3;

/// How wide a margin column may be, as a fraction of the region it stands in.
///
/// A tenth. A line number is two or three figures against a measure of sixty
/// characters; a column of a table is a third of its row, and this is what
/// refuses it before any of the other conditions is asked.
const MARGIN_COLUMN_MAX_WIDTH_FRACTION: f64 = 0.10;

/// How much of its own band's text a margin column may carry.
///
/// COUNTED IN CHARACTERS AND NOT IN WIDTH, so that a proportional face does
/// not decide it: `10` beside a line of prose is two characters of sixty.
const MARGIN_COLUMN_MAX_CHAR_SHARE: f64 = 0.15;

/// How much white a margin column needs on each side, in ems of its own type.
///
/// A fifth: under a word space in any face and over the fit between letters.
const MARGIN_COLUMN_MIN_CLEARANCE_EM: f64 = 0.2;

/// How many bands a margin column has to stand on before it is one.
///
/// Three, for the reason the running-head rule of #571 asks for three pages: a
/// pair of short runs at one x is a coincidence a page cannot rule out.
const MARGIN_COLUMN_MIN_BANDS: usize = 3;

/// The spans the gap scan may look past, per band (#582).
///
/// A statute and a US patent print their line numbers down the middle of the
/// page, INSIDE the gutter. The scan asks for an x free of every span, so such
/// a page has no free x at any depth and comes back read line by line across
/// both columns -- the whole of what the three patent pages of run
/// `structured-doclaynet-20260922T081244Z` lost, 1.227 of the 4.888 the
/// interleave class cost.
///
/// FOUR CONDITIONS, AND NONE OF THEM ABOUT WHAT THE SPAN SAYS. A rule that
/// read the text would fire on a verse number and on a marginal note, and
/// would have to be taught every language's figures:
///
/// 1. WHITE ON BOTH SIDES within its own band, measured against its OWN type
///    rather than against the gutter: a margin column stands inside a gutter
///    of twelve points and has three or four either side, so a threshold taken
///    from the gutter's width would refuse every real one.
/// 2. NARROW -- at most [`MARGIN_COLUMN_MAX_WIDTH_FRACTION`] of the region.
/// 3. A NEGLIGIBLE SHARE OF ITS BAND'S CHARACTERS.
/// 4. A COLUMN, AND EXACTLY ONE: the candidates stand at nearly the same x on
///    at least [`MARGIN_COLUMN_MIN_BANDS`] bands, and where two such columns
///    are found NOTHING is skipped.
///
/// The second and the fourth are what keep a table of figures out, and they
/// were asked for rather than assumed (T2, #577/#580): every cell of a bank of
/// figures has white on both sides, so the first condition alone would make
/// the boundary INSIDE a table findable and undo what #577 took out. A cell is
/// a third of its row, which the second refuses; and a table has several such
/// columns, which the fourth refuses.
/// Returns the spans to look past, one set per band, and whether the column
/// they form is FIGURES in every member its own bands found -- the evidence a
/// caller needs before it treats the page as one that numbers its lines
/// (codex, #1991): a column of `i`, `1`, `ii` is a list's markers.
fn a_margin_column(
    bands: &[TextBand],
    region_width: f64,
    min_gap: f64,
    word_space: f64,
) -> (Vec<HashSet<usize>>, bool) {
    let mut out: Vec<HashSet<usize>> = vec![HashSet::new(); bands.len()];
    if bands.len() < MARGIN_COLUMN_MIN_BANDS || region_width <= 0.0 {
        return (out, false);
    }
    let mut candidates: Vec<(usize, usize, f64)> = Vec::new();
    for (band, row) in bands.iter().enumerate() {
        let letters: usize = row.spans.iter().map(|s| s.text.chars().count()).sum();
        if letters == 0 {
            continue;
        }
        for (at, span) in row.spans.iter().enumerate() {
            let right = span.measured_right().max(span.x);
            if right - span.x > MARGIN_COLUMN_MAX_WIDTH_FRACTION * region_width {
                continue;
            }
            if span.text.chars().count() as f64 > MARGIN_COLUMN_MAX_CHAR_SHARE * letters as f64 {
                continue;
            }
            let before = row
                .spans
                .iter()
                .filter(|other| other.measured_right() <= span.x)
                .map(TextSpan::measured_right)
                .fold(f64::NEG_INFINITY, f64::max);
            let after = row
                .spans
                .iter()
                .filter(|other| other.x >= right)
                .map(|other| other.x)
                .fold(f64::INFINITY, f64::min);
            if !before.is_finite() || !after.is_finite() {
                continue;
            }
            let clear = (span.font_size * MARGIN_COLUMN_MIN_CLEARANCE_EM).max(word_space.min(1.0));
            if span.x - before <= clear || after - right <= clear {
                continue;
            }
            candidates.push((band, at, span.x + (right - span.x) * 0.5));
        }
    }
    if candidates.len() < MARGIN_COLUMN_MIN_BANDS {
        return (out, false);
    }
    candidates.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(Ordering::Equal));
    let mut columns: Vec<Vec<(usize, usize, f64)>> = Vec::new();
    for candidate in candidates {
        match columns.last_mut() {
            Some(column) if candidate.2 - column[column.len() - 1].2 <= min_gap => {
                column.push(candidate)
            }
            _ => columns.push(vec![candidate]),
        }
    }
    let mut found: Vec<&Vec<(usize, usize, f64)>> = columns
        .iter()
        .filter(|column| {
            let mut on: Vec<usize> = column.iter().map(|c| c.0).collect();
            on.sort_unstable();
            on.dedup();
            on.len() >= MARGIN_COLUMN_MIN_BANDS
        })
        .collect();
    // EXACTLY ONE. Two of them are the banks of a table, not a margin.
    //
    // UNLESS ONE OF THEM COUNTS THE PAGE'S LINES (#574). A US patent with a
    // table on it has the table's banks AND its gutter's line numbers, and the
    // rule above refused them all, so the page was read across both columns:
    // `6420562_AdobeOCR` p. 53, a bank of `0.0`, `4.0`, `64.9` beside the
    // gutter's `5`, `10` ... `65`. What a line-number column has and a bank of
    // figures does not is a count: whole numbers rising down the page by a
    // common step. A table's row numbers rise by one and are refused by the
    // step; a column of years has four figures and is refused by its width.
    if found.len() > 1 {
        found.retain(|column| counts_the_lines(column, bands));
    }
    if found.len() != 1 {
        return (out, false);
    }
    let column = found.pop().expect("one column");
    for (band, at, _) in column.iter() {
        out[*band].insert(*at);
    }
    // AND ITS MEMBERS THAT STAND BESIDE NO LINE (#1973). A line number beside
    // the first line of a paragraph in the right column, where the left
    // column's paragraph has just ended, has white after it and nothing before
    // it -- the first clause asks for both, and the figure stayed in the
    // gutter as ink. Once the column is found by the members that do have both,
    // a figure at the same x with white after it is one of them.
    //
    // A COLUMN OF FIGURES ONLY, where every member found is one. The same shape
    // with nothing before it is also the marker of a list hung out of its
    // column -- `i`, `ii`, `iii` down the left of a reconciliation table --
    // and taken into the column those markers let the cut through the table
    // (validation, OTC_SDXHF 0.774 -> 0.455). A column of line numbers says
    // what it is in every member it has.
    if !column
        .iter()
        .all(|&(band, at, _)| only_figures(&bands[band].spans[at].text))
    {
        return (out, false);
    }
    let centre = column.iter().map(|c| c.2).sum::<f64>() / column.len() as f64;
    for (band, row) in bands.iter().enumerate() {
        let letters: usize = row.spans.iter().map(|s| s.text.chars().count()).sum();
        for (at, span) in row.spans.iter().enumerate() {
            // Three characters at most, and not necessarily figures: the OCR
            // layer that draws these reads a `5` as an `s` often enough, and a
            // column that is figures in every member it was found by has said
            // what it holds (9017715_AdobeOCR, `hormone- s dependent`).
            if out[band].contains(&at) || span.text.trim().chars().count() > MARGIN_FIGURE_MAX_CHARS
            {
                continue;
            }
            let right = span.measured_right().max(span.x);
            if (span.x + (right - span.x) * 0.5 - centre).abs() > min_gap
                || right - span.x > MARGIN_COLUMN_MAX_WIDTH_FRACTION * region_width
                || span.text.chars().count() as f64 > MARGIN_COLUMN_MAX_CHAR_SHARE * letters as f64
            {
                continue;
            }
            let after = row
                .spans
                .iter()
                .filter(|other| other.x >= right)
                .map(|other| other.x)
                .fold(f64::INFINITY, f64::min);
            let clear = (span.font_size * MARGIN_COLUMN_MIN_CLEARANCE_EM).max(word_space.min(1.0));
            // AND WITH NOTHING AFTER IT (#574): where the right column has no
            // line -- a paragraph ends, a table stands in the other column --
            // the figure has the left column's line before it and white to the
            // page's edge. White before it is then what says it stands apart.
            let before = row
                .spans
                .iter()
                .filter(|other| other.measured_right() <= span.x)
                .map(TextSpan::measured_right)
                .fold(f64::NEG_INFINITY, f64::max);
            let apart = if after.is_finite() {
                after - right > clear
            } else {
                !before.is_finite() || span.x - before > clear
            };
            if apart {
                out[band].insert(at);
            }
        }
    }
    (out, true)
}

/// Whether the gap being cut at is a boundary INSIDE a table rather than THE
/// gutter of a page set in columns.
///
/// `a_column_of_labels` above reads the table whose thin side is a column of
/// names. This is the other half of the same defect and the one shape none of
/// the guards here could see (#577): a table of figures whose two banks of
/// columns are both dense on every band. `region_is_ruled` cannot refuse it --
/// the document rules the head off and nothing else, which is one gap of eight
/// -- and `columns_are_dense` was written to catch a thin side, so a bank of
/// numbers passes it. Cut down the strip between the banks, the table comes out
/// as two tables side by side: every row of it is halved, and the figures of
/// row four stand under the figures of row one.
///
/// WHAT SEPARATES THE TWO CASES IS WHETHER THE CUT IS SPECIAL. A page set in
/// columns puts its widest strip of white between the columns, and a line of
/// prose inside one has nothing but word spaces in it -- so a side of that cut
/// holds bands with no gap anywhere near the gutter's width. A bank of a table
/// is columns of cells, so its own bands break at gaps as wide as the strip
/// between the banks or wider: the gutter that was cut at is not the widest
/// thing on the page, it is one of several the same size, and a table is what
/// that means.
///
/// BOTH SIDES, and that is the guard against the page this must not refuse. A
/// paper set in two columns with a table in ONE of them breaks the lines of
/// that column and not of the other, and its column cut is right; a run of
/// bands broken on both sides of the cut, most of the way down, is one grid.
///
/// AND A SPAN THE GAP SCAN SKIPPED IS NOT A CELL BOUNDARY. `ignored` names, per
/// band, the spans the caller left out of the scan that found this gap -- the
/// case it exists for is a thin column standing INSIDE a gutter, printed line
/// numbers down the middle of a statute or a patent, which makes every x in the
/// gutter occupied and the cut unfindable at any depth (#582). Those spans have
/// to be left out here too. Counted as ink, a column of numbers beside the
/// gutter puts a wide strip of white between itself and the rest of its side on
/// nearly every band -- which reads as a bank's own cell boundary, and this
/// guard would refuse exactly the cut that skipping them was meant to allow.
///
/// ONE SET PER BAND, parallel to `bands`, because a band's spans are numbered
/// within that band: the band that carries a line number holds three runs where
/// the band beside it holds two, so one set of indices for the whole region
/// would strike the number on one band and a column of text on the next (#582).
/// An empty slice ignores nothing, which is what `try_vertical_cut` passes; a
/// caller that skips spans builds its sets from the same banding the guard is
/// given.
fn the_cut_is_a_column_of_a_table(
    bands: &[TextBand],
    cut_x: f64,
    gap: f64,
    ignored: &[HashSet<usize>],
) -> bool {
    if gap <= 0.0 {
        return false;
    }
    // THE SAME EDGE ON BOTH SIDES OF THE DIVISION. `region_left` and
    // `region_right` are taken from `TextSpan::right()`, which is the greater of
    // the measured width and an estimate from the character count -- and on
    // prose that estimate overshoots the glyphs by tens of points. The extent
    // below is measured ink. Dividing one by the other inflates the denominator
    // and understates the fill, which is how a page of prose could be read as a
    // bank of cells and a legitimate column cut refused (codex, #577). So the
    // measure is the measured extent of the bands themselves.
    //
    // THE EXTENT IS OF THE BANDS AS THE GUARD SEES THEM, so `ignored` reaches it
    // too: a line-number column left out of the scan must not set the edge that
    // the fill is divided by, or the side it stands on reads as emptier than it is.
    let (ink_left, ink_right) = measured_extent(bands, ignored);
    let left = one_side_of_a_cut(
        bands,
        cut_x,
        true,
        (cut_x - ink_left).max(0.0),
        gap,
        ignored,
    );
    let right = one_side_of_a_cut(
        bands,
        cut_x,
        false,
        (ink_right - cut_x).max(0.0),
        gap,
        ignored,
    );
    let (left, right) = match (left, right) {
        (Some(left), Some(right)) => (left, right),
        _ => return false,
    };
    let banks = |side: &SideOfACut| (side.broken as f64) > BANK_BREAK_SHARE * side.inked as f64;
    if banks(&left) && banks(&right) {
        return true;
    }
    // ONE SIDE OF CELLS IS ENOUGH WHERE THE OTHER SIDE IS CELLS TOO. The other
    // half of this shape is a table whose first column is one cell a row --
    // names, dates, a label -- against a bank of figures. That side breaks
    // nowhere, because one cell cannot have a gap in it, so the test above sees
    // one bank and lets the cut through; `a_column_of_labels` does not refuse it
    // either, because it asks the thin side to have ink on far fewer bands than
    // the wide one and here both sides have ink on nearly all of them (#577).
    //
    // What decides is the measurement `region_is_a_grid` is built on: a line of
    // prose fills its measure and a cell does not. So a side that breaks
    // nowhere is still a column OF the table while its typical band leaves
    // white beside it -- and the page this must not refuse, a paper set in two
    // columns with a table in one of them, has prose on the other side, filling
    // it.
    (banks(&left) && right.fill <= GRID_SLICE_FILL)
        || (banks(&right) && left.fill <= GRID_SLICE_FILL)
}

/// The measured ink of a set of bands, left and right.
///
/// `measured_right` rather than `right`, because the fill this feeds is computed
/// from measured ink and a measure taken from the estimate would not be the same
/// question (codex, #577).
///
/// `ignored` is the same per-band set the guard is given: a span left out of the
/// gap scan is left out of the extent too, or the edge it sets would be divided
/// into a fill it does not appear in (#582). An empty slice ignores nothing.
fn measured_extent(bands: &[TextBand], ignored: &[HashSet<usize>]) -> (f64, f64) {
    let mut left = f64::INFINITY;
    let mut right = f64::NEG_INFINITY;
    for (b, band) in bands.iter().enumerate() {
        let skip = ignored.get(b);
        for (i, span) in band.spans.iter().enumerate() {
            if skip.is_some_and(|set| set.contains(&i)) {
                continue;
            }
            left = left.min(span.x);
            right = right.max(span.measured_right());
        }
    }
    if left.is_finite() && right > left {
        (left, right)
    } else {
        (0.0, 0.0)
    }
}

/// One side of a proposed cut: how many bands have ink on it, how many of them
/// break at a gap as wide as the cut's, and how much of the side its typical
/// band covers.
struct SideOfACut {
    inked: usize,
    broken: usize,
    fill: f64,
}

/// `None` where the side has ink on too few bands to say anything.
fn one_side_of_a_cut(
    bands: &[TextBand],
    cut_x: f64,
    is_left: bool,
    measure: f64,
    gap: f64,
    ignored: &[HashSet<usize>],
) -> Option<SideOfACut> {
    if measure <= 0.0 {
        return None;
    }
    let mut inked = 0usize;
    let mut broken = 0usize;
    let mut fills: Vec<f64> = Vec::new();
    for (at, band) in bands.iter().enumerate() {
        let skipped = ignored.get(at);
        let mut ink: Vec<(f64, f64)> = band
            .spans
            .iter()
            .enumerate()
            .filter(|(span, _)| skipped.is_none_or(|set| !set.contains(span)))
            .map(|(_, span)| span)
            .filter(|span| {
                let middle = span.x + (span.measured_right() - span.x) * 0.5;
                (middle < cut_x) == is_left
            })
            .map(|span| (span.x, span.measured_right()))
            .collect();
        if ink.is_empty() {
            continue;
        }
        inked += 1;
        ink.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));
        // The widest strip of white INSIDE this side of the band. A cell
        // boundary of a bank, where the side is a bank; a word space, where it
        // is a line of prose.
        let widest = ink
            .windows(2)
            .map(|pair| pair[1].0 - pair[0].1)
            .fold(0.0_f64, f64::max);
        if widest >= gap {
            broken += 1;
        }
        let from = ink.iter().map(|pair| pair.0).fold(f64::INFINITY, f64::min);
        let to = ink
            .iter()
            .map(|pair| pair.1)
            .fold(f64::NEG_INFINITY, f64::max);
        if to > from {
            fills.push(((to - from) / measure).min(1.0));
        }
    }
    if inked < BANK_MIN_ROWS || fills.is_empty() {
        return None;
    }
    fills.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    Some(SideOfACut {
        inked,
        broken,
        fill: fills[fills.len() / 2],
    })
}

/// Whether the thin side of the cut is a column of LABELS for the other side:
/// a table whose cells hold sentences (#576).
///
/// `region_is_ruled` above says the same thing about a table the document drew
/// lines round. This is the commoner table -- no lines, a narrow first column
/// of names and a wide one of descriptions -- and cut down its gutter every
/// row of it is lost: the names come out as one paragraph, the descriptions as
/// another, and nothing says which name went with which description.
///
/// THE MEASUREMENT IS WHERE THE THIN SIDE'S INK SITS. A label stands against
/// the START of what it labels, so every band the thin side has ink on is a
/// band where the wide side BEGINS something: the wide side's step into that
/// band is wider than the leading it sets its own lines at. A note in the
/// margin -- the case this must not refuse -- falls where it falls, in the
/// middle of the running text beside it, and its bands are ordinary lines of
/// that text.
///
/// Every band, not most: the thin side is a handful of bands, and a share over
/// a handful is a coin toss. One label in the middle of a paragraph and this
/// is not a table.
fn a_column_of_labels(
    bands: &[TextBand],
    cut_x: f64,
    region_left: f64,
    region_right: f64,
    min_gap: f64,
) -> bool {
    fn on(band: &TextBand, left: bool, cut_x: f64) -> Vec<&TextSpan> {
        band.spans
            .iter()
            .filter(|span| {
                let middle = span.x + (span.measured_right() - span.x) * 0.5;
                (middle < cut_x) == left
            })
            .collect()
    }
    let inked = |left: bool| -> Vec<usize> {
        (0..bands.len())
            .filter(|at| !on(&bands[*at], left, cut_x).is_empty())
            .collect()
    };
    let (thin, wide, thin_is_left) = {
        let (l, r) = (inked(true), inked(false));
        if l.len() <= r.len() {
            (l, r, true)
        } else {
            (r, l, false)
        }
    };
    if thin.len() < LABEL_MIN_ROWS || thin.len() * 4 > wide.len() * 3 {
        return false;
    }

    // AND A LABEL IS SHORT. The other way a side has ink on few bands is that
    // it is the shorter column of a page set in columns -- one that ends
    // higher than its neighbour -- and its lines fill their measure to the
    // last word, which is what a line of prose does and a cell does not. So
    // the typical line of the thin side has to leave white beside it before
    // any of this is read as a label at all.
    let measure = if thin_is_left {
        cut_x - region_left
    } else {
        region_right - cut_x
    };
    if measure <= 0.0 {
        return false;
    }
    // AND A LABEL IS ONE CELL: a thin side whose own bands break into several
    // runs of ink is not a column at all but more columns, which is what a
    // page of glyph samples laid out in a grid looks like from here.
    let mut fills: Vec<f64> = Vec::with_capacity(thin.len());
    for at in &thin {
        let slice = on(&bands[*at], thin_is_left, cut_x);
        let broken = slice
            .windows(2)
            .any(|pair| pair[1].x - pair[0].measured_right() >= min_gap);
        if broken {
            return false;
        }
        let from = slice.iter().map(|s| s.x).fold(f64::INFINITY, f64::min);
        let to = slice
            .iter()
            .map(|s| s.measured_right())
            .fold(f64::NEG_INFINITY, f64::max);
        if to > from {
            fills.push(((to - from) / measure).min(1.0));
        }
    }
    if fills.is_empty() {
        return false;
    }
    fills.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    if fills[fills.len() / 2] > GRID_SLICE_FILL {
        return false;
    }

    // The wide side's own leading: the middle step between the bands it has
    // ink on. A step wider than `LABEL_PITCH_JUMP` of it is where it begins
    // something rather than carrying on.
    let mut steps: Vec<f64> = wide
        .windows(2)
        .map(|pair| bands[pair[0]].y - bands[pair[1]].y)
        .filter(|step| *step > 0.0)
        .collect();
    if steps.len() < 2 {
        return false;
    }
    steps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let leading = steps[steps.len() / 2];
    if leading <= 0.0 {
        return false;
    }

    // A LABEL MAY WRAP. `Community resources` set over two lines is one label
    // and its second line stands against nothing, so a band the thin side
    // carries on into -- one leading under a thin band of its own -- is as
    // much part of a label as the band that opened it.
    let opens_the_wide_side = |at: usize| match wide.iter().position(|w| *w >= at) {
        Some(0) => true,
        Some(where_) if wide[where_] == at => {
            bands[wide[where_ - 1]].y - bands[at].y > leading * LABEL_PITCH_JUMP
        }
        _ => false,
    };
    thin.iter().all(|at| {
        *at == 0
            || opens_the_wide_side(*at)
            || (thin.binary_search(&(at - 1)).is_ok() && bands[at - 1].y - bands[*at].y <= leading)
    })
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

    let _ = (left_width, right_width, MAX_SINGLE_SIDE_FRACTION);
    // ONE SPAN'S INK REACHING ACROSS THE CUT is what "a full-width element
    // masquerading as column content" actually looks like. The guard used to
    // ask a different question -- is a band that sits on one side wider than
    // 0.70 of that side -- and on a real two-column page the answer is yes for
    // any ordinary line of body text, because a line fills about 0.9 of its
    // column. Every such page has single-sided bands (a heading, a column that
    // ends short, a figure), so the old test refused the canonical academic
    // page every time (#429).
    //
    // Width was a proxy for crossing, and it is a proxy that cannot tell a
    // banner from a column line. Its own fixture shows why: the banner runs
    // x 40..320 against a cut at 200, which is ink on both sides. It counted as
    // "one side" only because sides were decided on each span's midpoint.
    for span in spans {
        if span.x < cut_x && span.measured_right() > cut_x {
            return false;
        }
    }

    // A LINE CANNOT BE HALF AGAIN AS WIDE AS THE TYPICAL LINE IN ITS OWN COLUMN
    // and still be a line of that column. That is the second way a full-width
    // element hides: `Intro paragraph` and `Outro paragraph` in
    // mixed_single_and_multi_column_regions_preserve_shared_bands are wider
    // than the column they sit above but still stop short of the gutter, so no
    // ink of theirs crosses the cut. Swallowed into the left column they would
    // read Intro, L1..L3, Outro, R1..R3 -- the outro before the right column it
    // precedes.
    //
    // Measured against the MEDIAN LINE OF THAT SIDE, not against the width of
    // the side. The old test used cut_x - region_left, which is a column plus
    // half a gutter, so an ordinary body line at ~0.9 of its column tripped it
    // and the canonical two-column page was refused every time (#429). The
    // median calibrates itself: on a real column most lines run full width, so
    // the median IS the column width and nothing legitimate exceeds it.
    const OUTLIER: f64 = 1.3;
    // EVERY band's per-side extent, not only the single-sided ones. Measuring
    // the typical line from the single-sided bands alone compares the outliers
    // with themselves: in the Intro/Outro fixture those two ARE the only
    // single-sided bands, so their median is their own width and nothing ever
    // stands out.
    let bands = group_spans_into_bands_with_stats(spans.to_vec(), stats);
    let mut widths: (Vec<f64>, Vec<f64>) = (Vec::new(), Vec::new());
    let mut single: Vec<(bool, f64)> = Vec::new();
    for band in &bands {
        let mut left_lo = f64::INFINITY;
        let mut left_hi = f64::NEG_INFINITY;
        let mut right_lo = f64::INFINITY;
        let mut right_hi = f64::NEG_INFINITY;
        for span in &band.spans {
            let r = span.measured_right();
            if span.x + (r - span.x) * 0.5 < cut_x {
                left_lo = left_lo.min(span.x);
                left_hi = left_hi.max(r);
            } else {
                right_lo = right_lo.min(span.x);
                right_hi = right_hi.max(r);
            }
        }
        let has_left = left_hi > left_lo || left_lo.is_finite();
        let has_right = right_hi > right_lo || right_lo.is_finite();
        if has_left {
            widths.0.push((left_hi - left_lo).max(0.0));
        }
        if has_right {
            widths.1.push((right_hi - right_lo).max(0.0));
        }
        match (has_left, has_right) {
            (true, false) => single.push((true, (left_hi - left_lo).max(0.0))),
            (false, true) => single.push((false, (right_hi - right_lo).max(0.0))),
            _ => {}
        }
    }
    // A PATENT'S FRONT PAGE: THE COLUMN'S WIDTH IS ITS LONG LINES. Its
    // bibliographic column is mostly short lines -- `(22) Filed: Nov. 8,
    // 2002` over `(58) Field of Search ..... 252/500; 528/422` -- so the
    // median stood at 152 pt where the column runs 199, the column's own
    // longest line read as a banner over both columns, and every line joined
    // its neighbour across the gutter (#574, the rig-like proxy's patents).
    // Where the page numbers its fields with INID codes -- three or more
    // lines opening `(21)`, `(75)`, `(58)` at the region's left edge -- the
    // column is measured by its
    // ninetieth percentile. Only there: taken for every page, the same
    // measure cut financial tables and scientific pages that the median
    // keeps whole (tables -0.010, geometric order -0.012 on the proxy).
    let inid = spans
        .iter()
        .filter(|span| {
            let t = span.text.trim_start();
            let b = t.as_bytes();
            // At the column's LEFT EDGE: an equation's number `(23)` stands
            // at the right of a scientific page and is not a field.
            span.x <= region_left + 2.0 * span.font_size.max(1.0)
                && b.len() >= 4
                && b[0] == b'('
                && b[1].is_ascii_digit()
                && b[2].is_ascii_digit()
                && b[3] == b')'
        })
        .count()
        >= 3;
    let median = |v: &mut Vec<f64>| -> Option<f64> {
        if v.is_empty() {
            return None;
        }
        v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        Some(if inid {
            v[(v.len() * 9 / 10).min(v.len() - 1)]
        } else {
            v[v.len() / 2]
        })
    };
    let left_median = median(&mut widths.0);
    let right_median = median(&mut widths.1);
    for (is_left, w) in single {
        let m = if is_left { left_median } else { right_median };
        if let Some(m) = m {
            if m > 0.0 && w > m * OUTLIER {
                return false;
            }
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
    // AND NEVER BELOW THE LEADING THE PAGE IS SET ON (#580). The threshold
    // above scales with `median_line_spacing`, which is the FIRST QUARTILE of
    // the band steps -- so a page carrying anything tighter than its body (a
    // table, a stack of figure labels, a run of sub- and superscripts) puts the
    // quartile on that tighter thing and the threshold lands BELOW the body's
    // own leading. Every line break on the page then clears it.
    //
    // WHAT THAT COSTS IS THE WHOLE PAGE, not one cut, and the reason is in
    // `blocks::paragraphs`: its stretches are bounded PER REGION, so lines that
    // the cut put in two regions can never be merged back. On
    // `PMC6019521-p0007` the threshold sat at 10.82 against a leading of 12.00
    // and the page came out as 83 blocks, 72 of them a single line of prose --
    // one block per line, unrecoverable downstream.
    //
    // A FLOOR AND NOT A DIFFERENT STATISTIC. Replacing the quartile with the
    // leading was measured too and it is wrong: the threshold then rises above
    // the paragraph break as well, no horizontal cut fires, the vertical cut is
    // refused on the full-width band that is left, and a two-column page reads
    // letter-perfect nonsense -- left and right column lines glued alternately.
    // The horizontal cut firing at paragraph-break scale is what makes the
    // vertical cut findable at all. So the window the threshold has to sit in is
    // (the leading, the paragraph break], and the floor is the only end of it
    // that was missing.
    let leading = leading_of_a_region(spans, stats);
    let min_gap = if leading > 0.0 {
        min_gap.max(leading * HORIZONTAL_CUT_LEADING_FLOOR)
    } else {
        min_gap
    };

    // Look for the largest gap between consecutive span y-values.
    // Once, before the scan: every candidate is asked the same question about the
    // same bands, and building them per candidate would make the guard's cost
    // quadratic in the gaps a page has.
    let bands = group_spans_into_bands(sorted.clone());
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

    // AND NOT INSIDE ONE GRID (#596): a table whose rows are grouped under labels
    // leaves more white between the groups than between its rows, and the
    // threshold above is a multiple of the ROW pitch.
    //
    // THE WHOLE REGION IS REFUSED, and the review asked for the other shape:
    // skip the grid-internal candidates while scanning and take the largest gap
    // left, so that a SMALLER qualifying gap which separates the table from its
    // caption is not lost with them. The hazard is real -- a table left merged
    // with prose above it is a region `tables::find` refuses outright -- but the
    // cure measured worse and the numbers are why it is not taken: over the 43
    // annotated pages this machine may hold, refusing the region leaves a summed
    // distance of 25 from the annotated table counts, and scanning past the
    // grid-internal gaps leaves 32 against 50 unguarded. `PMC3037919-p0007` goes
    // back from 1 table to 4 and `PMC2268700-p0008` from 1 to 3.
    //
    // The reason is this guard's own blind spots, which are counted on #596: where
    // the region has fewer than two bands on a side, or the widest band shows
    // fewer than two separations, a gap INSIDE the grid is not recognised as one.
    // Scanning on therefore does not find the caption's gap, it finds the next
    // unrecognised gap inside the table and cuts there. Refusing the region keeps
    // the table whole; scanning past cuts it somewhere else.
    if the_cut_is_inside_a_grid(&bands, cut_y, gap_size) {
        return None;
    }

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
    group_spans_into_blocks_legacy_from_bands(bands, &[])
}

fn group_spans_into_blocks_legacy_with_stats(
    spans: Vec<TextSpan>,
    stats: &PageStats,
    rules: &[HorizontalRule],
) -> Vec<TextBlock> {
    let bands = group_spans_into_bands_with_stats(spans, stats);
    group_spans_into_blocks_legacy_from_bands(bands, rules)
}

fn group_spans_into_blocks_legacy_from_bands(
    bands: Vec<TextBand>,
    rules: &[HorizontalRule],
) -> Vec<TextBlock> {
    if bands.is_empty() {
        return Vec::new();
    }

    let column_gap_threshold = compute_adaptive_column_gap(&bands);

    let mut blocks = Vec::new();
    let mut idx = 0;

    while idx < bands.len() {
        let gap_midpoints = bands[idx].gap_midpoints(column_gap_threshold);
        if gap_midpoints.is_empty() {
            idx = push_row(&bands, idx, &mut blocks);
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

        if region_is_columnar(&bands, &band_indices, &boundaries, gapped_band_count)
            && !region_is_ruled(&bands, &band_indices, rules)
            && !region_is_a_grid(&bands, &band_indices, &boundaries)
        {
            append_column_region_blocks(&bands, &band_indices, &boundaries, &mut blocks);
            idx = next_idx;
        } else {
            idx = push_row(&bands, idx, &mut blocks);
        }
    }

    blocks
}

/// Emit the band at `idx` and return the index of the first band after what was
/// emitted.
///
/// A band is normally one line of the page and becomes one block. The exception
/// is the row of a table one of whose cells wraps: the wrapped cell's second
/// line is a band of its own, and emitting the bands in turn puts it after the
/// row's remaining cells instead of after the text it continues (#434).
fn push_row(bands: &[TextBand], idx: usize, blocks: &mut Vec<TextBlock>) -> usize {
    if let Some(next) = append_wrapped_table_row(bands, idx, blocks) {
        return next;
    }
    blocks.push(bands[idx].row_block());
    idx + 1
}

/// The left edges of the cells the band at `idx` draws, as the row itself
/// states them: one edge per span start, and two starts within
/// `CELL_EDGE_TOLERANCE` are one edge.
///
/// Cell edges are taken from starts rather than from the gaps between spans
/// because a cell's measured extent is not a column boundary: `8.2.5.26-t05`
/// draws four of its five columns less than 16 pt apart, which no gutter
/// threshold that leaves ordinary word spacing alone would ever see.
fn cell_edges(band: &TextBand) -> Vec<f64> {
    let mut edges: Vec<f64> = band.spans.iter().map(|span| span.x).collect();
    edges.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    edges.dedup_by(|a, b| (*a - *b).abs() <= CELL_EDGE_TOLERANCE);
    edges
}

/// The cell a span belongs to: the edge it starts on, when it also stops before
/// the next edge. A span that starts between two edges, or that reaches into the
/// next cell, belongs to no cell of this grid and refuses the whole row.
///
/// The extent asked for is the measured one. A span's nominal width is the sum
/// of the advances the font asks for, which regularly overshoots the ink and so
/// the cell: `t05`'s wrapped cell advances 78 pt past its own text and into the
/// column after it.
fn cell_of(span: &TextSpan, edges: &[f64]) -> Option<usize> {
    let idx = edges
        .iter()
        .rposition(|edge| span.x >= edge - CELL_EDGE_TOLERANCE)?;
    if (span.x - edges[idx]).abs() > CELL_EDGE_TOLERANCE {
        return None;
    }
    if let Some(next_edge) = edges.get(idx + 1) {
        if span.measured_right() > next_edge - CELL_EDGE_TOLERANCE {
            return None;
        }
    }
    Some(idx)
}

/// The tallest glyph of the band, which is the unit both vertical distances
/// below are measured in.
fn band_line_height(band: &TextBand) -> f64 {
    band.spans
        .iter()
        .map(|span| span.height.max(span.font_size))
        .fold(0.0_f64, f64::max)
}

/// Whether `band` is another row of the same grid: every one of its spans sits
/// in a cell of `edges`, and it carries at least `TABLE_GRID_MIN_COLUMNS` of
/// them.
///
/// This is what separates a table row from two lines of prose that happen to
/// break in the right place. One line's spans always align with themselves; a
/// grid is only a grid when a second row draws the same edges.
/// The single cell of the grid a band fills, when it fills exactly one.
///
/// This is what a wrapped line looks like from above, and what the next row of
/// a table does not look like: a row with an empty cell still fills two of
/// them, so it is not mistaken for the line of a cell that ran on.
fn band_fills_one_cell(band: &TextBand, edges: &[f64]) -> Option<usize> {
    let mut filled: Option<usize> = None;
    for span in &band.spans {
        let cell = cell_of(span, edges)?;
        match filled {
            Some(seen) if seen != cell => return None,
            _ => filled = Some(cell),
        }
    }
    filled
}

fn band_matches_grid(band: &TextBand, edges: &[f64]) -> bool {
    let mut occupied = vec![false; edges.len()];
    for span in &band.spans {
        match cell_of(span, edges) {
            Some(cell) => occupied[cell] = true,
            None => return false,
        }
    }
    occupied.iter().filter(|seen| **seen).count() >= TABLE_GRID_MIN_COLUMNS
}

/// Emit the row at `idx` cell by cell when one of its cells wraps, and return
/// the index of the first band after the row. `None` when the band is not such
/// a row, which is the ordinary case.
///
/// The row is emitted slice-major: every cell in turn, and within a cell its
/// own lines in the order they are drawn. That is what both readers do, and it
/// is the only order in which a wrapped cell reads as one piece of text.
///
/// A wrapped line and the next row of the table look alike from above -- both
/// are a band under the row with some of its cells filled -- and telling them
/// apart is the whole of the work here. Two things say "line" rather than
/// "row": it fills a single cell of the grid, and it sits closer to the row
/// than the rows of the grid sit to each other.
///
/// What it does not do, deliberately:
///   * a row two of whose cells wrap onto the same line is a line that fills
///     two cells, which is what the next row of the table looks like, so it
///     stays row-major;
///   * so does a row every one of whose cells has a line under it: nothing
///     there says which line continues the row and which one begins the next,
///     and it is also the shape a two-column region takes when one column runs
///     on alone -- reordering that would be the column cut of #389 class O-B
///     done badly;
///   * a row of two cells is left alone, because that is also what a page set
///     in columns looks like -- see `TABLE_ROW_MIN_CELLS`;
///   * the first row of a table is left alone, because a single line's cell
///     edges are also the word boundaries of a sentence and no row above it
///     corroborates them;
///   * a continuation that is indented, centred or right-aligned inside its
///     cell does not start on the cell's edge and is not recognised.
fn append_wrapped_table_row(
    bands: &[TextBand],
    idx: usize,
    blocks: &mut Vec<TextBlock>,
) -> Option<usize> {
    let head = &bands[idx];
    if head.spans.len() < TABLE_ROW_MIN_CELLS {
        return None;
    }
    let edges = cell_edges(head);
    if edges.len() < TABLE_ROW_MIN_CELLS {
        return None;
    }
    let line_height = band_line_height(head);
    if line_height <= 0.0 {
        return None;
    }

    let head_cells: Vec<usize> = head
        .spans
        .iter()
        .map(|span| cell_of(span, &edges))
        .collect::<Option<Vec<usize>>>()?;

    // The pitch of the grid, from the band directly above: one line plus
    // whatever the cells are padded by. Nothing below may sit that far from
    // what it continues, or it is the next row rather than a wrapped line.
    let above = idx.checked_sub(1)?;
    let row_pitch = bands[above].y - head.y;
    if row_pitch <= 0.0 {
        return None;
    }
    let max_line_gap = (line_height * WRAPPED_CELL_LINE_GAP_FACTOR)
        .min(row_pitch * WRAPPED_CELL_MAX_PITCH_FRACTION);

    // The nearest row above that draws the same cell edges. It is what says
    // this is a grid at all: one line's spans always align with themselves, so
    // only a second row makes the edges evidence of anything. The bands stepped
    // over on the way are the wrapped lines of that row.
    let mut corroborator = above;
    loop {
        if band_matches_grid(&bands[corroborator], &edges) {
            break;
        }
        if band_fills_one_cell(&bands[corroborator], &edges).is_none()
            || idx - corroborator >= TABLE_GRID_MAX_LOOKBACK
            || corroborator == 0
        {
            return None;
        }
        corroborator -= 1;
    }

    // The continuation lines: each fills one cell of the grid that the row
    // itself fills, and sits less than the grid's pitch below the last line of
    // *that cell*.
    //
    // Per cell, and not below the line before it whichever cell that was: a
    // cell's lines are consecutive, so its third line is two gaps below the row
    // while another cell's second line is one. Measuring against the last line
    // accepted would let the row after a wrapped one -- if it happens to fill a
    // single cell -- sit one gap below that wrapped line and be read as a third
    // line of the row it follows.
    let mut continuations: Vec<(&TextBand, usize)> = Vec::new();
    let mut last_line_in_cell = vec![head.y; edges.len()];
    let mut next_idx = idx + 1;
    while next_idx < bands.len() {
        let band = &bands[next_idx];
        // One cell's line cannot carry more runs than the whole row it
        // continues; a band that does is a row of its own.
        if band.spans.len() >= head.spans.len() {
            break;
        }
        let Some(cell) = band_fills_one_cell(band, &edges) else {
            break;
        };
        if !head_cells.contains(&cell) {
            break;
        }
        let line_gap = last_line_in_cell[cell] - band.y;
        if line_gap <= 0.0 || line_gap > max_line_gap {
            break;
        }
        last_line_in_cell[cell] = band.y;
        continuations.push((band, cell));
        next_idx += 1;
    }
    if continuations.is_empty() {
        return None;
    }

    let wrapped_cells: Vec<usize> = continuations.iter().map(|(_, cell)| *cell).collect();
    if head_cells.iter().all(|cell| wrapped_cells.contains(cell)) {
        return None;
    }

    for cell in 0..edges.len() {
        let row_spans: Vec<TextSpan> = head
            .spans
            .iter()
            .zip(&head_cells)
            .filter(|(_, span_cell)| **span_cell == cell)
            .map(|(span, _)| span.clone())
            .collect();
        if !row_spans.is_empty() {
            blocks.push(TextBlock { spans: row_spans });
        }
        for (band, wrapped_cell) in &continuations {
            if *wrapped_cell == cell {
                blocks.push(band.row_block());
            }
        }
    }

    Some(next_idx)
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

    // A COLUMN OF MARKERS IS A LIST (#574). A bullet set well apart from its
    // item -- ASX_MRM draws a 19 pt `•` 13 pt before each item's text -- is a
    // gap on every band at one x, and read as a column the page came back as
    // four bullets and then four items, which no list pass can put together
    // again. Where every cell of the first column is a list marker, the bands
    // are lines.
    //
    // AN ITEM OPENS WITH A WORD: a `–` for nil down the first column of a
    // block of amounts is a column of cells, and the band beside it holds
    // figures (DocLayNet validation, a statement of receivables).
    let mut marked = 0usize;
    let markers_first = band_indices.iter().all(|&band_idx| {
        let slices = bands[band_idx].split_by_boundaries(boundaries);
        let joined = |slice: &Vec<TextSpan>| -> String {
            slice
                .iter()
                .map(|span| span.text.as_str())
                .collect::<String>()
        };
        let Some(first) = slices.first() else {
            return true;
        };
        let text = joined(first);
        let text = text.trim();
        if text.is_empty() {
            return true;
        }
        let item_follows = slices
            .iter()
            .skip(1)
            .map(joined)
            .find(|t| !t.trim().is_empty())
            // Words, wherever they start: `“Revenue increased”`, `2024
            // results` are items; a cell of figures is not (codex, #2045).
            .is_some_and(|t| t.chars().any(char::is_alphabetic));
        marked += 1;
        is_bullet_or_list_marker(text) && item_follows
    });
    if markers_first && marked >= 2 {
        return false;
    }

    (dense_slices as f64 / non_empty_slices as f64) >= MIN_DENSE_SLICE_RATIO
}

/// Whether a cell's whole text is a list's marker: a bullet glyph, a pointer
/// or picture the font had no letter for, or a short ordinal like `1.`, `a)`.
fn is_bullet_or_list_marker(text: &str) -> bool {
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => {
            crate::lists::BULLET_CHARS.contains(&c) || crate::lists::is_a_pointer_or_a_picture(c)
        }
        _ => is_list_marker(text),
    }
}

/// Whether this region is a TABLE rather than a page set in columns (#570).
///
/// `region_is_ruled` answers the same question from the document's own
/// graphics and says nothing at all about a table drawn without ruling lines --
/// which is the commonest kind there is, and which `region_is_columnar` reads
/// down its columns: the rows of it come back as one paragraph per column, and
/// every row of the table is lost. So there is a second answer, and it is made
/// of the one measurement that separates the two cases without a ruling line.
///
/// A LINE OF PROSE FILLS ITS COLUMN AND A CELL DOES NOT. That is the whole of
/// it. A paper set in two columns sets each of them to a measure and fills it:
/// the slice a band leaves in a column is as wide as the column, to the last
/// word of the last line. A table's cells are words and figures, ragged and
/// short, and the white to the right of them is what makes the thing look like
/// a table to a reader. So: a region whose columns are all filled in most of
/// its rows, and whose typical cell covers well under its column, is a grid.
///
/// Deliberately narrow. Three rows at least, every column present in three
/// quarters of them, and the MEDIAN fill under `GRID_SLICE_FILL` -- so one
/// short line in a column of prose, or one long cell in a table, decides
/// nothing.
fn region_is_a_grid(bands: &[TextBand], band_indices: &[usize], boundaries: &[f64]) -> bool {
    if band_indices.len() < GRID_MIN_ROWS || boundaries.is_empty() {
        return false;
    }

    let mut left = f64::INFINITY;
    let mut right = f64::NEG_INFINITY;
    for &band_idx in band_indices {
        for span in &bands[band_idx].spans {
            left = left.min(span.x);
            right = right.max(span.measured_right());
        }
    }
    if !(left.is_finite() && right > left) {
        return false;
    }
    let edges: Vec<f64> = std::iter::once(left)
        .chain(boundaries.iter().copied())
        .chain(std::iter::once(right))
        .collect();

    let mut present = vec![0usize; boundaries.len() + 1];
    let mut fills: Vec<f64> = Vec::new();
    for &band_idx in band_indices {
        for (column, slice) in bands[band_idx]
            .split_by_boundaries(boundaries)
            .iter()
            .enumerate()
        {
            if slice.is_empty() {
                continue;
            }
            present[column] += 1;
            let slice_left = slice
                .iter()
                .map(|span| span.x)
                .fold(f64::INFINITY, f64::min);
            let slice_right = slice
                .iter()
                .map(TextSpan::measured_right)
                .fold(f64::NEG_INFINITY, f64::max);
            let width = edges[column + 1] - edges[column];
            if width > 0.0 && slice_right > slice_left {
                fills.push(((slice_right - slice_left) / width).min(1.0));
            }
        }
    }

    let needed = GRID_COLUMN_SUPPORT * band_indices.len() as f64;
    if present.iter().any(|seen| (*seen as f64) < needed) {
        return false;
    }
    if fills.is_empty() {
        return false;
    }
    fills.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    fills[fills.len() / 2] <= GRID_SLICE_FILL
}

/// Whether the page rules this region into rows (#545).
///
/// A rule counts when it crosses the region -- `RULE_SPAN_FRACTION` of the
/// region's own width, so a line under a single cell does not speak for the
/// table -- and lies between the baselines of two bands that follow each other
/// in the region. Enough of the region's band gaps carrying such a rule is the
/// document saying, in its own graphics, that these bands are rows; and rows
/// are read across, never down.
///
/// What it does not claim: a table drawn without ruling lines is invisible
/// here, and this is deliberately the whole of the claim. The alternative
/// offered for that case was a width heuristic, and a width heuristic cannot
/// tell a table from a paper set in two columns -- it fixed the Isartor
/// appendix and cost `tracemonkey` p2 and p13, which is the class O-B danger
/// the #389 report names for exactly this defect.
fn region_is_ruled(bands: &[TextBand], band_indices: &[usize], rules: &[HorizontalRule]) -> bool {
    if rules.is_empty() || band_indices.len() < MIN_RULED_ROW_SEPARATORS + 1 {
        return false;
    }

    let mut left = f64::INFINITY;
    let mut right = f64::NEG_INFINITY;
    for &band_idx in band_indices {
        for span in &bands[band_idx].spans {
            left = left.min(span.x);
            right = right.max(span.measured_right());
        }
    }
    let width = right - left;
    if width <= 0.0 {
        return false;
    }

    let mut gaps = 0usize;
    let mut ruled = 0usize;
    for pair in band_indices.windows(2) {
        let upper = bands[pair[0]].y;
        let lower = bands[pair[1]].y;
        if upper <= lower {
            continue;
        }
        gaps += 1;
        let crossed = rules.iter().any(|rule| {
            rule.y > lower
                && rule.y < upper
                && (rule.right.min(right) - rule.left.max(left)) >= width * RULE_SPAN_FRACTION
        });
        if crossed {
            ruled += 1;
        }
    }

    gaps > 0
        && ruled >= MIN_RULED_ROW_SEPARATORS
        && (ruled as f64 / gaps as f64) >= MIN_RULED_GAP_SUPPORT
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
pub(crate) fn is_hyphen_wrap_candidate(accumulated: &str, next_trimmed: &str) -> bool {
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

    /// `span` with one 6 pt glyph box per character from `x` on.
    fn boxed(text: &str, x: f64) -> TextSpan {
        let n = text.chars().count();
        let mut s = span(text, x, 100.0, 6.0 * n as f64);
        s.char_bounds = (0..n)
            .map(|i| {
                let left = x + 6.0 * i as f64;
                [left, 100.0, left + 6.0, 112.0]
            })
            .collect();
        s
    }

    /// A U+20DD that reaches the line assembly is never moved or left out:
    /// placing a row-derived mark is the interpreter's (C4 rule 6), so a
    /// circle from `ToUnicode` keeps its x order here.
    #[test]
    fn a_circle_from_another_source_keeps_its_place() {
        let mut mark = boxed("\u{20DD}", 98.0);
        mark.char_bounds = vec![[98.0, 100.0, 110.0, 112.0]];
        mark.width = 12.0;
        let enclosing = TextBlock {
            spans: vec![mark.clone(), boxed("c", 100.0)],
        };
        assert!(
            enclosing.text().starts_with('\u{20DD}'),
            "{:?}",
            enclosing.text()
        );
        let lone = TextBlock { spans: vec![mark] };
        assert_eq!(lone.text(), "\u{20DD}");
    }

    /// A row as a letter-spaced line draws it: one span per glyph, letters
    /// `tracking` apart, a word space drawn as a span of its own where
    /// `words` part (#1973). `gap` puts that much before the word instead.
    fn tracked_row(words: &[&str], tracking: f64, gap: Option<(usize, f64)>) -> TextBlock {
        let mut spans = Vec::new();
        let mut x = 0.0;
        for (w, word) in words.iter().enumerate() {
            if w > 0 {
                match gap {
                    Some((at, wide)) if at == w => x += wide,
                    _ => {
                        spans.push(span(" ", x, 0.0, 3.0));
                        x += 3.0 + tracking;
                    }
                }
            }
            for c in word.chars() {
                spans.push(span(&c.to_string(), x, 0.0, 8.0));
                x += 8.0 + tracking;
            }
        }
        TextBlock { spans }
    }

    #[test]
    fn a_row_tracked_by_drawn_spaces_reads_as_its_words() {
        // `P O W E L L  I N D U S T R I E S`: five points of tracking on
        // twelve-point capitals is over the word-space threshold, so each
        // letter read as a word (#1973). The drawn space says where the words
        // part.
        let row = tracked_row(&["POWELL", "INDUSTRIES,", "INC."], 5.0, None);
        assert_eq!(row.text(), "POWELL INDUSTRIES, INC.");
    }

    #[test]
    fn a_row_of_letters_with_no_drawn_space_is_left_alone() {
        // No mark says where words part: letters set apart on purpose.
        let row = tracked_row(&["ABCDE"], 5.0, None);
        assert_eq!(row.text(), "A B C D E");
        // Nor lower case, a formula or a listing, whatever is drawn between.
        let row = tracked_row(&["xyzw", "abcd"], 5.0, None);
        assert_eq!(row.text(), "x y z w   a b c d");
    }

    #[test]
    fn lower_case_between_drawn_spaces_does_not_make_the_capitals_tracked() {
        // Capitals, lower-case letters one apart, capitals again, a space
        // drawn between each -- a formula's variables between two labels.
        // Only capitals make a tracked row, so neither space is the boundary
        // of one.
        let row = tracked_row(&["ABCD", "xyzw", "EFGH"], 5.0, None);
        assert_eq!(row.text(), "A B C D   x y z w   E F G H");
    }

    #[test]
    fn a_single_letter_after_the_drawn_space_is_not_a_tracked_word() {
        // A figure's label `F C F C`, a space, and a one-letter word: nothing
        // here is a tracked line of words.
        let row = tracked_row(&["FCFC", "S"], 5.0, None);
        assert_eq!(row.text(), "F C F C   S");
    }

    #[test]
    fn a_gap_far_wider_than_the_tracking_keeps_its_space() {
        // `NEW YORK` and then, with no drawn space but a wide gap, `DESK`: the
        // gap is a word boundary the page did not draw.
        let row = tracked_row(&["NEW", "YORK", "DESK"], 5.0, Some((2, 40.0)));
        assert_eq!(row.text(), "NEW YORK DESK");
    }

    #[test]
    fn letter_spacing_by_space_characters_reads_as_its_words() {
        // The same line when the page draws it in one span, a space after
        // every capital and two where the words part (#1973).
        let mut tracked = span(
            "P O W E L L  I N D U S T R I E S ,  I N C .",
            0.0,
            0.0,
            200.0,
        );
        untrack_spaced_letters(&mut tracked);
        assert_eq!(tracked.text, "POWELL INDUSTRIES, INC.");
        // A single space everywhere says nothing about the words: left alone.
        let mut apart = span("A B C D E F G", 0.0, 0.0, 100.0);
        untrack_spaced_letters(&mut apart);
        assert_eq!(apart.text, "A B C D E F G");
        // Lower case is a formula or a listing.
        let mut formula = span("x y z w  a b c d", 0.0, 0.0, 100.0);
        untrack_spaced_letters(&mut formula);
        assert_eq!(formula.text, "x y z w  a b c d");
        // A label beside one letter is not two tracked words (codex, #1974).
        let mut label = span("F C F C  S", 0.0, 0.0, 100.0);
        untrack_spaced_letters(&mut label);
        assert_eq!(label.text, "F C F C  S");
    }

    /// A span as the consensus leaves a tracked word: every letter a glyph
    /// `advance` wide and `gaps[k]` before letter `k + 1`, and a space in the
    /// text between each of them (#1973).
    fn consensus_spaced(letters: &str, advance: f64, gaps: &[f64], size: f64) -> TextSpan {
        let mut x = 0.0;
        let mut bounds = Vec::new();
        for (k, _) in letters.chars().enumerate() {
            bounds.push([x, 0.0, x + advance, size]);
            x += advance + gaps.get(k).copied().unwrap_or(0.0);
        }
        let text = letters
            .chars()
            .map(String::from)
            .collect::<Vec<_>>()
            .join(" ");
        TextSpan {
            char_bounds: bounds,
            font_size: size,
            ..span(&text, 0.0, 0.0, x)
        }
    }

    #[test]
    fn a_short_tracked_word_reads_as_one() {
        // `P E N N` at 6.2 points, 0.9 apart: three gaps, never the four #582
        // waits for, and a new span at the word space (#1973).
        let mut penn = consensus_spaced("PENN", 3.0, &[0.9, 0.9, 0.9], 6.2);
        untrack_spaced_letters(&mut penn);
        assert_eq!(penn.text, "PENN");
        // Kerned pairs sit tighter than the tracking, and are inside the word.
        let mut kerned = consensus_spaced(
            "OUTSTANDING",
            30.0,
            &[3.0, 3.0, 3.0, 3.0, 0.6, 3.0, 3.0, 3.0, 1.5, 3.0],
            60.0,
        );
        untrack_spaced_letters(&mut kerned);
        assert_eq!(kerned.text, "OUTSTANDING");
    }

    #[test]
    fn a_gap_wider_than_the_tracking_is_a_word_boundary() {
        let mut two = consensus_spaced("ABCDEF", 8.0, &[1.0, 1.0, 3.0, 1.0, 1.0], 12.0);
        untrack_spaced_letters(&mut two);
        assert_eq!(two.text, "ABC DEF");
        // As many word gaps as letter gaps (codex, #1980).
        let mut short = consensus_spaced("IAMOK", 8.0, &[3.0, 1.0, 3.0, 1.0], 12.0);
        untrack_spaced_letters(&mut short);
        assert_eq!(short.text, "I AM OK");
    }

    #[test]
    fn a_finished_span_is_left_alone_unless_it_is_tracked_capitals() {
        // Set solid: the boxes touch, and the consensus's spaces are words.
        let mut solid = consensus_spaced("ABCDE", 8.0, &[0.0, 0.0, 0.0, 0.0], 12.0);
        untrack_spaced_letters(&mut solid);
        assert_eq!(solid.text, "A B C D E");
        // Lower case: a formula's variables.
        let mut lower = consensus_spaced("xyzwv", 8.0, &[1.0, 1.0, 1.0, 1.0], 12.0);
        untrack_spaced_letters(&mut lower);
        assert_eq!(lower.text, "x y z w v");
        // Nor capitals beside lower case one apart, a label and a formula.
        let mut mixed = consensus_spaced("ABCDxyz", 8.0, &[1.0; 6], 12.0);
        untrack_spaced_letters(&mut mixed);
        assert_eq!(mixed.text, "A B C D x y z");
        // A glyph more than letters: a drawn space or a ligature, whose place
        // in the text this cannot know.
        let mut drawn = consensus_spaced("ABCDE", 8.0, &[1.0, 1.0, 1.0, 1.0], 12.0);
        drawn.char_bounds.push([100.0, 0.0, 104.0, 12.0]);
        untrack_spaced_letters(&mut drawn);
        assert_eq!(drawn.text, "A B C D E");
    }

    fn sized(text: &str, x: f64, y: f64, width: f64, size: f64) -> TextSpan {
        TextSpan {
            font_size: size,
            height: size,
            ..span(text, x, y, width)
        }
    }

    #[test]
    fn a_label_drawn_again_as_its_shadow_is_one_label() {
        // FAA-H-8083-6 (DocLayNet train), a moving map: `KFTC` drawn twice,
        // the second copy 0.94 right and 0.85 lower at 4.9 points, and read
        // `KFTCKFTC` (#1973).
        let first = sized("KFTC", 263.19, 656.12, 12.88, 4.9);
        let shadow = sized("KFTC", 264.13, 656.97, 12.88, 4.9);
        assert!(spans_are_overprint_duplicates(&first, &shadow));
        // A size larger, as a halo is: overlapping 77 per cent.
        let small = sized("NRST", 385.24, 476.85, 10.93, 4.90);
        let large = sized("NRST", 387.79, 477.16, 12.33, 5.52);
        assert!(spans_are_overprint_duplicates(&small, &large));
    }

    #[test]
    fn a_column_head_set_twice_half_over_itself_is_one_head() {
        // NASDAQ_SHEN_2003 (DocLayNet): `2003` and its copy 7.99 right, 0.86
        // higher, overlapping 60 per cent -- read `20032003` (#623).
        let first = sized("2003", 346.22, 549.77, 20.16, 10.08);
        let copy = sized("2003", 354.21, 550.63, 20.16, 10.08);
        assert!(spans_are_overprint_duplicates(&first, &copy));
        // Less than half over each other is two words, even of one text.
        let left = sized("2003", 346.22, 549.77, 20.16, 10.08);
        let right = sized("2003", 358.0, 549.77, 20.16, 10.08);
        assert!(!spans_are_overprint_duplicates(&left, &right));
        // Two characters keep the stricter share.
        let a = sized("12", 100.0, 100.0, 10.0, 10.0);
        let b = sized("12", 104.0, 100.3, 10.0, 10.0);
        assert!(!spans_are_overprint_duplicates(&a, &b));
    }

    #[test]
    fn a_shadow_needs_the_same_text_of_two_characters_close_by() {
        // One character: the two `1`s of a matrix, the `ll` of a word.
        let one = sized("1", 100.0, 100.0, 2.5, 5.0);
        let other = sized("1", 100.5, 100.9, 2.5, 5.0);
        assert!(!spans_are_overprint_duplicates(&one, &other));
        // Other text.
        let a = sized("KFTC", 263.19, 656.12, 12.88, 4.9);
        let b = sized("KCLN", 264.13, 656.97, 12.88, 4.9);
        assert!(!spans_are_overprint_duplicates(&a, &b));
        // Further off than a quarter em: the next line, not a shadow.
        let line = sized("KFTC", 263.19, 656.12, 12.88, 4.9);
        let below = sized("KFTC", 263.19, 654.50, 12.88, 4.9);
        assert!(!spans_are_overprint_duplicates(&line, &below));
    }

    #[test]
    fn a_tracked_span_that_loses_its_spaces_says_what_its_letters_are() {
        // Rich geometry marks a span holding a drawn space an estimate: a space
        // has no outline. With the spaces taken out the span is as tight as
        // its letters (codex, #1974).
        let text = "A B C D  E F G H";
        let mut tracked = span(text, 0.0, 0.0, 100.0);
        tracked.glyph_bounds_sources = text
            .chars()
            .map(|c| {
                if c == ' ' {
                    BoundsSource::Estimate
                } else {
                    BoundsSource::Tight
                }
            })
            .collect();
        tracked.bounds_source = BoundsSource::Estimate;
        untrack_spaced_letters(&mut tracked);
        assert_eq!(tracked.text, "ABCD EFGH");
        assert_eq!(tracked.glyph_bounds_sources.len(), 9);
        assert_eq!(
            tracked.bounds_source,
            BoundsSource::Estimate,
            "the one space kept is still a space"
        );
        let mut all_tight = span("A B C D  E F G H", 0.0, 0.0, 100.0);
        all_tight.glyph_bounds_sources = vec![BoundsSource::Tight; 16];
        all_tight.bounds_source = BoundsSource::Estimate;
        untrack_spaced_letters(&mut all_tight);
        assert_eq!(all_tight.bounds_source, BoundsSource::Tight);
    }

    /// A two-column academic page, the shape #429 is about: a full-width title,
    /// then two columns of body text whose right column ends one line short of
    /// the left. Every line is a realistic fraction of its column width.
    ///
    /// Returned in reading order — title, all of column one, then all of column
    /// two — so the test can say what the right answer is instead of guessing.
    ///
    /// The lines are prose-length on purpose. `estimated_width()` is
    /// `font_size * 0.5 * chars`, so a realistic line of about fifty characters
    /// estimates wider than it measures, and `right()` — which takes the larger
    /// of the two — then reaches across the gutter. Short labels would make a
    /// tidier fixture that never reproduces the defect.
    fn two_column_page() -> Vec<TextSpan> {
        const LEFT_X: f64 = 40.0;
        const RIGHT_X: f64 = 320.0;
        const COLUMN_WIDTH: f64 = 250.0;
        const TOP: f64 = 700.0;
        const LEADING: f64 = 14.0;
        const LINES: usize = 20;

        let mut out = vec![span(
            "A Trace-based Just-in-Time Compiler",
            40.0,
            740.0,
            530.0,
        )];
        for i in 0..LINES {
            out.push(span(
                &format!("left line {i:02} of the two-column academic body text"),
                LEFT_X,
                TOP - (i as f64) * LEADING,
                COLUMN_WIDTH,
            ));
        }
        // One line short: the right column ends above the left, which leaves the
        // left column's last band alone on its side of the gutter. Every real
        // two-column page has such bands -- a heading, a column that ends early,
        // a figure -- and the guard this test is about used to refuse the cut
        // for exactly that reason.
        for i in 0..(LINES - 1) {
            out.push(span(
                &format!("right line {i:02} of the two-column academic body text"),
                RIGHT_X,
                TOP - (i as f64) * LEADING,
                COLUMN_WIDTH,
            ));
        }
        out
    }

    /// #429: a two-column body is read column by column.
    ///
    /// The whole page in one assertion, because reading order is a property of
    /// the whole page and a per-block check would pass while the columns were
    /// interleaved. Revert either half of the fix and this goes red: with the
    /// gap scan on `right()` no gutter is found at all, and with the old
    /// band-aligned guard the cut is refused once it is.
    #[test]
    fn a_two_column_body_is_read_column_by_column() {
        let spans = two_column_page();
        let expected: Vec<String> = spans.iter().map(|s| s.text.clone()).collect();

        let produced: Vec<String> = group_spans_into_blocks(spans, &[])
            .into_iter()
            .flat_map(|b| b.spans.into_iter().map(|s| s.text))
            .collect();

        assert_eq!(produced, expected);
    }

    /// A JOURNAL GUTTER IS NARROWER THAN FOUR PERCENT OF THE PAGE (#389).
    ///
    /// The shape that the region-width rail refused: 9 pt type, a 493 pt text
    /// block, and the 13.2 pt gutter tracemonkey is actually set with. Four
    /// percent of that block is 19.7 pt, which is wider than the gutter of any
    /// journal, and the cut was refused on pages 2 and 13 of tracemonkey,
    /// highlights and tracemonkey_with_editable_annotations -- six of the eleven
    /// pages left under 95 % order agreement on the gate set.
    ///
    /// Returned in reading order, so the test says what the right answer is.
    fn narrow_gutter_page() -> Vec<TextSpan> {
        narrow_gutter_page_with_gutter(13.2)
    }

    /// The same page with the gutter set by the caller, so a test can ask for a
    /// gap that is not a gutter.
    fn narrow_gutter_page_with_gutter(gutter: f64) -> Vec<TextSpan> {
        const LEFT_X: f64 = 54.0;
        const COLUMN_WIDTH: f64 = 240.0;
        const FONT_SIZE: f64 = 9.0;
        const TOP: f64 = 700.0;
        const LEADING: f64 = 11.0;
        const LINES: usize = 20;

        let right_x = LEFT_X + COLUMN_WIDTH + gutter;
        let line = |text: &str, x: f64, y: f64| TextSpan {
            text: text.into(),
            x,
            y,
            width: COLUMN_WIDTH,
            height: FONT_SIZE,
            font_size: FONT_SIZE,
            ..TextSpan::default()
        };

        let mut out = vec![TextSpan {
            text: "Trace-based Just-in-Time Type Specialization".into(),
            x: LEFT_X,
            y: 740.0,
            width: 490.0,
            height: 14.0,
            font_size: 14.0,
            ..TextSpan::default()
        }];
        // PROSE-LENGTH LINES, for the same reason `two_column_page` gives them:
        // `estimated_width()` is font_size * 0.5 * chars, so a real line of
        // sixty characters overshoots its measured width and the overshoot
        // swallows the gutter -- which is how a page ends up with no measurable
        // word gaps at all and the adaptive threshold falls back to 20 pt.
        for i in 0..LINES {
            out.push(line(
                &format!("left line {i:02} of the two-column academic body of this paper"),
                LEFT_X,
                TOP - (i as f64) * LEADING,
            ));
        }
        for i in 0..(LINES - 1) {
            out.push(line(
                &format!("right line {i:02} of the two-column academic body of this paper"),
                right_x,
                TOP - (i as f64) * LEADING,
            ));
        }
        out
    }

    /// Red the moment a gutter has to be a fraction of the region again.
    ///
    /// Asked of `try_vertical_cut` itself, so that it names the cut and not the
    /// page: the reading order below is the consequence and this is the cause.
    #[test]
    fn a_thirteen_point_gutter_in_nine_point_type_is_a_gutter() {
        let spans = narrow_gutter_page();
        let body: Vec<TextSpan> = spans.iter().filter(|s| s.y < 720.0).cloned().collect();
        let stats = PageStats::from_spans(&body);

        let (groups, gap) = try_vertical_cut(&body, &stats, &[])
            .expect("no column cut found on a 13.2 pt gutter in 9 pt type");

        assert_eq!(groups.len(), 2);
        assert!(
            (gap - 13.2).abs() < 0.5,
            "the cut was taken on a gap of {gap}, not on the gutter"
        );
        assert!(
            groups[0].iter().all(|s| s.text.starts_with("left")),
            "the left group holds text from the right column"
        );
        assert!(
            groups[1].iter().all(|s| s.text.starts_with("right")),
            "the right group holds text from the left column"
        );
    }

    /// And the whole page, in reading order, through the path a document takes.
    /// Red with the rail back as well: the leaf's own band-by-band gutter
    /// detector does not rescue a page whose lines overshoot their measured
    /// width, which is every page set in a proportional face.
    #[test]
    fn the_narrow_gutter_page_reads_column_by_column() {
        let spans = narrow_gutter_page();
        let expected: Vec<String> = spans.iter().map(|s| s.text.clone()).collect();

        let produced: Vec<String> = group_spans_into_blocks(spans, &[])
            .into_iter()
            .flat_map(|b| b.spans.into_iter().map(|s| s.text))
            .collect();

        assert_eq!(produced, expected);
    }

    /// And the floor still does its work: the same page whose columns are six
    /// points apart is not two columns, because six points is a word space in
    /// nine point type. Red if the floor is dropped along with the rail.
    #[test]
    fn a_gap_narrower_than_the_type_is_not_a_gutter() {
        let spans = narrow_gutter_page_with_gutter(6.0);
        let body: Vec<TextSpan> = spans.iter().filter(|s| s.y < 720.0).cloned().collect();
        let stats = PageStats::from_spans(&body);

        assert!(
            try_vertical_cut(&body, &stats, &[]).is_none(),
            "a six point gap in nine point type was taken for a column gutter"
        );
    }

    fn nine(text: &str, x: f64, y: f64, width: f64) -> TextSpan {
        TextSpan {
            height: 9.0,
            font_size: 9.0,
            ..span(text, x, y, width)
        }
    }

    /// Two columns whose text is 23 pt apart, with the right column's bullets
    /// hung 8 pt from the left column's measure and the space after each
    /// drawn as a run of its own (NYSE_MSA, #574). Counted as ink, they leave
    /// no x free of every span.
    fn columns_with_hung_markers() -> Vec<TextSpan> {
        let mut out = Vec::new();
        for i in 0..12 {
            let y = 700.0 - 12.0 * i as f64;
            out.push(nine(
                &format!("left line {i:02} of the running text of this annual report"),
                72.0,
                y,
                225.0,
            ));
            if i % 4 == 0 {
                out.push(nine("t", 305.0, y, 3.3));
                out.push(nine("  ", 308.3, y, 5.0));
            }
            out.push(nine(
                &format!("right line {i:02} of the running text of this annual report"),
                320.0,
                y,
                225.0,
            ));
        }
        out
    }

    #[test]
    fn a_gutter_is_found_past_the_markers_hung_into_it() {
        let body = columns_with_hung_markers();
        let stats = PageStats::from_spans(&body);
        let (groups, _) = try_vertical_cut(&body, &stats, &[])
            .expect("no cut past the hung markers and their space runs");
        assert_eq!(groups.len(), 2);
        assert!(
            groups[0].iter().all(|s| s.text.starts_with("left")),
            "a hung marker went to the left column"
        );
    }

    /// The looking-past is for columns of text. NYSE_JWN's exhibit index has a
    /// right column of `Filed herewith electronically` on every row and one
    /// space drawn in the strip on its head row; looked past, the strip is
    /// white, and cutting there read every filing method after every exhibit.
    #[test]
    fn a_column_of_cells_found_past_a_space_is_not_cut_off() {
        let mut body = vec![
            nine("Exhibit", 192.0, 728.0, 28.0),
            nine(" ", 326.0, 728.0, 2.0),
            nine("Method of Filing", 413.0, 728.0, 63.0),
        ];
        for i in 0..10 {
            let y = 713.0 - 18.0 * i as f64;
            if i % 3 == 2 {
                // `101.SCH XBRL Taxonomy ...`: number and description one run,
                // across the white that would otherwise part them.
                body.push(nine(
                    "101.SCH XBRL Taxonomy Extension Schema Document",
                    52.0,
                    y,
                    213.0,
                ));
            } else {
                body.push(nine(&format!("{i}.1"), 52.0, y, 16.0));
                body.push(nine(
                    "Certification of Chief Financial Officer required",
                    92.0,
                    y,
                    226.0,
                ));
            }
            body.push(nine("Filed herewith electronically", 330.0, y, 100.0));
        }
        let stats = PageStats::from_spans(&body);
        assert!(
            try_vertical_cut(&body, &stats, &[]).is_none(),
            "the cell column of a table was cut off as a column of text"
        );
    }

    /// The last glyph of a line is not a marker hung into the gutter (codex,
    /// #2041). A left column whose lines close on `)` in a run of their own,
    /// 9 pt from the right column: looked past, the `)` would open a 15 pt
    /// strip and take a gutter the page does not have.
    #[test]
    fn a_closing_glyph_is_not_a_hung_marker() {
        let mut body = Vec::new();
        for i in 0..10 {
            let y = 700.0 - 12.0 * i as f64;
            // Ragged, so the closing glyphs stand at no one x.
            let width = [221.0, 204.0, 213.0, 196.0, 218.0][i % 5];
            body.push(nine(
                &format!("left line {i:02} of the running text of this annual report (see"),
                72.0,
                y,
                width,
            ));
            body.push(nine(")", 72.0 + width + 1.0, y, 3.0));
            body.push(nine(
                &format!("right line {i:02} of the running text of this annual report"),
                305.0,
                y,
                225.0,
            ));
        }
        let stats = PageStats::from_spans(&body);
        assert!(
            try_vertical_cut(&body, &stats, &[]).is_none(),
            "a line's closing glyph was looked past as a hung marker"
        );
    }

    /// A statement whose cells are parted by tab runs is not two columns of
    /// text (DocLayNet validation, #574). Every gap between its amounts holds
    /// a `\t` drawn as a run of its own; looked past as word spaces they left
    /// every column gap clear, and rows of figures as long as a line of prose
    /// passed for running text -- the statement was cut down its middle. A
    /// column of text is counted in letters.
    #[test]
    fn a_statement_parted_by_tab_runs_is_not_cut() {
        let mut body = Vec::new();
        for i in 0..10 {
            let y = 700.0 - 12.0 * i as f64;
            body.push(nine(
                &format!("Corporate, other and eliminations ({i:02})"),
                15.0,
                y,
                106.0,
            ));
            let mut x = 125.0;
            for cell in [
                "$ 438,082",
                "$ 9,836,950",
                "$ 3,304,152",
                "$ 5,919,819",
                "$ 429,405",
                "$ 3,692,551",
                "$ 840,333",
                "$ 1,049,328",
                "$ 6,086,009",
            ] {
                body.push(nine(cell, x, y, 38.0));
                body.push(nine("\t", x + 44.0, y, 2.4));
                x += 51.0;
            }
        }
        let stats = PageStats::from_spans(&body);
        assert!(
            try_vertical_cut(&body, &stats, &[]).is_none(),
            "a statement parted by tab runs was cut as two columns of text"
        );
    }

    /// A `–` for nil down a column of amounts is a cell, not a list's marker
    /// (DocLayNet validation, #574): read as a list the statement's figures
    /// lost their columns.
    #[test]
    fn a_column_of_nil_dashes_is_not_a_list() {
        let mut spans = Vec::new();
        for i in 0..5 {
            let y = 700.0 - 14.0 * i as f64;
            spans.push(nine("\u{2013}", 300.0, y, 5.0));
            spans.push(nine(&format!("22,795,60{i}"), 360.0, y, 40.0));
            spans.push(nine(&format!("22,788,10{i}"), 430.0, y, 40.0));
        }
        assert!(
            !is_bullet_or_list_marker("22,795,604"),
            "the fixture's figures read as markers"
        );
        let bands = group_spans_into_bands(spans);
        let indices: Vec<usize> = (0..bands.len()).collect();
        let boundaries = vec![330.0, 415.0];
        let gaps = indices.len();
        // A column of dashes beside figures is still columnar.
        assert!(
            region_is_columnar(&bands, &indices, &boundaries, gaps),
            "a column of nil dashes beside amounts was taken for a list"
        );
    }

    /// Rows of amounts count as rows with no letters (codex, #2045): left out
    /// of the sample, one long header on each side would stand for a whole
    /// statement of figures and pass it off as two columns of text.
    #[test]
    fn rows_of_amounts_keep_a_statement_from_reading_as_text() {
        let mut body = vec![
            nine(
                "Consolidated statement of financial position for the year",
                15.0,
                712.0,
                240.0,
            ),
            nine(
                "Consolidated statement of comprehensive income for the year",
                300.0,
                712.0,
                240.0,
            ),
        ];
        for i in 0..9 {
            let y = 700.0 - 12.0 * i as f64;
            let mut x = 15.0;
            for cell in [
                "438,082",
                "9,836,950",
                "3,304,152",
                "5,919,819",
                "429,405",
                "3,692,551",
                "840,333",
                "1,049,328",
                "6,086,009",
                "1,360,471",
            ] {
                body.push(nine(cell, x, y, 44.0));
                body.push(nine("\t", x + 48.0, y, 2.4));
                x += 54.0;
            }
        }
        let stats = PageStats::from_spans(&body);
        assert!(
            try_vertical_cut(&body, &stats, &[]).is_none(),
            "a statement of figures under two long headers was cut as two columns of text"
        );
    }

    /// An item may open with a quotation mark or a figure (codex, #2045):
    /// `“Revenue increased”` beside a column of bullets is still an item, and
    /// the bullets are still a list's.
    #[test]
    fn an_item_that_opens_with_punctuation_keeps_its_bullet() {
        let sized = |text: &str, x: f64, y: f64, width: f64, size: f64| TextSpan {
            height: size,
            font_size: size,
            ..span(text, x, y, width)
        };
        let mut spans = vec![sized(
            "The following items were reported during the course of the Financial Year:",
            86.8,
            720.0,
            313.3,
            10.2,
        )];
        for (k, (item, width)) in [
            ("\u{201c}Revenue increased\u{201d} across the group;", 190.0),
            ("(Formerly the Northern Division) was sold;", 180.0),
            ("2024 results were restated for the change.", 175.0),
            (
                "\u{201c}Margins improved\u{201d} in the second half.",
                170.0,
            ),
        ]
        .iter()
        .enumerate()
        {
            let y = 685.6 - 34.4 * k as f64;
            spans.push(sized("\u{2022}", 104.9, y - 4.5, 13.0, 19.0));
            spans.push(sized(item, 131.5, y, *width, 10.2));
        }
        for k in 0..4 {
            spans.push(sized(
                "Other than detailed in the Chairman's Report set out at pages 1 and 2 of this report",
                86.8,
                548.0 - 17.2 * k as f64,
                394.8,
                10.2,
            ));
        }
        let blocks = group_spans_into_blocks(spans, &[]);
        let bare = blocks
            .iter()
            .filter(|b| b.text().trim() == "\u{2022}")
            .count();
        assert_eq!(
            bare,
            0,
            "items opening with punctuation lost their bullets: {:?}",
            blocks.iter().map(|b| b.text()).collect::<Vec<_>>()
        );
    }

    /// A column of bullets is a list, not a column (#574).
    ///
    /// ASX_MRM's spans as the engine reads them: a 19 pt bullet 13 pt before
    /// each 10 pt item, its baseline 4.5 pt under the item's, and body lines
    /// round it. Every band of the list has a gap at one x, and read as two
    /// columns the list came back as four bullets and then four items.
    #[test]
    fn a_column_of_bullets_is_read_with_its_items() {
        let sized = |text: &str, x: f64, y: f64, width: f64, size: f64| TextSpan {
            height: size,
            font_size: size,
            ..span(text, x, y, width)
        };
        let mut spans = vec![
            sized(
                "Mermaid's principal activities during the course of the Financial Year were:",
                86.8,
                720.0,
                313.3,
                10.2,
            ),
            sized("PRINCIPAL ACTIVITIES", 416.8, 718.6, 123.3, 9.1),
        ];
        for (k, (item, width)) in [
            ("Operating crewed vessel charters;", 138.4),
            ("Vessel manning, management and logistics;", 179.8),
            ("Operating supply base facilities; and", 149.1),
            ("Equipment hire.", 67.1),
        ]
        .iter()
        .enumerate()
        {
            let y = 685.6 - 34.4 * k as f64;
            spans.push(sized("\u{2022}", 104.9, y - 4.5, 13.0, 19.0));
            spans.push(sized(item, 131.5, y, *width, 10.2));
        }
        for k in 0..4 {
            spans.push(sized(
                "Other than detailed in the Chairman's Report set out at pages 1 and 2 of this report",
                86.8,
                548.0 - 17.2 * k as f64,
                394.8,
                10.2,
            ));
        }
        let blocks = group_spans_into_blocks(spans, &[]);
        let bare = blocks
            .iter()
            .filter(|b| b.text().trim() == "\u{2022}")
            .count();
        assert_eq!(
            bare,
            0,
            "the bullets were read as a column of their own: {:?}",
            blocks.iter().map(|b| b.text()).collect::<Vec<_>>()
        );
    }

    /// A line number peeled off the line it was drawn into leaves a line whose
    /// transform still locates its own first glyph (codex, #1991).
    #[test]
    fn a_peeled_line_keeps_its_origin_on_its_first_glyph() {
        let glued = TextSpan {
            text: "10 example, aluminum and steel".into(),
            x: 298.8,
            y: 100.0,
            width: 229.4,
            height: 8.5,
            font_size: 8.5,
            transform: Some([8.5, 0.0, 0.0, 8.5, 298.8, 100.0]),
            char_bounds: std::iter::once([298.8, 100.0, 302.0, 108.5])
                .chain(std::iter::once([302.0, 100.0, 305.2, 108.5]))
                .chain((0..27).map(|k| {
                    let x = 310.9 + k as f64 * 4.0;
                    [x, 100.0, x + 4.0, 108.5]
                }))
                .collect(),
            ..TextSpan::default()
        };
        let mut spans = vec![glued];
        for i in 0..3 {
            spans.push(TextSpan {
                text: "a line of the right column".into(),
                x: 310.9,
                y: 90.0 - i as f64 * 10.0,
                width: 200.0,
                height: 8.5,
                font_size: 8.5,
                ..TextSpan::default()
            });
        }
        let (figure, rest) = peel_one(&spans[0], 0, &spans).expect("the figure is peeled");
        assert_eq!(figure.text, "10");
        assert_eq!(rest.text, "example, aluminum and steel");
        assert!((rest.x - 310.9).abs() < 1e-9);
        let t = rest.transform.expect("a transform");
        assert!(
            (t[4] - rest.x).abs() < 1e-9 && (t[5] - 100.0).abs() < 1e-9,
            "the rest's transform still points at the figure: {t:?}"
        );
    }

    /// A column the margin scan finds is a column of line numbers only when
    /// every member it found is a figure (codex, #1991): `i`, `1`, `ii` down a
    /// list is a list's markers, and one digit among them numbers no page.
    #[test]
    fn a_margin_column_of_mixed_labels_is_not_figures() {
        let band = |y: f64, label: &str| {
            let mut b = TextBand::new(TextSpan {
                text: "a line of the left column running its measure".into(),
                x: 77.0,
                y,
                width: 200.0,
                height: 9.0,
                font_size: 9.0,
                ..TextSpan::default()
            });
            b.spans.push(TextSpan {
                text: label.into(),
                x: 288.0,
                y,
                width: 6.0,
                height: 9.0,
                font_size: 9.0,
                ..TextSpan::default()
            });
            b.spans.push(TextSpan {
                text: "a line of the right column running its measure".into(),
                x: 300.0,
                y,
                width: 200.0,
                height: 9.0,
                font_size: 9.0,
                ..TextSpan::default()
            });
            b
        };
        let column = |labels: &[&str]| {
            let bands: Vec<TextBand> = labels
                .iter()
                .enumerate()
                .map(|(i, l)| band(700.0 - i as f64 * 11.0, l))
                .collect();
            a_margin_column(&bands, 423.0, 10.0, 2.5)
        };
        let (skipped, figures) = column(&["5", "10", "15", "20"]);
        assert!(figures && skipped.iter().all(|b| b.len() == 1));
        let (_, figures) = column(&["i", "1", "ii", "iii"]);
        assert!(!figures, "a mixed column was read as a column of figures");
    }

    /// Beside a bank of figures, the gutter's column is still the margin when it
    /// COUNTS the lines, and not when it is a table's row numbers (#574).
    #[test]
    fn a_counting_column_beside_a_bank_of_figures_is_the_margin() {
        let span = |text: &str, x: f64, width: f64, y: f64| TextSpan {
            text: text.into(),
            x,
            y,
            width,
            height: 9.0,
            font_size: 9.0,
            ..TextSpan::default()
        };
        let band = |y: f64, figure: &str, label: &str| {
            let mut b = TextBand::new(span("a line of the", 77.0, 60.0, y));
            b.spans.push(span(figure, 150.0, 12.0, y));
            b.spans
                .push(span("left column running its measure", 170.0, 107.0, y));
            b.spans.push(span(label, 288.0, 6.0, y));
            b.spans.push(span(
                "a line of the right column running its measure",
                300.0,
                200.0,
                y,
            ));
            b
        };
        let column = |labels: &[&str]| {
            let bands: Vec<TextBand> = ["0.0", "4.0", "64.9", "3.8"]
                .iter()
                .zip(labels)
                .enumerate()
                .map(|(i, (f, l))| band(700.0 - i as f64 * 11.0, f, l))
                .collect();
            a_margin_column(&bands, 423.0, 10.0, 2.5)
        };
        let (skipped, figures) = column(&["5", "10", "15", "20"]);
        assert!(
            figures && skipped.iter().all(|b| b.len() == 1 && b.contains(&3)),
            "the line numbers beside a bank were not the margin: {skipped:?}"
        );
        let (skipped, _) = column(&["1", "2", "3", "4"]);
        assert!(
            skipped.iter().all(HashSet::is_empty),
            "a table's row numbers were taken for line numbers: {skipped:?}"
        );
        let (skipped, _) = column(&["2018", "2020", "2022", "2024"]);
        assert!(
            skipped.iter().all(HashSet::is_empty),
            "a column of years was taken for line numbers: {skipped:?}"
        );
    }

    /// A span the gap scan skipped is not a cell boundary of a bank (#582).
    ///
    /// The caller that skips a thin column standing IN a gutter -- printed line
    /// numbers down the middle of a statute, which occupy every x of the gutter
    /// and make the cut unfindable -- has to say so here too. Counted as ink, a
    /// column beside the gutter puts a wide strip of white between itself and
    /// the rest of its side on every band, which is exactly the shape
    /// `the_cut_is_a_column_of_a_table` refuses a cut for: the guard would
    /// refuse the cut that skipping the column was meant to allow.
    #[test]
    fn a_span_the_scan_skipped_is_not_a_cell_boundary() {
        // Four bands. Each carries a line of the left column, a line number
        // sitting in the gutter, and a line of the right column.
        let mut spans = Vec::new();
        for band in 0..4 {
            let y = 700.0 - band as f64 * 14.0;
            spans.push(span("the committee considered the whole", 72.0, y, 150.0));
            spans.push(span("12", 240.0, y, 10.0));
            spans.push(span("of the proposals put before it and", 300.0, y, 150.0));
        }
        let stats = PageStats::from_spans(&spans);
        let bands = group_spans_into_bands_with_stats(spans.clone(), &stats);
        let cut_x = 270.0;
        let gap = 50.0;

        // With the line numbers counted as ink, the left side of the cut holds
        // two runs a band -- the text and the number -- with 18 points of white
        // between them, and each side reads as a bank of cells.
        let counted = one_side_of_a_cut(&bands, cut_x, true, cut_x - 72.0, 18.0, &[])
            .expect("the left side has ink on every band");
        assert_eq!(
            (counted.inked, counted.broken),
            (4, 4),
            "the line numbers are not being read as a second run of ink"
        );

        // Skipped, the side is one run of ink a band and breaks nowhere, so the
        // guard has nothing to refuse the cut for.
        // One set per band, which is the whole point: the index of the line
        // number is not the same number on every band.
        let skipped: Vec<HashSet<usize>> = bands
            .iter()
            .map(|band| {
                band.spans
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| s.text.trim() == "12")
                    .map(|(i, _)| i)
                    .collect()
            })
            .collect();
        let without = one_side_of_a_cut(&bands, cut_x, true, cut_x - 72.0, 18.0, &skipped)
            .expect("the left side still has ink on every band");
        assert_eq!(
            (without.inked, without.broken),
            (4, 0),
            "a span the scan skipped is still being measured as a cell boundary"
        );
        assert!(
            !the_cut_is_a_column_of_a_table(&bands, cut_x, gap, &skipped),
            "the cut was refused for a column the scan had already skipped"
        );
    }

    /// The gutter is only there in the MEASURED edges. `right()` returns
    /// x + width.max(estimated_width()), and on tracemonkey's abstract that
    /// estimate overshoots the measured edge by 61pt -- wider than the gutter
    /// it then covers, so the scan finds no corridor and the guards never run.
    #[test]
    fn a_column_gap_is_found_on_measured_edges_not_estimated_ones() {
        let spans = two_column_page();
        let body: Vec<TextSpan> = spans.iter().filter(|s| s.y < 720.0).cloned().collect();
        let stats = PageStats::from_spans(&body);

        let overshoots = body.iter().any(|s| s.right() > s.measured_right() + 1.0);
        assert!(
            overshoots,
            "fixture no longer exercises the overshoot; right() must exceed measured_right()              for this test to mean anything"
        );

        let cut = try_vertical_cut(&body, &stats, &[]);
        assert!(
            cut.is_some(),
            "no column cut found on a clean two-column body"
        );
    }

    fn block_texts(spans: Vec<TextSpan>) -> Vec<String> {
        block_texts_with_rules(spans, &[])
    }

    /// The same, with the horizontal rules the page drew (#545).
    fn block_texts_with_rules(spans: Vec<TextSpan>, rules: &[HorizontalRule]) -> Vec<String> {
        group_spans_into_blocks(spans, rules)
            .into_iter()
            .map(|block| block.text())
            .collect()
    }

    /// The same, but only the leaf of the XY-cut: the bands as
    /// `group_spans_into_blocks_legacy_from_bands` sees them.
    ///
    /// Some of the shapes below are refused by the leaf and taken by the column
    /// cut above it instead, which proves nothing about the leaf. This says what
    /// the leaf does with them.
    fn leaf_texts(spans: Vec<TextSpan>) -> Vec<String> {
        let bands = group_spans_into_bands(spans);
        group_spans_into_blocks_legacy_from_bands(bands, &[])
            .iter()
            .map(TextBlock::text)
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

    /// A header, three bands each of which draws two slices with a gutter
    /// between them, and a footer. Read as columns unless the page says
    /// otherwise, which is what the three tests after this one are about.
    fn two_column_region_spans() -> Vec<TextSpan> {
        vec![
            span("Header", 200.0, 740.0, 80.0),
            span("Left column line one", 40.0, 700.0, 115.0),
            span("Right column line one", 320.0, 700.0, 120.0),
            span("Left column line two", 40.0, 684.0, 115.0),
            span("Right column line two", 320.0, 684.0, 120.0),
            span("Left column line three", 40.0, 668.0, 125.0),
            span("Right column line three", 320.0, 668.0, 130.0),
            span("Footer", 200.0, 620.0, 80.0),
        ]
    }

    #[test]
    fn two_column_region_reads_column_major() {
        let texts = block_texts(two_column_region_spans());

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

    /// The rules the page draws between its rows, for the fixture above: a
    /// horizontal line across the whole table, in every gap between two rows.
    fn rules_between_the_rows() -> Vec<HorizontalRule> {
        [692.0, 676.0]
            .into_iter()
            .map(|y| HorizontalRule {
                left: 35.0,
                right: 445.0,
                y,
            })
            .collect()
    }

    /// The same three bands that read column by column above, with a ruling
    /// line between each pair of them, read across instead (#545). This is the
    /// Isartor appendix: two columns of text that are the two cells of a row,
    /// and nothing but the page's own graphics says so.
    #[test]
    fn a_ruled_two_column_region_is_read_row_by_row() {
        let texts = block_texts_with_rules(two_column_region_spans(), &rules_between_the_rows());

        assert_eq!(
            texts,
            vec![
                "Header",
                "Left column line one Right column line one",
                "Left column line two Right column line two",
                "Left column line three Right column line three",
                "Footer",
            ]
        );
    }

    /// A rule under one cell is not a rule between two rows. Widening the test
    /// to any line that happens to sit in the gap turns every underlined cell
    /// into a table, so the rule has to cross the region it claims to divide.
    #[test]
    fn a_rule_that_crosses_only_one_column_leaves_the_columns_alone() {
        let short: Vec<HorizontalRule> = rules_between_the_rows()
            .into_iter()
            .map(|rule| HorizontalRule {
                right: 160.0,
                ..rule
            })
            .collect();

        let texts = block_texts_with_rules(two_column_region_spans(), &short);

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

    /// One rule is a headline's underline, a footnote's separator, the line
    /// above a page number -- every document has them, and none of them makes
    /// the text below a table.
    ///
    /// Asked of the predicate rather than of the page, because a region of
    /// three bands with one rule in it is refused by the column cut above the
    /// leaf for its own reasons, and a page-level assertion would be green
    /// whatever this rule said.
    #[test]
    fn a_single_rule_does_not_make_a_region_a_table() {
        let bands = group_spans_into_bands(two_column_region_spans());
        let rows: Vec<usize> = bands
            .iter()
            .enumerate()
            .filter(|(_, band)| band.y < 720.0 && band.y > 640.0)
            .map(|(idx, _)| idx)
            .collect();
        assert_eq!(
            rows.len(),
            3,
            "the fixture's three rows are the region here"
        );

        let both = rules_between_the_rows();
        assert!(
            region_is_ruled(&bands, &rows, &both),
            "a rule in every gap of the region is a ruled table"
        );
        assert!(
            !region_is_ruled(&bands, &rows, &both[..1]),
            "one rule is not a table"
        );
    }

    /// One path may hold every rule a table draws, so a cap asked once per
    /// `draw_path` bounds nothing. The vector it would grow is walked per band
    /// by `region_is_ruled`, which is why the bound is on the vector and not on
    /// the call.
    #[test]
    fn one_path_of_many_rules_still_stops_at_the_cap() {
        let mut path = BezPath::new();
        for row in 0..(MAX_RECORDED_RULES * 2) {
            let y = 700.0 - row as f64;
            path.move_to((40.0, y));
            path.line_to((440.0, y));
            path.close_path();
        }

        let mut device = TextExtractionDevice::new();
        device.record_rule(&path, Affine::IDENTITY);

        assert_eq!(device.rules.len(), MAX_RECORDED_RULES);
    }

    /// A Form XObject's `/BBox` reaches this device as a clip, with the
    /// geometry inside it unclipped. A line ruled across one cell of such a
    /// form therefore arrives as a line across the page, and two of those are
    /// enough to refuse a column cut that should have been taken.
    #[test]
    fn a_rule_is_recorded_at_the_width_the_clip_leaves_it() {
        let mut wide = BezPath::new();
        wide.move_to((40.0, 600.0));
        wide.line_to((440.0, 600.0));

        let mut device = TextExtractionDevice::new();
        device.push_clip_path(&ClipPath {
            path: kurbo::Rect::new(40.0, 500.0, 160.0, 700.0).to_path(0.1),
            fill: pdf_render::pdf_interpret::FillRule::NonZero,
        });
        device.record_rule(&wide, Affine::IDENTITY);

        assert_eq!(device.rules.len(), 1, "the visible part is still a rule");
        assert!(
            device.rules[0].right < 161.0,
            "the rule was recorded past its clip: {:?}",
            device.rules[0]
        );

        let mut above = BezPath::new();
        above.move_to((40.0, 800.0));
        above.line_to((440.0, 800.0));
        device.record_rule(&above, Affine::IDENTITY);
        assert_eq!(
            device.rules.len(),
            1,
            "a rule the clip cuts away entirely was recorded: {:?}",
            device.rules
        );

        device.pop_clip_path();
        device.record_rule(&wide, Affine::IDENTITY);
        assert_eq!(
            device.rules[1].right, 440.0,
            "the clip outlived the pop: {:?}",
            device.rules[1]
        );
    }

    /// A table rules most of its gaps and a page of prose rules a couple: the
    /// share is what separates them, and it is measured rather than assumed.
    /// The Isartor appendix rules 26 of its 28 gaps; the two it leaves are the
    /// rows whose cell wraps, which is why the share is a half and not all.
    #[test]
    fn a_region_with_a_rule_in_a_minority_of_its_gaps_is_not_a_table() {
        let mut spans = Vec::new();
        for row in 0..6 {
            let y = 700.0 - row as f64 * 16.0;
            spans.push(span("Left column line", 40.0, y, 115.0));
            spans.push(span("Right column line", 320.0, y, 120.0));
        }
        let bands = group_spans_into_bands(spans);
        let rows: Vec<usize> = (0..bands.len()).collect();
        assert_eq!(bands.len(), 6, "six rows, so five gaps between them");

        let rule_at = |y: f64| HorizontalRule {
            left: 35.0,
            right: 445.0,
            y,
        };
        let two = vec![rule_at(692.0), rule_at(676.0)];
        let three = vec![rule_at(692.0), rule_at(676.0), rule_at(660.0)];

        assert!(
            !region_is_ruled(&bands, &rows, &two),
            "two of five is prose"
        );
        assert!(
            region_is_ruled(&bands, &rows, &three),
            "three of five is a table"
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

    /// A patent's front page: fields numbered by INID codes down the left,
    /// mostly short, with one long field line; references down the right on
    /// the same baselines. The long line is the column's measure, not a banner
    /// over both columns, so the page is read column by column (#574).
    #[test]
    fn a_patent_front_page_is_read_column_by_column() {
        // The measurements of 6942822's front page: a 14 pt gutter, the
        // column's long field line reaching into it, the rest ~150 pt.
        let fields = [
            ("(21) Appl. No.: 10/290,458", 150.0),
            ("(22) Filed: Nov. 8, 2002", 140.0),
            ("(51) Int. Cl. H01B 1/12", 160.0),
            ("(52) U.S. Cl. 252/500; 528/210", 150.0),
            ("(58) Field of Search .......... 252/500; 528/422", 219.0),
            ("(56) References Cited", 140.0),
        ];
        let mut spans = Vec::new();
        for (k, (field, width)) in fields.iter().enumerate() {
            let y = 700.0 - k as f64 * 16.0;
            spans.push(span(field, 77.0, y, *width));
            // The long field stands on a band of its own, as on the page:
            // the reference beside it ended a line higher.
            if k != 4 {
                spans.push(span(
                    "Journal of Polymer Science, vol. 38, pp. 3390",
                    310.0,
                    y,
                    220.0,
                ));
            }
        }
        let texts = block_texts(spans);
        assert!(
            texts
                .iter()
                .all(|t| !(t.contains(")") && t.contains("Journal"))),
            "a field was joined to the reference beside it: {texts:?}"
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

    /// A three-column table row whose middle cell wraps, at the block level:
    /// the reordering happens in the leaf of the XY-cut, and this is the leaf's
    /// own view of `8.2.5.26-t05-pass-d.pdf`'s second row (#434).
    #[test]
    fn a_wrapped_cell_is_emitted_with_its_own_cell() {
        let texts = block_texts(vec![
            span("Index", 60.0, 700.0, 30.0),
            span("Failure condition", 200.0, 700.0, 100.0),
            span("Section", 400.0, 700.0, 42.0),
            span("15-003", 60.0, 680.0, 36.0),
            span("In a table not organized", 200.0, 680.0, 144.0),
            span("UA1:7.5-2", 400.0, 680.0, 54.0),
            span("a cell without a Scope", 200.0, 668.0, 132.0),
        ]);

        assert_eq!(
            texts,
            vec![
                "Index Failure condition Section",
                "15-003",
                "In a table not organized",
                "a cell without a Scope",
                "UA1:7.5-2",
            ]
        );
    }

    /// The failure case the fix names: when every cell of the row has a line
    /// under it, nothing says which of those lines continues the row and which
    /// one begins the next -- the same shape a two-column region takes when one
    /// column runs on alone. Such a row keeps today's row-major order rather
    /// than being guessed at.
    /// The leading is deliberately tight, so that all three lines pass the
    /// distance test in their own cell and it is the last guard -- every cell
    /// of the row having a line under it -- that has to refuse the row. At that
    /// leading the column cut above the leaf takes the page, so this one asks
    /// the leaf directly.
    #[test]
    fn a_row_whose_every_cell_wraps_stays_row_major() {
        let texts = leaf_texts(vec![
            span("Index", 60.0, 700.0, 30.0),
            span("Failure condition", 200.0, 700.0, 100.0),
            span("Section", 400.0, 700.0, 42.0),
            span("15-003", 60.0, 680.0, 36.0),
            span("In a table not organized", 200.0, 680.0, 144.0),
            span("UA1:7.5-2", 400.0, 680.0, 54.0),
            span("is the index", 60.0, 674.0, 72.0),
            span("a cell without a Scope", 200.0, 668.0, 132.0),
            span("of the suite", 400.0, 662.0, 72.0),
        ]);

        assert_eq!(
            texts,
            vec![
                "Index Failure condition Section",
                "15-003 In a table not organized UA1:7.5-2",
                "is the index",
                "a cell without a Scope",
                "of the suite",
            ]
        );
    }

    /// The next row of a table with an empty cell also has fewer spans than the
    /// row above it and also starts on the same edges. It is the next row all
    /// the same, and what says so is that it fills two cells where a wrapped
    /// line fills one. The header of this table is padded, so the body's rows
    /// sit closer together than the pitch under the header and the distance
    /// alone would let this one pass.
    #[test]
    fn a_row_with_an_empty_cell_is_a_row_and_not_a_wrap() {
        let texts = block_texts(vec![
            span("Index", 60.0, 700.0, 30.0),
            span("Type", 200.0, 700.0, 24.0),
            span("How", 400.0, 700.0, 18.0),
            span("15-003", 60.0, 680.0, 36.0),
            span("Object", 200.0, 680.0, 36.0),
            span("Machine", 400.0, 680.0, 42.0),
            span("15-004", 60.0, 666.0, 36.0),
            span("Rope", 400.0, 666.0, 24.0),
        ]);

        assert_eq!(
            texts,
            vec!["Index Type How", "15-003 Object Machine", "15-004 Rope"]
        );
    }

    /// A row that fills a single cell, one line under a wrapped one. Measured
    /// against the wrapped line it is a line's distance away; measured against
    /// the cell it would join -- which has no line under it yet, so the row
    /// itself -- it is two, and it is a row.
    #[test]
    fn a_single_cell_row_under_a_wrapped_line_is_still_a_row() {
        let texts = block_texts(vec![
            span("Index", 60.0, 700.0, 30.0),
            span("Type", 200.0, 700.0, 24.0),
            span("How", 400.0, 700.0, 18.0),
            span("15-003", 60.0, 680.0, 36.0),
            span("In a table not organized", 200.0, 680.0, 144.0),
            span("UA1:7.5-2", 400.0, 680.0, 54.0),
            span("a cell without a Scope", 200.0, 668.0, 132.0),
            span("15-004", 60.0, 656.0, 36.0),
        ]);

        assert_eq!(
            texts,
            vec![
                "Index Type How",
                "15-003",
                "In a table not organized",
                "a cell without a Scope",
                "UA1:7.5-2",
                "15-004",
            ]
        );
    }

    /// The same, with a row that fills a single cell: nothing but the pitch
    /// separates it from a wrapped line, and the pitch is what says it.
    #[test]
    fn a_line_a_whole_row_pitch_down_is_a_row_and_not_a_wrap() {
        let texts = block_texts(vec![
            span("Index", 60.0, 700.0, 30.0),
            span("Type", 200.0, 700.0, 24.0),
            span("How", 400.0, 700.0, 18.0),
            span("15-003", 60.0, 680.0, 36.0),
            span("Object", 200.0, 680.0, 36.0),
            span("Machine", 400.0, 680.0, 42.0),
            span("Rope", 200.0, 660.0, 24.0),
        ]);

        assert_eq!(
            texts,
            vec!["Index Type How", "15-003 Object Machine", "Rope"]
        );
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
    fn a_space_drawn_over_a_ligature_is_no_word_space() {
        // `Speciﬁ` then ` cally`, the second run starting 0.3 em before the
        // first ends (#574): the page draws one word in two runs, and a real
        // word space never starts left of the letter before it.
        let block = TextBlock {
            spans: vec![
                span("Speciﬁ", 0.0, 0.0, 60.0),
                span(" cally", 56.4, 0.0, 36.0),
            ],
        };
        assert_eq!(block.text(), "Speciﬁcally");
        assert_eq!(crate::ligatures::fold(&block.text()), "Specifically");
    }

    #[test]
    fn a_space_drawn_over_an_initial_is_no_word_space() {
        // `T` then ` ransition`, the second run starting 0.6 em back: the
        // same signature without a ligature (#574).
        let block = TextBlock {
            spans: vec![span("T", 0.0, 0.0, 8.0), span(" ransition", 0.8, 0.0, 60.0)],
        };
        assert_eq!(block.text(), "Transition");
    }

    #[test]
    fn a_real_word_space_stays() {
        // `There` then ` is`, the second run starting 0.2 em AFTER the first
        // ends: a gap in reading direction is a word space (#574).
        let block = TextBlock {
            spans: vec![span("There", 0.0, 0.0, 30.0), span(" is", 32.4, 0.0, 15.0)],
        };
        assert_eq!(block.text(), "There is");
    }

    #[test]
    fn an_overlapping_run_that_starts_with_a_capital_or_digit_keeps_its_space() {
        // Formulas and tables overlap runs that open with a capital or a
        // digit; only a lower-case letter joins (#574).
        let digit = TextBlock {
            spans: vec![span("v", 0.0, 0.0, 8.0), span(" 2", 4.4, 0.0, 10.0)],
        };
        assert_eq!(digit.text(), "v 2");
        let capital = TextBlock {
            spans: vec![span("v", 0.0, 0.0, 8.0), span(" N", 4.4, 0.0, 10.0)],
        };
        assert_eq!(capital.text(), "v N");
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
        dev.pending_tj_offset = -250.0; // full em-space, to the right
        assert!(dev.evaluate_space_consensus(0.5, 12.0, "Hello", "World"));
    }

    #[test]
    fn a_pair_kern_of_any_size_is_not_a_word_space() {
        // `[(FINANCIAL RESUL)134.6(TS)] TJ`: the positive TJ pulls the `T`
        // back under the `L`, and the gap it leaves is below zero.
        let mut dev = make_device_with_median(6.0);
        dev.pending_tj_offset = 134.6;
        assert!(!dev.evaluate_space_consensus(-1.15, 8.5, "RESUL", "TS"));
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
        // A negative TJ offset moves the next glyph right, an explicit
        // inter-substring shift, and counts toward the consensus.
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

    /// One table whose rows are grouped under labels is not cut between the
    /// groups (#596).
    ///
    /// The shape is the tuning set's commonest table: a label column whose cell is
    /// filled on the first row of a group and empty under it, with a little more
    /// white between the groups than between the rows. The threshold of
    /// `try_horizontal_cut` is a multiple of the ROW pitch, so the gap between
    /// groups clears it and the table is cut into one region per group -- on
    /// `PMC3037919-p0007` that is twelve tables where the page holds one.
    ///
    /// Red without the guard: `try_horizontal_cut` returns a cut.
    #[test]
    fn one_table_is_not_cut_between_its_label_groups() {
        // Four columns, with gaps wide enough to separate runs, and a label column
        // that is empty on every row but the first of each group.
        let row = |label: &str, second: &str, third: &str, fourth: &str, y: f64| {
            let mut spans = Vec::new();
            if !label.is_empty() {
                spans.push(span(label, 72.0, y, 58.0));
            }
            spans.push(span(second, 160.0, y, 90.0));
            spans.push(span(third, 300.0, y, 30.0));
            spans.push(span(fourth, 360.0, y, 40.0));
            spans
        };
        let mut spans = Vec::new();
        spans.extend(row("Study Design", "Case control", "7", "25.93", 700.0));
        spans.extend(row("", "Cohort", "9", "33.33", 688.0));
        spans.extend(row("", "Cross-sectional", "11", "40.74", 676.0));
        // 24 points to the next group, against a 12 point row pitch.
        spans.extend(row("Gender studied", "Both", "21", "77.78", 652.0));
        spans.extend(row("", "Men", "3", "11.11", 640.0));

        let stats = PageStats::from_spans(&spans);
        let bands = group_spans_into_bands(spans.clone());
        let cut_y = (676.0 + 652.0) * 0.5;
        let gap = 676.0 - 652.0;

        // The premises, so the fixture cannot quietly stop being the defect: the
        // gap clears the unguarded threshold, and the leading floor of #580 does
        // not already refuse it.
        let unguarded = stats.median_line_spacing * PARAGRAPH_BREAK_LINE_SPACING_MULTIPLIER;
        assert!(
            gap >= unguarded,
            "the gap {gap} does not clear the unguarded threshold {unguarded}, so \
             this fixture would pass without the guard"
        );
        let floor = leading_of_a_region(&spans, &stats) * HORIZONTAL_CUT_LEADING_FLOOR;
        assert!(
            gap >= floor,
            "the #580 floor {floor} already refuses this gap {gap}, so the guard \
             is not what this test measures"
        );

        assert!(
            the_cut_is_inside_a_grid(&bands, cut_y, gap),
            "the bands either side of the cut were not read as one grid"
        );
        assert!(
            try_horizontal_cut(&spans, &stats).is_none(),
            "the table was cut between its label groups"
        );
    }

    /// Three one-line zones, equally spaced, are not a leading (codex on #580,
    /// round two).
    ///
    /// Their two gaps are every gap the region has, so counting occurrences
    /// cannot tell them from three lines of a paragraph. What can is the left
    /// edge: lines set together are flush, and a masthead over a centred title
    /// over an indented byline are not.
    ///
    /// Red without the alignment filter: the zone gap becomes the leading, the
    /// floor rises above it and both of the region's cuts are refused.
    #[test]
    fn three_zones_equally_spaced_are_not_a_leading() {
        let stats = PageStats::from_spans(&[span("body", 72.0, 700.0, 100.0)]);
        let em = stats.median_font_size;
        // Equally spaced by 20pt, which is inside [0.8, 3.0] ems of a 12pt line.
        let zones = vec![
            span("MASTHEAD", 72.0, 700.0, 120.0),
            span("A centred title", 200.0, 680.0, 120.0),
            span("by a named author", 140.0, 660.0, 120.0),
        ];
        assert!(
            20.0 >= LEADING_MIN_EM * em && 20.0 <= LEADING_MAX_EM * em,
            "the fixture's gap is outside the bounds, so it never reaches the rule"
        );
        assert_eq!(
            leading_of_a_region(&zones, &stats),
            0.0,
            "three zones that are not flush were read as a leading"
        );

        // And the same three steps DO count when the bands are flush, or the
        // filter would refuse every real paragraph as well.
        let lines = vec![
            span("the first line of a paragraph", 72.0, 700.0, 180.0),
            span("the second line of the same one", 72.0, 680.0, 180.0),
            span("and the third line of it", 72.0, 660.0, 150.0),
        ];
        assert_eq!(
            leading_of_a_region(&lines, &stats),
            20.0,
            "three flush lines did not yield their own leading"
        );
    }

    /// A region of two bands has no leading, so the floor does not refuse its
    /// only cut (codex on #580).
    ///
    /// The shape has to be a two-band REGION on a page that is not two bands,
    /// because `PageStats` is page-wide: a page of two bands would put its only
    /// gap in `median_line_spacing` too and the threshold would refuse the cut
    /// with or without a floor. What the review describes is a page whose
    /// quartile is pulled down by dense content -- a table -- and a sparse region
    /// further down that the recursion reaches with two bands in it.
    ///
    /// Here the table sets the quartile to 6, so the threshold is 10.8; the
    /// widest gap on the page is the 82 points above the banner, so the first cut
    /// separates the table from everything below it and the region left over is
    /// the banner and one line of body, 25 points apart. Without the occurrence
    /// rule that 25 is "the leading" of a two-band region on a count of one, the
    /// floor rises to 26.25, and the only cut that region has is refused: the
    /// banner is glued to the body it is not part of.
    ///
    /// Red without the occurrence rule.
    #[test]
    fn two_bands_have_no_leading_so_the_only_cut_survives() {
        let mut spans = Vec::new();
        // Dense rows, to pull the page-wide quartile under the gap below.
        for (row, y) in [700.0, 694.0, 688.0, 682.0].iter().enumerate() {
            spans.push(span(["SWA", "BHM", "SBS", "VBS"][row], 72.0, *y, 30.0));
            spans.push(span("2011", 150.0, *y, 30.0));
        }
        spans.push(span("Federal Register banner", 72.0, 600.0, 260.0));
        spans.push(span(
            "the body of the notice begins here",
            72.0,
            575.0,
            200.0,
        ));

        let stats = PageStats::from_spans(&spans);
        let region = vec![
            span("Federal Register banner", 72.0, 600.0, 260.0),
            span("the body of the notice begins here", 72.0, 575.0, 200.0),
        ];
        assert_eq!(
            leading_of_a_region(&region, &stats),
            0.0,
            "a single gap was read as the leading of a two-band region"
        );
        // And the fixture is the review's case: the gap is inside the bounds, so
        // only the occurrence rule keeps it from becoming a floor above itself.
        let gap = 600.0 - 575.0;
        assert!(
            gap >= LEADING_MIN_EM * stats.median_font_size
                && gap <= LEADING_MAX_EM * stats.median_font_size,
            "the fixture's gap is refused by the bounds, so it would not reach \
             the occurrence rule"
        );
        assert!(
            gap > stats.median_line_spacing * PARAGRAPH_BREAK_LINE_SPACING_MULTIPLIER,
            "the cut would not fire even unfloored, so this fixture proves nothing"
        );

        let texts = block_texts(spans);
        assert!(
            texts.iter().any(|t| t == "Federal Register banner"),
            "the banner was glued to the body: {texts:?}"
        );
    }

    /// A paragraph is not cut between its own lines because something tighter
    /// than the body is on the page too (#580).
    ///
    /// The shape is the one the tuning set is full of: a bank of table rows set
    /// on 6 points over a body set on 14. `median_line_spacing` is the FIRST
    /// QUARTILE of the band steps, so it lands on the table's pitch (6) and the
    /// paragraph-break threshold on 10.8 -- under the body's leading. Every line
    /// break then qualifies as a zone break, the recursion cuts the paragraph
    /// into one region per line, and `blocks::paragraphs` cannot put it back
    /// because its stretches are bounded per region.
    ///
    /// Red without the floor: the four lines come back as four blocks.
    #[test]
    fn a_paragraph_is_not_cut_at_its_own_leading_because_a_table_is_tighter() {
        fn line(text: &str, x: f64, y: f64) -> TextSpan {
            // The measured width kept equal to `estimated_width()` (font_size *
            // 0.5 * chars), so the line fills the measure by both readings and
            // the merge is deciding about the gap and nothing else.
            span(text, x, y, 6.0 * text.chars().count() as f64)
        }

        let mut spans = Vec::new();
        // A bank of figures on a 6 point pitch: three columns, four rows.
        for (row, y) in [700.0, 694.0, 688.0, 682.0].iter().enumerate() {
            spans.push(line(["SWA", "BHM", "SBS", "VBS"][row], 72.0, *y));
            spans.push(line("2011", 150.0, *y));
            spans.push(line(["11820", "2618", "3504", "2215"][row], 230.0, *y));
        }
        // One paragraph of prose on a 14 point leading, every line but the last
        // filling the measure and none of them ending a sentence.
        let prose = [
            "the occurrence of brachygnathia inferior and hernias",
            "showed statistically highly significant differences be",
            "tween breeds and the most common defect was recorded",
            "in the flocks of the region.",
        ];
        for (text, y) in prose.iter().zip([640.0, 626.0, 612.0, 598.0]) {
            spans.push(line(text, 72.0, y));
        }

        let stats = PageStats::from_spans(&spans);
        let leading = leading_of_a_region(&spans, &stats);
        assert!(
            stats.median_line_spacing < leading,
            "the fixture is not the defect: quartile {} is not under the leading {leading}",
            stats.median_line_spacing
        );
        assert!(
            stats.median_line_spacing * PARAGRAPH_BREAK_LINE_SPACING_MULTIPLIER < leading,
            "the unfloored threshold {} is not under the leading {leading}, so this \
             fixture would pass without the fix",
            stats.median_line_spacing * PARAGRAPH_BREAK_LINE_SPACING_MULTIPLIER
        );

        // `paragraph_blocks` and not `block_texts`: the merge is what this is
        // about, and `block_texts` stops at the bands.
        let texts: Vec<String> = paragraph_blocks(spans, &[], 0, 0, TableStrategy::Both)
            .into_iter()
            .map(|block| block.text)
            .collect();
        let whole = prose.join(" ");
        assert!(
            texts.iter().any(|block| block == &whole),
            "the paragraph was cut at its own leading: {texts:?}"
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

    /// A span drawn at `degrees`, with the transform a text matrix at that
    /// angle composes and the origin the glyphs start from.
    fn turned_span(text: &str, degrees: f64, x: f64, y: f64, width: f64) -> TextSpan {
        let (sin, cos) = degrees.to_radians().sin_cos();
        let scale = 12.0 / 1000.0;
        TextSpan {
            transform: Some([cos * scale, sin * scale, -sin * scale, cos * scale, x, y]),
            ..span(text, x, y, width)
        }
    }

    /// Two lines of one rotated block share the place the page's order held
    /// them, and they must come back in the order they are read in. Each is
    /// inserted at the same index, and an insertion there pushes the last one
    /// down -- so inserting them front to back would emit the block backwards
    /// (#435, review round 2).
    #[test]
    fn two_lines_at_one_place_come_back_in_reading_order() {
        // A 90-degree run reads up the page, and its next line is a leading
        // further along +x: text space moves down, and down is +x once the
        // matrix has turned a quarter turn.
        let mut spans = vec![span("the page itself", 100.0, 700.0, 80.0)];
        for (index, text) in ["first", "second"].iter().enumerate() {
            let x = 60.0 + 14.0 * index as f64;
            spans.push(turned_span(text, 90.0, x, 200.0, 30.0));
        }
        let blocks = vec![TextBlock { spans }];

        let out = reassemble_cross_direction_runs(blocks);

        let lines: Vec<String> = out.iter().map(|block| block.text()).collect();
        assert_eq!(
            lines,
            vec!["first", "second", "the page itself"],
            "a rotated block came back with its lines the wrong way round"
        );
    }

    /// The page's direction is weighed in glyphs, and a span holds more
    /// characters than glyphs whenever a glyph decodes to several of them or
    /// the space consensus put a space between two -- neither of which was
    /// drawn. Counting characters lets a minority direction outvote the page's
    /// own (#435, review round 2).
    #[test]
    fn the_pages_direction_is_weighed_in_glyphs_and_not_characters() {
        // Eight characters drawn as two glyphs: a pair of ligatures.
        let mut upright = span("fifffifl", 72.0, 700.0, 24.0);
        upright.char_bounds = vec![[0.0; 4]; 2];
        // Five characters drawn as five glyphs.
        let mut turned = turned_span("TURNS", 90.0, 300.0, 300.0, 30.0);
        turned.char_bounds = vec![[0.0; 4]; 5];

        let direction = page_direction(&[TextBlock {
            spans: vec![upright, turned],
        }])
        .expect("a direction");

        assert!(
            (direction.1).abs() > 0.5,
            "two ligature glyphs outvoted five drawn ones, so the count is of characters"
        );
    }

    /// A run drawn at an angle stops where its last glyph stops, and that is
    /// further along its baseline than its page-space width says: a 30-degree
    /// run advances `cos 30` of its length in x. Measuring the gap to the next
    /// span from `x + width` leaves an eighth of the run as phantom gap, which
    /// at a style boundary in the middle of a word puts a space there (#435,
    /// review round 1).
    #[test]
    fn a_turned_run_ends_where_its_last_glyph_ends() {
        const DEGREES: f64 = 30.0;
        const ADVANCE: f64 = 7.2; // Courier at 12 pt
        let (sin, cos) = DEGREES.to_radians().sin_cos();

        // "HELLO", five glyphs from (100, 400) along the 30-degree baseline,
        // as `draw_glyph` accumulates them: page-space bounds per glyph, and a
        // width that is the widest reach in page x.
        let mut hello = turned_span("HELLO", DEGREES, 100.0, 400.0, 0.0);
        for index in 0..5 {
            let x = 100.0 + cos * ADVANCE * index as f64;
            let y = 400.0 + sin * ADVANCE * index as f64;
            hello.char_bounds.push([x, y, x + ADVANCE, y + 12.0]);
            hello.width = hello.width.max(x + ADVANCE - 100.0);
        }
        // "WORLD" begins exactly where the fifth glyph's advance ended.
        let world = turned_span(
            "WORLD",
            DEGREES,
            100.0 + cos * ADVANCE * 5.0,
            400.0 + sin * ADVANCE * 5.0,
            ADVANCE * 5.0,
        );

        let block = TextBlock {
            spans: vec![hello, world],
        };
        assert_eq!(
            block.text(),
            "HELLOWORLD",
            "a space was invented at a seam the document draws closed"
        );
    }

    /// A page written entirely at an angle is a page of upright text seen
    /// turned, and its own bands read it correctly. Nothing is taken out of
    /// them: the direction most of the glyphs are drawn in IS the page's,
    /// whatever that direction is (#435).
    #[test]
    fn a_page_written_entirely_at_an_angle_keeps_its_own_bands() {
        let blocks = vec![
            TextBlock {
                spans: vec![
                    turned_span("first line", 90.0, 100.0, 100.0, 60.0),
                    turned_span("carried on", 90.0, 100.0, 160.0, 60.0),
                ],
            },
            TextBlock {
                spans: vec![turned_span("second line", 90.0, 120.0, 100.0, 66.0)],
            },
        ];

        let out = reassemble_cross_direction_runs(blocks.clone());

        let shape: Vec<Vec<&str>> = out
            .iter()
            .map(|b| b.spans.iter().map(|s| s.text.as_str()).collect())
            .collect();
        assert_eq!(
            shape,
            vec![vec!["first line", "carried on"], vec!["second line"]],
            "a page in one direction was rearranged by a rule that is about two"
        );
    }

    /// Turned glyphs on one baseline come back as one span, ordered along the
    /// baseline with a word space where the gap is one (#574).
    #[test]
    fn turned_glyphs_on_one_baseline_merge_into_one_span() {
        // `AB CD` at 90 degrees: the glyphs climb the page one span each,
        // with a word gap between the words wide enough to part the run at
        // the cut, which is what merges it.
        let spans = vec![
            turned_span("A", 90.0, 100.0, 100.0, 7.0),
            turned_span("B", 90.0, 100.0, 107.0, 7.0),
            turned_span("C", 90.0, 100.0, 128.0, 7.0),
            turned_span("D", 90.0, 100.0, 135.0, 7.0),
        ];
        let out = merge_rotated_runs(spans);
        assert_eq!(out.len(), 1, "the run was not joined: {out:?}");
        assert_eq!(out[0].text, "AB CD");
        assert_eq!((out[0].x, out[0].y), (100.0, 100.0));
    }

    /// Two runs that share a baseline but stand more than a word gap apart
    /// stay two (#574). Each piece earns its own merge: the tight pair stays
    /// as drawn under the lone-tight rule, exactly as if it stood alone.
    #[test]
    fn turned_runs_split_where_the_white_is_wider_than_any_word_gap() {
        let spans = vec![
            turned_span("A", 90.0, 100.0, 100.0, 7.0),
            turned_span("B", 90.0, 100.0, 107.0, 7.0),
            turned_span("C", 90.0, 100.0, 127.0, 7.0),
            turned_span("D", 90.0, 100.0, 180.0, 7.0),
            turned_span("E", 90.0, 100.0, 187.0, 7.0),
        ];
        let out = merge_rotated_runs(spans);
        let texts: Vec<&str> = out.iter().map(|span| span.text.as_str()).collect();
        assert_eq!(texts, vec!["AB C", "D", "E"]);
    }

    /// A style change still ends a turned run, the way it ends a span in the
    /// device: a bold word inside one keeps its face (#574).
    #[test]
    fn a_bold_word_inside_a_turned_run_keeps_its_own_span() {
        let lawful = turned_span("A", 90.0, 100.0, 100.0, 7.0);
        let mut bold = turned_span("B", 90.0, 100.0, 107.0, 7.0);
        bold.is_bold = true;
        let out = merge_rotated_runs(vec![lawful, bold]);
        let texts: Vec<&str> = out.iter().map(|span| span.text.as_str()).collect();
        assert_eq!(texts, vec!["A", "B"]);
        assert!(!out[0].is_bold && out[1].is_bold);
    }

    /// A skew below ten degrees is not quarter-turned: its spans come back as
    /// they went in (#574).
    #[test]
    fn a_skew_below_ten_degrees_is_left_alone() {
        let spans = vec![
            turned_span("A", 3.0, 100.0, 100.0, 7.0),
            turned_span("B", 3.0, 107.0, 100.5, 7.0),
        ];
        assert!(!spans.iter().any(span_is_quarter_turned));
        let out = merge_rotated_runs(spans);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].text, "A");
        assert_eq!(out[1].text, "B");
    }

    /// Upright spans are never candidates, however they stack (#574).
    #[test]
    fn upright_spans_come_back_untouched() {
        let spans = vec![
            span("upper", 100.0, 200.0, 35.0),
            span("lower", 100.0, 100.0, 35.0),
        ];
        let out = merge_rotated_runs(spans);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].text, "upper");
        assert_eq!(out[1].text, "lower");
    }

    /// A turned span with no partner on its baseline keeps its place and its
    /// shape (#574).
    #[test]
    fn a_turned_span_with_no_partner_is_left_exactly_as_it_was() {
        let spans = vec![
            span("body", 72.0, 700.0, 30.0),
            turned_span("!", 90.0, 500.0, 300.0, 7.0),
        ];
        let out = merge_rotated_runs(spans);
        assert_eq!(out.len(), 2);
        assert_eq!(
            (out[1].text.as_str(), out[1].x, out[1].y),
            ("!", 500.0, 300.0)
        );
    }

    /// A turned span's right edge is measured, not estimated from its
    /// characters: the estimate runs along the baseline, which for a run
    /// stood up the page is not an x extent at all (#574).
    #[test]
    fn a_turned_spans_right_edge_is_measured() {
        let turned = turned_span("a whole head", 90.0, 100.0, 540.0, 7.0);
        assert_eq!(turned.right(), 107.0);
        let upright = span("a whole head", 100.0, 540.0, 72.0);
        assert_eq!(upright.right(), 172.0);
    }

    /// A lone run with tight gaps is left as drawn: #435 already reads it
    /// forwards, and merging it only changes the shape downstream counts
    /// (#574).
    #[test]
    fn a_lone_tight_run_is_left_as_drawn() {
        let spans = vec![
            turned_span("A", 90.0, 100.0, 100.0, 7.0),
            turned_span("B", 90.0, 100.0, 107.0, 7.0),
        ];
        let out = merge_rotated_runs(spans);
        let texts: Vec<&str> = out.iter().map(|span| span.text.as_str()).collect();
        assert_eq!(texts, vec!["A", "B"]);
    }

    /// Capitals never join across the small digits set between them: a gap
    /// another turned run's glyph starts in is not a word gap of this run,
    /// however word-wide it is (#574: an axis label's `Delta Omega 01 W`).
    #[test]
    fn capitals_do_not_join_across_digits_set_between_them() {
        let capitals = [
            turned_span("O", 90.0, 100.0, 100.0, 7.0),
            turned_span("W", 90.0, 100.0, 128.0, 7.0),
        ];
        let mut zero = turned_span("0", 90.0, 101.8, 110.0, 7.0);
        zero.font_name = Some("SmallDigits".to_string());
        let mut one = turned_span("1", 90.0, 101.8, 117.0, 7.0);
        one.font_name = Some("SmallDigits".to_string());
        // The capitals 21 points apart would merge on the gap alone; the
        // digits starting inside it part them instead.
        let out = merge_rotated_runs(vec![capitals[0].clone(), zero, one, capitals[1].clone()]);
        let texts: Vec<&str> = out.iter().map(|span| span.text.as_str()).collect();
        assert_eq!(texts, vec!["O", "0", "1", "W"]);
        // And the control: nothing between the capitals, the run joins.
        let out = merge_rotated_runs(capitals.to_vec());
        assert_eq!(out.len(), 1, "the control run did not join: {out:?}");
        assert_eq!(out[0].text, "O W");
    }

    /// Parts of one run split by a style change are consecutive, not beside
    /// each other: their reading-first baselines stand an advance apart, so
    /// neither is the other's sibling and both stay as drawn (#574).
    #[test]
    fn style_split_parts_of_one_run_are_not_siblings() {
        let mut bold_a = turned_span("A", 90.0, 100.0, 100.0, 7.0);
        bold_a.is_bold = true;
        let mut bold_b = turned_span("B", 90.0, 100.0, 107.0, 7.0);
        bold_b.is_bold = true;
        let spans = vec![
            bold_a,
            bold_b,
            turned_span("C", 90.0, 100.0, 114.0, 7.0),
            turned_span("D", 90.0, 100.0, 121.0, 7.0),
        ];
        let out = merge_rotated_runs(spans);
        let texts: Vec<&str> = out.iter().map(|span| span.text.as_str()).collect();
        assert_eq!(texts, vec!["A", "B", "C", "D"]);
    }

    /// A run with a gap on the paragraph-break scale merges: the cut would
    /// part it there, and the pieces come back in page order (#574).
    #[test]
    fn a_run_with_a_cut_scale_gap_merges() {
        let spans = vec![
            turned_span("A", 90.0, 100.0, 100.0, 7.0),
            turned_span("B", 90.0, 100.0, 130.0, 7.0),
        ];
        let out = merge_rotated_runs(spans);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].text, "A B");
    }

    /// Two turned runs beside each other merge: the grid would row them
    /// together, one-glyph cells across (#574). Two stacked runs, whose
    /// reading-first baselines stand rows apart, stay as drawn.
    #[test]
    fn sibling_runs_beside_each_other_merge() {
        let beside = vec![
            turned_span("A", 90.0, 100.0, 100.0, 7.0),
            turned_span("B", 90.0, 100.0, 107.0, 7.0),
            turned_span("C", 90.0, 200.0, 100.0, 7.0),
            turned_span("D", 90.0, 200.0, 107.0, 7.0),
        ];
        let out = merge_rotated_runs(beside);
        let texts: Vec<&str> = out.iter().map(|span| span.text.as_str()).collect();
        assert_eq!(texts, vec!["AB", "CD"]);

        let stacked = vec![
            turned_span("A", 90.0, 100.0, 100.0, 7.0),
            turned_span("B", 90.0, 100.0, 107.0, 7.0),
            turned_span("C", 90.0, 100.0, 200.0, 7.0),
            turned_span("D", 90.0, 100.0, 207.0, 7.0),
        ];
        let out = merge_rotated_runs(stacked);
        let texts: Vec<&str> = out.iter().map(|span| span.text.as_str()).collect();
        assert_eq!(texts, vec!["A", "B", "C", "D"]);
    }

    /// The page's direction is the one most of its GLYPHS are drawn in, and a
    /// tie goes to the direction that was drawn first -- a rule the answer to
    /// which must not depend on which way an iterator happens to walk (#435).
    #[test]
    fn the_pages_direction_is_the_heaviest_and_a_tie_goes_to_the_first() {
        let upright = TextBlock {
            spans: vec![span("eight ch", 0.0, 700.0, 48.0)],
        };
        let turned = TextBlock {
            spans: vec![turned_span("ninety deg", 90.0, 300.0, 300.0, 60.0)],
        };

        let heavier = page_direction(&[upright.clone(), turned.clone()]).expect("a direction");
        assert!(
            (heavier.1).abs() > 0.5,
            "ten glyphs at ninety degrees outweigh eight upright ones, and did not"
        );

        let tie = TextBlock {
            spans: vec![span("ninety deg", 0.0, 700.0, 60.0)],
        };
        let first = page_direction(&[tie, turned]).expect("a direction");
        assert!(
            (first.1).abs() < 0.5,
            "a tie went to the direction that was not drawn first"
        );
    }

    /// A page whose every line runs up it is read in the frame it was written
    /// in. Its bands lie across its lines, so without the turn the two lines
    /// share one band and come back as one (#546).
    #[test]
    fn a_turned_page_is_read_in_the_frame_it_was_written_in() {
        // A quarter-turned page reads up the page, and its next line is a
        // leading further along +x: text space moves down, and down is +x once
        // the matrix has turned a quarter turn.
        let spans: Vec<TextSpan> = ["the first line", "the second line"]
            .iter()
            .enumerate()
            .map(|(index, text)| turned_span(text, 90.0, 100.0 + 16.0 * index as f64, 100.0, 80.0))
            .collect();

        let lines = block_texts(spans);

        assert_eq!(
            lines,
            vec!["the first line", "the second line"],
            "the lines of a turned page were read in the bands of the frame it is drawn in"
        );
    }

    /// The turn into the page's own frame and the turn back move no coordinate
    /// by a single bit: a quarter turn's sine and cosine are 0 and ±1, so what
    /// a caller reads off a span of a turned page is the page's own number and
    /// not a rounding of it (#546).
    #[test]
    fn the_turn_into_the_frame_and_back_leaves_every_number_as_it_was() {
        // Courier at 12 pt, and a run of six glyphs up the page.
        const ADVANCE: f64 = 7.2;
        const X: f64 = 123.75;
        const Y: f64 = 456.5;
        // The width is what `draw_glyph` accumulates: the furthest a glyph
        // reaches in page x, which for a quarter turn is one advance however
        // long the run is. Written as that subtraction and not as the advance,
        // because `x + advance - x` is not `advance` in binary and the number
        // the caller gets back is the number the device measured.
        let mut original = turned_span("TURNED", 90.0, X, Y, (X + ADVANCE) - X);
        for index in 0..6 {
            let y = Y + ADVANCE * index as f64;
            original.char_bounds.push([X, y, X + ADVANCE, y + 12.0]);
        }

        for turn in [PageTurn::Quarter, PageTurn::Half, PageTurn::ThreeQuarter] {
            let spans = turn.into_frame(vec![original.clone()]);
            let blocks = turn.back(vec![TextBlock { spans }]);
            let span = &blocks[0].spans[0];

            assert_eq!(
                (span.x, span.y),
                (original.x, original.y),
                "{turn:?} moved the span's origin"
            );
            assert_eq!(
                span.transform, original.transform,
                "{turn:?} left the baseline turned"
            );
            assert_eq!(
                span.char_bounds, original.char_bounds,
                "{turn:?} moved a glyph bound"
            );
            assert_eq!(
                span.width, original.width,
                "{turn:?} left a frame width behind"
            );
        }
    }

    /// Only a page written in a quarter turn is turned: an upright one, a page
    /// drawn entirely at a free angle, and a page whose turned text is the
    /// minority all keep the bands they have (#546).
    #[test]
    fn a_page_is_turned_by_a_quarter_turn_or_by_nothing() {
        let cases = [
            (0.0, PageTurn::None),
            (90.0, PageTurn::Quarter),
            (180.0, PageTurn::Half),
            (270.0, PageTurn::ThreeQuarter),
            (45.0, PageTurn::None),
            (-52.0, PageTurn::None),
        ];
        for (degrees, expected) in cases {
            let spans = vec![turned_span("a line of it", degrees, 100.0, 100.0, 72.0)];
            assert_eq!(
                PageTurn::of(&spans),
                expected,
                "a page written at {degrees} degrees"
            );
        }

        // The watermark of #435: turned, and outnumbered by the page it is on.
        let page = vec![
            span(
                "the body of the page is written upright",
                72.0,
                700.0,
                230.0,
            ),
            turned_span("SANDBOX", 90.0, 300.0, 200.0, 42.0),
        ];
        assert_eq!(
            PageTurn::of(&page),
            PageTurn::None,
            "a turned watermark turned the page it is drawn on"
        );
    }

    /// A half-turned page keeps the ruling lines the page drew, because a half
    /// turn maps a horizontal line to a horizontal line; a quarter-turned page
    /// keeps none, because what the device recorded there runs the other way
    /// across the page (#546, review round 2).
    #[test]
    fn only_a_half_turn_keeps_the_pages_ruling_lines() {
        let rule = HorizontalRule {
            left: 72.0,
            right: 540.0,
            y: 400.0,
        };

        let drawn = [rule];
        let upright = PageTurn::None.rules_in_frame(&drawn);
        assert_eq!(
            (
                upright.len(),
                upright[0].left,
                upright[0].right,
                upright[0].y
            ),
            (1, rule.left, rule.right, rule.y),
            "an upright page's rules were touched at all"
        );

        let turned = PageTurn::Half.rules_in_frame(&drawn);
        let rotation = PageTurn::Half.rotation();
        let (left, y) = turn_point(rule.right, rule.y, rotation);
        let (right, _) = turn_point(rule.left, rule.y, rotation);
        assert_eq!(
            (turned.len(), turned[0].left, turned[0].right, turned[0].y),
            (1, left, right, y),
            "a half turn lost the page's rules, or left them where they were drawn"
        );
        assert!(
            turned[0].left < turned[0].right,
            "the ends of a mirrored rule did not swap, so it reads as empty"
        );

        for turn in [PageTurn::Quarter, PageTurn::ThreeQuarter] {
            assert!(
                turn.rules_in_frame(&drawn).is_empty(),
                "{turn:?} kept a rule that runs across its reading direction"
            );
        }
    }

    /// A page has to be WRITTEN in a direction before it is read in it: with
    /// its glyphs split four ways, as pdfbox `AcroFormsRotation` splits them
    /// over four blocks of form fields at the four rotations, the heaviest
    /// direction is a quarter of the page, and turning by it moves every block
    /// on a page that was never turned at all (#546).
    #[test]
    fn a_page_with_no_majority_direction_is_not_turned() {
        let quarter_each = vec![
            span("rotation zero", 72.0, 700.0, 78.0),
            turned_span("rotation ninety", 90.0, 200.0, 200.0, 90.0),
            turned_span("rotation oneeighty", 180.0, 400.0, 400.0, 108.0),
            turned_span("rotation twoseventy", 270.0, 500.0, 600.0, 114.0),
        ];
        assert_eq!(
            PageTurn::of(&quarter_each),
            PageTurn::None,
            "a plurality of a quarter of the glyphs turned the whole page"
        );

        // The same page with the turned block carrying most of the glyphs is
        // a page written in that direction, and is read in it.
        let mut majority = quarter_each;
        majority[1] = turned_span(
            "a line of ninety degree text that is most of this page by a street",
            90.0,
            200.0,
            200.0,
            400.0,
        );
        assert_eq!(
            PageTurn::of(&majority),
            PageTurn::Quarter,
            "a direction carrying most of the page did not turn it"
        );
    }

    /// A span drawn by a horizontal reflection, `[-1, 0, 0, 1, e, f]`. Its
    /// baseline runs in `(-1, 0)`, which is where a half turn puts one too --
    /// and its vertical axis is upright, which is where a half turn does not.
    fn mirrored_span(text: &str, x: f64, y: f64, width: f64) -> TextSpan {
        let scale = 12.0 / 1000.0;
        TextSpan {
            transform: Some([-scale, 0.0, 0.0, scale, x, y]),
            ..span(text, x, y, width)
        }
    }

    /// A page drawn mirrored is not a page drawn upside down, and the baseline
    /// vector cannot tell them apart: both run in `(-1, 0)`. Turning it half a
    /// turn negates the vertical axis as well, so a block whose lines descend
    /// the page is read bottom to top -- an order invented from a page that was
    /// never turned. The determinant separates the two. (codex, review round 2)
    #[test]
    fn a_mirrored_page_is_not_a_half_turned_page() {
        let mirrored = vec![
            mirrored_span("the first line of a mirrored page", 400.0, 700.0, 300.0),
            mirrored_span("and the second line below it", 400.0, 686.0, 260.0),
        ];
        assert_eq!(
            PageTurn::of(&mirrored),
            PageTurn::None,
            "a reflection was read as a half turn, and turning it reverses the page"
        );

        // The control, so the assertion above cannot pass by refusing every
        // page: the same two lines actually drawn upside down ARE a half turn.
        let half_turned: Vec<TextSpan> = vec![
            turned_span(
                "the first line of a turned page",
                180.0,
                400.0,
                700.0,
                300.0,
            ),
            turned_span("and the second line below it", 180.0, 400.0, 686.0, 260.0),
        ];
        assert_eq!(
            PageTurn::of(&half_turned),
            PageTurn::Half,
            "a page genuinely drawn at 180 degrees is no longer turned into its frame"
        );

        // And a page that is mostly mirrored with an upright minority stays
        // untouched rather than being read in the minority's direction: the
        // share is of the whole page, not of the rotations on it.
        let mut mostly_mirrored = mirrored;
        mostly_mirrored.push(span("a short upright caption", 72.0, 100.0, 40.0));
        assert_eq!(
            PageTurn::of(&mostly_mirrored),
            PageTurn::None,
            "an upright minority carried the vote on a mirrored page"
        );
    }

    /// The seam allowance of `second_run_carries_on_from_the_first` is one
    /// GLYPH of mean advance, and a glyph may decode to more than one
    /// character. Six characters drawn as five glyphs -- `office` with a `fi`
    /// ligature -- have a 6 pt allowance, where dividing the same advance by
    /// the six characters would leave 5 pt and reject a 5.5 pt seam that a
    /// re-drawn word arrives with (#432, review round 2).
    #[test]
    fn the_seam_allowance_counts_glyphs_and_not_characters() {
        let prev = span("first", 0.0, 700.0, 30.0);
        let mut curr = span("office", 24.5, 700.0, 30.0);
        curr.char_bounds = vec![[0.0; 4]; 5];

        assert!(
            second_run_carries_on_from_the_first(&prev, &curr),
            "a ligature shrank the seam allowance to the character count"
        );

        curr.char_bounds.clear();
        assert!(
            !second_run_carries_on_from_the_first(&prev, &curr),
            "the character count is the fallback, so it has to be the stricter one here"
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

    // ---- M3: RichGeometry / tight glyph bounds tests ----

    #[test]
    fn m3_default_text_span_has_basic_mode() {
        let s = TextSpan::default();
        assert_eq!(s.geometry_mode, GeometryMode::Basic);
        assert_eq!(s.bounds_source, BoundsSource::Advance);
        assert!(s.tight_char_bounds.is_empty());
        assert!(s.glyph_advances.is_empty());
    }

    #[test]
    fn m3_geometry_mode_default_is_basic() {
        assert_eq!(GeometryMode::default(), GeometryMode::Basic);
    }

    #[test]
    fn m3_bounds_source_default_is_advance() {
        assert_eq!(BoundsSource::default(), BoundsSource::Advance);
    }

    #[test]
    fn m3_geometry_mode_variants_are_distinct() {
        assert_ne!(GeometryMode::Basic, GeometryMode::RichGeometry);
        assert_eq!(format!("{:?}", GeometryMode::Basic), "Basic");
        assert_eq!(format!("{:?}", GeometryMode::RichGeometry), "RichGeometry");
    }

    #[test]
    fn m3_bounds_source_variants_are_distinct() {
        assert_ne!(BoundsSource::Advance, BoundsSource::Tight);
        assert_ne!(BoundsSource::Tight, BoundsSource::Estimate);
        assert_eq!(format!("{:?}", BoundsSource::Advance), "Advance");
        assert_eq!(format!("{:?}", BoundsSource::Tight), "Tight");
        assert_eq!(format!("{:?}", BoundsSource::Estimate), "Estimate");
    }

    #[test]
    fn m3_text_span_rich_geometry_preserves_existing_fields() {
        let s = TextSpan {
            text: "Hello".into(),
            x: 10.0,
            y: 100.0,
            width: 30.0,
            height: 12.0,
            font_size: 12.0,
            font_name: Some("Helvetica".into()),
            is_bold: true,
            is_italic: false,
            color: Some([0, 0, 0, 255]),
            width_source: WidthSource::Metric,
            char_bounds: vec![[10.0, 100.0, 40.0, 112.0]],
            geometry_mode: GeometryMode::RichGeometry,
            bounds_source: BoundsSource::Tight,
            tight_char_bounds: vec![[8.5, 98.5, 41.5, 114.0]],
            glyph_advances: vec![30.0],
            glyph_bounds_sources: vec![BoundsSource::Tight],
            ..Default::default()
        };

        // Existing fields preserved.
        assert_eq!(s.text, "Hello");
        assert!((s.x - 10.0).abs() < 0.001);
        assert!((s.y - 100.0).abs() < 0.001);
        assert_eq!(s.width_source, WidthSource::Metric);
        assert_eq!(s.char_bounds.len(), 1);

        // M3 fields populated.
        assert_eq!(s.geometry_mode, GeometryMode::RichGeometry);
        assert_eq!(s.bounds_source, BoundsSource::Tight);
        assert_eq!(s.tight_char_bounds.len(), 1);
        assert_eq!(s.glyph_advances.len(), 1);
        assert!((s.glyph_advances[0] - 30.0).abs() < 0.001);
    }

    #[test]
    fn m3_text_span_basic_mode_does_not_populate_tight_fields() {
        let s = TextSpan {
            text: "A".into(),
            x: 0.0,
            y: 0.0,
            width: 7.0,
            height: 10.0,
            font_size: 10.0,
            geometry_mode: GeometryMode::Basic,
            bounds_source: BoundsSource::Advance,
            ..Default::default()
        };

        assert!(s.tight_char_bounds.is_empty());
        assert!(s.glyph_advances.is_empty());
    }

    #[test]
    fn m3_tight_char_bounds_are_optional_in_basic_mode() {
        // A span from Basic mode should have empty tight fields.
        let s = TextSpan {
            text: "test".into(),
            x: 0.0,
            y: 0.0,
            width: 40.0,
            height: 12.0,
            font_size: 12.0,
            geometry_mode: GeometryMode::Basic,
            ..Default::default()
        };
        assert!(s.tight_char_bounds.is_empty());
        assert!(s.glyph_advances.is_empty());
    }

    #[test]
    fn m3_test_span_constructor_preserves_all_fields() {
        // Verify the span constructor used in existing tests still works.
        let s = span("hello", 10.0, 100.0, 30.0);
        assert_eq!(s.geometry_mode, GeometryMode::Basic);
        assert_eq!(s.bounds_source, BoundsSource::Advance);
        assert!(s.tight_char_bounds.is_empty());
    }

    #[test]
    fn m3_width_source_unchanged_by_m3() {
        // WidthSource should not be affected by RichGeometry.
        assert_eq!(WidthSource::Metric, WidthSource::Metric);
        assert_eq!(WidthSource::Estimate, WidthSource::Estimate);
        assert_eq!(WidthSource::default(), WidthSource::Estimate);
    }

    #[test]
    fn m3_transform_bbox_corners_identity() {
        let bbox = [0.0, 0.0, 10.0, 12.0];
        let identity = kurbo::Affine::IDENTITY;
        let result = transform_bbox_corners(&bbox, &identity);
        assert!((result[0] - 0.0).abs() < 0.001);
        assert!((result[1] - 0.0).abs() < 0.001);
        assert!((result[2] - 10.0).abs() < 0.001);
        assert!((result[3] - 12.0).abs() < 0.001);
    }

    #[test]
    fn m3_transform_bbox_corners_translation() {
        let bbox = [0.0, 0.0, 10.0, 12.0];
        let t = kurbo::Affine::translate((50.0, 100.0));
        let result = transform_bbox_corners(&bbox, &t);
        assert!((result[0] - 50.0).abs() < 0.001);
        assert!((result[1] - 100.0).abs() < 0.001);
        assert!((result[2] - 60.0).abs() < 0.001);
        assert!((result[3] - 112.0).abs() < 0.001);
    }

    #[test]
    fn m3_transform_bbox_corners_scale() {
        let bbox = [0.0, 0.0, 10.0, 12.0];
        let s = kurbo::Affine::scale(2.0);
        let result = transform_bbox_corners(&bbox, &s);
        assert!((result[0] - 0.0).abs() < 0.001);
        assert!((result[1] - 0.0).abs() < 0.001);
        assert!((result[2] - 20.0).abs() < 0.001);
        assert!((result[3] - 24.0).abs() < 0.001);
    }

    #[test]
    fn m3_transform_bbox_corners_with_negative_bbox() {
        // Glyphs with descenders have negative y0 in font space.
        let bbox = [-5.0, -200.0, 15.0, 800.0];
        let s = kurbo::Affine::scale(0.012); // 12pt font at 1000-upem
        let result = transform_bbox_corners(&bbox, &s);
        assert!((result[0] - (-0.06)).abs() < 0.01);
        assert!((result[1] - (-2.4)).abs() < 0.01);
        assert!((result[2] - 0.18).abs() < 0.01);
        assert!((result[3] - 9.6).abs() < 0.01);
    }

    #[test]
    fn m3_tight_char_bounds_descender_detection() {
        // A glyph with a descender should have tight_char_bounds.y0 < origin.y0.
        let s = TextSpan {
            text: "g".into(),
            x: 100.0,
            y: 200.0, // baseline
            width: 8.0,
            height: 12.0,
            font_size: 12.0,
            geometry_mode: GeometryMode::RichGeometry,
            bounds_source: BoundsSource::Tight,
            tight_char_bounds: vec![[99.0, 197.5, 109.0, 210.0]],
            char_bounds: vec![[100.0, 200.0, 108.0, 212.0]],
            glyph_advances: vec![8.0],
            ..Default::default()
        };

        // Tight y0 (197.5) < span y (200.0) — descender extends below baseline.
        assert!(
            s.tight_char_bounds[0][1] < s.y,
            "descender should extend below baseline"
        );
    }

    #[test]
    fn m3_tight_char_bounds_ascender_detection() {
        // A glyph with an ascender should have tight_char_bounds.y1 > advance y1.
        let s = TextSpan {
            text: "f".into(),
            x: 100.0,
            y: 200.0,
            width: 7.0,
            height: 12.0,
            font_size: 12.0,
            geometry_mode: GeometryMode::RichGeometry,
            bounds_source: BoundsSource::Tight,
            tight_char_bounds: vec![[95.0, 200.0, 108.0, 214.0]],
            char_bounds: vec![[100.0, 200.0, 107.0, 212.0]],
            glyph_advances: vec![7.0],
            ..Default::default()
        };

        // Tight y1 (214.0) > advance y1 (200.0 + 12.0 = 212.0).
        let advance_y1 = s.y + s.height;
        assert!(
            s.tight_char_bounds[0][3] > advance_y1,
            "ascender should extend above advance bounds"
        );
    }

    #[test]
    fn m3_tight_char_bounds_differ_from_advance() {
        // Verify tight bounds produce a different bbox than advance bounds.
        let s = TextSpan {
            text: "A".into(),
            x: 10.0,
            y: 100.0,
            width: 7.22,
            height: 10.0,
            font_size: 10.0,
            geometry_mode: GeometryMode::RichGeometry,
            bounds_source: BoundsSource::Tight,
            char_bounds: vec![[10.0, 100.0, 17.22, 110.0]], // advance
            tight_char_bounds: vec![[9.5, 100.2, 17.5, 109.5]], // tight (different)
            glyph_advances: vec![7.22],
            ..Default::default()
        };

        let ac = s.char_bounds[0];
        let tc = s.tight_char_bounds[0];
        let differs = (ac[0] - tc[0]).abs() > 0.01
            || (ac[1] - tc[1]).abs() > 0.01
            || (ac[2] - tc[2]).abs() > 0.01
            || (ac[3] - tc[3]).abs() > 0.01;
        assert!(differs, "tight bounds should differ from advance bounds");
    }

    #[test]
    fn m3_device_new_uses_basic_mode() {
        let dev = TextExtractionDevice::new();
        assert_eq!(dev.geometry_mode, GeometryMode::Basic);
        assert!(dev.deferred_rich_glyphs.is_empty());
    }

    #[test]
    fn m3_device_with_mode_rich_geometry() {
        let dev = TextExtractionDevice::with_mode(GeometryMode::RichGeometry);
        assert_eq!(dev.geometry_mode, GeometryMode::RichGeometry);
        assert!(dev.deferred_rich_glyphs.is_empty());
    }

    /// A synthetic rectangular glyph outline in raw design space, plus its
    /// bbox. The renderer (`renderer.rs::fill_glyph`) fills this raw outline
    /// with `composed = transform * glyph_transform` and NO upem pre-scaling,
    /// so the page-space bbox of a tight glyph MUST equal
    /// `(composed * outline).bounding_box()`. These tests use that as the
    /// ground-truth oracle, which is what guards against scaling regressions.
    #[cfg(test)]
    fn synthetic_glyph_rect() -> (BezPath, kurbo::Rect) {
        let mut path = BezPath::new();
        path.move_to((100.0, 0.0));
        path.line_to((600.0, 0.0));
        path.line_to((600.0, 700.0));
        path.line_to((100.0, 700.0));
        path.close_path();
        let bbox = path.bounding_box();
        (path, bbox)
    }

    #[test]
    fn m3_basic_mode_skips_tight_bounds_computation() {
        // Basic mode must NOT compute tight bounds, even when a span is present.
        let mut dev = TextExtractionDevice::new();
        dev.spans.push(TextSpan {
            text: "A".into(),
            char_bounds: vec![[0.0, 0.0, 7.0, 10.0]],
            ..Default::default()
        });
        let spans = dev.into_spans();
        assert_eq!(spans.len(), 1);
        assert!(spans[0].tight_char_bounds.is_empty());
        assert!(spans[0].glyph_bounds_sources.is_empty());
        assert_eq!(spans[0].bounds_source, BoundsSource::Advance);
    }

    #[test]
    fn m3_rich_geometry_tight_bounds_match_rendered_outline() {
        // Axis-aligned scale 0.01: the page-space tight bbox must equal the
        // bbox of the transformed outline — i.e. design [100,600]x[0,700]
        // scaled by 0.01 → [1.0, 0.0, 6.0, 7.0]. A double-scale regression
        // (e.g. an extra font_size/1000) would yield ~[0.01, 0, 0.06, 0.07].
        let (path, bbox) = synthetic_glyph_rect();
        let composed = Affine::scale(0.01);
        let oracle = (composed * path.clone()).bounding_box();

        let mut dev = TextExtractionDevice::with_mode(GeometryMode::RichGeometry);
        dev.spans.push(TextSpan {
            text: "A".into(),
            char_bounds: vec![[0.0, 0.0, 7.0, 10.0]],
            geometry_mode: GeometryMode::RichGeometry,
            ..Default::default()
        });
        dev.deferred_rich_glyphs.push(DeferredGlyph {
            coeffs: composed.as_coeffs(),
            font_size: 10.0,
            glyph_width: 7.0,
            needs_exact: false,
            outline: Some(path),
            font_bbox: Some(bbox),
        });

        let spans = dev.into_spans();
        assert_eq!(spans[0].glyph_bounds_sources, vec![BoundsSource::Tight]);
        assert_eq!(spans[0].bounds_source, BoundsSource::Tight);
        let tb = spans[0].tight_char_bounds[0];
        assert!(
            (tb[0] - oracle.x0).abs() < 1e-6,
            "x0: got {} want {}",
            tb[0],
            oracle.x0
        );
        assert!(
            (tb[1] - oracle.y0).abs() < 1e-6,
            "y0: got {} want {}",
            tb[1],
            oracle.y0
        );
        assert!(
            (tb[2] - oracle.x1).abs() < 1e-6,
            "x1: got {} want {}",
            tb[2],
            oracle.x1
        );
        assert!(
            (tb[3] - oracle.y1).abs() < 1e-6,
            "y1: got {} want {}",
            tb[3],
            oracle.y1
        );
        // Sanity: tight height (~7) is commensurate with the advance font_size
        // (10), not ~100x smaller — the regression this test exists to catch.
        assert!(
            tb[3] - tb[1] > 1.0,
            "tight height must be commensurate with font size"
        );
    }

    #[test]
    fn m3_rich_geometry_tight_bounds_rotation_is_exact() {
        // 90° rotation routes through the needs_exact (transform-then-bound)
        // path. Oracle = bbox of the rotated outline.
        let (path, bbox) = synthetic_glyph_rect();
        let composed = Affine::scale(0.01) * Affine::rotate(std::f64::consts::FRAC_PI_2);
        let oracle = (composed * path.clone()).bounding_box();

        let mut dev = TextExtractionDevice::with_mode(GeometryMode::RichGeometry);
        dev.spans.push(TextSpan {
            text: "A".into(),
            char_bounds: vec![[0.0, 0.0, 7.0, 10.0]],
            geometry_mode: GeometryMode::RichGeometry,
            ..Default::default()
        });
        dev.deferred_rich_glyphs.push(DeferredGlyph {
            coeffs: composed.as_coeffs(),
            font_size: 10.0,
            glyph_width: 7.0,
            needs_exact: true,
            outline: Some(path),
            font_bbox: Some(bbox),
        });

        let spans = dev.into_spans();
        assert_eq!(spans[0].glyph_bounds_sources, vec![BoundsSource::Tight]);
        let tb = spans[0].tight_char_bounds[0];
        assert!(
            (tb[0] - oracle.x0).abs() < 1e-6,
            "x0: got {} want {}",
            tb[0],
            oracle.x0
        );
        assert!(
            (tb[1] - oracle.y0).abs() < 1e-6,
            "y0: got {} want {}",
            tb[1],
            oracle.y0
        );
        assert!(
            (tb[2] - oracle.x1).abs() < 1e-6,
            "x1: got {} want {}",
            tb[2],
            oracle.x1
        );
        assert!(
            (tb[3] - oracle.y1).abs() < 1e-6,
            "y1: got {} want {}",
            tb[3],
            oracle.y1
        );
    }

    #[test]
    fn m3_empty_device_rich_geometry_no_panic() {
        let dev = TextExtractionDevice::with_mode(GeometryMode::RichGeometry);
        let text = dev.into_text();
        assert!(text.is_empty());
    }

    #[test]
    fn m3_bounds_source_estimate_fallback() {
        // Verify that BoundsSource::Estimate can be set on a span.
        let s = TextSpan {
            geometry_mode: GeometryMode::RichGeometry,
            bounds_source: BoundsSource::Estimate,
            ..Default::default()
        };
        assert_eq!(s.bounds_source, BoundsSource::Estimate);
        assert_eq!(s.geometry_mode, GeometryMode::RichGeometry);
        // Fallback still has empty tight fields (no glyphs).
        assert!(s.tight_char_bounds.is_empty());
    }

    /// One paragraph block of a single line, as the merge leaves a line of a
    /// double-spaced run: `top` is the top of its box, the box one em deep.
    fn line_block(text: &str, left: f64, top: f64, width: f64, size: f64) -> Block {
        let span = TextSpan {
            text: text.into(),
            x: left,
            y: top - size * 0.8,
            width,
            height: size,
            font_size: size,
            ..TextSpan::default()
        };
        Block {
            kind: BlockKind::Paragraph,
            page: 0,
            bbox: [left, top - size, left + width, top],
            text: text.into(),
            spans: vec![span],
            index: 0,
            continues_previous: false,
            confidence: None,
            grid: None,
            table_origin: TableOrigin::Other,
            trace_learned: None,
        }
    }

    /// `texts` as single lines at `pitch`, all `size` on the same left edge
    /// and measure.
    fn run_of(texts: &[&str], pitch: f64, size: f64) -> Vec<Block> {
        texts
            .iter()
            .enumerate()
            .map(|(k, text)| line_block(text, 72.0, 700.0 - pitch * k as f64, 468.0, size))
            .collect()
    }

    #[test]
    fn a_double_spaced_paragraph_is_one_paragraph() {
        fn line(text: &str, y: f64) -> TextSpan {
            // One measure for every line, so the join is deciding about the
            // gaps and nothing else.
            span(text, 72.0, y, 360.0)
        }
        let mut spans = Vec::new();
        // Single-spaced body: three lines on a 14 point pitch, one paragraph.
        for (text, y) in [
            "the committee met in ordinary session on tuesday morning",
            "and heard the report of the working group on procedure",
            "before adjourning for the midday recess.",
        ]
        .iter()
        .zip([700.0, 686.0, 672.0])
        {
            spans.push(line(text, y));
        }
        // Six full-width lines on a 24 point pitch: one paragraph set double,
        // with a word broken across its second line end.
        let run = [
            "the occurrence of brachygnathia inferior and hernias showed",
            "statistically highly significant differences between breeds and the line-",
            "breaks in the middle of a word join back together without a",
            "space while the other lines of the passage join with one plain",
            "space each so that the whole passage reads as the typesetter",
            "set it before the measure broke it into lines.",
        ];
        for (text, y) in run.iter().zip([630.0, 606.0, 582.0, 558.0, 534.0, 510.0]) {
            spans.push(line(text, y));
        }
        // The join runs after heading classification in the extract paths
        // (#636); here every block is still a paragraph, so it runs directly.
        let blocks =
            join_double_spaced_runs(paragraph_blocks(spans, &[], 0, 0, TableStrategy::Both), 0);
        assert_eq!(
            blocks.len(),
            2,
            "the run joined and the body stayed: {:?}",
            blocks.iter().map(|block| &block.text).collect::<Vec<_>>()
        );
        assert_eq!(blocks[0].index, 0);
        assert_eq!(blocks[1].index, 1);
        assert_eq!(
            blocks[1].text,
            "the occurrence of brachygnathia inferior and hernias showed statistically \
             highly significant differences between breeds and the linebreaks in the \
             middle of a word join back together without a space while the other lines \
             of the passage join with one plain space each so that the whole passage \
             reads as the typesetter set it before the measure broke it into lines."
        );
        assert_eq!(blocks[1].spans.len(), 6);
        assert!(
            blocks[1].bbox[1] < 510.0 && blocks[1].bbox[3] > 630.0,
            "the box covers the run: {:?}",
            blocks[1].bbox
        );
    }

    #[test]
    fn a_one_line_list_on_wide_leading_stays_items() {
        let blocks = run_of(
            &[
                "• ripe red apples from the orchard",
                "• green pears from the walled garden",
                "• dark plums from the far paddock",
                "• yellow quinces from the old trees",
                "• blue sloes from the hedgerows",
            ],
            24.0,
            12.0,
        );
        let joined = join_double_spaced_runs(blocks, 0);
        assert_eq!(joined.len(), 5);
    }

    #[test]
    fn an_address_block_stays_lines() {
        let blocks = run_of(
            &[
                "Jonathan Pryce",
                "12 Red Lion Square",
                "London WC1R 4QH",
                "United Kingdom",
            ],
            22.0,
            12.0,
        );
        let joined = join_double_spaced_runs(blocks, 0);
        assert_eq!(joined.len(), 4);
    }

    #[test]
    fn label_and_value_lines_stay_apart() {
        let blocks = run_of(
            &["Ausfertigungsdatum: 20.12.1963", "Vollzitat:"],
            24.0,
            12.0,
        );
        let joined = join_double_spaced_runs(blocks, 3);
        assert_eq!(joined.len(), 2);
        assert_eq!(joined[0].index, 3);
        assert_eq!(joined[1].index, 4);
    }

    #[test]
    fn contents_lines_stay_apart() {
        let blocks = run_of(
            &[
                "25 Independent Auditors' Reports",
                "26 Consolidated Statements",
            ],
            24.0,
            12.0,
        );
        let joined = join_double_spaced_runs(blocks, 0);
        assert_eq!(joined.len(), 2);
    }

    #[test]
    fn two_paragraphs_on_one_pitch_stay_two() {
        let blocks = run_of(
            &[
                "the surveyors counted every head twice over the long winter.",
                "The year before the new regulations came into force was mild",
            ],
            24.0,
            12.0,
        );
        let joined = join_double_spaced_runs(blocks, 0);
        assert_eq!(joined.len(), 2);
    }

    #[test]
    fn section_lines_stay_apart() {
        let blocks = run_of(
            &[
                "§ 2 Begriffe und Definitionen der in diesem Vertrag verwendeten Terminologie",
                "§ 3 Anwendungsbereich und Geltung dieser Bestimmungen im gesamten Inland",
            ],
            24.0,
            12.0,
        );
        let joined = join_double_spaced_runs(blocks, 0);
        assert_eq!(joined.len(), 2);
    }

    #[test]
    fn article_lines_stay_apart() {
        let blocks = run_of(
            &[
                "Art. 2 Begriffe und Definitionen der in diesem Vertrag verwendeten Terminologie",
                "Art. 3 Anwendungsbereich und Geltung dieser Bestimmungen im gesamten Inland",
            ],
            24.0,
            12.0,
        );
        let joined = join_double_spaced_runs(blocks, 0);
        assert_eq!(joined.len(), 2);
    }

    #[test]
    fn headings_never_join() {
        let mut blocks = run_of(
            &[
                "the occurrence of brachygnathia inferior and hernias showed",
                "statistically highly significant differences between breeds and the passage",
            ],
            24.0,
            12.0,
        );
        blocks[1].kind = BlockKind::Heading { level: 2 };
        let joined = join_double_spaced_runs(blocks, 0);
        assert_eq!(joined.len(), 2);
    }

    #[test]
    fn m3_multiple_glyphs_tight_bounds_consistent_count() {
        // Simulate a span with 3 glyphs — tight_char_bounds and glyph_advances
        // should have the same length.
        let s = TextSpan {
            text: "abc".into(),
            x: 0.0,
            y: 0.0,
            width: 25.0,
            height: 12.0,
            font_size: 12.0,
            geometry_mode: GeometryMode::RichGeometry,
            bounds_source: BoundsSource::Tight,
            char_bounds: vec![[0., 0., 8., 12.], [8., 0., 16., 12.], [16., 0., 25., 12.]],
            tight_char_bounds: vec![
                [-1., -2., 9., 14.],
                [7., -2., 17., 14.],
                [15., -2., 26., 14.],
            ],
            glyph_advances: vec![8.0, 8.0, 9.0],
            ..Default::default()
        };

        assert_eq!(s.tight_char_bounds.len(), 3);
        assert_eq!(s.glyph_advances.len(), 3);
        assert_eq!(s.char_bounds.len(), 3);
        // Every glyph has advance >= 0.
        for &adv in &s.glyph_advances {
            assert!(adv >= 0.0, "glyph advance must be non-negative");
        }
    }

    fn styled_span(text: &str, x: f64, y: f64, width: f64, bold: bool) -> TextSpan {
        let mut out = span(text, x, y, width);
        out.is_bold = bold;
        out
    }

    fn block_of(spans: Vec<TextSpan>) -> Block {
        let mut block = Block {
            kind: crate::blocks::BlockKind::Paragraph,
            page: 0,
            bbox: [0.0; 4],
            text: String::new(),
            spans,
            index: 0,
            continues_previous: false,
            confidence: None,
            grid: None,
            table_origin: TableOrigin::Other,
            trace_learned: None,
        };
        let lines = lines_of_block(&block);
        let texts: Vec<String> = lines.iter().map(|line| line_text(&block, line)).collect();
        block.text = crate::blocks::join_lines(&texts);
        crate::blocks::recompute_bbox(&mut block);
        block
    }

    #[test]
    fn an_inid_field_keeps_its_continuation() {
        let upper = block_of(vec![
            styled_span("(73)", 72.0, 700.0, 30.0, true),
            styled_span(
                "Assignee: Schlumberger Technology",
                110.0,
                700.0,
                400.0,
                true,
            ),
        ]);
        let lower = block_of(vec![styled_span(
            "Corporation, Sugar Land, TX (US)",
            110.0,
            688.0,
            380.0,
            false,
        )]);
        let joined = join_marker_continuations(vec![upper, lower], 0);
        assert_eq!(
            joined.len(),
            1,
            "the field was cut off its continuation: {:?}",
            joined.iter().map(|b| &b.text).collect::<Vec<_>>()
        );
        assert!(
            joined[0]
                .text
                .contains("Schlumberger Technology Corporation"),
            "the join lost the middle: {:?}",
            joined[0].text
        );
        assert_eq!(joined[0].spans.len(), 3);
    }

    #[test]
    fn a_bulleted_item_keeps_its_hung_line() {
        let upper = block_of(vec![styled_span(
            "\u{2022} Making investments in research and development and in",
            72.0,
            700.0,
            440.0,
            true,
        )]);
        let lower = block_of(vec![styled_span(
            "anticipate future waves of innovation across the industry",
            100.0,
            688.0,
            400.0,
            false,
        )]);
        let joined = join_marker_continuations(vec![upper, lower], 0);
        assert_eq!(
            joined.len(),
            1,
            "the item was cut off its hung line: {:?}",
            joined.iter().map(|b| &b.text).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_field_label_ending_in_a_colon_stays_apart() {
        let upper = block_of(vec![styled_span(
            "(56) References cited:",
            72.0,
            700.0,
            440.0,
            true,
        )]);
        let lower = block_of(vec![styled_span(
            "A list of patents and publications considered",
            100.0,
            688.0,
            380.0,
            false,
        )]);
        let joined = join_marker_continuations(vec![upper, lower], 3);
        assert_eq!(
            joined.len(),
            2,
            "a label ending in a colon was joined to its value: {:?}",
            joined.iter().map(|b| &b.text).collect::<Vec<_>>()
        );
        assert_eq!(joined[0].index, 3);
        assert_eq!(joined[1].index, 4);
    }

    #[test]
    fn a_short_marker_line_is_not_joined() {
        let upper = block_of(vec![styled_span(
            "(12) United States Patent",
            72.0,
            700.0,
            150.0,
            true,
        )]);
        let lower = block_of(vec![styled_span(
            "Palaniappan et al.",
            100.0,
            688.0,
            200.0,
            false,
        )]);
        let joined = join_marker_continuations(vec![upper, lower], 0);
        assert_eq!(
            joined.len(),
            2,
            "a short marker line was joined to the line under it: {:?}",
            joined.iter().map(|b| &b.text).collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_next_marker_starts_a_new_block() {
        let upper = block_of(vec![styled_span(
            "(73) Assignee: Schlumberger Technology",
            72.0,
            700.0,
            440.0,
            true,
        )]);
        // Hung under the upper line on purpose, so that only the marker
        // clause keeps the two apart.
        let lower = block_of(vec![styled_span(
            "(21) Appl. No.: 09/789,473",
            100.0,
            688.0,
            380.0,
            false,
        )]);
        let joined = join_marker_continuations(vec![upper, lower], 0);
        assert_eq!(
            joined.len(),
            2,
            "the next marker was joined to the item above it: {:?}",
            joined.iter().map(|b| &b.text).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_wrapped_marker_item_keeps_its_hanging_continuation() {
        // The marker is on the FIRST line and the wrap on the LAST: three
        // lines before the weight cut parts the item from its continuation.
        // Read off the last line alone, the item opens with no marker and
        // stays cut.
        let upper = block_of(vec![
            styled_span(
                "\u{2022} Making investments in research and",
                72.0,
                700.0,
                440.0,
                true,
            ),
            styled_span(
                "development and in new facilities built",
                72.0,
                688.0,
                440.0,
                true,
            ),
            styled_span(
                "to meet demand across the coming years to",
                72.0,
                676.0,
                440.0,
                true,
            ),
        ]);
        let lower = block_of(vec![styled_span(
            "anticipate future waves of innovation across the industry",
            100.0,
            664.0,
            400.0,
            false,
        )]);
        let joined = join_marker_continuations(vec![upper, lower], 0);
        assert_eq!(
            joined.len(),
            1,
            "the wrapped item was cut off its hung line: {:?}",
            joined.iter().map(|b| &b.text).collect::<Vec<_>>()
        );
        assert!(
            joined[0].text.contains("coming years to anticipate"),
            "the join lost the middle: {:?}",
            joined[0].text
        );
        assert_eq!(joined[0].spans.len(), 4);
    }

    #[test]
    fn a_short_heading_line_over_a_full_paragraph_is_not_cut() {
        // The short-heading cut is deliberately not shipped with this guard:
        // a short line over full lines stays one block through this pass.
        let block = block_of(vec![
            span("Contingent liabilities", 72.0, 700.0, 150.0),
            span(
                "Goodwill represents the excess of the consideration transferred",
                72.0,
                688.0,
                440.0,
            ),
            span(
                "over the fair value of the identifiable net assets acquired",
                72.0,
                676.0,
                440.0,
            ),
            span(
                "and is tested for impairment at least once a year.",
                72.0,
                664.0,
                440.0,
            ),
        ]);
        let out = join_marker_continuations(vec![block], 0);
        assert_eq!(
            out.len(),
            1,
            "a short heading line was cut off its paragraph: {:?}",
            out.iter().map(|b| &b.text).collect::<Vec<_>>()
        );
        assert_eq!(out[0].spans.len(), 4);
        assert_eq!(out[0].index, 0);
    }

    #[test]
    fn a_marker_line_whose_next_line_does_not_hang_stays_apart() {
        let upper = block_of(vec![styled_span(
            "(73) Assignee: Schlumberger Technology",
            72.0,
            700.0,
            440.0,
            true,
        )]);
        // Flush with the marker line's left edge: a new block, not a hang.
        let lower = block_of(vec![styled_span(
            "Corporation, Sugar Land, TX (US)",
            72.0,
            688.0,
            380.0,
            false,
        )]);
        let joined = join_marker_continuations(vec![upper, lower], 0);
        assert_eq!(
            joined.len(),
            2,
            "a marker line was joined to a line that does not hang under it: {:?}",
            joined.iter().map(|b| &b.text).collect::<Vec<_>>()
        );
    }
}
