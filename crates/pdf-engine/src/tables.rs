//! The grid a table is, out of the rows and cells the page already gave up
//! (#570).
//!
//! `crate::text` finds the ROWS: a region the document rules into bands is a
//! table, a band whose cell wrapped is folded back into the row it belongs to,
//! and what arrives here is one line per row with every cell of it in the
//! line's spans (#434, #545). `crate::blocks` finds the run: a line with a gap
//! wide enough to be a gutter, confirmed by a second line showing one at the
//! same x, is a line no paragraph may swallow.
//!
//! What is missing between those two is the TABLE. A row with its cells in a
//! bag is not a grid: nothing says which cell of row four stands under which
//! cell of row one, so nothing can say the second column is a column at all,
//! and a serializer handed those rows can only write the words back out in the
//! order they were drawn -- which is the reading every tool this milestone is
//! measured against beats us on (#566: tables 0.000 against Docling's 0.553).
//!
//! THE COLUMN GRID IS THE CONSENSUS OF THE WHOLE TABLE, and that is the one
//! idea here. A strip of white that runs down through the rows, past the words
//! of every one of them, is a column boundary; a strip the words of one row
//! cross is the same boundary with a cell spanning it. Neither statement can be
//! made about a row on its own, which is why the grid is computed once for the
//! run and every row is then read against it.
//!
//! WHAT THE PAGE IS ASKED, AND WHAT IS REFUSED
//!
//! * A GUTTER IS FREE IN MOST ROWS, not in all of them. Free in all of them is
//!   a rule that cannot see a merged cell at all -- the header `Amount (EUR)`
//!   set across two columns writes ink into the strip between them, and a
//!   boundary that vanishes because one cell crossed it takes both columns with
//!   it. So the strip is a boundary while most rows leave it alone, and a row
//!   whose words cross it is a row with a COLUMN SPAN in it.
//! * A ROW SPAN IS SOMETHING THE DOCUMENT DREW. Two cells merged down the page
//!   are, to the text, a cell with an empty one under it -- and an empty cell is
//!   also just an empty cell, which is the commoner thing by far. The one page
//!   that tells them apart is a ruled one: the rule between two rows runs the
//!   width of the table and BREAKS over the columns whose cells are merged. So
//!   a row span is read off a broken rule and nowhere else, and a table with no
//!   rules has empty cells instead. A guess would be a grid that says something
//!   the document does not.
//! * A NESTED TABLE IS TEXT. Nothing here recurses into a cell: the inner
//!   table's rows are the outer cell's lines, and they come out as the cell's
//!   text. A grid inside a grid is a shape neither markdown nor the block model
//!   has, and inventing one to hold a case this rare would cost every consumer
//!   a second shape to walk.
//! * A WIDGET IS NOT A CELL. A form that looks like a table looks like one
//!   because of its widget annotations, and no widget draws a glyph into a
//!   content stream: the spans this reads are the page's text and the form's
//!   fields are not in them. A form's LABELS can still be a table, which is the
//!   right answer -- they are laid out as one.
//!
//! The cells come out as blocks of their own, one per cell, exactly as a list
//! comes out one block per item (#569): the cell is what a caller chunks,
//! cites and embeds, and a container kind would need a second shape of
//! [`crate::blocks::Block`] with its own text and its own box for a grouping
//! that `row`, `column` and the run of adjacent blocks already say.
//! `crate::markdown` reads such a run back as one table.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::cmp::Ordering;

use crate::text::{HorizontalRule, TextBlock};

/// The smallest gap inside a row, in ems, that can separate two cells.
///
/// Small, because it is only ever asked INSIDE a run the tabular rule of
/// `crate::blocks` has already confirmed at three ems, and because splitting
/// too eagerly is the harmless direction: two fragments of one cell land in the
/// same column of the grid and are put back together, while a split that never
/// happened is a column that cannot be found at all.
const SEGMENT_GAP_EM: f64 = 1.0;

/// How wide a gap between two rows' cells has to be, in ems, before the same
/// gap in the row under it makes the pair a table. Three, because that is what
/// `crate::blocks` asks of two LINES before it calls a gap a gutter (#545), and
/// two answers to one question should not be two numbers.
const CONFIRM_GAP_EM: f64 = 3.0;

/// How wide a strip of white has to be, in ems, before it can be a column
/// boundary. Past anything a justifier stretches a word space to, which is what
/// keeps a column out of the middle of a sentence.
const GUTTER_EM: f64 = 1.5;

/// How many of a table's rows have to leave a strip alone before it is read as
/// a column boundary. Three quarters: a boundary one row in four crosses is
/// still a boundary, and the rows that cross it are the merged cells.
const GUTTER_ROW_FRACTION: f64 = 0.75;

/// How many rows have to leave a strip alone whatever the fraction says. Two,
/// which is the same evidence `crate::blocks` asks for before it calls a gap a
/// gutter at all: one row saying it is one row's word space.
const GUTTER_MIN_ROWS: usize = 2;

/// The fewest rows a table has. Two: one row is a line.
const MIN_ROWS: usize = 2;

/// How many rows a candidate run may lose off its front before the run is given
/// up on. Two, because the thing that puts a row in front of a table is a title
/// or a caption set across it, and because every attempt costs a pass over the
/// run: without a bound, a page of a hundred short two-part lines is a hundred
/// passes over a hundred rows.
const MAX_TRIMMED_ROWS: usize = 2;

/// The fewest columns a table has. Two: one column is a paragraph.
const MIN_COLUMNS: usize = 2;

/// How full the grid has to be. Half, because a table is allowed empty cells
/// and a scatter of words at four left edges is not a table -- it is a title
/// page, a masthead or a figure's labels, and a grid drawn over it says a
/// structure the document has not got.
const MIN_FILL: f64 = 0.5;

/// How much of the table's width a rule has to cross before it is read as the
/// rule BETWEEN two rows rather than a rule under one cell. Half, so that a
/// rule broken over one merged column still counts as the row's rule.
const ROW_RULE_FRACTION: f64 = 0.5;

/// How far apart, in ems, two pieces of ink may lie and still be one rule. A
/// quarter: a broken separator's pieces are drawn on one line to the point,
/// and the tolerance is for the rounding a transform arrives through.
const COLLINEAR_EM: f64 = 0.25;

/// How much of a cell's own width a rule has to cover before it is read as
/// covering it. Three quarters: a rule stops a hair short of a cell edge on
/// almost every page that draws one.
const CELL_RULE_FRACTION: f64 = 0.75;

/// How many of a table's body rows may be set bold before the first row's own
/// weight stops saying "header". Fewer than half: a table whose last row is a
/// bold `Total` still has a header, and one set bold throughout has not.
const HEADER_BOLD_FRACTION: f64 = 0.5;

/// How much more white a band with one run of ink in it may have round it than
/// the table has between its rows, and still be a band OF the table.
///
/// Three quarters more. A section head inside a table sits on the table's own
/// pitch -- it is a row, and the document set it like one. A caption between
/// two tables is set apart, which is what a caption is for, and the white
/// above and below it is the evidence. Wider than this and one table swallows
/// the next; tighter and a section head ends the table it is inside.
const SECTION_WHITE: f64 = 1.75;

/// How far past a table's ink, in points, a rule may lie and still be
/// read as crossing the table. Two: the row boxes hold glyph ink, and a
/// rule drawn on the row's edge sits a point or two outside it.
const CROSSING_TOLERANCE: f64 = 2.0;

/// How far short of a table's side a crossing rule may stop, in points.
/// One: a rule stops a hair short of the ink on almost every page that
/// draws one, and a rule under one cell stops much shorter.
const CROSSING_SHORTFALL: f64 = 1.0;

/// One row of a candidate table: which of the run's lines it is drawn on, and
/// what the grid is decided on.
///
/// A ROW IS NOT ALWAYS ONE LINE. `crate::text` hands a row whose cell wrapped
/// back cell by cell -- the row's own runs and the continuation lines between
/// them, one block each (#434) -- so a row is a consecutive slice of lines, and
/// `crate::blocks` cuts the slices by the run's own row pitch before it asks
/// anything here.
pub(crate) struct Row {
    /// First line of the run this row is drawn on.
    pub(crate) first: usize,
    /// One past the last.
    pub(crate) end: usize,
    /// Leftmost edge of the row's text.
    pub(crate) left: f64,
    /// Rightmost edge of the row's text.
    pub(crate) right: f64,
    /// Top of the row's glyphs.
    pub(crate) top: f64,
    /// Bottom of the row's glyphs.
    pub(crate) bottom: f64,
    /// The type size most of the row is set in.
    pub(crate) font_size: f64,
    /// Whether most of the row's characters are set in a bold face.
    pub(crate) bold: bool,
}

/// One cell of a grid: where it sits, how far it reaches, and what drew it.
#[derive(Debug, Clone)]
pub(crate) struct Cell {
    /// 0-based row inside the table.
    pub(crate) row: u16,
    /// 0-based column inside the table.
    pub(crate) column: u16,
    /// How many rows the cell covers. 1 unless a broken rule said otherwise.
    pub(crate) row_span: u16,
    /// How many columns the cell covers. 1 unless its words crossed a boundary.
    pub(crate) column_span: u16,
    /// Whether the cell belongs to the table's header row.
    pub(crate) header: bool,
    /// The spans the cell is drawn with, as `(line, span)` pairs into the
    /// lines the run was built from.
    pub(crate) spans: Vec<(usize, usize)>,
}

/// A table: which rows of the run it is, and its cells in row-major order.
#[derive(Debug, Clone)]
pub(crate) struct Table {
    /// First row of the run this table starts at.
    pub(crate) first: usize,
    /// One past the last.
    pub(crate) end: usize,
    /// The cells, row-major: the order the table is read in.
    pub(crate) cells: Vec<Cell>,
    /// What the builder had to go on when it found this grid (#574). `grid` fills
    /// what it knows and `find` adds the two facts only it can see.
    pub(crate) doubt: crate::blocks::GridDoubt,
    /// Which stage produced the grid, for the env-gated table trace (#636).
    /// `grid` leaves `Other`; `find` and `find_whole` stamp their own tables
    /// (a joined table keeps its halves' stamp), and the block assembly
    /// carries it onto the cells. Never decides anything.
    pub(crate) origin: crate::table_trace::TableOrigin,
}

