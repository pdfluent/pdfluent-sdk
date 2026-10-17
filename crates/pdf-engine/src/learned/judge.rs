// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A judge over table-region proposals (#574).
//!
//! WHAT IT CHOOSES BETWEEN. Several proposers see a page's tables differently:
//! the builder's tables, T2's four region detectors (`aligned`, `booktabs`,
//! `pair`, `leader`, refused candidates included), #632's alignment proposer,
//! every one of those in pieces (`splits::pieces`), and the unions of the ones
//! that overlap. With the pieces, an oracle that picks per page the proposals
//! matching an annotated table reads 0.849 on DocLayNet validation. What the
//! judge reads besides the proposal's own shape: whether its detector kept it,
//! two signs of a chart's labels (year ticks, lines with no word), and what the
//! list models say of its lines (T5, `list_signal`). End to end on DocLayNet's
//! tuning half it takes tables 0.574 -> 0.591 over the first judge, with every
//! other axis up too (#574).
//!
//! THE UNIT IS A LINE -- `lines::lines_of`, block by block -- so two proposals
//! are compared exactly. `table_proposals.py` and `train_table_judge.py` are
//! the other half of this file, and the parity fixture holds the two together.
//!
//! IT PROPOSES NOTHING AND NEVER TOUCHES A GRID. What it returns is which sets
//! of lines are tables; the separation decoder builds their cells.

use super::features::{kind_name, PageWord};
use super::models;
use crate::{Block, BlockKind};

/// One line of the page, as the judge reads it.
#[derive(Debug, Clone)]
pub struct Line {
    /// The line's box, PDF user space, y up.
    pub bbox: [f64; 4],
    /// Its words joined by one space.
    pub text: String,
    /// How many words it holds.
    pub words: usize,
    /// How many of them hold an ASCII digit.
    pub digits: usize,
    /// The kind of the block it came from, as the dump names it.
    pub kind: &'static str,
    /// The builder's table the block belongs to, counted as the dump counts it.
    pub table: Option<u32>,
    /// The builder's `GridDoubt::fill` on that block, NaN where it has none.
    pub grid_fill: f64,
    /// The builder's `GridDoubt::columns` on that block, NaN where it has none.
    pub grid_columns: f64,
    /// The block the line came from.
    pub block: usize,
    /// What the list models say of the line (`list_signal`), NaN where they
    /// did not read it.
    pub list: f64,
}

/// Where a proposal came from: the judge's first seven features.
pub const KINDS: [&str; 8] = [
    "builder",
    "aligned",
    "booktabs",
    "pair",
    "leader",
    "alignment_lines",
    "split",
    "union",
];

/// A candidate table: a set of line indices, sorted, and the kind that drew it
/// first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proposal {
    /// Indices into the page's lines, ascending.
    pub lines: Vec<usize>,
    /// One of [`KINDS`].
    pub kind: &'static str,
    /// Whether the detector that drew it would keep it.
    pub accepted: bool,
}

/// The builder's table of each block, as `pdf-learned` counts it for the dump:
/// a new table where a cell does not follow the one before it in row-major
/// order, or where another block broke the run.
#[doc(hidden)]
pub fn builder_tables(blocks: &[&Block]) -> Vec<Option<u32>> {
    let mut count = 0u32;
    let mut previous: Option<(u16, u16)> = None;
    blocks
        .iter()
        .map(|b| match b.kind {
            BlockKind::TableCell { row, column, .. } => {
                let continues = previous.is_some_and(|(r, c)| row > r || (row == r && column > c));
                if !continues {
                    count += 1;
                }
                previous = Some((row, column));
                Some(count - 1)
            }
            _ => {
                previous = None;
                None
            }
        })
        .collect()
}

/// The page's lines in reading order: block by block, `lines_of` inside each.
#[doc(hidden)]
pub fn lines_of_page(blocks: &[&Block], words: &[PageWord], signal: &[f32]) -> Vec<Line> {
    lines_of_grouped(
        blocks,
        &super::lines::grouped_lines(words, blocks.len()),
        signal,
    )
}

