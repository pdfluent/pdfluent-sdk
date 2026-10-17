//! The learned table pass: where a table is, and what its grid is (#600).
//!
//! A THIRD ACTION. [`super`] says a model may do two things -- propose a
//! boundary, or propose a class -- and a table model does neither. It proposes
//! a GRID: for a run of blocks, which cell each word belongs to and where that
//! cell sits in rows and columns. That is a genuine extension of the contract
//! and it is written down here rather than smuggled in as a very elaborate
//! reclassification, because the blocks that come out are not the blocks that
//! went in.
//!
//! TWO MODELS IN SERIES, and they answer different questions.
//!
//! [`models::table_region`] answers, per block, "is this inside a table". Runs
//! of blocks it says yes to become the regions. This is the half that matters:
//! the structure model reaches TEDS 0.7956 on the annotation's own table box
//! and 0.5975 on the region our heuristics find, and ALL of that difference is
//! the region -- the engine's features and candidates reproduce the Python
//! pipeline to 0.7956 against 0.7960. Per TABLE on DocLayNet, one in six is
//! cleanly ours today, three in five partly, one in four not at all.
//!
//! [`models::table_structure`] answers, per PAIR of words in a region, whether
//! they share a cell, a row, a column or nothing, and [`super::grid`] closes
//! those answers transitively into a grid.
//!
//! WHAT IT IS WORTH, over 1,152 PubTables tables, with every table the region
//! never reaches scoring the zero it earns, and the region model trained on
//! DocLayNet and never shown PubTables:
//!
//! ```text
//! the annotation's own table box   0.7949   never reached     8
//! our table cells (before this)    0.6093   never reached   171
//! these two models                 0.6407   never reached    27
//! ```
//!
//! The complete-miss class falls sixfold, which is the part the structure
//! model could do nothing about: it only ever refines a region it is handed.
//!
//! EACH BEHIND ITS OWN SWITCH. The region model is worth more than the
//! structure model and they fail differently, so they are two switches and not
//! one: a round that disagrees with one of them can turn that one off.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use super::features::{page_features, PageWord, SpanWord};
use super::{judge, lattice, lines, models, pairs, region, separations};
use crate::blocks::{TatrOptions, TatrTestCell, TatrTestHook};
use crate::table_trace::{ForcedReason, LearnedTrace, TableOrigin};
use crate::text::{Ruling as TextRuling, TextSpan};
use crate::{Block, BlockKind, PdfDocument};

/// Whether a later pass cut a table the judge chose: title rows off the top,
/// a key-value section split through the grid, both, or neither.
#[doc(hidden)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableSplit {
    /// The table stands as the judge chose it.
    Intact,
    /// Title rows were cut off its top (`verifier::without_title_rows`).
    TitleRowsRemoved,
    /// A section row split its grid (`key_value_sections_split`).
    KeyValueSplit,
    /// Both cuts: title rows off and a section split through.
    TitleAndKeyValue,
}

/// Which is-table model decided a final table: the one whose probability
/// `p_is_table` carries and whose threshold it is read against.
#[doc(hidden)]
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsTableModel {
    /// v1 (`table_is`), which reads builder tables only; decides with v2 off.
    V1,
    /// v2 (`table_is_v2`), the final-table judge; decides with v2 on.
    V2,
}

impl IsTableModel {
    /// The decision threshold of the model that produced the number, so a
    /// router compares against the threshold of the model that decided and
    /// never mixes the two scales.
    pub fn threshold(&self) -> f64 {
        match self {
            IsTableModel::V1 => models::table_is::THRESHOLD,
            IsTableModel::V2 => TABLE_IS_V2_THRESHOLD,
        }
    }
}

/// How sure the table pass is of one table it emits: the numbers that chose
/// it, from the decision itself and never recomputed beside it.
#[doc(hidden)]
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub struct TableSignal {
    /// The judge probability of the chosen candidate (`judge::probability`).
    pub p_judge: f64,
    /// Its verifier score (`verifier::score`, a margin and not a probability).
    pub verifier_score: f64,
    /// Its score minus the best scored candidate it beat that shares a line
    /// with it; `None` when no candidate competed.
    pub verifier_margin: Option<f64>,
    /// The is-table probability of the table: v2's (`table_is_v2_probability`,
    /// the same number the refusal used) where v2 decided, v1's
    /// (`table_is_probability` of the builder table it came from) where v1
    /// did. Its scale follows the model that decided -- read
    /// `p_is_table_model` and compare against that model's [`IsTableModel::threshold`];
    /// `None` where neither model spoke (no builder table behind it with v2
    /// off, or no final-table verdict matched with v2 on).
    pub p_is_table: Option<f64>,
    /// Which model produced `p_is_table`; `None` where it is `None`.
    pub p_is_table_model: Option<IsTableModel>,
    /// Whether a later pass cut it.
    pub split: TableSplit,
    /// The box round the table's blocks, page coordinates, the same space as
    /// [`Block::bbox`]: the union of `block_range` in the page output.
    pub bbox: [f64; 4],
    /// The table's blocks in the page output: `page[block_range]` holds its
    /// cells in reading order, a section row between two key-value blocks
    /// aside, which belongs to neither table.
    pub block_range: std::ops::Range<usize>,
}

/// How sure the table pass is of one page: one entry per table it emits, in
/// reading order, and the doubt around them.
#[doc(hidden)]
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub struct PageTableSignals {
    /// One per table the learned pass emits, in reading order.
    pub tables: Vec<TableSignal>,
    /// The highest judge probability among candidates not chosen and sharing
    /// no line with a chosen one; `None` with no such candidate.
    pub p_best_rejected: Option<f64>,
    /// How many candidates the judge saw on the page.
    pub n_candidates: usize,
    /// The `p_is_table` of each builder table the is-table model refused.
    pub refused: Vec<f64>,
    /// Whether the page has rulings the table pass read.
    pub drawn: bool,
}

impl PageTableSignals {
    /// One JSON object, writing `None` as an absent field and never as 0.
    #[doc(hidden)]
    pub fn to_json(&self) -> String {
        fn number(value: f64) -> String {
            if value.is_finite() {
                format!("{value}")
            } else {
                "null".to_string()
            }
        }
        fn split_name(split: TableSplit) -> &'static str {
            match split {
                TableSplit::Intact => "intact",
                TableSplit::TitleRowsRemoved => "title_rows_removed",
                TableSplit::KeyValueSplit => "key_value_split",
                TableSplit::TitleAndKeyValue => "title_and_key_value",
            }
        }
        let tables: Vec<String> = self
            .tables
            .iter()
            .map(|t| {
                let mut fields = vec![
                    format!("\"p_judge\": {}", number(t.p_judge)),
                    format!("\"verifier_score\": {}", number(t.verifier_score)),
                ];
                if let Some(margin) = t.verifier_margin {
                    fields.push(format!("\"verifier_margin\": {}", number(margin)));
                }
                if let Some(p) = t.p_is_table {
                    fields.push(format!("\"p_is_table\": {}", number(p)));
                }
                if let Some(model) = t.p_is_table_model {
                    let name = match model {
                        IsTableModel::V1 => "v1",
                        IsTableModel::V2 => "v2",
                    };
                    fields.push(format!("\"p_is_table_model\": \"{name}\""));
                }
                fields.push(format!("\"split\": \"{}\"", split_name(t.split)));
                fields.push(format!(
                    "\"bbox\": [{}, {}, {}, {}]",
                    number(t.bbox[0]),
                    number(t.bbox[1]),
                    number(t.bbox[2]),
                    number(t.bbox[3])
                ));
                fields.push(format!(
                    "\"block_range\": [{}, {}]",
                    t.block_range.start, t.block_range.end
                ));
                format!("{{{}}}", fields.join(", "))
            })
            .collect();
        let mut fields = vec![format!("\"tables\": [{}]", tables.join(", "))];
        if let Some(p) = self.p_best_rejected {
            fields.push(format!("\"p_best_rejected\": {}", number(p)));
        }
        fields.push(format!("\"n_candidates\": {}", self.n_candidates));
        let refused: Vec<String> = self.refused.iter().map(|p| number(*p)).collect();
        fields.push(format!("\"refused\": [{}]", refused.join(", ")));
        fields.push(format!("\"drawn\": {}", self.drawn));
        format!("{{{}}}", fields.join(", "))
    }
}

/// The models that decided the table signals, as one JSON fragment for the
/// trace: the judge, verifier and is-table hashes the crate ships, with the
/// final-table hash beside them.
#[doc(hidden)]
pub fn model_stamp_json() -> String {
    format!(
        "\"models\": {{\"table_judge\": \"{}\", \"table_verifier\": \"{}\", \"table_is\": \"{}\", \"table_is_v2\": \"{}\"}}",
        super::models::table_judge::MODEL_SHA256,
        super::models::table_verifier::MODEL_SHA256,
        super::models::table_is::MODEL_SHA256,
        super::models::table_is_v2::MODEL_SHA256,
    )
}

/// The trace the router's calibration reads: one JSON line per page, each
/// with the model stamp, and the page's signals where the pass ran. `None`
/// writes no `signals` field, never zeros: "not asked" is not "no table".
#[doc(hidden)]
pub fn table_signals_jsonl(signals: &[Option<PageTableSignals>]) -> String {
    let stamp = model_stamp_json();
    let mut out = String::new();
    for (page, signals) in signals.iter().enumerate() {
        match signals {
            Some(signals) => out.push_str(&format!(
                "{{\"page\": {page}, {stamp}, \"signals\": {}}}\n",
                signals.to_json()
            )),
            None => out.push_str(&format!("{{\"page\": {page}, {stamp}}}\n")),
        }
    }
    out
}

/// The probability at or above which a block is inside a table.
///
/// Swept on the validation documents, never on the rig: 0.4 is where per-block
/// F1 peaks (0.924, against 0.786 for `kind == table_cell`) and it is the
/// recall-leaning side of the peak on purpose. A block wrongly left OUT of a
/// region cannot be recovered by anything downstream; a block wrongly let in
/// meets the structure model, which can still put it in a cell of its own.
const REGION_THRESHOLD: f64 = models::table_region::THRESHOLD;

/// How large a vertical gap, in line heights, breaks a run of in-table blocks
/// into two regions.
///
/// THREE, AND KNOWN TO BE A TIE-BREAK AND NOT AN ANSWER. T2 measured our own
/// splits (#600): the gap inside one table and the gap between two tables
/// OVERLAP COMPLETELY -- 62 of 63 joins and 21 of 33 boundaries share a range
/// -- so the best a gap alone can do is 92 per cent. It runs after the model
/// has spoken, on blocks the model already called table, which is a far easier
/// population than the one those 92 per cent were measured on.
const GAP_LINES: f64 = 3.0;

/// Runs of consecutive blocks the region model calls part of a table.
///
/// A REGION IS A RUN, NOT A CLUSTER, because the blocks arrive in reading
/// order and a table is a consecutive stretch of them. A run breaks where the
/// model says no, or where the vertical gap to the next block is more than
/// [`GAP_LINES`] line heights.
fn runs(page: &[&Block], inside: &[bool], sizes: &[f64]) -> Vec<Vec<usize>> {
    let mut out: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    for (i, block) in page.iter().enumerate() {
        if !inside[i] {
            if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
            continue;
        }
        if let Some(&last) = current.last() {
            let line = sizes[last].max(1.0);
            // A RUN ALSO BREAKS WHERE READING ORDER GOES BACK UP, which the
            // gap test alone cannot see: on a two-column page the order can
            // finish a table at the foot of the left column and continue at
            // the head of the right one, and then `last.bbox[1] -
            // block.bbox[3]` is NEGATIVE and never exceeds the gap. The two
            // tables became one run, one decoder call and one set of table
            // coordinates, and `weakness_of` took the FIRST table's record --
            // so the second was rebuilt on doubt that was never about it
            // (codex, #600).
            //
            // Rows of one table descend, so a block whose top is more than a
            // line ABOVE the previous block's top is not the next row of it.
            // A line of tolerance, because cells of one row share a top only
            // approximately.
            let jumped_up = block.bbox[3] - page[last].bbox[3] > line;
            if jumped_up || page[last].bbox[1] - block.bbox[3] > GAP_LINES * line {
                out.push(std::mem::take(&mut current));
            }
        }
        current.push(i);
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// The words of a region, and for each the block it came from.
fn words_of(page: &[&Block], run: &[usize]) -> (Vec<pairs::PairWord>, Vec<usize>) {
    let mut words = Vec::new();
    let mut from = Vec::new();
    for &i in run {
        let block = page[i];
        let (table, row, column) = match block.kind {
            BlockKind::TableCell { row, column, .. } => (Some(0u32), Some(row), Some(column)),
            _ => (None, None, None),
        };
        for span in &block.spans {
            for w in super::features::words_of(span) {
                let SpanWord { text, bbox, .. } = w;
                words.push(pairs::PairWord {
                    text,
                    bbox,
                    table,
                    row,
                    column,
                });
                from.push(i);
            }
        }
    }
    (words, from)
}

/// One region's grid, or `None` when the region is too small to be a table or
/// the decoder finds nothing.
/// The grid, as one decision per gap between bands of words (#574).
///
/// THIS REPLACED A PAIR DECODER, and the reason is a measurement rather than
/// a preference. Our own words placed in the annotation's grid score TEDS
/// 0.9404 on PubTables where the grid we ship scores 0.5996: the loss is all
/// grid. The pair model bought 0.0077 of that, because it asks about two
/// words at a time and closes the answer transitively, so one wrong pair
/// walks through the whole closure. Asked once per gap instead -- a median of
/// eleven row gaps per table -- the same words build a grid that reads
/// **0.7947** on the held-out half, and a wrong line spoils one row rather
/// than the table.
///
/// A header drawn over two columns would join their stripes, so the words
/// the bridge model names are left out of them and then span the columns they
/// cover: 0.8014 -> 0.8228 on the held-out half, and 0.8306 with the header
/// rows, section rows and row groups the grid then names (#574, #632).
fn grid_of(
    words: &[pairs::PairWord],
    rules: &[separations::Ruling],
    corridors: bool,
) -> Option<separations::Grid> {
    if words.len() < 4 {
        return None;
    }
    // THE LEARNED PASS SERVES NO RULINGS, and not because they were forgotten.
    // It is handed finished blocks and `refine_page` carries no page rulings,
    // so serving a constant where training saw a real flag would be the
    // train/serve skew this lane keeps paying for. The models are trained
    // without it: on PubTables a ruling separates row boundaries at AUC 0.577
    // and is not in either model's top five by gain. A region rebuild serves
    // the rulings its box holds, so a ruled table decodes by its lines.
    let mine: Vec<separations::Word> = words
        .iter()
        .map(|w| separations::Word {
            bbox: w.bbox,
            numeric: w.text.chars().any(|c| c.is_ascii_digit()),
        })
        .collect();
    corridor_cells(&mine, rules, corridors)
}

/// The region's cells with the corridor split on or off, as the table
/// switch says: on serves the shipped guard config, off the decoder the
/// boundary models were measured with. Under the `corridor-grid` feature
/// `PDFLUENT_CORRIDOR` overrides the switch, so one grid binary serves
/// every arm; without it the switch is the whole configuration.
#[cfg(not(feature = "corridor-grid"))]
fn corridor_cells(
    words: &[separations::Word],
    rules: &[separations::Ruling],
    corridors: bool,
) -> Option<separations::Grid> {
    if corridors {
        separations::cells_of_with_corridors(words, rules, SEPARATION_FLOOR)
    } else {
        separations::cells_of(words, rules, SEPARATION_FLOOR)
    }
}

/// [`corridor_cells`], with the measurement grid's override on top.
#[cfg(feature = "corridor-grid")]
fn corridor_cells(
    words: &[separations::Word],
    rules: &[separations::Ruling],
    corridors: bool,
) -> Option<separations::Grid> {
    match separations::resolve_corridor_config(corridors) {
        Some(config) => {
            separations::cells_of_with_corridors_in(words, rules, SEPARATION_FLOOR, config)
        }
        None => separations::cells_of(words, rules, SEPARATION_FLOOR),
    }
}

/// Whether two adjacent header cells read as one spanning label: on every
/// text line they both touch, the left cell's words end a word space before
/// the right cell's begin. The collapsed ink gap lies where a stacked head
/// breaks its lines at different places: fragments on different lines abut
/// across the column gutter while every shared line keeps the gutter's own
/// white, and joining on the collapsed gap would span the columns. Where the
/// cells touch on no common line at all, there is no per-line gap to read
/// and the collapsed gap decides as before.
fn header_cells_join(a: &[usize], b: &[usize], words: &[pairs::PairWord], limit: f64) -> bool {
    struct HeadWord {
        x0: f64,
        x1: f64,
        y0: f64,
        y1: f64,
        right: bool,
    }
    // The words' text lines, oldest first: two words share a line where their
    // boxes overlap vertically by more than half of the shorter one, the
    // rule the line grouping reads everywhere.
    let mut lines: Vec<Vec<HeadWord>> = Vec::new();
    let mut all: Vec<HeadWord> = Vec::new();
    for &i in a {
        let Some(word) = words.get(i) else {
            continue;
        };
        all.push(HeadWord {
            x0: word.bbox[0],
            x1: word.bbox[2],
            y0: word.bbox[1],
            y1: word.bbox[3],
            right: false,
        });
    }
    for &i in b {
        let Some(word) = words.get(i) else {
            continue;
        };
        all.push(HeadWord {
            x0: word.bbox[0],
            x1: word.bbox[2],
            y0: word.bbox[1],
            y1: word.bbox[3],
            right: true,
        });
    }
    for word in &all {
        let mut placed = false;
        for line in lines.iter_mut() {
            let low = line
                .iter()
                .map(|w: &HeadWord| w.y0)
                .fold(f64::INFINITY, f64::min);
            let high = line
                .iter()
                .map(|w: &HeadWord| w.y1)
                .fold(f64::NEG_INFINITY, f64::max);
            let mut shorter = (word.y1 - word.y0).min(high - low);
            if shorter == 0.0 {
                shorter = 1.0;
            }
            if (word.y1.min(high) - word.y0.max(low)) / shorter > 0.5 {
                line.push(HeadWord {
                    x0: word.x0,
                    x1: word.x1,
                    y0: word.y0,
                    y1: word.y1,
                    right: word.right,
                });
                placed = true;
                break;
            }
        }
        if !placed {
            lines.push(vec![HeadWord {
                x0: word.x0,
                x1: word.x1,
                y0: word.y0,
                y1: word.y1,
                right: word.right,
            }]);
        }
    }
    let mut shared = 0usize;
    for line in &lines {
        let left = line
            .iter()
            .filter(|w| !w.right)
            .map(|w| w.x1)
            .fold(f64::NEG_INFINITY, f64::max);
        let has_left = line.iter().any(|w| !w.right);
        let has_right = line.iter().any(|w| w.right);
        if !(has_left && has_right) {
            continue;
        }
        let gap = line
            .iter()
            .filter(|w| w.right)
            .map(|w| w.x0)
            .fold(f64::INFINITY, f64::min)
            - left;
        if gap >= limit {
            return false;
        }
        shared += 1;
    }
    if shared > 0 {
        return true;
    }
    // No text line touches both cells: stacked fragments, whose per-line gap
    // there is none of. The collapsed ink gap decides as it always did.
    let right = all
        .iter()
        .filter(|w| !w.right)
        .map(|w| w.x1)
        .fold(f64::NEG_INFINITY, f64::max);
    let left = all
        .iter()
        .filter(|w| w.right)
        .map(|w| w.x0)
        .fold(f64::INFINITY, f64::min);
    left - right < limit
}

/// A header row's cells that only a word space parts are one cell (#638).
///
/// The decoder cuts every row at the body's column boundaries, and a header
/// set across the columns -- `Three months ended March 31,`, `Incorporated by
/// Reference` -- falls on them word by word. A region the verifier widens to
/// take the header in then scores worse than the one that left it out, which
/// is what ParseBench showed on a bank's and a network vendor's page. Two
/// cells of one header row whose gap is under [`WORD_SPACE`] of the grid's
/// text height become one cell spanning both.
fn spaced_header_joined(
    mut grid: separations::Grid,
    words: &[pairs::PairWord],
) -> separations::Grid {
    let heights: Vec<f64> = words.iter().map(|w| w.bbox[3] - w.bbox[1]).collect();
    let height = {
        let mut h = heights;
        h.sort_by(f64::total_cmp);
        if h.is_empty() {
            return grid;
        }
        h[h.len() / 2]
    };
    for row in 0..grid.header_rows {
        loop {
            let mut mine: Vec<usize> = (0..grid.cells.len())
                .filter(|&k| grid.cells[k].rows == [row] && !grid.cells[k].words.is_empty())
                .collect();
            mine.sort_by_key(|&k| grid.cells[k].columns[0]);
            let Some(pair) = mine.windows(2).find(|p| {
                let (a, b) = (p[0], p[1]);
                *grid.cells[b].columns.first().unwrap_or(&0)
                    == grid.cells[a].columns.last().copied().unwrap_or(0) + 1
                    && header_cells_join(
                        &grid.cells[a].words,
                        &grid.cells[b].words,
                        words,
                        WORD_SPACE * height,
                    )
            }) else {
                break;
            };
            let (a, b) = (pair[0], pair[1]);
            let taken = grid.cells.remove(b);
            let a = if b < a { a - 1 } else { a };
            let cell = &mut grid.cells[a];
            cell.words.extend(taken.words);
            cell.columns =
                (cell.columns[0]..=*taken.columns.last().unwrap_or(&cell.columns[0])).collect();
            for k in 0..4 {
                cell.bbox[k] = if k < 2 {
                    cell.bbox[k].min(taken.bbox[k])
                } else {
                    cell.bbox[k].max(taken.bbox[k])
                };
            }
        }
    }
    grid
}

/// The widest gap between two words of one header cell, in text heights: a
/// word space is about a third of one, a column gap more than one.
///
/// Shared with the lone-span runs in `separations::cells_placed`, which group
/// a header row's held slots by this same threshold (#574).
pub(crate) const WORD_SPACE: f64 = 0.6;

/// How wide, in points, a window of white must be to split a decoded figure
/// column in two ([`split_twin_figure_columns`]).
const FIGURE_COLUMN_SPLIT_GAP: f64 = 2.0;

/// How many body rows each side of the split must hold words on
/// ([`split_twin_figure_columns`]).
const FIGURE_COLUMN_SPLIT_ROWS: usize = 3;

/// Whether the word is a bare currency prefix: `$` in `$ 990`, never `$—`.
fn is_bare_currency_prefix(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| "$€£¥¢₹".contains(c))
}

/// A decoded twin-figures column split back into its two columns (#574).
///
/// DLR p61's last two columns -- Owner's Equity and Total -- decode as one:
/// the column-boundary model merges their stripes (a 3pt gap, p 0.003), so
/// every cell reads both figures (`86,090 86,090`). Where a window of white
/// runs the column's full height between two figure clusters, the column
/// becomes two: each cell's words go left or right of the window's middle,
/// a cell with words on one side only keeps that side, a cell spanning the
/// column spans both, and the columns past it shift one right.
///
/// ONLY a figure column ever splits: every body word alphabetic-free, digits
/// and three populated rows on each side, so a `$` gutter never parts from
/// its figures and a worded column is never touched. A bare `$` groups with
/// the figure or nil to its right on its row before the window is read, so
/// the gutter inside `$ 990 | $ 990` never reads as the split. Header words
/// keep the window honest but license nothing. Decoded grids only -- a
/// lattice grid reads the page's own lines -- and one split per column.
fn split_twin_figure_columns(
    mut grid: separations::Grid,
    words: &[pairs::PairWord],
) -> separations::Grid {
    let width = grid
        .cells
        .iter()
        .flat_map(|cell| cell.columns.iter().copied())
        .max()
        .map_or(0, |last| last + 1);
    let mut splits: Vec<(usize, f64)> = Vec::new();
    for column in 0..width {
        if let Some(x) = twin_column_split(&grid, words, column) {
            splits.push((column, x));
        }
    }
    // From the right, so a split never renumbers a column still to split.
    splits.sort_by_key(|&(column, _)| std::cmp::Reverse(column));
    for (column, x) in splits {
        apply_column_split(&mut grid, words, column, x);
    }
    grid
}

/// One unit a column's words group into for the split: a word, or a bare
/// currency prefix with the figure or nil to its right on its row.
struct SplitUnit {
    x0: f64,
    x1: f64,
    row: usize,
    header: bool,
    digit: bool,
    alpha: bool,
}

/// Where column `column` splits, or `None` when it is one column: the middle
/// of the widest window of white its units leave, when the window clears
/// [`FIGURE_COLUMN_SPLIT_GAP`] and each side holds digits on
/// [`FIGURE_COLUMN_SPLIT_ROWS`] body rows. `None` where any body unit holds
/// a letter, where any cell of the column spans rows, or where the column
/// holds no usable window at all.
fn twin_column_split(
    grid: &separations::Grid,
    words: &[pairs::PairWord],
    column: usize,
) -> Option<f64> {
    let mut units: Vec<SplitUnit> = Vec::new();
    for cell in &grid.cells {
        if cell.columns != [column] {
            continue;
        }
        if cell.rows.len() != 1 {
            return None;
        }
        let row = cell.rows[0];
        let header = row < grid.header_rows;
        let mut mine: Vec<usize> = cell.words.clone();
        mine.sort_by(|&a, &b| words[a].bbox[0].total_cmp(&words[b].bbox[0]));
        let mut i = 0;
        while i < mine.len() {
            let w = mine[i];
            let next = mine.get(i + 1).copied();
            let grouped = next.filter(|&n| {
                is_bare_currency_prefix(&words[w].text)
                    && !is_bare_currency_prefix(&words[n].text)
                    && super::cell_merge::is_complete_figure_line(&words[n].text)
            });
            let (x0, x1, digit, alpha) = match grouped {
                Some(n) => (
                    words[w].bbox[0].min(words[n].bbox[0]),
                    words[w].bbox[2].max(words[n].bbox[2]),
                    words[w].text.chars().any(|c| c.is_ascii_digit())
                        || words[n].text.chars().any(|c| c.is_ascii_digit()),
                    words[w].text.chars().any(|c| c.is_ascii_alphabetic())
                        || words[n].text.chars().any(|c| c.is_ascii_alphabetic()),
                ),
                None => (
                    words[w].bbox[0],
                    words[w].bbox[2],
                    words[w].text.chars().any(|c| c.is_ascii_digit()),
                    words[w].text.chars().any(|c| c.is_ascii_alphabetic()),
                ),
            };
            units.push(SplitUnit {
                x0,
                x1,
                row,
                header,
                digit,
                alpha,
            });
            i += if grouped.is_some() { 2 } else { 1 };
        }
    }
    if units.iter().any(|unit| !unit.header && unit.alpha) {
        return None;
    }
    // The windows between the chained x-groups, widest first: a unit joins
    // the group its edge touches, so no unit ever crosses a window.
    units.sort_by(|a, b| a.x0.total_cmp(&b.x0));
    let mut windows: Vec<(f64, f64)> = Vec::new();
    let mut reach = f64::NEG_INFINITY;
    for unit in &units {
        if unit.x0 > reach {
            if reach > f64::NEG_INFINITY {
                windows.push((reach, unit.x0));
            }
            reach = unit.x1;
        } else {
            reach = reach.max(unit.x1);
        }
    }
    windows.sort_by(|a, b| (b.1 - b.0).total_cmp(&(a.1 - a.0)));
    for (low, high) in windows {
        if high - low < FIGURE_COLUMN_SPLIT_GAP {
            continue;
        }
        let mut left_rows = std::collections::BTreeSet::new();
        let mut right_rows = std::collections::BTreeSet::new();
        let (mut left_digit, mut right_digit) = (false, false);
        for unit in &units {
            if unit.header {
                continue;
            }
            if unit.x1 <= low {
                left_rows.insert(unit.row);
                left_digit |= unit.digit;
            } else if unit.x0 >= high {
                right_rows.insert(unit.row);
                right_digit |= unit.digit;
            }
        }
        if left_rows.len() >= FIGURE_COLUMN_SPLIT_ROWS
            && right_rows.len() >= FIGURE_COLUMN_SPLIT_ROWS
            && left_digit
            && right_digit
        {
            return Some((low + high) / 2.0);
        }
    }
    None
}

/// Column `column` split at `x`: each of its cells' words go left or right
/// of the middle by their centres, a spanning cell reaches over the new
/// column too, and the columns past the split shift one right.
fn apply_column_split(
    grid: &mut separations::Grid,
    words: &[pairs::PairWord],
    column: usize,
    x: f64,
) {
    let mut made: Vec<super::grid::Cell> = Vec::with_capacity(grid.cells.len() + 1);
    for cell in std::mem::take(&mut grid.cells) {
        if cell.columns == [column] && cell.rows.len() == 1 {
            let mut left: Vec<usize> = Vec::new();
            let mut right: Vec<usize> = Vec::new();
            for &w in &cell.words {
                if (words[w].bbox[0] + words[w].bbox[2]) / 2.0 < x {
                    left.push(w);
                } else {
                    right.push(w);
                }
            }
            if left.is_empty() || right.is_empty() {
                let mut kept = cell;
                kept.columns = vec![if left.is_empty() { column + 1 } else { column }];
                made.push(kept);
                continue;
            }
            // Each side's x extent is its own words'; the y extent stays the
            // row's, so the bands downstream still read one line.
            let side = |mine: Vec<usize>, at: usize| {
                let ordered = in_reading_order(mine, words);
                let bbox = [
                    ordered
                        .iter()
                        .map(|&w| words[w].bbox[0])
                        .fold(f64::INFINITY, f64::min),
                    cell.bbox[1],
                    ordered
                        .iter()
                        .map(|&w| words[w].bbox[2])
                        .fold(f64::NEG_INFINITY, f64::max),
                    cell.bbox[3],
                ];
                super::grid::Cell {
                    words: ordered,
                    bbox,
                    rows: cell.rows.clone(),
                    columns: vec![at],
                }
            };
            made.push(side(left, column));
            made.push(side(right, column + 1));
        } else if cell.columns.contains(&column) {
            let mut spanned = cell;
            let mut columns = Vec::with_capacity(spanned.columns.len() + 1);
            for c in std::mem::take(&mut spanned.columns) {
                columns.push(if c > column { c + 1 } else { c });
                if c == column {
                    columns.push(column + 1);
                }
            }
            spanned.columns = columns;
            made.push(spanned);
        } else {
            let mut kept = cell;
            kept.columns = kept
                .columns
                .iter()
                .map(|&c| if c > column { c + 1 } else { c })
                .collect();
            made.push(kept);
        }
    }
    grid.cells = made;
}

/// How many rows each flank of a section row must hold for it to split a
/// decoded grid ([`key_value_sections_split`]).
const KEY_VALUE_ROWS: usize = 3;

/// The share of each flank's rows that must be key-value rows for a section
/// row to split a decoded grid ([`key_value_sections_split`]).
const KEY_VALUE_SHARE: f64 = 0.5;

/// One table or one section heading out of a split grid.
#[derive(Debug)]
enum SplitPiece {
    /// One key-value block, its rows renumbered from 0, so
    /// `markdown::table_run` reads a new table where the coordinates stop
    /// climbing.
    Table {
        cells: Vec<super::grid::Cell>,
        header_rows: usize,
    },
    /// The section row between two blocks, as its original row number; its
    /// single cell becomes a heading, not a table cell.
    Section { row: usize },
}

/// A section row between two key-value blocks splits the table (#638).
///
/// On PubTables-train's first 300 eval pages, 16 of the 22 tables read too
/// large are crystallography pages (Acta Cryst E): a `Crystal data` block of
/// `label = value` cells, a row holding only `Data collection`, then a second
/// key-value block, all read as one two-column table. In the source they are
/// separate tables, and the truth annotates them as such. On PMC2959354-p0003
/// the builder's own table is 10 cells and the verifier's region an
/// `alignment_lines` proposal of 39 lines: the merged table is the decoded
/// grid, so the split goes on it, after [`spaced_header_joined`] and before
/// the cells are made -- never on a lattice grid, never on the builder's own
/// table.
///
/// NARROW ON PURPOSE: section rows inside financial tables stay (#576). The
/// grid must have two to four columns -- the decoder cuts many key-value cells
/// into three or four (`a` | `=` | `22.3794 (8) Å`); both flanks of the section
/// row must hold at least [`KEY_VALUE_ROWS`] rows, at least [`KEY_VALUE_SHARE`]
/// of them key-value rows (a row in which some cell's text contains `" = "`,
/// starts with `"= "`, ends with `" ="`, or is `"="`); the section row itself
/// is one cell of at most four words, no `=`, no digit. A table of figures
/// with a `Geographic locations` row meets none of the key-value half and
/// comes back whole.
///
/// Returns `None` where nothing splits, so the caller keeps the grid it had.
fn key_value_sections_split(
    grid: &separations::Grid,
    words: &[pairs::PairWord],
) -> Option<Vec<SplitPiece>> {
    let width = |cell: &super::grid::Cell| cell.columns.iter().max().copied().unwrap_or(0) + 1;
    if !(2..=4).contains(&grid.cells.iter().map(width).max().unwrap_or(0)) {
        return None;
    }
    let depth = |cell: &super::grid::Cell| cell.rows.iter().max().copied().unwrap_or(0) + 1;
    let n_rows = grid.cells.iter().map(depth).max().unwrap_or(0);
    if n_rows == 0 {
        return None;
    }
    let text_of = |cell: &super::grid::Cell| join_decoded_words(words, &cell.words);
    let holding = |row: usize| {
        grid.cells
            .iter()
            .filter(|cell| cell.rows.contains(&row))
            .collect::<Vec<_>>()
    };
    let is_key_value = |row: usize| {
        holding(row).iter().any(|cell| {
            let text = text_of(cell);
            text.contains(" = ") || text.starts_with("= ") || text.ends_with(" =") || text == "="
        })
    };
    let is_section = |row: usize| {
        let mine = holding(row);
        if mine.len() != 1 {
            return false;
        }
        let text = text_of(mine[0]);
        text.split_whitespace().count() <= 4
            && !text.contains('=')
            && !text.chars().any(|c| c.is_ascii_digit())
    };
    let candidates: Vec<usize> = (0..n_rows).filter(|&r| is_section(r)).collect();
    if candidates.is_empty() {
        return None;
    }
    // The split points, greedy from the top: a section row splits when the rows
    // since the previous split and the rows up to the next section row (or the
    // end) are both key-value blocks. A section row that fails the flanks stays
    // an ordinary row of the block around it.
    let mut splits: Vec<usize> = Vec::new();
    for (k, &at) in candidates.iter().enumerate() {
        let start = splits.last().map(|last| last + 1).unwrap_or(0);
        let end = candidates.get(k + 1).copied().unwrap_or(n_rows);
        let block = |rows: &[usize]| {
            rows.len() >= KEY_VALUE_ROWS
                && rows.iter().filter(|&&r| is_key_value(r)).count() as f64 / rows.len() as f64
                    >= KEY_VALUE_SHARE
        };
        let above: Vec<usize> = (start..at).collect();
        let below: Vec<usize> = (at + 1..end).collect();
        if block(&above) && block(&below) {
            splits.push(at);
        }
    }
    if splits.is_empty() {
        return None;
    }
    // A cell a split would tear -- one straddling a section row, or holding one
    // without being its single cell -- vetoes the whole split: the rule tears
    // between rows, never through a cell.
    let torn = grid.cells.iter().any(|cell| {
        if cell.rows.iter().any(|row| splits.contains(row)) {
            cell.rows.len() != 1
        } else {
            splits
                .iter()
                .any(|&at| cell.rows.iter().any(|&r| r < at) && cell.rows.iter().any(|&r| r > at))
        }
    });
    if torn {
        return None;
    }
    let mut pieces = Vec::with_capacity(2 * splits.len() + 1);
    let mut start = 0;
    for &at in &splits {
        pieces.push(table_piece(&grid.cells, start, at, grid.header_rows));
        pieces.push(SplitPiece::Section { row: at });
        start = at + 1;
    }
    pieces.push(table_piece(&grid.cells, start, n_rows, grid.header_rows));
    Some(pieces)
}

/// One key-value block as a table of its own: the block's cells with their rows
/// renumbered from 0, and the grid's header rows shifted down to it.
fn table_piece(
    cells: &[super::grid::Cell],
    start: usize,
    end: usize,
    header_rows: usize,
) -> SplitPiece {
    let mine: Vec<super::grid::Cell> = cells
        .iter()
        .filter(|cell| cell.rows.iter().all(|&r| r >= start && r < end))
        .map(|cell| {
            let mut renumbered = cell.clone();
            renumbered.rows = cell.rows.iter().map(|&r| r - start).collect();
            renumbered
        })
        .collect();
    SplitPiece::Table {
        cells: mine,
        header_rows: header_rows.saturating_sub(start).min(end - start),
    }
}

/// Whether a table piece carries a marked header row.
///
/// WHETHER THE TABLE HAS A HEADER is the builder's evidence; WHICH ROWS it is,
/// the grid's. The builder found a header somewhere in this piece or it did
/// not, and without that nothing is marked -- the first row of a table of data
/// is not a header because it is first (codex, #600). With it, every row above
/// the first full row is: a two-row header whose second row the builder did not
/// mark is still a header, and TEDS counts it as one (#574, #632).
/// OR THE GRID'S OWN: a cell across two or more columns in its top row, over a
/// header of two rows or more, is a heading over the columns -- `Three months
/// ended March 31,` over `2025 | 2024` -- and not the first row of the data
/// (#638). ParseBench keys its records by the header, and an unmarked one
/// there reads the title as the column names.
fn table_has_header(
    cells: &[super::grid::Cell],
    header_rows: usize,
    from: &[usize],
    refs: &[&Block],
) -> bool {
    let spanned_top = header_rows >= 2
        && cells
            .iter()
            .any(|cell| cell.rows.first() == Some(&0) && cell.columns.len() >= 2);
    spanned_top
        || cells.iter().any(|cell| {
            cell.words.iter().any(|&w| {
                matches!(
                    refs[from[w]].kind,
                    BlockKind::TableCell { header: true, .. }
                )
            })
        })
}

/// The spans that fall inside a made cell.
///
/// A WORD OCCURRENCE LANDS IN EXACTLY ONE CELL. The decoder already places
/// each word once, and the spans follow the same rule: a span whose centre
/// lands in the cell is claimed whole, and a cell no span's centre lands in
/// takes, per word of its own, the span under the word box's centre clipped
/// to the word -- never the source blocks' whole span lists, which re-emitted
/// each line band's words in every cell of a split row (PMC5139084-p0004 read
/// 740 cell words from a 162-word region, #636). A span covering words of two
/// cells is therefore clipped once per word and each clip lands in the word's
/// own cell only; a spanning cell takes each of its words once.
fn cell_spans(
    cell: &super::grid::Cell,
    words: &[pairs::PairWord],
    from: &[usize],
    refs: &[&Block],
) -> Vec<TextSpan> {
    // A CELL'S SPANS ARE THE ONES THAT FALL IN IT, not every span of every
    // block its words came from. The decoder routinely splits one source block
    // across several cells, and taking the block's whole span list gave each of
    // those cells ALL of them: the public `spans` then disagreed with the
    // cell's own `text` and `bbox`, and a consumer reading the text layer saw a
    // neighbour's words inside this cell (codex, #600).
    //
    // A span is placed by its centre, so a span is claimed by exactly one cell
    // and never by two.
    let mut spans: Vec<TextSpan> = Vec::new();
    let mut seen: Vec<usize> = Vec::new();
    for &w in &cell.words {
        let block = from[w];
        if seen.contains(&block) {
            continue;
        }
        seen.push(block);
        for span in &refs[block].spans {
            let x = span.x + span.width / 2.0;
            let y = span.y + span.height / 2.0;
            if x >= cell.bbox[0] && x <= cell.bbox[2] && y >= cell.bbox[1] && y <= cell.bbox[3] {
                spans.push(span.clone());
            }
        }
    }
    // AND A CELL IS NEVER LEFT WITHOUT GLYPHS. If no span's centre lands
    // inside -- a span wider than the cell, or geometry the decoder moved --
    // each word of the cell takes the span under its own box's centre, clipped
    // to the word box with the span's style. A word no span covers keeps its
    // text and takes no glyphs: an empty band never splits a row, while a
    // neighbour's spans did, once per line band they touched (#636).
    if spans.is_empty() {
        for &w in &cell.words {
            let word = &words[w];
            let cx = (word.bbox[0] + word.bbox[2]) / 2.0;
            let cy = (word.bbox[1] + word.bbox[3]) / 2.0;
            let cover = seen
                .iter()
                .flat_map(|&block| refs[block].spans.iter())
                .find(|span| {
                    cx >= span.x
                        && cx <= span.x + span.width
                        && cy >= span.y
                        && cy <= span.y + span.height
                });
            let Some(cover) = cover else {
                continue;
            };
            let mut clipped = cover.clone();
            clipped.text.clone_from(&word.text);
            clipped.x = word.bbox[0];
            clipped.y = word.bbox[1];
            clipped.width = word.bbox[2] - word.bbox[0];
            clipped.height = word.bbox[3] - word.bbox[1];
            // Per-glyph boxes describe the unclipped span; cleared rather than
            // left pointing at glyphs this clip does not hold.
            clipped.char_bounds.clear();
            clipped.tight_char_bounds.clear();
            clipped.glyph_advances.clear();
            clipped.glyph_bounds_sources.clear();
            let dupe = spans.iter().any(|span| {
                span.text == clipped.text
                    && span.x == clipped.x
                    && span.y == clipped.y
                    && span.width == clipped.width
                    && span.height == clipped.height
            });
            if !dupe {
                spans.push(clipped);
            }
        }
    }
    spans
}

/// The source span under one cell word's centre, if any covers it.
fn cell_word_span<'a>(
    word: &pairs::PairWord,
    from: usize,
    refs: &[&'a Block],
) -> Option<(usize, &'a TextSpan)> {
    let cx = (word.bbox[0] + word.bbox[2]) / 2.0;
    let cy = (word.bbox[1] + word.bbox[3]) / 2.0;
    refs[from].spans.iter().enumerate().find(|(_, span)| {
        cx >= span.x && cx <= span.x + span.width && cy >= span.y && cy <= span.y + span.height
    })
}

/// Whether the whitespace in the span's text was drawn as glyphs: one bound
/// per character, spaces included. A drawn space is a word space the page
/// means; fewer bounds than characters means some space was inserted between
/// the glyphs -- the consensus's, the leader fold's -- and the gap overrules
/// it. A ligature also breaks the count, but its words part by their gap
/// exactly as a drawn space would.
fn span_ws_is_drawn(span: &TextSpan) -> bool {
    span.text.chars().count() == span.char_bounds.len()
}

/// The type size of one cell word: the size of its source span, the block's
/// first span where none covers it, ten points where the block carries no
/// spans at all.
fn cell_word_size(word: &pairs::PairWord, from: usize, refs: &[&Block]) -> f64 {
    cell_word_span(word, from, refs)
        .map(|(_, span)| span.font_size)
        .or_else(|| refs[from].spans.first().map(|span| span.font_size))
        .unwrap_or(10.0)
}

/// The words of one cell as its text: adjacent words join where the shared
/// rule says their gap is an intra-word split, and are spaced otherwise --
/// the same boundary the word assembly and the hidden-text rebuild draw
/// ([`crate::words_join::pieces_join`]). A plain space-join would spell a
/// word the page split across spans letter by letter (`G r o u p A`), where
/// the gap says its pieces are one word.
///
/// The judge's join, not the spans' whitespace: two words of one span were
/// split on whitespace, but that whitespace is the consensus's guess as often
/// as the page's word space -- a decimal dot the leader fold glued reads
/// `1 .`, and only the gap says its pieces are one word. What parts instead
/// is geometry: a real word space parts by its gap, a run the page overstruck
/// by its overlap, and a word on a lower line by its drift.
///
/// The gap itself is read off the words' source spans, not the words: word
/// boxes are cut to the ink, span boxes to the advances, and the rebuild
/// whose verdicts this matches reads spans. Two words of one span, and a word
/// no span covers, read it off the words: a span gap inside one span is
/// meaningless, and a folded dot stands past its label's box.
///
/// One whitespace veto survives: a space the page DREW, as a glyph with a
/// bound of its own. Span advances abut across it -- the `j` of `Fox jumps`
/// rides in a span ` j` whose box starts at the space -- so the gap reads
/// zero and only the glyph says the words part. An inserted space carries no
/// bound, and the gap overrules it.
///
/// That is the join with `cell_path_words`; without it the spans'
/// whitespace always wins and the gap is read off the source spans, exactly
/// as before.
fn cell_text(
    cell: &super::grid::Cell,
    words: &[pairs::PairWord],
    from: &[usize],
    refs: &[&Block],
    cell_path_words: bool,
) -> String {
    let mut text = String::new();
    for (i, &w) in cell.words.iter().enumerate() {
        if i > 0 {
            let prev = &words[cell.words[i - 1]];
            let curr = &words[w];
            let prev_span = cell_word_span(prev, from[cell.words[i - 1]], refs);
            let curr_span = cell_word_span(curr, from[w], refs);
            let same_span = match (&prev_span, &curr_span) {
                (Some((p, _)), Some((c, _))) => from[cell.words[i - 1]] == from[w] && p == c,
                _ => false,
            };
            let join = if cell_path_words {
                // A drawn space parts whatever the gap says; an inserted one
                // leaves the decision to the gap.
                let drawn_ws = match (&prev_span, &curr_span) {
                    (Some((_, p)), Some((_, c))) => {
                        (same_span && span_ws_is_drawn(p))
                            || (p.text.chars().last().is_some_and(char::is_whitespace)
                                && span_ws_is_drawn(p))
                            || (c.text.chars().next().is_some_and(char::is_whitespace)
                                && span_ws_is_drawn(c))
                    }
                    _ => false,
                };
                if drawn_ws {
                    false
                } else {
                    let (gap, drift) = match (&prev_span, &curr_span) {
                        (Some((_, p)), Some((_, c))) if !same_span => {
                            (c.x - (p.x + p.width), (c.y - p.y).abs())
                        }
                        _ => (
                            curr.bbox[0] - prev.bbox[2],
                            (curr.bbox[1] - prev.bbox[1]).abs(),
                        ),
                    };
                    let em = cell_word_size(prev, from[cell.words[i - 1]], refs)
                        .max(cell_word_size(curr, from[w], refs))
                        .max(f64::MIN_POSITIVE);
                    gap > -0.1 * em && crate::words_join::pieces_join(gap, drift, em)
                }
            } else {
                // Whitespace the spans already carry always wins, the way the
                // rebuild reads it: two words of one span were split on
                // whitespace to begin with, and a span edge carrying
                // whitespace is a space the page drew.
                let boundary_space = match (&prev_span, &curr_span) {
                    (Some((_, p)), Some((_, c))) => {
                        p.text.chars().last().is_some_and(char::is_whitespace)
                            || c.text.chars().next().is_some_and(char::is_whitespace)
                    }
                    _ => false,
                };
                if same_span || boundary_space {
                    false
                } else {
                    let (gap, drift) = match (&prev_span, &curr_span) {
                        (Some((_, p)), Some((_, c))) => (c.x - (p.x + p.width), (c.y - p.y).abs()),
                        _ => (
                            curr.bbox[0] - prev.bbox[2],
                            (curr.bbox[1] - prev.bbox[1]).abs(),
                        ),
                    };
                    let em = cell_word_size(prev, from[cell.words[i - 1]], refs)
                        .max(cell_word_size(curr, from[w], refs));
                    crate::words_join::pieces_join(gap, drift, em)
                }
            };
            if !join {
                text.push(' ');
            }
        }
        text.push_str(&words[w].text);
    }
    text
}

/// One decoded cell's block: `header` says whether its row is a marked header
/// row (see [`table_has_header`] for which evidence and why).
///
/// The text of decoded words: fragments rejoined, words spaced, lines parted.
///
/// The decoder's words are span fragments -- one per glyph run, down to a
/// character -- and joining them with a space reads every rebuilt cell glyph
/// by glyph (`I t e m`). Two words on one line join where the shared word
/// rule joins them and part with a space otherwise; a word on a lower line
/// always parts, by the same half-height rule that groups the reading order,
/// which is what keeps two stacked lines two words apart where their extents
/// touch. A substantial same-line overlap parts too: kerned neighbours abut,
/// and runs the page overstruck are two words, not one. `order` is the
/// words' final order; a word's own height stands in for the em, the words
/// carrying no size of their own.
fn join_decoded_words(words: &[pairs::PairWord], order: &[usize]) -> String {
    let mut text = String::new();
    let mut line_top = f64::NAN;
    let mut prev: Option<&pairs::PairWord> = None;
    for &w in order {
        let Some(word) = words.get(w) else {
            continue;
        };
        let height = (word.bbox[3] - word.bbox[1]).max(f64::MIN_POSITIVE);
        let parted = match prev {
            None => {
                line_top = word.bbox[3];
                false
            }
            Some(last) => {
                if (line_top - word.bbox[3]).abs() > 0.5 * height {
                    line_top = word.bbox[3];
                    true
                } else {
                    let em = height.max((last.bbox[3] - last.bbox[1]).max(f64::MIN_POSITIVE));
                    let gap = word.bbox[0] - last.bbox[2];
                    let drift = (word.bbox[1] - last.bbox[1]).abs();
                    gap <= -0.1 * em || !crate::words_join::pieces_join(gap, drift, em)
                }
            }
        };
        if parted {
            text.push(' ');
        }
        text.push_str(&word.text);
        prev = Some(word);
    }
    text
}

/// [`cell_block_with_cell_path_words`] without the cell-path word rules.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn cell_block(
    cell: &super::grid::Cell,
    words: &[pairs::PairWord],
    from: &[usize],
    refs: &[&Block],
    weak: Option<Weakness>,
    page_number: usize,
    header: bool,
    confidence: Option<f64>,
    by_line: bool,
) -> Block {
    cell_block_with_cell_path_words(
        cell,
        words,
        from,
        refs,
        weak,
        page_number,
        header,
        confidence,
        by_line,
        false,
    )
}

