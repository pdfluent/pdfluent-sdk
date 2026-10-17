//! The balanced tier's page assembly: detector boxes plus our own text, page in, markdown out (#641).
//!
//! Behind the `ort-layout` cargo feature (off by default), beside
//! [`crate::layout`]. This is the Rust port of the balanced adapter --
//! `scripts/extraction/adapters/teacher_page.py` with detector A (Heron-ft
//! ep03) only, no union, and the chosen thresholds -- rule for rule:
//!
//! * regions from the detector's boxes, kept per class at
//!   [`THRESHOLDS`] (section_header/title 0.25, list_item 0.10,
//!   page_header/footer 0.10, table 0.75, everything else 0.50);
//! * lines from the engine's own words before the learned pass (the same
//!   [`crate::learned::features::page_features`] the `feature_dump` record
//!   carries), grouped exactly as `align_labels.lines_of` groups them;
//! * each line to the TIGHTEST kept region holding at least half its ink,
//!   placed where its first line falls in the engine's reading order;
//! * R1: a table region takes TATR's cells where the stage fires (only when
//!   [`BalancedOptions::with_tatr`] opts in), else the grid of the engine's
//!   own table in the same place (position filters, text ranks), else the
//!   grid the engine builds inside the region, else its lines as paragraphs;
//! * R2: a page with no words is the engine's own markdown for the page,
//!   verbatim;
//! * R3/R8/R9: bold/italic runs with the engine's markers, the engine's
//!   sup/sub/underline tags on aligned words, one style run across a line
//!   break;
//! * R4: a taken grid's leftover lines as paragraphs around it;
//! * R5: a short line (at most [`SHORT_HEADING_WORDS`] words) claimed by a
//!   below-threshold section_header/title box at or above [`SHORT_THRESHOLD`]
//!   is a heading;
//! * R7: an engine table no region took is written at the reading position of
//!   the lines it covers;
//! * R10: ligatures folded, hyphen wraps joined, a list item's own bullet
//!   dropped;
//! * furniture (page_header/page_footer) dropped, unless
//!   [`BalancedOptions::keep_furniture`] keeps it as paragraphs.
//!
//! [`balanced_page_markdown`] assembles one page from caller-held boxes --
//! [`crate::layout::layout_boxes`] output or cached boxes injected for
//! parity -- and [`balanced_markdown`] runs the detector per page. The
//! table-structure stage hooks in per kept table region through
//! [`BalancedOptions::with_tatr`]: TATR first, and only on a fire are its
//! cells the region's table. There is no coarser switch here: a whole-page
//! override would bypass the stage's own per-region invariants (no word
//! lost, the coverage bar, the gap fill).
//!
//! Byte parity with the Python adapter (same boxes, same engine) is the
//! contract: every helper below ports its Python original exactly, including
//! `difflib.SequenceMatcher` (autojunk off, no junk) for the R8 alignment and
//! the bench's `blocks_of`/`text_similarity` for the R1 match. Where the
//! engine already computes the identical value in this crate (`ligatures`,
//! `is_hyphen_wrap_candidate`, `BULLET_CHARS`), the port calls it rather than
//! copying it.
//!
//! Model failures come back as [`EngineError::Io`](crate::EngineError)
//! carrying the typed [`pdf_ocr::runtime::ModelError`] -- no new
//! `EngineError` variants.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::collections::{HashMap, HashSet};

use crate::layout::{layout_boxes, LayoutBox, RuntimeConfig};
use crate::learned::features::{page_features, PageWord};
use crate::table_trace::TableOrigin;
use crate::{Block, BlockKind, EngineError, ExtractOptions, PdfDocument, Result, TatrOptions};

/// Detector A (Heron-ft ep03) per-class keep thresholds: the balanced tier's
/// choices (T3's `chosen.json`: section_header/title 0.25, list_item 0.10,
/// page_header/footer 0.10, table 0.75, default 0.50). A box is kept when its
/// score reaches its class threshold; anything below the post threshold never
/// reaches this table (see [`crate::layout`]).
#[derive(Debug, Clone, Copy)]
struct Thresholds {
    /// Keep threshold for `section_header` and `title`.
    heading: f32,
    /// Keep threshold for `list_item`.
    list_item: f32,
    /// Keep threshold for `table`.
    table: f32,
    /// Keep threshold for `page_header` and `page_footer`.
    furniture: f32,
    /// Keep threshold for every other label.
    default: f32,
}

/// The balanced tier's thresholds, from T3's `chosen.json`.
const THRESHOLDS: Thresholds = Thresholds {
    heading: 0.25,
    list_item: 0.10,
    table: 0.75,
    furniture: 0.10,
    default: 0.50,
};

/// R5: a below-threshold section_header/title box at or above this score still
/// promotes the short lines it claims to headings.
const SHORT_THRESHOLD: f32 = 0.05;

/// R5: a line of at most this many words may become a heading on a weak
/// detector-A signal.
const SHORT_HEADING_WORDS: usize = 12;

/// R1: how alike a table region's text and an engine table's text must be
/// before the engine's grid is used.
const GRID_FLOOR: f64 = 0.5;

/// R1: an engine table is the region's when at least this share of its cells
/// lie inside; a cell counts when [`POSITION_CELLS`] of its own area does.
const POSITION_TABLE: f64 = 0.5;

/// R1: the share of a cell's own area inside the region for the cell to count.
const POSITION_CELLS: f64 = 0.5;

/// R7: an orphan engine table covers a line when half the line's ink lies
/// inside the table's cell-union box padded by this many points.
const ORPHAN_PAD: f64 = 2.0;

/// How wide a gap inside a line has to be, in ems of the line's own height,
/// before it is a column boundary rather than a space (`align_labels`).
const COLUMN_GAP_EM: f64 = 1.5;

/// Options for [`balanced_page_markdown`] and [`balanced_markdown`].
///
/// The tier's own settings (thresholds, R5's bar, R7-R10 on, furniture
/// dropped) are fixed; what varies per caller is whether furniture lines are
/// kept, and whether the table-structure stage runs inside the kept table
/// regions.
#[derive(Debug, Clone, Default)]
pub struct BalancedOptions {
    /// Keep page_header/page_footer lines as paragraphs instead of dropping
    /// them (the compat `to_markdown(header=, footer=)` mapping).
    keep_furniture: bool,
    /// TATR table structure inside the kept table regions. Default `None`:
    /// R1 as it always ran. Set with [`Self::with_tatr`] only -- a private
    /// field, so the opt-in stays a builder call.
    tatr: Option<TatrOptions>,
}

impl BalancedOptions {
    /// Keep furniture lines as paragraphs (`true`) or drop them (the default).
    #[must_use]
    pub fn keep_furniture(mut self, keep: bool) -> Self {
        self.keep_furniture = keep;
        self
    }

    /// Rebuild the kept table regions with TATR first, where it fires (track
    /// B, P4).
    ///
    /// The only way to set the TATR opt-in: there is no public field. With
    /// it set, R1 offers each kept table region to the Table Transformer
    /// before the engine grid runs; where TATR fires its cells are the
    /// region's table, and where it declines, fires under coverage, or the
    /// region fails, R1 continues exactly as without the option. R7 orphan
    /// engine tables never reach TATR: no detector region, no offer. A
    /// missing model refuses up front, never a silent run without the stage
    /// -- as does asking on a build without the `ort-tables` feature.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use pdf_engine::balanced::BalancedOptions;
    /// use pdf_engine::TatrOptions;
    ///
    /// let options = BalancedOptions::default().with_tatr(TatrOptions::default());
    /// # let _ = options;
    /// ```
    #[must_use]
    pub fn with_tatr(mut self, tatr: TatrOptions) -> Self {
        self.tatr = Some(tatr);
        self
    }
}

/// One page plus its layout boxes as markdown, the balanced tier's assembly.
///
/// `boxes` are detector A's boxes for `page` (from
/// [`layout_boxes`](crate::layout::layout_boxes), or cached boxes injected
/// for parity), each addressed against its own render size. `page` counts
/// from zero. The markdown holds no page marker; [`balanced_markdown`] joins
/// pages with a blank line.
pub fn balanced_page_markdown(
    doc: &PdfDocument,
    page: usize,
    boxes: &[LayoutBox],
    opts: &BalancedOptions,
) -> Result<String> {
    let count = doc.page_count();
    if page >= count {
        return Err(EngineError::PageOutOfRange { index: page, count });
    }
    // The TATR context up front, before any early return: where the stage is
    // asked for and the model is absent the page refuses here with the
    // shared layer's `MODEL_NOT_AVAILABLE`, never a silent run without it --
    // on a wordless page as on any other. `None` -- the stage not asked
    // for -- assembles exactly as it always has: the R1 step below is gated
    // on `Some`.
    let tatr = doc.tatr_context(opts.tatr.as_ref())?;
    let (width, height) = doc
        .page_geometry(page)
        .map(|g| g.effective_dimensions())
        .unwrap_or((0.0, 0.0));
    let (before, _) = doc.extract_blocks_and_rules_before_learning()?;
    let on_page: Vec<Block> = before.into_iter().filter(|b| b.page == page).collect();
    let refs: Vec<&Block> = on_page.iter().collect();
    let words = page_features(&refs, width, height).words;

    let options = ExtractOptions::default();
    let (blocks, rules, withdrawn) = doc.extract_blocks_rules_and_withdrawn(&options)?;
    // Rule A's inputs for the tables this tier builds itself (#636), and the
    // tables the final blocks already lost to it.
    let (page_rulings, page_graphics) = doc.page_rulings_and_graphics(page)?;
    let withdrawn: Vec<Vec<[f64; 4]>> = withdrawn
        .into_iter()
        .filter(|table| table.page == page)
        .map(|table| table.cells)
        .collect();
    let engine_blocks: Vec<Block> = blocks.into_iter().filter(|b| b.page == page).collect();
    let engine_markdown = crate::markdown::to_markdown_with_rules(&engine_blocks, &rules);
    if words.is_empty() {
        return Ok(engine_markdown);
    }

    let (regions, short) = regions_of(boxes, width, height, opts.keep_furniture);
    let mut matcher = GridMatcher::new(&engine_markdown, &engine_blocks);
    let region_table = |bbox: [f64; 4]| -> Option<RegionGrid> {
        match doc.table_in_region(page, bbox) {
            Ok(cells) => {
                let grid = crate::markdown::to_markdown(&cells);
                if grid.is_empty() || grid.trim().is_empty() {
                    None
                } else {
                    Some(RegionGrid { html: grid, cells })
                }
            }
            Err(_) => None,
        }
    };
    let tatr_table = |bbox: [f64; 4]| -> Option<RegionGrid> {
        let context = tatr.as_ref()?;
        match doc.tatr_cells_in_region(page, bbox, context) {
            Ok(Some(cells)) => {
                let grid = crate::markdown::to_markdown(&cells);
                if grid.is_empty() || grid.trim().is_empty() {
                    None
                } else {
                    Some(RegionGrid { html: grid, cells })
                }
            }
            Ok(None) | Err(_) => None,
        }
    };
    let tatr_table: Option<&dyn Fn([f64; 4]) -> Option<RegionGrid>> = if tatr.is_some() {
        Some(&tatr_table)
    } else {
        None
    };
    Ok(assemble(
        &words,
        on_page.len(),
        &regions,
        &short,
        &mut matcher,
        &region_table,
        tatr_table,
        &engine_markdown,
        page,
        &engine_blocks,
        &RuleAScope {
            rulings: &page_rulings,
            graphics: &page_graphics,
            withdrawn: &withdrawn,
        },
    ))
}

/// What rule A reads for one page in [`assemble`] (#636).
struct RuleAScope<'a> {
    /// The page's lattice rulings.
    rulings: &'a [crate::text::Ruling],
    /// The page's drawn-cluster boxes.
    graphics: &'a [[f64; 4]],
    /// The cell boxes of each engine table the final blocks already lost to
    /// rule A: a layout "table" box that holds one gets no rebuilt table.
    withdrawn: &'a [Vec<[f64; 4]>],
}

impl RuleAScope<'_> {
    /// Whether rule A keeps a table this tier built from `cells`.
    fn keeps(&self, cells: &[Block]) -> bool {
        let refs: Vec<&Block> = cells.iter().collect();
        let origin = crate::table_trace::run_origin(&refs);
        crate::table_trace::rule_a_keeps(cells, origin, self.rulings, self.graphics)
    }

    /// Whether `bbox` holds a withdrawn engine table: the matcher's own
    /// position test, [`POSITION_CELLS`] of a cell inside the box for
    /// [`POSITION_TABLE`] of the table's cells.
    fn holds_withdrawn(&self, bbox: &[f64; 4]) -> bool {
        self.withdrawn.iter().any(|cells| {
            let inside = cells
                .iter()
                .filter(|c| area(c) > 0.0 && overlap(c, bbox) / area(c) >= POSITION_CELLS)
                .count();
            !cells.is_empty() && inside as f64 / cells.len() as f64 >= POSITION_TABLE
        })
    }
}

/// The document as markdown through the balanced tier: [`layout_boxes`](crate::layout::layout_boxes)
/// per page, each page through [`balanced_page_markdown`].
///
/// Pages join with a blank line; empty pages contribute nothing. Needs the
/// layout model (see [`crate::layout`]); without it the error is the shared
/// layer's `MODEL_NOT_AVAILABLE`.
pub fn balanced_markdown(
    doc: &PdfDocument,
    config: &RuntimeConfig,
    opts: &BalancedOptions,
) -> Result<String> {
    let mut pages = Vec::new();
    for page in 0..doc.page_count() {
        let boxes = layout_boxes(doc, page, config)?;
        let markdown = balanced_page_markdown(doc, page, &boxes, opts)?;
        if !markdown.is_empty() {
            pages.push(markdown);
        }
    }
    Ok(pages.join("\n\n"))
}

/// What a region's lines become.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum RegionKind {
    /// `section_header`/`title`: `## `.
    Heading,
    /// `list_item`: `- `.
    ListItem,
    /// `table`: a grid.
    Table,
    /// `page_header`/`page_footer`: dropped, unless kept as paragraphs.
    Furniture,
    /// Every other label: a paragraph.
    Paragraph,
}

/// One kept detector region, in PDF user space (points, y up).
#[derive(Debug, Clone)]
struct Region {
    /// What the region's lines become.
    kind: RegionKind,
    /// The detector's score (orders candidates; never printed).
    score: f32,
    /// `[x0, y0, x1, y1]` in PDF points, y up.
    bbox: [f64; 4],
}

