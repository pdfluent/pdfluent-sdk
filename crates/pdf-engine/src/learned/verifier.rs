//! The table verifier: which of the judge's candidates is the table (#574, #638).
//!
//! THE LOSS IS THE REGION, NOT THE GRID. On PubTables pages no training run
//! reads, our decoder on the best proposal a page already makes scores TEDS
//! 0.852, while the region the judge chooses scores 0.672: it hits the truth
//! region for 57 % of the tables. So the judge still decides WHETHER a
//! candidate is a table (its floor), and this ranker decides WHICH: every
//! candidate is decoded, and its grid is read for what a wrong crop leaves --
//! words cut at the edge, a table running on past it, rows that stop being
//! regular, cells of running text.
//!
//! Trained by `scripts/extraction/learned/table_verifier.py`, whose
//! `candidates` and `features` this module computes; the parity test holds
//! the two together.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use super::features::PageWord;
use super::judge::{Line, Proposal};
use super::{models, separations};

/// The number of features, in `table_verifier.NAMES` order.
pub const FEATURES: usize = 18;

/// The candidates read per page: the judge's most likely, distinct by lines.
pub const MOST_CANDIDATES: usize = 12;

/// Row gaps under this share of a line are the same row.
const ROW_GAP: f64 = 0.5;
/// A table running on is within this many lines of the hull.
const NEAR: f64 = 1.5;
/// A caption sits within this many lines above the table.
const CAPTION_REACH: f64 = 3.0;
/// A cell of running text: this many words, across all but one column.
const PROSE_WORDS: usize = 8;
/// Two candidates agree when their hulls overlap this much.
const AGREE: f64 = 0.8;
/// A cell of a column of running text: this many words.
const COLUMN_PROSE_WORDS: usize = 6;
/// A column is read for running text when it holds this many cells.
const COLUMN_CELLS: usize = 3;
/// The decoder's floor, as the learned pass decodes a region.
const SEPARATION_FLOOR: f64 = 0.1;

/// The judge's most likely proposals, distinct by lines, most likely first.
pub fn candidates(all: &[Proposal], probabilities: &[f64]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..all.len()).collect();
    order.sort_by(|&a, &b| {
        probabilities[b]
            .total_cmp(&probabilities[a])
            .then(a.cmp(&b))
    });
    let mut seen: Vec<&[usize]> = Vec::new();
    let mut out = Vec::new();
    for i in order {
        let lines = all[i].lines.as_slice();
        if lines.is_empty() || seen.contains(&lines) {
            continue;
        }
        seen.push(lines);
        out.push(i);
        if out.len() >= MOST_CANDIDATES {
            break;
        }
    }
    out
}

/// The box round a set of lines.
pub fn hull_of(lines: &[Line], members: &[usize]) -> [f64; 4] {
    let mut hull = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for &j in members {
        let b = lines[j].bbox;
        hull[0] = hull[0].min(b[0]);
        hull[1] = hull[1].min(b[1]);
        hull[2] = hull[2].max(b[2]);
        hull[3] = hull[3].max(b[3]);
    }
    hull
}

fn centre(b: &[f64; 4]) -> (f64, f64) {
    ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0)
}

/// The words whose centre lies in the hull, in the page's order.
pub fn inside_of(words: &[PageWord], hull: &[f64; 4]) -> Vec<usize> {
    (0..words.len())
        .filter(|&i| {
            let (x, y) = centre(&words[i].bbox);
            hull[0] <= x && x <= hull[2] && hull[1] <= y && y <= hull[3]
        })
        .collect()
}

fn iou(a: &[f64; 4], b: &[f64; 4]) -> f64 {
    let ix = (a[2].min(b[2]) - a[0].max(b[0])).max(0.0);
    let iy = (a[3].min(b[3]) - a[1].max(b[1])).max(0.0);
    let union = (a[2] - a[0]) * (a[3] - a[1]) + (b[2] - b[0]) * (b[3] - b[1]) - ix * iy;
    if union > 0.0 {
        ix * iy / union
    } else {
        0.0
    }
}

