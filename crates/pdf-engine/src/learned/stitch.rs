// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Stitch a table across its wrapped labels and header rows (#574).
//!
//! WHERE AND WHY. [`super::tables::judge_regions`] calls this after
//! [`super::judge::verdict`] chooses, before blocks are assigned to tables.
//! The judge cuts tables short where a row label wraps over two or three lines
//! with the figures on its last one, where a section label row carries no
//! figures, or where year headers stand above the first figure row: the rest
//! of the statement becomes a second table, or prose. This pass works on the
//! judge's own line sets and changes only which lines a chosen set holds; it
//! proposes no region and touches no grid.
//!
//! LINES AND WORDS. A judge [`Line`][super::judge::Line] carries a bbox and
//! joined text; the stitch reads each line's words from the page's
//! [`PageWord`][super::features::PageWord]s, regrouped with
//! [`lines_of`][super::lines::lines_of] -- the same grouping the judge read,
//! so line indices align one to one. Where they do not, the page is left
//! alone.
//!
//! PIECES, NOT ROWS. A line here is a `lines_of` piece: a statement row whose
//! label stands a column gap from its figures arrives as two or more lines at
//! the same height. The walk vets same-height lines left to right, so a label
//! piece is held before its figure piece joins and commits it; the pitch
//! counts only positive gaps between tops, so paired pieces do not read as a
//! zero pitch.
//!
//! WHAT COUNTS AS A FIGURE. A figure token has the shape the table needs --
//! digits, currency, `%`, parentheses, dashes standing for nil -- and stands
//! where a figure stands: first on its piece, or a word space past the word
//! before it ([`FIGURE_GAP_EM`]; leader dots are looked through), or after
//! another figure. Without the position half, a label ending in digits (`Note
//! 12`) would drag the stub edge into the labels and every label would read as
//! prose crossing it; the stub reads only clustered tokens, so even a loose
//! digit that counts cannot drag it alone.
//!
//! THE WALK. Per chosen set, from its figure columns (right-edge clusters with
//! [`MIN_COLUMN_SUPPORT`] lines behind them), its stub edge (the smallest
//! figure left edge less [`STUB_INSET_PT`]), its pitch (the median positive
//! gap between tops) and its median text size: unclaimed lines x-overlapping
//! the hull are vetted outward, downward then upward, nearest first. Downward
//! a line at most [`GAP_FACTOR`] pitches out joins as a label-only line (held
//! until a figure row commits the tight chain back from it, the rest dropped
//! at the walk's end) or a figure row; upward it joins as a header row, a
//! figure row, or the stub cell of either on the same line -- or a lone
//! stub-only line is held, and only a header or figure row joining above
//! commits it, the section label under the column heads. A second consecutive
//! stub-only line caps the table at its top edge, as does a held line nothing
//! commits, released at the walk's end. Vetting for a merge the same line vets
//! like any label, so the walk still crosses it to the set beyond. A header row
//! is a year row whatever its alignment, or words inside the columns' span
//! grouped per column with at most [`HEADER_MAX_WORDS`] a group, or a group
//! spanning two or more columns with a label centred on the columns between
//! its neighbours. An oversize line, a
//! short caption, a downward header or year row, and unaligned figures are
//! stepped over without joining; prose crossing the stub, an outdented label,
//! a second consecutive stub-only line upward while absorbing, and a wider gap
//! end the walk.
//! Sets are walked bottom-up so a lower set has taken its own header before an
//! upper set may reach it.
//!
//! THE MERGE. Where the walk reaches another chosen set whose columns match --
//! [`MERGE_MIN_COLUMNS`] within [`TOL_PT`], [`MERGE_MIN_SHARE`] of them -- the
//! two become one with the lines between, and the walk goes on. No merge
//! across a title, a caption, or a header row between them, and none into a
//! set that starts with its own header: two stacked statements with the same
//! columns stay two.
//!
//! THREE SWITCHES. Absorbing headers upward (`table_stitch_header`), absorbing
//! rows downward (`table_stitch_rows`), and merging sets (`table_stitch_merge`)
//! are measured apart: where its switch is off, the walk vets without claiming
//! and only a merge takes the vetted lines between.
//!
//! EVERY THRESHOLD below is a named constant, each with a test at its boundary
//! value.

use super::features::PageWord;
use super::judge::Line;
use super::lines::lines_of;

/// The geometric tolerance, in points: figure right edges cluster and match
/// within it, and a label may start this far left of the table's edge.
const TOL_PT: f64 = 4.0;
/// How far left of the leftmost figure token the stub edge stands, in points:
/// a gutter, so a label ending where the figures start still ends left of it.
const STUB_INSET_PT: f64 = 2.0;
/// How large the gap to the next line may be, in pitches of the set's own
/// lines, before the walk ends.
const GAP_FACTOR: f64 = 1.6;
/// How much larger than the set's median text size a line may be and still
/// join; above it the line is a heading and never joins.
const TEXT_GROWTH: f64 = 1.15;
/// How many words may stand between two figure columns in an upward header row
/// before it reads as a sentence.
const HEADER_MAX_WORDS: usize = 4;
/// How many of the reached set's figure columns must stand on this set's
/// before the two merge (with [`MERGE_MIN_SHARE`] of them).
const MERGE_MIN_COLUMNS: usize = 2;
/// What share of the reached set's figure columns must stand on this set's
/// before the two merge.
const MERGE_MIN_SHARE: f64 = 0.6;
/// How many lines must carry a figure column before it is one.
const MIN_COLUMN_SUPPORT: usize = 2;
/// How far past the previous word on its piece, in ems of the line's own
/// size, a figure-shaped token must stand to count as a figure; closer and it
/// is label text that happens to hold digits (`Note 12`). A word space is
/// about a third of an em, a column gap more than one.
const FIGURE_GAP_EM: f64 = 0.6;
/// How close a held label must stand to the figures that commit it, in
/// pitches: a wrap continues line to line, while a title breathes two pitches
/// below it. Held lines past this stay pending to the walk's end, so the
/// figures join without them.
const HELD_COMMIT_PITCHES: f64 = 1.0;
/// A row of four-digit years between this and [`YEAR_MAX`] heads a statement;
/// outside it they are amounts.
const YEAR_MIN: u32 = 1800;
/// See [`YEAR_MIN`].
const YEAR_MAX: u32 = 2199;

/// Float dust for edge comparisons; not a threshold.
const EPSILON: f64 = 1e-6;

/// Punctuation a figure token may be made of where it holds no digits:
/// currency, `%`, parentheses, dashes standing for nil, decimal marks.
const FIGURE_PUNCT: &str = "$€£¥¢₹%‰()[]+-–—−.,";
/// Leader fill looked through when the previous word is measured: the amount
/// past the dots is a figure whatever space the dots leave.
const LEADER_DOTS: &str = ".·…";

/// Whether the token has a figure's shape: a digit anywhere, or nothing but
/// [`FIGURE_PUNCT`].
pub(crate) fn is_figure_shape(text: &str) -> bool {
    !text.is_empty()
        && (text.chars().any(|c| c.is_ascii_digit())
            || text.chars().all(|c| FIGURE_PUNCT.contains(c)))
}

/// Whether the token is leader fill rather than a word.
pub(crate) fn is_leader_dots(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| LEADER_DOTS.contains(c))
}

/// Whether the token is a year a statement would head its columns with.
pub(crate) fn is_year(text: &str) -> bool {
    text.len() == 4
        && text.chars().all(|c| c.is_ascii_digit())
        && text
            .parse::<u32>()
            .is_ok_and(|year| (YEAR_MIN..=YEAR_MAX).contains(&year))
}

/// Whether the line's text opens as a captioned table or figure: `Table 2`,
/// `Figure 12`, `Fig. 3` -- a keyword and a number, so `Table of contents` is
/// not one.
fn is_caption(text: &str) -> bool {
    let lower = text.trim_start().to_lowercase();
    // Longest first, so `figures` wins over `fig`.
    for key in [
        "figures", "figure", "tables", "table", "figs.", "fig.", "figs", "fig",
    ] {
        if let Some(rest) = lower.strip_prefix(key) {
            let rest = rest.trim_start_matches([' ', '\t', '(', '[']);
            return rest.chars().next().is_some_and(|c| c.is_ascii_digit());
        }
    }
    false
}

/// Whether the piece reads bold: most of its words in a bold face.
fn is_bold(piece: &[&PageWord]) -> bool {
    !piece.is_empty() && piece.iter().filter(|w| w.bold).count() * 2 > piece.len()
}

/// The middle value, upper of the two in the middle; 0.0 of nothing.
fn median(mut values: Vec<f64>) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

/// Which words of a piece count as figures: figure-shaped, letter-free, not
/// leader fill, and opening the piece, [`FIGURE_GAP_EM`] past the previous
/// non-dot word, or after another figure -- and symmetrically followed by a
/// figure or a gap, not by label text at a word space. Piece-first is exact,
/// not a default: pieces break at 1.5 em gaps, so a word opening one stands
/// past a column gap or opens its row. The neighbours are what keep `Note 12`
/// and `Q1 sales` labels while tight figures (`9,898 2,240` at half an em)
/// still count.
fn candidacy(piece: &[&PageWord], size: f64) -> Vec<bool> {
    let em = if size <= 0.0 { 1.0 } else { size };
    let shaped = |word: &PageWord| {
        is_figure_shape(&word.text)
            && !is_leader_dots(&word.text)
            && !word.text.chars().any(|c| c.is_ascii_alphabetic())
    };
    let mut out = vec![false; piece.len()];
    let mut prev: Option<usize> = None;
    for (j, word) in piece.iter().enumerate() {
        out[j] = shaped(word)
            && prev
                .is_none_or(|p| out[p] || (word.bbox[0] - piece[p].bbox[2]) / em >= FIGURE_GAP_EM);
        if !is_leader_dots(&word.text) {
            prev = Some(j);
        }
    }
    let mut next: Option<usize> = None;
    for j in (0..piece.len()).rev() {
        out[j] = out[j]
            && next.is_none_or(|n| {
                out[n] || (piece[n].bbox[0] - piece[j].bbox[2]) / em >= FIGURE_GAP_EM
            });
        if !is_leader_dots(&piece[j].text) {
            next = Some(j);
        }
    }
    out
}

/// A set's figure columns and stub edge from its figure tokens as `(left,
/// right, line)`: right edges clustered with spans of at most [`TOL_PT`], a
/// column the mean of a cluster with [`MIN_COLUMN_SUPPORT`] lines behind it.
/// The stub is the smallest left edge among the clustered tokens less
/// [`STUB_INSET_PT`], not among all of them: a lone digit far left (`Note 12`
/// set loose) is no column and must not drag the edge into the labels.
fn figure_columns(tokens: &[(f64, f64, usize)]) -> (Vec<f64>, f64) {
    let mut edges = tokens.to_vec();
    edges.sort_by(|a, b| a.1.total_cmp(&b.1));
    let mut columns = Vec::new();
    let mut lefts: Vec<f64> = Vec::new();
    let mut start = 0;
    while start < edges.len() {
        let mut end = start + 1;
        while end < edges.len() && edges[end].1 - edges[start].1 <= TOL_PT {
            end += 1;
        }
        let mut supporting: Vec<usize> =
            edges[start..end].iter().map(|&(_, _, line)| line).collect();
        supporting.sort_unstable();
        supporting.dedup();
        if supporting.len() >= MIN_COLUMN_SUPPORT {
            let mean = edges[start..end]
                .iter()
                .map(|&(_, right, _)| right)
                .sum::<f64>()
                / (end - start) as f64;
            columns.push(mean);
            lefts.extend(edges[start..end].iter().map(|&(left, _, _)| left));
        }
        start = end;
    }
    let stub = lefts.into_iter().fold(f64::INFINITY, f64::min) - STUB_INSET_PT;
    (columns, stub)
}

/// The figure columns a spanning header run stands for: every column strictly
/// between its neighbouring runs' ink, or every column where it stands alone
/// on its line. A centred label over eight figure columns spans eight, not
/// the three its own words happen to cover; with header groups left and right
/// it spans only the columns between them.
fn spanning_columns(prev_x1: f64, next_x0: f64, columns: &[f64]) -> Vec<f64> {
    columns
        .iter()
        .copied()
        .filter(|&c| prev_x1 < c && c < next_x0)
        .collect()
}

/// What a chosen set walks with: its figure columns, stub edge, pitch,
/// median text size, and whether its lines read bold -- all read once from
/// its own lines.
struct Params {
    columns: Vec<f64>,
    stub: f64,
    pitch: f64,
    size: f64,
    bold: bool,
}

/// One chosen set while the pass runs.
struct Live {
    members: Vec<usize>,
    hull: [f64; 4],
    /// The claimed edges: what the walk may look past.
    bottom: f64,
    top: f64,
    /// The walked edges: claimed lines plus held or vetted ones, what gaps
    /// are measured from. A three-line wrap puts its figures two pitches past
    /// the set's edge; the held label lines lend the walk their reach, and
    /// where no figure row commits them the edges go back.
    walk_bottom: f64,
    walk_top: f64,
    /// The median text size of its lines as chosen, fixed for the walk.
    size: f64,
    /// Whether most of its lines as chosen read bold.
    bold: bool,
    /// Whether it absorbed a header row upward: it starts with its own
    /// header, and no set merges into that head.
    headed: bool,
    consumed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Dir {
    Down,
    Up,
}

/// What the walk does with a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    /// It joins now: a figure row, or a label-only line upward.
    Join,
    /// It joins upward as a header row and heads the set.
    Header,
    /// It is label-only downward, held until a figure row commits it -- or a
    /// lone stub-only line upward, held until a header or figure row above
    /// commits it.
    Hold,
    /// It never joins but the walk goes on past it.
    Skip,
    /// It ends the walk: prose, an outdented label, a full-width caption.
    Stop,
}

struct Walker<'a> {
    lines: &'a [Line],
    pieces: Vec<Vec<&'a PageWord>>,
    sizes: Vec<f64>,
    candidates: Vec<Vec<bool>>,
    texts: Vec<String>,
    sets: Vec<Live>,
    owner: Vec<Option<usize>>,
    absorb_header: bool,
    absorb_rows: bool,
    merge: bool,
    split_stacked: bool,
}

