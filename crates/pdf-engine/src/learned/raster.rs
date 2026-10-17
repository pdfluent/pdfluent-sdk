//! What the page raster shows around a line: its image inputs to the line network (#574).
//!
//! The sibling of [`drawings`](super::drawings): the vector arm reads the rulings
//! and solid fills the device kept, and this reads only what the raster sees and
//! the vector record cannot -- ink weight (bold and heavy type, where font names
//! lie and synthetic bold is stroke-plus-fill), grey text, backgrounds from
//! images, patterns and shadings (`fills` keeps only solid axis-aligned
//! rectangles), rules drawn as images or hairline shapes the device does not
//! keep, and something drawn just left of the line (image bullets, icons, check
//! boxes: list markers `leading_marker` cannot see).
//!
//! No model is involved: these are deterministic pixel statistics over the 72-dpi
//! page raster, where 1 px is 1 pt. A CNN can come later through the same column
//! approach.
//!
//! THE PYTHON TWIN is `scripts/extraction/learned/raster.py`, held to this
//! value for value by `learned_raster_match_python`.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use super::features::PageWord;
use super::lines::line_boxes_and_sizes;

/// The names of [`line_image`]' values, in order: the dump key `line_image`'s
/// row (T3's schema on #574). ONE constant, so a rename is a one-line change.
pub const IMAGE_FEATURES: [&str; 6] = [
    "ink_density",
    "ink_luma",
    "bg_luma",
    "rule_px_above",
    "rule_px_below",
    "left_ink",
];

/// A pixel is ink when it differs from the line's background by at least
/// this: pale type, and light type on a dark band, are ink too. An absolute
/// `g < 160` read 12 per cent of the proxy's lines as blank -- grey body text
/// set light, and the white-on-grey headings of annual reports.
pub const INK_CONTRAST: u8 = 48;
/// How far the background window grows around the line box, in ems, on every side.
pub const BG_GROW_EM: f64 = 0.3;
/// How far above and below the line box a pixel rule is looked for, in ems.
pub const RULE_REACH_EM: f64 = 1.0;
/// The deepest run of spanning rows that is still a rule, in pixel rows
/// (points, at 72 dpi): a hairline or a 2 pt rule, not a line of type.
pub const RULE_MAX_ROWS: u32 = 2;
/// How much of the line box's columns one unbroken run of ink in a row must
/// cover for the row to be a rule.
pub const RULE_SPAN_SHARE: f64 = 0.6;
/// How far left of the line's left edge the marker band reaches, in ems.
pub const LEFT_BAND_EM: f64 = 2.0;
/// How far left of the line's left edge the marker band starts, in ems.
pub const LEFT_GAP_EM: f64 = 0.15;

/// A Gray8 page raster: one byte per pixel, 0 is black, row-major, top-left origin.
pub struct GrayPage<'a> {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Pixel bytes, `width * height` of them.
    pub data: &'a [u8],
}

/// Columns `[c0, c1)` and rows `[r0, r1)` of a point box, top-left origin.
///
/// The same arithmetic as `VisionRaster::crop_points` at 72 dpi (which is
/// behind the `vision` feature and so cannot be called from here -- this is
/// its maths, not a re-derivation): columns `floor(x0)..ceil(x1)`, rows
/// `H - ceil(y1)..H - floor(y0)`, clamped to the raster. The point box's y is
/// clamped to the page first, as there; a non-positive page height skips that
/// clamp. May be empty, in which case the feature reading it takes its default.
pub(crate) fn pixel_box(
    bbox: [f64; 4],
    page: &GrayPage<'_>,
    page_height_pt: f64,
) -> (u32, u32, u32, u32) {
    let top = if page_height_pt > 0.0 {
        page_height_pt
    } else {
        f64::INFINITY
    };
    let y0 = bbox[1].clamp(0.0, top);
    let y1 = bbox[3].clamp(0.0, top);
    let (w, h) = (f64::from(page.width), f64::from(page.height));
    let c0 = bbox[0].min(bbox[2]).floor().clamp(0.0, w) as u32;
    let c1 = bbox[0].max(bbox[2]).ceil().clamp(0.0, w) as u32;
    let r0 = (h - y0.max(y1).ceil()).clamp(0.0, h) as u32;
    let r1 = (h - y0.min(y1).floor()).clamp(0.0, h) as u32;
    (c0, c1, r0, r1)
}

