//! The learned grid's rows, tidied after the decoder (#574).
//!
//! THE LOSS ON GOOD REGIONS IS ROWS. On 209 PubTables training tables whose
//! region we got right (the first 300 pages of the evaluation list, a region
//! good when it and the truth hold at least 0.9 of each other's text) the
//! grid scored TEDS 0.743, and 91 of the tables had more rows than the truth:
//! 69 % of the loss, where columns were about 8 %. Two things made the extra
//! rows, and this pass undoes both:
//!
//! * A ROW NOTHING STARTS IN. The decoder numbers a line two rows tall when a
//!   cell on it reaches further, and the rows under such lines are held by no
//!   cell -- `0, 3, 5, 7, ...` -- which the writer sets as empty rows. A row
//!   in which no cell with text starts is dropped, and a cell that reached
//!   over it reaches one row less.
//! * A ROW THAT CONTINUES THE ONE ABOVE. A label set on two lines comes out
//!   as two rows: the second holds only the words that run on (`and
//!   liabilities`, `(continued)`), in columns whose cell above is filled. It
//!   is joined to the row above.
//!
//! Measured on those 300 pages, this pass on and off in one build: on the
//! 197 good regions TEDS 0.744 -> 0.826, GriTS-Con 0.833 -> 0.889 and
//! GriTS-Top 0.850 -> 0.902; over all 352 truth tables TEDS 0.547 -> 0.604,
//! 47 tables up and none down. The DocLayNet proxy, which has no cells to
//! score, reads the same on every axis.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

#[cfg(test)]
use crate::table_trace::TableOrigin;
use crate::text::TextBlock;
use crate::{Block, BlockKind};

/// Words a cell's text ends on when the line under it runs on.
const RUNS_ON: [&str; 12] = [
    "and", "or", "of", "the", "in", "with", "for", "to", "by", "&", "a", "an",
];

/// One table's cells with its empty rows dropped and its run-on rows joined,
/// in row-major order. Cells that are not table cells are left as they are.
///
/// `drawn` is a grid read from the lines the page drew. Its rows are the
/// page's, and a row the page ruled and left blank is a blank row, so only a
/// row no cell at all stands in goes -- the thin strip between the two rules
/// of a double rule -- where the decoder's rows go when no cell with text
/// starts in them (codex, #2069).
pub(crate) fn tidy(cells: &mut Vec<Block>, drawn: bool, runs_on_figures: bool) {
    split_stacked_rows(cells, runs_on_figures);
    drop_rows_nothing_starts_in(cells, drawn);
    join_rows_that_run_on(cells, runs_on_figures);
    if !runs_on_figures {
        join_stacked_figure_rows(cells);
    }
    span_section_rows(cells);
    in_row_major_order(cells);
}

/// The cells in row-major order, in the places cells held: a block that is
/// no cell -- a note the table let go -- stays where it was.
fn in_row_major_order(blocks: &mut [Block]) {
    let slots: Vec<usize> = (0..blocks.len())
        .filter(|&i| position(&blocks[i]).is_some())
        .collect();
    let mut cells: Vec<Block> = slots.iter().map(|&i| blocks[i].clone()).collect();
    cells.sort_by_key(|b| position(b).map(|(r, c, _, _)| (r, c)));
    for (slot, cell) in slots.into_iter().zip(cells) {
        blocks[slot] = cell;
    }
}

/// `(row, column, row_span, column_span)` of a table cell.
fn position(block: &Block) -> Option<(u16, u16, u16, u16)> {
    match block.kind {
        BlockKind::TableCell {
            row,
            column,
            row_span,
            column_span,
            ..
        } => Some((row, column, row_span.max(1), column_span.max(1))),
        _ => None,
    }
}

fn set_rows(block: &mut Block, first: u16, span: u16) {
    if let BlockKind::TableCell { row, row_span, .. } = &mut block.kind {
        *row = first;
        *row_span = span.max(1);
    }
}

fn set_column_span(block: &mut Block, span: u16) {
    if let BlockKind::TableCell { column_span, .. } = &mut block.kind {
        *column_span = span.max(1);
    }
}

fn set_column(block: &mut Block, column: u16) {
    if let BlockKind::TableCell { column: at, .. } = &mut block.kind {
        *at = column;
    }
}

/// One table's cells with every column nothing stands in removed (#574).
///
/// The lattice numbers its columns from the page's rulings and skips the
/// stripes no word falls into, so a statement comes out with columns no cell
/// starts in -- beside Home Depot's `$` gutter two of them, which also keep
/// the header block, decoded with fewer columns, a table apart. A column is
/// empty when no cell starting in it holds non-whitespace text, header rows
/// included; a span from a neighbour does not count, it loses one span with
/// the column it covers. A cell starting in a removed column is gone -- it
/// is blank, so no text is ever lost -- and the rest keeps its rows, its
/// text and its boxes: only the column numbers change, and row-major order
/// with them.
///
/// A table left with fewer than two columns keeps its grid. Cells that are
/// not table cells are left as they are. The companion per cell -- the
/// pass's origin map -- follows its cell, so one that goes takes it with it.
pub(crate) fn drop_empty_columns<T>(cells: &mut Vec<(Block, T)>) {
    let width = cells
        .iter()
        .filter_map(|(block, _)| position(block))
        .map(|(_, c, _, cs)| usize::from(c) + usize::from(cs))
        .max()
        .unwrap_or(0);
    if width < 2 {
        return;
    }
    let empty: Vec<bool> = (0..width)
        .map(|col| {
            cells
                .iter()
                .filter_map(|(block, _)| match block.kind {
                    BlockKind::TableCell { column, .. } if usize::from(column) == col => {
                        Some(block.text.trim().is_empty())
                    }
                    _ => None,
                })
                .all(|blank| blank)
        })
        .collect();
    let survivors: Vec<usize> = (0..width).filter(|col| !empty[*col]).collect();
    if survivors.len() < 2 || survivors.len() == width {
        return;
    }
    let mut rank = vec![0usize; width];
    for (n, col) in survivors.iter().enumerate() {
        rank[*col] = n;
    }
    let mut kept: Vec<(Block, T)> = Vec::with_capacity(cells.len());
    for (mut block, along) in cells.drain(..) {
        let Some((_, c, _, cs)) = position(&block) else {
            kept.push((block, along));
            continue;
        };
        let from = usize::from(c);
        // A cell starting in a removed column is gone: it is blank, because
        // the column is empty -- even where it reaches into a column that
        // stays, whose own cell holds the grid there.
        if empty[from] {
            continue;
        }
        // The surviving columns the cell covers: its own and the ones past
        // each removed column it reaches over, so a span over one loses one.
        // At least one: the cell's own column survives.
        let span = (from..from + usize::from(cs))
            .filter(|col| !empty[*col])
            .count();
        set_column(&mut block, rank[from] as u16);
        set_column_span(&mut block, span as u16);
        kept.push((block, along));
    }
    *cells = kept;
}

fn drop_rows_nothing_starts_in(cells: &mut Vec<Block>, drawn: bool) {
    let Some(last) = cells
        .iter()
        .filter_map(position)
        .map(|(r, _, span, _)| r + span - 1)
        .max()
    else {
        return;
    };
    let starts = |row: u16, cells: &[Block]| {
        cells.iter().any(|b| {
            position(b).is_some_and(|(r, ..)| r == row) && (drawn || !b.text.trim().is_empty())
        })
    };
    // From the bottom, so the numbers above a dropped row stay what they were.
    for row in (0..=last).rev() {
        if starts(row, cells) {
            continue;
        }
        cells.retain(|b| position(b).is_none_or(|(r, ..)| r != row));
        for block in cells.iter_mut() {
            if let Some((r, _, span, _)) = position(block) {
                if r > row {
                    set_rows(block, r - 1, span);
                } else if r + span > row {
                    set_rows(block, r, span - 1);
                }
            }
        }
    }
}

/// Whether the line is a complete accounting figure on its own: an optional
/// currency, digits with `,` and `.`, parentheses around the whole, or a nil
/// dash -- `86,090`, `(4,305)`, `$ 990`, `$ —` (`table_runs_on_figures`,
/// #574).
///
/// A parenthesised pair or range is one figure too -- `(1.22,6.12)`,
/// `(0.27, 7.38)`, `(-4.7, -0.36)`, `(2.14E-04, 1.4)`, `(1.2-3.4)` -- so a
/// statistics cell reads as a figure at all.
///
/// `(` stays a run-on signal for prose: `(a) Includes ...` carries letters,
/// and a parenthesised one-digit number (`(1)`) is a footnote marker, not
/// an amount; two digits (`(15)`) are an amount -- statements carry small
/// adjustments no footnote reaches. A line ending in `,` continues on its
/// own line (`31,`), as does an empty one.
pub(crate) fn is_complete_figure_line(text: &str) -> bool {
    let line = text.trim();
    if line.is_empty() || line.ends_with(',') {
        return false;
    }
    if line.starts_with('(') && line.ends_with(')') && line.len() > 2 {
        if let Some(inner) = line.get(1..line.len() - 1) {
            let inner = inner.trim();
            if !inner.contains(['(', ')']) {
                return is_paren_inner_figure(inner);
            }
        }
    }
    line.split_whitespace().all(is_figure_token)
}

/// Whether the token is one figure: a bare amount, a nil dash, a bare
/// currency glyph beside one (`$` in `$ 990`), or parentheses around one.
fn is_figure_token(token: &str) -> bool {
    if token.is_empty() || token.chars().any(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    if token.starts_with('(') && token.ends_with(')') && token.len() > 2 {
        let Some(inner) = token.get(1..token.len() - 1) else {
            return false;
        };
        let inner = inner.trim();
        if inner.contains(['(', ')']) {
            return false;
        }
        return is_paren_inner_figure(inner);
    }
    is_bare_figure(token)
}

/// Whether the inside of one pair of parentheses is an amount: `(4,305)`,
/// `($1,234)` and `(21%)` are; `(1)` is a footnote marker and `(a)` prose.
///
/// A pair or a range is one amount too -- `(1.22,6.12)`, `(0.27, 7.38)`,
/// `(-4.7, -0.36)`, `(2.14E-04, 1.4)`, `(1.2-3.4)`, `(1.2–3.4)` -- so the
/// confidence interval under an estimate reads as a figure.
fn is_paren_inner_figure(inner: &str) -> bool {
    let rest = inner.trim();
    if rest.is_empty() || rest.contains(['(', ')']) {
        return false;
    }
    if rest
        .chars()
        .any(|c| c.is_ascii_alphabetic() && !matches!(c, 'E' | 'e'))
    {
        return false;
    }
    // A bare one-digit number in parentheses is the footnote marker after a
    // label, not a negative amount; two digits are an amount -- `(15)` under
    // `1,204` is the adjustment, and `(38)` under `98` the blocked PM_2015
    // row, while footnotes past `(9)` do not occur in the sets.
    if rest.len() <= 1 && rest.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    if is_single_paren_amount(rest) {
        return true;
    }
    if rest.contains(',') && is_paren_pair(rest) {
        return true;
    }
    is_paren_range(rest)
}

/// The text without an outer currency glyph or percent sign.
fn strip_affixes_borrowed(text: &str) -> &str {
    let rest = text.trim();
    let rest = rest.trim_start_matches(['$', '€', '£', '¥', '¢', '₹']);
    let rest = rest.trim();
    let rest = rest.trim_end_matches(['%', '‰']);
    rest.trim()
}

/// Whether the text is one amount with optional thousands commas, a sign and
/// an exponent: `4,305`, `30.8`, `-5`, `2.14E-04`. No spaces: a pair with a
/// space is a pair, not a single. No footnote check here: `(1)` is refused
/// by the caller before stripping, and a range part `3` is a number.
fn is_single_paren_amount(text: &str) -> bool {
    let rest = strip_affixes_borrowed(text);
    if rest.is_empty() || rest.contains([' ', '(', ')']) {
        return false;
    }
    if rest
        .chars()
        .any(|c| c.is_ascii_alphabetic() && !matches!(c, 'E' | 'e'))
    {
        return false;
    }
    if !rest.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    if rest.contains(',') {
        if rest.starts_with(',') || rest.ends_with(',') || rest.contains(",,") {
            return false;
        }
        if !rest
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | ',' | 'E' | 'e' | '-' | '+' | '−'))
        {
            return false;
        }
        for part in rest.split(',') {
            let part = part.trim();
            if part.is_empty() || !part.chars().any(|c| c.is_ascii_digit()) {
                return false;
            }
        }
        let dense: String = rest.chars().filter(|c| *c != ',').collect();
        return is_plain_number_core(&dense);
    }
    is_plain_number_core(rest)
}

/// Whether the text is two numbers around one comma: `1.22,6.12`,
/// `0.27, 7.38`, `-4.7, -0.36`, `2.14E-04, 1.4`.
fn is_paren_pair(text: &str) -> bool {
    let rest = strip_affixes_borrowed(text);
    if rest.contains(['(', ')']) {
        return false;
    }
    if rest.chars().filter(|c| *c == ',').count() != 1 {
        return false;
    }
    let Some((left, right)) = rest.split_once(',') else {
        return false;
    };
    let (left, right) = (strip_affixes_borrowed(left), strip_affixes_borrowed(right));
    if left.is_empty() || right.is_empty() {
        return false;
    }
    if left.contains([',', '(', ')', ' ']) || right.contains([',', '(', ')', ' ']) {
        return false;
    }
    if left
        .chars()
        .any(|c| c.is_ascii_alphabetic() && !matches!(c, 'E' | 'e'))
        || right
            .chars()
            .any(|c| c.is_ascii_alphabetic() && !matches!(c, 'E' | 'e'))
    {
        return false;
    }
    is_plain_number_core(left) && is_plain_number_core(right)
}

/// Whether the text is two numbers around a dash: `1.2-3.4`, `1.2–3.4`.
fn is_paren_range(text: &str) -> bool {
    let rest = strip_affixes_borrowed(text);
    if rest.contains(['(', ')']) {
        return false;
    }
    if rest.contains('–') {
        if rest.chars().filter(|c| *c == '–').count() != 1 {
            return false;
        }
        let Some((left, right)) = rest.split_once('–') else {
            return false;
        };
        return is_range_part(left) && is_range_part(right);
    }
    let mut candidates: Vec<usize> = Vec::new();
    let mut previous: Option<char> = None;
    for (at, c) in rest.char_indices() {
        if c == '-' {
            if at == 0 || matches!(previous, Some('E' | 'e')) {
                // A sign, not a range dash.
            } else {
                candidates.push(at);
            }
        }
        previous = Some(c);
    }
    if candidates.len() != 1 {
        return false;
    }
    let (left, right) = rest.split_at(candidates[0]);
    let Some(right) = right.get(1..) else {
        return false;
    };
    if left.trim().is_empty() || right.trim().is_empty() {
        return false;
    }
    is_range_part(left) && is_range_part(right)
}

fn is_range_part(text: &str) -> bool {
    let part = strip_affixes_borrowed(text);
    if part.is_empty() || part.contains([' ', '(', ')']) {
        return false;
    }
    if part.contains(',') {
        return is_single_paren_amount(part);
    }
    if part
        .chars()
        .any(|c| c.is_ascii_alphabetic() && !matches!(c, 'E' | 'e'))
    {
        return false;
    }
    is_plain_number_core(part)
}