fn median(mut values: Vec<f64>) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    let n = values.len();
    if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    }
}

/// The decoder's grid over these words, as the learned pass decodes a region.
pub fn grid(words: &[&PageWord]) -> Option<separations::Grid> {
    if words.len() < 4 {
        return None;
    }
    let mine: Vec<separations::Word> = words
        .iter()
        .map(|w| separations::Word {
            bbox: w.bbox,
            numeric: w.text.chars().any(|c| c.is_ascii_digit()),
        })
        .collect();
    separations::cells_of(&mine, &[], SEPARATION_FLOOR)
}

/// `Table 2`, `TABLE IV:`, `Table S1`: `tables::announces_a_table` for tables alone.
fn table_caption(text: &str) -> bool {
    let mut words = text.split_whitespace();
    let (Some(first), Some(second)) = (words.next(), words.next()) else {
        return false;
    };
    first.trim_end_matches(':').eq_ignore_ascii_case("table")
        && crate::tables::identifier_of_a_table(second.trim_end_matches(['.', ':', ',']))
}

/// One candidate's features, in `table_verifier.NAMES` order.
#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub fn features(
    size: (f64, f64),
    lines: &[Line],
    words: &[PageWord],
    hull: &[f64; 4],
    hulls: &[[f64; 4]],
    me: usize,
    p: f64,
    builder: bool,
    cells: &[super::grid::Cell],
) -> [f64; FEATURES] {
    let inside = inside_of(words, hull);
    let mut scratch = VerifierScratch::default();
    features_with_inside(
        size,
        lines,
        words,
        hull,
        hulls,
        me,
        p,
        builder,
        cells,
        &inside,
        word_unit(words),
        &mut scratch,
    )
}

/// The median word height on the page, or 1.0 where it is 0.0: [`features`]'
/// unit, hoisted out of the per-candidate loop that read it up to twelve
/// times a page.
#[doc(hidden)]
pub fn word_unit(words: &[PageWord]) -> f64 {
    let m = median(words.iter().map(|w| w.bbox[3] - w.bbox[1]).collect());
    if m == 0.0 {
        1.0
    } else {
        m
    }
}

/// The buffers one candidate's features are assembled in: cleared per
/// candidate, allocated once per page -- so verifying twelve candidates does
/// not allocate twelve pages' worth of word lists.
#[doc(hidden)]
#[derive(Default)]
pub struct VerifierScratch {
    held: Vec<bool>,
    ys: Vec<f64>,
    gaps: Vec<f64>,
    lefts: Vec<f64>,
    numeric: Vec<bool>,
    head: Vec<bool>,
    body: Vec<bool>,
}