/// The image features of one line: `bbox` is `[x0, y0, x1, y1]` in PDF user
/// space (y up), `size` its type size in points, `page` the 72-dpi Gray8 page
/// raster, `page_height_pt` the page height in points. In [`IMAGE_FEATURES`]
/// order: the fraction of ink in the line box; the mean ink luma over 255
/// (1.0 with no ink); the median non-ink luma over 255 in the box grown by
/// [`BG_GROW_EM`] on every side (1.0 with no background pixel); whether a
/// pixel row strictly above or below the box, within [`RULE_REACH_EM`], carries
/// ink on [`RULE_SPAN_SHARE`] of the box's columns; and the fraction of ink in
/// the band [`LEFT_BAND_EM`] to [`LEFT_GAP_EM`] left of the box.
///
/// The median is the upper middle (`sorted[len / 2]`), the codebase's own
/// convention for a middle. A row's distance is counted in whole rows from the
/// box edge, so both sides scan the same rows with the same reach.
pub fn line_image(bbox: [f64; 4], size: f64, page: &GrayPage<'_>, page_height_pt: f64) -> [f64; 6] {
    let [x0, y0, x1, y1] = bbox;
    let em = size.max(1.0);
    let at = |c: u32, r: u32| page.data[r as usize * page.width as usize + c as usize];

    // The line's ground: the middle of the ring the grown box adds around
    // the line box, which the line's own type does not reach -- however
    // heavy the line, its ink cannot outvote the white around it. A ring
    // clipped away at the page's edge falls back to the whole grown box.
    let grow = BG_GROW_EM * em;
    let (g0, g1, gr0, gr1) = pixel_box(
        [x0 - grow, y0 - grow, x1 + grow, y1 + grow],
        page,
        page_height_pt,
    );
    let (c0, c1, r0, r1) = pixel_box(bbox, page, page_height_pt);
    let mut around: Vec<u8> = Vec::new();
    let mut ring: Vec<u8> = Vec::new();
    for r in gr0..gr1 {
        for c in g0..g1 {
            let g = at(c, r);
            around.push(g);
            if !(r0..r1).contains(&r) || !(c0..c1).contains(&c) {
                ring.push(g);
            }
        }
    }
    around.sort_unstable();
    ring.sort_unstable();
    let ground = ring
        .get(ring.len() / 2)
        .or_else(|| around.get(around.len() / 2))
        .copied()
        .unwrap_or(255);
    let is_ink = |g: u8| g.abs_diff(ground) >= INK_CONTRAST;

    let mut ink = 0u64;
    let mut ink_sum = 0u64;
    let mut n = 0u64;
    for r in r0..r1 {
        for c in c0..c1 {
            n += 1;
            let g = at(c, r);
            if is_ink(g) {
                ink += 1;
                ink_sum += u64::from(g);
            }
        }
    }
    let ink_density = if n == 0 { 0.0 } else { ink as f64 / n as f64 };
    let ink_luma = if ink == 0 {
        1.0
    } else {
        ink_sum as f64 / ink as f64 / 255.0
    };

    let bg: Vec<u8> = around.iter().copied().filter(|&g| !is_ink(g)).collect();
    let bg_luma = if bg.is_empty() {
        1.0
    } else {
        f64::from(bg[bg.len() / 2]) / 255.0
    };

    // An empty line box has no rows or no columns to measure a rule against.
    let (rule_px_above, rule_px_below) = if c0 == c1 || r0 == r1 {
        (0.0, 0.0)
    } else {
        let reach = RULE_REACH_EM * em;
        let ncols = f64::from(c1 - c0);
        // A RULE IS UNBROKEN. At 72 dpi a row through a line of type can be
        // ink in most of its columns, but between letters and words it
        // breaks; a rule runs on. So the row's longest unbroken run of ink,
        // not its count, must cover RULE_SPAN_SHARE of the line.
        let spans = |r: u32| {
            let (mut longest, mut run) = (0u32, 0u32);
            for c in c0..c1 {
                if is_ink(at(c, r)) {
                    run += 1;
                    longest = longest.max(run);
                } else {
                    run = 0;
                }
            }
            f64::from(longest) / ncols >= RULE_SPAN_SHARE
        };
        // A RULE IS THIN. Against its own ground the anti-aliased grey of
        // the next line's type spans a row as well as a rule does, but a line
        // of type does it for rows on end: a spanning row is a rule only
        // where the run of spanning rows it sits in, on its side of the line
        // box, is at most RULE_MAX_ROWS deep.
        let thin = |r: u32, floor: u32, ceil: u32| {
            if !spans(r) {
                return false;
            }
            let mut run = 1;
            let mut up = r;
            while up > floor && run <= RULE_MAX_ROWS && spans(up - 1) {
                up -= 1;
                run += 1;
            }
            let mut down = r + 1;
            while down < ceil && run <= RULE_MAX_ROWS && spans(down) {
                down += 1;
                run += 1;
            }
            run <= RULE_MAX_ROWS
        };
        let lo = (f64::from(r0) - reach).max(0.0).ceil().min(f64::from(r0)) as u32;
        let hi = ((f64::from(r1) + reach - 1.0).floor() + 1.0)
            .clamp(f64::from(r1), f64::from(page.height)) as u32;
        let above = if (lo..r0).any(|r| thin(r, 0, r0)) {
            1.0
        } else {
            0.0
        };
        let below = if (r1..hi).any(|r| thin(r, r1, page.height)) {
            1.0
        } else {
            0.0
        };
        (above, below)
    };

    let (b0, b1, br0, br1) = pixel_box(
        [x0 - LEFT_BAND_EM * em, y0, x0 - LEFT_GAP_EM * em, y1],
        page,
        page_height_pt,
    );
    let mut band_ink = 0u64;
    let mut band_n = 0u64;
    for r in br0..br1 {
        for c in b0..b1 {
            band_n += 1;
            if is_ink(at(c, r)) {
                band_ink += 1;
            }
        }
    }
    let left_ink = if band_n == 0 {
        0.0
    } else {
        band_ink as f64 / band_n as f64
    };

    [
        ink_density,
        ink_luma,
        bg_luma,
        rule_px_above,
        rule_px_below,
        left_ink,
    ]
}

