// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The block model as markdown (#568, #569, #570, #571).
//!
//! The heading classifier next door is only worth what a consumer can read off
//! it, and what a RAG pipeline reads is markdown: Docling, Marker and every
//! tool this milestone is measured against publish in it, and the benchmark
//! harness grades every column through one markdown reader. So a heading that
//! never reaches a `#` is a heading nobody downstream has.
//!
//! DELIBERATELY THE SMALLEST THING THAT IS TRUE. Headings become `#`..`###`,
//! list items become `-` and `1.`, tables become a table, paragraphs become
//! paragraphs, and nothing else claims a syntax it has not earned. The
//! serializers ticket (#572) owns what grows here -- page-break markers, the
//! other formats, and the golden tests over all of it. What this file owes that
//! ticket is the mapping and a deterministic byte for byte output, which it can
//! then extend rather than replace.
//!
//! WHY A TABLE IS SOMETIMES HTML. A markdown table is a rectangle: every row
//! the same number of cells, none reaching across two. A table that is not one
//! has to be written in something that can say `colspan`, and that is HTML --
//! which every markdown reader this milestone is measured through passes along
//! untouched, and which is the rule Docling follows for the same reason.
//!
//! AND WHY THE FIRST ROW IS THE HEADER ROW EVEN WHERE THE DOCUMENT DID NOT SAY
//! SO: a markdown table has no shape without a delimiter row, and the row above
//! it is the header whatever it holds. `BlockKind::TableCell` carries `header`
//! for the tables that DID say so, and the HTML form writes it as `th` against
//! `td`. The markdown form cannot, and says the only thing the format lets it.
//!
//! WHY A NESTED ITEM IS INDENTED BY ITS PARENT'S MARKER AND NOT BY A CONSTANT.
//! CommonMark reads a list inside an item only when the nested line starts at
//! or past the parent item's own CONTENT column, and that column is as wide as
//! the parent's marker: two for `- `, three for `1. `, five for `100. `. A
//! constant wide enough for the widest is a constant that opens an indented
//! code block under the narrowest, and a constant narrow enough for `- ` drops
//! out of `100. ` -- which the detector produces, because it accepts three
//! digits (codex, round 2). So the items still open are kept on a stack with
//! the column each of them opened, and a child is written at its parent's.
//!
//! AND WHY THE LEVEL IS NOT TAKEN AT FACE VALUE. A level is a rank among the
//! indent steps of a REGION, so a page whose outer list the detector missed can
//! open with an item at level 1 and no level 0 above it. Written out indented
//! with nothing to nest into, that line is an indented code block, and every
//! word of it leaves the text layer. So the depth actually written is never
//! more than one past the deepest item still open -- which is what the stack
//! answers anyway.
//!
//! WHY THE TEXT IS ESCAPED AT ALL. A paragraph that opens `# `, `- ` or `<`
//! is prose the document set, and emitting it raw would hand the reader a
//! heading, a list item or a block of HTML the document does not have -- the
//! same mistake in the other direction from missing one. Only the markers that
//! start a line are escaped, because those are the only ones the benchmark
//! reader (and CommonMark) reads at the front of a line.
//!
//! A FENCE IS THE WORST OF THEM, and it is why this is not a nicety. A
//! paragraph opening ``` -- a code listing, a manual, anything that quotes
//! markdown -- opens a fenced block that nothing here ever closes, and every
//! heading and paragraph after it on the page is swallowed into it (codex,
//! round 1). One backslash ends that.

//! WHAT DOES NOT REACH THE MARKDOWN AT ALL is the furniture: the running head
//! and the page number a document repeats are marked as such (#571) and are
//! not written, because standing between two paragraphs is exactly what makes
//! them a defect for anything that chunks the text. They are still in the
//! block model, which is where a caller that wants them reads them -- or the
//! caller asks [`MarkdownOptions::include_headers`] and
//! [`MarkdownOptions::include_footers`], and they are written as plain
//! paragraphs at their reading-order position.
//!
//! AND WHAT REACHES IT AS ONE PARAGRAPH is the sentence a page break cut in
//! half: `Block::continues_previous` says so, and the second half is written
//! without a break, without an escape -- it is not at the start of a line any
//! more -- and with the word the break was made inside of put back together.
//!
//! WHERE THE PAGE DID CHANGE, an HTML COMMENT says so (#572). A chunker that
//! cites the document needs the page a chunk came from, and markdown has no
//! syntax for one. A comment is the only thing CommonMark carries through
//! untouched and shows to nobody: every reader this milestone is measured
//! through passes it along, no renderer prints it, and the benchmark's own
//! reader strips it before a single axis sees it -- so the marker cannot buy
//! or cost a point on any of them. A caller that wants no marker asks
//! [`PageSeparators::None`], and the paragraphs stand separated by a blank
//! line.
//!
//! THE NUMBER IN IT IS `Block::page`, which is 0-BASED, and it is the same
//! number the JSON form carries under `page`. A reader's chrome counts pages
//! from one and this does not: a caller joining the two serialisations of one
//! document must find the same page in both, and a marker that agreed with
//! Acrobat's status bar instead would disagree with our own model.
//!
//! AND IT NEVER STANDS INSIDE A PARAGRAPH. A sentence the break cut in two is
//! one paragraph (above), and a marker between the halves would cut it again
//! -- which is the defect #571 removed, rebuilt out of comments. The marker is
//! therefore DEFERRED and not dropped: the new page is owed one, and the next
//! block on it opens with it. A paragraph whose second half ends the document
//! is the one case that leaves no marker at all, because there is nothing
//! after it to attribute to the new page.
//!
//! THE SYNTAX IS RESERVED, which is what makes the marker readable at all. A
//! document that QUOTES an HTML comment -- a manual, a style guide, a page of
//! markdown about markdown -- would otherwise put `<!-- page 7 -->` at the
//! start of a line as body text, byte for byte what this writes, and a
//! chunker following the convention would file the rest of the document under
//! page 7 (codex, round 1). So a `<` that opens a line is escaped like every
//! other marker below, and a marker at the start of a line is always ours.
//!
//! MONOSPACE READS AS CODE (#636). A run of spans the document set in a
//! monospace face is written `` `like this` `` inside a paragraph or a list
//! item, and a paragraph set wholly in one over two lines or more as a fenced
//! block with its line breaks kept -- the way pymupdf4llm renders what its
//! `ignore_code` switch reads. Tables, headings and blocks of tabular figures
//! never are, and [`ExtractOptions::code`] switches the whole of it off.
//!
//! [`ExtractOptions::code`]: crate::blocks::ExtractOptions::code

use crate::blocks::{Block, BlockKind, ExtractOptions, PageSeparators, TableRenderMode};
#[cfg(test)]
use crate::table_trace::TableOrigin;
use crate::text::{HorizontalRule, TextSpan};

/// What one cell of a table says about itself.
struct CellAt<'a> {
    row: usize,
    column: usize,
    row_span: usize,
    column_span: usize,
    header: bool,
    text: &'a str,
}

/// The same cell with its text owned, so the currency join can rewrite it.
struct OwnedCell {
    row: usize,
    column: usize,
    row_span: usize,
    column_span: usize,
    header: bool,
    text: String,
}

/// The cell a block is, or `None` where it is not one.
fn cell_of(block: &Block) -> Option<CellAt<'_>> {
    match block.kind {
        BlockKind::TableCell {
            row,
            column,
            row_span,
            column_span,
            header,
        } => Some(CellAt {
            row: row as usize,
            column: column as usize,
            row_span: (row_span as usize).max(1),
            column_span: (column_span as usize).max(1),
            header,
            text: block.text.trim(),
        }),
        _ => None,
    }
}

/// One past the last block of the table that starts at `from`, or `None` where
/// no table starts there.
///
/// A table is a run of adjacent cells in ROW-MAJOR order, so `(row, column)`
/// climbs with every block of it and the run ends where that stops: where the
/// blocks stop being cells, where the page changes, or where the coordinates go
/// backwards, which is a second table starting.
///
/// NOT "the next cell at row 0, column 0". A table whose first row opens with
/// an empty cell has no block there at all, so the run ran straight on into it
/// and the two grids came out as one -- with duplicate coordinates, which the
/// serializer resolves by taking the first and dropping the second table's
/// cells (codex, round 2).
pub(crate) fn table_run(blocks: &[Block], from: usize) -> Option<usize> {
    let first = cell_of(&blocks[from])?;
    let mut last = (first.row, first.column);
    let mut end = from + 1;
    while end < blocks.len() && blocks[end].page == blocks[from].page {
        match cell_of(&blocks[end]) {
            Some(cell) if (cell.row, cell.column) > last => {
                last = (cell.row, cell.column);
                end += 1;
            }
            _ => break,
        }
    }
    Some(end)
}

/// [`table_run`], and on across a page break where the next page's first cell
/// is marked as the same table's continuation ([`Block::continues_previous`],
/// set by `crate::blocks::tables_across_pages`, #574).
fn table_run_across(blocks: &[Block], from: usize) -> Option<usize> {
    let mut end = table_run(blocks, from)?;
    while end < blocks.len()
        && blocks[end].continues_previous
        && blocks[end].page != blocks[end - 1].page
        && cell_of(&blocks[end]).is_some()
    {
        end = table_run(blocks, end)?;
    }
    Some(end)
}

/// The cells of a run that may span pages, with every continuation's rows
/// numbered on from the rows before it -- and its first row left out where it
/// repeats the table's head, which is what a table set over two pages does.
fn cells_across(blocks: &[Block]) -> Vec<CellAt<'_>> {
    let mut out: Vec<CellAt> = Vec::new();
    let mut offset = 0usize;
    let mut segment_head: Option<usize> = None; // the row that heads this segment, if it repeats
    let head: Vec<&str> = blocks
        .iter()
        .filter_map(cell_of)
        .take_while(|c| c.row == 0)
        .map(|c| c.text)
        .collect();
    for (k, block) in blocks.iter().enumerate() {
        let Some(mut cell) = cell_of(block) else {
            continue;
        };
        if k > 0 && block.continues_previous && block.page != blocks[k - 1].page {
            let rows_so_far = out.iter().map(|c| c.row + c.row_span).max().unwrap_or(0);
            let first_row = cell.row;
            let this_head: Vec<&str> = blocks[k..]
                .iter()
                .take_while(|b| b.page == block.page)
                .filter_map(cell_of)
                .take_while(|c| c.row == first_row)
                .map(|c| c.text)
                .collect();
            segment_head = (!head.is_empty() && this_head == head).then_some(first_row);
            offset = rows_so_far - first_row.min(rows_so_far);
            if segment_head.is_some() {
                offset = rows_so_far.saturating_sub(first_row + 1);
            }
        }
        if segment_head == Some(cell.row) {
            continue;
        }
        cell.row += offset;
        out.push(cell);
    }
    out
}

/// A table's cells as markdown, or as HTML where markdown cannot hold them --
/// or in the one form `mode` asks for, whatever the table's shape.
fn table(blocks: &[Block], switches: crate::learned::Switches, mode: TableRenderMode) -> String {
    let mut owned: Vec<OwnedCell> = cells_across(blocks)
        .iter()
        .map(|cell| OwnedCell {
            row: cell.row,
            column: cell.column,
            row_span: cell.row_span,
            column_span: cell.column_span,
            header: cell.header,
            text: cell.text.to_string(),
        })
        .collect();
    join_currency_columns(&mut owned);
    let mut cells: Vec<CellAt> = owned
        .iter()
        .map(|cell| CellAt {
            row: cell.row,
            column: cell.column,
            row_span: cell.row_span,
            column_span: cell.column_span,
            header: cell.header,
            text: &cell.text,
        })
        .collect();
    let head = header_block(&cells, switches);
    for cell in cells.iter_mut().filter(|cell| cell.row < head) {
        cell.header = true;
    }
    let columns = cells
        .iter()
        .map(|cell| cell.column + cell.column_span)
        .max()
        .unwrap_or(0);
    let rows = cells
        .iter()
        .map(|cell| cell.row + cell.row_span)
        .max()
        .unwrap_or(0);
    if columns == 0 || rows == 0 {
        return String::new();
    }

    let rectangular = cells
        .iter()
        .all(|cell| cell.row_span == 1 && cell.column_span == 1)
        && (0..rows).all(|r| {
            let mut seen: Vec<usize> = cells
                .iter()
                .filter(|cell| cell.row == r)
                .map(|cell| cell.column)
                .collect();
            seen.sort_unstable();
            seen == (0..columns).collect::<Vec<usize>>()
        });

    // A pipe table holds ONE header row (the one above its delimiter); a header
    // of two lines needs HTML to say both are headers. And with no header
    // row at all a pipe table is a lie -- its first row reads as a header
    // the table does not have -- so under `markdown_headerless_html` such
    // a table is HTML with every cell as the decoder marked it. A caller
    // that names a mode gets that form for every table instead (#659).
    let pipe = match mode {
        TableRenderMode::Markdown => true,
        TableRenderMode::Html => false,
        TableRenderMode::Auto => {
            rectangular && head <= 1 && (head == 1 || !switches.markdown_headerless_html)
        }
    };
    if pipe {
        markdown_table(&cells, rows, columns)
    } else {
        html_table(&cells, rows, columns)
    }
}

