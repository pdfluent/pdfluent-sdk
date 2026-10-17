// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! A table's grid as one decision per gap, rather than one per pair of words (#574).
//!
//! WHY THE SHAPE CHANGED. Our own words placed in the annotation's grid score
//! TEDS 0.9404 on PubTables where the grid we build scores 0.5996: the whole
//! loss is the grid and none of it is the reading. The pair model of #600 buys
//! 0.0077 of that, because it asks "do these two words belong together" and
//! closes the answer transitively -- one wrong pair walks through the whole
//! closure, and no gate width can help a model that agrees with the builder.
//!
//! A separation is asked once per gap: a median of eleven row gaps in a table
//! against thousands of pairs, and a wrong answer stays one wrong row. Measured
//! on the held-out half of PubTables the grid this builds reads **0.7720**,
//! against 0.5996 shipped and 0.9085 for a rectangular grid of our own words.
//!
//! THE MERGED CELLS ARE A WORD QUESTION, NOT A GRID ONE. Rows and columns
//! make rectangles, and flattening the annotation's merges costs a perfect
//! grid 0.0319. But on our grid the merges were never the loss: a header
//! drawn over two columns overlaps the words of both, the stripes join, and
//! every body cell under it lands in one column. That happens to 47 per cent
//! of the spanning header cells in the training half. So a third model asks,
//! per word, whether it bridges; the stripes are built without the words it
//! names, and each of those takes the columns its extent covers -- which is
//! the merged cell, as a span. Out of fold on the training documents the grid
//! goes from 0.7999 to 0.8287 at the floor chosen there (0.03); on the
//! held-out half from 0.8014 to 0.8228, with the 0.9085 ceiling measured in
//! the same run. Every merge rule tried on the finished grid instead bought
//! 0.007 at most. The header rows, section rows and row groups read off the
//! placed grid (`cells_placed`) take it to **0.8306**, above pymupdf4llm's
//! 0.820.
//!
//! THE FEATURES ARE THE GAP FIRST. Over 1,143 tables the raw gap separates row
//! boundaries at AUC 0.803 pooled and 0.953 WITHIN one table, and columns at
//! 0.911. Three attempts to fold the per-table scale into one ratio all scored
//! WORSE than the raw gap -- dividing by the table's median reads 0.755 on rows
//! -- because 83 per cent of row gaps are boundaries, so the median gap is
//! itself a boundary and dividing by it throws away the size that was doing the
//! work. The table's own statistics are therefore separate inputs and the model
//! combines them; `separation_features.py` is the other half of this and the
//! parity fixture holds the two together.

use super::models;

/// A word as the separation models read it: a box and whether it holds a digit.
#[derive(Debug, Clone)]
pub struct Word {
    /// `[x0, y0, x1, y1]` in PDF user space, y up.
    pub bbox: [f64; 4],
    /// Whether the word contains a digit, which is the only thing about its
    /// text either model reads.
    pub numeric: bool,
}

/// A horizontal ruling on the page, as the row model reads it.
#[derive(Debug, Clone, Copy)]
pub struct Ruling {
    pub y: f64,
}

/// Group boxes that overlap along one axis into bands, in reading order.
///
/// A word joins the band before it when it overlaps ANY word already in it,
/// which is what makes a band a row even where its cells sit at slightly
/// different heights.
#[doc(hidden)]
pub fn bands(order: Vec<usize>, boxes: &[[f64; 4]], low: usize, high: usize) -> Vec<Vec<usize>> {
    let mut out: Vec<Vec<usize>> = Vec::new();
    for i in order {
        let joins = out.last().is_some_and(|band| {
            band.iter()
                .any(|&j| boxes[i][low] <= boxes[j][high] && boxes[i][high] >= boxes[j][low])
        });
        if joins {
            out.last_mut().expect("checked").push(i);
        } else {
            out.push(vec![i]);
        }
    }
    out
}

/// The median as `statistics.median` computes it, which is the median the
/// models were trained against.
///
/// THE MEAN OF THE TWO MIDDLE VALUES when the count is even, and not the
/// upper one. Taking the upper is the ordinary off-by-one of this function and
/// it is not small: on the first fixture it made `gap_over_table_median` read
/// 1.571 where Python reads 2.710, which is a different feature under the
/// same name. The parity test caught it on its first run (#574).
fn middle_of(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 1.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = values.len();
    let middle = if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    };
    if middle == 0.0 {
        1.0
    } else {
        middle
    }
}

/// The table's median gap and its tightest tenth, both never zero.
#[doc(hidden)]
pub fn scale(gaps: &[f64]) -> (f64, f64) {
    let mut positive: Vec<f64> = gaps.iter().copied().filter(|g| *g > 0.0).collect();
    if positive.is_empty() {
        return (1.0, 1.0);
    }
    let middle = middle_of(&mut positive);
    // `ordered[max(0, len // 10 - 1)]`, the same tenth Python takes.
    let at = (positive.len() / 10)
        .saturating_sub(1)
        .min(positive.len() - 1);
    let tight = positive[at];
    (
        if middle == 0.0 { 1.0 } else { middle },
        if tight == 0.0 { 1.0 } else { tight },
    )
}

/// Whether the edges of a band line up, as a share of its words.
fn agree(values: &[f64], slack: f64) -> f64 {
    if values.len() < 2 {
        return 1.0;
    }
    let first = values[0];
    values
        .iter()
        .filter(|v| (*v - first).abs() <= slack)
        .count() as f64
        / values.len() as f64
}

fn numeric_share(band: &[usize], words: &[Word]) -> f64 {
    if band.is_empty() {
        return 0.0;
    }
    band.iter().filter(|&&i| words[i].numeric).count() as f64 / band.len() as f64
}

/// One row gap's features, in `separation_features.ROW_NAMES` order.
#[allow(clippy::too_many_arguments)]
#[doc(hidden)]
pub fn row_features(
    above: &[usize],
    below: &[usize],
    words: &[Word],
    gap: f64,
    gaps: &[f64],
    median_gap: f64,
    tightest: f64,
    text_height: f64,
    // Kept so the shape matches Python's, and deliberately unread: see
    // `separation_features.row_candidates`.
    _rules: &[Ruling],
) -> [f64; 18] {
    let up = numeric_share(above, words);
    let down = numeric_share(below, words);
    let lefts: Vec<f64> = above.iter().map(|&i| words[i].bbox[0]).collect();
    let rights: Vec<f64> = above.iter().map(|&i| words[i].bbox[2]).collect();
    let span = |band: &[usize]| {
        band.iter()
            .map(|&i| words[i].bbox[3])
            .fold(f64::NEG_INFINITY, f64::max)
            - band
                .iter()
                .map(|&i| words[i].bbox[1])
                .fold(f64::INFINITY, f64::min)
    };
    [
        gap,
        gap / text_height.max(1e-6),
        gap / median_gap,
        gap / tightest,
        gaps.iter().filter(|g| **g < gap).count() as f64 / (gaps.len().max(1)) as f64,
        median_gap,
        tightest,
        gaps.len() as f64,
        up,
        down,
        (up - down).abs(),
        above.len() as f64,
        below.len() as f64,
        agree(&lefts, text_height / 2.0),
        agree(&rights, text_height / 2.0),
        text_height,
        span(above),
        span(below),
    ]
}

/// One column gap's features, in `separation_features.COLUMN_NAMES` order.
#[allow(clippy::too_many_arguments)]
#[doc(hidden)]
pub fn column_features(
    left: &[usize],
    right: &[usize],
    words: &[Word],
    gap: f64,
    gaps: &[f64],
    median_gap: f64,
    tightest: f64,
    word_width: f64,
) -> [f64; 17] {
    let a = numeric_share(left, words);
    let b = numeric_share(right, words);
    let tops: Vec<f64> = left.iter().map(|&i| words[i].bbox[3]).collect();
    let span = |band: &[usize]| {
        band.iter()
            .map(|&i| words[i].bbox[2])
            .fold(f64::NEG_INFINITY, f64::max)
            - band
                .iter()
                .map(|&i| words[i].bbox[0])
                .fold(f64::INFINITY, f64::min)
    };
    [
        gap,
        gap / word_width.max(1e-6),
        gap / median_gap,
        gap / tightest,
        gaps.iter().filter(|g| **g < gap).count() as f64 / (gaps.len().max(1)) as f64,
        median_gap,
        tightest,
        gaps.len() as f64,
        a,
        b,
        (a - b).abs(),
        left.len() as f64,
        right.len() as f64,
        agree(&tops, word_width / 2.0),
        span(left),
        span(right),
        word_width,
    ]
}

