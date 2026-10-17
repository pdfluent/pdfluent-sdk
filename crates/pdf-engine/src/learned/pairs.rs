//! What a learned decision reads about a PAIR of words (#600).
//!
//! The table model of #600 answers one question per pair of words -- same
//! cell, same row only, same column only, neither -- and this is what it is
//! given to answer it with.
//!
//! ONE COMPUTATION FOR THE DUMP AND FOR THE ENGINE, the rule
//! [`super::features`] sets and the reason it sets it: the model is trained on
//! what `feature_dump` writes and run on what the engine computes, and if
//! those were two implementations every rounding between them would be the
//! model reading input it never saw. It would look like a weak model, which is
//! the worst kind of bug -- you do not notice it, you explain it wrongly. So
//! the features live here, and both sides call this.
//!
//! WHICH PAIRS ARE ASKED ABOUT. Every pair on a page is quadratic and nearly
//! all of it is a word in one corner against a word in the other: a page holds
//! a median of 708 words and 2,750 neighbour pairs against about 250,000
//! possible ones. [`candidates`] picks the neighbours -- the next word along
//! the line, the nearest below, and the nearest few by centre -- and rows and
//! columns come back out of the decoder by transitivity rather than being
//! asked about directly.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/// A word as the pair features read it: a box, its text, and what our own
/// table reading already says about it.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub struct PairWord {
    /// The word, without surrounding whitespace.
    pub text: String,
    /// `[x0, y0, x1, y1]` in PDF user space, y up.
    pub bbox: [f64; 4],
    /// The table, row and column our heuristics put this word in, if any.
    /// `None` is "we have no opinion", which is not the same as a disagreement
    /// and must not be given to a model as a sentinel number.
    pub table: Option<u32>,
    pub row: Option<u16>,
    pub column: Option<u16>,
}

/// The names of [`features`]'s values, in order.
#[doc(hidden)]
pub const NAMES: [&str; 25] = [
    "gap_x",
    "gap_y",
    "centre_dx",
    "centre_dy",
    "overlap_x",
    "overlap_y",
    "left_aligned",
    "right_aligned",
    "centre_aligned",
    "height_ratio",
    "width_ratio",
    "gap_x_em",
    "gap_y_em",
    "same_band",
    "reading_distance",
    "both_numeric",
    "either_numeric",
    "length_ratio",
    "either_ends_colon",
    "either_all_caps",
    "both_in_a_table",
    "our_same_table",
    "our_same_row",
    "our_same_column",
    "our_same_cell",
];

/// The four classes, in the order the model scores them.
#[doc(hidden)]
pub const CLASSES: [&str; 4] = ["neither", "same_column", "same_row", "same_cell"];

/// Whether the text reads as a number, the way a table cell holds one.
///
/// Deliberately generous at the edges -- a thousands comma, a plus-minus, a
/// footnote dagger -- because what the feature is for is "this cell holds a
/// figure rather than a label", not arithmetic.
fn numeric(text: &str) -> bool {
    let body: String = text
        .trim()
        .trim_matches(|c: char| "()[]%*†‡§,.;:".contains(c))
        .chars()
        .filter(|c| !matches!(c, ',' | '±'))
        .map(|c| if c == '−' || c == '–' { '-' } else { c })
        .collect();
    !body.is_empty() && body.parse::<f64>().is_ok()
}