/// [`features`] over the hull's words, the page's unit and scratch buffers:
/// the same 18 numbers, without re-measuring the page, re-collecting the
/// hull's words or reallocating per candidate. The statements run in the same
/// order over the same values, so the floats come out bit for bit as
/// [`features`] computes them.
#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub fn features_with_inside(
    size: (f64, f64),
    lines: &[Line],
    words: &[PageWord],
    hull: &[f64; 4],
    hulls: &[[f64; 4]],
    me: usize,
    p: f64,
    builder: bool,
    cells: &[super::grid::Cell],
    inside: &[usize],
    unit: f64,
    scratch: &mut VerifierScratch,
) -> [f64; FEATURES] {
    let (width, height) = size;
    let VerifierScratch {
        held,
        ys,
        gaps,
        lefts,
        numeric,
        head,
        body,
    } = &mut *scratch;
    held.clear();
    held.resize(words.len(), false);
    for &i in inside {
        held[i] = true;
    }
    let held = &held[..];
    let near = (0..words.len())
        .filter(|&i| {
            let (x, y) = centre(&words[i].bbox);
            !held[i]
                && hull[0] <= x
                && x <= hull[2]
                && ((0.0 < hull[1] - y && hull[1] - y <= NEAR * unit)
                    || (0.0 < y - hull[3] && y - hull[3] <= NEAR * unit))
        })
        .count();
    let cut = (0..words.len())
        .filter(|&i| {
            let b = words[i].bbox;
            !held[i] && b[0] < hull[2] && b[2] > hull[0] && b[1] < hull[3] && b[3] > hull[1]
        })
        .count();
    ys.clear();
    ys.extend(inside.iter().map(|&i| centre(&words[i].bbox).1));
    ys.sort_by(f64::total_cmp);
    let ys = &ys[..];
    gaps.clear();
    gaps.extend(
        ys.windows(2)
            .map(|w| w[1] - w[0])
            .filter(|&g| g > ROW_GAP * unit),
    );
    let gaps = &gaps[..];
    let row_cv = if gaps.len() > 1 {
        let mean = gaps.iter().sum::<f64>() / gaps.len() as f64;
        let var = gaps.iter().map(|g| (g - mean) * (g - mean)).sum::<f64>() / gaps.len() as f64;
        var.sqrt() / mean
    } else {
        0.0
    };
    lefts.clear();
    lefts.extend(inside.iter().map(|&i| words[i].bbox[0]));
    let lefts = &lefts[..];
    let residual = if lefts.len() > 2 {
        let nearest: f64 = (0..lefts.len())
            .map(|j| {
                (0..lefts.len())
                    .filter(|&k| k != j)
                    .map(|k| (lefts[j] - lefts[k]).abs())
                    .fold(f64::INFINITY, f64::min)
            })
            .sum();
        nearest / lefts.len() as f64 / unit
    } else {
        0.0
    };
    let agree = hulls
        .iter()
        .enumerate()
        .filter(|&(k, h)| k != me && iou(h, hull) >= AGREE)
        .count();
    let caption = lines.iter().any(|l| {
        l.bbox[1] >= hull[3]
            && l.bbox[1] - hull[3] <= CAPTION_REACH * unit
            && l.bbox[0] < hull[2]
            && l.bbox[2] > hull[0]
            && table_caption(&l.text)
    });
    let n_rows = cells
        .iter()
        .filter_map(|c| c.rows.iter().max())
        .max()
        .map_or(0, |r| r + 1);
    let n_cols = cells
        .iter()
        .filter_map(|c| c.columns.iter().max())
        .max()
        .map_or(0, |c| c + 1);
    numeric.clear();
    numeric.extend(cells.iter().map(|c| {
        c.words
            .iter()
            .any(|&j| words[inside[j]].text.chars().any(|ch| ch.is_ascii_digit()))
    }));
    let numeric = &numeric[..];
    let share = |xs: &[bool]| xs.iter().filter(|&&n| n).count() as f64 / xs.len().max(1) as f64;
    let first_row = |c: &super::grid::Cell| c.rows.iter().min().copied().unwrap_or(0);
    head.clear();
    head.extend(
        cells
            .iter()
            .zip(numeric)
            .filter(|(c, _)| first_row(c) == 0)
            .map(|(_, &n)| n),
    );
    let head = &head[..];
    body.clear();
    body.extend(
        cells
            .iter()
            .zip(numeric)
            .filter(|(c, _)| first_row(c) > 0)
            .map(|(_, &n)| n),
    );
    let body = &body[..];
    let prose = cells
        .iter()
        .filter(|c| {
            c.words.len() >= PROSE_WORDS && c.columns.len() >= n_cols.saturating_sub(1).max(1)
        })
        .count();
    let spanning = cells.iter().filter(|c| c.columns.len() > 1).count();
    [
        p,
        if builder { 1.0 } else { 0.0 },
        inside.len() as f64,
        n_rows as f64,
        n_cols as f64,
        cells.len() as f64 / (n_rows * n_cols).max(1) as f64,
        share(numeric),
        share(body) - share(head),
        row_cv,
        residual,
        near as f64,
        cut as f64,
        agree as f64,
        if caption { 1.0 } else { 0.0 },
        prose as f64,
        (hull[2] - hull[0]) * (hull[3] - hull[1]) / (width * height).max(1.0),
        spanning as f64 / cells.len().max(1) as f64,
        prose_column(cells),
    ]
}