/// Indices in the order given, sorted stably by left edge and banded across.
fn across(mut order: Vec<usize>, boxes: &[[f64; 4]]) -> Vec<Vec<usize>> {
    order.sort_by(|&a, &b| {
        boxes[a][0]
            .partial_cmp(&boxes[b][0])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    bands(order, boxes, 0, 2)
}

/// One corridor candidate the trace reports: where it ran, what the guards
/// measured, and whether it split.
struct CorridorCandidate {
    from: f64,
    to: f64,
    /// Lines leaving the strip alone, of the stripe's lines.
    free: usize,
    total: usize,
    /// G1's shares, west and east (-1 where G1 is off).
    west: f64,
    east: f64,
    /// G2's spreads in text heights, left edges then right (-1, off).
    left_spread: f64,
    right_spread: f64,
    east_lines: usize,
    /// `split`, `empty`, `veto:g1` or `veto:g2`.
    verdict: &'static str,
}

/// What the guards read while one table's stripes split.
struct GuardCtx<'a> {
    /// Whether the no-split decode holds two columns or fewer (G4).
    narrow: bool,
    /// Per page line: at or below the no-split decode's header rows. The
    /// guards read body lines only; the header block above them never votes.
    body: &'a [bool],
    /// Where candidates go when the trace is on.
    trace: Option<&'a mut Vec<CorridorCandidate>>,
}

/// One top-level stripe's corridor verdict: its pieces, and what the split took.
struct StripeSplit {
    /// The stripe as `across` built it, for G3 to restore.
    original: Vec<usize>,
    /// The stripe's pieces left to right (just `original` when unsplit).
    pieces: Vec<Vec<usize>>,
    /// For each gap between two pieces, whether a corridor made it.
    between: Vec<bool>,
    /// The widest corridor that split it, in text heights (0 unsplit): G3
    /// drops the widest split first.
    width_em: f64,
    /// The words crossing its corridors, which span the columns.
    added: Vec<usize>,
}

/// Each stripe split where a white corridor runs through its lines (#574).
///
/// Returns one verdict per stripe, left to right; [`assemble_splits`] turns
/// the kept ones into stripes. The words crossing a corridor are pushed onto
/// the verdict's `added`: they take the columns their extent covers, as
/// model-flagged bridges do. Gaps between the stripes `across` built are
/// never forced.
fn split_stripes_at_corridors(
    stripes: &[Vec<usize>],
    boxes: &[[f64; 4]],
    lines: &[Vec<usize>],
    text_height: f64,
    config: CorridorConfig,
    guard: &mut GuardCtx,
) -> Vec<StripeSplit> {
    let mut line_of = vec![0usize; boxes.len()];
    for (i, band) in lines.iter().enumerate() {
        for &w in band {
            line_of[w] = i;
        }
    }
    stripes
        .iter()
        .map(|stripe| {
            let mut added = Vec::new();
            let (pieces, between, width_em) = split_stripe_at_corridors(
                stripe,
                boxes,
                &line_of,
                text_height,
                config,
                guard,
                &mut added,
            );
            StripeSplit {
                original: stripe.clone(),
                pieces,
                between,
                width_em,
                added,
            }
        })
        .collect()
}

/// The kept verdicts as stripes, with the forced gaps between them and the
/// words their corridors bridged. A dropped verdict goes back in whole.
fn assemble_splits(
    splits: &[StripeSplit],
    keep: &[bool],
) -> (Vec<Vec<usize>>, Vec<bool>, Vec<usize>) {
    let mut stripes: Vec<Vec<usize>> = Vec::with_capacity(splits.len());
    let mut forced: Vec<bool> = Vec::new();
    let mut added: Vec<usize> = Vec::new();
    for (split, &keep) in splits.iter().zip(keep.iter()) {
        if keep {
            added.extend(split.added.iter().copied());
        }
        let pieces: &[Vec<usize>] = if keep {
            &split.pieces
        } else {
            std::slice::from_ref(&split.original)
        };
        let between: &[bool] = if keep { &split.between } else { &[] };
        for (p, piece) in pieces.iter().enumerate() {
            if !stripes.is_empty() {
                forced.push(p > 0 && between[p - 1]);
            }
            stripes.push(piece.clone());
        }
    }
    (stripes, forced, added)
}

/// Ink on each side of `(from, to)` over the stripe's body lines: the share
/// holding ink west of it, then east. A word crossing the strip holds both
/// sides. No body lines is no evidence: both shares read 0.
fn corridor_occupancy(
    stripe: &[usize],
    boxes: &[[f64; 4]],
    line_of: &[usize],
    body_lines: &[usize],
    from: f64,
    to: f64,
) -> (f64, f64) {
    if body_lines.is_empty() {
        return (0.0, 0.0);
    }
    let mut west = 0usize;
    let mut east = 0usize;
    for &line in body_lines {
        let mut holds_west = false;
        let mut holds_east = false;
        for &w in stripe {
            if line_of[w] != line {
                continue;
            }
            let b = boxes[w];
            if b[0] < to && b[2] > from {
                holds_west = true;
                holds_east = true;
            } else if b[2] <= from {
                holds_west = true;
            } else {
                holds_east = true;
            }
        }
        west += usize::from(holds_west);
        east += usize::from(holds_east);
    }
    (
        west as f64 / body_lines.len() as f64,
        east as f64 / body_lines.len() as f64,
    )
}

/// The spread of the non-crossing words right of `(from, to)`: left edges
/// then right edges, in text heights, and how many body lines hold such a
/// word. A spanning label's edges would pollute both, so crossing words
/// stay out.
fn corridor_alignment(
    stripe: &[usize],
    boxes: &[[f64; 4]],
    line_of: &[usize],
    body_lines: &[usize],
    from: f64,
    to: f64,
    text_height: f64,
) -> (f64, f64, usize) {
    let mut lefts: Vec<f64> = Vec::new();
    let mut rights: Vec<f64> = Vec::new();
    for &line in body_lines {
        let mut left = f64::INFINITY;
        let mut right = f64::NEG_INFINITY;
        for &w in stripe {
            if line_of[w] != line {
                continue;
            }
            let b = boxes[w];
            if b[0] < to && b[2] > from {
                continue;
            }
            if b[2] > from {
                left = left.min(b[0]);
                right = right.max(b[2]);
            }
        }
        if left.is_finite() {
            lefts.push(left);
            rights.push(right);
        }
    }
    if lefts.len() < 2 {
        return (f64::INFINITY, f64::INFINITY, lefts.len());
    }
    let spread = |edges: &[f64]| {
        (edges.iter().fold(f64::NEG_INFINITY, |m, &e| m.max(e))
            - edges.iter().fold(f64::INFINITY, |m, &e| m.min(e)))
            / text_height.max(f64::MIN_POSITIVE)
    };
    (spread(&lefts), spread(&rights), lefts.len())
}

/// One stripe's pieces, left to right, which gaps between them a corridor
/// made, and the widest corridor width in text heights.
///
/// The stripe's lines are the page lines holding its words, in page order;
/// with fewer than `min_lines` of them there is nothing to check. Otherwise
/// the elementary intervals between the words' edges are counted free where
/// at least `share` of the lines leave them alone, adjacent free intervals
/// run together, and the widest run at least `em` text heights wide splits
/// the stripe: the words overlapping it join `added`, the rest fall left
/// and right of it, and each half is checked again. A run that would leave a
/// side empty is not a corridor, nor is one the guards veto -- the widest
/// one that splits stands.
#[allow(clippy::too_many_arguments)]
fn split_stripe_at_corridors(
    stripe: &[usize],
    boxes: &[[f64; 4]],
    line_of: &[usize],
    text_height: f64,
    config: CorridorConfig,
    guard: &mut GuardCtx,
    added: &mut Vec<usize>,
) -> (Vec<Vec<usize>>, Vec<bool>, f64) {
    let alone = |words: &[usize]| (vec![words.to_vec()], vec![], 0.0);
    let mut held: Vec<usize> = stripe.iter().map(|&w| line_of[w]).collect();
    held.sort_unstable();
    held.dedup();
    if held.len() < config.min_lines {
        return alone(stripe);
    }
    let needed = (config.share * held.len() as f64).ceil() as usize;
    let left = stripe
        .iter()
        .map(|&w| boxes[w][0])
        .fold(f64::INFINITY, f64::min);
    let right = stripe
        .iter()
        .map(|&w| boxes[w][2])
        .fold(f64::NEG_INFINITY, f64::max);
    let mut edges = vec![left, right];
    for &w in stripe {
        edges.push(boxes[w][0].max(left));
        edges.push(boxes[w][2].min(right));
    }
    edges.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    edges.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    // The stripe's ink per line, as x ranges: only its own words count, since
    // nothing outside the stripe reaches into its extent.
    let ink: Vec<Vec<(f64, f64)>> = held
        .iter()
        .map(|&line| {
            stripe
                .iter()
                .filter(|&&w| line_of[w] == line)
                .map(|&w| (boxes[w][0], boxes[w][2]))
                .collect()
        })
        .collect();
    // A run's free count: counted again per run below, for the trace.
    let free_of = |from: f64, to: f64| {
        ink.iter()
            .filter(|ranges| !ranges.iter().any(|&(l, r)| l < to && r > from))
            .count()
    };
    let mut runs: Vec<(f64, f64)> = Vec::new();
    let mut open: Option<(f64, f64)> = None;
    for pair in edges.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        if free_of(from, to) >= needed {
            open = Some(match open {
                Some((start, _)) => (start, to),
                None => (from, to),
            });
        } else if let Some(run) = open.take() {
            runs.push(run);
        }
    }
    runs.extend(open);
    runs.retain(|&(from, to)| to - from >= config.em * text_height);
    runs.sort_by(|a, b| {
        (b.1 - b.0)
            .partial_cmp(&(a.1 - a.0))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    // In a narrow table G4 replaces whatever G1/G2 say with the stricter
    // pair: both must pass, even where the base guards are off.
    let (occ, align) = if guard.narrow && config.guard_narrow {
        (
            Some(config.guard_occupancy_strict),
            Some(config.guard_align_em_strict),
        )
    } else {
        (config.guard_occupancy, config.guard_align_em)
    };
    let body_lines: Vec<usize> = held
        .iter()
        .copied()
        .filter(|&line| guard.body.get(line).copied().unwrap_or(true))
        .collect();
    for (from, to) in runs {
        let mut crossing = Vec::new();
        let mut west = Vec::new();
        let mut east = Vec::new();
        for &w in stripe {
            let b = boxes[w];
            if b[0] < to && b[2] > from {
                crossing.push(w);
            } else if b[2] <= from {
                west.push(w);
            } else {
                east.push(w);
            }
        }
        if west.is_empty() || east.is_empty() {
            if let Some(trace) = guard.trace.as_deref_mut() {
                trace.push(CorridorCandidate {
                    from,
                    to,
                    free: free_of(from, to),
                    total: held.len(),
                    west: -1.0,
                    east: -1.0,
                    left_spread: -1.0,
                    right_spread: -1.0,
                    east_lines: 0,
                    verdict: "empty",
                });
            }
            continue;
        }
        let (west_share, east_share) = match occ {
            Some(need) => {
                let (west_share, east_share) =
                    corridor_occupancy(stripe, boxes, line_of, &body_lines, from, to);
                if west_share < need || east_share < need {
                    if let Some(trace) = guard.trace.as_deref_mut() {
                        trace.push(CorridorCandidate {
                            from,
                            to,
                            free: free_of(from, to),
                            total: held.len(),
                            west: west_share,
                            east: east_share,
                            left_spread: -1.0,
                            right_spread: -1.0,
                            east_lines: 0,
                            verdict: "veto:g1",
                        });
                    }
                    continue;
                }
                (west_share, east_share)
            }
            None => (-1.0, -1.0),
        };
        let (left_spread, right_spread, east_lines) = match align {
            Some(limit) => {
                let (left_spread, right_spread, east_lines) =
                    corridor_alignment(stripe, boxes, line_of, &body_lines, from, to, text_height);
                if left_spread > limit && right_spread > limit {
                    if let Some(trace) = guard.trace.as_deref_mut() {
                        trace.push(CorridorCandidate {
                            from,
                            to,
                            free: free_of(from, to),
                            total: held.len(),
                            west: west_share,
                            east: east_share,
                            left_spread,
                            right_spread,
                            east_lines,
                            verdict: "veto:g2",
                        });
                    }
                    continue;
                }
                (left_spread, right_spread, east_lines)
            }
            None => (-1.0, -1.0, 0),
        };
        if let Some(trace) = guard.trace.as_deref_mut() {
            trace.push(CorridorCandidate {
                from,
                to,
                free: free_of(from, to),
                total: held.len(),
                west: west_share,
                east: east_share,
                left_spread,
                right_spread,
                east_lines,
                verdict: "split",
            });
        }
        added.extend(crossing);
        let (mut west_pieces, mut west_between, west_em) =
            split_stripe_at_corridors(&west, boxes, line_of, text_height, config, guard, added);
        let (east_pieces, east_between, east_em) =
            split_stripe_at_corridors(&east, boxes, line_of, text_height, config, guard, added);
        west_pieces.extend(east_pieces);
        west_between.push(true);
        west_between.extend(east_between);
        let width_em = ((to - from) / text_height.max(f64::MIN_POSITIVE)).max(west_em.max(east_em));
        return (west_pieces, west_between, width_em);
    }
    alone(stripe)
}

/// The lines down the page, top first, as the row model reads them.
fn lines_of(boxes: &[[f64; 4]]) -> Vec<Vec<usize>> {
    let mut down: Vec<usize> = (0..boxes.len()).collect();
    down.sort_by(|&a, &b| {
        boxes[b][3]
            .partial_cmp(&boxes[a][3])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    bands(down, boxes, 1, 3)
}

fn overlaps(word: &[f64; 4], band: &[usize], boxes: &[[f64; 4]]) -> bool {
    band.iter()
        .any(|&x| word[0] < boxes[x][2] && word[2] > boxes[x][0])
}

/// One line's view of its stripe: the words below it, the bands those make,
/// the bands every other line makes, and how many lines lie below.
type LineView = (Vec<usize>, Vec<Vec<usize>>, Vec<Vec<usize>>, usize);

/// Every word's bridge features, in `separation_features.BRIDGE_NAMES` order.
///
/// A WORD THAT BRIDGES TWO COLUMNS is asked about before the columns are. A
/// stripe is every word whose extent overlaps another's across the page, so
/// one header drawn over two columns joins them -- and every body cell below
/// it, in every row, lands in one column. The bands the words UNDER a word
/// form within its own stripe are the evidence that the stripe is really two
/// columns held together by it (#574).
#[doc(hidden)]
pub fn bridge_features(words: &[Word], word_width: f64) -> Vec<[f64; 14]> {
    let boxes: Vec<[f64; 4]> = words.iter().map(|w| w.bbox).collect();
    let lines = lines_of(&boxes);
    let mut line_of = vec![0usize; words.len()];
    for (i, band) in lines.iter().enumerate() {
        for &w in band {
            line_of[w] = i;
        }
    }
    let stripes = across((0..words.len()).collect(), &boxes);
    let mut out = vec![[0.0f64; 14]; words.len()];
    for stripe in &stripes {
        let mut mine: Vec<usize> = stripe.iter().map(|&w| line_of[w]).collect();
        mine.sort_unstable();
        mine.dedup();
        let left = stripe
            .iter()
            .map(|&w| boxes[w][0])
            .fold(f64::INFINITY, f64::min);
        let right = stripe
            .iter()
            .map(|&w| boxes[w][2])
            .fold(f64::NEG_INFINITY, f64::max);
        // Per LINE and not per word: every word of one line in one stripe has
        // the same words below it and the same others beside it.
        let mut by_line: std::collections::BTreeMap<usize, LineView> = Default::default();
        for &own in &mine {
            let below: Vec<usize> = stripe
                .iter()
                .copied()
                .filter(|&x| line_of[x] > own)
                .collect();
            let beside: Vec<usize> = stripe
                .iter()
                .copied()
                .filter(|&x| line_of[x] != own)
                .collect();
            let mut under_lines: Vec<usize> = below.iter().map(|&x| line_of[x]).collect();
            under_lines.sort_unstable();
            under_lines.dedup();
            let under = across(below.clone(), &boxes);
            let beside = across(beside, &boxes);
            by_line.insert(own, (below, under, beside, under_lines.len()));
        }
        for &w in stripe {
            let own = line_of[w];
            let (below, under, beside, lines_below) = &by_line[&own];
            let b = boxes[w];
            let mut hit_lines: Vec<usize> = below
                .iter()
                .copied()
                .filter(|&x| b[0] < boxes[x][2] && b[2] > boxes[x][0])
                .map(|x| line_of[x])
                .collect();
            hit_lines.sort_unstable();
            hit_lines.dedup();
            let width = b[2] - b[0];
            out[w] = [
                width / word_width.max(1e-6),
                width / (right - left).max(1e-6),
                mine.len() as f64,
                stripe.len() as f64,
                own as f64 / (lines.len().saturating_sub(1)).max(1) as f64,
                own as f64,
                lines[own].len() as f64,
                if words[w].numeric { 1.0 } else { 0.0 },
                under
                    .iter()
                    .filter(|band| overlaps(&b, band, &boxes))
                    .count() as f64,
                under.len() as f64,
                beside
                    .iter()
                    .filter(|band| overlaps(&b, band, &boxes))
                    .count() as f64,
                *lines_below as f64 / mine.len().max(1) as f64,
                stripes.len() as f64,
                hit_lines.len() as f64 / (*lines_below).max(1) as f64,
            ];
        }
    }
    out
}

/// Which words the bridge model leaves out of the stripes.
pub fn bridging(words: &[Word], floor: f64) -> Vec<bool> {
    if words.len() < 4 {
        return vec![false; words.len()];
    }
    let mut widths: Vec<f64> = words.iter().map(|w| w.bbox[2] - w.bbox[0]).collect();
    let word_width = middle_of(&mut widths);
    bridge_features(words, word_width)
        .iter()
        .map(|x| super::probability(models::table_column_bridge::predict_margin(x)) >= floor)
        .collect()
}

/// Where the separation models put each word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    /// The row of each word.
    pub row_of: Vec<usize>,
    /// The first column of each word.
    pub column_of: Vec<usize>,
    /// The last column each word's extent reaches: its own column for every
    /// word but a bridge, which reaches over the columns it was drawn across.
    pub last_column_of: Vec<usize>,
}

/// Which row and column each word falls in, by the lines the models predict.
///
/// A band's index is how many predicted lines lie before it, so a line called
/// wrongly misplaces the bands on one side of it and nothing else. That
/// locality is the whole reason this shape replaced the pair decoder.
///
/// THE STRIPES ARE BUILT WITHOUT THE WORDS IN `bridge`, which then take the
/// columns their extent covers. With `bridge` all false this is exactly the
/// decoder the boundary models were measured with; `grid_positions` in
/// `train_separations.py` is the other half of it.
///
/// This is the decoder WITHOUT the white-corridor split; the shipped path is
/// [`place_with_corridors`], which splits a stripe the bridge model left
/// joined where a strip of white runs down through its lines. The two agree
/// wherever no stripe holds such a strip, and this one is what the parity
/// test holds against the Python decoder.
pub fn place(words: &[Word], rules: &[Ruling], floor: f64, bridge: &[bool]) -> Option<Placement> {
    place_inner(words, rules, floor, bridge, None)
}

/// [`place`], with a joined stripe split where a white corridor runs through it.
///
/// After `across` builds the stripes, each stripe of at least
/// [`CORRIDOR_MIN_LINES`] lines is checked for a corridor: an x-interval
/// inside its extent free of ink in at least [`CORRIDOR_SHARE`] of its lines
/// and at least [`CORRIDOR_EM`] wide, in median text heights. Where one
/// exists the words crossing it join `bridge` -- they take the columns their
/// extent covers, exactly as model-flagged bridges do -- and the two sides
/// become two stripes, so the column model is asked about the gap. The
/// widest corridor splits first and each half is checked again. With
/// [`CORRIDOR_DECIDES`] a corridor gap is kept as a column boundary whatever
/// the model says; without it the gap is scored like any other.
pub fn place_with_corridors(
    words: &[Word],
    rules: &[Ruling],
    floor: f64,
    bridge: &[bool],
) -> Option<Placement> {
    place_inner(words, rules, floor, bridge, Some(CorridorConfig::SHIPPED))
}

/// [`place_with_corridors`] under an explicit corridor configuration: the
/// unit tests' entry point for the guards, and the measurement grid's under
/// the `corridor-grid` feature. Production serves [`CorridorConfig::SHIPPED`]
/// through the switch.
#[cfg(any(test, feature = "corridor-grid"))]
pub(crate) fn place_with_corridors_in(
    words: &[Word],
    rules: &[Ruling],
    floor: f64,
    bridge: &[bool],
    config: CorridorConfig,
) -> Option<Placement> {
    place_inner(words, rules, floor, bridge, Some(config))
}

/// What counts as a white corridor through a stripe's lines.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CorridorConfig {
    /// The share of the stripe's lines that must leave the strip alone.
    pub(crate) share: f64,
    /// How wide the strip must be, in median text heights.
    pub(crate) em: f64,
    /// The fewest lines a stripe holds before it is checked at all.
    pub(crate) min_lines: usize,
    /// Whether a corridor gap is kept regardless of the model score (V2)
    /// or scored like any other gap (V1).
    pub(crate) decides: bool,
    /// G1 both-sides occupancy: a corridor splits only if, on the stripe's
    /// body lines (its lines at or below the no-split decode's header rows),
    /// both sides hold ink on at least this share of the lines. `None` is
    /// off. Wrapped text, indented stub labels and ragged edges leave one
    /// side mostly empty.
    pub(crate) guard_occupancy: Option<f64>,
    /// G2 edge alignment: the non-crossing words right of the corridor have
    /// aligned left edges (spread at most this, in text heights) or aligned
    /// right edges (figures). `None` is off. A ragged continuation has
    /// neither.
    pub(crate) guard_align_em: Option<f64>,
    /// G3 header guard: the table is decoded with and without the corridor
    /// splits, and splits are dropped, widest first, until the all-`<th>`
    /// header row count is back at the no-split count.
    pub(crate) guard_header: bool,
    /// G4 narrow-table veto: no corridor split in a stripe whose table
    /// already decodes to two columns or fewer, unless G1 and G2 both pass
    /// with the stricter constants below.
    pub(crate) guard_narrow: bool,
    /// The G1 threshold G4 demands in a narrow table.
    pub(crate) guard_occupancy_strict: f64,
    /// The G2 spread G4 demands in a narrow table, in text heights.
    pub(crate) guard_align_em_strict: f64,
}

impl CorridorConfig {
    const SHIPPED: Self = Self {
        share: CORRIDOR_SHARE,
        em: CORRIDOR_EM,
        min_lines: CORRIDOR_MIN_LINES,
        decides: CORRIDOR_DECIDES,
        guard_occupancy: Some(CORRIDOR_GUARD_OCC),
        guard_align_em: None,
        guard_header: false,
        guard_narrow: false,
        guard_occupancy_strict: CORRIDOR_GUARD_OCC2,
        guard_align_em_strict: CORRIDOR_GUARD_ALIGN2,
    };

    /// Whether any guard is switched on: with none on the decode is exactly
    /// the shipped one, without even the no-split trial decode the guards
    /// read.
    fn any_guard(&self) -> bool {
        self.guard_occupancy.is_some()
            || self.guard_align_em.is_some()
            || self.guard_header
            || self.guard_narrow
    }
}

/// The G1 both-sides occupancy a corridor needs to split: the TUNING grid
/// in #574 ships 0.7 alone, the best PT+FTN mean whose lower CI bound vs
/// the shipped split stays at or above zero on both sets.
pub const CORRIDOR_GUARD_OCC: f64 = 0.7;

/// The G1 threshold G4 would demand in a narrow table: not shipped, the
/// grid in #574 leaves G4 off (it vetoes true narrow tables on FinTabNet).
pub const CORRIDOR_GUARD_OCC2: f64 = 0.9;

/// The G2 spread G4 would demand in a narrow table, in text heights: not
/// shipped, with G4.
pub const CORRIDOR_GUARD_ALIGN2: f64 = 0.5;

/// The share of a stripe's lines that must leave a corridor alone before it
/// splits the stripe: the builder's own gutter fraction, provisionally --
/// the grid in #574 picks the constants on the train-side sets.
pub const CORRIDOR_SHARE: f64 = 0.75;

/// How wide a corridor must be, in median text heights, before it splits the
/// stripe: chosen on FinTabNet 614 (train side) from {1.5, 1.0, 0.75} with
/// CORRIDOR_SHARE 0.75, V1: TEDS off 0.6014 / 1.5 0.6014 / 1.0 0.6024 / 0.75
/// 0.6016 (struct 0.6391 / 0.6391 / 0.6398 / 0.6384); confirm sets at 1.0:
/// ParseBench tuning table 0.6372 -> 0.6419, record match 0.4535 -> 0.4559;
/// independent 1,198 tables 0.5438 -> 0.5479; proxy flat on both halves; no
/// set down (T1, 05-10). Right-aligned figures of different widths leave a
/// common white corridor narrower than each line's own gap, which 1.5 em
/// misses (KSS_2009 p63).
pub const CORRIDOR_EM: f64 = 1.0;

/// The fewest lines a stripe holds before it is checked for a corridor at
/// all: two lines whose word gaps line up by chance are not two columns.
/// Provisional -- the grid in #574 picks the constants on the train-side sets.
pub const CORRIDOR_MIN_LINES: usize = 4;

/// Whether a corridor gap is kept as a column boundary regardless of the
/// column model's score (V2) or scored like any other gap (V1, provisionally --
/// the grid in #574 picks the variant on the train-side sets).
pub const CORRIDOR_DECIDES: bool = false;

/// What `PDFLUENT_CORRIDOR` says: the measurement grid's override, under
/// the `corridor-grid` feature only. The default build has no override:
/// the table switch decides, and a stray variable can neither change the
/// output nor abort the host process.
#[cfg(feature = "corridor-grid")]
#[derive(Debug, Clone, Copy)]
enum CorridorEnv {
    /// Unset: the switch decides, and on means [`CorridorConfig::SHIPPED`].
    Default,
    /// `off`: no corridor split, whatever the switch says (the U arm).
    Off,
    /// `k=v,...`: SHIPPED with fields overridden (the grid arms).
    Custom(CorridorConfig),
}

/// The corridor configuration one decode runs under: the environment wins
/// over the switch, so one grid binary serves every arm. Behind the
/// `corridor-grid` feature; without it the table pass serves
/// [`CorridorConfig::SHIPPED`] from its switch and this is not compiled.
#[cfg(feature = "corridor-grid")]
pub(crate) fn resolve_corridor_config(switch_on: bool) -> Option<CorridorConfig> {
    match corridor_env() {
        CorridorEnv::Off => None,
        CorridorEnv::Custom(config) => Some(config),
        CorridorEnv::Default => switch_on.then_some(CorridorConfig::SHIPPED),
    }
}

/// Read once: this is asked on every region decode, and an environment
/// lookup per region is a cost paid by every caller for a knob almost
/// nobody turns.
#[cfg(feature = "corridor-grid")]
fn corridor_env() -> CorridorEnv {
    static ENV: std::sync::OnceLock<CorridorEnv> = std::sync::OnceLock::new();
    *ENV.get_or_init(|| match std::env::var("PDFLUENT_CORRIDOR") {
        Err(_) => CorridorEnv::Default,
        Ok(value) if value.trim() == "off" => CorridorEnv::Off,
        Ok(value) => CorridorEnv::Custom(parse_corridor_env(&value)),
    })
}

/// `share,em,min_lines,decides` plus `g1,g2,g3,g4,occ2,align2`; anything
/// else is a mislabelled measurement, so it fails loud rather than running
/// under a config nobody asked for.
#[cfg(feature = "corridor-grid")]
fn parse_corridor_env(value: &str) -> CorridorConfig {
    let mut config = CorridorConfig::SHIPPED;
    let flag = |raw: &str, key: &str| match raw {
        "0" | "false" => false,
        "1" | "true" => true,
        _ => panic!("PDFLUENT_CORRIDOR: {key} wants 0/1, got {raw:?}"),
    };
    for part in value.split(',') {
        let (key, raw) = part
            .split_once('=')
            .unwrap_or_else(|| panic!("PDFLUENT_CORRIDOR: want k=v pairs, got {part:?}"));
        let (key, raw) = (key.trim(), raw.trim());
        match key {
            "share" => {
                config.share = raw.parse().unwrap_or_else(|_| {
                    panic!("PDFLUENT_CORRIDOR: share wants a number, got {raw:?}")
                })
            }
            "em" => {
                config.em = raw
                    .parse()
                    .unwrap_or_else(|_| panic!("PDFLUENT_CORRIDOR: em wants a number, got {raw:?}"))
            }
            "min_lines" => {
                config.min_lines = raw.parse().unwrap_or_else(|_| {
                    panic!("PDFLUENT_CORRIDOR: min_lines wants a number, got {raw:?}")
                })
            }
            "decides" => config.decides = flag(raw, key),
            "g1" => {
                config.guard_occupancy = Some(raw.parse().unwrap_or_else(|_| {
                    panic!("PDFLUENT_CORRIDOR: g1 wants a number, got {raw:?}")
                }))
            }
            "g2" => {
                config.guard_align_em = Some(raw.parse().unwrap_or_else(|_| {
                    panic!("PDFLUENT_CORRIDOR: g2 wants a number, got {raw:?}")
                }))
            }
            "g3" => config.guard_header = flag(raw, key),
            "g4" => config.guard_narrow = flag(raw, key),
            "occ2" => {
                config.guard_occupancy_strict = raw.parse().unwrap_or_else(|_| {
                    panic!("PDFLUENT_CORRIDOR: occ2 wants a number, got {raw:?}")
                })
            }
            "align2" => {
                config.guard_align_em_strict = raw.parse().unwrap_or_else(|_| {
                    panic!("PDFLUENT_CORRIDOR: align2 wants a number, got {raw:?}")
                })
            }
            _ => panic!("PDFLUENT_CORRIDOR: unknown key {key:?}"),
        }
    }
    config
}

/// Whether each decode says out loud what it split on: stripes, corridors,
/// guard verdicts, bridges and columns, one JSON line per decode on stderr.
/// A grid instrument, like the override: under the `corridor-grid` feature
/// `PDFLUENT_CORRIDOR_TRACE` switches it on, and the default build is quiet.
#[cfg(feature = "corridor-grid")]
fn corridor_tracing() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("PDFLUENT_CORRIDOR_TRACE").is_some())
}

