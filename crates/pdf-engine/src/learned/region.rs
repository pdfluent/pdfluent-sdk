//! What a learned decision reads about a BLOCK that might be inside a table
//! (#600).
//!
//! WHY THERE IS A REGION MODEL AT ALL. The structure model of #600 reaches
//! TEDS 0.7956 when it is given the annotation's own table box and 0.5975 when
//! it is given the region our heuristics found. Measured on one table set with
//! one split, ALL of that difference is the region: the engine's features and
//! candidates reproduce the Python pipeline to 0.7956 against 0.7960. There is
//! nothing left to win in the structure half, and everything left to win here.
//!
//! THE SIZE OF THE PROBLEM, on 12 DocLayNet train shards, per TABLE rather
//! than per block, because a table is judged whole:
//!
//! ```text
//! every block of it we call a cell      1,360    15.8 %
//! some blocks cells, some not           5,088    59.0 %
//! no block of it we call a cell         2,172    25.2 %
//! ```
//!
//! One table in six is cleanly ours.
//!
//! ONE COMPUTATION FOR THE DUMP AND FOR THE ENGINE, the rule [`super::pairs`]
//! states and the reason it states it. [`BLOCK_FEATURES`] of
//! [`super::features`] are the first 24; the 11 below are the ones a REGION
//! needs and a block on its own does not -- a block is inside a table mostly
//! because of what is around it.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use crate::Block;

/// The names of the 11 values [`extra`] returns, in order, and the same list
/// and order as `scripts/extraction/learned/table_region.py` (`EXTRA`).
#[doc(hidden)]
pub const EXTRA: [&str; 11] = [
    "leader_run",
    "numeric_share",
    "short_words",
    "kind_is_cell",
    "kind_is_paragraph",
    "prev_is_cell",
    "next_is_cell",
    "gap_above_line",
    "gap_below_line",
    "neighbours_within_a_line",
    "x_frac_of_widest",
];

/// How many of these in a row make a leader run.
///
/// THREE, AND MEASURED. T2 counted this on DocLayNet train (#600): a block
/// carrying three or more of these in a row is a table 1,869 times of 2,001 --
/// 93.4 per cent -- against 59 it calls text. The shape is a financial
/// statement row, `FTE Adjustment........ $ 3,757 $ 3,676`, and a table of
/// contents is the same shape. It is a feature here and not a rule in the
/// grid, because in the grid the same fact would cost a change to the premise
/// that a column divider is WHITE, in three separate places.
const LEADER_RUN: usize = 3;

fn is_leader(c: char) -> bool {
    matches!(c, '.' | '-' | '_' | '\u{00b7}' | '\u{2022}') || ('\u{2010}'..='\u{2015}').contains(&c)
}

/// A run of [`LEADER_RUN`] leader characters anywhere in the text.
fn has_leader_run(text: &str) -> bool {
    let mut run = 0usize;
    for c in text.chars() {
        run = if is_leader(c) { run + 1 } else { 0 };
        if run >= LEADER_RUN {
            return true;
        }
    }
    false
}

/// A word that is a number, however it is dressed: `1,234`, `$3.00`, `(4.5)`,
/// `12%`. The same test as `table_region.py`'s `NUMERIC`.
fn is_numeric(word: &str) -> bool {
    let body = word.trim_start_matches([' ', '$', '\u{20ac}', '\u{a3}', '(']);
    let body = body.trim_end_matches(['%', ')', ' ']);
    let body = body.strip_prefix('-').unwrap_or(body);
    !body.is_empty()
        && body
            .chars()
            .all(|c| c.is_ascii_digit() || c == ',' || c == '.')
}

