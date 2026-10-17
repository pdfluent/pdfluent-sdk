// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The heading LINE vector (#574, #632): one line, its block, and the lines
//! around it on the page.
//!
//! The same list and order as `FEATURES` in
//! `scripts/extraction/learned/train_heading_lines.py`; the parity test holds
//! the model's names to [`heading_line_features`], and the tests below hold the
//! layout.
//!
//! NOTHING HERE IS A NEW MEASUREMENT of the page. The line's own values are
//! [`super::lines::line_vectors`], the block's are
//! [`super::features::block_vector`], and the neighbours are other lines'
//! values -- each already held to Python by its own parity test. What this adds
//! is where the neighbours come from: the page's lines in READING order, block
//! after block, so the line above the first line of a block is the last line
//! of the block before it. That is what lets a model see a heading that the
//! heuristics glued to the paragraph under it.

use super::lines::LINE_FEATURES;
use super::BlockVectors;

/// How far the context reaches, in lines, each way.
pub const REACH: usize = 2;

/// The line features a neighbour contributes, by position in
/// [`LINE_FEATURES`]: marker_bullet, marker_ordered, first_token_chars,
/// indent_em, prev_ends_terminal, width_frac, size_ratio, bold, gap_above_em,
/// block_is_list, block_is_table_cell, max_gap_em.
pub const CONTEXT_AT: [usize; 12] = [0, 1, 3, 5, 8, 10, 11, 12, 15, 16, 20, 21];

/// How many of the block vector's values the model reads: all of them, the
/// block's own 24 and the six document facts after them. Without the facts --
/// the document's body face and type -- manuals lost 0.15 on the heading axis
/// against the block model, which has them.
pub const BLOCK_PREFIX: usize = 30;

/// The width of one row.
pub const WIDTH: usize = 23 + BLOCK_PREFIX + 4 * (CONTEXT_AT.len() + 1);

/// The names of a row's values, in order.
#[doc(hidden)]
pub fn heading_line_features() -> Vec<String> {
    let mut out: Vec<String> = LINE_FEATURES.iter().map(|s| s.to_string()).collect();
    out.extend(
        super::features::BLOCK_FEATURES[..BLOCK_PREFIX]
            .iter()
            .map(|n| format!("of_block_{n}")),
    );
    for off in offsets() {
        let side = if off < 0 { "up" } else { "down" };
        for &c in &CONTEXT_AT {
            out.push(format!("{side}{}_{}", off.unsigned_abs(), LINE_FEATURES[c]));
        }
        out.push(format!("{side}{}_same_block", off.unsigned_abs()));
    }
    out
}

fn offsets() -> impl Iterator<Item = isize> {
    let reach = REACH as isize;
    (-reach..=reach).filter(|o| *o != 0)
}

/// The classes the line model tells apart, in the order of its output.
///
/// FROZEN: the heading lane reads index 1, the list model index 2 and the
/// table judge index 3, each by position (#574).
pub const LINE_CLASSES: [&str; 4] = ["other", "heading", "list", "table"];

/// The probability of `heading` at or above which the CLASS model would call
/// a line a heading: the manifest's `heading_threshold`, held to it by a test.
/// Reported, not acted on -- the binary `heading_lines` model decides.
pub const HEADING_THRESHOLD: f64 = 0.3;

/// One row's class probabilities, in [`LINE_CLASSES`] order: the softmax of
/// the model's margins, as LightGBM's multiclass objective defines it.
pub fn class_probabilities(row: &[f64; WIDTH]) -> [f64; 4] {
    let margins = super::models::line_classes::predict_margins(row);
    let top = margins.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mut out = margins.map(|m| (m - top).exp());
    let sum: f64 = out.iter().sum();
    for v in &mut out {
        *v /= sum;
    }
    out
}

/// Every line's class probabilities, `[block][line]`, for one page's vectors.
pub fn page_class_probabilities(page: &[BlockVectors]) -> Vec<Vec<[f64; 4]>> {
    class_probabilities_of_rows(&page_rows(page))
}