/// [`lines_of_page`] over lines grouped once: the same lines, without
/// re-grouping the page for them.
#[doc(hidden)]
pub fn lines_of_grouped(
    blocks: &[&Block],
    grouped: &[Vec<Vec<&PageWord>>],
    signal: &[f32],
) -> Vec<Line> {
    let tables = builder_tables(blocks);
    let mut out = Vec::new();
    for (index, block) in blocks.iter().enumerate() {
        let (kind, _) = kind_name(&block.kind);
        let (fill, columns) = match block.kind {
            BlockKind::TableCell { .. } => block
                .grid
                .map(|g| (g.fill, g.columns as f64))
                .unwrap_or((f64::NAN, f64::NAN)),
            _ => (f64::NAN, f64::NAN),
        };
        let pieces: &[Vec<&PageWord>] = grouped.get(index).map(Vec::as_slice).unwrap_or(&[]);
        push_block_lines(
            &mut out,
            index,
            kind,
            tables[index],
            fill,
            columns,
            pieces,
            signal,
        );
    }
    out
}

/// One block's lines appended to the page's, in the block's line order: the
/// tail [`lines_of_page`] and [`lines_of_grouped`] share.
#[allow(clippy::too_many_arguments)]
fn push_block_lines(
    out: &mut Vec<Line>,
    index: usize,
    kind: &'static str,
    table: Option<u32>,
    fill: f64,
    columns: f64,
    pieces: &[Vec<&PageWord>],
    signal: &[f32],
) {
    for piece in pieces {
        if piece.is_empty() {
            continue;
        }
        let mut bbox = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for w in piece {
            bbox[0] = bbox[0].min(w.bbox[0]);
            bbox[1] = bbox[1].min(w.bbox[1]);
            bbox[2] = bbox[2].max(w.bbox[2]);
            bbox[3] = bbox[3].max(w.bbox[3]);
        }
        out.push(Line {
            bbox,
            text: piece
                .iter()
                .map(|w| w.text.as_str())
                .collect::<Vec<&str>>()
                .join(" "),
            words: piece.len(),
            digits: piece
                .iter()
                .filter(|w| w.text.chars().any(|c| c.is_ascii_digit()))
                .count(),
            kind,
            table,
            grid_fill: fill,
            grid_columns: columns,
            block: index,
            list: signal
                .get(out.len())
                .map(|&p| f64::from(p))
                .unwrap_or(f64::NAN),
        });
    }
}

/// The lines a region's hull holds: the line's centre inside the hull, grown by
/// half the line's height up and down -- the rule the dump's
/// `table_proposals` uses, so the model is served the sets it was trained on.
#[doc(hidden)]
pub fn held_by(hull: [f64; 4], lines: &[Line]) -> Vec<usize> {
    let [left, bottom, right, top] = hull;
    lines
        .iter()
        .enumerate()
        .filter(|(_, l)| {
            let b = l.bbox;
            let (cx, cy) = ((b[0] + b[2]) * 0.5, (b[1] + b[3]) * 0.5);
            let half = (b[3] - b[1]) * 0.5;
            cx >= left && cx <= right && cy >= bottom - half && cy <= top + half
        })
        .map(|(i, _)| i)
        .collect()
}

fn overlaps(a: &[usize], b: &[usize]) -> bool {
    // Both sorted ascending.
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
            std::cmp::Ordering::Equal => return true,
        }
    }
    false
}

fn union_of(sets: &[&Vec<usize>]) -> Vec<usize> {
    let mut out: Vec<usize> = sets.iter().flat_map(|s| s.iter().copied()).collect();
    out.sort_unstable();
    out.dedup();
    out
}

fn iou(a: &[usize], b: &[usize]) -> f64 {
    let both = a.iter().filter(|x| b.binary_search(x).is_ok()).count();
    let either = a.len() + b.len() - both;
    both as f64 / either.max(1) as f64
}

