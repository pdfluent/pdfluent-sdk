//! Table detection from PDF text blocks.
//!
//! Uses spatial analysis to detect tabular data: text blocks at aligned
//! x-positions across multiple lines form columns; lines at similar
//! y-positions form rows.

use pdf_extract::TextBlock;

/// Tolerance for grouping text blocks into lines (points).
const LINE_Y_TOLERANCE: f64 = 2.0;

/// Tolerance for column alignment (points).
const COLUMN_X_TOLERANCE: f64 = 5.0;

/// A detected table from a PDF page.
#[derive(Debug, Clone)]
pub struct DetectedTable {
    /// Table rows, each row is a vector of cell values.
    pub rows: Vec<Vec<CellValue>>,
    /// Number of columns.
    pub col_count: usize,
    /// Page number (1-based).
    pub page: u32,
}

/// A cell value with type detection.
#[derive(Debug, Clone)]
pub enum CellValue {
    /// Plain text.
    Text(String),
    /// Numeric value (parsed from text).
    Number(f64),
    /// Empty cell.
    Empty,
}

impl CellValue {
    /// Try to parse a string as a number, currency, or percentage.
    pub fn from_text(s: &str) -> Self {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return CellValue::Empty;
        }

        // Strip common currency symbols and thousand separators.
        let cleaned: String = trimmed
            .replace(['$', '€', '£', '¥', ','], "")
            .trim()
            .to_string();

        // Handle percentages.
        if let Some(pct) = cleaned.strip_suffix('%') {
            if let Ok(v) = pct.trim().parse::<f64>() {
                return CellValue::Number(v / 100.0);
            }
        }

        // Handle negative numbers in parentheses: (123.45) → -123.45
        if cleaned.starts_with('(') && cleaned.ends_with(')') {
            let inner = &cleaned[1..cleaned.len() - 1];
            if let Ok(v) = inner.parse::<f64>() {
                return CellValue::Number(-v);
            }
        }

        if let Ok(v) = cleaned.parse::<f64>() {
            return CellValue::Number(v);
        }

        CellValue::Text(trimmed.to_string())
    }

    /// Get the display text for this cell.
    pub fn as_text(&self) -> String {
        match self {
            CellValue::Text(s) => s.clone(),
            CellValue::Number(n) => n.to_string(),
            CellValue::Empty => String::new(),
        }
    }
}

/// A line of text blocks at roughly the same y-coordinate.
#[derive(Debug)]
struct Line {
    y: f64,
    blocks: Vec<TextBlock>,
}

/// Detect all tables on a page from text blocks.
pub fn detect_tables(blocks: &[TextBlock], page: u32) -> Vec<DetectedTable> {
    if blocks.is_empty() {
        return Vec::new();
    }

    let lines = group_into_lines(blocks);

    // Find column positions across all lines.
    let columns = find_columns(&lines);
    if columns.len() < 2 {
        return Vec::new();
    }

    // Split lines into table regions (contiguous runs of multi-column lines).
    let table_regions = find_table_regions(&lines, &columns);

    table_regions
        .into_iter()
        .map(|region| {
            let rows = region
                .iter()
                .map(|line| map_line_to_cells(line, &columns))
                .collect();
            DetectedTable {
                rows,
                col_count: columns.len(),
                page,
            }
        })
        .collect()
}