/// With `by_line` (the `table_cell_text_by_line` switch), the cell's words
/// are first put in [`in_reading_order`] -- top line first, left to right
/// within a line -- and text and spans both follow that order (#574). The
/// decoder's own within-cell order merges stacked lines whose extents touch
/// and sorts by x, and the spaced-header join concatenates slots without
/// re-sorting, so without this a header cell holding "March 31," over "2025"
/// reads "March 2025 31,". Only this cell's reading changes: the grid, its
/// box and the spans' set are untouched -- the spans only follow the new
/// order.
///
/// With `cell_path_words` the cell's words join on the cell path, as that
/// option reads them.
#[allow(clippy::too_many_arguments)]
pub(crate) fn cell_block_with_cell_path_words(
    cell: &super::grid::Cell,
    words: &[pairs::PairWord],
    from: &[usize],
    refs: &[&Block],
    weak: Option<Weakness>,
    page_number: usize,
    header: bool,
    confidence: Option<f64>,
    by_line: bool,
    cell_path_words: bool,
) -> Block {
    // THE LINES, NOT THE SLOTS. A cell of fewer than two words has exactly
    // one order; anything larger is re-read line by line, and the spans come
    // along so they still follow the text.
    let reordered;
    let cell = if by_line && cell.words.len() > 1 {
        reordered = super::grid::Cell {
            words: text_reading_order(cell.words.clone(), words),
            ..cell.clone()
        };
        &reordered
    } else {
        cell
    };
    let text = cell_text(cell, words, from, refs, cell_path_words);
    Block {
        kind: BlockKind::TableCell {
            row: *cell.rows.first().unwrap_or(&0) as u16,
            column: *cell.columns.first().unwrap_or(&0) as u16,
            row_span: cell.rows.len().max(1) as u16,
            column_span: cell.columns.len().max(1) as u16,
            // A HEADER IS EVIDENCE, NOT A ROW NUMBER.
            header,
        },
        page: page_number,
        bbox: cell.bbox,
        text,
        spans: cell_spans(cell, words, from, refs),
        index: 0,
        continues_previous: false,
        // THE JUDGE'S PROBABILITY, where the judge made this region a table:
        // `confidence` is the probability of the decision that produced the
        // kind (#624), and for a learned table that decision is the judge's.
        // `None` where no model spoke about the kind: a lattice grid, a kept
        // builder table, or a region-model table with the judge off.
        confidence,
        // THE BUILDER'S DOUBT ABOUT THE TABLE THIS REPLACED, carried forward
        // rather than dropped. It is still true of what the builder had, it is
        // the reason the gate let this model speak at all, and a consumer
        // routing a page to a vision model (#624) wants to know that a table
        // was rebuilt BECAUSE the grid was doubtful. Dropping it would tell
        // that consumer the opposite of what happened.
        grid: weak,
        // Unstamped: the rebuild's caller names the stage (forced or not).
        table_origin: TableOrigin::Other,
        trace_learned: None,
    }
}

/// What a section row between two key-value blocks becomes (#638).
///
/// A HEADING AT THE LEVEL IT ALREADY HAS, where an earlier pass named it one:
/// the table pass runs last and does not demote what the heading pass decided.
/// Otherwise a paragraph -- this pass has no document statistics to grade a
/// heading with, and an invented level would read as measured. On the Acta
/// pages the section line sits inside a paragraph of the heuristic reading, so
/// it is a paragraph there.
fn section_kind(cell: &super::grid::Cell, from: &[usize], refs: &[&Block]) -> BlockKind {
    cell.words
        .iter()
        .find_map(|&w| match refs[from[w]].kind {
            BlockKind::Heading { level } => Some(level),
            _ => None,
        })
        .map_or(BlockKind::Paragraph, |level| BlockKind::Heading { level })
}

/// A split grid's section row as its own block: the section cell's text at its
/// box, a heading where one was named and a paragraph otherwise.
fn section_block(
    cell: &super::grid::Cell,
    words: &[pairs::PairWord],
    from: &[usize],
    refs: &[&Block],
    page_number: usize,
) -> Block {
    let text = join_decoded_words(words, &cell.words);
    Block {
        kind: section_kind(cell, from, refs),
        page: page_number,
        bbox: cell.bbox,
        text,
        spans: cell_spans(cell, words, from, refs),
        index: 0,
        continues_previous: false,
        confidence: None,
        // NONE, AND NOT THE REGION'S DOUBT: `GridDoubt` is a property of the
        // table its cells carry, and the section row belongs to neither table.
        grid: None,
        table_origin: TableOrigin::Other,
        trace_learned: None,
    }
}

/// The page with its tables re-read by the two models, and for every block it
/// returns the position IN `page` of the block it came from.
///
/// A CELL COMES FROM SEVERAL BLOCKS, which is why this has to be said and
/// cannot be inferred by the caller: the blocks that come out are not the
/// blocks that went in. A re-gridded region reports the position of its FIRST
/// block for every cell it made -- the grid stands where the table started,
/// which is where reading order wants it, and `refine_with` needs one slot
/// per returned block.
///
/// PURE, like everything else in this pass: what it reads is its arguments and
/// what it changes is what it returns.
/// The page with its tables re-read, with the judge's numbers and the drawn
/// flag, and the table signals beside the blocks.
#[allow(clippy::too_many_arguments)]
pub(crate) fn refine_traced(
    page: Vec<Block>,
    size: (f64, f64),
    facts: &super::DocumentFacts,
    switches: super::Switches,
    judged: Option<&[Option<usize>]>,
    judge_trace: Option<&JudgeTrace>,
    lattices: &[super::lattice::Lattice],
    cells: &[CellHint],
    drawn: bool,
    tatr: Option<&TatrContext>,
    cell_path_words: bool,
) -> (Vec<Block>, Vec<usize>, Option<PageTableSignals>) {
    refine_judged_with_tatr(
        page,
        size,
        facts,
        switches,
        &weakness_of,
        judged,
        judge_trace,
        lattices,
        cells,
        drawn,
        tatr,
        cell_path_words,
    )
}

/// What the line models said of one block before the table pass (#574): the
/// heading decision the pass took on it while it was still a cell, for
/// [`demote_unchosen`] to keep when the cell goes back to prose.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[doc(hidden)]
pub struct CellHint {
    /// Every line of the block read as a heading.
    pub heading: bool,
    /// The mean heading probability of its lines, where the models read it.
    pub p_heading: Option<f64>,
}

/// The judge's numbers beside its regions: one computation, so the signals
/// cannot drift from the decision.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub struct JudgeTrace {
    /// One entry per chosen table, in the same order as the regions' `k`.
    pub details: Vec<super::verifier::Choice>,
    /// The highest judge probability among candidates not chosen and sharing
    /// no line with a chosen one; `None` with no such candidate.
    pub p_best_rejected: Option<f64>,
    /// How many candidates the judge saw on the page.
    pub n_candidates: usize,
    /// The judge probability of the builder proposal for each block's table,
    /// in page order; `None` where the block is no builder cell or the
    /// builder drew no proposal for it. The final-table judge names a builder
    /// table it chose no part of by this, from this same verdict.
    pub builder_p: Vec<Option<f64>>,
}

/// [`judge_regions`], with the numbers it decided by beside the regions.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub struct JudgedRegions {
    /// Which of the judge's tables each block belongs to, as [`judge_regions`].
    pub regions: Vec<Option<usize>>,
    /// The numbers behind them; `None` where the judge was not asked (no lines).
    pub trace: Option<JudgeTrace>,
}

/// Which of the judge's tables each block of the page belongs to (#574).
///
/// The judge chooses among sets of LINES; a block belongs to a chosen table
/// when at least half of its lines are in it. Run on the page as the
/// heuristics left it -- the state the judge was trained on -- before any other
/// learned decision moves a block; the caller carries the answer through its
/// own origin map to the table pass.
#[doc(hidden)]
pub fn judge_regions(
    page: &[Block],
    size: (f64, f64),
    rules: &[crate::text::HorizontalRule],
    facts: &super::DocumentFacts,
    switches: super::Switches,
) -> Vec<Option<usize>> {
    judge_regions_traced(page, size, rules, facts, switches).regions
}

/// [`judge_regions`], with the numbers it decided by: one computation, so the
/// signals cannot drift from the decision.
#[doc(hidden)]
pub fn judge_regions_traced(
    page: &[Block],
    size: (f64, f64),
    rules: &[crate::text::HorizontalRule],
    facts: &super::DocumentFacts,
    switches: super::Switches,
) -> JudgedRegions {
    let refs: Vec<&Block> = page.iter().collect();
    let features = page_features(&refs, size.0, size.1);
    let signal = super::list_signal(&refs, size, facts);
    let lines = judge::lines_of_page(&refs, &features.words, &signal);
    judge_regions_with_lines(page, size, rules, switches, &lines, &features.words)
}

/// [`judge_regions_traced`] over the page's lines: the same regions, without
/// re-measuring the page for them. The learned pass reads the judge and the
/// decisions off one measurement; this is the judge's half of it.
pub(crate) fn judge_regions_with_lines(
    page: &[Block],
    size: (f64, f64),
    rules: &[crate::text::HorizontalRule],
    switches: super::Switches,
    lines: &[judge::Line],
    words: &[PageWord],
) -> JudgedRegions {
    if lines.is_empty() {
        return JudgedRegions {
            regions: vec![None; page.len()],
            trace: None,
        };
    }
    let spans: Vec<crate::text::TextSpan> =
        page.iter().flat_map(|b| b.spans.iter().cloned()).collect();
    // THE CANDIDATES THE DUMP OFFERS, in its order: the detectors' regions,
    // refused ones included; the alignment proposer's; then every one of
    // those in pieces (`splits::pieces`, T2 #2016), which is where a table
    // that swallowed the other column's prose is offered without it.
    let mut drawn: Vec<(&'static str, Vec<usize>, bool)> =
        crate::text::table_proposals(&spans, rules)
            .into_iter()
            .map(|p| (p.detector, judge::held_by(p.hull, lines), p.accepted))
            .collect();
    let boxes: Vec<[f64; 4]> = lines.iter().map(|l| l.bbox).collect();
    drawn.extend(
        super::alignment::proposals(&boxes)
            .into_iter()
            .map(|s| ("alignment_lines", s, true)),
    );
    let sets: Vec<Vec<usize>> = drawn.iter().map(|(_, s, _)| s.clone()).collect();
    drawn.extend(
        super::splits::pieces(&sets, &boxes)
            .into_iter()
            .map(|s| ("split", s, true)),
    );
    let all = judge::proposals(lines, &drawn);
    judge_regions_from_proposals(page.len(), lines, words, &all, size, switches)
}

/// [`judge_regions_with_lines`] over the page's proposals: with no candidate
/// regions the answer is empty without running the verifier, exactly as the
/// full judging computes it for zero candidates (held by a test below).
pub(crate) fn judge_regions_from_proposals(
    n_blocks: usize,
    lines: &[judge::Line],
    words: &[PageWord],
    all: &[judge::Proposal],
    size: (f64, f64),
    switches: super::Switches,
) -> JudgedRegions {
    if all.is_empty() {
        // NOTHING CHOSEN, NOTHING REJECTED, NOTHING NAMED. The full judging
        // over zero proposals chooses no set (the verdict is empty), rejects
        // no builder table, and names no builder probability on any block --
        // so the skip returns that answer directly.
        return JudgedRegions {
            regions: vec![None; n_blocks],
            trace: Some(JudgeTrace {
                details: Vec::new(),
                p_best_rejected: None,
                n_candidates: 0,
                builder_p: vec![None; n_blocks],
            }),
        };
    }
    judge_regions_scored(n_blocks, lines, words, all, size, switches)
}

/// The full judging over a page's proposals. Shared by
/// [`judge_regions_from_proposals`] and by the test that proves the skip.
fn judge_regions_scored(
    n_blocks: usize,
    lines: &[judge::Line],
    words: &[PageWord],
    all: &[judge::Proposal],
    size: (f64, f64),
    switches: super::Switches,
) -> JudgedRegions {
    let mut traced = judge::verdict_on_proposals(size, lines, words, all);
    // The stitch takes the lines the judge cut off a table back into its set
    // -- wrapped labels, section rows, year headers -- and joins sets the
    // judge split, before any block is assigned (#574).
    traced.chosen = super::stitch::stitch_sets(
        lines,
        words,
        n_blocks,
        &traced.chosen,
        switches.table_stitch_header,
        switches.table_stitch_rows,
        switches.table_stitch_merge,
        switches.table_split_stacked,
    );
    let mut total = vec![0usize; n_blocks];
    for line in lines {
        total[line.block] += 1;
    }
    let mut out = vec![None; n_blocks];
    for (k, set) in traced.chosen.iter().enumerate() {
        let mut held = vec![0usize; n_blocks];
        for &i in set {
            held[lines[i].block] += 1;
        }
        for b in 0..n_blocks {
            if out[b].is_none() && held[b] > 0 && 2 * held[b] >= total[b] {
                out[b] = Some(k);
            }
        }
    }
    for set in &traced.rejected {
        for &i in set {
            if out[lines[i].block].is_none() {
                out[lines[i].block] = Some(REJECTED);
            }
        }
    }
    // A BLOCK TAKES ITS LINES' BUILDER PROBABILITY (they agree: one
    // table's lines are one proposal's). Blocks with no lines keep None.
    let mut builder_p = vec![None; n_blocks];
    for (i, line) in lines.iter().enumerate() {
        if builder_p[line.block].is_none() {
            builder_p[line.block] = traced.builder_p.get(i).copied().flatten();
        }
    }
    JudgedRegions {
        regions: out,
        trace: Some(JudgeTrace {
            details: traced.details,
            p_best_rejected: traced.p_best_rejected,
            n_candidates: traced.n_candidates,
            builder_p,
        }),
    }
}

/// What the judge said of one chosen region, for the final-table judge.
///
/// Built from [`JudgeTrace::details`]: `meta[k]` is what the judge said of
/// chosen region `k`, from the same traced verdict the signals use.
#[derive(Debug, Clone, Copy)]
#[doc(hidden)]
pub struct JudgeRegionMeta {
    /// The judge probability of the proposal the region came from.
    pub judge_p: f64,
    /// The proposer kind that drew it first.
    pub kind: &'static str,
    /// Whether the region came from the builder.
    pub from_builder: bool,
}

impl JudgeRegionMeta {
    /// The provenance of every chosen region, in the regions' `k` order.
    #[doc(hidden)]
    pub fn of_trace(trace: &JudgeTrace) -> Vec<JudgeRegionMeta> {
        trace
            .details
            .iter()
            .map(|d| JudgeRegionMeta {
                judge_p: d.p,
                kind: d.kind,
                from_builder: d.kind == "builder",
            })
            .collect()
    }
}

/// In [`judge_regions`]' answer: the block belongs to a builder table the
/// judge rejects ([`judge::REJECT`]), which is read back as prose even where
/// the builder was sure of its grid.
#[doc(hidden)]
pub const REJECTED: usize = usize::MAX;

/// The table-or-not numbers beside the refusal: one computation, so the
/// signals cannot drift from the decision.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TableIsTrace {
    /// The `p_is_table` of each builder table the model refused, in table order.
    pub refused: Vec<f64>,
    /// Every builder table's `p_is_table`, by `heuristic_tables` id.
    pub probs: std::collections::BTreeMap<usize, f64>,
}

// The tests' table-or-not bar, or nothing: `#[cfg(test)]`-only, thread-local
// so one test's forced bar never leaks into another test on another thread.
// Serving reads the model's threshold; the pin test below forces this to 0
// and to 1 to hold the dump's verdicts to not reading this model (#574).
#[cfg(test)]
thread_local! {
    static TABLE_IS_THRESHOLD_OVERRIDE: std::cell::Cell<Option<u64>> =
        const { std::cell::Cell::new(None) };
}

/// [`models::table_is::THRESHOLD`], or the tests' override where one stands.
fn table_is_threshold() -> f64 {
    #[cfg(test)]
    if let Some(bits) = TABLE_IS_THRESHOLD_OVERRIDE.with(|o| o.get()) {
        return f64::from_bits(bits);
    }
    models::table_is::THRESHOLD
}

/// Forcing the table-or-not bar for one scope; the override is gone when the
/// guard drops, so a failing assert cannot leak it into the next test on this
/// thread.
#[cfg(test)]
struct TableIsThresholdGuard;

#[cfg(test)]
impl TableIsThresholdGuard {
    /// Force the bar to `threshold` until the guard drops.
    fn set(threshold: f64) -> Self {
        TABLE_IS_THRESHOLD_OVERRIDE.with(|o| o.set(Some(f64::to_bits(threshold))));
        TableIsThresholdGuard
    }
}

#[cfg(test)]
impl Drop for TableIsThresholdGuard {
    fn drop(&mut self) {
        TABLE_IS_THRESHOLD_OVERRIDE.with(|o| o.set(None));
    }
}

/// The judge's answer with every builder table the table-or-not model
/// refuses marked [`REJECTED`] (#638); a region the judge chose goes back
/// only when [`REFUSED_SHARE`] of it is refused. The numbers beside the
/// refusal, so the signals cannot drift from the decision.
fn refused_by_the_table_model_traced(
    refs: &[&Block],
    judged: &[Option<usize>],
    cells: &[CellHint],
    height: f64,
) -> (Vec<Option<usize>>, TableIsTrace) {
    let mut out = judged.to_vec();
    let mut trace = TableIsTrace::default();
    if cells.len() == refs.len() {
        let tables = heuristic_tables(refs);
        let mut by: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
        for (i, t) in tables.iter().enumerate() {
            if let Some(t) = t {
                by.entry(*t).or_default().push(i);
            }
        }
        for (id, blocks) in &by {
            let p = table_is_probability(&table_is_features(refs, blocks, cells, height));
            trace.probs.insert(*id, p);
            if p < table_is_threshold() {
                trace.refused.push(p);
            }
        }
        let refused: Vec<bool> = {
            let mut r = vec![false; refs.len()];
            for (id, blocks) in &by {
                let kept = trace
                    .probs
                    .get(id)
                    .is_some_and(|p| *p >= table_is_threshold());
                if !kept {
                    for &i in blocks {
                        r[i] = true;
                    }
                }
            }
            r
        };
        // A REGION THE JUDGE CHOSE LOSES NO PART OF ITSELF: a builder
        // table that is a fragment of a real table reads as no table on
        // its own, and taking its blocks out of the region the verifier
        // chose cut rows off that table (PubTables TEDS -0.007). A chosen
        // region goes back to prose only when all of it is refused.
        let whole = |k: usize| {
            let mine: Vec<usize> = (0..refs.len()).filter(|&i| judged[i] == Some(k)).collect();
            let gone = mine.iter().filter(|&&i| refused[i]).count();
            gone as f64 >= REFUSED_SHARE * mine.len() as f64
        };
        for i in 0..refs.len() {
            if refused[i] && judged[i].is_none_or(|k| k == REJECTED || whole(k)) {
                out[i] = Some(REJECTED);
            }
        }
    }
    (out, trace)
}

/// The share of a chosen region's blocks the table-or-not model must refuse
/// before the region goes back to prose (#638).
const REFUSED_SHARE: f64 = 1.0;

/// The number of table-or-not features, in `train_table_is.NAMES` order.
pub const TABLE_IS_FEATURES: usize = 15;

/// A cell of running text holds this many words (`train_table_is.WORDY`).
const WORDY: usize = 8;

/// Whether a cell's text is a figure: signs, currency, digits and nothing else.
fn is_figure(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_whitespace() || c.is_ascii_digit() || "$€£%()+-–−.,".contains(c))
}

/// One builder table's features for the table-or-not model, in
/// `train_table_is.NAMES` order (#638).
#[doc(hidden)]
pub fn table_is_features(
    page: &[&Block],
    blocks: &[usize],
    hints: &[CellHint],
    height: f64,
) -> [f64; TABLE_IS_FEATURES] {
    let place = |i: usize| match page[i].kind {
        BlockKind::TableCell { row, column, .. } => (row as usize, column as usize),
        _ => (0, 0),
    };
    let n = blocks.len().max(1) as f64;
    let rows = blocks
        .iter()
        .map(|&i| place(i).0)
        .max()
        .map_or(0, |r| r + 1);
    let cols = blocks
        .iter()
        .map(|&i| place(i).1)
        .max()
        .map_or(0, |c| c + 1);
    let words: Vec<usize> = blocks
        .iter()
        .map(|&i| page[i].text.split_whitespace().count())
        .collect();
    let heading: Vec<f64> = blocks
        .iter()
        .map(|&i| hints[i].p_heading.unwrap_or(0.0))
        .collect();
    let mut by_column: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
    for (k, &i) in blocks.iter().enumerate() {
        by_column.entry(place(i).1).or_default().push(words[k]);
    }
    let means: Vec<f64> = by_column
        .values()
        .map(|v| v.iter().sum::<usize>() as f64 / v.len() as f64)
        .collect();
    let first: Vec<usize> = (0..blocks.len())
        .filter(|&k| place(blocks[k]).1 == 0)
        .collect();
    let mean = |xs: &[f64]| {
        if xs.is_empty() {
            0.0
        } else {
            xs.iter().sum::<f64>() / xs.len() as f64
        }
    };
    let y0 = blocks
        .iter()
        .map(|&i| page[i].bbox[1])
        .fold(f64::INFINITY, f64::min);
    let y1 = blocks
        .iter()
        .map(|&i| page[i].bbox[3])
        .fold(f64::NEG_INFINITY, f64::max);
    [
        rows as f64,
        cols as f64,
        blocks.len() as f64,
        blocks.iter().filter(|&&i| is_figure(&page[i].text)).count() as f64 / n,
        words.iter().sum::<usize>() as f64 / n,
        means.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        means.iter().copied().fold(f64::INFINITY, f64::min),
        heading.iter().filter(|&&p| p >= 0.5).count() as f64 / n,
        heading.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        mean(&heading),
        mean(&first.iter().map(|&k| heading[k]).collect::<Vec<f64>>()),
        1.0 - blocks.len() as f64 / (rows * cols).max(1) as f64,
        (y1 - y0) / height.max(1.0),
        words.iter().filter(|&&w| w >= WORDY).count() as f64 / n,
        mean(&first.iter().map(|&k| words[k] as f64).collect::<Vec<f64>>()),
    ]
}