/// How many leading rows are the table's column header: the rows above the first
/// row whose cells are mostly figures.
///
/// A two-line header (`Issue` over `Age`, `December 31,` over `2009`) came out
/// as body rows, so a reader keying columns by their header -- ParseBench's
/// record match keys each column by its leading `<th>` rows -- found no header
/// at all and fell back to `col_0, col_1, ...`: record match 0 on tables whose
/// cells were 97% right. Marked as header, ParseBench's table score on the
/// tuning half went 0.573 -> 0.600 (13 pages up, 6 down) and FinTabNet TEDS
/// 0.526 -> 0.531 (#641, 01-10-2026).
///
/// 0 -- nothing is marked -- when the first row is already figures, when no
/// row is (a table of words, unless `markdown_header_block_word_table` heads
/// its first row by position), when the block would be more than four rows
/// (that is a list of labels, not a header), or when a cell spans rows (the
/// header is not a band of whole rows then -- unless
/// `markdown_header_block_rowspan_crossing` narrows the veto to a span
/// straddling the head boundary).
///
/// A ROW OF YEARS IS NOT A FIGURES ROW. A statement heads its columns with the
/// year each holds (`2012 | 2011 | 2010` under `Year Ended December 31,`), and
/// a year reads as a figure, so the header stopped above it and the years came
/// out as body: FinTabNet TEDS 0.5302 -> 0.5362 (43 tables up, 0 down, #574).
///
/// A MIXED ROW IS A BODY ROW under `markdown_header_block_mixed_body`: a row
/// that already holds values beside its labels has turned from label to value,
/// even below the majority line, so the header ends above it. Years are not
/// values here: a stubbed year row (`Contractual Obligation | 2008 | 2009`)
/// heads its columns, so only a non-year figure mixes.
fn header_block(cells: &[CellAt], switches: crate::learned::Switches) -> usize {
    let rows = cells.iter().map(|cell| cell.row + 1).max().unwrap_or(0);
    if rows == 0 {
        return 0;
    }
    let figures = |r: usize| {
        if is_year_row(cells, r, switches.markdown_header_block_stub_years) {
            return false;
        }
        let texts: Vec<&str> = cells
            .iter()
            .filter(|cell| cell.row == r)
            .map(|cell| cell.text.trim())
            .filter(|text| !text.is_empty())
            .collect();
        if texts.is_empty() {
            return false;
        }
        let counted = texts
            .iter()
            .filter(|t| {
                is_figure(t)
                    || (switches.markdown_header_block_figures && is_stacked_figure_cell(t))
            })
            .count();
        let valued = texts
            .iter()
            .filter(|t| {
                (is_figure(t)
                    || (switches.markdown_header_block_figures && is_stacked_figure_cell(t)))
                    && !is_year_cell(t)
            })
            .count();
        let mixed = valued > 0 && valued < texts.len();
        2 * counted >= texts.len() || (switches.markdown_header_block_mixed_body && mixed)
    };
    let mut head = (0..rows).find(|&r| figures(r)).unwrap_or(rows);
    if head == rows && switches.markdown_header_block_word_table && rows > 1 {
        let first_has_text = cells
            .iter()
            .filter(|cell| cell.row == 0)
            .any(|cell| !cell.text.trim().is_empty());
        if first_has_text {
            head = 1;
        }
    }
    if head == rows || head > 4 {
        return 0;
    }
    if cells.iter().any(|cell| cell.row_span > 1) {
        if !switches.markdown_header_block_rowspan_crossing {
            return 0;
        }
        let crosses = cells
            .iter()
            .any(|cell| cell.row < head && cell.row + cell.row_span > head);
        if crosses {
            return 0;
        }
    }
    head
}

/// Whether a row is a row of years: every non-empty cell a year, at least two.
///
/// Under `markdown_header_block_stub_years` the stub column does not vote: a
/// row whose value cells are all years is a year row whatever its first cell
/// holds -- `($ in millions) | 2010 | 2009` heads a statement's columns, and
/// counting its years as figures ended the header above an empty block (#636).
fn is_year_row(cells: &[CellAt], row: usize, stub_years: bool) -> bool {
    let mut all_years = true;
    let mut years = 0usize;
    for cell in cells.iter().filter(|cell| cell.row == row) {
        let text = cell.text.trim();
        if text.is_empty() {
            continue;
        }
        if !is_year_cell(text) {
            all_years = false;
            break;
        }
        years += 1;
    }
    if all_years && years >= 2 {
        return true;
    }
    if !stub_years {
        return false;
    }
    let mut valued = 0usize;
    for cell in cells
        .iter()
        .filter(|cell| cell.row == row && cell.column != 0)
    {
        let text = cell.text.trim();
        if text.is_empty() {
            continue;
        }
        if !is_year_cell(text) {
            return false;
        }
        valued += 1;
    }
    valued >= 2
}

/// Whether a cell holds a year: `19xx`/`20xx`, optionally followed by a
/// footnote mark -- `(1)`/`( 1)` with or without the space before it, or `*`.
fn is_year_cell(text: &str) -> bool {
    let mut rest = text.trim();
    if rest.is_empty() {
        return false;
    }
    if let Some(without_star) = rest.strip_suffix('*') {
        rest = without_star.trim_end();
        if rest.is_empty() {
            return false;
        }
    }
    if let Some(open) = rest.find('(') {
        let (year, note) = rest.split_at(open);
        if !is_year(year.trim_end()) {
            return false;
        }
        let note = note.trim();
        if !(note.starts_with('(') && note.ends_with(')')) {
            return false;
        }
        let inner = note[1..note.len() - 1].trim();
        return !inner.is_empty() && inner.bytes().all(|b| b.is_ascii_digit());
    }
    if rest.contains(['(', ')']) {
        return false;
    }
    is_year(rest)
}

/// Whether a cell holds exactly a four-digit year of the last two centuries.
fn is_year(text: &str) -> bool {
    text.len() == 4
        && (text.starts_with("19") || text.starts_with("20"))
        && text.bytes().all(|b| b.is_ascii_digit())
}

/// Whether a cell holds exactly one currency symbol.
fn is_currency_symbol(text: &str) -> bool {
    matches!(text.trim(), "$" | "€" | "£" | "¥")
}

/// Whether a spanning cell covers both the currency column and the one it
/// joins onto: such a span is a header over the join, not a grid beside it.
fn contains_the_join(cell: &OwnedCell, join: usize) -> bool {
    cell.column_span > 1 && cell.column <= join && cell.column + cell.column_span >= join + 2
}

/// The first column that is only currency symbols, if there is one.
///
/// A CURRENCY COLUMN holds nothing but empty cells and exactly one symbol per
/// cell (`$`, `€`, `£`, `¥`), at least one symbol in the column, and no cell
/// only PARTLY overlapping it and the column to its right spans columns: a
/// span that ends at the currency column or starts at the next one means the
/// grid beside it is not one cell per row, so there is nothing to join onto.
/// A span that CONTAINS both columns is a header over the join and does not
/// block it; it shrinks by one with the column it covers (#574).
fn find_currency_column(cells: &[OwnedCell]) -> Option<usize> {
    let columns = cells
        .iter()
        .map(|cell| cell.column + cell.column_span)
        .max()
        .unwrap_or(0);
    if columns < 2 {
        return None;
    }
    for join in 0..columns - 1 {
        if cells.iter().any(|cell| {
            cell.column_span > 1
                && cell.column < join + 2
                && cell.column + cell.column_span > join
                && !contains_the_join(cell, join)
        }) {
            continue;
        }
        let mut symbol = false;
        let mut clean = true;
        for cell in cells
            .iter()
            .filter(|cell| cell.column == join && !contains_the_join(cell, join))
        {
            let text = cell.text.trim();
            if text.is_empty() {
                continue;
            }
            if is_currency_symbol(text) {
                symbol = true;
            } else {
                clean = false;
                break;
            }
        }
        if clean && symbol {
            return Some(join);
        }
    }
    None
}

/// Join every currency column onto the column to its right.
///
/// A statement sets its currency in a column of its own (`$` beside `115`),
/// and written as its own column the symbol reads as a column with no header
/// and the figure as a figure with no unit. Each symbol is prefixed to the
/// cell to its right (`$ 115`), the column is removed, and the columns after
/// it shift down by one, until no currency column remains: FinTabNet TEDS
/// 0.5302 -> 0.5406 (33 up, 0 down) and ParseBench 0.6001 -> 0.6022 (#574).
///
/// A header span over the join keeps covering the same figures: its
/// `column_span` shrinks by one with the column it loses.
fn join_currency_columns(cells: &mut Vec<OwnedCell>) {
    while let Some(join) = find_currency_column(cells) {
        let symbols: Vec<(usize, String, bool)> = cells
            .iter()
            .filter(|cell| cell.column == join)
            .filter_map(|cell| {
                let text = cell.text.trim();
                if is_currency_symbol(text) {
                    Some((cell.row, text.to_string(), cell.header))
                } else {
                    None
                }
            })
            .collect();
        for (row, symbol, header) in symbols {
            match cells
                .iter_mut()
                .find(|cell| cell.row == row && cell.column == join + 1)
            {
                Some(right) => {
                    let text = right.text.trim();
                    right.text = if text.is_empty() {
                        symbol
                    } else {
                        format!("{symbol} {text}")
                    };
                }
                None => cells.push(OwnedCell {
                    row,
                    column: join + 1,
                    row_span: 1,
                    column_span: 1,
                    header,
                    text: symbol,
                }),
            }
        }
        cells.retain(|cell| cell.column != join || contains_the_join(cell, join));
        for cell in cells
            .iter_mut()
            .filter(|cell| contains_the_join(cell, join))
        {
            cell.column_span -= 1;
        }
        for cell in cells.iter_mut().filter(|cell| cell.column > join) {
            cell.column -= 1;
        }
    }
}

/// Whether a cell holds a figure: an amount, a percentage, a year, a dash for
/// nil, or `n/a`.
pub(crate) fn is_figure(text: &str) -> bool {
    let t = text.trim();
    if t.eq_ignore_ascii_case("n/a") || t.eq_ignore_ascii_case("na") {
        return true;
    }
    if !t.is_empty() && t.chars().all(|c| matches!(c, '-' | '–' | '—')) {
        return true;
    }
    let core = t
        .trim_start_matches(|c: char| c.is_whitespace() || "$€£(-–—+".contains(c))
        .trim_end_matches(|c: char| c.is_whitespace() || "%*)".contains(c));
    !core.is_empty()
        && core.chars().any(|c| c.is_ascii_digit())
        && core
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == ',')
}

/// Whether a cell holds figures run together: every whitespace-separated
/// token a figure or a nil dash, with or without a currency symbol --
/// `86,090 (4,305)`, `$—`, `$ —`, `$ 990`
/// (`markdown_header_block_figures`, #574).
///
/// Two text lines fused into one row read as one cell, and `is_figure` fails
/// on the inner space -- so the fused body rows read as header. A bare
/// currency glyph is an amount's half, not a figure on its own: `$` alone is
/// no figure here, as under `is_figure`.
fn is_stacked_figure_cell(text: &str) -> bool {
    let mut substance = false;
    for token in text.split_whitespace() {
        if is_figure(token) {
            substance = true;
            continue;
        }
        // A currency symbol plus a nil dash, which `is_figure` strips to
        // empty: `$—`, `€ -`'s `-` beside its `€`.
        let rest = token.trim_start_matches(['$', '€', '£', '¥', '¢', '₹']);
        if rest.len() < token.len()
            && !rest.is_empty()
            && rest.chars().all(|c| matches!(c, '-' | '–' | '—'))
        {
            substance = true;
            continue;
        }
        // A bare currency glyph beside the figures: `$` in `$ —`, `$ 990`.
        if !token.is_empty() && token.chars().all(|c| "$€£¥¢₹".contains(c)) {
            continue;
        }
        return false;
    }
    substance
}

/// The rectangular case: a pipe table, its first row the header row.
fn markdown_table(cells: &[CellAt], rows: usize, columns: usize) -> String {
    let mut out = String::new();
    for r in 0..rows {
        out.push('|');
        for c in 0..columns {
            let text = cells
                .iter()
                .find(|cell| cell.row == r && cell.column == c)
                .map(|cell| cell.text)
                .unwrap_or_default();
            out.push(' ');
            out.push_str(&escape_cell(&cell_text(text, columns)));
            out.push_str(" |");
        }
        out.push('\n');
        if r == 0 {
            out.push('|');
            for _ in 0..columns {
                out.push_str(" --- |");
            }
            out.push('\n');
        }
    }
    out
}

