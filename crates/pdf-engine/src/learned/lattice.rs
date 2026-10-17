// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Tables the page draws as a grid of lines (#574).
//!
//! THE EASIEST TABLE THERE IS, AND THE ONE WE MISSED MOST. On ParseBench's
//! table pages every page we drew no table on at all -- 36 per cent of the gap
//! to pymupdf4llm -- draws its table as a lattice: vertical lines, horizontal
//! lines, or a box per cell. The engine kept horizontal rules only, so no
//! proposer ever saw those grids. Where the page draws the grid, the cells are
//! not a guess: they are where the lines cross, and a missing inner line is a
//! merged cell.
//!
//! IT DECIDES NOTHING A MODEL COULD DECIDE BETTER. A grid of at least two rows
//! and two columns of drawn lines, holding words, is a table; everything here
//! is arithmetic on the lines the page drew.

use crate::text::Ruling;

/// Two lines closer than this are one line; a crossing may miss by this much.
const SNAP: f64 = 1.5;

/// How far apart two stacked grids may sit and still be one table.
const STACK_GAP: f64 = 6.0;

/// A grid the page drew: the x of its vertical lines, left to right, and the y
/// of its horizontal lines, top to bottom, with the segments that draw them.
#[derive(Debug, Clone)]
pub struct Lattice {
    /// Distinct x positions of vertical lines, ascending.
    pub xs: Vec<f64>,
    /// Distinct y positions of horizontal lines, descending (top first).
    pub ys: Vec<f64>,
    horizontal: Vec<Ruling>,
    vertical: Vec<Ruling>,
}

impl Lattice {
    /// The grid's extent, `[left, bottom, right, top]`.
    pub fn bbox(&self) -> [f64; 4] {
        [
            self.xs[0],
            *self.ys.last().expect("two rows of lines"),
            *self.xs.last().expect("two columns of lines"),
            self.ys[0],
        ]
    }

    fn has_vertical(&self, x: f64, y: f64) -> bool {
        self.vertical
            .iter()
            .any(|r| (r.x0 - x).abs() <= SNAP && r.y0 - SNAP <= y && r.y1 + SNAP >= y)
    }

    fn has_horizontal(&self, y: f64, x: f64) -> bool {
        self.horizontal
            .iter()
            .any(|r| (r.y0 - y).abs() <= SNAP && r.x0 - SNAP <= x && r.x1 + SNAP >= x)
    }

    /// The grid's cells as `(rows, columns)` ranges, merged where the line
    /// between two atomic cells is not drawn, top row and left column first.
    pub fn cells(&self) -> Vec<(std::ops::Range<usize>, std::ops::Range<usize>)> {
        let rows = self.ys.len() - 1;
        let columns = self.xs.len() - 1;
        let at = |r: usize, c: usize| r * columns + c;
        let mut parent: Vec<usize> = (0..rows * columns).collect();
        fn find(parent: &mut [usize], i: usize) -> usize {
            let mut i = i;
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }
        for r in 0..rows {
            let middle_y = (self.ys[r] + self.ys[r + 1]) * 0.5;
            for c in 0..columns {
                let middle_x = (self.xs[c] + self.xs[c + 1]) * 0.5;
                if c + 1 < columns && !self.has_vertical(self.xs[c + 1], middle_y) {
                    let (a, b) = (find(&mut parent, at(r, c)), find(&mut parent, at(r, c + 1)));
                    parent[b] = a;
                }
                if r + 1 < rows && !self.has_horizontal(self.ys[r + 1], middle_x) {
                    let (a, b) = (find(&mut parent, at(r, c)), find(&mut parent, at(r + 1, c)));
                    parent[b] = a;
                }
            }
        }
        let mut groups: std::collections::BTreeMap<usize, (usize, usize, usize, usize)> =
            Default::default();
        for r in 0..rows {
            for c in 0..columns {
                let root = find(&mut parent, at(r, c));
                let e = groups.entry(root).or_insert((r, r, c, c));
                e.0 = e.0.min(r);
                e.1 = e.1.max(r);
                e.2 = e.2.min(c);
                e.3 = e.3.max(c);
            }
        }
        let mut out: Vec<(std::ops::Range<usize>, std::ops::Range<usize>)> = groups
            .into_values()
            .map(|(r0, r1, c0, c1)| (r0..r1 + 1, c0..c1 + 1))
            .collect();
        out.sort_by_key(|(r, c)| (r.start, c.start));
        out
    }