/// Whether the table-or-not model keeps a builder table as a table (#638).
pub fn is_a_table(page: &[&Block], blocks: &[usize], hints: &[CellHint], height: f64) -> bool {
    if blocks.is_empty() {
        return true;
    }
    table_is_probability(&table_is_features(page, blocks, hints, height)) >= table_is_threshold()
}

/// The table-or-not model's probability that a table is one.
pub fn table_is_probability(x: &[f64; TABLE_IS_FEATURES]) -> f64 {
    super::probability(models::table_is::predict_margin(x))
}

/// Which of the builder's tables each block belongs to, or `None` for a block
/// that is not a cell. A table is a run of adjacent cells in row-major order
/// on one page, exactly as `crate::markdown::table_run` reads it back, so a
/// new table starts where the coordinates stop climbing.
fn heuristic_tables(page: &[&Block]) -> Vec<Option<usize>> {
    let mut out = Vec::with_capacity(page.len());
    let mut id = 0usize;
    let mut last: Option<(usize, u16, u16)> = None;
    for block in page {
        match block.kind {
            BlockKind::TableCell { row, column, .. } => {
                match last {
                    Some((page_no, r, c)) if page_no == block.page && (row, column) > (r, c) => {}
                    Some(_) => id += 1,
                    None => {}
                }
                last = Some((block.page, row, column));
                out.push(Some(id));
            }
            _ => {
                if last.take().is_some() {
                    id += 1;
                }
                out.push(None);
            }
        }
    }
    out
}

/// A drawn grid with the words its cells index: [`lattice_runs`] places raw
/// span fragments, and the rebuild reads the cells against exactly that
/// list -- never against the page's joined words, whose shorter numbering
/// the fragment indices overrun.
#[derive(Debug, Clone)]
pub(crate) struct DrawnGrid {
    grid: separations::Grid,
    words: Vec<pairs::PairWord>,
    from: Vec<usize>,
}

/// Each drawn grid on the page that holds words, as the run of blocks inside
/// it and the cells its lines make (#574).
///
/// A block belongs to a grid when its centre lies inside the grid's extent.
/// A grid whose words do not almost all land in one of its cells -- a box
/// drawn round a figure, a form whose fields sit on its lines -- is not read
/// as a table.
fn lattice_runs(
    refs: &[&Block],
    lattices: &[super::lattice::Lattice],
) -> Vec<(Vec<usize>, Option<DrawnGrid>)> {
    let mut out: Vec<(Vec<usize>, Option<DrawnGrid>)> = Vec::new();
    for lattice in lattices {
        let [left, bottom, right, top] = lattice.bbox();
        let run: Vec<usize> = (0..refs.len())
            .filter(|&i| {
                let b = refs[i].bbox;
                let (x, y) = ((b[0] + b[2]) * 0.5, (b[1] + b[3]) * 0.5);
                x >= left && x <= right && y >= bottom && y <= top
            })
            .filter(|i| !out.iter().any(|(r, _)| r.contains(i)))
            .collect();
        if run.is_empty() {
            continue;
        }
        let (words, from) = words_of(refs, &run);
        if words.len() < 4 {
            continue;
        }
        let atoms = lattice.cells();
        let mut held: Vec<Vec<usize>> = vec![Vec::new(); atoms.len()];
        let mut placed = 0usize;
        for (w, word) in words.iter().enumerate() {
            let (x, y) = (
                (word.bbox[0] + word.bbox[2]) * 0.5,
                (word.bbox[1] + word.bbox[3]) * 0.5,
            );
            if let Some(c) = lattice.cell_at(&atoms, x, y) {
                held[c].push(w);
                placed += 1;
            }
        }
        if (placed as f64) < LATTICE_MIN_PLACED * words.len() as f64 {
            continue;
        }
        let mut cells: Vec<super::grid::Cell> = Vec::new();
        for (c, mut mine) in held.into_iter().enumerate() {
            if mine.is_empty() {
                continue;
            }
            // Reading order inside the cell: top line first, left to right.
            mine = in_reading_order(mine, &words);
            let bbox = [
                mine.iter()
                    .map(|&w| words[w].bbox[0])
                    .fold(f64::INFINITY, f64::min),
                mine.iter()
                    .map(|&w| words[w].bbox[1])
                    .fold(f64::INFINITY, f64::min),
                mine.iter()
                    .map(|&w| words[w].bbox[2])
                    .fold(f64::NEG_INFINITY, f64::max),
                mine.iter()
                    .map(|&w| words[w].bbox[3])
                    .fold(f64::NEG_INFINITY, f64::max),
            ];
            let (rows, columns) = atoms[c].clone();
            cells.push(super::grid::Cell {
                words: mine,
                bbox,
                rows: rows.collect(),
                columns: columns.collect(),
            });
        }
        let rows: std::collections::BTreeSet<usize> = cells
            .iter()
            .filter_map(|c| c.rows.first().copied())
            .collect();
        let columns: std::collections::BTreeSet<usize> = cells
            .iter()
            .filter_map(|c| c.columns.first().copied())
            .collect();
        if rows.len() < 2 || columns.len() < 2 {
            continue;
        }
        // A DRAWN CELL THAT HOLDS SEVERAL LINES ACROSS SEVERAL OF THE GRID'S
        // POSITIONS IS A GRID WHOSE INNER LINES WERE NOT DRAWN -- a box round
        // the table and a rule under its head -- and not one merged cell. The
        // region is still the grid's; its cells are the decoder's.
        let undrawn = cells.iter().any(|cell| {
            (cell.rows.len() > 1 || cell.columns.len() > 1) && lines_in(&cell.words, &words) > 1
        });
        let grid = (!undrawn).then_some(DrawnGrid {
            grid: separations::Grid {
                cells,
                header_rows: 1,
            },
            words,
            from,
        });
        out.push((run, grid));
    }
    out
}

/// How many text lines a set of words stands on: words whose vertical extents
/// overlap share a line.
fn lines_in(mine: &[usize], words: &[pairs::PairWord]) -> usize {
    let mut spans: Vec<(f64, f64)> = mine
        .iter()
        .map(|&w| (words[w].bbox[1], words[w].bbox[3]))
        .collect();
    spans.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut count = 0usize;
    let mut floor = f64::INFINITY;
    for (bottom, top) in spans {
        if top < floor {
            count += 1;
            floor = bottom;
        } else {
            floor = floor.min(bottom);
        }
    }
    count
}

/// The share of a grid's words that must land inside one of its cells for the
/// grid to be read as a table.
const LATTICE_MIN_PLACED: f64 = 0.9;

/// The judge's regions as runs of the page's blocks: list items left out,
/// every region grown to the builder tables it touches.
fn judged_runs(judged: &[Option<usize>], refs: &[&Block]) -> Vec<Vec<usize>> {
    let mut by: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
    for (i, k) in judged.iter().enumerate() {
        // A LIST ITEM STAYS A LIST ITEM. The judge read the page before
        // the list model ran, and a bulleted list set beside a table
        // was still paragraphs to it; a union then took the list into
        // the table (lists 0.492 -> 0.474 on DocLayNet's tuning half).
        // This pass runs after the list model, so its verdict is read
        // here: the item stays out of the region, and lists read
        // 0.492 again with tables at 0.534. Leaving every region that
        // held an item alone instead kept text 0.005 higher and cost
        // tables 0.017; that trade is written on #574.
        if matches!(refs[i].kind, BlockKind::ListItem { .. }) {
            continue;
        }
        if let Some(k) = k.filter(|&k| k != REJECTED) {
            by.entry(k).or_default().push(i);
        }
    }
    let regions: Vec<Vec<usize>> = by.into_values().collect();
    grown(regions, &heuristic_tables(refs))
}

/// Judged regions that never cut a builder's table: each takes in every
/// table it touches whole, and regions that then share a block become one.
///
/// A JUDGE MAY WIDEN A TABLE, NEVER SHRINK ONE. Its proposals are sets of
/// lines and a builder's table is a set of blocks, and a proposal that leaves
/// out a table's header line -- the alignment proposer's rows start at the
/// first row of two chunks -- would rebuild the rest of the table without it.
fn grown(regions: Vec<Vec<usize>>, tables: &[Option<usize>]) -> Vec<Vec<usize>> {
    let mut sets: Vec<std::collections::BTreeSet<usize>> = regions
        .into_iter()
        .map(|r| {
            let mut all: std::collections::BTreeSet<usize> = r.iter().copied().collect();
            let touched: Vec<usize> = r.iter().filter_map(|&i| tables[i]).collect();
            for (i, t) in tables.iter().enumerate() {
                if t.is_some_and(|t| touched.contains(&t)) {
                    all.insert(i);
                }
            }
            all
        })
        .collect();
    let mut merged = true;
    while merged {
        merged = false;
        'outer: for a in 0..sets.len() {
            for b in a + 1..sets.len() {
                if !sets[a].is_disjoint(&sets[b]) {
                    let other = sets.remove(b);
                    sets[a].extend(other);
                    merged = true;
                    break 'outer;
                }
            }
        }
    }
    let mut out: Vec<Vec<usize>> = sets.into_iter().map(|s| s.into_iter().collect()).collect();
    out.sort_by_key(|r| r[0]);
    out
}

/// Whether a run is exactly one of the builder's tables, every cell and
/// nothing else.
fn is_one_table(run: &[usize], tables: &[Option<usize>]) -> bool {
    let Some(table) = tables.get(run[0]).copied().flatten() else {
        return false;
    };
    let whole: Vec<usize> = (0..tables.len())
        .filter(|&i| tables[i] == Some(table))
        .collect();
    whole == run
}

/// The builder's cells no judged table holds, read back as one paragraph per
/// row: the row's cells joined in their order, at the row's first cell.
///
/// Or, with `per_cell` (the `demote_per_cell` switch), COLUMN BY COLUMN (#574).
/// The tables the judge rejects are mostly prose set in two columns, or a
/// list of labels over their values; joined by row, the left column's line
/// and the right column's were one paragraph, and a heading that was a cell
/// of its own -- `Operating leases`, `COMPENSATION COMMITTEE` -- came out glued
/// to the text beside it. On T6's rig-like proxy 274 of the 668 headings the
/// build missed were such a cell before the pass. So each column is read top
/// to bottom, a new block opens where the heading decision the pass took on
/// the cells changes, and a run of heading cells comes back a heading.
fn demote_unchosen(
    page: &[&Block],
    tables: &[Option<usize>],
    judged: &[Option<usize>],
    weakness: &Builder<'_>,
    hints: &[CellHint],
    per_cell: bool,
    replacement: &mut [Option<Vec<Block>>],
) {
    // ONLY A TABLE THE BUILDER ITSELF DOUBTED. A grid the builder was sure of
    // stays a table whatever the judge chose elsewhere on the page: demoting
    // every unchosen table drew fewer false tables (28 pages of 276 against
    // 48) and read four of the constructed tables in
    // `tables_from_columns_and_spans.rs` and one ruled table as prose.
    //
    // UNLESS THE JUDGE REJECTS IT OUTRIGHT (`judge::REJECT`): three columns
    // of running prose that the builder read as a sure grid score 0.004, and
    // they are prose whatever the builder's record says (#574, T6).
    //
    // UNLESS ITS COLUMNS ARE CLEAN: a grid of short aligned labels and
    // figures is a table the caller asked the strategy for, even where the
    // judge rejects it -- the same exemption the final-table verdict reads
    // (`table_has_clean_columns`, #659).
    let doubted = |t: usize| {
        let whole: Vec<usize> = (0..tables.len())
            .filter(|&i| tables[i] == Some(t))
            .collect();
        let cells: Vec<&Block> = whole.iter().filter_map(|&i| page.get(i).copied()).collect();
        (whole.iter().all(|&i| judged[i] == Some(REJECTED))
            || matches!(weakness(&whole, page), Some(w) if may_overrule(&w)))
            && !table_has_clean_columns(&cells)
    };
    let mut rows: std::collections::BTreeMap<(usize, u16), Vec<usize>> = Default::default();
    for (i, block) in page.iter().enumerate() {
        if judged[i].is_some_and(|k| k != REJECTED) || replacement[i].is_some() {
            continue;
        }
        if let (Some(table), BlockKind::TableCell { row, .. }) = (tables[i], block.kind) {
            if doubted(table) {
                rows.entry((table, row)).or_default().push(i);
            }
        }
    }
    if per_cell && hints.len() == page.len() {
        demote_by_column(page, rows, hints, replacement);
        return;
    }
    for cells in rows.into_values() {
        let first = cells[0];
        let mut made = page[first].clone();
        made.kind = BlockKind::Paragraph;
        made.text = cells
            .iter()
            .map(|&i| page[i].text.trim())
            .filter(|t| !t.is_empty())
            .collect::<Vec<&str>>()
            .join(" ");
        made.spans = cells
            .iter()
            .flat_map(|&i| page[i].spans.iter().cloned())
            .collect();
        made.bbox = [
            cells
                .iter()
                .map(|&i| page[i].bbox[0])
                .fold(f64::INFINITY, f64::min),
            cells
                .iter()
                .map(|&i| page[i].bbox[1])
                .fold(f64::INFINITY, f64::min),
            cells
                .iter()
                .map(|&i| page[i].bbox[2])
                .fold(f64::NEG_INFINITY, f64::max),
            cells
                .iter()
                .map(|&i| page[i].bbox[3])
                .fold(f64::NEG_INFINITY, f64::max),
        ];
        made.grid = None;
        made.confidence = None;
        replacement[first] = Some(vec![made]);
        for &i in &cells[1..] {
            replacement[i] = Some(Vec::new());
        }
    }
}

/// [`demote_unchosen`] column by column: each doubted table's cells in
/// column order, top to bottom, cut where the heading decision changes; every
/// part at the table's first cell, in that order.
fn demote_by_column(
    page: &[&Block],
    rows: std::collections::BTreeMap<(usize, u16), Vec<usize>>,
    hints: &[CellHint],
    replacement: &mut [Option<Vec<Block>>],
) {
    let mut by_table: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
    for ((table, _row), cells) in rows {
        by_table.entry(table).or_default().extend(cells);
    }
    for cells in by_table.into_values() {
        let first = *cells.iter().min().expect("a table has a cell");
        let at = |i: usize| match page[i].kind {
            BlockKind::TableCell { row, column, .. } => (column, row),
            _ => (0, 0),
        };
        let mut ordered = cells.clone();
        ordered.sort_by_key(|&i| (at(i), i));
        let mut parts: Vec<Vec<usize>> = Vec::new();
        for &i in &ordered {
            let opens = match parts.last().and_then(|p| p.last()) {
                None => true,
                Some(&prev) => at(prev).0 != at(i).0 || hints[prev].heading != hints[i].heading,
            };
            if opens {
                parts.push(vec![i]);
            } else if let Some(part) = parts.last_mut() {
                part.push(i);
            }
        }
        let made: Vec<Block> = parts
            .iter()
            .map(|part| {
                let mut block = joined(page, part);
                if hints[part[0]].heading {
                    block.kind = BlockKind::Heading { level: 1 };
                    let ps: Vec<f64> = part.iter().filter_map(|&i| hints[i].p_heading).collect();
                    block.confidence =
                        (!ps.is_empty()).then(|| ps.iter().sum::<f64>() / ps.len() as f64);
                }
                block
            })
            .collect();
        for &i in &cells {
            replacement[i] = Some(Vec::new());
        }
        replacement[first] = Some(made);
    }
}

/// The blocks `part` of `page` as one paragraph: texts joined, spans in
/// order, the box around them all.
fn joined(page: &[&Block], part: &[usize]) -> Block {
    let mut made = page[part[0]].clone();
    made.kind = BlockKind::Paragraph;
    made.text = part
        .iter()
        .map(|&i| page[i].text.trim())
        .filter(|t| !t.is_empty())
        .collect::<Vec<&str>>()
        .join(" ");
    made.spans = part
        .iter()
        .flat_map(|&i| page[i].spans.iter().cloned())
        .collect();
    made.bbox = [
        part.iter()
            .map(|&i| page[i].bbox[0])
            .fold(f64::INFINITY, f64::min),
        part.iter()
            .map(|&i| page[i].bbox[1])
            .fold(f64::INFINITY, f64::min),
        part.iter()
            .map(|&i| page[i].bbox[2])
            .fold(f64::NEG_INFINITY, f64::max),
        part.iter()
            .map(|&i| page[i].bbox[3])
            .fold(f64::NEG_INFINITY, f64::max),
    ];
    made.grid = None;
    made.confidence = None;
    made
}

/// Whether a run holds some of a builder's table and not all of it.
fn cuts_a_table(run: &[usize], tables: &[Option<usize>]) -> bool {
    let mut touched: Vec<usize> = run.iter().filter_map(|&i| tables[i]).collect();
    touched.sort_unstable();
    touched.dedup();
    touched.iter().any(|table| {
        tables
            .iter()
            .enumerate()
            .any(|(i, t)| *t == Some(*table) && !run.contains(&i))
    })
}

/// What the builder says about the table a run of blocks came from: the seam
/// [`refine_gated`] takes and [`weakness_of`] will fill once #624's follow-up
/// lands.
#[doc(hidden)]
pub type Builder<'a> = dyn Fn(&[usize], &[&Block]) -> Option<Weakness> + 'a;

/// [`refine`], with the builder's record supplied.
///
/// THE SEAM T2'S RECORD PLUGS INTO, and the one a test drives. It is an
/// argument rather than a global because the learned pass is pure per page --
/// no state between calls, so pages run in any order and at the same time, and
/// `learned_pages_run_in_parallel.rs` holds them to it. A settable global
/// would be the easy way to test this and would quietly break that.
#[doc(hidden)]
pub fn refine_gated(
    page: Vec<Block>,
    size: (f64, f64),
    facts: &super::DocumentFacts,
    switches: super::Switches,
    weakness: &Builder<'_>,
) -> (Vec<Block>, Vec<usize>) {
    refine_judged(page, size, facts, switches, weakness, None, &[], &[])
}

/// The judged region a run mostly came from: the most frequent `Some(k)` with
/// `k != REJECTED` among its blocks. A grown run holds blocks of several
/// regions plus the builder tables it took in; the frequent one is the region
/// that drew it.
fn primary_choice(run: &[usize], judged: &[Option<usize>]) -> Option<usize> {
    let mut counts: std::collections::BTreeMap<usize, usize> = Default::default();
    for &i in run {
        if let Some(k) = judged.get(i).copied().flatten().filter(|&k| k != REJECTED) {
            *counts.entry(k).or_default() += 1;
        }
    }
    counts
        .into_iter()
        .max_by(|a, b| a.1.cmp(&b.1).then(b.0.cmp(&a.0)))
        .map(|(k, _)| k)
}

/// The `p_is_table` of the builder table a run came from: `Some` when the run
/// holds exactly one builder table, `None` when none or several.
fn builder_prob(
    run: &[usize],
    tables: &[Option<usize>],
    probs: &std::collections::BTreeMap<usize, f64>,
) -> Option<f64> {
    let mut ids: Vec<usize> = run
        .iter()
        .filter_map(|&i| tables.get(i).copied().flatten())
        .collect();
    ids.sort_unstable();
    ids.dedup();
    (ids.len() == 1)
        .then(|| probs.get(&ids[0]).copied())
        .flatten()
}

fn split_of(title_removed: bool, key_split: bool) -> TableSplit {
    match (title_removed, key_split) {
        (false, false) => TableSplit::Intact,
        (true, false) => TableSplit::TitleRowsRemoved,
        (false, true) => TableSplit::KeyValueSplit,
        (true, true) => TableSplit::TitleAndKeyValue,
    }
}

/// The box round `range` of `page`: the union of its blocks' boxes.
fn union_of(page: &[Block], range: std::ops::Range<usize>) -> [f64; 4] {
    let mut bbox = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for block in page.get(range).unwrap_or(&[]) {
        bbox[0] = bbox[0].min(block.bbox[0]);
        bbox[1] = bbox[1].min(block.bbox[1]);
        bbox[2] = bbox[2].max(block.bbox[2]);
        bbox[3] = bbox[3].max(block.bbox[3]);
    }
    if bbox[0] > bbox[2] || bbox[1] > bbox[3] {
        [0.0; 4]
    } else {
        bbox
    }
}

/// How far a teacher box grows on every side before the region lines are read,
/// in median line heights: teacher boxes sit tight on the ink and a header
/// line's ascenders fall just outside.
pub(crate) const REGION_PAD_LINES: f64 = 0.5;

/// TATR table structure for one extraction (track B, P3): the page source
/// plus the options that asked for it.
///
/// Hidden rather than published: a caller gets
/// [`ExtractOptions::with_tatr`](crate::blocks::ExtractOptions::with_tatr),
/// and `document` builds one of these per extraction. It travels from the
/// public `refine_with_tatr` entry down to [`rebuild_region`] -- the page
/// and renderer the rebuild never had -- so the learned pass and the teacher
/// entry ask TATR through the same type. `#[non_exhaustive]` per the 1.x
/// rule, though every field is private and only this crate builds one.
#[doc(hidden)]
#[derive(Clone)]
#[non_exhaustive]
pub struct TatrContext<'a> {
    /// The document to render regions from. `None` only for a context a
    /// unit test builds around the fake seam, which renders nothing.
    document: Option<&'a PdfDocument>,
    /// The options that asked for TATR.
    options: TatrOptions,
}

impl std::fmt::Debug for TatrContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The document does not print: `PdfDocument` is not `Debug`, and
        // the options are what a reader of this context wants.
        f.debug_struct("TatrContext")
            .field("has_document", &self.document.is_some())
            .field("options", &self.options)
            .finish_non_exhaustive()
    }
}

impl<'a> TatrContext<'a> {
    /// One extraction's TATR context: render from `document` under `options`.
    pub(crate) fn new(document: &'a PdfDocument, options: TatrOptions) -> Self {
        Self {
            document: Some(document),
            options,
        }
    }
}

/// One region offered to TATR: the extraction's context plus where the
/// region came from. Built at the [`rebuild_region`] call site -- the
/// learned pass says false, the teacher box says true -- so the
/// `detector_regions_only` restriction reads the source, not the caller.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TatrRegion<'a> {
    /// The extraction's TATR context.
    context: &'a TatrContext<'a>,
    /// Whether a detector or teacher box drew this region.
    from_detector: bool,
}

impl<'a> TatrRegion<'a> {
    /// Offer one region from `from_detector` under `context`.
    pub(crate) fn of(context: &'a TatrContext<'a>, from_detector: bool) -> Self {
        Self {
            context,
            from_detector,
        }
    }
}

/// Whether each region says out loud who rebuilt it: TATR, or the current
/// grid and why.
///
/// Read once, the way `PDFLUENT_BLOCK_TRACE` is: asked on every region of
/// every page, and an environment lookup per region is a cost paid by every
/// caller for a switch almost nobody turns on.
fn tatr_tracing() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("PDFLUENT_TATR_TRACE").is_some())
}

/// One region's structure verdict, out under `PDFLUENT_TATR_TRACE`: `tatr`
/// where TATR's cells are the table, `fallback(<reason>)` where the current
/// grid runs, each with the attempt's milliseconds.
fn tatr_trace(page: usize, region: [f64; 4], outcome: TatrOutcome, started: std::time::Instant) {
    if !tatr_tracing() {
        return;
    }
    let outcome = match outcome {
        TatrOutcome::Fired => "tatr".to_string(),
        TatrOutcome::Fallback(reason) => format!("fallback({reason})"),
    };
    eprintln!(
        "TATR page={page} region=[{:.1} {:.1} {:.1} {:.1}] {outcome} ms={:.1}",
        region[0],
        region[1],
        region[2],
        region[3],
        started.elapsed().as_secs_f64() * 1000.0,
    );
}

