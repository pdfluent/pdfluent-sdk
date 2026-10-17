//! Table regions from the alignment of the page's lines alone: a proposer for
//! the table judge (#574, from #632's `tableregion.detect`).
//!
//! THE IDEA IS NURMINEN'S (tabula-java, MIT) as Camelot's stream mode also
//! reads it: a table is a stack of rows that break into chunks at the same
//! column edges, and a chunk that runs across one of those edges -- a line of
//! prose -- breaks the stack. #632 measured it at 0.50 on the DocLayNet
//! validation proxy with rules only, and on the tuning half it matched 174
//! annotated tables no region of ours matched (#574, 28-09). It is ported here
//! and not re-imagined, so the judge reads the proposer that was measured.
//!
//! ITS UNIT IS THE LINE. It reads the page's lines -- `lines_of`, block by
//! block, the unit the line model and the judge share -- as boxes, and answers
//! with sets of their indices, which is the shape a proposal has on #574.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/// How far apart two chunks' edges may be, in points, and still be one edge.
pub const EDGE_TOLERANCE: f64 = 3.0;
/// The fewest rows of two chunks or more that make a table.
pub const MIN_ROWS: usize = 3;
/// How much white between two rows, in rows' heights, a table may hold.
const ROW_GAP: f64 = 2.5;

/// A page row: the lines that overlap one another vertically by more than half
/// the shorter one's height, left to right.
struct Row {
    members: Vec<usize>,
    low: f64,
    high: f64,
}