/// [`page_class_probabilities`] over rows built once: the same classes,
/// without rebuilding the rows for them.
pub fn class_probabilities_of_rows(rows: &[Vec<[f64; WIDTH]>]) -> Vec<Vec<[f64; 4]>> {
    rows.iter()
        .map(|rows| rows.iter().map(class_probabilities).collect())
        .collect()
}

/// One row per line of every block, `rows[block][line]`, for one page's
/// vectors in page order.
pub fn page_rows(page: &[BlockVectors]) -> Vec<Vec<[f64; WIDTH]>> {
    // THE PAGE'S LINES IN READING ORDER, each with the block it is in.
    let flat: Vec<(usize, &[f64; 23])> = page
        .iter()
        .enumerate()
        .flat_map(|(b, v)| v.lines.iter().map(move |line| (b, line)))
        .collect();
    let mut out: Vec<Vec<[f64; WIDTH]>> = page
        .iter()
        .map(|v| Vec::with_capacity(v.lines.len()))
        .collect();
    for (at, &(block, own)) in flat.iter().enumerate() {
        let mut row = [f64::NAN; WIDTH];
        row[..23].copy_from_slice(own);
        row[23..23 + BLOCK_PREFIX].copy_from_slice(&page[block].block[..BLOCK_PREFIX]);
        let mut col = 23 + BLOCK_PREFIX;
        for off in offsets() {
            let j = at as isize + off;
            if j >= 0 && (j as usize) < flat.len() {
                let (other_block, other) = flat[j as usize];
                for &c in &CONTEXT_AT {
                    row[col] = other[c];
                    col += 1;
                }
                row[col] = if other_block == block { 1.0 } else { 0.0 };
                col += 1;
            } else {
                col += CONTEXT_AT.len() + 1;
            }
        }
        out[block].push(row);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(kind: &'static str, lines: usize, mark: f64) -> BlockVectors {
        BlockVectors {
            page: 0,
            kind,
            block: [mark; 30],
            lines: (0..lines)
                .map(|k| {
                    let mut line = [0.0; 23];
                    for (i, v) in line.iter_mut().enumerate() {
                        *v = mark * 100.0 + k as f64 * 10.0 + i as f64 / 100.0;
                    }
                    line
                })
                .collect(),
            line_boxes: Vec::new(),
            style: Vec::new(),
        }
    }

    #[test]
    fn the_names_are_as_wide_as_a_row() {
        assert_eq!(heading_line_features().len(), WIDTH);
        assert_eq!(WIDTH, 105);
    }

    #[test]
    fn the_line_above_a_block_is_the_last_line_of_the_block_before_it() {
        let page = [block("paragraph", 2, 1.0), block("paragraph", 1, 2.0)];
        let rows = page_rows(&page);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].len(), 2);
        assert_eq!(rows[1].len(), 1);
        let names = heading_line_features();
        let at = |n: &str| names.iter().position(|x| x == n).expect(n);
        let first_of_second = &rows[1][0];
        // Its own line, its own block.
        assert_eq!(first_of_second[0], page[1].lines[0][0]);
        assert_eq!(first_of_second[at("of_block_size_ratio")], 2.0);
        // The line above it is the second line of block 0, in another block.
        assert_eq!(first_of_second[at("up1_size_ratio")], page[0].lines[1][11]);
        assert_eq!(first_of_second[at("up1_same_block")], 0.0);
        assert_eq!(first_of_second[at("up2_same_block")], 0.0);
        assert_eq!(first_of_second[at("up2_bold")], page[0].lines[0][12]);
        // Nothing below the last line of the page.
        assert!(first_of_second[at("down1_bold")].is_nan());
        assert!(first_of_second[at("down1_same_block")].is_nan());
        // The first line of the page has nothing above it, and its own
        // block's second line below.
        let first = &rows[0][0];
        assert!(first[at("up1_bold")].is_nan());
        assert_eq!(first[at("down1_same_block")], 1.0);
        assert_eq!(first[at("down2_same_block")], 0.0);
    }
}