/// Every line of a page, as the network reads it: per block, per non-empty
/// line in [`lines_of`](super::lines::lines_of) order, the [`line_image`] of
/// the line's box at its middle word's size. Built exactly like
/// [`page_vector`](super::drawings::page_vector), from the same
/// [`line_boxes_and_sizes`](super::lines::line_boxes_and_sizes): the rows are
/// in exactly the same order and count as `line_vector`.
pub fn page_image(
    n_blocks: usize,
    words: &[PageWord],
    page: &GrayPage<'_>,
    page_height_pt: f64,
) -> Vec<Vec<[f64; 6]>> {
    (0..n_blocks)
        .map(|block| {
            line_boxes_and_sizes(block, words)
                .into_iter()
                .map(|(bbox, size)| line_image(bbox, size, page, page_height_pt))
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn white(width: u32, height: u32) -> Vec<u8> {
        vec![255; width as usize * height as usize]
    }

    fn fill(data: &mut [u8], width: u32, c0: u32, c1: u32, r0: u32, r1: u32, g: u8) {
        for r in r0..r1 {
            for c in c0..c1 {
                data[r as usize * width as usize + c as usize] = g;
            }
        }
    }

    fn close(got: f64, want: f64) -> bool {
        (got - want).abs() < 1e-9
    }

    #[test]
    fn a_bold_line_reads_heavier_than_a_regular_one() {
        // 100x40 white page; two 40x10 lines, one a two-row stripe, one eight.
        let mut data = white(100, 40);
        fill(&mut data, 100, 10, 50, 20, 22, 0);
        fill(&mut data, 100, 55, 95, 20, 28, 0);
        let page = GrayPage {
            width: 100,
            height: 40,
            data: &data,
        };
        let light = line_image([10.0, 10.0, 50.0, 20.0], 10.0, &page, 40.0)[0];
        let heavy = line_image([55.0, 10.0, 95.0, 20.0], 10.0, &page, 40.0)[0];
        assert!(close(light, 80.0 / 400.0), "the stripe: {light}");
        assert!(close(heavy, 320.0 / 400.0), "the block: {heavy}");
        assert!(heavy > light);
    }

    #[test]
    fn grey_text_reads_light() {
        let mut data = white(100, 40);
        fill(&mut data, 100, 10, 50, 20, 30, 120);
        fill(&mut data, 100, 55, 95, 20, 30, 20);
        let page = GrayPage {
            width: 100,
            height: 40,
            data: &data,
        };
        let grey = line_image([10.0, 10.0, 50.0, 20.0], 10.0, &page, 40.0)[1];
        let black = line_image([55.0, 10.0, 95.0, 20.0], 10.0, &page, 40.0)[1];
        assert!(close(grey, 120.0 / 255.0), "grey ink: {grey}");
        assert!(close(black, 20.0 / 255.0), "black ink: {black}");
        assert!(grey > black);
    }

    #[test]
    fn a_shaded_band_reads_in_bg_luma() {
        // The grown box of [30, 10, 70, 20] at em 10 is [27, 7, 73, 23]:
        // shade it 220 and put one dark word in the line.
        let mut data = white(100, 40);
        fill(&mut data, 100, 27, 73, 17, 33, 220);
        fill(&mut data, 100, 35, 55, 22, 28, 0);
        let page = GrayPage {
            width: 100,
            height: 40,
            data: &data,
        };
        let got = line_image([30.0, 10.0, 70.0, 20.0], 10.0, &page, 40.0);
        assert!(close(got[2], 220.0 / 255.0), "shaded ground: {}", got[2]);
        assert!(got[0] > 0.0, "the word still reads as ink: {}", got[0]);
    }

    #[test]
    fn a_pixel_rule_over_the_line_is_seen() {
        // A 100-column line; rows 10..20 above it and 30..40 below at em 10.
        let mut data = white(100, 40);
        fill(&mut data, 100, 0, 60, 15, 16, 0);
        fill(&mut data, 100, 0, 60, 35, 36, 0);
        let page = GrayPage {
            width: 100,
            height: 40,
            data: &data,
        };
        let got = line_image([0.0, 10.0, 100.0, 20.0], 10.0, &page, 40.0);
        assert_eq!(got[3], 1.0, "the row over the line spans 60%");
        assert_eq!(got[4], 1.0, "and so does the row under it");
    }

    #[test]
    fn a_line_of_type_over_the_line_is_no_rule() {
        // Three spanning rows over the line -- the band of the line above's
        // type -- and two under it, a 2 pt rule.
        let mut data = white(100, 40);
        fill(&mut data, 100, 0, 100, 13, 16, 0);
        fill(&mut data, 100, 0, 100, 34, 36, 0);
        let page = GrayPage {
            width: 100,
            height: 40,
            data: &data,
        };
        let got = line_image([0.0, 10.0, 100.0, 20.0], 10.0, &page, 40.0);
        assert_eq!(got[3], 0.0, "three rows deep is type, not a rule");
        assert_eq!(got[4], 1.0, "two rows deep is a rule");
    }

    #[test]
    fn a_row_of_letters_is_no_rule() {
        // Over the line: 80 of 100 columns ink, in words of 16 with a gap of
        // 4 -- no unbroken run reaches 60. Under it: one unbroken run of 60.
        let mut data = white(100, 40);
        for word in 0..5 {
            fill(&mut data, 100, word * 20, word * 20 + 16, 15, 16, 0);
        }
        fill(&mut data, 100, 20, 80, 35, 36, 0);
        let page = GrayPage {
            width: 100,
            height: 40,
            data: &data,
        };
        let got = line_image([0.0, 10.0, 100.0, 20.0], 10.0, &page, 40.0);
        assert_eq!(got[3], 0.0, "a row broken between words is type");
        assert_eq!(got[4], 1.0, "an unbroken 60% is a rule");
    }

    #[test]
    fn one_short_of_the_span_is_not() {
        let mut data = white(100, 40);
        fill(&mut data, 100, 0, 59, 15, 16, 0);
        fill(&mut data, 100, 0, 59, 35, 36, 0);
        let page = GrayPage {
            width: 100,
            height: 40,
            data: &data,
        };
        let got = line_image([0.0, 10.0, 100.0, 20.0], 10.0, &page, 40.0);
        assert_eq!(got[3], 0.0, "59% is no rule over the line");
        assert_eq!(got[4], 0.0, "nor under it");
    }

    #[test]
    fn a_pixel_at_the_threshold_is_background() {
        // On a white ground, INK_CONTRAST - 1 short of white is background and
        // INK_CONTRAST short of it is ink. The mutation check flips this
        // comparison, and this test is what goes red.
        let mut data = white(100, 40);
        fill(&mut data, 100, 10, 50, 20, 30, 255 - (INK_CONTRAST - 1));
        fill(&mut data, 100, 55, 95, 20, 30, 255 - INK_CONTRAST);
        let page = GrayPage {
            width: 100,
            height: 40,
            data: &data,
        };
        let at = line_image([10.0, 10.0, 50.0, 20.0], 10.0, &page, 40.0);
        assert_eq!(
            at[0], 0.0,
            "one step short of the contrast reads as background"
        );
        assert_eq!(at[1], 1.0, "with no ink the luma is white");
        let under = line_image([55.0, 10.0, 95.0, 20.0], 10.0, &page, 40.0);
        assert_eq!(under[0], 1.0, "the contrast itself reads as ink");
        assert!(
            close(under[1], f64::from(255 - INK_CONTRAST) / 255.0),
            "ink luma: {}",
            under[1]
        );
    }

    #[test]
    fn white_type_on_a_grey_band_is_ink() {
        // The white-on-grey heading of an annual report: a band of 180 with a
        // white word in the line. The word is ink, lighter than its ground.
        let mut data = white(100, 40);
        fill(&mut data, 100, 20, 80, 10, 40, 180);
        fill(&mut data, 100, 30, 50, 20, 30, 255);
        let page = GrayPage {
            width: 100,
            height: 40,
            data: &data,
        };
        let got = line_image([30.0, 10.0, 70.0, 20.0], 10.0, &page, 40.0);
        assert!(close(got[0], 0.5), "the white word is ink: {}", got[0]);
        assert_eq!(got[1], 1.0, "and it is white");
        assert!(close(got[2], 180.0 / 255.0), "on a grey ground: {}", got[2]);
        assert!(got[1] > got[2], "lighter than its ground");
    }

    #[test]
    fn an_image_bullet_reads_as_left_ink() {
        // The band left of [30, 10, 70, 20] at em 10 is [10, 28.5] x [10, 20]:
        // columns 10..29, rows 20..30. An 8x8 bullet sits in it.
        let mut data = white(100, 40);
        fill(&mut data, 100, 12, 20, 22, 30, 0);
        let page = GrayPage {
            width: 100,
            height: 40,
            data: &data,
        };
        let got = line_image([30.0, 10.0, 70.0, 20.0], 10.0, &page, 40.0);
        assert!(
            close(got[5], 64.0 / 190.0),
            "the bullet over its band: {}",
            got[5]
        );
        let blank = line_image([60.0, 10.0, 95.0, 20.0], 10.0, &page, 40.0);
        assert_eq!(blank[5], 0.0, "no bullet left of the second line");
    }
}