/// One region's structure verdict: TATR's cells, or the current grid and the
/// trace's reason.
#[derive(Debug, Clone, Copy)]
enum TatrOutcome {
    /// TATR's cells are the table.
    Fired,
    /// The current grid runs; the trace's reason.
    Fallback(&'static str),
}

/// What the hook concluded for one region: TATR's cells, or the fallback.
pub(crate) enum TatrVerdict {
    /// TATR's cells are the table.
    Cells(Vec<Block>),
    /// The current grid runs; the trace's reason.
    Fallback(&'static str),
}

/// The region's box in PDF points, y up: what its words cover.
fn region_bbox(words: &[pairs::PairWord]) -> [f64; 4] {
    let mut bbox = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for word in words {
        bbox[0] = bbox[0].min(word.bbox[0]);
        bbox[1] = bbox[1].min(word.bbox[1]);
        bbox[2] = bbox[2].max(word.bbox[2]);
        bbox[3] = bbox[3].max(word.bbox[3]);
    }
    if bbox[0] > bbox[2] || bbox[1] > bbox[3] {
        [0.0; 4]
    } else {
        bbox
    }
}

/// TATR's cells must claim at least this share of the region words whose
/// centre falls inside TATR's own table extent (the union of its cell
/// boxes), or the rebuild falls back to the current grid.
///
/// The claim is the direct one -- word centre in a cell, before the gap
/// fill -- so a grid that reaches past its words cannot pass on the fill's
/// charity. Lives here rather than in `tables_tatr` because that module is
/// behind `ort-tables` and the fake seam reads this on every build.
///
/// TUNED ON TUNING (P3b): 0.0, 0.5, 0.7, 0.9, 0.95 against this 0.8. TEDS
/// is flat across the range on both sets, all GO; a 402-word false table
/// stands at 0.5 and is gone at 0.7; 0.7 through 0.9 read byte-identical on
/// all 263 TUNING pages; 0.95 falls back two regions more for nothing and
/// would break the P3 2x2 seam (32 of 34 words). 0.8 stands, with margin on
/// both sides.
pub(crate) const TATR_MIN_COVERAGE: f64 = 0.8;

/// TATR's table extent: the union of its cell boxes, or `None` where there
/// is nothing to union.
///
/// Degenerate boxes -- zero or negative width or height -- are skipped, not
/// unioned: the fake seam builds a `[0, 0, 0, 0]` box for a cell that claims
/// nothing, and unioning it would stretch the extent to the page origin. A
/// decode whose every cell is degenerate has no extent, which is a decline,
/// not a fire.
pub(crate) fn tatr_table_extent(cell_boxes: &[[f64; 4]]) -> Option<[f64; 4]> {
    let mut extent: Option<[f64; 4]> = None;
    for cbox in cell_boxes {
        if cbox[0] >= cbox[2] || cbox[1] >= cbox[3] {
            continue;
        }
        extent = Some(match extent {
            None => *cbox,
            Some(had) => [
                had[0].min(cbox[0]),
                had[1].min(cbox[1]),
                had[2].max(cbox[2]),
                had[3].max(cbox[3]),
            ],
        });
    }
    extent
}

/// Of the words whose centre falls inside `extent`, how many `claims`
/// holds: `(claimed_inside, inside)`.
///
/// The coverage guard's numerator and denominator, over the DIRECT claims --
/// word centre in a cell, before [`tatr_fill_gaps`] -- so a grid that
/// reaches past its words cannot pass on the fill's charity. Claim indices
/// past the words are ignored: only a test builds those, and a typo there
/// must not panic the library. Both box lists share one frame; the caller
/// maps them there.
pub(crate) fn tatr_coverage_counts(
    word_boxes: &[[f64; 4]],
    extent: [f64; 4],
    claims: &[Vec<usize>],
) -> (usize, usize) {
    let mut claimed = vec![false; word_boxes.len()];
    for cell in claims {
        for &w in cell {
            if let Some(slot) = claimed.get_mut(w) {
                *slot = true;
            }
        }
    }
    let mut inside = 0usize;
    let mut claimed_inside = 0usize;
    for (w, bbox) in word_boxes.iter().enumerate() {
        let cx = (bbox[0] + bbox[2]) / 2.0;
        let cy = (bbox[1] + bbox[3]) / 2.0;
        if extent[0] <= cx && cx <= extent[2] && extent[1] <= cy && cy <= extent[3] {
            inside += 1;
            if claimed[w] {
                claimed_inside += 1;
            }
        }
    }
    (claimed_inside, inside)
}

/// Words inside `extent` that no cell claims, by word centre, join the
/// nearest cell in the same row band, then the same column band, then the
/// nearest cell: every inside word lands somewhere, never dropped.
///
/// A row band is a cell's own vertical span, a column band its horizontal
/// one; nearness inside a band is the gap to the cell's edge, 0 where the
/// word overlaps it, and past both bands the squared centre distance. Ties
/// keep the first cell, in row-major order, as the direct fill does. Words
/// outside the extent are not touched -- they are prose the table shrank
/// past, and the hook re-emits them as the blocks they came from. Each
/// cell's list is in word order afterwards: the direct claims already are,
/// and the filled words merge in.
///
/// Degenerate boxes never win while a placed cell stands: their bands are a
/// line and a point, and their centre sits wherever the seam put the box,
/// away from the region's words.
pub(crate) fn tatr_fill_gaps(
    word_boxes: &[[f64; 4]],
    cell_boxes: &[[f64; 4]],
    extent: [f64; 4],
    claims: &mut [Vec<usize>],
) {
    assert_eq!(
        claims.len(),
        cell_boxes.len(),
        "one claim list per cell box"
    );
    let mut claimed = vec![false; word_boxes.len()];
    for cell in claims.iter() {
        for &w in cell {
            if let Some(slot) = claimed.get_mut(w) {
                *slot = true;
            }
        }
    }
    for (w, bbox) in word_boxes.iter().enumerate() {
        if claimed[w] {
            continue;
        }
        let cx = (bbox[0] + bbox[2]) / 2.0;
        let cy = (bbox[1] + bbox[3]) / 2.0;
        if !(extent[0] <= cx && cx <= extent[2] && extent[1] <= cy && cy <= extent[3]) {
            continue;
        }
        let gap = |lo: f64, hi: f64, at: f64| {
            if at < lo {
                lo - at
            } else if at > hi {
                at - hi
            } else {
                0.0
            }
        };
        let mut best: Option<(usize, u8, f64)> = None;
        for (cell, cbox) in cell_boxes.iter().enumerate() {
            let in_row = cbox[1] <= cy && cy <= cbox[3];
            let in_column = cbox[0] <= cx && cx <= cbox[2];
            let (rank, distance) = if in_row {
                (0u8, gap(cbox[0], cbox[2], cx))
            } else if in_column {
                (1u8, gap(cbox[1], cbox[3], cy))
            } else {
                let dx = (cbox[0] + cbox[2]) / 2.0 - cx;
                let dy = (cbox[1] + cbox[3]) / 2.0 - cy;
                (2u8, dx * dx + dy * dy)
            };
            let better = match best {
                None => true,
                Some((_, best_rank, best_distance)) => {
                    (rank, distance) < (best_rank, best_distance)
                }
            };
            // Strictly better wins; ties keep the first cell.
            if better {
                best = Some((cell, rank, distance));
            }
        }
        if let Some((cell, _, _)) = best {
            claims[cell].push(w);
            claimed[w] = true;
        }
    }
    for cell in claims.iter_mut() {
        cell.sort_unstable();
    }
}

/// TATR's cells plus the region words no cell claimed, re-emitted as the
/// blocks they came from: prose stays prose, in reading order.
///
/// THE INVARIANT (P3b): a fire never loses a word. The grid path claims
/// every region word or returns empty and the originals are kept; TATR's
/// cells claim a subset, so the rest comes back here -- grouped by the
/// source block, prose kinds preserved, a table cell's words released as a
/// paragraph -- with the table standing where its first claimed block stood.
/// `placed` is one grid cell plus its header flag per TATR cell, words in
/// final-claim order; the prose of source blocks before the table's first
/// claimed block stands before it, the rest after, each side in block order.
pub(crate) fn conserved_table_blocks(
    placed: &[(super::grid::Cell, bool)],
    pair_words: &[pairs::PairWord],
    from: &[usize],
    page_blocks: &[&Block],
    page_number: usize,
    cell_path_words: bool,
) -> Vec<Block> {
    let mut claimed = vec![false; pair_words.len()];
    for (cell, _) in placed {
        for &w in &cell.words {
            if let Some(slot) = claimed.get_mut(w) {
                *slot = true;
            }
        }
    }
    let anchor = claimed
        .iter()
        .enumerate()
        .filter(|(_, c)| **c)
        .filter_map(|(w, _)| from.get(w).copied())
        .min();
    let mut groups: std::collections::BTreeMap<usize, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (w, c) in claimed.iter().enumerate() {
        if !c {
            groups.entry(from[w]).or_default().push(w);
        }
    }
    let mut before = Vec::new();
    let mut after = Vec::new();
    for (block, words) in &groups {
        let prose = prose_block_for(*block, words, pair_words, page_blocks, page_number);
        match anchor {
            Some(first) if *block < first => before.push(prose),
            _ => after.push(prose),
        }
    }
    let mut out = before;
    out.extend(placed.iter().map(|(cell, header)| {
        cell_block_with_cell_path_words(
            cell,
            pair_words,
            from,
            page_blocks,
            None,
            page_number,
            *header,
            None,
            false,
            cell_path_words,
        )
    }));
    out.extend(after);
    out
}

/// The region words of one source block that TATR's cells did not claim, as
/// the block they came from.
///
/// Prose kinds are preserved -- a paragraph, item or heading stays one --
/// and a table cell's words are released as a paragraph, the normal
/// non-table path for words the table shrank past. The text is the words in
/// page order, the box their union, the spans the source spans whose centre
/// falls in it (the source's own spans where none does, as [`cell_spans`]).
/// Never a cell: these words stand outside the table now.
fn prose_block_for(
    block: usize,
    words: &[usize],
    pair_words: &[pairs::PairWord],
    page_blocks: &[&Block],
    page_number: usize,
) -> Block {
    let source = page_blocks[block];
    let kind = match source.kind {
        BlockKind::TableCell { .. } => BlockKind::Paragraph,
        kind => kind,
    };
    let text = join_decoded_words(pair_words, words);
    let mut bbox = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for &w in words {
        let held = pair_words[w].bbox;
        bbox[0] = bbox[0].min(held[0]);
        bbox[1] = bbox[1].min(held[1]);
        bbox[2] = bbox[2].max(held[2]);
        bbox[3] = bbox[3].max(held[3]);
    }
    let mut spans: Vec<TextSpan> = source
        .spans
        .iter()
        .filter(|span| {
            let x = span.x + span.width / 2.0;
            let y = span.y + span.height / 2.0;
            x >= bbox[0] && x <= bbox[2] && y >= bbox[1] && y <= bbox[3]
        })
        .cloned()
        .collect();
    if spans.is_empty() {
        spans.extend(source.spans.iter().cloned());
    }
    Block {
        kind,
        page: page_number,
        bbox,
        text,
        spans,
        index: 0,
        continues_previous: false,
        confidence: None,
        grid: None,
        table_origin: TableOrigin::Other,
        trace_learned: None,
    }
}

/// The fake seam's answer through the same conservation the real structurer
/// reads: `None` where the canned cells earn no fire -- no cells, no
/// extent, or under [`TATR_MIN_COVERAGE`] of the words inside it -- so a
/// low-coverage fake reads the fallback it names. An index past the region's
/// words is dropped -- only a test builds these, and a typo there must not
/// panic the library.
fn fake_conserved_cells(
    cells: &[TatrTestCell],
    pair_words: &[pairs::PairWord],
    from: &[usize],
    page_blocks: &[&Block],
    page_number: usize,
    cell_path_words: bool,
) -> Option<Vec<Block>> {
    if cells.is_empty() {
        return None;
    }
    let mut boxes = Vec::with_capacity(cells.len());
    let mut claims = Vec::with_capacity(cells.len());
    for cell in cells {
        let claimed: Vec<usize> = cell
            .words
            .iter()
            .copied()
            .filter(|&w| w < pair_words.len())
            .collect();
        let mut bbox = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for &w in &claimed {
            let held = pair_words[w].bbox;
            bbox[0] = bbox[0].min(held[0]);
            bbox[1] = bbox[1].min(held[1]);
            bbox[2] = bbox[2].max(held[2]);
            bbox[3] = bbox[3].max(held[3]);
        }
        if claimed.is_empty() {
            bbox = [0.0; 4];
        }
        boxes.push(bbox);
        claims.push(claimed);
    }
    let extent = tatr_table_extent(&boxes)?;
    let word_boxes: Vec<[f64; 4]> = pair_words.iter().map(|w| w.bbox).collect();
    let (claimed_inside, inside) = tatr_coverage_counts(&word_boxes, extent, &claims);
    if inside == 0 || claimed_inside as f64 / (inside as f64) < TATR_MIN_COVERAGE {
        return None;
    }
    tatr_fill_gaps(&word_boxes, &boxes, extent, &mut claims);
    let placed: Vec<(super::grid::Cell, bool)> = cells
        .iter()
        .zip(claims)
        .zip(boxes)
        .map(|((cell, words), bbox)| {
            (
                super::grid::Cell {
                    words,
                    bbox,
                    rows: cell.rows.clone(),
                    columns: cell.columns.clone(),
                },
                cell.header,
            )
        })
        .collect();
    Some(conserved_table_blocks(
        &placed,
        pair_words,
        from,
        page_blocks,
        page_number,
        cell_path_words,
    ))
}

/// How far a span's baseline may lean from the horizontal and still read
/// upright for TATR: ten degrees, the quarter-turn tolerance #574 assembles
/// its turned runs by. A typeset line that merely drifts -- the 3-degree
/// skew the guard test pins -- fires; a landscape table at 90, 180 or 270
/// degrees, or at any free angle past this, declines.
const TATR_UPRIGHT_TOLERANCE_RAD: f64 = 0.174_533; // 10 degrees

/// Whether the span's baseline reads upright: within
/// [`TATR_UPRIGHT_TOLERANCE_RAD`] of +x.
///
/// The same reading `text` assembles its turned runs by -- the first column
/// of the composed transform, normalised -- stated here because that module
/// keeps it private. A span with no transform is hand-built and reads
/// upright, and a degenerate one has no direction and may as well read as
/// the page's own, exactly as there.
fn tatr_span_is_upright(span: &TextSpan) -> bool {
    let Some([a, b, ..]) = span.transform else {
        return true;
    };
    if (a * a + b * b).sqrt() <= f64::EPSILON {
        return true;
    }
    b.atan2(a).abs() <= TATR_UPRIGHT_TOLERANCE_RAD
}

/// Whether the region reads rotated: most of its words do (track B, P3d).
///
/// TATR decodes the region's pixels, so on a landscape-rotated table it
/// reads pixel rows for table rows -- PMC4904428-p0005 decodes 6 x 21 where
/// truth is 22 x 6, TEDS-struct 0 -- while the grid below reads the rotated
/// text in its own frame. A rotated region therefore never reaches the
/// structurer, real or fake, and falls back with `rotated` before any render
/// or inference.
///
/// The membership is [`words_of_region`]'s -- the lines' words where the
/// lines were built from these words, else the region blocks' words -- so
/// the verdict reads exactly the words the fill would. A word past its
/// block's spans counts as upright: only a test builds those, and a typo
/// there must fire as before rather than decline the region.
fn tatr_region_is_rotated(
    page_blocks: &[&Block],
    lines: &[judge::Line],
    words: &[PageWord],
    region_lines: &[usize],
) -> bool {
    let mut in_region = vec![false; lines.len()];
    for &i in region_lines {
        if let Some(slot) = in_region.get_mut(i) {
            *slot = true;
        }
    }
    let mut total = 0usize;
    let mut rotated = 0usize;
    let mut count = |word: &PageWord| {
        let turned = page_blocks
            .get(word.block)
            .and_then(|block| block.spans.get(word.span))
            .is_some_and(|span| !tatr_span_is_upright(span));
        total += 1;
        rotated += usize::from(turned);
    };
    if let Some(line_of) = line_of_each_word(page_blocks.len(), lines, words) {
        for (index, word) in words.iter().enumerate() {
            let held = line_of
                .get(index)
                .copied()
                .flatten()
                .is_some_and(|line| in_region[line]);
            if held && word.block < page_blocks.len() {
                count(word);
            }
        }
    } else {
        let mut blocks: Vec<usize> = region_lines
            .iter()
            .filter_map(|&i| lines.get(i).map(|line| line.block))
            .filter(|&b| b < page_blocks.len())
            .collect();
        blocks.sort_unstable();
        blocks.dedup();
        for word in words.iter() {
            if blocks.contains(&word.block) {
                count(word);
            }
        }
    }
    rotated * 2 > total
}

/// TATR first, the current grid after (track B, P3).
///
/// `None` -- TATR not asked for -- traces `fallback(no-model)` and runs the
/// grid, as does a region a detector never drew under `detector_regions_only`
/// (`fallback(not-a-detector-region)`) and a region that reads rotated
/// (`fallback(rotated)`). The fake seam answers next, without
/// touching the model on any build; then the real structurer, behind
/// `ort-tables`. A fired attempt's cells plus the region words no cell
/// claimed -- re-emitted as the blocks they came from -- are the verdict; a
/// decline, a low-coverage fire, or a failed region falls back with its
/// reason, and a failed SETUP refuses the extraction up front -- a region
/// only sees one past a file deleted mid-run.
///
/// `pair_words` is the rebuild's own region words, for the trace's box;
/// the fills recompute them over the joined words (the fake here, the real
/// structurer in [`tatr_attempt`](super::tables_tatr::tatr_attempt)). The
/// rest are the rebuild's own inputs.
/// [`tatr_for_region_with_cell_path_words`] without the cell-path word rules.
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn tatr_for_region(
    tatr: Option<&TatrRegion>,
    page_blocks: &[&Block],
    lines: &[judge::Line],
    words: &[PageWord],
    region_lines: &[usize],
    pair_words: &[pairs::PairWord],
    page_number: usize,
) -> TatrVerdict {
    tatr_for_region_with_cell_path_words(
        tatr,
        page_blocks,
        lines,
        words,
        region_lines,
        pair_words,
        page_number,
        false,
    )
}

/// [`tatr_for_region`] with the cell-path fill where `cell_path_words` is
/// true: the fake and the real structurer read the joined fill words.
#[allow(clippy::too_many_arguments)]
pub(crate) fn tatr_for_region_with_cell_path_words(
    tatr: Option<&TatrRegion>,
    page_blocks: &[&Block],
    lines: &[judge::Line],
    words: &[PageWord],
    region_lines: &[usize],
    pair_words: &[pairs::PairWord],
    page_number: usize,
    cell_path_words: bool,
) -> TatrVerdict {
    let started = std::time::Instant::now();
    let region = region_bbox(pair_words);
    let Some(request) = tatr else {
        tatr_trace(
            page_number,
            region,
            TatrOutcome::Fallback("no-model"),
            started,
        );
        return TatrVerdict::Fallback("no-model");
    };
    let options = &request.context.options;
    if options.detector_regions_only && !request.from_detector {
        tatr_trace(
            page_number,
            region,
            TatrOutcome::Fallback("not-a-detector-region"),
            started,
        );
        return TatrVerdict::Fallback("not-a-detector-region");
    }
    if tatr_region_is_rotated(page_blocks, lines, words, region_lines) {
        tatr_trace(
            page_number,
            region,
            TatrOutcome::Fallback("rotated"),
            started,
        );
        return TatrVerdict::Fallback("rotated");
    }
    if let Some(hook) = &options.test_hook {
        // THE ENGINE'S WORDS, as the real structurer fills: the fake's
        // indices name the joined words, not the raw page words.
        let fill_words = tatr_fill_words_with_cell_path_words(words, page_blocks, cell_path_words);
        let (fill_pair_words, fill_from) =
            words_of_region(page_blocks, lines, &fill_words, region_lines);
        let verdict = match hook {
            TatrTestHook::Cells(cells) => {
                match fake_conserved_cells(
                    cells,
                    &fill_pair_words,
                    &fill_from,
                    page_blocks,
                    page_number,
                    cell_path_words,
                ) {
                    Some(blocks) => TatrVerdict::Cells(blocks),
                    None => TatrVerdict::Fallback("low-coverage"),
                }
            }
            TatrTestHook::Declined => TatrVerdict::Fallback("declined"),
            TatrTestHook::Failed => TatrVerdict::Fallback("inference-failed"),
        };
        let outcome = match &verdict {
            TatrVerdict::Cells(_) => TatrOutcome::Fired,
            TatrVerdict::Fallback(reason) => TatrOutcome::Fallback(reason),
        };
        tatr_trace(page_number, region, outcome, started);
        return verdict;
    }
    #[cfg(feature = "ort-tables")]
    {
        real_tatr_for_region(
            request,
            page_blocks,
            lines,
            words,
            region_lines,
            region,
            page_number,
            started,
            cell_path_words,
        )
    }
    #[cfg(not(feature = "ort-tables"))]
    {
        // Unreachable through the public entries: `validate` refuses a TATR
        // request without the fake on these builds. A hand-built context
        // still falls back rather than panic.
        let _ = (page_blocks, lines, words, region_lines);
        tatr_trace(
            page_number,
            region,
            TatrOutcome::Fallback("no-model"),
            started,
        );
        TatrVerdict::Fallback("no-model")
    }
}

/// The real structurer: TATR through the shared session, behind `ort-tables`.
#[cfg(feature = "ort-tables")]
#[allow(clippy::too_many_arguments)]
fn real_tatr_for_region(
    request: &TatrRegion,
    page_blocks: &[&Block],
    lines: &[judge::Line],
    words: &[PageWord],
    region_lines: &[usize],
    region: [f64; 4],
    page_number: usize,
    started: std::time::Instant,
    cell_path_words: bool,
) -> TatrVerdict {
    let Some(document) = request.context.document else {
        // A unit test's fake-only context, asked past its seam: nothing to
        // render from, so the grid runs.
        tatr_trace(
            page_number,
            region,
            TatrOutcome::Fallback("no-model"),
            started,
        );
        return TatrVerdict::Fallback("no-model");
    };
    let config = request.context.options.runtime_config();
    let attempt = super::tables_tatr::tatr_attempt(
        document,
        page_number,
        region,
        page_blocks,
        lines,
        words,
        region_lines,
        &config,
        cell_path_words,
    );
    let verdict = match attempt {
        Ok(super::tables_tatr::TatrAttempt::Cells(cells)) => TatrVerdict::Cells(cells),
        Ok(super::tables_tatr::TatrAttempt::Declined(declined)) => {
            TatrVerdict::Fallback(match declined {
                super::tables_tatr::TatrDecline::FewRowsOrColumns { .. } => "few-rows-or-columns",
                super::tables_tatr::TatrDecline::LowConfidence { .. } => "low-confidence",
                super::tables_tatr::TatrDecline::LowCoverage { .. } => "low-coverage",
            })
        }
        Err(failed) => TatrVerdict::Fallback(match failed.stage {
            super::tables_tatr::TatrFailure::Render => "render-failed",
            super::tables_tatr::TatrFailure::Inference => "inference-failed",
            super::tables_tatr::TatrFailure::ModelMissing => "model-missing",
            super::tables_tatr::TatrFailure::ModelInvalid => "model-invalid",
        }),
    };
    let outcome = match &verdict {
        TatrVerdict::Cells(_) => TatrOutcome::Fired,
        TatrVerdict::Fallback(reason) => TatrOutcome::Fallback(reason),
    };
    tatr_trace(page_number, region, outcome, started);
    verdict
}

/// One region rebuilt as the learned table pass emits it: the decoder's grid
/// over the region lines' words (or the drawn grid where the page drew one),
/// the spaced header joined, key-value sections split, cells made and tidied.
///
/// ONE COMPUTATION for the pass and for a teacher box. [`refine_judged_traced`]
/// calls this for each of its regions, and `table_in_region` for the lines a
/// box holds; the two cannot drift because there is only one. `page_blocks`
/// are the page's blocks in page order, `lines` its judge lines and `words`
/// its page words, `rules` the rulings the region holds, `region_lines` the
/// lines to build from, `drawn` the lattice grid where one claimed the
/// region, and `switches` the table switches the rebuild reads
/// (`table_corridor_split`, `table_cell_text_by_line`,
/// `table_runs_on_figures`). Empty where the lines hold no grid.
///
/// `tatr` offers the region to TATR first (track B, P3): where it fires its
/// cells plus the region words no cell claimed -- re-emitted as the blocks
/// they came from -- are the rebuild, and where it declines, fires under
/// coverage, or the region fails, the grid below runs unchanged. `None` --
/// TATR not asked for -- runs the grid untouched, byte for byte as without
/// the hook.
#[allow(clippy::too_many_arguments)]
pub(crate) fn rebuild_region(
    page_blocks: &[&Block],
    lines: &[judge::Line],
    words: &[PageWord],
    rules: &[TextRuling],
    region_lines: &[usize],
    drawn: Option<&DrawnGrid>,
    switches: super::Switches,
    tatr: Option<&TatrRegion>,
    cell_path_words: bool,
) -> Vec<Block> {
    let valid: Vec<usize> = region_lines
        .iter()
        .copied()
        .filter(|&i| i < lines.len())
        .collect();
    if valid.is_empty() {
        return Vec::new();
    }
    let (pair_words, from) = words_of_region(page_blocks, lines, words, &valid);
    if pair_words.is_empty() {
        return Vec::new();
    }
    let page_number = valid
        .iter()
        .filter_map(|&i| lines.get(i))
        .filter_map(|line| page_blocks.get(line.block))
        .map(|block| block.page)
        .next()
        .unwrap_or(0);
    // TATR FIRST, past the shared preconditions: an empty region holds no
    // structure for any structurer, and the grid below is the fallback the
    // hook returns to.
    if let TatrVerdict::Cells(mut cells) = tatr_for_region_with_cell_path_words(
        tatr,
        page_blocks,
        lines,
        words,
        region_lines,
        &pair_words,
        page_number,
        cell_path_words,
    ) {
        // TATR's own stamp: the grid below never ran for these cells.
        for cell in cells.iter_mut() {
            if matches!(cell.kind, BlockKind::TableCell { .. }) {
                cell.table_origin = TableOrigin::Tatr;
            }
        }
        return cells;
    }
    let mut region_blocks: Vec<usize> = valid
        .iter()
        .filter_map(|&i| lines.get(i).map(|line| line.block))
        .filter(|&b| b < page_blocks.len())
        .collect();
    region_blocks.sort_unstable();
    region_blocks.dedup();
    let weak = region_blocks
        .iter()
        .find_map(|&b| match page_blocks[b].kind {
            BlockKind::TableCell { .. } => page_blocks[b].grid,
            _ => None,
        });
    let sep_rules: Vec<separations::Ruling> = rules
        .iter()
        .filter(|r| (r.y1 - r.y0).abs() < 1e-6)
        .map(|r| separations::Ruling { y: r.y0 })
        .collect();
    // A DRAWN GRID READS ITS OWN WORDS: its cells index the raw fragments
    // `lattice_runs` placed, and the region's joined words number shorter.
    // TATR above already saw the joined list; the grid below reads the
    // fragments, whose text rejoins on the way out.
    let (grid, is_drawn, pair_words, from) = match drawn {
        Some(drawn) => (
            drawn.grid.clone(),
            true,
            drawn.words.clone(),
            drawn.from.clone(),
        ),
        None => match lattice_grid_of(rules, &pair_words) {
            Some(grid) => (grid, true, pair_words, from),
            None => match grid_of(&pair_words, &sep_rules, switches.table_corridor_split) {
                Some(grid) => (
                    split_twin_figure_columns(spaced_header_joined(grid, &pair_words), &pair_words),
                    false,
                    pair_words,
                    from,
                ),
                None => return Vec::new(),
            },
        },
    };
    // ONLY A DECODED GRID EVER SPLITS: never a lattice grid, and never the
    // builder's own table.
    let split_grid = (!is_drawn).then(|| key_value_sections_split(&grid, &pair_words));
    let split_grid = split_grid.flatten();
    let pieces = split_grid.unwrap_or_else(|| {
        vec![SplitPiece::Table {
            cells: grid.cells.clone(),
            header_rows: grid.header_rows,
        }]
    });
    let mut made: Vec<Block> = Vec::new();
    for piece in &pieces {
        match piece {
            SplitPiece::Table { cells, header_rows } => {
                let has_header = table_has_header(cells, *header_rows, &from, page_blocks);
                let mut table: Vec<Block> = cells
                    .iter()
                    .map(|cell| {
                        cell_block_with_cell_path_words(
                            cell,
                            &pair_words,
                            &from,
                            page_blocks,
                            weak,
                            page_number,
                            has_header && cell.rows.first().is_some_and(|&r| r < *header_rows),
                            None,
                            switches.table_cell_text_by_line,
                            cell_path_words,
                        )
                    })
                    .collect();
                // TIDIED ONE TABLE AT A TIME: `cell_merge::tidy` renumbers the
                // rows it is given as one table, and the pieces of a split grid
                // each number theirs from 0 -- tidied together, the crystal page
                // came back as nine tables.
                super::cell_merge::tidy(&mut table, is_drawn, switches.table_runs_on_figures);
                made.extend(table);
            }
            SplitPiece::Section { row } => {
                let Some(cell) = grid.cells.iter().find(|cell| cell.rows == [*row]) else {
                    continue;
                };
                made.push(section_block(
                    cell,
                    &pair_words,
                    &from,
                    page_blocks,
                    page_number,
                ));
            }
        }
    }
    made
}

/// The words TATR's fill reads: the engine's own words, not raw page words.
///
/// A PDF that sets text glyph by glyph -- letter-spacing, per-glyph show
/// operators, a rotated table -- reaches [`words_of`] as one word per
/// fragment, and the fill joins those with spaces (`C o n c`), where the
/// engine's own cells read the same fragments joined (`Conc`). So the fill
/// reads what the engine read: each block's fragments are aligned onto its
/// [`Block::text`] tokens -- the line assembly's joined words -- and the
/// fragments of one token become one word, the token's text at the union of
/// their boxes.
///
/// Two guards keep the assembly's spacing judgments out of the fill. The
/// alignment is verified: where a block's fragments do not spell its text
/// -- right-to-left ink (the text is reordered, the fragments are in draw
/// order), an overprint the assembly trimmed, a word the typesetter broke
/// across a line -- the block keeps its raw fragments, exactly as today. A
/// block past the page's blocks keeps them too. And a token joins only
/// where its fragments touch: kerning and stacked glyph runs abut or
/// overlap, while a thin space the engine glued shows daylight, and that
/// token stays raw -- `20 (76.9%)` keeps its space, `[363]` loses its. Every
/// page word stands inside exactly one fill word either way, so a fire
/// still loses no word; claims, the coverage guard and the gap fill run
/// over these words exactly as over the raw ones. The decode's tokens stay
/// the raw page words.
///
/// [`words_of`]: super::features::words_of
#[cfg(test)]
pub(crate) fn tatr_fill_words(words: &[PageWord], page_blocks: &[&Block]) -> Vec<PageWord> {
    tatr_fill_words_with_cell_path_words(words, page_blocks, false)
}

/// [`tatr_fill_words`] with the cell-path fill where `cell_path_words` is
/// true: fragments aligned onto text tokens in both directions, fragments
/// that joined what the text splits as well as fragments that split what it
/// joins.
pub(crate) fn tatr_fill_words_with_cell_path_words(
    words: &[PageWord],
    page_blocks: &[&Block],
    cell_path_words: bool,
) -> Vec<PageWord> {
    let mut out: Vec<PageWord> = Vec::with_capacity(words.len());
    let mut start = 0usize;
    while start < words.len() {
        let block = words[start].block;
        let mut end = start + 1;
        while end < words.len() && words[end].block == block {
            end += 1;
        }
        if !join_block_words(&words[start..end], page_blocks, &mut out, cell_path_words) {
            out.extend(words[start..end].iter().cloned());
        }
        start = end;
    }
    out
}

/// One block's fragments aligned onto its text tokens, appended to `out`:
/// `false` where they do not spell it and the caller keeps them raw. Either
/// side may run ahead: fragments split what the text joins, or joined what
/// it splits, and the short side grows until both spell the same run.
///
/// That is the alignment with `cell_path_words`; without it only fragments
/// that split what the text joins align, exactly as before.
fn join_block_words(
    fragments: &[PageWord],
    page_blocks: &[&Block],
    out: &mut Vec<PageWord>,
    cell_path_words: bool,
) -> bool {
    let Some(block) = fragments
        .first()
        .and_then(|first| page_blocks.get(first.block))
    else {
        return false;
    };
    let tokens: Vec<&str> = block.text.split_whitespace().collect();
    if tokens.is_empty() {
        return false;
    }
    let mut built: Vec<PageWord> = Vec::with_capacity(tokens.len());
    let mut taken = 0usize;
    // One token's fragments as one fill word at the union box, carrying the
    // first fragment's seat.
    let emit = |built: &mut Vec<PageWord>, token: String, span: &[PageWord]| -> bool {
        // Daylight between two fragments of one token is a space the
        // engine glued: this token stays raw, the rest still join.
        if !span
            .windows(2)
            .all(|pair| fragments_touch(&pair[0], &pair[1]))
        {
            built.extend(span.iter().cloned());
            return true;
        }
        let Some(first) = span.first() else {
            return false;
        };
        let mut bbox = first.bbox;
        for fragment in &span[1..] {
            bbox = [
                bbox[0].min(fragment.bbox[0]),
                bbox[1].min(fragment.bbox[1]),
                bbox[2].max(fragment.bbox[2]),
                bbox[3].max(fragment.bbox[3]),
            ];
        }
        built.push(PageWord {
            text: token,
            bbox,
            block: first.block,
            span: first.span,
            size: first.size,
            bold: first.bold,
            italic: first.italic,
            font: first.font.clone(),
            color: first.color,
        });
        true
    };
    if cell_path_words {
        let mut ti = 0usize;
        // Both directions: fragments joined what the text splits -- a `1.1.1`
        // the page words read whole against tokens `1`, `.`, `1`, `.`, `1` --
        // as well as split what it joins. The short side grows until the two
        // spell the same run of characters or one side runs out.
        while ti < tokens.len() {
            let from = taken;
            let mut want: Vec<char> = tokens[ti].chars().collect();
            ti += 1;
            let mut got: Vec<char> = Vec::new();
            loop {
                while got.len() < want.len() {
                    let Some(head) = fragments.get(taken) else {
                        return false;
                    };
                    got.extend(head.text.chars());
                    taken += 1;
                }
                if got.len() == want.len() {
                    break;
                }
                let Some(tok) = tokens.get(ti) else {
                    return false;
                };
                want.extend(tok.chars());
                ti += 1;
            }
            if got != want {
                return false;
            }
            let token: String = want.iter().collect();
            if !emit(&mut built, token, &fragments[from..taken]) {
                return false;
            }
        }
    } else {
        for token in tokens {
            let want: Vec<char> = token.chars().collect();
            let mut got: Vec<char> = Vec::with_capacity(want.len());
            let from = taken;
            while got.len() < want.len() {
                let Some(head) = fragments.get(taken) else {
                    return false;
                };
                got.extend(head.text.chars());
                taken += 1;
            }
            if got != want {
                return false;
            }
            if !emit(&mut built, token.to_string(), &fragments[from..taken]) {
                return false;
            }
        }
    }
    if taken != fragments.len() {
        return false;
    }
    out.extend(built);
    true
}

/// Whether two fragments of one engine token touch: no daylight between
/// their boxes along the axis they share. A stacked run's boxes overlap by
/// half an em and kerned pairs abut; anything past a hair is a space.
fn fragments_touch(prev: &PageWord, next: &PageWord) -> bool {
    let share = |a0: f64, a1: f64, b0: f64, b1: f64| {
        let shared = a1.min(b1) - a0.max(b0);
        if shared <= 0.0 {
            return 0.0;
        }
        let smallest = (a1 - a0).min(b1 - b0);
        if smallest <= 0.0 {
            return 0.0;
        }
        shared / smallest
    };
    // A hair of slack for float noise: touching boxes abut exactly, and
    // the thinnest real space is four orders past this.
    const TOUCH_EPS: f64 = 1e-6;
    if share(prev.bbox[1], prev.bbox[3], next.bbox[1], next.bbox[3]) > 0.5 {
        return next.bbox[0] - prev.bbox[2] <= TOUCH_EPS;
    }
    if share(prev.bbox[0], prev.bbox[2], next.bbox[0], next.bbox[2]) > 0.5 {
        return next.bbox[1] - prev.bbox[3] <= TOUCH_EPS;
    }
    false
}

/// The words the region lines hold, in page order, and for each the block it
/// came from: the same words [`words_of`] reads for the same blocks, so a run
/// rebuilt through its lines comes back as it did through its blocks.
pub(crate) fn words_of_region(
    page_blocks: &[&Block],
    lines: &[judge::Line],
    words: &[PageWord],
    region_lines: &[usize],
) -> (Vec<pairs::PairWord>, Vec<usize>) {
    let mut in_region = vec![false; lines.len()];
    for &i in region_lines {
        if let Some(slot) = in_region.get_mut(i) {
            *slot = true;
        }
    }
    if let Some(line_of) = line_of_each_word(page_blocks.len(), lines, words) {
        let mut pair_words = Vec::new();
        let mut from = Vec::new();
        for (index, word) in words.iter().enumerate() {
            let held = line_of
                .get(index)
                .copied()
                .flatten()
                .is_some_and(|line| in_region[line]);
            if !held || word.block >= page_blocks.len() {
                continue;
            }
            let (table, row, column) = match page_blocks[word.block].kind {
                BlockKind::TableCell { row, column, .. } => (Some(0u32), Some(row), Some(column)),
                _ => (None, None, None),
            };
            pair_words.push(pairs::PairWord {
                text: word.text.clone(),
                bbox: word.bbox,
                table,
                row,
                column,
            });
            from.push(word.block);
        }
        return (pair_words, from);
    }
    // The lines were not built from these words: fall back to every word of
    // the region's blocks, which is the run's own reading where the region
    // holds whole blocks.
    let mut blocks: Vec<usize> = region_lines
        .iter()
        .filter_map(|&i| lines.get(i).map(|line| line.block))
        .filter(|&b| b < page_blocks.len())
        .collect();
    blocks.sort_unstable();
    blocks.dedup();
    let mut pair_words = Vec::new();
    let mut from = Vec::new();
    for word in words.iter() {
        if !blocks.contains(&word.block) {
            continue;
        }
        let (table, row, column) = match page_blocks[word.block].kind {
            BlockKind::TableCell { row, column, .. } => (Some(0u32), Some(row), Some(column)),
            _ => (None, None, None),
        };
        pair_words.push(pairs::PairWord {
            text: word.text.clone(),
            bbox: word.bbox,
            table,
            row,
            column,
        });
        from.push(word.block);
    }
    (pair_words, from)
}

/// Which line each word sits on, where `lines` is what
/// [`judge::lines_of_page`] built from `words`: the grouping recomputed, so a
/// word lands on exactly the line the judge put it on. `None` where the two
/// were not built together.
fn line_of_each_word(
    n_blocks: usize,
    lines: &[judge::Line],
    words: &[PageWord],
) -> Option<Vec<Option<usize>>> {
    let mut out: Vec<Option<usize>> = vec![None; words.len()];
    let base = words.as_ptr() as usize;
    let width = std::mem::size_of::<PageWord>().max(1);
    let mut at = 0usize;
    for block in 0..n_blocks {
        for piece in lines::lines_of(block, words) {
            if piece.is_empty() {
                continue;
            }
            let line = lines.get(at)?;
            if line.block != block {
                return None;
            }
            for word in piece {
                let offset = (word as *const PageWord as usize).wrapping_sub(base) / width;
                if offset >= words.len() || !std::ptr::eq(&words[offset], word) {
                    return None;
                }
                out[offset] = Some(at);
            }
            at += 1;
        }
    }
    (at == lines.len()).then_some(out)
}

/// The lattice grid the region's rulings draw over its words, if one holds
/// them: the same grid [`lattice_runs`] reads, scoped to one region.
fn lattice_grid_of(rules: &[TextRuling], words: &[pairs::PairWord]) -> Option<separations::Grid> {
    if rules.is_empty() || words.len() < 4 {
        return None;
    }
    for lattice in lattice::lattices(rules) {
        let atoms = lattice.cells();
        let mut held: Vec<Vec<usize>> = vec![Vec::new(); atoms.len()];
        let mut placed = 0usize;
        for (w, word) in words.iter().enumerate() {
            let (x, y) = (
                (word.bbox[0] + word.bbox[2]) * 0.5,
                (word.bbox[1] + word.bbox[3]) * 0.5,
            );
            if let Some(c) = lattice.cell_at(&atoms, x, y) {
                held[c].push(w);
                placed += 1;
            }
        }
        if (placed as f64) < LATTICE_MIN_PLACED * words.len() as f64 {
            continue;
        }
        let mut cells: Vec<super::grid::Cell> = Vec::new();
        for (c, mut mine) in held.into_iter().enumerate() {
            if mine.is_empty() {
                continue;
            }
            mine = in_reading_order(mine, words);
            let bbox = [
                mine.iter()
                    .map(|&w| words[w].bbox[0])
                    .fold(f64::INFINITY, f64::min),
                mine.iter()
                    .map(|&w| words[w].bbox[1])
                    .fold(f64::INFINITY, f64::min),
                mine.iter()
                    .map(|&w| words[w].bbox[2])
                    .fold(f64::NEG_INFINITY, f64::max),
                mine.iter()
                    .map(|&w| words[w].bbox[3])
                    .fold(f64::NEG_INFINITY, f64::max),
            ];
            let (rows, columns) = atoms[c].clone();
            cells.push(super::grid::Cell {
                words: mine,
                bbox,
                rows: rows.collect(),
                columns: columns.collect(),
            });
        }
        let rows: std::collections::BTreeSet<usize> = cells
            .iter()
            .filter_map(|c| c.rows.first().copied())
            .collect();
        let columns: std::collections::BTreeSet<usize> = cells
            .iter()
            .filter_map(|c| c.columns.first().copied())
            .collect();
        if rows.len() < 2 || columns.len() < 2 {
            continue;
        }
        // A DRAWN CELL THAT HOLDS SEVERAL LINES ACROSS SEVERAL OF THE GRID'S
        // POSITIONS IS A GRID WHOSE INNER LINES WERE NOT DRAWN, and not one
        // merged cell: the region is still the grid's, and its cells are the
        // decoder's.
        let undrawn = cells.iter().any(|cell| {
            (cell.rows.len() > 1 || cell.columns.len() > 1) && lines_in(&cell.words, words) > 1
        });
        if undrawn {
            continue;
        }
        return Some(separations::Grid {
            cells,
            header_rows: 1,
        });
    }
    None
}

/// A rebuild's table pieces as the signals read them: one span per table in
/// `made`, split where a section row stands and where the coordinates start
/// over, with whether a key-value split made more than one.
fn table_pieces_of(made: &[Block]) -> (bool, Vec<std::ops::Range<usize>>) {
    let mut spans: Vec<std::ops::Range<usize>> = Vec::new();
    let mut at = 0usize;
    while at < made.len() {
        let position = |block: &Block| match block.kind {
            BlockKind::TableCell { row, column, .. } => Some((row, column)),
            _ => None,
        };
        let Some(first) = position(&made[at]) else {
            at += 1;
            continue;
        };
        let start = at;
        let mut previous = first;
        at += 1;
        while at < made.len() {
            let Some(here) = position(&made[at]) else {
                break;
            };
            // Row-major order climbs; the coordinates starting over are the
            // next table, where a skipped section row left two tables beside
            // each other.
            if here <= previous {
                break;
            }
            previous = here;
            at += 1;
        }
        spans.push(start..at);
    }
    (spans.len() > 1, spans)
}

/// [`refine_gated`], and where the judge put the page's tables, if it ran.
///
/// WITH A JUDGE, THE JUDGE SAYS WHERE THE TABLES ARE. Its regions replace the
/// region model's and the builder's runs; a region that is exactly one of the
/// builder's tables still goes through the hybrid gate, and any other -- a
/// table the builder cut in two, missed, or stopped early -- is rebuilt by
/// the separation decoder. A builder table the judge chose no part of, and
/// that the builder itself doubted, is read back as one paragraph per row. A
/// judged region never cuts a builder table: it takes in every table it
/// touches ([`grown`]).
#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub fn refine_judged(
    page: Vec<Block>,
    size: (f64, f64),
    facts: &super::DocumentFacts,
    switches: super::Switches,
    weakness: &Builder<'_>,
    judged: Option<&[Option<usize>]>,
    lattices: &[super::lattice::Lattice],
    cells: &[CellHint],
) -> (Vec<Block>, Vec<usize>) {
    let (blocks, origin, _) = refine_judged_traced(
        page, size, facts, switches, weakness, judged, None, lattices, cells, false, true,
    );
    (blocks, origin)
}

/// Every table run of the pass's output with its empty columns dropped
/// (#574): rebuilt grids and kept builder tables alike, in place, each run
/// read the way [`crate::markdown::table_run`] reads one. Returns for each
/// old position the new one -- `None` for a cell that went with its column
/// -- so the signals' block ranges follow the blocks. The origin map is
/// spliced with the blocks, so a cell that goes takes its origin with it.
///
/// ALL OR NOTHING PER PAGE. Renumbering a run's columns cannot split it --
/// rows stay and distinct columns stay distinct in order -- but where two
/// runs stand adjacent on one row, the lower numbers could read as one run
/// climbing on. Where the runs after the drop are not the runs before it,
/// the page keeps its grid: the drop removes columns, never tables.
fn drop_empty_columns_from_tables(
    out: &mut Vec<Block>,
    origin: &mut Vec<usize>,
) -> Vec<Option<usize>> {
    let saved_blocks = out.clone();
    let saved_origin = origin.clone();
    let mut map: Vec<Option<usize>> = vec![None; out.len()];
    // Which pre-drop run each old position stood in; non-cells never join a
    // run, so they need none.
    let mut run_of: Vec<usize> = vec![usize::MAX; out.len()];
    let mut runs = 0usize;
    let mut old = 0usize;
    let mut i = 0usize;
    while i < out.len() {
        let Some(end) = crate::markdown::table_run(out, i) else {
            map[old] = Some(i);
            old += 1;
            i += 1;
            continue;
        };
        let run = end - i;
        for slot in run_of.iter_mut().skip(old).take(run) {
            *slot = runs;
        }
        runs += 1;
        let mut pairs: Vec<(Block, (usize, usize))> = out
            .drain(i..end)
            .zip(origin.drain(i..end))
            .zip(old..old + run)
            .map(|((block, from), o)| (block, (from, o)))
            .collect();
        old += run;
        super::cell_merge::drop_empty_columns(&mut pairs);
        let mut back: Vec<Block> = Vec::with_capacity(pairs.len());
        let mut back_from: Vec<usize> = Vec::with_capacity(pairs.len());
        for (block, (from, o)) in pairs {
            map[o] = Some(i + back.len());
            back.push(block);
            back_from.push(from);
        }
        let n = back.len();
        out.splice(i..i, back);
        origin.splice(i..i, back_from);
        i += n;
    }
    // Every run after the drop stands in exactly one run from before it, or
    // the page keeps its grid.
    let mut new_to_old = vec![0usize; out.len()];
    for (o, n) in map.iter().enumerate() {
        if let Some(n) = n {
            new_to_old[*n] = o;
        }
    }
    let mut i = 0usize;
    while i < out.len() {
        let Some(end) = crate::markdown::table_run(out, i) else {
            i += 1;
            continue;
        };
        let mut stood = std::collections::BTreeSet::new();
        for n in i..end {
            stood.insert(run_of[new_to_old[n]]);
        }
        if stood.len() > 1 {
            *out = saved_blocks;
            *origin = saved_origin;
            return (0..map.len()).map(Some).collect();
        }
        i = end;
    }
    map
}

/// [`refine_judged`], with the table signals beside the blocks: one
/// computation, so the signals cannot drift from the decision. `judge_trace`
/// is the judge's numbers for `judged` (from `judge_regions_traced`);
/// `drawn_flag` is whether the page has rulings the table pass read.
#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub fn refine_judged_traced(
    page: Vec<Block>,
    size: (f64, f64),
    facts: &super::DocumentFacts,
    switches: super::Switches,
    weakness: &Builder<'_>,
    judged: Option<&[Option<usize>]>,
    judge_trace: Option<&JudgeTrace>,
    lattices: &[super::lattice::Lattice],
    cells: &[CellHint],
    drawn_flag: bool,
    cell_path_words: bool,
) -> (Vec<Block>, Vec<usize>, Option<PageTableSignals>) {
    refine_judged_with_tatr(
        page,
        size,
        facts,
        switches,
        weakness,
        judged,
        judge_trace,
        lattices,
        cells,
        drawn_flag,
        None,
        cell_path_words,
    )
}