/// The default build never traces: no environment, no global.
#[cfg(not(feature = "corridor-grid"))]
fn corridor_tracing() -> bool {
    false
}

fn place_inner(
    words: &[Word],
    rules: &[Ruling],
    floor: f64,
    bridge: &[bool],
    corridors: Option<CorridorConfig>,
) -> Option<Placement> {
    if words.len() < 4 {
        return None;
    }
    let boxes: Vec<[f64; 4]> = words.iter().map(|w| w.bbox).collect();
    let mut heights: Vec<f64> = boxes.iter().map(|b| b[3] - b[1]).collect();
    let mut widths: Vec<f64> = boxes.iter().map(|b| b[2] - b[0]).collect();
    let text_height = middle_of(&mut heights);
    let word_width = middle_of(&mut widths);

    let lines = lines_of(&boxes);

    let mut core: Vec<usize> = (0..words.len())
        .filter(|&i| !bridge.get(i).copied().unwrap_or(false))
        .collect();
    let mut bridged: Vec<usize> = (0..words.len())
        .filter(|&i| bridge.get(i).copied().unwrap_or(false))
        .collect();
    // A table cannot be all bridge: with fewer than two words left there are
    // no columns to span, and the stripes are built from every word.
    if core.len() < 2 {
        core = (0..words.len()).collect();
        bridged.clear();
    }
    let stripes = across(core, &boxes);

    // The rows first: they read the lines only, never the stripes, so the
    // trial decodes below share them.
    let row_gaps: Vec<f64> = lines
        .windows(2)
        .map(|pair| {
            pair[0]
                .iter()
                .map(|&i| boxes[i][1])
                .fold(f64::INFINITY, f64::min)
                - pair[1]
                    .iter()
                    .map(|&i| boxes[i][3])
                    .fold(f64::NEG_INFINITY, f64::max)
        })
        .collect();
    let (row_median, row_tight) = scale(&row_gaps);

    let mut row_of = vec![0usize; words.len()];
    let mut at = 0usize;
    for (n, band) in lines.iter().enumerate() {
        for &i in band {
            row_of[i] = at;
        }
        if n < row_gaps.len() {
            let x = row_features(
                band,
                &lines[n + 1],
                words,
                row_gaps[n],
                &row_gaps,
                row_median,
                row_tight,
                text_height,
                rules,
            );
            if super::probability(models::table_row_boundary::predict_margin(&x)) >= floor {
                at += 1;
            }
        }
    }

    let tracing = corridor_tracing();
    // A WHITE CORRIDOR SPLITS A STRIPE THE BRIDGE MODEL LEFT JOINED (#574).
    // One word the model does not flag -- a long row label, a title line, a
    // header over two columns -- joins two columns into one stripe before any
    // column model is asked, and no threshold recovers a gap that was never a
    // candidate. Where a strip of white runs down through the stripe's lines
    // the words crossing it join `bridged` and the two sides become two
    // stripes. `forced[n]` is whether the gap between stripes `n` and `n + 1`
    // is such a corridor, kept whatever the model says when the variant
    // decides. With `corridors` off this is exactly `place` as the boundary
    // models were measured with.
    let mut guarded = GuardedSplit::default();
    let (stripes, forced) = match corridors {
        Some(config) => {
            let (stripes, forced, added, split) = split_guarded(
                words,
                &boxes,
                &lines,
                &stripes,
                text_height,
                word_width,
                &row_of,
                floor,
                &bridged,
                config,
                tracing,
            );
            bridged.extend(added);
            guarded = split;
            (stripes, forced)
        }
        None => {
            let gaps = stripes.len().saturating_sub(1);
            (stripes, vec![false; gaps])
        }
    };
    let decides = corridors.is_some_and(|c| c.decides);

    let (column_of, last_column_of, gaps) = decode_columns(
        words,
        &boxes,
        &stripes,
        &forced,
        &bridged,
        word_width,
        floor,
        decides,
        corridors.is_some(),
    );
    if tracing {
        emit_corridor_trace(
            words.len(),
            corridors,
            &guarded,
            &stripes,
            &boxes,
            &column_of,
            &row_of,
            &gaps,
        );
    }
    Some(Placement {
        row_of,
        column_of,
        last_column_of,
    })
}