/// The largest share of one column's own cells that hold running text: the
/// other column of a two-column page, taken in as a column (#638).
pub(crate) fn prose_column(cells: &[super::grid::Cell]) -> f64 {
    let mut columns: std::collections::BTreeMap<usize, Vec<usize>> = Default::default();
    for c in cells {
        if c.columns.len() == 1 {
            columns.entry(c.columns[0]).or_default().push(c.words.len());
        }
    }
    columns
        .values()
        .filter(|sizes| sizes.len() >= COLUMN_CELLS)
        .map(|sizes| {
            sizes.iter().filter(|&&n| n >= COLUMN_PROSE_WORDS).count() as f64 / sizes.len() as f64
        })
        .fold(0.0, f64::max)
}

/// The ranker's score for one candidate: higher is the better reading.
pub fn score(x: &[f64; FEATURES]) -> f64 {
    models::table_verifier::predict_margin(x)
}

/// One chosen table, with the numbers that chose it: the judge probability
/// of its candidate, its verifier score, and the margin to the best scored
/// candidate it beat that shares a line with it.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub struct Choice {
    /// Index into the proposals `choose` was given.
    pub proposal: usize,
    /// The judge probability of that candidate.
    pub p: f64,
    /// Its verifier score (`score`, a margin and not a probability).
    pub score: f64,
    /// Its score minus the best scored candidate it beat that shares a line
    /// with it; `None` when no scored candidate competed.
    pub margin: Option<f64>,
    /// Whether title rows were cut off its lines (`without_title_rows`).
    pub title_removed: bool,
    /// Its lines after the title rows leave: the table as chosen.
    pub lines: Vec<usize>,
    /// The proposer kind that drew the winning candidate first, one of
    /// [`KINDS`][super::judge::KINDS]: the final-table judge reads it beside
    /// the probability, from this same trace.
    pub kind: &'static str,
}

/// [`choose`], with the numbers it decided by beside the tables.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub struct ChooseTraced {
    /// The chosen tables, as [`choose`] returns them.
    pub chosen: Vec<Vec<usize>>,
    /// One entry per chosen table, in the same order.
    pub details: Vec<Choice>,
}