/// A run of one row's ink: the x it covers, and the spans that cover it.
#[derive(Debug, Clone)]
struct Segment {
    left: f64,
    right: f64,
    spans: Vec<(usize, usize)>,
}

/// Every table in one column of a page, in the order the column is read in.
///
/// `lines` are that column's lines and `rows` the slices of them the run reads
/// as rows, top to bottom; `rules` are the horizontal lines the page drew, in
/// the same frame. A row that is part of no table is left for the paragraph
/// merge, which is why this answers with ranges rather than by consuming the
/// run.
pub(crate) fn find(lines: &[&TextBlock], rows: &[Row], rules: &[HorizontalRule]) -> Vec<Table> {
    // Once for the whole run, because a candidate that turns out not to be a
    // grid is retried from its second row -- a title set over a table breaks
    // into two parts like a row does, and a run that gave up on the first
    // failure would lose the table under it.
    let cells: Vec<Vec<Segment>> = rows.iter().map(|row| segments(lines, row)).collect();

    // The white the page leaves between the rows of a table, measured once over
    // the whole run: what a band with one run of ink in it is held against.
    let white = row_white(rows, &cells);

    // Across the column, the way `find_whole` measures its region: what a
    // line of running text is held against (#638).
    let edge_left = cells
        .iter()
        .flatten()
        .map(|s| s.left)
        .fold(f64::INFINITY, f64::min);
    let edge_right = cells
        .iter()
        .flatten()
        .map(|s| s.right)
        .fold(f64::NEG_INFINITY, f64::max);
    let wide = |i: usize| {
        cells[i].iter().any(|s| {
            s.right - s.left >= PROSE_WIDTH * (edge_right - edge_left)
                && s.left - edge_left <= PROSE_INDENT * (edge_right - edge_left)
        })
    };
    // A LINE OF PROSE IS NOT A ROW (#638): one run of ink with a sentence's
    // worth of words in it is the text the table stands in. Table footnotes
    // fail the lettered share their codes and symbols drag down, and section
    // heads fail the word count, so neither is refused here.
    let reads_prose = |i: usize| cells[i].len() < MIN_COLUMNS && wide(i) && prose(lines, &rows[i]);

    let mut tables = Vec::new();
    let mut start = 0usize;
    while start < rows.len() {
        if cells[start].len() < MIN_COLUMNS {
            start += 1;
            continue;
        }
        let mut end = start + 1;
        let mut thin_rows = 0usize;
        while end < rows.len() {
            // A CAPTION SET APART ANNOUNCES THE NEXT TABLE (#600): the run
            // ends before it, the way `find_whole` cuts its stretches and
            // the way the continuation rule vetoes across it. Past the first
            // row only, and only where the white sets it apart: a caption
            // set on the table's own pitch -- `Table 2 Net sales` under a
            // section label -- is the learned pass's to place, and breaking
            // there cut one statement into two tables.
            if end > start
                && announces_another_table(lines, &rows[end])
                && !band_of_the_table(rows, end, white)
            {
                break;
            }
            if cells[end].len() >= MIN_COLUMNS {
                end += 1;
                continue;
            }
            // A BAND WITH ONE RUN OF INK IN IT IS A ROW OF THE TABLE when the
            // table goes on under it and the page put no more white round it
            // than it puts between its rows. Section heads set across a table
            // -- `Geographic locations`, `Study design` -- are the commonest
            // furniture inside one, and a run that ended at each of them wrote
            // one table out as four, none of which is the table the document
            // holds. The white is what tells that band from the caption of the
            // NEXT table, which is set apart on purpose (#576).
            if cells[end].len() == 1
                && end + 1 < rows.len()
                && cells[end + 1].len() >= MIN_COLUMNS
                && band_of_the_table(rows, end, white)
                && !reads_prose(end)
            {
                // Kept on the strength of its neighbours rather than its own
                // shape, which is one of the facts #574 records.
                thin_rows += 1;
                end += 2;
                continue;
            }
            break;
        }
        // The run is tried from its own first row and then from the next two,
        // because the thing that stands in front of a table is a title or a
        // caption set across it -- and no further, because every attempt is a
        // pass over the whole run.
        let mut found = None;
        let mut retried_from = 0usize;
        for from in start..=(start + MAX_TRIMMED_ROWS).min(end.saturating_sub(MIN_ROWS)) {
            found = grid(&rows[from..end], &cells[from..end], from, rules);
            if found.is_some() {
                retried_from = from - start;
                break;
            }
        }
        match found {
            // A LIST SET AS A TABLE IS A LIST (#1973), and the list pass reads
            // it once these rows are left to it.
            Some(table) if a_list_set_as_a_table(&table, lines) => start = end,
            // A FIGURE'S LABELS ARE NOT A TABLE (#636): a fragmentary grid in
            // a figure-caption neighbourhood is left to the paragraph merge
            // with the run's other rows.
            Some(table) if figure_fragment_veto(&table, lines, rows) => {
                crate::table_trace::emit_candidate(
                    crate::table_trace::TableOrigin::Find,
                    &table,
                    lines,
                    rows,
                    rules,
                );
                start = end;
            }
            Some(mut table) => {
                // The two facts only this loop sees: how many bands were admitted
                // thin, and how many of the run's first rows had to be trimmed
                // before anything was a grid at all (#574).
                table.doubt.thin_rows = thin_rows;
                table.doubt.retried_from = retried_from;
                table.origin = crate::table_trace::TableOrigin::Find;
                tables.push(table);
                start = end;
            }
            None => start += 1,
        }
    }
    one_table_that_carries_on_is_one_table(tables, rows, &cells, lines, rules)
}

/// The table of a region found before the page was cut (#574): the whole region
/// as ONE grid where it makes one, and [`find`]'s own reading where it does not.
///
/// `find` does not know the region is a table, so it reads it the way it reads
/// any column: a label row, a head set over the figures or a section head is a
/// place where one table may end and another begin, and a title in front may be
/// trimmed off. The region already says where the table starts and stops, so it
/// is asked first as one run from its first row to its last; only a region that
/// makes no grid that way falls back to `find`.
pub(crate) fn find_whole(
    lines: &[&TextBlock],
    rows: &[Row],
    rules: &[HorizontalRule],
) -> Vec<Table> {
    // A CAPTION INSIDE THE REGION OPENS ANOTHER TABLE (#574). A region can
    // hold two tables stacked with the second one's `Table 2` between them, and
    // read as one grid it writes both, and the caption, as one table: on
    // PubTables' round of 29-09 that was the largest share of the gap to
    // Docling. So each stretch between captions is its own region, and the
    // caption is left to the paragraphs.
    //
    // NOR IS WHAT HANGS UNDER THE LAST ROW: a band with one run of ink after
    // the grid's last full row is a note (`*P < 0.05`, `Abbreviations: ...`),
    // and a section head there would head nothing.
    let cells: Vec<Vec<Segment>> = rows.iter().map(|row| segments(lines, row)).collect();
    let thin = |i: usize| cells[i].len() < MIN_COLUMNS;
    // Across the region, not in one of its columns: a long cell of a
    // scientific table's text column is a row, and it is narrower than this.
    let left = cells
        .iter()
        .flatten()
        .map(|s| s.left)
        .fold(f64::INFINITY, f64::min);
    let right = cells
        .iter()
        .flatten()
        .map(|s| s.right)
        .fold(f64::NEG_INFINITY, f64::max);
    // and from its left edge: the long text of a table's second column wraps
    // into bands of one run of ink that start where that column does.
    let wide = |i: usize| {
        cells[i].iter().any(|s| {
            s.right - s.left >= PROSE_WIDTH * (right - left)
                && s.left - left <= PROSE_INDENT * (right - left)
        })
    };
    // A LINE OF PROSE IS NOT A ROW EITHER (#638): one run of ink with a
    // sentence's worth of words in it is the text the table stands in, and a
    // region that reached over it wrote a paragraph line as a row of one cell
    // -- the commonest thing our tables swallow on DocLayNet. A section head
    // inside a table is a few words; this is where the region ends.
    let cuts: Vec<usize> = (0..rows.len())
        .filter(|&i| {
            announces_another_table(lines, &rows[i])
                || (thin(i) && wide(i) && prose(lines, &rows[i]))
        })
        .collect();
    let mut stretches = Vec::new();
    let mut from = 0usize;
    for to in cuts.iter().copied().chain(std::iter::once(rows.len())) {
        let first = from;
        let mut end = to;
        while end > first && thin(end - 1) {
            end -= 1;
        }
        stretches.push((first, end));
        from = to + 1;
    }
    let mut tables = Vec::new();
    for (first, end) in stretches {
        if end < first + MIN_ROWS {
            continue;
        }
        match grid(&rows[first..end], &cells[first..end], first, rules) {
            Some(table) if a_list_set_as_a_table(&table, lines) => {}
            Some(table) if figure_fragment_veto(&table, lines, rows) => {
                crate::table_trace::emit_candidate(
                    crate::table_trace::TableOrigin::FindWhole,
                    &table,
                    lines,
                    rows,
                    rules,
                );
            }
            Some(mut table) => {
                table.doubt.thin_rows = (first..end).filter(|&i| thin(i)).count();
                table.origin = crate::table_trace::TableOrigin::FindWhole;
                tables.push(table);
            }
            // A stretch that makes no grid is text the region reached over,
            // and it is left to the paragraphs -- unless nothing in the
            // region is a grid, which is `find`'s to read.
            _ => {}
        }
    }
    if !tables.is_empty() {
        return tables;
    }
    find(lines, rows, rules)
}

