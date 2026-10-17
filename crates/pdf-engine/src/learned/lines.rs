//! A block's lines, and what each line says about a list (#599).
//!
//! THE SAME LINES THE LABELS WERE PUT ON. `scripts/extraction/learned/
//! align_labels.py` (`lines_of`) groups a block's words into lines, and the
//! `opens` label is computed on exactly those lines. The model is then trained
//! on the vectors `scripts/extraction/learned/list_lines.py` makes of them. A
//! second grouping here would run the model on lines it was never labelled on,
//! so this is a port of both, line for line, and the parity test holds the
//! three together.
//!
//! Every place where Python and Rust could quietly disagree is written out:
//! Python's `round` rounds half to even, `str.split()` and `str.rstrip()` use
//! Unicode white space, and a sort in Python is stable.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::collections::HashMap;

use super::features::PageWord;

/// The line features, in the order the line model takes them. The same list
/// and order as `LINE_FEATURES` in `list_lines.py`.
#[doc(hidden)]
pub const LINE_FEATURES: [&str; 23] = [
    "marker_bullet",
    "marker_ordered",
    "marker_figure",
    "first_token_chars",
    "first_gap_em",
    "indent_em",
    "indent_vs_prev_em",
    "prev_short_em",
    "prev_ends_terminal",
    "prev_runs_on",
    "width_frac",
    "size_ratio",
    "bold",
    "line_index",
    "lines_in_block",
    "gap_above_em",
    "block_is_list",
    "block_size_ratio",
    "first_is_lone_symbol",
    "same_head_on_page",
    "block_is_table_cell",
    "max_gap_em",
    "prev_block_ends_colon",
];

const BULLETS: &[char] = &[
    '\u{2022}', '\u{2023}', '\u{2043}', '\u{2219}', '\u{b7}', '\u{25aa}', '\u{25ab}', '\u{25a0}',
    '\u{25a1}', '\u{25cf}', '\u{25cb}', '\u{25e6}', '\u{25c6}', '\u{25c7}', '\u{2756}', '\u{2713}',
    '\u{2714}',
];
const DASHES: &[&str] = &["-", "\u{2013}", "\u{2014}"];
const FIGURE_SIGNS: &[char] = &[
    '$', '\u{20ac}', '\u{a3}', '\u{a5}', '\u{a2}', '%', '\u{2030}', '#', '+', '=',
];
const RUNS_ON_ENDINGS: &[char] = &[',', ';', ':', '-', '\u{2013}', '\u{2014}'];
const RUNS_ON_WORDS: &[&str] = &["and", "or", "of", "the", "to"];

/// The words of one block grouped into lines, top to bottom; each line left to
/// right. `align_labels.lines_of`, ported.
///
/// Two words are on one line when their boxes overlap vertically by more than
/// half of the shorter of the two -- measured against the line's own growing
/// span, so a superscript opening a line does not decide its height.
#[doc(hidden)]
pub fn lines_of(block: usize, words: &[PageWord]) -> Vec<Vec<&PageWord>> {
    let ink: Vec<&PageWord> = words.iter().filter(|w| w.block == block).collect();
    group_sorted_words(ink)
}