/// Everything else: an HTML table, which can say `colspan`, `rowspan` and the
/// difference between a header cell and a body cell.
///
/// WALKED COLUMN BY COLUMN, not cell by cell. An HTML row has no way to say
/// "this cell is in the third column"; position is counted from the cells
/// before it. So a row whose second cell is missing -- which is the commonest
/// thing a grid that is not a rectangle has in it -- would put its third cell
/// in the second column of every reader, and the `27%` under `Amount` would
/// come out under `Region` (codex, round 1). An empty `td` holds the place, and
/// a column a cell above reaches down into holds it without one.
fn html_table(cells: &[CellAt], rows: usize, columns: usize) -> String {
    let mut taken = vec![vec![false; columns]; rows];
    let mut out = String::from("<table>\n");
    for r in 0..rows {
        out.push_str("<tr>");
        let mut c = 0usize;
        while c < columns {
            if taken[r][c] {
                c += 1;
                continue;
            }
            let Some(cell) = cells.iter().find(|cell| cell.row == r && cell.column == c) else {
                out.push_str("<td></td>");
                c += 1;
                continue;
            };
            for row in taken.iter_mut().take((r + cell.row_span).min(rows)).skip(r) {
                for held in row
                    .iter_mut()
                    .take((c + cell.column_span).min(columns))
                    .skip(c)
                {
                    *held = true;
                }
            }
            let tag = if cell.header { "th" } else { "td" };
            out.push('<');
            out.push_str(tag);
            if cell.column_span > 1 {
                out.push_str(&format!(" colspan=\"{}\"", cell.column_span));
            }
            if cell.row_span > 1 {
                out.push_str(&format!(" rowspan=\"{}\"", cell.row_span));
            }
            out.push('>');
            out.push_str(&escape_html(&cell_text(cell.text, columns)));
            out.push_str(&format!("</{tag}>"));
            c += cell.column_span.max(1);
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</table>");
    out
}

/// A cell's text as the table writes it: leaders out of a statement, kept in a
/// contents list.
///
/// A table of two columns is a label and its number -- a table of contents
/// (`Dell . . . . 16`), a citation list -- and there the dots are what the
/// line is made of: the DocLayNet truth keeps them, and stripping them cost
/// three such tables their match on the proxy (tables 0.657 -> 0.636). A
/// statement has a label and several figures, and there they are layout.
fn cell_text(text: &str, columns: usize) -> String {
    if columns >= 3 {
        without_leaders(text)
    } else {
        text.to_string()
    }
}

/// A cell's text without its dot leaders.
///
/// A financial statement runs a row label out to its figures on dots --
/// `United States . . . . ........ $11,772` -- and the leaders land in the
/// label's cell. They are layout, not content: on 614 FinTabNet tables (train
/// side, companies no engine model saw) they cost 0.093 TEDS, and 82 tables
/// were not matched to their truth at all because the dots outweighed the
/// words (01-10-2026, #641). A run of four or more dots with at most one
/// whitespace between them is a leader -- the rule `judge::has_leader` uses --
/// so an ellipsis (`...`) and a decimal point stay.
fn without_leaders(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '.' {
            // the run: dots, each followed by at most one whitespace
            let mut j = i;
            let mut dots = 0usize;
            while j < chars.len() && chars[j] == '.' {
                dots += 1;
                j += 1;
                if j < chars.len() && chars[j].is_whitespace() && chars.get(j + 1) == Some(&'.') {
                    j += 1;
                }
            }
            if dots >= 4 {
                out.push(' ');
                i = j;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out.split_whitespace().collect::<Vec<&str>>().join(" ")
}

/// A cell's text inside a pipe table: a pipe of its own would end the cell, and
/// a line break would end the row.
fn escape_cell(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '|' => out.push_str("\\|"),
            '\n' | '\r' => out.push(' '),
            _ => out.push(c),
        }
    }
    out
}

/// A cell's text inside an HTML table.
fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

/// How many digits an ordered list marker may open with before it stops being
/// one. Nine, which is what CommonMark reads: `1234567890.` is a figure and a
/// full stop.
const ORDINAL_MAX_DIGITS: usize = 9;

/// Whether a line opening with `marker` really opens a marker.
///
/// A bullet, an ATX heading and an ordered item are all the character AND a
/// space after it -- or the end of the line, which is an empty item and an
/// empty heading. Without that clause every figure a document sets at the head
/// of a line (`-3,503`, `0.1`, `#4`) is written with a backslash in front of
/// it, and the backslash is in the text layer of everything downstream: the
/// benchmark's reader, a chunker, an embedding.
///
/// THE RULE OF THREE IS THE OTHER HALF. `---` and `***` are a thematic break
/// with no space anywhere in them, and `---` alone under a paragraph is a
/// setext heading; a line that is nothing but three or more of the marker is
/// escaped whatever follows it.
fn opens_a_marker(line: &str, marker: char) -> bool {
    // AN ATX HEADING OPENS WITH A RUN, and the space is asked for after the
    // run rather than after the first hash: `## Terms` is a level-two heading
    // and has to keep its backslash. Six is where CommonMark stops counting,
    // so a seventh hash makes the line prose and no escape is owed.
    let run = if marker == '#' {
        line.chars().take_while(|c| *c == '#').count().min(6)
    } else {
        marker.len_utf8()
    };
    let rest = &line[run..];
    if matches!(rest.chars().next(), None | Some(' ') | Some('\t')) {
        return true;
    }
    matches!(marker, '-' | '*')
        && line.chars().filter(|c| !c.is_whitespace()).count() >= 3
        && line.chars().all(|c| c == marker || c.is_whitespace())
}

/// A paragraph's own text, with anything that would read as a marker at the
/// start of a line backslashed.
fn escape_leading_marker(text: &str) -> String {
    let trimmed = text.trim_start();
    let indent = &text[..text.len() - trimmed.len()];
    // A fence opener first: three or more of the same character, which the
    // single-character arms below would leave as a fence of two.
    for fence in ['`', '~'] {
        if trimmed.starts_with([fence, fence, fence].iter().collect::<String>().as_str()) {
            return format!("{indent}\\{trimmed}");
        }
    }
    let mut chars = trimmed.chars();
    let escaped = match chars.next() {
        // A blockquote, a table row and our own page marker need no space
        // after them: the character alone is the syntax.
        Some(c @ ('>' | '|' | '`' | '~' | '<')) => format!("\\{c}{}", chars.as_str()),
        // A bullet and an ATX heading do. `- hire purchase` is a list item and
        // `-3,503` is a figure; `# 4` is a heading and `#4` is not one in any
        // reader this milestone is measured through. An escape that changes
        // nothing is a backslash in the text layer, and the round of #579
        // counted 79 of them over 200 pages.
        Some(c @ ('#' | '-' | '*' | '+')) if opens_a_marker(trimmed, c) => {
            format!("\\{c}{}", chars.as_str())
        }
        Some(c) if c.is_ascii_digit() => {
            // `1. Introduction` is a list item; `1984 was` and `0.1` are not.
            let rest = chars.as_str();
            let digits = trimmed.chars().take_while(|c| c.is_ascii_digit()).count();
            let after = trimmed[digits..].chars().next();
            let delimited = matches!(after, Some('.') | Some(')'))
                && opens_a_marker(&trimmed[digits..], after.unwrap_or('.'));
            if digits <= ORDINAL_MAX_DIGITS && delimited {
                format!("{}\\{}", &trimmed[..digits], &trimmed[digits..])
            } else {
                format!("{c}{rest}")
            }
        }
        Some(c) => format!("{c}{}", chars.as_str()),
        None => String::new(),
    };
    format!("{indent}{escaped}")
}

/// The marker a page change writes, `page` being the 0-based `Block::page` of
/// the page that OPENS -- the same number the JSON form carries.
///
/// A comment and not a rule (`---`): a thematic break is content, and a reader
/// that renders the markdown would print a line the document does not have.
fn page_marker(page: usize) -> String {
    format!("<!-- page {page} -->")
}

/// Write the page marker where the page has changed, and remember the page.
///
/// The open list items are dropped with it: an HTML block ends a list in
/// CommonMark whatever follows, so the items after the marker open a list of
/// their own, and an item still indented under a parent that no longer exists
/// is an indented code block -- every word of it out of the text layer.
///
/// With [`PageSeparators::None`] nothing is written -- the caller separates
/// the paragraphs with a blank line as it does within a page -- but the page
/// is still remembered, and the list stays open: with no HTML block between
/// them the items on either side are one list.
fn note_page(
    out: &mut String,
    open: &mut Vec<(usize, usize)>,
    last: &mut Option<usize>,
    page: usize,
    separators: PageSeparators,
) {
    if *last == Some(page) {
        return;
    }
    if last.is_some() && !out.is_empty() && separators == PageSeparators::Comment {
        out.push_str("\n\n");
        out.push_str(&page_marker(page));
        open.clear();
    }
    *last = Some(page);
}

/// A character of a right-to-left script (Hebrew, Arabic and their
/// presentation forms).
fn is_right_to_left(c: char) -> bool {
    matches!(c as u32, 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF)
}

/// The first place from `from` where `run` stands in `text` as whole words: no
/// letter or digit touching it on either side.
fn whole_at(text: &str, run: &str, from: usize) -> Option<usize> {
    let mut at = from;
    while let Some(found) = text[at..].find(run) {
        let start = at + found;
        let end = start + run.len();
        let before = text[..start].chars().next_back();
        let after = text[end..].chars().next();
        if !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric) {
            return Some(start);
        }
        at = start + run.chars().next().map_or(1, char::len_utf8);
    }
    None
}

/// How a run of text was set, as far as Markdown can say it (#628, ParseBench's
/// formatting axis): weight, slant, and a raised or lowered baseline.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Set {
    Bold,
    Italic,
    BoldItalic,
    Sup,
    Sub,
}

impl Set {
    fn marks(self) -> (&'static str, &'static str) {
        match self {
            Set::Bold => ("**", "**"),
            Set::Italic => ("*", "*"),
            Set::BoldItalic => ("***", "***"),
            Set::Sup => ("<sup>", "</sup>"),
            Set::Sub => ("<sub>", "</sub>"),
        }
    }
}

/// A code span: its byte range in the block's text.
type CodeSpan = (usize, usize);

/// A style mark: its byte range, the setting, and whether it is underlined.
type StyleMark = (usize, usize, Option<Set>, bool);

/// The block's text with its bold, italic, superscript and subscript runs
/// marked, from the spans the engine already reads (#628).
///
/// The text is the logical reading, with lines joined and broken words put
/// back; the spans are the page's runs. So each marked span is FOUND in the
/// text, in order, and a span that cannot be found (a word the line break cut)
/// is left unmarked rather than guessed at. A run whose own characters are
/// Markdown or HTML syntax is left unmarked too: marking it would need escapes
/// the rest of this serializer does not write. Strikethrough is not here -- the
/// spans carry no drawn line to read it from.
///
/// A raised or lowered run is one of at most four characters, set at most 0.85
/// of the size of the run before it on the same line, whose baseline moved by
/// at least a fifth (up) or a tenth (down) of that run's size.
/// A body block's line: escaped where it opens like a marker, styled otherwise.
///
/// NOT BOTH (#628). A paragraph whose own words open `3. ` is escaped (`3\. `)
/// so that no reader takes it for a list item -- and a bold `3.` wrapped as
/// `**3.** ...` escapes nothing, because the line no longer opens with the
/// digit, while any reader that reads through the emphasis sees `3. ...` again:
/// on nine DocLayNet pages of the round of 27-09 such lines read as list items;
/// our lists fell 0.627 -> 0.618. A line that needs its escape keeps it, unstyled.
///
/// CODE IS THE ONE MARK THAT STANDS IN FRONT OF AN ESCAPE (#636). A line the
/// document set in monospace opens with OUR fence after marking, and a `#` or
/// a digit inside backticks is literal text no reader takes for a heading or
/// an item: the span neutralizes the marker, so no escape is owed. Escaping it
/// instead would break the span it opens.
fn body_of(block: &Block, text: &str, rules: &[HorizontalRule], code: bool) -> String {
    let (spans, styles) = rich_marks(block, text, rules, code);
    let rich = emit_rich(text, &styles, &spans);
    if spans.first().is_some_and(|(start, _)| *start == 0) {
        return rich;
    }
    let escaped = escape_leading_marker(text);
    if escaped != text {
        escaped
    } else {
        escape_leading_marker(&rich)
    }
}

/// `text` with its style and code marks, for a line that continues a paragraph
/// rather than opening one: the second half of a paragraph the page broke in
/// two. No escape is owed mid-line.
fn rich_text(block: &Block, text: &str, rules: &[HorizontalRule], code: bool) -> String {
    let (spans, styles) = rich_marks(block, text, rules, code);
    emit_rich(text, &styles, &spans)
}

/// The code spans of `text` where they are read, and its style marks outside
/// them: code wins every overlap, because a bold marker inside backticks is
/// literal text, not emphasis.
fn rich_marks(
    block: &Block,
    text: &str,
    rules: &[HorizontalRule],
    code: bool,
) -> (Vec<CodeSpan>, Vec<StyleMark>) {
    let spans = if code && code_here(block, text) {
        code_ranges(block, text)
    } else {
        Vec::new()
    };
    let mut styles = styled_ranges(block, text, rules);
    styles.retain(|(start, end, _, _)| !spans.iter().any(|(cs, ce)| *start < *ce && *cs < *end));
    (spans, styles)
}

/// Whether inline code is read in this block at all: a paragraph or a list
/// item, of words rather than figures, set left to right (#636). Headings and
/// table cells never are -- their arms do not call here -- and neither is a
/// caption or a right-to-left block, whose spans are not in the text's order.
fn code_here(block: &Block, text: &str) -> bool {
    matches!(
        block.kind,
        BlockKind::Paragraph | BlockKind::ListItem { .. }
    ) && !mostly_digits(text)
        && !text.chars().any(is_right_to_left)
}

/// Whether `text` is tabular figures rather than words: more than half its
/// non-space characters are digits or punctuation (#636). Financial reports set
/// their figures in monospace faces, and a column of amounts is not code.
fn mostly_digits(text: &str) -> bool {
    let mut total = 0usize;
    let mut figures = 0usize;
    for c in text.chars() {
        if c.is_whitespace() {
            continue;
        }
        total += 1;
        if c.is_ascii_digit() || c.is_ascii_punctuation() {
            figures += 1;
        }
    }
    total > 0 && 2 * figures > total
}

/// The byte ranges of `text` set in a monospace face, in order (#636).
///
/// Like [`styled_ranges`], each span is FOUND in the text in draw order, and a
/// span the line breaks cut apart is left unmarked rather than guessed at.
/// Unlike it, the match is not asked to be whole words: a backtick opens
/// anywhere, where an asterisk mid-word opens nothing. A backtick of the
/// document's own is fenced rather than skipped, and neighbouring monospace
/// spans apart by white space only are one span, the way neighbouring bold
/// runs are one mark.
fn code_ranges(block: &Block, text: &str) -> Vec<CodeSpan> {
    let mut ranges: Vec<CodeSpan> = Vec::new();
    let mut cursor = 0usize;
    for span in &block.spans {
        let run = span.text.trim();
        if run.is_empty() {
            continue;
        }
        let Some(found) = text[cursor..].find(run) else {
            continue;
        };
        let start = cursor + found;
        let end = start + run.len();
        cursor = end;
        if span.is_monospace != Some(true) {
            continue;
        }
        match ranges.last_mut() {
            Some(last) if text[last.1..start].trim().is_empty() => last.1 = end,
            _ => ranges.push((start, end)),
        }
    }
    // Like a style mark, a code span never closes after a trailing hyphen: the
    // next block may continue this paragraph across the page, and the join
    // looks at the last character written.
    if let Some(last) = ranges.last() {
        if last.1 == text.len() && text.ends_with('-') {
            ranges.pop();
        }
    }
    ranges
}

/// The longest run of `` ` `` in `text`.
fn longest_backtick_run(text: &str) -> usize {
    let mut best = 0usize;
    let mut run = 0usize;
    for c in text.chars() {
        if c == '`' {
            run += 1;
            best = best.max(run);
        } else {
            run = 0;
        }
    }
    best
}

/// `content` as an inline code span: a backtick fence one longer than any run
/// inside it, as CommonMark wants, with a space of padding where the content
/// itself opens or closes with a backtick.
fn inline_code(content: &str) -> String {
    let fence = "`".repeat(longest_backtick_run(content) + 1);
    if content.starts_with('`') || content.ends_with('`') {
        format!("{fence} {content} {fence}")
    } else {
        format!("{fence}{content}{fence}")
    }
}

/// A paragraph set wholly in a monospace face over two lines or more, as its
/// lines, top first (#636). `None` everywhere else: a list item keeps inline
/// spans, tables and headings never read as code, and neither does tabular
fn fenced_lines(block: &Block) -> Option<Vec<String>> {
    if !matches!(block.kind, BlockKind::Paragraph) {
        return None;
    }
    if block.text.chars().any(is_right_to_left) || mostly_digits(&block.text) {
        return None;
    }
    if !block.spans.iter().any(|s| !s.text.trim().is_empty())
        || block
            .spans
            .iter()
            .any(|s| !s.text.trim().is_empty() && s.is_monospace != Some(true))
    {
        return None;
    }
    let lines: Vec<String> = crate::text::lines_of_block(block)
        .iter()
        .map(|line| crate::text::line_text(block, line))
        .map(|line| line.trim_end().to_string())
        .filter(|line| !line.trim().is_empty())
        .collect();
    (lines.len() >= 2).then_some(lines)
}

/// Those lines as a fenced block: the fence on its own lines, one backtick
/// longer than any run the lines hold.
fn fenced_block(lines: &[String]) -> String {
    let fence = "`".repeat(
        lines
            .iter()
            .map(|line| longest_backtick_run(line))
            .max()
            .unwrap_or(0)
            .max(2)
            + 1,
    );
    format!("{fence}\n{}\n{fence}", lines.join("\n"))
}