/// Group text blocks into lines based on y-coordinate proximity.
fn group_into_lines(blocks: &[TextBlock]) -> Vec<Line> {
    let mut sorted: Vec<&TextBlock> = blocks.iter().collect();
    sorted.sort_by(|a, b| {
        b.bbox[1]
            .partial_cmp(&a.bbox[1])
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut lines: Vec<Line> = Vec::new();

    for block in sorted {
        let y = block.bbox[1];
        let matched = lines
            .iter_mut()
            .find(|line| (line.y - y).abs() < LINE_Y_TOLERANCE);

        if let Some(line) = matched {
            line.blocks.push(block.clone());
        } else {
            lines.push(Line {
                y,
                blocks: vec![block.clone()],
            });
        }
    }

    for line in &mut lines {
        line.blocks.sort_by(|a, b| {
            a.bbox[0]
                .partial_cmp(&b.bbox[0])
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    lines
}

/// Find column x-positions from the text blocks across all lines.
fn find_columns(lines: &[Line]) -> Vec<f64> {
    let mut x_positions: Vec<f64> = Vec::new();

    for line in lines {
        for block in &line.blocks {
            let x = block.bbox[0];
            if !x_positions
                .iter()
                .any(|&px| (px - x).abs() < COLUMN_X_TOLERANCE)
            {
                x_positions.push(x);
            }
        }
    }

    x_positions.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    x_positions
}

/// Find contiguous regions of lines that have data in multiple columns.
fn find_table_regions<'a>(lines: &'a [Line], columns: &[f64]) -> Vec<Vec<&'a Line>> {
    let mut regions: Vec<Vec<&'a Line>> = Vec::new();
    let mut current_region: Vec<&Line> = Vec::new();

    for line in lines {
        let col_hits = count_column_hits(line, columns);
        if col_hits >= 2 {
            current_region.push(line);
        } else if current_region.len() >= 2 {
            regions.push(std::mem::take(&mut current_region));
        } else {
            current_region.clear();
        }
    }

    if current_region.len() >= 2 {
        regions.push(current_region);
    }

    regions
}

/// Count how many distinct columns a line has content in.
fn count_column_hits(line: &Line, columns: &[f64]) -> usize {
    let mut hits = std::collections::HashSet::new();
    for block in &line.blocks {
        if let Some(col) = columns
            .iter()
            .position(|&px| (px - block.bbox[0]).abs() < COLUMN_X_TOLERANCE)
        {
            hits.insert(col);
        }
    }
    hits.len()
}

/// Map a line's text blocks to cell values based on column positions.
fn map_line_to_cells(line: &Line, columns: &[f64]) -> Vec<CellValue> {
    let mut cells = vec![CellValue::Empty; columns.len()];

    for block in &line.blocks {
        let col_idx = columns
            .iter()
            .position(|&px| (px - block.bbox[0]).abs() < COLUMN_X_TOLERANCE)
            .unwrap_or(0);

        let existing = &cells[col_idx];
        let new_text = match existing {
            CellValue::Empty => block.text.clone(),
            CellValue::Text(s) => format!("{} {}", s, block.text),
            CellValue::Number(_) => block.text.clone(),
        };
        cells[col_idx] = CellValue::from_text(&new_text);
    }

    cells
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_block(text: &str, x: f64, y: f64) -> TextBlock {
        TextBlock {
            text: text.to_string(),
            page: 1,
            bbox: [x, y, x + text.len() as f64 * 6.0, y + 12.0],
            font_name: "F1".to_string(),
            font_size: 12.0,
        }
    }

    #[test]
    fn detect_simple_table() {
        let blocks = vec![
            make_block("Name", 72.0, 700.0),
            make_block("Age", 200.0, 700.0),
            make_block("Alice", 72.0, 684.0),
            make_block("30", 200.0, 684.0),
            make_block("Bob", 72.0, 668.0),
            make_block("25", 200.0, 668.0),
        ];

        let tables = detect_tables(&blocks, 1);
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].col_count, 2);
        assert_eq!(tables[0].rows.len(), 3);
    }

    #[test]
    fn numeric_detection() {
        assert!(matches!(CellValue::from_text("42"), CellValue::Number(_)));
        assert!(matches!(CellValue::from_text("3.14"), CellValue::Number(_)));
        assert!(matches!(
            CellValue::from_text("$1,000"),
            CellValue::Number(_)
        ));
        assert!(matches!(CellValue::from_text("50%"), CellValue::Number(_)));
        assert!(matches!(
            CellValue::from_text("(100)"),
            CellValue::Number(_)
        ));
        assert!(matches!(CellValue::from_text("Hello"), CellValue::Text(_)));
        assert!(matches!(CellValue::from_text(""), CellValue::Empty));
    }

    #[test]
    fn no_table_from_single_column() {
        let blocks = vec![
            make_block("Line 1", 72.0, 700.0),
            make_block("Line 2", 72.0, 684.0),
        ];
        let tables = detect_tables(&blocks, 1);
        assert!(tables.is_empty());
    }

    #[test]
    fn empty_blocks() {
        let tables = detect_tables(&[], 1);
        assert!(tables.is_empty());
    }

    #[test]
    fn currency_parsing() {
        if let CellValue::Number(v) = CellValue::from_text("$1,234.56") {
            assert!((v - 1234.56).abs() < 0.01);
        } else {
            panic!("Expected Number");
        }
    }

    #[test]
    fn percentage_parsing() {
        if let CellValue::Number(v) = CellValue::from_text("75%") {
            assert!((v - 0.75).abs() < 0.001);
        } else {
            panic!("Expected Number");
        }
    }

    #[test]
    fn negative_parentheses() {
        if let CellValue::Number(v) = CellValue::from_text("(500)") {
            assert!((v - (-500.0)).abs() < 0.01);
        } else {
            panic!("Expected Number");
        }
    }
}