/// Whether a table the grid found is a list the page set in two columns: a
/// column of markers and a column of what they mark (#1973).
///
/// A statute's contents page, `§ 70 | Statthaftigkeit der Rechtsbeschwerde`, a
/// tender's `• | Form 1: Tenderer Contact Form`: two runs of ink per line at
/// two steady x's, which is a grid by every measure the builder has, and which
/// the annotation -- and a reader -- calls a list. On DocLayNet's validation
/// half, refusing these gave +9.3 pages' worth of list F1 (statutes +8.4,
/// financial reports +0.9) and moved the table axis on no page of any genre.
///
/// NARROW, because the loose version cost eight pages of table F1 there:
///
/// - TWO filled columns, and no more. A third is the table's data (`1 NO`,
///   `Y`), and a bank of bullets down a table of switch states is a table;
/// - the first column is list markers, the whole of each cell, on at least
///   [`LIST_MARKER_SHARE`] of the rows -- and a bare number is not one: `0`,
///   `87` and `1 term` down a first column are row labels, ids and line
///   numbers of a real table;
/// - the second column is words: no cell of it is only figures, and its cells
///   run to [`LIST_TEXT_MIN_CHARS`] characters on average.
fn a_list_set_as_a_table(table: &Table, lines: &[&TextBlock]) -> bool {
    let text_of = |cell: &Cell| -> String {
        cell.spans
            .iter()
            .filter_map(|&(line, span)| lines.get(line)?.spans.get(span))
            .map(|s| s.text.trim())
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mut columns: Vec<u16> = table
        .cells
        .iter()
        .filter(|c| !text_of(c).is_empty())
        .map(|c| c.column)
        .collect();
    columns.sort_unstable();
    columns.dedup();
    let [first, second] = columns[..] else {
        return false;
    };
    let rows = table.cells.iter().map(|c| c.row).max().map_or(0, |r| r + 1) as usize;
    let markers = table
        .cells
        .iter()
        .filter(|c| c.column == first)
        .map(text_of)
        .filter(|t| !t.is_empty())
        .filter(|t| {
            // The whole cell is the marker: asked with a word after it, the
            // marker must end where the cell does.
            let probe = format!("{t} X");
            crate::lists::marker_at(&probe, crate::lists::Lead::default())
                .is_some_and(|m| m.len >= t.len())
        })
        .count();
    if rows < 2 || markers < 2 || (markers as f64) < LIST_MARKER_SHARE * rows as f64 {
        return false;
    }
    let text: Vec<String> = table
        .cells
        .iter()
        .filter(|c| c.column == second)
        .map(text_of)
        .filter(|t| !t.is_empty())
        .collect();
    if text.is_empty()
        || text.iter().any(|t| {
            t.chars()
                .all(|ch| ch.is_ascii_digit() || " .,%$()-".contains(ch))
        })
    {
        return false;
    }
    let mean = text.iter().map(|t| t.chars().count()).sum::<usize>() as f64 / text.len() as f64;
    mean >= LIST_TEXT_MIN_CHARS
}

/// How many of a two-column table's rows must open with a list marker before
/// it is read as a list: three in five.
const LIST_MARKER_SHARE: f64 = 0.6;

/// How long, on average, the second column's cells of a list set as a table
/// run: a line's worth of words, not a figure or a code.
const LIST_TEXT_MIN_CHARS: f64 = 12.0;

/// Whether a candidate grid is a figure's own labels read as a table (#636).
///
/// Chemistry-figure OCR fragments and chart axes, legends and ticks come out
/// of the repaired word boxes column-aligned, so the grid builder proposes a
/// table where the page holds a figure. The veto fires only when BOTH hold:
///
/// - FIGURE CONTEXT: a row of the grid itself, or a line within
///   [`FIGURE_CAPTION_NEAR_EM`] ems above or below it and overlapping it
///   sideways, carries a figure-caption marker (`FIG. n`, `Figure n`,
///   `[Chemical Formula n]`);
/// - FRAGMENTARY CELLS: the median cell holds at most
///   [`FRAGMENT_CELL_MEDIAN`] alphanumeric characters, and more than a third
///   of the grid's positions are empty.
///
/// A genuine data table under a `Figure 2` caption keeps its table: its cells
/// hold words. A fragmentary scatter with no caption nearby keeps its table
/// too: the veto needs both halves, so display-equation arrays (no caption)
/// and two-column prose sidebars (word cells) are untouched by it.
///
/// A patent drawing sheet needs no caption to tell it apart: its panels each
/// carry a "-continued" label in the grid's first row, which no data table
/// does twice ([`continued_panels`]).
fn figure_fragment_veto(table: &Table, lines: &[&TextBlock], rows: &[Row]) -> bool {
    (fragmentary_cells(table, lines) && near_figure_caption(table, lines, rows))
        || continued_panels(table, lines)
        || continued_panel_fragments(table, lines, rows)
}

/// The most alphanumeric characters the median cell of a fragmentary grid may
/// hold (#636). Figure labels, axis ticks and OCR shards read a character or
/// two per cell; a data cell holds a word, or a figure with its unit.
pub(crate) const FRAGMENT_CELL_MEDIAN: usize = 3;

/// How far above or below a candidate grid a figure-caption marker may sit and
/// still put the grid in the caption's neighbourhood (#636). Twelve ems: a
/// caption is set a line or two from its figure, and the figure's own labels
/// can stand well above it; past that the marker belongs to another figure.
pub(crate) const FIGURE_CAPTION_NEAR_EM: f64 = 12.0;

/// Whether a candidate grid is a patent drawing sheet rather than a data
/// table (#636): two or more panel labels in its first row.
///
/// Each panel of a continued structure figure carries its own "-continued"
/// marker, so the grid holds the word twice or more where a data table holds
/// it never. The labels must all stand in the first row: a "continued" note
/// in a body's status column is data, not a panel head. Counted over
/// occurrences, because the rebuild can merge both panels into one cell.
fn continued_panels(table: &Table, lines: &[&TextBlock]) -> bool {
    let mut hits = 0usize;
    for cell in &table.cells {
        let text = cell
            .spans
            .iter()
            .filter_map(|&(line, span)| {
                lines
                    .get(line)
                    .and_then(|block| block.spans.get(span))
                    .map(|span| span.text.trim())
            })
            .collect::<Vec<_>>()
            .join(" ");
        let hits_here = text.to_ascii_lowercase().matches("continu").count();
        if hits_here > 0 && cell.row != 0 {
            return false;
        }
        hits += hits_here;
    }
    hits >= 2
}

/// Whether a candidate grid is the fragments hugging continued panels: short
/// cells with a panel label just outside the grid (#636).
///
/// The panels' own header can fall outside the candidate -- `find` trims it,
/// or the run breaks there -- and the shards alone still grid (they are the
/// same shape as caption-less fragments, which rightly stay tables). What
/// tells them apart is the label beside them: a run row within two rows of
/// the grid whose text LEADS with "continued". Prose buries the word
/// mid-sentence and table captions lead with the table ("Table 3
/// (continued)"), so neither reads as a panel. Caption-split runs never even
/// reach here with their caption beside them.
fn continued_panel_fragments(table: &Table, lines: &[&TextBlock], rows: &[Row]) -> bool {
    if table.cells.is_empty() {
        return false;
    }
    let mut lens: Vec<usize> = Vec::with_capacity(table.cells.len());
    for cell in &table.cells {
        let text = cell
            .spans
            .iter()
            .filter_map(|&(line, span)| {
                lines
                    .get(line)
                    .and_then(|block| block.spans.get(span))
                    .map(|span| span.text.trim())
            })
            .collect::<Vec<_>>()
            .join(" ");
        lens.push(text.chars().filter(|c| c.is_alphanumeric()).count());
    }
    lens.sort_unstable();
    let mid = lens.len() / 2;
    let short = if lens.len() % 2 == 1 {
        lens[mid] <= FRAGMENT_CELL_MEDIAN
    } else {
        lens[mid - 1] + lens[mid] <= 2 * FRAGMENT_CELL_MEDIAN
    };
    if !short {
        return false;
    }
    let lo = table.first.saturating_sub(2);
    let hi = (table.end + 2).min(rows.len());
    let nearby = |row: &Row| -> bool {
        let end = row.end.max(row.first);
        let text = lines
            .get(row.first..end)
            .map(|blocks| {
                blocks
                    .iter()
                    .flat_map(|block| block.spans.iter())
                    .map(|span| span.text.trim())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
        text.trim_start_matches(|c: char| c == '-' || c == '—' || c.is_whitespace())
            .to_ascii_lowercase()
            .starts_with("continu")
    };
    rows.get(lo..table.first.min(rows.len()))
        .is_some_and(|above| above.iter().any(&nearby))
        || rows
            .get(table.end.min(rows.len())..hi)
            .is_some_and(|below| below.iter().any(&nearby))
}

/// Whether every cell of a candidate grid reads as shards: short cells, and
/// mostly holes where the grid says cells should be (#636).
///
/// Emptiness is counted over grid POSITIONS, not over cells: a hole leaves no
/// cell behind, so the filled positions are the cells' column spans (row spans
/// counted) against the run's rows times the grid's columns.
fn fragmentary_cells(table: &Table, lines: &[&TextBlock]) -> bool {
    if table.cells.is_empty() || table.end <= table.first {
        return false;
    }
    let columns = table
        .cells
        .iter()
        .map(|cell| cell.column as usize + cell.column_span as usize)
        .max()
        .unwrap_or(0);
    if columns == 0 {
        return false;
    }
    let positions = (table.end - table.first) * columns;
    let filled: usize = table
        .cells
        .iter()
        .map(|cell| cell.column_span as usize * cell.row_span.max(1) as usize)
        .sum();
    if positions == 0 || filled >= positions {
        return false;
    }
    if (positions - filled) * 3 <= positions {
        return false;
    }
    let mut lens: Vec<usize> = Vec::with_capacity(table.cells.len());
    for cell in &table.cells {
        let text = cell
            .spans
            .iter()
            .filter_map(|&(line, span)| {
                lines
                    .get(line)
                    .and_then(|block| block.spans.get(span))
                    .map(|span| span.text.trim())
            })
            .collect::<Vec<_>>()
            .join(" ");
        lens.push(text.chars().filter(|c| c.is_alphanumeric()).count());
    }
    lens.sort_unstable();
    let mid = lens.len() / 2;
    if lens.len() % 2 == 1 {
        lens[mid] <= FRAGMENT_CELL_MEDIAN
    } else {
        lens[mid - 1] + lens[mid] <= 2 * FRAGMENT_CELL_MEDIAN
    }
}

/// Whether a candidate grid stands in a figure-caption neighbourhood (#636):
/// one of its own rows, or a line close above or below it and overlapping it
/// sideways, carries a figure-caption marker.
fn near_figure_caption(table: &Table, lines: &[&TextBlock], rows: &[Row]) -> bool {
    let Some(span) = rows.get(table.first..table.end) else {
        return false;
    };
    let mut grid_left = f64::INFINITY;
    let mut grid_right = f64::NEG_INFINITY;
    let mut grid_lo = f64::INFINITY;
    let mut grid_hi = f64::NEG_INFINITY;
    let mut em = f64::MIN_POSITIVE;
    for row in span {
        grid_left = grid_left.min(row.left);
        grid_right = grid_right.max(row.right);
        grid_lo = grid_lo.min(row.top.min(row.bottom));
        grid_hi = grid_hi.max(row.top.max(row.bottom));
        em = em.max(row.font_size);
    }
    if !(grid_left.is_finite() && grid_right > grid_left && grid_hi >= grid_lo) {
        return false;
    }
    let near = FIGURE_CAPTION_NEAR_EM * em;
    lines.iter().any(|line| {
        if !is_figure_caption(&line.text()) {
            return false;
        }
        let mut left = f64::INFINITY;
        let mut right = f64::NEG_INFINITY;
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for span in &line.spans {
            left = left.min(span.x);
            right = right.max(crate::blocks::span_right(span));
            lo = lo.min(span.y);
            hi = hi.max(span.y + span.height);
        }
        if !(left.is_finite() && right > left && hi >= lo) {
            return false;
        }
        if right <= grid_left || left >= grid_right {
            return false;
        }
        let gap = if hi < grid_lo {
            grid_lo - hi
        } else if lo > grid_hi {
            lo - grid_hi
        } else {
            0.0
        };
        gap <= near
    })
}

/// Whether a line carries a figure-caption marker: `FIG. 3`, `Figure 20`,
/// `[Chemical Formula 4]` (#636).
///
/// The marker needs its identifier: the word `figure` inside running prose is
/// not a caption, and neither is a bare `Fig` with no number after it.
pub(crate) fn is_figure_caption(text: &str) -> bool {
    if text.to_ascii_lowercase().contains("chemical formula") {
        return true;
    }
    let words: Vec<&str> = text.split_whitespace().collect();
    words.iter().enumerate().any(|(at, word)| {
        let bare = word
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_ascii_lowercase();
        if !matches!(bare.as_str(), "fig" | "figure") {
            return false;
        }
        words.get(at + 1).is_some_and(|next| {
            next.trim_matches(|c: char| !c.is_alphanumeric())
                .starts_with(|c: char| c.is_alphanumeric())
        })
    })
}

/// Two tables of one run that carry on from each other are one table (#600).
///
/// A run ends where `grid` stops accepting rows, and on 28 of the 128 regions
/// measured over 120 pages that happens INSIDE one table: the region is not split,
/// the run is, so this cannot be answered by joining regions -- which is where I
/// first put it, and it never fired. The split is here, so the question is here.
///
/// Asked of two tables the run already produced: is the second a continuation of
/// the first? If it is, the rows from the first's start to the second's end are
/// offered to `grid` again as one table. Nothing merges two finished grids; `grid`
/// does the work it already does, and if it refuses the combined range the two
/// tables stand as they were.
///
/// WHAT DECIDES IS WHETHER THE PAGE ANNOUNCED A SECOND TABLE, and it took a red
/// test to find that out. The first version of this rule decided on the white
/// between the two, in ems, and it joined the two tables of
/// `a_caption_between_two_tables_does_not_join_them` (#576). That is not a
/// threshold that needed moving: a heading INSIDE one table and a caption
/// ANNOUNCING the next are the same shape on the geometry. The two fixtures differ
/// in one thing only -- "Southern hemisphere" against "Table 2 The same by
/// subtype" -- so no measurement of white can separate them, and a white veto
/// turned #576 green by turning this rule's own fixture red.
///
/// So the evidence is the text of the bands between them. Measured on the same 96
/// labelled pairs as before -- 63 inside one annotated table, 33 in different ones,
/// both classes our own candidates from 300 pages of PubTables' training split, no
/// page of the benchmark's own set among them:
///
/// ```text
/// join every pair                       66 per cent, 33 wrong joins of 33
/// white between them, under 11 ems      92 per cent,  3 wrong joins, 58 of 63 joined
/// no band between announces a table     97 per cent,  3 wrong joins, 63 of 63 joined
/// ```
///
/// The text rule dominates the white one outright rather than trading against it:
/// the same three wrong joins, and it misses no continuation at all. `Table 2` never
/// once stood between two candidates of ONE annotated table (0 of 63) and stood
/// between different ones 30 times of 33, so this is not a threshold with a happier
/// side -- it is the page saying what it did.
///
/// THE VOCABULARY IS THE ONE `pdf-learned` ALREADY CARRIES (`caption`), with the
/// word folded for case because `TABLE 3 | R2 Changes ...` is how one of the 33
/// writes it, which the exact form missed: folding costs nothing on the other side
/// (still 0 of 63) and catches two more. The NUMBER IS REQUIRED. Without it the
/// rule vetoes two real continuations whose head happens to open with the word
/// "Table", and a numbered section head like `7.2 Text` is not a caption -- which
/// is T3's warning, and `section_number` in `pdf-learned` says the same. A wider
/// vocabulary (chart, exhibit, appendix, scheme, panel) was measured and buys
/// nothing on these pairs, so it is not here: an unmeasured word list is a guess
/// with a comment on it.
///
/// WHAT IS LEFT WRONG, and it is left wrong on purpose. Three boundaries have no
/// caption between them to read: one says `Refinement`, one is a crystallography
/// table with eight blocks between, and one pair overlaps in y -- two candidates
/// side by side, which is a different defect from a continuation and is not
/// answered here. Requiring the second to lie below the first would catch that one
/// and lose four real joins, so it is not required; a missed continuation leaves
/// two tables where there was one, and that is the cheaper error.
fn one_table_that_carries_on_is_one_table(
    tables: Vec<Table>,
    rows: &[Row],
    cells: &[Vec<Segment>],
    lines: &[&TextBlock],
    rules: &[HorizontalRule],
) -> Vec<Table> {
    if tables.len() < 2 {
        return tables;
    }
    let mut out: Vec<Table> = Vec::new();
    for table in tables {
        let Some(previous) = out.last() else {
            out.push(table);
            continue;
        };
        let announced =
            (previous.end..table.first).any(|band| announces_another_table(lines, &rows[band]));
        if !announced {
            let first = previous.first;
            if let Some(mut joined) = grid(
                &rows[first..table.end],
                &cells[first..table.end],
                first,
                rules,
            ) {
                // THE EVIDENCE OF BOTH HALVES SURVIVES THE JOIN (codex on #1989).
                // `grid` fills what it can see and `find` adds the two facts only
                // the run loop knows, so a table rebuilt here would report
                // `thin_rows` and `retried_from` of zero -- and a joined table is
                // exactly the case where a section row admitted by #576's rule is
                // likely to be inside it. Reporting zero would tell the learned
                // model's gate, which asks for at least one thin row, that this
                // table had no doubt at all.
                // RECOUNTED OVER THE JOINED RANGE, not summed from the halves.
                // Summing was my first answer and the test showed it wrong: the
                // band that ended the run lies BETWEEN the two tables, so neither
                // half had counted it and the sum was still zero. Every row of the
                // combined range with too few runs of ink to be a row on its own
                // is in the grid now, whichever side of the break it came from.
                let previous_doubt = out.last().map(|t| t.doubt).unwrap_or_default();
                joined.doubt.thin_rows = (first..table.end)
                    .filter(|row| cells[*row].len() < MIN_COLUMNS)
                    .count();
                joined.doubt.retried_from =
                    previous_doubt.retried_from.max(table.doubt.retried_from);
                // The re-grid is this run's own work: it keeps its halves'
                // stamp (both halves are this function's tables).
                joined.origin = table.origin;
                out.pop();
                out.push(joined);
                continue;
            }
        }
        out.push(table);
    }
    out
}

/// Whether a band reads as a line of running text: at least [`PROSE_WORDS`]
/// words, nearly all of them words rather than figures (#638).
fn prose(lines: &[&TextBlock], row: &Row) -> bool {
    // The lines as they read, not their spans spaced out: the spans are
    // glyph runs, and joining them with a space counts characters as words.
    let text: String = lines[row.first..row.end]
        .iter()
        .map(|line| line.text())
        .collect::<Vec<_>>()
        .join(" ");
    let words: Vec<&str> = text.split_whitespace().collect();
    let lettered = words
        .iter()
        .filter(|w| w.chars().filter(|c| c.is_alphabetic()).count() >= 2)
        .count();
    words.len() >= PROSE_WORDS && lettered * 10 >= words.len() * 8
}

/// The fewest words a band of running text has; a section head set across a
/// table has fewer (#638).
const PROSE_WORDS: usize = 8;

/// The share of the region's width a line of running text covers (#638).
const PROSE_WIDTH: f64 = 0.6;

/// How far in from the region's left edge a line of running text may start,
/// as a share of the region's width (#638).
const PROSE_INDENT: f64 = 0.05;

/// Whether a band announces a table of its own: `Table 2`, `TABLE I`, `Table S2`.
///
/// The word and then an identifier, which is `pdf-learned`'s `caption` with the
/// word folded for case (#600). The identifier is what keeps a numbered section
/// head -- `7.2 Text` -- and a row whose first cell reads `Table` out of it.
///
/// THREE FORMS OF IDENTIFIER, and each is here because it is attested in a set
/// this engine is graded on rather than because it sounded likely (codex, #1969):
/// `2` in both sets, `I` in DocLayNet's round of 24-09, `S2` in PubTables'.
/// Journals number their tables in all three ways and a caption the veto misses
/// is a wrong join -- two tables the reader can see merged into one -- which is
/// the costlier of the two errors this rule can make. On the 96 labelled pairs
/// the wider identifier changes nothing either way (still 0 wrong vetoes of 63,
/// still 30 boundaries of 33), because no pair there is numbered that way; so it
/// is taken on the attestation and the asymmetry, not on a measured gain, and the
/// table count over 300 pages is unmoved by it.
///
/// `roman_value` is deliberately lenient -- it reads `iiii` as four -- so on its
/// own it accepts `MIX` and `CIVIL` as numerals. The length bound the list code
/// already uses for the same reason is what keeps an English word out, and a
/// second word this long after `Table` is a numeral or nothing.
fn announces_another_table(lines: &[&TextBlock], row: &Row) -> bool {
    // The lines as they read: `Table 2` set in glyph runs is `T a b l e 2`
    // with a space between every span, which no caption opens with.
    let text: String = lines[row.first..row.end]
        .iter()
        .map(|line| line.text())
        .collect::<Vec<_>>()
        .join(" ");
    announces_a_table(&text)
}

/// Whether the page drew a rule across a found table (#659).
///
/// The [`TableStrategy::Lines`](crate::TableStrategy::Lines) half of the
/// detection choice: a table with a drawn rule crossing its rows stands,
/// and one without goes back to prose. Crossing means the full width --
/// a rule under one cell is not the table's -- with a point of
/// shortfall and two points past the ink forgiven.
pub(crate) fn table_has_ruling(rows: &[Row], table: &Table, rules: &[HorizontalRule]) -> bool {
    if table.first >= table.end {
        return false;
    }
    let Some(span) = rows.get(table.first..table.end) else {
        return false;
    };
    let mut left = f64::INFINITY;
    let mut right = f64::NEG_INFINITY;
    let mut bottom = f64::INFINITY;
    let mut top = f64::NEG_INFINITY;
    for row in span {
        left = left.min(row.left);
        right = right.max(row.right);
        bottom = bottom.min(row.bottom);
        top = top.max(row.top);
    }
    rules.iter().any(|rule| {
        rule.left <= left + CROSSING_SHORTFALL
            && rule.right >= right - CROSSING_SHORTFALL
            && rule.y >= bottom - CROSSING_TOLERANCE
            && rule.y <= top + CROSSING_TOLERANCE
    })
}

/// Whether a line opens with a table's or a figure's caption: `Table 2`,
/// `Figure S1`, `Table IV:` (#600, and #574 for the region that must stop there).
pub(crate) fn announces_a_table(text: &str) -> bool {
    let mut words = text.split_whitespace();
    let Some(first) = words.next() else {
        return false;
    };
    let word = first.trim_end_matches(':').to_ascii_lowercase();
    if !matches!(word.as_str(), "table" | "figure" | "fig" | "fig.") {
        return false;
    }
    let Some(second) = words.next() else {
        return false;
    };
    let identifier = second.trim_end_matches(['.', ':', ',']);
    identifier_of_a_table(identifier)
}

/// `2`, `I`, `S2`: the three ways a caption names its table (#600).
pub(crate) fn identifier_of_a_table(identifier: &str) -> bool {
    if identifier.is_empty() {
        return false;
    }
    if identifier.starts_with(|c: char| c.is_ascii_digit()) {
        return true;
    }
    // `S2`, `s3`: a supplementary table, numbered in its own series.
    if let Some(rest) = identifier.strip_prefix(['S', 's']) {
        if !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()) {
            return true;
        }
    }
    identifier.chars().all(|c| c.is_ascii_uppercase())
        && crate::lists::roman_value(identifier)
            .is_some_and(|value| (1..=CAPTION_ROMAN_MAX).contains(&value))
}

/// The highest Roman numeral that names a table (#600).
///
/// `roman_value` answers arithmetic and not whether the letters were meant as a
/// numeral: it reads `iiii` as four, and so it reads `CIVIL` as 143 and `MIX` as
/// 1009. A LENGTH bound does not separate those -- `CIVIL` is five letters, which
/// is fewer than `XXXVII` -- and I wrote one before the test for it went red.
/// A VALUE bound does, with room to spare: over the English words that are made
/// only of Roman letters, the lowest value this function gives any of them is 94
/// (`VILL`), while `MIX` is 1009, `DIM` 499 and `CIVIC` 193. Fifty is L, past any
/// table count a document has, and it leaves a factor of nearly two before the
/// nearest word.
const CAPTION_ROMAN_MAX: u32 = 50;

/// The white a table leaves between its rows, over the whole run.
///
/// The median of the gaps between two bands that BOTH hold a grid's worth of
/// ink, because those are the pairs that are certainly rows: a band with one
/// run of ink in it is the thing being measured and cannot be part of the
/// measure. A run with no such pair falls back to every gap it has, and a run
/// with no gap at all answers zero, which refuses every band.
fn row_white(rows: &[Row], cells: &[Vec<Segment>]) -> f64 {
    let gap = |i: usize| (rows[i].bottom - rows[i + 1].top).max(0.0);
    let mut between: Vec<f64> = (0..rows.len().saturating_sub(1))
        .filter(|i| cells[*i].len() >= MIN_COLUMNS && cells[i + 1].len() >= MIN_COLUMNS)
        .map(gap)
        .collect();
    if between.is_empty() {
        between = (0..rows.len().saturating_sub(1)).map(gap).collect();
    }
    if between.is_empty() {
        return 0.0;
    }
    between.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    between[between.len() / 2]
}

/// Whether the band at `at` is set on the table's own pitch, above and below.
///
/// Both gaps, because a caption has its white on one side of it only when the
/// table above it ends tight -- and a band set apart on either side is not a
/// row of what is on the other side of the white.
fn band_of_the_table(rows: &[Row], at: usize, white: f64) -> bool {
    if at == 0 || at + 1 >= rows.len() {
        return false;
    }
    let limit = SECTION_WHITE * white;
    let above = (rows[at - 1].bottom - rows[at].top).max(0.0);
    let below = (rows[at].bottom - rows[at + 1].top).max(0.0);
    above <= limit && below <= limit
}

/// The runs of ink in one row, left to right: the cells, before anything knows
/// which column each of them is in.
///
/// The spans of every line the row is drawn on are taken together in x order
/// and merged while they are closer than a gutter, so a cell whose text wrapped
/// comes out whole: its two lines stand at the same x, which is what puts them
/// in one segment and, later, in one cell.
fn segments(lines: &[&TextBlock], row: &Row) -> Vec<Segment> {
    let em = row.font_size.max(f64::MIN_POSITIVE);
    let gap = SEGMENT_GAP_EM * em;
    let mut order: Vec<(usize, usize)> = (row.first..row.end)
        .flat_map(|line| {
            (0..lines[line].spans.len())
                .filter(move |span| !lines[line].spans[*span].text.trim().is_empty())
                .map(move |span| (line, span))
        })
        .collect();
    order.sort_by(|a, b| {
        lines[a.0].spans[a.1]
            .x
            .partial_cmp(&lines[b.0].spans[b.1].x)
            .unwrap_or(Ordering::Equal)
    });

    let mut out: Vec<Segment> = Vec::new();
    for at in order {
        let span = &lines[at.0].spans[at.1];
        let left = span.x;
        let right = crate::blocks::span_right(span);
        match out.last_mut() {
            Some(last) if left - last.right < gap => {
                last.right = last.right.max(right);
                last.spans.push(at);
            }
            _ => out.push(Segment {
                left,
                right,
                spans: vec![at],
            }),
        }
    }
    out
}

/// Whether two rows of the run put white at the same x, wide enough that no
/// justifier put it there.
///
/// This is the evidence that the run is a table at all where the document ruled
/// nothing: one row's wide gap is one row's word space, and the same gap in the
/// row under it is a gutter. The same statement `crate::blocks` makes about two
/// lines before it refuses to merge them (#545), made here about two rows.
fn gutters_agree(per_row: &[Vec<Segment>], run: &[Row]) -> bool {
    let wide = |at: usize| -> Vec<(f64, f64)> {
        let em = run[at].font_size.max(f64::MIN_POSITIVE);
        per_row[at]
            .windows(2)
            .map(|pair| (pair[0].right, pair[1].left))
            .filter(|(left, right)| right - left >= CONFIRM_GAP_EM * em)
            .collect()
    };
    (0..per_row.len().saturating_sub(1)).any(|at| {
        let (mine, theirs) = (wide(at), wide(at + 1));
        mine.iter()
            .any(|a| theirs.iter().any(|b| a.0 < b.1 && b.0 < a.1))
    })
}

/// The grid of one candidate run, or `None` where the run is not a table.
fn grid(
    run: &[Row],
    per_row: &[Vec<Segment>],
    first: usize,
    rules: &[HorizontalRule],
) -> Option<Table> {
    // The run has to have said "table" for something other than the fact that
    // its rows break in more than one place: either two rows put white at the
    // same x, or the document ruled the rows apart itself. Two lines that
    // happen to break twice are two lines.
    let confirmed = gutters_agree(per_row, run) || ruled_rows(run, rules) >= MIN_ROWS - 1;
    if !confirmed {
        return None;
    }

    let em = run
        .iter()
        .map(|row| row.font_size)
        .fold(0.0_f64, f64::max)
        .max(f64::MIN_POSITIVE);
    let left = run.iter().map(|row| row.left).fold(f64::INFINITY, f64::min);
    let right = run
        .iter()
        .map(|row| row.right)
        .fold(f64::NEG_INFINITY, f64::max);
    if !(left.is_finite() && right > left) {
        return None;
    }

    let bounds = column_bounds(per_row, left, right, em)?;
    let columns = bounds.len();
    if columns < MIN_COLUMNS {
        return None;
    }

    // Every segment against the grid. A segment that reaches into two columns
    // is one cell spanning them; two segments that reach into the same column
    // are a row the grid does not describe, and the whole run is given back to
    // the paragraph merge rather than written out as a table that is wrong.
    let mut cells: Vec<Cell> = Vec::new();
    for (r, segments) in per_row.iter().enumerate() {
        // A BAND WITH ONE RUN OF INK IN IT IS ONE CELL, AS WIDE AS ITS INK.
        // `find` admits such a band only inside a run whose other rows are
        // grids, so it is the head set across the table -- and the columns it
        // covers are the columns its ink covers, which is what every other
        // cell here is measured by.
        //
        // NOT OUT TO THE TABLE'S EDGE. "Nothing stands to its right" is an
        // argument about the row and not a measurement of the cell: it reads
        // a head over two columns of four as a merged cell over all four,
        // and it feeds the fill test a width the page never drew (codex,
        // #576). A row with one short value in its first column keeps that
        // column, which is what a sparse row of data is.
        if segments.len() == 1 {
            let (from, to) = covered(&segments[0], &bounds)?;
            cells.push(Cell {
                row: r as u16,
                column: from as u16,
                row_span: 1,
                column_span: (to - from + 1) as u16,
                header: false,
                spans: segments[0].spans.clone(),
            });
            continue;
        }
        let claims = claim_columns(segments, &bounds)?;
        for (segment, (from, to)) in segments.iter().zip(claims) {
            cells.push(Cell {
                row: r as u16,
                column: from as u16,
                row_span: 1,
                column_span: (to - from + 1) as u16,
                header: false,
                spans: segment.spans.clone(),
            });
        }
    }

    // HOW FULL, COUNTED IN COLUMNS AND NOT IN CELLS. A cell that spans three
    // columns fills three of them, and a count of cells says it fills one --
    // which turned a table with a merged header, or a section head across it,
    // into a scatter of words at a few left edges and gave the whole run back
    // to the paragraph merge (#576).
    let filled: usize = cells.iter().map(|cell| cell.column_span as usize).sum();
    if filled < (MIN_FILL * (run.len() * columns) as f64).ceil() as usize {
        return None;
    }
    // A run whose every row is one cell across the whole grid is a paragraph
    // with a wide word space in it, not a table.
    if cells
        .iter()
        .all(|cell| cell.column_span as usize == columns)
    {
        return None;
    }

    merge_rows(&mut cells, run, &bounds, rules);
    mark_header(&mut cells, run, rules);
    cells.sort_by_key(|cell| (cell.row, cell.column));

    // WHAT THE BUILDER HAD TO GO ON (#574). `filled` and `columns` are the numbers
    // the fill test above already computes and then reduces to a pass; `full_rows`
    // is what `column_bounds` derives the boundaries from, and a grid resting on
    // one of them has almost no evidence for where its columns are. Recorded here
    // because here is where they are known; `find` adds the two it alone sees.
    let mut per_row = vec![0usize; run.len()];
    for cell in &cells {
        if let Some(slot) = per_row.get_mut(cell.row as usize) {
            *slot += cell.column_span as usize;
        }
    }
    let doubt = crate::blocks::GridDoubt {
        thin_rows: 0,
        fill: filled as f64 / (run.len() * columns).max(1) as f64,
        retried_from: 0,
        full_rows: per_row
            .iter()
            .filter(|covered| **covered >= columns)
            .count(),
        columns,
    };

    Some(Table {
        first,
        end: first + run.len(),
        cells,
        doubt,
        origin: crate::table_trace::TableOrigin::Other,
    })
}

/// How many of a run's row boundaries the document ruled.
fn ruled_rows(run: &[Row], rules: &[HorizontalRule]) -> usize {
    (0..run.len().saturating_sub(1))
        .filter(|i| row_rule(&run[*i], &run[i + 1], rules).is_some())
        .count()
}

/// The ink the document drew between two rows, as the x ranges it covers, when
/// it covers most of the pair's width.
///
/// THE RULE IS NOT ONE PATH. A separator broken around a merged cell is drawn
/// as two subpaths -- which is what makes the break readable at all -- and
/// `record_rule` stores each of them as a `HorizontalRule` of its own. A test
/// that asks whether ONE of them crosses half the table rejects both on exactly
/// the tables this exists to read, because the break is what shortens them
/// (codex, round 1). So the ranges are taken together, overlapping ones merged,
/// and the width they cover between them is what decides.
fn row_rule(above: &Row, below: &Row, rules: &[HorizontalRule]) -> Option<Vec<(f64, f64)>> {
    // GROUPED BY HEIGHT FIRST. Two pieces of one broken rule lie on one line;
    // two unrelated marks between the same pair of rows do not, and adding
    // their widths together makes a separator the document never drew -- which
    // is enough to call a run a table and to infer a header and a row span
    // inside it (codex, round 2). So the pieces are collected per height, and
    // the best of those heights is what the pair is judged on.
    let mut ink: Vec<&HorizontalRule> = rules
        .iter()
        .filter(|rule| rule.y >= below.top && rule.y <= above.bottom)
        .filter(|rule| rule.right > rule.left)
        .collect();
    if ink.is_empty() {
        return None;
    }
    ink.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(Ordering::Equal));

    let left = above.left.min(below.left);
    let right = above.right.max(below.right);
    let width = (right - left).max(f64::MIN_POSITIVE);
    let apart = COLLINEAR_EM * above.font_size.max(below.font_size).max(f64::MIN_POSITIVE);

    let mut best: Option<(f64, Vec<(f64, f64)>)> = None;
    let mut at = 0usize;
    while at < ink.len() {
        let mut end = at + 1;
        while end < ink.len() && ink[end].y - ink[end - 1].y <= apart {
            end += 1;
        }
        let mut spans: Vec<(f64, f64)> = ink[at..end].iter().map(|r| (r.left, r.right)).collect();
        spans.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));
        let mut merged: Vec<(f64, f64)> = Vec::with_capacity(spans.len());
        for span in spans {
            match merged.last_mut() {
                Some(last) if span.0 <= last.1 => last.1 = last.1.max(span.1),
                _ => merged.push(span),
            }
        }
        let covered = covered_width(&merged, left, right);
        if best.as_ref().is_none_or(|(most, _)| covered > *most) {
            best = Some((covered, merged));
        }
        at = end;
    }

    best.filter(|(covered, _)| covered / width >= ROW_RULE_FRACTION)
        .map(|(_, merged)| merged)
}