impl<'a> Walker<'a> {
    fn new(
        lines: &'a [Line],
        pieces: Vec<Vec<&'a PageWord>>,
        chosen: &[Vec<usize>],
        absorb_header: bool,
        absorb_rows: bool,
        merge: bool,
        split_stacked: bool,
    ) -> Self {
        let sizes: Vec<f64> = pieces
            .iter()
            .zip(lines.iter())
            .map(|(piece, line)| {
                if piece.is_empty() {
                    line.bbox[3] - line.bbox[1]
                } else {
                    median(piece.iter().map(|w| w.size).collect())
                }
            })
            .collect();
        let candidates: Vec<Vec<bool>> = pieces
            .iter()
            .zip(sizes.iter())
            .map(|(piece, &size)| candidacy(piece, size))
            .collect();
        // The lines as their words read: the judge's joined text spaces every
        // glyph fragment out, which no caption opens with.
        let texts: Vec<String> = pieces
            .iter()
            .map(|piece| {
                piece
                    .iter()
                    .map(|word| word.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();
        let mut sets = Vec::with_capacity(chosen.len());
        let mut owner = vec![None; lines.len()];
        for set in chosen {
            let mut hull = [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ];
            for &i in set {
                let b = lines[i].bbox;
                hull[0] = hull[0].min(b[0]);
                hull[1] = hull[1].min(b[1]);
                hull[2] = hull[2].max(b[2]);
                hull[3] = hull[3].max(b[3]);
                owner[i] = Some(sets.len());
            }
            let bold = set.iter().filter(|&&i| is_bold(&pieces[i])).count() * 2 > set.len();
            sets.push(Live {
                members: set.clone(),
                hull,
                bottom: hull[1],
                top: hull[3],
                walk_bottom: hull[1],
                walk_top: hull[3],
                size: median(set.iter().map(|&i| sizes[i]).collect()),
                bold,
                headed: false,
                consumed: false,
            });
        }
        Self {
            lines,
            pieces,
            sizes,
            candidates,
            texts,
            sets,
            owner,
            absorb_header,
            absorb_rows,
            merge,
            split_stacked,
        }
    }

    /// Whether the walk in `dir` claims the lines it takes: upward the
    /// header switch, downward the rows switch. Where it does not, a taken
    /// line is only vetted, and only a merge takes it.
    fn claims(&self, dir: Dir) -> bool {
        match dir {
            Dir::Down => self.absorb_rows,
            Dir::Up => self.absorb_header,
        }
    }

    /// The walking parameters of a set, or `None` where it has no figure
    /// columns or no pitch to walk with: it is left alone.
    fn params_of(&self, set: usize) -> Option<Params> {
        let (columns, stub) = figure_columns(&self.figure_tokens_of(set));
        if columns.is_empty() {
            return None;
        }
        let mut tops: Vec<f64> = self.sets[set]
            .members
            .iter()
            .map(|&i| self.lines[i].bbox[3])
            .collect();
        tops.sort_by(f64::total_cmp);
        let gaps: Vec<f64> = tops
            .windows(2)
            .map(|pair| pair[1] - pair[0])
            .filter(|&gap| gap > 0.0)
            .collect();
        // Paired pieces of one row share a top; the pitch is the row step, so
        // only positive gaps count. A single line falls back to its height.
        let pitch = if gaps.is_empty() {
            median(
                self.sets[set]
                    .members
                    .iter()
                    .map(|&i| self.lines[i].bbox[3] - self.lines[i].bbox[1])
                    .collect(),
            )
        } else {
            median(gaps)
        };
        if pitch <= 0.0 {
            return None;
        }
        Some(Params {
            columns,
            stub,
            pitch,
            size: self.sets[set].size,
            bold: self.sets[set].bold,
        })
    }

    /// The set's figure tokens as `(left, right, line)`.
    fn figure_tokens_of(&self, set: usize) -> Vec<(f64, f64, usize)> {
        let mut tokens = Vec::new();
        for &i in &self.sets[set].members {
            for (j, word) in self.pieces[i].iter().enumerate() {
                if self.candidates[i][j] {
                    tokens.push((word.bbox[0], word.bbox[2], i));
                }
            }
        }
        tokens
    }

    /// The next line to vet in `dir`: x-overlapping the hull, not the set's
    /// own, not vetted yet. First whatever shares the last vetted line's row,
    /// left to right, so joining one piece never hides its siblings past the
    /// moved edge; then the nearest line past the set's edge, same-height
    /// lines left to right so a label piece is held before its figure piece
    /// joins.
    fn nearest(
        &self,
        set: usize,
        dir: Dir,
        seen: &[bool],
        active: Option<[f64; 4]>,
    ) -> Option<usize> {
        let hull = self.sets[set].hull;
        let (bottom, top) = (self.sets[set].bottom, self.sets[set].top);
        let mut best: Option<usize> = None;
        let mut best_row = false;
        for (i, line) in self.lines.iter().enumerate() {
            if self.owner[i] == Some(set) || seen[i] {
                continue;
            }
            let b = line.bbox;
            if b[0] > hull[2] || b[2] < hull[0] {
                continue;
            }
            let shares_row = active.is_some_and(|a| b[1] < a[3] && b[3] > a[1]);
            let eligible = shares_row
                || match dir {
                    Dir::Down => b[3] <= bottom + EPSILON,
                    Dir::Up => b[1] >= top - EPSILON,
                };
            if !eligible {
                continue;
            }
            let better = match (shares_row, best) {
                (true, Some(k)) if best_row => b[0] < self.lines[k].bbox[0],
                (true, _) => true,
                (false, Some(_)) if best_row => false,
                (false, _) => match dir {
                    Dir::Down => {
                        best.is_none_or(|k| match b[3].total_cmp(&self.lines[k].bbox[3]) {
                            std::cmp::Ordering::Greater => true,
                            std::cmp::Ordering::Equal => b[0] < self.lines[k].bbox[0],
                            std::cmp::Ordering::Less => false,
                        })
                    }
                    Dir::Up => best.is_none_or(|k| match b[1].total_cmp(&self.lines[k].bbox[1]) {
                        std::cmp::Ordering::Less => true,
                        std::cmp::Ordering::Equal => b[0] < self.lines[k].bbox[0],
                        std::cmp::Ordering::Greater => false,
                    }),
                },
            };
            if better {
                best = Some(i);
                best_row = shares_row;
            }
        }
        best
    }

    /// Whether every word right of the stub has a figure column at its right
    /// edge. True of no right words; callers with a stub-only line decide
    /// for it before asking.
    fn is_figure_row(&self, line: usize, right: &[usize], params: &Params) -> bool {
        right.iter().all(|&j| {
            self.candidates[line][j]
                && params
                    .columns
                    .iter()
                    .any(|&c| (self.pieces[line][j].bbox[2] - c).abs() <= TOL_PT)
        })
    }

    /// Whether the words right of the stub read as a heading over the
    /// columns: each within their span, and every run between two columns a
    /// per-column group of at most [`HEADER_MAX_WORDS`] words -- or a group
    /// spanning two or more columns with a centred label, a spanning header.
    /// A row of years is one whatever its alignment. False of no right
    /// words: a header hangs over the columns, and a stub-only line is a
    /// title or a note, never a header.
    fn is_header_row(&self, line: usize, right: &[usize], params: &Params) -> bool {
        if right.is_empty() {
            return false;
        }
        let (Some(&first), Some(&last)) = (params.columns.first(), params.columns.last()) else {
            return false;
        };
        if right.iter().any(|&j| {
            let b = self.pieces[line][j].bbox;
            b[0] < first - TOL_PT || b[2] > last + TOL_PT
        }) {
            return false;
        }
        if right.iter().all(|&j| is_year(&self.pieces[line][j].text)) {
            return true;
        }
        let mut order = right.to_vec();
        order.sort_by(|&a, &b| {
            self.pieces[line][a].bbox[0].total_cmp(&self.pieces[line][b].bbox[0])
        });
        let mut run: Vec<usize> = Vec::new();
        let mut runs: Vec<Vec<usize>> = Vec::new();
        for &j in &order {
            if let Some(&p) = run.last() {
                let between = params
                    .columns
                    .iter()
                    .any(|&c| self.pieces[line][p].bbox[2] < c && c < self.pieces[line][j].bbox[0]);
                if between {
                    runs.push(std::mem::take(&mut run));
                }
            }
            run.push(j);
        }
        runs.push(run);
        let extents: Vec<(f64, f64)> = runs
            .iter()
            .map(|run| {
                let x0 = run
                    .iter()
                    .map(|&j| self.pieces[line][j].bbox[0])
                    .fold(f64::INFINITY, f64::min);
                let x1 = run
                    .iter()
                    .map(|&j| self.pieces[line][j].bbox[2])
                    .fold(f64::NEG_INFINITY, f64::max);
                (x0, x1)
            })
            .collect();
        runs.iter().enumerate().all(|(k, run)| {
            let prev = extents[..k]
                .last()
                .map(|&(_, x1)| x1)
                .unwrap_or(f64::NEG_INFINITY);
            let next = extents
                .get(k + 1)
                .map(|&(x0, _)| x0)
                .unwrap_or(f64::INFINITY);
            self.is_header_group(line, run, prev, next, params)
        })
    }

    /// Whether one run of header words reads as a per-column group or a
    /// spanning header: over at most one column it may hold
    /// [`HEADER_MAX_WORDS`] words; over two or more its label must sit
    /// centred on the columns it spans -- every column between its neighbours,
    /// or every column standing alone -- within [`TOL_PT`], whatever its
    /// length. A wide run off centre is a title, not a header.
    fn is_header_group(
        &self,
        line: usize,
        run: &[usize],
        prev_x1: f64,
        next_x0: f64,
        params: &Params,
    ) -> bool {
        let x0 = run
            .iter()
            .map(|&j| self.pieces[line][j].bbox[0])
            .fold(f64::INFINITY, f64::min);
        let x1 = run
            .iter()
            .map(|&j| self.pieces[line][j].bbox[2])
            .fold(f64::NEG_INFINITY, f64::max);
        let covered: Vec<f64> = params
            .columns
            .iter()
            .copied()
            .filter(|&c| x0 - EPSILON <= c && c <= x1 + EPSILON)
            .collect();
        if covered.len() < 2 {
            return run.len() <= HEADER_MAX_WORDS;
        }
        let span = spanning_columns(prev_x1, next_x0, &params.columns);
        let (first, last) = match (span.first(), span.last()) {
            (Some(&first), Some(&last)) => (first, last),
            _ => (
                covered.first().copied().unwrap_or(x0),
                covered.last().copied().unwrap_or(x1),
            ),
        };
        ((x0 + x1) / 2.0 - (first + last) / 2.0).abs() <= TOL_PT
    }

    /// Whether the stub-only line shares its row with a header or figure
    /// piece: same block, overlapping height, words right of the stub that
    /// read as a header or figure row. That stub cell joins with its row;
    /// without such a sibling the line stands alone.
    fn shares_row_with_table(&self, line: usize, params: &Params) -> bool {
        let (block, bbox) = (self.lines[line].block, self.lines[line].bbox);
        (0..self.lines.len()).any(|j| {
            j != line
                && self.lines[j].block == block
                && self.lines[j].bbox[1] < bbox[3]
                && self.lines[j].bbox[3] > bbox[1]
                && {
                    let right: Vec<usize> = (0..self.pieces[j].len())
                        .filter(|&k| self.pieces[j][k].bbox[2] > params.stub)
                        .collect();
                    !right.is_empty()
                        && (self.is_header_row(j, &right, params)
                            || self.is_figure_row(j, &right, params))
                }
        })
    }

    /// Whether the right words read as prose crossing the stub: a non-figure
    /// among them and no header over the columns.
    fn reads_as_prose(&self, line: usize, right: &[usize], params: &Params) -> bool {
        !self.is_header_row(line, right, params)
            && right
                .iter()
                .any(|&j| !is_figure_shape(&self.pieces[line][j].text))
    }

    /// What the walk does with a line in reach. `held_stub` tells whether the
    /// walk already holds a stub-only line it has not committed: a second
    /// consecutive one upward caps the walk.
    fn classify(
        &self,
        line: usize,
        params: &Params,
        dir: Dir,
        hull: [f64; 4],
        held_stub: bool,
    ) -> Class {
        let right: Vec<usize> = (0..self.pieces[line].len())
            .filter(|&j| self.pieces[line][j].bbox[2] > params.stub)
            .collect();
        // A caption is a boundary marker, never table content: full-width it
        // reads as prose and ends the walk, short it is stepped over and the
        // merge guard refuses to join across it.
        if is_caption(&self.texts[line]) {
            return if self.reads_as_prose(line, &right, params) {
                Class::Stop
            } else {
                Class::Skip
            };
        }
        if self.sizes[line] > TEXT_GROWTH * params.size {
            return Class::Skip;
        }
        if right.is_empty() {
            if self.lines[line].bbox[0] >= hull[0] - TOL_PT {
                if dir == Dir::Down {
                    return Class::Hold;
                }
                // Vetting for a merge the line is only crossed, never taken,
                // so it vets like any label; a bold one over a regular table
                // still caps the walk, as it always did. Only while absorbing
                // does a lone stub-only line cap the table at its top edge.
                if !self.claims(dir) {
                    if !params.bold && is_bold(&self.pieces[line]) {
                        return Class::Stop;
                    }
                    return Class::Join;
                }
                // Upward the stub cell of a header or figure row joins with
                // its row; a lone stub-only line is held, and only a header
                // or figure row joining above commits it -- the section label
                // under the column heads. A second consecutive one is a title
                // over that label and caps the table, as does the bare title
                // nothing commits, released at the walk's end.
                return if self.shares_row_with_table(line, params) {
                    Class::Join
                } else if held_stub {
                    Class::Stop
                } else {
                    Class::Hold
                };
            }
            return Class::Stop;
        }
        if right.iter().all(|&j| is_year(&self.pieces[line][j].text)) {
            return if dir == Dir::Down {
                Class::Skip
            } else {
                Class::Header
            };
        }
        if self.is_figure_row(line, &right, params) {
            return Class::Join;
        }
        if self.is_header_row(line, &right, params) {
            return if dir == Dir::Down {
                Class::Skip
            } else {
                Class::Header
            };
        }
        if right
            .iter()
            .all(|&j| is_figure_shape(&self.pieces[line][j].text))
        {
            return Class::Skip;
        }
        Class::Stop
    }

    /// Take a line into the set: claimed where the direction's switch is on,
    /// tracked where it only vets. A claim moves every edge; a tracked line
    /// only lends the walk its reach, and the edges go back where no merge
    /// takes it.
    fn take(&mut self, set: usize, line: usize, dir: Dir, vetted: &mut Vec<usize>) {
        if self.claims(dir) {
            self.claim(set, line);
        } else {
            vetted.push(line);
            self.stretch_walk(set, line);
        }
    }

    /// Lend the walk a held or vetted line's reach without claiming it.
    fn lend(&mut self, set: usize, line: usize) {
        self.stretch_walk(set, line);
    }

    fn claim(&mut self, set: usize, line: usize) {
        self.owner[line] = Some(set);
        self.sets[set].members.push(line);
        let b = self.lines[line].bbox;
        let live = &mut self.sets[set];
        live.hull[0] = live.hull[0].min(b[0]);
        live.hull[1] = live.hull[1].min(b[1]);
        live.hull[2] = live.hull[2].max(b[2]);
        live.hull[3] = live.hull[3].max(b[3]);
        live.bottom = live.bottom.min(b[1]);
        live.top = live.top.max(b[3]);
        live.walk_bottom = live.walk_bottom.min(b[1]);
        live.walk_top = live.walk_top.max(b[3]);
    }

    fn stretch_walk(&mut self, set: usize, line: usize) {
        let b = self.lines[line].bbox;
        let live = &mut self.sets[set];
        live.hull[0] = live.hull[0].min(b[0]);
        live.hull[1] = live.hull[1].min(b[1]);
        live.hull[2] = live.hull[2].max(b[2]);
        live.hull[3] = live.hull[3].max(b[3]);
        live.walk_bottom = live.walk_bottom.min(b[1]);
        live.walk_top = live.walk_top.max(b[3]);
    }

    fn checkpoint_of(&self, set: usize) -> (f64, f64, f64, f64, [f64; 4]) {
        let live = &self.sets[set];
        (
            live.bottom,
            live.top,
            live.walk_bottom,
            live.walk_top,
            live.hull,
        )
    }

    fn restore(&mut self, set: usize, point: (f64, f64, f64, f64, [f64; 4])) {
        let live = &mut self.sets[set];
        live.bottom = point.0;
        live.top = point.1;
        live.walk_bottom = point.2;
        live.walk_top = point.3;
        live.hull = point.4;
    }

    /// Commit the held stub-only line a header or figure row joins above:
    /// the upward mirror of [`commit_chain`][Self::commit_chain], the tight
    /// chain from the set's top edge up to the joining row, each within
    /// [`HELD_COMMIT_PITCHES`] of the next. At most one line is ever held
    /// upward -- a second consecutive stub-only line stops the walk -- so the
    /// chain is one link either side of it; where either link breathes past a
    /// pitch the line stays pending to the walk's end and the row joins
    /// without it. `edge` is the set's top edge before the joining row was
    /// taken: the take moves it past the held line.
    fn commit_chain_up(
        &mut self,
        set: usize,
        joiner: usize,
        params: &Params,
        held: &mut Vec<usize>,
        edge: f64,
    ) {
        let mut cursor = edge;
        for &h in held.iter() {
            if self.lines[h].bbox[1] - cursor > HELD_COMMIT_PITCHES * params.pitch {
                return;
            }
            cursor = self.lines[h].bbox[3];
        }
        if self.lines[joiner].bbox[1] - cursor > HELD_COMMIT_PITCHES * params.pitch {
            return;
        }
        for h in std::mem::take(held) {
            self.claim(set, h);
        }
    }

    /// Commit the held labels a figure row joins after: the tight chain back
    /// from the figure, each within [`HELD_COMMIT_PITCHES`] of the next. A
    /// wrap commits whole; a label left two pitches behind -- a title the
    /// walk stepped under -- stays pending to the walk's end. Only a merge
    /// takes every held line, with the lines between.
    fn commit_chain(&mut self, set: usize, figure: usize, params: &Params, held: &mut Vec<usize>) {
        let mut cursor = self.lines[figure].bbox[3];
        while let Some(&h) = held.last() {
            if self.lines[h].bbox[1] - cursor > HELD_COMMIT_PITCHES * params.pitch {
                break;
            }
            held.pop();
            self.claim(set, h);
            cursor = self.lines[h].bbox[3];
        }
    }

    /// One walk outward from a set's edge: downward then upward per set, each
    /// set bottom-up across the page.
    fn walk(&mut self, set: usize, dir: Dir) {
        let params = match self.params_of(set) {
            Some(params) => params,
            None => return,
        };
        let mut held: Vec<usize> = Vec::new();
        let mut vetted: Vec<usize> = Vec::new();
        let mut seen = vec![false; self.lines.len()];
        // Held and vetted lines lend the walk their reach; where no figure
        // row or merge takes them the edges go back, so trailing labels leave
        // nothing behind. The checkpoint moves past every claim.
        let mut checkpoint = self.checkpoint_of(set);
        let mut active: Option<[f64; 4]> = None;
        loop {
            let Some(i) = self.nearest(set, dir, &seen, active) else {
                break;
            };
            seen[i] = true;
            active = Some(self.lines[i].bbox);
            let gap = match dir {
                Dir::Down => self.sets[set].walk_bottom - self.lines[i].bbox[3],
                Dir::Up => self.lines[i].bbox[1] - self.sets[set].walk_top,
            };
            if gap > GAP_FACTOR * params.pitch {
                break;
            }
            if let Some(other) = self.owner[i] {
                if self.merge && self.try_merge(set, other, dir, &params, &held, &vetted) {
                    held.clear();
                    vetted.clear();
                    checkpoint = self.checkpoint_of(set);
                    continue;
                }
                break;
            }
            match self.classify(i, &params, dir, self.sets[set].hull, !held.is_empty()) {
                Class::Join => {
                    let edge = self.sets[set].top;
                    self.take(set, i, dir, &mut vetted);
                    if self.claims(dir) {
                        match dir {
                            Dir::Down => self.commit_chain(set, i, &params, &mut held),
                            Dir::Up => self.commit_chain_up(set, i, &params, &mut held, edge),
                        }
                        checkpoint = self.checkpoint_of(set);
                    }
                }
                Class::Header => {
                    let edge = self.sets[set].top;
                    self.take(set, i, dir, &mut vetted);
                    self.sets[set].headed = true;
                    if self.claims(dir) {
                        // Headers only join upward; downward they skip.
                        if dir == Dir::Up {
                            self.commit_chain_up(set, i, &params, &mut held, edge);
                        }
                        checkpoint = self.checkpoint_of(set);
                    }
                }
                Class::Hold => {
                    self.lend(set, i);
                    if self.claims(dir) {
                        held.push(i);
                    } else {
                        vetted.push(i);
                    }
                }
                Class::Skip => {}
                Class::Stop => break,
            }
        }
        self.restore(set, checkpoint);
    }

    /// Whether a title, a caption, or a header row lies between the two sets'
    /// facing edges: kinds the walk stepped over, read against the walking
    /// set's columns. Members of either set do not count: a label the walk
    /// took is content, not a boundary.
    fn blocked_between(&self, walker: usize, reached: usize, dir: Dir, params: &Params) -> bool {
        let hull = self.sets[walker].hull;
        let (low, high) = match dir {
            Dir::Down => (self.sets[reached].top, self.sets[walker].bottom),
            Dir::Up => (self.sets[walker].top, self.sets[reached].bottom),
        };
        let biggest = self.sets[walker].size.max(self.sets[reached].size);
        for (i, line) in self.lines.iter().enumerate() {
            if self.owner[i] == Some(walker) || self.owner[i] == Some(reached) {
                continue;
            }
            let b = line.bbox;
            if b[0] > hull[2] || b[2] < hull[0] {
                continue;
            }
            if b[3] > high + EPSILON || b[1] < low - EPSILON {
                continue;
            }
            if line.kind == "heading"
                || self.sizes[i] > TEXT_GROWTH * biggest
                || is_bold(&self.pieces[i])
            {
                return true;
            }
            if line.kind == "caption" || is_caption(&self.texts[i]) {
                return true;
            }
            let right: Vec<usize> = (0..self.pieces[i].len())
                .filter(|&j| self.pieces[i][j].bbox[2] > params.stub)
                .collect();
            // A figure row reads as a header by shape, so only a header that
            // is no figure row -- or a row of years, which is one -- blocks.
            if self.is_header_row(i, &right, params)
                && (!self.is_figure_row(i, &right, params)
                    || right.iter().all(|&j| is_year(&self.pieces[i][j].text)))
                && !right.is_empty()
            {
                return true;
            }
        }
        false
    }

    /// Whether the lower set starts with its own year head: its topmost line
    /// carries a year right of the stub and reads as no figure row
    /// (`table_split_stacked`, #574). The merge guard refuses into a set that
    /// absorbed a header, but a year head the judge chose the set with --
    /// `Year Ended December 31, 2009` opening the lower of two stacked
    /// statements -- is a member, not an absorbed line, so the merge passed
    /// it and the two came out as one.
    fn starts_with_year_head(&self, lower: usize, params: &Params) -> bool {
        let top = self.sets[lower]
            .members
            .iter()
            .max_by(|&&a, &&b| self.lines[a].bbox[3].total_cmp(&self.lines[b].bbox[3]));
        let Some(&line) = top else {
            return false;
        };
        let right: Vec<usize> = (0..self.pieces[line].len())
            .filter(|&j| self.pieces[line][j].bbox[2] > params.stub)
            .collect();
        !right.is_empty()
            && !self.is_figure_row(line, &right, params)
            && right.iter().any(|&j| is_year(&self.pieces[line][j].text))
    }

    /// Merge the walking set with the reached one where their columns match
    /// and nothing lies between: the reached set and the vetted lines between
    /// join the walker, which walks on. Held labels join with the merge: the
    /// reached set's figure rows stand after them.
    fn try_merge(
        &mut self,
        walker: usize,
        reached: usize,
        dir: Dir,
        params: &Params,
        held: &[usize],
        vetted: &[usize],
    ) -> bool {
        let (columns, _) = figure_columns(&self.figure_tokens_of(reached));
        let matched = columns
            .iter()
            .filter(|&c| params.columns.iter().any(|&a| (a - c).abs() <= TOL_PT))
            .count();
        if matched < MERGE_MIN_COLUMNS {
            return false;
        }
        if matched as f64 / columns.len() as f64 >= MERGE_MIN_SHARE {
            let lower_headed = match dir {
                Dir::Down => self.sets[reached].headed,
                Dir::Up => self.sets[walker].headed,
            };
            let lower = match dir {
                Dir::Down => reached,
                Dir::Up => walker,
            };
            if lower_headed
                || self.blocked_between(walker, reached, dir, params)
                || (self.split_stacked && self.starts_with_year_head(lower, params))
            {
                return false;
            }
            let between = if self.claims(dir) { held } else { vetted };
            for &i in between {
                self.claim(walker, i);
            }
            let members = std::mem::take(&mut self.sets[reached].members);
            for i in members {
                self.claim(walker, i);
            }
            self.sets[reached].consumed = true;
            return true;
        }
        false
    }

    fn finish(mut self) -> Vec<Vec<usize>> {
        let mut out = Vec::new();
        for set in &mut self.sets {
            if set.consumed {
                continue;
            }
            set.members.sort_unstable();
            out.push(std::mem::take(&mut set.members));
        }
        out
    }
}

/// The stitch's words: glyph fragments rejoined into the words the rules read.
///
/// The page's words arrive one per glyph run, down to a character, and every
/// rule here -- figure candidacy, year and caption shapes, header word counts
/// -- was written and tested against whole words. Fragments of one line join
/// where the shared word rule joins them, across neither a style change (a
/// bold run past a regular one is a new word) nor a substantial overlap
/// (overstruck runs are two words); anything else parts with the gap it
/// stands in. Line for line, so the regrouping still aligns with the judge.
fn join_line_words(pieces: &[Vec<&PageWord>]) -> Vec<Vec<PageWord>> {
    pieces
        .iter()
        .map(|line| {
            let mut sorted: Vec<&PageWord> = line.to_vec();
            sorted.sort_by(|a, b| a.bbox[0].total_cmp(&b.bbox[0]));
            let mut words: Vec<PageWord> = Vec::with_capacity(sorted.len());
            for word in sorted {
                let join = match words.last() {
                    None => false,
                    Some(last) => {
                        let em = last.size.max(word.size).max(f64::MIN_POSITIVE);
                        let gap = word.bbox[0] - last.bbox[2];
                        let drift = (word.bbox[1] - last.bbox[1]).abs();
                        last.bold == word.bold
                            && last.italic == word.italic
                            && last.font == word.font
                            && (last.size - word.size).abs() <= 0.1 * em
                            && gap > -0.1 * em
                            && crate::words_join::pieces_join(gap, drift, em)
                    }
                };
                if join {
                    let last = words.last_mut().expect("a join has a last word");
                    last.text.push_str(&word.text);
                    last.bbox[0] = last.bbox[0].min(word.bbox[0]);
                    last.bbox[1] = last.bbox[1].min(word.bbox[1]);
                    last.bbox[2] = last.bbox[2].max(word.bbox[2]);
                    last.bbox[3] = last.bbox[3].max(word.bbox[3]);
                    last.size = last.size.max(word.size);
                } else {
                    words.push((*word).clone());
                }
            }
            words
        })
        .collect()
}

/// Stitch the judge's chosen line sets across wrapped labels and header rows.
///
/// `lines` are the judge's lines of the page and `words` its words, regrouped
/// per block with [`lines_of`][super::lines::lines_of] so piece `i` is line
/// `i`; `chosen` holds line indices. With `absorb_header` the walk takes
/// header rows above each set; with `absorb_rows` it takes label-only lines
/// and figure rows below; with `merge` a set reached across such lines joins
/// the walker where their columns match; with `split_stacked` the merge
/// refuses into a set starting with its own year head. With none of them, or
/// where the regrouping does not align, the sets come back as chosen.
#[allow(clippy::too_many_arguments)]
pub(crate) fn stitch_sets(
    lines: &[Line],
    words: &[PageWord],
    n_blocks: usize,
    chosen: &[Vec<usize>],
    absorb_header: bool,
    absorb_rows: bool,
    merge: bool,
    split_stacked: bool,
) -> Vec<Vec<usize>> {
    if (!absorb_header && !absorb_rows && !merge) || chosen.is_empty() {
        return chosen.to_vec();
    }
    let mut pieces = Vec::with_capacity(lines.len());
    for block in 0..n_blocks {
        for piece in lines_of(block, words) {
            if !piece.is_empty() {
                pieces.push(piece);
            }
        }
    }
    if pieces.len() != lines.len() {
        return chosen.to_vec();
    }
    // The words the rules read: glyph fragments rejoined line for line. The
    // count of lines is unchanged, so the regrouping still aligns with the
    // judge; only the words inside each piece are whole now.
    let joined = join_line_words(&pieces);
    let pieces: Vec<Vec<&PageWord>> = joined.iter().map(|line| line.iter().collect()).collect();
    let mut walker = Walker::new(
        lines,
        pieces,
        chosen,
        absorb_header,
        absorb_rows,
        merge,
        split_stacked,
    );
    // Bottom-up: a lower set takes its own header before an upper set's walk
    // may reach it, so headed sets refuse the merge.
    let mut order: Vec<usize> = (0..chosen.len()).collect();
    order.sort_by(|&a, &b| walker.sets[a].top.total_cmp(&walker.sets[b].top));
    for k in order {
        if walker.sets[k].consumed {
            continue;
        }
        walker.walk(k, Dir::Down);
        walker.walk(k, Dir::Up);
    }
    walker.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: f64 = 10.0;
    const PITCH: f64 = 12.0;

    /// One word: text and x-span at its row's height.
    struct W<'a> {
        text: &'a str,
        x0: f64,
        x1: f64,
        bold: bool,
    }

    fn w(text: &str, x0: f64, x1: f64) -> W<'_> {
        W {
            text,
            x0,
            x1,
            bold: false,
        }
    }

    fn wb(text: &str, x0: f64, x1: f64) -> W<'_> {
        W {
            text,
            x0,
            x1,
            bold: true,
        }
    }