/// Whether the text is one number: an optional sign, digits with at most one
/// `.`, an optional `E` exponent. No commas, no spaces, no affixes.
fn is_plain_number_core(text: &str) -> bool {
    let rest = text.trim();
    if rest.is_empty() || rest.contains([' ', ',', '(', ')']) {
        return false;
    }
    if !rest.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    if !rest
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'E' | 'e' | '-' | '+' | '−'))
    {
        return false;
    }
    let rest = rest.strip_prefix(['-', '+', '−']).unwrap_or(rest);
    if rest.is_empty() {
        return false;
    }
    let (base, exponent) = match rest.find(['E', 'e']) {
        None => (rest, None),
        Some(at) => {
            let (base, tail) = rest.split_at(at);
            let Some(exponent) = tail.get(1..) else {
                return false;
            };
            if base.is_empty() || exponent.is_empty() {
                return false;
            }
            (base, Some(exponent))
        }
    };
    if !is_decimal_base(base) {
        return false;
    }
    if let Some(exponent) = exponent {
        let exponent = exponent.strip_prefix(['-', '+', '−']).unwrap_or(exponent);
        if exponent.is_empty() || !exponent.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
    }
    true
}

fn is_decimal_base(text: &str) -> bool {
    if text.is_empty() || !text.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    if text.chars().filter(|c| *c == '.').count() > 1 {
        return false;
    }
    text.chars().all(|c| c.is_ascii_digit() || c == '.')
}

/// Whether the token is a bare amount or a nil dash, with an optional
/// currency before it and a percent after: `86,090`, `$ 990`'s `990`, `—`,
/// `-5`, `12%`.
fn is_bare_figure(token: &str) -> bool {
    let rest = token.trim_start_matches(['$', '€', '£', '¥', '¢', '₹']);
    if rest.is_empty() {
        // A bare currency glyph: an amount's half, as in `$ 990` or `$ —`.
        return true;
    }
    let rest = rest.trim_end_matches(['%', '‰']);
    if rest.is_empty() {
        return true;
    }
    if rest.chars().all(|c| matches!(c, '-' | '–' | '—')) {
        return true;
    }
    let rest = rest.trim_start_matches(['-', '+', '−']);
    !rest.is_empty()
        && rest.chars().any(|c| c.is_ascii_digit())
        && rest
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == ',')
}

/// Whether `below` reads as the rest of `above`.
fn runs_on(above: &str, below: &str, figures: bool) -> bool {
    if figures && is_complete_figure_line(above) && is_complete_figure_line(below) {
        return false;
    }
    let (above, below) = (above.trim(), below.trim());
    let Some(first) = below.chars().next() else {
        return false;
    };
    if above.is_empty() {
        return false;
    }
    if first.is_lowercase() || "(,;&/-".contains(first) {
        return true;
    }
    above.ends_with(['-', ',', '(', '/', '&'])
        || above
            .rsplit(char::is_whitespace)
            .next()
            .is_some_and(|word| RUNS_ON.contains(&word))
}

/// Whether the token is one parenthesised figure: `(4,305)`, `(30.8)`,
/// `(1.22,6.12)`. A bare figure, a nil dash or a currency figure is not.
fn is_parenthesised_figure_token(token: &str) -> bool {
    let token = token.trim();
    if token.len() <= 2 || !token.starts_with('(') || !token.ends_with(')') {
        return false;
    }
    let Some(inner) = token.get(1..token.len() - 1) else {
        return false;
    };
    let inner = inner.trim();
    if inner.contains(['(', ')']) {
        return false;
    }
    is_paren_inner_figure(inner)
}

/// How many digits a bracketed integer may hold and still read as a spread
/// without a decimal point: `(15)` under `12.4` is a spread, `(102)` under
/// `38` is the PM_2015 adjustment in millions. Two, not three: three-digit
/// integers under plain uppers are amounts, and the bar holds PM_2015 freed.
const MAX_SPREAD_INTEGER_DIGITS: usize = 2;

/// Whether the text carries a thousands separator or a currency glyph: `,`
/// or `$ € £ ¥ ¢ ₹`. A spread has neither; an amount has one of them, on
/// either line of the pair.
fn has_separator_or_currency(text: &str) -> bool {
    text.contains(',') || text.contains(['$', '€', '£', '¥', '¢', '₹'])
}

/// Whether the bracketed lower token is a spread under its upper: a pair or
/// range always (`(1.22,6.12)`, `(1.2-3.4)`), a single only when neither line
/// carries a separator or a currency glyph and the single holds a decimal
/// point or at most [`MAX_SPREAD_INTEGER_DIGITS`] digits. `12.4` over `(3.1)`
/// is a spread; `1,204` over `(15)` is not, nor `$ 990` over `(12)`, nor
/// `86,090` over `(4,305)`.
///
/// A thousands comma reads single first: `(4,305)` matches the pair shape
/// (`4`, `305`) but `is_single_paren_amount` claims it, so it falls under
/// the single rule and its comma refuses it.
fn is_spread_token(lower_token: &str, upper_token: &str) -> bool {
    let token = lower_token.trim();
    if token.len() <= 2 || !token.starts_with('(') || !token.ends_with(')') {
        return false;
    }
    let Some(inner) = token.get(1..token.len() - 1).map(str::trim) else {
        return false;
    };
    if inner.is_empty() || inner.contains(['(', ')']) {
        return false;
    }
    if is_single_paren_amount(inner) {
        if has_separator_or_currency(inner) {
            return false;
        }
        let digits = inner.chars().filter(|c| c.is_ascii_digit()).count();
        if !inner.contains('.') && digits > MAX_SPREAD_INTEGER_DIGITS {
            return false;
        }
        return !has_separator_or_currency(upper_token);
    }
    if inner.contains(',') && is_paren_pair(inner) {
        return true;
    }
    is_paren_range(inner)
}

/// The lower line's figure tokens as spread flags under the upper line, or
/// `None` when the pair holds no figure at all. A whole parenthesised line
/// -- including a pair with a space, which whitespace splitting would tear
/// apart -- is one spread candidate under the whole upper line; otherwise
/// each lower token pairs with the upper token above it where the counts
/// match, else with the whole upper line. A bare lower token -- a plain
/// figure, a nil dash, a currency figure -- is no spread.
fn lower_spread_flags(upper: &str, lower: &str) -> Option<Vec<bool>> {
    let (upper_line, lower_line) = (upper.trim(), lower.trim());
    if upper_line.is_empty() || lower_line.is_empty() {
        return None;
    }
    if lower_line.starts_with('(') && lower_line.ends_with(')') && lower_line.len() > 2 {
        if let Some(inner) = lower_line.get(1..lower_line.len() - 1) {
            let inner = inner.trim();
            if !inner.contains(['(', ')']) && is_paren_inner_figure(inner) {
                if is_single_paren_amount(inner) {
                    if has_separator_or_currency(inner) {
                        return Some(vec![false]);
                    }
                    let digits = inner.chars().filter(|c| c.is_ascii_digit()).count();
                    if !inner.contains('.') && digits > MAX_SPREAD_INTEGER_DIGITS {
                        return Some(vec![false]);
                    }
                    return Some(vec![!has_separator_or_currency(upper_line)]);
                }
                return Some(vec![true]);
            }
        }
    }
    if !is_complete_figure_line(upper_line) || !is_complete_figure_line(lower_line) {
        return None;
    }
    let lowers: Vec<&str> = lower_line.split_whitespace().collect();
    let uppers: Vec<&str> = upper_line.split_whitespace().collect();
    let pairwise = lowers.len() == uppers.len();
    Some(
        lowers
            .iter()
            .enumerate()
            .map(|(i, token)| {
                if !is_parenthesised_figure_token(token) {
                    return false;
                }
                let over = if pairwise { uppers[i] } else { upper_line };
                is_spread_token(token, over)
            })
            .collect(),
    )
}

/// Whether the fused row's figure-over-figure pairs are a statistics stack:
/// every lower-line figure token a spread under its upper, as a mean over
/// its `(SD)` or an estimate over its `(95% CI)`. An accounting row carries
/// an amount below -- a separator, a currency glyph, or three digits without
/// a decimal -- so at least one token reads amount and the row still splits.
fn is_statistics_stack(cell_texts: &[Vec<String>]) -> bool {
    let mut flags: Vec<bool> = Vec::new();
    for texts in cell_texts {
        for pair in texts.windows(2) {
            if is_complete_figure_line(&pair[0]) && is_complete_figure_line(&pair[1]) {
                if let Some(lower) = lower_spread_flags(&pair[0], &pair[1]) {
                    flags.extend(lower);
                }
            }
        }
    }
    !flags.is_empty() && flags.iter().all(|spread| *spread)
}

/// Whether the row's stub cell holds stacked labels: two or more lines that
/// each start a fresh label -- an upper-case letter or a label marker
/// (`(a)`, `-`, `1.`) -- so the fused row is two rows with two labels, not
/// one wrapped label over a statistics stack (#574).
fn stub_holds_stacked_labels(cells: &[Block], row: u16) -> bool {
    let Some(stub) = cells
        .iter()
        .find(|b| position(b).is_some_and(|(r, c, _, _)| r == row && c == 0))
    else {
        return false;
    };
    if !has_text(stub) {
        return false;
    }
    let bands = line_bands(stub);
    bands.len() >= 2 && bands.iter().all(|band| starts_labelled(&line_text(band)))
}

/// Whether the line starts a fresh label: an upper-case letter or a label
/// marker -- `(a)`, `-`, `1.` -- rather than a wrapped continuation. `(a)`
/// is one parenthesised letter or digit: a figure (`(456)`) or a wrapped
/// word (`(Gains)`) is no marker.
fn starts_labelled(line: &str) -> bool {
    let line = line.trim_start();
    let Some(first) = line.chars().next() else {
        return false;
    };
    if first.is_uppercase() || first == '-' {
        return true;
    }
    if first == '(' {
        let mut inner = line[1..].chars();
        return matches!(inner.next(), Some(c) if c.is_ascii_alphanumeric())
            && matches!(inner.next(), Some(')'));
    }
    first.is_ascii_digit() && line[1..].starts_with('.')
}

/// Whether the row would stay whole without B's veto: at least one of its
/// lines reads as run-on under today's rule.
fn split_is_on_figures(cell_texts: &[Vec<String>]) -> bool {
    cell_texts.iter().any(|texts| {
        texts
            .windows(2)
            .any(|pair| runs_on(&pair[0], &pair[1], false))
    })
}

/// Whether the row pair under the join is a statistics stack: every
/// lower-row figure over a figure above reads as a spread under its upper.
fn join_is_statistics_stack(
    cells: &[Block],
    below: &[Option<usize>],
    above: &[Option<usize>],
) -> bool {
    let mut seen: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
    let mut flags: Vec<bool> = Vec::new();
    for &slot in below {
        let Some(from) = slot else { continue };
        if cells[from].text.trim().is_empty() || !seen.insert(from) {
            continue;
        }
        let (_, column, _, span) = position(&cells[from]).unwrap_or((0, 0, 1, 1));
        let at = usize::from(column);
        let over = usize::from(span);
        if at + over > above.len() {
            continue;
        }
        let over = &above[at..at + over];
        let [Some(into), ..] = over else { continue };
        let into = *into;
        if !over.iter().all(|&slot| slot == Some(into)) {
            continue;
        }
        let (upper, lower) = (cells[into].text.trim(), cells[from].text.trim());
        if upper.is_empty() || lower.is_empty() {
            continue;
        }
        if is_complete_figure_line(upper) && is_complete_figure_line(lower) {
            if let Some(spread) = lower_spread_flags(upper, lower) {
                flags.extend(spread);
            }
        }
    }
    !flags.is_empty() && flags.iter().all(|spread| *spread)
}

/// Whether `below` joins `above` when the row pair may be a statistics stack:
/// B's veto does not fire for a stack, which joins as today.
fn join_runs_on(above: &str, below: &str, figures: bool, statistics: bool) -> bool {
    if figures && statistics && is_complete_figure_line(above) && is_complete_figure_line(below) {
        return runs_on(above, below, false);
    }
    runs_on(above, below, figures)
}

/// One table's run-on rows joined to the rows above them, in row-major order.
/// For a grid whose rows are the page's own -- the builder's, or lines the
/// page drew -- where no row is empty for the decoder's reasons.
pub(crate) fn join_run_on_rows(cells: &mut Vec<Block>, runs_on_figures: bool) {
    split_stacked_rows(cells, runs_on_figures);
    join_rows_that_run_on(cells, runs_on_figures);
    span_section_rows(cells);
    in_row_major_order(cells);
}

/// Rows the grid stacked into one, split back into one row per line.
///
/// Two or more table rows set close together come out as ONE row whose every
/// cell holds the same number of lines at the same heights (`WFNS-3 WFNS-4 |
/// 4 (9%) 9 (21%)`), the second-largest TEDS loss on good PubTables regions
/// after this file's joins. Each such row becomes one row per line; a row
/// whose lines read as run-on is one wrapped value and stays whole.
///
/// Split BEFORE joining, so a row that is split is never joined back.
fn split_stacked_rows(cells: &mut Vec<Block>, runs_on_figures: bool) {
    let rows: std::collections::BTreeSet<u16> = cells
        .iter()
        .filter_map(|b| position(b).map(|(r, _, _, _)| r))
        .collect();
    let mut splits: Vec<(u16, usize)> = Vec::new();
    for row in rows {
        if let Some(lines) = stacked_lines(cells, row, runs_on_figures) {
            splits.push((row, lines));
        }
    }
    // From the bottom, so the numbers above a split row stay what they were.
    for (row, lines) in splits.into_iter().rev() {
        split_row(cells, row, lines);
    }
    in_row_major_order(cells);
}

/// How many rows `row` is, when every cell with text in it holds that many
/// lines at the same heights -- or `None` when the row is one row.
fn stacked_lines(cells: &[Block], row: u16, runs_on_figures: bool) -> Option<usize> {
    let mut lines: Option<usize> = None;
    let mut bands: Vec<Vec<Vec<crate::text::TextSpan>>> = Vec::new();
    let mut filled = 0usize;
    for block in cells.iter() {
        let Some((r, _, span, _)) = position(block) else {
            continue;
        };
        if r != row {
            continue;
        }
        if span != 1 {
            return None;
        }
        if !has_text(block) {
            continue;
        }
        filled += 1;
        let bands_of = line_bands(block);
        if bands_of.len() < 2 || lines.is_some_and(|k| bands_of.len() != k) {
            return None;
        }
        lines = Some(bands_of.len());
        bands.push(bands_of);
    }
    let lines = lines.filter(|_| filled >= 2)?;
    // The lines of one row set across columns: the i-th bands all stand at
    // the same heights.
    for i in 0..lines {
        let (mut low, mut high) = band_extent(&bands[0][i]);
        for cell in &bands[1..] {
            let (bottom, top) = band_extent(&cell[i]);
            low = low.max(bottom);
            high = high.min(top);
        }
        if low >= high {
            return None;
        }
    }
    // But lines that read as run-on are one wrapped value, not rows.
    let cell_texts: Vec<Vec<String>> = bands
        .iter()
        .map(|cell| cell.iter().map(|band| line_text(band)).collect())
        .collect();
    for texts in &cell_texts {
        if texts
            .windows(2)
            .any(|pair| runs_on(&pair[0], &pair[1], runs_on_figures))
        {
            return None;
        }
    }
    // A statistics stack is one value with its spread, not rows: B leaves a
    // row whose every lower-line figure reads as a spread fused -- unless
    // the stub holds stacked labels, which split regardless (#574).
    if runs_on_figures
        && split_is_on_figures(&cell_texts)
        && is_statistics_stack(&cell_texts)
        && !stub_holds_stacked_labels(cells, row)
    {
        return None;
    }
    Some(lines)
}

/// Whether the cell holds anything to stand on a line.
fn has_text(block: &Block) -> bool {
    !block.text.trim().is_empty() || block.spans.iter().any(|s| !s.text.trim().is_empty())
}