/// How much of `[from, to]` a set of disjoint, ascending x ranges covers.
fn covered_width(ink: &[(f64, f64)], from: f64, to: f64) -> f64 {
    ink.iter()
        .map(|(left, right)| (right.min(to) - left.max(from)).max(0.0))
        .sum()
}

/// The columns of the whole run: the x ranges between the strips of white that
/// run down through it.
///
/// Returns the column ranges left to right, with each boundary set at the
/// middle of the strip that made it -- so a cell ending a hair into the gutter,
/// which is what kerning and a right-aligned figure both do, still lands in the
/// column it was set in.
fn column_bounds(
    per_row: &[Vec<Segment>],
    left: f64,
    right: f64,
    em: f64,
) -> Option<Vec<(f64, f64)>> {
    // Every edge any row's ink starts or stops at. The strips between two
    // adjacent edges are the elementary intervals: inside one of them, every
    // row either has ink or has not, so counting the rows that leave it alone
    // is a count and not a search.
    let mut edges: Vec<f64> = vec![left, right];
    for row in per_row {
        for segment in row {
            edges.push(segment.left.max(left));
            edges.push(segment.right.min(right));
        }
    }
    edges.retain(|x| *x >= left && *x <= right);
    edges.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    edges.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
    if edges.len() < 2 {
        return None;
    }

    let needed = ((GUTTER_ROW_FRACTION * per_row.len() as f64).ceil() as usize)
        .max(GUTTER_MIN_ROWS)
        .min(per_row.len());
    let free: Vec<bool> = edges
        .windows(2)
        .map(|pair| {
            let (from, to) = (pair[0], pair[1]);
            let rows = per_row
                .iter()
                .filter(|row| {
                    !row.iter()
                        .any(|segment| segment.left < to && segment.right > from)
                })
                .count();
            rows >= needed
        })
        .collect();

    // The strips, run together and kept where they are wide enough to be a
    // gutter rather than the space after a full stop.
    let mut gutters: Vec<(f64, f64)> = Vec::new();
    let mut open: Option<(f64, f64)> = None;
    for (i, is_free) in free.iter().enumerate() {
        match (is_free, open) {
            (true, Some((from, _))) => open = Some((from, edges[i + 1])),
            (true, None) => open = Some((edges[i], edges[i + 1])),
            (false, Some(strip)) => {
                gutters.push(strip);
                open = None;
            }
            (false, None) => {}
        }
    }
    gutters.extend(open);
    gutters.retain(|(from, to)| to - from >= GUTTER_EM * em);

    let mut bounds: Vec<(f64, f64)> = Vec::new();
    let mut start = left;
    for (from, to) in &gutters {
        // A gutter that runs off either end of the run bounds no column on that
        // side: the table starts and stops at its ink.
        if *from <= start {
            start = start.max(*to);
            continue;
        }
        if *to >= right {
            break;
        }
        let middle = (from + to) * 0.5;
        bounds.push((start, middle));
        start = middle;
    }
    if right > start {
        bounds.push((start, right));
    }
    Some(bounds)
}

