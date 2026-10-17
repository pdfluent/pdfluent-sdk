//! Scan detection: how much OCR a page needs, before any recognizer runs.
//!
//! [`page_needs_ocr`] surveys one page through the interpreter and reports its
//! shape as an [`OcrNeed`]: born-digital text ([`OcrNeed::None`]), an image of
//! a page with no usable text layer ([`OcrNeed::Full`]), or text around image
//! regions that carry no text of their own ([`OcrNeed::Partial`]).
//!
//! The survey counts what the page draws rather than what its resources name:
//! a `/XObject` entry nothing ever draws is not a scan of anything. Glyphs are
//! split three ways -- readable, invisible (render mode 3) and garbage (no
//! usable Unicode mapping) -- and only the readable ones count as a text
//! layer. Image coverage is the union of the drawn image rectangles over the
//! page rectangle, so overlapping images are not counted twice.
//!
//! This is page SHAPE, not policy. Whether a [`OcrNeed::Full`] page is OCR'd is
//! decided by the caller's mode (off / auto / force); in particular an already
//! searchable scan -- image plus a full invisible text layer -- still reports
//! [`OcrNeed::Full`], because an invisible layer is not trusted to be the
//! page's text. Skipping pages whose invisible layer already explains them is a
//! policy refinement for the OCR wiring, not a detection verdict.
//!
//! ## Thresholds
//!
//! | Constant | Value | Meaning |
//! |---|---|---|
//! | `MIN_READABLE_GLYPHS` | 40 | readable visible glyphs that make a text layer |
//! | `FULL_PAGE_IMAGE_COVERAGE` | 0.50 | image share of the page that makes a scan Full |
//! | `PARTIAL_MIN_IMAGE_FRACTION` | 0.15 | image share of the page that makes one region large |
//! | `PARTIAL_MAX_TEXT_OVERLAP` | 0.05 | text share of an image below which it is uncovered |
//!
//! Each constant carries its own rationale below. The 40-glyph floor is shared
//! with the vision routing's character floor: a scan regularly carries a
//! stamped page number or a scanner header in real text while the body is
//! pixels, and forty is above that and far below what any page of prose draws.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::cmp::Ordering;

use kurbo::{Affine, Point, Rect};
use pdf_render::pdf_interpret::cmap::BfString;
use pdf_render::pdf_interpret::font::Glyph;
use pdf_render::pdf_interpret::{
    BlendMode, ClipPath, Device, GlyphDrawMode, Image, Paint, PathDrawMode, SoftMask,
};

/// How much OCR a page needs.
///
/// `#[non_exhaustive]` because finer verdicts may land in a published
/// crate: a caller that matches on this keeps a fallback arm, and adding
/// one must not be a breaking change.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OcrNeed {
    /// The page reads as it is: it has a text layer and no large image region
    /// without text over it. This is also the answer for a blank page and for
    /// a page that cannot be surveyed at all -- detection never invents work
    /// it cannot see.
    None,
    /// The page is a scan: no text layer -- nothing drawn, or only invisible
    /// or garbage glyphs -- while images cover at least half of it. The whole
    /// page wants recognizing.
    Full,
    /// The page mixes text with at least one large image region (at least 15%
    /// of the page) that has next to no text over it (under 5% of the region).
    /// Only those regions want recognizing; the rest already reads.
    Partial,
}

/// How much OCR page `page` (0-based) of `doc` needs.
///
/// A page index that does not exist, a page with no area, and a page the
/// interpreter cannot walk all report [`OcrNeed::None`]: there is nothing here
/// OCR could read, or nothing this survey could see.
pub fn page_needs_ocr(doc: &crate::document::PdfDocument, page: usize) -> OcrNeed {
    doc.page_needs_ocr(page)
}

/// Readable visible glyphs that make a text layer.
///
/// Forty, shared with the vision routing's character floor: a scan regularly
/// carries a stamped page number or a scanner header in real text while the
/// body is pixels, and forty is above that and far below what any page of
/// prose draws. Glyphs, not characters: a ligature is one glyph whatever it
/// spells.
const MIN_READABLE_GLYPHS: usize = 40;

/// Image share of the page that makes a scan [`OcrNeed::Full`].
///
/// Half. A scanner fills the frame, so a real scan lands near 1.0; a page of
/// prose with figures lands far below. "Most of the page" is therefore
/// uncontroversial anywhere near the middle, and 0.5 is where the two
/// populations sit.
const FULL_PAGE_IMAGE_COVERAGE: f64 = 0.5;

