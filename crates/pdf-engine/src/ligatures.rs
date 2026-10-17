//! A typographic ligature is one glyph for several letters, and `extract_text`
//! hands back the letters (#437, class R-G of #389).
//!
//! `office` set in a serif face is drawn with one `ﬃ` glyph, and a font that
//! carries a `ToUnicode` map for it usually names U+FB03 -- the code point for
//! the ligature -- rather than the three letters it stands for. Keeping that
//! code point is faithful to the file and useless to everyone downstream: a
//! search for `office`, a RAG chunk, a `grep`, an index all compare characters,
//! and U+FB03 is not `f` followed by `f` followed by `i`. mutool folds;
//! pdftotext does not, and its output is the reason a person who copies a word
//! out of a PDF cannot paste it back into a search box.
//!
//! So every TEXT output folds, and folds in exactly one place: each span is
//! rewritten on the way out of extraction ([`fold_span`]), after every layout
//! and model decision has run on the characters the glyphs declared. Plain
//! text, blocks, markdown, JSON and the bindings' per-block text all assemble
//! from spans, so all of them inherit the fold, and none of the decisions that
//! arranged those spans can tell it happened.
//!
//! The boxes go with the letters: a folded span carries one box per character,
//! and a ligature's letters split its glyph's box evenly among them, so
//! character positions stay aligned with the text. The span's own box and the
//! block's are untouched -- they cover the same ink either way.
//!
//! The option to keep the code points rides on the extraction device
//! ([`crate::text::TextExtractionDevice::keep_ligatures`], reachable from
//! [`crate::PdfDocument::set_keep_ligatures`]) and covers every TEXT output.
//! What is not a TEXT output is outside it: the training input
//! ([`crate::PdfDocument::extract_blocks_before_learning`]) stays unfolded
//! because the models read the characters the glyphs declared, and glyph-level
//! readers outside this crate keep whatever the file declared.
//!
//! WHAT IS FOLDED, AND WHAT IS DELIBERATELY NOT:
//!
//! - The ligatures of the Alphabetic Presentation Forms block, and only those:
//!   U+FB00-FB06 (Latin) and U+FB13-FB17 (Armenian). U+FB07-FB12 are
//!   unassigned, so the two ranges together are every ligature the block has.
//! - U+FB05 `ﬅ` folds to `st`, not to NFKC's `ſt`. The long s is a second
//!   presentation form of the same letter and has the same problem the
//!   ligature has: nobody types it. This is the one place the table departs
//!   from NFKC, and it departs in the direction the rest of the table goes.
//! - No other compatibility folding. NFKC also rewrites superscripts,
//!   fractions, full-width forms and the Arabic presentation forms, and each of
//!   those changes what a document says rather than how it was set: `2³` is not
//!   `23`, and `½` is not `1/2` in a table of measurements. A ligature is the
//!   only compatibility mapping where the glyph carries no meaning the letters
//!   do not.
//! - The Hebrew ligature U+FB4F `ﭏ` is outside both ranges and stays: it sits
//!   among the Hebrew presentation forms, where the neighbouring code points
//!   are pointed letters that must not be folded.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::borrow::Cow;

use crate::blocks::Block;
use crate::text::{BoundsSource, TextBlock, TextSpan};

/// The letters a ligature stands for, or `None` for every other character.
///
/// Latin first (U+FB00-FB06), then Armenian (U+FB13-FB17). The Armenian
/// expansions are the NFKC ones; the Latin ones are too, apart from U+FB05,
/// which the module docs explain.
pub(crate) const fn letters_behind(ch: char) -> Option<&'static str> {
    match ch {
        '\u{FB00}' => Some("ff"),
        '\u{FB01}' => Some("fi"),
        '\u{FB02}' => Some("fl"),
        '\u{FB03}' => Some("ffi"),
        '\u{FB04}' => Some("ffl"),
        '\u{FB05}' => Some("st"),
        '\u{FB06}' => Some("st"),
        '\u{FB13}' => Some("\u{574}\u{576}"),
        '\u{FB14}' => Some("\u{574}\u{565}"),
        '\u{FB15}' => Some("\u{574}\u{56B}"),
        '\u{FB16}' => Some("\u{57E}\u{576}"),
        '\u{FB17}' => Some("\u{574}\u{56D}"),
        _ => None,
    }
}