/// The columns a segment reaches into, first and last.
///
/// `None` where it reaches into none, which the caller reads as "this run is
/// not a grid": every segment lies inside the run's own extent, so a segment
/// with no column is a bug in the bounds rather than a fact about the page.
fn covered(segment: &Segment, bounds: &[(f64, f64)]) -> Option<(usize, usize)> {
    let hit: Vec<usize> = bounds
        .iter()
        .enumerate()
        .filter(|(_, (from, to))| segment.left < *to && segment.right > *from)
        .map(|(i, _)| i)
        .collect();
    Some((*hit.first()?, *hit.last()?))
}

/// Which columns each cell of one row holds: a column belongs to the cell whose
/// ink covers most of it.
///
/// `None` where the row cannot be read against the grid at all -- a cell with
/// no column of its own, which is a row the grid does not describe -- and the
/// caller gives the whole run back to the paragraph merge rather than writing
/// out a table that is wrong.
///
/// WHY MOST OF IT AND NOT ANY OF IT. The boundary between two columns is the
/// middle of the strip of white the rows left free, and the rows that leave it
/// free are the ones with figures in them. A header set in words is wider than
/// the figures under it and overhangs that strip by a few points -- so, with any
/// overlap claiming a column, the header of column three reached into column
/// four, the next cell of the same row found column four taken, and the run was
/// refused. Seven rows of a table came back as seven paragraphs for a two-point
/// overhang (#577).
///
/// Asking who covers MOST of a column answers that without softening the case
/// the strict test was written for: a cell the document really did set across
/// two columns covers both of them and nothing else competes, so it still comes
/// out as one cell with a span of two; two cells that genuinely stand in one
/// column still leave one of them with no column at all, and the run is still
/// refused.
fn claim_columns(segments: &[Segment], bounds: &[(f64, f64)]) -> Option<Vec<(usize, usize)>> {
    let overlap = |segment: &Segment, column: &(f64, f64)| {
        (segment.right.min(column.1) - segment.left.max(column.0)).max(0.0)
    };
    // The owner of each column: the segment whose ink covers most of it, and
    // the leftmost of them where two cover the same width, so that the answer
    // does not depend on the order the page drew its glyphs in.
    let owner: Vec<Option<usize>> = bounds
        .iter()
        .map(|column| {
            segments
                .iter()
                .enumerate()
                .map(|(at, segment)| (at, overlap(segment, column)))
                .filter(|(_, covered)| *covered > 0.0)
                .fold(
                    None,
                    |best: Option<(usize, f64)>, (at, covered)| match best {
                        Some((_, most)) if most >= covered => best,
                        _ => Some((at, covered)),
                    },
                )
                .map(|(at, _)| at)
        })
        .collect();

    let mut claims = Vec::with_capacity(segments.len());
    for at in 0..segments.len() {
        let held: Vec<usize> = owner
            .iter()
            .enumerate()
            .filter(|(_, held)| **held == Some(at))
            .map(|(column, _)| column)
            .collect();
        let (first, last) = match (held.first(), held.last()) {
            // A CELL THAT OWNS NO COLUMN IS A ROW THE GRID DOES NOT DESCRIBE.
            // Its ink is inside a column another cell of the same row covers
            // more of, which is two cells in one column -- the case the strict
            // test was for, and it is still refused.
            (Some(first), Some(last)) => (*first, *last),
            _ => return None,
        };
        // And a cell whose columns are not next to each other describes a row
        // that overlaps another cell of it in the middle, which is the same
        // refusal made about a wider shape.
        if last - first + 1 != held.len() {
            return None;
        }
        claims.push((first, last));
    }
    Some(claims)
}