/// The keep threshold for one detector label.
fn class_threshold(label: &str) -> f32 {
    match label {
        "section_header" | "title" => THRESHOLDS.heading,
        "list_item" => THRESHOLDS.list_item,
        "table" => THRESHOLDS.table,
        "page_header" | "page_footer" => THRESHOLDS.furniture,
        _ => THRESHOLDS.default,
    }
}

/// One detector box (render px, top-left origin) in PDF user space (points,
/// bottom-left origin).
fn to_pdf_box(bbox: [f32; 4], render_w: u32, render_h: u32, page_w: f64, page_h: f64) -> [f64; 4] {
    let (w, h) = (f64::from(render_w), f64::from(render_h));
    let (sx, sy) = (page_w / w, page_h / h);
    let bbox = [
        f64::from(bbox[0]),
        f64::from(bbox[1]),
        f64::from(bbox[2]),
        f64::from(bbox[3]),
    ];
    [
        bbox[0] * sx,
        page_h - bbox[3] * sy,
        bbox[2] * sx,
        page_h - bbox[1] * sy,
    ]
}

/// The kept regions plus the R5 short regions, in input order.
///
/// Kept are the boxes at or above their class threshold; short are
/// section_header/title boxes at or above [`SHORT_THRESHOLD`] but below it.
/// With `keep_furniture`, furniture boxes read as paragraphs everywhere.
fn regions_of(
    boxes: &[LayoutBox],
    page_w: f64,
    page_h: f64,
    keep_furniture: bool,
) -> (Vec<Region>, Vec<Region>) {
    let mut kept = Vec::new();
    let mut short = Vec::new();
    for b in boxes {
        let bbox = to_pdf_box(b.bbox_px, b.render_w, b.render_h, page_w, page_h);
        let threshold = class_threshold(&b.label);
        if b.score >= threshold {
            let mut kind = match b.label.as_str() {
                "section_header" | "title" => RegionKind::Heading,
                "list_item" => RegionKind::ListItem,
                "table" => RegionKind::Table,
                "page_header" | "page_footer" => RegionKind::Furniture,
                _ => RegionKind::Paragraph,
            };
            if keep_furniture && kind == RegionKind::Furniture {
                kind = RegionKind::Paragraph;
            }
            kept.push(Region {
                kind,
                score: b.score,
                bbox,
            });
        } else if (b.label == "section_header" || b.label == "title") && b.score >= SHORT_THRESHOLD
        {
            short.push(Region {
                kind: RegionKind::Heading,
                score: b.score,
                bbox,
            });
        }
    }
    (kept, short)
}

/// The area of one `[x0, y0, x1, y1]` box.
fn area(box_: &[f64; 4]) -> f64 {
    (box_[2] - box_[0]).max(0.0) * (box_[3] - box_[1]).max(0.0)
}

/// The overlap area of two `[x0, y0, x1, y1]` boxes.
fn overlap(one: &[f64; 4], other: &[f64; 4]) -> f64 {
    (one[2].min(other[2]) - one[0].max(other[0])).max(0.0)
        * (one[3].min(other[3]) - one[1].max(other[1])).max(0.0)
}

/// The words of one block grouped into lines, top to bottom, as word indices.
///
/// Ports `align_labels.lines_of`: words join a line when their boxes overlap
/// vertically by more than half of the shorter of the two (against the line's
/// own span, not the word that opened it), lines sort left to right, split
/// across a column gap, and come back in reading order.
fn lines_of(block: usize, words: &[PageWord]) -> Vec<Vec<usize>> {
    let mut ink: Vec<usize> = words
        .iter()
        .enumerate()
        .filter(|(_, w)| w.block == block)
        .map(|(i, _)| i)
        .collect();
    // Stable, as Python's sort is: identical boxes keep word order.
    ink.sort_by(|&a, &b| {
        words[b].bbox[3]
            .total_cmp(&words[a].bbox[3])
            .then_with(|| words[a].bbox[0].total_cmp(&words[b].bbox[0]))
    });
    let mut lines: Vec<Vec<usize>> = Vec::new();
    for word in ink {
        let box_ = words[word].bbox;
        let mut placed = false;
        for line in &mut lines {
            let low = line
                .iter()
                .map(|&w| words[w].bbox[1])
                .fold(f64::INFINITY, f64::min);
            let high = line
                .iter()
                .map(|&w| words[w].bbox[3])
                .fold(f64::NEG_INFINITY, f64::max);
            let shorter = (box_[3] - box_[1]).min(high - low);
            let shorter = if shorter == 0.0 { 1.0 } else { shorter };
            let share = (box_[3].min(high) - box_[1].max(low)) / shorter;
            if share > 0.5 {
                line.push(word);
                placed = true;
                break;
            }
        }
        if !placed {
            lines.push(vec![word]);
        }
    }
    for line in &mut lines {
        line.sort_by(|&a, &b| words[a].bbox[0].total_cmp(&words[b].bbox[0]));
    }
    let pieces: Vec<Vec<usize>> = lines
        .iter()
        .flat_map(|line| across_a_column(line, words))
        .collect();
    in_reading_order(pieces, words)
}

/// One y-grouped line, cut where it crosses from one column into another.
///
/// A gap at or above [`COLUMN_GAP_EM`] ems of the line's own (median) height
/// is a column boundary. The median is the upper middle, and a zero height
/// reads as 1.0, as in `align_labels`.
fn across_a_column(line: &[usize], words: &[PageWord]) -> Vec<Vec<usize>> {
    if line.len() < 2 {
        return vec![line.to_vec()];
    }
    let mut heights: Vec<f64> = line
        .iter()
        .map(|&w| words[w].bbox[3] - words[w].bbox[1])
        .collect();
    heights.sort_by(f64::total_cmp);
    let mut height = heights[heights.len() / 2];
    if height == 0.0 {
        height = 1.0;
    }
    let mut out = Vec::new();
    let mut run = vec![line[0]];
    for window in line.windows(2) {
        let (before, word) = (window[0], window[1]);
        if (words[word].bbox[0] - words[before].bbox[2]) / height >= COLUMN_GAP_EM {
            out.push(run);
            run = vec![word];
        } else {
            run.push(word);
        }
    }
    out.push(run);
    out
}

/// The pieces of a block, column by column and top to bottom inside each.
///
/// A piece joins a column when it shares more than half of the narrower of
/// the two x-ranges with it; columns sort by their left edge, pieces inside
/// by their top (stable), as in `align_labels`.
fn in_reading_order(pieces: Vec<Vec<usize>>, words: &[PageWord]) -> Vec<Vec<usize>> {
    if pieces.len() < 2 {
        return pieces;
    }
    struct Column {
        left: f64,
        right: f64,
        pieces: Vec<Vec<usize>>,
    }
    let mut columns: Vec<Column> = Vec::new();
    for piece in pieces {
        let left = piece
            .iter()
            .map(|&w| words[w].bbox[0])
            .fold(f64::INFINITY, f64::min);
        let right = piece
            .iter()
            .map(|&w| words[w].bbox[2])
            .fold(f64::NEG_INFINITY, f64::max);
        let mut placed = false;
        for column in &mut columns {
            let width = (right - left).min(column.right - column.left);
            let width = if width == 0.0 { 1.0 } else { width };
            let shared = right.min(column.right) - left.max(column.left);
            if shared / width > 0.5 {
                column.left = column.left.min(left);
                column.right = column.right.max(right);
                column.pieces.push(piece.clone());
                placed = true;
                break;
            }
        }
        if !placed {
            columns.push(Column {
                left,
                right,
                pieces: vec![piece],
            });
        }
    }
    columns.sort_by(|a, b| a.left.total_cmp(&b.left));
    let mut out = Vec::new();
    for column in columns {
        let mut pieces = column.pieces;
        pieces.sort_by(|a, b| {
            let top = |piece: &[usize]| {
                piece
                    .iter()
                    .map(|&w| words[w].bbox[3])
                    .fold(f64::NEG_INFINITY, f64::max)
            };
            top(b).total_cmp(&top(a))
        });
        out.extend(pieces);
    }
    out
}

/// One placed line: its reading-order sequence, plain and shown text, box,
/// and block index.
struct PlacedLine {
    /// Reading-order sequence across the page.
    seq: usize,
    /// The words joined by a space.
    plain: String,
    /// The words with their inline style.
    styled: String,
    /// The union of the words' boxes.
    bbox: [f64; 4],
    /// The block index (for grouping loose lines).
    block: usize,
}

/// A region-built grid, with the cells it was written from: the trace reads
/// the numbers off the cells, and the markdown is what the page prints.
struct RegionGrid {
    /// The grid as markdown.
    html: String,
    /// The cells the grid was written from (already stamped by their stage).
    cells: Vec<Block>,
}