/// Replace every typographic ligature in `text` with the letters it stands for.
///
/// Borrows when there is nothing to fold, which is every page of almost every
/// document: the scan is one pass and the common answer costs no allocation.
pub(crate) fn fold(text: &str) -> Cow<'_, str> {
    if !text.chars().any(|ch| letters_behind(ch).is_some()) {
        return Cow::Borrowed(text);
    }

    // A ligature is never longer folded than three characters, and the input is
    // mostly not ligatures, so its own length is the right first guess.
    let mut folded = String::with_capacity(text.len());
    for ch in text.chars() {
        match letters_behind(ch) {
            Some(letters) => folded.push_str(letters),
            None => folded.push(ch),
        }
    }
    Cow::Owned(folded)
}

/// Fold the ligatures of one span, keeping its boxes aligned with its text.
///
/// One ligature glyph is one character with one box; folded it is two or three
/// letters, and the glyph's box is split evenly across them, so every letter
/// keeps the glyph's bbox and the span comes out with one box per character.
/// The span's own box is untouched -- it covers the same ink either way.
///
/// A span without a ligature is returned as it came in, vectors and all: the
/// only spans this rebuilds are the ones whose text it changes.
pub(crate) fn fold_span(span: &mut TextSpan) {
    let chars: Vec<char> = span.text.chars().collect();
    if !chars.iter().any(|ch| letters_behind(*ch).is_some()) {
        return;
    }
    let span_box = [span.x, span.y, span.x + span.width, span.y + span.height];
    let mut text = String::with_capacity(span.text.len() + 8);
    let mut bounds: Vec<[f64; 4]> = Vec::new();
    let mut tight: Vec<[f64; 4]> = Vec::new();
    let mut advances: Vec<f64> = Vec::new();
    let mut sources: Vec<BoundsSource> = Vec::new();
    let fold_boxes = !span.char_bounds.is_empty();
    let fold_tight = !span.tight_char_bounds.is_empty();
    let fold_advances = !span.glyph_advances.is_empty();
    let fold_sources = !span.glyph_bounds_sources.is_empty();
    // One walk per per-glyph vector: they run side by side in practice, and
    // mapping each against its own length keeps every index valid whatever a
    // hand-built span carries.
    let box_of = align_chars_to_glyphs(&chars, span.char_bounds.len());
    let tight_of = align_chars_to_glyphs(&chars, span.tight_char_bounds.len());
    let advance_of = align_chars_to_glyphs(&chars, span.glyph_advances.len());
    let source_of = align_chars_to_glyphs(&chars, span.glyph_bounds_sources.len());
    for (i, ch) in chars.iter().enumerate() {
        let letters = letters_behind(*ch);
        let count = letters.map_or(1, |s| s.chars().count());
        match letters {
            Some(letters) => text.push_str(letters),
            None => text.push(*ch),
        }
        if fold_boxes {
            push_char_boxes(&mut bounds, &span.char_bounds, &box_of, i, count, span_box);
        }
        if fold_tight {
            push_char_boxes(
                &mut tight,
                &span.tight_char_bounds,
                &tight_of,
                i,
                count,
                span_box,
            );
        }
        if fold_advances {
            match advance_of[i] {
                // An inserted space owns no advance: nothing is added, so the
                // sum over the span is still the sum over its glyphs.
                None => advances.push(0.0),
                Some(g) => {
                    let each = span.glyph_advances[g] / count as f64;
                    advances.extend(std::iter::repeat_n(each, count));
                }
            }
        }
        if fold_sources {
            // An inserted space was never measured: it is an estimate, which
            // is also what the aggregate below will say about the span.
            let source = source_of[i]
                .map(|g| span.glyph_bounds_sources[g])
                .unwrap_or(BoundsSource::Estimate);
            sources.extend(std::iter::repeat_n(source, count));
        }
    }
    span.text = text;
    if fold_boxes {
        span.char_bounds = bounds;
    }
    if fold_tight {
        span.tight_char_bounds = tight;
    }
    if fold_advances {
        span.glyph_advances = advances;
    }
    if fold_sources {
        span.glyph_bounds_sources = sources;
        span.bounds_source = crate::text::aggregate_bounds_source(&span.glyph_bounds_sources);
    }
}

