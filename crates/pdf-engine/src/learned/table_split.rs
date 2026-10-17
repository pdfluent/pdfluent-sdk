// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Split one emitted table back into the tables glued into it (#574).
//!
//! WHERE AND WHY. Tuning found record-gap pages where two tables come out as
//! one: side by side (two statements next to each other read as one wide
//! grid) and stacked (two statements one above the other, the second's year
//! header sitting inside the grid after figure rows). The judge chose one
//! region for both, or the stitch merged two -- by the time the grid is
//! decoded the seam is only visible in the cells. So this pass runs on the
//! final cell grid, before the blocks are emitted: rebuilt and pass-through
//! tables alike, so `extract_blocks`, markdown and every dump read the same
//! tables. Side by side splits in [`super::tables::refine_judged_traced`],
//! after every region is rebuilt; stacked splits after the final-table
//! judge, on the tables v2 kept
//! ([`super::tables::split_stacked_after_v2`]), so a glued table v2 refuses
//! goes back to prose whole instead of passing as one part.
//!
//! TWO SWITCHES: STACKED SHIPS ON, SIDE BY SIDE OFF (#574).
//! `table_split_side_by_side` splits a grid vertically where a stub column
//! repeats the left part past the widest gap; `table_split_stacked` splits
//! it horizontally before a year row with figure rows above and below. Each
//! part is renumbered from zero and rows empty in
//! a part are dropped, so each part is a table by the [`BlockKind::TableCell`]
//! contract; a table that does not split comes back bit-identical, which the
//! off-shape of every operator test pins.
//!
//! A FIGURE CELL is the stitch's per-word shape without its gap neighbours:
//! figure-shaped, letter-free, not leader fill. The gap test needs words on
//! a line and a grid cell has none; the letters are what keep a label stub
//! (`Class D, (3-mo. EURIBOR at 3.3`) from reading as figures, which the raw
//! shape -- a digit anywhere -- would call it.
//!
//! EVERY THRESHOLD below is a named constant, each with a test at its boundary
//! value.

use crate::blocks::{Block, BlockKind};

/// How many columns each part of a vertical split keeps.
const MIN_PART_COLUMNS: u32 = 2;
/// What share of a stub column's non-empty cells is non-figure text.
const STUB_TEXT_SHARE: f64 = 0.6;
/// How many year cells a year row holds -- or year numbers in its text for a
/// period header -- unless the table has [`YEAR_CELLS_FEW_FIGURES_AT`] figure
/// columns.
const YEAR_CELLS: usize = 2;
/// The figure-column count at which one year is enough for a year row.
const YEAR_CELLS_FEW_FIGURES_AT: usize = 2;
/// How many year cells (or numbers) that narrower table's year row holds.
const YEAR_CELLS_FEW_FIGURES: usize = 1;
/// How many figure rows stand above a stacked split.
const FIGURE_ROWS_ABOVE: usize = 2;
/// How many figure rows stand below a stacked split.
const FIGURE_ROWS_BELOW: usize = 1;

/// Words a period header is made of: what a year row's full text contains
/// where its stub cell plus its years read as one (`At December 31: 2018
/// 2017`, `Year Ended December 31, 2009`), lowercased, matched by substring.
const PERIOD_WORDS: &[&str] = &[
    "year ended",
    "years ended",
    "year-end",
    "year end",
    "ended december",
    "december 31",
    "dec 31",
    "dec. 31",
    "at december",
    "as of",
    "for the year",
    "for the years",
    "fiscal year",
];

/// Whether the cell text reads as a figure: the stitch's figure shape,
/// letter-free and not leader fill (see the module docs).
fn is_figure_cell(text: &str) -> bool {
    let trimmed = text.trim();
    super::stitch::is_figure_shape(trimmed)
        && !trimmed.chars().any(|c| c.is_ascii_alphabetic())
        && !super::stitch::is_leader_dots(trimmed)
}

/// Whether the cell text is a year a statement would head its columns with:
/// the stitch's [`super::stitch::is_year`], on trimmed text.
fn is_year_cell(text: &str) -> bool {
    super::stitch::is_year(text.trim())
}

/// `(row, column, row_span, column_span, header)` of a table cell, spans at
/// least one like [`super::cell_merge`] reads them.
fn position(block: &Block) -> Option<(u32, u32, u32, u32, bool)> {
    match block.kind {
        BlockKind::TableCell {
            row,
            column,
            row_span,
            column_span,
            header,
        } => Some((
            u32::from(row),
            u32::from(column),
            u32::from(row_span.max(1)),
            u32::from(column_span.max(1)),
            header,
        )),
        _ => None,
    }
}

