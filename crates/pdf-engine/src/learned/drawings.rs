//! What a page draws around a line: its vector inputs to the line network (#574).
//!
//! pymupdf4llm reads a line against the rulings near it -- a rule over it, an
//! underline, a box around it, shading behind it -- and a heading, a table's
//! row and a boxed note are often told apart by exactly that. These are the
//! same questions asked of our own drawings: the thin lines and rectangle
//! edges [`crate::text::Ruling`] keeps for the lattice and the solid fills the
//! text device keeps beside them.
//!
//! THE PYTHON TWIN is `scripts/extraction/learned/drawings.py`, held to this
//! value for value by `learned_drawings_match_python`.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use super::features::PageWord;
use super::lines::line_boxes_and_sizes;
use crate::text::Ruling;

/// The names of [`line_drawings`]' values, in order.
pub const DRAWING_FEATURES: [&str; 5] = [
    "rule_above_em",
    "rule_below_em",
    "underlined",
    "framed",
    "filled",
];

/// The names of a `line_vector` row, the dump key the line network reads
/// (T3's schema on #574): [`DRAWING_FEATURES`].
pub const VECTOR_FEATURES: [&str; 5] = DRAWING_FEATURES;

/// The farthest a rule is looked for, in ems; a line with none nearer reads this.
pub const RULE_FAR_EM: f64 = 5.0;
/// How much of a line a horizontal rule must span to be its rule.
const RULE_OVERLAP: f64 = 0.5;
/// How far under a line's box, in ems, an underline may stand.
const UNDERLINE_EM: f64 = 0.4;
/// How much of a line an underline must span.
const UNDERLINE_OVERLAP: f64 = 0.6;
/// How far from a line, in ems, the four sides of its frame may stand.
const FRAME_EM: f64 = 3.0;

fn overlap(a0: f64, a1: f64, b0: f64, b1: f64) -> f64 {
    (a1.min(b1) - a0.max(b0)).max(0.0)
}

/// The drawings around one line: `bbox` is `[x0, y0, x1, y1]` in PDF user space
/// (y up), `size` its type size in points. In [`DRAWING_FEATURES`] order.
pub fn line_drawings(
    bbox: [f64; 4],
    size: f64,
    rulings: &[Ruling],
    fills: &[[f64; 4]],
) -> [f64; 5] {
    let [x0, y0, x1, y1] = bbox;
    let em = size.max(1.0);
    let width = (x1 - x0).max(1e-6);
    let height = (y1 - y0).max(1e-6);
    let horizontal = |r: &&Ruling| (r.y1 - r.y0).abs() < 1e-6;
    let vertical = |r: &&Ruling| (r.x1 - r.x0).abs() < 1e-6;
    let spans = |r: &Ruling, share: f64| overlap(x0, x1, r.x0, r.x1) >= share * width;

    let mut above = RULE_FAR_EM;
    let mut below = RULE_FAR_EM;
    let mut underlined = 0.0;
    for r in rulings.iter().filter(horizontal) {
        if !spans(r, RULE_OVERLAP) {
            continue;
        }
        if r.y0 >= y1 {
            above = above.min((r.y0 - y1) / em);
        } else if r.y0 <= y0 {
            below = below.min((y0 - r.y0) / em);
            if y0 - r.y0 <= UNDERLINE_EM * em && spans(r, UNDERLINE_OVERLAP) {
                underlined = 1.0;
            }
        }
    }
    let reach = FRAME_EM * em;
    let covers_y = |r: &Ruling| overlap(y0, y1, r.y0, r.y1) >= 0.5 * height;
    let left = rulings
        .iter()
        .filter(vertical)
        .any(|r| r.x0 <= x0 && x0 - r.x0 <= reach && covers_y(r));
    let right = rulings
        .iter()
        .filter(vertical)
        .any(|r| r.x0 >= x1 && r.x0 - x1 <= reach && covers_y(r));
    let framed = if left && right && above * em <= reach && below * em <= reach {
        1.0
    } else {
        0.0
    };
    let filled = fills
        .iter()
        .map(|f| overlap(x0, x1, f[0], f[2]) * overlap(y0, y1, f[1], f[3]) / (width * height))
        .fold(0.0, f64::max)
        .min(1.0);
    [above, below, underlined, framed, filled]
}

/// Every line of a page, as the network reads it: per block, per line in
/// [`lines_of`](super::lines::lines_of) order, the [`line_drawings`] of the
/// line's box -- the union of its words' boxes -- at its middle word's size.
/// The order is the one `line_class_probs`, `list_signal` and the network's
/// nodes share.
pub fn page_vector(
    n_blocks: usize,
    words: &[PageWord],
    rulings: &[Ruling],
    fills: &[[f64; 4]],
) -> Vec<Vec<[f64; 5]>> {
    (0..n_blocks)
        .map(|block| {
            line_boxes_and_sizes(block, words)
                .into_iter()
                .map(|(bbox, size)| line_drawings(bbox, size, rulings, fills))
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(x0: f64, x1: f64, y: f64) -> Ruling {
        Ruling {
            x0,
            y0: y,
            x1,
            y1: y,
        }
    }
    fn v(x: f64, y0: f64, y1: f64) -> Ruling {
        Ruling {
            x0: x,
            y0,
            x1: x,
            y1,
        }
    }

    #[test]
    fn a_line_reads_the_rule_over_it_its_underline_its_frame_and_its_shading() {
        let line = [100.0, 500.0, 300.0, 510.0];
        let rulings = [
            h(90.0, 310.0, 520.0),
            h(100.0, 290.0, 498.0),
            v(95.0, 490.0, 525.0),
            v(305.0, 490.0, 525.0),
        ];
        let fills = [[90.0, 495.0, 310.0, 515.0]];
        let got = line_drawings(line, 10.0, &rulings, &fills);
        assert_eq!(got, [1.0, 0.2, 1.0, 1.0, 1.0]);
        // Nothing drawn: no rule within reach, no frame, no shading.
        assert_eq!(
            line_drawings(line, 10.0, &[], &[]),
            [RULE_FAR_EM, RULE_FAR_EM, 0.0, 0.0, 0.0]
        );
        // A rule under only a word of the line is no underline.
        let short = [h(100.0, 150.0, 498.0)];
        assert_eq!(line_drawings(line, 10.0, &short, &[])[2], 0.0);
    }
}