/// Every distinct proposal on a page, with the kind that drew it first.
///
/// `drawn` is what the dump's `table_proposals` holds, in its order: the
/// detectors' regions as `(detector, lines, accepted)`, then the alignment
/// proposer's as `("alignment_lines", lines, true)`, then the pieces as
/// `("split", lines, true)`.
#[doc(hidden)]
pub fn proposals(lines: &[Line], drawn: &[(&'static str, Vec<usize>, bool)]) -> Vec<Proposal> {
    // The builder's tables, keyed as Python keys them: `sorted(..., key=str)`
    // over the table numbers, so table 10 comes before table 2.
    let mut keys: Vec<u32> = lines
        .iter()
        .filter_map(|l| match l.kind {
            "table_cell" => l.table,
            _ => None,
        })
        .collect();
    keys.sort_unstable();
    keys.dedup();
    keys.sort_by_key(|k| k.to_string());
    let builder: Vec<Vec<usize>> = keys
        .iter()
        .map(|&k| {
            (0..lines.len())
                .filter(|&i| lines[i].kind == "table_cell" && lines[i].table == Some(k))
                .collect()
        })
        .collect();
    let drawn: Vec<(&'static str, Vec<usize>, bool)> = drawn
        .iter()
        .map(|(k, s, a)| {
            let mut s: Vec<usize> = s.iter().copied().filter(|&i| i < lines.len()).collect();
            s.sort_unstable();
            s.dedup();
            (*k, s, *a)
        })
        .filter(|(_, s, _)| !s.is_empty())
        .collect();
    let regions: Vec<&Vec<usize>> = builder
        .iter()
        .chain(
            drawn
                .iter()
                .filter(|(k, _, _)| *k != "alignment_lines" && *k != "split")
                .map(|(_, s, _)| s),
        )
        .collect();
    let aligned: Vec<&Vec<usize>> = drawn
        .iter()
        .filter(|(k, _, _)| *k == "alignment_lines")
        .map(|(_, s, _)| s)
        .collect();
    let mut unions: Vec<Vec<usize>> = Vec::new();
    for a in &regions {
        for b in &aligned {
            if overlaps(a, b) {
                unions.push(union_of(&[a, b]));
            }
        }
    }
    for b in &aligned {
        let hit: Vec<&Vec<usize>> = builder.iter().filter(|a| overlaps(a, b)).collect();
        if hit.len() >= 2 {
            let mut all = hit.clone();
            all.push(b);
            unions.push(union_of(&all));
        }
    }
    let mut out: Vec<Proposal> = Vec::new();
    let mut push = |set: &Vec<usize>, kind: &'static str, accepted: bool| {
        if !out.iter().any(|p| &p.lines == set) {
            let kind = if KINDS.contains(&kind) {
                kind
            } else {
                "aligned"
            };
            out.push(Proposal {
                lines: set.clone(),
                kind,
                accepted,
            });
        }
    };
    for s in &builder {
        push(s, "builder", true);
    }
    for (k, s, a) in &drawn {
        push(s, k, *a);
    }
    for s in &unions {
        push(s, "union", true);
    }
    out
}

/// A run of four or more dots, dashes or underscores with at most one
/// whitespace between each: `(\.\s?){4,}|(-\s?){4,}|(_\s?){4,}`.
fn has_leader(text: &str) -> bool {
    for mark in ['.', '-', '_'] {
        let mut run = 0usize;
        let mut between = 0usize;
        for c in text.chars() {
            if c == mark {
                run = if run > 0 && between <= 1 { run + 1 } else { 1 };
                between = 0;
                if run >= 4 {
                    return true;
                }
            } else if c.is_whitespace() && run > 0 && between == 0 {
                between = 1;
            } else {
                run = 0;
                between = 0;
            }
        }
    }
    false
}

/// `statistics.median`: the mean of the two middle values when the count is
/// even.
fn median(mut values: Vec<f64>) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = values.len();
    if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    }
}