/// One cell of the table under the knife: its block and its grid position.
#[derive(Clone, Copy)]
struct Cell {
    block: usize,
    row: u32,
    col: u32,
    row_span: u32,
    col_span: u32,
    header: bool,
}

/// What a column reads as: figures, text, or nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Figure,
    Text,
}

fn cells_of(table: &[Block]) -> Vec<Cell> {
    table
        .iter()
        .enumerate()
        .filter_map(|(block, b)| {
            position(b).map(|(row, col, row_span, col_span, header)| Cell {
                block,
                row,
                col,
                row_span,
                col_span,
                header,
            })
        })
        .collect()
}

/// Whether the cell's text is non-empty.
fn non_empty(table: &[Block], cell: &Cell) -> bool {
    !table[cell.block].text.trim().is_empty()
}

/// The kind of one column inside the part: figures where most of its
/// non-empty cells read as figures, text otherwise; `None` where it holds no
/// non-empty cell.
fn part_kind(table: &[Block], cells: &[Cell], part: &[usize], col: u32) -> Option<Kind> {
    let mut figures = 0usize;
    let mut total = 0usize;
    for &i in part.iter().filter(|&&i| cells[i].col == col) {
        if non_empty(table, &cells[i]) {
            total += 1;
            if is_figure_cell(&table[cells[i].block].text) {
                figures += 1;
            }
        }
    }
    if total == 0 {
        None
    } else if figures * 2 > total {
        Some(Kind::Figure)
    } else {
        Some(Kind::Text)
    }
}

/// The left edge of a column: the smallest x0 of its single-column cells;
/// infinity where it holds none. A spanning cell's ink is evidence for no
/// one column's extent, so spans stay out of the geometry.
fn column_left(table: &[Block], cells: &[Cell], part: &[usize], col: u32) -> f64 {
    part.iter()
        .filter(|&&i| cells[i].col == col && cells[i].col_span == 1)
        .map(|&i| table[cells[i].block].bbox[0])
        .fold(f64::INFINITY, f64::min)
}

/// The right edge of a column: the largest x1 of its single-column cells;
/// negated infinity where it holds none. Spans stay out, as in
/// [`column_left`].
fn column_right(table: &[Block], cells: &[Cell], part: &[usize], col: u32) -> f64 {
    part.iter()
        .filter(|&&i| cells[i].col == col && cells[i].col_span == 1)
        .map(|&i| table[cells[i].block].bbox[2])
        .fold(f64::NEG_INFINITY, f64::max)
}

/// How many rows of the part the span covers: the rows it spans that hold
/// a cell of the part, so a span crossing the cut is truncated to it.
fn span_rows(rows: &[u32], row: u32, row_span: u32) -> u16 {
    rows.iter()
        .filter(|&&r| row <= r && r < row + row_span)
        .count() as u16
}

/// How many columns of the part the span covers: as [`span_rows`].
fn span_cols(cols: &[u32], col: u32, col_span: u32) -> u16 {
    cols.iter()
        .filter(|&&c| col <= c && c < col + col_span)
        .count() as u16
}

/// The texts of the header row's cells in the column range, in column order:
/// the first row holding a header cell. Empty where the table has no header
/// row or the range holds none of it.
fn header_texts(
    table: &[Block],
    cells: &[Cell],
    part: &[usize],
    cols: std::ops::Range<u32>,
) -> Vec<String> {
    let header_row = part
        .iter()
        .map(|&i| cells[i].row)
        .filter(|&row| part.iter().any(|&j| cells[j].row == row && cells[j].header))
        .min();
    let Some(row) = header_row else {
        return Vec::new();
    };
    let mut at: Vec<(u32, String)> = part
        .iter()
        .filter(|&&i| cells[i].row == row && cols.contains(&cells[i].col))
        .filter(|&&i| non_empty(table, &cells[i]))
        .map(|&i| (cells[i].col, table[cells[i].block].text.trim().to_string()))
        .collect();
    at.sort();
    at.into_iter().map(|(_, text)| text).collect()
}

/// The whitespace before column `k`: its left edge minus the previous
/// column's right edge, in the columns' x geometry.
fn gap_before(table: &[Block], cells: &[Cell], part: &[usize], k: u32) -> f64 {
    column_left(table, cells, part, k) - column_right(table, cells, part, k - 1)
}