/// Image share of the page that makes one region large enough to OCR on its own.
///
/// Fifteen per cent: a half-page figure is in, a logo or an icon is out. On a
/// letter page that is about 270 by 270 points -- well above anything that is
/// decoration, well below anything that is the page.
const PARTIAL_MIN_IMAGE_FRACTION: f64 = 0.15;

/// Text share of an image below which the image counts as uncovered.
///
/// Five per cent. A caption overlapping an image's edge, or a glyph box grazing
/// it, must not mark the region as read; anything genuinely set over the image
/// covers far more.
const PARTIAL_MAX_TEXT_OVERLAP: f64 = 0.05;

/// The survey behind [`page_needs_ocr`]: an interpreter device that counts
/// readable glyphs and records where text and images sit.
pub(crate) struct OcrSurveyDevice {
    page: Rect,
    /// Boxes of the readable visible glyphs, in device space.
    text_boxes: Vec<Rect>,
    readable: usize,
    images: Vec<Rect>,
    /// Fill glyphs are drawn again as strokes under render mode 2; the second
    /// pass is the same character (see `TextExtractionDevice`) and must not be
    /// counted twice.
    last_fill: Option<([f64; 6], Option<BfString>)>,
}

impl OcrSurveyDevice {
    /// A survey over a page `width` by `height` in device units.
    pub(crate) fn new(width: f64, height: f64) -> Self {
        Self {
            page: Rect::new(0.0, 0.0, width.max(0.0), height.max(0.0)),
            text_boxes: Vec::new(),
            readable: 0,
            images: Vec::new(),
            last_fill: None,
        }
    }

    /// The verdict over what the page drew.
    pub(crate) fn decide(&self) -> OcrNeed {
        let page_area = rect_area(self.page);
        if page_area <= 0.0 {
            return OcrNeed::None;
        }
        let clipped: Vec<Rect> = self
            .images
            .iter()
            .filter_map(|r| intersect(*r, self.page))
            .collect();
        if self.readable < MIN_READABLE_GLYPHS
            && union_area(&clipped) / page_area >= FULL_PAGE_IMAGE_COVERAGE
        {
            return OcrNeed::Full;
        }
        for image in &clipped {
            let image_area = rect_area(*image);
            if image_area / page_area < PARTIAL_MIN_IMAGE_FRACTION {
                continue;
            }
            let covered: f64 = self
                .text_boxes
                .iter()
                .filter_map(|t| intersect(*t, *image))
                .map(rect_area)
                .sum();
            if covered / image_area < PARTIAL_MAX_TEXT_OVERLAP {
                return OcrNeed::Partial;
            }
        }
        OcrNeed::None
    }
}

impl Device<'_> for OcrSurveyDevice {
    fn set_soft_mask(&mut self, _: Option<SoftMask<'_>>) {}
    fn set_blend_mode(&mut self, _: BlendMode) {}
    fn draw_path(&mut self, _: &kurbo::BezPath, _: Affine, _: &Paint<'_>, _: &PathDrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'_>>, _: BlendMode) {}
    fn pop_clip_path(&mut self) {}
    fn pop_transparency_group(&mut self) {}

    fn draw_glyph(
        &mut self,
        glyph: &Glyph<'_>,
        transform: Affine,
        glyph_transform: Affine,
        _: &Paint<'_>,
        draw_mode: &GlyphDrawMode,
    ) {
        // Render mode 2 draws each glyph filled and then stroked; the stroke
        // that repeats the fill just before it, at the same transform, is the
        // same character and is counted once.
        let placed = (transform * glyph_transform).as_coeffs();
        let repeats_fill = matches!(draw_mode, GlyphDrawMode::Stroke(_))
            && self
                .last_fill
                .as_ref()
                .is_some_and(|(at, unicode)| *at == placed && *unicode == glyph.as_unicode());
        self.last_fill = match draw_mode {
            GlyphDrawMode::Fill => Some((placed, glyph.as_unicode())),
            _ => None,
        };
        if repeats_fill {
            return;
        }
        // Invisible text is not a text layer this survey trusts: it may be a
        // stale OCR pass in the wrong language rather than the page's words.
        if matches!(draw_mode, GlyphDrawMode::Invisible) {
            return;
        }
        if !glyph_is_readable(glyph) {
            return;
        }
        self.readable += 1;
        self.text_boxes
            .push(glyph_box(transform * glyph_transform, advance_width(glyph)));
    }

    fn draw_image(&mut self, image: Image<'_, '_>, transform: Affine) {
        // The transform maps the image's pixel rectangle into device space.
        let w = f64::from(image.width().max(1));
        let h = f64::from(image.height().max(1));
        let corners = [
            transform * Point::new(0.0, 0.0),
            transform * Point::new(w, 0.0),
            transform * Point::new(w, h),
            transform * Point::new(0.0, h),
        ];
        let (mut x0, mut y0, mut x1, mut y1) = (
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        );
        for p in corners {
            x0 = x0.min(p.x);
            y0 = y0.min(p.y);
            x1 = x1.max(p.x);
            y1 = y1.max(p.y);
        }
        // A bounding box rather than the parallelogram: a rotated image counts
        // a little more than it covers, which errs towards OCRing it.
        self.images.push(Rect::new(x0, y0, x1, y1));
    }

    fn draw_placed_text(&mut self, text: &str, rect: Rect) {
        // A form value with no appearance stream: real text at a known place,
        // which the extractor reads and this survey counts with it.
        let readable: usize = text.chars().filter(|c| char_is_readable(*c)).count();
        if readable > 0 {
            self.readable += readable;
            self.text_boxes.push(rect);
        }
    }
}