fn shares_line(a: &[usize], b: &[usize]) -> bool {
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

/// The tables on a page: among the candidates the judge would take (at or
/// over `floor`), best ranked first, none sharing a line with one before it.
pub fn choose(
    size: (f64, f64),
    lines: &[Line],
    words: &[PageWord],
    all: &[Proposal],
    probabilities: &[f64],
    floor: f64,
) -> Vec<Vec<usize>> {
    choose_traced(size, lines, words, all, probabilities, floor).chosen
}

/// [`choose`], with the numbers it decided by: one computation, so the
/// signals cannot drift from the decision.
#[doc(hidden)]
pub fn choose_traced(
    size: (f64, f64),
    lines: &[Line],
    words: &[PageWord],
    all: &[Proposal],
    probabilities: &[f64],
    floor: f64,
) -> ChooseTraced {
    let picked = candidates(all, probabilities);
    let hulls: Vec<[f64; 4]> = picked
        .iter()
        .map(|&i| hull_of(lines, &all[i].lines))
        .collect();
    // The candidate buffers, once per page rather than once per candidate;
    // each hull's words are collected once and read by both the grid decode
    // and the features. The page's unit is read on the first scored
    // candidate and shared by the rest -- nothing is measured where nothing
    // scores, as before.
    let mut unit: Option<f64> = None;
    let mut scratch = VerifierScratch::default();
    let mut mine: Vec<&PageWord> = Vec::new();
    let mut scored: Vec<(usize, f64, Vec<usize>)> = Vec::new();
    for (k, &i) in picked.iter().enumerate() {
        if probabilities[i] < floor {
            continue;
        }
        let unit = *unit.get_or_insert_with(|| word_unit(words));
        let inside = inside_of(words, &hulls[k]);
        mine.clear();
        mine.extend(inside.iter().map(|&j| &words[j]));
        let cells = grid(&mine).map(|g| g.cells).unwrap_or_default();
        let x = features_with_inside(
            size,
            lines,
            words,
            &hulls[k],
            &hulls,
            k,
            probabilities[i],
            all[i].kind == "builder",
            &cells,
            &inside,
            unit,
            &mut scratch,
        );
        let kept = without_title_rows(lines, words, &all[i].lines, &inside, &cells);
        scored.push((i, score(&x), kept));
    }
    scored.sort_by(|a, b| {
        b.1.total_cmp(&a.1)
            .then(probabilities[b.0].total_cmp(&probabilities[a.0]))
            .then(a.0.cmp(&b.0))
    });
    let mut used: Vec<bool> = vec![false; lines.len()];
    let mut out: Vec<Vec<usize>> = Vec::new();
    let mut winners: Vec<usize> = Vec::new();
    for (i, _, kept) in &scored {
        if all[*i].lines.iter().any(|&j| used[j]) {
            continue;
        }
        for &j in kept {
            used[j] = true;
        }
        winners.push(*i);
        out.push(kept.clone());
    }
    let is_winner = |i: usize| winners.contains(&i);
    let mut details = Vec::with_capacity(winners.len());
    for (pos, kept) in out.iter().enumerate() {
        let i = winners[pos];
        let mine = scored
            .iter()
            .find(|(j, _, _)| *j == i)
            .map(|(_, s, _)| *s)
            .unwrap_or(f64::NAN);
        let best_beaten = scored
            .iter()
            .filter(|(j, _, _)| *j != i && !is_winner(*j))
            .filter(|(j, _, _)| shares_line(&all[i].lines, &all[*j].lines))
            .map(|(_, s, _)| *s)
            .fold(f64::NEG_INFINITY, f64::max);
        details.push(Choice {
            proposal: i,
            p: probabilities[i],
            score: mine,
            margin: (best_beaten != f64::NEG_INFINITY).then_some(mine - best_beaten),
            title_removed: *kept != all[i].lines,
            lines: kept.clone(),
            kind: all[i].kind,
        });
    }
    ChooseTraced {
        chosen: out,
        details,
    }
}

/// A candidate's lines without the title rows its grid opens with (#638).
///
/// A region that reaches up over `Table of Contents` and `Exhibit Index` reads
/// each as a row of one cell above the table, and ParseBench keys the table's
/// records by its top rows, so a title there costs the whole table. A leading
/// row of exactly one cell in one column, with rows under it, is a title and
/// its lines leave the table; a row whose one cell spans columns is a heading
/// over them and stays.
pub fn without_title_rows(
    lines: &[Line],
    words: &[PageWord],
    members: &[usize],
    inside: &[usize],
    cells: &[super::grid::Cell],
) -> Vec<usize> {
    let top = |c: &super::grid::Cell| c.rows.iter().min().copied().unwrap_or(0);
    let rows = cells
        .iter()
        .filter_map(|c| c.rows.iter().max())
        .max()
        .map_or(0, |r| r + 1);
    let mut low = f64::INFINITY;
    let mut high = f64::NEG_INFINITY;
    for r in 0..rows {
        let here: Vec<&super::grid::Cell> = cells.iter().filter(|c| top(c) == r).collect();
        if here.len() != 1 || here[0].columns.len() != 1 || r + 1 >= rows {
            break;
        }
        for &w in &here[0].words {
            let b = words[inside[w]].bbox;
            low = low.min(b[1]);
            high = high.max(b[3]);
        }
    }
    if low > high {
        return members.to_vec();
    }
    let kept: Vec<usize> = members
        .iter()
        .copied()
        .filter(|&j| {
            let y = (lines[j].bbox[1] + lines[j].bbox[3]) / 2.0;
            !(low <= y && y <= high)
        })
        .collect();
    if kept.is_empty() {
        members.to_vec()
    } else {
        kept
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(words: usize, row: usize, column: usize) -> super::super::grid::Cell {
        super::super::grid::Cell {
            words: vec![0; words],
            bbox: [0.0; 4],
            rows: vec![row],
            columns: vec![column],
        }
    }

    fn page_word(text: &str, x0: f64, y0: f64, block: usize) -> PageWord {
        PageWord {
            text: text.to_string(),
            bbox: [x0, y0, x0 + 40.0, y0 + 10.0],
            block,
            span: 0,
            size: 10.0,
            bold: false,
            italic: false,
            font: None,
            color: None,
        }
    }

    fn page_line(text: &str, y0: f64, block: usize) -> Line {
        Line {
            bbox: [72.0, y0, 300.0, y0 + 10.0],
            text: text.to_string(),
            words: text.split_whitespace().count(),
            digits: 0,
            kind: "paragraph",
            table: None,
            grid_fill: f64::NAN,
            grid_columns: f64::NAN,
            block,
            list: f64::NAN,
        }
    }

    /// Features over the hull's words, the page's unit and reused buffers are
    /// the features: over two hulls, twice, so a buffer the second candidate
    /// does not clear fails.
    #[test]
    fn features_over_reused_buffers_match_features() {
        let words = vec![
            page_word("Name", 72.0, 700.0, 0),
            page_word("2021", 150.0, 700.0, 0),
            page_word("2022", 230.0, 700.0, 0),
            page_word("Cash", 72.0, 686.0, 0),
            page_word("10", 150.0, 686.0, 0),
            page_word("20", 230.0, 686.0, 0),
        ];
        let lines = vec![
            page_line("Table 1 Sales", 716.0, 1),
            page_line("Name 2021 2022", 700.0, 0),
            page_line("Cash 10 20", 686.0, 0),
        ];
        let hulls = [[70.0, 684.0, 280.0, 712.0], [70.0, 698.0, 280.0, 712.0]];
        let size = (612.0, 792.0);
        let unit = word_unit(&words);
        let mut scratch = VerifierScratch::default();
        for _ in 0..2 {
            for (me, hull) in hulls.iter().enumerate() {
                let inside = inside_of(&words, hull);
                assert!(!inside.is_empty(), "hull {me} holds no word");
                let cells: Vec<super::super::grid::Cell> = inside
                    .iter()
                    .enumerate()
                    .map(|(k, _)| super::super::grid::Cell {
                        words: vec![k],
                        bbox: [0.0; 4],
                        rows: vec![k / 3],
                        columns: vec![k % 3],
                    })
                    .collect();
                let reused = features_with_inside(
                    size,
                    &lines,
                    &words,
                    hull,
                    &hulls,
                    me,
                    0.8,
                    true,
                    &cells,
                    &inside,
                    unit,
                    &mut scratch,
                );
                let fresh = features(size, &lines, &words, hull, &hulls, me, 0.8, true, &cells);
                for (k, (&a, &b)) in reused.iter().zip(&fresh).enumerate() {
                    assert!(
                        a == b || (a.is_nan() && b.is_nan()),
                        "hull {me} feature {k} reads {a} over reused buffers, {b} fresh"
                    );
                }
            }
        }
    }

    #[test]
    fn a_column_of_running_text_reads_as_prose() {
        let mut wordy: Vec<super::super::grid::Cell> = Vec::new();
        let mut plain: Vec<super::super::grid::Cell> = Vec::new();
        for r in 0..4 {
            wordy.push(cell(1, r, 0));
            wordy.push(cell(1, r, 1));
            wordy.push(cell(10, r, 2));
            plain.push(cell(1, r, 0));
            plain.push(cell(1, r, 1));
            plain.push(cell(1, r, 2));
        }
        assert_eq!(prose_column(&wordy), 1.0);
        assert_eq!(prose_column(&plain), 0.0);
    }
}
