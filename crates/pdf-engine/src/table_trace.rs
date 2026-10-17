// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! An env-gated trace of which stage produced each table (#636, diagnostic).
//!
//! `PDFLUENT_TABLE_TRACE=1` prints one `TABLE ...` line per table on stderr:
//! every table in the final output of `extract_blocks`, every table the
//! balanced assembly emits, and every candidate a veto removed. Off by
//! default. The gate reads the environment on every call rather than caching
//! it the way `PDFLUENT_TATR_TRACE` does: emission points are cold (final
//! tables and vetoed candidates, never per region), so the lookup costs
//! nothing measurable, and tests can toggle the variable freely.
//!
//! The trace prints NUMBERS ONLY: geometry, counts and veto verdicts. Cell
//! and page text never reaches the output; it is only measured (lengths,
//! runs, marker matches) to evaluate the veto predicates.
//!
//! Origin is carried through on the blocks, never guessed from shape: the
//! stage that builds a table's cells stamps them (see [`TableOrigin`]), and
//! the trace reads the stamp back. Where a run mixes stamps the trace prints
//! `other`.

use std::collections::HashSet;
use std::fmt::Write as _;

use crate::blocks::Block;
use crate::tables::{is_figure_caption, Cell, Row, FIGURE_CAPTION_NEAR_EM, FRAGMENT_CELL_MEDIAN};
use crate::text::{HorizontalRule, Ruling, TextBlock, TextSpan, WidthSource};

/// Whether the table trace is on: `PDFLUENT_TABLE_TRACE` set to anything.
pub(crate) fn tracing() -> bool {
    std::env::var_os("PDFLUENT_TABLE_TRACE").is_some()
}

/// The stage that produced a table's cells.
///
/// Stamped where the cells are built: `find`/`find_whole` in
/// [`crate::tables`], the learned rebuild in `learned::tables` (forced or
/// not), the region rebuild and the TATR attempt in
/// [`crate::document`](crate::PdfDocument::table_in_region). The balanced
/// tier does not stamp: its lines name the R1 step that supplied the grid
/// (`region`, `engine-grid-match`, `tatr`), which is exact where it is
/// printed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum TableOrigin {
    /// No stage claimed the cells (test fixtures, prose rebuilt as cells).
    #[default]
    Other,
    /// The column reader [`crate::tables::find`].
    Find,
    /// The region reader [`crate::tables::find_whole`], stretch path or its
    /// own reading: `find_whole` stamps its stretches, and tables from its
    /// internal `find` fallback arrive already stamped `find`.
    FindWhole,
    /// The learned table pass rebuilt the region without forcing it (the
    /// weakness-overrule path).
    LearnedRefine,
    /// The learned table pass rebuilt the region it was forced to (a drawn
    /// lattice, or a judged region that is not exactly one builder table).
    LearnedForced,
    /// The region rebuild behind
    /// [`crate::PdfDocument::table_in_region`](crate::PdfDocument::table_in_region).
    Region,
    /// The balanced tier's engine-grid matcher (R1 step 2 and R7 orphans).
    /// Constructed in `balanced.rs` (`ort-layout`); kept on every build so
    /// the trace words are stable.
    #[allow(dead_code)]
    EngineGridMatch,
    /// TATR's cells: the TATR attempt in `table_in_region`, or TATR firing
    /// inside a rebuild.
    Tatr,
}

impl TableOrigin {
    /// The trace word for this origin.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            TableOrigin::Other => "other",
            TableOrigin::Find => "find",
            TableOrigin::FindWhole => "find_whole",
            TableOrigin::LearnedRefine => "learned-refine",
            TableOrigin::LearnedForced => "learned-forced",
            TableOrigin::Region => "region",
            TableOrigin::EngineGridMatch => "engine-grid-match",
            TableOrigin::Tatr => "tatr",
        }
    }
}

// The page the current `find`/`find_whole` call reads, for removed-candidate
// lines: those functions never learn the page, so the one caller that knows
// it (the table building in `crate::blocks`) scopes it around the call.
// `None` outside such a scope (unit tests calling `find` directly): removed
// lines are skipped there rather than mislabelled.
thread_local! {
    static TRACE_PAGE: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

/// Scope [`trace_page`] to `page` for the duration of the call.
pub(crate) fn trace_page_guard(page: usize) -> TracePageGuard {
    TRACE_PAGE.set(Some(page));
    TracePageGuard
}

/// The guard [`trace_page_guard`] returns; clears the page on drop.
pub(crate) struct TracePageGuard;

impl Drop for TracePageGuard {
    fn drop(&mut self) {
        TRACE_PAGE.set(None);
    }
}

/// The page in scope, if any (see [`trace_page_guard`]).
pub(crate) fn trace_page() -> Option<usize> {
    TRACE_PAGE.get()
}

/// Why the learned pass forced a region into a table, for the `forced` field.
///
/// Stamped beside [`TableOrigin::LearnedForced`] where the forcing is
/// decided ([`crate::learned::tables`]); it never decides what is a table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ForcedReason {
    /// A drawn lattice forced the region.
    Lattice,
    /// A judged region that is not exactly one builder table forced it.
    JudgedNotOneTable,
}

impl ForcedReason {
    /// The trace word for this reason.
    fn as_trace(self) -> &'static str {
        match self {
            ForcedReason::Lattice => "lattice",
            ForcedReason::JudgedNotOneTable => "judged-not-one-table",
        }
    }
}

/// Trace-only record of the learned pass's numbers for one cell (#636).
///
/// Stamped where the pass stamps the origin; read back where the trace
/// prints. `judge_p` is the table-judge probability the region went through
/// (`None` on lattice-forced and unjudged paths), `forced` why a forced
/// region was forced. Neither ever decides what is a table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LearnedTrace {
    /// The judge probability, if the region went through the judge.
    pub judge_p: Option<f64>,
    /// Why the region was forced, if it was.
    pub forced: Option<ForcedReason>,
}

/// Page graphics the trace reads but never changes: one page's lattice
/// rulings and drawn-cluster boxes, for the `rules_*`, `ruled_frac` and
/// `drawing_ops` fields. Paths that do not hold the data pass `None` and
/// print `-` for those fields.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PageCtx<'a> {
    /// Thin lines and rectangle edges ([`Ruling`]).
    pub rulings: &'a [Ruling],
    /// Drawn-cluster boxes (`take_graphics`).
    pub graphics: &'a [[f64; 4]],
}

/// One veto's verdict: fired, or the first failing condition in the veto's
/// own evaluation order (see each `*_verdict` function).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// The veto fired.
    Fired,
    /// The veto did not fire; the first failing condition, without `no:`.
    No(&'static str),
}

impl Verdict {
    /// The trace word: `fired`, or `no:<reason>`.
    fn as_trace(self) -> String {
        match self {
            Verdict::Fired => "fired".to_string(),
            Verdict::No(reason) => format!("no:{reason}"),
        }
    }
}

/// One cell as the trace reads it, from a finished block or a candidate.
#[derive(Debug, Clone)]
struct CellView {
    /// The cell's text (measured, never printed).
    text: String,
    /// 0-based row inside the table.
    row: u16,
    /// 0-based column inside the table.
    col: u16,
    /// How many rows the cell covers.
    row_span: u16,
    /// How many columns the cell covers.
    col_span: u16,
    /// The cell's box, `[x0, y0, x1, y1]`.
    bbox: [f64; 4],
    /// Span-text characters in the cell (the `ocr_text` weight).
    chars: usize,
    /// Of those, drawn invisible or without glyph metrics.
    ocr_chars: usize,
    /// The fonts of the cell's spans (`None` where the span names none).
    fonts: Vec<Option<String>>,
    /// Whether the cell's text reads as one number (see [`is_numeric_cell`]).
    numeric: bool,
    /// Whether the cell holds a dictionary-shaped word (alphabetic run ≥ 4).
    wordly: bool,
}

