//! Spatial grouping of text blocks into lines, paragraphs, and tables.

use pdf_extract::TextBlock;

/// Tolerance for grouping text blocks into lines (points).
const LINE_Y_TOLERANCE: f64 = 2.0;

/// Vertical gap threshold for paragraph breaks (fraction of font size).
const PARAGRAPH_GAP_FACTOR: f64 = 1.5;

/// Tolerance for column alignment in table detection (points).
const TABLE_X_TOLERANCE: f64 = 5.0;

/// A run of text with formatting.
#[derive(Debug, Clone)]
pub struct Run {
    pub text: String,
    pub font_name: String,
    pub font_size: f64,
    pub bold: bool,
    pub italic: bool,
}

/// A paragraph composed of one or more runs.
#[derive(Debug, Clone)]
pub struct Paragraph {
    pub runs: Vec<Run>,
}

/// A table with rows and columns.
#[derive(Debug, Clone)]
pub struct Table {
    pub rows: Vec<Vec<String>>,
    pub col_count: usize,
}

/// An image to include in the document.
#[derive(Debug, Clone)]
pub struct DocxImage {
    pub data: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub content_type: String,
    pub id: String,
}

/// Page content after layout analysis.
#[derive(Debug, Clone)]
pub enum PageElement {
    Para(Paragraph),
    Tbl(Table),
    Img(DocxImage),
}

/// A line of text (blocks at roughly the same y-coordinate).
#[derive(Debug)]
struct Line {
    y: f64,
    font_size: f64,
    blocks: Vec<TextBlock>,
}

/// Analyze text blocks from a page and group them into paragraphs and tables.
pub fn analyze_page(blocks: &[TextBlock]) -> Vec<PageElement> {
    if blocks.is_empty() {
        return Vec::new();
    }

    let lines = group_into_lines(blocks);
    let table = try_detect_table(&lines);

    if let Some(tbl) = table {
        return vec![PageElement::Tbl(tbl)];
    }

    group_into_paragraphs(&lines)
}

