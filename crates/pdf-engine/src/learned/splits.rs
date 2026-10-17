//! The pieces of a table proposal, proposed beside it (#574).
//!
//! A PROPOSAL THAT HOLDS TWO TABLES IS WRONG ABOUT BOTH. On the DocLayNet
//! tuning half the largest share of the annotated tables no proposal matched
//! were not missed but swallowed: a financial statement page set in two
//! columns, a table in each, and a proposal across both; or two statements
//! one under the other with white between them, and one proposal over the pair
//! (216 of the 329 unmatched financial tables, 28-09). The detectors are right
//! that a table is there. They are wrong about where it ends.
//!
//! So every proposal is also offered in pieces, and the table judge chooses:
//! split at the widest strip of white that runs down through all its lines and
//! at the one nearest its middle, and split at every gap between its rows that is more than
//! twice the proposal's own row gap -- and the column pieces split that way
//! too. A splitter that is generous costs the judge a few more candidates to
//! refuse; one that is not costs a table nobody can choose.
//!
//! Measured offline on the tuning half: proposals matching an annotated table
//! 983 -> 1,177 of 1,818 (0.541 -> 0.647), at about 3.7 pieces per proposal.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/// The narrowest strip of white, in points, that divides a proposal into two
/// columns.
const COLUMN_WHITE: f64 = 8.0;
/// How many times the proposal's median row gap a gap must be to divide it.
const ROW_GAP_FACTOR: f64 = 2.0;
/// The least gap, in points, that divides it at all.
const ROW_GAP_FLOOR: f64 = 6.0;

/// Every piece of every proposal in `proposals` (sets of indices into
/// `lines`, `[x0, y0, x1, y1]` with y up) that is not itself one of the
/// proposals or an earlier piece.
#[doc(hidden)]
pub fn pieces(proposals: &[Vec<usize>], lines: &[[f64; 4]]) -> Vec<Vec<usize>> {
    let mut seen: std::collections::HashSet<Vec<usize>> = proposals
        .iter()
        .map(|p| {
            let mut p = p.clone();
            p.sort_unstable();
            p
        })
        .collect();
    let mut out = Vec::new();
    for proposal in proposals {
        let mut all = Vec::new();
        for column in by_columns(proposal, lines) {
            all.extend(by_rows(&column, lines));
            all.push(column);
        }
        all.extend(by_rows(proposal, lines));
        for mut piece in all {
            piece.sort_unstable();
            if seen.insert(piece.clone()) {
                out.push(piece);
            }
        }
    }
    out
}