/// A cell whose text reads as one number: an optional sign, digits with
/// thousand separators, at most one fraction point, an optional percent.
/// Currency signs around it are stripped first. `2.5`, `-10`, `12%` and
/// `1,000` count; `2.5.1`, `12a` and `eight` do not.
fn is_numeric_cell(text: &str) -> bool {
    let mut rest = text.trim();
    rest = rest
        .strip_prefix(['$', '€', '£', '¥'])
        .unwrap_or(rest)
        .trim();
    rest = rest.strip_suffix('%').unwrap_or(rest).trim();
    rest = rest.strip_prefix(['+', '-']).unwrap_or(rest);
    if rest.is_empty() {
        return false;
    }
    let mut digits = 0usize;
    let mut points = 0usize;
    for c in rest.chars() {
        if c.is_ascii_digit() {
            digits += 1;
        } else if c == '.' {
            points += 1;
        } else if c == ',' || c == '\'' || c == ' ' || c == '\u{a0}' {
            // Thousand separators and thin gaps: digits must surround them,
            // which the digit count below enforces loosely.
        } else {
            return false;
        }
    }
    digits > 0 && points <= 1
}

/// Span facts for one cell: `(chars, ocr_chars, fonts)`.
///
/// A character counts as OCR text where its span is drawn invisible (render
/// mode 3) or its width was estimated, which is what spans read when no
/// glyph metric was available (Type3 or metric-less fonts).
fn span_facts(spans: &[&TextSpan]) -> (usize, usize, Vec<Option<String>>) {
    let mut chars = 0usize;
    let mut ocr_chars = 0usize;
    let mut fonts = Vec::new();
    for span in spans {
        let n = span.text.chars().count();
        chars += n;
        if span.render_mode == Some(3) || span.width_source == WidthSource::Estimate {
            ocr_chars += n;
        }
        fonts.push(span.font_name.clone());
    }
    (chars, ocr_chars, fonts)
}

impl CellView {
    /// Finish a cell view from its text, grid position, box and spans.
    fn of(
        text: String,
        row: u16,
        col: u16,
        row_span: u16,
        col_span: u16,
        bbox: [f64; 4],
        spans: &[&TextSpan],
    ) -> CellView {
        let (chars, ocr_chars, fonts) = span_facts(spans);
        let numeric = is_numeric_cell(&text);
        let wordly = {
            let mut best = 0usize;
            let mut run = 0usize;
            for c in text.chars() {
                if c.is_alphabetic() {
                    run += 1;
                    best = best.max(run);
                } else {
                    run = 0;
                }
            }
            best >= 4
        };
        CellView {
            text,
            row,
            col,
            row_span,
            col_span,
            bbox,
            chars,
            ocr_chars,
            fonts,
            numeric,
            wordly,
        }
    }
}

impl CellView {
    /// Alphanumeric characters in the cell.
    fn alnum(&self) -> usize {
        self.text.chars().filter(|c| c.is_alphanumeric()).count()
    }

    /// The longest run of alphabetic characters in the cell.
    fn alpha_run(&self) -> usize {
        let mut best = 0usize;
        let mut run = 0usize;
        for c in self.text.chars() {
            if c.is_alphabetic() {
                run += 1;
                best = best.max(run);
            } else {
                run = 0;
            }
        }
        best
    }

    /// Occurrences of the panel-label stem in the cell.
    fn continu(&self) -> usize {
        self.text.to_ascii_lowercase().matches("continu").count()
    }
}

/// A figure-caption marker near the grid, for the `caption_marker` field:
/// the nearest marker's offset from the grid in points, and the grid's em.
#[derive(Debug, Clone, Copy)]
struct CaptionHit {
    /// Marker centre x minus grid centre x, in points.
    dx: f64,
    /// Signed vertical gap in PDF y-up points: negative where the marker
    /// stands below the grid (smaller y, later in reading order), positive
    /// above it, zero on vertical overlap.
    dy: f64,
    /// The grid's em (see [`table_stats`]).
    em: f64,
}

/// The numbers one `TABLE` line prints, before the veto verdicts.
pub(crate) struct TableStats {
    /// Grid rows.
    rows: usize,
    /// Grid columns.
    cols: usize,
    /// Cell blocks counted on the grid.
    ncells: usize,
    /// Union of the cells' boxes.
    bbox: [f64; 4],
    /// The median cell's alphanumeric count (upper median on even counts,
    /// the way the vetoes read it).
    median_alnum: usize,
    /// The longest alphabetic run in any cell.
    max_alpha_run: usize,
    /// Share of grid positions left empty, 0..=1.
    empty_frac: f64,
    /// Panel-label occurrences in row 0.
    continu_row0: usize,
    /// Rows holding at least one panel-label occurrence.
    continu_rows: usize,
    /// The nearest caption marker, if the scope holds one.
    caption: Option<CaptionHit>,
}

/// Read finished cell blocks the way the vetoes see them.
fn views_of_blocks(cells: &[&Block]) -> Vec<CellView> {
    cells
        .iter()
        .map(|block| {
            let (row, col, row_span, col_span) = match block.kind {
                crate::BlockKind::TableCell {
                    row,
                    column: c,
                    row_span,
                    column_span: cs,
                    ..
                } => (row, c, row_span, cs),
                _ => (0, 0, 1, 1),
            };
            let spans: Vec<&TextSpan> = block.spans.iter().collect();
            CellView::of(
                block.text.clone(),
                row,
                col,
                row_span,
                col_span,
                block.bbox,
                &spans,
            )
        })
        .collect()
}

/// Read a `find`-path candidate the way the vetoes see it: cell text joined
/// from spans exactly as `fragmentary_cells` joins it, cell boxes from the
/// spans' own geometry.
fn views_of_candidate(table: &crate::tables::Table, lines: &[&TextBlock]) -> Vec<CellView> {
    table
        .cells
        .iter()
        .map(|cell: &Cell| {
            let text = cell
                .spans
                .iter()
                .filter_map(|&(line, span)| {
                    lines
                        .get(line)
                        .and_then(|block| block.spans.get(span))
                        .map(|span| span.text.trim())
                })
                .collect::<Vec<_>>()
                .join(" ");
            let mut bbox = [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ];
            let mut spans = Vec::new();
            for &(line, span) in &cell.spans {
                if let Some(span) = lines.get(line).and_then(|block| block.spans.get(span)) {
                    bbox[0] = bbox[0].min(span.x);
                    bbox[1] = bbox[1].min(span.y);
                    bbox[2] = bbox[2].max(crate::blocks::span_right(span));
                    bbox[3] = bbox[3].max(span.y + span.height);
                    spans.push(span);
                }
            }
            if !bbox[0].is_finite() {
                bbox = [0.0; 4];
            }
            CellView::of(
                text,
                cell.row,
                cell.column,
                cell.row_span,
                cell.column_span,
                bbox,
                &spans,
            )
        })
        .collect()
}