/// One pair, in the order of [`NAMES`], with the earlier word first.
///
/// Ordered by reading order rather than by whichever the caller passed, so a
/// pair has one description and not two mirror images of it: the model is
/// handed the symmetry instead of having to learn it from examples.
#[doc(hidden)]
pub fn features(a: &PairWord, b: &PairWord) -> [f64; 25] {
    let first_is_a = (-a.bbox[3], a.bbox[0]) <= (-b.bbox[3], b.bbox[0]);
    let (first, second) = if first_is_a { (a, b) } else { (b, a) };
    let [ax0, ay0, ax1, ay1] = first.bbox;
    let [bx0, by0, bx1, by1] = second.bbox;
    let ah = (ay1 - ay0).max(1e-6);
    let aw = (ax1 - ax0).max(1e-6);
    let bh = (by1 - by0).max(1e-6);
    let bw = (bx1 - bx0).max(1e-6);
    let em = ((ah + bh) / 2.0).max(1e-6);

    let gap_x = (ax0.max(bx0) - ax1.min(bx1)).max(0.0);
    let gap_y = (ay0.max(by0) - ay1.min(by1)).max(0.0);
    let over_x = (ax1.min(bx1) - ax0.max(bx0)).max(0.0) / aw.min(bw);
    let over_y = (ay1.min(by1) - ay0.max(by0)).max(0.0) / ah.min(bh);
    let at = first.text.as_str();
    let bt = second.text.as_str();
    let ours = what_we_think(first, second);
    [
        gap_x,
        gap_y,
        (bx0 + bx1) / 2.0 - (ax0 + ax1) / 2.0,
        (by0 + by1) / 2.0 - (ay0 + ay1) / 2.0,
        over_x,
        over_y,
        (ax0 - bx0).abs() / em,
        (ax1 - bx1).abs() / em,
        ((ax0 + ax1) - (bx0 + bx1)).abs() / (2.0 * em),
        ah / bh,
        aw / bw,
        gap_x / em,
        gap_y / em,
        if over_y > 0.5 { 1.0 } else { 0.0 },
        (gap_x + gap_y) / em,
        if numeric(at) && numeric(bt) { 1.0 } else { 0.0 },
        if numeric(at) || numeric(bt) { 1.0 } else { 0.0 },
        at.chars().count() as f64 / (bt.chars().count().max(1)) as f64,
        if at.ends_with(':') || bt.ends_with(':') {
            1.0
        } else {
            0.0
        },
        if is_upper(at) || is_upper(bt) {
            1.0
        } else {
            0.0
        },
        ours[0],
        ours[1],
        ours[2],
        ours[3],
        ours[4],
    ]
}

/// Python's `str.isupper`: at least one cased character, and none lowercase.
fn is_upper(text: &str) -> bool {
    text.chars().any(|c| c.is_alphabetic()) && !text.chars().any(|c| c.is_lowercase())
}

/// Our own reading of the two words, as five flags.
///
/// A word outside any table of ours carries zeroes, which is the honest
/// encoding: not "they disagree" but "we have no opinion". A sentinel such as
/// minus one would have the model learn a magic number and read an absence as
/// a claim.
fn what_we_think(a: &PairWord, b: &PairWord) -> [f64; 5] {
    let (Some(at), Some(bt)) = (a.table, b.table) else {
        return [0.0; 5];
    };
    if at != bt {
        return [1.0, 0.0, 0.0, 0.0, 0.0];
    }
    let same_row = a.row.is_some() && a.row == b.row;
    let same_column = a.column.is_some() && a.column == b.column;
    [
        1.0,
        1.0,
        if same_row { 1.0 } else { 0.0 },
        if same_column { 1.0 } else { 0.0 },
        if same_row && same_column { 1.0 } else { 0.0 },
    ]
}

/// The pairs worth asking about: neighbours, not every pair on the page.
///
/// Three kinds, unioned and each `(i, j)` with `i < j`: the next word along the
/// line, the nearest word below with an overlapping x-range, and the `nearest`
/// closest by centre distance so nothing is invisible for sitting diagonally.
///
/// A GRID, NOT A DOUBLE LOOP. The k-nearest part is the quadratic one and a
/// page holds up to 1,828 words; the words are put in cells of about one line
/// and only the neighbouring cells are searched, which is what keeps this off
/// the critical path (#600).
/// How many nearest neighbours by centre [`candidates`] is asked for where the
/// engine and the dump call it.
///
/// SIX, AND MEASURED AT SIX. The decoder builds rows and columns by transitive
/// closure over the edges it is given, so a sparser candidate set does not
/// lose a little accuracy -- it starves the closure and the grid falls apart.
/// At two the pairs inside a table are 28 per cent of what six gives and TEDS
/// on the same tables is 0.0889 against 0.7960 (#600). It is a cheap-looking
/// knob that is not one.
#[doc(hidden)]
pub const NEAREST: usize = 6;