/// One column gap's read: its width, the boundary model's probability, and
/// whether it became a column boundary.
struct ColumnGap {
    gap: f64,
    p: f64,
    kept: bool,
}

/// Which column each word falls in, over the stripes given.
///
/// ONE COMPUTATION for the final decode and the guards' trial decodes: the
/// column gaps are read off these stripes, each is scored, and the words in
/// `bridged` take the columns their extent covers. This is the column half
/// of `place_inner`, moved whole. Where `neighbor` is set -- the corridor
/// path, never the measured decoder the parity test holds against Python --
/// a word the stripes cover nowhere reads with its line instead of with the
/// nearest stripe.
#[allow(clippy::too_many_arguments)]
fn decode_columns(
    words: &[Word],
    boxes: &[[f64; 4]],
    stripes: &[Vec<usize>],
    forced: &[bool],
    bridged: &[usize],
    word_width: f64,
    floor: f64,
    decides: bool,
    neighbor: bool,
) -> (Vec<usize>, Vec<usize>, Vec<ColumnGap>) {
    let col_gaps: Vec<f64> = stripes
        .windows(2)
        .map(|pair| {
            pair[1]
                .iter()
                .map(|&i| boxes[i][0])
                .fold(f64::INFINITY, f64::min)
                - pair[0]
                    .iter()
                    .map(|&i| boxes[i][2])
                    .fold(f64::NEG_INFINITY, f64::max)
        })
        .collect();
    let (col_median, col_tight) = scale(&col_gaps);
    let mut column_of = vec![0usize; words.len()];
    let mut extent: Vec<(f64, f64, usize)> = Vec::with_capacity(stripes.len());
    let mut gaps = Vec::with_capacity(col_gaps.len());
    let mut at = 0usize;
    for (n, band) in stripes.iter().enumerate() {
        for &i in band {
            column_of[i] = at;
        }
        extent.push((
            band.iter()
                .map(|&i| boxes[i][0])
                .fold(f64::INFINITY, f64::min),
            band.iter()
                .map(|&i| boxes[i][2])
                .fold(f64::NEG_INFINITY, f64::max),
            at,
        ));
        if n < col_gaps.len() {
            let forced_gap = decides && forced[n];
            let x = column_features(
                band,
                &stripes[n + 1],
                words,
                col_gaps[n],
                &col_gaps,
                col_median,
                col_tight,
                word_width,
            );
            let p = super::probability(models::table_column_boundary::predict_margin(&x));
            let kept = forced_gap || p >= floor;
            gaps.push(ColumnGap {
                gap: col_gaps[n],
                p,
                kept,
            });
            if kept {
                at += 1;
            }
        }
    }
    let mut last_column_of = column_of.clone();
    // A bridged word takes the columns its extent covers; one the stripes do
    // not cover at all reads with its line where `neighbor` asks, and with
    // the nearest stripe otherwise. A gutter word sits nearer one stripe's
    // centre by how much ink each column holds past it, which the page's
    // word widths decide far from the word; its nearest placed neighbour on
    // its own text line reads in a column already, and the gutter word reads
    // with it. Words with no placed neighbour on their line keep the nearest
    // stripe, as before.
    let mut leftover: Vec<usize> = Vec::new();
    for &i in bridged {
        let b = boxes[i];
        let hit: Vec<usize> = extent
            .iter()
            .filter(|e| b[0] < e.1 && b[2] > e.0)
            .map(|e| e.2)
            .collect();
        if hit.is_empty() {
            leftover.push(i);
            continue;
        }
        let (first, last) = (
            *hit.iter().min().expect("non-empty"),
            *hit.iter().max().expect("non-empty"),
        );
        column_of[i] = first;
        last_column_of[i] = last;
    }
    for &i in &leftover {
        let b = boxes[i];
        let mut best: Option<(usize, f64)> = None;
        // The corridor path reads a gutter word with its line; the measured
        // decoder keeps Python's nearest stripe.
        if neighbor {
            for (j, ob) in boxes.iter().enumerate() {
                if j == i || leftover.contains(&j) {
                    continue;
                }
                let mut shorter = (b[3] - b[1]).min(ob[3] - ob[1]);
                if shorter == 0.0 {
                    shorter = 1.0;
                }
                if (b[3].min(ob[3]) - b[1].max(ob[1])) / shorter <= 0.5 {
                    continue;
                }
                // Overlapping words read together: touching ink on one line
                // is the nearest a neighbour gets.
                let gap = if ob[2] <= b[0] {
                    b[0] - ob[2]
                } else if ob[0] >= b[2] {
                    ob[0] - b[2]
                } else {
                    0.0
                };
                if best.is_none_or(|(_, g)| gap < g) {
                    best = Some((column_of[j], gap));
                }
            }
        }
        let (first, last) = match best {
            Some((column, _)) => (column, column),
            None => {
                // The nearest stripe by centre; the first of two equally
                // near, as Python's `min` takes it.
                let middle = (b[0] + b[2]) / 2.0;
                let mut best = extent[0];
                for e in &extent[1..] {
                    if ((e.0 + e.1) / 2.0 - middle).abs() < ((best.0 + best.1) / 2.0 - middle).abs()
                    {
                        best = *e;
                    }
                }
                (best.2, best.2)
            }
        };
        column_of[i] = first;
        last_column_of[i] = last;
    }
    (column_of, last_column_of, gaps)
}

/// The corridor arm's verdict, and what the guards read to reach it.
#[derive(Default)]
struct GuardedSplit {
    /// The words the kept corridors bridged.
    added: Vec<usize>,
    /// Columns and all-`<th>` header rows of the no-split decode.
    cols0: usize,
    head0: usize,
    /// The same of the decode with the kept splits.
    cols1: usize,
    head1: usize,
    /// Corridor widths G3 dropped, widest first.
    dropped: Vec<f64>,
    /// Every candidate, in the order the stripes were checked.
    candidates: Vec<CorridorCandidate>,
}