/// Advance width in em of one glyph, for its box.
fn advance_width(glyph: &Glyph<'_>) -> f64 {
    match glyph {
        Glyph::Outline(outline) => outline
            .advance_width()
            .map(|w| f64::from(w) / 1000.0)
            .unwrap_or(0.5),
        Glyph::Type3(_) => 0.5,
    }
}

/// Device-space box of a glyph: origin plus advance by a full em high.
///
/// An approximation in the extractor's own style -- ascent to descent rather
/// than ink -- which can only over-cover an image and therefore errs towards
/// calling a region read.
fn glyph_box(composed: Affine, advance_em: f64) -> Rect {
    let coeffs = composed.as_coeffs();
    let (x, y) = (coeffs[4], coeffs[5]);
    let scale = (coeffs[0] * coeffs[0] + coeffs[1] * coeffs[1]).sqrt().abs();
    let font_size = scale * 1000.0;
    let width = (advance_em * font_size).max(font_size * 0.25);
    Rect::new(
        x.min(x + width),
        y.min(y + font_size),
        x.max(x + width),
        y.max(y + font_size),
    )
}

/// Whether a glyph carries text worth counting.
///
/// Readable is "mapped and not garbage": a glyph with no Unicode mapping
/// (`.notdef`, a broken encoding, a picture font without `/ToUnicode`) is the
/// common case, and an opaque mapping ([`is_opaque`]) or the replacement
/// character completes it. Everything else -- letters, numbers, punctuation,
/// symbols, marks, spaces -- reads.
fn glyph_is_readable(glyph: &Glyph<'_>) -> bool {
    match glyph.as_unicode() {
        None => false,
        Some(BfString::Char(c)) => char_is_readable(c),
        Some(BfString::String(s)) => {
            let mut any = false;
            for c in s.chars() {
                if !char_is_readable(c) {
                    return false;
                }
                any = true;
            }
            any
        }
    }
}

/// Whether one mapped character is text rather than garbage.
///
/// The mapping chain's own definition of opaque, plus the replacement
/// character: tab, line feed, form feed and carriage return say where the text
/// breaks and read; U+FFFD is what a decoder emits when it could not decode,
/// so a page of it is a page the fonts could not say.
fn char_is_readable(c: char) -> bool {
    if c == '\u{FFFD}' {
        return false;
    }
    !is_opaque(c)
}

/// Whether `c` carries no information: controls outside the four that break
/// text, the private-use areas, FDD0-FDEF and the noncharacters.
///
/// Mirrors the `is_opaque` the Unicode fallback chain filters with, which is
/// why this survey and the extractor agree on what is garbage. Most opaque
/// mappings never reach this function -- the chain already refused them and
/// the glyph arrives unmapped -- but the check stands on its own rather than
/// trusting every present and future chain step to filter.
fn is_opaque(c: char) -> bool {
    let v = c as u32;
    (v < 0x20 && !matches!(v, 0x09 | 0x0A | 0x0C | 0x0D))
        || (0x7F..=0x9F).contains(&v)
        || (0xE000..=0xF8FF).contains(&v)
        || (0xFDD0..=0xFDEF).contains(&v)
        || (v & 0xFFFE) == 0xFFFE
        || (0xF0000..=0xFFFFD).contains(&v)
        || (0x10_0000..=0x10FFFD).contains(&v)
}

/// Area of a rectangle, never negative.
fn rect_area(r: Rect) -> f64 {
    (r.width().max(0.0)) * (r.height().max(0.0))
}

/// Intersection of two rectangles, or `None` where they do not meet.
fn intersect(a: Rect, b: Rect) -> Option<Rect> {
    let r = a.intersect(b);
    if r.width() > 0.0 && r.height() > 0.0 {
        Some(r)
    } else {
        None
    }
}