/// Which glyph each character of a span was drawn by: the glyph index, or
/// `None` for an inserted space, which owns no box.
///
/// Boxes are one per GLYPH while characters are one per decoded scalar plus
/// the spaces the consensus inserted, so the two counts disagree whenever a
/// span holds an inserted space or a glyph that decoded to several
/// characters. The walk keeps both in order: a letter consumes the next
/// glyph, and a space consumes one only while characters and glyphs still run
/// even -- the surplus spaces are the inserted ones, which were never drawn.
///
/// Exact for every span whose glyphs decoded to one character each, which is
/// every span this fold meets except the rare one holding a multi-character
/// glyph decoding or an `/ActualText` replacement: past such a glyph the
/// letters still land on neighbouring boxes, which is the direction that
/// cannot invent a position.
fn align_chars_to_glyphs(chars: &[char], glyphs: usize) -> Vec<Option<usize>> {
    if glyphs == 0 {
        return vec![None; chars.len()];
    }
    let n = chars.len();
    let mut at = 0usize;
    chars
        .iter()
        .enumerate()
        .map(|(i, ch)| {
            let chars_left = n - i;
            let glyphs_left = glyphs.saturating_sub(at);
            if ch.is_whitespace() && chars_left > glyphs_left {
                None
            } else if glyphs_left == 0 {
                Some(glyphs - 1)
            } else {
                at += 1;
                Some(at - 1)
            }
        })
        .collect()
}

/// The boxes one character contributes: its glyph's box split evenly across
/// its letters, or the gap between its neighbours for an inserted space.
fn push_char_boxes(
    out: &mut Vec<[f64; 4]>,
    glyphs: &[[f64; 4]],
    aligned: &[Option<usize>],
    i: usize,
    count: usize,
    span_box: [f64; 4],
) {
    match aligned[i] {
        Some(g) => push_split(out, glyphs[g], count),
        None => out.push(gap_box(glyphs, aligned, i, span_box)),
    }
}

/// One glyph's box, split evenly across its letters.
///
/// The last edge is the glyph's own right edge rather than one more step, so
/// the letters tile the box to the bit instead of drifting off it by a ulp.
fn push_split(out: &mut Vec<[f64; 4]>, glyph: [f64; 4], count: usize) {
    if count == 1 {
        out.push(glyph);
        return;
    }
    let (x0, y0, x1, y1) = (glyph[0], glyph[1], glyph[2], glyph[3]);
    for t in 0..count {
        let left = x0 + (x1 - x0) * t as f64 / count as f64;
        let right = if t + 1 == count {
            x1
        } else {
            x0 + (x1 - x0) * (t + 1) as f64 / count as f64
        };
        out.push([left, y0, right, y1]);
    }
}

/// The box of an inserted space: the gap between the glyphs beside it.
///
/// A space owns no ink, so what it owns is where the ink is not -- from the
/// previous glyph's right edge to the next one's left edge, clamped to a
/// non-negative width. At either end of the span the missing neighbour is the
/// span's own edge, and the height is whatever neighbour exists.
fn gap_box(
    glyphs: &[[f64; 4]],
    aligned: &[Option<usize>],
    i: usize,
    span_box: [f64; 4],
) -> [f64; 4] {
    let prev = aligned[..i].iter().rev().find_map(|a| a.map(|g| glyphs[g]));
    let next = aligned[i..].iter().find_map(|a| a.map(|g| glyphs[g]));
    let x0 = prev.map_or(span_box[0], |b| b[2]);
    let x1 = next.map_or(x0, |b| b[0]);
    let (y0, y1) = prev
        .or(next)
        .map_or((span_box[1], span_box[3]), |b| (b[1], b[3]));
    [x0, y0, x1.max(x0), y1]
}

/// Fold every span of a line. [`TextBlock::text`] assembles from the spans, so
/// the line it hands back folds with them.
pub(crate) fn fold_text_block(block: &mut TextBlock) {
    for span in &mut block.spans {
        fold_span(span);
    }
}

