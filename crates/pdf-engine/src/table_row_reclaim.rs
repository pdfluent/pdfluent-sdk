// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Marker-led rows the table pass left at the edge of an accepted table (#636).
//!
//! The table judge and the column guards stay exactly as they are: this step
//! never proposes a table, never scores one, and never moves a threshold. It
//! only reads the blocks the learned pass already accepted. Where a paragraph
//! sits directly above the first row, or directly below the last row, of an
//! accepted table, and every visual line of that paragraph opens with a short
//! line-number marker in the table's marker column, the paragraph is the
//! table's own edge rows: a line whose ink reaches the table's amount column
//! reads as marker, stub and amount cells, and a checked-option line over the
//! table's checkbox rulings reads as one colspan row. Anything else -- wrapped
//! prose, a note between two tables, a marker line whose amount lands
//! elsewhere -- stays a paragraph.
//!
//! What joins here still passes through `crate::blocks::peel_table_edges`
//! like any row: a reclaimed single-cell edge row that reads as a note or a
//! sentence may be peeled back to prose, in which case the reclaim silently
//! does not apply there. Stacks join inner rows first, so an outer row that
//! does join arrives shielded mid-run.

use crate::blocks::{Block, BlockKind};
use crate::text::{Ruling, TextSpan};

/// How far, in points, a paragraph may stand off the table edge and still be
/// its row. The income rows of the measured form stand 4 points apart.
const EDGE_GAP: f64 = 12.0;

/// How much of the paragraph's width must overlap the table's width.
const WIDTH_OVERLAP: f64 = 0.4;

/// A table run needs this many cells before it can adopt an edge row. One or
/// two stray cells are no table to join.
const MIN_RUN_CELLS: usize = 3;

/// A line-number marker is at most this many characters: `d`, `7a`, `b`.
const MARKER_LEN: usize = 3;

/// Slack, in points, around the table's marker and amount columns.
const COLUMN_SLACK: f64 = 4.0;

/// The widest ink, in points, that still counts as an amount entry.
const AMOUNT_WIDTH: f64 = 40.0;

/// A visual line splits off only past this fraction of the line height, so
/// drifted words on one line never read as two.
const LINE_SPLIT: f64 = 0.75;

/// The smallest and largest side, in points, of a checkbox ruling.
const CHECKBOX_MIN: f64 = 3.0;
const CHECKBOX_SIDE: f64 = 16.0;

/// Box of a span in page space, y up.
fn span_box(span: &TextSpan) -> [f64; 4] {
    [span.x, span.y, span.x + span.width, span.y + span.height]
}

/// Whether two 1-D ranges overlap by at least `need` of the first.
fn overlaps(a0: f64, a1: f64, b0: f64, b1: f64, need: f64) -> bool {
    let lo = a0.max(b0);
    let hi = a1.min(b1);
    hi - lo >= need
}

/// Whether the text reads as a line-number marker.
fn is_marker(text: &str) -> bool {
    let trimmed = text.trim();
    !trimmed.is_empty()
        && trimmed.len() <= MARKER_LEN
        && trimmed.chars().all(|c| c.is_alphanumeric())
}

/// Union box of span references.
fn union_box(spans: &[&TextSpan]) -> [f64; 4] {
    let first = span_box(spans[0]);
    spans[1..].iter().fold(first, |seen, span| {
        let box_ = span_box(span);
        [
            seen[0].min(box_[0]),
            seen[1].min(box_[1]),
            seen[2].max(box_[2]),
            seen[3].max(box_[3]),
        ]
    })
}

