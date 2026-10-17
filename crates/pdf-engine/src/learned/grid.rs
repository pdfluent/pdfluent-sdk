//! Pairwise decisions about words into a grid with spans (#600).
//!
//! The model of #600 answers four questions about a PAIR of words, and none of
//! those is a table. This is the deterministic half that turns them into one:
//! cells, their row and column numbers, and the spans that make a grid rather
//! than a list.
//!
//! IT DECIDES NOTHING. Every choice here is arithmetic on the relations it is
//! handed, so a grid that comes out wrong is either a relation that came in
//! wrong or a rule written down below. A decoder that guessed would hide the
//! model's mistakes behind its own and neither could then be measured.
//!
//! THE ONE THING THAT IS NOT A PARTITION. "Same cell" is an equivalence: two
//! words are in one cell or they are not, and the classes are the cells. "Same
//! row" is not. A cell spanning rows 0 and 1 shares a row with a cell in row 0
//! and with one in row 1, while those two share none with each other -- so
//! grouping by connected components would swallow the whole table into one
//! row. Rows are built as cliques first and membership assigned second, which
//! are two different questions: what the lines ARE, and who is ON them.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/// A cell the decoder built: the words it holds, its box, and where it sits.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub struct Cell {
    /// Indices into the words the decoder was given, in reading order.
    pub words: Vec<usize>,
    /// `[x0, y0, x1, y1]` in PDF user space, y up. Empty for an inferred cell.
    pub bbox: [f64; 4],
    /// The rows this cell occupies, ascending, 0 at the top.
    pub rows: Vec<usize>,
    /// The columns this cell occupies, ascending, 0 at the left.
    pub columns: Vec<usize>,
}

/// Connected components as a label per item.
fn components(count: usize, edges: &[(usize, usize)]) -> Vec<usize> {
    let mut parent: Vec<usize> = (0..count).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for &(a, b) in edges {
        let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
        if ra != rb {
            parent[ra] = rb;
        }
    }
    let mut seen: Vec<Option<usize>> = vec![None; count];
    let mut next = 0;
    let mut out = vec![0; count];
    for (i, slot) in out.iter_mut().enumerate() {
        let r = root(&mut parent, i);
        *slot = match seen[r] {
            Some(v) => v,
            None => {
                seen[r] = Some(next);
                next += 1;
                next - 1
            }
        };
    }
    out
}

/// Index sets along one axis, in two passes.
///
/// FIRST the lines are found, and a cell joins one only if it shares that axis
/// with EVERY cell already in it. Sharing with merely one would let a spanning
/// cell act as a bridge and pull the whole table into a single row.
///
/// THEN every cell is assigned to every line it shares the axis with
/// throughout, which is what puts a spanning cell in each of the lines it
/// spans. A line wholly contained in another is dropped: it can only come from
/// an ordering in which a spanning cell opened a line of its own, and it names
/// no row the containing line does not.
fn sweep(order: &[usize], shares: &dyn Fn(usize, usize) -> bool) -> Vec<Vec<usize>> {
    let mut lines: Vec<Vec<usize>> = Vec::new();
    for &index in order {
        let mut joined = false;
        for line in lines.iter_mut() {
            if line.iter().all(|&other| shares(index, other)) {
                line.push(index);
                joined = true;
                break;
            }
        }
        if !joined {
            lines.push(vec![index]);
        }
    }
    let filled: Vec<Vec<usize>> = lines
        .iter()
        .map(|line| {
            order
                .iter()
                .copied()
                .filter(|&index| line.iter().all(|&other| shares(index, other)))
                .collect()
        })
        .collect();
    let mut kept: Vec<Vec<usize>> = Vec::new();
    for line in &filled {
        let here: std::collections::BTreeSet<usize> = line.iter().copied().collect();
        let inside_another = filled.iter().any(|other| {
            let there: std::collections::BTreeSet<usize> = other.iter().copied().collect();
            here.len() < there.len() && here.is_subset(&there)
        });
        if inside_another {
            continue;
        }
        if kept.iter().any(|other| {
            other
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                == here
        }) {
            continue;
        }
        kept.push(line.clone());
    }
    kept
}