/// The vertical cut before the first qualifying column, or `None`: both
/// parts keep [`MIN_PART_COLUMNS`] columns, the cut column reads as a stub
/// with a figure column right of it, the right part repeats the left part's
/// column kinds or header texts, and the gap before it stands out against
/// its neighbours. A span crossing the cut is truncated to its part.
///
/// THE GAP STANDS OUT LOCALLY, not globally. The spec asks for the table's
/// widest gap, but on the motivating pages the whitespace inside a statement
/// -- a ragged stub column ending 52pt before its figures -- is wider than
/// the 27pt gutter between the statements, so the widest gap never marks
/// the seam. What the gutter does is exceed its figure-figure neighbour
/// (21pt): the cut stands where the whitespace widens against at least one
/// side. A uniformly set table still never splits, which is what the gap
/// check is for.
fn side_cut(table: &[Block], cells: &[Cell], part: &[usize]) -> Option<(Vec<usize>, Vec<usize>)> {
    let min_col = part.iter().map(|&i| cells[i].col).min()?;
    let max_col = part.iter().map(|&i| cells[i].col).max()?;

    for k in min_col + 1..=max_col {
        // Both parts keep [`MIN_PART_COLUMNS`] columns.
        let mut west_cols: Vec<u32> = part
            .iter()
            .map(|&i| cells[i].col)
            .filter(|&c| c < k)
            .collect();
        west_cols.sort_unstable();
        west_cols.dedup();
        let mut east_cols: Vec<u32> = part
            .iter()
            .map(|&i| cells[i].col)
            .filter(|&c| c >= k)
            .collect();
        east_cols.sort_unstable();
        east_cols.dedup();
        if west_cols.len() < MIN_PART_COLUMNS as usize
            || east_cols.len() < MIN_PART_COLUMNS as usize
        {
            continue;
        }
        let mut texts = 0usize;
        let mut total = 0usize;
        for &i in part.iter().filter(|&&i| cells[i].col == k) {
            if non_empty(table, &cells[i]) {
                total += 1;
                if !is_figure_cell(&table[cells[i].block].text) {
                    texts += 1;
                }
            }
        }
        if total == 0 {
            continue;
        }
        let share = texts as f64 / total as f64;
        if share < STUB_TEXT_SHARE {
            continue;
        }
        if !(k + 1..=max_col)
            .any(|j| part_kind(table, cells, part, j).is_some_and(|kind| kind == Kind::Figure))
        {
            continue;
        }
        let left: Vec<Kind> = (min_col..k)
            .filter_map(|j| part_kind(table, cells, part, j))
            .collect();
        let right: Vec<Kind> = (k..=max_col)
            .filter_map(|j| part_kind(table, cells, part, j))
            .collect();
        // The right part repeats the left where it opens with the same kinds:
        // a prefix, not equality, so three glued statements come apart one
        // cut at a time -- the first cut's right part still holds two.
        let kinds_repeat =
            !left.is_empty() && right.len() >= left.len() && right[..left.len()] == left[..];
        let left_head = header_texts(table, cells, part, min_col..k);
        let right_head = header_texts(table, cells, part, k..max_col + 1);
        let heads_repeat =
            !left_head.is_empty() && !right_head.is_empty() && left_head == right_head;
        if !kinds_repeat && !heads_repeat {
            continue;
        }
        let gap = gap_before(table, cells, part, k);
        let left_gap = (k > min_col + 1).then(|| gap_before(table, cells, part, k - 1));
        let right_gap = (k < max_col).then(|| gap_before(table, cells, part, k + 1));
        let stands_out = gap.is_finite()
            && (left_gap.is_some_and(|g| g.is_finite() && gap > g)
                || right_gap.is_some_and(|g| g.is_finite() && gap > g));

        if !stands_out {
            continue;
        }
        let west: Vec<usize> = part.iter().copied().filter(|&i| cells[i].col < k).collect();
        let east: Vec<usize> = part
            .iter()
            .copied()
            .filter(|&i| cells[i].col >= k)
            .collect();
        if west.is_empty() || east.is_empty() {
            continue;
        }
        return Some((west, east));
    }
    None
}

/// Whether the row holds a non-year figure outside the stub column.
fn row_has_figures(table: &[Block], cells: &[Cell], part: &[usize], row: u32) -> bool {
    part.iter().any(|&i| {
        cells[i].row == row
            && cells[i].col >= 1
            && non_empty(table, &cells[i])
            && is_figure_cell(&table[cells[i].block].text)
            && !is_year_cell(&table[cells[i].block].text)
    })
}

/// The distinct years the row's full text names, stub cell first.
fn row_years(table: &[Block], cells: &[Cell], part: &[usize], row: u32) -> Vec<String> {
    let mut order: Vec<(u32, usize)> = part
        .iter()
        .filter(|&&i| cells[i].row == row)
        .map(|&i| (cells[i].col, i))
        .collect();
    order.sort();
    let mut years: Vec<String> = Vec::new();
    for (_, i) in order {
        for token in table[cells[i].block]
            .text
            .split(|c: char| !c.is_ascii_alphanumeric())
        {
            if is_year_cell(token) && !years.iter().any(|y| y == token) {
                years.push(token.to_string());
            }
        }
    }
    years
}