/// Whether a drawn rule runs under `span` as its underline (#623, ParseBench's
/// `is_underline`: we wrote none, 0.000 against pymupdf4llm's 0.335).
///
/// The rule sits just under the baseline -- up to a third of an em below it,
/// or a twentieth above for a rule drawn through the descenders -- and covers
/// most of the run. And it is no wider than the run's own LINE, give or take
/// an em: a table border or a separator under a heading runs past the words
/// above it, an underline stops with them.
fn underlined(span: &TextSpan, line: (f64, f64), rules: &[HorizontalRule]) -> bool {
    let size = span.font_size;
    if size <= 0.0 || span.width <= 0.0 {
        return false;
    }
    let (left, right) = (span.x, span.x + span.width);
    rules.iter().any(|r| {
        r.y <= span.y + 0.05 * size
            && r.y >= span.y - 0.35 * size
            && r.left <= left + 0.1 * span.width
            && r.right >= right - 0.1 * span.width
            && r.left >= line.0 - size
            && r.right <= line.1 + size
    })
}

/// The horizontal extent of the spans on `span`'s baseline in this block.
fn line_of(block: &Block, span: &TextSpan) -> (f64, f64) {
    block
        .spans
        .iter()
        .filter(|s| (s.y - span.y).abs() <= 0.2 * span.font_size.max(1.0))
        .fold((span.x, span.x + span.width), |(l, r), s| {
            (l.min(s.x), r.max(s.x + s.width))
        })
}

fn page_rules<'a>(rules: &'a [Vec<HorizontalRule>], block: &Block) -> &'a [HorizontalRule] {
    rules.get(block.page).map_or(&[], Vec::as_slice)
}

/// The style marks alone, for the tests that pin them: production writes
/// styles beside code through [`rich_marks`].
#[cfg(test)]
fn styled(block: &Block, text: &str, rules: &[HorizontalRule]) -> String {
    emit_rich(text, &styled_ranges(block, text, rules), &[])
}

/// The byte ranges of `text` carrying a style mark, in order: the setting and
/// whether a drawn rule underlines it. [`styled`] writes these alone; the body
/// writes them beside the code spans, outside every one of them.
fn styled_ranges(block: &Block, text: &str, rules: &[HorizontalRule]) -> Vec<StyleMark> {
    // RIGHT-TO-LEFT TEXT IS LEFT UNMARKED (codex, #1999): the block's text is
    // reordered into logical order and its spans are not, so matching one
    // against the other in sequence would drop or misplace marks.
    if text.chars().any(is_right_to_left) {
        return Vec::new();
    }
    let mut ranges: Vec<StyleMark> = Vec::new();
    let mut cursor = 0usize;
    let mut previous: Option<&crate::text::TextSpan> = None;
    for (index, span) in block.spans.iter().enumerate() {
        let run = span.text.trim();
        let before = previous;
        previous = Some(span);
        if run.is_empty() || run.contains(['*', '_', '<', '>', '`', '[', ']', '\\', '~']) {
            continue;
        }
        let raised = before.and_then(|prev| {
            let small = span.font_size > 0.0
                && prev.font_size > 0.0
                && span.font_size <= 0.85 * prev.font_size
                && run.chars().count() <= 4;
            if !small {
                return None;
            }
            let shift = span.y - prev.y;
            // ON THE SAME LINE (codex, #1999): the run before it may end the
            // line above, and a line's own step is larger than any shift.
            if shift.abs() >= 0.8 * prev.font_size || span.x < prev.x {
                return None;
            }
            if shift >= 0.2 * prev.font_size {
                Some(Set::Sup)
            } else if shift <= -0.1 * prev.font_size {
                Some(Set::Sub)
            } else {
                None
            }
        });
        let set = raised.or(match (span.is_bold, span.is_italic) {
            (true, true) => Some(Set::BoldItalic),
            (true, false) => Some(Set::Bold),
            (false, true) => Some(Set::Italic),
            (false, false) => None,
        });
        // FOUND WHOLE: a run that matches only inside a word (`hyphen-` in
        // `hyphen-ated`, which the line break made) would mark half a word.
        // A raised or lowered run sits inside its word (`H2O`, `m2`) by nature.
        //
        // JOINED WHOLE comes first: the block rebuild joins spans the shared
        // word rule joins, so a sub-word span (`2` in `2025`) never matches
        // whole -- but it still sits exactly at the cursor, past the blanks
        // the whitespace spans left behind, when the spans and the text
        // agree. Placing it there never jumps the cursor the way a later
        // whole match (`.` in `U.S.` finding `office .`) would; the same
        // rule that joined it tells half a word (`hyphen-` off its line)
        // from a word the page drew in pieces.
        let place = if matches!(set, Some(Set::Sup | Set::Sub)) {
            text[cursor..].find(run).map(|found| cursor + found)
        } else {
            let rest = &text[cursor..];
            let skipped = rest
                .find(|c: char| !c.is_whitespace())
                .unwrap_or(rest.len());
            let at = cursor + skipped;
            let joined = text[at..].starts_with(run)
                && (before.is_some_and(|prev| crate::words_join::spans_join(prev, span))
                    || block
                        .spans
                        .get(index + 1)
                        .is_some_and(|next| crate::words_join::spans_join(span, next)));
            if joined {
                Some(at)
            } else {
                whole_at(text, run, cursor)
            }
        };
        let Some(start) = place else {
            continue;
        };
        let end = start + run.len();
        cursor = end;
        let under = !rules.is_empty() && underlined(span, line_of(block, span), rules);
        if set.is_some() || under {
            match ranges.last_mut() {
                // Neighbouring runs of one setting, apart by white space only,
                // are one run: `**two words**`, not `**two** **words**`.
                Some(last)
                    if last.2 == set
                        && last.3 == under
                        && text[last.1..start].trim().is_empty() =>
                {
                    last.1 = end
                }
                _ => ranges.push((start, end, set, under)),
            }
        }
    }
    // A MARK NEVER CLOSES AFTER A TRAILING HYPHEN (codex, #1999): the next
    // block may continue this paragraph across the page, and the join looks at
    // the last character written -- `**hyphen-**` hides the hyphen it needs.
    if let Some(last) = ranges.last() {
        if last.1 == text.len() && text.ends_with('-') {
            ranges.pop();
        }
    }
    ranges
}

/// `text` with its style marks and its code spans, both in byte order. The two
/// never overlap -- code wins every overlap before this is called -- so one
/// walk writes both.
fn emit_rich(text: &str, styles: &[StyleMark], code: &[CodeSpan]) -> String {
    if styles.is_empty() && code.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + styles.len() * 6 + code.len() * 2);
    let mut at = 0usize;
    let mut styles = styles;
    let mut code = code;
    loop {
        let take_style = match (styles.first(), code.first()) {
            (Some((s, _, _, _)), Some((c, _))) => *s < *c,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => break,
        };
        if take_style {
            let (start, end, set, under) = styles[0];
            styles = &styles[1..];
            let (open, close) = set.map_or(("", ""), Set::marks);
            out.push_str(&text[at..start]);
            out.push_str(open);
            if under {
                out.push_str("<u>");
            }
            out.push_str(&text[start..end]);
            if under {
                out.push_str("</u>");
            }
            out.push_str(close);
            at = end;
        } else {
            let (start, end) = code[0];
            code = &code[1..];
            out.push_str(&text[at..start]);
            out.push_str(&inline_code(&text[start..end]));
            at = end;
        }
    }
    out.push_str(&text[at..]);
    out
}

/// Which blocks reach the Markdown, and how a change of page is marked.
///
/// The switches [`ExtractOptions`](crate::ExtractOptions) carries for
/// [`PdfDocument::extract_markdown`](crate::PdfDocument::extract_markdown),
/// as the `to_markdown_with_*_options` functions below take them. Default is
/// today's output: no furniture, and every page change a comment.
///
/// `#[non_exhaustive]`, so a switch added here cannot break a caller that
/// never named it: start from [`Default`] and assign the fields you want.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct MarkdownOptions {
    /// Whether [`BlockKind::Header`] blocks are written as plain paragraphs
    /// at their reading-order position. Default false: they are left out,
    /// exactly as without the option. Never as headings: a running head is
    /// not a section title.
    pub include_headers: bool,
    /// Whether [`BlockKind::Footer`] blocks are written as plain paragraphs
    /// at their reading-order position. Default false: they are left out,
    /// exactly as without the option. Never as headings either.
    pub include_footers: bool,
    /// How a change of page is marked. Default
    /// [`PageSeparators::Comment`]: today's `<!-- page N -->` marker.
    pub page_separators: PageSeparators,
    /// Whether monospace runs reach the Markdown as code. Default true:
    /// `` `like this` `` in a paragraph, a fenced block for a paragraph
    /// set wholly in one. False restores the output from before code
    /// existed, byte for byte.
    pub code: bool,
    /// Whether figures are written as image references. Default true.
    ///
    /// False writes no reference at all, the way the furniture reads
    /// without its switches.
    pub include_figures: bool,
    /// How tables reach the Markdown. Default [`TableRenderMode::Auto`]:
    /// pipes where Markdown holds the table's shape, HTML elsewhere --
    /// today's output, byte for byte.
    pub table_render_mode: TableRenderMode,
}

impl Default for MarkdownOptions {
    /// Today's output: no furniture, every page change a comment, and
    /// monospace as code.
    fn default() -> Self {
        Self {
            include_headers: false,
            include_footers: false,
            page_separators: PageSeparators::Comment,
            code: default_code(),
            include_figures: true,
            table_render_mode: TableRenderMode::Auto,
        }
    }
}

impl From<&ExtractOptions> for MarkdownOptions {
    /// The markdown half of an [`ExtractOptions`](crate::ExtractOptions):
    /// what `extract_markdown` serializes with.
    fn from(options: &ExtractOptions) -> Self {
        Self {
            include_headers: options.include_headers,
            include_footers: options.include_footers,
            page_separators: options.page_separators,
            code: options.code,
            include_figures: options.include_figures,
            table_render_mode: options.table_render_mode,
        }
    }
}

/// Every block as markdown, blocks separated by a blank line.
///
/// Preview: output quality is still improving and the API may change in a minor release.
///
/// Headings become `#`..`###`, list items `-` and `1.`, a table a pipe table
/// or an HTML one, a page change a comment, and the furniture nothing at all.
/// [`to_markdown_with_options`] is the same serialisation with the
/// switches: furniture as plain paragraphs, and no page marker at all.
/// Monospace runs become code, as [`ExtractOptions::code`] says by default
/// (see [`to_markdown_with_code`] for the switch).
///
/// Deterministic: the same blocks produce the same bytes, which is what makes a
/// golden test over it possible at all (#572).
///
/// [`ExtractOptions::code`]: crate::blocks::ExtractOptions::code
pub fn to_markdown(blocks: &[Block]) -> String {
    to_markdown_with_options(blocks, &MarkdownOptions::default())
}

/// [`to_markdown`], with the monospace runs as code or as plain text (#636).
///
/// Preview: output quality is still improving and the API may change in a minor release.
///
/// False writes what [`to_markdown`] wrote before code existed: the runs are
/// still found, they are just not marked. The switch
/// [`ExtractOptions::code`] reads.
///
/// [`ExtractOptions::code`]: crate::blocks::ExtractOptions::code
pub fn to_markdown_with_code(blocks: &[Block], code: bool) -> String {
    let options = MarkdownOptions {
        code,
        ..MarkdownOptions::default()
    };
    to_markdown_with_options(blocks, &options)
}

/// Whether monospace reads as code where no caller said otherwise: the one
/// default, [`ExtractOptions::default`], read here so the serializer and the
/// option cannot drift apart.
///
/// [`ExtractOptions::default`]: crate::blocks::ExtractOptions::default
fn default_code() -> bool {
    crate::blocks::ExtractOptions::default().code
}

/// [`to_markdown`], with the horizontal rules each page drew, in page order,
/// as [`crate::PdfDocument::extract_blocks_and_rules`] gives them: a run with
/// a rule drawn under it is written `<u>underlined</u>` (#623).
#[doc(hidden)]
pub fn to_markdown_with_rules(blocks: &[Block], rules: &[Vec<HorizontalRule>]) -> String {
    render_markdown(
        blocks,
        rules,
        crate::learned::SWITCHES,
        &MarkdownOptions::default(),
    )
}

/// [`to_markdown`], with the learned switches given, so a test names the
/// header-block rule its table is read with: the shipped switches read the
/// shipped rule.
#[doc(hidden)]
pub fn to_markdown_with_switches(blocks: &[Block], switches: crate::learned::Switches) -> String {
    render_markdown(blocks, &[], switches, &MarkdownOptions::default())
}

/// [`to_markdown`], with the furniture and the page marker as `options` says.
///
/// Preview: output quality is still improving and the API may change in a minor release.
///
/// With [`MarkdownOptions::include_headers`] the running heads, and with
/// [`MarkdownOptions::include_footers`] the page numbers, are written as
/// plain paragraphs at their reading-order position; with
/// [`PageSeparators::None`] a change of page writes no marker, and the
/// paragraphs on either side stand separated by a blank line.
pub fn to_markdown_with_options(blocks: &[Block], options: &MarkdownOptions) -> String {
    render_markdown(blocks, &[], crate::learned::SWITCHES, options)
}

/// Whether every span in the document is monospaced (lane L1).
///
/// Empty of spans counts as set in monospace, which changes nothing: with
/// no spans there are no code runs to mark either way.
fn document_is_set_in_monospace(blocks: &[Block]) -> bool {
    blocks
        .iter()
        .flat_map(|block| block.spans.iter())
        .all(|span| span.is_monospace == Some(true))
}