/// Which cells the document merged down the page, read off the rules it drew.
///
/// The rule between two rows runs the width of the table; where two cells are
/// one, it stops. So a column the rule does not cover, whose upper row has a
/// cell and whose lower row has none, is a cell that spans both -- and a column
/// with no rule over it at all is simply an empty cell.
fn merge_rows(cells: &mut [Cell], run: &[Row], bounds: &[(f64, f64)], rules: &[HorizontalRule]) {
    for r in 0..run.len().saturating_sub(1) {
        let Some(ink) = row_rule(&run[r], &run[r + 1], rules) else {
            continue;
        };
        for (column, (from, to)) in bounds.iter().enumerate() {
            let width = (to - from).max(f64::MIN_POSITIVE);
            if covered_width(&ink, *from, *to) / width >= CELL_RULE_FRACTION {
                continue;
            }
            let below_is_empty = !cells
                .iter()
                .any(|cell| cell.row as usize == r + 1 && spans_column(cell, column));
            if !below_is_empty {
                continue;
            }
            if let Some(cell) = cells.iter_mut().find(|cell| {
                (cell.row + cell.row_span) as usize == r + 1 && spans_column(cell, column)
            }) {
                cell.row_span += 1;
            }
        }
    }
}

/// Whether a cell covers a column, spans counted.
fn spans_column(cell: &Cell, column: usize) -> bool {
    let from = cell.column as usize;
    column >= from && column < from + cell.column_span as usize
}