/// Whether the row is a year row: its non-empty cells right of the stub
/// column are all years, or its stub cell text plus those years reads as a
/// period header -- with at least `need` years either way, and no figure
/// outside the stub in the period reading.
fn is_year_row(table: &[Block], cells: &[Cell], part: &[usize], row: u32, need: usize) -> bool {
    let mut right = 0usize;
    let mut years = 0usize;
    for &i in part
        .iter()
        .filter(|&&i| cells[i].row == row && cells[i].col >= 1 && non_empty(table, &cells[i]))
    {
        right += 1;
        if is_year_cell(&table[cells[i].block].text) {
            years += 1;
        }
    }
    if right > 0 && years == right && years >= need {
        return true;
    }
    if row_has_figures(table, cells, part, row) {
        return false;
    }
    let mut order: Vec<(u32, usize)> = part
        .iter()
        .filter(|&&i| cells[i].row == row)
        .map(|&i| (cells[i].col, i))
        .collect();
    order.sort();
    let text: String = order
        .iter()
        .map(|&(_, i)| table[cells[i].block].text.trim())
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    row_years(table, cells, part, row).len() >= need
        && PERIOD_WORDS.iter().any(|word| text.contains(word))
}

/// The horizontal cut before row `r`, or `None`: `r` is a year row with at
/// least [`FIGURE_ROWS_ABOVE`] figure rows above it and [`FIGURE_ROWS_BELOW`]
/// below, and is not the table's first non-header row. Rows directly above
/// `r` that are header-like join the lower part as its header. No cut
/// through a spanning cell.
fn stacked_cut(
    table: &[Block],
    cells: &[Cell],
    part: &[usize],
) -> Option<(Vec<usize>, Vec<usize>)> {
    let min_col = part.iter().map(|&i| cells[i].col).min()?;
    let max_col = part.iter().map(|&i| cells[i].col).max()?;
    let mut rows: Vec<u32> = part.iter().map(|&i| cells[i].row).collect();
    rows.sort_unstable();
    rows.dedup();

    let figures = (min_col..=max_col)
        .filter(|&j| part_kind(table, cells, part, j).is_some_and(|k| k == Kind::Figure))
        .count();
    let need = if figures == YEAR_CELLS_FEW_FIGURES_AT {
        YEAR_CELLS_FEW_FIGURES
    } else {
        YEAR_CELLS
    };
    let first_body = part
        .iter()
        .map(|&i| cells[i].row)
        .filter(|&row| {
            part.iter()
                .any(|&j| cells[j].row == row && !cells[j].header)
        })
        .min();
    for &r in &rows {
        if !is_year_row(table, cells, part, r, need) {
            continue;
        }
        if rows
            .iter()
            .filter(|&&row| row < r && row_has_figures(table, cells, part, row))
            .count()
            < FIGURE_ROWS_ABOVE
            || rows
                .iter()
                .filter(|&&row| row > r && row_has_figures(table, cells, part, row))
                .count()
                < FIGURE_ROWS_BELOW
        {
            continue;
        }
        if first_body.is_some_and(|first| first == r) {
            continue;
        }
        // Header-like rows directly above `r` join the lower part; an empty
        // row holds no figure either and vanishes in the dense renumber.
        let mut top = r;
        while top > 0 && !row_has_figures(table, cells, part, top - 1) {
            top -= 1;
        }
        let upper: Vec<usize> = part
            .iter()
            .copied()
            .filter(|&i| cells[i].row < top)
            .collect();
        let lower: Vec<usize> = part
            .iter()
            .copied()
            .filter(|&i| cells[i].row >= top)
            .collect();
        if upper.is_empty() || lower.is_empty() {
            continue;
        }
        return Some((upper, lower));
    }
    None
}

/// Split one table, side by side first and then stacked: the vertical cut at
/// the first qualifying column, repeated in the right part, and the
/// horizontal cut at the first qualifying row, repeated in both parts. Parts
/// keep reading order: west before east, upper before lower.
fn split_one(
    table: &[Block],
    cells: &[Cell],
    part: Vec<usize>,
    side: bool,
    stacked: bool,
) -> Vec<Vec<usize>> {
    let mut vertical = Vec::new();
    if side {
        let mut rest = part;
        while let Some((west, east)) = side_cut(table, cells, &rest) {
            vertical.push(west);
            rest = east;
        }
        vertical.push(rest);
    } else {
        vertical.push(part);
    }
    if !stacked {
        return vertical;
    }
    let mut out = Vec::new();
    for piece in vertical {
        stacked_fixpoint(table, cells, piece, &mut out);
    }
    out
}