/// One block's words grouped into lines: the tail of [`lines_of`], shared so
/// that grouping one block and grouping the whole page cannot disagree.
fn group_sorted_words(mut ink: Vec<&PageWord>) -> Vec<Vec<&PageWord>> {
    // Python's `sorted(key=(-y1, x0))`: stable, y1 descending, then x0.
    ink.sort_by(|a, b| {
        (-a.bbox[3])
            .partial_cmp(&(-b.bbox[3]))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                a.bbox[0]
                    .partial_cmp(&b.bbox[0])
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });
    let mut lines: Vec<Vec<&PageWord>> = Vec::new();
    for word in ink {
        let bx = word.bbox;
        let mut placed = false;
        for line in lines.iter_mut() {
            let low = line.iter().map(|w| w.bbox[1]).fold(f64::INFINITY, f64::min);
            let high = line
                .iter()
                .map(|w| w.bbox[3])
                .fold(f64::NEG_INFINITY, f64::max);
            let mut shorter = (bx[3] - bx[1]).min(high - low);
            if shorter == 0.0 {
                shorter = 1.0;
            }
            let share = (bx[3].min(high) - bx[1].max(low)) / shorter;
            if share > 0.5 {
                line.push(word);
                placed = true;
                break;
            }
        }
        if !placed {
            lines.push(vec![word]);
        }
    }
    for line in lines.iter_mut() {
        line.sort_by(|a, b| {
            a.bbox[0]
                .partial_cmp(&b.bbox[0])
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    in_reading_order(lines.into_iter().flat_map(across_a_column).collect())
}

/// Every block's words, `partition[block]` in the page's word order.
///
/// The one scan [`grouped_lines`] replaces `n_blocks` scans with: each
/// [`lines_of`] call filters the whole page for its block, so grouping a page
/// block by block reads every word once per block. A partition holds exactly
/// the words the filter would have held, in the same order -- the sort in
/// [`group_sorted_words`] is stable, so the same input order groups the same.
fn partition_words(words: &[PageWord], n_blocks: usize) -> Vec<Vec<&PageWord>> {
    let mut out: Vec<Vec<&PageWord>> = vec![Vec::new(); n_blocks];
    for word in words {
        if word.block < n_blocks {
            out[word.block].push(word);
        }
    }
    out
}

/// Every block's lines, `grouped[block]` in [`lines_of`] order.
///
/// Exactly what calling [`lines_of`] for each block returns, through the same
/// tail -- the page's words are partitioned once instead of filtered once per
/// block. The learned pass groups a page for its vectors, its style, its head
/// count and the judge; grouping once and passing the lines along is what
/// keeps those from each re-reading the page.
#[doc(hidden)]
pub fn grouped_lines(words: &[PageWord], n_blocks: usize) -> Vec<Vec<Vec<&PageWord>>> {
    partition_words(words, n_blocks)
        .into_iter()
        .map(group_sorted_words)
        .collect()
}

/// The pieces of a block, column by column and top to bottom inside each.
///
/// WHY ORDER IS PART OF THE CUT. A cut is an INDEX, and `parts_of` reads it as
/// a monotonic boundary: every line at or after the cut goes to the part after
/// it. Split by height alone a two-column block comes out interleaved -- left
/// row 1, right row 1, left row 2 -- so no index separates one column from the
/// other, and a cut meant for one takes half of the other with it (codex,
/// #1992). In reading order a contiguous range of indices is a contiguous
/// piece of the page again.
///
/// Columns are found by overlap and not by a grid: a piece joins a column when
/// it shares more than half of the narrower of the two x-ranges with it. A
/// single-column block therefore comes back exactly as it went in.
fn in_reading_order<'a>(pieces: Vec<Vec<&'a PageWord>>) -> Vec<Vec<&'a PageWord>> {
    if pieces.len() < 2 {
        return pieces;
    }
    struct Column<'a> {
        left: f64,
        right: f64,
        pieces: Vec<Vec<&'a PageWord>>,
    }
    let mut columns: Vec<Column<'a>> = Vec::new();
    for piece in pieces {
        let left = piece
            .iter()
            .map(|w| w.bbox[0])
            .fold(f64::INFINITY, f64::min);
        let right = piece
            .iter()
            .map(|w| w.bbox[2])
            .fold(f64::NEG_INFINITY, f64::max);
        let mine = columns.iter().position(|column| {
            let mut width = (right - left).min(column.right - column.left);
            if width == 0.0 {
                width = 1.0;
            }
            (right.min(column.right) - left.max(column.left)) / width > 0.5
        });
        match mine {
            Some(k) => {
                columns[k].left = columns[k].left.min(left);
                columns[k].right = columns[k].right.max(right);
                columns[k].pieces.push(piece);
            }
            None => columns.push(Column {
                left,
                right,
                pieces: vec![piece],
            }),
        }
    }
    columns.sort_by(|a, b| {
        a.left
            .partial_cmp(&b.left)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut out = Vec::new();
    for mut column in columns {
        // Python sorts by `-max(y1)`, which is a stable sort on the negated top.
        column.pieces.sort_by(|a, b| {
            let top = |p: &Vec<&PageWord>| {
                -p.iter()
                    .map(|w| w.bbox[3])
                    .fold(f64::NEG_INFINITY, f64::max)
            };
            top(a)
                .partial_cmp(&top(b))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        out.extend(column.pieces);
    }
    out
}

/// How wide a gap inside a line has to be, in ems of the line's own height,
/// before it is a column boundary rather than a space. Measured, not chosen:
/// see `align_labels.COLUMN_GAP_EM`, which has to hold the same number.
const COLUMN_GAP_EM: f64 = 1.5;

/// One y-grouped line, cut where it crosses from one column into another.
///
/// WHY A LINE IS NOT A LINE. Words are grouped by their vertical overlap, so on
/// a two-column page the left column's line and the right column's line at the
/// same height come out as ONE line -- and every line feature then describes a
/// line that is not on the page: its width, its x, the gap to its neighbour.
///
/// It is not a rare shape. Of the lines the join could give to no region, 81.5
/// per cent straddle two regions and 77.4 per cent of those are side by side;
/// splitting them takes the unlabelled lines over 300 validation pages from 707
/// to 294 and quadruples the buried headings that are visible line by line,
/// from 6 to 25 (#602).
fn across_a_column(line: Vec<&PageWord>) -> Vec<Vec<&PageWord>> {
    if line.len() < 2 {
        return vec![line];
    }
    // The median word height, the same way Python takes it: sorted, index len/2.
    let mut heights: Vec<f64> = line.iter().map(|w| w.bbox[3] - w.bbox[1]).collect();
    heights.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut height = heights[heights.len() / 2];
    if height == 0.0 {
        height = 1.0;
    }
    let mut out: Vec<Vec<&PageWord>> = Vec::new();
    let mut run: Vec<&PageWord> = vec![line[0]];
    for pair in line.windows(2) {
        let (before, word) = (pair[0], pair[1]);
        if (word.bbox[0] - before.bbox[2]) / height >= COLUMN_GAP_EM {
            out.push(std::mem::take(&mut run));
        }
        run.push(word);
    }
    out.push(run);
    out
}

/// Each line's box and median size, in [`lines_of`] order: where a line
/// stands, for a feature that measures it against something else on the page.
pub fn line_boxes(block: usize, words: &[PageWord]) -> Vec<([f64; 4], f64)> {
    line_boxes_on_lines(&lines_of(block, words))
}

/// [`line_boxes`] over one block's lines grouped once: the same boxes,
/// without re-grouping the block for them.
#[doc(hidden)]
pub fn line_boxes_on_lines(lines: &[Vec<&PageWord>]) -> Vec<([f64; 4], f64)> {
    lines
        .iter()
        .map(|line| (line_box(line), line_size(line)))
        .collect()
}

/// Every non-empty line's box and middle word's size, in [`lines_of`] order:
/// the union of the line's word boxes at `sizes[len / 2]`.
///
/// THE ONE STEP the line network's per-line readers share: `drawings::page_vector`
/// and `raster::page_image` both start here, so the vector arm and the image
/// arm can never read different boxes or sizes for one line (#574). The size
/// is the raw median -- each reader takes `max(size, 1.0)` itself, the way
/// `line_drawings` always has.
pub fn line_boxes_and_sizes(block: usize, words: &[PageWord]) -> Vec<([f64; 4], f64)> {
    lines_of(block, words)
        .iter()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let mut sizes: Vec<f64> = line.iter().map(|w| w.size).collect();
            sizes.sort_by(f64::total_cmp);
            (line_box(line), sizes[sizes.len() / 2])
        })
        .collect()
}

fn line_box(line: &[&PageWord]) -> [f64; 4] {
    [
        line.iter().map(|w| w.bbox[0]).fold(f64::INFINITY, f64::min),
        line.iter().map(|w| w.bbox[1]).fold(f64::INFINITY, f64::min),
        line.iter()
            .map(|w| w.bbox[2])
            .fold(f64::NEG_INFINITY, f64::max),
        line.iter()
            .map(|w| w.bbox[3])
            .fold(f64::NEG_INFINITY, f64::max),
    ]
}

fn line_size(line: &[&PageWord]) -> f64 {
    let mut sizes: Vec<f64> = line.iter().map(|w| w.size).collect();
    sizes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let size = sizes[sizes.len() / 2];
    if size == 0.0 {
        1.0
    } else {
        size
    }
}

/// What the first token of a line is, as a list marker: `list_lines.marker_kind`.
fn marker_kind(token: &str) -> Option<&'static str> {
    let first = token.chars().next()?;
    let count = token.chars().count();
    if FIGURE_SIGNS.contains(&first) {
        return Some("figure");
    }
    if (count == 1 || count <= 2) && BULLETS.contains(&first) {
        return Some("bullet");
    }
    if DASHES.contains(&token) {
        return Some("bullet");
    }
    if is_ordinal(token) {
        return Some("ordered");
    }
    None
}

fn is_roman(s: &str) -> bool {
    let n = s.chars().count();
    (1..=6).contains(&n) && s.chars().all(|c| "ivxlcIVXLC".contains(c))
}

/// `list_lines.ORDINAL`, without a regex engine: `1.` `1)` `(1)` `[1]` `a.`
/// `a)` `(a)` `iv.` `iv)` `(iv)` `(iv` and the like, one to three digits.
fn is_ordinal(token: &str) -> bool {
    let digits =
        |s: &str| (1..=3).contains(&s.chars().count()) && s.chars().all(|c| c.is_ascii_digit());
    let letter = |s: &str| s.chars().count() == 1 && s.chars().all(|c| c.is_ascii_alphabetic());
    // `\[\d{1,3}\]`
    if let Some(inner) = token.strip_prefix('[').and_then(|t| t.strip_suffix(']')) {
        return digits(inner);
    }
    // `\(\d{1,3}\)`, `\([a-zA-Z]\)`, `\([ivxlc]{1,6}\)`
    if let Some(inner) = token.strip_prefix('(').and_then(|t| t.strip_suffix(')')) {
        if digits(inner) || letter(inner) || is_roman(inner) {
            return true;
        }
    }
    // `\(?X[.)]` for X a number, a letter or a roman numeral
    let Some(body) = token.strip_suffix(['.', ')']) else {
        return false;
    };
    let body = body.strip_prefix('(').unwrap_or(body);
    digits(body) || letter(body) || is_roman(body)
}

fn runs_on(text: &str) -> bool {
    let text = text.trim_end();
    if text.ends_with(RUNS_ON_ENDINGS) {
        return true;
    }
    text.split_whitespace()
        .next_back()
        .is_some_and(|last| RUNS_ON_WORDS.contains(&last))
}

/// Python's `round(x)` on a float: half to even.
fn round_half_even(x: f64) -> i64 {
    x.round_ties_even() as i64
}

/// How often each (first token, left edge in whole points) opens a line on the
/// page: `list_lines.heads_on_page`. A bullet list says so by repetition, and
/// that holds whatever the glyph decodes to.
#[doc(hidden)]
pub fn heads_on_page(n_blocks: usize, words: &[PageWord]) -> HashMap<(String, i64), u32> {
    heads_on_grouped(&grouped_lines(words, n_blocks))
}

/// [`heads_on_page`] over lines grouped once: the same count, without
/// re-grouping the page for it.
#[doc(hidden)]
pub fn heads_on_grouped(grouped: &[Vec<Vec<&PageWord>>]) -> HashMap<(String, i64), u32> {
    let mut heads = HashMap::new();
    for lines in grouped {
        for line in lines {
            if let Some(first) = line.first() {
                *heads
                    .entry((first.text.clone(), round_half_even(first.bbox[0])))
                    .or_insert(0) += 1;
            }
        }
    }
    heads
}

/// The facts about the block a line sits in that the line vector reads.
#[doc(hidden)]
pub struct LineContext<'a> {
    /// The block's box.
    pub bbox: [f64; 4],
    /// The block's kind, as the dump names it.
    pub kind: &'a str,
    /// The block's median glyph size.
    pub size: f64,
    /// The page's median glyph size.
    pub body_size: f64,
    /// Whether the page's previous block ends in a colon; `None` for the
    /// first block of the page.
    pub prev_ends_colon: Option<bool>,
}