    /// Which cell of [`Self::cells`] holds a point, if any.
    pub fn cell_at(
        &self,
        cells: &[(std::ops::Range<usize>, std::ops::Range<usize>)],
        x: f64,
        y: f64,
    ) -> Option<usize> {
        let column = (0..self.xs.len() - 1).find(|&c| x >= self.xs[c] && x <= self.xs[c + 1])?;
        let row = (0..self.ys.len() - 1).find(|&r| y <= self.ys[r] && y >= self.ys[r + 1])?;
        cells
            .iter()
            .position(|(rows, columns)| rows.contains(&row) && columns.contains(&column))
    }
}

/// Collinear segments that touch or overlap, joined into one.
fn merged(mut segments: Vec<Ruling>, horizontal: bool) -> Vec<Ruling> {
    type Of = fn(&Ruling) -> f64;
    let (pos, lo, hi): (Of, Of, Of) = if horizontal {
        (|r| r.y0, |r| r.x0, |r| r.x1)
    } else {
        (|r| r.x0, |r| r.y0, |r| r.y1)
    };
    segments.sort_by(|a, b| {
        pos(a)
            .partial_cmp(&pos(b))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                lo(a)
                    .partial_cmp(&lo(b))
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });
    let mut out: Vec<Ruling> = Vec::new();
    for s in segments {
        if let Some(last) = out.last_mut() {
            if (pos(last) - pos(&s)).abs() <= SNAP && lo(&s) <= hi(last) + SNAP {
                if horizontal {
                    last.x1 = last.x1.max(s.x1);
                } else {
                    last.y1 = last.y1.max(s.y1);
                }
                continue;
            }
        }
        out.push(s);
    }
    out
}

fn distinct(mut values: Vec<f64>) -> Vec<f64> {
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut out: Vec<f64> = Vec::new();
    for v in values {
        if out.last().is_none_or(|last| v - last > SNAP) {
            out.push(v);
        }
    }
    out
}