/// Row or column numbers when each cell also says how far it reaches.
///
/// The sweep says which cells share a line and in what order; the count says
/// how many rows a cell occupies. Neither alone recovers a band two rows tall
/// that nothing divides -- the sweep cannot see the division and the count
/// cannot say where it falls -- and together they do:
///
/// * a line's HEIGHT is the largest count among the cells that sit on that
///   line alone, a cell reaching further being the only evidence the line
///   holds more than one row;
/// * a line STARTS where the lines above it end;
/// * a cell occupies its count of rows from the first line it sits on.
///
/// A cell on several lines sets no height: it reaches across lines other cells
/// already measured, and letting it set one would count its reach twice.
fn numbered(
    cells: usize,
    lines: &[Vec<usize>],
    span_of: &dyn Fn(usize) -> usize,
) -> Vec<Vec<usize>> {
    let mut on: Vec<std::collections::BTreeSet<usize>> = vec![Default::default(); cells];
    for (number, line) in lines.iter().enumerate() {
        for &index in line {
            on[index].insert(number);
        }
    }
    let mut height = vec![1usize; lines.len()];
    for (index, where_it_is) in on.iter().enumerate() {
        if where_it_is.len() == 1 {
            let only = *where_it_is.iter().next().expect("one");
            height[only] = height[only].max(span_of(index).max(1));
        }
    }
    let mut start = Vec::with_capacity(lines.len());
    let mut running = 0usize;
    for &tall in &height {
        start.push(running);
        running += tall;
    }
    (0..cells)
        .map(|index| {
            let first = on[index].iter().next().map(|&n| start[n]).unwrap_or(0);
            let reach = span_of(index).max(1);
            (first..first + reach).collect()
        })
        .collect()
}

/// The grid from the neighbour pairs a model scored, and nothing else.
///
/// `scored` gives, per candidate pair, the four class probabilities in the
/// order of [`super::pairs::CLASSES`]. Only NEIGHBOURS are scored, so the
/// relations for distant pairs are answered nowhere -- and that is what this
/// function is for.
///
/// IT CLOSES THE RELATIONS TRANSITIVELY BEFORE DECODING. Handed the scored
/// pairs directly, the sweep would ask whether two cells at opposite ends of a
/// row share one, find no answer, and read the silence as "no", so every table
/// would come apart for reasons that have nothing to do with the model. Here
/// the neighbour edges become components first, and the sweep is then handed
/// relations that hold of a pair because they hold of the chain between them.
///
/// A CELL THAT SPANS ROWS DOES NOT BRIDGE THEM: a word in a cell reaching over
/// two rows is joined to words in both, and a component would swallow the
/// pair. Such words are left out of the row graph and their cells take their
/// rows from the span count instead.
#[doc(hidden)]
pub fn from_edges(
    words: usize,
    boxes: &[[f64; 4]],
    scored: &[((usize, usize), [f64; 4])],
    row_span: &dyn Fn(usize) -> usize,
    column_span: &dyn Fn(usize) -> usize,
    floor: f64,
) -> Vec<Cell> {
    let cell_edges: Vec<(usize, usize)> = scored
        .iter()
        .filter(|(_, p)| p[3] > floor)
        .map(|&(pair, _)| pair)
        .collect();
    let cell_of = components(words, &cell_edges);

    let bridging = |index: usize, span: &dyn Fn(usize) -> usize| span(index) > 1;
    let row_edges: Vec<(usize, usize)> = scored
        .iter()
        .filter(|(pair, p)| {
            p[3] + p[2] > floor && !bridging(pair.0, row_span) && !bridging(pair.1, row_span)
        })
        .map(|&(pair, _)| pair)
        .collect();
    let column_edges: Vec<(usize, usize)> = scored
        .iter()
        .filter(|(pair, p)| {
            p[3] + p[1] > floor && !bridging(pair.0, column_span) && !bridging(pair.1, column_span)
        })
        .map(|&(pair, _)| pair)
        .collect();
    let row_of = components(words, &row_edges);
    let column_of = components(words, &column_edges);

    decode(
        words,
        boxes,
        &|i, j| cell_of[i] == cell_of[j],
        &|i, j| cell_of[i] == cell_of[j] || row_of[i] == row_of[j],
        &|i, j| cell_of[i] == cell_of[j] || column_of[i] == column_of[j],
        row_span,
        column_span,
    )
}