/// Our lines onto the regions, as markdown.
///
/// Ports `teacher_page.markdown` with the balanced settings: the tightest
/// region holding at least half a line's ink wins; blocks stand where their
/// first line falls in reading order; furniture drops. `table_grid` is the
/// R1 matcher, `region_table` the region entry, `tatr_table` the TATR-only
/// attempt per kept table region (`None` where the stage is off, and R1
/// then runs exactly as without it), `engine_markdown` the page's own
/// markdown (R2's fallback, R8's tags). `page` and `page_blocks` are the
/// page's own number and engine blocks: the table trace's scope.
#[allow(clippy::too_many_arguments)]
fn assemble(
    words: &[PageWord],
    block_count: usize,
    regions: &[Region],
    short: &[Region],
    table_grid: &mut GridMatcher,
    region_table: &dyn Fn([f64; 4]) -> Option<RegionGrid>,
    tatr_table: Option<&dyn Fn([f64; 4]) -> Option<RegionGrid>>,
    engine_markdown: &str,
    page: usize,
    page_blocks: &[Block],
    rule_a: &RuleAScope<'_>,
) -> String {
    let page_ctx = Some(crate::table_trace::PageCtx {
        rulings: rule_a.rulings,
        graphics: rule_a.graphics,
    });
    let pool: Vec<&Region> = regions.iter().chain(short.iter()).collect();
    let n_kept = regions.len();
    let styles = if engine_markdown.is_empty() {
        None
    } else {
        Some(engine_styles_for(words, engine_markdown))
    };
    let mut items: HashMap<usize, Vec<PlacedLine>> = HashMap::new();
    let mut order: Vec<usize> = Vec::new();
    let mut loose: Vec<PlacedLine> = Vec::new();
    let mut seq = 0usize;
    for block in 0..block_count {
        for piece in lines_of(block, words) {
            if piece.is_empty() {
                continue;
            }
            let mut ink: f64 = piece.iter().map(|&w| area(&words[w].bbox)).sum();
            if ink == 0.0 {
                ink = 1.0;
            }
            let mut cands: Vec<(f64, f32, usize)> = Vec::new();
            for (ri, region) in pool.iter().enumerate() {
                let held: f64 = piece
                    .iter()
                    .map(|&w| overlap(&words[w].bbox, &region.bbox))
                    .sum();
                if held / ink >= 0.5 && (ri < n_kept || piece.len() <= SHORT_HEADING_WORDS) {
                    cands.push((area(&region.bbox), region.score, ri));
                }
            }
            cands.sort_by(|a, b| {
                a.0.total_cmp(&b.0)
                    .then_with(|| b.1.total_cmp(&a.1))
                    .then_with(|| a.2.cmp(&b.2))
            });
            let pick = cands.first().map(|(_, _, ri)| *ri);
            let plain = piece
                .iter()
                .map(|&w| words[w].text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            let piece_words: Vec<&PageWord> = piece.iter().map(|&w| &words[w]).collect();
            let styled = match &styles {
                Some(styles) => styled_line_engine(&piece, &piece_words, styles),
                None => styled_line(&piece_words),
            };
            let bbox = [
                piece
                    .iter()
                    .map(|&w| words[w].bbox[0])
                    .fold(f64::INFINITY, f64::min),
                piece
                    .iter()
                    .map(|&w| words[w].bbox[1])
                    .fold(f64::INFINITY, f64::min),
                piece
                    .iter()
                    .map(|&w| words[w].bbox[2])
                    .fold(f64::NEG_INFINITY, f64::max),
                piece
                    .iter()
                    .map(|&w| words[w].bbox[3])
                    .fold(f64::NEG_INFINITY, f64::max),
            ];
            let line = PlacedLine {
                seq,
                plain,
                styled,
                bbox,
                block,
            };
            seq += 1;
            match pick {
                Some(ri) => {
                    if !items.contains_key(&ri) {
                        order.push(ri);
                    }
                    items.entry(ri).or_default().push(line);
                }
                None => loose.push(line),
            }
        }
    }

    // What each claimed region becomes: TATR's cells where the stage fires,
    // else the matched engine grid, else the region-built grid, else
    // paragraphs.
    let mut decided: HashMap<usize, (RegionKind, Option<String>, bool)> = HashMap::new();
    for &ri in &order {
        let lines = &items[&ri];
        let text = lines
            .iter()
            .map(|l| l.plain.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let mut kind = pool[ri].kind;
        let mut grid: Option<String> = None;
        let mut engine_grid = false;
        // R1 step (1): TATR only, per kept table region. Only kept regions
        // reach this with `Table` kind -- short regions are headings, and
        // orphans never pass through here (R7 reads the leftovers below).
        // On a fire its cells are the grid and the region's unclaimed lines
        // stay for R4, exactly as with an engine grid; on a fallback R1
        // continues below as without the stage.
        // Rule A (#636) over every table this step could emit. A layout
        // "table" box that holds an engine table rule A already withdrew gets
        // no rebuilt grid either: the region and TATR rebuilds would read the
        // same figure labels and put the table back.
        let mut refused = kind == RegionKind::Table && rule_a.holds_withdrawn(&pool[ri].bbox);
        if let (None, RegionKind::Table, Some(tatr), false) = (&grid, kind, tatr_table, refused) {
            if let Some(built) = tatr(pool[ri].bbox) {
                crate::table_trace::emit_run(page, &built.cells, page_blocks, page_ctx);
                if rule_a.keeps(&built.cells) {
                    grid = Some(built.html);
                    engine_grid = true;
                } else {
                    refused = true;
                }
            }
        }
        if grid.is_none() && kind == RegionKind::Table {
            grid = table_grid.table_grid(&text, &pool[ri].bbox);
            if grid.is_some() {
                engine_grid = true;
                if let Some(cells) = table_grid.picked_cells() {
                    let refs: Vec<&Block> = cells.iter().collect();
                    crate::table_trace::emit_cells(
                        page,
                        &refs,
                        page_blocks,
                        TableOrigin::EngineGridMatch,
                        page_ctx,
                    );
                    if !rule_a.keeps(cells) {
                        grid = None;
                        engine_grid = false;
                        refused = true;
                    }
                }
            }
        }
        if grid.is_none() && kind == RegionKind::Table {
            if !refused {
                if let Some(built) = region_table(pool[ri].bbox) {
                    let html = built.html;
                    if !html.is_empty() && !html.trim().is_empty() {
                        crate::table_trace::emit_run(page, &built.cells, page_blocks, page_ctx);
                        if rule_a.keeps(&built.cells) {
                            grid = Some(html);
                            engine_grid = true;
                        }
                    }
                }
            }
            if grid.is_none() {
                kind = RegionKind::Paragraph;
            }
        }
        decided.insert(ri, (kind, grid, engine_grid));
    }

    type BlockRow = (
        usize,
        RegionKind,
        String,
        Option<String>,
        Vec<String>,
        Vec<String>,
    );
    let mut blocks: Vec<BlockRow> = Vec::new();
    // R7: each engine table no region took, at the reading position of the
    // first line it covers. Covered lines leave their blocks, except heading
    // lines (a heading stays a heading); the table is skipped where a covered
    // line sits in a decided table, where half the covered lines sit in
    // heading or list_item regions, or where any sits in a list_item region.
    for (html, tb, oi) in table_grid.orphans() {
        // Rule A (#636): an orphan engine table it would withdraw stays text.
        let orphan_cells = table_grid.orphan_cells(oi);
        if !orphan_cells.is_empty() && !rule_a.keeps(orphan_cells) {
            let refs: Vec<&Block> = orphan_cells.iter().collect();
            crate::table_trace::emit_cells(
                page,
                &refs,
                page_blocks,
                TableOrigin::EngineGridMatch,
                page_ctx,
            );
            continue;
        }
        let pad = [
            tb[0] - ORPHAN_PAD,
            tb[1] - ORPHAN_PAD,
            tb[2] + ORPHAN_PAD,
            tb[3] + ORPHAN_PAD,
        ];
        let inside = |box_: &[f64; 4]| -> bool {
            let box_area = area(box_);
            box_area > 0.0 && overlap(box_, &pad) / box_area >= 0.5
        };
        let hits: Vec<(usize, usize)> = items
            .iter()
            .flat_map(|(ri, lines)| {
                lines
                    .iter()
                    .filter(|l| inside(&l.bbox))
                    .map(|l| (*ri, l.seq))
            })
            .collect();
        if hits
            .iter()
            .any(|(ri, _)| matches!(decided.get(ri), Some((RegionKind::Table, Some(_), _))))
        {
            continue;
        }
        let lhits: Vec<usize> = loose
            .iter()
            .filter(|l| inside(&l.bbox))
            .map(|l| l.seq)
            .collect();
        let mut kinds: HashMap<RegionKind, usize> = HashMap::new();
        for (ri, _) in &hits {
            *kinds.entry(decided[ri].0).or_default() += 1;
        }
        let n_all = hits.len() + lhits.len();
        let heading = kinds.get(&RegionKind::Heading).copied().unwrap_or(0);
        let list_item = kinds.get(&RegionKind::ListItem).copied().unwrap_or(0);
        if 2 * (heading + list_item) >= n_all || list_item > 0 {
            continue;
        }
        let keep_hits: HashSet<usize> = hits
            .iter()
            .filter(|(ri, _)| !matches!(decided[ri].0, RegionKind::Heading | RegionKind::Furniture))
            .map(|(_, seq)| *seq)
            .collect();
        if keep_hits.is_empty() && lhits.is_empty() {
            continue;
        }
        let mut at = usize::MAX;
        for seq in keep_hits.iter().chain(lhits.iter()) {
            at = at.min(*seq);
        }
        let gone: HashSet<usize> = keep_hits.into_iter().chain(lhits).collect();
        for lines in items.values_mut() {
            lines.retain(|l| !gone.contains(&l.seq));
        }
        items.retain(|_, lines| !lines.is_empty());
        order.retain(|ri| items.contains_key(ri));
        loose.retain(|l| !gone.contains(&l.seq));
        // The trace's line for the orphan grid, where it is written.
        let refs: Vec<&Block> = orphan_cells.iter().collect();
        crate::table_trace::emit_cells(
            page,
            &refs,
            page_blocks,
            TableOrigin::EngineGridMatch,
            page_ctx,
        );
        blocks.push((
            at,
            RegionKind::Table,
            "table".to_string(),
            Some(html),
            vec![],
            vec![],
        ));
    }

    for &ri in &order {
        let lines = &items[&ri];
        let (kind, grid, engine_grid) = decided.remove(&ri).unwrap_or((pool[ri].kind, None, false));
        let mut shown = if kind == RegionKind::Heading {
            join_lines(
                &lines
                    .iter()
                    .map(|l| (l.plain.as_str(), l.plain.as_str()))
                    .collect::<Vec<_>>(),
            )
        } else {
            join_runs(&join_lines(
                &lines
                    .iter()
                    .map(|l| (l.plain.as_str(), l.styled.as_str()))
                    .collect::<Vec<_>>(),
            ))
        };
        if kind == RegionKind::ListItem {
            shown = strip_bullet(&shown);
        }
        shown = fold_ligatures(&shown).into_owned();
        let (mut befores, mut afters) = (Vec::new(), Vec::new());
        let mut grid = grid;
        if kind == RegionKind::Table && grid.is_some() && engine_grid {
            let (before, after, held) = leftover_split(lines, grid.unwrap_or_default());
            befores = before;
            afters = after;
            grid = Some(held);
        }
        let at = lines.iter().map(|l| l.seq).min().unwrap_or(usize::MAX);
        blocks.push((at, kind, shown, grid, befores, afters));
    }
    let mut by_block: HashMap<usize, Vec<(usize, &str, &str)>> = HashMap::new();
    let mut block_order: Vec<usize> = Vec::new();
    for line in &loose {
        if !by_block.contains_key(&line.block) {
            block_order.push(line.block);
        }
        by_block
            .entry(line.block)
            .or_default()
            .push((line.seq, &line.styled, &line.plain));
    }
    for bi in block_order {
        let lines = &by_block[&bi];
        let at = lines.iter().map(|l| l.0).min().unwrap_or(usize::MAX);
        let pairs: Vec<(&str, &str)> = lines
            .iter()
            .map(|(_, styled, plain)| (*plain, *styled))
            .collect();
        let shown = fold_ligatures(&join_runs(&join_lines(&pairs))).into_owned();
        blocks.push((at, RegionKind::Paragraph, shown, None, vec![], vec![]));
    }
    blocks.sort_by_key(|b| b.0);

    let mut out: Vec<String> = Vec::new();
    for (_, kind, text, grid, befores, afters) in blocks {
        for para in befores {
            let para = collapse_ws(&para);
            if !para.is_empty() {
                out.push(para);
            }
        }
        let text = collapse_ws(&text);
        if !text.is_empty() && kind != RegionKind::Furniture {
            out.push(match kind {
                RegionKind::Heading => format!("## {text}"),
                RegionKind::ListItem => format!("- {text}"),
                RegionKind::Table => {
                    grid.unwrap_or_else(|| format!("<table><tr><td>{text}</td></tr></table>"))
                }
                RegionKind::Paragraph | RegionKind::Furniture => text,
            });
        }
        for para in afters {
            let para = collapse_ws(&para);
            if !para.is_empty() {
                out.push(para);
            }
        }
    }
    out.join("\n\n")
}

/// Python's `str.isspace`: Unicode whitespace plus U+001C-U+001F, which Rust's
/// [`char::is_whitespace`] (White_Space) does not include.
fn is_py_space(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, '\u{1c}'..='\u{1f}')
}

/// Python's `" ".join(text.split())`: collapse every whitespace run to one
/// space, dropping the ends.
fn collapse_ws(text: &str) -> String {
    text.split(is_py_space)
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Words whose own characters are Markdown or HTML syntax stay unmarked, as in
/// the engine's writer.
const SYNTAX_CHARS: [char; 8] = ['*', '_', '<', '>', '`', '[', ']', '\\'];

/// Tilde joins the syntax set above (kept separate for readability).
const SYNTAX_TILDE: char = '~';

/// Whether any character of `text` is Markdown or HTML syntax.
fn has_syntax(text: &str) -> bool {
    text.chars()
        .any(|ch| ch == SYNTAX_TILDE || SYNTAX_CHARS.contains(&ch))
}

/// One style run with the engine's markers (`**`, `*`, `***`); plain where it
/// must stay readable.
fn marked(run: &str, bold: bool, italic: bool) -> String {
    if run.trim_matches(is_py_space).is_empty() || !(bold || italic) || has_syntax(run) {
        return run.to_string();
    }
    let mark = match (bold, italic) {
        (true, true) => "***",
        (true, false) => "**",
        (false, true) => "*",
        (false, false) => "",
    };
    format!("{mark}{run}{mark}")
}

/// One line's words with consecutive bold/italic runs marked, as the engine's
/// writer marks them.
fn styled_line(piece: &[&PageWord]) -> String {
    let mut parts = Vec::new();
    let mut run: Vec<&str> = Vec::new();
    let mut setting = (false, false);
    for word in piece.iter().map(|w| Some(*w)).chain(std::iter::once(None)) {
        let key = word.map(|w| (w.bold, w.italic));
        if key != Some(setting) {
            parts.push(marked(&run.join(" "), setting.0, setting.1));
            run.clear();
            setting = key.unwrap_or((false, false));
        }
        if let Some(w) = word {
            run.push(w.text.as_str());
        }
    }
    parts
        .into_iter()
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// One engine word's per-character sup/sub/underline, for R8.
type CharTags = (bool, bool, bool);

/// Case-insensitive ASCII substring search from `from`; the byte index.
fn ci_find(text: &str, from: usize, needle: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let pat = needle.as_bytes();
    if pat.is_empty() || from + pat.len() > bytes.len() {
        return None;
    }
    (from..=bytes.len() - pat.len()).find(|&i| {
        bytes[i..i + pat.len()]
            .iter()
            .zip(pat.iter())
            .all(|(a, b)| a.to_ascii_lowercase() == *b)
    })
}

/// `re.sub(r"<table.*?</table>", "\n", text, flags=re.I | re.S)`: each table
/// element (non-greedy to its first closer) becomes one newline.
fn strip_md_tables(text: &str) -> String {
    let mut out = String::new();
    let mut i = 0usize;
    while i < text.len() {
        let Some(p) = ci_find(text, i, "<table") else {
            out.push_str(&text[i..]);
            break;
        };
        let Some(q) = ci_find(text, p, "</table>") else {
            // No closer at or after this opener: none can match later either.
            out.push_str(&text[i..]);
            break;
        };
        out.push_str(&text[i..p]);
        out.push('\n');
        i = q + "</table>".len();
    }
    out
}

/// One `<sup>`/`<sub>`/`<u>` tag at char `pos` (`st` as chars): the tag name
/// and the char index past it. Case-insensitive, no attributes, as the
/// adapter's `_STYLE_TAG`.
fn style_tag_at(chars: &[char], pos: usize) -> Option<(&'static str, bool, usize)> {
    if chars.get(pos) != Some(&'<') {
        return None;
    }
    let mut at = pos + 1;
    let closing = chars.get(at) == Some(&'/');
    if closing {
        at += 1;
    }
    for name in ["sup", "sub", "u"] {
        let end = at + name.len();
        if end < chars.len()
            && chars[at..end]
                .iter()
                .zip(name.chars())
                .all(|(a, b)| a.to_ascii_lowercase() == b)
            && chars[end] == '>'
        {
            return Some((name, closing, end + 1));
        }
    }
    None
}

/// A `<br>` break at char `pos`: past the `>` when `</?br\s*/?>` matches
/// (case-insensitive), as the adapter reads it.
fn br_end_at(chars: &[char], pos: usize) -> Option<usize> {
    if chars.get(pos) != Some(&'<') {
        return None;
    }
    let mut at = pos + 1;
    if chars.get(at) == Some(&'/') {
        at += 1;
    }
    if at + 1 >= chars.len()
        || !chars[at].eq_ignore_ascii_case(&'b')
        || !chars[at + 1].eq_ignore_ascii_case(&'r')
    {
        return None;
    }
    at += 2;
    while chars.get(at).is_some_and(|ch| is_py_space(*ch)) {
        at += 1;
    }
    if chars.get(at) == Some(&'/') {
        at += 1;
    }
    if chars.get(at) == Some(&'>') {
        Some(at + 1)
    } else {
        None
    }
}

/// The engine markdown's words outside tables, each with a per-character
/// style, read from its own markers (`***`/`**`/`*`, `<sup>`, `<sub>`, `<u>`).
///
/// A heading line's bold/italic read as unsettled: the `## ` carries the
/// weight there. Only the tags are kept per character -- bold and italic stay
/// the words' own flags downstream, as in the adapter.
fn engine_style_tokens(engine_markdown: &str) -> Vec<(String, Vec<CharTags>)> {
    let text = strip_md_tables(engine_markdown);
    let mut toks = Vec::new();
    for line in text.split('\n') {
        let stripped = line.trim_matches(is_py_space);
        if stripped.starts_with('|') {
            continue;
        }
        // A heading line's bold/italic would read as unsettled here; only the
        // tags below are kept, so the flag itself is unused (see docs).
        let _heading = stripped.starts_with('#');
        let mut st = stripped;
        // `^(#+ |- |\d+\. )`, once: the engine's own markers are ASCII.
        if let Some(rest) = st.strip_prefix("- ") {
            st = rest;
        } else if st.starts_with('#') {
            let hashes = st.chars().take_while(|ch| *ch == '#').count();
            let after: String = st.chars().skip(hashes).collect();
            if let Some(rest) = after.strip_prefix(' ') {
                st = &st[hashes + 1..];
                let _ = rest;
            }
        } else {
            let digits = st.chars().take_while(|ch| ch.is_ascii_digit()).count();
            let after: String = st.chars().skip(digits).collect();
            if digits > 0 && after.starts_with(". ") {
                st = &st[digits + 2..];
            }
        }
        let chars: Vec<char> = st.chars().collect();
        let mut tags = [false, false, false];
        let mut cur = String::new();
        let mut sty: Vec<CharTags> = Vec::new();
        let mut i = 0usize;
        let mut flush = |cur: &mut String, sty: &mut Vec<CharTags>| {
            if !cur.is_empty() {
                toks.push((std::mem::take(cur), std::mem::take(sty)));
            }
        };
        while i < chars.len() {
            if let Some((name, closing, end)) = style_tag_at(&chars, i) {
                let slot = match name {
                    "sup" => 0,
                    "sub" => 1,
                    _ => 2,
                };
                tags[slot] = !closing;
                i = end;
                continue;
            }
            if chars[i] == '<' {
                if let Some(end) = br_end_at(&chars, i) {
                    flush(&mut cur, &mut sty);
                    i = end;
                    continue;
                }
            }
            if chars[i] == '*' {
                let mut j = i;
                while j < chars.len() && chars[j] == '*' {
                    j += 1;
                }
                // Bold/italic toggles parsed as the adapter parses them; only
                // the tags below are kept (see the function docs).
                i = j;
                continue;
            }
            if is_py_space(chars[i]) {
                flush(&mut cur, &mut sty);
                i += 1;
                continue;
            }
            cur.push(chars[i]);
            sty.push((tags[0], tags[1], tags[2]));
            i += 1;
        }
        flush(&mut cur, &mut sty);
    }
    toks
}

/// Word index -> per-character tags, for every word aligned to an engine word,
/// plus the glued words (no space before them).
struct EngineStyles {
    /// Per-character tags by word index.
    tags: HashMap<usize, Vec<CharTags>>,
    /// Word indices glued to the previous word (no space).
    glue: HashSet<usize>,
}

/// `id(word)` -> per-character style, for every feature-dump word aligned to
/// an engine word: aligned by difflib on the word texts (autojunk off), same
/// text 1:1; where consecutive words concatenate to ONE engine token they
/// take that token's characters in order, and the later word is glued to the
/// earlier (no space) unless it is wholly raised or lowered.
fn engine_styles_for(words: &[PageWord], engine_markdown: &str) -> EngineStyles {
    let mut out = EngineStyles {
        tags: HashMap::new(),
        glue: HashSet::new(),
    };
    let eng = engine_style_tokens(engine_markdown);
    if eng.is_empty() || words.is_empty() {
        return out;
    }
    let a: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
    let b: Vec<&str> = eng.iter().map(|(t, _)| t.as_str()).collect();
    for opcode in SequenceMatcher::new(&a, &b).opcodes() {
        match opcode.tag {
            OpTag::Equal => {
                for k in 0..opcode.a_end - opcode.a_start {
                    if eng[opcode.b_start + k].1.len() == a[opcode.a_start + k].chars().count() {
                        out.tags
                            .insert(opcode.a_start + k, eng[opcode.b_start + k].1.clone());
                    }
                }
            }
            OpTag::Replace => {
                let (mut i, mut j) = (opcode.a_start, opcode.b_start);
                while i < opcode.a_end && j < opcode.b_end {
                    let (tok, sty) = &eng[j];
                    let tok_len = tok.chars().count();
                    let (mut k, mut acc) = (i, String::new());
                    while k < opcode.a_end
                        && acc.chars().count() < tok_len
                        && tok.starts_with(&format!("{acc}{}", a[k]))
                    {
                        acc.push_str(a[k]);
                        k += 1;
                    }
                    if acc == *tok && k > i {
                        let mut at = 0usize;
                        for (off, word) in a[i..k].iter().enumerate() {
                            let m = i + off;
                            let len = word.chars().count();
                            out.tags.insert(m, sty[at..at + len].to_vec());
                            let raised = sty[at..at + len].iter().all(|t| t.0 || t.1);
                            at += len;
                            if m > i && !raised {
                                out.glue.insert(m);
                            }
                        }
                        i = k;
                        j += 1;
                    } else if a[i].chars().count() < tok_len && tok.starts_with(a[i]) {
                        i += 1;
                    } else {
                        j += 1;
                    }
                }
            }
            OpTag::Delete | OpTag::Insert => {}
        }
    }
    out
}

/// One word with its sup/sub/u character runs wrapped in tags.
fn tagged(text: &str, styles: &[CharTags]) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut parts = Vec::new();
    let mut run = String::new();
    let mut key: Option<CharTags> = None;
    let flush = |run: &mut String, key: Option<CharTags>, parts: &mut Vec<String>| {
        if run.is_empty() {
            return;
        }
        let mut body = std::mem::take(run);
        if let Some((sup, sub, u)) = key {
            if sup {
                body = format!("<sup>{body}</sup>");
            } else if sub {
                body = format!("<sub>{body}</sub>");
            }
            if u {
                body = format!("<u>{body}</u>");
            }
        }
        parts.push(body);
    };
    // `zip(text, styles)`: the shorter side truncates, as in the adapter.
    for (index, ch) in chars.iter().enumerate().take(styles.len()) {
        let next = Some(styles[index]);
        if next != key && !run.is_empty() {
            flush(&mut run, key, &mut parts);
        }
        key = next;
        run.push(*ch);
    }
    flush(&mut run, key, &mut parts);
    parts.join("")
}

/// `styled_line` where a word the engine wrote takes the engine's sup/sub/u
/// per character. Bold and italic stay the words' own; a glued word joins the
/// run's last word with no space, across a style boundary where it must.
fn styled_line_engine(piece: &[usize], words: &[&PageWord], styles: &EngineStyles) -> String {
    fn close(run: &[String], plain_run: &[String], setting: (bool, bool), parts: &mut Vec<String>) {
        let plain = plain_run.join(" ");
        let tagged_text = run.join(" ");
        if plain.trim_matches(is_py_space).is_empty() {
            return;
        }
        if !(setting.0 || setting.1) || has_syntax(&plain) {
            parts.push(tagged_text);
        } else {
            let mark = match setting {
                (true, true) => "***",
                (true, false) => "**",
                (false, true) => "*",
                (false, false) => "",
            };
            parts.push(format!("{mark}{tagged_text}{mark}"));
        }
    }

    let mut parts: Vec<String> = Vec::new();
    let mut run: Vec<String> = Vec::new();
    let mut plain_run: Vec<String> = Vec::new();
    let mut setting = (false, false);
    let len = piece.len();
    for n in 0..=len {
        let key = if n < len {
            Some((words[n].bold, words[n].italic))
        } else {
            None
        };
        if key != Some(setting) {
            close(&run, &plain_run, setting, &mut parts);
            run.clear();
            plain_run.clear();
            setting = key.unwrap_or((false, false));
        }
        if n < len {
            let index = piece[n];
            let word = words[n];
            let tagged_text = match styles.tags.get(&index) {
                Some(st) if !has_syntax(&word.text) => tagged(&word.text, st),
                _ => word.text.clone(),
            };
            // `plain_run` mirrors `run`: both grow in every branch below.
            if styles.glue.contains(&index) && !run.is_empty() {
                if let (Some(last), Some(plain)) = (run.last_mut(), plain_run.last_mut()) {
                    last.push_str(&tagged_text);
                    plain.push_str(&word.text);
                }
            } else if styles.glue.contains(&index) && !parts.is_empty() {
                run.push(format!("\x00{tagged_text}"));
                plain_run.push(word.text.clone());
            } else {
                run.push(tagged_text);
                plain_run.push(word.text.clone());
            }
        }
    }
    // `re.sub(r" ?\x00", "", ...)`: the glue marker and one space before it.
    join_non_empty(&parts)
        .replace(" \x00", "")
        .replace('\x00', "")
}

/// `" ".join(p for p in parts if p)`.
fn join_non_empty(parts: &[String]) -> String {
    parts
        .iter()
        .filter(|p| !p.is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(" ")
}

/// One opcode tag of [`SequenceMatcher::opcodes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OpTag {
    /// `a[i1:i2]` should be replaced by `b[j1:j2]`.
    Replace,
    /// `a[i1:i2]` should be deleted.
    Delete,
    /// `b[j1:j2]` should be inserted at `a[i1:i1]`.
    Insert,
    /// `a[i1:i2] == b[j1:j2]`.
    Equal,
}

/// One opcode: how to turn `a[i1:i2]` into `b[j1:j2]`.
struct Opcode {
    /// What to do.
    tag: OpTag,
    /// Start in `a`.
    a_start: usize,
    /// End in `a`.
    a_end: usize,
    /// Start in `b`.
    b_start: usize,
    /// End in `b`.
    b_end: usize,
}

/// `difflib.SequenceMatcher(None, a, b, autojunk=False)` over word texts.
///
/// Ports CPython's matcher exactly (junk never, no popular-element filter):
/// `b2j` maps each element to its ascending index list, `find_longest_match`
/// keeps the earliest maximal block (`k > bestsize`, strict), matching blocks
/// collapse adjacency, and opcodes read off them. The R8 alignment is its
/// only caller.
struct SequenceMatcher<'a> {
    /// The first sequence (feature-dump word texts).
    a: &'a [&'a str],
    /// The second sequence (engine word texts).
    b: &'a [&'a str],
    /// Each element of `b` to its ascending index list.
    b2j: HashMap<&'a str, Vec<usize>>,
}