/// The walker the entry points share: [`to_markdown`] is this with no
/// rules and [`MarkdownOptions::default`].
fn render_markdown(
    blocks: &[Block],
    rules: &[Vec<HorizontalRule>],
    switches: crate::learned::Switches,
    options: &MarkdownOptions,
) -> String {
    // Lane L1: a document set wholly in monospace is prose, not code.
    // Fences would mark the whole document, so only the fenced form is
    // suppressed; a lone monospaced line still reads inline (SPEC G10).
    let code = options.code;
    let fence_code = code && !document_is_set_in_monospace(blocks);
    let mut out = String::new();
    // The items still open in the run being written, innermost last: the column
    // each was written at, and how wide its marker is. A child is written at
    // the last one's content column, which is the sum of the two, and a list
    // that has closed leaves nothing behind.
    let mut open: Vec<(usize, usize)> = Vec::new();
    // Whether the last thing written was a paragraph, which is the only thing
    // the half of a paragraph on the next page can be joined onto (#571).
    let mut after_paragraph = false;
    // The page the last written block was drawn on, or `None` before the first
    // one: the marker stands at a CHANGE, so there is none above the opening
    // block however far into the document that block's page is.
    let mut last_page: Option<usize> = None;
    // Figures rendered so far per page: a figure's name is its number among
    // its page's figures in block order, the same enumeration the document
    // pass resolves embeds and files by.
    let mut figure_ordinals: std::collections::BTreeMap<usize, usize> =
        std::collections::BTreeMap::new();
    let mut at = 0usize;
    while at < blocks.len() {
        // A table first, because it is the one thing here made of several
        // blocks: the run is written whole and `at` steps over all of it.
        if let Some(end) = table_run_across(blocks, at) {
            let page = blocks[at].page;
            let written = table(&blocks[at..end], switches, options.table_render_mode);
            at = end;
            if written.is_empty() {
                continue;
            }
            note_page(
                &mut out,
                &mut open,
                &mut last_page,
                page,
                options.page_separators,
            );
            open.clear();
            after_paragraph = false;
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str(written.trim_end_matches('\n'));
            continue;
        }
        let block = &blocks[at];
        at += 1;
        // The furniture is out of the flow: a running head between two
        // paragraphs is the break this milestone is about (#571). It is still
        // in the block model, which is where a caller that wants it reads it --
        // or the caller asks `include_headers` / `include_footers`, and it is
        // written below as a plain paragraph, never as a heading.
        let wanted = match block.kind {
            BlockKind::Header => options.include_headers,
            BlockKind::Footer => options.include_footers,
            BlockKind::Figure => options.include_figures,
            _ => true,
        };
        if !wanted {
            continue;
        }
        // A figure is its own paragraph: an image reference, never prose.
        // The target is the block's text where the document pass resolved
        // one -- an embed or a written file -- else the figure's derived
        // name. Figures carry no text of their own, so this stands before
        // the empty-text skip below.
        if matches!(block.kind, BlockKind::Figure) {
            let ordinal = figure_ordinals.get(&block.page).copied().unwrap_or(0);
            let target = block.text.trim();
            let marker;
            let target = if target.is_empty() {
                marker = crate::markdown_images::figure_name(block.page, ordinal);
                marker.as_str()
            } else {
                target
            };
            figure_ordinals.insert(block.page, ordinal + 1);
            note_page(
                &mut out,
                &mut open,
                &mut last_page,
                block.page,
                options.page_separators,
            );
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            after_paragraph = false;
            open.clear();
            out.push_str("![](");
            out.push_str(target);
            out.push(')');
            continue;
        }
        let is_furniture = matches!(block.kind, BlockKind::Header | BlockKind::Footer);
        let text = block.text.trim();
        if text.is_empty() {
            continue;
        }
        // A paragraph set wholly in monospace over two lines or more is a
        // fenced block, and a fence joins onto nothing: it opens its own
        // paragraph even where the page broke one across it (#636).
        let fence = if fence_code {
            fenced_lines(block)
        } else {
            None
        };
        // A paragraph the page broke in two is written as one. The second half
        // is not at the start of a line any more, so it is neither separated
        // nor escaped -- and a word the break was made inside of is put back
        // together, the way `crate::blocks` puts back a word broken across a
        // line. Furniture never joins: a running head is its own paragraph,
        // not the second half of the one above it.
        if block.continues_previous
            && after_paragraph
            && fence.is_none()
            && !out.is_empty()
            && !is_furniture
        {
            // The same test the merge inside a page applies, and for the same
            // reason: a trailing hyphen is a broken word only where the letters
            // on both sides of it say so, and `x-` before `ray` is a compound
            // the page broke at the hyphen it already had (codex, round 1).
            if crate::text::is_hyphen_wrap_candidate(&out, text) {
                out.pop();
            } else {
                out.push(' ');
            }
            out.push_str(&rich_text(block, text, page_rules(rules, block), code));
            // AND THE PAGE IS LEFT UNMARKED RATHER THAN MARKED OFF. The half
            // written here is on the new page, but no marker may stand in
            // front of it, so the page it moved to is still owed one -- and
            // `last_page` is what owes it. Recording the move here paid the
            // debt with nothing: the heading and the paragraphs that followed
            // on that page came out under the page before it, and a chunker
            // reading the markers filed the lot one page early (codex, round
            // 2). Left alone, the next block on the new page opens with the
            // marker, which is where it belongs.
            continue;
        }
        note_page(
            &mut out,
            &mut open,
            &mut last_page,
            block.page,
            options.page_separators,
        );
        if !out.is_empty() {
            // One newline between items of the same list, two everywhere else.
            // A blank line between items makes the list LOOSE in CommonMark,
            // which wraps every item in a paragraph of its own and puts the
            // list's own text a level further down for any reader that walks
            // the tree.
            let tight = !open.is_empty() && matches!(block.kind, BlockKind::ListItem { .. });
            out.push_str(if tight { "\n" } else { "\n\n" });
        }
        after_paragraph = matches!(block.kind, BlockKind::Paragraph);
        match block.kind {
            BlockKind::Heading { level } => {
                open.clear();
                let level = level.clamp(1, 6) as usize;
                out.push_str(&"#".repeat(level));
                out.push(' ');
                out.push_str(text);
            }
            BlockKind::ListItem { level, ordinal } => {
                let depth = (level as usize).min(open.len());
                open.truncate(depth);
                let indent = open.last().map_or(0, |(column, marker)| column + marker);
                let marker = match ordinal {
                    Some(number) => format!("{number}. "),
                    None => "- ".to_string(),
                };
                out.push_str(&" ".repeat(indent));
                out.push_str(&marker);
                open.push((indent, marker.chars().count()));
                // Escaped like any other body text: an item whose own words
                // open `# ` or ``` would otherwise close the item and open a
                // heading or a fence inside the list.
                out.push_str(&body_of(block, text, page_rules(rules, block), code));
            }
            // A CAPTION IS SET APART AS ITALIC, the way a reader sees it under
            // its picture: its own paragraph, emphasised, so a chunker keeps it
            // beside the figure and never reads it as body (#636).
            BlockKind::Caption => {
                open.clear();
                out.push('*');
                out.push_str(body_of(block, text, page_rules(rules, block), code).trim());
                out.push('*');
            }
            _ => {
                open.clear();
                if let Some(lines) = &fence {
                    // A fence is not a paragraph for the join above: the half
                    // of a sentence on the next page opens something new rather
                    // than writing onto a code block.
                    after_paragraph = false;
                    out.push_str(&fenced_block(lines));
                } else {
                    out.push_str(&body_of(block, text, page_rules(rules, block), code));
                }
            }
        }
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_cell_loses_its_dot_leaders_but_keeps_an_ellipsis_and_decimals() {
        use super::without_leaders;
        assert_eq!(
            without_leaders("United States . . . . ........ $11,772"),
            "United States $11,772"
        );
        assert_eq!(
            without_leaders("2005 Plan ......................"),
            "2005 Plan"
        );
        assert_eq!(without_leaders("Net sales"), "Net sales");
        assert_eq!(
            without_leaders("to be determined..."),
            "to be determined..."
        );
        assert_eq!(without_leaders("1.25 . 3.5"), "1.25 . 3.5");
        assert_eq!(without_leaders("....."), "");
        assert_eq!(
            super::cell_text("Dell . . . . . 16", 2),
            "Dell . . . . . 16"
        );
        assert_eq!(super::cell_text("Revenues ...... 954", 3), "Revenues 954");
    }

    fn cell_at(row: usize, column: usize, text: &str) -> super::CellAt<'_> {
        super::CellAt {
            row,
            column,
            row_span: 1,
            column_span: 1,
            header: false,
            text,
        }
    }

    /// Every header switch off, whatever the shipped defaults become: these
    /// tests pin the rule each switch changes, not the shipping state.
    fn off() -> crate::learned::Switches {
        crate::learned::Switches {
            markdown_header_block_figures: false,
            markdown_header_block_rowspan_crossing: false,
            markdown_header_block_word_table: false,
            markdown_header_block_mixed_body: false,
            markdown_headerless_html: false,
            markdown_header_block_stub_years: false,
            ..crate::learned::SWITCHES
        }
    }

    #[test]
    fn a_two_line_header_is_marked_and_a_table_of_words_is_not() {
        use super::header_block;
        let two = [
            cell_at(0, 0, ""),
            cell_at(0, 1, "Issue"),
            cell_at(1, 0, "Class"),
            cell_at(1, 1, "Age"),
            cell_at(2, 0, "MN"),
            cell_at(2, 1, "35"),
            cell_at(3, 0, "MN"),
            cell_at(3, 1, "$1,500.00"),
        ];
        assert_eq!(header_block(&two, off()), 2);
        let words = [
            cell_at(0, 0, "Name"),
            cell_at(0, 1, "Role"),
            cell_at(1, 0, "Ann"),
            cell_at(1, 1, "Chair"),
        ];
        assert_eq!(header_block(&words, off()), 0);
        let figures_first = [
            cell_at(0, 0, "Total"),
            cell_at(0, 1, "12"),
            cell_at(1, 0, "Net"),
            cell_at(1, 1, "(3)"),
        ];
        assert_eq!(header_block(&figures_first, off()), 0);
        assert!(super::is_figure("(1,234.5)") && super::is_figure("12%") && super::is_figure("—"));
        assert!(!super::is_figure("Fair Value") && !super::is_figure("December 31,"));
    }

    /// Figures run together read as figures with the switch
    /// (`markdown_header_block_figures`, #574): every token a figure or a nil
    /// dash, with or without a currency symbol. A bare `$` is an amount's
    /// half, not a figure, and prose is prose.
    #[test]
    fn stacked_figure_cells_count_as_figures_under_the_switch() {
        for text in [
            "86,090 (4,305)",
            "12,000 (1,000)",
            "$—",
            "$ —",
            "$ 990",
            "€ -",
            "— —",
            "(4,305)",
        ] {
            assert!(
                super::is_stacked_figure_cell(text),
                "{text:?} reads as figures"
            );
        }
        for text in [
            "$",
            "",
            "December 31,",
            "Fair Value",
            "Contributions Distributions",
            "86,090 and more",
        ] {
            assert!(
                !super::is_stacked_figure_cell(text),
                "{text:?} reads as text"
            );
        }
    }

    /// DLR p61's fused rows in miniature: the header, a nil row, a fused
    /// figures row, a plain figures row. Without the switch the fused rows
    /// read as text and the header block covers rows 0-2; with it the nil
    /// row ends the header. Red with the rule removed: 3 either way.
    #[test]
    fn fused_figure_rows_end_the_header_block_under_the_switch() {
        use super::header_block;
        let cells = [
            cell_at(0, 0, ""),
            cell_at(0, 1, "Shares"),
            cell_at(0, 2, "Amount"),
            cell_at(1, 0, "Balance at December 31, 2001"),
            cell_at(1, 1, "— —"),
            cell_at(1, 2, "— —"),
            cell_at(2, 0, "Contributions Distributions"),
            cell_at(2, 1, "86,090 (4,305)"),
            cell_at(2, 2, "12,000 (1,000)"),
            cell_at(3, 0, "Net loss"),
            cell_at(3, 1, "(5,000)"),
            cell_at(3, 2, "(6,000)"),
        ];
        assert_eq!(header_block(&cells, off()), 3);
        assert_eq!(
            header_block(
                &cells,
                crate::learned::Switches {
                    markdown_header_block_figures: true,
                    ..off()
                }
            ),
            1
        );
    }

    #[test]
    fn a_row_of_years_does_not_end_the_header() {
        use super::header_block;
        fn spanned(row: usize, column: usize, column_span: usize, text: &str) -> super::CellAt<'_> {
            super::CellAt {
                row,
                column,
                row_span: 1,
                column_span,
                header: false,
                text,
            }
        }
        let years = [
            cell_at(0, 0, ""),
            spanned(0, 1, 3, "Year Ended December 31,"),
            cell_at(1, 0, ""),
            cell_at(1, 1, "2012"),
            cell_at(1, 2, "2011"),
            cell_at(1, 3, "2010"),
            cell_at(2, 0, "Revenue"),
            cell_at(2, 1, "1"),
            cell_at(2, 2, "2"),
            cell_at(2, 3, "3"),
        ];
        assert_eq!(header_block(&years, off()), 2);
        let footnote = [
            cell_at(0, 0, ""),
            spanned(0, 1, 2, "Year Ended December 31,"),
            cell_at(1, 0, ""),
            cell_at(1, 1, "2012 (1)"),
            cell_at(1, 2, "2011"),
            cell_at(2, 0, "Revenue"),
            cell_at(2, 1, "1"),
            cell_at(2, 2, "2"),
        ];
        assert_eq!(header_block(&footnote, off()), 2);
        let not_all_years = [
            cell_at(0, 0, "Item"),
            cell_at(0, 1, "Value"),
            cell_at(0, 2, "Share"),
            cell_at(1, 0, "Total"),
            cell_at(1, 1, "2012"),
            cell_at(1, 2, "35.0%"),
            cell_at(2, 0, "Net"),
            cell_at(2, 1, "3"),
            cell_at(2, 2, "4%"),
        ];
        assert_eq!(header_block(&not_all_years, off()), 1);
    }

    /// A rowspan inside the header or the body keeps the header under the
    /// switch (`markdown_header_block_rowspan_crossing`, #636): only a span
    /// straddling the head boundary vetoes. Red with the rule removed: 0
    /// wherever a cell spans rows.
    #[test]
    fn a_rowspan_clear_of_the_boundary_keeps_the_header_under_the_switch() {
        use super::header_block;
        fn tall(row: usize, column: usize, row_span: usize, text: &str) -> super::CellAt<'_> {
            super::CellAt {
                row,
                column,
                row_span,
                column_span: 1,
                header: false,
                text,
            }
        }
        let on = || crate::learned::Switches {
            markdown_header_block_rowspan_crossing: true,
            ..off()
        };
        // A row group in the body: the stub spans rows 3-4, the head is 2.
        let grouped = [
            cell_at(0, 0, "Region"),
            cell_at(0, 1, "Sales"),
            cell_at(1, 0, "North"),
            cell_at(1, 1, "Q1"),
            cell_at(2, 0, "East"),
            cell_at(2, 1, "12"),
            tall(3, 0, 2, "West"),
            cell_at(3, 1, "9"),
            cell_at(4, 1, "7"),
        ];
        assert_eq!(header_block(&grouped, off()), 0);
        assert_eq!(header_block(&grouped, on()), 2);
        // A group stub two rows high inside the header: no veto either.
        let stub = [
            tall(0, 0, 2, "Test"),
            cell_at(0, 1, "Low"),
            cell_at(1, 1, "High"),
            cell_at(2, 0, "A"),
            cell_at(2, 1, "3"),
        ];
        assert_eq!(header_block(&stub, off()), 0);
        assert_eq!(header_block(&stub, on()), 2);
        // But a span across the boundary still vetoes: row 1 runs on into
        // the body, so the header is not a band of whole rows.
        let straddling = [
            cell_at(0, 0, "Region"),
            cell_at(0, 1, "Sales"),
            tall(1, 0, 2, "North"),
            cell_at(1, 1, "Q1"),
            cell_at(2, 1, "12"),
        ];
        assert_eq!(header_block(&straddling, off()), 0);
        assert_eq!(header_block(&straddling, on()), 0);
    }

    /// A table of words heads its first row under the switch
    /// (`markdown_header_block_word_table`, #636): with no figures row to
    /// end it, the first row is the header by position. Red with the rule
    /// removed: 0, the old word-table veto.
    #[test]
    fn a_word_table_heads_its_first_row_under_the_switch() {
        use super::header_block;
        let on = || crate::learned::Switches {
            markdown_header_block_word_table: true,
            ..off()
        };
        let words = [
            cell_at(0, 0, ""),
            cell_at(0, 1, "Enablers"),
            cell_at(0, 2, "Barriers"),
            cell_at(1, 0, "Hiring"),
            cell_at(1, 1, "Referrals"),
            cell_at(1, 2, "Costs"),
            cell_at(2, 0, "Keeping"),
            cell_at(2, 1, "Pay"),
            cell_at(2, 2, "Hours"),
        ];
        assert_eq!(header_block(&words, off()), 0);
        assert_eq!(header_block(&words, on()), 1);
        // One row is no table with a header: the block would be the table.
        let single = [cell_at(0, 0, "Name"), cell_at(0, 1, "Role")];
        assert_eq!(header_block(&single, on()), 0);
        // And a first row with no text in it heads nothing.
        let blank = [
            cell_at(0, 0, ""),
            cell_at(0, 1, " "),
            cell_at(1, 0, "Ann"),
            cell_at(1, 1, "Chair"),
        ];
        assert_eq!(header_block(&blank, on()), 0);
    }

    /// A row mixing labels and figures ends the header under the switch
    /// (`markdown_header_block_mixed_body`, #636): it has already turned
    /// from label to value. Red with the rule removed: the mixed row reads
    /// as labels and the header runs past it.
    #[test]
    fn a_mixed_row_ends_the_header_under_the_switch() {
        use super::header_block;
        let on = || crate::learned::Switches {
            markdown_header_block_mixed_body: true,
            ..off()
        };
        let over = [
            cell_at(0, 0, "Region"),
            cell_at(0, 1, "North"),
            cell_at(0, 2, "South"),
            cell_at(0, 3, "Share"),
            cell_at(1, 0, "Age (years)"),
            cell_at(1, 1, "North"),
            cell_at(1, 2, "South"),
            cell_at(1, 3, "0.73"),
            cell_at(2, 0, "Female"),
            cell_at(2, 1, "26"),
            cell_at(2, 2, "20"),
            cell_at(2, 3, "0.22"),
        ];
        assert_eq!(header_block(&over, off()), 2);
        assert_eq!(header_block(&over, on()), 1);
        // A headerless grid, label beside value from the first row on:
        // nothing is marked.
        let headerless = [
            cell_at(0, 0, "Genome size"),
            cell_at(0, 1, "North"),
            cell_at(0, 2, "South"),
            cell_at(0, 3, "14.6"),
            cell_at(1, 0, "Assembled"),
            cell_at(1, 1, "13"),
            cell_at(1, 2, "12"),
            cell_at(1, 3, "11"),
        ];
        assert_eq!(header_block(&headerless, off()), 1);
        assert_eq!(header_block(&headerless, on()), 0);
        // But a stubbed year row is no mixed row: years head columns, so a
        // bare year beside labels does not turn the row to values. Red with
        // the year guard removed: 1, the header demoted to one row.
        let stubbed_years = [
            cell_at(0, 0, "Payments due by period"),
            cell_at(0, 1, ""),
            cell_at(0, 2, ""),
            cell_at(1, 0, "Contractual Obligation"),
            cell_at(1, 1, "2008"),
            cell_at(1, 2, "2009 and 2010"),
            cell_at(2, 0, "Long-term debt"),
            cell_at(2, 1, "$ 629"),
            cell_at(2, 2, "$ 2,334"),
        ];
        assert_eq!(header_block(&stubbed_years, off()), 2);
        assert_eq!(header_block(&stubbed_years, on()), 2);
    }

    /// A stubbed year row heads under the switch
    /// (`markdown_header_block_stub_years`, #636): the years in the value
    /// columns head them, whatever the stub cell holds. Red with the rule
    /// removed: 0, the years read as figures in the first row.
    #[test]
    fn a_stubbed_year_row_heads_under_the_switch() {
        use super::header_block;
        let on = || crate::learned::Switches {
            markdown_header_block_stub_years: true,
            ..off()
        };
        let stubbed = [
            cell_at(0, 0, "($ in millions)"),
            cell_at(0, 1, "2010"),
            cell_at(0, 2, "2009"),
            cell_at(1, 0, "Sales"),
            cell_at(1, 1, "$ 914"),
            cell_at(1, 2, "$ 850"),
        ];
        assert_eq!(header_block(&stubbed, off()), 0);
        assert_eq!(header_block(&stubbed, on()), 1);
        // One year is no year row: the stub variant keeps the old bar of two.
        let single = [
            cell_at(0, 0, "As of"),
            cell_at(0, 1, "December"),
            cell_at(0, 2, "2010"),
            cell_at(1, 0, "Cash"),
            cell_at(1, 1, "1"),
            cell_at(1, 2, "2"),
        ];
        assert_eq!(header_block(&single, on()), 1);
        // And a plain all-years row heads with or without the switch.
        let plain = [
            cell_at(0, 0, "2010"),
            cell_at(0, 1, "2009"),
            cell_at(1, 0, "Sales"),
            cell_at(1, 1, "914"),
        ];
        assert_eq!(header_block(&plain, off()), 1);
        assert_eq!(header_block(&plain, on()), 1);
    }

    use super::*;

    fn block(kind: BlockKind, text: &str) -> Block {
        Block {
            kind,
            page: 0,
            bbox: [0.0; 4],
            text: text.to_string(),
            spans: Vec::new(),
            index: 0,
            continues_previous: false,
            confidence: None,
            grid: None,
            table_origin: TableOrigin::Other,
            trace_learned: None,
        }
    }

    /// A caption is set apart as an italic paragraph of its own (#636).
    #[test]
    fn a_caption_is_an_italic_paragraph_of_its_own() {
        let md = to_markdown(&[
            block(BlockKind::Paragraph, "The plant runs day and night."),
            block(BlockKind::Caption, "Figure 1. The plant at night."),
        ]);
        assert_eq!(
            md,
            "The plant runs day and night.\n\n*Figure 1. The plant at night.*\n"
        );
    }

    fn run(text: &str, y: f64, size: f64, bold: bool, italic: bool) -> crate::text::TextSpan {
        crate::text::TextSpan {
            text: text.to_string(),
            y,
            font_size: size,
            is_bold: bold,
            is_italic: italic,
            ..Default::default()
        }
    }

    fn set_block(text: &str, spans: Vec<crate::text::TextSpan>) -> Block {
        Block {
            spans,
            ..block(BlockKind::Paragraph, text)
        }
    }

    fn placed(text: &str, x: f64, width: f64, bold: bool) -> crate::text::TextSpan {
        crate::text::TextSpan {
            x,
            width,
            ..run(text, 700.0, 10.0, bold, false)
        }
    }

    fn rule(left: f64, right: f64, y: f64) -> HorizontalRule {
        HorizontalRule { left, right, y }
    }

    #[test]
    fn a_rule_under_a_run_underlines_it_and_a_border_does_not() {
        let b = set_block(
            "See the terms of service before signing.",
            vec![
                placed("See the", 100.0, 35.0, false),
                placed("terms of service", 140.0, 80.0, true),
                placed("before signing.", 225.0, 75.0, false),
            ],
        );
        // Under the bold run, just below the baseline: both marks, nested.
        assert_eq!(
            styled(&b, &b.text, &[rule(140.0, 220.0, 698.5)]),
            "See the **<u>terms of service</u>** before signing."
        );
        // A table border runs far past the line: not an underline.
        assert_eq!(
            styled(&b, &b.text, &[rule(40.0, 560.0, 698.5)]),
            "See the **terms of service** before signing."
        );
        // A rule a line below is the next line's business.
        assert_eq!(
            styled(&b, &b.text, &[rule(140.0, 220.0, 688.0)]),
            "See the **terms of service** before signing."
        );
        // An underline over the plain run marks it alone.
        assert_eq!(
            styled(&b, &b.text, &[rule(225.0, 300.0, 699.0)]),
            "See the **terms of service** <u>before signing.</u>"
        );
    }

    #[test]
    fn bold_and_italic_runs_are_marked_from_the_spans() {
        let b = set_block(
            "Pay the net amount within thirty days of the invoice.",
            vec![
                run("Pay the", 700.0, 10.0, false, false),
                run("net amount", 700.0, 10.0, true, false),
                run("within", 700.0, 10.0, false, false),
                run("thirty days", 700.0, 10.0, false, true),
                run("of the invoice.", 700.0, 10.0, false, false),
            ],
        );
        assert_eq!(
            to_markdown(&[b]),
            "Pay the **net amount** within *thirty days* of the invoice.\n"
        );
    }

    #[test]
    fn sub_word_spans_the_rebuild_joined_are_marked_whole() {
        // The rebuild joins `2 0 2 5` to `2025`; the marks follow the same
        // boundary, so the joined word reads bold (#668).
        let b = set_block(
            "2025",
            vec![
                placed("2", 100.0, 3.0, true),
                placed("0", 103.0, 3.0, true),
                placed("2", 106.0, 3.0, true),
                placed("5", 109.0, 3.0, true),
            ],
        );
        assert_eq!(styled(&b, &b.text, &[]), "**2025**");
    }

    #[test]
    fn a_hyphen_off_its_line_is_still_half_a_word() {
        // The joined-word mark needs the shared boundary: `hyphen-` sits a
        // line above `ated`, so it stays unmarked the way it always was.
        let b = set_block(
            "hyphen-ated",
            vec![
                placed("hyphen-", 100.0, 35.0, true),
                crate::text::TextSpan {
                    x: 100.0,
                    width: 20.0,
                    ..run("ated", 688.0, 10.0, false, false)
                },
            ],
        );
        assert_eq!(styled(&b, &b.text, &[]), "hyphen-ated");
    }

    #[test]
    fn neighbouring_runs_of_one_setting_are_one_mark() {
        let b = set_block(
            "Two bold words here.",
            vec![
                run("Two", 700.0, 10.0, true, false),
                run("bold", 700.0, 10.0, true, false),
                run("words here.", 700.0, 10.0, false, false),
            ],
        );
        assert_eq!(to_markdown(&[b]), "**Two bold** words here.\n");
    }

    #[test]
    fn raised_and_lowered_runs_are_sup_and_sub() {
        let b = set_block(
            "Water is H2O and the area is 4 m2 here.",
            vec![
                run("Water is H", 700.0, 10.0, false, false),
                run("2", 697.0, 7.0, false, false),
                run("O and the area is 4 m", 700.0, 10.0, false, false),
                run("2", 703.5, 7.0, false, false),
                run("here.", 700.0, 10.0, false, false),
            ],
        );
        assert_eq!(
            to_markdown(&[b]),
            "Water is H<sub>2</sub>O and the area is 4 m<sup>2</sup> here.\n"
        );
    }

    #[test]
    fn a_line_that_opens_like_a_list_item_keeps_its_escape_and_no_style() {
        let b = set_block(
            "3. The parties agree.",
            vec![
                run("3.", 700.0, 10.0, true, false),
                run("The parties agree.", 700.0, 10.0, false, false),
            ],
        );
        assert_eq!(to_markdown(&[b]), "3\\. The parties agree.\n");
    }

    #[test]
    fn a_small_run_opening_the_next_line_is_not_a_subscript() {
        let b = set_block(
            "the end of a line and 12 more words",
            vec![
                run("the end of a line and", 700.0, 10.0, false, false),
                run("12", 688.0, 8.0, false, false),
                run("more words", 688.0, 10.0, false, false),
            ],
        );
        assert_eq!(to_markdown(&[b]), "the end of a line and 12 more words\n");
    }

    #[test]
    fn a_bold_run_ending_in_a_page_break_hyphen_still_joins() {
        let mut first = set_block(
            "the word is hyphen-",
            vec![
                run("the word is", 700.0, 10.0, false, false),
                run("hyphen-", 700.0, 10.0, true, false),
            ],
        );
        first.page = 0;
        let mut second = block(BlockKind::Paragraph, "ated across the page.");
        second.page = 1;
        second.continues_previous = true;
        assert_eq!(
            to_markdown(&[first, second]),
            "the word is hyphenated across the page.\n"
        );
    }

    #[test]
    fn right_to_left_text_is_left_unmarked() {
        let b = set_block(
            "שלום עולם",
            vec![
                run("עולם", 700.0, 10.0, true, false),
                run("שלום", 700.0, 10.0, false, false),
            ],
        );
        assert_eq!(to_markdown(&[b]), "שלום עולם\n");
    }

    #[test]
    fn a_run_that_holds_markup_or_cannot_be_found_is_left_alone() {
        let b = set_block(
            "Use a*b and the hyphen-ated word.",
            vec![
                run("Use a*b", 700.0, 10.0, true, false),
                run("hyphen-", 700.0, 10.0, true, false),
            ],
        );
        assert_eq!(to_markdown(&[b]), "Use a*b and the hyphen-ated word.\n");
    }

    fn mono(text: &str, x: f64, y: f64) -> crate::text::TextSpan {
        crate::text::TextSpan {
            text: text.to_string(),
            x,
            y,
            width: text.len().max(1) as f64 * 6.0,
            font_size: 10.0,
            is_monospace: Some(true),
            ..Default::default()
        }
    }

    fn plain(text: &str, x: f64, y: f64) -> crate::text::TextSpan {
        crate::text::TextSpan {
            text: text.to_string(),
            x,
            y,
            width: text.len().max(1) as f64 * 6.0,
            font_size: 10.0,
            ..Default::default()
        }
    }

    #[test]
    fn a_monospace_word_in_a_sentence_is_inline_code() {
        let b = set_block(
            "Use foo here.",
            vec![
                plain("Use ", 100.0, 700.0),
                mono("foo", 130.0, 700.0),
                plain(" here.", 150.0, 700.0),
            ],
        );
        assert_eq!(to_markdown(&[b]), "Use `foo` here.\n");
    }

    #[test]
    fn neighbouring_monospace_runs_are_one_span() {
        let b = set_block(
            "Run foo bar now.",
            vec![
                plain("Run ", 100.0, 700.0),
                mono("foo", 130.0, 700.0),
                mono("bar", 155.0, 700.0),
                plain(" now.", 180.0, 700.0),
            ],
        );
        assert_eq!(to_markdown(&[b]), "Run `foo bar` now.\n");
    }

    #[test]
    fn a_backtick_in_code_lengthens_the_fence() {
        let b = set_block(
            "Use a`b here.",
            vec![
                plain("Use ", 100.0, 700.0),
                mono("a`b", 130.0, 700.0),
                plain(" here.", 155.0, 700.0),
            ],
        );
        assert_eq!(to_markdown(&[b]), "Use ``a`b`` here.\n");
    }

    #[test]
    fn code_wins_over_bold_on_the_same_run() {
        let mut code = mono("foo", 130.0, 700.0);
        code.is_bold = true;
        let b = set_block(
            "Use foo here.",
            vec![
                plain("Use ", 100.0, 700.0),
                code,
                plain(" here.", 150.0, 700.0),
            ],
        );
        assert_eq!(to_markdown(&[b]), "Use `foo` here.\n");
    }

    #[test]
    fn code_at_the_opening_needs_no_escape() {
        let b = set_block("# foo", vec![mono("# foo", 100.0, 700.0)]);
        assert_eq!(to_markdown(&[b]), "`# foo`\n");
    }

    #[test]
    fn a_list_item_reads_its_monospace_inline() {
        let item = block(
            BlockKind::ListItem {
                level: 0,
                ordinal: None,
            },
            "Run foo now.",
        );
        let item = Block {
            spans: vec![
                plain("Run ", 100.0, 700.0),
                mono("foo", 130.0, 700.0),
                plain(" now.", 150.0, 700.0),
            ],
            ..item
        };
        assert_eq!(to_markdown(&[item]), "- Run `foo` now.\n");
    }

    #[test]
    fn an_all_monospace_paragraph_over_two_lines_reads_inline() {
        // Lane L1: the whole-document typewriter switch reads a wholly
        // monospaced document as prose, so this block is inline code now,
        // fenced only against a proportional face.
        let b = set_block(
            "let x = 1; let y = 2;",
            vec![
                mono("let x = 1;", 100.0, 700.0),
                mono("let y = 2;", 100.0, 688.0),
            ],
        );
        assert_eq!(to_markdown(&[b]), "`let x = 1; let y = 2;`\n");
    }

    #[test]
    fn a_fence_inside_an_unfenced_block_stays_inline() {
        // Lane L1: as above, the typewriter switch leaves a wholly
        // monospaced block unfenced; the backticks read inline.
        let b = set_block(
            "``` code",
            vec![mono("```", 100.0, 700.0), mono("code", 100.0, 688.0)],
        );
        assert_eq!(
            to_markdown(std::slice::from_ref(&b)),
            "```` ``` code ````\n"
        );
        // Against a proportional face the block still fences, and the
        // fence still lengthens past the backticks it holds.
        let prose = set_block("words", vec![plain("words", 100.0, 712.0)]);
        assert_eq!(to_markdown(&[prose, b]), "words\n\n````\n```\ncode\n````\n");
    }

    #[test]
    fn a_single_monospace_line_stays_inline() {
        let b = set_block("print(x)", vec![mono("print(x)", 100.0, 700.0)]);
        assert_eq!(to_markdown(&[b]), "`print(x)`\n");
    }

    #[test]
    fn a_paragraph_only_partly_monospace_stays_inline() {
        let b = set_block(
            "code_x words here",
            vec![
                mono("code_x", 100.0, 700.0),
                plain("words here", 100.0, 688.0),
            ],
        );
        assert_eq!(to_markdown(&[b]), "`code_x` words here\n");
    }

    #[test]
    fn monospace_figures_are_not_code() {
        let b = set_block(
            "1,234 5,678 9,012",
            vec![mono("1,234 5,678 9,012", 100.0, 700.0)],
        );
        assert_eq!(to_markdown(&[b]), "1,234 5,678 9,012\n");
        // Neither fenced over two lines: the guard skips the block, not the span.
        let b = set_block(
            "1,234 5,678",
            vec![mono("1,234", 100.0, 700.0), mono("5,678", 100.0, 688.0)],
        );
        assert_eq!(to_markdown(&[b]), "1,234 5,678\n");
    }

    #[test]
    fn tables_headings_and_captions_never_carry_code() {
        let mut code = cell(1, 0, false, "a1");
        code.spans = vec![mono("a1", 100.0, 700.0)];
        let table = vec![
            cell(0, 0, true, "Code"),
            cell(0, 1, true, "Means"),
            code,
            cell(1, 1, false, "either"),
        ];
        assert_eq!(
            to_markdown(&table),
            "| Code | Means |\n| --- | --- |\n| a1 | either |\n"
        );
        let heading = block(BlockKind::Heading { level: 1 }, "Setup foo.");
        let heading = Block {
            spans: vec![
                plain("Setup ", 100.0, 700.0),
                mono("foo", 150.0, 700.0),
                plain(".", 170.0, 700.0),
            ],
            ..heading
        };
        assert_eq!(to_markdown(&[heading]), "# Setup foo.\n");
        let caption = block(BlockKind::Caption, "Figure 1 shows foo.");
        let caption = Block {
            spans: vec![
                plain("Figure 1 shows ", 100.0, 700.0),
                mono("foo", 200.0, 700.0),
                plain(".", 220.0, 700.0),
            ],
            ..caption
        };
        assert_eq!(to_markdown(&[caption]), "*Figure 1 shows foo.*\n");
    }

    #[test]
    fn code_false_writes_todays_bytes() {
        let b = set_block(
            "Use foo here.",
            vec![
                plain("Use ", 100.0, 700.0),
                mono("foo", 130.0, 700.0),
                plain(" here.", 150.0, 700.0),
            ],
        );
        assert_eq!(
            to_markdown_with_code(std::slice::from_ref(&b), false),
            "Use foo here.\n"
        );
        let fenced = set_block(
            "let x = 1; let y = 2;",
            vec![
                mono("let x = 1;", 100.0, 700.0),
                mono("let y = 2;", 100.0, 688.0),
            ],
        );
        assert_eq!(
            to_markdown_with_code(&[fenced], false),
            "let x = 1; let y = 2;\n"
        );
        assert_eq!(to_markdown(&[b]), "Use `foo` here.\n");
    }

    #[test]
    fn a_code_span_never_closes_after_a_page_break_hyphen() {
        let mut first = set_block(
            "the setter is config-",
            vec![
                plain("the setter is ", 100.0, 700.0),
                mono("config-", 190.0, 700.0),
            ],
        );
        first.page = 0;
        let mut second = block(BlockKind::Paragraph, "file on the next page.");
        second.page = 1;
        second.continues_previous = true;
        assert_eq!(
            to_markdown(&[first, second]),
            "the setter is configfile on the next page.\n"
        );
    }

    #[test]
    fn heading_levels_become_one_to_three_hashes() {
        let blocks = vec![
            block(BlockKind::Heading { level: 1 }, "Scope"),
            block(BlockKind::Paragraph, "This document describes."),
            block(BlockKind::Heading { level: 2 }, "Terms"),
            block(BlockKind::Heading { level: 3 }, "Party"),
        ];
        assert_eq!(
            to_markdown(&blocks),
            "# Scope\n\nThis document describes.\n\n## Terms\n\n### Party\n"
        );
    }

    #[test]
    fn a_paragraph_that_opens_like_a_marker_is_escaped() {
        let blocks = vec![
            block(BlockKind::Paragraph, "# not a heading"),
            block(BlockKind::Paragraph, "- not an item"),
            block(BlockKind::Paragraph, "1. not an item"),
            block(BlockKind::Paragraph, "1984 was a year"),
        ];
        assert_eq!(
            to_markdown(&blocks),
            "\\# not a heading\n\n\\- not an item\n\n1\\. not an item\n\n1984 was a year\n"
        );
    }

    #[test]
    fn a_figure_at_the_head_of_a_line_is_not_escaped() {
        // A marker is the character AND the space after it. Without that
        // clause every figure a document sets at the head of a line carries a
        // backslash into the text layer of everything downstream -- 79 of them
        // over the 200 pages of run structured-doclaynet-20260921T000905Z,
        // against a ground truth that has none (#579).
        let blocks = vec![
            block(BlockKind::Paragraph, "-3,503 6,200"),
            block(BlockKind::Paragraph, "0.1 0.01 0.001"),
            block(BlockKind::Paragraph, "#4 of the series"),
            block(BlockKind::Paragraph, "+3HnN is the rate"),
            block(BlockKind::Paragraph, "2.94 0.98 0.88"),
            // Seven hashes is prose in CommonMark, which stops counting at six.
            block(BlockKind::Paragraph, "####### not a heading"),
        ];
        assert_eq!(
            to_markdown(&blocks),
            "-3,503 6,200\n\n0.1 0.01 0.001\n\n#4 of the series\n\n\
             +3HnN is the rate\n\n2.94 0.98 0.88\n\n####### not a heading\n"
        );
    }

    #[test]
    fn a_heading_that_opens_with_a_run_of_hashes_is_still_escaped() {
        // The mutation the case above pays for: asking for the space after the
        // FIRST hash rather than after the run lets `## Terms` out as a
        // level-two heading the document does not have.
        let blocks = vec![
            block(BlockKind::Paragraph, "## Terms"),
            block(BlockKind::Paragraph, "###### Sixth"),
            block(BlockKind::Paragraph, "###"),
        ];
        assert_eq!(
            to_markdown(&blocks),
            "\\## Terms\n\n\\###### Sixth\n\n\\###\n"
        );
    }

    #[test]
    fn a_rule_of_three_is_escaped_whatever_follows_it() {
        // The other half of the same clause: `---` and `***` are a thematic
        // break with no space in them anywhere, and a document that sets one
        // as prose would otherwise draw a line across the page.
        let blocks = vec![
            block(BlockKind::Paragraph, "---"),
            block(BlockKind::Paragraph, "***"),
            block(BlockKind::Paragraph, "-- an em rule"),
        ];
        assert_eq!(to_markdown(&blocks), "\\---\n\n\\***\n\n-- an em rule\n");
    }

    #[test]
    fn a_paragraph_that_opens_a_fence_cannot_swallow_the_document() {
        let blocks = vec![
            block(BlockKind::Paragraph, "```rust"),
            block(BlockKind::Heading { level: 1 }, "Scope"),
            block(BlockKind::Paragraph, "~~~ a tilde fence"),
            block(BlockKind::Paragraph, "`inline code` is not a fence"),
        ];
        let markdown = to_markdown(&blocks);
        assert!(
            markdown.starts_with("\\```rust"),
            "the fence opens a block nothing closes: {markdown}"
        );
        assert!(
            markdown.contains("\n# Scope\n"),
            "the heading after the fence was swallowed: {markdown}"
        );
        assert!(markdown.contains("\\~~~ a tilde fence"), "{markdown}");
        assert!(markdown.contains("\\`inline code`"), "{markdown}");
    }

    fn cell(row: u16, column: u16, header: bool, text: &str) -> Block {
        block(
            BlockKind::TableCell {
                row,
                column,
                row_span: 1,
                column_span: 1,
                header,
            },
            text,
        )
    }

    /// A rectangle with no header row is HTML under the switch
    /// (`markdown_headerless_html`, #636): the pipe form would force its
    /// first row into a header it does not have. Red with the rule removed:
    /// a pipe table either way.
    #[test]
    fn a_headerless_rectangle_is_html_under_the_switch() {
        let blocks = vec![
            cell(0, 0, false, "Genome size"),
            cell(0, 1, false, "14.6"),
            cell(1, 0, false, "Assembled"),
            cell(1, 1, false, "13"),
        ];
        assert_eq!(
            super::to_markdown_with_switches(&blocks, off()),
            "| Genome size | 14.6 |\n| --- | --- |\n| Assembled | 13 |\n"
        );
        let on = crate::learned::Switches {
            markdown_headerless_html: true,
            ..off()
        };
        let html = super::to_markdown_with_switches(&blocks, on);
        assert!(
            html.starts_with("<table>"),
            "a headerless table is HTML, not pipes: {html:?}"
        );
        assert!(
            !html.contains("<th"),
            "no cell is promoted to a header: {html:?}"
        );
    }

    #[test]
    fn a_pipe_in_a_cell_does_not_open_a_column() {
        let blocks = vec![
            cell(0, 0, true, "Code"),
            cell(0, 1, true, "Means"),
            cell(1, 0, false, "a|b"),
            cell(1, 1, false, "either"),
        ];
        assert_eq!(
            to_markdown(&blocks),
            "| Code | Means |\n| --- | --- |\n| a\\|b | either |\n"
        );
    }

    #[test]
    fn a_cell_that_holds_markup_is_escaped_in_the_html_form() {
        let mut blocks = vec![
            block(
                BlockKind::TableCell {
                    row: 0,
                    column: 0,
                    row_span: 1,
                    column_span: 2,
                    header: true,
                },
                "a < b & c",
            ),
            cell(1, 0, false, "left"),
            cell(1, 1, false, "right"),
        ];
        blocks.push(cell(2, 0, false, "under"));
        blocks.push(cell(2, 1, false, "it"));
        assert_eq!(
            to_markdown(&blocks),
            "<table>\n\
             <tr><th colspan=\"2\">a &lt; b &amp; c</th></tr>\n\
             <tr><td>left</td><td>right</td></tr>\n\
             <tr><td>under</td><td>it</td></tr>\n\
             </table>\n"
        );
    }

    #[test]
    fn a_currency_column_joins_the_column_to_its_right() {
        let joined = vec![
            cell(0, 0, false, "Service cost"),
            cell(0, 1, false, "$"),
            cell(0, 2, false, "115"),
            cell(1, 0, false, "Interest"),
            cell(1, 1, false, ""),
            cell(1, 2, false, "82"),
        ];
        let markdown = to_markdown(&joined);
        assert!(markdown.contains("$ 115"), "{markdown}");
        // HTML, not pipes: both rows are label beside value, so the grid has
        // no header row and the pipe form would force one (#636).
        assert!(markdown.starts_with("<table>"), "{markdown}");
        assert!(!markdown.contains("<th"), "{markdown}");
        assert!(!markdown.contains("<td>$</td>"), "{markdown}");
        let not_currency = vec![
            cell(0, 0, false, "A"),
            cell(0, 1, false, "$"),
            cell(0, 2, false, "1"),
            cell(1, 0, false, "B"),
            cell(1, 1, false, "12"),
            cell(1, 2, false, "2"),
        ];
        let markdown = to_markdown(&not_currency);
        assert!(markdown.contains("<td>$</td>"), "{markdown}");
        assert!(!markdown.contains("$ 1"), "{markdown}");
        let spanning = vec![
            cell(0, 0, false, "Service"),
            cell(0, 1, false, "$"),
            block(
                BlockKind::TableCell {
                    row: 0,
                    column: 2,
                    row_span: 1,
                    column_span: 2,
                    header: false,
                },
                "115",
            ),
            cell(1, 0, false, "Interest"),
            cell(1, 1, false, ""),
            cell(1, 2, false, "82"),
            cell(1, 3, false, "83"),
        ];
        let markdown = to_markdown(&spanning);
        assert!(!markdown.contains("$ 115"), "{markdown}");
    }

    #[test]
    fn a_header_span_over_the_join_shrinks_with_it_and_a_partial_span_blocks() {
        fn spanned(row: u16, column: u16, column_span: u16, text: &str) -> Block {
            block(
                BlockKind::TableCell {
                    row,
                    column,
                    row_span: 1,
                    column_span,
                    header: false,
                },
                text,
            )
        }
        // CMG_2010 p56 t0 as cells: a header over columns 1..6, three `$`
        // columns at 1, 3, 5 each beside its figures. The span contains every
        // join, so all three fire and it shrinks 6 -> 3 over columns 1..3.
        let under_span = vec![
            cell(0, 0, false, "Item"),
            spanned(0, 1, 6, "For the years ended December 31"),
            cell(1, 0, false, "Net sales"),
            cell(1, 1, false, "$"),
            cell(1, 2, false, "114,750"),
            cell(1, 3, false, "$"),
            cell(1, 4, false, "101,029"),
            cell(1, 5, false, "$"),
            cell(1, 6, false, "90,547"),
            cell(2, 0, false, "Cost of sales"),
            cell(2, 1, false, ""),
            cell(2, 2, false, "86,133"),
            cell(2, 3, false, ""),
            cell(2, 4, false, "74,920"),
            cell(2, 5, false, ""),
            cell(2, 6, false, "66,610"),
        ];
        let markdown = to_markdown(&under_span);
        assert!(
            markdown.contains("<th colspan=\"3\">For the years ended December 31</th>"),
            "the span shrinks with the three columns it covers: {markdown}"
        );
        assert!(markdown.contains("$ 114,750"), "{markdown}");
        assert!(
            !markdown.contains(">$<"),
            "no bare currency column survives: {markdown}"
        );
        // A span ending exactly at the currency column still blocks: the grid
        // beside it is not one cell per row.
        let partial = vec![
            spanned(0, 0, 2, "Sales"),
            cell(0, 2, false, "Amount"),
            cell(1, 0, false, "Net"),
            cell(1, 1, false, "$"),
            cell(1, 2, false, "1"),
        ];
        let markdown = to_markdown(&partial);
        assert!(
            !markdown.contains("$ 1"),
            "a partial span still blocks the join: {markdown}"
        );
        assert!(
            markdown.contains(">$<"),
            "the blocked currency column survives: {markdown}"
        );
    }

    /// Two tables with nothing between them are two tables: the second opens
    /// at row 0, column 0, which is the only thing that says so.
    #[test]
    fn a_second_table_starts_where_the_grid_starts_again() {
        let blocks = vec![
            cell(0, 0, false, "one"),
            cell(0, 1, false, "two"),
            cell(1, 0, false, "three"),
            cell(1, 1, false, "four"),
            cell(0, 0, false, "five"),
            cell(0, 1, false, "six"),
            cell(1, 0, false, "seven"),
            cell(1, 1, false, "eight"),
        ];
        let markdown = to_markdown(&blocks);
        assert_eq!(markdown.matches("| --- | --- |").count(), 2, "{markdown}");
        assert!(markdown.contains("| four |\n\n| five |"), "{markdown}");
    }

    /// A table whose first row opens with an empty cell has no block at row 0,
    /// column 0 at all, so a run that ended only there ran straight into it and
    /// read two grids as one -- with duplicate coordinates, and the second
    /// table's cells dropped (codex, round 2).
    #[test]
    fn a_second_table_that_opens_with_a_hole_is_still_a_second_table() {
        let blocks = vec![
            cell(0, 0, false, "one"),
            cell(0, 1, false, "two"),
            cell(1, 0, false, "three"),
            cell(1, 1, false, "four"),
            cell(0, 1, false, "six"),
            cell(1, 0, false, "seven"),
            cell(1, 1, false, "eight"),
        ];
        let markdown = to_markdown(&blocks);
        assert_eq!(
            markdown.matches("<table>").count()
                + markdown
                    .lines()
                    .filter(|line| line.starts_with("| ---"))
                    .count(),
            2,
            "{markdown}"
        );
        for word in ["one", "four", "six", "eight"] {
            assert!(markdown.contains(word), "{word} was dropped: {markdown}");
        }
    }

    fn on_page(kind: BlockKind, text: &str, page: usize) -> Block {
        Block {
            page,
            ..block(kind, text)
        }
    }

    /// A document that quotes an HTML comment must not be able to write our
    /// own page marker as body text: a chunker following the convention would
    /// file everything after it under a page the document never had.
    #[test]
    fn a_paragraph_that_quotes_the_page_marker_is_not_one() {
        let blocks = vec![
            block(BlockKind::Paragraph, "<!-- page 7 -->"),
            block(BlockKind::Paragraph, "<table> opens a block too"),
        ];
        let markdown = to_markdown(&blocks);
        assert_eq!(
            markdown,
            "\\<!-- page 7 -->\n\n\\<table> opens a block too\n"
        );
        assert!(
            !markdown.lines().any(|line| line == "<!-- page 7 -->"),
            "document text came out as a marker: {markdown}"
        );
    }

    #[test]
    fn a_page_change_is_marked_and_the_first_page_is_not() {
        let blocks = vec![
            on_page(BlockKind::Paragraph, "The first page ends here.", 0),
            on_page(BlockKind::Paragraph, "The second opens here.", 1),
        ];
        assert_eq!(
            to_markdown(&blocks),
            "The first page ends here.\n\n<!-- page 1 -->\n\nThe second opens here.\n"
        );
    }

    /// The marker counts the page the way the block model does, from zero, so
    /// that a caller holding both serialisations of one document finds the same
    /// page in each.
    #[test]
    fn the_number_in_the_marker_is_the_block_s_own_page() {
        let blocks = vec![
            on_page(BlockKind::Paragraph, "Opening.", 4),
            on_page(BlockKind::Paragraph, "Next.", 7),
        ];
        assert!(to_markdown(&blocks).contains("<!-- page 7 -->"));
    }

    /// The half of a sentence on the next page is written onto the first half,
    /// and a marker between them would cut the paragraph the joining exists to
    /// keep whole (#571). The page still MOVED, which the marker after it has
    /// to say: measured from the first page it would name a page two behind.
    #[test]
    fn a_paragraph_the_break_cut_in_two_is_not_cut_again_by_the_marker() {
        let mut middle = on_page(BlockKind::Paragraph, "to its end.", 1);
        middle.continues_previous = true;
        let blocks = vec![
            on_page(BlockKind::Paragraph, "The sentence runs on", 0),
            middle,
            on_page(BlockKind::Paragraph, "A new one.", 2),
        ];
        assert_eq!(
            to_markdown(&blocks),
            "The sentence runs on to its end.\n\n<!-- page 2 -->\n\nA new one.\n"
        );
    }

    /// The half of the paragraph on the next page cannot have a marker in
    /// front of it, but the page it is on still has to be named: the block
    /// AFTER it opens with the marker. Without that the whole of the new page
    /// reads as the page before it (codex, round 2).
    #[test]
    fn the_deferred_marker_stands_after_the_joined_paragraph() {
        let mut middle = on_page(BlockKind::Paragraph, "to its end.", 1);
        middle.continues_previous = true;
        let blocks = vec![
            on_page(BlockKind::Paragraph, "The sentence runs on", 0),
            middle,
            on_page(BlockKind::Heading { level: 1 }, "Finance", 1),
            on_page(BlockKind::Paragraph, "Revenue was ahead of budget.", 1),
        ];
        assert_eq!(
            to_markdown(&blocks),
            "The sentence runs on to its end.\n\n<!-- page 1 -->\n\n# Finance\n\n\
             Revenue was ahead of budget.\n"
        );
    }

    /// An HTML block ends a list in CommonMark, so the items after the marker
    /// are a list of their own -- and an item still written at its old parent's
    /// content column would be an indented code block with nothing to nest in.
    #[test]
    fn the_marker_closes_the_list_it_stands_after() {
        let blocks = vec![
            on_page(
                BlockKind::ListItem {
                    level: 0,
                    ordinal: None,
                },
                "first",
                0,
            ),
            on_page(
                BlockKind::ListItem {
                    level: 1,
                    ordinal: None,
                },
                "under it",
                1,
            ),
        ];
        assert_eq!(
            to_markdown(&blocks),
            "- first\n\n<!-- page 1 -->\n\n- under it\n"
        );
    }

    /// The furniture is written nowhere, so it moves nothing: the running heads
    /// stand after the body in the reading order (#571), and a marker taken off
    /// their pages would put one at the end of every document.
    #[test]
    fn the_running_head_does_not_mark_a_page() {
        let blocks = vec![
            on_page(BlockKind::Paragraph, "Body one.", 0),
            on_page(BlockKind::Paragraph, "Body two.", 1),
            on_page(BlockKind::Header, "Annual Report 2026", 0),
            on_page(BlockKind::Header, "Annual Report 2026", 1),
        ];
        assert_eq!(
            to_markdown(&blocks),
            "Body one.\n\n<!-- page 1 -->\n\nBody two.\n"
        );
    }

    /// Asked for, the furniture is written where it stands, as plain
    /// paragraphs: a running head is not a section title, and a page number
    /// not a heading either.
    #[test]
    fn furniture_asked_for_is_plain_paragraphs_where_they_stand() {
        let blocks = vec![
            on_page(BlockKind::Paragraph, "Body one.", 0),
            on_page(BlockKind::Paragraph, "Body two.", 1),
            on_page(BlockKind::Header, "Annual Report 2026", 0),
            on_page(BlockKind::Footer, "Page 1 of 2", 0),
        ];
        let options = MarkdownOptions {
            include_headers: true,
            include_footers: true,
            ..Default::default()
        };
        let markdown = to_markdown_with_options(&blocks, &options);
        assert_eq!(
            markdown,
            "Body one.\n\n<!-- page 1 -->\n\nBody two.\n\n<!-- page 0 -->\n\n\
             Annual Report 2026\n\nPage 1 of 2\n"
        );
        assert!(
            !markdown.lines().any(|line| line.starts_with('#')),
            "no heading syntax for furniture:\n{markdown}"
        );
    }

    /// Headers on and footers off keeps only the running head; the reverse
    /// keeps only the footer. Each switch reads only its own furniture role.
    #[test]
    fn headers_and_footers_are_asked_for_separately() {
        let blocks = vec![
            on_page(BlockKind::Paragraph, "Body one.", 0),
            on_page(BlockKind::Header, "Annual Report 2026", 0),
            on_page(BlockKind::Footer, "Page 1 of 2", 0),
        ];
        let heads = MarkdownOptions {
            include_headers: true,
            ..Default::default()
        };
        let heads_only = to_markdown_with_options(&blocks, &heads);
        assert!(
            heads_only.contains("Annual Report 2026"),
            "the head is written:\n{heads_only}"
        );
        assert!(
            !heads_only.contains("Page 1 of 2"),
            "the footer is not:\n{heads_only}"
        );
        let feet = MarkdownOptions {
            include_footers: true,
            ..Default::default()
        };
        let feet_only = to_markdown_with_options(&blocks, &feet);
        assert!(
            !feet_only.contains("Annual Report 2026"),
            "the head is not:\n{feet_only}"
        );
        assert!(
            feet_only.contains("Page 1 of 2"),
            "the footer is written:\n{feet_only}"
        );
    }

    /// With the separators off there is no marker: the paragraphs on either
    /// side of a page change stand separated by a blank line, like two
    /// paragraphs of one page.
    #[test]
    fn without_separators_pages_stand_a_blank_line_apart() {
        let blocks = vec![
            on_page(BlockKind::Paragraph, "Body one.", 0),
            on_page(BlockKind::Paragraph, "Body two.", 1),
        ];
        let options = MarkdownOptions {
            page_separators: PageSeparators::None,
            ..Default::default()
        };
        assert_eq!(
            to_markdown_with_options(&blocks, &options),
            "Body one.\n\nBody two.\n"
        );
    }

    #[test]
    fn a_table_the_page_broke_is_two_tables_with_the_marker_between_them() {
        let mut blocks = vec![cell(0, 0, true, "Code"), cell(0, 1, true, "Means")];
        blocks.push(on_page(
            BlockKind::TableCell {
                row: 0,
                column: 0,
                row_span: 1,
                column_span: 1,
                header: true,
            },
            "Code",
            1,
        ));
        blocks.push(on_page(
            BlockKind::TableCell {
                row: 0,
                column: 1,
                row_span: 1,
                column_span: 1,
                header: true,
            },
            "Means",
            1,
        ));
        // HTML tables, not pipes: one row is no header block, so the form
        // follows the decoder's own marks rather than forcing a header (#636).
        assert_eq!(
            to_markdown(&blocks),
            "<table>\n<tr><th>Code</th><th>Means</th></tr>\n</table>\n\n<!-- page 1 -->\n\n<table>\n<tr><th>Code</th><th>Means</th></tr>\n</table>\n"
        );
    }

    #[test]
    fn nothing_in_means_nothing_out() {
        assert_eq!(to_markdown(&[]), "");
        assert_eq!(to_markdown(&[block(BlockKind::Paragraph, "   ")]), "");
    }
}