/// Group text blocks into lines based on y-coordinate proximity.
fn group_into_lines(blocks: &[TextBlock]) -> Vec<Line> {
    let mut sorted: Vec<&TextBlock> = blocks.iter().collect();
    // Sort by y descending (PDF origin is bottom-left), then x ascending.
    sorted.sort_by(|a, b| {
        let y_cmp = b.bbox[1]
            .partial_cmp(&a.bbox[1])
            .unwrap_or(std::cmp::Ordering::Equal);
        if y_cmp == std::cmp::Ordering::Equal {
            a.bbox[0]
                .partial_cmp(&b.bbox[0])
                .unwrap_or(std::cmp::Ordering::Equal)
        } else {
            y_cmp
        }
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
                font_size: block.font_size,
                blocks: vec![block.clone()],
            });
        }
    }

    // Sort each line's blocks by x-coordinate.
    for line in &mut lines {
        line.blocks.sort_by(|a, b| {
            a.bbox[0]
                .partial_cmp(&b.bbox[0])
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    lines
}

/// Try to detect a table from aligned text lines.
///
/// A table is detected when multiple lines share the same column structure
/// (i.e., text blocks start at similar x-positions across lines).
fn try_detect_table(lines: &[Line]) -> Option<Table> {
    if lines.len() < 2 {
        return None;
    }

    // Collect all unique x-positions across all lines.
    let mut x_positions: Vec<f64> = Vec::new();
    for line in lines {
        for block in &line.blocks {
            let x = block.bbox[0];
            if !x_positions
                .iter()
                .any(|&px| (px - x).abs() < TABLE_X_TOLERANCE)
            {
                x_positions.push(x);
            }
        }
    }
    x_positions.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    if x_positions.len() < 2 {
        return None;
    }

    // Check if most lines have blocks at multiple column positions.
    let multi_col_lines = lines
        .iter()
        .filter(|line| {
            let unique_cols = line
                .blocks
                .iter()
                .map(|b| {
                    x_positions
                        .iter()
                        .position(|&px| (px - b.bbox[0]).abs() < TABLE_X_TOLERANCE)
                        .unwrap_or(0)
                })
                .collect::<std::collections::HashSet<_>>();
            unique_cols.len() >= 2
        })
        .count();

    // At least 60% of lines need multiple columns for table detection.
    if multi_col_lines * 100 / lines.len() < 60 {
        return None;
    }

    let col_count = x_positions.len();
    let mut rows = Vec::new();

    for line in lines {
        let mut row = vec![String::new(); col_count];
        for block in &line.blocks {
            let col_idx = x_positions
                .iter()
                .position(|&px| (px - block.bbox[0]).abs() < TABLE_X_TOLERANCE)
                .unwrap_or(0);
            if !row[col_idx].is_empty() {
                row[col_idx].push(' ');
            }
            row[col_idx].push_str(&block.text);
        }
        rows.push(row);
    }

    Some(Table { rows, col_count })
}

/// Group lines into paragraphs based on vertical spacing.
fn group_into_paragraphs(lines: &[Line]) -> Vec<PageElement> {
    let mut elements = Vec::new();
    let mut current_runs: Vec<Run> = Vec::new();
    let mut prev_y: Option<f64> = None;
    let mut prev_font_size: f64 = 12.0;

    for line in lines {
        let line_text: String = line
            .blocks
            .iter()
            .map(|b| b.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        if line_text.trim().is_empty() {
            continue;
        }

        let is_new_paragraph = if let Some(py) = prev_y {
            let gap = (py - line.y).abs();
            gap > prev_font_size * PARAGRAPH_GAP_FACTOR
        } else {
            false
        };

        if is_new_paragraph && !current_runs.is_empty() {
            elements.push(PageElement::Para(Paragraph {
                runs: std::mem::take(&mut current_runs),
            }));
        }

        let font_name = line
            .blocks
            .first()
            .map(|b| b.font_name.clone())
            .unwrap_or_default();
        let font_size = line.font_size;

        let bold = font_name.contains("Bold") || font_name.contains("bold");
        let italic = font_name.contains("Italic")
            || font_name.contains("italic")
            || font_name.contains("Oblique");

        current_runs.push(Run {
            text: line_text,
            font_name,
            font_size,
            bold,
            italic,
        });

        prev_y = Some(line.y);
        prev_font_size = font_size;
    }

    if !current_runs.is_empty() {
        elements.push(PageElement::Para(Paragraph { runs: current_runs }));
    }

    elements
}

/// Detect bold/italic from a PDF font name.
pub fn map_font_name(pdf_font: &str) -> &str {
    // Strip common prefixes like "ABCDEF+" used in subset fonts.
    let name = if let Some(pos) = pdf_font.find('+') {
        &pdf_font[pos + 1..]
    } else {
        pdf_font
    };

    // Map common font families.
    if name.contains("Times") || name.contains("Serif") {
        "Times New Roman"
    } else if name.contains("Arial") || name.contains("Helvetica") || name.contains("Sans") {
        "Arial"
    } else if name.contains("Courier") || name.contains("Mono") {
        "Courier New"
    } else if name.contains("Symbol") {
        "Symbol"
    } else {
        "Calibri"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_block(text: &str, x: f64, y: f64, font_size: f64) -> TextBlock {
        TextBlock {
            text: text.to_string(),
            page: 1,
            bbox: [x, y, x + text.len() as f64 * font_size * 0.5, y + font_size],
            font_name: "F1".to_string(),
            font_size,
        }
    }

    #[test]
    fn single_line_becomes_paragraph() {
        let blocks = vec![make_block("Hello World", 72.0, 720.0, 12.0)];
        let elements = analyze_page(&blocks);
        assert_eq!(elements.len(), 1);
        assert!(matches!(elements[0], PageElement::Para(_)));
    }

    #[test]
    fn two_close_lines_same_paragraph() {
        let blocks = vec![
            make_block("Line 1", 72.0, 720.0, 12.0),
            make_block("Line 2", 72.0, 706.0, 12.0), // gap = 14, < 12*1.5=18
        ];
        let elements = analyze_page(&blocks);
        assert_eq!(elements.len(), 1);
    }

    #[test]
    fn two_distant_lines_different_paragraphs() {
        let blocks = vec![
            make_block("Para 1", 72.0, 720.0, 12.0),
            make_block("Para 2", 72.0, 680.0, 12.0), // gap = 40, > 18
        ];
        let elements = analyze_page(&blocks);
        assert_eq!(elements.len(), 2);
    }

    #[test]
    fn table_detection() {
        let blocks = vec![
            // Row 1
            make_block("Name", 72.0, 700.0, 12.0),
            make_block("Age", 200.0, 700.0, 12.0),
            // Row 2
            make_block("Alice", 72.0, 684.0, 12.0),
            make_block("30", 200.0, 684.0, 12.0),
            // Row 3
            make_block("Bob", 72.0, 668.0, 12.0),
            make_block("25", 200.0, 668.0, 12.0),
        ];
        let elements = analyze_page(&blocks);
        assert_eq!(elements.len(), 1);
        assert!(matches!(elements[0], PageElement::Tbl(_)));
        if let PageElement::Tbl(ref tbl) = elements[0] {
            assert_eq!(tbl.rows.len(), 3);
            assert_eq!(tbl.col_count, 2);
        }
    }

    #[test]
    fn empty_blocks_returns_empty() {
        let elements = analyze_page(&[]);
        assert!(elements.is_empty());
    }
}