/// The names of the 35 values [`vector`] returns, in order: the 24 of
/// [`super::features::BLOCK_FEATURES`] and then the 11 of [`EXTRA`].
#[doc(hidden)]
pub fn names() -> Vec<&'static str> {
    // THE FIRST 24 OF `BLOCK_FEATURES`, not all 30. The heading lane appended
    // six page-local features to that array after this model was trained
    // (#599), and the model reads by INDEX -- so taking the whole array would
    // hand it six columns it has never seen and shift nothing else, which is
    // the quiet kind of wrong. The same reason `list_item` takes 24 and says
    // so. Whether the six would help this model is an untested question and
    // not this one.
    let mut out: Vec<&'static str> = super::features::BLOCK_FEATURES[..24].to_vec();
    out.extend(EXTRA);
    out
}

/// One block's vector for the region model, in [`names`] order.
///
/// `index` is the block's place on its page in reading order -- the order the
/// dump measured it in -- and `features` and `blocks` are that page's, in the
/// same order.
#[doc(hidden)]
pub fn vector(
    features: &[super::features::BlockFeatures],
    blocks: &[&Block],
    index: usize,
) -> [f64; 35] {
    let f = &features[index];
    let kinds: Vec<&str> = features.iter().map(|b| b.kind).collect();
    let whole = super::features::block_vector(f);
    let head: [f64; 24] = whole[..24].try_into().expect("24 of 30");
    vector_of(
        head,
        &kinds,
        f.size,
        (f.gap_above_em, f.gap_below_em),
        blocks,
        index,
    )
}

/// [`vector`] from the pieces it actually reads, so a parity fixture can drive
/// it without building a whole [`super::features::BlockFeatures`].
///
/// `head` is the block's 24 values in [`super::features::BLOCK_FEATURES`]
/// order, `kinds` is every block's kind on the page in the same order as
/// `blocks`, and `size` and `gaps` are this block's own.
#[doc(hidden)]
pub fn vector_of(
    head: [f64; 24],
    kinds: &[&str],
    size: f64,
    gaps: (f64, f64),
    blocks: &[&Block],
    index: usize,
) -> [f64; 35] {
    let block = blocks[index];
    let text = block.text.as_str();
    let words: Vec<&str> = text.split_whitespace().collect();
    let numeric = words.iter().filter(|w| is_numeric(w)).count();
    let short = words.iter().filter(|w| w.chars().count() <= 3).count();
    let denominator = words.len().max(1) as f64;

    // The same line height `table_region.py` uses: the block's own size, or
    // ten points when it has none. Not the body size -- a table set two
    // points smaller than the body has its own idea of a line.
    let line = if size > 0.0 { size } else { 10.0 };

    let kind_is =
        |k: usize, want: &str| -> bool { kinds.get(k).map(|b| *b == want).unwrap_or(false) };

    // HOW MANY BLOCKS SIT A LINE AWAY. A table row has neighbours above and
    // below at about a line's distance; a paragraph in running prose does not.
    let (top, bottom) = (block.bbox[3], block.bbox[1]);
    let mut close = 0usize;
    for (k, other) in blocks.iter().enumerate() {
        if k == index {
            continue;
        }
        if (other.bbox[3] - top).abs() <= line || (other.bbox[1] - bottom).abs() <= line {
            close += 1;
        }
    }

    let widest = blocks
        .iter()
        .map(|b| b.bbox[2] - b.bbox[0])
        .fold(1.0f64, f64::max);
    let mine = block.bbox[2] - block.bbox[0];

    let tail = [
        if has_leader_run(text) { 1.0 } else { 0.0 },
        numeric as f64 / denominator,
        short as f64 / denominator,
        if kinds[index] == "table_cell" {
            1.0
        } else {
            0.0
        },
        if kinds[index] == "paragraph" {
            1.0
        } else {
            0.0
        },
        if index > 0 && kind_is(index - 1, "table_cell") {
            1.0
        } else {
            0.0
        },
        if kind_is(index + 1, "table_cell") {
            1.0
        } else {
            0.0
        },
        gaps.0,
        gaps.1,
        close as f64,
        mine / widest.max(1e-6),
    ];

    let mut out = [0.0f64; 35];
    out[..24].copy_from_slice(&head);
    out[24..].copy_from_slice(&tail);
    out
}
