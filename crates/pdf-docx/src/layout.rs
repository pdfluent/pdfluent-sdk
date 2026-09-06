//! Spatial grouping of text blocks into lines, paragraphs, and tables.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_extract::TextBlock;

/// Tolerance for grouping text blocks into lines (points).
const LINE_Y_TOLERANCE: f64 = 2.0;

/// Vertical gap threshold for paragraph breaks (fraction of font size).
const PARAGRAPH_GAP_FACTOR: f64 = 1.5;

/// Tolerance for column alignment in table detection (points).
const TABLE_X_TOLERANCE: f64 = 5.0;

/// A run of text with consistent formatting — the smallest unit emitted to
/// the resulting DOCX.
///
/// A [`Paragraph`] is a sequence of one or more runs. A new run is started
/// whenever the layout detector observes a change in font, size, weight, or
/// style on the same line; consecutive characters with the same formatting
/// stay in a single run.
#[derive(Debug, Clone)]
pub struct Run {
    /// The actual text content of this run, in source order. May contain
    /// any UTF-8 characters extracted from the PDF page.
    pub text: String,
    /// PostScript name of the font as it appears in the PDF (e.g.
    /// `Helvetica`, `TimesNewRomanPS-BoldMT`). Mapped to a Word font name
    /// during DOCX writing.
    pub font_name: String,
    /// Font size in PDF user-space points. Persisted to DOCX as half-points
    /// (Word's native unit).
    pub font_size: f64,
    /// Whether the run is rendered bold. Detected from font name suffix
    /// (`-Bold`, `Bd`) or PDF font flags.
    pub bold: bool,
    /// Whether the run is rendered italic. Detected from font name suffix
    /// (`-Italic`, `It`, `Oblique`) or PDF font flags.
    pub italic: bool,
}

/// A paragraph composed of one or more [`Run`]s.
///
/// Produced by the line-grouping pass: lines whose vertical gap is below
/// [`PARAGRAPH_GAP_FACTOR`] times the font size are considered part of the
/// same paragraph; a larger gap ends the paragraph.
#[derive(Debug, Clone)]
pub struct Paragraph {
    /// The runs that make up this paragraph, in reading order. Empty
    /// paragraphs are valid and represent blank lines.
    pub runs: Vec<Run>,
}

/// A table reconstructed from text blocks aligned in columns.
///
/// Detected when consecutive lines share the same column x-coordinates
/// within [`TABLE_X_TOLERANCE`]. Rebuilt into a regular grid where each
/// row has the same number of cells (`col_count`); short rows are
/// right-padded with empty strings.
#[derive(Debug, Clone)]
pub struct Table {
    /// Row-major cell content. `rows[r][c]` is the text in column `c` of
    /// row `r`. All rows have length [`Self::col_count`].
    pub rows: Vec<Vec<String>>,
    /// Number of columns in the table — the maximum column count observed
    /// during column-alignment detection.
    pub col_count: usize,
}

/// An image to embed into the resulting DOCX document.
///
/// Produced when the layout pass identifies an image XObject on the page
/// that should be carried over to the Word document. The bytes are kept
/// verbatim; the DOCX writer wraps them in the appropriate `w:drawing`
/// element with the given dimensions.
#[derive(Debug, Clone)]
pub struct DocxImage {
    /// Raw image bytes in the format described by [`Self::content_type`]
    /// (typically PNG or JPEG).
    pub data: Vec<u8>,
    /// Image width in pixels. Used to compute the on-page rendered size.
    pub width: u32,
    /// Image height in pixels. Used to compute the on-page rendered size.
    pub height: u32,
    /// MIME type of [`Self::data`] — e.g. `image/png`, `image/jpeg`. Drives
    /// the part name and `Override` content-type entry in the DOCX `[Content_Types].xml`.
    pub content_type: String,
    /// Stable identifier used to deduplicate images that appear on multiple
    /// pages and to wire up the relationship reference in the DOCX.
    pub id: String,
}