/// Area covered by at least one rectangle: a sweep over the distinct x edges,
/// merging y intervals per slab. Quadratic in the edge count, which is twice
/// the image count -- a handful on any page that is not a contact sheet.
fn union_area(rects: &[Rect]) -> f64 {
    let mut xs = Vec::with_capacity(rects.len() * 2);
    for r in rects {
        if r.width() > 0.0 && r.height() > 0.0 {
            xs.push(r.x0);
            xs.push(r.x1);
        }
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    xs.dedup();
    let mut area = 0.0;
    for slab in xs.windows(2) {
        let (x0, x1) = (slab[0], slab[1]);
        if x1 <= x0 {
            continue;
        }
        let mut ys: Vec<(f64, f64)> = rects
            .iter()
            .filter(|r| r.x0 <= x0 && r.x1 >= x1 && r.height() > 0.0)
            .map(|r| (r.y0, r.y1))
            .collect();
        if ys.is_empty() {
            continue;
        }
        ys.sort_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap_or(Ordering::Equal)
                .then(a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal))
        });
        let mut running = ys[0];
        let mut height = 0.0;
        for &(lo, hi) in &ys[1..] {
            if lo <= running.1 {
                running.1 = running.1.max(hi);
            } else {
                height += running.1 - running.0;
                running = (lo, hi);
            }
        }
        area += (x1 - x0) * (height + running.1 - running.0);
    }
    area
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A survey over a letter page is the test fixture's frame.
    fn survey() -> OcrSurveyDevice {
        OcrSurveyDevice::new(612.0, 792.0)
    }

    #[test]
    fn an_empty_page_needs_nothing() {
        assert_eq!(survey().decide(), OcrNeed::None);
    }

    #[test]
    fn forty_glyphs_are_a_text_layer_and_thirty_nine_are_not() {
        let mut yes = survey();
        yes.readable = MIN_READABLE_GLYPHS;
        assert_eq!(yes.decide(), OcrNeed::None);
        let mut no = survey();
        no.readable = MIN_READABLE_GLYPHS - 1;
        no.images.push(Rect::new(0.0, 0.0, 612.0, 792.0));
        assert_eq!(no.decide(), OcrNeed::Full);
    }

    #[test]
    fn half_the_page_in_images_is_a_scan() {
        let mut full = survey();
        full.images.push(Rect::new(0.0, 0.0, 612.0, 396.0));
        assert_eq!(full.decide(), OcrNeed::Full);
        // Overlapping images count once: two copies of the same quarter page
        // are a quarter page, not half.
        let mut overlap = survey();
        overlap.images.push(Rect::new(0.0, 0.0, 306.0, 396.0));
        overlap.images.push(Rect::new(0.0, 0.0, 306.0, 396.0));
        assert_eq!(overlap.decide(), OcrNeed::Partial);
    }

    #[test]
    fn a_small_image_alone_is_not_worth_ocr() {
        let mut small = survey();
        small.images.push(Rect::new(0.0, 0.0, 100.0, 100.0));
        assert_eq!(small.decide(), OcrNeed::None);
    }

    #[test]
    fn a_large_uncovered_image_is_partial() {
        let mut partial = survey();
        partial.readable = MIN_READABLE_GLYPHS;
        partial
            .text_boxes
            .push(Rect::new(72.0, 700.0, 540.0, 712.0));
        partial.images.push(Rect::new(0.0, 0.0, 612.0, 396.0));
        assert_eq!(partial.decide(), OcrNeed::Partial);
    }

    #[test]
    fn text_over_the_image_marks_it_read() {
        let mut read = survey();
        read.readable = MIN_READABLE_GLYPHS;
        read.images.push(Rect::new(0.0, 0.0, 612.0, 396.0));
        // A fifth of the image under text is past the five-per-cent line.
        read.text_boxes.push(Rect::new(0.0, 0.0, 612.0, 80.0));
        assert_eq!(read.decide(), OcrNeed::None);
    }

    #[test]
    fn union_area_counts_overlaps_once() {
        let rects = vec![
            Rect::new(0.0, 0.0, 10.0, 10.0),
            Rect::new(5.0, 0.0, 15.0, 10.0),
        ];
        assert_eq!(union_area(&rects), 150.0);
        assert_eq!(union_area(&[]), 0.0);
    }

    #[test]
    fn readability_is_mapping_minus_garbage() {
        assert!(char_is_readable('a'));
        assert!(char_is_readable('7'));
        assert!(char_is_readable(','));
        assert!(char_is_readable(' '));
        assert!(!char_is_readable('\u{1}'));
        assert!(!char_is_readable('\u{FFFD}'));
        assert!(!char_is_readable('\u{E000}'));
        assert!(!char_is_readable('\u{10FFFD}'));
    }
}