impl<'a> SequenceMatcher<'a> {
    /// Chain `b`: every element to every index it stands at, ascending.
    fn new(a: &'a [&'a str], b: &'a [&'a str]) -> Self {
        let mut b2j: HashMap<&str, Vec<usize>> = HashMap::new();
        for (i, elt) in b.iter().enumerate() {
            b2j.entry(elt).or_default().push(i);
        }
        Self { a, b, b2j }
    }

    /// The longest matching block in `a[alo:ahi]` and `b[blo:bhi]`, as
    /// `(i, j, size)`: of all maximal blocks the one starting earliest in
    /// `a`, and of those the one starting earliest in `b`.
    fn find_longest_match(
        &self,
        alo: usize,
        ahi: usize,
        blo: usize,
        bhi: usize,
    ) -> (usize, usize, usize) {
        let (mut besti, mut bestj, mut bestsize) = (alo, blo, 0);
        let mut j2len: HashMap<usize, usize> = HashMap::new();
        for i in alo..ahi {
            let mut newj2len: HashMap<usize, usize> = HashMap::new();
            if let Some(js) = self.b2j.get(self.a[i]) {
                for &j in js {
                    if j < blo {
                        continue;
                    }
                    if j >= bhi {
                        break;
                    }
                    let k = j2len.get(&(j.wrapping_sub(1))).copied().unwrap_or(0) + 1;
                    newj2len.insert(j, k);
                    if k > bestsize {
                        besti = i + 1 - k;
                        bestj = j + 1 - k;
                        bestsize = k;
                    }
                }
            }
            j2len = newj2len;
        }
        // Nothing is junk, so the extension below sucks up every matching
        // element on both sides (the junk-only loops of the original match
        // nothing and are omitted).
        while besti > alo && bestj > blo && self.a[besti - 1] == self.b[bestj - 1] {
            besti -= 1;
            bestj -= 1;
            bestsize += 1;
        }
        while besti + bestsize < ahi
            && bestj + bestsize < bhi
            && self.a[besti + bestsize] == self.b[bestj + bestsize]
        {
            bestsize += 1;
        }
        (besti, bestj, bestsize)
    }

    /// The matching blocks, monotonically increasing, adjacent blocks
    /// collapsed, with the `(len(a), len(b), 0)` dummy at the end.
    fn matching_blocks(&self) -> Vec<(usize, usize, usize)> {
        let (la, lb) = (self.a.len(), self.b.len());
        let mut queue = vec![(0, la, 0, lb)];
        let mut matching = Vec::new();
        while let Some((alo, ahi, blo, bhi)) = queue.pop() {
            let (i, j, k) = self.find_longest_match(alo, ahi, blo, bhi);
            if k > 0 {
                matching.push((i, j, k));
                if alo < i && blo < j {
                    queue.push((alo, i, blo, j));
                }
                if i + k < ahi && j + k < bhi {
                    queue.push((i + k, ahi, j + k, bhi));
                }
            }
        }
        matching.sort();
        let mut non_adjacent = Vec::new();
        let (mut i1, mut j1, mut k1) = (0, 0, 0);
        for (i2, j2, k2) in matching {
            if i1 + k1 == i2 && j1 + k1 == j2 {
                k1 += k2;
            } else {
                if k1 > 0 {
                    non_adjacent.push((i1, j1, k1));
                }
                (i1, j1, k1) = (i2, j2, k2);
            }
        }
        if k1 > 0 {
            non_adjacent.push((i1, j1, k1));
        }
        non_adjacent.push((la, lb, 0));
        non_adjacent
    }

    /// How to turn `a` into `b`, one opcode per step.
    fn opcodes(&self) -> Vec<Opcode> {
        let (mut i, mut j) = (0, 0);
        let mut out = Vec::new();
        for (ai, bj, size) in self.matching_blocks() {
            let tag = if i < ai && j < bj {
                Some(OpTag::Replace)
            } else if i < ai {
                Some(OpTag::Delete)
            } else if j < bj {
                Some(OpTag::Insert)
            } else {
                None
            };
            if let Some(tag) = tag {
                out.push(Opcode {
                    tag,
                    a_start: i,
                    a_end: ai,
                    b_start: j,
                    b_end: bj,
                });
            }
            (i, j) = (ai + size, bj + size);
            if size > 0 {
                out.push(Opcode {
                    tag: OpTag::Equal,
                    a_start: ai,
                    a_end: i,
                    b_start: bj,
                    b_end: j,
                });
            }
        }
        out
    }
}

/// One style run across a line break (R9): where two lines of one block carry
/// the same marker, `**a** **b**` becomes `**a b**`, as the engine writes a
/// wrapped bold sentence; likewise `</u> <u>` becomes one space.
///
/// Ports `re.sub(r"(?<=[^\s*])(\*{1,3}) \1(?=[^\s*])", " ", text)`: the seam
/// needs a non-space non-star on both sides, and the greedy star count tries
/// three first.
fn join_runs(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0usize;
    while i < chars.len() {
        let mut end = None;
        if chars[i] == '*' && i > 0 && !is_py_space(chars[i - 1]) && chars[i - 1] != '*' {
            let mut run = 0usize;
            while i + run < chars.len() && chars[i + run] == '*' {
                run += 1;
            }
            for n in (1..=run.min(3)).rev() {
                let mid = i + n;
                let second = mid + 1;
                if chars.get(mid) == Some(&' ')
                    && second + n <= chars.len()
                    && chars[second..second + n].iter().all(|ch| *ch == '*')
                    && chars
                        .get(second + n)
                        .is_some_and(|ch| !is_py_space(*ch) && *ch != '*')
                {
                    end = Some(second + n);
                    break;
                }
            }
        }
        match end {
            Some(stop) => {
                out.push(' ');
                i = stop;
            }
            None => {
                out.push(chars[i]);
                i += 1;
            }
        }
    }
    out.replace("</u> <u>", " ")
}