/// The cell's lines: its spans with non-blank text, grouped by vertical
/// position, top to bottom. Two spans are on one line when their centres
/// (`y + height / 2`) differ by less than half the smaller height, each span
/// measured against its band's first as a line of prose is.
fn line_bands(block: &Block) -> Vec<Vec<crate::text::TextSpan>> {
    let centre = |span: &crate::text::TextSpan| span.y + span.height / 2.0;
    let mut spans: Vec<&crate::text::TextSpan> = block
        .spans
        .iter()
        .filter(|s| !s.text.trim().is_empty())
        .collect();
    spans.sort_by(|a, b| {
        centre(b)
            .partial_cmp(&centre(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut bands: Vec<Vec<crate::text::TextSpan>> = Vec::new();
    for span in spans {
        let same = bands
            .last()
            .is_some_and(|band: &Vec<crate::text::TextSpan>| {
                let first = &band[0];
                (centre(first) - centre(span)).abs() < first.height.min(span.height) / 2.0
            });
        match bands.last_mut() {
            Some(band) if same => band.push(span.clone()),
            _ => bands.push(vec![span.clone()]),
        }
    }
    for band in &mut bands {
        band.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal));
    }
    bands
}

/// The vertical extent of one band, bottom and top.
fn band_extent(band: &[crate::text::TextSpan]) -> (f64, f64) {
    let mut extent = (f64::INFINITY, f64::NEG_INFINITY);
    for span in band {
        extent.0 = extent.0.min(span.y);
        extent.1 = extent.1.max(span.y + span.height);
    }
    extent
}

/// One band's spans joined in x order, as the cell's text is built.
fn line_text(band: &[crate::text::TextSpan]) -> String {
    TextBlock {
        spans: band.to_vec(),
    }
    .text()
}

/// Row `row`, which [`stacked_lines`] counted `lines` tall, as one row per
/// line: the rows below shift down, a cell reaching over it from above grows,
/// and each of its cells becomes one cell per line.
fn split_row(cells: &mut Vec<Block>, row: u16, lines: usize) {
    let below = lines as u16 - 1;
    let mut split: Vec<Block> = Vec::with_capacity(cells.len() + below as usize);
    for block in std::mem::take(cells) {
        let Some((r, _, span, _)) = position(&block) else {
            split.push(block);
            continue;
        };
        if r > row {
            let mut moved = block;
            set_rows(&mut moved, r + below, span);
            split.push(moved);
        } else if r < row && r + span > row {
            let mut grown = block;
            set_rows(&mut grown, r, span + below);
            split.push(grown);
        } else if r == row && span == 1 && has_text(&block) {
            let bands = line_bands(&block);
            if bands.len() != lines {
                split.push(block);
                continue;
            }
            for (i, band) in bands.into_iter().enumerate() {
                split.push(piece(&block, row + i as u16, band));
            }
        } else {
            split.push(block);
        }
    }
    *cells = split;
}

/// One line of `of` as a cell of its own row: that line's spans and text, the
/// y of their union under the original x extent, the header kept.
fn piece(of: &Block, row: u16, band: Vec<crate::text::TextSpan>) -> Block {
    let mut piece = of.clone();
    set_rows(&mut piece, row, 1);
    piece.text = line_text(&band);
    let (bottom, top) = band_extent(&band);
    piece.bbox = [of.bbox[0], bottom, of.bbox[2], top];
    piece.spans = band;
    piece
}

/// Whether `row` is a header band: a header row -- a cell of it is marked --
/// whose only cell with text spans two or more columns. Positions with no
/// cell count as empty, as in the join. A body row keeps the join exactly as
/// today, however wide its cells.
fn is_header_band(cells: &[Block], row: u16) -> bool {
    let mut marked = false;
    let mut written: Option<u16> = None;
    for block in cells {
        let Some((r, _, _, column_span)) = position(block) else {
            continue;
        };
        if r != row {
            continue;
        }
        if matches!(block.kind, BlockKind::TableCell { header: true, .. }) {
            marked = true;
        }
        if block.text.trim().is_empty() {
            continue;
        }
        if written.is_some() {
            return false;
        }
        written = Some(column_span);
    }
    marked && written.is_some_and(|span| span >= 2)
}

/// How far below the row above a wrapped continuation may sit, in median
/// line pitches of its table (#574).
///
/// A wrap is set at the line's own leading and a new row of a table with
/// blank stubs at row spacing, so the lower row's top must lie within this
/// many pitches of the row above's bottom. From {1.0, 1.2, 1.5}, chosen on
/// the tuning sets only (proxy tuning half, FinTabNet dev, PubTables VAL
/// dev-300), confirmed with 0 sets down.
const WRAPPED_ROW_GAP_FACTOR: f64 = 1.2;

/// How many continuation rows one row may take (#574).
///
/// Without it chains of joins collapse a table (PMC4875172: 13 joins,
/// -0.635 TEDS in the HTML simulation). From {1, 2, 3}, chosen on the
/// tuning sets only, confirmed with 0 sets down.
const MAX_WRAP_CONTINUATIONS: usize = 2;

fn join_rows_that_run_on(cells: &mut Vec<Block>, runs_on_figures: bool) {
    join_rows_that_run_on_lexically(cells, runs_on_figures);
    join_wrapped_rows(cells);
}

/// One table's run-on rows joined to the rows above them, on the words alone.
///
/// Unchanged by the wrapped-row pass below it: that pass runs after this one
/// and only adds joins, so every join here fires on the grid it always saw.
fn join_rows_that_run_on_lexically(cells: &mut Vec<Block>, runs_on_figures: bool) {
    let Some(rows) = cells
        .iter()
        .filter_map(position)
        .map(|(r, _, span, _)| r + span)
        .max()
    else {
        return;
    };
    let width = cells
        .iter()
        .filter_map(position)
        .map(|(_, c, _, span)| usize::from(c + span))
        .max()
        .unwrap_or(0);
    let mut row = rows;
    while row > 1 {
        row -= 1;
        // Both rows one row tall: the cell over each column, a cell across
        // columns over each of them. A column with no cell in the row is an
        // empty one -- a grid does not always make a cell where nothing was
        // written.
        let slots = |r: u16| -> Option<Vec<Option<usize>>> {
            let mut at: Vec<Option<usize>> = vec![None; width];
            for (k, b) in cells.iter().enumerate() {
                let Some((cr, c, rs, cs)) = position(b) else {
                    continue;
                };
                if cr <= r && r < cr + rs {
                    if rs != 1 {
                        return None;
                    }
                    for slot in &mut at[usize::from(c)..usize::from(c + cs)] {
                        *slot = Some(k);
                    }
                }
            }
            Some(at)
        };
        let (Some(below), Some(above)) = (slots(row), slots(row - 1)) else {
            continue;
        };
        // A HEADER BAND never takes the row under it: a label set across the
        // columns is a head, and the line under it is the next level of the
        // head -- years, units -- not a continuation of the label (#574).
        if is_header_band(cells, row - 1) {
            continue;
        }
        let empty = |k: Option<usize>| k.is_none_or(|k| cells[k].text.trim().is_empty());
        if !below.iter().any(|&k| empty(k)) {
            continue;
        }
        // A statistics stack joins as today: B's veto does not fire when
        // every lower-row figure over a figure reads as a spread (#574).
        let statistics = runs_on_figures && join_is_statistics_stack(cells, &below, &above);
        // Each written cell of the lower row, and the one cell over all its
        // columns that it runs on from.
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        let mut joins = true;
        for &k in &below {
            let Some(from) = k else { continue };
            if empty(k) || pairs.iter().any(|&(f, _)| f == from) {
                continue;
            }
            let (_, c, _, cs) = position(&cells[from]).unwrap_or((0, 0, 1, 1));
            let over: Vec<Option<usize>> = (usize::from(c)..usize::from(c + cs))
                .map(|i| above[i])
                .collect();
            match over[..] {
                [Some(into), ..]
                    if over.iter().all(|&o| o == Some(into))
                        && join_runs_on(
                            &cells[into].text,
                            &cells[from].text,
                            runs_on_figures,
                            statistics,
                        ) =>
                {
                    pairs.push((from, into));
                }
                _ => {
                    joins = false;
                    break;
                }
            }
        }
        if !joins || pairs.is_empty() {
            continue;
        }
        for &(from, into) in &pairs {
            let tail = cells[from].text.trim().to_string();
            let spans = std::mem::take(&mut cells[from].spans);
            let bbox = cells[from].bbox;
            let target = &mut cells[into];
            let head = target.text.trim_end();
            target.text = if head.ends_with('-') {
                format!("{head}{tail}")
            } else {
                format!("{head} {tail}")
            };
            target.spans.extend(spans);
            target.bbox = [
                target.bbox[0].min(bbox[0]),
                target.bbox[1].min(bbox[1]),
                target.bbox[2].max(bbox[2]),
                target.bbox[3].max(bbox[3]),
            ];
        }
        let gone: std::collections::BTreeSet<usize> = below.into_iter().flatten().collect();
        let mut k = 0;
        cells.retain(|_| {
            k += 1;
            !gone.contains(&(k - 1))
        });
        for block in cells.iter_mut() {
            if let Some((r, _, span, _)) = position(block) {
                if r > row {
                    set_rows(block, r - 1, span);
                }
            }
        }
    }
}

/// A wrapped cell's continuation joined to its row, with geometry and a cap.
///
/// The lexical pass only fires when EVERY written cell of the lower row runs
/// on in words, so one continuation starting uppercase blocks the row, and a
/// continuation under the wrong column cannot join. A lower row joins the row
/// above when ALL of:
/// (a) its first-column cell is empty, the row above's is not, and the row
///     above is no header row: flagged, or the grid's first row, which holds
///     the header by position where the decoder marked none (a stub-empty
///     sub-header row otherwise joins the head it sits under);
/// (b) none of its written cells is a figure;
/// (c) its top lies within [`WRAPPED_ROW_GAP_FACTOR`] median line pitches of
///     the row above's bottom;
/// (d) the row above has taken fewer than [`MAX_WRAP_CONTINUATIONS`].
/// Each joined cell's text is appended with one space, column by column; a
/// lower-row cell under no written cell of the upper row stays a new row.
///
/// (e) A complete lower row joins only onto fragmentary heads: where more
/// than half of its data cells hold text, each paired cell above must hold
/// an unbalanced opener, or the row stays. A majority-filled row of
/// independent values -- grouped stubs, estimate plus CI -- is a data row,
/// not a continuation; an open paren licenses the join. Tuning-found, past
/// the spec's (a)-(d): without it grouped rows collapse on PubTables dev.
/// (A run-on exception for mixed rows reintroduced unit-driven group joins
/// -- lowercase `mm` runs on -- and measured worse.)
///
/// TOP-DOWN, staying on the row after a join: the next row shifts into it, so
/// a stub row absorbs its continuations in order up to the cap. A bottom-up
/// pass could take only the row directly under a stub row, and the cap would
/// never bind.
///
/// The cap is read off the row, not counted: a join appends the continuation's
/// spans to the cell above, so a row that took continuations holds that many
/// extra lines. A counter would reset every time this pass runs -- the learned
/// rebuild tidies and then the edge peel joins again -- and the cap would let
/// through twice what it says.
fn join_wrapped_rows(cells: &mut Vec<Block>) {
    let pitch = median_line_pitch(cells);
    let width = cells
        .iter()
        .filter_map(position)
        .map(|(_, c, _, span)| usize::from(c + span))
        .max()
        .unwrap_or(0);
    let Some(mut end) = cells
        .iter()
        .filter_map(position)
        .map(|(r, _, span, _)| r + span)
        .max()
    else {
        return;
    };
    let mut row: u16 = 1;
    while row < end {
        let slots = |r: u16| -> Option<Vec<Option<usize>>> {
            let mut at: Vec<Option<usize>> = vec![None; width];
            for (k, b) in cells.iter().enumerate() {
                let Some((cr, c, rs, cs)) = position(b) else {
                    continue;
                };
                if cr <= r && r < cr + rs {
                    if rs != 1 {
                        return None;
                    }
                    for slot in &mut at[usize::from(c)..usize::from(c + cs)] {
                        *slot = Some(k);
                    }
                }
            }
            Some(at)
        };
        let (Some(below), Some(above)) = (slots(row), slots(row - 1)) else {
            row += 1;
            continue;
        };
        let empty = |k: Option<usize>| k.is_none_or(|k| cells[k].text.trim().is_empty());
        // (a) A continuation has no stub; what it continues does, and is no
        // header row. A missing first-column cell is an empty one, as above.
        // The header by position counts too: the decoder marks few heads, and
        // a stub-empty sub-header row sits directly under the grid's first.
        let stub_below_empty = width == 0 || empty(below[0]);
        let stub_above_filled =
            width > 0 && above[0].is_some_and(|k| !cells[k].text.trim().is_empty());
        if !stub_below_empty || !stub_above_filled || row == 1 || is_header_row(cells, row - 1) {
            row += 1;
            continue;
        }
        // Each written cell under one written cell over all its columns, and
        // (b) none of them a figure. Under no cell -- or no written one -- the
        // row stays a new row: no cross-column join.
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        let mut joins = true;
        for &k in &below {
            let Some(from) = k else { continue };
            if empty(k) || pairs.iter().any(|&(f, _)| f == from) {
                continue;
            }
            if crate::markdown::is_figure(&cells[from].text)
                || is_complete_figure_line(&cells[from].text)
            {
                joins = false;
                break;
            }
            let (_, c, _, cs) = position(&cells[from]).unwrap_or((0, 0, 1, 1));
            let over: Vec<Option<usize>> = (usize::from(c)..usize::from(c + cs))
                .map(|i| above[i])
                .collect();
            match over[..] {
                [Some(into), ..]
                    if over.iter().all(|&o| o == Some(into))
                        && !cells[into].text.trim().is_empty() =>
                {
                    pairs.push((from, into));
                }
                _ => {
                    joins = false;
                    break;
                }
            }
        }
        if !joins || pairs.is_empty() {
            row += 1;
            continue;
        }
        // (e) A complete lower row joins only onto fragmentary heads: more
        // than half of its data cells filled, so a grouped row missing
        // values still counts.
        let filled = (1..width)
            .filter(|&i| below[i].is_some_and(|k| !cells[k].text.trim().is_empty()))
            .count();
        let complete = width > 1 && filled * 2 > width - 1;
        if complete
            && pairs
                .iter()
                .any(|&(_, into)| !has_unbalanced_opener(&cells[into].text))
        {
            row += 1;
            continue;
        }
        // (c) A wrap sits at the line's own leading; a new row of a table
        // with blank stubs at row spacing. Overlapping rows are no stack.
        let gap = row_bottom(cells, row - 1)
            .zip(row_top(cells, row))
            .map(|(bottom, top)| bottom - top);
        let near = gap.is_some_and(|gap| {
            0.0 <= gap && pitch.is_some_and(|pitch| gap <= WRAPPED_ROW_GAP_FACTOR * pitch)
        });
        // (d) A row takes at most K continuation rows: past K + 1 lines in
        // any of its written cells it takes no more.
        let capped = row_lines(cells, row - 1) > MAX_WRAP_CONTINUATIONS;
        if !near || capped {
            row += 1;
            continue;
        }
        for &(from, into) in &pairs {
            let tail = cells[from].text.trim().to_string();
            let spans = std::mem::take(&mut cells[from].spans);
            let bbox = cells[from].bbox;
            let target = &mut cells[into];
            target.text = format!("{} {tail}", target.text.trim_end());
            target.spans.extend(spans);
            target.bbox = [
                target.bbox[0].min(bbox[0]),
                target.bbox[1].min(bbox[1]),
                target.bbox[2].max(bbox[2]),
                target.bbox[3].max(bbox[3]),
            ];
        }
        let gone: std::collections::BTreeSet<usize> = below.into_iter().flatten().collect();
        let mut k = 0;
        cells.retain(|_| {
            k += 1;
            !gone.contains(&(k - 1))
        });
        for block in cells.iter_mut() {
            if let Some((r, _, span, _)) = position(block) {
                if r > row {
                    set_rows(block, r - 1, span);
                }
            }
        }
        end -= 1;
    }
}

/// Whether the text holds an unbalanced opener: more `(` `[` `{` than their
/// closers. A head broken mid-phrase -- `SCAN (Schedule for Clinical` --
/// licenses a complete row below to join it; balanced heads do not.
fn has_unbalanced_opener(text: &str) -> bool {
    let (mut paren, mut bracket, mut brace) = (0i32, 0i32, 0i32);
    for c in text.chars() {
        match c {
            '(' => paren += 1,
            ')' => paren -= 1,
            '[' => bracket += 1,
            ']' => bracket -= 1,
            '{' => brace += 1,
            '}' => brace -= 1,
            _ => {}
        }
    }
    paren > 0 || bracket > 0 || brace > 0
}

/// Whether any cell of `row` is marked a header cell.
fn is_header_row(cells: &[Block], row: u16) -> bool {
    cells.iter().any(|b| {
        matches!(b.kind, BlockKind::TableCell { header: true, .. })
            && position(b).is_some_and(|(r, _, _, _)| r == row)
    })
}

/// The top of `row`'s ink: the highest top of its written cells, or `None`
/// where the row holds no text.
fn row_top(cells: &[Block], row: u16) -> Option<f64> {
    cells
        .iter()
        .filter(|b| position(b).is_some_and(|(r, _, _, _)| r == row) && !b.text.trim().is_empty())
        .map(|b| b.bbox[3])
        .reduce(f64::max)
}

/// The bottom of `row`'s ink: the lowest bottom of its written cells, or
/// `None` where the row holds no text.
fn row_bottom(cells: &[Block], row: u16) -> Option<f64> {
    cells
        .iter()
        .filter(|b| position(b).is_some_and(|(r, _, _, _)| r == row) && !b.text.trim().is_empty())
        .map(|b| b.bbox[1])
        .reduce(f64::min)
}

/// How many lines `row`'s fullest written cell holds: its spans in bands,
/// one where the cell holds text and no spans to band.
///
/// A join appends the continuation's spans to the cell above, so this is one
/// plus the continuations the row took -- what the cap reads.
fn row_lines(cells: &[Block], row: u16) -> usize {
    cells
        .iter()
        .filter(|b| position(b).is_some_and(|(r, _, _, _)| r == row) && !b.text.trim().is_empty())
        .map(|b| line_bands(b).len().max(1))
        .max()
        .unwrap_or(0)
}

/// The table's median line pitch: the median top-to-top step between the rows
/// that hold text, distinct tops to a tenth of a point, as
/// `pair_features.page_stats` measures it for the decoder.
///
/// `None` where fewer than two rows hold text: with no rhythm measured the
/// geometry guard fails closed and no wrapped row joins.
fn median_line_pitch(cells: &[Block]) -> Option<f64> {
    let mut tops: Vec<f64> = cells
        .iter()
        .filter_map(|b| position(b).map(|(r, _, _, _)| r))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .filter_map(|r| row_top(cells, r))
        .map(|top| (top * 10.0).round() / 10.0)
        .collect();
    tops.sort_by(f64::total_cmp);
    tops.dedup();
    let mut steps: Vec<f64> = tops.windows(2).map(|pair| pair[1] - pair[0]).collect();
    steps.retain(|step| *step > 0.0);
    steps.sort_by(f64::total_cmp);
    steps
        .get(steps.len() / 2)
        .copied()
        .filter(|pitch| *pitch > 0.0)
}

/// Stacked figure rows fused back into one row, with B off (#574).
///
/// The decoder splits DLR p61's Contributions/Distributions pairs: on joined
/// words the row-boundary model votes boundary (p 0.185 over a 0.1 floor)
/// where the spec's fragments voted none (p 0.095). So with B off the tidy
/// puts the pairs back together here, and B keeps the decoder's split --
/// the same two readings the switch always named, fused off and split on.
///
/// A pair is two adjacent rows over the same columns, each cell one row tall
/// and one column wide: both stubs one label run out on leader dots
/// (`Contributions.....`, never a bare `Control` or a wrapped
/// `Net loss ... Other ...`), every other column a complete figure over a
/// complete figure. Wrapped continuations fail the stub (`translation ...`
/// is lowercase, `Balance at ...` is several words), sparse rows fail the
/// column sets, and a statistics stack stays split -- every lower figure
/// a spread under its upper -- by the same guard that holds the split side.
/// The fused text is upper plus lower with one space, so the halves run back
/// together to the cell the off run emitted.
///
/// AFTER the run-on join, which only joins rows with an empty column -- a
/// full figure pair is never its business -- and before the section spans.
fn join_stacked_figure_rows(cells: &mut Vec<Block>) {
    loop {
        let rows: Vec<u16> = cells
            .iter()
            .filter_map(|b| position(b).map(|(r, _, _, _)| r))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut fused = false;
        for pair in rows.windows(2) {
            let (up, down) = (pair[0], pair[1]);
            if down == up + 1 && stacked_figure_pair(cells, up, down) {
                fuse_figure_rows(cells, up, down);
                fused = true;
                break;
            }
        }
        if !fused {
            break;
        }
    }
}

/// Whether rows `up` and `down` are a stacked-figure pair: adjacent rows over
/// the same 1x1 columns, one label apiece in the stub, figures over figures
/// past it, and no statistics stack.
fn stacked_figure_pair(cells: &[Block], up: u16, down: u16) -> bool {
    let slots = |row: u16| -> Option<std::collections::BTreeMap<u16, usize>> {
        let mut at = std::collections::BTreeMap::new();
        for (k, block) in cells.iter().enumerate() {
            let Some((r, column, row_span, column_span)) = position(block) else {
                continue;
            };
            if r != row {
                continue;
            }
            if row_span != 1 || column_span != 1 || at.insert(column, k).is_some() {
                return None;
            }
        }
        Some(at)
    };
    let (Some(above), Some(below)) = (slots(up), slots(down)) else {
        return false;
    };
    if above.len() < 2
        || above.len() != below.len()
        || above.keys().collect::<Vec<_>>() != below.keys().collect::<Vec<_>>()
    {
        return false;
    }
    let (Some(&stub_up), Some(&stub_down)) = (above.get(&0), below.get(&0)) else {
        return false;
    };
    if !is_single_label_stub(&cells[stub_up].text) || !is_single_label_stub(&cells[stub_down].text)
    {
        return false;
    }
    let mut figured: Vec<Vec<String>> = Vec::new();
    for (column, &ku) in &above {
        if *column == 0 {
            continue;
        }
        let Some(&kd) = below.get(column) else {
            return false;
        };
        let (upper, lower) = (
            cells[ku].text.trim().to_string(),
            cells[kd].text.trim().to_string(),
        );
        if upper.is_empty()
            || lower.is_empty()
            || !is_complete_figure_line(&upper)
            || !is_complete_figure_line(&lower)
        {
            return false;
        }
        figured.push(vec![upper, lower]);
    }
    if figured.is_empty() {
        return false;
    }
    !is_statistics_stack(&figured)
}

/// Whether the stub holds one label run out on leader dots: a single token
/// past a run of at least four trailing dots, starting upper-case or a
/// marker -- `Contributions.....`, `(a) .....` -- so a bare `Control` or
/// `WFNS-3` never fuses, and neither does a wrapped `Net loss ... Other ...`
/// or a dated `Balance at ...`. Four dots is the leader the judge counts
/// (`has_leader`); an ellipsis or a decimal point is no leader.
fn is_single_label_stub(text: &str) -> bool {
    let trimmed = text.trim();
    let core = trimmed.trim_end_matches('.');
    if trimmed.len() - core.len() < 4 {
        return false;
    }
    let core = core.trim_end();
    let mut tokens = core.split_whitespace();
    matches!((tokens.next(), tokens.next()), (Some(one), None) if starts_labelled(one))
}

/// Rows `up` and `down` as one row: each column's texts joined with one
/// space, spans appended and boxes unioned, the lower row's cells gone and
/// the rows below renumbered -- the run-on join's own mechanics.
fn fuse_figure_rows(cells: &mut Vec<Block>, up: u16, down: u16) {
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    for (k, block) in cells.iter().enumerate() {
        let Some((r, column, row_span, column_span)) = position(block) else {
            continue;
        };
        if r != down || row_span != 1 || column_span != 1 {
            continue;
        }
        let into = cells.iter().position(|b| {
            position(b).is_some_and(|(r, c, rs, cs)| r == up && c == column && rs == 1 && cs == 1)
        });
        let Some(into) = into else {
            continue;
        };
        pairs.push((k, into));
    }
    for &(from, into) in &pairs {
        let tail = cells[from].text.clone();
        let spans = std::mem::take(&mut cells[from].spans);
        let bbox = cells[from].bbox;
        let target = &mut cells[into];
        target.text = format!("{} {tail}", target.text);
        target.spans.extend(spans);
        target.bbox = [
            target.bbox[0].min(bbox[0]),
            target.bbox[1].min(bbox[1]),
            target.bbox[2].max(bbox[2]),
            target.bbox[3].max(bbox[3]),
        ];
    }
    let gone: std::collections::BTreeSet<usize> = pairs.iter().map(|&(from, _)| from).collect();
    let mut k = 0;
    cells.retain(|_| {
        k += 1;
        !gone.contains(&(k - 1))
    });
    for block in cells.iter_mut() {
        if let Some((r, _, span, _)) = position(block) {
            if r > down {
                set_rows(block, r - 1, span);
            }
        }
    }
}

/// A head set across the table, as one cell as wide as the table.
///
/// A row whose only text is a label in the stub column, and whose label's ink
/// runs on past where the second column starts (`Geographic locations of the
/// studies`), is a head set across the table: that is what the page drew, so
/// it becomes its column-0 cell with `column_span` set to the table's width,
/// and the row's empty cells go. Two such lines in a row, with a row of data
/// under them, are one head on two lines.
///
/// A SHORT LABEL THAT FITS ITS COLUMN IS LEFT ALONE (`Whites`, `Other`): it may
/// be a group label or a row of data with its figures blank, and the page does
/// not say which. The datasets do not agree either -- PubTables spans such a
/// label across the table and FinTabNet keeps it in its column with empty
/// cells beside it -- so spanning it would fit one annotation style at the
/// other's cost (#574: PubTables train +0.012 TEDS, FinTabNet train -0.007).
///
/// LAST, after the split and the run-on join, so a row that runs on has
/// already joined the one above. Nothing else moves.
fn span_section_rows(cells: &mut Vec<Block>) {
    join_two_line_labels(cells);
    let width = cells
        .iter()
        .filter_map(position)
        .map(|(_, c, _, cs)| c + cs)
        .max()
        .unwrap_or(0);
    if width == 0 {
        return;
    }
    let sections: std::collections::BTreeSet<u16> = cells
        .iter()
        .filter_map(|b| position(b).map(|(r, _, _, _)| r))
        .filter(|&r| is_section_row(cells, r))
        .collect();
    if sections.is_empty() {
        return;
    }
    let mut gone: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
    for (k, block) in cells.iter_mut().enumerate() {
        let Some((r, c, _, _)) = position(block) else {
            continue;
        };
        if !sections.contains(&r) {
            continue;
        }
        if c == 0 {
            set_column_span(block, width);
        } else {
            gone.insert(k);
        }
    }
    let mut k = 0;
    cells.retain(|_| {
        k += 1;
        !gone.contains(&(k - 1))
    });
}

/// Whether `row` is a group label alone in the stub column: not the first
/// row, no header in it, every cell one row tall with no cell reaching into
/// it from above, and exactly one cell with text, standing in column 0.
/// Positions with no cell count as empty.
fn is_section_row(cells: &[Block], row: u16) -> bool {
    let Some(first) = cells
        .iter()
        .filter_map(position)
        .map(|(r, _, _, _)| r)
        .min()
    else {
        return false;
    };
    if row == first {
        return false;
    }
    if cells.iter().any(|b| {
        matches!(b.kind, BlockKind::TableCell { header: true, .. })
            && position(b).is_some_and(|(r, _, _, _)| r == row)
    }) {
        return false;
    }
    if cells
        .iter()
        .filter_map(position)
        .any(|(r, _, rs, _)| r < row && row < r + rs)
    {
        return false;
    }
    let mut label: Option<u16> = None;
    for block in cells {
        let Some((r, c, rs, _)) = position(block) else {
            continue;
        };
        if r != row {
            continue;
        }
        if rs != 1 {
            return false;
        }
        if block.text.trim().is_empty() {
            continue;
        }
        if label.is_some() {
            return false;
        }
        label = Some(c);
    }
    label == Some(0)
        && !reads_as_key_and_value(&cells_label(cells, row))
        && reaches_past_its_column(cells, row)
}

/// Whether the label of `row` is set across the table rather than in its
/// first column: the builder already gave it more than one column, or its ink
/// runs past where the second column starts. A short label that fits its
/// column -- `Other` with its figures left blank -- is a row of data.
fn reaches_past_its_column(cells: &[Block], row: u16) -> bool {
    let Some(label) = cells
        .iter()
        .find(|b| position(b).is_some_and(|(r, c, _, _)| r == row && c == 0))
    else {
        return false;
    };
    if position(label).is_some_and(|(_, _, _, cs)| cs > 1) {
        return true;
    }
    let second = cells
        .iter()
        .filter(|b| {
            position(b).is_some_and(|(_, c, _, cs)| c == 1 && cs == 1) && !b.text.trim().is_empty()
        })
        .map(|b| b.bbox[0])
        .fold(f64::INFINITY, f64::min);
    second.is_finite() && label.bbox[2] > second
}

/// The text of the column-0 cell of `row`.
fn cells_label(cells: &[Block], row: u16) -> String {
    cells
        .iter()
        .find(|b| position(b).is_some_and(|(r, c, _, _)| r == row && c == 0))
        .map(|b| b.text.trim().to_string())
        .unwrap_or_default()
}

/// Whether a label is a key with its value -- `Z = 4`, `Absorption
/// correction: multi-scan` -- the row of a key-value table whose value column
/// is empty, not a group label. On PubTables train such rows are most of what
/// spanning got wrong: crystal-data tables of two columns.
fn reads_as_key_and_value(label: &str) -> bool {
    label.contains('=')
        || label
            .split_once(':')
            .is_some_and(|(_, value)| !value.trim().is_empty())
}

/// A group label set on two lines: two section rows following each other,
/// where the row under them exists and is no section row, are one label.
/// The lower line joins the upper with a space between, as the run-on join
/// does, and the rows below shift up. Three in a row are levels, not one
/// wrapped label, and stay three.
fn join_two_line_labels(cells: &mut Vec<Block>) {
    let rows: std::collections::BTreeSet<u16> = cells
        .iter()
        .filter_map(|b| position(b).map(|(r, _, _, _)| r))
        .collect();
    let mut pairs: Vec<(u16, u16)> = Vec::new();
    for &row in &rows {
        let (Some(next), Some(after)) = (row.checked_add(1), row.checked_add(2)) else {
            continue;
        };
        if !rows.contains(&next) || !rows.contains(&after) {
            continue;
        }
        if !is_section_row(cells, row) || !is_section_row(cells, next) {
            continue;
        }
        if is_section_row(cells, after) {
            continue;
        }
        if row
            .checked_sub(1)
            .is_some_and(|over| rows.contains(&over) && is_section_row(cells, over))
        {
            continue;
        }
        pairs.push((row, next));
    }
    // From the bottom, so the numbers above a joined row stay what they were.
    // No cell reaches into a section row from above, so none spans over the
    // row that goes: only the rows below shift up.
    for (row, next) in pairs.into_iter().rev() {
        let mut into: Option<usize> = None;
        let mut from: Option<usize> = None;
        for (k, block) in cells.iter().enumerate() {
            if position(block).is_some_and(|(r, c, _, _)| r == row && c == 0) {
                into = Some(k);
            }
            if position(block).is_some_and(|(r, c, _, _)| r == next && c == 0) {
                from = Some(k);
            }
        }
        let (Some(into), Some(from)) = (into, from) else {
            continue;
        };
        let tail = cells[from].text.trim().to_string();
        let spans = std::mem::take(&mut cells[from].spans);
        let bbox = cells[from].bbox;
        let target = &mut cells[into];
        target.text = format!("{} {tail}", target.text.trim_end());
        target.spans.extend(spans);
        target.bbox = [
            target.bbox[0].min(bbox[0]),
            target.bbox[1].min(bbox[1]),
            target.bbox[2].max(bbox[2]),
            target.bbox[3].max(bbox[3]),
        ];
        cells.retain(|b| position(b).is_none_or(|(r, ..)| r != next));
        for block in cells.iter_mut() {
            if let Some((r, _, span, _)) = position(block) {
                if r > next {
                    set_rows(block, r - 1, span);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::TextSpan;

    fn cell(row: u16, column: u16, row_span: u16, text: &str) -> Block {
        Block {
            kind: BlockKind::TableCell {
                row,
                column,
                row_span,
                column_span: 1,
                header: false,
            },
            page: 0,
            bbox: [
                f64::from(column) * 100.0,
                700.0 - f64::from(row) * 14.0,
                f64::from(column) * 100.0 + 90.0,
                709.0 - f64::from(row) * 14.0,
            ],
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

    fn span(text: &str, x: f64, y: f64) -> TextSpan {
        TextSpan {
            text: text.to_string(),
            x,
            y,
            width: 40.0,
            height: 8.0,
            font_size: 8.0,
            ..Default::default()
        }
    }

    /// A cell whose text stands on the given lines, each `(text, y)`.
    fn lined(row: u16, column: u16, lines: &[(&str, f64)]) -> Block {
        let mut block = cell(
            row,
            column,
            1,
            &lines
                .iter()
                .map(|(text, _)| *text)
                .collect::<Vec<_>>()
                .join(" "),
        );
        let x = f64::from(column) * 100.0;
        block.spans = lines.iter().map(|(text, y)| span(text, x, *y)).collect();
        block
    }

    fn grid(cells: &[Block]) -> Vec<(u16, u16, u16, String)> {
        cells
            .iter()
            .filter_map(|b| position(b).map(|(r, c, rs, _)| (r, c, rs, b.text.clone())))
            .collect()
    }

    fn head(row: u16, column: u16, text: &str) -> Block {
        let mut block = cell(row, column, 1, text);
        if let BlockKind::TableCell { header, .. } = &mut block.kind {
            *header = true;
        }
        block
    }

    /// A label in the stub column whose ink runs on past where the second
    /// column starts: a head set across the table.
    fn across(row: u16, text: &str) -> Block {
        let mut block = cell(row, 0, 1, text);
        block.bbox[2] = 250.0;
        block
    }

    fn column_span_of(block: &Block) -> u16 {
        match block.kind {
            BlockKind::TableCell { column_span, .. } => column_span,
            _ => 0,
        }
    }

    fn in_row(cells: &[Block], row: u16) -> Vec<&Block> {
        cells
            .iter()
            .filter(|b| position(b).is_some_and(|(r, _, _, _)| r == row))
            .collect()
    }

    /// Lines numbered two rows tall leave rows nothing starts in; they go,
    /// and the cell that reached over one reaches one row less.
    #[test]
    fn rows_nothing_starts_in_are_dropped() {
        let mut cells = vec![
            cell(0, 0, 2, "Group"),
            cell(0, 1, 1, "n"),
            cell(1, 1, 1, ""),
            cell(3, 0, 1, "Control"),
            cell(3, 1, 1, "12"),
            cell(5, 0, 1, "Treated"),
            cell(5, 1, 1, "14"),
        ];
        tidy(&mut cells, false, false);
        assert_eq!(
            grid(&cells),
            vec![
                (0, 0, 1, "Group".into()),
                (0, 1, 1, "n".into()),
                (1, 0, 1, "Control".into()),
                (1, 1, 1, "12".into()),
                (2, 0, 1, "Treated".into()),
                (2, 1, 1, "14".into()),
            ]
        );
    }

    /// A label set on two lines is one cell; a row of its own is not joined,
    /// and `Equity`, short enough for its column, keeps its empty cell.
    #[test]
    fn a_row_that_runs_on_joins_the_row_above() {
        let mut cells = vec![
            cell(0, 0, 1, "Total assets"),
            cell(0, 1, 1, "4,704"),
            cell(1, 0, 1, "Deferred tax and"),
            cell(1, 1, 1, "1,398"),
            cell(2, 0, 1, "other liabilities"),
            cell(2, 1, 1, ""),
            cell(3, 0, 1, "Equity"),
            cell(3, 1, 1, ""),
        ];
        tidy(&mut cells, false, false);
        assert_eq!(
            grid(&cells),
            vec![
                (0, 0, 1, "Total assets".into()),
                (0, 1, 1, "4,704".into()),
                (1, 0, 1, "Deferred tax and other liabilities".into()),
                (1, 1, 1, "1,398".into()),
                (2, 0, 1, "Equity".into()),
                (2, 1, 1, "".into()),
            ]
        );
    }

    /// A drawn grid keeps a row the page ruled and left blank, and loses
    /// the strip no cell stands in.
    #[test]
    fn a_drawn_grid_keeps_its_blank_rows() {
        let mut cells = vec![
            cell(0, 0, 1, "Group"),
            cell(0, 1, 1, "n"),
            cell(2, 0, 1, ""),
            cell(2, 1, 1, ""),
            cell(3, 0, 1, "Control"),
            cell(3, 1, 1, "12"),
        ];
        tidy(&mut cells, true, false);
        assert_eq!(
            grid(&cells),
            vec![
                (0, 0, 1, "Group".into()),
                (0, 1, 1, "n".into()),
                (1, 0, 1, "".into()),
                (1, 1, 1, "".into()),
                (2, 0, 1, "Control".into()),
                (2, 1, 1, "12".into()),
            ]
        );
    }

    /// A header set on two lines, where the grid made no cell for the columns
    /// with nothing on the second line, is one header row.
    #[test]
    fn a_run_on_row_with_missing_cells_joins_the_row_above() {
        let mut cells = vec![
            cell(0, 0, 1, "Question"),
            cell(0, 1, 1, "Pre-"),
            cell(0, 2, 1, "Effect size"),
            cell(0, 3, 1, "P-value"),
            cell(1, 1, 1, "intervention"),
            cell(1, 2, 1, "(d)"),
            cell(2, 0, 1, "The morning report"),
            cell(2, 1, 1, "4.27"),
            cell(2, 2, 1, "0.24"),
            cell(2, 3, 1, "0.019"),
        ];
        tidy(&mut cells, false, false);
        let rows = grid(&cells);
        assert_eq!(rows.len(), 8, "{rows:?}");
        assert_eq!(rows[1], (0, 1, 1, "Pre-intervention".into()));
        assert_eq!(rows[2], (0, 2, 1, "Effect size (d)".into()));
        assert_eq!(rows[4], (1, 0, 1, "The morning report".into()));
    }

    /// A header whose group heads span columns: the line under a one-column
    /// head joins it, and the group head over two columns does not stop it.
    #[test]
    fn a_run_on_row_under_a_header_with_column_spans_joins_it() {
        let mut span = cell(0, 2, 1, "All participants");
        if let BlockKind::TableCell { column_span, .. } = &mut span.kind {
            *column_span = 2;
        }
        let mut cells = vec![
            cell(0, 0, 1, "Total PCBs"),
            cell(0, 1, 1, "CYP1A1 M1"),
            span,
            cell(1, 1, 1, "genotype"),
            cell(2, 0, 1, "<0.430"),
            cell(2, 1, 1, "Non-M1"),
            cell(2, 2, 1, "66/75"),
            cell(2, 3, 1, "Referent"),
        ];
        join_run_on_rows(&mut cells, false);
        let rows = grid(&cells);
        assert_eq!(rows[1], (0, 1, 1, "CYP1A1 M1 genotype".into()), "{rows:?}");
        assert_eq!(rows[3], (1, 0, 1, "<0.430".into()), "{rows:?}");
    }

    /// A header band does not swallow the row under it: the year line under a
    /// full-width header span stays its own row, however it reads (#574).
    #[test]
    fn a_header_band_does_not_take_the_year_row() {
        let mut band = head(0, 0, "Year Ended December 31,");
        set_column_span(&mut band, 4);
        let mut cells = vec![
            band,
            head(1, 0, "(In thousands)"),
            head(1, 1, "2018"),
            head(1, 2, "2017"),
            cell(1, 3, 1, ""),
            cell(2, 0, 1, "Tractors"),
            cell(2, 1, 1, "185,209"),
            cell(2, 2, 1, "123,152"),
            cell(2, 3, 1, "114,166"),
        ];
        tidy(&mut cells, false, false);
        assert_eq!(
            grid(&cells),
            vec![
                (0, 0, 1, "Year Ended December 31,".into()),
                (1, 0, 1, "(In thousands)".into()),
                (1, 1, 1, "2018".into()),
                (1, 2, 1, "2017".into()),
                (1, 3, 1, "".into()),
                (2, 0, 1, "Tractors".into()),
                (2, 1, 1, "185,209".into()),
                (2, 2, 1, "123,152".into()),
                (2, 3, 1, "114,166".into()),
            ]
        );
    }

    /// Without the header mark a spanning row still takes the line that runs
    /// on under it: the guard is the mark, not the width.
    #[test]
    fn an_unmarked_spanning_row_still_takes_its_run_on() {
        let mut span = cell(0, 0, 1, "Deferred tax and");
        set_column_span(&mut span, 2);
        let mut cells = vec![span, cell(1, 0, 1, "other liabilities"), cell(1, 1, 1, "")];
        tidy(&mut cells, false, false);
        assert_eq!(
            grid(&cells),
            vec![(0, 0, 1, "Deferred tax and other liabilities".into())]
        );
    }

    /// Two table rows set close together come out as one row whose cells hold
    /// the same lines at the same heights; they come apart, one row per line,
    /// and the rows below shift down.
    #[test]
    fn stacked_rows_in_one_row_come_apart() {
        let mut cells = vec![
            cell(0, 0, 1, "Measure"),
            cell(0, 1, 1, "Value"),
            lined(1, 0, &[("WFNS-3", 600.0), ("WFNS-4", 588.0)]),
            lined(1, 1, &[("4 (9%)", 600.0), ("9 (21%)", 588.0)]),
            cell(2, 0, 1, "Total"),
            cell(2, 1, 1, "13"),
        ];
        tidy(&mut cells, false, false);
        assert_eq!(
            grid(&cells),
            vec![
                (0, 0, 1, "Measure".into()),
                (0, 1, 1, "Value".into()),
                (1, 0, 1, "WFNS-3".into()),
                (1, 1, 1, "4 (9%)".into()),
                (2, 0, 1, "WFNS-4".into()),
                (2, 1, 1, "9 (21%)".into()),
                (3, 0, 1, "Total".into()),
                (3, 1, 1, "13".into()),
            ]
        );
    }

    /// Lines that read as run-on are one wrapped value, not two rows: the
    /// comma catches the second column.
    #[test]
    fn a_wrapped_value_is_not_split() {
        let mut cells = vec![
            lined(0, 0, &[("Age (95%", 600.0), ("CI)", 588.0)]),
            lined(0, 1, &[("86.8 (83.8,", 600.0), ("89.9)", 588.0)]),
        ];
        tidy(&mut cells, false, false);
        assert_eq!(
            grid(&cells),
            vec![
                (0, 0, 1, "Age (95% CI)".into()),
                (0, 1, 1, "86.8 (83.8, 89.9)".into()),
            ]
        );
    }

    /// A complete figure never runs on from another (`table_runs_on_figures`,
    /// #574): `86,090` over `(4,305)` reads as run-on today, by the
    /// first-character rule, and as two figures with the switch. Prose keeps
    /// today's rule either way: `(` stays a run-on signal, and `(1)` is a
    /// footnote marker.
    #[test]
    fn complete_figures_never_run_on_and_prose_keeps_its_rule() {
        for (above, below) in [
            ("86,090", "(4,305)"),
            ("131,181", "(82,891)"),
            ("86,090", "-"),
            ("12%", "(3%)"),
            ("$—", "($1,000)"),
        ] {
            assert!(
                runs_on(above, below, false),
                "{above:?} over {below:?} runs on today, or the veto below proves nothing"
            );
            assert!(
                !runs_on(above, below, true),
                "{above:?} over {below:?} is two figures, not a run-on"
            );
        }
        for (above, below) in [
            ("Deferred tax and", "other liabilities"),
            ("Total", "(a) Includes deferred"),
            ("86,090", "(1)"),
            ("December 31,", "2004"),
        ] {
            assert!(
                runs_on(above, below, true),
                "{above:?} over {below:?} still runs on with the switch"
            );
        }
    }

    /// DLR p61's fused row in miniature: labels over labels, nil over nil in
    /// five columns, `86,090` over `(4,305)` in two. One row today, by the
    /// run-on veto; two rows with the switch, the guard reading the nil
    /// dashes bare. Red with the veto removed: one row.
    #[test]
    fn stacked_figure_rows_come_apart_under_the_switch() {
        let fused = vec![
            lined(0, 0, &[("Contributions", 600.0), ("Distributions", 588.0)]),
            lined(0, 1, &[("—", 600.0), ("—", 588.0)]),
            lined(0, 2, &[("—", 600.0), ("—", 588.0)]),
            lined(0, 3, &[("—", 600.0), ("—", 588.0)]),
            lined(0, 4, &[("—", 600.0), ("—", 588.0)]),
            lined(0, 5, &[("—", 600.0), ("—", 588.0)]),
            lined(0, 6, &[("86,090", 600.0), ("(4,305)", 588.0)]),
            lined(0, 7, &[("12,000", 600.0), ("(1,000)", 588.0)]),
        ];
        let mut off = fused.clone();
        tidy(&mut off, false, false);
        assert_eq!(
            grid(&off),
            vec![
                (0, 0, 1, "Contributions Distributions".into()),
                (0, 1, 1, "— —".into()),
                (0, 2, 1, "— —".into()),
                (0, 3, 1, "— —".into()),
                (0, 4, 1, "— —".into()),
                (0, 5, 1, "— —".into()),
                (0, 6, 1, "86,090 (4,305)".into()),
                (0, 7, 1, "12,000 (1,000)".into()),
            ],
            "one fused row today"
        );
        let mut on = fused;
        tidy(&mut on, false, true);
        assert_eq!(
            grid(&on),
            vec![
                (0, 0, 1, "Contributions".into()),
                (0, 1, 1, "—".into()),
                (0, 2, 1, "—".into()),
                (0, 3, 1, "—".into()),
                (0, 4, 1, "—".into()),
                (0, 5, 1, "—".into()),
                (0, 6, 1, "86,090".into()),
                (0, 7, 1, "12,000".into()),
                (1, 0, 1, "Distributions".into()),
                (1, 1, 1, "—".into()),
                (1, 2, 1, "—".into()),
                (1, 3, 1, "—".into()),
                (1, 4, 1, "—".into()),
                (1, 5, 1, "—".into()),
                (1, 6, 1, "(4,305)".into()),
                (1, 7, 1, "(1,000)".into()),
            ],
            "two rows with the switch"
        );
    }

    /// A statistics stack joins under the switch as today: every lower-line
    /// figure a spread under its upper, so B's veto does not fire. An amount
    /// under an amount (`86,090` over `(4,305)`) is no stack and stays apart
    /// (see the wrap-spaced pins). Red with the guard removed: two rows.
    #[test]
    fn a_statistics_stack_joins_under_the_switch() {
        let rows = vec![
            cell(0, 0, 1, "12.4"),
            cell(0, 1, 1, "0.53"),
            cell(1, 0, 1, "(3.1)"),
            cell(1, 1, 1, ""),
        ];
        let mut off = rows.clone();
        tidy(&mut off, false, false);
        assert_eq!(
            grid(&off),
            vec![(0, 0, 1, "12.4 (3.1)".into()), (0, 1, 1, "0.53".into()),],
            "today the spread row joins the one above"
        );
        let mut on = rows;
        tidy(&mut on, false, true);
        assert_eq!(
            grid(&on),
            vec![(0, 0, 1, "12.4 (3.1)".into()), (0, 1, 1, "0.53".into()),],
            "a stack of spreads stays one row with the switch"
        );
    }

    /// A parenthesised pair or range is one figure: the confidence interval
    /// under an estimate reads as a figure, so B's veto fires for it. Red
    /// without the pair shape: the spaced pair reads as prose and runs on.
    #[test]
    fn a_parenthesised_pair_or_range_is_one_figure() {
        for cell in [
            "(4.5)",
            "(30.8)",
            "(12)",
            "(15)",
            "(38)",
            "(1.22,6.12)",
            "(0.27, 7.38)",
            "(-4.7, -0.36)",
            "(2.14E-04, 1.4)",
            "(1.2-3.4)",
            "(1.2–3.4)",
        ] {
            assert!(
                is_complete_figure_line(cell),
                "{cell:?} reads as one figure"
            );
        }
        for (above, below) in [
            ("3.53", "(1.22,6.12)"),
            ("3.53", "(0.27, 7.38)"),
            ("3.53", "(-4.7, -0.36)"),
            ("3.53", "(2.14E-04, 1.4)"),
            ("3.53", "(1.2-3.4)"),
            ("3.53", "(1.2–3.4)"),
        ] {
            assert!(
                runs_on(above, below, false),
                "{above:?} over {below:?} runs on today, or the veto proves nothing"
            );
            assert!(
                !runs_on(above, below, true),
                "{above:?} over {below:?} is two figures, not a run-on"
            );
        }
        for cell in ["(a)", "(1)", "books (i)"] {
            assert!(
                !is_complete_figure_line(cell),
                "{cell:?} stays prose or a footnote marker"
            );
        }
    }

    /// What reads as a spread under its upper: a pair or range always, a
    /// single only when neither line carries a separator or a currency glyph
    /// and the single holds a decimal or at most two digits. `12.4` over
    /// `(3.1)` is a spread; `1,204` over `(15)` is not -- the lower alone
    /// qualifies, so the pair proves the upper is checked -- nor `$ 990`
    /// over `(12)`, nor any thousands comma below. Red with a lower-only
    /// predicate: `1,204` over `(15)` reads spread and the row stays fused.
    #[test]
    fn spread_shapes_read_as_spreads_under_plain_uppers_only() {
        for (upper, lower) in [
            ("3.53", "(1.22,6.12)"),
            ("3.53", "(0.27, 7.38)"),
            ("3.53", "(-4.7, -0.36)"),
            ("3.53", "(2.14E-04, 1.4)"),
            ("3.53", "(1.2-3.4)"),
            ("3.53", "(1.2–3.4)"),
            ("12.4", "(3.1)"),
            ("0.53", "(0.12)"),
            ("297.3", "(30.8)"),
            ("12.4", "(15)"),
            ("98", "(38)"),
        ] {
            assert!(
                is_spread_token(lower, upper),
                "{upper:?} over {lower:?} reads as a spread"
            );
        }
        for (upper, lower) in [
            ("1,204", "(15)"),
            ("$ 990", "(12)"),
            ("86,090", "(4,305)"),
            ("12,000", "(1,000)"),
            ("1,234", "(456)"),
            ("38", "(102)"),
            ("206", "(235)"),
            ("16,750", "(108)"),
        ] {
            assert!(
                !is_spread_token(lower, upper),
                "{upper:?} over {lower:?} reads as an amount, no spread"
            );
        }
        // A thousands comma reads single first, never as a pair.
        assert!(!is_spread_token("(4,305)", "86,090"));
        assert!(is_spread_token("(1.22,6.12)", "3.53"));
    }

    /// PMC2685810 p4 in miniature: a wrapped stub over two rows, every figure
    /// `n` over `(n)`, the annual total only in the upper row. One row today
    /// and one row with the switch, the guard leaving the mean-`(SD)` stack
    /// fused. Red with the guard removed: two rows.
    #[test]
    fn a_mean_over_sd_stack_stays_one_row_with_the_switch() {
        let rows = vec![
            cell(0, 0, 1, "Clinics' outpatient"),
            cell(0, 1, 1, "297.3"),
            cell(0, 2, 1, "230.5"),
            cell(0, 3, 1, "15840"),
            cell(1, 0, 1, "books (i)"),
            cell(1, 1, 1, "(30.8)"),
            cell(1, 2, 1, "(14.5)"),
            cell(1, 3, 1, ""),
        ];
        let mut off = rows.clone();
        tidy(&mut off, false, false);
        assert_eq!(grid(&off).len(), 4, "one fused row today: {:?}", grid(&off));
        let mut on = rows;
        tidy(&mut on, false, true);
        assert_eq!(
            grid(&on),
            vec![
                (0, 0, 1, "Clinics' outpatient books (i)".into()),
                (0, 1, 1, "297.3 (30.8)".into()),
                (0, 2, 1, "230.5 (14.5)".into()),
                (0, 3, 1, "15840".into()),
            ],
            "the mean-(SD) stack stays one row with the switch"
        );
    }

    /// PMC2853555 p6 in miniature: an estimate over its `(95% CI)`, the stub
    /// a single line and the last figure only in the upper row. One row with
    /// the switch. Red with the guard removed: two rows.
    #[test]
    fn an_estimate_over_ci_stack_stays_one_row_with_the_switch() {
        let rows = vec![
            cell(0, 0, 1, "Effect estimate"),
            cell(0, 1, 1, "3.53"),
            cell(0, 2, 1, "106.9"),
            cell(1, 0, 1, ""),
            cell(1, 1, 1, "(1.22,6.12)"),
            cell(1, 2, 1, ""),
        ];
        let mut on = rows;
        tidy(&mut on, false, true);
        assert_eq!(
            grid(&on),
            vec![
                (0, 0, 1, "Effect estimate".into()),
                (0, 1, 1, "3.53 (1.22,6.12)".into()),
                (0, 2, 1, "106.9".into()),
            ],
            "the estimate-(CI) stack stays one row with the switch"
        );
    }

    /// ASX_ORI's fused row in miniature: `0.3` over `3.4` beside `(0.2)` over
    /// `(2.7)`. The plain lower reads bare, so the guard lets B split. Red
    /// without B: one fused row.
    #[test]
    fn an_accounting_row_with_a_plain_lower_still_splits() {
        let fused = vec![
            lined(
                0,
                0,
                &[("Effect aggregate", 600.0), ("Effect obligation", 588.0)],
            ),
            lined(0, 1, &[("0.3", 600.0), ("3.4", 588.0)]),
            lined(0, 2, &[("(0.2)", 600.0), ("(2.7)", 588.0)]),
        ];
        let mut off = fused.clone();
        tidy(&mut off, false, false);
        assert_eq!(
            grid(&off),
            vec![
                (0, 0, 1, "Effect aggregate Effect obligation".into()),
                (0, 1, 1, "0.3 3.4".into()),
                (0, 2, 1, "(0.2) (2.7)".into()),
            ],
            "one fused row without B"
        );
        let mut on = fused;
        tidy(&mut on, false, true);
        assert_eq!(
            grid(&on),
            vec![
                (0, 0, 1, "Effect aggregate".into()),
                (0, 1, 1, "0.3".into()),
                (0, 2, 1, "(0.2)".into()),
                (1, 0, 1, "Effect obligation".into()),
                (1, 1, 1, "3.4".into()),
                (1, 2, 1, "(2.7)".into()),
            ],
            "a plain lower lets B split"
        );
    }

    /// An accounting row whose lower-line figures are ALL negative splits:
    /// `Revenue` over `Costs`, `1,234` over `(456)`, no dash column to read
    /// bare. The amounts are no spreads -- separators above, three digits
    /// without a decimal below -- so B splits, and the stacked stub labels
    /// would split it regardless. Two rows with the switch, one without.
    /// Red with B off: one fused row.
    #[test]
    fn an_accounting_row_all_negative_lower_line_splits_on_stacked_labels() {
        let fused = vec![
            lined(0, 0, &[("Revenue", 600.0), ("Costs", 588.0)]),
            lined(0, 1, &[("1,234", 600.0), ("(456)", 588.0)]),
            lined(0, 2, &[("5,678", 600.0), ("(789)", 588.0)]),
        ];
        let mut off = fused.clone();
        tidy(&mut off, false, false);
        assert_eq!(
            grid(&off),
            vec![
                (0, 0, 1, "Revenue Costs".into()),
                (0, 1, 1, "1,234 (456)".into()),
                (0, 2, 1, "5,678 (789)".into()),
            ],
            "one fused row without the switch"
        );
        let mut on = fused;
        tidy(&mut on, false, true);
        assert_eq!(
            grid(&on),
            vec![
                (0, 0, 1, "Revenue".into()),
                (0, 1, 1, "1,234".into()),
                (0, 2, 1, "5,678".into()),
                (1, 0, 1, "Costs".into()),
                (1, 1, 1, "(456)".into()),
                (1, 2, 1, "(789)".into()),
            ],
            "amounts under amounts split, stub stacked or not"
        );
    }

    /// PMC2685810's shape as one fused row: a wrapped stub --
    /// `Clinics' outpatient` over `books (i)` -- with every lower figure in
    /// brackets. The continuation starts lowercase, so the stub holds no
    /// stacked labels and the guard leaves the stack fused.
    #[test]
    fn a_wrapped_stub_over_bracketed_lowers_stays_one_row() {
        let mut cells = vec![
            lined(
                0,
                0,
                &[("Clinics' outpatient", 600.0), ("books (i)", 588.0)],
            ),
            lined(0, 1, &[("297.3", 600.0), ("(30.8)", 588.0)]),
            lined(0, 2, &[("230.5", 600.0), ("(14.5)", 588.0)]),
        ];
        tidy(&mut cells, false, true);
        assert_eq!(
            grid(&cells),
            vec![
                (0, 0, 1, "Clinics' outpatient books (i)".into()),
                (0, 1, 1, "297.3 (30.8)".into()),
                (0, 2, 1, "230.5 (14.5)".into()),
            ],
            "a lowercase continuation is no stacked label"
        );
    }

    /// Numbered stub labels split like upper-case ones: `1. Revenue` over
    /// `2. Costs` with every lower figure in brackets is two rows with the
    /// switch. Red with the stub rule removed: one fused row.
    #[test]
    fn a_numbered_stub_over_bracketed_lowers_splits() {
        let mut cells = vec![
            lined(0, 0, &[("1. Revenue", 600.0), ("2. Costs", 588.0)]),
            lined(0, 1, &[("1,234", 600.0), ("(456)", 588.0)]),
            lined(0, 2, &[("5,678", 600.0), ("(789)", 588.0)]),
        ];
        tidy(&mut cells, false, true);
        assert_eq!(
            grid(&cells),
            vec![
                (0, 0, 1, "1. Revenue".into()),
                (0, 1, 1, "1,234".into()),
                (0, 2, 1, "5,678".into()),
                (1, 0, 1, "2. Costs".into()),
                (1, 1, 1, "(456)".into()),
                (1, 2, 1, "(789)".into()),
            ],
            "numbered labels split even when every lower reads parenthesised"
        );
    }

    /// Two spreads under plain values stay one fused row: `12.4` over `(3.1)`
    /// and `0.53` over `(0.12)`. The guard holds every spread. Red with the
    /// guard removed: two rows.
    #[test]
    fn spreads_under_plain_values_stay_one_fused_row() {
        let mut cells = vec![
            lined(0, 0, &[("12.4", 600.0), ("(3.1)", 588.0)]),
            lined(0, 1, &[("0.53", 600.0), ("(0.12)", 588.0)]),
        ];
        tidy(&mut cells, false, true);
        assert_eq!(
            grid(&cells),
            vec![
                (0, 0, 1, "12.4 (3.1)".into()),
                (0, 1, 1, "0.53 (0.12)".into()),
            ],
            "spreads under plain values stay fused"
        );
    }

    /// An amount under an amount splits: `1,204` over `(15)`, where the lower
    /// alone qualifies (two digits) but the upper carries a separator. Only
    /// the spread predicate frees it. Red with a lower-only predicate: one
    /// fused row.
    #[test]
    fn an_amount_under_a_separated_amount_splits_without_stacked_labels() {
        let fused = vec![
            lined(0, 0, &[("1,204", 600.0), ("(15)", 588.0)]),
            lined(0, 1, &[("2,310", 600.0), ("(22)", 588.0)]),
        ];
        let mut off = fused.clone();
        tidy(&mut off, false, false);
        assert_eq!(
            grid(&off),
            vec![
                (0, 0, 1, "1,204 (15)".into()),
                (0, 1, 1, "2,310 (22)".into()),
            ],
            "one fused row without the switch"
        );
        let mut on = fused;
        tidy(&mut on, false, true);
        assert_eq!(
            grid(&on),
            vec![
                (0, 0, 1, "1,204".into()),
                (0, 1, 1, "2,310".into()),
                (1, 0, 1, "(15)".into()),
                (1, 1, 1, "(22)".into()),
            ],
            "a separated upper frees the row even when the lower qualifies alone"
        );
    }

    /// PM_2015's blocked row in miniature: `38` over `(102)` and `206` over
    /// `(235)`. Three digits without a decimal are amounts even under plain
    /// uppers, so the row splits. Red with m258's parenthesised-only
    /// predicate: one fused row.
    #[test]
    fn three_digit_integers_under_plain_uppers_split() {
        let mut cells = vec![
            lined(0, 0, &[("38", 600.0), ("(102)", 588.0)]),
            lined(0, 1, &[("206", 600.0), ("(235)", 588.0)]),
        ];
        tidy(&mut cells, false, true);
        assert_eq!(
            grid(&cells),
            vec![
                (0, 0, 1, "38".into()),
                (0, 1, 1, "206".into()),
                (1, 0, 1, "(102)".into()),
                (1, 1, 1, "(235)".into()),
            ],
            "three-digit integers are amounts, no spreads"
        );
    }

    /// Stacked stub labels split even when every lower is a spread: `Revenue`
    /// over `Costs` with `12.4` over `(3.1)` is two rows with two labels, not
    /// one value with its spread. The stub rule overrides the guard. Red with
    /// the stub rule removed: one fused row.
    #[test]
    fn stacked_labels_split_even_when_every_lower_is_a_spread() {
        let mut cells = vec![
            lined(0, 0, &[("Revenue", 600.0), ("Costs", 588.0)]),
            lined(0, 1, &[("12.4", 600.0), ("(3.1)", 588.0)]),
            lined(0, 2, &[("0.53", 600.0), ("(0.12)", 588.0)]),
        ];
        tidy(&mut cells, false, true);
        assert_eq!(
            grid(&cells),
            vec![
                (0, 0, 1, "Revenue".into()),
                (0, 1, 1, "12.4".into()),
                (0, 2, 1, "0.53".into()),
                (1, 0, 1, "Costs".into()),
                (1, 1, 1, "(3.1)".into()),
                (1, 2, 1, "(0.12)".into()),
            ],
            "two labels are two rows even over spreads"
        );
    }

    /// What starts a fresh stub label: an upper-case letter or a marker --
    /// `(a)`, `-`, a digit and a dot. A figure or a wrapped word in
    /// parentheses is no marker, however it starts.
    #[test]
    fn stub_label_starts_read_as_labels() {
        for line in [
            "Revenue",
            "  Costs",
            "(a) Revenue",
            "(1) Revenue",
            "- Costs",
            "1. Costs",
            "2. Costs",
        ] {
            assert!(starts_labelled(line), "{line:?} starts a fresh label");
        }
        for line in [
            "",
            "books (i)",
            "(Gains) losses",
            "(456)",
            "(12)",
            "(1.22,6.12)",
            "15840",
        ] {
            assert!(!starts_labelled(line), "{line:?} starts no fresh label");
        }
    }

    /// A title-cased wrapped label with figures only on its last line stays
    /// one row: no figure stands under a figure, so neither B nor the stub
    /// arm may split it.
    #[test]
    fn a_wrapped_title_cased_label_with_figures_only_below_stays_one_row() {
        let mut cells = vec![
            lined(0, 0, &[("Total Operating", 600.0), ("Expenses", 588.0)]),
            lined(0, 1, &[("1,234", 588.0)]),
            lined(0, 2, &[("5,678", 588.0)]),
        ];
        tidy(&mut cells, false, true);
        assert_eq!(
            grid(&cells),
            vec![
                (0, 0, 1, "Total Operating Expenses".into()),
                (0, 1, 1, "1,234".into()),
                (0, 2, 1, "5,678".into()),
            ],
            "no figure under a figure: one row"
        );
    }

    /// A `(1)` footnote marker under a figure stays joined: it is no amount,
    /// so the veto never fires for it. Red with the footnote guard removed:
    /// two rows.
    #[test]
    fn a_footnote_marker_under_a_figure_stays_joined() {
        let mut cells = vec![
            lined(0, 0, &[("86,090", 600.0), ("(1)", 588.0)]),
            lined(0, 1, &[("12,000", 600.0), ("(1,000)", 588.0)]),
        ];
        tidy(&mut cells, false, true);
        assert_eq!(
            grid(&cells),
            vec![
                (0, 0, 1, "86,090 (1)".into()),
                (0, 1, 1, "12,000 (1,000)".into()),
            ]
        );
    }

    /// A wrapped label under a label stays joined: `(` stays a run-on signal
    /// for prose, which carries letters no amount has.
    #[test]
    fn a_wrapped_label_under_a_label_stays_joined() {
        let mut cells = vec![
            lined(0, 0, &[("Total", 600.0), ("(a) Includes deferred", 588.0)]),
            lined(0, 1, &[("1,398", 600.0), ("2,100", 588.0)]),
        ];
        tidy(&mut cells, false, true);
        assert_eq!(
            grid(&cells),
            vec![
                (0, 0, 1, "Total (a) Includes deferred".into()),
                (0, 1, 1, "1,398 2,100".into()),
            ]
        );
    }

    /// Two lines each, but one column's lines sit between the other's: wrapped
    /// text in each column, not rows set across them.
    #[test]
    fn lines_at_different_heights_are_not_split() {
        let mut cells = vec![
            lined(0, 0, &[("Mud and wattle", 600.0), ("Clay brick", 580.0)]),
            lined(0, 1, &[("16%", 590.0), ("6%", 570.0)]),
        ];
        tidy(&mut cells, false, false);
        assert_eq!(
            grid(&cells),
            vec![
                (0, 0, 1, "Mud and wattle Clay brick".into()),
                (0, 1, 1, "16% 6%".into()),
            ]
        );
    }

    /// A cell reaching over the split row from above reaches over its pieces.
    #[test]
    fn a_row_spanning_cell_grows_over_the_split() {
        let mut cells = vec![
            cell(0, 0, 2, "Group"),
            cell(0, 1, 1, "n"),
            cell(0, 2, 1, "%"),
            lined(1, 1, &[("32", 600.0), ("64", 588.0)]),
            lined(1, 2, &[("3 (6)", 600.0), ("14 (28)", 588.0)]),
            cell(2, 0, 1, "Treated"),
            cell(2, 1, 1, "14"),
            cell(2, 2, 1, "28"),
        ];
        tidy(&mut cells, false, false);
        assert_eq!(
            grid(&cells),
            vec![
                (0, 0, 3, "Group".into()),
                (0, 1, 1, "n".into()),
                (0, 2, 1, "%".into()),
                (1, 1, 1, "32".into()),
                (1, 2, 1, "3 (6)".into()),
                (2, 1, 1, "64".into()),
                (2, 2, 1, "14 (28)".into()),
                (3, 0, 1, "Treated".into()),
                (3, 1, 1, "14".into()),
                (3, 2, 1, "28".into()),
            ]
        );
    }

    /// A full row is a row, however it starts.
    #[test]
    fn a_full_row_is_never_joined() {
        let mut cells = vec![
            cell(0, 0, 1, "Revenue and"),
            cell(0, 1, 1, "13,110"),
            cell(1, 0, 1, "costs"),
            cell(1, 1, 1, "8,406"),
        ];
        tidy(&mut cells, false, false);
        assert_eq!(grid(&cells).len(), 4);
    }

    /// A stub-empty continuation starting uppercase joins the stubbed body
    /// row above it, where the lexical run-on would not fire. The row below
    /// is partial -- its last cell empty -- so the complete-row rule sleeps.
    #[test]
    fn an_uppercase_continuation_joins_its_row() {
        let mut cells = vec![
            cell(0, 0, 1, "Item"),
            cell(0, 1, 1, "Description"),
            cell(0, 2, 1, "Qty"),
            cell(1, 0, 1, "Widget A"),
            cell(1, 1, 1, "Stainless steel"),
            cell(1, 2, 1, "12"),
            cell(2, 0, 1, ""),
            cell(2, 1, 1, "Screws"),
            cell(2, 2, 1, ""),
            cell(3, 0, 1, "Widget B"),
            cell(3, 1, 1, "Brass"),
            cell(3, 2, 1, "8"),
        ];
        join_run_on_rows(&mut cells, false);
        let grid: Vec<(u16, u16, String)> = cells
            .iter()
            .map(|b| {
                let (r, c, _, _) = position(b).unwrap_or((0, 0, 1, 1));
                (r, c, b.text.clone())
            })
            .collect();
        assert_eq!(
            grid,
            vec![
                (0, 0, "Item".to_string()),
                (0, 1, "Description".to_string()),
                (0, 2, "Qty".to_string()),
                (1, 0, "Widget A".to_string()),
                (1, 1, "Stainless steel Screws".to_string()),
                (1, 2, "12".to_string()),
                (2, 0, "Widget B".to_string()),
                (2, 1, "Brass".to_string()),
                (2, 2, "8".to_string()),
            ],
            "{cells:?}"
        );
    }

    /// A complete lower row under balanced heads stands: grouped stubs are
    /// data rows, not continuations.
    #[test]
    fn a_complete_row_under_balanced_heads_stands() {
        let mut cells = vec![
            cell(0, 0, 1, "Variable"),
            cell(0, 1, 1, "Category"),
            cell(1, 0, 1, "1Age"),
            cell(1, 1, 1, "0-2years"),
            cell(2, 0, 1, ""),
            cell(2, 1, 1, "2-5years"),
            cell(3, 0, 1, "2Sex"),
            cell(3, 1, 1, "Female"),
        ];
        join_run_on_rows(&mut cells, false);
        assert_eq!(cells.len(), 8, "{cells:?}");
        assert_eq!(cells[5].text, "2-5years");
    }

    /// Three fifths filled still counts as complete: a grouped row missing
    /// two values stands under balanced heads. (Under a two-thirds line it
    /// would join; the majority line holds it.)
    #[test]
    fn a_majority_filled_row_under_balanced_heads_stands() {
        let mut cells = vec![
            cell(0, 0, 1, "Variable"),
            cell(0, 1, 1, "Category"),
            cell(0, 2, 1, "Crude"),
            cell(0, 3, 1, "P"),
            cell(0, 4, 1, "Adjusted"),
            cell(0, 5, 1, "P"),
            cell(1, 0, 1, "MTB genotypes"),
            cell(1, 1, 1, "MTB Non-Uganda"),
            cell(1, 2, 1, "1"),
            cell(1, 3, 1, "0.043"),
            cell(1, 4, 1, "1"),
            cell(1, 5, 1, "0.251"),
            cell(2, 0, 1, ""),
            cell(2, 1, 1, "MTB Uganda"),
            cell(2, 2, 1, "0.308 (0.099-0.966)"),
            cell(2, 3, 1, ""),
            cell(2, 4, 1, "0.304 (0.039-2.326)"),
            cell(2, 5, 1, ""),
            cell(3, 0, 1, "Age"),
            cell(3, 1, 1, "up to 5 years"),
            cell(3, 2, 1, "1"),
            cell(3, 3, 1, "0.081"),
            cell(3, 4, 1, "1"),
            cell(3, 5, 1, "0.059"),
        ];
        join_run_on_rows(&mut cells, false);
        assert_eq!(cells.len(), 24, "{cells:?}");
        assert_eq!(cells[13].text, "MTB Uganda");
    }

    /// Two thirds filled still counts as complete: a grouped row missing one
    /// value stands under balanced heads.
    #[test]
    fn a_near_complete_row_under_balanced_heads_stands() {
        let mut cells = vec![
            cell(0, 0, 1, "Aspect"),
            cell(0, 1, 1, "Preference"),
            cell(0, 2, 1, "n"),
            cell(0, 3, 1, "p"),
            cell(1, 0, 1, "Choice of venue"),
            cell(1, 1, 1, "No preference"),
            cell(1, 2, 1, "5769/6776"),
            cell(1, 3, 1, "0.001"),
            cell(2, 0, 1, ""),
            cell(2, 1, 1, "Got preference"),
            cell(2, 2, 1, "4507/5214"),
            cell(2, 3, 1, ""),
            cell(3, 0, 1, "Time of day"),
            cell(3, 1, 1, "No preference"),
            cell(3, 2, 1, "3248"),
            cell(3, 3, 1, "0.001"),
        ];
        join_run_on_rows(&mut cells, false);
        assert_eq!(cells.len(), 16, "{cells:?}");
        assert_eq!(cells[9].text, "Got preference");
    }

    /// A complete lower row under fragmentary heads joins: an open paren
    /// above licenses the completion below.
    #[test]
    fn a_complete_row_under_an_open_paren_joins() {
        let mut cells = vec![
            cell(0, 0, 1, "Domains"),
            cell(0, 1, 1, "Ill parent"),
            cell(1, 0, 1, "Mental health"),
            cell(1, 1, 1, "SCAN (Schedule for Clinical"),
            cell(2, 0, 1, ""),
            cell(2, 1, 1, "Assessment in Neuropsychiatry"),
            cell(3, 0, 1, "Daily functioning"),
            cell(3, 1, 1, "PSP"),
        ];
        join_run_on_rows(&mut cells, false);
        let grid: Vec<(u16, u16, String)> = cells
            .iter()
            .map(|b| {
                let (r, c, _, _) = position(b).unwrap_or((0, 0, 1, 1));
                (r, c, b.text.clone())
            })
            .collect();
        assert_eq!(
            grid,
            vec![
                (0, 0, "Domains".to_string()),
                (0, 1, "Ill parent".to_string()),
                (1, 0, "Mental health".to_string()),
                (
                    1,
                    1,
                    "SCAN (Schedule for Clinical Assessment in Neuropsychiatry".to_string()
                ),
                (2, 0, "Daily functioning".to_string()),
                (2, 1, "PSP".to_string()),
            ],
            "{cells:?}"
        );
    }

    /// A stub-empty row holding a figure stands: figures are no continuation.
    #[test]
    fn a_continuation_holding_a_figure_stands() {
        let mut cells = vec![
            cell(0, 0, 1, "Item"),
            cell(0, 1, 1, "Description"),
            cell(1, 0, 1, "Widget C"),
            cell(1, 1, 1, "Copper"),
            cell(2, 0, 1, ""),
            cell(2, 1, 1, "12%"),
            cell(3, 0, 1, "Widget D"),
            cell(3, 1, 1, "Tin"),
        ];
        join_run_on_rows(&mut cells, false);
        assert_eq!(cells.len(), 8, "{cells:?}");
        assert_eq!(cells[5].text, "12%");
    }

    /// Under the runs-on switch a wrap-spaced row of complete figures stays
    /// apart from the stub row above it: the lexical veto fires on the pair,
    /// and the wrapped pass refuses the figure. `(4,305)` under `86,090` is
    /// an amount under an amount, no spread, so the guard does not join it.
    /// Red with the flag unthreaded from the lexical pass, and red with
    /// m258's parenthesised-only predicate (which joins the pair).
    /// (`table_runs_on_figures`, #574).
    #[test]
    fn a_wrap_spaced_figure_row_never_joins_under_the_switch() {
        let mut cells = vec![
            cell(0, 0, 1, "Item"),
            cell(0, 1, 1, "Description"),
            cell(1, 0, 1, "Revenue"),
            cell(1, 1, 1, "86,090"),
            cell(2, 0, 1, ""),
            cell(2, 1, 1, "(4,305)"),
            cell(3, 0, 1, "Cost"),
            cell(3, 1, 1, "12,000"),
        ];
        join_run_on_rows(&mut cells, true);
        assert_eq!(cells.len(), 8, "{cells:?}");
        assert_eq!(cells[5].text, "(4,305)");
    }

    /// A wrap-spaced row holding a complete figure never joins the figure row
    /// above it: `(4,305)` is refused by `is_figure`, and `$ —` -- which
    /// `is_figure` strips to an empty core -- by `is_complete_figure_line`.
    /// A third, empty column keeps the lower row partial under rule (e), so
    /// (b) is what holds it; red without the second term for `$ —` (#574).
    ///
    /// `(4,305)` runs on lexically from `(` with the switch off -- the
    /// shipped run-on, which the wrapped pass leaves alone -- so only `$ —`
    /// is pinned with the switch off; `(4,305)` is pinned with the switch on,
    /// where the lexical veto leaves the pair to the wrapped pass.
    #[test]
    fn a_wrapped_figure_row_never_joins_a_figure_row() {
        for (below, figures) in [("(4,305)", true), ("$ —", true), ("$ —", false)] {
            let mut cells = vec![
                cell(0, 0, 1, "Item"),
                cell(0, 1, 1, "Description"),
                cell(0, 2, 1, ""),
                cell(1, 0, 1, "Revenue"),
                cell(1, 1, 1, "86,090"),
                cell(1, 2, 1, ""),
                cell(2, 0, 1, ""),
                cell(2, 1, 1, below),
                cell(2, 2, 1, ""),
                cell(3, 0, 1, "Cost"),
                cell(3, 1, 1, "12,000"),
                cell(3, 2, 1, ""),
            ];
            join_run_on_rows(&mut cells, figures);
            assert_eq!(
                cells.len(),
                12,
                "below={below} figures={figures}: {cells:?}"
            );
            assert_eq!(cells[7].text, below);
        }
    }

    /// A stub-empty continuation under the grid's first row stands: the first
    /// row holds the header by position, marked or not.
    #[test]
    fn a_continuation_under_the_first_row_stands() {
        let mut cells = vec![
            cell(0, 0, 1, "Patient characteristic"),
            cell(0, 1, 1, "Preference"),
            cell(1, 0, 1, ""),
            cell(1, 1, 1, "OR (95 % CI)"),
            cell(2, 0, 1, "Age 18-24"),
            cell(2, 1, 1, "Reference"),
        ];
        join_run_on_rows(&mut cells, false);
        assert_eq!(cells.len(), 6, "{cells:?}");
        assert_eq!(cells[3].text, "OR (95 % CI)");
    }

    /// A stub-empty row under a flagged header row stands: continuations
    /// continue body rows, not heads.
    #[test]
    fn a_continuation_under_a_header_row_stands() {
        let mut cells = vec![
            cell(0, 0, 1, "Table 3"),
            cell(0, 1, 1, "Characteristics"),
            head(1, 0, "Item"),
            head(1, 1, "Description"),
            cell(2, 0, 1, ""),
            cell(2, 1, 1, "Screws"),
            cell(3, 0, 1, "Widget B"),
            cell(3, 1, 1, "Brass"),
        ];
        join_run_on_rows(&mut cells, false);
        assert_eq!(cells.len(), 8, "{cells:?}");
        assert_eq!(cells[5].text, "Screws");
    }

    /// With no second row of ink the pitch is unmeasured and the geometry
    /// guard fails closed: the row stands, and nothing panics.
    #[test]
    fn without_a_pitch_no_wrapped_row_joins() {
        let mut cells = vec![cell(0, 0, 1, "Widget A"), cell(0, 1, 1, "Copper")];
        join_run_on_rows(&mut cells, false);
        assert_eq!(cells.len(), 2, "{cells:?}");
        assert!(median_line_pitch(&cells).is_none());
    }

    /// A group label alone in the stub column is one cell across the table.
    #[test]
    fn a_section_row_spans_the_table() {
        let mut cells = vec![
            head(0, 0, "Group"),
            head(0, 1, "n"),
            head(0, 2, "%"),
            cell(1, 0, 1, "Control"),
            cell(1, 1, 1, "12"),
            cell(1, 2, 1, "3"),
            across(2, "Organ failure"),
            cell(2, 1, 1, ""),
            cell(2, 2, 1, ""),
            cell(3, 0, 1, "Treated"),
            cell(3, 1, 1, "14"),
            cell(3, 2, 1, "5"),
        ];
        tidy(&mut cells, false, false);
        let row = in_row(&cells, 2);
        assert_eq!(row.len(), 1, "{cells:?}");
        assert_eq!(row[0].text, "Organ failure");
        assert_eq!(column_span_of(row[0]), 3);
    }

    /// Positions with no cell count as empty: the same, where the grid made
    /// no cells at all for the section row's other columns.
    #[test]
    fn a_section_row_whose_other_cells_are_missing_spans_too() {
        let mut cells = vec![
            head(0, 0, "Group"),
            head(0, 1, "n"),
            head(0, 2, "%"),
            cell(1, 0, 1, "Control"),
            cell(1, 1, 1, "12"),
            cell(1, 2, 1, "3"),
            across(2, "Organ failure"),
            cell(3, 0, 1, "Treated"),
            cell(3, 1, 1, "14"),
            cell(3, 2, 1, "5"),
        ];
        tidy(&mut cells, false, false);
        let row = in_row(&cells, 2);
        assert_eq!(row.len(), 1, "{cells:?}");
        assert_eq!(row[0].text, "Organ failure");
        assert_eq!(column_span_of(row[0]), 3);
    }

    /// Two section rows following each other, with a data row under them, are
    /// one label set on two lines: one spanning row, the data row up by one.
    #[test]
    fn a_label_on_two_lines_is_one_section_row() {
        let mut cells = vec![
            head(0, 0, "Group"),
            head(0, 1, "n"),
            head(0, 2, "%"),
            across(1, "African"),
            cell(1, 1, 1, ""),
            cell(1, 2, 1, ""),
            across(2, "Americans"),
            cell(2, 1, 1, ""),
            cell(2, 2, 1, ""),
            cell(3, 0, 1, "Control"),
            cell(3, 1, 1, "12"),
            cell(3, 2, 1, "3"),
        ];
        tidy(&mut cells, false, false);
        let row = in_row(&cells, 1);
        assert_eq!(row.len(), 1, "{cells:?}");
        assert_eq!(row[0].text, "African Americans");
        assert_eq!(column_span_of(row[0]), 3);
        assert_eq!(
            grid(&cells)
                .iter()
                .find(|(r, c, _, _)| (*r, *c) == (2, 0))
                .map(|(_, _, _, text)| text.clone()),
            Some("Control".to_string()),
            "{cells:?}"
        );
    }

    /// Three section rows in a row are levels, not one wrapped label: three
    /// spanning rows, none joined, the data row where it was.
    #[test]
    fn three_section_rows_stay_three() {
        let mut cells = vec![
            head(0, 0, "Group"),
            head(0, 1, "n"),
            head(0, 2, "%"),
            across(1, "Whites"),
            cell(1, 1, 1, ""),
            cell(1, 2, 1, ""),
            across(2, "Blacks"),
            cell(2, 1, 1, ""),
            cell(2, 2, 1, ""),
            across(3, "Other"),
            cell(3, 1, 1, ""),
            cell(3, 2, 1, ""),
            cell(4, 0, 1, "Control"),
            cell(4, 1, 1, "12"),
            cell(4, 2, 1, "3"),
        ];
        tidy(&mut cells, false, false);
        for (row, label) in [(1, "Whites"), (2, "Blacks"), (3, "Other")] {
            let cells_in = in_row(&cells, row);
            assert_eq!(cells_in.len(), 1, "row {row}: {cells:?}");
            assert_eq!(cells_in[0].text, label);
            assert_eq!(column_span_of(cells_in[0]), 3);
        }
        assert_eq!(in_row(&cells, 4).len(), 3, "{cells:?}");
    }

    /// Two cells with text are a data row, however empty the columns between
    /// them: nothing spans.
    #[test]
    fn a_row_with_a_figure_is_no_section_row() {
        let mut cells = vec![
            head(0, 0, "Group"),
            head(0, 1, "n"),
            head(0, 2, "%"),
            cell(1, 0, 1, "Total"),
            cell(1, 1, 1, ""),
            cell(1, 2, 1, "42"),
        ];
        tidy(&mut cells, false, false);
        assert_eq!(
            grid(&cells),
            vec![
                (0, 0, 1, "Group".into()),
                (0, 1, 1, "n".into()),
                (0, 2, 1, "%".into()),
                (1, 0, 1, "Total".into()),
                (1, 1, 1, "".into()),
                (1, 2, 1, "42".into()),
            ]
        );
    }

    /// A lone label in the first row is a header or a caption, not a section:
    /// nothing spans.
    /// A key whose value column is empty is a row of a key-value table,
    /// not a section: it keeps its cells.
    #[test]
    fn a_key_with_its_value_is_no_section_row() {
        for key in ["Z = 4", "Absorption correction: multi-scan"] {
            let mut cells = vec![
                cell(0, 0, 1, "Mr = 363.41"),
                cell(0, 1, 1, "Dx = 1.394 Mg m-3"),
                across(1, key),
                cell(1, 1, 1, ""),
                cell(2, 0, 1, "a = 7.8524 (16) A"),
                cell(2, 1, 1, "T = 120 (2) K"),
            ];
            tidy(&mut cells, false, false);
            let row = cells.iter().find(|b| b.text == key).expect("key");
            assert_eq!(column_span_of(row), 1, "`{key}` was spanned");
        }
    }

    /// A short label that fits its column is left where it is: `Other` with
    /// its figures blank may be a row of data, and the datasets disagree on
    /// how to write a group label that short.
    #[test]
    fn a_short_label_that_fits_its_column_is_no_section_row() {
        let mut cells = vec![
            head(0, 0, "Group"),
            head(0, 1, "n"),
            head(0, 2, "%"),
            cell(1, 0, 1, "Control"),
            cell(1, 1, 1, "12"),
            cell(1, 2, 1, "3"),
            cell(2, 0, 1, "Other"),
            cell(2, 1, 1, ""),
            cell(2, 2, 1, ""),
            cell(3, 0, 1, "Total"),
            cell(3, 1, 1, "14"),
            cell(3, 2, 1, "5"),
        ];
        tidy(&mut cells, false, false);
        assert_eq!(in_row(&cells, 2).len(), 3, "{cells:?}");
    }

    /// A table of one column has no section rows: two labels over an empty
    /// row stay two rows.
    #[test]
    fn a_one_column_table_has_no_section_rows() {
        let mut cells = vec![
            cell(0, 0, 1, "Contents"),
            cell(1, 0, 1, "Methods"),
            cell(2, 0, 1, "Results"),
            cell(3, 0, 1, ""),
            cell(4, 0, 1, "Discussion"),
        ];
        join_run_on_rows(&mut cells, false);
        let texts: Vec<String> = cells.iter().map(|b| b.text.clone()).collect();
        assert!(texts.contains(&"Methods".to_string()), "{texts:?}");
        assert!(texts.contains(&"Results".to_string()), "{texts:?}");
    }

    #[test]
    fn the_first_row_is_never_a_section_row() {
        let mut cells = vec![
            cell(0, 0, 1, "Demographic"),
            cell(0, 1, 1, ""),
            cell(0, 2, 1, ""),
            cell(1, 0, 1, "Control"),
            cell(1, 1, 1, "12"),
            cell(1, 2, 1, "3"),
        ];
        tidy(&mut cells, false, false);
        assert_eq!(grid(&cells).len(), 6);
        assert_eq!(in_row(&cells, 0).len(), 3);
    }

    fn wide(row: u16, column: u16, column_span: u16, text: &str) -> Block {
        let mut block = cell(row, column, 1, text);
        set_column_span(&mut block, column_span);
        block
    }

    /// The cells paired with their index, through the drop, as
    /// `(row, column, column_span, text, old index)`.
    fn dropped(cells: Vec<Block>) -> Vec<(u16, u16, u16, String, usize)> {
        let mut paired: Vec<(Block, usize)> =
            cells.into_iter().enumerate().map(|(i, b)| (b, i)).collect();
        drop_empty_columns(&mut paired);
        paired
            .into_iter()
            .map(|(b, i)| match b.kind {
                BlockKind::TableCell {
                    row,
                    column,
                    column_span,
                    ..
                } => (row, column, column_span, b.text.clone(), i),
                _ => unreachable!("test cells are cells"),
            })
            .collect()
    }

    /// A column blank in every row goes, the figures shift down, and the
    /// companions follow their cells.
    #[test]
    fn a_column_blank_in_every_row_goes() {
        let out = dropped(vec![
            cell(0, 0, 1, "Net sales"),
            cell(0, 1, 1, ""),
            cell(0, 2, 1, "9,898"),
            cell(1, 0, 1, "Cost"),
            cell(1, 1, 1, "  "),
            cell(1, 2, 1, "1,111"),
        ]);
        assert_eq!(
            out,
            vec![
                (0, 0, 1, "Net sales".into(), 0),
                (0, 1, 1, "9,898".into(), 2),
                (1, 0, 1, "Cost".into(), 3),
                (1, 1, 1, "1,111".into(), 5),
            ]
        );
    }

    /// A column no cell starts in -- a hole in the numbering, as the lattice
    /// leaves -- goes too.
    #[test]
    fn a_column_nothing_starts_in_goes() {
        let out = dropped(vec![
            cell(0, 0, 1, "Net sales"),
            cell(0, 2, 1, "9,898"),
            cell(1, 0, 1, "Cost"),
            cell(1, 2, 1, "1,111"),
        ]);
        assert_eq!(
            out,
            vec![
                (0, 0, 1, "Net sales".into(), 0),
                (0, 1, 1, "9,898".into(), 1),
                (1, 0, 1, "Cost".into(), 2),
                (1, 1, 1, "1,111".into(), 3),
            ]
        );
    }

    /// One value in the column keeps it, wherever it stands.
    #[test]
    fn a_column_with_one_value_stays() {
        let out = dropped(vec![
            cell(0, 0, 1, "Net sales"),
            cell(0, 1, 1, ""),
            cell(0, 2, 1, "9,898"),
            cell(1, 0, 1, "Cost"),
            cell(1, 1, 1, "$"),
            cell(1, 2, 1, "1,111"),
        ]);
        assert_eq!(out.len(), 6);
        assert!(out.iter().any(|c| c.1 == 1 && c.3 == "$"), "{out:?}");
    }

    /// Text in a header row counts: the column stays.
    #[test]
    fn a_value_in_the_header_row_keeps_its_column() {
        let out = dropped(vec![
            head(0, 0, "Item"),
            head(0, 1, "$"),
            head(0, 2, "2024"),
            cell(1, 0, 1, "Net sales"),
            cell(1, 1, 1, ""),
            cell(1, 2, 1, "9,898"),
        ]);
        assert_eq!(out.len(), 6);
        assert!(out.iter().any(|c| c.1 == 1 && c.3 == "$"), "{out:?}");
    }

    /// A span over the removed column keeps its text and loses one span.
    #[test]
    fn a_span_over_the_removed_column_loses_one() {
        let out = dropped(vec![
            wide(0, 0, 3, "Fiscal year"),
            cell(1, 0, 1, "Net sales"),
            cell(1, 1, 1, ""),
            cell(1, 2, 1, "9,898"),
        ]);
        assert_eq!(
            out,
            vec![
                (0, 0, 2, "Fiscal year".into(), 0),
                (1, 0, 1, "Net sales".into(), 1),
                (1, 1, 1, "9,898".into(), 3),
            ]
        );
    }

    /// A blank cell starting in the removed column goes even where it
    /// reaches into a column that stays, whose own cell holds the grid.
    #[test]
    fn a_blank_starter_reaching_past_the_column_goes() {
        let out = dropped(vec![
            cell(0, 0, 1, "Net sales"),
            wide(0, 1, 2, ""),
            cell(0, 2, 1, "9,898"),
            cell(1, 0, 1, "Cost"),
            cell(1, 2, 1, "1,111"),
        ]);
        assert_eq!(
            out,
            vec![
                (0, 0, 1, "Net sales".into(), 0),
                (0, 1, 1, "9,898".into(), 2),
                (1, 0, 1, "Cost".into(), 3),
                (1, 1, 1, "1,111".into(), 4),
            ]
        );
    }

    /// Two columns left standing is the floor: one column is no table to
    /// renumber, so the grid stays as it was.
    #[test]
    fn fewer_than_two_columns_left_keeps_the_grid() {
        let two = dropped(vec![
            cell(0, 0, 1, "Net sales"),
            cell(0, 1, 1, ""),
            cell(1, 0, 1, "Cost"),
            cell(1, 1, 1, ""),
        ]);
        assert_eq!(two.len(), 4);
        assert!(two.iter().any(|c| c.1 == 1), "{two:?}");
        let three = dropped(vec![
            cell(0, 0, 1, "Net sales"),
            cell(0, 1, 1, ""),
            cell(0, 2, 1, ""),
            cell(1, 0, 1, "Cost"),
            cell(1, 1, 1, ""),
            cell(1, 2, 1, ""),
        ]);
        assert_eq!(three.len(), 6);
    }
}