/// Measure one grid: extents, holes, lengths, panel labels, nearest marker.
///
/// `grid_box` is the box the caption gap is measured against and `em` the em
/// it is measured in; each path passes what its own veto uses (row geometry
/// and type size on the `find` path, cell boxes and median cell height on
/// finished cells). `markers` are the scope's `(text, box)` pairs.
fn table_stats(
    cells: &[CellView],
    grid_box: [f64; 4],
    em: f64,
    markers: &[(String, [f64; 4])],
) -> Option<TableStats> {
    if cells.is_empty() {
        return None;
    }
    let rows = cells
        .iter()
        .map(|cell| cell.row as usize + cell.row_span.max(1) as usize)
        .max()
        .unwrap_or(0);
    let cols = cells
        .iter()
        .map(|cell| cell.col as usize + cell.col_span as usize)
        .max()
        .unwrap_or(0);
    if rows == 0 || cols == 0 {
        return None;
    }
    let positions = rows * cols;
    let filled: usize = cells
        .iter()
        .map(|cell| cell.col_span as usize * cell.row_span.max(1) as usize)
        .sum();
    let empty_frac = if positions == 0 {
        0.0
    } else {
        (positions.saturating_sub(filled)) as f64 / positions as f64
    };
    let mut lens: Vec<usize> = cells.iter().map(|cell| cell.alnum()).collect();
    lens.sort_unstable();
    let median_alnum = lens[lens.len() / 2];
    let max_alpha_run = cells.iter().map(|cell| cell.alpha_run()).max().unwrap_or(0);
    let continu_row0: usize = cells
        .iter()
        .filter(|cell| cell.row == 0)
        .map(|cell| cell.continu())
        .sum();
    let mut continu_hit_rows = std::collections::HashSet::new();
    for cell in cells {
        if cell.continu() > 0 {
            continu_hit_rows.insert(cell.row);
        }
    }
    let mut bbox = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for cell in cells {
        bbox[0] = bbox[0].min(cell.bbox[0]);
        bbox[1] = bbox[1].min(cell.bbox[1]);
        bbox[2] = bbox[2].max(cell.bbox[2]);
        bbox[3] = bbox[3].max(cell.bbox[3]);
    }
    if !(bbox[0].is_finite() && bbox[2] > bbox[0] && bbox[3] >= bbox[1]) {
        bbox = grid_box;
    }
    let caption = nearest_marker(grid_box, markers).map(|(dx, dy)| CaptionHit { dx, dy, em });
    Some(TableStats {
        rows,
        cols,
        ncells: cells.len(),
        bbox,
        median_alnum,
        max_alpha_run,
        empty_frac,
        continu_row0,
        continu_rows: continu_hit_rows.len(),
        caption,
    })
}

/// The nearest caption marker's offset from the grid box.
///
/// Markers are `(text, box)` pairs already filtered to caption markers; the
/// nearest wins by vertical gap (zero on overlap), ties to page order. `None`
/// where the scope holds no marker.
fn nearest_marker(grid_box: [f64; 4], markers: &[(String, [f64; 4])]) -> Option<(f64, f64)> {
    let cx = (grid_box[0] + grid_box[2]) / 2.0;
    let mut best: Option<(f64, f64, f64)> = None;
    for (_, bbox) in markers {
        let gap = if bbox[3] < grid_box[1] {
            grid_box[1] - bbox[3]
        } else if bbox[1] > grid_box[3] {
            bbox[1] - grid_box[3]
        } else {
            0.0
        };
        let dy = if bbox[3] < grid_box[1] {
            bbox[3] - grid_box[1]
        } else if bbox[1] > grid_box[3] {
            bbox[1] - grid_box[3]
        } else {
            0.0
        };
        let dx = (bbox[0] + bbox[2]) / 2.0 - cx;
        let better = match best {
            None => true,
            Some((g, _, _)) => gap < g,
        };
        if better {
            best = Some((gap, dx, dy));
        }
    }
    best.map(|(_, dx, dy)| (dx, dy))
}

/// The caption half of the vetoes, shared: whether any marker stands near
/// the grid in the veto's sense (sideways overlap within `near` above or
/// below it).
///
/// Reasons in code order: no marker at all, a marker but none overlapping
/// sideways, overlap but past the neighbourhood.
fn caption_verdict(
    grid_box: [f64; 4],
    near: f64,
    markers: &[(String, [f64; 4])],
) -> Result<(), &'static str> {
    if markers.is_empty() {
        return Err("no_caption_marker");
    }
    if !markers.iter().any(|(_, bbox)| {
        let [left, _, right, _] = *bbox;
        left.is_finite() && right > left && !(right <= grid_box[0] || left >= grid_box[2])
    }) {
        return Err("not_sideways");
    }
    let near_enough = markers.iter().any(|(_, bbox)| {
        let [left, lo, right, hi] = *bbox;
        if !(left.is_finite() && right > left && hi >= lo) {
            return false;
        }
        if right <= grid_box[0] || left >= grid_box[2] {
            return false;
        }
        let gap = if hi < grid_box[1] {
            grid_box[1] - hi
        } else if lo > grid_box[3] {
            lo - grid_box[3]
        } else {
            0.0
        };
        gap <= near
    });
    if near_enough {
        Ok(())
    } else {
        Err("no_caption_within_12em")
    }
}

/// The `find`-path veto ([`crate::tables`]' figure-fragment veto) as a
/// verdict: the fragmentary-cells-plus-caption conjunction, then the
/// continued-panels disjunct, then the panel-fragments disjunct, each read in
/// the veto's own order. The first failing condition is the reason.
///
/// `grid_box`/`em` are what the veto measures against (row geometry and row
/// type size on the `find` path); `markers` are the scope's caption markers
/// as `(text, box)`; `continu_lead_nearby` is the panel-fragments
/// neighbour check, which only exists pre-block -- finished cells pass
/// `None`, which reads as its failure.
fn fig_verdict(
    cells: &[CellView],
    rows: usize,
    grid_box: [f64; 4],
    em: f64,
    markers: &[(String, [f64; 4])],
    continu_lead_nearby: Option<bool>,
) -> Verdict {
    if cells.is_empty() || rows == 0 {
        return Verdict::No("no_cells");
    }
    let cols = cells
        .iter()
        .map(|cell| cell.col as usize + cell.col_span as usize)
        .max()
        .unwrap_or(0);
    if cols == 0 {
        return Verdict::No("no_columns");
    }
    let positions = rows * cols;
    let filled: usize = cells
        .iter()
        .map(|cell| cell.col_span as usize * cell.row_span.max(1) as usize)
        .sum();
    if positions == 0 || filled >= positions {
        return Verdict::No("dense");
    }
    if (positions - filled) * 3 <= positions {
        return Verdict::No("holes<=third");
    }
    let mut lens: Vec<usize> = cells.iter().map(|cell| cell.alnum()).collect();
    lens.sort_unstable();
    let mid = lens.len() / 2;
    let short = if lens.len() % 2 == 1 {
        lens[mid] <= FRAGMENT_CELL_MEDIAN
    } else {
        lens[mid - 1] + lens[mid] <= 2 * FRAGMENT_CELL_MEDIAN
    };
    if !short {
        return Verdict::No("median>3");
    }
    let near = FIGURE_CAPTION_NEAR_EM * em;
    let caption_reason = caption_verdict(grid_box, near, markers).err();
    let mut hits = 0usize;
    let mut outside_row0 = false;
    for cell in cells {
        let here = cell.continu();
        if here > 0 && cell.row != 0 {
            outside_row0 = true;
        }
        hits += here;
    }
    let continued = if hits < 2 {
        Err("continu<2")
    } else if outside_row0 {
        Err("continu_not_in_row0")
    } else {
        Ok(())
    };
    // The veto fires where the conjunction holds, or either continued
    // disjunct does; the reason is the first failure in that order.
    if caption_reason.is_none() {
        return Verdict::Fired;
    }
    if continued.is_ok() {
        return Verdict::Fired;
    }
    if short && continu_lead_nearby == Some(true) {
        return Verdict::Fired;
    }
    Verdict::No(
        caption_reason
            .or(continued.err())
            .or(if !short {
                Some("cells_not_short")
            } else {
                Some("no_continu_lead_nearby")
            })
            .unwrap_or("median>3"),
    )
}