/// Ligature glyphs (U+FB00-FB06, Armenian U+FB13-FB17) as their letters: the
/// engine's fold, whose table is NFKC's on exactly these code points (R10).
fn fold_ligatures(text: &str) -> std::borrow::Cow<'_, str> {
    crate::ligatures::fold(text)
}

/// Whether `prev` ends in a hyphen wrap into `nxt` (R10): the engine's
/// `is_hyphen_wrap_candidate` -- at least three letters, `-`, the next line
/// opening in three ASCII lowercase -- over Python's left-strip.
fn hyphen_wrap(prev: &str, nxt: &str) -> bool {
    let trimmed: String = nxt.chars().skip_while(|ch| is_py_space(*ch)).collect();
    crate::text::is_hyphen_wrap_candidate(prev, &trimmed)
}

/// A block's lines (plain, shown) joined by a space -- or, across an
/// end-of-line hyphen wrap whose next line opens in lowercase, without the
/// hyphen (R10).
fn join_lines(lines: &[(&str, &str)]) -> String {
    let mut out = String::new();
    let mut prev_plain: Option<&str> = None;
    for (plain, shown) in lines {
        let wrap = prev_plain.is_some_and(|prev| hyphen_wrap(prev, plain))
            && out.ends_with('-')
            && shown.chars().next().is_some_and(|ch| ch.is_lowercase());
        if prev_plain.is_none() {
            out.push_str(shown);
        } else if wrap {
            out.pop();
            out.push_str(shown);
        } else {
            out.push(' ');
            out.push_str(shown);
        }
        prev_plain = Some(plain);
    }
    out
}

/// A list item's own bullet glyph (`lists.rs BULLET_CHARS`, which the `- `
/// replaces) is dropped where it stands before a space (R10).
fn strip_bullet(text: &str) -> String {
    let trimmed: String = text.chars().skip_while(|ch| is_py_space(*ch)).collect();
    let mut chars = trimmed.chars();
    match (chars.next(), chars.next()) {
        (Some(first), Some(' ')) if crate::lists::BULLET_CHARS.contains(&first) => trimmed
            [trimmed
                .char_indices()
                .nth(2)
                .map(|(i, _)| i)
                .unwrap_or(trimmed.len())..]
            .trim_start_matches(is_py_space)
            .to_string(),
        _ => text.to_string(),
    }
}

/// Whether `ch` is a regex word character, as the bench's `[\w*]` reads it:
/// alphanumeric or underscore.
fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// `INLINE_TAG.sub("", line)`: inline HTML tags gone entirely.
fn strip_inline_tags(line: &str) -> String {
    // The regex's own alternation order.
    const TAGS: [&str; 9] = ["sub", "sup", "b", "i", "em", "strong", "s", "del", "u"];
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::new();
    let mut i = 0usize;
    while i < chars.len() {
        let mut end = None;
        if chars[i] == '<' {
            let mut at = i + 1;
            if chars.get(at) == Some(&'/') {
                at += 1;
            }
            for name in TAGS {
                let stop = at + name.len();
                if stop < chars.len()
                    && chars[at..stop]
                        .iter()
                        .zip(name.chars())
                        .all(|(a, b)| a.to_ascii_lowercase() == b)
                    && chars[stop] == '>'
                {
                    end = Some(stop + 1);
                    break;
                }
            }
        }
        match end {
            Some(stop) => i = stop,
            None => {
                out.push(chars[i]);
                i += 1;
            }
        }
    }
    out
}

/// One branch of an `unmarked` delimiter pass at `i`: the content span and
/// the match end, for `delim` runs around `\S(?:.*?\S)?` content. The lazy
/// tail tries present-first, shortest first; then the bare `\S`. The tail's
/// `.` matches every line char, so the span always qualifies -- only its end
/// must be a non-space with the closer.
fn delimited_at(chars: &[char], i: usize, delim: &[char]) -> Option<(usize, usize, usize)> {
    if !chars[i..].starts_with(delim) {
        return None;
    }
    let p = i + delim.len();
    if chars.get(p).is_none_or(|ch| is_py_space(*ch)) {
        return None;
    }
    let mut at = p + 1;
    while at < chars.len() {
        if !is_py_space(chars[at]) && chars[at + 1..].starts_with(delim) {
            return Some((p, at + 1, at + 1 + delim.len()));
        }
        at += 1;
    }
    if chars[p + 1..].starts_with(delim) {
        return Some((p, p + 1, p + 1 + delim.len()));
    }
    None
}

/// `INLINE_STRONG`: one scan, both branches at each position -- the `***`
/// branch first, then `**` -- as the regex alternation tries them. Two
/// passes would let `***` consume text the `**` branch matches from an
/// earlier position.
fn strip_strong(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let triple: Vec<char> = "***".chars().collect();
    let double: Vec<char> = "**".chars().collect();
    let mut out = String::new();
    let mut i = 0usize;
    while i < chars.len() {
        let matched = delimited_at(&chars, i, &triple).or_else(|| delimited_at(&chars, i, &double));
        match matched {
            Some((from, to, stop)) => {
                out.extend(chars[from..to].iter());
                i = stop;
            }
            None => {
                out.push(chars[i]);
                i += 1;
            }
        }
    }
    out
}

/// `INLINE_STRIKE.sub(...)`: `~~` runs around `\S(?:.*?\S)?` content become
/// the content. Leftmost, non-overlapping, as `re.sub` scans.
fn strip_strike(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let wide: Vec<char> = "~~".chars().collect();
    let mut out = String::new();
    let mut i = 0usize;
    while i < chars.len() {
        match delimited_at(&chars, i, &wide) {
            Some((from, to, stop)) => {
                out.extend(chars[from..to].iter());
                i = stop;
            }
            None => {
                out.push(chars[i]);
                i += 1;
            }
        }
    }
    out
}

/// `INLINE_EM.sub(r"\1", line)`: single-star runs around
/// `\S(?:[^*\n]*?\S)?` content become the content, with a non-word non-star
/// required on both sides. The tail's span holds no star (its end may be
/// one); tried present-first, shortest first, then the bare `\S`.
fn strip_em(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let closed = |after: usize| -> bool {
        chars.get(after) == Some(&'*')
            && chars
                .get(after + 1)
                .is_none_or(|ch| !is_word_char(*ch) && *ch != '*')
    };
    let mut out = String::new();
    let mut i = 0usize;
    while i < chars.len() {
        let mut matched: Option<(usize, usize)> = None;
        if chars[i] == '*' && (i == 0 || (!is_word_char(chars[i - 1]) && chars[i - 1] != '*')) {
            let p = i + 1;
            if chars.get(p).is_some_and(|ch| !is_py_space(*ch)) {
                let mut at = p + 1;
                // A star in the span ends the growth, but still stands as a
                // tail end itself (the tail's `\S` allows it).
                while at < chars.len() && chars[p + 1..at].iter().all(|ch| *ch != '*') {
                    if !is_py_space(chars[at]) && closed(at + 1) {
                        matched = Some((p, at + 1));
                        break;
                    }
                    at += 1;
                }
                if matched.is_none() && closed(p + 1) {
                    matched = Some((p, p + 1));
                }
            }
        }
        match matched {
            Some((from, to)) => {
                out.extend(chars[from..to].iter());
                i = to + 1;
            }
            None => {
                out.push(chars[i]);
                i += 1;
            }
        }
    }
    out
}

/// A line's text without inline emphasis, for the R1 match: inline tags gone,
/// `***`/`**`/`*` runs unwrapped, `~~` unwrapped -- in that order, as the
/// bench's `unmarked` reads them.
fn unmarked(line: &str) -> String {
    let line = strip_inline_tags(line);
    let line = strip_strong(&line);
    let line = strip_em(&line);
    strip_strike(&line)
}

/// `COMMENT.sub(" ", text)`: `<!--...-->` (across lines) becomes one space.
fn strip_comments(text: &str) -> String {
    let mut out = String::new();
    let mut i = 0usize;
    while i < text.len() {
        let Some(p) = ci_find(text, i, "<!--") else {
            out.push_str(&text[i..]);
            break;
        };
        // The closer search is case-sensitive (`-->` has no letters).
        let Some(q) = text[p..].find("-->").map(|k| p + k) else {
            out.push_str(&text[i..]);
            break;
        };
        out.push_str(&text[i..p]);
        out.push(' ');
        i = q + "-->".len();
    }
    out
}

/// `IMAGE.sub(" ", text)`: a Markdown image becomes one space.
fn strip_images(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0usize;
    while i < chars.len() {
        let mut end = None;
        if chars[i] == '!' && chars.get(i + 1) == Some(&'[') {
            let mut j = i + 2;
            while j < chars.len() && chars[j] != ']' && chars[j] != '\n' {
                j += 1;
            }
            if chars.get(j) == Some(&']') && chars.get(j + 1) == Some(&'(') {
                let mut k = j + 2;
                while k < chars.len() && chars[k] != ')' && !is_py_space(chars[k]) {
                    k += 1;
                }
                // The optional ` \s+"..."`, greedy, then the closer.
                let mut title_end = None;
                let mut s = k;
                while s < chars.len() && is_py_space(chars[s]) {
                    s += 1;
                }
                if s > k && chars.get(s) == Some(&'"') {
                    // Greedy `[^"]*"`: the last quote that still closes.
                    let mut quotes = Vec::new();
                    let mut q = s + 1;
                    while q < chars.len() {
                        if chars[q] == '"' {
                            quotes.push(q);
                        }
                        q += 1;
                    }
                    for q in quotes.iter().rev() {
                        if chars.get(q + 1) == Some(&')') {
                            title_end = Some(q + 2);
                            break;
                        }
                    }
                }
                if let Some(stop) = title_end {
                    end = Some(stop);
                } else if chars.get(k) == Some(&')') {
                    end = Some(k + 1);
                }
            }
        }
        match end {
            Some(stop) => {
                out.push(' ');
                i = stop;
            }
            None => {
                out.push(chars[i]);
                i += 1;
            }
        }
    }
    out
}