/// The horizontal cuts of one part, upper before lower.
fn stacked_fixpoint(table: &[Block], cells: &[Cell], piece: Vec<usize>, out: &mut Vec<Vec<usize>>) {
    match stacked_cut(table, cells, &piece) {
        Some((upper, lower)) => {
            stacked_fixpoint(table, cells, upper, out);
            stacked_fixpoint(table, cells, lower, out);
        }
        None => out.push(piece),
    }
}

/// The dense number of a row or column inside the part: how many of the
/// part's distinct numbers stand below it.
fn rank(numbers: &[u32], value: u32) -> u16 {
    numbers.iter().filter(|&&n| n < value).count() as u16
}

/// Renumber the part's rows and columns densely from zero, in row-major
/// order; rows (and columns) holding no cell of the part fall away.
fn emit(table: &[Block], cells: &[Cell], part: &[usize]) -> Vec<Block> {
    let mut rows: Vec<u32> = part.iter().map(|&i| cells[i].row).collect();
    rows.sort_unstable();
    rows.dedup();
    let mut cols: Vec<u32> = part.iter().map(|&i| cells[i].col).collect();
    cols.sort_unstable();
    cols.dedup();
    let mut order: Vec<usize> = part.to_vec();
    order.sort_by_key(|&i| (cells[i].row, cells[i].col));
    order
        .into_iter()
        .map(|i| {
            let mut block = table[cells[i].block].clone();
            if let BlockKind::TableCell {
                row: cell_row,
                column: cell_col,
                row_span,
                column_span,
                ..
            } = &mut block.kind
            {
                *cell_row = rank(&rows, cells[i].row);
                *cell_col = rank(&cols, cells[i].col);
                *row_span = span_rows(&rows, cells[i].row, cells[i].row_span).max(1);
                *column_span = span_cols(&cols, cells[i].col, cells[i].col_span).max(1);
            }
            block
        })
        .collect()
}