/// Every grid of at least two rows and two columns of drawn lines on a page.
pub fn lattices(rulings: &[Ruling]) -> Vec<Lattice> {
    let horizontal = merged(
        rulings.iter().copied().filter(|r| r.y0 == r.y1).collect(),
        true,
    );
    let vertical = merged(
        rulings
            .iter()
            .copied()
            .filter(|r| r.x0 == r.x1 && r.y0 != r.y1)
            .collect(),
        false,
    );
    let n = horizontal.len() + vertical.len();
    if horizontal.len() < 3 || vertical.len() < 3 {
        return Vec::new();
    }
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut [usize], i: usize) -> usize {
        let mut i = i;
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for (i, h) in horizontal.iter().enumerate() {
        for (j, v) in vertical.iter().enumerate() {
            let crosses = v.x0 >= h.x0 - SNAP
                && v.x0 <= h.x1 + SNAP
                && h.y0 >= v.y0 - SNAP
                && h.y0 <= v.y1 + SNAP;
            if crosses {
                let (a, b) = (
                    find(&mut parent, i),
                    find(&mut parent, horizontal.len() + j),
                );
                parent[b] = a;
            }
        }
    }
    let mut groups: std::collections::BTreeMap<usize, (Vec<Ruling>, Vec<Ruling>)> =
        Default::default();
    for i in 0..n {
        let root = find(&mut parent, i);
        let e = groups.entry(root).or_default();
        if i < horizontal.len() {
            e.0.push(horizontal[i]);
        } else {
            e.1.push(vertical[i - horizontal.len()]);
        }
    }
    // GRIDS STACKED ON ONE ANOTHER THAT SHARE THEIR COLUMN LINES ARE ONE
    // TABLE: a head drawn in a box of its own over the body's grid.
    let mut groups: Vec<(Vec<Ruling>, Vec<Ruling>)> = groups.into_values().collect();
    let extent = |g: &(Vec<Ruling>, Vec<Ruling>)| {
        let xs: Vec<f64> = g.1.iter().map(|r| r.x0).collect();
        let bottom =
            g.0.iter()
                .map(|r| r.y0)
                .chain(g.1.iter().map(|r| r.y0))
                .fold(f64::INFINITY, f64::min);
        let top =
            g.0.iter()
                .map(|r| r.y0)
                .chain(g.1.iter().map(|r| r.y1))
                .fold(f64::NEG_INFINITY, f64::max);
        (distinct(xs), bottom, top)
    };
    let mut joined = true;
    while joined {
        joined = false;
        'pairs: for a in 0..groups.len() {
            for b in 0..groups.len() {
                if a == b {
                    continue;
                }
                let (xa, bottom_a, _) = extent(&groups[a]);
                let (xb, _, top_b) = extent(&groups[b]);
                let shared = xa
                    .iter()
                    .filter(|x| xb.iter().any(|y| (*x - y).abs() <= SNAP))
                    .count();
                let touching = (bottom_a - top_b).abs() <= STACK_GAP;
                if touching && shared >= 2 && shared * 2 >= xa.len().min(xb.len()) {
                    let other = groups.remove(b);
                    let a = if b < a { a - 1 } else { a };
                    groups[a].0.extend(other.0);
                    groups[a].1.extend(other.1);
                    joined = true;
                    break 'pairs;
                }
            }
        }
    }
    let mut out = Vec::new();
    for (h, v) in groups {
        // AN OPEN SIDE IS STILL A SIDE. A table ruled between its rows and
        // columns but not round its outside has horizontal lines that run past
        // its last vertical line: where they end is its edge, and the column
        // beyond the last vertical line is its first or last column.
        let xs = distinct(
            v.iter()
                .map(|r| r.x0)
                .chain(h.iter().map(|r| r.x0))
                .chain(h.iter().map(|r| r.x1))
                .filter(|&x| {
                    v.iter().any(|r| (r.x0 - x).abs() <= SNAP)
                        || h.iter()
                            .filter(|r| (r.x0 - x).abs() <= SNAP || (r.x1 - x).abs() <= SNAP)
                            .count()
                            >= 2
                })
                .collect(),
        );
        let mut ys = distinct(
            h.iter()
                .map(|r| r.y0)
                .chain(v.iter().map(|r| r.y0))
                .chain(v.iter().map(|r| r.y1))
                .filter(|&y| {
                    h.iter().any(|r| (r.y0 - y).abs() <= SNAP)
                        || v.iter()
                            .filter(|r| (r.y0 - y).abs() <= SNAP || (r.y1 - y).abs() <= SNAP)
                            .count()
                            >= 2
                })
                .collect(),
        );
        ys.reverse();
        if xs.len() >= 3 && ys.len() >= 3 {
            out.push(Lattice {
                xs,
                ys,
                horizontal: h,
                vertical: v,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(y: f64, x0: f64, x1: f64) -> Ruling {
        Ruling {
            x0,
            y0: y,
            x1,
            y1: y,
        }
    }
    fn v(x: f64, y0: f64, y1: f64) -> Ruling {
        Ruling {
            x0: x,
            y0,
            x1: x,
            y1,
        }
    }

    /// Three rows by three columns, the top row one cell across all three.
    #[test]
    fn a_drawn_grid_is_its_cells_and_a_missing_line_is_a_merged_cell() {
        let mut lines = vec![
            h(100.0, 0.0, 90.0),
            h(80.0, 0.0, 90.0),
            h(60.0, 0.0, 90.0),
            h(40.0, 0.0, 90.0),
            v(0.0, 40.0, 100.0),
            v(90.0, 40.0, 100.0),
        ];
        // The inner verticals stop at the head row.
        lines.push(v(30.0, 40.0, 80.0));
        lines.push(v(60.0, 40.0, 80.0));
        let found = lattices(&lines);
        assert_eq!(found.len(), 1);
        let grid = &found[0];
        assert_eq!(grid.xs, vec![0.0, 30.0, 60.0, 90.0]);
        assert_eq!(grid.ys, vec![100.0, 80.0, 60.0, 40.0]);
        let cells = grid.cells();
        assert_eq!(cells[0], (0..1, 0..3), "the head is one cell across");
        assert_eq!(cells.len(), 1 + 2 * 3);
        assert_eq!(grid.cell_at(&cells, 45.0, 70.0), Some(2));
    }

    /// A grid ruled between its columns but not round its outside still has
    /// its outer columns: where the horizontal lines end is its edge.
    #[test]
    fn an_open_side_is_still_a_side() {
        let lines = vec![
            h(100.0, 0.0, 90.0),
            h(80.0, 0.0, 90.0),
            h(60.0, 0.0, 90.0),
            v(30.0, 60.0, 100.0),
            v(60.0, 60.0, 100.0),
            v(45.0, 60.0, 100.0),
        ];
        let found = lattices(&lines);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].xs, vec![0.0, 30.0, 45.0, 60.0, 90.0]);
    }

    /// Four boxes of a page frame and a rule are not a table.
    #[test]
    fn a_frame_round_the_page_is_not_a_grid() {
        let lines = vec![
            h(800.0, 20.0, 590.0),
            h(20.0, 20.0, 590.0),
            v(20.0, 20.0, 800.0),
            v(590.0, 20.0, 800.0),
            h(400.0, 20.0, 590.0),
        ];
        assert!(lattices(&lines).is_empty());
    }
}