/// `FENCE.sub("", text)`: a code fence's marker line becomes empty (its
/// content is kept). `^[ \t]*```[\w+-]*[ \t]*$` per line; the word class is
/// ASCII (info strings the engine writes are).
fn strip_fences(text: &str) -> String {
    text.split('\n')
        .map(|line| {
            let fenced = line
                .trim_matches([' ', '\t'])
                .strip_prefix("```")
                .is_some_and(|rest| {
                    rest.chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '+' || ch == '-')
                });
            if fenced {
                ""
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Whether the (unmarked) line is a pipe-table row: `^\s*\|.*\|\s*$`.
fn is_table_row(line: &str) -> bool {
    let trimmed = line.trim_matches(is_py_space);
    let mut chars = trimmed.chars();
    match (chars.next(), trimmed.chars().count()) {
        (Some('|'), n) if n >= 2 => trimmed.ends_with('|'),
        _ => false,
    }
}

/// Whether the (unmarked) line is a delimiter row: `^\s*\|[\s:|-]+\|\s*$`.
fn is_table_rule(line: &str) -> bool {
    let trimmed = line.trim_matches(is_py_space);
    let chars: Vec<char> = trimmed.chars().collect();
    chars.len() >= 3
        && chars[0] == '|'
        && chars[chars.len() - 1] == '|'
        && chars[1..chars.len() - 1]
            .iter()
            .all(|ch| is_py_space(*ch) || matches!(*ch, ':' | '|' | '-'))
}

/// A markdown pipe table as the words it holds, in reading order.
fn pipe_to_text(rows: &[String]) -> String {
    let mut cells = Vec::new();
    for row in rows {
        for cell in row.trim_matches(is_py_space).trim_matches('|').split('|') {
            let cell = cell.trim_matches(is_py_space);
            if !cell.is_empty() {
                cells.push(cell);
            }
        }
    }
    cells.join(" ")
}

/// A markdown pipe table as the HTML that TEDS reads: rows above the
/// delimiter as `<th>`, the rest as `<td>`, unescaped.
fn pipe_to_html(rows: &[String], header: usize) -> String {
    let mut out = String::from("<table>");
    for (index, row) in rows.iter().enumerate() {
        let cells: Vec<&str> = row
            .trim_matches(is_py_space)
            .trim_matches('|')
            .split('|')
            .map(|c| c.trim_matches(is_py_space))
            .collect();
        let tag = if index < header { "th" } else { "td" };
        out.push_str("<tr>");
        for cell in cells {
            out.push_str(&format!("<{tag}>{cell}</{tag}>"));
        }
        out.push_str("</tr>");
    }
    out.push_str("</table>");
    out
}

/// Strip tags (`<[^>]+>`: at least one char) for an HTML table's text.
fn strip_tags_plus(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '<' {
            let mut j = i + 1;
            while j < chars.len() && chars[j] != '>' {
                j += 1;
            }
            if j < chars.len() && j > i + 1 {
                out.push(' ');
                i = j + 1;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// The engine markdown's tables, as (text, html): ports the table half of the
/// bench's `blocks_of` (headings, items and paragraphs are never consumed
/// downstream). Comments and images become a space, fence markers vanish,
/// every line is unmarked, and a table is a run of pipe rows or an HTML
/// table.
fn engine_tables_of_markdown(markdown: &str) -> Vec<(String, String)> {
    let prepared = strip_fences(&strip_images(&strip_comments(markdown)));
    let mut tables = Vec::new();
    let mut rows: Vec<String> = Vec::new();
    let mut header_rows: Vec<usize> = Vec::new();
    let mut inside_html: Vec<String> = Vec::new();
    let flush_table = |rows: &mut Vec<String>,
                       header_rows: &mut Vec<usize>,
                       tables: &mut Vec<(String, String)>| {
        if !rows.is_empty() {
            let header = header_rows.first().copied().unwrap_or(0);
            tables.push((pipe_to_text(rows), pipe_to_html(rows, header)));
            rows.clear();
            header_rows.clear();
        }
    };
    for line in prepared.split('\n') {
        let line = unmarked(line);
        if !inside_html.is_empty() || line.to_lowercase().contains("<table") {
            inside_html.push(line.clone());
            if line.to_lowercase().contains("</table>") {
                let html = inside_html.join("\n");
                tables.push((collapse_ws(&strip_tags_plus(&html)), html));
                inside_html.clear();
            }
            continue;
        }
        if is_table_row(&line) {
            if is_table_rule(&line) {
                if header_rows.is_empty() {
                    header_rows.push(rows.len());
                }
            } else {
                rows.push(line);
            }
            continue;
        }
        flush_table(&mut rows, &mut header_rows, &mut tables);
    }
    flush_table(&mut rows, &mut header_rows, &mut tables);
    tables
}

/// Levenshtein, exactly: two rows over characters.
fn edit_distance(left: &str, right: &str) -> usize {
    if left == right {
        return 0;
    }
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();
    if left.is_empty() {
        return right.len();
    }
    if right.is_empty() {
        return left.len();
    }
    let (long, short) = if left.len() >= right.len() {
        (left, right)
    } else {
        (right, left)
    };
    let mut prev: Vec<usize> = (0..=short.len()).collect();
    let mut cur = vec![0usize; short.len() + 1];
    for (i, lc) in long.iter().enumerate() {
        cur[0] = i + 1;
        for (j, rc) in short.iter().enumerate() {
            cur[j + 1] = (prev[j + 1] + 1)
                .min(cur[j] + 1)
                .min(prev[j] + usize::from(lc != rc));
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[short.len()]
}

/// The edit distance read as a score: 1 minus the distance over the longer
/// text in characters.
fn text_similarity(left: &str, right: &str) -> f64 {
    if left.is_empty() && right.is_empty() {
        return 1.0;
    }
    let max_len = left.chars().count().max(right.chars().count());
    1.0 - edit_distance(left, right) as f64 / max_len as f64
}

/// `table_grid(text, bbox)` over the engine's tables on the page: position
/// filters, text ranks. Each table is given out once; the tables no region
/// took come back from [`GridMatcher::orphans`] for R7.
struct GridMatcher {
    /// The engine tables' (text, html), in markdown order.
    tables: Vec<(String, String)>,
    /// The engine tables' cell boxes, aligned with `tables` (empty where the
    /// runs do not line up with the markdown's tables, switching geometry
    /// off).
    geos: Vec<Vec<[f64; 4]>>,
    /// The engine tables' cells, aligned with `tables`: the trace's numbers
    /// for matched and orphan grids, and rule A's input (#636).
    snaps: Vec<Vec<Block>>,
    /// Tables already given out.
    used: HashSet<usize>,
    /// The table the last `table_grid` call gave out, for the trace.
    last_pick: Option<usize>,
}

impl GridMatcher {
    /// The matcher over one page's engine markdown and blocks.
    fn new(engine_markdown: &str, blocks: &[Block]) -> Self {
        let tables = engine_tables_of_markdown(engine_markdown);
        let mut geos = engine_tables_of(blocks);
        let mut snaps = engine_table_cells(blocks);
        if geos.len() != tables.len() {
            geos = Vec::new();
            snaps = Vec::new();
        } else if !snaps.is_empty() && snaps.len() != geos.len() {
            // The snapshot runs drifted from the geometry runs: no trace
            // numbers rather than the wrong table's.
            snaps = Vec::new();
        }
        Self {
            tables,
            geos,
            snaps,
            used: HashSet::new(),
            last_pick: None,
        }
    }

    /// The cells of the table the last `table_grid` call gave out, if the
    /// trace recorded them.
    fn picked_cells(&self) -> Option<&[Block]> {
        self.last_pick
            .and_then(|i| self.snaps.get(i))
            .map(Vec::as_slice)
    }

    /// The cells of one engine table, for an orphan line.
    fn orphan_cells(&self, index: usize) -> &[Block] {
        self.snaps.get(index).map(Vec::as_slice).unwrap_or(&[])
    }

    /// The HTML of the engine table most like `text` among those whose cells
    /// lie inside `bbox` -- and only where no table overlaps that way, the
    /// HTML of the table whose text is most like `text` (at least
    /// [`GRID_FLOOR`]). None when nothing matches.
    fn table_grid(&mut self, text: &str, bbox: &[f64; 4]) -> Option<String> {
        self.last_pick = None;
        if !self.geos.is_empty() {
            let mut eligible: Vec<(f64, f64, usize, usize)> = Vec::new();
            for (i, cells) in self.geos.iter().enumerate() {
                if self.used.contains(&i) || cells.is_empty() {
                    continue;
                }
                let inside = cells
                    .iter()
                    .filter(|c| area(c) > 0.0 && overlap(c, bbox) / area(c) >= POSITION_CELLS)
                    .count();
                let score = inside as f64 / cells.len() as f64;
                if score < POSITION_TABLE {
                    continue;
                }
                eligible.push((text_similarity(text, &self.tables[i].0), score, inside, i));
            }
            if !eligible.is_empty() {
                // POSITION FILTERS, TEXT RANKS: the most similar text wins,
                // position only breaking ties; the first of equals.
                let mut pick = 0usize;
                let mut best: Option<(f64, f64, usize)> = None;
                for (sim, pos, inside, i) in &eligible {
                    let key = (*sim, *pos, *inside);
                    let better = match &best {
                        None => true,
                        Some(b) => {
                            key.0 > b.0
                                || (key.0 == b.0 && (key.1 > b.1 || (key.1 == b.1 && key.2 > b.2)))
                        }
                    };
                    if better {
                        best = Some(key);
                        pick = *i;
                    }
                }
                self.used.insert(pick);
                self.last_pick = Some(pick);
                return Some(collapse_ws(&self.tables[pick].1));
            }
        }
        // Ties break to the higher index: `max` over `(similarity, i)`.
        let mut best: Option<(f64, usize)> = None;
        for (i, table) in self.tables.iter().enumerate() {
            if self.used.contains(&i) {
                continue;
            }
            let sim = text_similarity(text, &table.0);
            let better = match &best {
                None => true,
                Some((s, j)) => sim > *s || (sim == *s && i > *j),
            };
            if better {
                best = Some((sim, i));
            }
        }
        match best {
            Some((sim, i)) if sim >= GRID_FLOOR => {
                self.used.insert(i);
                self.last_pick = Some(i);
                Some(collapse_ws(&self.tables[i].1))
            }
            _ => None,
        }
    }

    /// The engine tables no region took, as (html, union bbox of their
    /// cells, table index for the trace's cells).
    fn orphans(&self) -> Vec<(String, [f64; 4], usize)> {
        let mut out = Vec::new();
        for (i, cells) in self.geos.iter().enumerate() {
            if self.used.contains(&i) || cells.is_empty() {
                continue;
            }
            let box_ = [
                cells.iter().map(|c| c[0]).fold(f64::INFINITY, f64::min),
                cells.iter().map(|c| c[1]).fold(f64::INFINITY, f64::min),
                cells.iter().map(|c| c[2]).fold(f64::NEG_INFINITY, f64::max),
                cells.iter().map(|c| c[3]).fold(f64::NEG_INFINITY, f64::max),
            ];
            out.push((collapse_ws(&self.tables[i].1), box_, i));
        }
        out
    }
}

/// The page's engine tables as their cells, run for run the way
/// [`engine_tables_of`] reads them: the trace's numbers for matched and
/// orphan grids, aligned with the markdown's tables in the same order.
/// Only built with the trace on.
fn engine_table_cells(blocks: &[Block]) -> Vec<Vec<Block>> {
    let mut tables = Vec::new();
    let mut cells: Vec<Block> = Vec::new();
    let mut last: Option<(u16, u16)> = None;
    for block in blocks {
        match block.kind {
            BlockKind::TableCell { row, column, .. } => {
                let at = (row, column);
                if last.is_none_or(|prev| at <= prev) && !cells.is_empty() {
                    tables.push(std::mem::take(&mut cells));
                }
                cells.push(block.clone());
                last = Some(at);
            }
            _ => {
                if !cells.is_empty() {
                    tables.push(std::mem::take(&mut cells));
                }
                last = None;
            }
        }
    }
    if !cells.is_empty() {
        tables.push(std::mem::take(&mut cells));
    }
    tables
}

/// The page's engine tables as their cells' boxes, from the same blocks
/// `structured_markdown --json` prints: a table is a run of adjacent
/// `tableCell` blocks in row-major order -- the writer's own `table_run`
/// rule, so the runs line up with the markdown's tables in order.
fn engine_tables_of(blocks: &[Block]) -> Vec<Vec<[f64; 4]>> {
    let mut tables = Vec::new();
    let mut cells: Vec<[f64; 4]> = Vec::new();
    let mut last: Option<(u16, u16)> = None;
    let flush = |cells: &mut Vec<[f64; 4]>, tables: &mut Vec<Vec<[f64; 4]>>| {
        if !cells.is_empty() {
            tables.push(std::mem::take(cells));
        }
    };
    for block in blocks {
        match block.kind {
            BlockKind::TableCell { row, column, .. } => {
                let at = (row, column);
                if last.is_none_or(|prev| at <= prev) {
                    flush(&mut cells, &mut tables);
                }
                cells.push(block.bbox);
                last = Some(at);
            }
            _ => {
                flush(&mut cells, &mut tables);
                last = None;
            }
        }
    }
    flush(&mut cells, &mut tables);
    tables
}

/// Strip ASCII punctuation off both ends and lowercase, as the adapter's
/// `_norm` reads a word.
fn norm_word(word: &str) -> String {
    word.trim_matches(|ch: char| ch.is_ascii_punctuation())
        .to_lowercase()
}

/// The grid's words, tags gone, for the leftover test.
fn grid_words(grid: &str) -> HashSet<String> {
    strip_tags_star(grid)
        .split(is_py_space)
        .map(norm_word)
        .filter(|w| !w.is_empty())
        .collect()
}

/// Strip tags (`<[^>]*>`: possibly empty) for the leftover vocabulary.
fn strip_tags_star(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '<' {
            let mut j = i + 1;
            while j < chars.len() && chars[j] != '>' {
                j += 1;
            }
            if j < chars.len() {
                out.push(' ');
                i = j + 1;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// A leftover line as a paragraph: a bare rule line is escaped so it stays
/// words, not structure.
fn safe_para(text: &str) -> String {
    // `(---+|\*+)`: three or more dashes, or one or more stars.
    let collapsed = collapse_ws(text);
    let dashes = collapsed.len() >= 3 && collapsed.chars().all(|ch| ch == '-');
    let stars = !collapsed.is_empty() && collapsed.chars().all(|ch| ch == '*');
    if dashes || stars {
        format!("\\{collapsed}")
    } else {
        collapsed
    }
}

/// A taken grid's leftover lines: the region's lines the grid does not hold,
/// in reading order. A line is held when every one of its words is among the
/// grid's (case- and punctuation-blind); the rest become paragraphs before
/// the table (lines above the held ones) or after it. Returns the
/// before/after paragraphs and the grid.
fn leftover_split(lines: &[PlacedLine], grid: String) -> (Vec<String>, Vec<String>, String) {
    let vocab = grid_words(&grid);
    let held = |line: &PlacedLine| -> bool {
        let words: Vec<String> = line
            .plain
            .split(is_py_space)
            .map(norm_word)
            .filter(|w| !w.is_empty())
            .collect();
        !words.is_empty() && words.iter().all(|w| vocab.contains(w))
    };
    let flags: Vec<bool> = lines.iter().map(held).collect();
    let left: Vec<&PlacedLine> = lines
        .iter()
        .zip(flags.iter())
        .filter(|(_, held)| !**held)
        .map(|(line, _)| line)
        .collect();
    if left.is_empty() {
        return (Vec::new(), Vec::new(), grid);
    }
    let kept: Vec<&PlacedLine> = lines
        .iter()
        .zip(flags.iter())
        .filter(|(_, held)| **held)
        .map(|(line, _)| line)
        .collect();
    let (before, after): (Vec<&PlacedLine>, Vec<&PlacedLine>) = if kept.is_empty() {
        (Vec::new(), left)
    } else {
        let top = kept
            .iter()
            .map(|line| line.bbox[3])
            .fold(f64::NEG_INFINITY, f64::max);
        left.into_iter().partition(|line| line.bbox[1] >= top)
    };
    (
        before.iter().map(|line| safe_para(&line.styled)).collect(),
        after.iter().map(|line| safe_para(&line.styled)).collect(),
        grid,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `difflib` opcode as a readable tuple.
    fn opcode_strings(a: &[&str], b: &[&str]) -> Vec<(String, usize, usize, usize, usize)> {
        SequenceMatcher::new(a, b)
            .opcodes()
            .iter()
            .map(|o| {
                let tag = match o.tag {
                    OpTag::Replace => "replace",
                    OpTag::Delete => "delete",
                    OpTag::Insert => "insert",
                    OpTag::Equal => "equal",
                };
                (tag.to_string(), o.a_start, o.a_end, o.b_start, o.b_end)
            })
            .collect()
    }

    #[test]
    fn sequence_matcher_ports_difflib() {
        // The docstring's cases, plus repeats, empties and nesting.
        let opcodes = |a: &[&str], b: &[&str]| opcode_strings(a, b);
        assert_eq!(
            opcodes(&["a", "b", "x", "c", "d"], &["a", "b", "c", "d"]),
            [
                ("equal".to_string(), 0, 2, 0, 2),
                ("delete".to_string(), 2, 3, 2, 2),
                ("equal".to_string(), 3, 5, 2, 4),
            ]
        );
        assert_eq!(
            opcodes(
                &["q", "a", "b", "x", "c", "d"],
                &["a", "b", "y", "c", "d", "f"]
            ),
            [
                ("delete".to_string(), 0, 1, 0, 0),
                ("equal".to_string(), 1, 3, 0, 2),
                ("replace".to_string(), 3, 4, 2, 3),
                ("equal".to_string(), 4, 6, 3, 5),
                ("insert".to_string(), 6, 6, 5, 6),
            ]
        );
        assert_eq!(
            opcodes(&["x", "x", "x", "x"], &["x", "x", "x", "x"]),
            [("equal".to_string(), 0, 4, 0, 4)]
        );
        assert_eq!(opcodes(&["a"], &[]), [("delete".to_string(), 0, 1, 0, 0)]);
        assert_eq!(
            opcodes(&[], &["a", "b"]),
            [("insert".to_string(), 0, 0, 0, 2)]
        );
        // A repeated element aligns to its earliest index: the `>` is
        // strict, so the first maximal block wins, never the last.
        assert_eq!(
            opcodes(&["c"], &["c", "c", "b"]),
            [
                ("equal".to_string(), 0, 1, 0, 1),
                ("insert".to_string(), 1, 1, 1, 3),
            ]
        );
        // Earliest maximal block wins, as `find_longest_match` promises.
        assert_eq!(
            opcodes(&["a", "a", "b", "a"], &["a", "b", "a", "a"]),
            [
                ("delete".to_string(), 0, 1, 0, 0),
                ("equal".to_string(), 1, 4, 0, 3),
                ("insert".to_string(), 4, 4, 3, 4),
            ]
        );
    }

    #[test]
    fn unmarked_unwraps_tags_strong_em_and_strike_in_order() {
        // The alternation tries `***` before `**` at each position: the `**`
        // branch may span what a `***`-first pass would have consumed.
        assert_eq!(unmarked("**a** ***a***"), "a** *a***");
        assert_eq!(unmarked("H<sub>2</sub>O and **net**"), "H2O and net");
        assert_eq!(unmarked("*days* gone"), "days gone");
        assert_eq!(unmarked("~~gone~~"), "gone");
        assert_eq!(unmarked("***x***"), "x");
        assert_eq!(unmarked("**a**b"), "ab");
        assert_eq!(unmarked("a ** b"), "a ** b");
        assert_eq!(unmarked("*a**"), "a*");
        assert_eq!(unmarked("**"), "**");
        assert_eq!(unmarked("text"), "text");
    }

    #[test]
    fn join_runs_merges_only_true_seams() {
        assert_eq!(join_runs("**a** **b**"), "**a b**");
        assert_eq!(join_runs("*a* *b*"), "*a b*");
        assert_eq!(join_runs("***a*** ***b***"), "***a b***");
        // The `</u> <u>` seam is one space; the emission collapses the rest.
        assert_eq!(join_runs("keep </u> <u> one"), "keep   one");
        // No seam without the marker on both sides, or at the string's edge.
        assert_eq!(join_runs("**a** b"), "**a** b");
        assert_eq!(join_runs("**a**"), "**a**");
        assert_eq!(join_runs("a** **b"), "a b");
        assert_eq!(join_runs("** **"), "** **");
        assert_eq!(join_runs("plain"), "plain");
    }

    #[test]
    fn join_lines_joins_only_hyphen_wraps() {
        let plain = |lines: &[(&str, &str)]| join_lines(lines);
        assert_eq!(
            plain(&[("docu-", "docu-"), ("ments", "ments")]),
            "documents"
        );
        assert_eq!(
            plain(&[("docu-", "**docu-**"), ("ments", "**ments**")]),
            "**docu-** **ments**"
        );
        // The next line must open in three ASCII lowercase.
        assert_eq!(plain(&[("docu-", "docu-"), ("Flow", "Flow")]), "docu- Flow");
        assert_eq!(plain(&[("docu-", "docu-"), ("me", "me")]), "docu- me");
        // Three letters must stand before the hyphen.
        assert_eq!(plain(&[("co-", "co-"), ("op", "op")]), "co- op");
        assert_eq!(
            plain(&[("plain", "plain"), ("words", "words")]),
            "plain words"
        );
    }

    #[test]
    fn strip_bullet_drops_only_a_spaced_bullet() {
        assert_eq!(strip_bullet("\u{2022} item"), "item");
        assert_eq!(strip_bullet("- item"), "item");
        assert_eq!(strip_bullet("  \u{00b7} spaced"), "spaced");
        assert_eq!(strip_bullet("-x"), "-x");
        assert_eq!(strip_bullet("-- x"), "-- x");
        assert_eq!(strip_bullet("plain"), "plain");
        assert_eq!(strip_bullet(""), "");
    }

    #[test]
    fn safe_para_escapes_only_bare_rule_lines() {
        assert_eq!(safe_para("---"), "\\---");
        assert_eq!(safe_para("***"), "\\***");
        assert_eq!(safe_para("  ***  "), "\\***");
        assert_eq!(safe_para("-"), "-");
        assert_eq!(safe_para("*"), "\\*");
        assert_eq!(safe_para("--- x"), "--- x");
        assert_eq!(safe_para("words"), "words");
    }

    #[test]
    fn grid_words_reads_the_tagless_vocab() {
        let words = grid_words("<table><tr><td>Hello, World!</td></tr></table>");
        assert_eq!(
            words,
            HashSet::from(["hello".to_string(), "world".to_string()])
        );
        assert!(grid_words("<>").is_empty());
        // No entity decoding: only tags go.
        assert_eq!(
            grid_words("A&amp;B"),
            HashSet::from(["a&amp;b".to_string()])
        );
    }

    #[test]
    fn text_similarity_is_one_minus_normalised_distance() {
        assert_eq!(text_similarity("same", "same"), 1.0);
        assert_eq!(text_similarity("", ""), 1.0);
        assert_eq!(text_similarity("abc", ""), 0.0);
        assert_eq!(edit_distance("kitten", "sitting"), 3);
        let sim = text_similarity("kitten", "sitting");
        assert!((sim - (1.0 - 3.0 / 7.0)).abs() < 1e-12, "got {sim}");
    }

    #[test]
    fn engine_tables_of_markdown_reads_pipe_and_html() {
        let tables = engine_tables_of_markdown(
            "# Title\n\n| Name | Songs |\n| --- | --- |\n| **Abba** | *Waterloo* |\n",
        );
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].0, "Name Songs Abba Waterloo");
        assert_eq!(
            tables[0].1,
            "<table><tr><th>Name</th><th>Songs</th></tr>\
             <tr><td>Abba</td><td>Waterloo</td></tr></table>"
        );
        let tables = engine_tables_of_markdown(
            "<!-- page 1 -->\n\n<table>\n<tr><td>H<sub>2</sub>O</td></tr>\n</table>\n",
        );
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].0, "H2O");
        assert!(tables[0].1.contains("<td>H2O</td>"), "got {}", tables[0].1);
        // A fence marker and an image are syntax, not content.
        let tables = engine_tables_of_markdown("```text\n![](a.png)\n```\n");
        assert!(tables.is_empty());
    }

    /// A word for the line tests: geometry and flags only, no PDF.
    fn word(text: &str, bbox: [f64; 4], block: usize, bold: bool, italic: bool) -> PageWord {
        PageWord {
            text: text.to_string(),
            bbox,
            block,
            span: 0,
            size: 10.0,
            bold,
            italic,
            font: None,
            color: None,
        }
    }

    #[test]
    fn lines_of_groups_splits_and_orders() {
        // Two columns, two rows: one y-grouped line per row, cut at the gap,
        // read column by column.
        let words = [
            word("left-one", [72.0, 700.0, 130.0, 710.0], 0, false, false),
            word("right-one", [360.0, 700.0, 430.0, 710.0], 0, false, false),
            word("left-two", [72.0, 686.0, 130.0, 696.0], 0, false, false),
            word("right-two", [360.0, 686.0, 430.0, 696.0], 0, false, false),
        ];
        let lines = lines_of(0, &words);
        let texts: Vec<Vec<&str>> = lines
            .iter()
            .map(|line| line.iter().map(|&w| words[w].text.as_str()).collect())
            .collect();
        assert_eq!(
            texts,
            vec![
                vec!["left-one"],
                vec!["left-two"],
                vec!["right-one"],
                vec!["right-two"],
            ]
        );
        // A raised word stays on its line: overlap against the line's span.
        let words = [
            word("body", [72.0, 700.0, 110.0, 710.0], 0, false, false),
            word("mark", [112.0, 706.0, 130.0, 712.0], 0, false, false),
        ];
        assert_eq!(lines_of(0, &words).len(), 1);
    }

    #[test]
    fn regions_of_applies_thresholds_furniture_and_short() {
        fn box_of(label: &str, score: f32) -> LayoutBox {
            LayoutBox {
                label: label.to_string(),
                score,
                bbox_px: [0.0, 0.0, 100.0, 100.0],
                render_w: 612,
                render_h: 792,
            }
        }
        // At the bar: kept. Under it: ignored -- except a weak heading
        // signal, which reads as a short region.
        let (kept, short) = regions_of(
            &[
                box_of("table", 0.75),
                box_of("table", 0.74),
                box_of("list_item", 0.10),
                box_of("list_item", 0.09),
                box_of("section_header", 0.25),
                box_of("title", 0.10),
                box_of("title", 0.04),
                box_of("text", 0.50),
            ],
            612.0,
            792.0,
            false,
        );
        assert_eq!(kept.len(), 4);
        assert_eq!(kept[0].kind, RegionKind::Table);
        assert_eq!(kept[1].kind, RegionKind::ListItem);
        assert_eq!(kept[2].kind, RegionKind::Heading);
        assert_eq!(kept[3].kind, RegionKind::Paragraph);
        assert_eq!(short.len(), 1);
        assert_eq!(short[0].kind, RegionKind::Heading);
        // Furniture reads as furniture -- unless kept, when it is a paragraph.
        let (kept, _) = regions_of(&[box_of("page_header", 0.5)], 612.0, 792.0, false);
        assert_eq!(kept[0].kind, RegionKind::Furniture);
        let (kept, _) = regions_of(&[box_of("page_footer", 0.5)], 612.0, 792.0, true);
        assert_eq!(kept[0].kind, RegionKind::Paragraph);
    }

    // -- The page tests below run through `balanced_page_markdown` on
    // -- operator-built PDFs, with injected detector boxes.

    /// One drawn run: text, Type1 font, size, and position.
    struct Piece {
        /// The text.
        text: String,
        /// The Type1 base font.
        font: String,
        /// The font size.
        size: f64,
        /// The x position.
        x: f64,
        /// The y position.
        y: f64,
    }

    /// A one-page PDF drawing `pieces` at their positions (612x792 pt).
    fn pdf_of(pieces: &[Piece]) -> Vec<u8> {
        use lopdf::content::{Content, Operation};
        use lopdf::{dictionary, Document, Object, Stream, StringFormat};

        /// WinAnsi bytes: ASCII as itself, the bullet as 0x95.
        fn encode(text: &str) -> Vec<u8> {
            let mut out = Vec::new();
            for ch in text.chars() {
                if ch == '\u{2022}' {
                    out.push(0x95);
                } else if ch.is_ascii() {
                    out.push(ch as u8);
                } else {
                    out.push(b'?');
                }
            }
            out
        }

        let mut doc = Document::with_version("1.7");
        let mut fonts: std::collections::HashMap<String, lopdf::ObjectId> =
            std::collections::HashMap::new();
        for piece in pieces {
            fonts.entry(piece.font.clone()).or_insert_with(|| {
                doc.add_object(Object::Dictionary(dictionary! {
                    "Type" => Object::Name(b"Font".to_vec()),
                    "Subtype" => Object::Name(b"Type1".to_vec()),
                    "BaseFont" => Object::Name(piece.font.as_bytes().to_vec()),
                    "Encoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
                }))
            });
        }
        let mut names: Vec<String> = fonts.keys().cloned().collect();
        names.sort();
        let mut operations = Vec::new();
        for piece in pieces {
            let index = names.iter().position(|n| n == &piece.font).unwrap_or(0);
            let name = format!("F{}", index + 1);
            operations.push(Operation::new("BT", vec![]));
            operations.push(Operation::new(
                "Tf",
                vec![Object::Name(name.as_bytes().to_vec()), piece.size.into()],
            ));
            operations.push(Operation::new(
                "Tm",
                vec![
                    1.into(),
                    0.into(),
                    0.into(),
                    1.into(),
                    piece.x.into(),
                    piece.y.into(),
                ],
            ));
            operations.push(Operation::new(
                "Tj",
                vec![Object::String(encode(&piece.text), StringFormat::Literal)],
            ));
            operations.push(Operation::new("ET", vec![]));
        }
        let content = Content { operations }.encode().expect("encode content");
        let content_id = doc.add_object(Object::Stream(Stream::new(dictionary! {}, content)));
        let mut fontdict = dictionary! {};
        for (i, name) in names.iter().enumerate() {
            fontdict.set(format!("F{}", i + 1), Object::Reference(fonts[name]));
        }
        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "MediaBox" => Object::Array(vec![0.into(), 0.into(), 612.into(), 792.into()]),
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(dictionary! {
                "Font" => Object::Dictionary(fontdict),
            }),
        }));
        let pages_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
            "Count" => Object::Integer(1),
        }));
        if let Ok(Object::Dictionary(page)) = doc.get_object_mut(page_id) {
            page.set("Parent", Object::Reference(pages_id));
        }
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).expect("save");
        bytes
    }

    /// Shorthand for a drawn run.
    fn at(text: &str, font: &str, size: f64, x: f64, y: f64) -> Piece {
        Piece {
            text: text.to_string(),
            font: font.to_string(),
            size,
            x,
            y,
        }
    }

    /// An injected detector box over a PDF-points bbox: the render is
    /// addressed 1:1 with the 612x792 page, flipped vertically.
    fn layout_box(label: &str, score: f32, bbox: [f64; 4]) -> LayoutBox {
        LayoutBox {
            label: label.to_string(),
            score,
            bbox_px: [
                bbox[0] as f32,
                (792.0 - bbox[3]) as f32,
                bbox[2] as f32,
                (792.0 - bbox[1]) as f32,
            ],
            render_w: 612,
            render_h: 792,
        }
    }

    /// The page through the public function.
    fn page_of(bytes: Vec<u8>, boxes: &[LayoutBox], opts: &BalancedOptions) -> String {
        let doc = PdfDocument::open(bytes).expect("open");
        balanced_page_markdown(&doc, 0, boxes, opts).expect("balanced")
    }

    #[test]
    fn tightest_covering_box_wins() {
        let bytes = pdf_of(&[
            at("Opening line here", "Helvetica", 10.0, 72.0, 740.0),
            at("Following line here", "Helvetica", 10.0, 72.0, 726.0),
        ]);
        // The outer box holds both lines; the inner heading box holds only
        // the first -- and wins it despite the lower score.
        let boxes = [
            layout_box("text", 0.9, [60.0, 700.0, 540.0, 760.0]),
            layout_box("section_header", 0.3, [60.0, 735.0, 540.0, 755.0]),
        ];
        assert_eq!(
            page_of(bytes, &boxes, &BalancedOptions::default()),
            "## Opening line here\n\nFollowing line here"
        );
    }

    #[test]
    fn below_threshold_boxes_are_ignored() {
        let bytes = pdf_of(&[
            at(
                "A first paragraph line with more than twelve words in it here yes",
                "Helvetica",
                10.0,
                72.0,
                740.0,
            ),
            at(
                "A second paragraph line with more than twelve words in it here too",
                "Helvetica",
                10.0,
                72.0,
                726.0,
            ),
            at(
                "\u{2022} A bullet line here with more than twelve words in it too yes",
                "Helvetica",
                10.0,
                72.0,
                700.0,
            ),
        ]);
        // A table signal under 0.75 is no table; a weak heading signal on
        // long lines is no heading either (short lines would still turn).
        // Unclaimed, the lines stay in their one block as one paragraph.
        let boxes = [
            layout_box("table", 0.74, [60.0, 690.0, 540.0, 760.0]),
            layout_box("section_header", 0.2, [60.0, 690.0, 540.0, 760.0]),
            layout_box("list_item", 0.09, [60.0, 690.0, 540.0, 710.0]),
        ];
        let opts = BalancedOptions::default();
        assert_eq!(
            page_of(bytes, &boxes, &opts),
            "A first paragraph line with more than twelve words in it here yes \
             A second paragraph line with more than twelve words in it here too\n\n\u{2022} A bullet line here with more than twelve words in it too yes"
        );
    }

    /// A stub plus three figure columns over four rows: the engine reads it
    /// as a table.
    fn table_pieces() -> Vec<Piece> {
        let rows = [
            ("Revenue", "100", "200", "300"),
            ("Costs", "10", "20", "30"),
            ("Profit", "90", "180", "270"),
            ("Margin", "9", "18", "27"),
        ];
        let mut pieces = Vec::new();
        for (i, (a, b, c, d)) in rows.iter().enumerate() {
            let y = 700.0 - i as f64 * 14.0;
            pieces.push(at(a, "Helvetica", 9.0, 72.0, y));
            pieces.push(at(b, "Helvetica", 9.0, 360.0, y));
            pieces.push(at(c, "Helvetica", 9.0, 430.0, y));
            pieces.push(at(d, "Helvetica", 9.0, 500.0, y));
        }
        pieces
    }

    /// The engine's grid for [`table_pieces`]: the engine reads the first row
    /// (three figures in four cells) as body, so since the header switches
    /// shipped default-on this table is headerless HTML, all `<td>`, collapsed
    /// to one line by the grid matcher.
    const TABLE_GRID: &str =
        "<table> <tr><td>Revenue</td><td>100</td><td>200</td><td>300</td></tr> \
         <tr><td>Costs</td><td>10</td><td>20</td><td>30</td></tr> \
         <tr><td>Profit</td><td>90</td><td>180</td><td>270</td></tr> \
         <tr><td>Margin</td><td>9</td><td>18</td><td>27</td></tr> </table>";

    #[test]
    fn table_region_takes_the_engine_grid() {
        let bytes = pdf_of(&table_pieces());
        let boxes = [layout_box("table", 0.8, [60.0, 640.0, 540.0, 720.0])];
        assert_eq!(
            page_of(bytes, &boxes, &BalancedOptions::default()),
            TABLE_GRID
        );
    }

    #[test]
    fn table_region_without_a_grid_is_paragraphs() {
        let bytes = pdf_of(&[at(
            "Only prose on this page",
            "Helvetica",
            10.0,
            72.0,
            740.0,
        )]);
        // A confident table box over prose: no engine grid matches, the
        // region entry finds fewer than two lines, and the line stays text.
        let boxes = [layout_box("table", 0.9, [60.0, 700.0, 540.0, 760.0])];
        assert_eq!(
            page_of(bytes, &boxes, &BalancedOptions::default()),
            "Only prose on this page"
        );
    }

    #[test]
    fn orphan_tables_come_from_the_engine() {
        // No boxes at all: the engine table is an orphan, written as the
        // engine's own grid.
        let bytes = pdf_of(&table_pieces());
        assert_eq!(page_of(bytes, &[], &BalancedOptions::default()), TABLE_GRID);
    }

    #[test]
    fn wordless_page_is_the_engine_page() {
        use lopdf::{dictionary, Document, Object, Stream};
        // A figure and no words: the engine's own markdown, verbatim.
        let mut doc = Document::with_version("1.7");
        let img = doc.add_object(Object::Stream(Stream::new(
            dictionary! {
                "Type" => Object::Name(b"XObject".to_vec()),
                "Subtype" => Object::Name(b"Image".to_vec()),
                "Width" => 64,
                "Height" => 64,
                "ColorSpace" => Object::Name(b"DeviceGray".to_vec()),
                "BitsPerComponent" => 8,
            },
            vec![128u8; 64 * 64],
        )));
        let content = doc.add_object(Stream::new(
            dictionary! {},
            b"q 100 0 0 100 72 600 cm /Im1 Do Q".to_vec(),
        ));
        let page = doc.add_object(dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content,
            "Resources" => dictionary! { "XObject" => dictionary! { "Im1" => img } },
        });
        let pages = doc.new_object_id();
        doc.objects.insert(
            pages,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page.into()],
                "Count" => 1,
            }),
        );
        if let Ok(Object::Dictionary(p)) = doc.get_object_mut(page) {
            p.set("Parent", Object::Reference(pages));
        }
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
        doc.trailer.set("Root", catalog);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).expect("save");
        let engine = crate::structured_batch::markdown_of_bytes(bytes.clone()).expect("engine");
        assert!(!engine.is_empty(), "the engine marks the figure");
        assert_eq!(
            page_of(bytes, &[], &BalancedOptions::default()),
            engine,
            "R2 returns the engine page verbatim"
        );
    }

    #[test]
    fn bold_and_italic_runs_carry_markers() {
        let bytes = pdf_of(&[
            at("The quick ", "Helvetica", 10.0, 72.0, 700.0),
            at("brown fox", "Helvetica-Bold", 10.0, 130.0, 700.0),
            at(" jumps", "Helvetica", 10.0, 190.0, 700.0),
            at("over the", "Helvetica", 10.0, 72.0, 686.0),
            at("lazy dog", "Helvetica-Oblique", 10.0, 125.0, 686.0),
        ]);
        assert_eq!(
            page_of(bytes, &[], &BalancedOptions::default()),
            "The quick **brown fox** jumps over the *lazy dog*"
        );
    }

    #[test]
    fn headings_stay_plain() {
        let bytes = pdf_of(&[at("Title Words Here", "Helvetica-Bold", 14.0, 72.0, 740.0)]);
        // The detector calls it a heading: `## ` carries the weight, and no
        // markers are written beside it.
        let boxes = [layout_box("title", 0.9, [60.0, 730.0, 540.0, 760.0])];
        assert_eq!(
            page_of(bytes, &boxes, &BalancedOptions::default()),
            "## Title Words Here"
        );
    }

    #[test]
    fn leftover_label_lines_stay_paragraphs() {
        let mut pieces = vec![at("Annual figures", "Helvetica", 10.0, 72.0, 724.0)];
        pieces.extend(table_pieces());
        let bytes = pdf_of(&pieces);
        // The region holds the label and the grid: the grid does not hold
        // the label, so it stands before the table.
        let boxes = [layout_box("table", 0.8, [60.0, 640.0, 540.0, 740.0])];
        assert_eq!(
            page_of(bytes, &boxes, &BalancedOptions::default()),
            format!("Annual figures\n\n{TABLE_GRID}")
        );
    }

    #[test]
    fn short_line_on_weak_signal_is_a_heading() {
        let bytes = pdf_of(&[
            at("A short line", "Helvetica", 10.0, 72.0, 740.0),
            at(
                "A much longer paragraph line with more than twelve words in it here",
                "Helvetica",
                10.0,
                72.0,
                726.0,
            ),
        ]);
        // Below the 0.25 bar but at the 0.05 one: only the short line turns.
        let boxes = [
            layout_box("section_header", 0.1, [60.0, 735.0, 540.0, 755.0]),
            layout_box("section_header", 0.1, [60.0, 715.0, 540.0, 735.0]),
        ];
        assert_eq!(
            page_of(bytes, &boxes, &BalancedOptions::default()),
            "## A short line\n\nA much longer paragraph line with more than twelve words in it here"
        );
    }

    #[test]
    fn orphan_engine_table_is_written_at_its_lines() {
        let mut pieces = vec![at("Opening words first", "Helvetica", 10.0, 72.0, 740.0)];
        pieces.extend(table_pieces());
        pieces.push(at("Closing words last", "Helvetica", 10.0, 72.0, 620.0));
        let bytes = pdf_of(&pieces);
        // No boxes: the engine table is written where its lines stood, and
        // its lines leave their paragraphs.
        assert_eq!(
            page_of(bytes, &[], &BalancedOptions::default()),
            format!("Opening words first\n\n{TABLE_GRID}\n\nClosing words last")
        );
    }

    #[test]
    fn aligned_words_take_engine_tags() {
        // The raised "2" stands apart: the engine writes it as its own token.
        let bytes = pdf_of(&[
            at("See the documents.", "Helvetica", 10.0, 72.0, 700.0),
            at("2", "Helvetica", 7.0, 168.0, 705.0),
            at("for details here.", "Helvetica", 10.0, 176.0, 700.0),
        ]);
        assert_eq!(
            page_of(bytes, &[], &BalancedOptions::default()),
            "See the documents. <sup>2</sup> for details here."
        );
        // The raised "2" touches its neighbours: one engine token, glued.
        let bytes = pdf_of(&[
            at("See documents.", "Helvetica", 10.0, 72.0, 700.0),
            at("2", "Helvetica", 7.0, 145.0, 705.0),
            at("here follows.", "Helvetica", 10.0, 150.0, 700.0),
        ]);
        // "here" glues (its chars take the token's tagless tail); the
        // raised "2" never glues, so its space stands.
        assert_eq!(
            page_of(bytes, &[], &BalancedOptions::default()),
            "See documents. <sup>2</sup>here follows."
        );
    }

    #[test]
    fn style_run_merges_across_lines() {
        let bytes = pdf_of(&[
            at("The brown fox", "Helvetica-Bold", 10.0, 72.0, 700.0),
            at("jumps high", "Helvetica-Bold", 10.0, 72.0, 686.0),
        ]);
        assert_eq!(
            page_of(bytes, &[], &BalancedOptions::default()),
            "**The brown fox jumps high**"
        );
    }

    #[test]
    fn hyphen_wrap_joins_and_bullet_drops() {
        let bytes = pdf_of(&[
            at("A docu-", "Helvetica", 10.0, 72.0, 700.0),
            at("ments word", "Helvetica", 10.0, 72.0, 686.0),
            at("\u{2022} Item one text", "Helvetica", 10.0, 72.0, 660.0),
        ]);
        let boxes = [layout_box("list_item", 0.9, [60.0, 650.0, 540.0, 670.0])];
        assert_eq!(
            page_of(bytes, &boxes, &BalancedOptions::default()),
            "A documents word\n\n- Item one text"
        );
    }

    #[test]
    fn ligatures_fold_when_the_engine_keeps_them() {
        use lopdf::{dictionary, Document, Object, Stream};
        // MacRoman 0xDE is the fi ligature; with the engine's own fold off,
        // the adapter's still folds it.
        let mut doc = Document::with_version("1.7");
        let font = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => Object::Name(b"Font".to_vec()),
            "Subtype" => Object::Name(b"Type1".to_vec()),
            "BaseFont" => Object::Name(b"Helvetica".to_vec()),
            "Encoding" => Object::Name(b"MacRomanEncoding".to_vec()),
        }));
        let content = doc.add_object(Stream::new(
            dictionary! {},
            b"BT /F1 10 Tf 72 700 Td <6FDE6365> Tj ET".to_vec(),
        ));
        let page = doc.add_object(dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        });
        let pages = doc.new_object_id();
        doc.objects.insert(
            pages,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page.into()],
                "Count" => 1,
            }),
        );
        if let Ok(Object::Dictionary(p)) = doc.get_object_mut(page) {
            p.set("Parent", Object::Reference(pages));
        }
        let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
        doc.trailer.set("Root", catalog);
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).expect("save");
        let mut doc = PdfDocument::open(bytes).expect("open");
        doc.set_keep_ligatures(true);
        assert_eq!(
            balanced_page_markdown(&doc, 0, &[], &BalancedOptions::default()).expect("balanced"),
            "ofice"
        );
    }

    #[test]
    fn furniture_drops_unless_kept() {
        let bytes = pdf_of(&[
            at("Running head words", "Helvetica", 9.0, 72.0, 760.0),
            at("Body words here", "Helvetica", 10.0, 72.0, 700.0),
        ]);
        let boxes = [layout_box("page_header", 0.9, [60.0, 750.0, 540.0, 775.0])];
        assert_eq!(
            page_of(bytes.clone(), &boxes, &BalancedOptions::default()),
            "Body words here"
        );
        assert_eq!(
            page_of(
                bytes,
                &boxes,
                &BalancedOptions::default().keep_furniture(true)
            ),
            "Running head words\n\nBody words here"
        );
    }

    #[test]
    fn page_out_of_range_is_an_error() {
        let bytes = pdf_of(&[at("words", "Helvetica", 10.0, 72.0, 700.0)]);
        let doc = PdfDocument::open(bytes).expect("open");
        let err = balanced_page_markdown(&doc, 3, &[], &BalancedOptions::default())
            .expect_err("page 3 of one");
        assert!(matches!(
            err,
            crate::EngineError::PageOutOfRange { index: 3, count: 1 }
        ));
    }

    #[test]
    fn balanced_markdown_needs_the_model() {
        let bytes = pdf_of(&[at("words", "Helvetica", 10.0, 72.0, 700.0)]);
        let doc = PdfDocument::open(bytes).expect("open");
        let mut dir = std::env::temp_dir();
        dir.push("pdfluent-balanced-no-model");
        let config = RuntimeConfig::new().with_model_dir(dir);
        let err =
            balanced_markdown(&doc, &config, &BalancedOptions::default()).expect_err("no model");
        assert!(
            matches!(err, crate::EngineError::Io(_)),
            "a model failure rides Io: {err:?}"
        );
    }
}