/// Split every glued table in the block run: side by side where `side`, and
/// stacked where `stacked`. Tables are the maximal runs of table cells cut
/// where the coordinates start over, as [`super::tables`] reads them; other
/// blocks pass through, and a table that does not split comes back
/// bit-identical.
pub(crate) fn split_tables(blocks: Vec<Block>, side: bool, stacked: bool) -> Vec<Block> {
    if !side && !stacked {
        return blocks;
    }
    let mut out: Vec<Block> = Vec::with_capacity(blocks.len());
    let mut run: Vec<Block> = Vec::new();
    let flush = |run: &mut Vec<Block>, out: &mut Vec<Block>| {
        while !run.is_empty() {
            let mut end = 1usize;
            while end < run.len() {
                let here = position(&run[end]);
                let prev = position(&run[end - 1]);
                match (here, prev) {
                    (Some((r, c, _, _, _)), Some((pr, pc, _, _, _))) if (r, c) > (pr, pc) => {
                        end += 1;
                    }
                    _ => break,
                }
            }
            let table: Vec<Block> = run.drain(0..end).collect();
            let cells = cells_of(&table);
            let all: Vec<usize> = (0..cells.len()).collect();
            let parts = split_one(&table, &cells, all, side, stacked);
            if parts.len() == 1 {
                out.extend(table);
            } else {
                for part in &parts {
                    out.extend(emit(&table, &cells, part));
                }
            }
        }
    };
    for block in blocks {
        if matches!(block.kind, BlockKind::TableCell { .. }) {
            run.push(block);
        } else {
            flush(&mut run, &mut out);
            out.push(block);
        }
    }
    flush(&mut run, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(row: u16, col: u16, text: &str, x0: f64, x1: f64) -> Block {
        cell_as(row, col, 1, 1, false, text, x0, x1)
    }

    fn head(row: u16, col: u16, text: &str, x0: f64, x1: f64) -> Block {
        cell_as(row, col, 1, 1, true, text, x0, x1)
    }

    #[allow(clippy::too_many_arguments)]
    fn cell_as(
        row: u16,
        col: u16,
        row_span: u16,
        col_span: u16,
        header: bool,
        text: &str,
        x0: f64,
        x1: f64,
    ) -> Block {
        Block::new(
            BlockKind::TableCell {
                row,
                column: col,
                row_span,
                column_span: col_span,
                header,
            },
            0,
            [x0, 0.0, x1, 10.0],
            text.to_string(),
            0,
        )
    }

    /// `(rows, columns)` of each emitted table, in order.
    fn shapes(blocks: &[Block]) -> Vec<(u16, u16)> {
        let mut shapes = Vec::new();
        let mut rows = 0u16;
        let mut cols = 0u16;
        let mut open = false;
        for block in blocks {
            if let BlockKind::TableCell { row, column, .. } = block.kind {
                if row == 0 && column == 0 {
                    if open {
                        shapes.push((rows, cols));
                    }
                    open = true;
                    rows = 0;
                    cols = 0;
                }
                rows = rows.max(row + 1);
                cols = cols.max(column + 1);
            }
        }
        if open {
            shapes.push((rows, cols));
        }
        shapes
    }

    fn texts(blocks: &[Block]) -> Vec<String> {
        blocks.iter().map(|b| b.text.clone()).collect()
    }

    /// Two 3-column statements, the gutter between columns 2 and 3 the
    /// widest gap; the last row holds left cells only.
    fn side_by_side() -> Vec<Block> {
        let x = [(10.0, 100.0), (110.0, 150.0), (160.0, 200.0)];
        let y = [(300.0, 390.0), (400.0, 440.0), (450.0, 490.0)];
        let mut table = Vec::new();
        let heads = ["Security", "Shares", "Value"];
        for (c, (x0, x1)) in x.iter().enumerate() {
            table.push(cell(0, c as u16, heads[c], *x0, *x1));
        }
        for (c, (x0, x1)) in y.iter().enumerate() {
            table.push(cell(0, c as u16 + 3, heads[c], *x0, *x1));
        }
        for (r, label) in ["Alpha", "Bravo", "Charlie", "Delta"].iter().enumerate() {
            let r = r as u16 + 1;
            table.push(cell(r, 0, label, x[0].0, x[0].1));
            table.push(cell(r, 1, "100", x[1].0, x[1].1));
            table.push(cell(r, 2, "200", x[2].0, x[2].1));
            if r < 4 {
                table.push(cell(r, 3, label, y[0].0, y[0].1));
                table.push(cell(r, 4, "300", y[1].0, y[1].1));
                table.push(cell(r, 5, "400", y[2].0, y[2].1));
            }
        }
        table
    }

    #[test]
    fn side_by_side_parts_where_kinds_repeat() {
        let split = split_tables(side_by_side(), true, false);
        assert_eq!(shapes(&split), vec![(5, 3), (4, 3)]);
        // The row empty in the right part is dropped from it, not from the
        // left; both parts open at zero.
        assert!(texts(&split).contains(&"Delta".to_string()));
        assert_eq!(texts(&split).iter().filter(|t| *t == "Delta").count(), 1);
    }

    #[test]
    fn side_by_side_keeps_west_before_east_with_both_switches_on() {
        let split = split_tables(side_by_side(), true, true);
        assert_eq!(shapes(&split), vec![(5, 3), (4, 3)]);
        // The west part's cells stand first: its header opens the run and
        // the east part's opens after its 15 cells.
        assert_eq!(&texts(&split)[0..3], &["Security", "Shares", "Value"]);
        assert_eq!(&texts(&split)[15..18], &["Security", "Shares", "Value"]);
    }

    #[test]
    fn side_by_side_stub_share_has_its_boundary_at_three_of_five() {
        // Column 3 with three text cells of five splits; with two it stays.
        // ("Security" stays text in both; `kept` counts the rest.)
        for (kept, split) in [(2usize, true), (1usize, false)] {
            let mut table = side_by_side();
            table.push(cell(4, 3, "Echo", 300.0, 390.0));
            let mut seen = 0;
            for block in table.iter_mut() {
                if let BlockKind::TableCell { column: 3, .. } = block.kind {
                    if block.text == "Security" {
                        continue;
                    }
                    seen += 1;
                    if seen > kept {
                        block.text = "500".to_string();
                    }
                }
            }
            let out = split_tables(table, true, false);
            assert_eq!(
                shapes(&out),
                if split {
                    vec![(5, 3), (5, 3)]
                } else {
                    vec![(5, 6)]
                },
                "with {} text cells of five",
                kept + 1,
            );
        }
    }

    #[test]
    fn side_by_side_needs_a_gap_that_stands_out() {
        // Every gap 10 wide: no cut anywhere.
        let mut table = side_by_side();
        for block in table.iter_mut() {
            if let BlockKind::TableCell { column, .. } = block.kind {
                let x0 = 10.0 + f64::from(column) * 60.0;
                block.bbox = [x0, 0.0, x0 + 50.0, 10.0];
            }
        }
        let out = split_tables(table.clone(), true, false);
        assert_eq!(shapes(&out), vec![(5, 6)]);
        assert_eq!(texts(&out), texts(&table));
    }

    #[test]
    fn side_by_side_splits_where_the_gutter_exceeds_its_neighbour() {
        // The gutter (27pt) is narrower than the stub-figure whitespace
        // (52pt) but wider than its figure-figure neighbour (21pt): the
        // motivating pages' shape, and still a cut.
        let mut table = side_by_side();
        let edges = [
            (10.0, 100.0),
            (152.0, 180.0),
            (201.0, 230.0),
            (257.0, 347.0),
            (399.0, 427.0),
            (448.0, 477.0),
        ];
        for block in table.iter_mut() {
            if let BlockKind::TableCell { column, .. } = block.kind {
                let (x0, x1) = edges[column as usize];
                block.bbox = [x0, 0.0, x1, 10.0];
            }
        }
        let out = split_tables(table, true, false);
        assert_eq!(shapes(&out), vec![(5, 3), (4, 3)]);
    }

    #[test]
    fn side_by_side_truncates_a_span_crossing_the_cut() {
        let mut table = side_by_side();
        for block in table.iter_mut() {
            if matches!(
                block.kind,
                BlockKind::TableCell {
                    row: 1,
                    column: 2,
                    ..
                }
            ) {
                if let BlockKind::TableCell { column_span, .. } = &mut block.kind {
                    *column_span = 2;
                }
            }
        }
        let out = split_tables(table, true, false);
        assert_eq!(shapes(&out), vec![(5, 3), (4, 3)]);
        let spanned = out
            .iter()
            .find(|b| {
                matches!(
                    b.kind,
                    BlockKind::TableCell {
                        row: 1,
                        column: 2,
                        ..
                    }
                )
            })
            .expect("the spanning cell stays in the west part");
        assert!(
            matches!(spanned.kind, BlockKind::TableCell { column_span: 1, .. }),
            "the span is truncated to the cut: {:?}",
            spanned.kind
        );
    }

    #[test]
    fn side_by_side_repeats_into_the_right_part() {
        // Three 2-column statements: the first cut's right part still holds
        // two, and the second cut parts those.
        let x = [
            (10.0, 100.0),
            (110.0, 150.0),
            (200.0, 290.0),
            (300.0, 340.0),
            (380.0, 470.0),
            (480.0, 520.0),
        ];
        let mut table = Vec::new();
        for row in 0..4u16 {
            for col in 0..6u16 {
                let text = if col % 2 == 0 { "Item" } else { "12" };
                table.push(cell(row, col, text, x[col as usize].0, x[col as usize].1));
            }
        }
        let split = split_tables(table, true, false);
        assert_eq!(shapes(&split), vec![(4, 2), (4, 2), (4, 2)]);
    }

    #[test]
    fn side_by_side_splits_on_equal_headers_where_kinds_differ() {
        // The right part reads text/figure/text against the left's
        // text/figure/figure, but the header texts match.
        let mut table = side_by_side();
        for block in table.iter_mut() {
            if let BlockKind::TableCell { row, column: 5, .. } = block.kind {
                if row > 0 {
                    block.text = "note".to_string();
                }
            }
            if let BlockKind::TableCell { row: 0, .. } = block.kind {
                if let BlockKind::TableCell { header, .. } = &mut block.kind {
                    *header = true;
                }
            }
        }
        let split = split_tables(table, true, false);
        assert_eq!(shapes(&split), vec![(5, 3), (4, 3)]);
    }

    /// Two stacked statements with a year row between the figure rows.
    fn stacked(year_cells: &[&str], stub: &str) -> Vec<Block> {
        let mut table = vec![
            head(0, 0, "For the year", 10.0, 100.0),
            head(0, 1, "2018", 110.0, 150.0),
            head(0, 2, "2017", 160.0, 200.0),
        ];
        for (r, label) in ["Alpha", "Bravo"].iter().enumerate() {
            let r = r as u16 + 1;
            table.push(cell(r, 0, label, 10.0, 100.0));
            table.push(cell(r, 1, "100", 110.0, 150.0));
            table.push(cell(r, 2, "200", 160.0, 200.0));
        }
        table.push(cell(3, 0, stub, 10.0, 100.0));
        for (c, year) in year_cells.iter().enumerate() {
            table.push(cell(3, c as u16 + 1, year, 110.0, 200.0));
        }
        for (r, label) in ["Charlie", "Delta"].iter().enumerate() {
            let r = r as u16 + 4;
            table.push(cell(r, 0, label, 10.0, 100.0));
            table.push(cell(r, 1, "300", 110.0, 150.0));
            table.push(cell(r, 2, "400", 160.0, 200.0));
        }
        table
    }

    #[test]
    fn stacked_parts_before_a_year_row() {
        let split = split_tables(stacked(&["2018", "2017"], "At December 31:"), false, true);
        assert_eq!(shapes(&split), vec![(3, 3), (3, 3)]);
        assert_eq!(texts(&split)[9], "At December 31:");
    }

    #[test]
    fn stacked_needs_two_figure_rows_above_and_one_below() {
        // One figure row above: no split.
        let mut table = stacked(&["2018", "2017"], "At December 31:");
        table.retain(|b| !matches!(b.kind, BlockKind::TableCell { row: 1, .. }));
        let out = split_tables(table, false, true);
        assert_eq!(shapes(&out), vec![(6, 3)]);
        // No figure row below: no split.
        let mut table = stacked(&["2018", "2017"], "At December 31:");
        table.retain(|b| {
            !matches!(b.kind, BlockKind::TableCell { row: 4, .. })
                && !matches!(b.kind, BlockKind::TableCell { row: 5, .. })
        });
        let out = split_tables(table, false, true);
        assert_eq!(shapes(&out), vec![(4, 3)]);
    }

    #[test]
    fn stacked_needs_two_years_or_one_with_two_figure_columns() {
        // One year with three figure columns: no split.
        let mut table = stacked(&["2018"], "At December 31:");
        let extra: Vec<Block> = [0u16, 1, 2, 4, 5]
            .iter()
            .map(|&row| {
                let text = if row == 0 { "2016" } else { "600" };
                cell(row, 3, text, 210.0, 250.0)
            })
            .collect();
        table.extend(extra);
        table.sort_by_key(|b| match b.kind {
            BlockKind::TableCell { row, column, .. } => (row, column),
            _ => (u16::MAX, u16::MAX),
        });
        let out = split_tables(table, false, true);
        assert_eq!(shapes(&out), vec![(6, 4)]);
        // One year with two figure columns: split.
        let out = split_tables(stacked(&["2018"], "At December 31:"), false, true);
        assert_eq!(shapes(&out), vec![(3, 3), (3, 3)]);
    }

    #[test]
    fn stacked_year_row_under_the_header_does_not_split() {
        // The figure rows above read as header cells, so the year row is the
        // first non-header row: it reads as the table's header, not a new
        // table. (With plain figure rows above, the figure count is what
        // blocks, and the natural shape is pinned through the product path.)
        let mut table = stacked(&["2018", "2017"], "At December 31:");
        for block in table.iter_mut() {
            if let BlockKind::TableCell { row: 1 | 2, .. } = block.kind {
                if let BlockKind::TableCell { header, .. } = &mut block.kind {
                    *header = true;
                }
            }
        }
        let out = split_tables(table, false, true);
        assert_eq!(shapes(&out), vec![(6, 3)]);
    }

    #[test]
    fn stacked_moves_header_like_rows_down_with_the_year_row() {
        let mut table = stacked(&["2018", "2017"], "At December 31:");
        // The label row stands between the figures and the year row: shift
        // rows 3.. down by one, then insert it in row-major order.
        for block in table.iter_mut() {
            if let BlockKind::TableCell { row, .. } = &mut block.kind {
                if *row >= 3 {
                    *row += 1;
                }
            }
        }
        table.insert(9, cell(3, 0, "Balance Sheet:", 10.0, 100.0));
        let split = split_tables(table, false, true);
        assert_eq!(shapes(&split), vec![(3, 3), (4, 3)]);
        assert_eq!(texts(&split)[9], "Balance Sheet:");
        assert_eq!(texts(&split)[10], "At December 31:");
    }

    #[test]
    fn stacked_truncates_a_span_crossing_the_cut() {
        let mut table = stacked(&["2018", "2017"], "At December 31:");
        for block in table.iter_mut() {
            if matches!(
                block.kind,
                BlockKind::TableCell {
                    row: 2,
                    column: 0,
                    ..
                }
            ) {
                if let BlockKind::TableCell { row_span, .. } = &mut block.kind {
                    *row_span = 2;
                }
            }
        }
        let out = split_tables(table, false, true);
        assert_eq!(shapes(&out), vec![(3, 3), (3, 3)]);
        let spanned = out
            .iter()
            .find(|b| {
                matches!(
                    b.kind,
                    BlockKind::TableCell {
                        row: 2,
                        column: 0,
                        ..
                    }
                )
            })
            .expect("the spanning cell stays in the upper part");
        assert!(
            matches!(spanned.kind, BlockKind::TableCell { row_span: 1, .. }),
            "the span is truncated to the cut: {:?}",
            spanned.kind
        );
    }

    #[test]
    fn without_either_switch_the_blocks_come_back_untouched() {
        let table = side_by_side();
        let out = split_tables(table.clone(), false, false);
        assert_eq!(texts(&out), texts(&table));
        assert_eq!(shapes(&out), shapes(&table));
    }
}