/// [`refine_judged_traced`], with TATR table structure for the pass's regions
/// (track B, P3). `tatr` is the extraction's context; `None` rebuilds every
/// region from the grid, exactly as without the hook. The pass's regions are
/// its own -- never detector boxes -- so `detector_regions_only` skips them
/// all, and the teacher entry below is where that switch lets TATR act.
#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub fn refine_judged_with_tatr(
    page: Vec<Block>,
    size: (f64, f64),
    facts: &super::DocumentFacts,
    switches: super::Switches,
    weakness: &Builder<'_>,
    judged: Option<&[Option<usize>]>,
    judge_trace: Option<&JudgeTrace>,
    lattices: &[super::lattice::Lattice],
    cells: &[CellHint],
    drawn_flag: bool,
    tatr: Option<&TatrContext>,
    cell_path_words: bool,
) -> (Vec<Block>, Vec<usize>, Option<PageTableSignals>) {
    let identity = |n: usize| (0..n).collect::<Vec<usize>>();
    if !switches.table_region
        && !switches.table_structure
        && judged.is_none()
        && lattices.is_empty()
    {
        let n = page.len();
        return (page, identity(n), None);
    }
    let refs: Vec<&Block> = page.iter().collect();
    // A BUILDER TABLE THAT IS NOT A TABLE GOES BACK TO PROSE (#638): the
    // table-or-not model reads each of the builder's tables whole, with the
    // heading decisions the pass took on its cells, and a table it refuses is
    // rejected as if the judge had -- no region rebuilds it, and
    // `demote_unchosen` sends it back, cell by cell where that switch is on.
    //
    // WITH THE FINAL-TABLE SWITCH ON, V1 DOES NOT RUN: v2 replaces it for
    // every final table, after the cells are decoded, and v1 stays for the
    // switch off. The judged mapping below is then the judge's own.
    let (refused, is_trace): (Option<Vec<Option<usize>>>, TableIsTrace) = match judged {
        Some(judged) if !switches.table_is_v2 => {
            let (regions, trace) = refused_by_the_table_model_traced(&refs, judged, cells, size.1);
            (Some(regions), trace)
        }
        Some(judged) => (Some(judged.to_vec()), TableIsTrace::default()),
        None => (None, TableIsTrace::default()),
    };
    let judged = refused.as_deref();
    let mut features = page_features(&refs, size.0, size.1);
    // REPEATS OR THE MODEL READS A HOLE. `repeats_in_document` is the one
    // feature about the whole document, and `page_features` cannot know it --
    // it is filled from `DocumentFacts`, as `page_vectors` does for the list
    // models. The region model was TRAINED on a dump where it is a real count,
    // so leaving it missing here would feed the model an input it never saw in
    // training: the exact defect this ticket spent its day on, one lane over.
    let texts: Vec<String> = refs.iter().map(|b| b.text.clone()).collect();
    facts.fill(&mut features, &texts);
    let sizes: Vec<f64> = features
        .blocks
        .iter()
        .map(|b| if b.size > 0.0 { b.size } else { 10.0 })
        .collect();

    // WHERE THE TABLES ARE. With the region model, what it says; without it,
    // the runs of cells the heuristics already found -- so the structure model
    // can be measured on its own.
    let inside: Vec<bool> = if judged.is_some() {
        vec![false; refs.len()]
    } else if switches.table_region {
        (0..refs.len())
            .map(|i| {
                let x = region::vector(&features.blocks, &refs, i);
                super::probability(models::table_region::predict_margin(&x)) >= REGION_THRESHOLD
            })
            .collect()
    } else {
        refs.iter()
            .map(|b| matches!(b.kind, BlockKind::TableCell { .. }))
            .collect()
    };
    let regions = match judged {
        Some(judged) => judged_runs(judged, &refs),
        None => runs(&refs, &inside, &sizes),
    };
    // WHERE THE PAGE DRAWS THE GRID, THE GRID IS THE TABLE. A lattice region
    // takes its blocks from any other region, and its cells come from its
    // lines rather than from a decoder.
    let drawn = lattice_runs(&refs, lattices);
    let taken: Vec<usize> = drawn
        .iter()
        .flat_map(|(run, _)| run.iter().copied())
        .collect();
    // (run, whether a drawn grid claimed it, the grid's own cells if its
    // lines make them)
    let regions: Vec<(Vec<usize>, bool, Option<DrawnGrid>)> = drawn
        .into_iter()
        .map(|(run, grid)| (run, true, grid))
        .chain(regions.into_iter().filter_map(|run| {
            let run: Vec<usize> = run.into_iter().filter(|i| !taken.contains(i)).collect();
            (!run.is_empty()).then_some((run, false, None))
        }))
        .collect();
    if judged.is_none() && taken.is_empty() && (regions.is_empty() || !switches.table_structure) {
        let n = page.len();
        return (page, identity(n), None);
    }

    // ONE SNAPSHOT, as everywhere in this pass: every region is decided from
    // the blocks as they came in, and only then applied.
    let mut replacement: Vec<Option<Vec<Block>>> = (0..page.len()).map(|_| None).collect();
    let tables = heuristic_tables(&refs);
    // One entry per table piece emitted: where its run stood in the input,
    // the run's numbers, and the piece's span inside the run's `made`.
    type PendingSignal = (
        usize,
        (
            f64,
            f64,
            Option<f64>,
            Option<f64>,
            Option<IsTableModel>,
            TableSplit,
        ),
        std::ops::Range<usize>,
    );
    let mut signals: Vec<PendingSignal> = Vec::new();
    let page_lines = judge::lines_of_page(&refs, &features.words, &[]);
    for (run, lattice, drawn) in &regions {
        // A JUDGED REGION THAT IS NOT EXACTLY ONE OF THE BUILDER'S TABLES is
        // rebuilt whatever the builder's record says: the judge chose it over
        // the builder's own reading, which is the point of asking it. A drawn
        // grid always is.
        let forced = *lattice || (judged.is_some() && !is_one_table(run, &tables));
        // A RUN THAT HOLDS PART OF A TABLE MAY NOT REBUILD THAT PART (#574).
        // The run breaks at the region model's first "no" and at a gap of
        // three lines, and a financial statement has both inside it: the
        // white between its groups and a label row the model doubts. The
        // grid decoded from the part then replaced those cells and left the
        // rest of the builder's table standing beside it, so one table came
        // out as several -- the defect the tables axis loses most to. Whole
        // tables only; the builder keeps any table a run cuts.
        if !forced && cuts_a_table(run, &tables) {
            continue;
        }
        // THE HYBRID GATE. The heuristic builder leads; the model overrules it
        // only where the builder itself was short of evidence. A run with no
        // record at all is one the builder never made into a table, and `None`
        // means it said nothing rather than that it was sure -- so it is left
        // alone too.
        let weak = match weakness(run, &refs) {
            Some(weak) if may_overrule(&weak) => Some(weak),
            recorded if forced => recorded,
            _ => continue,
        };
        // THE SAME REBUILD A TEACHER BOX ASKS FOR: the run through its lines,
        // so the pass and the region entry cannot drift.
        let mut in_run = vec![false; refs.len()];
        for &i in run {
            if let Some(slot) = in_run.get_mut(i) {
                *slot = true;
            }
        }
        let region_lines: Vec<usize> = page_lines
            .iter()
            .enumerate()
            .filter(|(_, line)| in_run.get(line.block).copied().unwrap_or(false))
            .map(|(i, _)| i)
            .collect();
        // The pass's own region, never a detector box: TATR is offered it
        // unless `detector_regions_only` says detector boxes only.
        let region_tatr = tatr.map(|context| TatrRegion::of(context, false));
        let mut made = rebuild_region(
            &refs,
            &page_lines,
            &features.words,
            &[],
            &region_lines,
            drawn.as_ref(),
            switches,
            region_tatr.as_ref(),
            cell_path_words,
        );
        if made.is_empty() {
            continue;
        }
        // GLUED TABLES COME APART HERE, on the final grid (#574): a rebuilt
        // region holding two statements side by side emits two tables, and
        // `table_pieces_of` below reads each piece's span from the
        // renumbered coordinates, so the signals follow without further
        // work. Switched off it returns the blocks untouched. The stacked
        // split is not this call: it acts after the final-table judge, on
        // the tables v2 kept -- see `split_stacked_after_v2` -- so a glued
        // table v2 refuses goes back to prose whole instead of passing as
        // one part.
        made = super::table_split::split_tables(made, switches.table_split_side_by_side, false);
        // THE JUDGE'S NUMBERS FOR THIS RUN, where the judge drew it: the
        // primary region's candidate. A lattice grid and a region-model run
        // have none, and keep `None`.
        let choice: Option<&super::verifier::Choice> = match (judged, judge_trace, *lattice) {
            (Some(judged), Some(trace), false) => {
                primary_choice(run, judged).and_then(|k| trace.details.get(k))
            }
            _ => None,
        };
        let confidence = choice.map(|c| c.p);
        // THE DECISION'S OWN NUMBERS, where the decision made this region a
        // table: the rebuild carries the builder's doubt and no probability,
        // and the pass stamps the record and the judge's number it gated on.
        for block in made.iter_mut() {
            if matches!(block.kind, BlockKind::TableCell { .. }) {
                block.grid = weak;
                block.confidence = confidence;
                // The trace's copy of the pass's numbers beside the stamp:
                // the judge probability the region went through, and why a
                // forced region was forced. Read-only for the trace; the
                // decisions above already ran.
                block.trace_learned = Some(LearnedTrace {
                    judge_p: confidence,
                    forced: if forced {
                        Some(if *lattice {
                            ForcedReason::Lattice
                        } else {
                            // Forced without a lattice is the
                            // judged-not-one-table arm, the only other arm
                            // that sets `forced` above.
                            ForcedReason::JudgedNotOneTable
                        })
                    } else {
                        None
                    },
                });
                // The rebuild's own stamp, unless TATR fired inside the
                // rebuild: those cells are TATR's, not the grid's.
                if block.table_origin != TableOrigin::Tatr {
                    block.table_origin = if forced {
                        TableOrigin::LearnedForced
                    } else {
                        TableOrigin::LearnedRefine
                    };
                }
            }
        }
        let (key_split, table_spans) = table_pieces_of(&made);
        let first = run[0];
        // One signal per table piece the run emits: a split grid emits several,
        // each with the run's numbers and the split marked. The span is inside
        // `made`; the page indices follow once `out` is assembled.
        if let Some(detail) = choice {
            let p_is_table = builder_prob(run, &tables, &is_trace.probs);
            // V1's number where v1 ran (v2 off) and spoke; with v2 on the
            // builder model does not run, so this is `None` here and the
            // v2 post-pass fills the signal from its own verdict instead.
            let p_is_table_model = p_is_table.map(|_| IsTableModel::V1);
            let split = split_of(detail.title_removed, key_split);
            for span in table_spans {
                if span.is_empty() {
                    continue;
                }
                signals.push((
                    first,
                    (
                        detail.p,
                        detail.score,
                        detail.margin,
                        p_is_table,
                        p_is_table_model,
                        split,
                    ),
                    span,
                ));
            }
        }
        // The grid stands where the region's FIRST block stood, in reading
        // order; the rest of the region's blocks are consumed by it.
        replacement[first] = Some(made);
        for &i in &run[1..] {
            replacement[i] = Some(Vec::new());
        }
    }

    if let Some(judged) = judged {
        demote_unchosen(
            &refs,
            &tables,
            judged,
            weakness,
            cells,
            switches.demote_per_cell,
            &mut replacement,
        );
    }

    // GLUED TABLES THE BUILDER KEEPS come apart the same way (#574): a table
    // the pass never rebuilt -- the hybrid gate shut, the judge agreeing it
    // is one table -- may still hold two statements side by side. Each
    // maximal run of untouched cells splits like a rebuilt grid above, in
    // place, so the prefix sums below and the signals (which pass-through
    // tables carry none of) need no repair. Stacked splits after the
    // final-table judge instead (`split_stacked_after_v2`).
    if switches.table_split_side_by_side {
        let mut at = 0usize;
        while at < page.len() {
            let untouched =
                replacement[at].is_none() && matches!(page[at].kind, BlockKind::TableCell { .. });
            if !untouched {
                at += 1;
                continue;
            }
            let start = at;
            while at < page.len()
                && replacement[at].is_none()
                && matches!(page[at].kind, BlockKind::TableCell { .. })
            {
                at += 1;
            }
            let run: Vec<Block> = page[start..at].to_vec();
            replacement[start] = Some(super::table_split::split_tables(
                run,
                switches.table_split_side_by_side,
                false,
            ));
            for slot in replacement.iter_mut().take(at).skip(start + 1) {
                *slot = Some(Vec::new());
            }
        }
    }

    // Where each input block's replacement starts in `out`: the prefix sums
    // of the replacement lengths, so a run's `made` span maps to page indices.
    let mut out_start = vec![0usize; replacement.len()];
    let mut total = 0usize;
    for (i, slot) in replacement.iter().enumerate() {
        out_start[i] = total;
        total += slot.as_ref().map_or(1, Vec::len);
    }
    let mut out: Vec<Block> = Vec::with_capacity(page.len());
    let mut origin: Vec<usize> = Vec::with_capacity(page.len());
    for (i, block) in page.into_iter().enumerate() {
        match replacement[i].take() {
            Some(made) => {
                origin.extend(std::iter::repeat_n(i, made.len()));
                out.extend(made);
            }
            None => {
                origin.push(i);
                out.push(block);
            }
        }
    }
    // THE EMPTY COLUMNS GO LAST, over the pass's own output: rebuilt grids
    // and kept builder tables alike, so `extract_blocks`, the markdown and
    // the dump all read the same grid (#574). The signals' block ranges are
    // computed after it, through the positions it returns.
    let remap: Option<Vec<Option<usize>>> = if switches.table_drop_empty_columns {
        Some(drop_empty_columns_from_tables(&mut out, &mut origin))
    } else {
        None
    };
    for (i, block) in out.iter_mut().enumerate() {
        block.index = i;
    }
    // IN READING ORDER: the runs were lattice-first, so the signals are sorted
    // back to where their tables stand. Stable, so the pieces of one split
    // grid keep their order.
    signals.sort_by_key(|(first, _, _)| *first);
    let tables: Vec<TableSignal> = signals
        .into_iter()
        .map(
            |(
                first,
                (p_judge, verifier_score, verifier_margin, p_is_table, p_is_table_model, split),
                span,
            )| {
                let base = out_start.get(first).copied().unwrap_or(0);
                // Through the empty-column drop where it ran: the span's old
                // positions to the survivors', so the range still covers the
                // piece's cells. A piece always keeps cells -- the drop keeps
                // two columns -- so the empty range is unreachable.
                let block_range = match &remap {
                    None => base + span.start..base + span.end,
                    Some(map) => {
                        let new: Vec<usize> = (base + span.start..base + span.end)
                            .filter_map(|old| map.get(old).copied().flatten())
                            .collect();
                        match (new.first(), new.last()) {
                            (Some(lo), Some(hi)) => *lo..hi + 1,
                            _ => out.len()..out.len(),
                        }
                    }
                };
                let bbox = union_of(&out, block_range.clone());
                TableSignal {
                    p_judge,
                    verifier_score,
                    verifier_margin,
                    p_is_table,
                    p_is_table_model,
                    split,
                    bbox,
                    block_range,
                }
            },
        )
        .collect();
    let page_signals = match (judged, judge_trace) {
        (Some(_), Some(trace)) => Some(PageTableSignals {
            tables,
            p_best_rejected: trace.p_best_rejected,
            n_candidates: trace.n_candidates,
            refused: is_trace.refused,
            drawn: drawn_flag,
        }),
        _ => None,
    };
    (out, origin, page_signals)
}

/// The probability at which a gap becomes a line.
///
/// CHOSEN ON THE TRAINING HALF AND REPORTED ON THE HELD-OUT ONE, which is not
/// the same as picking the best of a sweep. Swept over the documents the
/// models were trained on, TEDS reads 0.7981 at 0.05, 0.7998 at 0.10 and
/// 0.7940 at 0.25: a peak, and 0.10 is it.
///
/// The first sweep was run on the half the figure is quoted from, where 0.15
/// looked best at 0.8004. That number is the sweep's and not the floor's, and
/// reporting it would have been tuning on the evaluation set.
///
/// Low because the two mistakes are not equal: a line the decoder misses
/// merges two rows into one cell, a line it invents splits one row in two,
/// and TEDS charges the merge more.
const SEPARATION_FLOOR: f64 = 0.1;

/// What the builder had to go on, as it records it: [`crate::GridDoubt`].
///
/// THE RECORD IS T2'S AND SO IS ITS SHAPE (#624, #1989). This lane asked for
/// four facts and a column count, argued against collapsing them into a score,
/// and got exactly that -- so there is no type of our own here, only a gate
/// written over theirs. Two consumers, two explicit predicates, able to
/// disagree visibly.
pub use crate::blocks::GridDoubt as Weakness;

/// Whether the model may overrule the heuristic grid for a table.
///
/// FITTED AGAINST THE GRID THE ENGINE ACTUALLY BUILDS (#600), which the first
/// fit was not. On 1,152 PubTables tables the gate opens on 89 of them, 7.7
/// per cent, and there:
///
/// ```text
/// the engine keeps it      TEDS 0.4312
/// the model rebuilds it    TEDS 0.5350
/// ```
///
/// So the record does find the tables the builder is worst at -- 0.4312 where
/// it opens against 0.5996 over all of them -- and the model is better by
/// 0.104 on exactly those. Over every table that is 0.5996 to 0.6068.
///
/// AND ALL 23 TESTS IN `tables_from_columns_and_spans.rs` STAY GREEN at this
/// width with both switches on. Ungated the learned grid turns 15 tests red,
/// and it reads 0.6073 on this corpus while doing so: the tests are the reason
/// the gate is narrow, and no average can see them.
///
/// WHAT THE FIRST FIT COMPARED, because the number it published was wrong and
/// somebody will find it in the history. `region_end_to_end.py` scored every
/// column through `teds_of`, and `teds_of` runs the LEARNED pair model -- so
/// the row labelled "the heuristics" was the heuristics' CELL SELECTION put
/// through the learned decoder, and the engine's own grid was never scored at
/// all. It read 0.5357 against 0.5701 where the gate opened; the shipped grid
/// reads 0.4312. Both arms had contained the thing being tested.
///
/// AND THE HARNESS HAD A DIFFERENT GATE. `--gate-thin-rows` still defaulted to
/// 1 after the thin-row clause was dropped from this function, so the harness
/// measured a three-condition rule: it opened on 34 tables where this one
/// opens on 89, and on that subset the model LOSES. One rule in two places is
/// only safe while the two agree.
///
/// TWO CONDITIONS AND NOT THREE. The thin-row clause opened on 0.7 per cent of
/// tables and changed the aggregate by a thousandth: a good description of a
/// doubtful table and far too rare to be a gate.
///
/// AND PUBTABLES CANNOT FIT THE WIDTH, which is worth knowing before anyone
/// re-fits it. Swept over every pair of thresholds on this corpus the best
/// aggregate is a much wider gate -- `fill` below 1.0 and at most 3 full rows,
/// opening on about a quarter of them -- and that width is chosen by a corpus
/// whose tables are clean (median fill 0.94, median full_rows 6) and scored by
/// a metric that cannot see the 23 tests. This width is the one that keeps
/// them green. A corpus with doubtful tables in it, which is the rig, is what
/// can say whether it should be wider.
pub fn may_overrule(weak: &Weakness) -> bool {
    weak.fill < 0.8 && weak.full_rows <= 2
}

/// The builder's record for the table a run of blocks came from.
///
/// EVERY CELL OF A TABLE CARRIES THE SAME RECORD, which is the contract
/// `GridDoubt` documents, so the first cell of the run answers for it. A run
/// with no cell at all -- one the region model assembled out of blocks the
/// heuristics never made into a table -- has no record, and `None` means the
/// builder said nothing rather than that it was confident.
///
/// A RUN CAN HOLD CELLS OF TWO TABLES where two grown regions met, so the
/// record taken is the FIRST cell's and not an average: averaging two tables'
/// evidence would describe neither, and the first cell is the one the run is
/// anchored on.
fn weakness_of(run: &[usize], page: &[&Block]) -> Option<Weakness> {
    run.iter()
        .filter_map(|&i| page.get(i))
        .find_map(|block| block.grid)
}

/// Words of one cell top line first, left to right.
///
/// A TOTAL ORDER, AND THE TOLERANCE OUTSIDE IT. The comparator this replaces
/// called two words one line when their tops were within half of the FIRST
/// word's height, and ordered them by x then: neither symmetric nor
/// transitive, and Rust's sort aborts on such a comparator -- the release
/// profile turns the panic into an abort, and 5 of the 791 DocLayNet
/// validation pages (2 PDFs, 28-09-2026) lost their whole page. So the words
/// are sorted by their top, which is a total order, cut into lines where the
/// next top drops by more than half a word's height, and each line is sorted
/// by its left edge.
///
/// The cell's words in the order its text reads them: [`in_reading_order`]
/// with a folded leader run after its label. The fold appends leader dots to
/// the label's text, so within a cell they trail the words they fill after,
/// while geometric order can read them mid-label. A period that ends a word
/// or a sentence is no leader dot and keeps its place (see
/// [`is_sentence_period`]). Text only: the grid keeps the geometric order,
/// which its splits are decided on (#636).
fn text_reading_order(mut mine: Vec<usize>, words: &[pairs::PairWord]) -> Vec<usize> {
    // The same lines [`in_reading_order`] cuts; only the within-line key
    // puts a folded leader run after its label, so dots never cross a line.
    mine.sort_by(|&a, &b| words[b].bbox[3].total_cmp(&words[a].bbox[3]));
    let mut lines: Vec<Vec<usize>> = Vec::new();
    let mut line: Vec<usize> = Vec::new();
    let mut line_top = f64::NAN;
    for w in mine {
        let top = words[w].bbox[3];
        let height = words[w].bbox[3] - words[w].bbox[1];
        if !line.is_empty() && (line_top - top).abs() > 0.5 * height {
            lines.push(std::mem::take(&mut line));
        }
        if line.is_empty() {
            line_top = top;
        }
        line.push(w);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    join_disputed_singletons(&mut lines, words);
    let mut out = Vec::with_capacity(lines.iter().map(Vec::len).sum());
    for line in &mut lines {
        // Geometric order first: whether a dot-only word is a leader is
        // decided against its own neighbours, so the line must read
        // geometrically before it is partitioned. Leaders trail in their own
        // geometric order; everything else -- and a sentence period -- keeps
        // its place, so no word space drops out of the reorder.
        line.sort_by(|&a, &b| words[a].bbox[0].total_cmp(&words[b].bbox[0]));
        let mut text: Vec<usize> = Vec::with_capacity(line.len());
        let mut leaders: Vec<usize> = Vec::new();
        for (i, &w) in line.iter().enumerate() {
            let prev = i.checked_sub(1).map(|k| &words[line[k]]);
            let next = line.get(i + 1).map(|&k| &words[k]);
            if is_dot_fill(&words[w]) && !is_sentence_period(&words[w], prev, next) {
                leaders.push(w);
            } else {
                text.push(w);
            }
        }
        text.append(&mut leaders);
        out.append(&mut text);
    }
    out
}

/// A word the half-height cut leaves on its own line joins the neighbouring
/// line its glyph box overlaps most, where the cut and the overlap disagree
/// (see [`text_reading_order`]).
///
/// The cut compares tops, so a word straddling two lines lands alone even
/// where most of its glyph box overlaps one of them: the f1040 `27a` marker
/// sits 4.26 past the `qualifying child,` top on an 8.0-tall box, yet shares
/// 4.37 of that height with the `you may need to` line below and only 2.74
/// with the line above, and reads mid-stub. The lone word joins the neighbour
/// it overlaps most, but only past half its own height -- the same bar the
/// cut sets -- so a word halfway between two lines keeps the cut's order, as
/// does one the extents merely touch. Only whole neighbouring lines qualify:
/// two lone words never merge with each other. [`in_reading_order`] is
/// untouched, so grid decisions never see this.
fn join_disputed_singletons(lines: &mut Vec<Vec<usize>>, words: &[pairs::PairWord]) {
    if lines.len() < 2 {
        return;
    }
    let extent = |line: &[usize]| {
        let mut low = f64::INFINITY;
        let mut high = f64::NEG_INFINITY;
        for &w in line {
            low = low.min(words[w].bbox[1]);
            high = high.max(words[w].bbox[3]);
        }
        (low, high)
    };
    let mut target: Vec<Option<usize>> = vec![None; lines.len()];
    for (i, line) in lines.iter().enumerate() {
        if line.len() != 1 {
            continue;
        }
        let w = line[0];
        let height = words[w].bbox[3] - words[w].bbox[1];
        if height <= 0.0 {
            continue;
        }
        let overlap = |j: usize| {
            let (low, high) = extent(&lines[j]);
            (words[w].bbox[3].min(high) - words[w].bbox[1].max(low)).max(0.0)
        };
        let prev = if i > 0 && lines[i - 1].len() > 1 {
            overlap(i - 1)
        } else {
            0.0
        };
        let next = if i + 1 < lines.len() && lines[i + 1].len() > 1 {
            overlap(i + 1)
        } else {
            0.0
        };
        if prev > next && prev > 0.5 * height {
            target[i] = Some(i - 1);
        } else if next > prev && next > 0.5 * height {
            target[i] = Some(i + 1);
        }
    }
    for (i, choices) in target.iter().enumerate() {
        if let Some(j) = choices {
            let w = lines[i].pop().expect("a singleton line holds one word");
            lines[*j].push(w);
        }
    }
    lines.retain(|line| !line.is_empty());
}

/// Whether a dot-only word is a sentence period rather than leader fill
/// (see [`text_reading_order`]): a single period that ends a word or a
/// sentence keeps its place, while a run of dots separated by leader spacing
/// still trails its label.
///
/// Abutting is the join's own boundary: a dot the shared gap rule would join
/// to the word before it ends that word -- `you.` in `refunded to you. If`
/// abuts `you` exactly -- while a leader stands off across white. Overstruck
/// ink is no abut: a dot printed over its word is two words, not one, so the
/// daylight may be dust but never overlap.
///
/// A period before a capitalised word after a normal word gap is a sentence
/// boundary the same way. The gap is a word gap, not leader spacing: word
/// spaces sit under one em even set wide, while a fill spreads past it, so a
/// dot out on leader fill never matches, whatever follows it. The drift bars
/// are the join's own, since all three words share one line by construction.
fn is_sentence_period(
    word: &pairs::PairWord,
    prev: Option<&pairs::PairWord>,
    next: Option<&pairs::PairWord>,
) -> bool {
    if word.text != "." {
        return false;
    }
    let tall = (word.bbox[3] - word.bbox[1]).max(f64::MIN_POSITIVE);
    // A member of a dot run is never a period, however it stands: folded
    // dots keep no glyph boxes of their own, so neighbours tile them with no
    // daylight between, and only the run around the dot says it is fill. The
    // bar is leader spacing, not abutment, so a period far ahead of a fill
    // still reads as one.
    for neighbour in [prev, next].into_iter().flatten() {
        if is_dot_fill(neighbour) {
            let em = tall.max((neighbour.bbox[3] - neighbour.bbox[1]).max(f64::MIN_POSITIVE));
            let gap = if neighbour.bbox[0] < word.bbox[0] {
                word.bbox[0] - neighbour.bbox[2]
            } else {
                neighbour.bbox[0] - word.bbox[2]
            };
            if gap <= 1.5 * em {
                return false;
            }
        }
    }
    if let Some(before) = prev {
        let em = tall.max((before.bbox[3] - before.bbox[1]).max(f64::MIN_POSITIVE));
        let gap = word.bbox[0] - before.bbox[2];
        let drift = (word.bbox[1] - before.bbox[1]).abs();
        if gap >= -0.02 * em && gap < 0.15 * em && drift < 0.3 * em {
            return true;
        }
    }
    if let Some(after) = next {
        if after.text.chars().next().is_some_and(|c| c.is_uppercase()) {
            let em = tall.max((after.bbox[3] - after.bbox[1]).max(f64::MIN_POSITIVE));
            let gap = after.bbox[0] - word.bbox[2];
            let drift = (after.bbox[1] - word.bbox[1]).abs();
            if gap >= 0.15 * em && gap <= 1.0 * em && drift < 0.3 * em {
                return true;
            }
        }
    }
    false
}

/// Whether a word is nothing but leader dots (see [`text_reading_order`]).
/// The fold appends leader runs to the label's text, so within a table cell
/// they always trail the words they fill after; geometric order can read
/// them mid-label where the fallback box covers the label.
fn is_dot_fill(word: &pairs::PairWord) -> bool {
    !word.text.is_empty() && word.text.chars().all(|c| matches!(c, '.' | '·' | '…'))
}

fn in_reading_order(mut mine: Vec<usize>, words: &[pairs::PairWord]) -> Vec<usize> {
    mine.sort_by(|&a, &b| words[b].bbox[3].total_cmp(&words[a].bbox[3]));
    let mut out = Vec::with_capacity(mine.len());
    let mut line: Vec<usize> = Vec::new();
    let mut line_top = f64::NAN;
    for w in mine {
        let top = words[w].bbox[3];
        let height = words[w].bbox[3] - words[w].bbox[1];
        if !line.is_empty() && (line_top - top).abs() > 0.5 * height {
            line.sort_by(|&a, &b| words[a].bbox[0].total_cmp(&words[b].bbox[0]));
            out.append(&mut line);
        }
        if line.is_empty() {
            line_top = top;
        }
        line.push(w);
    }
    line.sort_by(|&a, &b| words[a].bbox[0].total_cmp(&words[b].bbox[0]));
    out.append(&mut line);
    out
}

/// The number of final-table features, in [`TABLE_IS_V2_NAMES`] order.
pub const TABLE_IS_V2_FEATURES: usize = 29;

/// The final-table feature names, in order.
///
/// The first fifteen are v1's (`train_table_is.NAMES`), computed on this
/// table's cells; the rest are the final-table additions: words per filled
/// cell (median and mean), the share of filled cells holding a figure, the
/// share ending a sentence, median lines per cell, `run_on_share`,
/// `row_overlap`, `column_start_spread`, the judge probability and whether
/// the region came from the builder, grid rows and columns, filled share,
/// and whether the page drew rulings under the table (#574, #638).
///
/// ONE IMPLEMENTATION. [`table_is_v2_features`] below is the only code that
/// computes these; Python never recomputes them for training (the trainer
/// reads them from the dump) and recomputes them on fixture pages only for
/// the parity test.
pub const TABLE_IS_V2_NAMES: [&str; TABLE_IS_V2_FEATURES] = [
    "rows",
    "cols",
    "cells",
    "numeric",
    "words_cell",
    "col_words_max",
    "col_words_min",
    "heading_share",
    "p_head_max",
    "p_head_mean",
    "first_col_p_head",
    "empty",
    "height_share",
    "wordy_share",
    "first_col_words",
    "words_filled_median",
    "words_filled_mean",
    "figure_share_filled",
    "sentence_share",
    "lines_median",
    "run_on_share",
    "row_overlap",
    "column_start_spread",
    "judge_p",
    "from_builder",
    "grid_rows",
    "grid_columns",
    "filled_share",
    "has_rulings",
];

/// Whether a final-table cell holds text.
fn v2_is_filled(block: &Block) -> bool {
    !block.text.trim().is_empty()
}

/// The median of a list, 0.0 where it is empty.
///
/// 0.0 AND NOT NaN, which is where this differs from the hard-negatives
/// median it is moved from: a feature must be a number, and an empty table
/// holds no words. The two middle values are averaged when the count is
/// even, as `statistics.median` does.
fn v2_median(mut values: Vec<f64>) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[mid - 1] + values[mid]) * 0.5
    } else {
        values[mid]
    }
}

/// The population standard deviation, 0.0 where there is nothing to spread.
fn v2_stddev(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let var = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
    var.sqrt()
}

/// Whether a cell's text ends a sentence: `[a-z][.!?]$`.
fn v2_ends_sentence(text: &str) -> bool {
    let trimmed = text.trim_end();
    let mut chars = trimmed.chars().rev();
    match (chars.next(), chars.next()) {
        (Some(last), Some(before)) => {
            matches!(last, '.' | '!' | '?') && before.is_ascii_lowercase()
        }
        _ => false,
    }
}

/// Whether a cell's text starts lower case.
fn v2_starts_lower(text: &str) -> bool {
    text.trim_start()
        .chars()
        .next()
        .is_some_and(|c| c.is_lowercase())
}

/// How many text lines a cell's spans stand on.
///
/// Moved from the hard-negatives crate: two spans share a line when their
/// boxes overlap vertically by more than half of the shorter one, which
/// holds a superscript on its line and still separates a line from the next
/// at any leading. One where the cell carries no spans: a cell with text
/// and no glyph boxes is still one line.
fn v2_lines_of_cell(spans: &[TextSpan]) -> usize {
    let mut boxes: Vec<[f64; 4]> = spans
        .iter()
        .map(|s| [s.x, s.y, s.x + s.width, s.y + s.height])
        .collect();
    if boxes.is_empty() {
        return 1;
    }
    boxes.sort_by(|a, b| b[3].total_cmp(&a[3]));
    let mut lines: Vec<(f64, f64)> = Vec::new();
    for b in boxes {
        let mut placed = false;
        for line in lines.iter_mut() {
            let shorter = ((b[3] - b[1]).min(line.1 - line.0)).max(f64::MIN_POSITIVE);
            let shared = (b[3].min(line.1) - b[1].max(line.0)).max(0.0);
            if shared / shorter > 0.5 {
                line.0 = line.0.min(b[1]);
                line.1 = line.1.max(b[3]);
                placed = true;
                break;
            }
        }
        if !placed {
            lines.push((b[1], b[3]));
        }
    }
    lines.len().max(1)
}

/// Shared vertical extent over mean height, over rows with at least two
/// filled cells.
///
/// Moved from the hard-negatives crate: a shared row reads close to 1,
/// items stacked at their own heights read low. 1.0 where no row holds two
/// filled cells: there is no row to stagger.
fn v2_row_overlap(cells: &[&Block]) -> f64 {
    use std::collections::BTreeMap;
    let mut by_row: BTreeMap<u16, Vec<&Block>> = BTreeMap::new();
    for cell in cells.iter().filter(|c| v2_is_filled(c)) {
        if let BlockKind::TableCell { row, .. } = cell.kind {
            by_row.entry(row).or_default().push(cell);
        }
    }
    let mut ratios = Vec::new();
    for row in by_row.values() {
        if row.len() < 2 {
            continue;
        }
        let bottom = row
            .iter()
            .map(|c| c.bbox[1])
            .fold(f64::NEG_INFINITY, f64::max);
        let top = row.iter().map(|c| c.bbox[3]).fold(f64::INFINITY, f64::min);
        let shared = (top - bottom).max(0.0);
        let mean_height = row
            .iter()
            .map(|c| (c.bbox[3] - c.bbox[1]).max(0.0))
            .sum::<f64>()
            / row.len() as f64;
        if mean_height > 0.0 {
            ratios.push(shared / mean_height);
        }
    }
    if ratios.is_empty() {
        1.0
    } else {
        ratios.iter().sum::<f64>() / ratios.len() as f64
    }
}

/// Spread of column starts, in median cell heights.
///
/// Moved from the hard-negatives crate: the top of the first filled cell in
/// each column (the smallest row), whose standard deviation is divided by
/// the median cell height. Columns that start at their own height read
/// high; one column reads 0.0.
fn v2_column_start_spread(cells: &[&Block]) -> f64 {
    use std::collections::BTreeMap;
    let mut by_column: BTreeMap<u16, Vec<&Block>> = BTreeMap::new();
    for cell in cells.iter().filter(|c| v2_is_filled(c)) {
        if let BlockKind::TableCell { column, .. } = cell.kind {
            by_column.entry(column).or_default().push(cell);
        }
    }
    let mut tops = Vec::new();
    for column in by_column.values() {
        let mut sorted = column.clone();
        sorted.sort_by_key(|c| match c.kind {
            BlockKind::TableCell { row, .. } => row,
            _ => u16::MAX,
        });
        if let Some(first) = sorted.first() {
            tops.push(first.bbox[3]);
        }
    }
    let heights: Vec<f64> = cells
        .iter()
        .filter(|c| v2_is_filled(c))
        .map(|c| (c.bbox[3] - c.bbox[1]).max(0.0))
        .collect();
    let median_height = {
        let mut h = heights;
        if h.is_empty() {
            f64::NAN
        } else {
            h.sort_by(f64::total_cmp);
            let mid = h.len() / 2;
            if h.len().is_multiple_of(2) {
                (h[mid - 1] + h[mid]) * 0.5
            } else {
                h[mid]
            }
        }
    };
    if !median_height.is_finite() || median_height <= 0.0 {
        0.0
    } else {
        v2_stddev(&tops) / median_height
    }
}