/// How many bands boxes form along one axis, joining where they overlap.
fn band_count(boxes: &[[f64; 4]], low: usize, high: usize) -> usize {
    let mut order: Vec<usize> = (0..boxes.len()).collect();
    order.sort_by(|&a, &b| {
        boxes[a][low]
            .partial_cmp(&boxes[b][low])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut count = 0usize;
    let mut reach = f64::NEG_INFINITY;
    for i in order {
        if count > 0 && boxes[i][low] <= reach {
            reach = reach.max(boxes[i][high]);
        } else {
            count += 1;
            reach = boxes[i][high];
        }
    }
    count
}

/// The number of features, in `train_table_judge.NAMES` order.
pub const FEATURES: usize = 35;

/// What every proposal's features on a page share: computed once per verdict,
/// read once per proposal.
#[doc(hidden)]
pub struct JudgeShared<'a> {
    /// The median line height, or 1.0 where it is 0.0: [`features`]' unit.
    pub unit: f64,
    /// The proposals that are neither alignment, split nor union lines, in
    /// the verdict's order: what [`features`] reads its overlap against.
    pub regions: Vec<&'a Proposal>,
}

impl<'a> JudgeShared<'a> {
    /// The page's shared values for one verdict over `all`.
    #[doc(hidden)]
    pub fn of(lines: &[Line], all: &'a [Proposal]) -> Self {
        let unit = {
            let m = median(lines.iter().map(|l| l.bbox[3] - l.bbox[1]).collect());
            if m == 0.0 {
                1.0
            } else {
                m
            }
        };
        let regions: Vec<&Proposal> = all
            .iter()
            .filter(|p| p.kind != "alignment_lines" && p.kind != "split" && p.kind != "union")
            .collect();
        JudgeShared { unit, regions }
    }
}

/// The buffers one proposal's features are assembled in: cleared per
/// proposal, allocated once per verdict -- so judging ten proposals does not
/// allocate ten pages' worth of line lists.
#[doc(hidden)]
#[derive(Default)]
pub struct JudgeScratch<'a> {
    mine: Vec<&'a Line>,
    boxes: Vec<[f64; 4]>,
    above: Vec<f64>,
    below: Vec<f64>,
    cells: Vec<&'a Line>,
    said: Vec<f64>,
}

/// One proposal's features, in `train_table_judge.NAMES` order.
#[doc(hidden)]
pub fn features(
    page: (f64, f64),
    lines: &[Line],
    candidate: &Proposal,
    all: &[Proposal],
) -> [f64; FEATURES] {
    let shared = JudgeShared::of(lines, all);
    let mut scratch = JudgeScratch::default();
    features_with_shared(page, lines, candidate, all, &shared, &mut scratch)
}