/// The region veto (the `#636` region-fragment veto in
/// [`crate::document`](crate::PdfDocument)) as a verdict: the continued
/// shape first, then the short-cells-plus-caption shape, each in the veto's
/// own order.
fn region_verdict(
    cells: &[CellView],
    facts: &CellFacts,
    grid_box: [f64; 4],
    em: f64,
    markers: &[(String, [f64; 4])],
) -> Verdict {
    if cells.is_empty() {
        return Verdict::No("no_cells");
    }
    let hits: usize = cells.iter().map(|cell| cell.continu()).sum();
    let in_first_row = cells
        .iter()
        .all(|cell| cell.continu() == 0 || cell.row == 0);
    if hits >= 2 && in_first_row && (facts.sparse || facts.median <= 2 * FRAGMENT_CELL_MEDIAN) {
        return Verdict::Fired;
    }
    // First failing condition in the veto's order.
    if hits < 2 {
        // The continued shape is out; the label shape decides.
        if !facts.short {
            return Verdict::No("median>3");
        }
        if facts.has_word {
            return Verdict::No("alpha_run>=4");
        }
        let near = FIGURE_CAPTION_NEAR_EM * em;
        return match caption_verdict(grid_box, near, markers) {
            Ok(()) => Verdict::Fired,
            Err(reason) => Verdict::No(reason),
        };
    }
    if !in_first_row {
        return Verdict::No("continu_not_in_row0");
    }
    Verdict::No("dense_and_long")
}

/// The demote veto (the finished-output sweep in
/// [`crate::document`](crate::PdfDocument)) as a verdict: the continued
/// shape first, then the sparse-short-caption shape, each in the sweep's own
/// order.
fn demote_verdict(
    cells: &[CellView],
    facts: &CellFacts,
    grid_box: [f64; 4],
    em: f64,
    markers: &[(String, [f64; 4])],
) -> Verdict {
    if cells.is_empty() {
        return Verdict::No("no_cells");
    }
    let hits: usize = cells.iter().map(|cell| cell.continu()).sum();
    let in_first_row = cells
        .iter()
        .all(|cell| cell.continu() == 0 || cell.row == 0);
    if hits >= 2 && in_first_row && (facts.sparse || facts.median <= 2 * FRAGMENT_CELL_MEDIAN) {
        return Verdict::Fired;
    }
    if hits < 2 {
        if !facts.sparse {
            return Verdict::No("not_sparse");
        }
        if !facts.short {
            return Verdict::No("median>3");
        }
        if facts.has_word {
            return Verdict::No("alpha_run>=4");
        }
        let near = FIGURE_CAPTION_NEAR_EM * em;
        return match caption_verdict(grid_box, near, markers) {
            Ok(()) => Verdict::Fired,
            Err(reason) => Verdict::No(reason),
        };
    }
    if !in_first_row {
        return Verdict::No("continu_not_in_row0");
    }
    Verdict::No("dense_and_long")
}

/// Sparse, median and word facts over cells, the way the region-fragment
/// veto reads them.
#[derive(Debug, Clone, Copy)]
struct CellFacts {
    /// Over a third of the grid's positions are holes.
    sparse: bool,
    /// The median cell's alphanumeric count.
    median: usize,
    /// The median cell holds at most the fragment median.
    short: bool,
    /// Some cell holds an alphabetic run of four or more.
    has_word: bool,
}

fn cell_facts(cells: &[CellView]) -> CellFacts {
    let rows = cells
        .iter()
        .map(|cell| cell.row as usize + cell.row_span.max(1) as usize)
        .max()
        .unwrap_or(0);
    let cols = cells
        .iter()
        .map(|cell| cell.col as usize + cell.col_span as usize)
        .max()
        .unwrap_or(0);
    let positions = rows * cols;
    let filled: usize = cells
        .iter()
        .map(|cell| cell.col_span as usize * cell.row_span.max(1) as usize)
        .sum();
    let sparse = positions > 0 && filled < positions && (positions - filled) * 3 > positions;
    let mut lens: Vec<usize> = cells.iter().map(|cell| cell.alnum()).collect();
    lens.sort_unstable();
    let mid = lens.len() / 2;
    let short = if lens.len() % 2 == 1 {
        lens[mid] <= FRAGMENT_CELL_MEDIAN
    } else {
        lens[mid - 1] + lens[mid] <= 2 * FRAGMENT_CELL_MEDIAN
    };
    let median = lens[mid];
    let has_word = cells.iter().any(|cell| cell.alpha_run() >= 4);
    CellFacts {
        sparse,
        median,
        short,
        has_word,
    }
}

/// The per-table features one `TABLE` line prints after the veto verdicts.
///
/// `None` reads as `-`: the path does not hold that datum (no page graphics
/// beside the grid, no judge behind the table, no table count for a lone
/// candidate). Everything else is a number. See `REPORT.md` under
/// `meas-out/table-features` for each field's definition.
pub(crate) struct TableFeatures {
    /// The table-judge probability, if the table went through the judge.
    judge_p: Option<f64>,
    /// The forced reason, for learned-forced tables (`None` prints `-`;
    /// learned-forced without a stamped reason prints `other`).
    forced: Option<Option<ForcedReason>>,
    /// Horizontal lattice rulings inside the bbox.
    rules_h: Option<usize>,
    /// Vertical lattice rulings inside the bbox.
    rules_v: Option<usize>,
    /// Share of cells the lattice encloses (see [`ruled_fraction`]).
    ruled_frac: Option<f64>,
    /// Share of the table's span characters drawn invisible (render mode 3)
    /// or without glyph metrics.
    ocr_text: f64,
    /// Distinct fonts in the table's spans.
    fonts: usize,
    /// Share of cells reading as one number.
    num_frac: f64,
    /// Share of cells holding a dictionary-shaped word.
    word_frac: f64,
    /// Share of columns whose cell edges align within 2 pt.
    cols_aligned: f64,
    /// Tables on the page.
    page_tables: Option<usize>,
    /// Drawn-cluster boxes intersecting the bbox.
    drawing_ops: Option<usize>,
    /// The eight candidate rule flags, in line order.
    cand: [bool; 8],
}

/// Whether two 1-D ranges overlap with positive length.
fn overlaps_1d(a0: f64, a1: f64, b0: f64, b1: f64) -> bool {
    a0.min(a1) < b0.max(b1) && b0.min(b1) < a0.max(a1)
}

/// Whether a box intersects another box with positive area.
fn box_hits(a: [f64; 4], b: [f64; 4]) -> bool {
    overlaps_1d(a[0], a[2], b[0], b[2]) && overlaps_1d(a[1], a[3], b[1], b[3])
}