fn rows_of(lines: &[[f64; 4]]) -> Vec<Row> {
    let mut order: Vec<usize> = (0..lines.len()).collect();
    // Top first; stable, so lines of one height keep their order.
    order.sort_by(|&a, &b| {
        lines[b][3]
            .partial_cmp(&lines[a][3])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut rows: Vec<Row> = Vec::new();
    for i in order {
        let b = lines[i];
        let place = rows.iter().position(|r| {
            let shorter = (b[3] - b[1]).min(r.high - r.low);
            let shorter = if shorter == 0.0 { 1.0 } else { shorter };
            (b[3].min(r.high) - b[1].max(r.low)) / shorter > 0.5
        });
        match place {
            Some(at) => {
                let r = &mut rows[at];
                r.members.push(i);
                r.low = r.low.min(b[1]);
                r.high = r.high.max(b[3]);
            }
            None => rows.push(Row {
                members: vec![i],
                low: b[1],
                high: b[3],
            }),
        }
    }
    for r in rows.iter_mut() {
        r.members.sort_by(|&a, &b| {
            lines[a][0]
                .partial_cmp(&lines[b][0])
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    rows.sort_by(|a, b| {
        b.high
            .partial_cmp(&a.high)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    rows
}

fn edges(lines: &[[f64; 4]], row: &Row) -> Vec<(f64, f64)> {
    row.members
        .iter()
        .map(|&i| (lines[i][0], lines[i][2]))
        .collect()
}

/// How many of `a`'s chunks share a left, right or centre edge with one of
/// `b`'s.
fn aligned(a: &[(f64, f64)], b: &[(f64, f64)], tol: f64) -> usize {
    a.iter()
        .filter(|(a0, a1)| {
            b.iter().any(|(b0, b1)| {
                (a0 - b0).abs() <= tol
                    || (a1 - b1).abs() <= tol
                    || ((a0 + a1) - (b0 + b1)).abs() / 2.0 <= tol
            })
        })
        .count()
}

/// Whether a chunk of a row runs across one of the table's column gaps.
fn crosses(chunks: &[(f64, f64)], gaps: &[(f64, f64)]) -> bool {
    chunks
        .iter()
        .any(|(x0, x1)| gaps.iter().any(|(g0, g1)| *x0 < g0 - 1.0 && *x1 > g1 + 1.0))
}

/// The rows of a ragged head absorbed above row `r`, oldest first, where the
/// stack over `r` reads `[single, multi, single]`: a fragment line, a head
/// row, and a wrapped line under it, over a row that joins the head row.
/// A wrapped head line reads as one chunk across the head row's column gaps,
/// so the loop below always breaks on it and the head never joins the table.
/// The prose line that must keep breaking tables stands over a row of its
/// own, and a caption over a fresh table has no head row over it, so neither
/// matches the stack. `None` where the rows over `r` are no such head, where
/// any of them already sits in a pushed table, or where either single row is
/// as wide as the joining row or runs past its right edge: a wrapped line is
/// narrower than the row it wraps into, and prose spanning the columns is
/// wider than any of them.
fn head_stack_above(
    rows: &[Row],
    lines: &[[f64; 4]],
    r: usize,
    tables: &[Vec<usize>],
) -> Option<[usize; 3]> {
    if r < 3 {
        return None;
    }
    let (over, head, wrap) = (&rows[r - 3], &rows[r - 2], &rows[r - 1]);
    if over.members.len() != 1 || head.members.len() < 2 || wrap.members.len() != 1 {
        return None;
    }
    if [r - 3, r - 2, r - 1].iter().any(|&row| {
        rows[row]
            .members
            .iter()
            .any(|m| tables.iter().any(|t| t.contains(m)))
    }) {
        return None;
    }
    let height = |row: &Row| {
        if row.high - row.low == 0.0 {
            1.0
        } else {
            row.high - row.low
        }
    };
    let below = &rows[r];
    if head.low - below.high >= ROW_GAP * height(below)
        || over.low - head.high >= ROW_GAP * height(head)
        || wrap.low - below.high >= ROW_GAP * height(below)
    {
        return None;
    }
    let mut prev = edges(lines, head);
    prev.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let gaps: Vec<(f64, f64)> = prev.windows(2).map(|w| (w[0].1, w[1].0)).collect();
    let wrap_edge = edges(lines, wrap);
    if !crosses(&wrap_edge, &gaps) {
        return None;
    }
    if aligned(&prev, &edges(lines, below), EDGE_TOLERANCE) < 2 {
        return None;
    }
    let span = |chunks: &[(f64, f64)]| {
        chunks
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &(x0, x1)| {
                (lo.min(x0), hi.max(x1))
            })
    };
    let (below_lo, below_hi) = span(&edges(lines, below));
    let (wrap_lo, wrap_hi) = (wrap_edge[0].0, wrap_edge[0].1);
    let frag = edges(lines, over);
    let (head_lo, head_hi) = span(&prev);
    let (frag_lo, frag_hi) = (frag[0].0, frag[0].1);
    if wrap_hi - wrap_lo >= below_hi - below_lo || frag_hi - frag_lo >= below_hi - below_lo {
        return None;
    }
    if wrap_hi > below_hi + EDGE_TOLERANCE {
        return None;
    }
    if !(wrap_lo < below_hi && wrap_hi > below_lo && frag_lo < head_hi && frag_hi > head_lo) {
        return None;
    }
    Some([r - 3, r - 2, r - 1])
}

/// The tables alignment finds among `lines` (`[x0, y0, x1, y1]`, y up), each
/// as the sorted indices of its lines.
#[doc(hidden)]
pub fn proposals(lines: &[[f64; 4]]) -> Vec<Vec<usize>> {
    let rows = rows_of(lines);
    let mut tables: Vec<Vec<usize>> = Vec::new();
    let mut current: Vec<usize> = Vec::new(); // indices into `rows`
    let mut prev_multi: Option<usize> = None;

    let close = |current: &mut Vec<usize>, tables: &mut Vec<Vec<usize>>| {
        let multi: Vec<usize> = current
            .iter()
            .copied()
            .filter(|&r| rows[r].members.len() >= 2)
            .collect();
        let widest = multi
            .iter()
            .map(|&r| rows[r].members.len())
            .max()
            .unwrap_or(0);
        if multi.len() >= MIN_ROWS || (multi.len() >= 2 && widest >= 3) {
            while current.last().is_some_and(|&r| rows[r].members.len() < 2) {
                current.pop();
            }
            let mut members: Vec<usize> = current
                .iter()
                .flat_map(|&r| rows[r].members.iter().copied())
                .collect();
            members.sort_unstable();
            members.dedup();
            tables.push(members);
        }
    };

    for (index, row) in rows.iter().enumerate() {
        let chunks = edges(lines, row);
        let gap_to_prev = current
            .last()
            .map_or(0.0, |&last| rows[last].low - row.high);
        let height = if row.high - row.low == 0.0 {
            1.0
        } else {
            row.high - row.low
        };
        if chunks.len() >= 2 {
            let joins = prev_multi.is_some_and(|p| {
                aligned(&edges(lines, &rows[p]), &chunks, EDGE_TOLERANCE) >= 2
                    && gap_to_prev < ROW_GAP * height
            });
            if !joins {
                if !current.is_empty() {
                    close(&mut current, &mut tables);
                }
                current.clear();
                // A ragged head over the row starting the table: the wrapped
                // line broke the stack, so the head row and its fragment never
                // joined. They come back with it where they read as its head.
                if let Some(stack) = head_stack_above(&rows, lines, index, &tables) {
                    current.extend(stack);
                }
            }
            current.push(index);
            prev_multi = Some(index);
        } else {
            // A single chunk is inside a table only where it crosses none of
            // the table's column gaps.
            if let (false, Some(p)) = (current.is_empty(), prev_multi) {
                if gap_to_prev < ROW_GAP * height {
                    let mut previous = edges(lines, &rows[p]);
                    previous.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                    let gaps: Vec<(f64, f64)> =
                        previous.windows(2).map(|w| (w[0].1, w[1].0)).collect();
                    if !crosses(&chunks, &gaps) {
                        current.push(index);
                        continue;
                    }
                }
            }
            if !current.is_empty() {
                close(&mut current, &mut tables);
            }
            current.clear();
            prev_multi = None;
        }
    }
    if !current.is_empty() {
        close(&mut current, &mut tables);
    }
    tables
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(x0: f64, x1: f64, y: f64) -> [f64; 4] {
        [x0, y, x1, y + 9.0]
    }

    #[test]
    fn a_grid_of_three_columns_is_one_table() {
        let mut lines = Vec::new();
        for r in 0..4 {
            let y = 700.0 - 14.0 * f64::from(r);
            lines.push(cell(72.0, 140.0, y));
            lines.push(cell(300.0, 340.0, y));
            lines.push(cell(400.0, 440.0, y));
        }
        assert_eq!(proposals(&lines), vec![(0..12).collect::<Vec<usize>>()]);
    }

    #[test]
    fn a_line_across_the_column_gap_ends_the_table() {
        let mut lines = Vec::new();
        for r in 0..3 {
            let y = 700.0 - 14.0 * f64::from(r);
            lines.push(cell(72.0, 140.0, y));
            lines.push(cell(300.0, 340.0, y));
        }
        // Prose right under it, running from the stub across to the figures.
        lines.push(cell(72.0, 420.0, 658.0));
        lines.push(cell(72.0, 140.0, 644.0));
        lines.push(cell(300.0, 340.0, 644.0));
        let found = proposals(&lines);
        assert_eq!(found, vec![(0..6).collect::<Vec<usize>>()]);
    }

    #[test]
    fn prose_is_no_table() {
        let lines: Vec<[f64; 4]> = (0..6)
            .map(|r| cell(72.0, 520.0, 700.0 - 14.0 * f64::from(r)))
            .collect();
        assert!(proposals(&lines).is_empty());
    }
}