#[doc(hidden)]
pub fn candidates(words: &[PairWord], nearest: usize) -> Vec<(usize, usize)> {
    use std::collections::{HashMap, HashSet};
    let mut found: HashSet<(usize, usize)> = HashSet::new();
    if words.len() < 2 {
        return Vec::new();
    }
    let step = {
        let mut heights: Vec<f64> = words.iter().map(|w| w.bbox[3] - w.bbox[1]).collect();
        heights.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        (heights[heights.len() / 2] * 2.0).max(1.0)
    };
    let cell_of = |w: &PairWord| {
        (
            ((w.bbox[0] + w.bbox[2]) / 2.0 / step).floor() as i64,
            ((w.bbox[1] + w.bbox[3]) / 2.0 / step).floor() as i64,
        )
    };
    let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, w) in words.iter().enumerate() {
        grid.entry(cell_of(w)).or_default().push(i);
    }

    let overlaps = |a: &[f64; 4], b: &[f64; 4], vertical: bool| -> f64 {
        let (i, j) = if vertical { (1, 3) } else { (0, 2) };
        let wide = a[j].min(b[j]) - a[i].max(b[i]);
        wide / (a[j] - a[i]).min(b[j] - b[i]).max(1e-6)
    };

    // TWICE THE CENTRE, SUMMED AND SUBTRACTED IN PYTHON'S ORDER. The ordering
    // is the same for any grouping, but the ROUNDING is not, and a pair at a
    // near-tie then falls one way here and the other way there. Written as
    // `pair_features.candidates` writes it -- `(b0 + b2) - h0 - h2`, left to
    // right -- so the two answers are equal and not merely close.
    let twice_apart = |a: &PairWord, b: &PairWord| {
        let dx = (b.bbox[0] + b.bbox[2]) - a.bbox[0] - a.bbox[2];
        let dy = (b.bbox[1] + b.bbox[3]) - a.bbox[1] - a.bbox[3];
        dx * dx + dy * dy
    };

    // THE RING GROWS UNTIL IT CANNOT HOLD A BETTER ANSWER. A fixed window is
    // what a grid is for, and it is also how this was wrong: at two cells it
    // missed 159 of the 544 pairs Python's plain double loop finds on a real
    // region -- and the ones it missed were the LONG edges, the ones that join
    // the far side of a table to the near one. The decoder closes rows and
    // columns transitively, so losing the long edges does not cost a little
    // accuracy, it cuts the table in half. Here the ring widens while a word
    // outside it could still beat the best found inside, so the answer is the
    // double loop's and only the work is smaller.
    let (mut lo_x, mut hi_x, mut lo_y, mut hi_y) = (i64::MAX, i64::MIN, i64::MAX, i64::MIN);
    for key in grid.keys() {
        lo_x = lo_x.min(key.0);
        hi_x = hi_x.max(key.0);
        lo_y = lo_y.min(key.1);
        hi_y = hi_y.max(key.1);
    }
    for (i, here) in words.iter().enumerate() {
        let (cx, cy) = cell_of(here);
        let span = (cx - lo_x)
            .abs()
            .max((hi_x - cx).abs())
            .max((cy - lo_y).abs())
            .max((hi_y - cy).abs());
        let mut radius: i64 = 2;
        let mut near: Vec<usize> = Vec::new();
        // THE NEXT WORD ALONG AND THE ONE BELOW ARE A PLAIN SCAN. Their test
        // is a gap between box EDGES while the grid is built on CENTRES, so a
        // wide box can sit nearer than any ring of cells can promise -- the
        // bound that looked sound here missed four pairs on the first page it
        // was asked about. The scan is what `pair_features.candidates` does
        // and it is the cheap half; the k-nearest below is the quadratic one
        // and keeps its grid.
        let mut right: Option<(usize, f64)> = None;
        let mut down: Option<(usize, f64)> = None;
        for (j, other) in words.iter().enumerate() {
            if i == j {
                continue;
            }
            let there = &other.bbox;
            if there[0] >= here.bbox[2] && overlaps(&here.bbox, there, true) > 0.5 {
                let gap = there[0] - here.bbox[2];
                if right.is_none_or(|(_, best)| gap < best) {
                    right = Some((j, gap));
                }
            }
            if there[3] <= here.bbox[1] && overlaps(&here.bbox, there, false) > 0.0 {
                let gap = here.bbox[1] - there[3];
                if down.is_none_or(|(_, best)| gap < best) {
                    down = Some((j, gap));
                }
            }
        }
        let mut by_distance: Vec<(f64, usize)>;
        loop {
            near.clear();
            for dx in -radius..=radius {
                for dy in -radius..=radius {
                    if let Some(bucket) = grid.get(&(cx + dx, cy + dy)) {
                        near.extend(bucket.iter().copied());
                    }
                }
            }
            by_distance = near
                .iter()
                .copied()
                .filter(|&j| j != i)
                .map(|j| (twice_apart(here, &words[j]), j))
                .collect();
            by_distance.sort_by(|a, b| {
                a.0.partial_cmp(&b.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.1.cmp(&b.1))
            });
            if radius >= span {
                break;
            }
            // A word outside the ring sits at least this far away in one axis,
            // so it cannot beat the k-th best already inside it. `by_distance`
            // holds TWICE each coordinate difference, squared, so the bound is
            // squared and multiplied by four in the same breath -- the scale
            // has to match or the ring stops early and the answer is wrong
            // without anything saying so.
            let sure = radius as f64 * step;
            if by_distance.len() >= nearest && by_distance[nearest - 1].0 <= 4.0 * sure * sure {
                break;
            }
            radius += 1;
        }
        if let Some((j, _)) = right {
            found.insert((i.min(j), i.max(j)));
        }
        if let Some((j, _)) = down {
            found.insert((i.min(j), i.max(j)));
        }
        for &(_d, j) in by_distance.iter().take(nearest) {
            found.insert((i.min(j), i.max(j)));
        }
    }
    let mut out: Vec<(usize, usize)> = found.into_iter().collect();
    out.sort_unstable();
    out
}