/// The corridor arm: the splits, with the guards' say.
///
/// Without guards this is the shipped split. With any guard on, the stripes
/// are first decoded whole -- G1 and G2 read that decode's header rows for
/// their body lines, G4 its column count -- and with G3 on the splits are
/// dropped, widest first, until the header row count is back at the
/// no-split count.
#[allow(clippy::too_many_arguments)]
fn split_guarded(
    words: &[Word],
    boxes: &[[f64; 4]],
    lines: &[Vec<usize>],
    stripes0: &[Vec<usize>],
    text_height: f64,
    word_width: f64,
    row_of: &[usize],
    floor: f64,
    model_bridged: &[usize],
    config: CorridorConfig,
    tracing: bool,
) -> (Vec<Vec<usize>>, Vec<bool>, Vec<usize>, GuardedSplit) {
    let decides = config.decides;
    if !config.any_guard() {
        let mut candidates = Vec::new();
        let mut guard = GuardCtx {
            narrow: false,
            body: &[],
            trace: tracing.then_some(&mut candidates),
        };
        let splits =
            split_stripes_at_corridors(stripes0, boxes, lines, text_height, config, &mut guard);
        let keep = vec![true; splits.len()];
        let (stripes, forced, added) = assemble_splits(&splits, &keep);
        let mut out = GuardedSplit {
            added: added.clone(),
            candidates,
            ..GuardedSplit::default()
        };
        if tracing {
            let (column_of, last_column_of, _) = decode_columns(
                words,
                boxes,
                &stripes,
                &forced,
                &[model_bridged, added.as_slice()].concat(),
                word_width,
                floor,
                decides,
                true,
            );
            let placed = Placement {
                row_of: row_of.to_vec(),
                column_of,
                last_column_of,
            };
            out.cols1 = placed.column_of.iter().max().map(|m| m + 1).unwrap_or(0);
            out.head1 = cells_placed(words, &placed)
                .map(|g| g.header_rows)
                .unwrap_or(0);
        }
        return (stripes, forced, added, out);
    }
    // The no-split trial decode: the guards' reference.
    let forced0 = vec![false; stripes0.len().saturating_sub(1)];
    let (column0, last0, _) = decode_columns(
        words,
        boxes,
        stripes0,
        &forced0,
        model_bridged,
        word_width,
        floor,
        decides,
        true,
    );
    let placed0 = Placement {
        row_of: row_of.to_vec(),
        column_of: column0,
        last_column_of: last0,
    };
    let cols0 = placed0.column_of.iter().max().map(|m| m + 1).unwrap_or(0);
    let grid0 = cells_placed(words, &placed0);
    // Each line stands in exactly one row -- `place_inner` numbers bands --
    // so a line is body where its row is at or below the header rows. With
    // no grid there is no header block, and every line is body.
    let body: Vec<bool> = match &grid0 {
        Some(grid) => lines
            .iter()
            .map(|band| band.first().is_some_and(|&w| row_of[w] >= grid.header_rows))
            .collect(),
        None => vec![true; lines.len()],
    };
    let head0 = grid0.map(|g| g.header_rows).unwrap_or(0);
    let mut candidates = Vec::new();
    let mut guard = GuardCtx {
        narrow: cols0 <= 2,
        body: &body,
        trace: tracing.then_some(&mut candidates),
    };
    let splits =
        split_stripes_at_corridors(stripes0, boxes, lines, text_height, config, &mut guard);
    let mut keep = vec![true; splits.len()];
    let trial = |keep: &[bool]| {
        let (stripes, forced, added) = assemble_splits(&splits, keep);
        let bridged = [model_bridged, added.as_slice()].concat();
        let (column_of, last_column_of, _) = decode_columns(
            words, boxes, &stripes, &forced, &bridged, word_width, floor, decides, true,
        );
        let placed = Placement {
            row_of: row_of.to_vec(),
            column_of,
            last_column_of,
        };
        let cols = placed.column_of.iter().max().map(|m| m + 1).unwrap_or(0);
        let head = cells_placed(words, &placed)
            .map(|g| g.header_rows)
            .unwrap_or(0);
        (stripes, forced, added, cols, head)
    };
    let mut dropped = Vec::new();
    if config.guard_header {
        let (_, _, _, _, head_all) = trial(&keep);
        if head_all < head0 {
            let mut order: Vec<usize> = splits
                .iter()
                .enumerate()
                .filter(|(_, s)| s.pieces.len() > 1)
                .map(|(i, _)| i)
                .collect();
            order.sort_by(|&a, &b| {
                splits[b]
                    .width_em
                    .partial_cmp(&splits[a].width_em)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            for i in order {
                keep[i] = false;
                dropped.push(splits[i].width_em);
                let (_, _, _, _, head) = trial(&keep);
                if head >= head0 {
                    break;
                }
            }
        }
    }
    let (stripes, forced, added, cols1, head1) = trial(&keep);
    (
        stripes,
        forced,
        added.clone(),
        GuardedSplit {
            added,
            cols0,
            head0,
            cols1,
            head1,
            dropped,
            candidates,
        },
    )
}

/// One decode's corridor trace, as JSON on stderr: stripes, candidates,
/// guard verdicts, bridges and columns. The markdown on stdout is untouched.
#[allow(clippy::too_many_arguments)]
fn emit_corridor_trace(
    n_words: usize,
    corridors: Option<CorridorConfig>,
    guarded: &GuardedSplit,
    stripes: &[Vec<usize>],
    boxes: &[[f64; 4]],
    column_of: &[usize],
    rows_of: &[usize],
    gaps: &[ColumnGap],
) {
    fn num(v: f64) -> String {
        if v.is_finite() {
            format!("{:.3}", (v * 1000.0).round() / 1000.0)
        } else {
            "null".to_string()
        }
    }
    let extent = |band: &[usize]| {
        (
            band.iter()
                .map(|&i| boxes[i][0])
                .fold(f64::INFINITY, f64::min),
            band.iter()
                .map(|&i| boxes[i][2])
                .fold(f64::NEG_INFINITY, f64::max),
        )
    };
    let mut out = String::from("{\"corridor\":{");
    out.push_str(&format!("\"words\":{n_words},"));
    match corridors {
        None => out.push_str("\"mode\":\"off\","),
        Some(c) => {
            let g1 = c.guard_occupancy.map_or("-1".to_string(), num);
            let g2 = c.guard_align_em.map_or("-1".to_string(), num);
            out.push_str(&format!(
                "\"mode\":\"on\",\"share\":{},\"em\":{},\"min_lines\":{},\"decides\":{},\
                 \"g1\":{g1},\"g2\":{g2},\"g3\":{},\"g4\":{},\"occ2\":{},\"align2\":{},",
                num(c.share),
                num(c.em),
                c.min_lines,
                c.decides,
                c.guard_header,
                c.guard_narrow,
                num(c.guard_occupancy_strict),
                num(c.guard_align_em_strict),
            ));
        }
    }
    out.push_str(&format!(
        "\"cols0\":{},\"head0\":{},\"cols1\":{},\"head1\":{},\"added\":{},",
        guarded.cols0,
        guarded.head0,
        guarded.cols1,
        guarded.head1,
        guarded.added.len(),
    ));
    out.push_str("\"dropped\":[");
    for (i, w) in guarded.dropped.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&num(*w));
    }
    out.push_str("],\"candidates\":[");
    for (i, cand) in guarded.candidates.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"from\":{},\"to\":{},\"free\":{},\"total\":{},\"west\":{},\"east\":{},\
             \"ls\":{},\"rs\":{},\"east_lines\":{},\"verdict\":\"{}\"}}",
            num(cand.from),
            num(cand.to),
            cand.free,
            cand.total,
            num(cand.west),
            num(cand.east),
            num(cand.left_spread),
            num(cand.right_spread),
            cand.east_lines,
            cand.verdict,
        ));
    }
    out.push_str("],\"stripes\":[");
    for (i, band) in stripes.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        let (x0, x1) = extent(band);
        out.push_str(&format!(
            "{{\"n\":{},\"x0\":{},\"x1\":{}}}",
            band.len(),
            num(x0),
            num(x1),
        ));
    }
    out.push_str("],\"gaps\":[");
    for (i, gap) in gaps.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"gap\":{},\"p\":{},\"kept\":{}}}",
            num(gap.gap),
            num(gap.p),
            gap.kept,
        ));
    }
    let cols = column_of.iter().max().map(|m| m + 1).unwrap_or(0);
    let rows = rows_of.iter().max().map(|m| m + 1).unwrap_or(0);
    out.push_str(&format!("],\"cols\":{cols},\"rows\":{rows}}}}}"));
    eprintln!("{out}");
}

/// The probability at which a word is held to bridge two columns: the
/// shipped model's own, chosen out of fold on the training documents.
pub const BRIDGE_FLOOR: f64 = models::table_column_bridge::THRESHOLD;

/// More header rows than this and "the rows above the first full row" has
/// found no body at all; the header is then the first row alone.
pub const MOST_HEADER_ROWS: usize = 4;

/// Whether a lone header label spans the stub column as well (#574).
///
/// A header row whose only filled slot stands outside column 0 spans the
/// columns it heads, in one of two variants measured against the current
/// decoder on PubTables-300, FinTabNet, the proxy held-out half and
/// ParseBench before either lands:
/// * `false` (`SPAN_TO_ALL`): columns 1 through the last, the stub slot
///   left empty, as FinTabNet writes it;
/// * `true` (`SPAN_TO_ALL_WITH_STUB`): columns 0 through the last, as
///   PubTables writes it.
///
/// `HEADER_SPAN_TAKES_STUB` in `train_separations.py` is the other half of
/// this switch; the two must agree or the parity test names the table.
pub const HEADER_SPAN_TAKES_STUB: bool = true;

/// The fewest figure columns a lone header label may span (#574).
///
/// With a single figure column the label sits above the one column it names,
/// so spanning would claim a row it does not head; it keeps its bridge reach.
pub const LONE_SPAN_MIN_COLUMNS: usize = 2;

/// How far into the figure columns' extent a lone header label must sit (#574).
///
/// The label's horizontal centre must lie within the middle band of the body
/// words' extent -- inside `[left + BAND * w, right - BAND * w]` with `w` the
/// extent's width -- or it names its own column ("| | | Amount") rather than
/// heading the row, and keeps its bridge reach.
pub const LONE_SPAN_BAND: f64 = 0.25;

/// A decoded table: its cells, and how many of its first rows are header.
#[derive(Debug, Clone)]
pub struct Grid {
    /// Every cell that holds a word, with the rows and columns it spans.
    pub cells: Vec<super::grid::Cell>,
    /// Rows `0..header_rows` are the table's header.
    pub header_rows: usize,
}

/// The cells the predicted lines make, and what the grid says about itself.
///
/// A CELL SPANS THE COLUMNS ITS BRIDGING WORDS WERE DRAWN ACROSS, and stops
/// at the first column its row already fills, so two cells never claim one
/// slot. That is the merged header cell the annotation carries: rows and
/// columns alone make rectangles, and on PubTables flattening the merges costs
/// 0.0319 TEDS of a perfect grid (#574).
pub fn cells_of(words: &[Word], rules: &[Ruling], floor: f64) -> Option<Grid> {
    let bridge = bridging(words, BRIDGE_FLOOR);
    cells_placed(words, &place(words, rules, floor, &bridge)?)
}

/// [`cells_of`] over [`place_with_corridors`]: the shipped decode, which
/// splits a stripe the bridge model left joined where a white corridor runs
/// through its lines (#574).
pub fn cells_of_with_corridors(words: &[Word], rules: &[Ruling], floor: f64) -> Option<Grid> {
    cells_placed(
        words,
        &place_with_corridors(words, rules, floor, &bridging(words, BRIDGE_FLOOR))?,
    )
}

/// [`cells_of_with_corridors`] under an explicit corridor configuration:
/// the measurement grid's decode under the `corridor-grid` feature.
#[cfg(feature = "corridor-grid")]
pub(crate) fn cells_of_with_corridors_in(
    words: &[Word],
    rules: &[Ruling],
    floor: f64,
    config: CorridorConfig,
) -> Option<Grid> {
    cells_placed(
        words,
        &place_with_corridors_in(words, rules, floor, &bridging(words, BRIDGE_FLOOR), config)?,
    )
}

/// The columns a lone header label spans, or nothing when the row is not one.
///
/// `Some((first, last))` when the header row holds exactly one label outside
/// the stub column, the table holds at least [`LONE_SPAN_MIN_COLUMNS`] figure
/// columns, and the label's horizontal centre sits in the middle band of the
/// figure columns' body extent (`extent`, the leftmost word in column 1 and
/// the rightmost in the last column over the body rows); `None` otherwise. A
/// row of two labels keeps the bridge reach of each ("Consolidated | The
/// Company"), a label in the stub column heads no figure column, and a label
/// over its own last column ("| | | Amount") is not a heading.
fn lone_header_span(
    held_here: &[usize],
    columns: usize,
    centre: f64,
    extent: Option<(f64, f64)>,
    takes_stub: bool,
) -> Option<(usize, usize)> {
    if held_here.len() != 1 || held_here[0] == 0 {
        return None;
    }
    if columns.saturating_sub(1) < LONE_SPAN_MIN_COLUMNS {
        return None;
    }
    let (left, right) = extent?;
    let w = right - left;
    if w.is_nan() || w < 0.0 {
        return None;
    }
    if centre < left + LONE_SPAN_BAND * w || centre > right - LONE_SPAN_BAND * w {
        return None;
    }
    Some((if takes_stub { 0 } else { 1 }, columns - 1))
}