/// Fold a paragraph block: its text and the spans under it together.
///
/// The serializers read the two side by side -- a styled run is matched
/// against the text it sits in -- so folding one and not the other would
/// unmatch them.
pub(crate) fn fold_block(block: &mut Block) {
    if let Cow::Owned(folded) = fold(&block.text) {
        block.text = folded;
    }
    for span in &mut block.spans {
        fold_span(span);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_latin_ligature_folds() {
        assert_eq!(
            fold("\u{FB00}\u{FB01}\u{FB02}\u{FB03}\u{FB04}\u{FB05}\u{FB06}"),
            "fffiflffifflstst"
        );
    }

    #[test]
    fn every_armenian_ligature_folds() {
        assert_eq!(
            fold("\u{FB13}\u{FB14}\u{FB15}\u{FB16}\u{FB17}"),
            "\u{574}\u{576}\u{574}\u{565}\u{574}\u{56B}\u{57E}\u{576}\u{574}\u{56D}"
        );
    }

    #[test]
    fn a_ligature_folds_where_it_sits() {
        assert_eq!(fold("o\u{FB03}ce hours"), "office hours");
    }

    /// The fast path has to be the same answer as the slow one, and it has to
    /// borrow: a page without a ligature must not be copied.
    #[test]
    fn text_without_a_ligature_is_not_copied() {
        assert!(matches!(fold("office hours"), Cow::Borrowed(_)));
    }

    /// The neighbours of the two ranges. U+FB07-FB12 are unassigned and U+FB4F
    /// is the Hebrew ligature the docs name; none of them may move.
    #[test]
    fn nothing_outside_the_two_ranges_folds() {
        for ch in ['\u{FB07}', '\u{FB12}', '\u{FB4F}', '\u{FEFF}', '\u{17F}'] {
            assert_eq!(letters_behind(ch), None, "{ch:?} should not fold");
        }
    }

    /// NFKC would fold these too, and the decision on #437 is that it must not:
    /// each one changes what the document says.
    #[test]
    fn no_other_compatibility_folding() {
        let untouched = "\u{B3}\u{BD}\u{FF21}\u{FDF2}"; // ³ ½ Ａ ﷲ
        assert_eq!(fold(untouched), untouched);
    }

    /// A folded span carries one box per character: the ligature's letters
    /// split its glyph's box evenly, and every other box is untouched.
    #[test]
    fn a_ligature_splits_its_glyph_box_evenly() {
        let mut span = TextSpan {
            text: "o\u{FB03}ce".into(),
            char_bounds: vec![
                [0.0, 0.0, 10.0, 10.0],
                [10.0, 0.0, 40.0, 10.0],
                [40.0, 0.0, 50.0, 10.0],
                [50.0, 0.0, 60.0, 10.0],
            ],
            ..Default::default()
        };
        fold_span(&mut span);
        assert_eq!(span.text, "office");
        assert_eq!(
            span.char_bounds,
            vec![
                [0.0, 0.0, 10.0, 10.0],
                [10.0, 0.0, 20.0, 10.0],
                [20.0, 0.0, 30.0, 10.0],
                [30.0, 0.0, 40.0, 10.0],
                [40.0, 0.0, 50.0, 10.0],
                [50.0, 0.0, 60.0, 10.0],
            ]
        );
    }

    /// An inserted space owns no box and takes none: the ligature beside it
    /// still splits its own glyph's box, and the space gets the gap between
    /// its neighbours.
    #[test]
    fn an_inserted_space_does_not_steal_the_ligature_box() {
        let mut span = TextSpan {
            text: "a\u{FB01} b".into(),
            x: 0.0,
            y: 0.0,
            width: 60.0,
            height: 10.0,
            char_bounds: vec![
                [0.0, 0.0, 10.0, 10.0],
                [12.0, 0.0, 32.0, 10.0],
                [40.0, 0.0, 50.0, 10.0],
            ],
            ..Default::default()
        };
        fold_span(&mut span);
        assert_eq!(span.text, "afi b");
        assert_eq!(span.char_bounds.len(), 5);
        assert_eq!(span.char_bounds[0], [0.0, 0.0, 10.0, 10.0]);
        assert_eq!(span.char_bounds[1], [12.0, 0.0, 22.0, 10.0]);
        assert_eq!(span.char_bounds[2], [22.0, 0.0, 32.0, 10.0]);
        assert_eq!(span.char_bounds[3], [32.0, 0.0, 40.0, 10.0]);
        assert_eq!(span.char_bounds[4], [40.0, 0.0, 50.0, 10.0]);
    }

    /// The rich-geometry vectors run beside the boxes, so they fold beside
    /// them: the tight box splits, the advance splits, the source repeats.
    #[test]
    fn the_rich_geometry_vectors_fold_with_the_boxes() {
        let mut span = TextSpan {
            text: "\u{FB00}x".into(),
            char_bounds: vec![[0.0, 0.0, 20.0, 10.0], [20.0, 0.0, 30.0, 10.0]],
            tight_char_bounds: vec![[1.0, 1.0, 19.0, 9.0], [21.0, 1.0, 29.0, 9.0]],
            glyph_advances: vec![20.0, 10.0],
            glyph_bounds_sources: vec![BoundsSource::Tight, BoundsSource::Tight],
            bounds_source: BoundsSource::Tight,
            ..Default::default()
        };
        fold_span(&mut span);
        assert_eq!(span.text, "ffx");
        assert_eq!(
            span.tight_char_bounds,
            vec![
                [1.0, 1.0, 10.0, 9.0],
                [10.0, 1.0, 19.0, 9.0],
                [21.0, 1.0, 29.0, 9.0],
            ]
        );
        assert_eq!(span.glyph_advances, vec![10.0, 10.0, 10.0]);
        assert_eq!(
            span.glyph_bounds_sources,
            vec![
                BoundsSource::Tight,
                BoundsSource::Tight,
                BoundsSource::Tight
            ]
        );
        assert_eq!(span.bounds_source, BoundsSource::Tight);
    }

    /// A span with no ligature comes back as it came in: same text, same
    /// vectors, nothing rebuilt.
    #[test]
    fn a_span_without_a_ligature_is_untouched() {
        let mut span = TextSpan {
            text: "office hours".into(),
            char_bounds: vec![[0.0, 0.0, 10.0, 10.0]],
            tight_char_bounds: vec![[0.0, 0.0, 9.0, 9.0]],
            glyph_advances: vec![10.0],
            glyph_bounds_sources: vec![BoundsSource::Advance],
            ..Default::default()
        };
        let before = span.clone();
        fold_span(&mut span);
        assert_eq!(span.text, before.text);
        assert_eq!(span.char_bounds, before.char_bounds);
        assert_eq!(span.tight_char_bounds, before.tight_char_bounds);
        assert_eq!(span.glyph_advances, before.glyph_advances);
        assert_eq!(span.glyph_bounds_sources, before.glyph_bounds_sources);
    }

    /// A span with no boxes still folds its text: placed text and synthetic
    /// markers carry no per-glyph boxes, and the letters do not need them.
    #[test]
    fn text_without_boxes_still_folds() {
        let mut span = TextSpan {
            text: "o\u{FB03}ce".into(),
            ..Default::default()
        };
        fold_span(&mut span);
        assert_eq!(span.text, "office");
        assert!(span.char_bounds.is_empty());
    }

    /// The paragraph fold covers the block's own text as well as its spans:
    /// the two are read side by side downstream, so they fold together.
    #[test]
    fn a_block_folds_its_text_and_its_spans() {
        let mut block = Block::new(
            crate::blocks::BlockKind::Paragraph,
            0,
            [0.0, 0.0, 60.0, 10.0],
            "o\u{FB03}ce".into(),
            0,
        );
        block.spans = vec![TextSpan {
            text: "o\u{FB03}ce".into(),
            char_bounds: vec![
                [0.0, 0.0, 10.0, 10.0],
                [10.0, 0.0, 40.0, 10.0],
                [40.0, 0.0, 50.0, 10.0],
                [50.0, 0.0, 60.0, 10.0],
            ],
            ..Default::default()
        }];
        fold_block(&mut block);
        assert_eq!(block.text, "office");
        assert_eq!(block.spans[0].text, "office");
        assert_eq!(block.spans[0].char_bounds.len(), 6);
    }
}