/// Whether the upper of two vertically adjacent cells reads on into the
/// lower: the upper does not end a sentence and the lower starts lower
/// case (#574, #638).
///
/// NARROWER THAN the hard-negatives `reads_on` beside it: that one also
/// reads a trailing comma, dash or `and`/`of`/`the`/`to`/`in` as open. The
/// final-table feature is the spec's two clauses and nothing else, so a
/// fragment whose rows end in commas does not read as prose for it.
fn v2_reads_on(upper: &str, lower: &str) -> bool {
    let upper = upper.trim_end();
    let lower = lower.trim_start();
    if upper.is_empty() || lower.is_empty() {
        return false;
    }
    !v2_ends_sentence(upper) && v2_starts_lower(lower)
}

/// The share of vertically adjacent filled cells in one column that read
/// on.
///
/// Adjacent in row order among the filled cells of each column; 0.0 where
/// no column holds two filled cells.
fn v2_run_on_share(cells: &[&Block]) -> f64 {
    use std::collections::BTreeMap;
    let mut by_column: BTreeMap<u16, Vec<&Block>> = BTreeMap::new();
    for cell in cells.iter().filter(|c| v2_is_filled(c)) {
        if let BlockKind::TableCell { column, .. } = cell.kind {
            by_column.entry(column).or_default().push(cell);
        }
    }
    let mut pairs = 0usize;
    let mut on = 0usize;
    for column in by_column.values() {
        let mut sorted = column.clone();
        sorted.sort_by_key(|c| match c.kind {
            BlockKind::TableCell { row, .. } => row,
            _ => u16::MAX,
        });
        for window in sorted.windows(2) {
            pairs += 1;
            if v2_reads_on(&window[0].text, &window[1].text) {
                on += 1;
            }
        }
    }
    if pairs == 0 {
        0.0
    } else {
        on as f64 / pairs as f64
    }
}

/// Grid rows and columns: the max of `row + row_span` and of
/// `column + column_span` over the cells.
///
/// NOT v1's `rows`/`cols`, which read the max coordinate plus one and so
/// miss a spanning cell's reach. A header over two columns is two columns
/// wide here and one coordinate there.
fn v2_grid_size(cells: &[Block]) -> (usize, usize) {
    let mut rows = 0usize;
    let mut columns = 0usize;
    for cell in cells {
        if let BlockKind::TableCell {
            row,
            column,
            row_span,
            column_span,
            ..
        } = cell.kind
        {
            rows = rows.max(usize::from(row) + usize::from(row_span.max(1)));
            columns = columns.max(usize::from(column) + usize::from(column_span.max(1)));
        }
    }
    (rows, columns)
}

/// One final table's features for the final-table model, in
/// [`TABLE_IS_V2_NAMES`] order (#574, #638).
///
/// THE ONLY IMPLEMENTATION. `table` holds the table's cells and nothing
/// else, `hints` the heading decision per cell in the same order,
/// `height` the page height in points, `judge_p` the judge probability of
/// the region (NaN where the table came from no judged region),
/// `from_builder` whether the region came from the builder, and
/// `has_rulings` whether the page drew rulings under the table. Serving
/// and the dump both call this; Python reads the dump's vector for
/// training and recomputes it on fixture pages only for the parity test.
#[doc(hidden)]
pub fn table_is_v2_features(
    table: &[Block],
    hints: &[CellHint],
    height: f64,
    judge_p: f64,
    from_builder: bool,
    has_rulings: bool,
) -> [f64; TABLE_IS_V2_FEATURES] {
    let refs: Vec<&Block> = table.iter().collect();
    let all: Vec<usize> = (0..table.len()).collect();
    // A HINT PER CELL, in the table's order. A short slice reads as no
    // heading evidence rather than panicking: serving maps decoded cells to
    // their first source block and a caller that miscounts must not abort
    // the page.
    let padded: Vec<CellHint> = (0..table.len())
        .map(|i| hints.get(i).copied().unwrap_or_default())
        .collect();
    let v1 = table_is_features(&refs, &all, &padded, height);
    let filled: Vec<&&Block> = refs.iter().filter(|c| v2_is_filled(c)).collect();
    let n_filled = filled.len().max(1) as f64;
    let words: Vec<f64> = filled
        .iter()
        .map(|c| c.text.split_whitespace().count() as f64)
        .collect();
    let figure = filled.iter().filter(|c| is_figure(&c.text)).count() as f64 / n_filled;
    let sentence = filled.iter().filter(|c| v2_ends_sentence(&c.text)).count() as f64 / n_filled;
    let lines = v2_median(
        filled
            .iter()
            .map(|c| v2_lines_of_cell(&c.spans) as f64)
            .collect(),
    );
    let (grid_rows, grid_columns) = v2_grid_size(table);
    let filled_share = if grid_rows == 0 || grid_columns == 0 {
        0.0
    } else {
        filled.len() as f64 / (grid_rows * grid_columns) as f64
    };
    let mut x = [0.0f64; TABLE_IS_V2_FEATURES];
    x[..TABLE_IS_FEATURES].copy_from_slice(&v1);
    x[TABLE_IS_FEATURES] = v2_median(words.clone());
    x[TABLE_IS_FEATURES + 1] = if words.is_empty() {
        0.0
    } else {
        words.iter().sum::<f64>() / words.len() as f64
    };
    x[TABLE_IS_FEATURES + 2] = if filled.is_empty() { 0.0 } else { figure };
    x[TABLE_IS_FEATURES + 3] = if filled.is_empty() { 0.0 } else { sentence };
    x[TABLE_IS_FEATURES + 4] = lines;
    x[TABLE_IS_FEATURES + 5] = v2_run_on_share(&refs);
    x[TABLE_IS_FEATURES + 6] = v2_row_overlap(&refs);
    x[TABLE_IS_FEATURES + 7] = v2_column_start_spread(&refs);
    x[TABLE_IS_FEATURES + 8] = judge_p;
    x[TABLE_IS_FEATURES + 9] = if from_builder { 1.0 } else { 0.0 };
    x[TABLE_IS_FEATURES + 10] = grid_rows as f64;
    x[TABLE_IS_FEATURES + 11] = grid_columns as f64;
    x[TABLE_IS_FEATURES + 12] = filled_share;
    x[TABLE_IS_FEATURES + 13] = if has_rulings { 1.0 } else { 0.0 };
    x
}

/// The final-table model's threshold.
///
/// Chosen out of fold with at most 1 % of real tables refused, as v1's,
/// and written into the model like v1's.
pub const TABLE_IS_V2_THRESHOLD: f64 = models::table_is_v2::THRESHOLD;

/// The final-table model's probability that a table is one.
pub fn table_is_v2_probability(x: &[f64; TABLE_IS_V2_FEATURES]) -> f64 {
    super::probability(models::table_is_v2::predict_margin(x))
}

/// Whether the final-table model keeps a table as a table.
pub fn is_a_table_v2(x: &[f64; TABLE_IS_V2_FEATURES]) -> bool {
    table_is_v2_probability(x) >= TABLE_IS_V2_THRESHOLD
}

/// One table the pass would emit, with its final-table vector.
#[derive(Debug, Clone)]
#[doc(hidden)]
pub struct FinalTableV2 {
    /// The union of the table's cells, in user space, y up.
    pub bbox: [f64; 4],
    /// The proposer kind that drew the region first.
    pub kind: &'static str,
    /// The feature vector, in [`TABLE_IS_V2_NAMES`] order.
    pub features: [f64; TABLE_IS_V2_FEATURES],
    /// The table's cells as positions in the emitted page.
    pub cells: Vec<usize>,
}

/// Whether a ruling or a horizontal rule lies under a table.
///
/// A ruling overlaps when its box meets the table's; a horizontal rule when
/// its height stands inside the table's and its extent meets it. Edges
/// included: a rule on the table's own border is under it.
#[doc(hidden)]
pub fn has_rulings_under(
    bbox: &[f64; 4],
    rules: &[crate::text::HorizontalRule],
    rulings: &[crate::text::Ruling],
) -> bool {
    rules
        .iter()
        .any(|r| r.y >= bbox[1] && r.y <= bbox[3] && r.left <= bbox[2] && r.right >= bbox[0])
        || rulings.iter().any(|r| {
            let (x0, x1) = (r.x0.min(r.x1), r.x0.max(r.x1));
            let (y0, y1) = (r.y0.min(r.y1), r.y0.max(r.y1));
            x0 <= bbox[2] && x1 >= bbox[0] && y0 <= bbox[3] && y1 >= bbox[1]
        })
}

/// Every table the pass would emit, with its final-table vector.
///
/// `out` is the emitted page and `origin` its map back to the table pass's
/// input; `judged` that input's judge mapping (None on the region-model
/// path), `meta` the chosen regions' provenance, `builder_p` each input
/// block's builder probability, `hints` each input block's heading hint,
/// and `height` the page height. Serving refuses from this; the dump writes
/// it. ONE computation, so there is no train/serve skew in what is refused
/// and what is trained on.
#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub fn final_tables_for_is_table_v2(
    out: &[Block],
    origin: &[usize],
    judged: Option<&[Option<usize>]>,
    meta: &[JudgeRegionMeta],
    builder_p: &[Option<f64>],
    hints: &[CellHint],
    rules: &[crate::text::HorizontalRule],
    rulings: &[crate::text::Ruling],
    height: f64,
) -> Vec<FinalTableV2> {
    let refs: Vec<&Block> = out.iter().collect();
    let tables = heuristic_tables(&refs);
    let mut by: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
    for (i, t) in tables.iter().enumerate() {
        if let Some(t) = t {
            by.entry(*t).or_default().push(i);
        }
    }
    // THE TABLES AS EMITTED, AFTER THE STITCH: `out` was decoded from the
    // judge's regions, and the stitch ran inside `judge_regions_traced`
    // before any block was assigned -- so what is grouped here is already
    // what the pass emits, one entry per emitted table.
    let grouped: Vec<Vec<Block>> = by
        .values()
        .map(|cells| cells.iter().map(|&i| out[i].clone()).collect())
        .collect();
    let order: Vec<Vec<usize>> = by.into_values().collect();
    let mut finals = Vec::with_capacity(grouped.len());
    for (table, cells) in grouped.into_iter().zip(order) {
        let first = cells.first().copied().unwrap_or(0);
        let from = origin.get(first).copied().unwrap_or(first);
        let region = judged.and_then(|j| j.get(from).copied().flatten());
        // A JUDGED REGION names its provenance; anything else -- a builder
        // table the judge chose no part of, or, with no judge at all, a
        // region-model table -- is read as a builder table with its builder
        // probability where it has one.
        let (judge_p, kind, from_builder): (f64, &'static str, bool) = match region {
            Some(k) if k != REJECTED => match meta.get(k) {
                Some(m) => (m.judge_p, m.kind, m.from_builder),
                None => (f64::NAN, "builder", true),
            },
            _ => (
                builder_p.get(from).copied().flatten().unwrap_or(f64::NAN),
                "builder",
                true,
            ),
        };
        let bbox = table.iter().fold(
            [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ],
            |mut acc, b| {
                acc[0] = acc[0].min(b.bbox[0]);
                acc[1] = acc[1].min(b.bbox[1]);
                acc[2] = acc[2].max(b.bbox[2]);
                acc[3] = acc[3].max(b.bbox[3]);
                acc
            },
        );
        let bbox = if bbox[0].is_infinite() {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            bbox
        };
        let cell_hints: Vec<CellHint> = cells
            .iter()
            .map(|&i| {
                origin
                    .get(i)
                    .and_then(|&o| hints.get(o))
                    .copied()
                    .unwrap_or_default()
            })
            .collect();
        let features = table_is_v2_features(
            &table,
            &cell_hints,
            height,
            judge_p,
            from_builder,
            has_rulings_under(&bbox, rules, rulings),
        );
        finals.push(FinalTableV2 {
            bbox,
            kind,
            features,
            cells,
        });
    }
    finals
}

/// One released block: the prose a refused table's cells became, with the
/// pre-join positions of the cells it was built from, so the origin map
/// splices along and the verdict cells remap.
struct ReleasedBlock {
    /// The prose itself: a paragraph for two cells or more, today's
    /// heading-or-paragraph for one.
    block: Block,
    /// The first cell's origin: a joined block's origin is its first cell's.
    from: usize,
    /// The pre-join positions of its cells, top to bottom; the first maps to
    /// the block and the rest map to nothing.
    members: Vec<usize>,
}

/// The narrow-column join's reading of one released cell: its edges from its
/// spans where it has them and its box where it has none, its baseline from
/// the span that carries most of its characters, its type from the same
/// counts the builder reads, and a marker wherever the grammar finds one.
///
/// The builder's span-geometry leads -- a picture font, a raised figure, a
/// glyph set apart -- need the line's own measurement, which a cell never
/// had; a cell reads on its text alone, which is where the common markers
/// live. A cell with no spans reads as sizeless, so the pair join refuses
/// it: with no type to measure, keeping today's block is the honest answer.
fn join_line_of_cell(cell: &Block) -> crate::blocks::JoinLine<'_> {
    let left = cell.spans.iter().map(|s| s.x).fold(f64::INFINITY, f64::min);
    let left = if left.is_finite() { left } else { cell.bbox[0] };
    let right = cell
        .spans
        .iter()
        .map(crate::blocks::span_right)
        .fold(f64::NEG_INFINITY, f64::max);
    let right = if right.is_finite() {
        right
    } else {
        cell.bbox[2]
    };
    crate::blocks::JoinLine {
        left,
        // No drop cap beside a cell: the indent is the text's own edge.
        indent_left: left,
        right,
        baseline: crate::blocks::anchor_span(cell.spans.iter())
            .map(|s| s.y)
            .unwrap_or(cell.bbox[1]),
        top: cell.bbox[3],
        bottom: cell.bbox[1],
        font_size: crate::blocks::dominant_font_size(cell.spans.iter()),
        bold: crate::blocks::spans_read_bold(cell.spans.iter()),
        face: crate::blocks::dominant_face_of_spans(&cell.spans),
        text: &cell.text,
        marker_or_item: crate::lists::marker_at(&cell.text, crate::lists::Lead::default())
            .is_some(),
        tabular: false,
    }
}

/// One refused cell back to prose, in place: a heading where the line models
/// read one, else a paragraph, with no grid record and no confidence.
///
/// ONE definition for both release paths: the switch-off loop below, and the
/// single unjoined cell of the rejoin.
fn release_cell(cell: &mut Block, hint: CellHint) {
    if hint.heading {
        cell.kind = BlockKind::Heading { level: 1 };
        cell.confidence = hint.p_heading;
    } else {
        cell.kind = BlockKind::Paragraph;
        cell.confidence = None;
    }
    cell.grid = None;
}

/// One refused table's cells released as prose: its released blocks with the
/// pre-join position each is emitted at.
///
/// FIRST PASS READ, SECOND PASS WRITE: the column each cell stands in is
/// read here, before any kind is overwritten -- the splice that writes the
/// released blocks runs after every table is released. Grouping goes by
/// START column. A spanning cell (either span above one) never joins and is
/// never joined onto: it breaks its column into segments and stays its own
/// block. Within a segment a cell joins the block above it where m157's
/// narrow-column pair conditions hold, read on the column's own pitch and
/// measure; a block built from two cells or more is a paragraph.
///
/// `column_order` emits the released blocks column by column -- the left
/// column top to bottom, then the next, all at the table's own slot -- where
/// the table has two columns or more and some column holds a joined block;
/// otherwise each released block stands where its first cell stood.
fn release_runs(
    page: &[&Block],
    origin: &[usize],
    cells: &[usize],
    hints: &[CellHint],
    rules: &[crate::text::HorizontalRule],
    column_order: bool,
) -> Vec<(usize, ReleasedBlock)> {
    let mut columns: std::collections::BTreeMap<u16, Vec<usize>> = Default::default();
    let mut spanning: std::collections::HashSet<usize> = Default::default();
    for &i in cells {
        let (column, span) = match page[i].kind {
            BlockKind::TableCell {
                column,
                row_span,
                column_span,
                ..
            } => (column, column_span > 1 || row_span > 1),
            _ => (u16::MAX, true),
        };
        columns.entry(column).or_default().push(i);
        if span {
            spanning.insert(i);
        }
    }
    // Top to bottom inside each column: the grid's row order, geometry
    // breaking ties, the page order breaking those.
    for column in columns.values_mut() {
        column.sort_by(|&a, &b| {
            let row = |i: usize| match page[i].kind {
                BlockKind::TableCell { row, .. } => row,
                _ => 0,
            };
            row(a).cmp(&row(b)).then_with(|| {
                page[b].bbox[3]
                    .total_cmp(&page[a].bbox[3])
                    .then_with(|| a.cmp(&b))
            })
        });
    }
    let lines: std::collections::HashMap<usize, crate::blocks::JoinLine<'_>> = cells
        .iter()
        .map(|&i| (i, join_line_of_cell(page[i])))
        .collect();
    // One column's runs, top to bottom: spanning cells as singletons between
    // the joined segments.
    let mut runs: Vec<(u16, Vec<usize>)> = Vec::new();
    for (column, members) in &columns {
        // The column's own pitch: the middle step between its consecutive
        // cells, over the steps a leading could be, as `run_leading`
        // measures a column's. None where the column gives no step to
        // measure, floored at solid setting per pair.
        let em = members
            .iter()
            .map(|&i| lines.get(&i).map(|l| l.font_size).unwrap_or(0.0))
            .fold(0.0f64, f64::max)
            .max(1.0);
        let mut raw: Vec<f64> = members
            .windows(2)
            .map(|w| {
                lines.get(&w[0]).map(|l| l.baseline).unwrap_or(0.0)
                    - lines.get(&w[1]).map(|l| l.baseline).unwrap_or(0.0)
            })
            .filter(|s| *s > 0.0 && *s < 4.0 * em)
            .collect();
        let column_pitch = crate::blocks::median_step(&mut raw);
        // The column's own measure: the right edge its joinable cells reach.
        // A spanning cell is not in it -- it stands across columns, not in
        // this one's measure.
        let measure = members
            .iter()
            .filter(|i| !spanning.contains(i))
            .map(|&i| lines.get(&i).map(|l| l.right).unwrap_or(0.0))
            .fold(f64::NEG_INFINITY, f64::max);
        let mut segment: Vec<usize> = Vec::new();
        let flush = |segment: &mut Vec<usize>, runs: &mut Vec<(u16, Vec<usize>)>| {
            let mut open: Vec<usize> = Vec::new();
            for &i in segment.iter() {
                let Some(&above) = open.last() else {
                    open.push(i);
                    continue;
                };
                // The pitch of the block above: its own, once the join has
                // given it two lines to measure, and the column's until
                // then, floored at solid setting.
                let pitch = if open.len() >= 2 {
                    let mut steps: Vec<f64> = open
                        .windows(2)
                        .map(|w| {
                            lines.get(&w[0]).map(|l| l.baseline).unwrap_or(0.0)
                                - lines.get(&w[1]).map(|l| l.baseline).unwrap_or(0.0)
                        })
                        .collect();
                    crate::blocks::median_step(&mut steps)
                } else {
                    let em = lines
                        .get(&above)
                        .map(|l| l.font_size)
                        .unwrap_or(0.0)
                        .max(lines.get(&i).map(|l| l.font_size).unwrap_or(0.0));
                    column_pitch.max(crate::blocks::SOLID_PITCH_EM * em)
                };
                let joins = match (lines.get(&above), lines.get(&i)) {
                    (Some(upper), Some(lower)) => crate::blocks::narrow_column_pair_joins(
                        upper, false, lower, pitch, measure, rules,
                    ),
                    _ => false,
                };
                if joins {
                    open.push(i);
                } else {
                    runs.push((*column, std::mem::take(&mut open)));
                    open.push(i);
                }
            }
            if !open.is_empty() {
                runs.push((*column, open));
            }
            segment.clear();
        };
        for &i in members {
            if spanning.contains(&i) {
                flush(&mut segment, &mut runs);
                runs.push((*column, vec![i]));
            } else {
                segment.push(i);
            }
        }
        flush(&mut segment, &mut runs);
    }
    let table_at = cells.iter().min().copied().unwrap_or(0);
    if column_order && columns.len() >= 2 && runs.iter().any(|(_, r)| r.len() >= 2) {
        runs.into_iter()
            .map(|(_, members)| {
                let from = origin.get(members[0]).copied().unwrap_or(members[0]);
                let block = released_block(page, origin, &members, hints);
                (
                    table_at,
                    ReleasedBlock {
                        block,
                        from,
                        members,
                    },
                )
            })
            .collect()
    } else {
        let mut runs = runs;
        runs.sort_by_key(|(_, members)| members.iter().min().copied().unwrap_or(0));
        runs.into_iter()
            .map(|(_, members)| {
                let at = members.iter().min().copied().unwrap_or(table_at);
                let from = origin.get(members[0]).copied().unwrap_or(members[0]);
                let block = released_block(page, origin, &members, hints);
                (
                    at,
                    ReleasedBlock {
                        block,
                        from,
                        members,
                    },
                )
            })
            .collect()
    }
}

/// The prose one run of a refused table's cells becomes: a paragraph for two
/// cells or more, today's heading-or-paragraph for one.
fn released_block(
    page: &[&Block],
    origin: &[usize],
    members: &[usize],
    hints: &[CellHint],
) -> Block {
    if members.len() >= 2 {
        return joined(page, members);
    }
    let i = members[0];
    let hint = origin
        .get(i)
        .and_then(|&o| hints.get(o))
        .copied()
        .unwrap_or_default();
    let mut block = page[i].clone();
    release_cell(&mut block, hint);
    block
}

/// The emitted page with every final table the model refuses sent back to
/// prose, whole.
///
/// A refused table's cells become paragraphs -- a heading where the line
/// models read one, as [`demote_unchosen`] does column by column -- at
/// their own boxes, with no grid record and no confidence. Kept tables are
/// untouched, so with nothing refused the page is byte-identical to the one
/// that came in.
///
/// With `table_v2_release_rejoin` on, a refused table goes back as
/// paragraphs instead: within each column a cell joins the block above it
/// where m157's narrow-column pair conditions hold, and a block built from
/// two cells or more is a paragraph. Kept tables are still untouched, and
/// with the switch off the release is still one block per cell.
#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub fn apply_table_is_v2(
    out: Vec<Block>,
    origin: Vec<usize>,
    judged: Option<&[Option<usize>]>,
    meta: &[JudgeRegionMeta],
    builder_p: &[Option<f64>],
    hints: &[CellHint],
    rules: &[crate::text::HorizontalRule],
    rulings: &[crate::text::Ruling],
    height: f64,
    switches: super::Switches,
) -> (Vec<Block>, Vec<usize>) {
    let (out, origin, _, _) = apply_table_is_v2_traced(
        out, origin, judged, meta, builder_p, hints, rules, rulings, height, switches,
    );
    (out, origin)
}

/// One final table's verdict: the cells it covers in the emitted page and
/// the probability the refusal read.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub struct TableIsV2Verdict {
    /// The table's cells as positions in the emitted page.
    pub cells: Vec<usize>,
    /// `table_is_v2_probability` of its features: the number the refusal
    /// compared against [`TABLE_IS_V2_THRESHOLD`].
    pub probability: f64,
    /// Whether the table stands: the model kept it, or its columns are
    /// clean. The probability above is the model's own either way, so the
    /// signals read the doubt rather than the exemption.
    pub kept: bool,
}

/// How far apart, in points, a clean column's left edges may stand (#659).
///
/// The codebase's own left-edge tolerance: a heading's lines may stand a
/// point apart and still be one heading.
const CLEAN_COLUMN_ALIGN_TOLERANCE: f64 = 1.0;

/// How many words a clean column's cell holds at most (#659).
///
/// Below `WORDY` on purpose: the model's wordy rule counts running text at
/// eight words, but a three-word fragment down a column ("Anna Berg joined")
/// is already prose beside a label. A cell under this holds a label or a
/// figure.
const CLEAN_COLUMN_MAX_WORDS: usize = 2;

/// How many characters a clean column's cell holds at least (#659).
///
/// A lone glyph down a column is figure debris -- an axis tick, a legend
/// fragment -- not a label: the prose page's figure grids are columns of
/// single digits and letters the words rule above cannot see. A label or a
/// figure runs two characters at least.
const CLEAN_COLUMN_MIN_CHARS: usize = 2;

/// Whether a table's columns are clean (#659): the grids a strategy
/// reports even where the models refuse them.
///
/// A grid is clean when its shape is a grid -- two rows and two columns at
/// least -- every column's filled cells start at one left edge within
/// [`CLEAN_COLUMN_ALIGN_TOLERANCE`], no filled cell reads as a sentence:
/// it neither ends with sentence punctuation (`v2_ends_sentence`) nor holds
/// more than [`CLEAN_COLUMN_MAX_WORDS`] words -- and no filled cell is a
/// lone glyph: every one holds [`CLEAN_COLUMN_MIN_CHARS`] characters at
/// least.
///
/// The models were trained to refuse prose grids, and short aligned grids
/// of labels and figures score under their bars beside them: the borderless
/// two by two of SPEC G98 reads 0.20 against the final table's 0.22 bar,
/// and the judge rejects it outright before that. The exemption keeps
/// exactly those grids at both gates -- [`demote_unchosen`] and the
/// final-table verdict -- in every strategy, the default included, which
/// SPEC G98/G99 pin as finding them -- while prose grids still refuse: a
/// year-head sentence ends its cells, and run-on fragments run three words
/// and more. Under `TableStrategy::Lines` an unruled grid never reaches
/// this: the builder leaves it to prose before learning.
fn table_has_clean_columns(cells: &[&Block]) -> bool {
    use std::collections::{BTreeMap, BTreeSet};
    if cells.is_empty() {
        return false;
    }
    let mut rows: BTreeSet<u16> = BTreeSet::new();
    let mut by_column: BTreeMap<u16, Vec<&Block>> = BTreeMap::new();
    for &cell in cells {
        match cell.kind {
            BlockKind::TableCell { row, column, .. } => {
                rows.insert(row);
                by_column.entry(column).or_default().push(cell);
            }
            _ => return false,
        }
    }
    if rows.len() < 2 || by_column.len() < 2 {
        return false;
    }
    for column in by_column.values() {
        // The filled cells only: an empty cell holds no edge.
        let mut edges = Vec::new();
        for cell in column.iter().filter(|c| v2_is_filled(c)) {
            let edge = cell.spans.iter().map(|s| s.x).fold(f64::INFINITY, f64::min);
            edges.push(if edge.is_finite() { edge } else { cell.bbox[0] });
        }
        if edges.len() >= 2 {
            let lo = edges.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = edges.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            if hi - lo > CLEAN_COLUMN_ALIGN_TOLERANCE {
                return false;
            }
        }
    }
    for &cell in cells {
        if !v2_is_filled(cell) {
            continue;
        }
        if v2_ends_sentence(&cell.text) {
            return false;
        }
        if cell.text.split_whitespace().count() > CLEAN_COLUMN_MAX_WORDS {
            return false;
        }
        if cell.text.chars().filter(|c| !c.is_whitespace()).count() < CLEAN_COLUMN_MIN_CHARS {
            return false;
        }
    }
    true
}

/// [`apply_table_is_v2`], with the verdict beside the refusal: one
/// computation, so the signals cannot drift from the decision.
///
/// The map is each pre-join position's post-join one -- `None` for a cell a
/// join absorbed -- as the empty-column drop's is for its drop: joining
/// shrinks `out`, so the verdict cells remap through it here and the caller
/// remaps the signals' block ranges through it ([`remap_table_signal_ranges`])
/// before [`fill_is_table_v2_signals`] matches the two. With the rejoin off
/// nothing shrinks and the map is the identity.
#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub fn apply_table_is_v2_traced(
    out: Vec<Block>,
    origin: Vec<usize>,
    judged: Option<&[Option<usize>]>,
    meta: &[JudgeRegionMeta],
    builder_p: &[Option<f64>],
    hints: &[CellHint],
    rules: &[crate::text::HorizontalRule],
    rulings: &[crate::text::Ruling],
    height: f64,
    switches: super::Switches,
) -> (
    Vec<Block>,
    Vec<usize>,
    Vec<TableIsV2Verdict>,
    Vec<Option<usize>>,
) {
    let finals = final_tables_for_is_table_v2(
        &out, &origin, judged, meta, builder_p, hints, rules, rulings, height,
    );
    let mut verdicts = Vec::with_capacity(finals.len());
    for table in &finals {
        let probability = table_is_v2_probability(&table.features);
        let cells: Vec<&Block> = table.cells.iter().filter_map(|&i| out.get(i)).collect();
        let kept = probability >= TABLE_IS_V2_THRESHOLD || table_has_clean_columns(&cells);
        verdicts.push(TableIsV2Verdict {
            cells: table.cells.clone(),
            probability,
            kept,
        });
    }
    release_v2_verdicts(out, origin, verdicts, hints, rules, switches)
}

/// The verdicts' half of [`apply_table_is_v2_traced`]: every refused table
/// released to prose, whole, from the verdicts given instead of judged.
///
/// Production judges each final table and calls this with what it judged;
/// the stacked-split structure test hands its own verdicts, so a refused
/// grid and a kept one are exercised without reading the model. A verdict's
/// cells are its table's cells in the page given -- the judge clones them
/// onto the verdict -- so releasing from the verdicts is the refusal.
pub(crate) fn release_v2_verdicts(
    mut out: Vec<Block>,
    origin: Vec<usize>,
    mut verdicts: Vec<TableIsV2Verdict>,
    hints: &[CellHint],
    rules: &[crate::text::HorizontalRule],
    switches: super::Switches,
) -> (
    Vec<Block>,
    Vec<usize>,
    Vec<TableIsV2Verdict>,
    Vec<Option<usize>>,
) {
    if !switches.table_v2_release_rejoin {
        for verdict in &verdicts {
            if verdict.kept {
                continue;
            }
            for &i in &verdict.cells {
                let hint = origin
                    .get(i)
                    .and_then(|&o| hints.get(o))
                    .copied()
                    .unwrap_or_default();
                release_cell(&mut out[i], hint);
            }
        }
        for (i, block) in out.iter_mut().enumerate() {
            block.index = i;
        }
        let map = (0..out.len()).map(Some).collect();
        return (out, origin, verdicts, map);
    }
    // THE REJOIN: every refused table released as prose first, the splice
    // after -- first pass read, second pass write, so no table reads a kind
    // another table's release overwrote.
    let mut queued: std::collections::BTreeMap<usize, Vec<ReleasedBlock>> = Default::default();
    let mut consumed: std::collections::HashSet<usize> = Default::default();
    {
        let page: Vec<&Block> = out.iter().collect();
        for verdict in &verdicts {
            if verdict.kept {
                continue;
            }
            for (at, released) in release_runs(
                &page,
                &origin,
                &verdict.cells,
                hints,
                rules,
                switches.table_v2_release_column_order,
            ) {
                consumed.extend(released.members.iter().copied());
                queued.entry(at).or_default().push(released);
            }
        }
    }
    let mut new_out: Vec<Block> = Vec::with_capacity(out.len());
    let mut new_origin: Vec<usize> = Vec::with_capacity(origin.len());
    let mut map: Vec<Option<usize>> = vec![None; out.len()];
    for (i, block) in out.into_iter().enumerate() {
        if let Some(released) = queued.remove(&i) {
            for r in released {
                map[r.members[0]] = Some(new_out.len());
                new_origin.push(r.from);
                new_out.push(r.block);
            }
        }
        if consumed.contains(&i) {
            continue;
        }
        map[i] = Some(new_out.len());
        new_origin.push(origin[i]);
        new_out.push(block);
    }
    for (i, block) in new_out.iter_mut().enumerate() {
        block.index = i;
    }
    // Through the join, in emitted order: the verdict names the positions
    // its table covers in the page above, and the column order permutes
    // within the table, so the survivors sort.
    for verdict in &mut verdicts {
        let mut cells: Vec<usize> = verdict
            .cells
            .iter()
            .filter_map(|o| map.get(*o).copied().flatten())
            .collect();
        cells.sort_unstable();
        verdict.cells = cells;
    }
    (new_out, new_origin, verdicts, map)
}

/// Each table signal's block range through the release rejoin: the pre-join
/// positions to the survivors', so the range still covers the table's
/// blocks, with its box the union of them.
///
/// A refused table's range covers the prose it went back to -- the verdict
/// beside it keeps its number under the threshold, as before the rejoin --
/// and a kept table's covers its own cells, shifted past the joins above.
/// Where the join absorbed every position of a range, which a refused table
/// always survives, the range is empty at the end of the page.
#[doc(hidden)]
pub fn remap_table_signal_ranges(
    signals: &mut PageTableSignals,
    map: &[Option<usize>],
    out: &[Block],
) {
    for table in &mut signals.tables {
        let new: Vec<usize> = table
            .block_range
            .clone()
            .filter_map(|old| map.get(old).copied().flatten())
            .collect();
        table.block_range = match (new.first(), new.last()) {
            (Some(lo), Some(hi)) => *lo..hi + 1,
            _ => out.len()..out.len(),
        };
        table.bbox = union_of(out, table.block_range.clone());
    }
}

/// Each table signal's `p_is_table` from the v2 verdict on exactly its
/// table: the same number the refusal used, with the model that decided.
///
/// A signal matches the verdict whose cells are exactly its `block_range` --
/// a refused table's signal keeps its number too, under the threshold, so
/// the router reads the doubt rather than an absence. A signal no verdict
/// covers (a decoded table the grouping merged with its neighbour) is left
/// alone: no number is more honest than a neighbour's.
#[doc(hidden)]
pub fn fill_is_table_v2_signals(signals: &mut PageTableSignals, verdicts: &[TableIsV2Verdict]) {
    for table in &mut signals.tables {
        let range: Vec<usize> = table.block_range.clone().collect();
        if let Some(verdict) = verdicts.iter().find(|v| v.cells == range) {
            table.p_is_table = Some(verdict.probability);
            table.p_is_table_model = Some(IsTableModel::V2);
        }
    }
}

/// The emitted page with every kept table split stacked-only, and for each
/// old position the new one -- `None` for a block the split dropped.
fn split_out_stacked(mut out: Vec<Block>) -> (Vec<Block>, Vec<Option<usize>>) {
    for (i, block) in out.iter_mut().enumerate() {
        block.index = i;
    }
    let old_len = out.len();
    let split = super::table_split::split_tables(out, false, true);
    let mut remap: Vec<Option<usize>> = vec![None; old_len];
    let mut next: Vec<Block> = Vec::with_capacity(split.len());
    for mut block in split {
        let old = block.index.min(old_len.saturating_sub(1));
        remap[old] = Some(next.len());
        block.index = next.len();
        next.push(block);
    }
    (next, remap)
}