/// The grid, from relations that hold of every pair.
#[doc(hidden)]
#[allow(clippy::too_many_arguments)]
pub fn decode(
    words: usize,
    boxes: &[[f64; 4]],
    same_cell: &dyn Fn(usize, usize) -> bool,
    same_row: &dyn Fn(usize, usize) -> bool,
    same_column: &dyn Fn(usize, usize) -> bool,
    row_span: &dyn Fn(usize) -> usize,
    column_span: &dyn Fn(usize) -> usize,
) -> Vec<Cell> {
    // Cells are components, which IS right here: a word in a cell with another
    // word is in that cell, and the relation cannot disagree with itself the
    // way `same_row` can.
    let mut edges = Vec::new();
    for i in 0..words {
        for j in (i + 1)..words {
            if same_cell(i, j) {
                edges.push((i, j));
            }
        }
    }
    let label = components(words, &edges);
    let count = label.iter().copied().max().map(|m| m + 1).unwrap_or(0);
    let mut members: Vec<Vec<usize>> = vec![Vec::new(); count];
    for (index, &group) in label.iter().enumerate() {
        members[group].push(index);
    }
    // Reading order inside the cell: down the lines, left to right along them.
    for group in members.iter_mut() {
        group.sort_by(|&a, &b| {
            (-boxes[a][3], boxes[a][0])
                .partial_cmp(&(-boxes[b][3], boxes[b][0]))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    let mut cells: Vec<Cell> = members
        .iter()
        .map(|group| Cell {
            words: group.clone(),
            bbox: [
                group
                    .iter()
                    .map(|&i| boxes[i][0])
                    .fold(f64::INFINITY, f64::min),
                group
                    .iter()
                    .map(|&i| boxes[i][1])
                    .fold(f64::INFINITY, f64::min),
                group
                    .iter()
                    .map(|&i| boxes[i][2])
                    .fold(f64::NEG_INFINITY, f64::max),
                group
                    .iter()
                    .map(|&i| boxes[i][3])
                    .fold(f64::NEG_INFINITY, f64::max),
            ],
            rows: Vec::new(),
            columns: Vec::new(),
        })
        .collect();

    let cell_shares = |relation: &dyn Fn(usize, usize) -> bool, a: usize, b: usize| {
        members[a]
            .iter()
            .any(|&i| members[b].iter().any(|&j| relation(i, j)))
    };
    // Rows top-first and columns left-first, so the numbers a reader expects
    // are the numbers that come out.
    let mut down: Vec<usize> = (0..cells.len()).collect();
    down.sort_by(|&a, &b| {
        (-cells[a].bbox[3], cells[a].bbox[0])
            .partial_cmp(&(-cells[b].bbox[3], cells[b].bbox[0]))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut across: Vec<usize> = (0..cells.len()).collect();
    across.sort_by(|&a, &b| {
        (cells[a].bbox[0], -cells[a].bbox[3])
            .partial_cmp(&(cells[b].bbox[0], -cells[b].bbox[3]))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let rows = sweep(&down, &|a, b| cell_shares(same_row, a, b));
    let columns = sweep(&across, &|a, b| cell_shares(same_column, a, b));
    let row_count = |c: usize| members[c].first().map(|&w| row_span(w)).unwrap_or(1);
    let column_count = |c: usize| members[c].first().map(|&w| column_span(w)).unwrap_or(1);
    let row_numbers = numbered(cells.len(), &rows, &row_count);
    let column_numbers = numbered(cells.len(), &columns, &column_count);
    for (index, cell) in cells.iter_mut().enumerate() {
        cell.rows = row_numbers[index].clone();
        cell.columns = column_numbers[index].clone();
    }
    cells.sort_by(|a, b| {
        (a.rows.first().copied(), a.columns.first().copied())
            .cmp(&(b.rows.first().copied(), b.columns.first().copied()))
    });
    cells
}

/// The cells, plus an empty one at every grid position nothing covers.
///
/// A blank cell holds no words, so nothing built on words can SEE it -- but
/// the grid is a rectangle, and once the rows and columns are known from the
/// cells that do hold words, a position no cell and no span covers can only be
/// an empty cell. It is inferred rather than read.
///
/// What it cannot reach: an empty cell that should have been part of a SPAN
/// comes out as a cell of its own, and a row or column made entirely of empty
/// cells is not in the rectangle at all, because no word ever put it there.
/// Those two want the ruling lines.
#[doc(hidden)]
pub fn fill_empty(mut cells: Vec<Cell>) -> Vec<Cell> {
    use std::collections::BTreeSet;
    let covered: BTreeSet<(usize, usize)> = cells
        .iter()
        .flat_map(|c| {
            c.rows
                .iter()
                .flat_map(move |&r| c.columns.iter().map(move |&k| (r, k)))
        })
        .collect();
    if covered.is_empty() {
        return cells;
    }
    let rows = covered.iter().map(|&(r, _)| r).max().unwrap_or(0) + 1;
    let columns = covered.iter().map(|&(_, k)| k).max().unwrap_or(0) + 1;
    for row in 0..rows {
        for column in 0..columns {
            if !covered.contains(&(row, column)) {
                cells.push(Cell {
                    words: Vec::new(),
                    bbox: [0.0; 4],
                    rows: vec![row],
                    columns: vec![column],
                });
            }
        }
    }
    cells.sort_by(|a, b| {
        (a.rows.first().copied(), a.columns.first().copied())
            .cmp(&(b.rows.first().copied(), b.columns.first().copied()))
    });
    cells
}

/// Give each cell the columns its ink actually covers, and absorb the empty
/// cells it covers.
///
/// WHY THIS EXISTS. The decoder is told each cell spans one column
/// (`tables::grid_of`), because a learned span classifier measured WORSE on
/// TEDS than assuming one. That is true and it is also not the whole story: a
/// header drawn over two columns comes back as a cell in the first and an
/// EMPTY cell in the second. `golden/merged.md` holds such a table with
/// `colspan="2"`, which the heuristics get right today, and TEDS over a corpus
/// of clean scientific tables is not sensitive to the difference. A reader is.
///
/// THE SAME RULE THE HEURISTICS USE, not a new one: a cell covers the columns
/// its INK covers (`tables.rs`, `covered`). What that needs is the table's
/// column boundaries, and the trap is that the spanning cell itself widens the
/// column it starts in, so boundaries computed from every cell put the
/// boundary beyond its right edge and it never spans anything. That is the
/// first version of this function and it changed nothing at all.
///
/// SO THE BOUNDARIES COME FROM THE FULL ROWS -- the rows with a cell in every
/// column, which are the rows that show where the columns are. A header over
/// two of three columns has two cells, is not full, and does not get a vote on
/// where the boundary it crosses lies. With no full row nothing is widened,
/// which is the right answer rather than a guess.
///
/// A HOLE IS STILL A HOLE. An empty cell is absorbed only where a cell's ink
/// actually reaches over it; `a_hole_in_a_row_keeps_its_place_in_the_html`
/// holds that open, and an earlier version of this that absorbed by position
/// rather than by ink broke it and five others.
#[doc(hidden)]
pub fn widen_spans(mut cells: Vec<Cell>) -> Vec<Cell> {
    use std::collections::{BTreeMap, BTreeSet};

    let columns: BTreeSet<usize> = cells
        .iter()
        .flat_map(|c| c.columns.iter().copied())
        .collect();
    let rows: BTreeSet<usize> = cells.iter().flat_map(|c| c.rows.iter().copied()).collect();
    if columns.len() < 2 || rows.is_empty() {
        return cells;
    }

    // The rows that have a cell with words in every column: the rows that show
    // where the columns are.
    let mut full: Vec<usize> = Vec::new();
    for &row in &rows {
        let here: BTreeSet<usize> = cells
            .iter()
            .filter(|c| c.rows.contains(&row) && !c.words.is_empty())
            .flat_map(|c| c.columns.iter().copied())
            .collect();
        if here == columns {
            full.push(row);
        }
    }
    if full.is_empty() {
        return cells;
    }

    let mut extent: BTreeMap<usize, (f64, f64)> = BTreeMap::new();
    for cell in cells
        .iter()
        .filter(|c| !c.words.is_empty() && c.rows.iter().any(|r| full.contains(r)))
    {
        for &column in &cell.columns {
            let e = extent.entry(column).or_insert((f64::MAX, f64::MIN));
            e.0 = e.0.min(cell.bbox[0]);
            e.1 = e.1.max(cell.bbox[2]);
        }
    }
    if extent.len() != columns.len() {
        return cells;
    }

    // The boundary between two columns is the middle of the gap between them.
    let ordered: Vec<(usize, (f64, f64))> = extent.into_iter().collect();
    let mut band: BTreeMap<usize, (f64, f64)> = BTreeMap::new();
    for (n, (column, (lo, hi))) in ordered.iter().enumerate() {
        let left = if n == 0 {
            f64::NEG_INFINITY
        } else {
            (ordered[n - 1].1 .1 + lo) / 2.0
        };
        let right = if n + 1 == ordered.len() {
            f64::INFINITY
        } else {
            (hi + ordered[n + 1].1 .0) / 2.0
        };
        band.insert(*column, (left, right));
    }

    let mut absorbed: Vec<usize> = Vec::new();
    for index in 0..cells.len() {
        if cells[index].words.is_empty() {
            continue;
        }
        let (x0, x1) = (cells[index].bbox[0], cells[index].bbox[2]);
        let covers: Vec<usize> = band
            .iter()
            .filter(|(_, (left, right))| x1 > *left && x0 < *right)
            .map(|(column, _)| *column)
            .collect();
        if covers.len() <= cells[index].columns.len() {
            continue;
        }
        let rows_here = cells[index].rows.clone();
        // Only over EMPTY cells: a cell whose ink reaches into a column that
        // holds words of its own is two cells overlapping, which is a
        // different defect and not one to paper over by merging them.
        let blocked = cells.iter().enumerate().any(|(k, other)| {
            k != index
                && !other.words.is_empty()
                && other.rows.iter().any(|r| rows_here.contains(r))
                && other.columns.iter().any(|c| covers.contains(c))
        });
        if blocked {
            continue;
        }
        for (k, other) in cells.iter().enumerate() {
            if k != index
                && other.words.is_empty()
                && other.rows.iter().any(|r| rows_here.contains(r))
                && other.columns.iter().all(|c| covers.contains(c))
            {
                absorbed.push(k);
            }
        }
        cells[index].columns = covers;
    }

    absorbed.sort_unstable();
    absorbed.dedup();
    for index in absorbed.into_iter().rev() {
        cells.remove(index);
    }
    cells.sort_by(|a, b| {
        (a.rows.first().copied(), a.columns.first().copied())
            .cmp(&(b.rows.first().copied(), b.columns.first().copied()))
    });
    cells
}
