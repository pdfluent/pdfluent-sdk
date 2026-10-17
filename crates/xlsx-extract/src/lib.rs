#![warn(missing_docs)]
//! XLSX workbook extraction onto [`pdf_engine`](pdf_engine) blocks.
//!
//! Each sheet becomes a level-1 heading (the tab name) followed by one table
//! block run over the used range, so [`pdf_engine::to_markdown`] renders the
//! workbook as headings plus pipe tables (or HTML tables where merged cells
//! need `colspan`/`rowspan`). Sheet *i* lands on page *i*.
//!
//! What each cell shows:
//!
//! * shared strings, inline strings and error literals as themselves;
//! * booleans as `TRUE`/`FALSE`, numbers in general format;
//! * date-styled numbers as ISO dates (`YYYY-MM-DD`, or
//!   `YYYY-MM-DDTHH:MM:SS` where the format carries a clock time);
//! * formula cells show their CACHED value — the `<v>` Excel wrote when it
//!   last calculated. This crate never recalculates and never claims to;
//!   the formula text itself is available on [`SheetBlocks::formulas`]
//!   because [`Block`](pdf_engine::Block) carries no metadata field for it.
//!
//! Empty trailing rows and columns are skipped: the used range is derived
//! from the cells, not from the `dimension` attribute writers leave behind.
//! Each sheet is capped at [`MAX_CELLS_PER_SHEET`] cells; past it the table
//! is cut to whole rows and a truncation paragraph follows it.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

pub mod error;
mod workbook;

pub use error::{Result, XlsxExtractError};
use workbook::parse_workbook;

use pdf_engine::{Block, BlockKind};

/// Default cap on the cells emitted per sheet (used-range rows × columns).
///
/// A full-column accident (`A:A` formatted to row 1048576) would otherwise
/// turn one sheet into a million-block table. Past the cap the table keeps
/// its leading whole rows and a truncation note follows it.
pub const MAX_CELLS_PER_SHEET: usize = 200_000;

/// Extraction limits. Today only the per-sheet cell cap.
#[derive(Debug, Clone)]
pub struct ExtractLimits {
    /// Maximum used-range cells emitted per sheet before truncation.
    pub max_cells_per_sheet: usize,
}

impl Default for ExtractLimits {
    fn default() -> Self {
        Self {
            max_cells_per_sheet: MAX_CELLS_PER_SHEET,
        }
    }
}

/// One formula cell: where it is, what it says, what it showed.
///
/// `row` and `column` are 0-based. `cached` is the value Excel last
/// calculated — this crate reads it, it never recomputes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormulaCell {
    /// The sheet (tab name) holding the formula.
    pub sheet: String,
    /// 0-based row.
    pub row: u32,
    /// 0-based column.
    pub column: u32,
    /// The formula text without a leading `=`, as stored in `<f>`.
    pub formula: String,
    /// The cached display value shown in the cell's block.
    pub cached: String,
}

/// One sheet's extraction: its blocks plus its formulas.
#[derive(Debug, Clone)]
pub struct SheetBlocks {
    /// The tab name (also the heading block's text).
    pub name: String,
    /// Heading, table cells in row-major order, and any truncation note.
    pub blocks: Vec<Block>,
    /// Every formula cell on the sheet, in row-major order.
    pub formulas: Vec<FormulaCell>,
    /// Whether the cell cap cut the table short.
    pub truncated: bool,
}

/// A whole workbook's extraction: one [`SheetBlocks`] per sheet, in order.
#[derive(Debug, Clone)]
pub struct WorkbookBlocks {
    /// Sheets in workbook order.
    pub sheets: Vec<SheetBlocks>,
}

impl WorkbookBlocks {
    /// Every sheet's blocks in order, with document-wide reading indices.
    pub fn blocks(&self) -> Vec<Block> {
        self.sheets
            .iter()
            .flat_map(|sheet| sheet.blocks.iter().cloned())
            .collect()
    }
}

/// Extract an `.xlsx` file's bytes to engine blocks, one table per sheet.
///
/// Shorthand for [`extract_workbook`] followed by [`WorkbookBlocks::blocks`].
pub fn extract_blocks(data: &[u8]) -> Result<Vec<Block>> {
    Ok(extract_workbook(data)?.blocks())
}

/// Extract with explicit [`ExtractLimits`].
pub fn extract_blocks_with_limits(data: &[u8], limits: &ExtractLimits) -> Result<Vec<Block>> {
    Ok(extract_workbook_with_limits(data, limits)?.blocks())
}

/// Extract an `.xlsx` file's bytes to per-sheet blocks and formula metadata.
pub fn extract_workbook(data: &[u8]) -> Result<WorkbookBlocks> {
    extract_workbook_with_limits(data, &ExtractLimits::default())
}