/// The parts of one whole table's cells after the stacked split: the new
/// positions, cut where the coordinates start over. A refused table's cells
/// are prose by now and cut nowhere, so the whole comes back as one part.
fn split_parts_of(out: &[Block], cells: &[usize], remap: &[Option<usize>]) -> Vec<Vec<usize>> {
    let mut mapped: Vec<usize> = cells
        .iter()
        .filter_map(|&old| remap.get(old).copied().flatten())
        .collect();
    mapped.sort_unstable();
    let mut parts: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::new();
    let mut previous: Option<(usize, Option<(u16, u16)>)> = None;
    for pos in mapped {
        let coord = match out.get(pos).map(|block| &block.kind) {
            Some(BlockKind::TableCell { row, column, .. }) => Some((*row, *column)),
            _ => None,
        };
        let restart = match previous {
            None => false,
            Some((before, had)) => {
                pos != before + 1 || matches!((had, coord), (Some(p), Some(c)) if c <= p)
            }
        };
        if restart {
            parts.push(std::mem::take(&mut current));
        }
        current.push(pos);
        previous = Some((pos, coord));
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

/// Every stacked split of the emitted page, after the final-table judge.
///
/// v2 judges each table the pass would emit WHOLE: the split ran before it,
/// inside `refine_judged_traced`, so a glued table v2 would have refused
/// passed as one part and the refusal never saw it. With the split after
/// the refusal, a refused table is prose by now and no split touches it;
/// every table left standing is one v2 kept, and each splits stacked-only,
/// in place. With v2 off there are no verdicts and every table splits the
/// same way.
///
/// The verdicts and the signals follow the parts: one verdict and one
/// signal per part, the kept probability on each, the cell ranges
/// remapped -- no second judgement. A refused entry keeps its single
/// range. Block count does not change -- cells are renumbered, never
/// removed -- and the old->new map underneath is the receipt, as m168's
/// drop keeps.
#[doc(hidden)]
pub fn split_stacked_after_v2(
    out: Vec<Block>,
    origin: Vec<usize>,
    verdicts: Option<Vec<TableIsV2Verdict>>,
    signals: Option<PageTableSignals>,
) -> (
    Vec<Block>,
    Vec<usize>,
    Option<Vec<TableIsV2Verdict>>,
    Option<PageTableSignals>,
) {
    let (out, remap) = split_out_stacked(out);
    let mut next_origin: Vec<usize> = vec![0; out.len()];
    for (old, new) in remap.iter().enumerate() {
        if let Some(new) = new {
            next_origin[*new] = origin.get(old).copied().unwrap_or(old);
        }
    }
    let verdicts = verdicts.map(|verdicts| {
        let mut remapped = Vec::with_capacity(verdicts.len());
        for verdict in &verdicts {
            let parts = split_parts_of(&out, &verdict.cells, &remap);
            if parts.is_empty() {
                remapped.push(TableIsV2Verdict {
                    cells: Vec::new(),
                    probability: verdict.probability,
                    kept: verdict.kept,
                });
            }
            for cells in parts {
                remapped.push(TableIsV2Verdict {
                    cells,
                    probability: verdict.probability,
                    kept: verdict.kept,
                });
            }
        }
        remapped
    });
    let signals = signals.map(|mut signals| {
        let mut remapped = Vec::with_capacity(signals.tables.len());
        for signal in &signals.tables {
            let cells: Vec<usize> = signal.block_range.clone().collect();
            let parts = split_parts_of(&out, &cells, &remap);
            if parts.is_empty() {
                remapped.push(TableSignal {
                    bbox: [0.0; 4],
                    block_range: out.len()..out.len(),
                    ..signal.clone()
                });
            }
            for part in parts {
                let block_range = match (part.first(), part.last()) {
                    (Some(lo), Some(hi)) => *lo..hi + 1,
                    _ => out.len()..out.len(),
                };
                let bbox = union_of(&out, block_range.clone());
                remapped.push(TableSignal {
                    bbox,
                    block_range,
                    ..signal.clone()
                });
            }
        }
        signals.tables = remapped;
        signals
    });
    (out, next_origin, verdicts, signals)
}

/// [`split_stacked_after_v2`] for the dump: the final tables follow the
/// parts, one record per part with the whole's features and the remapped
/// cells, so the dump describes the tables as emitted. The features stay
/// the whole's -- v2 judges wholes, and training must see what serving
/// saw.
#[doc(hidden)]
pub fn split_final_tables_stacked(
    out: Vec<Block>,
    finals: Vec<FinalTableV2>,
) -> (Vec<Block>, Vec<FinalTableV2>) {
    let (out, remap) = split_out_stacked(out);
    let mut remapped = Vec::with_capacity(finals.len());
    for table in &finals {
        let parts = split_parts_of(&out, &table.cells, &remap);
        if parts.is_empty() {
            remapped.push(FinalTableV2 {
                bbox: [0.0; 4],
                kind: table.kind,
                features: table.features,
                cells: Vec::new(),
            });
        }
        for cells in parts {
            let bbox = cells.iter().fold(
                [
                    f64::INFINITY,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::NEG_INFINITY,
                ],
                |mut acc, &i| {
                    if let Some(block) = out.get(i) {
                        acc[0] = acc[0].min(block.bbox[0]);
                        acc[1] = acc[1].min(block.bbox[1]);
                        acc[2] = acc[2].max(block.bbox[2]);
                        acc[3] = acc[3].max(block.bbox[3]);
                    }
                    acc
                },
            );
            let bbox = if bbox[0].is_infinite() {
                [0.0; 4]
            } else {
                bbox
            };
            remapped.push(FinalTableV2 {
                bbox,
                kind: table.kind,
                features: table.features,
                cells,
            });
        }
    }
    (out, remapped)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header_word(text: &str, x0: f64, x1: f64, y: f64) -> pairs::PairWord {
        pairs::PairWord {
            text: text.to_string(),
            bbox: [x0, y, x1, y + 10.0],
            table: None,
            row: None,
            column: None,
        }
    }

    fn header_cell(
        words: Vec<usize>,
        row: usize,
        column: usize,
        all: &[pairs::PairWord],
    ) -> super::super::grid::Cell {
        let b = |f: fn(&[f64; 4]) -> f64, min: bool| {
            words.iter().map(|&i| f(&all[i].bbox)).fold(
                if min {
                    f64::INFINITY
                } else {
                    f64::NEG_INFINITY
                },
                |a, v| if min { a.min(v) } else { a.max(v) },
            )
        };
        super::super::grid::Cell {
            bbox: [
                b(|x| x[0], true),
                b(|x| x[1], true),
                b(|x| x[2], false),
                b(|x| x[3], false),
            ],
            words,
            rows: vec![row],
            columns: vec![column],
        }
    }

    /// Judging with no candidate regions is the full judging for zero
    /// candidates: no block in a region, and a trace that names nothing.
    #[test]
    fn no_candidate_regions_judge_as_the_full_judging_judges() {
        let lines = vec![judge_line(0), judge_line(0)];
        let words: Vec<PageWord> = Vec::new();
        let all: Vec<judge::Proposal> = Vec::new();
        let size = (612.0, 792.0);
        assert_eq!(
            judge_regions_from_proposals(1, &lines, &words, &all, size, super::super::SWITCHES),
            judge_regions_scored(1, &lines, &words, &all, size, super::super::SWITCHES),
            "the skip returns what the full judging computes"
        );
        let skipped =
            judge_regions_from_proposals(1, &lines, &words, &all, size, super::super::SWITCHES);
        assert_eq!(skipped.regions, vec![None]);
        let trace = skipped.trace.expect("the skip keeps a trace");
        assert!(trace.details.is_empty());
        assert_eq!(trace.p_best_rejected, None);
        assert_eq!(trace.n_candidates, 0);
        assert_eq!(trace.builder_p, vec![None]);
    }

    /// A header set across the columns and cut at them word by word is one
    /// cell again; the body's cells, a column gap apart, stay apart (#638).
    #[test]
    fn a_header_parted_only_by_word_spaces_is_one_cell() {
        // `Three months ended` over two columns at x 100 and 160, words 3 pt
        // apart; the body's figures 40 pt apart.
        let words = vec![
            header_word("Three", 100.0, 125.0, 100.0),
            header_word("months", 128.0, 158.0, 100.0),
            header_word("ended", 161.0, 185.0, 100.0),
            header_word("2025", 100.0, 120.0, 80.0),
            header_word("2024", 160.0, 180.0, 80.0),
            header_word("1", 100.0, 105.0, 60.0),
            header_word("2", 160.0, 165.0, 60.0),
        ];
        let grid = separations::Grid {
            cells: vec![
                header_cell(vec![0], 0, 0, &words),
                header_cell(vec![1], 0, 1, &words),
                header_cell(vec![2], 0, 2, &words),
                header_cell(vec![3], 1, 0, &words),
                header_cell(vec![4], 1, 2, &words),
                header_cell(vec![5], 2, 0, &words),
                header_cell(vec![6], 2, 2, &words),
            ],
            header_rows: 2,
        };
        let joined = spaced_header_joined(grid, &words);
        let top: Vec<_> = joined.cells.iter().filter(|c| c.rows == [0]).collect();
        assert_eq!(top.len(), 1, "the header came apart: {top:?}");
        assert_eq!(top[0].columns, vec![0, 1, 2]);
        assert_eq!(top[0].words, vec![0, 1, 2]);
        assert_eq!(
            joined.cells.iter().filter(|c| c.rows == [1]).count(),
            2,
            "two header cells a column gap apart were joined"
        );
    }

    /// Every builder table the table-or-not model refuses comes back
    /// [`REJECTED`], and every table it keeps is left alone (#638), on the
    /// fixture's real tables from DocLayNet-train. Take the refusal out of
    /// `refine_judged` and the refused ones stay tables.
    #[test]
    fn a_table_the_model_refuses_is_rejected_and_one_it_keeps_is_not() {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/learned/table_is_parity.json"
        ))
        .expect("fixture");
        let fixture: serde_json::Value = serde_json::from_str(&text).expect("json");
        let number = |v: &serde_json::Value| v.as_f64().unwrap_or(f64::NAN);
        let (mut refused, mut kept) = (0, 0);
        for table in fixture["tables"].as_array().expect("tables") {
            let cells = table["cells"].as_array().expect("cells");
            let page: Vec<Block> = cells
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let b = c["bbox"].as_array().expect("bbox");
                    Block {
                        kind: BlockKind::TableCell {
                            row: c["row"].as_u64().expect("row") as u16,
                            column: c["column"].as_u64().expect("column") as u16,
                            row_span: 1,
                            column_span: 1,
                            header: false,
                        },
                        page: 0,
                        bbox: [number(&b[0]), number(&b[1]), number(&b[2]), number(&b[3])],
                        text: c["text"].as_str().unwrap_or_default().to_string(),
                        spans: Vec::new(),
                        index: i,
                        continues_previous: false,
                        confidence: None,
                        grid: None,
                        table_origin: TableOrigin::Other,
                        trace_learned: None,
                    }
                })
                .collect();
            let hints: Vec<CellHint> = cells
                .iter()
                .map(|c| CellHint {
                    heading: false,
                    p_heading: c["p"].as_f64(),
                })
                .collect();
            let refs: Vec<&Block> = page.iter().collect();
            // One builder table per fixture entry, as the dump grouped it.
            let tables = heuristic_tables(&refs);
            if tables
                .iter()
                .flatten()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != 1
            {
                continue;
            }
            let (out, _) = refused_by_the_table_model_traced(
                &refs,
                &vec![None; page.len()],
                &hints,
                number(&table["height"]),
            );
            if number(&table["probability"]) < models::table_is::THRESHOLD {
                assert!(
                    out.iter().all(|j| *j == Some(REJECTED)),
                    "a refused table stayed a table"
                );
                refused += 1;
            } else {
                assert!(out.iter().all(|j| j.is_none()), "a kept table was refused");
                kept += 1;
            }
        }
        assert!(
            refused >= 3 && kept >= 3,
            "the fixture holds {refused} refused and {kept} kept tables"
        );
    }

    /// The table-or-not model computes what `train_table_is.py` computes
    /// (#638): builder tables from DocLayNet-train pages, real and false, are
    /// rebuilt from the fixture (`make_table_is_parity.py`) and their 15
    /// features and the model's probability recomputed.
    #[test]
    fn the_table_or_not_model_reads_a_table_as_the_python_does() {
        let text = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/learned/table_is_parity.json"
        ))
        .expect("fixture");
        let fixture: serde_json::Value = serde_json::from_str(&text).expect("json");
        let number = |v: &serde_json::Value| v.as_f64().unwrap_or(f64::NAN);
        assert_eq!(
            fixture["names"].as_array().expect("names").len(),
            TABLE_IS_FEATURES
        );
        let tables = fixture["tables"].as_array().expect("tables");
        assert!(tables.len() >= 16, "too few tables to hold the model");
        for (n, table) in tables.iter().enumerate() {
            let cells = table["cells"].as_array().expect("cells");
            let blocks: Vec<Block> = cells
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let b = c["bbox"].as_array().expect("bbox");
                    Block {
                        kind: BlockKind::TableCell {
                            row: c["row"].as_u64().expect("row") as u16,
                            column: c["column"].as_u64().expect("column") as u16,
                            row_span: 1,
                            column_span: 1,
                            header: false,
                        },
                        page: 0,
                        bbox: [number(&b[0]), number(&b[1]), number(&b[2]), number(&b[3])],
                        text: c["text"].as_str().unwrap_or_default().to_string(),
                        spans: Vec::new(),
                        index: i,
                        continues_previous: false,
                        confidence: None,
                        grid: None,
                        table_origin: TableOrigin::Other,
                        trace_learned: None,
                    }
                })
                .collect();
            let refs: Vec<&Block> = blocks.iter().collect();
            let hints: Vec<CellHint> = cells
                .iter()
                .map(|c| CellHint {
                    heading: false,
                    p_heading: c["p"].as_f64(),
                })
                .collect();
            let all: Vec<usize> = (0..blocks.len()).collect();
            let x = table_is_features(&refs, &all, &hints, number(&table["height"]));
            for (f, v) in x.iter().enumerate() {
                let python = number(&table["features"][f]);
                assert!(
                    (v - python).abs() <= 1e-9 * python.abs().max(1.0),
                    "table {n}: feature {f} ({}) is {v} here and {python} in Python",
                    fixture["names"][f]
                );
            }
            let p = table_is_probability(&x);
            let python = number(&table["probability"]);
            assert!(
                (p - python).abs() <= 1e-6,
                "table {n}: {p} against LightGBM's {python}"
            );
        }
    }

    /// THE EXCLUSION, PINNED, NOT ONLY ARGUED (#574). `cell_hints`,
    /// `judge_regions` and `page_confidence` -- the dumped verdicts a
    /// refine-off record carries -- read no is-table verdict: with the bar
    /// forced to keep every table (0) and to refuse every table (1), all three
    /// give identical output on a page with a real builder table. If a
    /// decision ever starts reading the is-table model, this goes red and the
    /// exclusion in `training_rules.SELF_EXCLUDABLE` has to go.
    #[test]
    fn the_dump_s_verdicts_do_not_read_the_table_model() {
        fn span(text: &str, x0: f64, y0: f64, x1: f64, y1: f64) -> TextSpan {
            let words: Vec<&str> = text.split_whitespace().collect();
            let n = words.len().max(1) as f64;
            let char_bounds = words
                .iter()
                .enumerate()
                .map(|(k, _)| {
                    let left = x0 + (x1 - x0) * k as f64 / n;
                    let right = x0 + (x1 - x0) * (k as f64 + 0.9) / n;
                    [left, y0, right, y1]
                })
                .collect();
            TextSpan {
                text: text.to_string(),
                x: x0,
                y: y0,
                width: x1 - x0,
                height: y1 - y0,
                font_size: 10.0,
                char_bounds,
                ..Default::default()
            }
        }
        let mut blocks = Vec::new();
        let mut head = Block::new(
            BlockKind::Paragraph,
            0,
            [72.0, 700.0, 400.0, 712.0],
            "Quarterly revenue by region".to_string(),
            0,
        );
        head.spans = vec![span(
            "Quarterly revenue by region",
            72.0,
            700.0,
            250.0,
            712.0,
        )];
        blocks.push(head);
        for row in 0..2u16 {
            for column in 0..2u16 {
                let x0 = 72.0 + f64::from(column) * 90.0;
                let y0 = 640.0 - f64::from(row) * 20.0;
                let text = format!("value {row} {column}");
                let mut b = Block::new(
                    BlockKind::TableCell {
                        row,
                        column,
                        row_span: 1,
                        column_span: 1,
                        header: row == 0,
                    },
                    0,
                    [x0, y0, x0 + 80.0, y0 + 12.0],
                    text.clone(),
                    blocks.len(),
                );
                b.spans = vec![span(&text, x0, y0, x0 + 80.0, y0 + 12.0)];
                blocks.push(b);
            }
        }
        let mut tail = Block::new(
            BlockKind::Paragraph,
            0,
            [72.0, 560.0, 400.0, 572.0],
            "Totals exclude tax".to_string(),
            blocks.len(),
        );
        tail.spans = vec![span("Totals exclude tax", 72.0, 560.0, 200.0, 572.0)];
        blocks.push(tail);

        let sizes = [(612.0, 792.0)];
        let images: [Vec<[f64; 4]>; 1] = [Vec::new()];
        let facts = super::super::document_facts(&blocks);
        let no_rules: Vec<crate::text::HorizontalRule> = Vec::new();
        let verdicts = || {
            (
                super::super::cell_hints(&blocks, &sizes, &images),
                judge_regions(&blocks, sizes[0], &no_rules, &facts, super::super::SWITCHES),
                super::super::page_confidence(&blocks, &sizes, &images),
            )
        };
        let keep_everything = {
            let _force = TableIsThresholdGuard::set(0.0);
            verdicts()
        };
        // The fixture asked the models of something: identical empties would
        // pin nothing.
        assert!(
            keep_everything.0.iter().any(|h| h.p_heading.is_some()),
            "the fixture asked the heading model of nothing"
        );
        let refuse_everything = {
            let _force = TableIsThresholdGuard::set(1.0);
            verdicts()
        };
        assert_eq!(
            keep_everything, refuse_everything,
            "a dumped verdict moved with the table-or-not bar: the is-table \
             model reaches a refine-off record, and the exclusion has to go"
        );
        // The hook is live: the forced bar flips the table-or-not verdict
        // itself on this fixture's table.
        let refs: Vec<&Block> = blocks.iter().collect();
        let cells: Vec<usize> = (1..5).collect();
        let hints = vec![
            CellHint {
                heading: false,
                p_heading: Some(0.5),
            };
            blocks.len()
        ];
        let p = table_is_probability(&table_is_features(&refs, &cells, &hints, sizes[0].1));
        assert!(
            (0.0..1.0).contains(&p),
            "the fixture's table reads p {p}: the forced bars prove nothing on it"
        );
        let kept = {
            let _force = TableIsThresholdGuard::set(0.0);
            is_a_table(&refs, &cells, &hints, sizes[0].1)
        };
        let dropped = {
            let _force = TableIsThresholdGuard::set(1.0);
            is_a_table(&refs, &cells, &hints, sizes[0].1)
        };
        assert!(
            kept && !dropped,
            "the forced bar moved no table-or-not verdict: the hook is dead, \
             and the equality above pins nothing"
        );
    }

    fn cell_word(x: f64, top: f64, height: f64) -> pairs::PairWord {
        pairs::PairWord {
            text: "w".into(),
            bbox: [x, top - height, x + 5.0, top],
            table: None,
            row: None,
            column: None,
        }
    }

    /// The comparator this replaced was not transitive: tops 10.0, 9.4 and 8.8
    /// with heights 1.0 are pairwise "one line" in a chain, and Rust's sort
    /// aborted the page on such input (5 DocLayNet validation pages, 28-09).
    /// Sorted by top first, the order is total, and each line reads by x.
    #[test]
    fn words_of_a_cell_read_line_by_line_whatever_their_drift() {
        let words: Vec<pairs::PairWord> = vec![
            cell_word(30.0, 10.0, 1.0),
            cell_word(10.0, 9.8, 1.0),
            cell_word(20.0, 9.4, 1.0),
            cell_word(10.0, 7.0, 1.0),
            cell_word(5.0, 8.8, 1.0),
        ];
        assert_eq!(
            in_reading_order((0..5).collect(), &words),
            vec![1, 0, 2, 4, 3]
        );
        // Many words, every drift: it must not panic.
        let many: Vec<pairs::PairWord> = (0..500)
            .map(|i| cell_word(((i * 37) % 101) as f64, 100.0 - (i % 17) as f64 * 0.3, 1.0))
            .collect();
        assert_eq!(in_reading_order((0..500).collect(), &many).len(), 500);
    }

    /// A cell of two stacked lines joins the top line first with `by_line`
    /// (#574): corp-q1-2025 p13's header cell holds "March 31," over "2025"
    /// and the decoder stores them x-sorted, so the stored order joins "March
    /// 2025 31,". Without `by_line` that stored order joins untouched, and
    /// the box is the cell's either way.
    #[test]
    fn a_cell_of_two_stacked_lines_joins_top_line_first() {
        let words = vec![
            header_word("March", 410.0, 433.0, 648.87),
            header_word("2025", 425.0, 445.0, 640.87),
            header_word("31,", 435.0, 445.0, 648.87),
        ];
        let made = header_cell(vec![0, 1, 2], 0, 2, &words);
        let source = cell(made.bbox, "", true, Vec::new());
        let refs = vec![&source];
        let from = vec![0, 0, 0];
        let lined = cell_block(&made, &words, &from, &refs, None, 0, true, None, true);
        assert_eq!(lined.text, "March 31, 2025");
        assert_eq!(lined.bbox, made.bbox);
        let stored = cell_block(&made, &words, &from, &refs, None, 0, true, None, false);
        assert_eq!(stored.text, "March 2025 31,");
    }

    /// One line has one order: `by_line` leaves a single-line cell exactly as
    /// the stored order joins it (#574).
    #[test]
    fn a_single_line_cell_joins_as_stored() {
        let words = vec![
            header_word("March", 410.0, 433.0, 648.87),
            header_word("31,", 435.0, 445.0, 648.87),
        ];
        let made = header_cell(vec![0, 1], 0, 2, &words);
        let source = cell(made.bbox, "", true, Vec::new());
        let refs = vec![&source];
        let from = vec![0, 0];
        let lined = cell_block(&made, &words, &from, &refs, None, 0, true, None, true);
        assert_eq!(lined.text, "March 31,");
    }

    /// A fallback cell takes only its own words' spans, clipped to the words.
    ///
    /// One line drawn as one span, decoded into two cells with the span's
    /// centre in the word gap between them (PMC5139084-p0004's label row,
    /// #636): no span's centre lands in either cell, so both used to fall
    /// back to the source block's whole span lists, and the stacked-row split
    /// then re-emitted each line in both cells. Now each word takes the span
    /// under its box's centre, clipped to the word, and a word occurrence
    /// lands in its own cell only.
    #[test]
    fn a_fallback_cell_takes_only_its_own_words_spans() {
        fn span(text: &str, x0: f64, y0: f64, x1: f64, y1: f64) -> TextSpan {
            TextSpan {
                text: text.to_string(),
                x: x0,
                y: y0,
                width: x1 - x0,
                height: y1 - y0,
                font_size: 10.0,
                ..Default::default()
            }
        }
        let words = vec![
            header_word("Concentric", 64.0, 94.0, 240.0),
            header_word("knee", 99.0, 110.0, 240.0),
            header_word("flexor", 115.0, 132.0, 240.0),
            header_word("PLA", 64.0, 77.0, 226.0),
        ];
        let left = header_cell(vec![0], 1, 0, &words);
        let right = header_cell(vec![1, 2], 1, 1, &words);
        let source = cell(
            [56.0, 226.0, 140.0, 250.0],
            "Concentric knee flexor PLA",
            false,
            vec![
                span("Concentric knee flexor", 64.0, 240.0, 132.0, 248.0),
                span("PLA", 64.0, 226.0, 77.0, 234.0),
            ],
        );
        let refs = vec![&source];
        let from = vec![0, 0, 0, 0];
        let held = cell_spans(&left, &words, &from, &refs);
        assert_eq!(
            held.iter().map(|s| s.text.clone()).collect::<Vec<_>>(),
            vec!["Concentric"],
            "the left cell takes its word, not the block's spans: {held:?}"
        );
        assert_eq!(
            [held[0].x, held[0].y, held[0].width, held[0].height],
            [64.0, 240.0, 30.0, 10.0],
            "the clip is the word box, not the line span: {held:?}"
        );
        let held = cell_spans(&right, &words, &from, &refs);
        assert_eq!(
            held.iter().map(|s| s.text.clone()).collect::<Vec<_>>(),
            vec!["knee", "flexor"],
            "the right cell takes its words, not the block's spans: {held:?}"
        );
        // And the made blocks join their own words, with glyphs to match.
        let made = cell_block(&left, &words, &from, &refs, None, 0, false, None, true);
        assert_eq!(made.text, "Concentric");
        assert_eq!(made.spans.len(), 1);
        let made = cell_block(&right, &words, &from, &refs, None, 0, false, None, true);
        assert_eq!(made.text, "knee flexor");
        assert_eq!(made.spans.len(), 2);
    }

    /// A judged region never cuts a builder's table (#574): it takes in every
    /// table it touches whole, and two regions that then share a block merge.
    #[test]
    fn a_judged_region_grows_to_the_tables_it_touches_and_never_cuts_one() {
        // Blocks 0-3 are table 0, block 4 prose, blocks 5-6 table 1.
        let tables = [Some(0), Some(0), Some(0), Some(0), None, Some(1), Some(1)];
        // The judge left out table 0's header block (0), and a second region
        // reached into table 0 from the prose below it.
        let got = grown(vec![vec![1, 2, 3], vec![3, 4]], &tables);
        assert_eq!(got, vec![vec![0, 1, 2, 3, 4]]);
        // A region over prose alone stays as it is.
        assert_eq!(grown(vec![vec![4]], &tables), vec![vec![4]]);
        // And one touching table 1 takes all of it.
        assert_eq!(grown(vec![vec![6]], &tables), vec![vec![5, 6]]);
    }

    #[test]
    fn a_list_item_stays_out_of_a_judged_region() {
        let block =
            |kind: BlockKind| Block::new(kind, 0, [0.0, 0.0, 10.0, 10.0], "x".to_string(), 0);
        let cell = BlockKind::TableCell {
            row: 0,
            column: 0,
            row_span: 1,
            column_span: 1,
            header: false,
        };
        let page = [
            block(BlockKind::Paragraph),
            block(BlockKind::ListItem {
                level: 0,
                ordinal: None,
            }),
            block(cell),
        ];
        let refs: Vec<&Block> = page.iter().collect();
        let got = judged_runs(&[Some(0), Some(0), Some(0)], &refs);
        assert_eq!(
            got,
            vec![vec![0, 2]],
            "the list item was taken into the table"
        );
    }

    #[test]
    fn a_region_is_one_table_only_when_it_is_that_table_exactly() {
        let tables = [Some(0), Some(0), None, Some(1)];
        assert!(is_one_table(&[0, 1], &tables));
        assert!(!is_one_table(&[0], &tables));
        assert!(!is_one_table(&[0, 1, 2], &tables));
        assert!(!is_one_table(&[2], &tables));
    }
    fn cell(bbox: [f64; 4], text: &str, header: bool, spans: Vec<TextSpan>) -> Block {
        Block {
            kind: BlockKind::TableCell {
                row: 0,
                column: 0,
                row_span: 1,
                column_span: 1,
                header,
            },
            page: 0,
            bbox,
            text: text.to_string(),
            spans,
            index: 0,
            continues_previous: false,
            confidence: None,
            grid: None,
            table_origin: TableOrigin::Other,
            trace_learned: None,
        }
    }

    /// A rejected table of two columns of prose goes back COLUMN BY COLUMN,
    /// and the cell the line models read as a heading comes back a heading
    /// (#574). Joined by row, `Operating leases` in the right column was one
    /// paragraph with the left column's line beside it, which is how 144 of
    /// the headings the build missed on T6's proxy were lost.
    #[test]
    fn a_rejected_table_goes_back_column_by_column_with_its_heading() {
        let at = |row: u16, column: u16, text: &str| {
            let mut b = cell([0.0; 4], text, false, Vec::new());
            b.kind = BlockKind::TableCell {
                row,
                column,
                row_span: 1,
                column_span: 1,
                header: false,
            };
            b
        };
        // Row-major, as the builder leaves a grid.
        let blocks = [
            at(0, 0, "Basic EPS is calculated by dividing"),
            at(0, 1, "Operating leases"),
            at(1, 0, "the net profit by the weighted average."),
            at(1, 1, "The minimum lease payments are expensed"),
            at(2, 0, "Diluted EPS adjusts the shares."),
            at(2, 1, "on a straight-line basis."),
        ];
        let page: Vec<&Block> = blocks.iter().collect();
        let tables = vec![Some(0); page.len()];
        let judged = vec![Some(REJECTED); page.len()];
        let mut hints = vec![CellHint::default(); page.len()];
        hints[1] = CellHint {
            heading: true,
            p_heading: Some(0.9),
        };
        let never: &Builder<'_> = &|_, _| None;
        let run = |per_cell: bool| {
            let mut replacement: Vec<Option<Vec<Block>>> = vec![None; page.len()];
            demote_unchosen(
                &page,
                &tables,
                &judged,
                never,
                &hints,
                per_cell,
                &mut replacement,
            );
            replacement
                .into_iter()
                .flatten()
                .flatten()
                .map(|b| (matches!(b.kind, BlockKind::Heading { .. }), b.text))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            run(true),
            vec![
                (
                    false,
                    "Basic EPS is calculated by dividing the net profit by the weighted \
                     average. Diluted EPS adjusts the shares."
                        .to_string()
                ),
                (true, "Operating leases".to_string()),
                (
                    false,
                    "The minimum lease payments are expensed on a straight-line basis.".to_string()
                ),
            ],
            "the left column, then the heading, then the right column's text"
        );
        // And off, the row join this replaces: the heading glued to its row.
        assert_eq!(
            run(false)[0].1,
            "Basic EPS is calculated by dividing Operating leases"
        );
    }

    /// Reading order that goes back up starts a new run.
    ///
    /// A two-column page: a table at the foot of the left column, then one at
    /// the head of the right. The gap test alone cannot see it -- the
    /// subtraction is negative -- so the two became ONE run, one decoder call,
    /// and the second table was rebuilt on the first's doubt record
    /// (codex, #600).
    #[test]
    fn a_run_breaks_where_reading_order_jumps_to_the_next_column() {
        // y is up: the left column's rows sit low, the right column's high.
        let blocks = [
            cell([10.0, 100.0, 90.0, 110.0], "left a", false, vec![]),
            cell([10.0, 88.0, 90.0, 98.0], "left b", false, vec![]),
            cell([110.0, 700.0, 190.0, 710.0], "right a", false, vec![]),
            cell([110.0, 688.0, 190.0, 698.0], "right b", false, vec![]),
        ];
        let page: Vec<&Block> = blocks.iter().collect();
        let inside = [true; 4];
        let sizes = [10.0; 4];

        let got = runs(&page, &inside, &sizes);
        assert_eq!(
            got,
            vec![vec![0, 1], vec![2, 3]],
            "the two columns' tables were run together, so one decoder saw \
             both and the second was judged on the first's record"
        );
    }

    /// And a table that simply continues downwards stays one run.
    #[test]
    fn a_run_survives_the_next_row_of_the_same_table() {
        let blocks = [
            cell([10.0, 100.0, 90.0, 110.0], "row one", false, vec![]),
            cell([10.0, 88.0, 90.0, 98.0], "row two", false, vec![]),
            cell([10.0, 76.0, 90.0, 86.0], "row three", false, vec![]),
        ];
        let page: Vec<&Block> = blocks.iter().collect();
        let got = runs(&page, &[true; 3], &[10.0; 3]);
        assert_eq!(got, vec![vec![0, 1, 2]], "one table was cut into pieces");
    }

    /// Cells of one row share a top only approximately, and jitter is not a jump.
    #[test]
    fn a_wobbling_row_is_not_a_column_jump() {
        let blocks = [
            cell([10.0, 100.0, 40.0, 110.0], "a", false, vec![]),
            // half a line higher: the same row, set a shade above its neighbour
            cell([50.0, 105.0, 90.0, 115.0], "b", false, vec![]),
        ];
        let page: Vec<&Block> = blocks.iter().collect();
        let got = runs(&page, &[true; 2], &[10.0; 2]);
        assert_eq!(
            got,
            vec![vec![0, 1]],
            "a row whose cells do not share an exact top was cut in two"
        );
    }

    fn kv_word(text: &str, x0: f64, x1: f64, y: f64) -> pairs::PairWord {
        pairs::PairWord {
            text: text.to_string(),
            bbox: [x0, y, x1, y + 10.0],
            table: None,
            row: None,
            column: None,
        }
    }

    fn kv_cell(
        words: Vec<usize>,
        rows: Vec<usize>,
        columns: Vec<usize>,
        all: &[pairs::PairWord],
    ) -> super::super::grid::Cell {
        let edge = |f: fn(&[f64; 4]) -> f64, min: bool| {
            words.iter().map(|&i| f(&all[i].bbox)).fold(
                if min {
                    f64::INFINITY
                } else {
                    f64::NEG_INFINITY
                },
                |a, v| if min { a.min(v) } else { a.max(v) },
            )
        };
        super::super::grid::Cell {
            bbox: [
                edge(|x| x[0], true),
                edge(|x| x[1], true),
                edge(|x| x[2], false),
                edge(|x| x[3], false),
            ],
            words,
            rows,
            columns,
        }
    }

    /// A two-column grid from `(left, right)` row texts, with row `section` as
    /// one spanning cell; `span_section` stretches that cell over the next row
    /// down as well.
    fn section_grid(
        rows: &[(&str, &str)],
        section: usize,
        span_section: bool,
    ) -> (Vec<pairs::PairWord>, separations::Grid) {
        let mut words = Vec::new();
        let mut cells = Vec::new();
        for (r, (left, right)) in rows.iter().enumerate() {
            let y = 200.0 - 20.0 * r as f64;
            if r == section {
                let from = words.len();
                for (k, word) in left.split_whitespace().enumerate() {
                    let x = 100.0 + 30.0 * k as f64;
                    words.push(kv_word(word, x, x + 25.0, y));
                }
                let mine: Vec<usize> = (from..words.len()).collect();
                let span = if span_section {
                    vec![r, r + 1]
                } else {
                    vec![r]
                };
                cells.push(kv_cell(mine, span, vec![0, 1], &words));
            } else {
                let a = words.len();
                words.push(kv_word(left, 100.0, 150.0, y));
                words.push(kv_word(right, 160.0, 210.0, y));
                cells.push(kv_cell(vec![a], vec![r], vec![0], &words));
                cells.push(kv_cell(vec![a + 1], vec![r], vec![1], &words));
            }
        }
        (
            words,
            separations::Grid {
                cells,
                header_rows: 1,
            },
        )
    }

    /// A grid with any column count from per-row cell texts, with row `section`
    /// as one spanning cell (`rows[section]` holds the section text alone).
    fn section_grid_n(
        rows: &[&[&str]],
        section: usize,
    ) -> (Vec<pairs::PairWord>, separations::Grid) {
        let n_cols = rows
            .iter()
            .enumerate()
            .filter(|(r, _)| *r != section)
            .map(|(_, row)| row.len())
            .max()
            .unwrap_or(0);
        let mut words = Vec::new();
        let mut cells = Vec::new();
        for (r, row) in rows.iter().enumerate() {
            let y = 200.0 - 20.0 * r as f64;
            if r == section {
                let from = words.len();
                for (k, word) in row[0].split_whitespace().enumerate() {
                    let x = 100.0 + 30.0 * k as f64;
                    words.push(kv_word(word, x, x + 25.0, y));
                }
                let mine: Vec<usize> = (from..words.len()).collect();
                cells.push(kv_cell(mine, vec![r], (0..n_cols).collect(), &words));
            } else {
                for (c, text) in row.iter().enumerate() {
                    let a = words.len();
                    let x0 = 100.0 + 60.0 * c as f64;
                    words.push(kv_word(text, x0, x0 + 50.0, y));
                    cells.push(kv_cell(vec![a], vec![r], vec![c], &words));
                }
            }
        }
        (
            words,
            separations::Grid {
                cells,
                header_rows: 1,
            },
        )
    }

    /// A section row between two key-value blocks splits the table (#638).
    ///
    /// Four `label = value` rows, one row holding only `Data collection`, four
    /// more key-value rows: two tables with the section between them, row
    /// numbers restarted, the section text in neither table. Take the split out
    /// of `refine_judged` and the grid comes back whole.
    #[test]
    fn a_section_row_between_key_value_blocks_splits_the_table() {
        let rows = [
            ("Mr = 623.79", "cubic"),
            ("a = 22.3794 (8) Å", "= 2.065"),
            ("Z = 4", "red"),
            ("V = 2006.85", "prism"),
            ("Data collection", ""),
            ("T min = 0.219", "graphite"),
            ("h = 0→26", "sealed tube"),
            ("k = 0→14", "none"),
            ("R int = 0.042", "decay: none"),
        ];
        let (words, grid) = section_grid(&rows, 4, false);
        let pieces = key_value_sections_split(&grid, &words).expect("the grid did not split");
        let (first, second) = match &pieces[..] {
            [SplitPiece::Table {
                cells: a,
                header_rows: before,
            }, SplitPiece::Section { row }, SplitPiece::Table {
                cells: b,
                header_rows: after,
            }] => {
                assert_eq!(*row, 4, "the split is at the wrong row");
                assert_eq!((*before, *after), (1, 0), "the header rows did not follow");
                (a, b)
            }
            other => panic!("no split: {other:?}"),
        };
        for (n, table) in [first, second].into_iter().enumerate() {
            assert!(
                table.iter().all(|c| c.rows.iter().all(|&r| r < 4)),
                "table {n} rows were not renumbered from 0"
            );
            assert_eq!(
                table.iter().map(|c| c.rows[0]).max(),
                Some(3),
                "table {n} lost a row in the split"
            );
            for cell in table {
                let text = cell
                    .words
                    .iter()
                    .map(|&w| words[w].text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ");
                assert!(
                    !text.contains("Data collection"),
                    "table {n} kept the section: {text:?}"
                );
            }
        }
        assert_eq!(
            first.len() + second.len(),
            16,
            "cells went missing in the split"
        );
        // The decoder cuts many key-value cells into three columns -- `a` | `=`
        // | `22.3794 (8) Å` -- so the split must fire there too, with `=` as a
        // cell of its own. Fails against the column gate at `== 2`. The upper
        // flank sits exactly at the key-value share (one `=` of its own, one
        // trailing ` =`), so either new match arm going missing fails it too.
        let wide: &[&[&str]] = &[
            &["a", "=", "22.3794 (8) Å"],
            &["b =", "13.1", "cubic"],
            &["colour", "red", "prism"],
            &["habit", "plate", "small"],
            &["Data collection"],
            &["Tmin", "=", "0.219"],
            &["h", "=", "0→26"],
            &["= 14", "k", "none"],
            &["Rint", "=", "0.042"],
        ];
        let (words, grid) = section_grid_n(wide, 4);
        let pieces =
            key_value_sections_split(&grid, &words).expect("the three-column grid did not split");
        match &pieces[..] {
            [SplitPiece::Table {
                cells: a,
                header_rows: before,
            }, SplitPiece::Section { row }, SplitPiece::Table {
                cells: b,
                header_rows: after,
            }] => {
                assert_eq!(*row, 4, "the split is at the wrong row");
                assert_eq!((*before, *after), (1, 0), "the header rows did not follow");
                assert_eq!(
                    a.len() + b.len(),
                    24,
                    "cells went missing in the three-column split"
                );
                for (n, table) in [a, b].into_iter().enumerate() {
                    assert!(
                        table.iter().all(|c| c.rows.iter().all(|&r| r < 4)),
                        "table {n} rows were not renumbered from 0"
                    );
                }
            }
            other => panic!("no split: {other:?}"),
        }
    }

    /// A section row in a table of FIGURES does not split it (#576).
    ///
    /// The same shape with no `=` anywhere -- `Revenues | 1,234`, a row
    /// `Geographic locations`, more figure rows -- is a financial table, and
    /// the section row stays a row of it. This is the guard on the rule above:
    /// it fails against a split that fires at any section row without asking
    /// what stands on either side of it.
    #[test]
    fn a_section_row_in_a_table_of_figures_does_not_split_it() {
        let rows = [
            ("Revenues", "1,234"),
            ("Costs", "987"),
            ("Margin", "12%"),
            ("Tax", "345"),
            ("Geographic locations", ""),
            ("North", "654"),
            ("South", "321"),
            ("East", "111"),
            ("West", "222"),
        ];
        let (words, grid) = section_grid(&rows, 4, false);
        assert!(
            key_value_sections_split(&grid, &words).is_none(),
            "a financial table split at its section row"
        );
        // The widened gate admits four columns, but a figures table holds no
        // `=`, so its section row still splits nothing.
        let wide: &[&[&str]] = &[
            &["Revenues", "Q1", "Q2", "1,234"],
            &["Costs", "Q1", "Q2", "987"],
            &["Margin", "Q1", "Q2", "12%"],
            &["Tax", "Q1", "Q2", "345"],
            &["Geographic locations"],
            &["North", "Q1", "Q2", "654"],
            &["South", "Q1", "Q2", "321"],
            &["East", "Q1", "Q2", "111"],
            &["West", "Q1", "Q2", "222"],
        ];
        let (words, grid) = section_grid_n(wide, 4);
        assert!(
            key_value_sections_split(&grid, &words).is_none(),
            "a four-column financial table split at its section row"
        );
    }

    /// A section cell spanning two rows vetoes the split (#638).
    ///
    /// The rule tears between rows, never through a cell: a section cell whose
    /// rows straddle the split would belong to neither piece, so its words
    /// would vanish from both tables. Take the veto out and `Data collection`
    /// is gone from the blocks.
    #[test]
    fn a_section_cell_spanning_two_rows_vetoes_the_split() {
        let rows = [
            ("Mr = 623.79", "cubic"),
            ("Z = 4", "red"),
            ("V = 2006.85", "prism"),
            ("Data collection", ""),
            ("T min = 0.219", "graphite"),
            ("h = 0→26", "sealed tube"),
            ("k = 0→14", "none"),
            ("R int = 0.042", "decay: none"),
            ("F 000 = 1176", "observed"),
        ];
        let (words, grid) = section_grid(&rows, 3, true);
        assert!(
            key_value_sections_split(&grid, &words).is_none(),
            "the split tore through a spanning section cell"
        );
    }

    /// A refused table that joins, followed by a kept table: the kept table's
    /// signal still carries its own verdict's number (#574).
    ///
    /// Joining shrinks the page, so the verdict cells and the signals' block
    /// ranges remap through the join map before the fill matches them --
    /// exactly the composition `refine_page_traced` runs. Without the remap
    /// the kept table's range still points at the pre-join positions and the
    /// fill misses it: its `p_is_table` stays `None`, as the stale clone at
    /// the end shows.
    #[test]
    fn a_kept_table_behind_a_joined_refusal_keeps_its_own_signal() {
        // Two builder tables: run-on prose columns the judge refuses, then a
        // statement of figures it keeps. The refused cells carry their type
        // -- one span each, one face, fourteen-point steps -- so the columns
        // join.
        let col0 = [
            "Anna Berg joined",
            "the board in late",
            "spring and now",
            "chairs the audit",
            "group each year",
        ];
        let col1 = [
            "she previously served as chief",
            "financial officer for a decade",
            "across three continents with",
            "patience through every season",
            "and retired last spring.",
        ];
        let mut page: Vec<Block> = Vec::new();
        for r in 0..5u16 {
            let y = 700.0 - 14.0 * f64::from(r);
            for (c, text) in [col0[usize::from(r)], col1[usize::from(r)]]
                .iter()
                .enumerate()
            {
                let x = 70.0 + 150.0 * c as f64;
                let mut b = Block::new(
                    BlockKind::TableCell {
                        row: r,
                        column: c as u16,
                        row_span: 1,
                        column_span: 1,
                        header: false,
                    },
                    0,
                    [x, y, x + 140.0, y + 12.0],
                    text.to_string(),
                    page.len(),
                );
                b.spans = vec![TextSpan {
                    x,
                    y: y + 2.0,
                    width: 100.0,
                    height: 10.0,
                    font_size: 10.0,
                    font_name: Some("Helvetica".to_string()),
                    text: text.to_string(),
                    ..Default::default()
                }];
                page.push(b);
            }
        }
        let head = ["", "2025", "2024", "2023"];
        for (c, t) in head.iter().enumerate() {
            let x = 70.0 + 90.0 * c as f64;
            page.push(Block::new(
                BlockKind::TableCell {
                    row: 0,
                    column: c as u16,
                    row_span: 1,
                    column_span: 1,
                    header: false,
                },
                0,
                [x, 560.0, x + 80.0, 572.0],
                t.to_string(),
                page.len(),
            ));
        }
        let labels = ["Revenue", "Costs", "Profit"];
        for r in 1..=3u16 {
            let y = 560.0 - 20.0 * f64::from(r);
            page.push(Block::new(
                BlockKind::TableCell {
                    row: r,
                    column: 0,
                    row_span: 1,
                    column_span: 1,
                    header: false,
                },
                0,
                [70.0, y, 150.0, y + 12.0],
                labels[usize::from(r) - 1].to_string(),
                page.len(),
            ));
            for c in 1..4u16 {
                let x = 70.0 + 90.0 * f64::from(c);
                page.push(Block::new(
                    BlockKind::TableCell {
                        row: r,
                        column: c,
                        row_span: 1,
                        column_span: 1,
                        header: false,
                    },
                    0,
                    [x, y, x + 80.0, y + 12.0],
                    format!("{}", 10_000 + usize::from(r) * 100 + usize::from(c)),
                    page.len(),
                ));
            }
        }
        assert_eq!(page.len(), 26, "ten refused cells, sixteen kept");
        let origin: Vec<usize> = (0..page.len()).collect();
        let hints = vec![
            CellHint {
                heading: false,
                p_heading: Some(0.1),
            };
            page.len()
        ];
        let switches = super::super::Switches {
            table_v2_release_rejoin: true,
            table_v2_release_column_order: false,
            ..super::super::SWITCHES
        };
        let (out, out_origin, verdicts, map) = apply_table_is_v2_traced(
            page.clone(),
            origin,
            None,
            &[],
            &vec![None; page.len()],
            &hints,
            &[],
            &[],
            792.0,
            switches,
        );
        assert_eq!(verdicts.len(), 2, "two tables, two verdicts");
        assert!(
            !verdicts[0].kept && verdicts[1].kept,
            "the prose refuses and the statement keeps: {verdicts:?}"
        );
        // Both columns joined whole: two paragraphs, then the sixteen kept
        // cells shifted past them.
        assert_eq!(out.len(), 18, "ten cells joined into two");
        assert_eq!(verdicts[0].cells, vec![0, 1]);
        assert_eq!(
            verdicts[1].cells,
            (2..18).collect::<Vec<usize>>(),
            "the kept verdict names its cells where the join left them"
        );
        assert_eq!(out_origin.len(), out.len(), "the origin spliced along");
        assert_eq!(
            (out_origin[0], out_origin[1]),
            (0, 1),
            "a joined block's origin is its first cell's"
        );
        // The signals as the table pass emits them: one contiguous range per
        // table over the PRE-join positions, with no is-table number yet.
        let signal = |range: std::ops::Range<usize>| TableSignal {
            p_judge: 0.9,
            verifier_score: 1.0,
            verifier_margin: None,
            p_is_table: None,
            p_is_table_model: None,
            split: TableSplit::Intact,
            bbox: union_of(&page, range.clone()),
            block_range: range,
        };
        let mut signals = PageTableSignals {
            tables: vec![signal(0..10), signal(10..26)],
            p_best_rejected: None,
            n_candidates: 2,
            refused: Vec::new(),
            drawn: false,
        };
        let mut stale = signals.clone();
        remap_table_signal_ranges(&mut signals, &map, &out);
        fill_is_table_v2_signals(&mut signals, &verdicts);
        assert_eq!(signals.tables[0].block_range, 0..2);
        assert_eq!(signals.tables[1].block_range, 2..18);
        for (table, verdict) in signals.tables.iter().zip(&verdicts) {
            assert_eq!(
                table.p_is_table,
                Some(verdict.probability),
                "range {:?} does not carry its own verdict's number",
                table.block_range
            );
            assert_eq!(table.p_is_table_model, Some(IsTableModel::V2));
        }
        // And without the remap the fill misses the kept table: its range
        // still points at the pre-join positions.
        fill_is_table_v2_signals(&mut stale, &verdicts);
        assert_eq!(
            stale.tables[1].p_is_table, None,
            "the stale range still matched, so this test says nothing"
        );
    }

    /// A section row keeps the heading level it already has, and is otherwise
    /// prose (#638): the table pass runs last and neither demotes what the
    /// heading pass named nor invents a level of its own.
    #[test]
    fn a_section_row_keeps_its_heading_level_and_is_otherwise_prose() {
        let named = Block::new(
            BlockKind::Heading { level: 2 },
            0,
            [0.0, 0.0, 10.0, 10.0],
            "Data collection".to_string(),
            0,
        );
        let plain = Block::new(
            BlockKind::Paragraph,
            0,
            [0.0, 0.0, 10.0, 10.0],
            "Data collection".to_string(),
            0,
        );
        let words = vec![kv_word("Data", 0.0, 5.0, 0.0)];
        let cell = kv_cell(vec![0], vec![4], vec![0, 1], &words);
        let refs = [&named];
        assert_eq!(
            section_kind(&cell, &[0], &refs),
            BlockKind::Heading { level: 2 },
            "a named heading was demoted"
        );
        let refs = [&plain];
        assert_eq!(
            section_kind(&cell, &[0], &refs),
            BlockKind::Paragraph,
            "an unnamed section row became a heading"
        );
    }

    fn drop_cell(row: u16, column: u16, text: &str) -> Block {
        Block::new(
            BlockKind::TableCell {
                row,
                column,
                row_span: 1,
                column_span: 1,
                header: false,
            },
            0,
            [0.0, 0.0, 10.0, 10.0],
            text.to_string(),
            0,
        )
    }

    /// The drop splices the origin map with the blocks, and the positions it
    /// returns mark the cells that went, so the signals follow (#574).
    #[test]
    fn the_drop_carries_the_origin_map_and_the_positions() {
        let mut out = vec![
            drop_cell(0, 0, "a"),
            drop_cell(0, 1, ""),
            drop_cell(0, 2, "b"),
            drop_cell(1, 0, "c"),
            drop_cell(1, 1, ""),
            drop_cell(1, 2, "d"),
            Block::new(
                BlockKind::Paragraph,
                0,
                [0.0, 0.0, 10.0, 10.0],
                "note".to_string(),
                0,
            ),
        ];
        let mut origin = vec![0, 1, 2, 3, 4, 5, 6];
        let map = drop_empty_columns_from_tables(&mut out, &mut origin);
        let coords: Vec<(u16, u16, String)> = out
            .iter()
            .filter_map(|b| match &b.kind {
                BlockKind::TableCell { row, column, .. } => Some((*row, *column, b.text.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(
            coords,
            vec![
                (0, 0, "a".into()),
                (0, 1, "b".into()),
                (1, 0, "c".into()),
                (1, 1, "d".into()),
            ],
            "the column did not go"
        );
        assert_eq!(origin, vec![0, 2, 3, 5, 6], "the origins did not follow");
        assert_eq!(
            map,
            vec![Some(0), None, Some(1), Some(2), None, Some(3), Some(4)],
            "the positions did not mark the cells that went"
        );
    }

    /// A refused grid stays prose after the stacked split, and a kept one
    /// splits in two with the verdict on both parts (#574).
    ///
    /// The verdicts are hand-made -- refused reads 0.0, kept 1.0, neither the
    /// model's number -- through [`release_v2_verdicts`] and
    /// [`split_stacked_after_v2`], in the production order: the judge's
    /// release first, the split after. A refused table is prose by the split
    /// and never divides, so its single verdict stands; a kept table divides
    /// and each part carries the whole's verdict, with no second judgement.
    ///
    /// Red with the order reversed: split first and the refused whole divides
    /// like the kept one, so the refusal comes back as two verdicts instead
    /// of one.
    #[test]
    fn a_refused_grid_stays_whole_and_a_kept_one_splits_in_two() {
        fn glued() -> Vec<Block> {
            let rows = [
                ("For the year ended", ["2018", "2017"]),
                ("Revenue", ["79,591", "79,139"]),
                ("Net income", ["8,728", "5,753"]),
                ("At December 31:", ["2018", "2017"]),
                ("Total assets", ["123,382", "125,356"]),
                ("Total debt", ["45,812", "46,824"]),
            ];
            let mut out = Vec::new();
            for (r, &(stub, figs)) in rows.iter().enumerate() {
                for (c, text) in [stub, figs[0], figs[1]].iter().enumerate() {
                    let x = 70.0 + 150.0 * c as f64;
                    let y = 700.0 - 20.0 * r as f64;
                    out.push(Block::new(
                        BlockKind::TableCell {
                            row: r as u16,
                            column: c as u16,
                            row_span: 1,
                            column_span: 1,
                            header: false,
                        },
                        0,
                        [x, y, x + 140.0, y + 12.0],
                        text.to_string(),
                        out.len(),
                    ));
                }
            }
            out
        }
        fn tables_in(blocks: &[Block]) -> usize {
            let mut tables = 0;
            let mut last: Option<(u16, u16)> = None;
            for block in blocks {
                match block.kind {
                    BlockKind::TableCell { row, column, .. } => {
                        if last.is_none_or(|seen| (row, column) <= seen) {
                            tables += 1;
                        }
                        last = Some((row, column));
                    }
                    _ => last = None,
                }
            }
            tables
        }
        fn judge(kept: bool, n: usize) -> Vec<TableIsV2Verdict> {
            vec![TableIsV2Verdict {
                cells: (0..n).collect(),
                probability: if kept { 1.0 } else { 0.0 },
                kept,
            }]
        }
        let switches = super::super::Switches {
            table_v2_release_rejoin: false,
            ..super::super::SWITCHES
        };
        // The refusal: prose after the release, still prose after the split,
        // one verdict standing for the whole.
        let page = glued();
        let origin: Vec<usize> = (0..page.len()).collect();
        let hints = vec![CellHint::default(); page.len()];
        let (released, released_origin, verdicts, _) =
            release_v2_verdicts(page, origin, judge(false, 18), &hints, &[], switches);
        assert_eq!(tables_in(&released), 0, "the refusal kept cells");
        let (split, _, verdicts, _) =
            split_stacked_after_v2(released, released_origin, Some(verdicts), None);
        assert_eq!(tables_in(&split), 0, "a refused grid split into a table");
        let verdicts = verdicts.expect("the verdicts follow the page");
        assert_eq!(verdicts.len(), 1, "the refusal divided: {verdicts:?}");
        assert_eq!(verdicts[0].probability, 0.0, "not the hand-made refusal");
        assert!(!verdicts[0].kept, "not the hand-made refusal");
        // The mirror: untouched by the release, two tables after the split,
        // the kept verdict on both parts.
        let page = glued();
        let origin: Vec<usize> = (0..page.len()).collect();
        let (released, released_origin, verdicts, _) =
            release_v2_verdicts(page, origin, judge(true, 18), &hints, &[], switches);
        assert_eq!(tables_in(&released), 1, "the kept whole is gone");
        let (split, _, verdicts, _) =
            split_stacked_after_v2(released, released_origin, Some(verdicts), None);
        assert_eq!(tables_in(&split), 2, "the kept grid did not split in two");
        let verdicts = verdicts.expect("the verdicts follow the page");
        assert_eq!(verdicts.len(), 2, "one verdict per part: {verdicts:?}");
        for (n, verdict) in verdicts.iter().enumerate() {
            assert_eq!(
                verdict.probability, 1.0,
                "part {n} lost the hand-made verdict: {verdicts:?}"
            );
            assert!(verdict.kept, "part {n} lost the hand-made verdict");
        }
        assert_eq!(
            verdicts[0].cells.len() + verdicts[1].cells.len(),
            18,
            "the parts do not tile the whole: {verdicts:?}"
        );
    }

    /// Where the lower numbers would read two runs as one, the page keeps
    /// its grid: the drop removes columns, never tables (#574).
    #[test]
    fn runs_that_would_read_as_one_keep_the_page() {
        // The first run drops its blank columns and ends at (0, 0), where
        // the second -- unchanged, its columns all filled -- starts at
        // (0, 1): without the guard the two read as one run climbing on.
        let mut out = vec![
            drop_cell(0, 0, ""),
            drop_cell(0, 1, ""),
            drop_cell(0, 2, "x"),
            drop_cell(0, 1, "y"),
            drop_cell(1, 0, "w"),
            drop_cell(1, 1, "v"),
        ];
        let mut origin = vec![0, 1, 2, 3, 4, 5];
        let map = drop_empty_columns_from_tables(&mut out, &mut origin);
        assert_eq!(out.len(), 6, "cells went where two runs would join");
        assert_eq!(origin, vec![0, 1, 2, 3, 4, 5], "the origins moved");
        assert_eq!(
            map,
            vec![Some(0), Some(1), Some(2), Some(3), Some(4), Some(5)],
            "the positions moved"
        );
        assert_eq!(out[2].text, "x", "the page was not put back as it was");
        assert!(
            matches!(
                out[2].kind,
                BlockKind::TableCell {
                    row: 0,
                    column: 2,
                    ..
                }
            ),
            "the page was not put back as it was"
        );
    }

    #[test]
    fn tatr_extent_unions_cells_and_skips_degenerate_boxes() {
        assert_eq!(
            tatr_table_extent(&[
                [0.0, 0.0, 10.0, 10.0],
                [20.0, 5.0, 30.0, 15.0],
                [0.0, 0.0, 0.0, 0.0],
            ]),
            Some([0.0, 0.0, 30.0, 15.0])
        );
        assert_eq!(tatr_table_extent(&[]), None);
        assert_eq!(tatr_table_extent(&[[0.0, 0.0, 0.0, 0.0]]), None);
    }

    #[test]
    fn tatr_coverage_counts_direct_claims_inside_the_extent() {
        let cells = [[0.0, 0.0, 10.0, 20.0], [10.0, 0.0, 20.0, 20.0]];
        let words = [
            [1.0, 15.0, 2.0, 16.0],
            [11.0, 15.0, 12.0, 16.0],
            [5.0, 5.0, 6.0, 6.0],
            [100.0, 100.0, 101.0, 101.0],
        ];
        let extent = tatr_table_extent(&cells).expect("two placed cells");
        // Word 2 sits in the first cell's box but is claimed by none: the
        // guard reads the direct claims, not the geometry.
        assert_eq!(
            tatr_coverage_counts(&words, extent, &[vec![0], vec![1, 99]]),
            (2, 3)
        );
        assert_eq!(
            tatr_coverage_counts(&words, extent, &[vec![], vec![]]),
            (0, 3)
        );
    }

    #[test]
    fn tatr_gap_fill_prefers_the_row_band_then_the_column_band() {
        // Two cells side by side with a gutter between them, one row below.
        let cells = [
            [0.0, 10.0, 10.0, 20.0],
            [20.0, 10.0, 30.0, 20.0],
            [0.0, 0.0, 30.0, 8.0],
        ];
        let extent = tatr_table_extent(&cells).expect("three placed cells");
        let words = [
            [1.0, 15.0, 2.0, 16.0],
            [21.0, 15.0, 22.0, 16.0],
            [16.0, 15.0, 17.0, 16.0],
            [5.0, 4.0, 6.0, 5.0],
            [100.0, 100.0, 101.0, 101.0],
        ];
        let mut claims = vec![vec![0], vec![1], vec![3]];
        tatr_fill_gaps(&words, &cells, extent, &mut claims);
        // Word 2 stands in the gutter in the top row's band: nearer the
        // second cell's edge, so it joins the second cell. Word 4 stands
        // past the extent and is prose, never filled.
        assert_eq!(claims, vec![vec![0], vec![1, 2], vec![3]]);
    }

    #[test]
    fn tatr_gap_fill_falls_back_to_the_nearest_cell() {
        // Two cells on a diagonal; a word inside the extent but past every
        // band still lands.
        let cells = [[0.0, 0.0, 10.0, 10.0], [20.0, 20.0, 30.0, 30.0]];
        let extent = tatr_table_extent(&cells).expect("two placed cells");
        let words = [
            [1.0, 1.0, 2.0, 2.0],
            [15.0, 15.0, 16.0, 16.0],
            [100.0, 100.0, 101.0, 101.0],
        ];
        let mut claims = vec![vec![0], vec![]];
        tatr_fill_gaps(&words, &cells, extent, &mut claims);
        // Word 1 is in no row band and no column band: nearer the second
        // cell's centre than the first's, so it joins the second.
        assert_eq!(claims, vec![vec![0], vec![1]]);
    }

    #[test]
    fn tatr_gap_fill_keeps_word_order_inside_a_cell() {
        let cells = [[0.0, 0.0, 30.0, 10.0]];
        let extent = tatr_table_extent(&cells).expect("one placed cell");
        let words = [
            [1.0, 1.0, 2.0, 2.0],
            [25.0, 1.0, 26.0, 2.0],
            [100.0, 100.0, 101.0, 101.0],
        ];
        // Word 1 claimed directly, word 0 filled past it: the cell reads
        // them in word order, not in claim order.
        let mut claims = vec![vec![1]];
        tatr_fill_gaps(&words, &cells, extent, &mut claims);
        assert_eq!(claims, vec![vec![0, 1]]);
    }

    fn fill_word(text: &str, bbox: [f64; 4], block: usize, span: usize) -> PageWord {
        PageWord {
            text: text.to_string(),
            bbox,
            block,
            span,
            size: 9.0,
            bold: false,
            italic: false,
            font: None,
            color: None,
        }
    }

    fn text_block(text: &str) -> Block {
        Block {
            kind: BlockKind::Paragraph,
            page: 0,
            bbox: [0.0; 4],
            text: text.to_string(),
            spans: Vec::new(),
            index: 0,
            continues_previous: false,
            confidence: None,
            grid: None,
            table_origin: TableOrigin::Other,
            trace_learned: None,
        }
    }

    fn fill_texts(words: &[PageWord], blocks: &[Block]) -> Vec<String> {
        let refs: Vec<&Block> = blocks.iter().collect();
        tatr_fill_words(words, &refs)
            .iter()
            .map(|w| w.text.clone())
            .collect()
    }

    #[test]
    fn tatr_fill_words_reads_the_block_s_joined_text() {
        // Four fragments, two tokens: the word the engine joined is one
        // fill word at the union box, carrying the first fragment's seat.
        let blocks = vec![text_block("Conc Total")];
        let words = vec![
            fill_word("C", [0.0, 0.0, 5.0, 9.0], 0, 0),
            fill_word("o", [5.0, 0.0, 10.0, 9.0], 0, 1),
            fill_word("n", [10.0, 0.0, 15.0, 9.0], 0, 2),
            fill_word("c", [15.0, 0.0, 20.0, 9.0], 0, 3),
            fill_word("Total", [26.0, 0.0, 51.0, 9.0], 0, 4),
        ];
        let refs: Vec<&Block> = blocks.iter().collect();
        let joined = tatr_fill_words(&words, &refs);
        assert_eq!(fill_texts(&words, &blocks), vec!["Conc", "Total"]);
        assert_eq!(joined[0].bbox, [0.0, 0.0, 20.0, 9.0]);
        assert_eq!((joined[0].block, joined[0].span), (0, 0));
        assert_eq!(joined[1].bbox, [26.0, 0.0, 51.0, 9.0]);
    }

    #[test]
    fn tatr_fill_words_joins_overlapping_stacked_fragments() {
        // A landscape table's word, set bottom to top: shared x, boxes an
        // em tall so neighbours overlap.
        let blocks = vec![text_block("MS")];
        let words = vec![
            fill_word("M", [0.0, 0.0, 9.0, 9.0], 0, 0),
            fill_word("S", [0.0, 5.0, 9.0, 14.0], 0, 1),
        ];
        assert_eq!(fill_texts(&words, &blocks), vec!["MS"]);
    }

    #[test]
    fn tatr_fill_words_keeps_daylight_fragments_raw() {
        // A thin space the engine glued: the fragments show daylight, so
        // the token stays raw even though the text joins it -- while the
        // kerned token beside it joins.
        let blocks = vec![text_block("20(76.9%) [363]")];
        let words = vec![
            fill_word("20", [0.0, 0.0, 8.6, 9.0], 0, 0),
            fill_word("(76.9%)", [10.7, 0.0, 40.0, 9.0], 0, 1),
            fill_word("[", [50.0, 0.0, 52.7, 9.0], 0, 2),
            fill_word("363", [52.7, 0.0, 64.2, 9.0], 0, 3),
            fill_word("]", [64.2, 0.0, 66.9, 9.0], 0, 4),
        ];
        assert_eq!(fill_texts(&words, &blocks), vec!["20", "(76.9%)", "[363]"]);
    }

    #[test]
    fn tatr_fill_words_never_joins_across_blocks() {
        let blocks = vec![text_block("ab"), text_block("cd")];
        let words = vec![
            fill_word("a", [0.0, 0.0, 5.0, 9.0], 0, 0),
            fill_word("b", [5.0, 0.0, 10.0, 9.0], 0, 1),
            fill_word("c", [0.0, 0.0, 5.0, 9.0], 1, 0),
            fill_word("d", [5.0, 0.0, 10.0, 9.0], 1, 1),
        ];
        assert_eq!(fill_texts(&words, &blocks), vec!["ab", "cd"]);
    }

    #[test]
    fn tatr_fill_words_keeps_raw_fragments_where_they_do_not_spell_the_text() {
        // Right-to-left ink: the text is reordered, the fragments are in
        // draw order, so they cannot spell it and stay raw.
        let blocks = vec![text_block("ac")];
        let words = vec![
            fill_word("c", [6.0, 0.0, 11.0, 9.0], 0, 0),
            fill_word("a", [0.0, 0.0, 5.0, 9.0], 0, 1),
        ];
        assert_eq!(fill_texts(&words, &blocks), vec!["c", "a"]);
        // An overprint the assembly trimmed: twice the fragments, once the
        // text.
        let blocks = vec![text_block("Conc")];
        let words = vec![
            fill_word("Conc", [0.0, 0.0, 20.0, 9.0], 0, 0),
            fill_word("Conc", [0.0, 0.0, 20.0, 9.0], 0, 1),
        ];
        assert_eq!(fill_texts(&words, &blocks), vec!["Conc", "Conc"]);
        // A word the typesetter broke across a line: the text puts it back
        // together, the fragments still carry the hyphen.
        let blocks = vec![text_block("progressive")];
        let words = vec![
            fill_word("pro-", [0.0, 0.0, 15.0, 9.0], 0, 0),
            fill_word("gressive", [0.0, 0.0, 30.0, 9.0], 0, 1),
        ];
        assert_eq!(fill_texts(&words, &blocks), vec!["pro-", "gressive"]);
        // A block past the page's blocks keeps its fragments too.
        let blocks = vec![text_block("ab")];
        let words = vec![
            fill_word("a", [0.0, 0.0, 5.0, 9.0], 7, 0),
            fill_word("b", [6.0, 0.0, 11.0, 9.0], 7, 1),
        ];
        assert_eq!(fill_texts(&words, &blocks), vec!["a", "b"]);
    }

    fn turned_span(transform: Option<[f64; 6]>) -> TextSpan {
        TextSpan {
            text: "w".to_string(),
            x: 0.0,
            y: 0.0,
            width: 5.0,
            height: 9.0,
            font_size: 9.0,
            transform,
            ..Default::default()
        }
    }

    fn judge_line(block: usize) -> judge::Line {
        judge::Line {
            bbox: [0.0; 4],
            text: String::new(),
            words: 0,
            digits: 0,
            kind: "paragraph",
            table: None,
            grid_fill: f64::NAN,
            grid_columns: f64::NAN,
            block,
            list: f64::NAN,
        }
    }

    #[test]
    fn tatr_span_upright_reads_the_baseline_against_ten_degrees() {
        // Upright, and the 3-degree skew the layout pins: upright.
        assert!(tatr_span_is_upright(&turned_span(Some([
            10.0, 0.0, 0.0, 10.0, 0.0, 0.0
        ]))));
        let lean = 3.0f64.to_radians();
        assert!(tatr_span_is_upright(&turned_span(Some([
            10.0 * lean.cos(),
            10.0 * lean.sin(),
            0.0,
            10.0,
            0.0,
            0.0
        ]))));
        // The quarter turns, and a free angle past ten degrees: turned.
        for coeffs in [
            [0.0, 10.0, -10.0, 0.0, 0.0, 0.0],
            [-10.0, 0.0, 0.0, -10.0, 0.0, 0.0],
            [0.0, -10.0, 10.0, 0.0, 0.0, 0.0],
            [8.66, 5.0, -5.0, 8.66, 0.0, 0.0],
        ] {
            assert!(
                !tatr_span_is_upright(&turned_span(Some(coeffs))),
                "a turned baseline read upright: {coeffs:?}"
            );
        }
        // Hand-built and degenerate spans carry no direction: upright.
        assert!(tatr_span_is_upright(&turned_span(None)));
        assert!(tatr_span_is_upright(&turned_span(Some([
            0.0, 0.0, 0.0, 0.0, 0.0, 0.0
        ]))));
    }

    #[test]
    fn tatr_region_rotated_reads_the_majority_of_its_words() {
        // Two rows in one block: the first upright, the second turned.
        let mut block = text_block("a b c d");
        block.spans = vec![
            turned_span(Some([10.0, 0.0, 0.0, 10.0, 0.0, 0.0])),
            turned_span(Some([10.0, 0.0, 0.0, 10.0, 0.0, 0.0])),
            turned_span(Some([0.0, 10.0, -10.0, 0.0, 0.0, 0.0])),
            turned_span(Some([0.0, 10.0, -10.0, 0.0, 0.0, 0.0])),
        ];
        let refs = vec![&block];
        let words = vec![
            fill_word("a", [0.0, 20.0, 5.0, 29.0], 0, 0),
            fill_word("b", [6.0, 20.0, 11.0, 29.0], 0, 1),
            fill_word("c", [0.0, 0.0, 5.0, 9.0], 0, 2),
            fill_word("d", [6.0, 0.0, 11.0, 9.0], 0, 3),
        ];
        let lines = vec![judge_line(0), judge_line(0)];
        // Both rows: a 2-2 tie is not a majority, so the region fires.
        assert!(!tatr_region_is_rotated(&refs, &lines, &words, &[0, 1]));
        // The turned row alone: rotated.
        assert!(tatr_region_is_rotated(&refs, &lines, &words, &[1]));
        // The upright row alone: not.
        assert!(!tatr_region_is_rotated(&refs, &lines, &words, &[0]));
        // No lines, no words: not rotated, never a decline on nothing.
        assert!(!tatr_region_is_rotated(&refs, &[], &[], &[]));
    }

    #[test]
    fn tatr_for_region_declines_a_rotated_region_before_the_fake() {
        // A firing fake over a turned region still falls back, with the
        // trace's reason; the upright twin fires.
        for turned in [true, false] {
            let mut block = text_block("a b c");
            let coeffs = if turned {
                [0.0, 10.0, -10.0, 0.0, 0.0, 0.0]
            } else {
                [10.0, 0.0, 0.0, 10.0, 0.0, 0.0]
            };
            block.spans = vec![
                turned_span(Some(coeffs)),
                turned_span(Some(coeffs)),
                turned_span(Some(coeffs)),
            ];
            let refs = vec![&block];
            let words = vec![
                fill_word("a", [0.0, 0.0, 5.0, 9.0], 0, 0),
                fill_word("b", [6.0, 0.0, 11.0, 9.0], 0, 1),
                fill_word("c", [12.0, 0.0, 17.0, 9.0], 0, 2),
            ];
            let lines = vec![judge_line(0)];
            let pair_words = vec![pairs::PairWord {
                text: "a b c".to_string(),
                bbox: [0.0, 0.0, 17.0, 9.0],
                table: None,
                row: None,
                column: None,
            }];
            let options = TatrOptions {
                test_hook: Some(TatrTestHook::Cells(vec![TatrTestCell::new(
                    vec![0],
                    vec![0],
                    false,
                    vec![0, 1, 2],
                )])),
                ..Default::default()
            };
            let context = TatrContext {
                document: None,
                options,
            };
            let region = TatrRegion::of(&context, false);
            match tatr_for_region(Some(&region), &refs, &lines, &words, &[0], &pair_words, 0) {
                TatrVerdict::Fallback(reason) => {
                    assert!(turned, "the upright twin declined: {reason}");
                    assert_eq!(reason, "rotated", "the trace's reason drifted");
                }
                TatrVerdict::Cells(_) => {
                    assert!(!turned, "the rotated region fired past the decline")
                }
            }
        }
    }
}