/// The paragraph's spans grouped into visual lines, top line first, each line
/// left to right.
fn visual_lines(spans: &[TextSpan]) -> Vec<Vec<&TextSpan>> {
    let mut ordered: Vec<&TextSpan> = spans.iter().collect();
    ordered.sort_by(|a, b| {
        let center_a = a.y + a.height / 2.0;
        let center_b = b.y + b.height / 2.0;
        center_b
            .partial_cmp(&center_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut heights: Vec<f64> = ordered.iter().map(|span| span.height).collect();
    heights.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median = heights[heights.len() / 2];
    let mut lines: Vec<Vec<&TextSpan>> = Vec::new();
    for span in ordered {
        let center = span.y + span.height / 2.0;
        let continues = lines.last().is_some_and(|line: &Vec<&TextSpan>| {
            let top = line[0].y + line[0].height / 2.0;
            (top - center).abs() <= LINE_SPLIT * median
        });
        match lines.last_mut() {
            Some(line) if continues => line.push(span),
            _ => lines.push(vec![span]),
        }
    }
    for line in &mut lines {
        line.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
    }
    lines
}

/// Joined text of spans, one space between words.
fn joined_text(spans: &[TextSpan]) -> String {
    spans
        .iter()
        .flat_map(|span| span.text.split_whitespace())
        .collect::<Vec<_>>()
        .join(" ")
}

/// One reclaimed cell: its column, its span count, and its member spans.
struct ReclaimedCell {
    column: u16,
    column_span: u16,
    spans: Vec<TextSpan>,
}

/// A run of adjacent table cells on one page with the geometry an edge row
/// must match: the marker and amount columns, the edge rows' outer sides,
/// and the table's width.
struct TableRun {
    start: usize,
    end: usize,
    first_row: u16,
    last_row: u16,
    first_col: u16,
    last_col: u16,
    marker: [f64; 4],
    amount: [f64; 4],
    top: f64,
    bottom: f64,
    extent: [f64; 4],
}

/// The run of cells holding `anchor`, or `None` where it is no table.
fn table_run(blocks: &[Block], anchor: usize) -> Option<TableRun> {
    if !matches!(blocks.get(anchor)?.kind, BlockKind::TableCell { .. }) {
        return None;
    }
    let page = blocks[anchor].page;
    let mut start = anchor;
    while start > 0
        && matches!(blocks[start - 1].kind, BlockKind::TableCell { .. })
        && blocks[start - 1].page == page
    {
        start -= 1;
    }
    let mut end = anchor;
    while end + 1 < blocks.len()
        && matches!(blocks[end + 1].kind, BlockKind::TableCell { .. })
        && blocks[end + 1].page == page
    {
        end += 1;
    }
    if end + 1 - start < MIN_RUN_CELLS {
        return None;
    }
    let mut rows = Vec::new();
    for block in &blocks[start..=end] {
        if let BlockKind::TableCell { row, column, .. } = block.kind {
            rows.push((row, column, block.bbox));
        }
    }
    let first_row = rows.iter().map(|cell| cell.0).min()?;
    let last_row = rows.iter().map(|cell| cell.0).max()?;
    let first_col = rows.iter().map(|cell| cell.1).min()?;
    let last_col = rows.iter().map(|cell| cell.1).max()?;
    let column_box = |column: u16| {
        rows.iter()
            .filter(|cell| cell.1 == column)
            .map(|cell| cell.2)
            .reduce(|seen, bbox| {
                [
                    seen[0].min(bbox[0]),
                    seen[1].min(bbox[1]),
                    seen[2].max(bbox[2]),
                    seen[3].max(bbox[3]),
                ]
            })
    };
    let top = rows
        .iter()
        .filter(|cell| cell.0 == first_row)
        .map(|cell| cell.2[3])
        .reduce(f64::max)?;
    let bottom = rows
        .iter()
        .filter(|cell| cell.0 == last_row)
        .map(|cell| cell.2[1])
        .reduce(f64::min)?;
    let extent = rows.iter().map(|cell| cell.2).reduce(|seen, bbox| {
        [
            seen[0].min(bbox[0]),
            seen[1].min(bbox[1]),
            seen[2].max(bbox[2]),
            seen[3].max(bbox[3]),
        ]
    })?;
    Some(TableRun {
        start,
        end,
        first_row,
        last_row,
        first_col,
        last_col,
        marker: column_box(first_col)?,
        amount: column_box(last_col)?,
        top,
        bottom,
        extent,
    })
}

/// Whether a ruled entry box stands around the amount span: the span's box,
/// grown by a hair, meets at least two small rulings -- the box's own sides.
/// Long rules never count.
fn has_entry_box(rulings: &[Ruling], amount: [f64; 4]) -> bool {
    let grown = [
        amount[0] - 3.0,
        amount[1] - 3.0,
        amount[2] + 3.0,
        amount[3] + 3.0,
    ];
    rulings
        .iter()
        .filter(|ruling| {
            let width = (ruling.x1 - ruling.x0).abs();
            let height = (ruling.y1 - ruling.y0).abs();
            width.max(height) <= 44.0
                && overlaps(
                    ruling.x0.min(ruling.x1),
                    ruling.x0.max(ruling.x1),
                    grown[0],
                    grown[2],
                    0.0,
                )
                && overlaps(
                    ruling.y0.min(ruling.y1),
                    ruling.y0.max(ruling.y1),
                    grown[1],
                    grown[3],
                    0.0,
                )
        })
        .count()
        >= 2
}

/// Whether a checkbox ruling stands on the line: a small ruling overlapping
/// the line's band inside the table's width.
fn has_checkbox(rulings: &[Ruling], line_box: [f64; 4], extent: [f64; 4]) -> bool {
    rulings.iter().any(|ruling| {
        let left = ruling.x0.min(ruling.x1);
        let right = ruling.x0.max(ruling.x1);
        let bottom = ruling.y0.min(ruling.y1);
        let top = ruling.y0.max(ruling.y1);
        let side = (right - left).max(top - bottom);
        (CHECKBOX_MIN..=CHECKBOX_SIDE).contains(&side)
            && overlaps(bottom, top, line_box[1], line_box[3], 0.0)
            && overlaps(left, right, extent[0], extent[2], 0.0)
    })
}

/// Index of the line's amount span: a short span past the marker whose middle
/// stands in the table's amount column.
fn amount_index(line: &[&TextSpan], run: &TableRun) -> Option<usize> {
    line.iter()
        .enumerate()
        .skip(1)
        .find(|(_, span)| {
            let center = span.x + span.width / 2.0;
            center >= run.amount[0] - COLUMN_SLACK
                && center <= run.amount[2] + COLUMN_SLACK
                && span.width <= AMOUNT_WIDTH
        })
        .map(|(index, _)| index)
}

/// Whether the line's leftmost span is a marker in the table's marker column.
fn marker_at(line: &[&TextSpan], run: &TableRun) -> Option<usize> {
    line.iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
        .and_then(|(index, span)| {
            let center = span.x + span.width / 2.0;
            (is_marker(&span.text)
                && center >= run.marker[0] - COLUMN_SLACK
                && center <= run.marker[2] + COLUMN_SLACK)
                .then_some(index)
        })
}

/// Plan the paragraph's rows against the table, or `None` where the
/// paragraph stays one.
fn plan_rows(
    paragraph: &Block,
    run: &TableRun,
    rulings: &[Ruling],
) -> Option<Vec<Vec<ReclaimedCell>>> {
    let lines = visual_lines(&paragraph.spans);
    if lines.iter().any(Vec::is_empty) {
        return None;
    }
    if lines.iter().any(|line| marker_at(line, run).is_none()) {
        return None;
    }
    let widths = run.last_col - run.first_col;
    if lines.len() == 1 && amount_index(&lines[0], run).is_none() {
        // A single checked-option line over the table's checkbox rulings
        // joins as one colspan row; any other single line without an amount
        // stays out.
        let line_box = union_box(&lines[0]);
        if !has_checkbox(rulings, line_box, run.extent) {
            return None;
        }
        return Some(vec![vec![ReclaimedCell {
            column: run.first_col,
            column_span: widths + 1,
            spans: lines[0].iter().map(|span| (*span).clone()).collect(),
        }]]);
    }
    // Every amount line reads as marker, stub and amount cells, every other
    // line as marker and stub cells, and at least one line carries an amount
    // -- otherwise wrapped prose only happens to open with markers.
    if lines.iter().all(|line| amount_index(line, run).is_none()) {
        return None;
    }
    let mut rows = Vec::new();
    for line in &lines {
        let marker = marker_at(line, run)?;
        let mut cells = Vec::new();
        match amount_index(line, run) {
            Some(amount) => {
                // The amount stands in a ruled entry box, not merely under
                // a column number that lines up.
                if !has_entry_box(rulings, span_box(line[amount])) {
                    return None;
                }
                let mut head: Vec<TextSpan> = vec![line[marker].clone()];
                let stub: Vec<TextSpan> = line
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| *index != marker && *index != amount)
                    .map(|(_, span)| (*span).clone())
                    .collect();
                if widths >= 2 {
                    cells.push(ReclaimedCell {
                        column: run.first_col,
                        column_span: 1,
                        spans: std::mem::take(&mut head),
                    });
                    cells.push(ReclaimedCell {
                        column: run.first_col + 1,
                        column_span: widths - 1,
                        spans: stub,
                    });
                } else {
                    head.extend(stub);
                    cells.push(ReclaimedCell {
                        column: run.first_col,
                        column_span: widths + 1,
                        spans: head,
                    });
                }
                cells.push(ReclaimedCell {
                    column: run.last_col,
                    column_span: 1,
                    spans: vec![line[amount].clone()],
                });
            }
            None => {
                let stub: Vec<TextSpan> = line
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| *index != marker)
                    .map(|(_, span)| (*span).clone())
                    .collect();
                if widths >= 1 {
                    cells.push(ReclaimedCell {
                        column: run.first_col,
                        column_span: 1,
                        spans: vec![line[marker].clone()],
                    });
                    cells.push(ReclaimedCell {
                        column: run.first_col + 1,
                        column_span: widths,
                        spans: stub,
                    });
                } else {
                    let mut spans = vec![line[marker].clone()];
                    spans.extend(stub);
                    cells.push(ReclaimedCell {
                        column: run.first_col,
                        column_span: 1,
                        spans,
                    });
                }
            }
        }
        rows.push(cells);
    }
    Some(rows)
}

/// Whether the paragraph touches the run's edge closely enough, and across
/// enough of the table's width, to be its row.
fn adjacent(paragraph: &Block, run: &TableRun, blocks: &[Block], index: usize) -> bool {
    if paragraph.page != blocks[run.start].page {
        return false;
    }
    let gap = if index + 1 == run.start {
        paragraph.bbox[1] - run.top
    } else if index == run.end + 1 {
        run.bottom - paragraph.bbox[3]
    } else {
        return false;
    };
    if gap < 0.0 {
        return false;
    }
    if gap > EDGE_GAP {
        return false;
    }
    let width = paragraph.bbox[2] - paragraph.bbox[0];
    overlaps(
        paragraph.bbox[0],
        paragraph.bbox[2],
        run.extent[0],
        run.extent[2],
        WIDTH_OVERLAP * width,
    )
}

/// Reclaim the marker-led edge rows of already accepted tables.
///
/// Runs after the learned pass, over its blocks: the judge, the verifier and
/// the column guards are untouched, and only a paragraph directly above or
/// below an accepted table's edge can join it. Repeats to a fixpoint so a
/// stack of edge rows joins one row at a time, each against the table the
/// row before already widened.
pub(crate) fn reclaim_table_edge_rows(blocks: &mut Vec<Block>, lattices: &[Vec<Ruling>]) {
    loop {
        let mut at = None;
        for (index, block) in blocks.iter().enumerate() {
            if !matches!(block.kind, BlockKind::Paragraph) {
                continue;
            }
            let anchor = if index + 1 < blocks.len()
                && matches!(blocks[index + 1].kind, BlockKind::TableCell { .. })
            {
                index + 1
            } else if index > 0 && matches!(blocks[index - 1].kind, BlockKind::TableCell { .. }) {
                index - 1
            } else {
                continue;
            };
            let Some(run) = table_run(blocks, anchor) else {
                continue;
            };

            if !adjacent(block, &run, blocks, index) {
                continue;
            }
            let rulings: &[Ruling] = lattices.get(block.page).map(Vec::as_slice).unwrap_or(&[]);
            let Some(rows) = plan_rows(block, &run, rulings) else {
                continue;
            };
            if rows
                .iter()
                .flatten()
                .any(|cell| joined_text(&cell.spans).is_empty())
            {
                continue;
            }
            at = Some((index, run, rows));
            break;
        }
        let Some((index, run, rows)) = at else {
            return;
        };
        let paragraph = blocks.remove(index);
        // The run slides one slot down the vector past a removed paragraph
        // above it; below it nothing moves.
        let prepend = index + 1 == run.start;
        let start = if prepend { run.start - 1 } else { run.start };
        let end = if prepend { run.end - 1 } else { run.end };
        // Prepending above row zero shifts the run down first.
        let shift = (rows.len() as u16).saturating_sub(run.first_row);
        let shift = if prepend { shift } else { 0 };
        if shift > 0 {
            for block in &mut blocks[start..=end] {
                if let BlockKind::TableCell { row, .. } = &mut block.kind {
                    *row += shift;
                }
            }
        }
        let base = if prepend {
            run.first_row + shift - rows.len() as u16
        } else {
            run.last_row + 1
        };
        // u16 rows cannot collide: prepending never crosses zero after the
        // shift, appending never crosses the ceiling on a real page.
        let mut at = if prepend { start } else { end + 1 };
        for (offset, cells) in rows.iter().enumerate() {
            for cell in cells {
                let text = joined_text(&cell.spans);
                let bbox = union_box(&cell.spans.iter().collect::<Vec<_>>());
                let mut block = Block::new(
                    BlockKind::TableCell {
                        row: base + offset as u16,
                        column: cell.column,
                        row_span: 1,
                        column_span: cell.column_span,
                        header: false,
                    },
                    paragraph.page,
                    bbox,
                    text,
                    at,
                );
                block.spans = cell.spans.clone();
                blocks.insert(at, block);
                at += 1;
            }
        }
        for (position, block) in blocks.iter_mut().enumerate() {
            block.index = position;
        }
    }
}