/// Extract with explicit [`ExtractLimits`].
pub fn extract_workbook_with_limits(data: &[u8], limits: &ExtractLimits) -> Result<WorkbookBlocks> {
    let workbook = parse_workbook(data)?;
    let mut sheets = Vec::with_capacity(workbook.sheets.len());
    let mut index = 0usize;

    for (page, sheet) in workbook.sheets.iter().enumerate() {
        let mut blocks = vec![Block::new(
            BlockKind::Heading { level: 1 },
            page,
            [0.0; 4],
            sheet.name.clone(),
            index,
        )];
        index += 1;

        let (rows, cols) = used_range(sheet);
        let (kept_rows, truncated) = if rows == 0 || cols == 0 {
            (0, false)
        } else {
            apply_cap(rows, cols, limits.max_cells_per_sheet)
        };

        let mut formulas = Vec::new();
        for ((row, col), cell) in sheet.cells.iter() {
            if *row >= kept_rows as u32 {
                continue;
            }
            if let Some(formula) = &cell.formula {
                formulas.push(FormulaCell {
                    sheet: sheet.name.clone(),
                    row: *row,
                    column: *col,
                    formula: formula.clone(),
                    cached: cell.text.clone(),
                });
            }
        }

        for row in 0..kept_rows {
            for col in 0..cols {
                let row = row as u32;
                let col = col as u32;
                let Some((row_span, col_span)) = span_at(&sheet.merges, row, col, kept_rows as u32)
                else {
                    // Covered by a merge whose head sits elsewhere: no block,
                    // so the table run stays a clean row-major sequence.
                    continue;
                };
                let text = sheet
                    .cells
                    .get(&(row, col))
                    .map_or("", |cell| cell.text.as_str());
                blocks.push(Block::new(
                    BlockKind::TableCell {
                        row: row as u16,
                        column: col as u16,
                        row_span: row_span as u16,
                        column_span: col_span as u16,
                        header: row == 0,
                    },
                    page,
                    [0.0; 4],
                    text.to_string(),
                    index,
                ));
                index += 1;
            }
        }

        if truncated {
            blocks.push(Block::new(
                BlockKind::Paragraph,
                page,
                [0.0; 4],
                format!(
                    "Table truncated: showing the first {kept_rows} of {rows} rows ({} cell limit).",
                    limits.max_cells_per_sheet
                ),
                index,
            ));
            index += 1;
        }

        sheets.push(SheetBlocks {
            name: sheet.name.clone(),
            blocks,
            formulas,
            truncated,
        });
    }

    Ok(WorkbookBlocks { sheets })
}

/// The used range as `(rows, columns)`: one past the last row and column
/// holding a cell or touched by a merge. Trailing emptiness falls away
/// because nothing beyond content is counted.
fn used_range(sheet: &workbook::Sheet) -> (usize, usize) {
    let mut rows: usize = 0;
    let mut cols: usize = 0;
    for ((row, col), _) in sheet.cells.iter() {
        rows = rows.max(*row as usize + 1);
        cols = cols.max(*col as usize + 1);
    }
    for &(_, (last_row, last_col)) in sheet.merges.iter() {
        rows = rows.max(last_row as usize + 1);
        cols = cols.max(last_col as usize + 1);
    }
    (rows, cols)
}

/// How many leading rows survive the cap, and whether anything was cut.
///
/// The grid keeps all its columns and loses whole trailing rows, so what
/// remains is still rectangular. `u16` is a second, silent cap: [`BlockKind::TableCell`]
/// counts rows in `u16`, and a sheet taller than 65535 rows cannot be said.
fn apply_cap(rows: usize, cols: usize, max_cells: usize) -> (usize, bool) {
    let addressable = rows.min(u16::MAX as usize);
    if cols == 0 {
        return (0, false);
    }
    let capped = if addressable.saturating_mul(cols) > max_cells {
        (max_cells / cols).max(1).min(addressable)
    } else {
        addressable
    };
    (capped, capped < rows)
}

/// The span emitted at `(row, col)`: the merge head's extent clamped to the
/// emitted rows, `(1, 1)` for an ordinary cell, or `None` where a merge
/// covers the cell from a head elsewhere.
fn span_at(
    merges: &[workbook::MergeRange],
    row: u32,
    col: u32,
    kept_rows: u32,
) -> Option<(u32, u32)> {
    for &((first_row, first_col), (last_row, last_col)) in merges {
        if row < first_row || row > last_row || col < first_col || col > last_col {
            continue;
        }
        if row == first_row && col == first_col {
            let clamped_last = last_row.min(kept_rows.saturating_sub(1));
            return Some((clamped_last - first_row + 1, last_col - first_col + 1));
        }
        return None;
    }
    Some((1, 1))
}