/// One vector per line of the block, in the order [`lines_of`] returns them:
/// `list_lines.line_vectors`, ported.
#[doc(hidden)]
pub fn line_vectors(
    block: usize,
    context: &LineContext<'_>,
    words: &[PageWord],
    heads: &HashMap<(String, i64), u32>,
) -> Vec<[f64; 23]> {
    line_vectors_on_lines(&lines_of(block, words), context, heads)
}

/// [`line_vectors`] over one block's lines grouped once: the same vectors,
/// without re-grouping the block for them.
#[doc(hidden)]
pub fn line_vectors_on_lines(
    lines: &[Vec<&PageWord>],
    context: &LineContext<'_>,
    heads: &HashMap<(String, i64), u32>,
) -> Vec<[f64; 23]> {
    let bx = context.bbox;
    let measure = (bx[2] - bx[0]).max(1.0);
    let body = if context.body_size == 0.0 {
        1.0
    } else {
        context.body_size
    };
    let mut out = Vec::with_capacity(lines.len());
    for (at, line) in lines.iter().enumerate() {
        let bxl = line_box(line);
        let size = line_size(line);
        let first = line.first().map_or("", |w| w.text.as_str());
        let kind = marker_kind(first);
        let gap = if line.len() > 1 {
            (line[1].bbox[0] - line[0].bbox[2]) / size
        } else {
            f64::NAN
        };
        let (indent_vs_prev, prev_short, prev_terminal, prev_runs, gap_above) = if at > 0 {
            let prev = &lines[at - 1];
            let pbox = line_box(prev);
            let ptext = prev
                .iter()
                .map(|w| w.text.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            (
                (bxl[0] - pbox[0]) / size,
                (bx[2] - pbox[2]) / size,
                if ptext.trim_end().ends_with(['.', '!', '?']) {
                    1.0
                } else {
                    0.0
                },
                if runs_on(&ptext) { 1.0 } else { 0.0 },
                (pbox[1] - bxl[3]) / size,
            )
        } else {
            (f64::NAN, f64::NAN, f64::NAN, f64::NAN, f64::NAN)
        };
        let bold = line.iter().filter(|w| w.bold).count() as f64 / line.len() as f64;
        let first_chars = first.chars().count();
        let lone_symbol =
            first_chars == 1 && (!first.chars().all(char::is_alphanumeric) || gap >= 0.4);
        let block_size = if context.size == 0.0 {
            size
        } else {
            context.size
        };
        out.push([
            if kind == Some("bullet") { 1.0 } else { 0.0 },
            if kind == Some("ordered") { 1.0 } else { 0.0 },
            if kind == Some("figure") { 1.0 } else { 0.0 },
            first_chars as f64,
            gap,
            (bxl[0] - bx[0]) / size,
            indent_vs_prev,
            prev_short,
            prev_terminal,
            prev_runs,
            (bxl[2] - bxl[0]) / measure,
            size / body,
            bold,
            at as f64,
            lines.len() as f64,
            gap_above,
            if context.kind == "list_item" {
                1.0
            } else {
                0.0
            },
            block_size / body,
            if lone_symbol { 1.0 } else { 0.0 },
            f64::from(
                heads
                    .get(&(first.to_string(), round_half_even(line[0].bbox[0])))
                    .copied()
                    .unwrap_or(0),
            ),
            if context.kind == "table_cell" {
                1.0
            } else {
                0.0
            },
            line.windows(2)
                .map(|pair| (pair[1].bbox[0] - pair[0].bbox[2]) / size)
                .fold(
                    f64::NAN,
                    |acc, gap| if acc.is_nan() || gap > acc { gap } else { acc },
                ),
            match context.prev_ends_colon {
                None => f64::NAN,
                Some(true) => 1.0,
                Some(false) => 0.0,
            },
        ]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str, block: usize, x0: f64, y0: f64) -> PageWord {
        PageWord {
            text: text.to_string(),
            bbox: [x0, y0, x0 + 30.0, y0 + 10.0],
            block,
            span: 0,
            size: 10.0,
            bold: false,
            italic: false,
            font: None,
            color: None,
        }
    }

    fn texts(lines: &[Vec<&PageWord>]) -> Vec<Vec<String>> {
        lines
            .iter()
            .map(|line| line.iter().map(|w| w.text.clone()).collect())
            .collect()
    }

    /// Line vectors, compared the way `assert_eq!` cannot: missing features
    /// are NaN by design, and NaN never equals itself. Same shape, same
    /// values, same NaN placement -- a NaN against a number fails.
    fn assert_line_vectors_equal(block: usize, left: &[[f64; 23]], right: &[[f64; 23]]) {
        assert_eq!(left.len(), right.len(), "block {block}: line count differs");
        for (row, expected) in left.iter().zip(right.iter()) {
            for (got, ask) in row.iter().zip(expected.iter()) {
                assert!(
                    got == ask || (got.is_nan() && ask.is_nan()),
                    "block {block} vectors differ: {row:?} vs {expected:?}"
                );
            }
        }
    }

    /// Grouping the page once is grouping each block: the partition holds
    /// what the per-block filter held, and both go through the same tail.
    #[test]
    fn grouped_lines_match_lines_of_block_by_block() {
        let words = vec![
            word("left", 0, 72.0, 700.0),
            // Same height, a column gap away: one y-group, two lines.
            word("right", 0, 300.0, 700.0),
            word("second", 0, 72.0, 686.0),
            word("other", 1, 72.0, 700.0),
            word("block", 1, 110.0, 700.0),
        ];
        let grouped = grouped_lines(&words, 2);
        assert_eq!(grouped.len(), 2);
        for (block, lines) in grouped.iter().enumerate() {
            assert_eq!(
                texts(lines),
                texts(&lines_of(block, &words)),
                "block {block} groups differently whole-page than alone"
            );
        }
        assert!(
            grouped[0].len() >= 2,
            "the fixture groups to one line and tests nothing: {grouped:?}"
        );
        // The head count reads the grouped lines too.
        assert_eq!(heads_on_grouped(&grouped), heads_on_page(2, &words));
        // So do the vectors and the boxes, line for line.
        let heads = heads_on_page(2, &words);
        let context = LineContext {
            bbox: [72.0, 680.0, 340.0, 710.0],
            kind: "paragraph",
            size: 10.0,
            body_size: 10.0,
            prev_ends_colon: None,
        };
        for (block, lines) in grouped.iter().enumerate() {
            assert_line_vectors_equal(
                block,
                &line_vectors_on_lines(lines, &context, &heads),
                &line_vectors(block, &context, &words, &heads),
            );
            assert_eq!(
                line_boxes_on_lines(lines),
                line_boxes(block, &words),
                "block {block} boxes differ"
            );
        }
    }

    #[test]
    fn a_left_edge_is_rounded_the_way_python_rounds_it() {
        // `same_head_on_page` keys a line by its first token and its left edge
        // in whole points, and the model learned the keys Python's `round`
        // made: half to EVEN. Rust's `f64::round` rounds half away from zero,
        // so an edge at 72.5 would be 73 here and 72 in training, and a bullet
        // list at that edge would count as unrepeated (#599).
        assert_eq!(round_half_even(72.5), 72);
        assert_eq!(round_half_even(73.5), 74);
        assert_eq!(round_half_even(72.4), 72);
        assert_eq!(round_half_even(-0.5), 0);
    }
}
