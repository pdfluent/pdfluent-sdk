// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The caption LINE vector (#636): the heading line model's row, and where the
//! line stands against the pictures the page draws.
//!
//! A caption is recognised by what it stands next to. The row the heading
//! model reads says nothing about pictures -- the text device dropped them --
//! and measured as an upper bound on DocLayNet's validation pages, knowing
//! where they are takes a line model's caption F1 from 0.681 to 0.812 (#574).
//!
//! The same four values and order as `picture_features` in
//! `scripts/extraction/learned/train_heading_lines.py`; the parity test holds
//! the model's names to [`caption_line_features`].

use super::heading_lines::{heading_line_features, page_rows, WIDTH};
use super::BlockVectors;

/// The picture features, by name, appended to the heading line row.
pub const PICTURE_FEATURES: [&str; 4] = [
    "picture_distance_em",
    "picture_above",
    "picture_below",
    "picture_under",
];

/// How near a picture must be, in the line's own ems, to stand directly above
/// or below it.
const NEAR_EM: f64 = 3.0;

/// The width of one caption row.
pub const CAPTION_WIDTH: usize = WIDTH + PICTURE_FEATURES.len();

/// The names of a caption row's values, in order.
#[doc(hidden)]
pub fn caption_line_features() -> Vec<String> {
    let mut out = heading_line_features();
    out.extend(PICTURE_FEATURES.iter().map(|s| s.to_string()));
    out
}

/// Where a line stands against the page's images, `[left, bottom, right, top]`
/// in the same frame as the line's box (PDF space, y up).
///
/// The distance is the gap between the two boxes in the line's ems, NaN on a
/// page with no image. "Above" is a picture whose bottom stands at most
/// `NEAR_EM` over the line's top and that overlaps it across; "below" the
/// same under it; "under" a line inside a picture's box.
pub fn picture_features(bbox: [f64; 4], size: f64, images: &[[f64; 4]]) -> [f64; 4] {
    if images.is_empty() {
        return [f64::NAN, 0.0, 0.0, 0.0];
    }
    let [x0, y0, x1, y1] = bbox;
    let em = if size > 0.0 { size } else { 1.0 };
    let (mut best, mut above, mut below, mut under) = (f64::INFINITY, 0.0, 0.0, 0.0);
    for &[a0, b0, a1, b1] in images {
        let dx = (a0 - x1).max(x0 - a1).max(0.0);
        let dy = (b0 - y1).max(y0 - b1).max(0.0);
        best = best.min((dx * dx + dy * dy).sqrt() / em);
        let across = x1.min(a1) - x0.max(a0) > 0.0;
        if across && (0.0..=NEAR_EM * em).contains(&(b0 - y1)) {
            above = 1.0;
        }
        if across && (0.0..=NEAR_EM * em).contains(&(y0 - b1)) {
            below = 1.0;
        }
        if dx == 0.0 && dy == 0.0 {
            under = 1.0;
        }
    }
    [best, above, below, under]
}

/// One caption row per line of every block, `rows[block][line]`.
pub fn page_caption_rows(
    page: &[BlockVectors],
    images: &[[f64; 4]],
) -> Vec<Vec<[f64; CAPTION_WIDTH]>> {
    page_caption_rows_on_rows(page, images, &page_rows(page))
}

/// [`page_caption_rows`] over heading rows built once: the same rows, without
/// rebuilding them for the pictures.
pub fn page_caption_rows_on_rows(
    page: &[BlockVectors],
    images: &[[f64; 4]],
    rows: &[Vec<[f64; WIDTH]>],
) -> Vec<Vec<[f64; CAPTION_WIDTH]>> {
    rows.iter()
        .zip(page)
        .map(|(rows, v)| {
            rows.iter()
                .zip(&v.line_boxes)
                .map(|(row, &(bbox, size))| {
                    let mut out = [f64::NAN; CAPTION_WIDTH];
                    out[..WIDTH].copy_from_slice(row);
                    out[WIDTH..].copy_from_slice(&picture_features(bbox, size, images));
                    out
                })
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pages of caption rows, compared the way `assert_eq!` cannot: missing
    /// context is NaN by design, and NaN never equals itself. Same shape,
    /// same values, same NaN placement -- a NaN against a number fails.
    fn assert_caption_pages_equal(
        left: &[Vec<[f64; CAPTION_WIDTH]>],
        right: &[Vec<[f64; CAPTION_WIDTH]>],
    ) {
        assert_eq!(left.len(), right.len(), "page count differs");
        for (lines, want) in left.iter().zip(right.iter()) {
            assert_eq!(lines.len(), want.len(), "row count differs");
            for (row, expected) in lines.iter().zip(want.iter()) {
                for (got, ask) in row.iter().zip(expected.iter()) {
                    assert!(
                        got == ask || (got.is_nan() && ask.is_nan()),
                        "rows differ: {row:?} vs {expected:?}"
                    );
                }
            }
        }
    }

    /// Caption rows over shared heading rows are caption rows: the zip is
    /// what this pins, since the picture features are untouched.
    #[test]
    fn caption_rows_over_shared_rows_match_page_caption_rows() {
        let block = |kind: &'static str, lines: usize| BlockVectors {
            page: 0,
            kind,
            block: [1.0; 30],
            lines: vec![[2.0; 23]; lines],
            line_boxes: (0..lines)
                .map(|k| {
                    (
                        [
                            72.0,
                            700.0 - k as f64 * 12.0,
                            200.0,
                            710.0 - k as f64 * 12.0,
                        ],
                        10.0,
                    )
                })
                .collect(),
            style: Vec::new(),
        };
        let page = [block("paragraph", 2), block("heading", 1)];
        let images: [[f64; 4]; 1] = [[72.0, 500.0, 272.0, 600.0]];
        let rows = page_rows(&page);
        assert_caption_pages_equal(
            &page_caption_rows_on_rows(&page, &images, &rows),
            &page_caption_rows(&page, &images),
        );
        assert_caption_pages_equal(
            &page_caption_rows_on_rows(&page, &[], &rows),
            &page_caption_rows(&page, &[]),
        );
    }

    #[test]
    fn a_line_under_a_picture_has_the_picture_above_it() {
        // A picture at y 500..600, a 10-point line whose top is 5 points under
        // it: half an em, and the picture directly above it, across.
        let got = picture_features(
            [72.0, 485.0, 200.0, 495.0],
            10.0,
            &[[72.0, 500.0, 272.0, 600.0]],
        );
        assert!((got[0] - 0.5).abs() < 1e-9, "{got:?}");
        assert_eq!(got[1..], [1.0, 0.0, 0.0], "{got:?}");
    }

    #[test]
    fn a_page_with_no_picture_says_so() {
        let got = picture_features([0.0, 0.0, 1.0, 1.0], 10.0, &[]);
        assert!(got[0].is_nan());
        assert_eq!(got[1..], [0.0, 0.0, 0.0]);
    }

    #[test]
    fn the_names_are_as_wide_as_a_row() {
        assert_eq!(caption_line_features().len(), CAPTION_WIDTH);
    }
}