    /// One visual row: top, size, words left to right.
    struct Row<'a> {
        top: f64,
        size: f64,
        words: Vec<W<'a>>,
    }

    fn row(top: f64, words: Vec<W<'_>>) -> Row<'_> {
        Row {
            top,
            size: SIZE,
            words,
        }
    }

    fn row_at(top: f64, size: f64, words: Vec<W<'_>>) -> Row<'_> {
        Row { top, size, words }
    }

    /// One block: kind and rows. Table rows stand one block per row so pieces
    /// stay in row order; prose stands as one paragraph block.
    struct Blk<'a> {
        kind: &'static str,
        rows: Vec<Row<'a>>,
    }

    fn blk<'a>(kind: &'static str, rows: Vec<Row<'a>>) -> Blk<'a> {
        Blk { kind, rows }
    }

    struct Page {
        words: Vec<PageWord>,
        lines: Vec<Line>,
        n_blocks: usize,
    }

    /// The page as the judge reads it: words grouped into pieces exactly as
    /// `judge::lines_of_page` groups them, so indices align by construction.
    fn page_of(blocks: &[Blk]) -> Page {
        let mut words = Vec::new();
        for (b, block) in blocks.iter().enumerate() {
            for r in &block.rows {
                for word in &r.words {
                    words.push(PageWord {
                        text: word.text.to_string(),
                        bbox: [word.x0, r.top - r.size, word.x1, r.top],
                        block: b,
                        span: 0,
                        size: r.size,
                        bold: word.bold,
                        italic: false,
                        font: None,
                        color: None,
                    });
                }
            }
        }
        let mut lines = Vec::new();
        for (b, block) in blocks.iter().enumerate() {
            for piece in lines_of(b, &words) {
                if piece.is_empty() {
                    continue;
                }
                let mut bbox = [
                    f64::INFINITY,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::NEG_INFINITY,
                ];
                for w in &piece {
                    bbox[0] = bbox[0].min(w.bbox[0]);
                    bbox[1] = bbox[1].min(w.bbox[1]);
                    bbox[2] = bbox[2].max(w.bbox[2]);
                    bbox[3] = bbox[3].max(w.bbox[3]);
                }
                lines.push(Line {
                    bbox,
                    text: piece
                        .iter()
                        .map(|w| w.text.as_str())
                        .collect::<Vec<_>>()
                        .join(" "),
                    words: piece.len(),
                    digits: piece
                        .iter()
                        .filter(|w| w.text.chars().any(|c| c.is_ascii_digit()))
                        .count(),
                    kind: block.kind,
                    table: None,
                    grid_fill: f64::NAN,
                    grid_columns: f64::NAN,
                    block: b,
                    list: f64::NAN,
                });
            }
        }
        Page {
            words,
            lines,
            n_blocks: blocks.len(),
        }
    }

    /// A figure row with three tight figures (one figure piece): label at the
    /// stub, columns at 400, 445, 490, stub edge at 358.
    fn figrow<'a>(top: f64, label: &'a str, figs: [&'a str; 3]) -> Row<'a> {
        row(
            top,
            vec![
                w(label, 72.0, 300.0),
                w(figs[0], 360.0, 400.0),
                w(figs[1], 405.0, 445.0),
                w(figs[2], 450.0, 490.0),
            ],
        )
    }

    fn stitch(
        page: &Page,
        chosen: &[Vec<usize>],
        absorb_header: bool,
        absorb_rows: bool,
        merge: bool,
    ) -> Vec<Vec<usize>> {
        stitch_sets(
            &page.lines,
            &page.words,
            page.n_blocks,
            chosen,
            absorb_header,
            absorb_rows,
            merge,
            false,
        )
    }

    /// The line holding `needle`, which must be unique on the page.
    fn find(page: &Page, needle: &str) -> usize {
        let found: Vec<usize> = page
            .lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.text.contains(needle))
            .map(|(i, _)| i)
            .collect();
        assert_eq!(
            found.len(),
            1,
            "{needle:?} is not unique: {:?}",
            found
                .iter()
                .map(|&i| page.lines[i].text.as_str())
                .collect::<Vec<_>>()
        );
        found[0]
    }

    /// The pieces of the rows labelled `labels`: each label piece and whatever
    /// shares its top to its right in its block. Same block, because a prose
    /// column may share the top from the next block over.
    fn rows_of(page: &Page, labels: &[&str]) -> Vec<usize> {
        let mut set = Vec::new();
        for label in labels {
            let l = find(page, label);
            set.push(l);
            let (top, right, block) = (
                page.lines[l].bbox[3],
                page.lines[l].bbox[2],
                page.lines[l].block,
            );
            for (i, line) in page.lines.iter().enumerate() {
                if i != l
                    && line.block == block
                    && line.bbox[3].total_cmp(&top) == std::cmp::Ordering::Equal
                    && line.bbox[0] > right
                {
                    set.push(i);
                }
            }
        }
        set.sort_unstable();
        set
    }

    fn texts_of(page: &Page, set: &[usize]) -> Vec<String> {
        let mut texts: Vec<String> = set.iter().map(|&i| page.lines[i].text.clone()).collect();
        texts.sort();
        texts
    }

    /// With both switches off the sets come back as chosen. Every behavior
    /// test asserts this alongside an on-effect that differs, so each fails
    /// with the pass removed.
    fn assert_off_is_identity(page: &Page, chosen: &[Vec<usize>]) {
        assert_eq!(stitch(page, chosen, false, false, false), chosen);
    }

    /// Four figure rows and the two sets the judge cut them into: two rows a
    /// set, so either half columns on its own.
    fn split_statement() -> (Page, Vec<usize>, Vec<usize>) {
        let page = page_of(&[
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "table_cell",
                vec![figrow(676.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
            blk(
                "table_cell",
                vec![figrow(664.0, "delta", ["7,777", "8,888", "9,999"])],
            ),
        ]);
        let upper = rows_of(&page, &["alpha", "bravo"]);
        let lower = rows_of(&page, &["charlie", "delta"]);
        (page, upper, lower)
    }

    #[test]
    fn a_wrapped_row_label_keeps_the_statement_one_table() {
        let page = page_of(&[
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "paragraph",
                vec![row(
                    676.0,
                    vec![w("Add: stock-based compensation", 72.0, 300.0)],
                )],
            ),
            blk(
                "paragraph",
                vec![row(
                    664.0,
                    vec![w("expense included in reported", 72.0, 300.0)],
                )],
            ),
            blk(
                "table_cell",
                vec![row(
                    652.0,
                    vec![
                        w("net income, net of tax", 72.0, 300.0),
                        w("7,777", 360.0, 400.0),
                        w("8,888", 405.0, 445.0),
                        w("9,999", 450.0, 490.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![figrow(640.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
        ]);
        // The judge ended the region at the wrap's first line: the statement
        // arrives as two sets with the wrap between them.
        let upper = rows_of(&page, &["alpha", "bravo"]);
        let lower = rows_of(&page, &["net income", "charlie"]);
        let chosen = vec![upper, lower];
        assert_off_is_identity(&page, &chosen);
        let stitched = stitch(&page, &chosen, true, true, true);
        assert_eq!(stitched.len(), 1, "the wrap still splits the table");
        let texts = texts_of(&page, &stitched[0]);
        for needle in [
            "Add: stock-based compensation",
            "expense included in reported",
            "net income, net of tax",
            "7,777",
            "alpha",
            "bravo",
            "charlie",
        ] {
            assert!(
                texts.iter().any(|t| t.contains(needle)),
                "the wrap line {needle:?} is not in the table: {texts:?}"
            );
        }
    }

    #[test]
    fn a_section_label_row_does_not_end_the_table() {
        let page = page_of(&[
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "paragraph",
                vec![row(676.0, vec![w("Deferred income taxes:", 72.0, 250.0)])],
            ),
            blk(
                "table_cell",
                vec![figrow(664.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
            blk(
                "table_cell",
                vec![figrow(652.0, "delta", ["7,777", "8,888", "9,999"])],
            ),
        ]);
        let chosen = vec![rows_of(&page, &["alpha", "bravo"])];
        assert_off_is_identity(&page, &chosen);
        let stitched = stitch(&page, &chosen, true, true, true);
        assert_eq!(stitched.len(), 1);
        let texts = texts_of(&page, &stitched[0]);
        for needle in ["Deferred income taxes:", "charlie", "delta"] {
            assert!(
                texts.iter().any(|t| t.contains(needle)),
                "{needle:?} did not join the table: {texts:?}"
            );
        }
        // Without the rows switch nothing below joins: only that switch
        // takes lines downward.
        assert_eq!(
            stitch(&page, &chosen, true, false, true),
            chosen,
            "a row joined with its switch off"
        );
    }

    #[test]
    fn the_year_header_joins_its_table() {
        let page = page_of(&[
            blk(
                "table_cell",
                vec![row(
                    712.0,
                    vec![
                        w("Fiscal", 72.0, 110.0),
                        w("Year", 115.0, 150.0),
                        w("2003", 378.0, 400.0),
                        w("2002", 423.0, 445.0),
                        w("2001", 468.0, 490.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
        ]);
        let chosen = vec![rows_of(&page, &["alpha", "bravo"])];
        assert_off_is_identity(&page, &chosen);
        let stitched = stitch(&page, &chosen, true, true, true);
        assert_eq!(stitched.len(), 1);
        let texts = texts_of(&page, &stitched[0]);
        for needle in ["Fiscal Year", "2003", "2002", "2001"] {
            assert!(
                texts.iter().any(|t| t.contains(needle)),
                "the year header did not join: {texts:?}"
            );
        }
        // The table's first row is the header: nothing stands above it.
        let top = stitched[0]
            .iter()
            .map(|&i| page.lines[i].bbox[3])
            .fold(f64::NEG_INFINITY, f64::max);
        let first: Vec<&str> = stitched[0]
            .iter()
            .filter(|&&i| page.lines[i].bbox[3].total_cmp(&top) == std::cmp::Ordering::Equal)
            .map(|&i| page.lines[i].text.as_str())
            .collect();
        assert!(
            first
                .iter()
                .any(|t| t.contains("Fiscal Year") || t.contains("2003")),
            "the header is not the table's first row: {first:?}"
        );
        // Without the header switch the header stays out: only that switch
        // takes lines upward.
        assert_eq!(
            stitch(&page, &chosen, false, true, true),
            chosen,
            "the header joined with its switch off"
        );
    }

    #[test]
    fn a_two_row_column_header_joins_whole() {
        // Four figure columns at 350, 390, 450, 490: a spanning label over
        // all four, centred on them, then years, then a unit row.
        let page = page_of(&[
            blk(
                "table_cell",
                vec![row(
                    736.0,
                    vec![
                        w("Consolidated", 346.0, 410.0),
                        w("The", 418.0, 440.0),
                        w("Company", 448.0, 494.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![row(
                    724.0,
                    vec![
                        w("2003", 320.0, 350.0),
                        w("2002", 360.0, 390.0),
                        w("2003", 420.0, 450.0),
                        w("2002", 460.0, 490.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![row(
                    712.0,
                    vec![
                        w("$000", 320.0, 350.0),
                        w("$000", 360.0, 390.0),
                        w("$000", 420.0, 450.0),
                        w("$000", 460.0, 490.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![row(
                    700.0,
                    vec![
                        w("alpha", 72.0, 300.0),
                        w("9,898", 320.0, 350.0),
                        w("2,240", 360.0, 390.0),
                        w("2,598", 420.0, 450.0),
                        w("7,111", 460.0, 490.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![row(
                    688.0,
                    vec![
                        w("bravo", 72.0, 300.0),
                        w("1,111", 320.0, 350.0),
                        w("2,222", 360.0, 390.0),
                        w("3,333", 420.0, 450.0),
                        w("4,444", 460.0, 490.0),
                    ],
                )],
            ),
        ]);
        let chosen = vec![rows_of(&page, &["alpha", "bravo"])];
        assert_off_is_identity(&page, &chosen);
        let stitched = stitch(&page, &chosen, true, true, true);
        assert_eq!(stitched.len(), 1);
        let texts = texts_of(&page, &stitched[0]);
        for needle in ["Consolidated", "The Company", "2003", "2002", "$000"] {
            assert!(
                texts.iter().any(|t| t.contains(needle)),
                "{needle:?} did not join the table: {texts:?}"
            );
        }
        // Without the header switch none of the three rows joins.
        assert_eq!(
            stitch(&page, &chosen, false, true, true),
            chosen,
            "a header row joined with its switch off"
        );

        // The same label off centre is a title, not a header: it stays out
        // and caps the walk, while the rows below it still join.
        let tilted = page_of(&[
            blk(
                "table_cell",
                vec![row(
                    736.0,
                    vec![
                        w("Consolidated", 346.0, 400.0),
                        w("The", 408.0, 430.0),
                        w("Company", 438.0, 480.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![row(
                    724.0,
                    vec![
                        w("2003", 320.0, 350.0),
                        w("2002", 360.0, 390.0),
                        w("2003", 420.0, 450.0),
                        w("2002", 460.0, 490.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![row(
                    712.0,
                    vec![
                        w("$000", 320.0, 350.0),
                        w("$000", 360.0, 390.0),
                        w("$000", 420.0, 450.0),
                        w("$000", 460.0, 490.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![row(
                    700.0,
                    vec![
                        w("alpha", 72.0, 300.0),
                        w("9,898", 320.0, 350.0),
                        w("2,240", 360.0, 390.0),
                        w("2,598", 420.0, 450.0),
                        w("7,111", 460.0, 490.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![row(
                    688.0,
                    vec![
                        w("bravo", 72.0, 300.0),
                        w("1,111", 320.0, 350.0),
                        w("2,222", 360.0, 390.0),
                        w("3,333", 420.0, 450.0),
                        w("4,444", 460.0, 490.0),
                    ],
                )],
            ),
        ]);
        let chosen = vec![rows_of(&tilted, &["alpha", "bravo"])];
        let texts = texts_of(&tilted, &stitch(&tilted, &chosen, true, true, true)[0]);
        assert!(texts.iter().any(|t| t.contains("2003")), "{texts:?}");
        assert!(texts.iter().any(|t| t.contains("$000")), "{texts:?}");
        assert!(
            !texts.iter().any(|t| t.contains("Consolidated")),
            "an off-centre wide label joined as a header: {texts:?}"
        );
    }

    #[test]
    fn a_centred_spanning_label_longer_than_four_words_joins() {
        // Five words in one run would read as a sentence per column -- but
        // centred over four columns it is a spanning header, whatever its
        // length.
        let page = page_of(&[
            blk(
                "table_cell",
                vec![row(
                    712.0,
                    vec![
                        w("aa", 346.0, 370.0),
                        w("bb", 378.0, 402.0),
                        w("cc", 410.0, 434.0),
                        w("dd", 442.0, 466.0),
                        w("ee", 470.0, 494.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![row(
                    700.0,
                    vec![
                        w("alpha", 72.0, 300.0),
                        w("9,898", 320.0, 350.0),
                        w("2,240", 360.0, 390.0),
                        w("2,598", 420.0, 450.0),
                        w("7,111", 460.0, 490.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![row(
                    688.0,
                    vec![
                        w("bravo", 72.0, 300.0),
                        w("1,111", 320.0, 350.0),
                        w("2,222", 360.0, 390.0),
                        w("3,333", 420.0, 450.0),
                        w("4,444", 460.0, 490.0),
                    ],
                )],
            ),
        ]);
        let chosen = vec![rows_of(&page, &["alpha", "bravo"])];
        assert_off_is_identity(&page, &chosen);
        let texts = texts_of(&page, &stitch(&page, &chosen, true, true, true)[0]);
        assert!(
            texts.iter().any(|t| t.contains("aa")),
            "a five-word centred spanning label did not join: {texts:?}"
        );
    }

    #[test]
    fn a_centred_spanning_label_over_eight_columns_spans_eight() {
        // Eight figure columns at 200, 230, 260, 290, 320, 350, 390, 430. The
        // label stands alone on its line; its words cover the middle three
        // but sit centred on all eight, so the run spans eight.
        let page = page_of(&[
            blk(
                "table_cell",
                vec![row(
                    712.0,
                    vec![
                        w("Amino", 280.0, 305.0),
                        w("acid", 308.0, 328.0),
                        w("identity", 331.0, 345.0),
                        w("(%)", 346.0, 350.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![row(
                    700.0,
                    vec![
                        w("alpha", 72.0, 150.0),
                        w("1", 180.0, 200.0),
                        w("2", 210.0, 230.0),
                        w("3", 240.0, 260.0),
                        w("4", 270.0, 290.0),
                        w("5", 300.0, 320.0),
                        w("6", 330.0, 350.0),
                        w("7", 370.0, 390.0),
                        w("8", 410.0, 430.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![row(
                    688.0,
                    vec![
                        w("bravo", 72.0, 150.0),
                        w("1", 180.0, 200.0),
                        w("2", 210.0, 230.0),
                        w("3", 240.0, 260.0),
                        w("4", 270.0, 290.0),
                        w("5", 300.0, 320.0),
                        w("6", 330.0, 350.0),
                        w("7", 370.0, 390.0),
                        w("8", 410.0, 430.0),
                    ],
                )],
            ),
        ]);
        let chosen = vec![rows_of(&page, &["alpha", "bravo"])];
        assert_off_is_identity(&page, &chosen);
        let texts = texts_of(&page, &stitch(&page, &chosen, true, true, true)[0]);
        assert!(
            texts.iter().any(|t| t.contains("Amino")),
            "the spanning label did not join: {texts:?}"
        );
        let walker = walker_of(&page, &chosen, true, true, true);
        let params = walker.params_of(0).expect("columns");
        assert_eq!(
            params.columns.len(),
            8,
            "not eight columns: {:?}",
            params.columns
        );
        // The words cover three columns; the run they stand for spans eight.
        let label = find(&page, "Amino");
        let (x0, x1) = walker.pieces[label]
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), word| {
                (lo.min(word.bbox[0]), hi.max(word.bbox[2]))
            });
        let covered = params
            .columns
            .iter()
            .filter(|&&c| x0 - EPSILON <= c && c <= x1 + EPSILON)
            .count();
        assert_eq!(covered, 3, "the words cover {covered}, not three");
        let span = spanning_columns(f64::NEG_INFINITY, f64::INFINITY, &params.columns);
        assert_eq!(span.len(), 8, "the label spans {span:?}, not all eight");
    }

    #[test]
    fn a_spanning_label_between_header_groups_spans_only_between_them() {
        // Six figure columns; a per-column group left and right, a spanning
        // label in the middle. Its words cover two columns but the columns
        // between its neighbours are four, and four is what it spans.
        let page = page_of(&[
            blk(
                "table_cell",
                vec![row(
                    712.0,
                    vec![
                        w("Total", 196.0, 220.0),
                        w("Mid", 270.0, 295.0),
                        w("span", 305.0, 330.0),
                        w("End", 380.0, 404.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![row(
                    700.0,
                    vec![
                        w("alpha", 72.0, 150.0),
                        w("1", 180.0, 200.0),
                        w("2", 220.0, 240.0),
                        w("3", 260.0, 280.0),
                        w("4", 300.0, 320.0),
                        w("5", 340.0, 360.0),
                        w("6", 380.0, 400.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![row(
                    688.0,
                    vec![
                        w("bravo", 72.0, 150.0),
                        w("1", 180.0, 200.0),
                        w("2", 220.0, 240.0),
                        w("3", 260.0, 280.0),
                        w("4", 300.0, 320.0),
                        w("5", 340.0, 360.0),
                        w("6", 380.0, 400.0),
                    ],
                )],
            ),
        ]);
        let chosen = vec![rows_of(&page, &["alpha", "bravo"])];
        assert_off_is_identity(&page, &chosen);
        let texts = texts_of(&page, &stitch(&page, &chosen, true, true, true)[0]);
        assert!(
            texts.iter().any(|t| t.contains("Mid")),
            "the header line did not join: {texts:?}"
        );
        let walker = walker_of(&page, &chosen, true, true, true);
        let params = walker.params_of(0).expect("columns");
        assert_eq!(
            params.columns.len(),
            6,
            "not six columns: {:?}",
            params.columns
        );
        let span = spanning_columns(220.0, 380.0, &params.columns);
        assert_eq!(
            span,
            vec![240.0, 280.0, 320.0, 360.0],
            "the label spans {span:?}, not the columns between its neighbours"
        );
    }

    #[test]
    fn a_per_column_header_group_is_unchanged() {
        // Two runs over at most one column each: per-column groups, read by
        // word count rather than by centring, as before.
        let page = page_of(&[
            blk(
                "table_cell",
                vec![row(
                    712.0,
                    vec![w("One", 396.0, 405.0), w("Two", 450.0, 460.0)],
                )],
            ),
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
        ]);
        let chosen = vec![rows_of(&page, &["alpha", "bravo"])];
        assert_off_is_identity(&page, &chosen);
        let texts = texts_of(&page, &stitch(&page, &chosen, true, true, true)[0]);
        for needle in ["One", "Two"] {
            assert!(
                texts.iter().any(|t| t.contains(needle)),
                "{needle:?} did not join as a per-column header: {texts:?}"
            );
        }
    }

    #[test]
    fn a_stub_only_line_never_joins_upward() {
        // The note stands a line above the table, set like a section row --
        // close enough to reach, with nothing stub-only may hide behind. The
        // section row below proves the pass ran.
        let page = page_of(&[
            blk(
                "paragraph",
                vec![row(
                    712.0,
                    vec![
                        w("Note", 72.0, 110.0),
                        w("5:", 115.0, 135.0),
                        w("Interest", 140.0, 200.0),
                        w("Expense,", 205.0, 270.0),
                        w("Net", 275.0, 300.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "paragraph",
                vec![row(676.0, vec![w("Deferred income taxes:", 72.0, 250.0)])],
            ),
            blk(
                "table_cell",
                vec![figrow(664.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
        ]);
        let chosen = vec![rows_of(&page, &["alpha", "bravo"])];
        assert_off_is_identity(&page, &chosen);
        let texts = texts_of(&page, &stitch(&page, &chosen, true, true, true)[0]);
        assert!(
            texts.iter().any(|t| t.contains("Deferred income taxes:")),
            "{texts:?}"
        );
        assert!(texts.iter().any(|t| t.contains("charlie")), "{texts:?}");
        assert!(
            !texts.iter().any(|t| t.contains("Interest Expense")),
            "a stub-only note joined upward: {texts:?}"
        );
    }

    /// A per-column header row over the `figrow` columns (400, 445, 490): two
    /// two-word groups, one per column pair, inside the columns' span.
    fn daterow(top: f64) -> Row<'static> {
        row(
            top,
            vec![
                w("March", 398.0, 430.0),
                w("31,", 432.0, 444.0),
                w("December", 447.0, 480.0),
                w("31,", 482.0, 494.0),
            ],
        )
    }

    #[test]
    fn a_stub_only_section_label_held_upward_commits_with_its_header() {
        // The corp-q1 shape: date headers over a bold section label over
        // figure rows. The label is held, and the header joining above
        // commits it with the chain tight either side.
        let page = page_of(&[
            blk("table_cell", vec![daterow(724.0)]),
            blk(
                "paragraph",
                vec![row(712.0, vec![wb("Liabilities", 72.0, 180.0)])],
            ),
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
        ]);
        let chosen = vec![rows_of(&page, &["alpha", "bravo"])];
        assert_off_is_identity(&page, &chosen);
        let texts = texts_of(&page, &stitch(&page, &chosen, true, true, true)[0]);
        for needle in ["March 31,", "December 31,", "Liabilities", "alpha", "bravo"] {
            assert!(
                texts.iter().any(|t| t.contains(needle)),
                "{needle:?} did not join with the header: {texts:?}"
            );
        }
        // Without the header switch the label caps the walk as a bold title
        // would while vetting, and the header stays out.
        assert_eq!(
            stitch(&page, &chosen, false, true, true),
            chosen,
            "the header joined with its switch off"
        );
    }

    #[test]
    fn a_second_stub_only_line_upward_stops_the_walk() {
        // A title over the section label: the label is held, the title is the
        // second consecutive stub-only line and caps the walk, so the header
        // above it is never reached.
        let page = page_of(&[
            blk("table_cell", vec![daterow(736.0)]),
            blk(
                "paragraph",
                vec![row(724.0, vec![wb("Regional figures", 72.0, 250.0)])],
            ),
            blk(
                "paragraph",
                vec![row(712.0, vec![wb("Liabilities", 72.0, 180.0)])],
            ),
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
        ]);
        let chosen = vec![rows_of(&page, &["alpha", "bravo"])];
        assert_off_is_identity(&page, &chosen);
        assert_eq!(
            stitch(&page, &chosen, true, true, true),
            chosen,
            "a second stub-only line did not cap the walk"
        );
    }

    #[test]
    fn a_held_stub_at_the_commit_boundary_commits_and_past_it_drops() {
        // Pitch 12: the label a full pitch past the set's edge commits with
        // its header, a hair past it the header joins without it.
        for (top, joins) in [(722.0, true), (722.01, false)] {
            let page = page_of(&[
                blk("table_cell", vec![daterow(top + 12.0)]),
                blk(
                    "paragraph",
                    vec![row(top, vec![w("Loose label", 72.0, 220.0)])],
                ),
                blk(
                    "table_cell",
                    vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
                ),
                blk(
                    "table_cell",
                    vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
                ),
            ]);
            let chosen = vec![rows_of(&page, &["alpha", "bravo"])];
            let texts = texts_of(&page, &stitch(&page, &chosen, true, true, true)[0]);
            assert!(
                texts.iter().any(|t| t.contains("March 31,")),
                "the header did not join at label top {top}: {texts:?}"
            );
            let joined = texts.iter().any(|t| t.contains("Loose label"));
            assert!(
                joined == joins,
                "the held label joined={joined} at top {top}, want {joins}: {texts:?}"
            );
        }
    }

    #[test]
    fn a_figure_row_above_commits_the_held_section_label() {
        // A figure row joining upward commits the held label the same way a
        // header row does: the cut between two figure rows heals whole.
        let page = page_of(&[
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "paragraph",
                vec![row(676.0, vec![w("Deferred income taxes:", 72.0, 250.0)])],
            ),
            blk(
                "table_cell",
                vec![figrow(664.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
            blk(
                "table_cell",
                vec![figrow(652.0, "delta", ["7,777", "8,888", "9,999"])],
            ),
        ]);
        let chosen = vec![rows_of(&page, &["charlie", "delta"])];
        assert_off_is_identity(&page, &chosen);
        let texts = texts_of(&page, &stitch(&page, &chosen, true, true, true)[0]);
        for needle in [
            "alpha",
            "bravo",
            "Deferred income taxes:",
            "charlie",
            "delta",
        ] {
            assert!(
                texts.iter().any(|t| t.contains(needle)),
                "{needle:?} did not join upward: {texts:?}"
            );
        }
        // Without the header switch nothing above joins: only that switch
        // takes lines upward.
        assert_eq!(
            stitch(&page, &chosen, false, true, true),
            chosen,
            "a figure row joined upward with its switch off"
        );
    }

    #[test]
    fn in_millions_joins_with_its_header_line() {
        // The unit note stands at the stub on the year header's own line: it
        // joins with that row, though a stub-only line alone never would.
        let page = page_of(&[
            blk(
                "table_cell",
                vec![row(
                    712.0,
                    vec![
                        w("(in", 72.0, 100.0),
                        w("millions)", 105.0, 170.0),
                        w("2003", 378.0, 400.0),
                        w("2002", 423.0, 445.0),
                        w("2001", 468.0, 490.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
        ]);
        let chosen = vec![rows_of(&page, &["alpha", "bravo"])];
        assert_off_is_identity(&page, &chosen);
        let texts = texts_of(&page, &stitch(&page, &chosen, true, true, true)[0]);
        for needle in ["millions", "2003", "2002", "2001"] {
            assert!(
                texts.iter().any(|t| t.contains(needle)),
                "{needle:?} did not join with its header line: {texts:?}"
            );
        }
        // Without the header switch the whole line stays out.
        assert_eq!(
            stitch(&page, &chosen, false, true, true),
            chosen,
            "the header line joined with its switch off"
        );
    }

    #[test]
    fn a_note_paragraph_under_a_table_stays_prose() {
        let page = page_of(&[
            blk(
                "table_cell",
                vec![row(
                    712.0,
                    vec![
                        w("Fiscal", 72.0, 110.0),
                        w("Year", 115.0, 150.0),
                        w("2003", 378.0, 400.0),
                        w("2002", 423.0, 445.0),
                        w("2001", 468.0, 490.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "paragraph",
                vec![
                    row(
                        676.0,
                        vec![
                            w("The", 72.0, 95.0),
                            w("revaluation", 100.0, 170.0),
                            w("was", 175.0, 200.0),
                            w("performed", 205.0, 270.0),
                            w("at", 275.0, 290.0),
                            w("market", 295.0, 340.0),
                            w("value", 345.0, 385.0),
                        ],
                    ),
                    row(664.0, vec![w("during the year.", 72.0, 180.0)]),
                ],
            ),
        ]);
        let chosen = vec![rows_of(&page, &["alpha", "bravo"])];
        assert_off_is_identity(&page, &chosen);
        let stitched = stitch(&page, &chosen, true, true, true);
        assert_eq!(stitched.len(), 1);
        let texts = texts_of(&page, &stitched[0]);
        // The header above proves the pass ran; the note below stays out.
        assert!(texts.iter().any(|t| t.contains("Fiscal Year")), "{texts:?}");
        assert!(
            !texts
                .iter()
                .any(|t| t.contains("revaluation") || t.contains("during the year")),
            "the note joined the table: {texts:?}"
        );
    }

    #[test]
    fn a_caption_above_with_a_larger_gap_stays_prose() {
        let page = page_of(&[
            blk(
                "paragraph",
                vec![row(
                    740.0,
                    vec![
                        w("Table", 72.0, 110.0),
                        w("2:", 115.0, 135.0),
                        w("Net", 140.0, 165.0),
                        w("sales", 170.0, 205.0),
                        w("by", 210.0, 228.0),
                        w("region", 233.0, 280.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "paragraph",
                vec![row(676.0, vec![w("Deferred income taxes:", 72.0, 250.0)])],
            ),
            blk(
                "table_cell",
                vec![figrow(664.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
        ]);
        let chosen = vec![rows_of(&page, &["alpha", "bravo"])];
        assert_off_is_identity(&page, &chosen);
        let stitched = stitch(&page, &chosen, true, true, true);
        let texts = texts_of(&page, &stitched[0]);
        // The section row below proves the pass ran; the gapped caption stays.
        assert!(
            texts.iter().any(|t| t.contains("Deferred income taxes:")),
            "{texts:?}"
        );
        assert!(!texts.iter().any(|t| t.contains("Table 2:")), "{texts:?}");
    }

    #[test]
    fn two_stacked_tables_with_different_figure_columns_stay_two() {
        let page = page_of(&[
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "table_cell",
                vec![row(
                    676.0,
                    vec![
                        w("ratio", 72.0, 120.0),
                        w("1.2", 376.0, 420.0),
                        w("3.4", 446.0, 490.0),
                        w("5.6", 516.0, 560.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![row(
                    664.0,
                    vec![
                        w("margin", 72.0, 120.0),
                        w("7.8", 376.0, 420.0),
                        w("9.0", 446.0, 490.0),
                        w("1.2", 516.0, 560.0),
                    ],
                )],
            ),
            blk(
                "paragraph",
                vec![row(652.0, vec![w("Trailing remark", 72.0, 200.0)])],
            ),
            blk(
                "table_cell",
                vec![row(
                    640.0,
                    vec![
                        w("spread", 72.0, 120.0),
                        w("2.3", 376.0, 420.0),
                        w("4.5", 446.0, 490.0),
                        w("6.7", 516.0, 560.0),
                    ],
                )],
            ),
        ]);
        let upper = rows_of(&page, &["alpha", "bravo"]);
        let lower = rows_of(&page, &["ratio", "margin"]);
        let chosen = vec![upper, lower];
        assert_off_is_identity(&page, &chosen);
        let stitched = stitch(&page, &chosen, true, true, true);
        assert_eq!(stitched.len(), 2, "different columns merged");
        // The lower table still absorbs its own continuation below.
        let texts = texts_of(&page, &stitched[1]);
        assert!(texts.iter().any(|t| t.contains("spread")), "{texts:?}");
        assert!(!texts.iter().any(|t| t.contains("alpha")), "{texts:?}");
    }

    #[test]
    fn a_two_column_page_of_prose_beside_a_table_is_untouched() {
        let page = page_of(&[
            blk(
                "table_cell",
                vec![row(
                    712.0,
                    vec![
                        w("Fiscal", 72.0, 110.0),
                        w("Year", 115.0, 150.0),
                        w("2003", 378.0, 400.0),
                        w("2002", 423.0, 445.0),
                        w("2001", 468.0, 490.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "paragraph",
                vec![
                    row(
                        724.0,
                        vec![w("Earlier", 500.0, 550.0), w("prose", 555.0, 590.0)],
                    ),
                    row(
                        700.0,
                        vec![w("Beside", 500.0, 550.0), w("prose", 555.0, 590.0)],
                    ),
                    row(
                        688.0,
                        vec![w("More", 500.0, 540.0), w("beside", 545.0, 590.0)],
                    ),
                    row(
                        664.0,
                        vec![w("Later", 500.0, 545.0), w("prose", 550.0, 590.0)],
                    ),
                ],
            ),
        ]);
        let chosen = vec![rows_of(&page, &["alpha", "bravo"])];
        assert_off_is_identity(&page, &chosen);
        let stitched = stitch(&page, &chosen, true, true, true);
        assert_eq!(stitched.len(), 1);
        let texts = texts_of(&page, &stitched[0]);
        assert!(texts.iter().any(|t| t.contains("Fiscal Year")), "{texts:?}");
        assert!(
            !texts
                .iter()
                .any(|t| t.contains("prose") || t.contains("Beside") || t.contains("More")),
            "the prose column joined the table: {texts:?}"
        );
    }

    #[test]
    fn two_stacked_statements_with_their_own_titles_stay_apart() {
        let page = page_of(&[
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "heading",
                vec![row_at(
                    676.0,
                    14.0,
                    vec![
                        w("Consolidated", 72.0, 170.0),
                        w("Statements", 175.0, 260.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![row(
                    660.0,
                    vec![
                        w("Fiscal", 72.0, 110.0),
                        w("Year", 115.0, 150.0),
                        w("2003", 378.0, 400.0),
                        w("2002", 423.0, 445.0),
                        w("2001", 468.0, 490.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![figrow(648.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
            blk(
                "table_cell",
                vec![figrow(636.0, "delta", ["7,777", "8,888", "9,999"])],
            ),
        ]);
        // Same figure columns, but the lower statement starts with its own
        // title and year header: separate tables.
        let upper = rows_of(&page, &["alpha", "bravo"]);
        let lower = rows_of(&page, &["charlie", "delta"]);
        let chosen = vec![upper, lower];
        assert_off_is_identity(&page, &chosen);
        let stitched = stitch(&page, &chosen, true, true, true);
        assert_eq!(stitched.len(), 2, "the stacked statements merged");
        let below = texts_of(&page, &stitched[1]);
        assert!(below.iter().any(|t| t.contains("Fiscal Year")), "{below:?}");
        assert!(below.iter().any(|t| t.contains("2003")), "{below:?}");
        assert!(
            !below.iter().any(|t| t.contains("Consolidated")),
            "{below:?}"
        );
        let above = texts_of(&page, &stitched[0]);
        assert!(
            !above
                .iter()
                .any(|t| t.contains("Consolidated") || t.contains("Fiscal")),
            "{above:?}"
        );
    }

    #[test]
    fn a_title_between_two_tables_blocks_their_merge() {
        // Tight enough to reach across (gap 18 of 19.2), so only the title
        // keeps them apart. The year header above proves the pass ran.
        let page = page_of(&[
            blk(
                "table_cell",
                vec![row(
                    712.0,
                    vec![
                        w("Fiscal", 72.0, 110.0),
                        w("Year", 115.0, 150.0),
                        w("2003", 378.0, 400.0),
                        w("2002", 423.0, 445.0),
                        w("2001", 468.0, 490.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "heading",
                vec![row_at(
                    676.0,
                    14.0,
                    vec![w("Second", 72.0, 130.0), w("Statement", 135.0, 230.0)],
                )],
            ),
            blk(
                "table_cell",
                vec![figrow(660.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
            blk(
                "table_cell",
                vec![figrow(648.0, "delta", ["7,777", "8,888", "9,999"])],
            ),
        ]);
        let chosen = vec![
            rows_of(&page, &["alpha", "bravo"]),
            rows_of(&page, &["charlie", "delta"]),
        ];
        assert_off_is_identity(&page, &chosen);
        let stitched = stitch(&page, &chosen, true, true, true);
        assert_eq!(stitched.len(), 2, "the title did not block the merge");
        assert!(
            texts_of(&page, &stitched[0])
                .iter()
                .any(|t| t.contains("Fiscal Year")),
            "the pass did not run"
        );
        for set in &stitched {
            assert!(
                !texts_of(&page, set)
                    .iter()
                    .any(|t| t.contains("Second Statement")),
                "the title joined a table"
            );
        }
    }

    #[test]
    fn a_caption_between_two_tables_blocks_their_merge() {
        // The year header above proves the pass ran.
        let page = page_of(&[
            blk(
                "table_cell",
                vec![row(
                    712.0,
                    vec![
                        w("Fiscal", 72.0, 110.0),
                        w("Year", 115.0, 150.0),
                        w("2003", 378.0, 400.0),
                        w("2002", 423.0, 445.0),
                        w("2001", 468.0, 490.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "paragraph",
                vec![row(
                    676.0,
                    vec![w("Table", 400.0, 438.0), w("2", 443.0, 452.0)],
                )],
            ),
            blk(
                "table_cell",
                vec![figrow(664.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
            blk(
                "table_cell",
                vec![figrow(652.0, "delta", ["7,777", "8,888", "9,999"])],
            ),
        ]);
        let chosen = vec![
            rows_of(&page, &["alpha", "bravo"]),
            rows_of(&page, &["charlie", "delta"]),
        ];
        assert_off_is_identity(&page, &chosen);
        let stitched = stitch(&page, &chosen, true, true, true);
        assert_eq!(stitched.len(), 2, "the caption did not block the merge");
        assert!(
            texts_of(&page, &stitched[0])
                .iter()
                .any(|t| t.contains("Fiscal Year")),
            "the pass did not run"
        );
        for set in &stitched {
            assert!(
                !texts_of(&page, set).iter().any(|t| t.contains("Table 2")),
                "{set:?}"
            );
        }
    }

    #[test]
    fn a_plain_label_between_two_tables_merges_with_them() {
        let page = page_of(&[
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk("paragraph", vec![row(676.0, vec![w("Notes", 72.0, 120.0)])]),
            blk(
                "table_cell",
                vec![figrow(664.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
            blk(
                "table_cell",
                vec![figrow(652.0, "delta", ["7,777", "8,888", "9,999"])],
            ),
        ]);
        let chosen = vec![
            rows_of(&page, &["alpha", "bravo"]),
            rows_of(&page, &["charlie", "delta"]),
        ];
        assert_off_is_identity(&page, &chosen);
        let stitched = stitch(&page, &chosen, true, true, true);
        assert_eq!(stitched.len(), 1, "the halves did not merge");
        assert!(texts_of(&page, &stitched[0])
            .iter()
            .any(|t| t.contains("Notes")));
    }

    #[test]
    fn a_label_line_between_two_halves_merges_with_both_absorb_switches_off() {
        // The upper half is set tight (pitch 5, reach 8) and the label stands
        // 9 below its edge, past what its downward walk may cross; the lower
        // half is set loose (pitch 12) and reaches the label upward. Only the
        // upward merge walk crosses it, so with both absorb switches off the
        // halves become one only where a stub-only line still vets upward.
        let page = page_of(&[
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(695.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "paragraph",
                vec![row(676.0, vec![w("Capital lease", 72.0, 200.0)])],
            ),
            blk(
                "table_cell",
                vec![figrow(660.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
            blk(
                "table_cell",
                vec![figrow(648.0, "delta", ["7,777", "8,888", "9,999"])],
            ),
        ]);
        let chosen = vec![
            rows_of(&page, &["alpha", "bravo"]),
            rows_of(&page, &["charlie", "delta"]),
        ];
        assert_off_is_identity(&page, &chosen);
        let stitched = stitch(&page, &chosen, false, false, true);
        assert_eq!(stitched.len(), 1, "the halves stayed two with both off");
        let texts = texts_of(&page, &stitched[0]);
        for needle in ["Capital lease", "alpha", "bravo", "charlie", "delta"] {
            assert!(
                texts.iter().any(|t| t.contains(needle)),
                "{needle:?} is not in the merged table: {texts:?}"
            );
        }
    }

    #[test]
    fn a_merge_into_a_set_starting_with_a_year_head_refuses() {
        // The lower half opens with its own year head -- `Year Ended December
        // 31, 2009` with the year standing right of the stub, off the figure
        // columns -- as the judge chose it: a member, not an absorbed line,
        // so `headed` never sees it and the columns match. With
        // `split_stacked` the merge refuses; without, the halves become one.
        let page = page_of(&[
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "table_cell",
                vec![row(
                    676.0,
                    vec![
                        w("Year", 72.0, 110.0),
                        w("Ended", 115.0, 160.0),
                        w("December", 165.0, 230.0),
                        w("31,", 235.0, 260.0),
                        w("2009", 500.0, 540.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![figrow(664.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
            blk(
                "table_cell",
                vec![figrow(652.0, "delta", ["7,777", "8,888", "9,999"])],
            ),
        ]);
        let chosen = vec![
            rows_of(&page, &["alpha", "bravo"]),
            rows_of(&page, &["Year", "charlie", "delta"]),
        ];
        assert_off_is_identity(&page, &chosen);
        let merged = stitch(&page, &chosen, false, false, true);
        assert_eq!(merged.len(), 1, "the halves stayed two with the flag off");
        let kept = stitch_sets(
            &page.lines,
            &page.words,
            page.n_blocks,
            &chosen,
            false,
            false,
            true,
            true,
        );
        assert_eq!(kept.len(), 2, "the halves merged past the year head");
        assert!(
            texts_of(&page, &kept[1]).iter().any(|t| t.contains("2009")),
            "the year head is not in the lower half: {:?}",
            texts_of(&page, &kept[1]),
        );
    }

    #[test]
    fn trailing_label_lines_without_figures_after_them_are_dropped() {
        let page = page_of(&[
            blk(
                "table_cell",
                vec![row(
                    712.0,
                    vec![
                        w("Fiscal", 72.0, 110.0),
                        w("Year", 115.0, 150.0),
                        w("2003", 378.0, 400.0),
                        w("2002", 423.0, 445.0),
                        w("2001", 468.0, 490.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "paragraph",
                vec![row(676.0, vec![w("See accompanying notes", 72.0, 250.0)])],
            ),
            blk(
                "paragraph",
                vec![row(664.0, vec![w("to the statements", 72.0, 220.0)])],
            ),
        ]);
        let chosen = vec![rows_of(&page, &["alpha", "bravo"])];
        assert_off_is_identity(&page, &chosen);
        let stitched = stitch(&page, &chosen, true, true, true);
        let texts = texts_of(&page, &stitched[0]);
        assert!(texts.iter().any(|t| t.contains("Fiscal Year")), "{texts:?}");
        assert!(
            !texts
                .iter()
                .any(|t| t.contains("accompanying") || t.contains("to the statements")),
            "trailing labels joined alone: {texts:?}"
        );
    }

    #[test]
    fn a_dot_leader_row_joins_its_table() {
        let page = page_of(&[
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "zebra", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "table_cell",
                vec![row(
                    676.0,
                    vec![
                        w("Accumulated", 72.0, 150.0),
                        w("depreciation", 155.0, 240.0),
                        w(".", 245.0, 250.0),
                        w(".", 255.0, 260.0),
                        w(".", 265.0, 270.0),
                        w(".", 275.0, 280.0),
                        w("8,500", 360.0, 400.0),
                        w("8,501", 405.0, 445.0),
                        w("8,502", 450.0, 490.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![figrow(664.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
        ]);
        let chosen = vec![rows_of(&page, &["alpha", "zebra"])];
        assert_off_is_identity(&page, &chosen);
        let stitched = stitch(&page, &chosen, true, true, true);
        let texts = texts_of(&page, &stitched[0]);
        assert!(texts.iter().any(|t| t.contains("Accumulated")), "{texts:?}");
        assert!(texts.iter().any(|t| t.contains("8,500")), "{texts:?}");
        assert!(texts.iter().any(|t| t.contains("charlie")), "{texts:?}");
    }

    #[test]
    fn digit_labels_do_not_drag_the_stub() {
        // Labels holding digits, one token and two: the figures still own the
        // stub edge, so the wrap below joins as label-only lines rather than
        // reading as prose crossing it.
        let page = page_of(&[
            blk(
                "table_cell",
                vec![figrow(700.0, "Note 12", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "Note 13", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "paragraph",
                vec![row(676.0, vec![w("Add: stock-based", 72.0, 220.0)])],
            ),
            blk(
                "paragraph",
                vec![row(664.0, vec![w("compensation", 72.0, 180.0)])],
            ),
            blk(
                "table_cell",
                vec![figrow(652.0, "Note 14", ["4,444", "5,555", "6,666"])],
            ),
        ]);
        let chosen = vec![rows_of(&page, &["Note 12", "Note 13"])];
        assert_off_is_identity(&page, &chosen);
        let stitched = stitch(&page, &chosen, true, true, true);
        let texts = texts_of(&page, &stitched[0]);
        for needle in ["Add: stock-based", "compensation", "Note 14"] {
            assert!(
                texts.iter().any(|t| t.contains(needle)),
                "{needle:?} stayed out: {texts:?}"
            );
        }
        let words = page_of(&[
            blk(
                "table_cell",
                vec![row(
                    700.0,
                    vec![
                        w("Note", 72.0, 110.0),
                        w("12", 113.0, 137.0),
                        w("9,898", 360.0, 400.0),
                        w("2,240", 405.0, 445.0),
                        w("2,598", 450.0, 490.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "paragraph",
                vec![row(676.0, vec![w("Add: stock-based", 72.0, 220.0)])],
            ),
            blk(
                "table_cell",
                vec![figrow(664.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
        ]);
        let chosen = vec![rows_of(&words, &["Note", "bravo"])];
        let texts = texts_of(&words, &stitch(&words, &chosen, true, true, true)[0]);
        for needle in ["Add: stock-based", "charlie"] {
            assert!(
                texts.iter().any(|t| t.contains(needle)),
                "{needle:?} stayed out: {texts:?}"
            );
        }
    }

    #[test]
    fn aligned_non_figure_words_do_not_join_as_a_figure_row() {
        let page = page_of(&[
            blk(
                "table_cell",
                vec![row(
                    712.0,
                    vec![
                        w("Fiscal", 72.0, 110.0),
                        w("Year", 115.0, 150.0),
                        w("2003", 378.0, 400.0),
                        w("2002", 423.0, 445.0),
                        w("2001", 468.0, 490.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "paragraph",
                vec![row(
                    676.0,
                    vec![
                        w("apples", 360.0, 400.0),
                        w("pears", 405.0, 445.0),
                        w("plums", 450.0, 490.0),
                    ],
                )],
            ),
        ]);
        let chosen = vec![rows_of(&page, &["alpha", "bravo"])];
        assert_off_is_identity(&page, &chosen);
        let stitched = stitch(&page, &chosen, true, true, true);
        let texts = texts_of(&page, &stitched[0]);
        assert!(texts.iter().any(|t| t.contains("Fiscal Year")), "{texts:?}");
        assert!(!texts.iter().any(|t| t.contains("apples")), "{texts:?}");
    }

    #[test]
    fn a_split_row_is_vetted_left_to_right() {
        // Label and figures 15 pt apart: four pieces at one top. The label is
        // held before its figures join and commit it.
        let page = page_of(&[
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "table_cell",
                vec![row(
                    676.0,
                    vec![
                        w("wide", 72.0, 300.0),
                        w("4,444", 360.0, 400.0),
                        w("5,555", 415.0, 445.0),
                        w("6,666", 460.0, 490.0),
                    ],
                )],
            ),
        ]);
        assert_eq!(
            page.lines.len(),
            8,
            "the row did not split into four pieces"
        );
        let chosen = vec![rows_of(&page, &["alpha", "bravo"])];
        assert_off_is_identity(&page, &chosen);
        let stitched = stitch(&page, &chosen, true, true, true);
        let texts = texts_of(&page, &stitched[0]);
        assert!(
            texts.iter().any(|t| t.contains("wide")),
            "the label piece was dropped: {texts:?}"
        );
        for needle in ["4,444", "5,555", "6,666"] {
            assert!(
                texts.iter().any(|t| t.contains(needle)),
                "{needle:?} stayed out: {texts:?}"
            );
        }
    }

    #[test]
    fn a_line_at_the_gap_factor_joins_and_past_it_stops() {
        // Pitch 10, so the reach is exactly 16 pt and the tops are exact.
        assert!((GAP_FACTOR * 10.0).total_cmp(&16.0) == std::cmp::Ordering::Equal);
        let page = page_of(&[
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(690.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "table_cell",
                vec![figrow(664.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
            blk(
                "table_cell",
                vec![figrow(637.999, "delta", ["7,777", "8,888", "9,999"])],
            ),
        ]);
        let chosen = vec![rows_of(&page, &["alpha", "bravo"])];
        assert_off_is_identity(&page, &chosen);
        let stitched = stitch(&page, &chosen, true, true, true);
        let texts = texts_of(&page, &stitched[0]);
        // Gap 16.0 joins; 16.001 past the joined row ends the walk, so delta stays out.
        assert!(texts.iter().any(|t| t.contains("charlie")), "{texts:?}");
        assert!(!texts.iter().any(|t| t.contains("delta")), "{texts:?}");
    }

    #[test]
    fn a_line_at_the_size_boundary_joins_and_past_it_never_does() {
        let big = TEXT_GROWTH * SIZE;
        let page = page_of(&[
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "paragraph",
                vec![row_at(676.0, big + 0.001, vec![w("too big", 72.0, 200.0)])],
            ),
            blk(
                "paragraph",
                vec![row_at(663.0, big, vec![w("joinable", 72.0, 200.0)])],
            ),
            blk(
                "table_cell",
                vec![figrow(650.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
        ]);
        let chosen = vec![rows_of(&page, &["alpha", "bravo"])];
        assert_off_is_identity(&page, &chosen);
        let stitched = stitch(&page, &chosen, true, true, true);
        let texts = texts_of(&page, &stitched[0]);
        // The nearer line is oversize and stepped over; the walk goes on past
        // it to the line at exactly the boundary, which is held and committed
        // by the figure row below.
        assert!(!texts.iter().any(|t| t.contains("too big")), "{texts:?}");
        assert!(texts.iter().any(|t| t.contains("joinable")), "{texts:?}");
        assert!(texts.iter().any(|t| t.contains("charlie")), "{texts:?}");
    }

    #[test]
    fn a_figure_edge_at_the_tolerance_matches_and_past_it_does_not() {
        let page = page_of(&[
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "table_cell",
                vec![row(
                    676.0,
                    vec![
                        w("on edge", 72.0, 200.0),
                        w("4,444", 364.0, 404.0),
                        w("5,555", 409.0, 449.0),
                        w("6,666", 454.0, 494.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![row(
                    664.0,
                    vec![
                        w("past edge", 72.0, 200.0),
                        w("7,777", 364.001, 404.001),
                        w("8,888", 409.001, 449.001),
                        w("9,999", 454.001, 494.001),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![figrow(652.0, "charlie", ["1,010", "2,020", "3,030"])],
            ),
        ]);
        let chosen = vec![rows_of(&page, &["alpha", "bravo"])];
        assert_off_is_identity(&page, &chosen);
        let stitched = stitch(&page, &chosen, true, true, true);
        let texts = texts_of(&page, &stitched[0]);
        assert!(texts.iter().any(|t| t.contains("on edge")), "{texts:?}");
        assert!(texts.iter().any(|t| t.contains("4,444")), "{texts:?}");
        // Past the tolerance the figures stay out but the walk goes on: the
        // row below still joins, committing the skipped row's label with it.
        assert!(!texts.iter().any(|t| t.contains("7,777")), "{texts:?}");
        assert!(texts.iter().any(|t| t.contains("past edge")), "{texts:?}");
        assert!(texts.iter().any(|t| t.contains("charlie")), "{texts:?}");
    }

    #[test]
    fn a_label_at_the_indent_tolerance_joins_and_past_it_stops() {
        let page = page_of(&[
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "paragraph",
                vec![row(676.0, vec![w("flush label", 68.0, 200.0)])],
            ),
            blk(
                "table_cell",
                vec![figrow(664.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
        ]);
        let chosen = vec![rows_of(&page, &["alpha", "bravo"])];
        assert_off_is_identity(&page, &chosen);
        let stitched = stitch(&page, &chosen, true, true, true);
        assert!(
            texts_of(&page, &stitched[0])
                .iter()
                .any(|t| t.contains("flush label")),
            "a label at exactly H.x0 - 4 did not join"
        );

        let past = page_of(&[
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "paragraph",
                vec![row(676.0, vec![w("outdented", 67.999, 200.0)])],
            ),
            blk(
                "table_cell",
                vec![figrow(664.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
        ]);
        // The outdented label is nearest downward and ends the walk, so the
        // figure row below it is never reached.
        let chosen = vec![rows_of(&past, &["alpha", "bravo"])];
        let grown = stitch(&past, &chosen, true, true, true);
        assert_eq!(grown, chosen, "an outdented label did not end the walk");
    }

    #[test]
    fn a_four_word_header_group_joins_and_five_read_as_prose() {
        let headed = page_of(&[
            blk(
                "table_cell",
                vec![row(
                    712.0,
                    vec![
                        w("qq1", 396.0, 410.0),
                        w("qq2", 412.0, 426.0),
                        w("qq3", 428.0, 442.0),
                        w("qq4", 452.0, 466.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
        ]);
        let chosen = vec![rows_of(&headed, &["alpha", "bravo"])];
        assert_off_is_identity(&headed, &chosen);
        let stitched = stitch(&headed, &chosen, true, true, true);
        assert!(
            texts_of(&headed, &stitched[0])
                .iter()
                .any(|t| t.contains("qq1")),
            "a four-word header group did not join"
        );

        // Two wide columns, so five words fit between them in one group.
        let wordy = page_of(&[
            blk(
                "paragraph",
                vec![row(724.0, vec![w("never reached", 72.0, 200.0)])],
            ),
            blk(
                "table_cell",
                vec![row(
                    712.0,
                    vec![
                        w("aa", 405.0, 417.0),
                        w("bb", 419.0, 431.0),
                        w("cc", 433.0, 445.0),
                        w("dd", 447.0, 459.0),
                        w("ee", 461.0, 473.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![row(
                    700.0,
                    vec![
                        w("alpha", 72.0, 200.0),
                        w("9,898", 360.0, 400.0),
                        w("2,240", 500.0, 540.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![row(
                    688.0,
                    vec![
                        w("bravo", 72.0, 200.0),
                        w("1,111", 360.0, 400.0),
                        w("3,333", 500.0, 540.0),
                    ],
                )],
            ),
        ]);
        // Five words in one group read as a sentence crossing the stub: prose,
        // ending the walk before the label above.
        let chosen = vec![rows_of(&wordy, &["alpha", "bravo"])];
        let grown = stitch(&wordy, &chosen, true, true, true);
        assert_eq!(grown, chosen, "a five-word group did not end the walk");
    }

    #[test]
    fn one_shared_column_does_not_merge_and_two_do() {
        let one = page_of(&[
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "table_cell",
                vec![row(
                    676.0,
                    vec![w("single", 72.0, 200.0), w("4,444", 360.0, 400.0)],
                )],
            ),
            blk(
                "table_cell",
                vec![row(
                    664.0,
                    vec![w("column", 72.0, 200.0), w("5,555", 360.0, 400.0)],
                )],
            ),
            blk(
                "table_cell",
                vec![row(
                    652.0,
                    vec![w("tail", 72.0, 200.0), w("6,666", 360.0, 400.0)],
                )],
            ),
        ]);
        let chosen = vec![
            rows_of(&one, &["alpha", "bravo"]),
            rows_of(&one, &["single", "column"]),
        ];
        assert_off_is_identity(&one, &chosen);
        let stitched = stitch(&one, &chosen, true, true, true);
        assert_eq!(stitched.len(), 2, "one shared column merged");
        assert!(
            texts_of(&one, &stitched[1])
                .iter()
                .any(|t| t.contains("tail")),
            "the lower table did not absorb its own tail"
        );

        let two = page_of(&[
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "table_cell",
                vec![row(
                    676.0,
                    vec![
                        w("narrow", 72.0, 200.0),
                        w("4,444", 360.0, 400.0),
                        w("5,555", 405.0, 445.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![row(
                    664.0,
                    vec![
                        w("table", 72.0, 200.0),
                        w("6,666", 360.0, 400.0),
                        w("7,777", 405.0, 445.0),
                    ],
                )],
            ),
        ]);
        let chosen = vec![
            rows_of(&two, &["alpha", "bravo"]),
            rows_of(&two, &["narrow", "table"]),
        ];
        let stitched = stitch(&two, &chosen, true, true, true);
        assert_eq!(stitched.len(), 1, "two shared columns did not merge");
    }

    #[test]
    fn a_sixty_percent_column_share_merges_and_below_does_not() {
        /// Five wide columns at 300, 360, 420, 480, 540.
        fn wide<'a>(top: f64, label: &'a str, figs: [(&'a str, f64, f64); 5]) -> Row<'a> {
            let mut words = vec![w(label, 72.0, 200.0)];
            for (text, x0, x1) in figs {
                words.push(w(text, x0, x1));
            }
            row(top, words)
        }
        let fifths: [(&str, f64, f64); 5] = [
            ("1,111", 265.0, 300.0),
            ("2,222", 325.0, 360.0),
            ("3,333", 385.0, 420.0),
            ("4,444", 445.0, 480.0),
            ("5,555", 505.0, 540.0),
        ];
        let three: [(&str, f64, f64); 5] = [
            ("1,111", 265.0, 300.0),
            ("2,222", 325.0, 360.0),
            ("3,333", 385.0, 420.0),
            ("4,444", 465.0, 500.0),
            ("5,555", 525.0, 560.0),
        ];
        let two: [(&str, f64, f64); 5] = [
            ("1,111", 265.0, 300.0),
            ("2,222", 325.0, 360.0),
            ("3,333", 465.0, 500.0),
            ("4,444", 525.0, 560.0),
            ("5,555", 585.0, 620.0),
        ];
        let sixty = page_of(&[
            blk("table_cell", vec![wide(700.0, "alpha", fifths)]),
            blk("table_cell", vec![wide(688.0, "bravo", fifths)]),
            blk("table_cell", vec![wide(676.0, "charlie", three)]),
            blk("table_cell", vec![wide(664.0, "delta", three)]),
        ]);
        let chosen = vec![
            rows_of(&sixty, &["alpha", "bravo"]),
            rows_of(&sixty, &["charlie", "delta"]),
        ];
        assert_off_is_identity(&sixty, &chosen);
        assert_eq!(
            stitch(&sixty, &chosen, true, true, true).len(),
            1,
            "three of five shared columns did not merge"
        );

        let forty = page_of(&[
            blk("table_cell", vec![wide(700.0, "alpha", fifths)]),
            blk("table_cell", vec![wide(688.0, "bravo", fifths)]),
            blk("table_cell", vec![wide(676.0, "charlie", two)]),
            blk("table_cell", vec![wide(664.0, "delta", two)]),
            blk("table_cell", vec![wide(652.0, "tail", two)]),
        ]);
        let chosen = vec![
            rows_of(&forty, &["alpha", "bravo"]),
            rows_of(&forty, &["charlie", "delta"]),
        ];
        assert_off_is_identity(&forty, &chosen);
        let stitched = stitch(&forty, &chosen, true, true, true);
        assert_eq!(stitched.len(), 2, "two of five shared columns merged");
        assert!(texts_of(&forty, &stitched[1])
            .iter()
            .any(|t| t.contains("tail")));
    }

    #[test]
    fn a_column_needs_two_lines_of_support() {
        let lone = page_of(&[
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "paragraph",
                vec![row(688.0, vec![w("Deferred income taxes:", 72.0, 250.0)])],
            ),
            blk(
                "table_cell",
                vec![figrow(676.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
        ]);
        // One figure row: every cluster stands on one line, so there are no
        // columns and the set is left alone.
        let chosen = vec![rows_of(&lone, &["alpha"])];
        assert_eq!(stitch(&lone, &chosen, true, true, true), chosen);

        let pair = page_of(&[
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "extra", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "paragraph",
                vec![row(676.0, vec![w("Deferred income taxes:", 72.0, 250.0)])],
            ),
            blk(
                "table_cell",
                vec![figrow(664.0, "bravo", ["4,444", "5,555", "6,666"])],
            ),
        ]);
        let chosen = vec![rows_of(&pair, &["alpha", "extra"])];
        assert_off_is_identity(&pair, &chosen);
        let texts = texts_of(&pair, &stitch(&pair, &chosen, true, true, true)[0]);
        assert!(
            texts.iter().any(|t| t.contains("Deferred income taxes:")),
            "{texts:?}"
        );
        assert!(texts.iter().any(|t| t.contains("bravo")), "{texts:?}");
    }

    #[test]
    fn a_word_ending_at_the_stub_is_label_and_past_it_prose() {
        // The stub stands at 358; a label ending there is held downward and
        // committed by the figure row below.
        let at = page_of(&[
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "paragraph",
                vec![row(676.0, vec![w("flush label", 72.0, 358.0)])],
            ),
            blk(
                "table_cell",
                vec![figrow(664.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
        ]);
        let chosen = vec![rows_of(&at, &["alpha", "bravo"])];
        assert_off_is_identity(&at, &chosen);
        assert!(
            texts_of(&at, &stitch(&at, &chosen, true, true, true)[0])
                .iter()
                .any(|t| t.contains("flush label")),
            "a word ending at the stub did not read as label"
        );

        // A hair past it the word crosses the stub with non-figures: prose,
        // ending the walk before the row below. The year header above proves
        // the pass ran.
        let past = page_of(&[
            blk(
                "table_cell",
                vec![row(
                    712.0,
                    vec![
                        w("Fiscal", 72.0, 110.0),
                        w("Year", 115.0, 150.0),
                        w("2003", 378.0, 400.0),
                        w("2002", 423.0, 445.0),
                        w("2001", 468.0, 490.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "paragraph",
                vec![row(676.0, vec![w("crossing", 72.0, 358.001)])],
            ),
            blk(
                "table_cell",
                vec![figrow(664.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
        ]);
        let chosen = vec![rows_of(&past, &["alpha", "bravo"])];
        assert_off_is_identity(&past, &chosen);
        let texts = texts_of(&past, &stitch(&past, &chosen, true, true, true)[0]);
        assert!(texts.iter().any(|t| t.contains("Fiscal Year")), "{texts:?}");
        assert!(
            !texts
                .iter()
                .any(|t| t.contains("crossing") || t.contains("charlie")),
            "{texts:?}"
        );
    }

    #[test]
    fn a_digit_at_the_figure_gap_counts_and_below_does_not() {
        // `Note 12` with the digit exactly 0.6 em past `Note` counts as a
        // figure; aligned over two rows it columns and drags the stub left,
        // so the wrap below reads as prose crossing it and the set is
        // unchanged.
        let gap = FIGURE_GAP_EM * SIZE;
        let wide = page_of(&[
            blk(
                "table_cell",
                vec![row(
                    700.0,
                    vec![
                        w("Note", 72.0, 110.0),
                        w("12", 110.0 + gap, 140.0 + gap),
                        w("9,898", 360.0, 400.0),
                        w("2,240", 405.0, 445.0),
                        w("2,598", 450.0, 490.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![row(
                    688.0,
                    vec![
                        w("Note", 72.0, 110.0),
                        w("13", 110.0 + gap, 140.0 + gap),
                        w("1,111", 360.0, 400.0),
                        w("2,222", 405.0, 445.0),
                        w("3,333", 450.0, 490.0),
                    ],
                )],
            ),
            blk(
                "paragraph",
                vec![row(676.0, vec![w("Add: stock-based", 72.0, 220.0)])],
            ),
            blk(
                "table_cell",
                vec![figrow(664.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
        ]);
        let chosen = vec![rows_of(&wide, &["Note 12", "Note 13"])];
        assert_eq!(
            stitch(&wide, &chosen, true, true, true),
            chosen,
            "a digit at the figure gap did not count as a figure"
        );

        // A hair closer it stays label text and the wrap joins.
        let tight = page_of(&[
            blk(
                "table_cell",
                vec![row(
                    700.0,
                    vec![
                        w("Note", 72.0, 110.0),
                        w("12", 110.0 + gap - 0.001, 140.0 + gap - 0.001),
                        w("9,898", 360.0, 400.0),
                        w("2,240", 405.0, 445.0),
                        w("2,598", 450.0, 490.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![row(
                    688.0,
                    vec![
                        w("Note", 72.0, 110.0),
                        w("13", 110.0 + gap - 0.001, 140.0 + gap - 0.001),
                        w("1,111", 360.0, 400.0),
                        w("2,222", 405.0, 445.0),
                        w("3,333", 450.0, 490.0),
                    ],
                )],
            ),
            blk(
                "paragraph",
                vec![row(676.0, vec![w("Add: stock-based", 72.0, 220.0)])],
            ),
            blk(
                "table_cell",
                vec![figrow(664.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
        ]);
        let chosen = vec![rows_of(&tight, &["Note 12", "Note 13"])];
        assert_off_is_identity(&tight, &chosen);
        let texts = texts_of(&tight, &stitch(&tight, &chosen, true, true, true)[0]);
        assert!(
            texts.iter().any(|t| t.contains("Add: stock-based")),
            "{texts:?}"
        );
        assert!(texts.iter().any(|t| t.contains("charlie")), "{texts:?}");
    }

    #[test]
    fn year_rows_skip_down_and_join_up_but_amounts_join_down() {
        // Aligned years step aside downward: the figures stay out while the
        // row below still joins.
        let down = page_of(&[
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "table_cell",
                vec![row(
                    676.0,
                    vec![
                        w("Fiscal", 72.0, 200.0),
                        w("1800", 360.0, 400.0),
                        w("2199", 405.0, 445.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![figrow(664.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
        ]);
        let chosen = vec![rows_of(&down, &["alpha", "bravo"])];
        assert_off_is_identity(&down, &chosen);
        let texts = texts_of(&down, &stitch(&down, &chosen, true, true, true)[0]);
        assert!(!texts.iter().any(|t| t.contains("1800")), "{texts:?}");
        assert!(texts.iter().any(|t| t.contains("charlie")), "{texts:?}");

        // Upward the same row heads the table.
        let up = page_of(&[
            blk(
                "table_cell",
                vec![row(
                    712.0,
                    vec![
                        w("Fiscal", 72.0, 200.0),
                        w("2003", 360.0, 400.0),
                        w("2002", 405.0, 445.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
        ]);
        let chosen = vec![rows_of(&up, &["alpha", "bravo"])];
        assert!(
            texts_of(&up, &stitch(&up, &chosen, true, true, true)[0])
                .iter()
                .any(|t| t.contains("2003")),
            "a year row did not join upward"
        );

        // Outside the year range the same shape is a figure row and joins down.
        let amounts = page_of(&[
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "table_cell",
                vec![row(
                    676.0,
                    vec![
                        w("shares", 72.0, 200.0),
                        w("1799", 360.0, 400.0),
                        w("2200", 405.0, 445.0),
                    ],
                )],
            ),
        ]);
        let chosen = vec![rows_of(&amounts, &["alpha", "bravo"])];
        let texts = texts_of(&amounts, &stitch(&amounts, &chosen, true, true, true)[0]);
        assert!(texts.iter().any(|t| t.contains("1799")), "{texts:?}");
    }

    #[test]
    fn a_bold_title_caps_the_table_but_a_bold_section_row_joins() {
        let page = page_of(&[
            blk(
                "paragraph",
                vec![row(724.0, vec![w("never reached", 72.0, 200.0)])],
            ),
            blk(
                "paragraph",
                vec![row(712.0, vec![wb("Regional figures", 72.0, 250.0)])],
            ),
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "paragraph",
                vec![row(676.0, vec![wb("Deferred income taxes:", 72.0, 250.0)])],
            ),
            blk(
                "table_cell",
                vec![figrow(664.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
        ]);
        let chosen = vec![rows_of(&page, &["alpha", "bravo"])];
        assert_off_is_identity(&page, &chosen);
        let stitched = stitch(&page, &chosen, true, true, true);
        let texts = texts_of(&page, &stitched[0]);
        // The bold title ends the upward walk before the label above it; the
        // bold section row between figure rows joins downward like any label.
        assert!(
            !texts.iter().any(|t| t.contains("Regional figures")),
            "{texts:?}"
        );
        assert!(
            !texts.iter().any(|t| t.contains("never reached")),
            "{texts:?}"
        );
        assert!(
            texts.iter().any(|t| t.contains("Deferred income taxes:")),
            "{texts:?}"
        );
        assert!(texts.iter().any(|t| t.contains("charlie")), "{texts:?}");
    }

    #[test]
    fn a_same_size_bold_title_between_tables_blocks_their_merge() {
        // The year header above proves the pass ran.
        let page = page_of(&[
            blk(
                "table_cell",
                vec![row(
                    712.0,
                    vec![
                        w("Fiscal", 72.0, 110.0),
                        w("Year", 115.0, 150.0),
                        w("2003", 378.0, 400.0),
                        w("2002", 423.0, 445.0),
                        w("2001", 468.0, 490.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "paragraph",
                vec![row(676.0, vec![wb("Second Statement", 72.0, 230.0)])],
            ),
            blk(
                "table_cell",
                vec![figrow(664.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
            blk(
                "table_cell",
                vec![figrow(652.0, "delta", ["7,777", "8,888", "9,999"])],
            ),
        ]);
        let chosen = vec![
            rows_of(&page, &["alpha", "bravo"]),
            rows_of(&page, &["charlie", "delta"]),
        ];
        assert_off_is_identity(&page, &chosen);
        let stitched = stitch(&page, &chosen, true, true, true);
        assert_eq!(stitched.len(), 2, "the bold title did not block the merge");
        assert!(
            texts_of(&page, &stitched[0])
                .iter()
                .any(|t| t.contains("Fiscal Year")),
            "the pass did not run"
        );
        for set in &stitched {
            assert!(
                !texts_of(&page, set)
                    .iter()
                    .any(|t| t.contains("Second Statement")),
                "{set:?}"
            );
        }
    }

    #[test]
    fn a_held_label_at_the_commit_boundary_commits_and_past_it_drops() {
        let at = page_of(&[
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "paragraph",
                vec![row(676.0, vec![w("Add: stock-based", 72.0, 220.0)])],
            ),
            blk(
                "table_cell",
                vec![figrow(
                    666.0 - PITCH,
                    "charlie",
                    ["4,444", "5,555", "6,666"],
                )],
            ),
        ]);
        let chosen = vec![rows_of(&at, &["alpha", "bravo"])];
        assert_off_is_identity(&at, &chosen);
        let texts = texts_of(&at, &stitch(&at, &chosen, true, true, true)[0]);
        assert!(
            texts.iter().any(|t| t.contains("Add: stock-based")),
            "{texts:?}"
        );
        assert!(texts.iter().any(|t| t.contains("charlie")), "{texts:?}");

        // A hair past one pitch the held label stays pending to the walk's
        // end while the figures still join without it.
        let past = page_of(&[
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "paragraph",
                vec![row(676.0, vec![w("Loose caption", 72.0, 220.0)])],
            ),
            blk(
                "table_cell",
                vec![figrow(
                    666.0 - PITCH - 0.001,
                    "charlie",
                    ["4,444", "5,555", "6,666"],
                )],
            ),
        ]);
        let chosen = vec![rows_of(&past, &["alpha", "bravo"])];
        let texts = texts_of(&past, &stitch(&past, &chosen, true, true, true)[0]);
        assert!(
            !texts.iter().any(|t| t.contains("Loose caption")),
            "{texts:?}"
        );
        assert!(texts.iter().any(|t| t.contains("charlie")), "{texts:?}");
    }

    #[test]
    fn caption_shapes_are_recognized() {
        for caption in [
            "Table 2: Net sales",
            "TABLE 1",
            "Figure 12",
            "Fig. 3",
            "Fig 4",
            "Tables 2-4",
            "  table 9 (continued)",
        ] {
            assert!(is_caption(caption), "{caption:?} did not read as a caption");
        }
        for prose in [
            "Table of contents",
            "Tabular data",
            "Figure skating",
            "Fig leaf",
            "Tables",
            "2 Tables",
            "",
        ] {
            assert!(!is_caption(prose), "{prose:?} read as a caption");
        }
    }

    #[test]
    fn without_either_switch_the_sets_come_back_unchanged() {
        let (page, upper, lower) = split_statement();
        // Adjacent halves with matching columns: anything on would merge them.
        let chosen = vec![upper, lower];
        assert_eq!(stitch(&page, &chosen, false, false, false), chosen);
        assert_eq!(stitch(&page, &chosen, true, true, true).len(), 1);
    }

    /// Two halves with a year header above the upper one, a wrap between
    /// them, and a figure row below the lower one: each absorb switch grows
    /// its own direction without joining the halves.
    fn split_with_header_and_tail() -> (Page, Vec<usize>, Vec<usize>) {
        let page = page_of(&[
            blk(
                "table_cell",
                vec![row(
                    712.0,
                    vec![
                        w("Fiscal", 72.0, 110.0),
                        w("Year", 115.0, 150.0),
                        w("2003", 378.0, 400.0),
                        w("2002", 423.0, 445.0),
                        w("2001", 468.0, 490.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "paragraph",
                vec![row(676.0, vec![w("Add: stock-based", 72.0, 220.0)])],
            ),
            blk(
                "paragraph",
                vec![row(664.0, vec![w("compensation", 72.0, 180.0)])],
            ),
            blk(
                "table_cell",
                vec![figrow(652.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
            blk(
                "table_cell",
                vec![figrow(640.0, "delta", ["7,777", "8,888", "9,999"])],
            ),
            blk(
                "table_cell",
                vec![figrow(628.0, "tail", ["1,010", "2,020", "3,030"])],
            ),
        ]);
        let upper = rows_of(&page, &["alpha", "bravo"]);
        let lower = rows_of(&page, &["charlie", "delta"]);
        (page, upper, lower)
    }

    #[test]
    fn absorbing_without_merging_grows_but_does_not_join() {
        let (page, upper, lower) = split_with_header_and_tail();
        let chosen = vec![upper, lower];
        assert_off_is_identity(&page, &chosen);
        let stitched = stitch(&page, &chosen, true, true, false);
        assert_eq!(stitched.len(), 2, "absorb-only merged the sets");
        // The upper half takes its header upward, the lower half its tail
        // downward; the wrap between them is held but never committed, so it
        // stays out without a merge.
        let above = texts_of(&page, &stitched[0]);
        assert!(above.iter().any(|t| t.contains("Fiscal Year")), "{above:?}");
        assert!(above.iter().any(|t| t.contains("2003")), "{above:?}");
        assert!(
            !above.iter().any(|t| t.contains("compensation")),
            "{above:?}"
        );
        let below = texts_of(&page, &stitched[1]);
        assert!(below.iter().any(|t| t.contains("tail")), "{below:?}");
        assert!(
            !below
                .iter()
                .any(|t| t.contains("Add: stock-based") || t.contains("compensation")),
            "{below:?}"
        );
    }

    #[test]
    fn absorbing_headers_without_rows_grows_upward_only() {
        let (page, upper, lower) = split_with_header_and_tail();
        let chosen = vec![upper, lower];
        let stitched = stitch(&page, &chosen, true, false, false);
        assert_eq!(stitched.len(), 2, "header-only merged the sets");
        let above = texts_of(&page, &stitched[0]);
        assert!(above.iter().any(|t| t.contains("Fiscal Year")), "{above:?}");
        assert!(above.iter().any(|t| t.contains("2003")), "{above:?}");
        // Nothing downward: the tail figure row and the wrap stay out.
        for set in &stitched {
            let texts = texts_of(&page, set);
            assert!(
                !texts.iter().any(|t| t.contains("tail")
                    || t.contains("Add: stock-based")
                    || t.contains("compensation")),
                "header-only absorbed downward: {texts:?}"
            );
        }
    }

    #[test]
    fn absorbing_rows_without_headers_grows_downward_only() {
        let (page, upper, lower) = split_with_header_and_tail();
        let chosen = vec![upper, lower];
        let stitched = stitch(&page, &chosen, false, true, false);
        assert_eq!(stitched.len(), 2, "rows-only merged the sets");
        let below = texts_of(&page, &stitched[1]);
        assert!(below.iter().any(|t| t.contains("tail")), "{below:?}");
        // Nothing upward: the year header stays out, and the wrap between the
        // halves is held below the upper one but never committed.
        for set in &stitched {
            let texts = texts_of(&page, set);
            assert!(
                !texts.iter().any(|t| t.contains("Fiscal Year")
                    || t.contains("2003")
                    || t.contains("Add: stock-based")
                    || t.contains("compensation")),
                "rows-only absorbed upward: {texts:?}"
            );
        }
    }

    #[test]
    fn merging_without_absorbing_claims_only_between() {
        let page = page_of(&[
            blk(
                "table_cell",
                vec![row(
                    712.0,
                    vec![
                        w("Fiscal", 72.0, 110.0),
                        w("Year", 115.0, 150.0),
                        w("2003", 378.0, 400.0),
                        w("2002", 423.0, 445.0),
                        w("2001", 468.0, 490.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk("paragraph", vec![row(676.0, vec![w("Notes", 72.0, 120.0)])]),
            blk(
                "table_cell",
                vec![figrow(664.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
            blk(
                "table_cell",
                vec![figrow(652.0, "delta", ["7,777", "8,888", "9,999"])],
            ),
            blk(
                "table_cell",
                vec![figrow(640.0, "tail", ["1,010", "2,020", "3,030"])],
            ),
        ]);
        let chosen = vec![
            rows_of(&page, &["alpha", "bravo"]),
            rows_of(&page, &["charlie", "delta"]),
        ];
        assert_off_is_identity(&page, &chosen);
        let stitched = stitch(&page, &chosen, false, false, true);
        assert_eq!(stitched.len(), 1, "merge-only did not merge");
        let texts = texts_of(&page, &stitched[0]);
        assert!(texts.iter().any(|t| t.contains("Notes")), "{texts:?}");
        assert!(
            !texts
                .iter()
                .any(|t| t.contains("Fiscal Year") || t.contains("tail")),
            "merge-only absorbed past the merge: {texts:?}"
        );
    }

    /// A walker over the page as [`stitch_sets`] builds it, for calling the
    /// merge guard directly.
    fn walker_of<'x>(
        page: &'x Page,
        chosen: &[Vec<usize>],
        absorb_header: bool,
        absorb_rows: bool,
        merge: bool,
    ) -> Walker<'x> {
        let mut pieces = Vec::new();
        for block in 0..page.n_blocks {
            for piece in lines_of(block, &page.words) {
                if !piece.is_empty() {
                    pieces.push(piece);
                }
            }
        }
        assert_eq!(pieces.len(), page.lines.len());
        Walker::new(
            &page.lines,
            pieces,
            chosen,
            absorb_header,
            absorb_rows,
            merge,
            false,
        )
    }

    #[test]
    fn a_title_caption_or_header_between_blocks_a_merge() {
        // A between-line of each kind, each reached directly so the guard --
        // not the walk -- decides.
        let title = page_of(&[
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "heading",
                vec![row_at(
                    676.0,
                    14.0,
                    vec![w("Second Statement", 72.0, 230.0)],
                )],
            ),
            blk(
                "table_cell",
                vec![figrow(660.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
            blk(
                "table_cell",
                vec![figrow(648.0, "delta", ["7,777", "8,888", "9,999"])],
            ),
        ]);
        let chosen = vec![
            rows_of(&title, &["alpha", "bravo"]),
            rows_of(&title, &["charlie", "delta"]),
        ];
        let mut walker = walker_of(&title, &chosen, true, true, true);
        let params = walker.params_of(0).expect("columns");
        assert!(
            !walker.try_merge(0, 1, Dir::Down, &params, &[], &[]),
            "a title between merged"
        );

        let caption = page_of(&[
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "paragraph",
                vec![row(
                    676.0,
                    vec![w("Table", 400.0, 438.0), w("2", 443.0, 452.0)],
                )],
            ),
            blk(
                "table_cell",
                vec![figrow(664.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
            blk(
                "table_cell",
                vec![figrow(652.0, "delta", ["7,777", "8,888", "9,999"])],
            ),
        ]);
        let chosen = vec![
            rows_of(&caption, &["alpha", "bravo"]),
            rows_of(&caption, &["charlie", "delta"]),
        ];
        let mut walker = walker_of(&caption, &chosen, true, true, true);
        let params = walker.params_of(0).expect("columns");
        assert!(
            !walker.try_merge(0, 1, Dir::Down, &params, &[], &[]),
            "a caption between merged"
        );

        // An aligned year header: a figure row by shape, a header row by year.
        let header = page_of(&[
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "table_cell",
                vec![row(
                    676.0,
                    vec![
                        w("Fiscal", 72.0, 110.0),
                        w("Year", 115.0, 150.0),
                        w("2003", 378.0, 400.0),
                        w("2002", 423.0, 445.0),
                        w("2001", 468.0, 490.0),
                    ],
                )],
            ),
            blk(
                "table_cell",
                vec![figrow(664.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
            blk(
                "table_cell",
                vec![figrow(652.0, "delta", ["7,777", "8,888", "9,999"])],
            ),
        ]);
        let chosen = vec![
            rows_of(&header, &["alpha", "bravo"]),
            rows_of(&header, &["charlie", "delta"]),
        ];
        let mut walker = walker_of(&header, &chosen, true, true, true);
        let params = walker.params_of(0).expect("columns");
        assert!(
            !walker.try_merge(0, 1, Dir::Down, &params, &[], &[]),
            "a header between merged"
        );

        // A same-size bold title between: no size or caption tells it, the
        // face does.
        let bold = page_of(&[
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk(
                "paragraph",
                vec![row(676.0, vec![wb("Second Statement", 72.0, 230.0)])],
            ),
            blk(
                "table_cell",
                vec![figrow(664.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
            blk(
                "table_cell",
                vec![figrow(652.0, "delta", ["7,777", "8,888", "9,999"])],
            ),
        ]);
        let chosen = vec![
            rows_of(&bold, &["alpha", "bravo"]),
            rows_of(&bold, &["charlie", "delta"]),
        ];
        let mut walker = walker_of(&bold, &chosen, true, true, true);
        let params = walker.params_of(0).expect("columns");
        assert!(
            !walker.try_merge(0, 1, Dir::Down, &params, &[], &[]),
            "a bold title between merged"
        );

        // And a plain label between merges.
        let plain = page_of(&[
            blk(
                "table_cell",
                vec![figrow(700.0, "alpha", ["9,898", "2,240", "2,598"])],
            ),
            blk(
                "table_cell",
                vec![figrow(688.0, "bravo", ["1,111", "2,222", "3,333"])],
            ),
            blk("paragraph", vec![row(676.0, vec![w("Notes", 72.0, 120.0)])]),
            blk(
                "table_cell",
                vec![figrow(664.0, "charlie", ["4,444", "5,555", "6,666"])],
            ),
            blk(
                "table_cell",
                vec![figrow(652.0, "delta", ["7,777", "8,888", "9,999"])],
            ),
        ]);
        let chosen = vec![
            rows_of(&plain, &["alpha", "bravo"]),
            rows_of(&plain, &["charlie", "delta"]),
        ];
        let mut walker = walker_of(&plain, &chosen, true, true, true);
        let params = walker.params_of(0).expect("columns");
        assert!(
            walker.try_merge(0, 1, Dir::Down, &params, &[], &[]),
            "clean halves did not merge"
        );
        assert_eq!(walker.finish().len(), 1);
    }
}