/// The two sides of a strip of white that runs down through every line of the
/// proposal, when it is at least `COLUMN_WHITE` wide and each side holds two
/// lines or more -- for two strips: the widest, and the one nearest the
/// proposal's middle. They differ exactly where it matters: on a page set in
/// two columns the widest strip is often inside a table, between its labels
/// and its figures, and the page's gutter is the narrower one in the middle.
fn by_columns(ids: &[usize], lines: &[[f64; 4]]) -> Vec<Vec<usize>> {
    if ids.len() < 4 {
        return Vec::new();
    }
    let mut spans: Vec<(f64, f64)> = ids.iter().map(|&i| (lines[i][0], lines[i][2])).collect();
    spans.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut covered: Vec<(f64, f64)> = Vec::new();
    for (a, b) in spans {
        match covered.last_mut() {
            Some(last) if a <= last.1 => last.1 = last.1.max(b),
            _ => covered.push((a, b)),
        }
    }
    let (Some(first), Some(last)) = (covered.first(), covered.last()) else {
        return Vec::new();
    };
    let centre = (first.0 + last.1) * 0.5;
    let strips: Vec<(f64, f64)> = covered // (width, middle)
        .windows(2)
        .map(|pair| (pair[1].0 - pair[0].1, (pair[0].1 + pair[1].0) * 0.5))
        .filter(|(white, _)| *white >= COLUMN_WHITE)
        .collect();
    let widest = strips
        .iter()
        .copied()
        .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let central = strips.iter().copied().min_by(|a, b| {
        (a.1 - centre)
            .abs()
            .partial_cmp(&(b.1 - centre).abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut middles: Vec<f64> = widest.into_iter().chain(central).map(|s| s.1).collect();
    middles.dedup();
    let mut out = Vec::new();
    for middle in middles {
        let left: Vec<usize> = ids
            .iter()
            .copied()
            .filter(|&i| lines[i][2] <= middle)
            .collect();
        let right: Vec<usize> = ids
            .iter()
            .copied()
            .filter(|&i| lines[i][0] >= middle)
            .collect();
        out.extend([left, right].into_iter().filter(|p| p.len() >= 2));
    }
    out
}

/// The blocks of rows between the gaps that are more than `ROW_GAP_FACTOR`
/// times the proposal's median row gap, when there are two blocks or more.
fn by_rows(ids: &[usize], lines: &[[f64; 4]]) -> Vec<Vec<usize>> {
    if ids.len() < 4 {
        return Vec::new();
    }
    let mut order: Vec<usize> = ids.to_vec();
    order.sort_by(|&a, &b| {
        lines[b][3]
            .partial_cmp(&lines[a][3])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    // Rows: a line joins the row above when it rises more than half its own
    // height into it.
    let mut rows: Vec<(Vec<usize>, f64, f64)> = Vec::new(); // members, low, high
    for i in order {
        let b = lines[i];
        match rows.last_mut() {
            Some((members, low, _)) if b[3] > *low + 0.5 * (b[3] - b[1]) => {
                members.push(i);
                *low = low.min(b[1]);
            }
            _ => rows.push((vec![i], b[1], b[3])),
        }
    }
    if rows.len() < 3 {
        return Vec::new();
    }
    let gaps: Vec<f64> = rows.windows(2).map(|w| w[0].1 - w[1].2).collect();
    let mut sorted = gaps.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    // Python's `statistics.median`: the mean of the middle two when even.
    let n = sorted.len();
    let median = if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) * 0.5
    };
    let limit = (ROW_GAP_FACTOR * median).max(ROW_GAP_FLOOR);
    let mut out: Vec<Vec<usize>> = Vec::new();
    let mut start = 0;
    for k in 0..rows.len() {
        let last = k + 1 == rows.len();
        if last || gaps[k] > limit {
            let piece: Vec<usize> = rows[start..=k]
                .iter()
                .flat_map(|(members, _, _)| members.iter().copied())
                .collect();
            if piece.len() >= 2 {
                out.push(piece);
            }
            start = k + 1;
        }
    }
    if out.len() > 1 {
        out
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(x0: f64, x1: f64, y: f64) -> [f64; 4] {
        [x0, y, x1, y + 9.0]
    }

    /// Two tables side by side, one proposal over both: the two are pieces.
    #[test]
    fn a_proposal_over_two_columns_is_offered_as_each() {
        let mut lines = Vec::new();
        for r in 0..3 {
            let y = 700.0 - 14.0 * f64::from(r);
            lines.push(cell(72.0, 120.0, y));
            lines.push(cell(200.0, 240.0, y));
            // The page's gutter, wider than the gaps inside either table.
            lines.push(cell(340.0, 380.0, y));
            lines.push(cell(420.0, 460.0, y));
        }
        let all: Vec<usize> = (0..12).collect();
        let found = pieces(&[all], &lines);
        assert!(found.contains(&vec![0, 1, 4, 5, 8, 9]), "{found:?}");
        assert!(found.contains(&vec![2, 3, 6, 7, 10, 11]), "{found:?}");
    }

    /// Two statements side by side whose own label-to-figure white is wider
    /// than the page's gutter: the gutter, in the middle, is a cut too.
    #[test]
    fn the_gutter_in_the_middle_is_offered_when_a_wider_strip_is_inside_a_table() {
        let mut lines = Vec::new();
        for r in 0..3 {
            let y = 700.0 - 14.0 * f64::from(r);
            lines.push(cell(72.0, 115.0, y)); // label
            lines.push(cell(185.0, 315.0, y)); // figures, 70 pt of white before
            lines.push(cell(330.0, 420.0, y)); // the other column, 15 pt gutter
            lines.push(cell(480.0, 576.0, y));
        }
        let all: Vec<usize> = (0..12).collect();
        let found = pieces(&[all], &lines);
        assert!(found.contains(&vec![0, 1, 4, 5, 8, 9]), "{found:?}");
        assert!(found.contains(&vec![2, 3, 6, 7, 10, 11]), "{found:?}");
    }

    /// Two tables one under the other with white between: each is a piece.
    #[test]
    fn a_proposal_over_two_stacked_tables_is_offered_as_each() {
        let mut lines = Vec::new();
        for y in [700.0, 686.0, 672.0, 620.0, 606.0, 592.0] {
            lines.push(cell(72.0, 120.0, y));
            lines.push(cell(300.0, 340.0, y));
        }
        let all: Vec<usize> = (0..12).collect();
        let found = pieces(&[all], &lines);
        assert!(found.contains(&(0..6).collect::<Vec<usize>>()), "{found:?}");
        assert!(
            found.contains(&(6..12).collect::<Vec<usize>>()),
            "{found:?}"
        );
    }

    /// One table, evenly set, is offered as nothing but itself.
    #[test]
    fn one_even_table_has_no_pieces_of_rows() {
        let mut lines = Vec::new();
        for r in 0..4 {
            let y = 700.0 - 14.0 * f64::from(r);
            lines.push(cell(72.0, 120.0, y));
            lines.push(cell(122.0, 160.0, y));
        }
        let all: Vec<usize> = (0..8).collect();
        assert!(pieces(&[all], &lines).is_empty());
    }
}