/// [`cells_of`] once the words are placed.
///
/// THEN THE THREE READINGS #632 NAMED, each measured out of fold on the
/// training documents before it was kept (3,538 tables, from 0.8287):
///
/// * HEADER ROWS are the rows above the first row that fills every column,
///   when there are at most [`MOST_HEADER_ROWS`] of them (+0.0037);
/// * a LONE HEADER LABEL is a header row whose only filled slot stands
///   outside the stub column, centred over at least
///   [`LONE_SPAN_MIN_COLUMNS`] figure columns, and it spans the columns it
///   heads -- [`HEADER_SPAN_TAKES_STUB`] says whether the stub column is
///   among them (#574);
/// * a SECTION ROW is a body row whose only cell is in the first column, and
///   it spans the table (+0.0040);
/// * a ROW GROUP is a first-column cell with empty first-column slots under
///   it in rows that hold data, and it spans them (+0.0032).
///
/// The fourth, folding a continuation row into the row above, LOST 0.0509
/// as a rule and is not here. `dress` in `train_separations.py` is the other
/// half of this.
#[doc(hidden)]
pub fn cells_placed(words: &[Word], placed: &Placement) -> Option<Grid> {
    use std::collections::{BTreeMap, BTreeSet};
    let mut held: BTreeMap<(usize, usize), Vec<usize>> = Default::default();
    for i in 0..words.len() {
        held.entry((placed.row_of[i], placed.column_of[i]))
            .or_default()
            .push(i);
    }
    if held.len() < 4 {
        return None;
    }
    let columns = held.keys().map(|&(_, c)| c).max().unwrap_or(0) + 1;
    let rows = held.keys().map(|&(r, _)| r).max().unwrap_or(0) + 1;
    let full = |r: usize| (0..columns).all(|c| held.contains_key(&(r, c)));

    let mut header_rows = rows;
    for r in 1..rows {
        if full(r) {
            header_rows = r;
            break;
        }
    }
    if header_rows > MOST_HEADER_ROWS {
        header_rows = 1;
    }

    // The columns each cell spans: a bridge's reach, stopped at a filled slot.
    let mut span: BTreeMap<(usize, usize), (usize, usize)> = BTreeMap::new();
    for (&(row, column), mine) in &held {
        let reach = mine
            .iter()
            .map(|&i| placed.last_column_of[i])
            .max()
            .unwrap_or(column);
        let mut last = column;
        while last < reach && !held.contains_key(&(row, last + 1)) {
            last += 1;
        }
        span.insert((row, column), (column, last));
    }
    let mut taken: BTreeSet<(usize, usize)> = held.keys().copied().collect();
    if columns >= 2 {
        for r in header_rows..rows {
            let alone =
                held.contains_key(&(r, 0)) && (1..columns).all(|c| !held.contains_key(&(r, c)));
            if alone {
                span.insert((r, 0), (0, columns - 1));
                taken.extend((0..columns).map(|k| (r, k)));
            }
        }
    }
    // A LONE HEADER LABEL SPANS THE COLUMNS IT HEADS (#574), when it is
    // centred over them. The bridge only reaches as far as the model names,
    // so one centred label over eight figure columns came out colspan 3
    // against a truth of 9; a header row holding that one label spans the
    // figures instead -- but a label over its own last column ("| | | Amount")
    // names that column and keeps its reach. `dress` in
    // `train_separations.py` is the other half of this.
    //
    // ONE LABEL IS ONE RUN, NOT ONE SLOT. `place()` puts each word of a
    // multi-word label in the column stripe under it, so the row holds
    // several slots; the slots in column order form runs of contiguous
    // columns whose consecutive word gaps are under the same threshold
    // `spaced_header_joined` fuses by. A single slot is a single run.
    let extent = {
        let last = columns.saturating_sub(1);
        let mut left: Option<f64> = None;
        let mut right: Option<f64> = None;
        for (i, word) in words.iter().enumerate() {
            if placed.row_of[i] < header_rows {
                continue;
            }
            if placed.column_of[i] == 1 {
                left = Some(left.map_or(word.bbox[0], |m: f64| m.min(word.bbox[0])));
            }
            if placed.column_of[i] == last {
                right = Some(right.map_or(word.bbox[2], |m: f64| m.max(word.bbox[2])));
            }
        }
        left.zip(right)
    };
    let height = {
        let mut h: Vec<f64> = words.iter().map(|w| w.bbox[3] - w.bbox[1]).collect();
        h.sort_by(f64::total_cmp);
        h.get(h.len() / 2).copied().unwrap_or(1.0)
    };
    let max_gap = super::tables::WORD_SPACE * height;
    for r in 0..header_rows {
        let here: Vec<usize> = (0..columns)
            .filter(|&c| held.contains_key(&(r, c)))
            .collect();
        if here.is_empty() {
            continue;
        }
        let mut runs: Vec<Vec<usize>> = Vec::new();
        for &c in &here {
            let mut starts = true;
            if let Some(prev) = runs.last() {
                let p = *prev.last().expect("a run holds a slot");
                if c == p + 1 {
                    let right_prev = held[&(r, p)]
                        .iter()
                        .map(|&i| words[i].bbox[2])
                        .fold(f64::NEG_INFINITY, f64::max);
                    let left_cur = held[&(r, c)]
                        .iter()
                        .map(|&i| words[i].bbox[0])
                        .fold(f64::INFINITY, f64::min);
                    if left_cur - right_prev < max_gap {
                        starts = false;
                    }
                }
            }
            if starts {
                runs.push(vec![c]);
            } else {
                runs.last_mut().expect("a run holds a slot").push(c);
            }
        }
        if runs.len() != 1 || runs[0][0] == 0 {
            continue;
        }
        let run = &runs[0];
        let mut x0 = f64::INFINITY;
        let mut x1 = f64::NEG_INFINITY;
        for &c in run {
            for &i in &held[&(r, c)] {
                x0 = x0.min(words[i].bbox[0]);
                x1 = x1.max(words[i].bbox[2]);
            }
        }
        if let Some((first, last)) = lone_header_span(
            &run[0..1],
            columns,
            (x0 + x1) / 2.0,
            extent,
            HEADER_SPAN_TAKES_STUB,
        ) {
            let slot = run[0];
            if run.len() > 1 {
                let mut merged = Vec::new();
                for &c in &run[1..] {
                    if let Some(v) = held.remove(&(r, c)) {
                        merged.extend(v);
                    }
                    span.remove(&(r, c));
                }
                if let Some(v) = held.get_mut(&(r, slot)) {
                    v.extend(merged);
                }
            }
            span.insert((r, slot), (first, last));
            taken.extend((first..columns).map(|k| (r, k)));
        }
    }
    let mut down: BTreeMap<(usize, usize), usize> = BTreeMap::new();
    for r in header_rows..rows {
        if !held.contains_key(&(r, 0)) || span[&(r, 0)].1 > 0 {
            continue;
        }
        let mut last = r;
        while last + 1 < rows
            && !taken.contains(&(last + 1, 0))
            && (1..columns).any(|k| held.contains_key(&(last + 1, k)))
        {
            last += 1;
        }
        if last > r {
            taken.extend((r + 1..=last).map(|q| (q, 0)));
            down.insert((r, 0), last);
        }
    }

    let boxes: Vec<[f64; 4]> = words.iter().map(|w| w.bbox).collect();
    let mut out = Vec::with_capacity(held.len());
    for (&(row, column), mine) in &held {
        let (first, last) = span[&(row, column)];
        let bottom = down.get(&(row, column)).copied().unwrap_or(row);
        // READING ORDER INSIDE THE CELL, DOWN THEN ACROSS.
        //
        // Sorting by x alone was wrong for a cell the row model correctly
        // leaves holding two visual lines: the leftmost word of the LOWER line
        // came before the words above it, and `refine_gated` joins this order
        // straight into the public `Block.text`, so a wrapped cell read as
        // interleaved nonsense (codex, #574).
        //
        // Lines first, by the same overlap rule the bands use, then left to
        // right inside a line. A cell of one line is unaffected.
        let order = {
            let mut down: Vec<usize> = mine.clone();
            down.sort_by(|&a, &b| {
                words[b].bbox[3]
                    .partial_cmp(&words[a].bbox[3])
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            bands(down, &boxes, 1, 3)
        };
        let mine: Vec<usize> = order
            .into_iter()
            .flat_map(|mut line| {
                line.sort_by(|&a, &b| {
                    words[a].bbox[0]
                        .partial_cmp(&words[b].bbox[0])
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                line
            })
            .collect();
        let x0 = mine
            .iter()
            .map(|&i| words[i].bbox[0])
            .fold(f64::INFINITY, f64::min);
        let y0 = mine
            .iter()
            .map(|&i| words[i].bbox[1])
            .fold(f64::INFINITY, f64::min);
        let x1 = mine
            .iter()
            .map(|&i| words[i].bbox[2])
            .fold(f64::NEG_INFINITY, f64::max);
        let y1 = mine
            .iter()
            .map(|&i| words[i].bbox[3])
            .fold(f64::NEG_INFINITY, f64::max);
        out.push(super::grid::Cell {
            words: mine,
            bbox: [x0, y0, x1, y1],
            rows: (row..=bottom).collect(),
            columns: (first..=last).collect(),
        });
    }
    Some(Grid {
        cells: out,
        header_rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(x0: f64, y0: f64, x1: f64, y1: f64) -> Word {
        Word {
            bbox: [x0, y0, x1, y1],
            numeric: false,
        }
    }

    /// A bridge spans the columns it was drawn across, into empty slots only.
    #[test]
    fn a_bridge_spans_its_columns_and_stops_at_a_filled_one() {
        // Row 0: a header over columns 0-2 and a second header in column 3.
        // Row 1 and 2: one word per column.
        let mut words = vec![word(0.0, 20.0, 50.0, 25.0), word(60.0, 20.0, 70.0, 25.0)];
        for row in 0..2 {
            let y = 10.0 - 10.0 * row as f64;
            for column in 0..4 {
                let x = 20.0 * column as f64;
                words.push(word(x, y, x + 10.0, y + 5.0));
            }
        }
        let n = words.len();
        let mut row_of = vec![0usize; n];
        let mut column_of = vec![0usize; n];
        for i in 2..n {
            row_of[i] = 1 + (i - 2) / 4;
            column_of[i] = (i - 2) % 4;
        }
        column_of[1] = 3;
        let mut last_column_of = column_of.clone();
        // The header reaches over all four columns; column 3 is taken by the
        // second header, so the cell must stop at column 2.
        last_column_of[0] = 3;
        let cells = cells_placed(
            &words,
            &Placement {
                row_of,
                column_of,
                last_column_of,
            },
        )
        .expect("cells")
        .cells;
        let header = cells
            .iter()
            .find(|c| c.words == vec![0])
            .expect("the header's cell");
        assert_eq!(header.columns, vec![0, 1, 2]);
        let other = cells
            .iter()
            .find(|c| c.words == vec![1])
            .expect("the second header");
        assert_eq!(other.columns, vec![3]);
        assert!(cells
            .iter()
            .filter(|c| c.rows == vec![1] || c.rows == vec![2])
            .all(|c| c.columns.len() == 1));
    }

    fn at(row: usize, column: usize) -> (usize, usize) {
        (row, column)
    }

    /// Header rows, a section row and a row group, read off a placed grid.
    #[test]
    fn the_grid_names_its_header_its_sections_and_its_row_groups() {
        // Rows, by the slots that hold a word:
        //   0: (0,1) (0,2)          -- header, the stub head empty
        //   1: (1,0) (1,1) (1,2)    -- the first full row: data
        //   2: (2,0)                -- alone in the first column: a section
        //   3: (3,0) (3,1) (3,2)
        //   4:       (4,1) (4,2)    -- the stub label above covers this row
        let slots = [
            at(0, 1),
            at(0, 2),
            at(1, 0),
            at(1, 1),
            at(1, 2),
            at(2, 0),
            at(3, 0),
            at(3, 1),
            at(3, 2),
            at(4, 1),
            at(4, 2),
        ];
        let words: Vec<Word> = slots
            .iter()
            .map(|&(r, c)| {
                let (x, y) = (20.0 * c as f64, 100.0 - 10.0 * r as f64);
                word(x, y, x + 10.0, y + 5.0)
            })
            .collect();
        let placed = Placement {
            row_of: slots.iter().map(|s| s.0).collect(),
            column_of: slots.iter().map(|s| s.1).collect(),
            last_column_of: slots.iter().map(|s| s.1).collect(),
        };
        let grid = cells_placed(&words, &placed).expect("grid");
        assert_eq!(grid.header_rows, 1, "row 1 is the first full row");
        let cell = |w: usize| {
            grid.cells
                .iter()
                .find(|c| c.words.contains(&w))
                .expect("a cell")
        };
        assert_eq!(cell(5).columns, vec![0, 1, 2], "row 2 is a section row");
        assert_eq!(cell(6).rows, vec![3, 4], "row 3's stub label covers row 4");
        assert_eq!(cell(2).rows, vec![1], "a section row below stops a group");
    }

    /// One word per slot, placed where it sits, with the bridge reach given
    /// as (word, last column) pairs.
    fn placed_of(slots: &[(usize, usize)], reach: &[(usize, usize)]) -> (Vec<Word>, Placement) {
        let words: Vec<Word> = slots
            .iter()
            .map(|&(r, c)| {
                let (x, y) = (20.0 * c as f64, 100.0 - 10.0 * r as f64);
                word(x, y, x + 10.0, y + 5.0)
            })
            .collect();
        let mut last_column_of: Vec<usize> = slots.iter().map(|s| s.1).collect();
        for &(i, last) in reach {
            last_column_of[i] = last;
        }
        let placed = Placement {
            row_of: slots.iter().map(|s| s.0).collect(),
            column_of: slots.iter().map(|s| s.1).collect(),
            last_column_of,
        };
        (words, placed)
    }

    /// A lone centred header label spans the figure columns it heads (#574).
    #[test]
    fn a_lone_header_label_spans_the_columns_it_heads() {
        // Row 0: one label in column 4 over a stub and eight figure columns.
        // Rows 1 and 2: full, so row 0 is the header. The label's own bridge
        // reach is its own column; the decoder spans it anyway.
        let mut slots = vec![(0, 4)];
        for r in 1..3 {
            for c in 0..9 {
                slots.push((r, c));
            }
        }
        let (words, placed) = placed_of(&slots, &[]);
        let grid = cells_placed(&words, &placed).expect("grid");
        assert_eq!(grid.header_rows, 1);
        let header = grid
            .cells
            .iter()
            .find(|c| c.words == vec![0])
            .expect("the header's cell");
        let first = if HEADER_SPAN_TAKES_STUB { 0 } else { 1 };
        assert_eq!(header.columns, (first..9).collect::<Vec<_>>());
    }

    /// Each span variant reaches the columns its corpus writes (#574).
    #[test]
    fn each_span_variant_reaches_the_columns_its_dataset_writes() {
        // SPAN_TO_ALL leaves the stub slot empty, as FinTabNet writes it;
        // SPAN_TO_ALL_WITH_STUB takes it, as PubTables writes it. The label
        // is centred (85) in the body extent (20..170).
        let centred = Some((20.0, 170.0));
        assert_eq!(
            lone_header_span(&[4], 9, 85.0, centred, false),
            Some((1, 8))
        );
        assert_eq!(lone_header_span(&[4], 9, 85.0, centred, true), Some((0, 8)));
        // Two labels keep the bridge reach of each; a stub label heads no
        // figure column -- even centred.
        assert_eq!(lone_header_span(&[1, 5], 9, 85.0, centred, false), None);
        assert_eq!(lone_header_span(&[1, 5], 9, 85.0, centred, true), None);
        assert_eq!(lone_header_span(&[0], 9, 85.0, centred, false), None);
        assert_eq!(lone_header_span(&[0], 9, 85.0, centred, true), None);
    }

    /// A label over the last column names it; it does not head the row (#574).
    #[test]
    fn a_label_over_the_last_column_keeps_one_column() {
        // Row 0: "| | | Amount", the other header slots empty, over a stub
        // and three figure columns. Rows 1 and 2: full, so row 0 is the
        // header. The label's centre (65) sits past the middle band of the
        // body extent (20..70, band 32.5..57.5): it keeps its own column.
        let mut slots = vec![(0, 3)];
        for r in 1..3 {
            for c in 0..4 {
                slots.push((r, c));
            }
        }
        let (words, placed) = placed_of(&slots, &[]);
        let grid = cells_placed(&words, &placed).expect("grid");
        assert_eq!(grid.header_rows, 1);
        let header = grid
            .cells
            .iter()
            .find(|c| c.words == vec![0])
            .expect("the header's cell");
        assert_eq!(header.columns, vec![3]);
    }

    /// One figure column is named, not headed; two may span (#574).
    #[test]
    fn a_lone_label_over_one_figure_column_does_not_span() {
        // A stub and a single figure column, the label centred (25) in the
        // body extent (20..30, band 22.5..27.5): centred is not enough with
        // one column to head, so it keeps its own column.
        let mut slots = vec![(0, 1)];
        for r in 1..3 {
            for c in 0..2 {
                slots.push((r, c));
            }
        }
        let (words, placed) = placed_of(&slots, &[]);
        let grid = cells_placed(&words, &placed).expect("grid");
        assert_eq!(grid.header_rows, 1);
        let header = grid
            .cells
            .iter()
            .find(|c| c.words == vec![0])
            .expect("the header's cell");
        assert_eq!(header.columns, vec![1]);
        // The boundary: two figure columns, the label's centre (35) in the
        // middle band of the body extent (20..50, band 27.5..42.5), spans.
        let mut slots = vec![(0, 1)];
        for r in 1..3 {
            for c in 0..3 {
                slots.push((r, c));
            }
        }
        let (mut words, placed) = placed_of(&slots, &[]);
        words[0].bbox[0] = 25.0;
        words[0].bbox[2] = 45.0;
        let grid = cells_placed(&words, &placed).expect("grid");
        let header = grid
            .cells
            .iter()
            .find(|c| c.words == vec![0])
            .expect("the header's cell");
        let first = if HEADER_SPAN_TAKES_STUB { 0 } else { 1 };
        assert_eq!(header.columns, (first..3).collect::<Vec<_>>());
    }

    /// The middle band holds its edges and nothing past them (#574).
    #[test]
    fn a_label_centred_at_the_band_edge_spans_and_past_it_does_not() {
        // A stub and four figure columns: the body extent runs 20..90, so the
        // middle band is 37.5..72.5. The label sits in column 2; its box is
        // moved to put its centre on each edge and just past it.
        let columns_of = |x0: f64, x1: f64| {
            let mut slots = vec![(0, 2)];
            for r in 1..3 {
                for c in 0..5 {
                    slots.push((r, c));
                }
            }
            let (mut words, placed) = placed_of(&slots, &[]);
            words[0].bbox[0] = x0;
            words[0].bbox[2] = x1;
            cells_placed(&words, &placed)
                .expect("grid")
                .cells
                .iter()
                .find(|c| c.words == vec![0])
                .expect("the header's cell")
                .columns
                .clone()
        };
        let first = if HEADER_SPAN_TAKES_STUB { 0 } else { 1 };
        let spanned: Vec<usize> = (first..5).collect();
        assert_eq!(columns_of(32.5, 42.5), spanned, "on the low edge: spans");
        assert_eq!(columns_of(32.4, 42.4), vec![2], "past the low edge: stays");
        assert_eq!(columns_of(67.5, 77.5), spanned, "on the high edge: spans");
        assert_eq!(columns_of(67.6, 77.6), vec![2], "past the high edge: stays");
    }

    /// Two header groups on one row keep their own bridge spans (#574).
    #[test]
    fn two_header_labels_keep_their_own_spans() {
        // Row 0: "Consolidated" over columns 1-3 and "The Company" over 5-7.
        // Rows 1 and 2: full, so row 0 is the header. Neither label spans
        // the row: each keeps the reach its words were drawn across.
        let mut slots = vec![(0, 1), (0, 5)];
        for r in 1..3 {
            for c in 0..9 {
                slots.push((r, c));
            }
        }
        let (words, placed) = placed_of(&slots, &[(0, 3), (1, 7)]);
        let grid = cells_placed(&words, &placed).expect("grid");
        assert_eq!(grid.header_rows, 1);
        let cell = |w: usize| {
            grid.cells
                .iter()
                .find(|c| c.words.contains(&w))
                .expect("a cell")
        };
        assert_eq!(cell(0).columns, vec![1, 2, 3]);
        assert_eq!(cell(1).columns, vec![5, 6, 7]);
    }

    /// A header row holding only the stub label is not a span (#574).
    #[test]
    fn a_stub_header_label_stays_put() {
        // Row 0: only (0,0); rows 1 and 2 full. A column-0-only row keeps
        // the section reading in the body and takes no span in the header.
        let mut slots = vec![(0, 0)];
        for r in 1..3 {
            for c in 0..9 {
                slots.push((r, c));
            }
        }
        let (words, placed) = placed_of(&slots, &[]);
        let grid = cells_placed(&words, &placed).expect("grid");
        assert_eq!(grid.header_rows, 1);
        let corner = grid
            .cells
            .iter()
            .find(|c| c.words == vec![0])
            .expect("the stub head's cell");
        assert_eq!(corner.columns, vec![0]);
    }

    /// A three-slot label placed by `place()` spans the row it heads (#574).
    ///
    /// REGRESSION: the lone-span tests above hand-build a one-slot
    /// `Placement`, a shape `place()` never produces for a label of several
    /// words -- production puts each word in the column stripe under it, so
    /// the one-slot guard skipped the row and `spaced_header_joined` fused
    /// the slots into `colspan=3`. This test builds its precondition through
    /// `place()` itself (four words in slots 3, 4 and 5) and fails without
    /// the runs grouping in `cells_placed`.
    #[test]
    fn a_three_slot_label_placed_by_place_spans_the_row_it_heads() {
        // Four words over a stub and eight figure columns; the middle two
        // share a slot, so the label lands in three slots with word-space
        // gaps between them.
        let header = [(60.0, 70.0), (72.0, 82.0), (84.0, 97.0), (99.0, 109.0)];
        let mut words = Vec::new();
        for (x0, x1) in header {
            words.push(word(x0, 40.0, x1, 45.0));
        }
        for r in 0..4 {
            let y = 30.0 - 10.0 * r as f64;
            for c in 0..9 {
                let x = 20.0 * c as f64;
                words.push(Word {
                    bbox: [x, y, x + 10.0, y + 5.0],
                    numeric: c != 0,
                });
            }
        }
        let placed = place(&words, &[], 0.1, &bridging(&words, BRIDGE_FLOOR)).expect("placed");
        // The precondition is production: three held slots, not one.
        {
            use std::collections::BTreeSet;
            let here: BTreeSet<usize> = (0..words.len())
                .filter(|&i| placed.row_of[i] == 0)
                .map(|i| placed.column_of[i])
                .collect();
            assert_eq!(
                here,
                BTreeSet::from([3, 4, 5]),
                "place() scatters the label over its stripes"
            );
        }
        let grid = cells_placed(&words, &placed).expect("grid");
        assert_eq!(grid.header_rows, 1);
        let first = if HEADER_SPAN_TAKES_STUB { 0 } else { 1 };
        let header_cells: Vec<_> = grid.cells.iter().filter(|c| c.rows == vec![0]).collect();
        assert_eq!(
            header_cells.len(),
            1,
            "one run, one cell: {:?}",
            header_cells
                .iter()
                .map(|c| (&c.words, &c.columns))
                .collect::<Vec<_>>()
        );
        assert_eq!(header_cells[0].columns, (first..9).collect::<Vec<_>>());
        assert_eq!(header_cells[0].words, vec![0, 1, 2, 3]);
    }

    /// Six body rows over two columns, joined into one stripe by a long label
    /// in the last row crossing the gutter; the bridge model is not asked.
    /// Words are 6 high, so the 26-wide gutter reads 4.3 em.
    fn joined_by_a_last_row_label() -> Vec<Word> {
        let mut words = Vec::new();
        for row in 0..6 {
            let y = 50.0 - 9.0 * row as f64;
            words.push(word(0.0, y, 12.0, y + 6.0));
            words.push(word(38.0, y, 50.0, y + 6.0));
        }
        words.push(word(5.0, 5.0, 45.0, 11.0));
        words
    }

    /// A white corridor splits the stripe a long label joined, and the label
    /// spans the columns it crosses (#574).
    ///
    /// The label sits in the last body row, where the bridge model does not
    /// look for a header; the gutter is white in every other row. Without the
    /// rule the stripe is one and the columns never become candidates.
    #[test]
    fn a_white_corridor_splits_the_stripe_a_long_label_joined() {
        let words = joined_by_a_last_row_label();
        let label = words.len() - 1;
        // The premise the rule is for: the model leaves the label in.
        assert!(
            !bridging(&words, BRIDGE_FLOOR)[label],
            "the bridge model flags the label, so the corridor is not what splits"
        );
        let joined = place(&words, &[], 0.1, &vec![false; words.len()]).expect("placed");
        assert!(
            joined.column_of.iter().all(|&c| c == 0),
            "without the rule the label joins the two columns"
        );
        let split =
            place_with_corridors(&words, &[], 0.1, &vec![false; words.len()]).expect("placed");
        assert_ne!(split.column_of[0], split.column_of[1]);
        assert_eq!(split.column_of[label], split.column_of[0]);
        assert_eq!(split.last_column_of[label], split.column_of[1]);
    }

    /// A corridor gap the model rejects is kept when the variant decides (V2)
    /// and dropped when it is scored (V1, #574).
    #[test]
    fn a_rejected_corridor_gap_is_kept_only_when_the_variant_decides() {
        // A gutter of 0.5 em over six rows, crossed once: a corridor under a
        // narrow reading, and narrow enough that the column model says no.
        let mut words = Vec::new();
        for row in 0..6 {
            let y = 50.0 - 9.0 * row as f64;
            words.push(word(0.0, y, 12.0, y + 6.0));
            words.push(word(15.0, y, 27.0, y + 6.0));
        }
        words.push(word(5.0, 5.0, 21.0, 11.0));
        let bridge = vec![false; words.len()];
        let narrow = CorridorConfig {
            em: 0.4,
            ..CorridorConfig::SHIPPED
        };
        let scored = place_inner(
            &words,
            &[],
            0.1,
            &bridge,
            Some(CorridorConfig {
                decides: false,
                ..narrow
            }),
        )
        .expect("placed");
        assert!(
            scored.column_of.iter().all(|&c| c == 0),
            "the model accepts a 0.5 em gap, so this no longer tells V1 from V2"
        );
        let kept = place_inner(
            &words,
            &[],
            0.1,
            &bridge,
            Some(CorridorConfig {
                decides: true,
                ..narrow
            }),
        )
        .expect("placed");
        assert_ne!(kept.column_of[0], kept.column_of[1]);
    }

    /// Each half is checked again: two corridors make three columns (#574).
    #[test]
    fn each_half_is_checked_again() {
        // Three columns over six rows; row 4 crosses the first gutter, row 5
        // the second, so each gutter is white in five of six rows.
        let mut words = Vec::new();
        for row in 0..4 {
            let y = 50.0 - 9.0 * row as f64;
            words.push(word(0.0, y, 12.0, y + 6.0));
            words.push(word(38.0, y, 50.0, y + 6.0));
            words.push(word(76.0, y, 88.0, y + 6.0));
        }
        words.push(word(5.0, 14.0, 45.0, 20.0));
        words.push(word(76.0, 14.0, 88.0, 20.0));
        words.push(word(0.0, 5.0, 12.0, 11.0));
        words.push(word(43.0, 5.0, 83.0, 11.0));
        let split =
            place_with_corridors(&words, &[], 0.1, &vec![false; words.len()]).expect("placed");
        assert_eq!(split.column_of[0], 0);
        assert_eq!(split.column_of[1], 1);
        assert_eq!(split.column_of[2], 2);
        assert_eq!(split.last_column_of[12], 1, "row 4 crosses one gutter");
        assert_eq!(split.column_of[14], 0);
        assert_eq!(split.last_column_of[15], 2, "row 5 crosses the other");
    }

    /// Two lines joined into one stripe are not split, whatever their gaps (#574).
    ///
    /// A corridor free in every line is already an inter-stripe gap, so on two
    /// lines the share alone vetoes whatever the count does; this pins the
    /// behaviour both vetoes agree on.
    #[test]
    fn two_lines_with_aligned_gaps_do_not_split() {
        let words = vec![
            word(0.0, 10.0, 12.0, 16.0),
            word(38.0, 10.0, 50.0, 16.0),
            word(5.0, 1.0, 45.0, 7.0),
            word(40.0, 1.0, 52.0, 7.0),
        ];
        let split =
            place_with_corridors(&words, &[], 0.1, &vec![false; words.len()]).expect("placed");
        assert!(
            split.column_of.iter().all(|&c| c == 0),
            "two lines split: {split:?}"
        );
    }

    /// Below the grid's shares the line count vetoes on its own (#574).
    ///
    /// At share 0.5 a gap free in 1 of 2 lines passes the share, so the two
    /// lines stand only because the count vetoes them; with the count at 1
    /// the corridor splits. In the grid the share fires first on short
    /// stripes, and the count is defence in depth.
    #[test]
    fn the_line_count_vetoes_a_short_stripe_on_its_own() {
        let words = vec![
            word(0.0, 10.0, 12.0, 16.0),
            word(38.0, 10.0, 50.0, 16.0),
            word(5.0, 1.0, 45.0, 7.0),
            word(-10.0, 1.0, -2.0, 7.0),
        ];
        let bridge = vec![false; words.len()];
        let held = place_inner(
            &words,
            &[],
            0.1,
            &bridge,
            Some(CorridorConfig {
                share: 0.5,
                decides: true,
                ..CorridorConfig::SHIPPED
            }),
        )
        .expect("placed");
        assert!(
            held.column_of.iter().all(|&c| c == 0),
            "the count did not veto two lines: {held:?}"
        );
        let split = place_inner(
            &words,
            &[],
            0.1,
            &bridge,
            Some(CorridorConfig {
                share: 0.5,
                min_lines: 1,
                decides: true,
                ..CorridorConfig::SHIPPED
            }),
        )
        .expect("placed");
        assert!(
            split.column_of.contains(&1),
            "one line did not release the corridor: {split:?}"
        );
    }

    /// Word gaps that never line up across the lines are no corridor (#574).
    #[test]
    fn a_ragged_column_holds_no_corridor() {
        // Each line holds two words with a 2 em gap, and each line's gap
        // starts 4 points later than the last: the words chain into one
        // stripe, but no 1.0 em strip is free in the five of six lines a
        // corridor needs.
        let mut words = Vec::new();
        for row in 0..6 {
            let y = 50.0 - 9.0 * row as f64;
            let x = 4.0 * row as f64 + 2.0;
            words.push(word(x, y, x + 10.0, y + 6.0));
            words.push(word(x + 22.0, y, x + 32.0, y + 6.0));
        }
        let split =
            place_with_corridors(&words, &[], 0.1, &vec![false; words.len()]).expect("placed");
        assert!(
            split.column_of.iter().all(|&c| c == 0),
            "a ragged column split: {split:?}"
        );
    }

    /// An aligned strip narrower than the width is no corridor (#574).
    #[test]
    fn a_narrow_aligned_strip_is_no_corridor() {
        // A 0.75 em gutter over six rows, crossed once: aligned and
        // long-lived, but under the 1.0 em width.
        let mut words = Vec::new();
        for row in 0..6 {
            let y = 50.0 - 9.0 * row as f64;
            words.push(word(0.0, y, 12.0, y + 6.0));
            words.push(word(16.5, y, 28.5, y + 6.0));
        }
        words.push(word(5.0, 5.0, 25.0, 11.0));
        let split =
            place_with_corridors(&words, &[], 0.1, &vec![false; words.len()]).expect("placed");
        assert!(
            split.column_of.iter().all(|&c| c == 0),
            "a 0.75 em strip split: {split:?}"
        );
    }

    /// A word left out of the stripes no longer joins the columns under it.
    ///
    /// The header's extent overlaps both body columns, so built from every
    /// word the stripes are one; without it they are two, and it takes both.
    #[test]
    fn leaving_a_bridge_out_keeps_the_columns_under_it_apart() {
        let mut words = vec![word(5.0, 60.0, 45.0, 66.0)];
        for row in 0..6 {
            let y = 50.0 - 9.0 * row as f64;
            words.push(word(0.0, y, 12.0, y + 6.0));
            words.push(word(38.0, y, 50.0, y + 6.0));
        }
        let joined = place(&words, &[], 0.1, &vec![false; words.len()]).expect("placed");
        assert!(
            joined.column_of[1..].iter().all(|&c| c == 0),
            "built from every word, the header joins the two columns"
        );
        let mut bridge = vec![false; words.len()];
        bridge[0] = true;
        let apart = place(&words, &[], 0.1, &bridge).expect("placed");
        assert_ne!(apart.column_of[1], apart.column_of[2]);
        assert_eq!(apart.column_of[0], apart.column_of[1]);
        assert_eq!(apart.last_column_of[0], apart.column_of[2]);
    }

    /// The shipped split with every guard off: the control arm the guards
    /// are pinned against, so each test isolates exactly one guard.
    fn unguarded() -> CorridorConfig {
        CorridorConfig {
            guard_occupancy: None,
            guard_align_em: None,
            guard_header: false,
            guard_narrow: false,
            ..CorridorConfig::SHIPPED
        }
    }

    /// Six body rows over two columns, joined by a last-row label, with east
    /// ink on only `east_rows` of the six lines: wrapped prose with a ragged
    /// right edge. The gutter stays a corridor; only G1's shares move.
    fn wrapped_prose(east_rows: usize) -> Vec<Word> {
        let mut words = Vec::new();
        for row in 0..6 {
            let y = 60.0 - 12.0 * row as f64;
            words.push(word(0.0, y, 12.0, y + 6.0));
            if row < east_rows {
                words.push(word(38.0, y, 50.0, y + 6.0));
            }
        }
        words.push(word(5.0, 5.0, 45.0, 11.0));
        words
    }

    /// G1 vetoes a corridor whose east side is mostly empty, and keeps one
    /// both sides fill (#574).
    #[test]
    fn the_occupancy_guard_vetoes_a_half_empty_side() {
        let ragged = wrapped_prose(2);
        let bridge = vec![false; ragged.len()];
        let split =
            place_with_corridors_in(&ragged, &[], 0.1, &bridge, unguarded()).expect("placed");
        assert!(
            split.column_of.contains(&1),
            "the unguarded corridor did not split the wrapped stripe: {split:?}"
        );
        let config = CorridorConfig {
            guard_occupancy: Some(0.8),
            ..unguarded()
        };
        let held = place_with_corridors_in(&ragged, &[], 0.1, &bridge, config).expect("placed");
        assert!(
            held.column_of.iter().all(|&c| c == 0),
            "G1 did not veto the half-empty east side: {held:?}"
        );
        let full = wrapped_prose(6);
        let bridge = vec![false; full.len()];
        let split = place_with_corridors_in(&full, &[], 0.1, &bridge, config).expect("placed");
        assert!(
            split.column_of.contains(&1),
            "G1 vetoed a corridor both sides fill: {split:?}"
        );
    }

    /// Six body rows over two columns, joined by a last-row label, with the
    /// east words' edges spread over `drift` points per row: neither edge
    /// aligns, but the (12, 38) strip stays free in every body line.
    fn ragged_edges(drift: f64) -> Vec<Word> {
        let mut words = Vec::new();
        for row in 0..6 {
            let y = 60.0 - 12.0 * row as f64;
            words.push(word(0.0, y, 12.0, y + 6.0));
            let x = 38.0 + drift * row as f64;
            words.push(word(x, y, x + 10.0, y + 6.0));
        }
        words.push(word(5.0, 5.0, 45.0, 11.0));
        words
    }

    /// G2 vetoes a corridor with ragged edges on both sides, and keeps one
    /// with an aligned edge (#574).
    #[test]
    fn the_alignment_guard_vetoes_ragged_edges() {
        let ragged = ragged_edges(2.0);
        let bridge = vec![false; ragged.len()];
        let split =
            place_with_corridors_in(&ragged, &[], 0.1, &bridge, unguarded()).expect("placed");
        assert!(
            split.column_of.contains(&1),
            "the unguarded corridor did not split the ragged stripe: {split:?}"
        );
        let config = CorridorConfig {
            guard_align_em: Some(1.0),
            ..unguarded()
        };
        let held = place_with_corridors_in(&ragged, &[], 0.1, &bridge, config).expect("placed");
        assert!(
            held.column_of.iter().all(|&c| c == 0),
            "G2 did not veto the ragged edges: {held:?}"
        );
        let aligned = ragged_edges(0.0);
        let bridge = vec![false; aligned.len()];
        let split = place_with_corridors_in(&aligned, &[], 0.1, &bridge, config).expect("placed");
        assert!(
            split.column_of.contains(&1),
            "G2 vetoed an aligned edge: {split:?}"
        );
    }

    /// Eight rows over a chained pair plus a right column that starts at row
    /// 3, with the middle column missing rows 3-4: the no-split decode reads
    /// three header rows, and the corridor's third column breaks the first
    /// full row past the header cap, so the count falls back to one.
    fn header_breaking_split() -> Vec<Word> {
        let mut words = Vec::new();
        for row in 0..8 {
            let y = 100.0 - 12.0 * row as f64;
            words.push(word(0.0, y, 8.0, y + 6.0));
            if !(3..=4).contains(&row) {
                words.push(word(24.0, y, 32.0, y + 6.0));
            }
            if row >= 3 {
                words.push(word(60.0, y, 68.0, y + 6.0));
            }
        }
        words.push(word(4.0, 2.0, 36.0, 8.0));
        words
    }

    /// G3 drops a split that costs header rows, and keeps the no-split
    /// header count (#574).
    #[test]
    fn the_header_guard_drops_a_split_that_costs_header_rows() {
        let words = header_breaking_split();
        let bridge = vec![false; words.len()];
        let plain = place(&words, &[], 0.1, &bridge).expect("placed");
        let grid0 = cells_placed(&words, &plain).expect("grid");
        assert_eq!(grid0.header_rows, 3, "the no-split decode reads {grid0:?}");
        let split =
            place_with_corridors_in(&words, &[], 0.1, &bridge, unguarded()).expect("placed");
        let grid1 = cells_placed(&words, &split).expect("grid");
        assert_eq!(
            grid1.header_rows, 1,
            "the unguarded split did not cost header rows: {grid1:?}"
        );
        let config = CorridorConfig {
            guard_header: true,
            ..unguarded()
        };
        let held = place_with_corridors_in(&words, &[], 0.1, &bridge, config).expect("placed");
        let grid = cells_placed(&words, &held).expect("grid");
        assert_eq!(
            grid.header_rows, 3,
            "G3 did not keep the header rows: {grid:?}"
        );
        assert!(
            held.column_of.iter().all(|&c| c <= 1),
            "G3 kept the splitting column: {held:?}"
        );
    }

    /// G4 vetoes a corridor in a two-column table with ragged figure edges,
    /// and still splits one with aligned edges (#574).
    #[test]
    fn the_narrow_veto_holds_a_ragged_two_column_table() {
        let ragged = ragged_edges(2.0);
        let bridge = vec![false; ragged.len()];
        let plain = place(&ragged, &[], 0.1, &bridge).expect("placed");
        assert!(
            plain.column_of.iter().all(|&c| c == 0),
            "the no-split decode is not narrow: {plain:?}"
        );
        let config = CorridorConfig {
            guard_narrow: true,
            ..unguarded()
        };
        let held = place_with_corridors_in(&ragged, &[], 0.1, &bridge, config).expect("placed");
        assert!(
            held.column_of.iter().all(|&c| c == 0),
            "G4 did not veto the ragged narrow table: {held:?}"
        );
        let aligned = ragged_edges(0.0);
        let bridge = vec![false; aligned.len()];
        let split = place_with_corridors_in(&aligned, &[], 0.1, &bridge, config).expect("placed");
        assert!(
            split.column_of.contains(&1),
            "G4 vetoed an aligned narrow table: {split:?}"
        );
    }
}