/// The words a page's table model is asked about, ONE LIST PER TABLE our own
/// heuristics already found.
///
/// WHY A REGION AT ALL. The model runs under [`super::refine_page`], which
/// refines the blocks the engine produced; it does not go looking for tables
/// in prose. So at run time the pairs it sees are the ones around a table, and
/// if the dump wrote every pair on the page the model would be TRAINED on a
/// population it is never RUN on.
///
/// ONE LIST PER TABLE HERE, JOINED BY THE CALLER. The argument for keeping
/// them apart is that [`candidates`] gives each word six nearest neighbours,
/// and over a merged list those six are competed for by the words of the other
/// tables. It measured WORSE -- 0.5372 against 0.5789 on 150 tables with one
/// model, one set of words and one floor (#600) -- because keeping them apart
/// also cuts every edge between a table and the line above it that belongs to
/// it. So the lists are built per table and the dump joins them; they are
/// returned separately because whether to join them is the caller's to decide
/// and was twice decided wrongly from the armchair.
///
/// GROWN BY TWO LINES, not tight: a table's title, its units row and its
/// footnote sit just outside the cells the heuristics claimed, and whether
/// they belong to the table is exactly what the model is for. Two line heights
/// above and below, and half of one left and right, where a line height is the
/// median height of the words the heuristics did claim.
///
/// Each list is sorted, and a word may appear in two of them where two grown
/// regions overlap. The lists come in table order.
/// How far past the cells the heuristics claimed a table's region reaches, in
/// line heights above and below (and a quarter of that to either side).
///
/// TWO, AND MEASURED AGAINST NONE. The argument for a tight region is a good
/// one -- the four classes cannot say "this word belongs to no table", so a
/// line of prose drawn into the region can only be answered with `neither`,
/// which the decoder still gives a cell of its own. It is wrong: at zero,
/// TEDS is 0.5446 against 0.5975 at two, on the same 4,000 pages with the
/// model retrained for each (#600). A table's units row, its continuation
/// line and its footnote are worth more than the prose costs.
#[doc(hidden)]
pub const GROWTH: f64 = 2.0;

#[doc(hidden)]
pub fn in_table_regions(words: &[PairWord]) -> Vec<Vec<usize>> {
    let mut tables: Vec<u32> = words.iter().filter_map(|w| w.table).collect();
    tables.sort_unstable();
    tables.dedup();
    let mut out: Vec<Vec<usize>> = Vec::new();
    for t in tables {
        let mine: Vec<usize> = words
            .iter()
            .enumerate()
            .filter(|(_, w)| w.table == Some(t))
            .map(|(i, _)| i)
            .collect();
        if mine.is_empty() {
            continue;
        }
        let mut box_ = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        let mut heights: Vec<f64> = Vec::with_capacity(mine.len());
        for &i in &mine {
            let b = words[i].bbox;
            box_ = [
                box_[0].min(b[0]),
                box_[1].min(b[1]),
                box_[2].max(b[2]),
                box_[3].max(b[3]),
            ];
            heights.push(b[3] - b[1]);
        }
        heights.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let line = heights[heights.len() / 2].max(1.0);
        let grown = [
            box_[0] - line * GROWTH / 4.0,
            box_[1] - line * GROWTH,
            box_[2] + line * GROWTH / 4.0,
            box_[3] + line * GROWTH,
        ];
        let mut keep: Vec<usize> = Vec::new();
        for (i, w) in words.iter().enumerate() {
            let (cx, cy) = ((w.bbox[0] + w.bbox[2]) / 2.0, (w.bbox[1] + w.bbox[3]) / 2.0);
            // A word the heuristics already claimed for THIS table is in it
            // whatever its centre says -- a cell that spans the page can have
            // a centre outside a box grown from the others. And a word another
            // table claimed is not, however close it falls.
            let claimed_elsewhere = w.table.is_some_and(|other| other != t);
            if w.table == Some(t)
                || (!claimed_elsewhere
                    && grown[0] <= cx
                    && cx <= grown[2]
                    && grown[1] <= cy
                    && cy <= grown[3])
            {
                keep.push(i);
            }
        }
        out.push(keep);
    }
    out
}