/// One element in the per-page layout: a paragraph, a table, or an image.
///
/// Produced by the layout analysis pass. Pages emit a `Vec<PageElement>` in
/// reading order; the DOCX writer iterates these and produces matching
/// Word document parts.
#[derive(Debug, Clone)]
pub enum PageElement {
    /// A flowing paragraph of text — see [`Paragraph`].
    Para(Paragraph),
    /// A reconstructed table — see [`Table`].
    Tbl(Table),
    /// An embedded image — see [`DocxImage`].
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

/// A column is an x-position that recurs on several lines.
///
/// Not every distinct x. That was the bug: `col_count` was the number of
/// distinct x-positions across the whole block set, and in a real PDF almost
/// every word starts at its own x. A line of twenty words therefore produced
/// twenty columns -- roughly one word per cell, which leaves the file valid and
/// unusable. See #161.
///
/// What distinguishes a column from an accidental word position is recurrence:
/// text beginning at (nearly) the same x on several lines. An x that appears on
/// one line only is a word.
///
/// Two lines or 30% was still too little, and the end-to-end test said so: a
/// grid of four rows by two columns came back with six columns. In a real table
/// a cell holds words, and every word starts at its own x. Those word positions
/// shift only a few points from row to row, so within [`TABLE_X_TOLERANCE`] two
/// of them keep landing together -- which already satisfied the old threshold.
///
/// What separates a column from a word is therefore not *that* the x recurs but
/// *how often*: a column start recurs on nearly every line, a word position does
/// not. Hence 75%. Lower lets word positions through; much higher loses tables
/// in which a cell is occasionally empty.
const MIN_COLUMN_LINES: usize = 2;
const MIN_COLUMN_SUPPORT_PERCENT: usize = 75;

/// The x-positions that behave as a column, ordered left to right.
fn column_positions(lines: &[Line]) -> Vec<f64> {
    // Per candidate x: on how many distinct lines does text start there?
    let mut candidates: Vec<(f64, usize)> = Vec::new();
    for line in lines {
        let mut counted: Vec<usize> = Vec::new();
        for block in &line.blocks {
            let x = block.bbox[0];
            match candidates
                .iter()
                .position(|&(px, _)| (px - x).abs() < TABLE_X_TOLERANCE)
            {
                Some(i) => {
                    // One line counts at most once for the same column;
                    // otherwise a line with five words at the same x invents a
                    // column that is not there.
                    if !counted.contains(&i) {
                        candidates[i].1 += 1;
                        counted.push(i);
                    }
                }
                None => {
                    candidates.push((x, 1));
                    counted.push(candidates.len() - 1);
                }
            }
        }
    }

    // Rounded up, not down.
    //
    // At three lines `3 * 75 / 100` gave two, so a word position that happened
    // to coincide on two of the three lines still counted as a column. On a
    // small table every line is evidence, and the requirement must not evaporate
    // through integer division.
    let required = (lines.len() * MIN_COLUMN_SUPPORT_PERCENT).div_ceil(100);
    let threshold = std::cmp::max(MIN_COLUMN_LINES, required);
    let mut positions: Vec<f64> = candidates
        .into_iter()
        .filter(|&(_, support)| support >= threshold)
        .map(|(x, _)| x)
        .collect();
    positions.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    positions
}

/// The column this block belongs to: the nearest column that does not sit to
/// the right of the block.
///
/// Previously everything that did not start exactly on a column fell back to
/// column 0 (`unwrap_or(0)`), which dumped unaligned text from the whole line
/// into the first cell. Text belongs to the column it sits under.
fn column_for(x: f64, positions: &[f64]) -> usize {
    let mut best = 0;
    for (i, &px) in positions.iter().enumerate() {
        if px <= x + TABLE_X_TOLERANCE {
            best = i;
        } else {
            break;
        }
    }
    best
}

/// Try to detect a table from aligned text lines.
///
/// A table is detected when multiple lines share the same column structure
/// (i.e., text blocks start at similar x-positions across lines).
fn try_detect_table(lines: &[Line]) -> Option<Table> {
    if lines.len() < 2 {
        return None;
    }

    let x_positions = column_positions(lines);
    if x_positions.len() < 2 {
        return None;
    }

    // Do most lines carry text in more than one column?
    let multi_col_lines = lines
        .iter()
        .filter(|line| {
            line.blocks
                .iter()
                .map(|b| column_for(b.bbox[0], &x_positions))
                .collect::<std::collections::HashSet<_>>()
                .len()
                >= 2
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
            let col_idx = column_for(block.bbox[0], &x_positions);
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
            actual_text: None,
            base_font: None,
            is_bold: false,
            is_italic: false,
            color: None,
            width_source: Default::default(),
            char_bounds: vec![],
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

    // ---- column detection (#161) ----

    #[test]
    fn a_sentence_does_not_become_a_table_with_a_column_per_word() {
        // The symptom from #161, reduced to its core: ordinary lines of text in
        // which every word has its own x. Previously every distinct x became a
        // column, so you got roughly one word per cell -- a valid .docx nobody
        // can use.
        let mut blocks = Vec::new();
        for (row, y) in [(0, 700.0), (1, 684.0), (2, 668.0)] {
            // Words at ever-different x-positions, as in flowing text.
            for w in 0..8 {
                let x = 72.0 + (w as f64) * 37.0 + (row as f64) * 11.0;
                blocks.push(make_block("word", x, y, 12.0));
            }
        }
        let elements = analyze_page(&blocks);
        for el in &elements {
            if let PageElement::Tbl(t) = el {
                assert!(
                    t.col_count <= 4,
                    "flowing text became a table of {} columns; that is the \
                     symptom from #161",
                    t.col_count
                );
            }
        }
    }

    #[test]
    fn a_real_table_keeps_its_columns() {
        // The counter-test. Were the new requirement too strict, tables that do
        // exist would disappear -- a worse failure than too many columns, since
        // then the structure is gone entirely.
        let mut blocks = Vec::new();
        for y in [700.0_f64, 684.0, 668.0, 652.0] {
            blocks.push(make_block("left", 72.0, y, 12.0));
            blocks.push(make_block("middle", 200.0, y, 12.0));
            blocks.push(make_block("right", 340.0, y, 12.0));
        }
        let elements = analyze_page(&blocks);
        let table = elements.iter().find_map(|e| match e {
            PageElement::Tbl(t) => Some(t),
            _ => None,
        });
        let t = table.expect("an aligned three-column table was not detected");
        assert_eq!(t.col_count, 3, "columns: {:?}", t.rows.first());
        assert_eq!(t.rows.len(), 4);
    }

    #[test]
    fn text_without_a_column_of_its_own_does_not_land_in_the_first_cell() {
        // Previously everything that did not start exactly on a column ended up
        // in column 0 (`unwrap_or(0)`), so loose text from the whole line landed
        // in the first cell. Text belongs to the column it sits under.
        let mut blocks = Vec::new();
        for y in [700.0_f64, 684.0, 668.0] {
            blocks.push(make_block("A", 72.0, y, 12.0));
            blocks.push(make_block("B", 300.0, y, 12.0));
        }
        // One block that forms no column anywhere, but does sit on the right.
        blocks.push(make_block("loose", 310.0, 652.0, 12.0));
        blocks.push(make_block("A", 72.0, 652.0, 12.0));

        let elements = analyze_page(&blocks);
        if let Some(PageElement::Tbl(t)) =
            elements.iter().find(|e| matches!(e, PageElement::Tbl(_)))
        {
            let last = t.rows.last().expect("no rows");
            assert!(
                !last[0].contains("loose"),
                "unaligned text was dumped into the first cell: {last:?}"
            );
        }
    }
}