/// A ruling is horizontal where its y does not move, vertical where its x
/// does not: the lattice draws axis-aligned strokes only.
fn ruling_orientation(ruling: &Ruling) -> Option<bool> {
    let horizontal = (ruling.y1 - ruling.y0).abs() < 1e-6;
    let vertical = (ruling.x1 - ruling.x0).abs() < 1e-6;
    match (horizontal, vertical) {
        (true, false) => Some(true),
        (false, true) => Some(false),
        _ => None,
    }
}

/// Lattice rulings inside the bbox: `(horizontal, vertical)`.
///
/// A ruling counts where its line lies within the bbox grown by half a
/// point and its span overlaps the bbox with positive length.
fn rulings_in(bbox: [f64; 4], rulings: &[Ruling]) -> (usize, usize) {
    let mut horizontal = 0usize;
    let mut vertical = 0usize;
    for ruling in rulings {
        match ruling_orientation(ruling) {
            Some(true) => {
                if ruling.y0 >= bbox[1] - 0.5
                    && ruling.y0 <= bbox[3] + 0.5
                    && overlaps_1d(ruling.x0, ruling.x1, bbox[0], bbox[2])
                {
                    horizontal += 1;
                }
            }
            Some(false) => {
                if ruling.x0 >= bbox[0] - 0.5
                    && ruling.x0 <= bbox[2] + 0.5
                    && overlaps_1d(ruling.y0, ruling.y1, bbox[1], bbox[3])
                {
                    vertical += 1;
                }
            }
            None => {}
        }
    }
    (horizontal, vertical)
}

/// Share of cells the lattice encloses, 0..=1.
///
/// A cell is bounded where a horizontal ruling stands at or just past each
/// of its top and bottom edges (within a point, overlapping the cell
/// sideways) and a vertical ruling likewise left and right. Rulings run in
/// the gaps between rows, not on glyph edges, so coincidence with the edge
/// itself would never hold; the straddle is what "bounded" means. Only
/// rulings near the table count: within the text bbox grown by 12 pt (about
/// one row gap, so outer borders count and page-level lines do not).
fn ruled_fraction(cells: &[CellView], bbox: [f64; 4], rulings: &[Ruling]) -> f64 {
    if cells.is_empty() {
        return 0.0;
    }
    const NEAR: f64 = 12.0;
    const TOUCH: f64 = 1.0;
    let horizontals: Vec<&Ruling> = rulings
        .iter()
        .filter(|r| {
            ruling_orientation(r) == Some(true)
                && r.y0 >= bbox[1] - NEAR
                && r.y0 <= bbox[3] + NEAR
                && overlaps_1d(r.x0, r.x1, bbox[0], bbox[2])
        })
        .collect();
    let verticals: Vec<&Ruling> = rulings
        .iter()
        .filter(|r| {
            ruling_orientation(r) == Some(false)
                && r.x0 >= bbox[0] - NEAR
                && r.x0 <= bbox[2] + NEAR
                && overlaps_1d(r.y0, r.y1, bbox[1], bbox[3])
        })
        .collect();
    let bounded = cells
        .iter()
        .filter(|cell| {
            let [x0, y0, x1, y1] = cell.bbox;
            horizontals.iter().any(|above| {
                above.y0 >= y1 - TOUCH
                    && overlaps_1d(above.x0, above.x1, x0, x1)
                    && horizontals.iter().any(|below| {
                        below.y0 <= y0 + TOUCH && overlaps_1d(below.x0, below.x1, x0, x1)
                    })
            }) && verticals.iter().any(|right| {
                right.x0 >= x1 - TOUCH
                    && overlaps_1d(right.y0, right.y1, y0, y1)
                    && verticals
                        .iter()
                        .any(|left| left.x0 <= x0 + TOUCH && overlaps_1d(left.y0, left.y1, y0, y1))
            })
        })
        .count();
    bounded as f64 / cells.len() as f64
}

/// Share of columns whose cell edges align within 2 pt: the left edges
/// agree, or the right edges, or the centres. A column counts by its cells'
/// starting column; a column with fewer than two cells counts as aligned.
fn aligned_fraction(cells: &[CellView], cols: usize) -> f64 {
    if cols == 0 {
        return 0.0;
    }
    let mut aligned = 0usize;
    for col in 0..cols {
        let edges: Vec<(f64, f64, f64)> = cells
            .iter()
            .filter(|cell| cell.col as usize == col)
            .map(|cell| {
                let centre = (cell.bbox[0] + cell.bbox[2]) / 2.0;
                (cell.bbox[0], cell.bbox[2], centre)
            })
            .collect();
        if edges.len() < 2 {
            aligned += 1;
            continue;
        }
        let spread = |pick: fn(&(f64, f64, f64)) -> f64| {
            let mut values: Vec<f64> = edges.iter().map(pick).collect();
            values.sort_by(f64::total_cmp);
            values[values.len() - 1] - values[0]
        };
        if spread(|e| e.0) <= 2.0 || spread(|e| e.1) <= 2.0 || spread(|e| e.2) <= 2.0 {
            aligned += 1;
        }
    }
    aligned as f64 / cols as f64
}

/// The eight candidate rule flags, in line order (see the report for each
/// rule's definition). Read-only over the measured features: they never
/// decide what is a table.
#[allow(clippy::too_many_arguments)]
fn candidate_flags(
    origin: TableOrigin,
    judge_p: Option<f64>,
    rows: usize,
    cols: usize,
    rules: Option<(usize, usize)>,
    ocr_text: f64,
    word_frac: f64,
    cols_aligned: f64,
    drawing_ops: Option<usize>,
) -> [bool; 8] {
    let is_forced = origin == TableOrigin::LearnedForced;
    let no_rulings = rules == Some((0, 0));
    // A present judge number below one half. A missing number is not a low
    // one: lattice-forced tables never went through the judge, and real
    // ruled tables arrive exactly that way.
    let low_judge = judge_p.is_some_and(|p| p < 0.5);
    let drawings = drawing_ops.unwrap_or(0);
    let drawings_known = drawing_ops.is_some();
    [
        // Forced without rulings: a forced grid with no drawn lattice behind it.
        is_forced && no_rulings,
        // Forced on a low judge: forced with no judge number or p < 0.5.
        is_forced && low_judge,
        // OCR text with drawings: at least half invisible/metric-less glyphs
        // beside at least one drawn cluster.
        ocr_text >= 0.5 && drawings_known && drawings >= 1,
        // Unaligned columns: fewer than half the columns align.
        cols_aligned < 0.5,
        // Drawing-dense region: three or more drawn clusters in the bbox.
        drawings_known && drawings >= 3,
        // Single-row/column fragment.
        rows <= 1 || cols <= 1,
        // Wordless: no cell holds a dictionary-shaped word.
        word_frac == 0.0,
        // Near-wholly OCR text: nine tenths invisible/metric-less glyphs.
        ocr_text >= 0.9,
    ]
}