/// Whether the table's first row is its header, and it is the first row or
/// nothing.
///
/// TWO WAYS A DOCUMENT SAYS SO, and they are the two it has: it rules the first
/// row off from the rest and rules nothing else, or it sets the first row bold
/// and the body not. A third row of evidence -- the first row is short, or it
/// has no digits in it -- is a guess about what tables say rather than about
/// what this one drew, and a header claimed wrongly moves a row of data out of
/// the body of every reader downstream.
fn mark_header(cells: &mut [Cell], run: &[Row], rules: &[HorizontalRule]) {
    if run.len() < MIN_ROWS + 1 {
        // With two rows there is no body to stand the first against: a rule
        // between them is the rule of a two-row table and bold in the first is
        // a table of two labels. Nothing is claimed.
        return;
    }
    let ruled_off = row_rule(&run[0], &run[1], rules).is_some()
        && (1..run.len() - 1).all(|i| row_rule(&run[i], &run[i + 1], rules).is_none());
    let body_bold = run[1..].iter().filter(|row| row.bold).count();
    let bold_header =
        run[0].bold && (body_bold as f64) < HEADER_BOLD_FRACTION * (run.len() - 1) as f64;
    if !(ruled_off || bold_header) {
        return;
    }
    for cell in cells.iter_mut().filter(|cell| cell.row == 0) {
        cell.header = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::TextSpan;

    fn span(text: &str, x: f64, y: f64) -> TextSpan {
        TextSpan {
            text: text.to_string(),
            x,
            y,
            width: 5.0 * text.chars().count() as f64,
            height: 10.0,
            font_size: 10.0,
            ..Default::default()
        }
    }

    fn line(cells: &[(&str, f64)], y: f64) -> TextBlock {
        TextBlock {
            spans: cells
                .iter()
                .map(|(text, x)| span(text, *x, y))
                .collect::<Vec<_>>(),
        }
    }

    /// One row per line, which is what a table with no wrapped cell gives.
    fn rows_of(lines: &[&TextBlock]) -> Vec<Row> {
        lines
            .iter()
            .enumerate()
            .map(|(i, line)| {
                let y = line.spans[0].y;
                Row {
                    first: i,
                    end: i + 1,
                    left: line.spans.iter().map(|s| s.x).fold(f64::INFINITY, f64::min),
                    right: line
                        .spans
                        .iter()
                        .map(crate::blocks::span_right)
                        .fold(f64::NEG_INFINITY, f64::max),
                    top: y + 8.0,
                    bottom: y,
                    font_size: 10.0,
                    bold: false,
                }
            })
            .collect()
    }

    /// Three rows at three left edges: three columns, nine cells, and every
    /// cell in the column it was set in.
    /// A table that carries on under a heading of its own is one table (#600).
    ///
    /// `find`'s run ends where `grid` stops accepting rows or where a band holds too
    /// few runs of ink -- NOT at a wide gap, which is what my first fixture assumed
    /// and why it passed without the rule. What ends a run here is the single-run
    /// band between the banks: #576 reads such a band as a row of the table when the
    /// page puts no more white round it than between its rows, and this one has far
    /// more, so the run stops. Two tables come out of one region.
    ///
    /// On 28 of the 128 regions measured over 120 pages that is what happens, which
    /// is why the question is answered here and not by joining regions -- the first
    /// version of the rule merged regions and never fired once.
    ///
    /// Red without `one_table_that_carries_on_is_one_table`: two tables, not one.
    #[test]
    fn a_table_that_carries_on_under_its_own_heading_is_one_table() {
        let drawn = [
            line(
                &[("Region", 72.0), ("Amount", 200.0), ("Share", 320.0)],
                700.0,
            ),
            line(&[("North", 72.0), ("120", 200.0), ("41%", 320.0)], 686.0),
            line(&[("South", 72.0), ("95", 200.0), ("32%", 320.0)], 672.0),
            // A head set across the table, with much more white round it than the
            // rows have between them, so #576 will not read it as a row.
            line(&[("Southern hemisphere", 72.0)], 640.0),
            line(&[("East", 72.0), ("60", 200.0), ("20%", 320.0)], 610.0),
            line(&[("West", 72.0), ("21", 200.0), ("7%", 320.0)], 596.0),
        ];
        let lines: Vec<&TextBlock> = drawn.iter().collect();
        let rows = rows_of(&lines);
        let found = find(&lines, &rows, &[]);
        assert_eq!(found.len(), 1, "the table was left in pieces: {found:?}");
    }

    /// The same page, with `TABLE 3 | ...` where the heading was: two tables (#600).
    ///
    /// The geometry is identical to the test above -- same rows, same columns, the
    /// same white round the band between them. Only the words differ, which is the
    /// whole finding: on the geometry a heading inside one table and a caption
    /// announcing the next are one shape, and the page's own words are what tells
    /// them apart. Red without the caption veto, and red with the exact-case
    /// vocabulary `pdf-learned` shipped: `TABLE 3` is how one of the 33 boundaries
    /// in the measured set writes it.
    #[test]
    fn a_caption_in_capitals_announces_a_table_of_its_own() {
        let drawn = [
            line(
                &[("Region", 72.0), ("Amount", 200.0), ("Share", 320.0)],
                700.0,
            ),
            line(&[("North", 72.0), ("120", 200.0), ("41%", 320.0)], 686.0),
            line(&[("South", 72.0), ("95", 200.0), ("32%", 320.0)], 672.0),
            line(&[("TABLE 3 | The same by subtype", 72.0)], 640.0),
            line(&[("East", 72.0), ("60", 200.0), ("20%", 320.0)], 610.0),
            line(&[("West", 72.0), ("21", 200.0), ("7%", 320.0)], 596.0),
        ];
        let lines: Vec<&TextBlock> = drawn.iter().collect();
        let rows = rows_of(&lines);
        let found = find(&lines, &rows, &[]);
        assert_eq!(
            found.len(),
            2,
            "a caption announcing the next table was read as a heading inside this \
             one: {found:?}"
        );
    }

    /// `TABLE I` and `Table S2` announce a table too (codex, #1969).
    ///
    /// Both forms are attested in the sets this engine is graded on -- `TABLE I` in
    /// DocLayNet's round of 24-09, `Table S2` in PubTables' -- and a caption the
    /// veto misses is a wrong join, which is the costlier error. Red without the
    /// Roman and supplementary identifiers: the run is joined and the caption is
    /// absorbed as a cell.
    #[test]
    fn a_roman_or_supplementary_caption_announces_a_table_of_its_own() {
        for caption in ["TABLE I Something else entirely", "Table S2 The supplement"] {
            let drawn = [
                line(
                    &[("Region", 72.0), ("Amount", 200.0), ("Share", 320.0)],
                    700.0,
                ),
                line(&[("North", 72.0), ("120", 200.0), ("41%", 320.0)], 686.0),
                line(&[("South", 72.0), ("95", 200.0), ("32%", 320.0)], 672.0),
                line(&[(caption, 72.0)], 640.0),
                line(&[("East", 72.0), ("60", 200.0), ("20%", 320.0)], 610.0),
                line(&[("West", 72.0), ("21", 200.0), ("7%", 320.0)], 596.0),
            ];
            let lines: Vec<&TextBlock> = drawn.iter().collect();
            let rows = rows_of(&lines);
            let found = find(&lines, &rows, &[]);
            assert_eq!(
                found.len(),
                2,
                "{caption:?} was read as a heading inside the table above it: {found:?}"
            );
        }
    }

    /// And an English word made of Roman letters is not an identifier (#600).
    ///
    /// `roman_value` answers arithmetic rather than intent -- it reads `iiii` as
    /// four -- so on its own it accepts `MIX` and `CIVIL`. A row whose first cell
    /// reads `Table` and whose second is such a word is a row, not a caption, and
    /// joining the table it sits in is the whole point of the rule.
    #[test]
    fn a_word_of_roman_letters_is_not_a_table_number() {
        let drawn = [
            line(
                &[("Region", 72.0), ("Amount", 200.0), ("Share", 320.0)],
                700.0,
            ),
            line(&[("North", 72.0), ("120", 200.0), ("41%", 320.0)], 686.0),
            line(&[("South", 72.0), ("95", 200.0), ("32%", 320.0)], 672.0),
            line(&[("Table CIVIL and military", 72.0)], 640.0),
            line(&[("East", 72.0), ("60", 200.0), ("20%", 320.0)], 610.0),
            line(&[("West", 72.0), ("21", 200.0), ("7%", 320.0)], 596.0),
        ];
        let lines: Vec<&TextBlock> = drawn.iter().collect();
        let rows = rows_of(&lines);
        let found = find(&lines, &rows, &[]);
        assert_eq!(
            found.len(),
            1,
            "an English word of Roman letters was read as a table number: {found:?}"
        );
    }

    /// A numbered section head is not a caption, so the table carries on (#600).
    ///
    /// `7.2 Study design` opens with a word-and-number just as `Table 2` does, which
    /// is why the vocabulary is a vocabulary and not "a word followed by a digit":
    /// T3 named this shape, `pdf-learned`'s `section_number` already reads it, and
    /// in the measured set a numbered section head stood between two candidates of
    /// ONE annotated table seven times and between different ones never.
    ///
    /// Red when a band that merely OPENS with a number counts as a caption, which is
    /// the shape a reader reaches for first and which this page would lose a table
    /// to.
    #[test]
    fn a_numbered_section_head_is_not_a_caption() {
        let drawn = [
            line(
                &[("Region", 72.0), ("Amount", 200.0), ("Share", 320.0)],
                700.0,
            ),
            line(&[("North", 72.0), ("120", 200.0), ("41%", 320.0)], 686.0),
            line(&[("South", 72.0), ("95", 200.0), ("32%", 320.0)], 672.0),
            line(&[("7.2 Southern hemisphere", 72.0)], 640.0),
            line(&[("East", 72.0), ("60", 200.0), ("20%", 320.0)], 610.0),
            line(&[("West", 72.0), ("21", 200.0), ("7%", 320.0)], 596.0),
        ];
        let lines: Vec<&TextBlock> = drawn.iter().collect();
        let rows = rows_of(&lines);
        let found = find(&lines, &rows, &[]);
        assert_eq!(
            found.len(),
            1,
            "a numbered section head was read as the caption of a new table: {found:?}"
        );
    }

    /// A joined table keeps the doubt of both halves (codex on #1989).
    ///
    /// The continuation rule calls `grid` directly on the combined range, so the
    /// rebuilt table would report the zero `grid` fills in rather than the thin
    /// row #576's rule admitted inside one of the halves. That is the case the
    /// learned model's gate is about -- it asks for at least one thin row -- so a
    /// joined table reporting none would say the builder had no doubt exactly
    /// where it had most.
    ///
    /// Red without the carry-over: `thin_rows` reads 0.
    #[test]
    fn a_joined_table_keeps_the_doubt_of_both_halves() {
        let drawn = [
            line(
                &[("Region", 72.0), ("Amount", 200.0), ("Share", 320.0)],
                700.0,
            ),
            line(&[("North", 72.0), ("120", 200.0), ("41%", 320.0)], 686.0),
            line(&[("South", 72.0), ("95", 200.0), ("32%", 320.0)], 672.0),
            // A head with far more white round it than the rows have between
            // them, so the run ends here and the continuation rule rejoins it.
            line(&[("Southern hemisphere", 72.0)], 640.0),
            line(&[("East", 72.0), ("60", 200.0), ("20%", 320.0)], 610.0),
            line(&[("West", 72.0), ("21", 200.0), ("7%", 320.0)], 596.0),
        ];
        let lines: Vec<&TextBlock> = drawn.iter().collect();
        let found = find(&lines, &rows_of(&lines), &[]);
        assert_eq!(found.len(), 1, "the halves were not joined: {found:?}");
        assert!(
            found[0].doubt.thin_rows >= 1,
            "the joined table reports no thin row, so the evidence of the halves \
             was lost: {:?}",
            found[0].doubt
        );
    }

    /// The builder records what it had to go on (#574).
    ///
    /// Three clean rows of three columns: nothing thin, nothing trimmed, every row
    /// full, and a fill of one because the page drew ink in every cell. A grid the
    /// builder had no doubt about, which is the case that pins the OTHER end --
    /// if these read anything but this, the facts are being computed from the
    /// wrong thing rather than merely being pessimistic.
    #[test]
    fn a_clean_grid_records_no_doubt() {
        let drawn = [
            line(
                &[("Region", 72.0), ("Amount", 200.0), ("Share", 320.0)],
                700.0,
            ),
            line(&[("North", 72.0), ("120", 200.0), ("41%", 320.0)], 686.0),
            line(&[("South", 72.0), ("95", 200.0), ("32%", 320.0)], 672.0),
        ];
        let lines: Vec<&TextBlock> = drawn.iter().collect();
        let found = find(&lines, &rows_of(&lines), &[]);
        assert_eq!(found.len(), 1, "{found:?}");
        let doubt = found[0].doubt;
        assert_eq!(doubt.columns, 3, "three columns: {doubt:?}");
        assert_eq!(doubt.full_rows, 3, "every row is full: {doubt:?}");
        assert_eq!(doubt.thin_rows, 0, "no row was admitted thin: {doubt:?}");
        assert_eq!(doubt.retried_from, 0, "no row had to be trimmed: {doubt:?}");
        assert!(
            (doubt.fill - 1.0).abs() < 1e-9,
            "every cell has ink, so the fill is one: {doubt:?}"
        );
    }

    /// And it records the doubt when there is some (#574).
    ///
    /// The same table with a section head across it, which #576 admits as a row on
    /// the strength of its neighbours: that row is thin, it is not full, and the
    /// fill drops because the head fills one column of three. Each fact moves for
    /// its own reason, which is what publishing five of them is for.
    #[test]
    fn a_head_inside_a_table_is_recorded_as_doubt() {
        let drawn = [
            line(
                &[("Region", 72.0), ("Amount", 200.0), ("Share", 320.0)],
                700.0,
            ),
            line(&[("North", 72.0), ("120", 200.0), ("41%", 320.0)], 686.0),
            line(&[("Southern hemisphere", 72.0)], 672.0),
            line(&[("South", 72.0), ("95", 200.0), ("32%", 320.0)], 658.0),
            line(&[("East", 72.0), ("60", 200.0), ("20%", 320.0)], 644.0),
        ];
        let lines: Vec<&TextBlock> = drawn.iter().collect();
        let found = find(&lines, &rows_of(&lines), &[]);
        assert_eq!(found.len(), 1, "{found:?}");
        let doubt = found[0].doubt;
        assert_eq!(
            doubt.thin_rows, 1,
            "the head was admitted on its neighbours' strength: {doubt:?}"
        );
        assert!(
            doubt.full_rows < 5,
            "the head's row is not full, so not every row can be: {doubt:?}"
        );
        assert!(
            doubt.fill < 1.0,
            "a row of one cell in a three-column grid lowers the fill: {doubt:?}"
        );
    }

    #[test]
    fn a_column_is_a_strip_of_white_through_every_row() {
        let drawn = [
            line(
                &[("Region", 72.0), ("Amount", 200.0), ("Share", 320.0)],
                700.0,
            ),
            line(&[("North", 72.0), ("120", 200.0), ("41%", 320.0)], 686.0),
            line(&[("South", 72.0), ("95", 200.0), ("32%", 320.0)], 672.0),
        ];
        let lines: Vec<&TextBlock> = drawn.iter().collect();
        let found = find(&lines, &rows_of(&lines), &[]);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0].cells.len(), 9);
        assert_eq!(found[0].cells.iter().map(|cell| cell.column).max(), Some(2));
        assert!(found[0].cells.iter().all(|cell| cell.column_span == 1));
    }

    /// Prose is not a table however many lines of it there are: no strip of
    /// white runs down through it.
    #[test]
    fn running_text_has_no_column_in_it() {
        let drawn = [
            line(&[("the committee considered", 72.0)], 700.0),
            line(&[("the proposals put before", 72.0)], 686.0),
            line(&[("it and resolved that each", 72.0)], 672.0),
        ];
        let lines: Vec<&TextBlock> = drawn.iter().collect();
        assert!(find(&lines, &rows_of(&lines), &[]).is_empty());
    }

    /// Two short marks at two different heights between the same pair of rows
    /// are not a separator, however much of the width they cover between them
    /// (codex, round 2).
    #[test]
    fn marks_at_two_heights_are_not_one_rule() {
        let drawn = [
            line(&[("alpha", 72.0), ("one", 117.0)], 700.0),
            line(&[("beta", 72.0), ("two", 117.0)], 686.0),
            line(&[("gamma", 72.0), ("six", 117.0)], 672.0),
        ];
        let lines: Vec<&TextBlock> = drawn.iter().collect();
        let mark = |left: f64, right: f64, y: f64| HorizontalRule { left, right, y };
        // The gap between the two columns is under the three ems a gutter is
        // confirmed at, so the rules are the only evidence there could be;
        // apart, they are two marks and the run is three lines.
        let apart = [
            mark(70.0, 90.0, 699.0),
            mark(100.0, 125.0, 695.0),
            mark(70.0, 90.0, 685.0),
            mark(100.0, 125.0, 681.0),
        ];
        assert!(
            find(&lines, &rows_of(&lines), &apart).is_empty(),
            "two marks at two heights were read as a separator"
        );
        // Drawn on one line, the same two pieces ARE the separator a broken
        // rule is, and the run is a table.
        let together = [
            mark(70.0, 90.0, 697.0),
            mark(100.0, 125.0, 697.0),
            mark(70.0, 90.0, 683.0),
            mark(100.0, 125.0, 683.0),
        ];
        assert!(!find(&lines, &rows_of(&lines), &together).is_empty());
    }

    /// Two rows that break in the same two places once each are two rows of a
    /// table; two that break in different places are two lines.
    #[test]
    fn a_gutter_is_believed_only_when_a_second_row_shows_it() {
        let drawn = vec![
            line(&[("alpha", 72.0), ("one", 200.0)], 700.0),
            line(&[("beta gamma delta epsilon", 72.0), ("two", 330.0)], 686.0),
        ];
        let lines: Vec<&TextBlock> = drawn.iter().collect();
        assert!(find(&lines, &rows_of(&lines), &[]).is_empty(), "{drawn:?}");
    }
}