/// [`features`] over one verdict's shared values and scratch buffers: the
/// same 35 numbers, without re-measuring the page or reallocating per
/// proposal. The statements run in the same order over the same values, so
/// the floats come out bit for bit as [`features`] computes them.
#[doc(hidden)]
pub fn features_with_shared<'a>(
    page: (f64, f64),
    lines: &'a [Line],
    candidate: &Proposal,
    all: &[Proposal],
    shared: &JudgeShared<'_>,
    scratch: &mut JudgeScratch<'a>,
) -> [f64; FEATURES] {
    let mut x = [0.0f64; FEATURES];
    for (k, name) in KINDS.iter().enumerate() {
        x[k] = if candidate.kind == *name { 1.0 } else { 0.0 };
    }
    let JudgeScratch {
        mine,
        boxes,
        above,
        below,
        cells,
        said,
    } = &mut *scratch;
    mine.clear();
    mine.extend(candidate.lines.iter().map(|&i| &lines[i]));
    let n = mine.len() as f64;
    let unit = shared.unit;
    let x0 = mine.iter().map(|l| l.bbox[0]).fold(f64::INFINITY, f64::min);
    let x1 = mine
        .iter()
        .map(|l| l.bbox[2])
        .fold(f64::NEG_INFINITY, f64::max);
    let y0 = mine.iter().map(|l| l.bbox[1]).fold(f64::INFINITY, f64::min);
    let y1 = mine
        .iter()
        .map(|l| l.bbox[3])
        .fold(f64::NEG_INFINITY, f64::max);
    let words: usize = mine.iter().map(|l| l.words).sum();
    let share = |kind: &str| mine.iter().filter(|l| l.kind == kind).count() as f64 / n;
    boxes.clear();
    boxes.extend(mine.iter().map(|l| l.bbox));
    let boxes = &boxes[..];
    let rows = band_count(boxes, 1, 3) as f64;
    let columns = band_count(boxes, 0, 2) as f64;
    let outside = |j: usize| candidate.lines.binary_search(&j).is_err();
    let beside = |l: &Line| l.bbox[0] < x1 && l.bbox[2] > x0;
    above.clear();
    above.extend(
        lines
            .iter()
            .enumerate()
            .filter(|(j, l)| outside(*j) && l.bbox[1] >= y1 - 1e-6 && beside(l))
            .map(|(_, l)| l.bbox[1] - y1),
    );
    let above = &above[..];
    below.clear();
    below.extend(
        lines
            .iter()
            .enumerate()
            .filter(|(j, l)| outside(*j) && l.bbox[3] <= y0 + 1e-6 && beside(l))
            .map(|(_, l)| y0 - l.bbox[3]),
    );
    let below = &below[..];
    let regions = &shared.regions;
    let best_iou = |pool: &mut dyn Iterator<Item = &Proposal>| {
        pool.filter(|p| p.lines != candidate.lines)
            .map(|p| iou(&candidate.lines, &p.lines))
            .fold(0.0f64, f64::max)
    };
    cells.clear();
    cells.extend(mine.iter().filter(|l| l.kind == "table_cell").copied());
    let cells = &cells[..];
    let k = KINDS.len();
    x[k] = n;
    x[k + 1] = n / lines.len().max(1) as f64;
    x[k + 2] = words as f64 / n.max(1.0);
    x[k + 3] = (x1 - x0) / page.0.max(1e-6);
    x[k + 4] = (y1 - y0) / page.1.max(1e-6);
    x[k + 5] = mine.iter().map(|l| l.digits).sum::<usize>() as f64 / words.max(1) as f64;
    x[k + 6] = mine.iter().filter(|l| has_leader(&l.text)).count() as f64 / n;
    x[k + 7] = mine.iter().filter(|l| l.words <= 3).count() as f64 / n;
    x[k + 8] = share("table_cell");
    x[k + 9] = share("paragraph");
    x[k + 10] = share("list_item");
    x[k + 11] = share("heading");
    x[k + 12] = rows;
    x[k + 13] = columns;
    x[k + 14] = n / (rows * columns).max(1.0);
    x[k + 15] = if above.is_empty() {
        99.0
    } else {
        above.iter().copied().fold(f64::INFINITY, f64::min) / unit
    };
    x[k + 16] = if below.is_empty() {
        99.0
    } else {
        below.iter().copied().fold(f64::INFINITY, f64::min) / unit
    };
    x[k + 17] = best_iou(&mut regions.iter().copied());
    x[k + 18] = best_iou(&mut all.iter().filter(|p| p.kind == "alignment_lines"));
    x[k + 19] = regions
        .iter()
        .filter(|p| overlaps(&p.lines, &candidate.lines))
        .count() as f64;
    x[k + 20] = if cells.is_empty() {
        f64::NAN
    } else {
        cells.iter().map(|l| l.grid_fill).sum::<f64>() / cells.len() as f64
    };
    x[k + 21] = if cells.is_empty() {
        f64::NAN
    } else {
        cells.iter().map(|l| l.grid_columns).sum::<f64>() / cells.len() as f64
    };
    x[k + 22] = if candidate.accepted { 1.0 } else { 0.0 };
    // A CHART'S LABELS ALIGN LIKE A TABLE: a row of two-digit year ticks,
    // and runs of figures with no text column.
    let ticks = mine
        .iter()
        .flat_map(|l| l.text.split_whitespace())
        .filter(|t| is_year_tick(t))
        .count();
    x[k + 23] = ticks as f64 / words.max(1) as f64;
    x[k + 24] = mine.iter().filter(|l| has_three_letters(&l.text)).count() as f64 / n;
    // What the list models say of the proposal's lines.
    said.clear();
    said.extend(mine.iter().map(|l| l.list).filter(|v| !v.is_nan()));
    let said = &said[..];
    x[k + 25] = if said.is_empty() {
        f64::NAN
    } else {
        said.iter().sum::<f64>() / said.len() as f64
    };
    x[k + 26] = said.iter().copied().fold(f64::NAN, f64::max);
    x
}