/// Measure one grid's features: stamps off the cells, geometry off the page
/// context. `rows`/`cols` are the grid's, `bbox` its union box, `origin` its
/// stamp, `page_tables` the page's table count (`None` for a lone candidate).
/// `rules_override` carries a `find`-path candidate's horizontal rule count
/// where no lattice list reaches it; the ruled fraction stays unknown there,
/// which needs both orientations.
#[allow(clippy::too_many_arguments)]
fn table_features(
    cells: &[CellView],
    learned: &[Option<LearnedTrace>],
    rows: usize,
    cols: usize,
    bbox: [f64; 4],
    origin: TableOrigin,
    page_tables: Option<usize>,
    page: Option<PageCtx<'_>>,
    rules_override: Option<usize>,
) -> TableFeatures {
    let judge_p = learned
        .iter()
        .filter_map(|slot| *slot)
        .find_map(|slot| slot.judge_p);
    let forced = if origin == TableOrigin::LearnedForced {
        Some(
            learned
                .iter()
                .filter_map(|slot| *slot)
                .find_map(|slot| slot.forced),
        )
    } else {
        None
    };
    let (rules, ruled_frac, drawing_ops) = match (page, rules_override) {
        (Some(ctx), _) => {
            let (h, v) = rulings_in(bbox, ctx.rulings);
            let ruled = ruled_fraction(cells, bbox, ctx.rulings);
            let drawings = ctx
                .graphics
                .iter()
                .filter(|figure| box_hits(**figure, bbox))
                .count();
            (Some((h, v)), Some(ruled), Some(drawings))
        }
        (None, Some(h)) => (Some((h, 0)), None, None),
        (None, None) => (None, None, None),
    };
    let (rules_h, rules_v) = match rules {
        // A `find`-path override carries horizontals only: verticals unknown.
        Some((h, _)) if page.is_none() && rules_override.is_some() => (Some(h), None),
        Some((h, v)) => (Some(h), Some(v)),
        None => (None, None),
    };
    let total_chars: usize = cells.iter().map(|cell| cell.chars).sum();
    let ocr_chars: usize = cells.iter().map(|cell| cell.ocr_chars).sum();
    let ocr_text = if total_chars == 0 {
        0.0
    } else {
        ocr_chars as f64 / total_chars as f64
    };
    let fonts: HashSet<Option<&String>> = cells
        .iter()
        .flat_map(|cell| cell.fonts.iter().map(Option::as_ref))
        .collect();
    let fonts = fonts.len();
    let num_frac = if cells.is_empty() {
        0.0
    } else {
        cells.iter().filter(|cell| cell.numeric).count() as f64 / cells.len() as f64
    };
    let word_frac = if cells.is_empty() {
        0.0
    } else {
        cells.iter().filter(|cell| cell.wordly).count() as f64 / cells.len() as f64
    };
    let cols_aligned = aligned_fraction(cells, cols);
    let cand = candidate_flags(
        origin,
        judge_p,
        rows,
        cols,
        match (rules_h, rules_v) {
            (Some(h), Some(v)) => Some((h, v)),
            // Horizontals-only reads as ruled where a horizontal stands.
            (Some(h), None) => {
                if h > 0 {
                    Some((h, 1))
                } else {
                    Some((0, 0))
                }
            }
            _ => None,
        },
        ocr_text,
        word_frac,
        cols_aligned,
        drawing_ops,
    );
    TableFeatures {
        judge_p,
        forced,
        rules_h,
        rules_v,
        ruled_frac,
        ocr_text,
        fonts,
        num_frac,
        word_frac,
        cols_aligned,
        page_tables,
        drawing_ops,
        cand,
    }
}

/// Caption markers in a scope: `(text, box)` pairs whose text carries a
/// figure-caption marker.
fn markers_of(text_boxes: impl Iterator<Item = (String, [f64; 4])>) -> Vec<(String, [f64; 4])> {
    text_boxes
        .filter(|(text, _)| is_figure_caption(text))
        .collect()
}

/// One `TABLE ...` line on stderr. Numbers only: geometry, counts, verdicts,
/// features and candidate flags; cell and page text never reaches the output.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit(
    page: usize,
    stats: &TableStats,
    origin: TableOrigin,
    fig: Verdict,
    region: Verdict,
    demote: Verdict,
    features: &TableFeatures,
) {
    if !tracing() {
        return;
    }
    let caption = match stats.caption {
        None => "none".to_string(),
        Some(hit) => format!("{:.1},{:.1},{:.1}", hit.dx, hit.dy, hit.em),
    };
    let judge_p = match features.judge_p {
        Some(p) => format!("{p:.2}"),
        None => "-".to_string(),
    };
    let forced = match (origin, features.forced) {
        (TableOrigin::LearnedForced, Some(Some(reason))) => reason.as_trace().to_string(),
        (TableOrigin::LearnedForced, _) => "other".to_string(),
        _ => "-".to_string(),
    };
    let opt = |value: Option<usize>| match value {
        Some(n) => n.to_string(),
        None => "-".to_string(),
    };
    let frac = |value: Option<f64>| match value {
        Some(f) => format!("{f:.2}"),
        None => "-".to_string(),
    };
    let cand = features.cand;
    let mut line = String::new();
    let _ = write!(
        line,
        "TABLE page={page} bbox=[{:.1} {:.1} {:.1} {:.1}] rows={} cols={} cells={} origin={} median_alnum={} max_alpha_run={} empty_frac={:.2} continu_row0={} continu_rows={} caption_marker={caption} fig_veto={} region_veto={} demote={} judge_p={judge_p} forced={forced} rules_h={} rules_v={} ruled_frac={} ocr_text={:.2} fonts={} num_frac={:.2} word_frac={:.2} cols_aligned={:.2} page_tables={} drawing_ops={} cand_forced_noruling={} cand_forced_lowjudge={} cand_ocr_drawings={} cand_unaligned_cols={} cand_drawing_dense={} cand_single_strip={} cand_wordless={} cand_ocr_only={} rule_a={}",
        stats.bbox[0],
        stats.bbox[1],
        stats.bbox[2],
        stats.bbox[3],
        stats.rows,
        stats.cols,
        stats.ncells,
        origin.as_str(),
        stats.median_alnum,
        stats.max_alpha_run,
        stats.empty_frac,
        stats.continu_row0,
        stats.continu_rows,
        fig.as_trace(),
        region.as_trace(),
        demote.as_trace(),
        opt(features.rules_h),
        opt(features.rules_v),
        frac(features.ruled_frac),
        features.ocr_text,
        features.fonts,
        features.num_frac,
        features.word_frac,
        features.cols_aligned,
        opt(features.page_tables),
        opt(features.drawing_ops),
        cand[0] as u8,
        cand[1] as u8,
        cand[2] as u8,
        cand[3] as u8,
        cand[4] as u8,
        cand[5] as u8,
        cand[6] as u8,
        cand[7] as u8,
        rule_a(features).as_trace(),
    );
    eprintln!("{line}");
}

/// Rule A2 (#636, owner decision D37): `drawing_dense`.
///
/// A table is withdrawn when three or more drawn clusters stand inside its
/// box (`cand_drawing_dense`); its cells go back to text. The other
/// candidate flags are diagnostics: they print in the trace and never decide
/// anything. In particular a forced low-judge wordless grid stays a table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuleA {
    /// The table goes back to text.
    Withdrawn,
    /// The table stays; the reason is the trace word.
    Kept(&'static str),
}

impl RuleA {
    /// The trace word: `withdrawn`, or `kept:<reason>`.
    fn as_trace(self) -> String {
        match self {
            RuleA::Withdrawn => "withdrawn".to_string(),
            RuleA::Kept(reason) => format!("kept:{reason}"),
        }
    }
}

/// Below this ruled fraction a drawing-dense box is a figure, not a table.
/// Drawings inside a well-ruled box are the table's own rulings and shading.
const A2_RULED_FRAC_MAX: f64 = 0.6;

