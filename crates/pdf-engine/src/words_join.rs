// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! The rule that decides whether two adjacent pieces read as one word.
//!
//! One rule, three callers: the text assembly joins its pieces with it, the
//! word assembly joins its spans with it, and the emphasis marks find their
//! runs with it, so joining the words of a line with single spaces is always
//! the line's own text. Every caller checks for whitespace between the pieces
//! first and joins only when there is none.

/// Whether two adjacent pieces with no whitespace between them read as one word.
///
/// `gap` is the horizontal distance in points from the end of the left piece
/// to the start of the right piece -- negative where they overlap.
/// `baseline_drift` is the absolute difference of their baselines in points.
/// `em` is the larger of the two pieces' type sizes in points.
///
/// The pieces join when the gap is under 0.15 em and the drift under 0.3 em:
/// a real inter-word space parts them, and so does a superscript riding off
/// the baseline, while a kern split or a style change inside the word does not.
///
/// A page breaks one word across adjacent spans more often than it looks: a
/// style change inside the word, a kerning split written as two `Tj`, a
/// ligature fallback. Inter-word spaces in body type are around 0.25 em and
/// more, while kerning gaps are well under 0.1 em, so 0.15 em sits between a
/// real space and an intra-word split. A superscript rides a third of an em
/// off the baseline and more, while one line's drift stays under it, so
/// 0.3 em keeps `x2` two words without splitting a line that merely slopes.
pub fn pieces_join(gap: f64, baseline_drift: f64, em: f64) -> bool {
    gap < 0.15 * em && baseline_drift < 0.3 * em
}

/// Whether two neighbouring spans read as one word, whitespace included.
///
/// The boundary [`pieces_join`] draws, with the whitespace check every caller
/// owes it: a blank at either edge parts the spans whatever the geometry says.
/// The block rebuild and the emphasis marks share this, so a run the rebuild
/// joined is a run the marks still find.
pub(crate) fn spans_join(prev: &crate::text::TextSpan, curr: &crate::text::TextSpan) -> bool {
    let spaced = prev.text.chars().last().is_some_and(char::is_whitespace)
        || curr.text.chars().next().is_some_and(char::is_whitespace);
    !spaced
        && pieces_join(
            curr.x - (prev.x + prev.width),
            (curr.y - prev.y).abs(),
            prev.font_size.max(curr.font_size),
        )
}