/// `^['\u{2019}]?\d{2}$`: `'13`, `’14` or `15`.
fn is_year_tick(token: &str) -> bool {
    let rest = token
        .strip_prefix('\'')
        .or_else(|| token.strip_prefix('\u{2019}'))
        .unwrap_or(token);
    rest.chars().count() == 2 && rest.chars().all(|c| c.is_ascii_digit())
}

/// `[A-Za-z]{3}` anywhere in the text.
fn has_three_letters(text: &str) -> bool {
    let mut run = 0;
    for c in text.chars() {
        if c.is_ascii_alphabetic() {
            run += 1;
            if run >= 3 {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

/// The probability the judge gives a proposal.
pub fn probability(x: &[f64; FEATURES]) -> f64 {
    super::probability(models::table_judge::predict_margin(x))
}

/// The floor the judge decides at: the shipped model's own.
pub const FLOOR: f64 = models::table_judge::THRESHOLD;

/// Greedy by probability; a proposal sharing a line with a chosen one is
/// dropped. Ties keep the proposals' own order, as Python's stable sort does.
#[doc(hidden)]
pub fn choose(proposals: &[Proposal], probabilities: &[f64], floor: f64) -> Vec<Vec<usize>> {
    let mut order: Vec<usize> = (0..proposals.len()).collect();
    order.sort_by(|&a, &b| {
        probabilities[b]
            .partial_cmp(&probabilities[a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut used: Vec<usize> = Vec::new();
    let mut out = Vec::new();
    for i in order {
        if probabilities[i] < floor {
            break;
        }
        if overlaps(&used, &proposals[i].lines) {
            continue;
        }
        out.push(proposals[i].lines.clone());
        used.extend(proposals[i].lines.iter().copied());
        used.sort_unstable();
    }
    out
}

/// Below this probability a table the builder drew is rejected: read back
/// as prose even when the builder was sure of its grid (#574, T6's prose read
/// as tables: a page of three running-text columns scores 0.004).
///
/// CHOSEN OUT OF FOLD ON TRAINING DOCUMENTS, by what the engine does: the
/// judge's tables plus every unchosen builder table it does not reject.
/// Tables F1 on pages with one / false tables per 100 without one:
/// never 0.6045 / 73.0, 0.02 0.6146 / 35.8, **0.05 0.6159 / 30.4** (the best
/// F1), all unchosen 0.6149 / 25.9. Not "all unchosen": that read six
/// constructed tables as prose; at 0.05 every named test holds.
pub const REJECT: f64 = 0.05;

/// The judge's tables on one page: sets of line indices.
pub fn tables(
    page: (f64, f64),
    lines: &[Line],
    words: &[super::features::PageWord],
    drawn: &[(&'static str, Vec<usize>, bool)],
) -> Vec<Vec<usize>> {
    verdict(page, lines, words, drawn).0
}

/// [`verdict`], with the numbers it decided by beside the tables.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub struct VerdictTraced {
    /// The chosen tables, as [`verdict`] returns them.
    pub chosen: Vec<Vec<usize>>,
    /// The builder tables it rejects, as [`verdict`] returns them.
    pub rejected: Vec<Vec<usize>>,
    /// One entry per chosen table, in the same order.
    pub details: Vec<super::verifier::Choice>,
    /// The highest judge probability among candidates not chosen and sharing
    /// no line with a chosen one; `None` with no such candidate.
    pub p_best_rejected: Option<f64>,
    /// How many candidates the judge saw on the page.
    pub n_candidates: usize,
    /// The judge probability of the builder proposal covering each line, in
    /// line order; `None` where no builder proposal covers the line. The
    /// final-table judge names a builder table it chose no part of by this,
    /// from this same verdict rather than a second one.
    pub builder_p: Vec<Option<f64>>,
}

/// The judge's tables, and the builder's tables it rejects: those under
/// [`REJECT`] that share no line with a chosen table.
pub fn verdict(
    page: (f64, f64),
    lines: &[Line],
    words: &[super::features::PageWord],
    drawn: &[(&'static str, Vec<usize>, bool)],
) -> (Vec<Vec<usize>>, Vec<Vec<usize>>) {
    let traced = verdict_traced(page, lines, words, drawn);
    (traced.chosen, traced.rejected)
}

/// [`verdict`], with the numbers it decided by: one computation, so the
/// signals cannot drift from the decision.
#[doc(hidden)]
pub fn verdict_traced(
    page: (f64, f64),
    lines: &[Line],
    words: &[super::features::PageWord],
    drawn: &[(&'static str, Vec<usize>, bool)],
) -> VerdictTraced {
    verdict_on_proposals(page, lines, words, &proposals(lines, drawn))
}

/// [`verdict_traced`] over the page's proposals: with no candidate regions
/// the verdict is empty without running the verifier, exactly as the full
/// verdict computes it for zero candidates (held by a test below).
pub(crate) fn verdict_on_proposals(
    page: (f64, f64),
    lines: &[Line],
    words: &[super::features::PageWord],
    all: &[Proposal],
) -> VerdictTraced {
    if all.is_empty() {
        // NOTHING TO CHOOSE, REJECT OR NAME. The full verdict over zero
        // proposals chooses nothing (no scored candidate), rejects nothing
        // (no builder proposal under the bar), names no best rejected and
        // covers no line with a builder probability -- so the skip returns
        // that verdict directly, and the test below holds it to the full
        // computation.
        return VerdictTraced {
            chosen: Vec::new(),
            rejected: Vec::new(),
            details: Vec::new(),
            p_best_rejected: None,
            n_candidates: 0,
            builder_p: vec![None; lines.len()],
        };
    }
    verdict_scored(page, lines, words, all)
}

/// The full verdict: every proposal scored, the verifier's choice beside it.
/// Shared by [`verdict_on_proposals`] and by the test that proves the skip.
fn verdict_scored(
    page: (f64, f64),
    lines: &[Line],
    words: &[super::features::PageWord],
    all: &[Proposal],
) -> VerdictTraced {
    let shared = JudgeShared::of(lines, all);
    let mut scratch = JudgeScratch::default();
    let p: Vec<f64> = all
        .iter()
        .map(|c| {
            probability(&features_with_shared(
                page,
                lines,
                c,
                all,
                &shared,
                &mut scratch,
            ))
        })
        .collect();
    // THE JUDGE SAYS WHETHER, THE VERIFIER WHICH (#638): among the
    // candidates at or over the judge's floor, the one whose decoded grid
    // reads best is taken first.
    let traced = super::verifier::choose_traced(page, lines, words, all, &p, FLOOR);
    let chosen = traced.chosen;
    let used: Vec<usize> = {
        let mut u: Vec<usize> = chosen.iter().flatten().copied().collect();
        u.sort_unstable();
        u
    };
    let rejected = all
        .iter()
        .zip(&p)
        .filter(|(c, q)| c.kind == "builder" && **q < REJECT && !overlaps(&used, &c.lines))
        .map(|(c, _)| c.lines.clone())
        .collect();
    let winners: Vec<usize> = traced.details.iter().map(|d| d.proposal).collect();
    let mut best: Option<f64> = None;
    for (i, candidate) in all.iter().enumerate() {
        if winners.contains(&i) {
            continue;
        }
        if overlaps(&used, &candidate.lines) {
            continue;
        }
        best = Some(best.map_or(p[i], |b: f64| b.max(p[i])));
    }
    let n_candidates = all.len();
    // THE BUILDER PROBABILITY PER LINE: each builder proposal's lines are
    // the lines of one builder table, so every line of it names that table's
    // probability. The last writer wins where two builder proposals share a
    // line; branch order there is detector order, as everywhere else.
    let mut builder_p = vec![None; lines.len()];
    for (proposal, prob) in all.iter().zip(&p) {
        if proposal.kind != "builder" {
            continue;
        }
        for &i in &proposal.lines {
            builder_p[i] = Some(*prob);
        }
    }
    VerdictTraced {
        chosen,
        rejected,
        details: traced.details,
        p_best_rejected: best,
        n_candidates,
        builder_p,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, y0: f64, kind: &'static str, block: usize) -> Line {
        Line {
            bbox: [72.0, y0, 300.0, y0 + 10.0],
            text: text.to_string(),
            words: text.split_whitespace().count(),
            digits: text
                .split_whitespace()
                .filter(|w| w.chars().any(|c| c.is_ascii_digit()))
                .count(),
            kind,
            table: if kind == "table_cell" { Some(0) } else { None },
            grid_fill: if kind == "table_cell" { 0.9 } else { f64::NAN },
            grid_columns: if kind == "table_cell" { 3.0 } else { f64::NAN },
            block,
            list: if kind == "paragraph" { 0.7 } else { f64::NAN },
        }
    }

    /// Features over shared values and reused buffers are the features: run
    /// twice, so a buffer the second proposal does not clear fails.
    #[test]
    fn features_over_reused_buffers_match_features() {
        let lines = vec![
            line("Name 2021 2022", 700.0, "table_cell", 0),
            line("Cash 10 20", 686.0, "table_cell", 0),
            line("A paragraph of running text here", 660.0, "paragraph", 1),
        ];
        let all = vec![
            Proposal {
                lines: vec![0, 1],
                kind: "builder",
                accepted: true,
            },
            Proposal {
                lines: vec![0, 1, 2],
                kind: "aligned",
                accepted: false,
            },
            Proposal {
                lines: vec![2],
                kind: "alignment_lines",
                accepted: true,
            },
        ];
        let page = (612.0, 792.0);
        let shared = JudgeShared::of(&lines, &all);
        let mut scratch = JudgeScratch::default();
        for _ in 0..2 {
            for candidate in &all {
                let reused =
                    features_with_shared(page, &lines, candidate, &all, &shared, &mut scratch);
                let fresh = features(page, &lines, candidate, &all);
                for (k, (&a, &b)) in reused.iter().zip(&fresh).enumerate() {
                    assert!(
                        a == b || (a.is_nan() && b.is_nan()),
                        "proposal {:?} feature {k} reads {a} over reused buffers, {b} fresh",
                        candidate.lines
                    );
                }
            }
        }
    }

    /// The skipped verdict is the full verdict for zero candidates: nothing
    /// chosen, nothing rejected, no best, zero candidates, no line named.
    #[test]
    fn the_skipped_verdict_is_the_full_verdict_for_no_candidates() {
        let lines = vec![
            line("A paragraph of running text here", 700.0, "paragraph", 0),
            line("And one more line under it", 686.0, "paragraph", 0),
        ];
        let words: Vec<PageWord> = Vec::new();
        let all: Vec<Proposal> = Vec::new();
        let page = (612.0, 792.0);
        assert_eq!(
            verdict_on_proposals(page, &lines, &words, &all),
            verdict_scored(page, &lines, &words, &all),
            "the skip returns what the full verdict computes"
        );
        let skipped = verdict_on_proposals(page, &lines, &words, &all);
        assert!(skipped.chosen.is_empty());
        assert!(skipped.rejected.is_empty());
        assert!(skipped.details.is_empty());
        assert_eq!(skipped.p_best_rejected, None);
        assert_eq!(skipped.n_candidates, 0);
        assert_eq!(skipped.builder_p, vec![None, None]);
    }

    #[test]
    fn a_leader_is_four_marks_with_at_most_one_space_between() {
        assert!(has_leader("Revenues ........ 954"));
        assert!(has_leader("a . . . . b"));
        assert!(has_leader("----"));
        assert!(!has_leader("a ...  . b"));
        assert!(!has_leader("1.2.3"));
        assert!(!has_leader("a.  .  .  . b"));
    }
}