/// Rule A2 over the measured features: `cand_drawing_dense` (index 4) AND a
/// ruled fraction below [`A2_RULED_FRAC_MAX`] withdraw. An unmeasured ruled
/// fraction is never read as unruled: the table stays. Drawing counts exist
/// only where the page context does, and that context measures the ruled
/// fraction for every origin, so the unknown case is a guard, not a path.
fn rule_a(features: &TableFeatures) -> RuleA {
    if !features.cand[4] {
        return RuleA::Kept("no-trigger");
    }
    match features.ruled_frac {
        Some(frac) if frac < A2_RULED_FRAC_MAX => RuleA::Withdrawn,
        Some(_) => RuleA::Kept("ruled"),
        None => RuleA::Kept("ruled-unknown"),
    }
}

/// Rule A for one run of finished cells, whether or not the trace is on.
/// A run with no usable box is kept: nothing can be measured on it.
pub(crate) fn rule_a_of_cells(
    cells: &[&Block],
    origin: TableOrigin,
    page_ctx: Option<PageCtx<'_>>,
) -> RuleA {
    if cells.is_empty() {
        return RuleA::Kept("no-cells");
    }
    let views = views_of_blocks(cells);
    let Some((bbox, rows, cols)) = grid_shape(&views) else {
        return RuleA::Kept("no-box");
    };
    let learned: Vec<Option<LearnedTrace>> =
        cells.iter().map(|block| block.trace_learned).collect();
    let features = table_features(
        &views, &learned, rows, cols, bbox, origin, None, page_ctx, None,
    );
    rule_a(&features)
}

/// Rule A over one page's cells in the balanced tier: whether the table
/// stays. `rulings`/`graphics` are that page's.
#[cfg(feature = "ort-layout")]
pub(crate) fn rule_a_keeps(
    cells: &[Block],
    origin: TableOrigin,
    rulings: &[Ruling],
    graphics: &[[f64; 4]],
) -> bool {
    let refs: Vec<&Block> = cells.iter().collect();
    rule_a_of_cells(&refs, origin, Some(PageCtx { rulings, graphics })) != RuleA::Withdrawn
}

/// The union box, row count and column count of a run of cell views, or
/// `None` where the cells hold no usable box.
fn grid_shape(views: &[CellView]) -> Option<([f64; 4], usize, usize)> {
    let mut bbox = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for view in views {
        bbox[0] = bbox[0].min(view.bbox[0]);
        bbox[1] = bbox[1].min(view.bbox[1]);
        bbox[2] = bbox[2].max(view.bbox[2]);
        bbox[3] = bbox[3].max(view.bbox[3]);
    }
    if !(bbox[0].is_finite() && bbox[2] > bbox[0] && bbox[3] >= bbox[1]) {
        return None;
    }
    let rows = views
        .iter()
        .map(|view| view.row as usize + view.row_span.max(1) as usize)
        .max()
        .unwrap_or(0);
    let cols = views
        .iter()
        .map(|view| view.col as usize + view.col_span as usize)
        .max()
        .unwrap_or(0);
    Some((bbox, rows, cols))
}

/// Median cell height over finished cell boxes, the region veto's em.
fn median_cell_height(cells: &[CellView]) -> f64 {
    let mut heights: Vec<f64> = cells
        .iter()
        .map(|cell| cell.bbox[3] - cell.bbox[1])
        .collect();
    heights.sort_by(f64::total_cmp);
    heights.get(heights.len() / 2).copied().unwrap_or(0.0)
}

/// Tables on one page: runs of adjacent cells in row-major order, the same
/// runs [`settle_finals`] prints one line each.
fn page_table_count(page_blocks: &[Block], page: usize) -> usize {
    let mut count = 0usize;
    let mut last: Option<(u16, u16)> = None;
    for block in page_blocks.iter().filter(|block| block.page == page) {
        let at = match block.kind {
            crate::BlockKind::TableCell { row, column, .. } => Some((row, column)),
            _ => None,
        };
        match (at, last) {
            (Some((row, col)), Some((prow, pcol))) if (row, col) > (prow, pcol) => {
                last = Some((row, col));
            }
            (Some(at), _) => {
                count += 1;
                last = Some(at);
            }
            (None, _) => {
                last = None;
            }
        }
    }
    count
}

/// One `TABLE` line for a run of finished cell blocks: stats and all three
/// vetoes read off the cells, captions from the page's blocks.
///
/// `origin` is the run's stamp (`other` where the run mixes stamps).
/// `page_ctx` carries the page's graphics where the caller holds them.
/// Does nothing with the trace off.
pub(crate) fn emit_cells(
    page: usize,
    cells: &[&Block],
    page_blocks: &[Block],
    origin: TableOrigin,
    page_ctx: Option<PageCtx<'_>>,
) {
    if !tracing() || cells.is_empty() {
        return;
    }
    let views = views_of_blocks(cells);
    let Some((bbox, rows, cols)) = grid_shape(&views) else {
        return;
    };
    let em = median_cell_height(&views);
    let markers: Vec<(String, [f64; 4])> = markers_of(
        page_blocks
            .iter()
            .filter(|block| block.page == page)
            .map(|block| (block.text.clone(), block.bbox)),
    );
    let Some(stats) = table_stats(&views, bbox, em, &markers) else {
        return;
    };
    // The `find`-path veto re-read on finished cells: the fragmentary and
    // continued-panels disjuncts exactly; the panel-fragments neighbour check
    // has no rows beside the grid anymore and reads as its failure.
    let fig = fig_verdict(&views, rows, bbox, em, &markers, None);
    let facts = cell_facts(&views);
    let region = region_verdict(&views, &facts, bbox, em, &markers);
    let demote = demote_verdict(&views, &facts, bbox, em, &markers);
    let learned: Vec<Option<LearnedTrace>> =
        cells.iter().map(|block| block.trace_learned).collect();
    let features = table_features(
        &views,
        &learned,
        rows,
        cols,
        stats.bbox,
        origin,
        Some(page_table_count(page_blocks, page)),
        page_ctx,
        None,
    );
    emit(page, &stats, origin, fig, region, demote, &features);
}

/// The panel-fragments neighbour check beside a `find`-path candidate: a run
/// row within two rows of the grid whose text leads with "continued".
fn continu_lead_nearby(table: &crate::tables::Table, lines: &[&TextBlock], rows: &[Row]) -> bool {
    if table.cells.is_empty() {
        return false;
    }
    let nearby = |row: &Row| -> bool {
        let end = row.end.max(row.first);
        let text = lines
            .get(row.first..end)
            .map(|blocks| {
                blocks
                    .iter()
                    .flat_map(|block| block.spans.iter())
                    .map(|span| span.text.trim())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
        text.trim_start_matches(|c: char| c == '-' || c == '—' || c.is_whitespace())
            .to_ascii_lowercase()
            .starts_with("continu")
    };
    let lo = table.first.saturating_sub(2);
    let hi = (table.end + 2).min(rows.len());
    rows.get(lo..table.first.min(rows.len()))
        .is_some_and(|above| above.iter().any(&nearby))
        || rows
            .get(table.end.min(rows.len())..hi)
            .is_some_and(|below| below.iter().any(&nearby))
}

/// One `TABLE` line for a `find`-path candidate a veto removed: the veto
/// that fired reads `fired`, the others are evaluated on the candidate.
///
/// The page comes from [`trace_page_guard`]: skipped outside its scope
/// rather than mislabelled. `rules` are the path's horizontal rules, for the
/// `rules_h` field; every other page-context field prints `-` on this path.
/// Does nothing with the trace off.
pub(crate) fn emit_candidate(
    origin: TableOrigin,
    table: &crate::tables::Table,
    lines: &[&TextBlock],
    rows: &[Row],
    rules: &[HorizontalRule],
) {
    if !tracing() {
        return;
    }
    let Some(page) = trace_page() else {
        return;
    };
    let views = views_of_candidate(table, lines);
    if views.is_empty() {
        return;
    }
    // Row geometry and type size, the way the `find`-path veto measures the
    // grid (see `near_figure_caption`).
    let span = rows.get(table.first..table.end);
    let (mut grid_box, mut em) = ([0.0; 4], 0.0f64);
    let mut ok = false;
    if let Some(span) = span {
        let mut left = f64::INFINITY;
        let mut right = f64::NEG_INFINITY;
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        let mut size = f64::MIN_POSITIVE;
        for row in span {
            left = left.min(row.left);
            right = right.max(row.right);
            lo = lo.min(row.top.min(row.bottom));
            hi = hi.max(row.top.max(row.bottom));
            size = size.max(row.font_size);
        }
        if left.is_finite() && right > left && hi >= lo {
            grid_box = [left, lo, right, hi];
            em = size;
            ok = true;
        }
    }
    if !ok {
        return;
    }
    let markers: Vec<(String, [f64; 4])> = markers_of(lines.iter().map(|line| {
        let mut left = f64::INFINITY;
        let mut right = f64::NEG_INFINITY;
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for span in &line.spans {
            left = left.min(span.x);
            right = right.max(crate::blocks::span_right(span));
            lo = lo.min(span.y);
            hi = hi.max(span.y + span.height);
        }
        (line.text(), [left, lo, right, hi])
    }));
    let Some(stats) = table_stats(&views, grid_box, em, &markers) else {
        return;
    };
    let nearby = continu_lead_nearby(table, lines, rows);
    let fig = fig_verdict(
        &views,
        table.end - table.first,
        grid_box,
        em,
        &markers,
        Some(nearby),
    );
    // The region and demote vetoes read off finished cells; on a candidate
    // the cell boxes come from the spans, and the em from the rows above.
    // The verdicts still say which shape the veto sees.
    let facts = cell_facts(&views);
    let region = region_verdict(&views, &facts, grid_box, em, &markers);
    let demote = demote_verdict(&views, &facts, grid_box, em, &markers);
    let cols = views
        .iter()
        .map(|view| view.col as usize + view.col_span as usize)
        .max()
        .unwrap_or(0);
    let rules_h = rules
        .iter()
        .filter(|rule| {
            rule.y >= stats.bbox[1] - 0.5
                && rule.y <= stats.bbox[3] + 0.5
                && overlaps_1d(rule.left, rule.right, stats.bbox[0], stats.bbox[2])
        })
        .count();
    let learned: Vec<Option<LearnedTrace>> = views.iter().map(|_| None).collect();
    let features = table_features(
        &views,
        &learned,
        table.end - table.first,
        cols,
        stats.bbox,
        origin,
        None,
        None,
        Some(rules_h),
    );
    emit(page, &stats, origin, fig, region, demote, &features);
}

/// One `TABLE` line for a run of finished cell blocks that share a page:
/// the run's own stamp, or `other` where it mixes stamps. `page_ctx` carries
/// the page's graphics where the caller holds them. Does nothing with the
/// trace off.
pub(crate) fn emit_run(
    page: usize,
    cells: &[Block],
    page_blocks: &[Block],
    page_ctx: Option<PageCtx<'_>>,
) {
    if !tracing() {
        return;
    }
    let refs: Vec<&Block> = cells.iter().collect();
    emit_cells(page, &refs, page_blocks, run_origin(&refs), page_ctx);
}

/// The origin one run of finished cells reports: its first cell's stamp, or
/// `other` where the run mixes stamps.
pub(crate) fn run_origin(cells: &[&Block]) -> TableOrigin {
    let mut origin = None;
    for cell in cells {
        if origin.is_none() {
            origin = Some(cell.table_origin);
        } else if origin != Some(cell.table_origin) {
            return TableOrigin::Other;
        }
    }
    origin.unwrap_or(TableOrigin::Other)
}

/// A table rule A took out of the final blocks: its page and its cells' boxes.
/// The balanced tier reads these so its region rebuild cannot bring the table
/// back.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WithdrawnTable {
    /// The page the table stood on.
    pub page: usize,
    /// The boxes of the cells that went back to text.
    pub cells: Vec<[f64; 4]>,
}

/// Where each table of finished blocks stands: runs of adjacent cells in
/// row-major order (a run restarts where `(row, column)` stops advancing, as
/// the serializers read tables), as `start..end` block ranges.
fn final_runs(blocks: &[Block]) -> Vec<std::ops::Range<usize>> {
    let mut runs = Vec::new();
    let mut start: Option<usize> = None;
    let mut last: Option<(usize, u16, u16)> = None;
    for (i, block) in blocks.iter().enumerate() {
        let at = match block.kind {
            crate::BlockKind::TableCell { row, column, .. } => Some((block.page, row, column)),
            _ => None,
        };
        match (at, last) {
            (Some((page, row, col)), Some((ppage, prow, pcol)))
                if page == ppage && (row, col) > (prow, pcol) =>
            {
                last = Some((page, row, col));
            }
            (Some(at), _) => {
                if let Some(first) = start.take() {
                    runs.push(first..i);
                }
                start = Some(i);
                last = Some(at);
            }
            (None, _) => {
                if let Some(first) = start.take() {
                    runs.push(first..i);
                }
                last = None;
            }
        }
    }
    if let Some(first) = start {
        runs.push(first..blocks.len());
    }
    runs
}

/// Rule A over the final blocks (#636), then one `TABLE` line per table when
/// the trace is on. A withdrawn table's cells go back to paragraphs in the
/// order they stand, the way `demote_fragment_tables` leaves a vetoed run:
/// kind, record and confidence cleared, no text lost. `rulings`/`graphics`
/// are the pages' lattice rulings and drawn-cluster boxes, read only.
pub(crate) fn settle_finals(
    blocks: &mut [Block],
    rulings: &[Vec<Ruling>],
    graphics: &[Vec<[f64; 4]>],
) -> Vec<WithdrawnTable> {
    let mut withdrawn: Vec<std::ops::Range<usize>> = Vec::new();
    for run in final_runs(blocks) {
        let group: Vec<&Block> = blocks[run.clone()].iter().collect();
        let page = group[0].page;
        let page_ctx = rulings.get(page).map(|page_rulings| PageCtx {
            rulings: page_rulings,
            graphics: graphics.get(page).map(Vec::as_slice).unwrap_or(&[]),
        });
        let origin = run_origin(&group);
        // The line first: it prints the verdict this run is about to get.
        emit_cells(page, &group, blocks, origin, page_ctx);
        if rule_a_of_cells(&group, origin, page_ctx) == RuleA::Withdrawn {
            withdrawn.push(run);
        }
    }
    let mut gone = Vec::with_capacity(withdrawn.len());
    for run in withdrawn {
        gone.push(WithdrawnTable {
            page: blocks[run.start].page,
            cells: blocks[run.clone()].iter().map(|block| block.bbox).collect(),
        });
        for block in &mut blocks[run] {
            block.kind = crate::BlockKind::Paragraph;
            block.grid = None;
            block.confidence = None;
            block.continues_previous = false;
            block.table_origin = TableOrigin::Other;
            block.trace_learned = None;
        }
    }
    gone
}
