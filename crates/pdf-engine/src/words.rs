//! Word-level boxes for citations, highlighting and grounding.
//!
//! [`PdfDocument::extract_words`] is the `extract_words=True` of pymupdf4llm:
//! every word of the document with the box it was drawn in. It is built from
//! the blocks [`PdfDocument::extract_blocks`] returns -- same options, same
//! window -- splitting each span's text on whitespace and boxing each word
//! from the glyphs that drew it. One function, no second text pass.
//!
//! A word's box is the union of its glyphs' boxes turned into the page frame:
//! an upright word's boxes pass through untouched, a turned word's become the
//! axis-aligned envelope of its turned glyphs. Boxes are one per glyph while
//! characters count the spaces the text pass inserted too, so each span first
//! aligns its two: a space sitting in a gap between glyphs owns no box, a
//! drawn one owns its glyph's, and a glyph one character stands for keeps its
//! whole box however many letters it decodes to. A word the page split across
//! spans joins by the rule text joins by
//! ([`crate::words_join::pieces_join`]), so joining a line's words with single
//! spaces is the line's own text.
//!
//! [`PdfDocument::extract_words`]: crate::PdfDocument::extract_words
//! [`PdfDocument::extract_blocks`]: crate::PdfDocument::extract_blocks

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use crate::blocks::{Block, ExtractOptions};
use crate::text::TextSpan;

/// One word of a document's text, with the box it was drawn in.
///
/// Built from the blocks [`PdfDocument::extract_blocks`] returns, so a word
/// carries where its block sits as well as where it sits itself. Words come
/// in reading order, block by block and line by line.
///
/// [`PdfDocument::extract_blocks`]: crate::PdfDocument::extract_blocks
#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    /// 0-based page index the word was drawn on.
    pub page: usize,
    /// The word, without surrounding whitespace.
    pub text: String,
    /// Bounding box in PDF user space, `[x0, y0, x1, y1]` with y up and
    /// `y0 < y1`: the union of the word's glyph boxes, in the frame block
    /// boxes use.
    pub bbox: [f64; 4],
    /// Position of the word's block in the `Vec` `extract_blocks` returned
    /// beside these words -- the window's numbering, not the document's.
    ///
    /// [`Block::index`] keeps the document's own numbering when a window is
    /// asked for; this is the index into the answer the caller is holding,
    /// so `blocks[word.block]` is the block the word came from.
    pub block: usize,
    /// 0-based line within the block, in the order the block's lines read.
    pub line: usize,
    /// 0-based word within the line, in the order the line reads.
    pub word: usize,
    /// Baseline angle in degrees, counter-clockwise from +x; 0 upright.
    ///
    /// A word joined across spans takes the angle of its longer piece, the
    /// left one on a tie -- the same longest-piece rule as the style fields.
    pub dir: f64,
    /// Whether the word's span is set bold.
    pub bold: bool,
    /// Whether the word's span is set italic.
    pub italic: bool,
    /// Font size of the word's span, in points.
    pub font_size: f64,
}

fn union(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}

/// The box a span claims, from its origin and extent.
///
/// Glyph spans measure both; a span that was placed by a rectangle rather
/// than drawn carries the placement in the same fields. Where either extent
/// is missing the wire contract's fallback answers instead -- half an em per
/// character across, the type size up -- so the box is never degenerate.
fn span_box(span: &TextSpan, chars: usize) -> [f64; 4] {
    let width = if span.width > 0.0 {
        span.width
    } else {
        span.font_size.max(1.0) * 0.5 * chars as f64
    };
    let height = if span.height > 0.0 {
        span.height
    } else {
        span.font_size.max(1.0)
    };
    [span.x, span.y, span.x + width, span.y + height]
}

/// A span's baseline angle in degrees counter-clockwise from +x, with the unit
/// direction its baseline runs in.
///
/// The first column of the span's composed transform, normalised: `(1, 0)`
/// for upright text, `(0, 1)` for a 90-degree run. A span not built from a
/// glyph carries no transform and reads as upright, as does a degenerate one
/// with no direction to speak of.
fn span_angle(span: &TextSpan) -> (f64, (f64, f64)) {
    let Some(coeffs) = span.transform.as_ref() else {
        return (0.0, (1.0, 0.0));
    };
    let (a, b) = (coeffs[0], coeffs[1]);
    let length = (a * a + b * b).sqrt();
    if length <= f64::EPSILON {
        return (0.0, (1.0, 0.0));
    }
    let dir = b.atan2(a).to_degrees();
    (if dir == 0.0 { 0.0 } else { dir }, (a / length, b / length))
}

/// The axis-aligned envelope of one glyph box turned into the page frame.
///
/// A `char_bound` is an advance and a size hung off a page-space origin, not
/// the rectangle a turned glyph covers, so each corner is turned about the
/// origin by the span's baseline direction and the envelope is taken. Upright
/// text passes through untouched -- to the bit, by the early return rather
/// than by arithmetic that rounds to the same.
fn turned_box(bound: [f64; 4], direction: (f64, f64)) -> [f64; 4] {
    let (dx, dy) = direction;
    if dx == 1.0 && dy == 0.0 {
        return bound;
    }
    let (x0, y0) = (bound[0], bound[1]);
    let (advance, size) = (bound[2] - bound[0], bound[3] - bound[1]);
    let mut envelope: Option<[f64; 4]> = None;
    for (u, v) in [(0.0, 0.0), (advance, 0.0), (advance, size), (0.0, size)] {
        let corner = [x0 + u * dx - v * dy, y0 + u * dy + v * dx];
        envelope = Some(match envelope {
            Some(b) => [
                b[0].min(corner[0]),
                b[1].min(corner[1]),
                b[2].max(corner[0]),
                b[3].max(corner[1]),
            ],
            None => [corner[0], corner[1], corner[0], corner[1]],
        });
    }
    envelope.unwrap_or(bound)
}

/// How far along the span's baseline a point sits: page `x` for upright text,
/// the distance up the page for a 90-degree run.
fn along(x: f64, y: f64, direction: (f64, f64)) -> f64 {
    x * direction.0 + y * direction.1
}

/// The white between two glyphs along the baseline they share.
///
/// Each bound carries its glyph's advance (`bound[2] - bound[0]`), so where
/// the left glyph stops is known whatever angle it was drawn at. Negative
/// where the glyphs overlap.
fn gap_between(left: [f64; 4], right: [f64; 4], direction: (f64, f64)) -> f64 {
    along(right[0], right[1], direction)
        - (along(left[0], left[1], direction) + (left[2] - left[0]))
}

/// The word a whitespace-owned glyph joins, if any.
///
/// The proportional mapping leaves a glyph to whitespace both where the page
/// drew a space and where the text pass inserted one, and only the geometry
/// tells them apart: an inserted space sits in a gap between glyphs, a drawn
/// one sits on its own glyph with ink on both sides. So with more characters
/// than glyphs, a glyph whose owning space stands alone joins the word on its
/// left where the gap on its right breaks the line and the one on its left
/// does not -- the shared join rule, read both ways. Anywhere else the glyph
/// stays out: a drawn space must never widen the word beside it.
fn skipped_glyph_word(
    chars: &[char],
    word_of_char: &[Option<usize>],
    glyphs: &[[f64; 4]],
    index: usize,
    owner: usize,
    direction: (f64, f64),
    em: f64,
) -> Option<usize> {
    // NaN reaches no further: every comparison below is false for it, so the
    // glyph stays out -- the same answer as for a degenerate size.
    if em <= 0.0 || em.is_nan() || chars.len() <= glyphs.len() {
        return None;
    }
    if owner > 0 && chars[owner - 1].is_whitespace() {
        return None;
    }
    if owner + 1 < chars.len() && chars[owner + 1].is_whitespace() {
        return None;
    }
    if index == 0 || index + 1 >= glyphs.len() {
        return None;
    }
    let joins_left = crate::words_join::pieces_join(
        gap_between(glyphs[index - 1], glyphs[index], direction),
        0.0,
        em,
    );
    let breaks_right = !crate::words_join::pieces_join(
        gap_between(glyphs[index], glyphs[index + 1], direction),
        0.0,
        em,
    );
    if joins_left && breaks_right {
        (0..owner).rev().find_map(|k| word_of_char[k])
    } else {
        None
    }
}

/// The words of one span: each run of non-whitespace with its box.
///
/// Each glyph lands in exactly one word -- or in none, when it only carries
/// whitespace -- and a word's box is the union of its glyphs' boxes turned
/// into the page frame. A word the glyphs left out takes its proportional
/// slice of the span's own box, by character count, spaces included: the
/// slices tile the span left to right, so they lie inside it and never
/// overlap.
///
/// The alignment is positional. Boxes are one per glyph while characters
/// count the spaces the text pass inserted too, so character `c` of `n` maps
/// to glyph `c * g / n`, and a glyph one character stands for is that word's
/// however many letters it decodes to -- a ligature's letters keep its whole
/// box, and so does an `/ActualText` replacement. A glyph the mapping leaves
/// to whitespace is still a letter's where the page says so: with more
/// characters than glyphs, a lone space beside a gap between glyphs is the
/// inserted one, and the glyph on its other side belongs to the word on its
/// left ([`skipped_glyph_word`]).
fn words_of_span(span: &TextSpan) -> Vec<(String, [f64; 4])> {
    let chars: Vec<char> = span.text.chars().collect();
    let n = chars.len();
    if n == 0 {
        return Vec::new();
    }
    let glyphs = &span.char_bounds;
    let g = glyphs.len();
    let box_of_span = span_box(span, n);

    // Which word each character belongs to, whitespace being in none.
    let mut word_of_char: Vec<Option<usize>> = Vec::with_capacity(n);
    let mut texts: Vec<String> = Vec::new();
    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut in_word = false;
    for (i, ch) in chars.iter().enumerate() {
        if ch.is_whitespace() {
            in_word = false;
            word_of_char.push(None);
        } else {
            if !in_word {
                texts.push(String::new());
                ranges.push((i, i));
                in_word = true;
            }
            let w = texts.len() - 1;
            texts[w].push(*ch);
            ranges[w].1 = i + 1;
            word_of_char.push(Some(w));
        }
    }
    if texts.is_empty() {
        return Vec::new();
    }

    let (_, direction) = span_angle(span);
    let mut boxes: Vec<Option<[f64; 4]>> = vec![None; texts.len()];
    // The character a glyph comes from: the first character mapping to it, or
    // the nearest one before it where the mapping skips it, which is what a
    // span with more glyphs than characters does.
    for (index, glyph_box) in glyphs.iter().enumerate() {
        let owner = (0..n)
            .find(|&c| c * g / n == index)
            .or_else(|| (0..n).rev().find(|&c| c * g / n <= index));
        let Some(c) = owner else { continue };
        // A glyph shared by a whitespace character and a word character
        // belongs to the word: look forward inside the glyph's characters
        // for one.
        let word = (c..n)
            .take_while(|&k| k * g / n == index || k == c)
            .find_map(|k| word_of_char[k])
            .or_else(|| {
                skipped_glyph_word(
                    &chars,
                    &word_of_char,
                    glyphs,
                    index,
                    c,
                    direction,
                    span.font_size,
                )
            });
        if let Some(w) = word {
            let envelope = turned_box(*glyph_box, direction);
            boxes[w] = Some(match boxes[w] {
                Some(b) => union(b, envelope),
                None => envelope,
            });
        }
    }

    let total = n as f64;
    texts
        .into_iter()
        .zip(ranges)
        .zip(boxes)
        .map(|((text, (start, end)), bbox)| {
            let bbox = bbox.unwrap_or_else(|| {
                let left =
                    box_of_span[0] + (box_of_span[2] - box_of_span[0]) * start as f64 / total;
                let right = if end == n {
                    box_of_span[2]
                } else {
                    box_of_span[0] + (box_of_span[2] - box_of_span[0]) * end as f64 / total
                };
                [left, box_of_span[1], right, box_of_span[3]]
            });
            (text, bbox)
        })
        .collect()
}

/// Whether `text` opens with whitespace.
fn starts_with_whitespace(text: &str) -> bool {
    text.chars().next().is_some_and(|c| c.is_whitespace())
}

/// Whether `text` closes with whitespace.
fn ends_with_whitespace(text: &str) -> bool {
    text.chars().last().is_some_and(|c| c.is_whitespace())
}

/// The rightmost piece so far: what the next span's first piece may join.
struct Tail {
    /// Position in the line of the span that drew the last piece.
    span_pos: usize,
    /// Baseline of that span.
    y: f64,
    /// Font size of that span.
    font_size: f64,
    /// Characters of the piece that lent the last word its style and angle.
    style_len: usize,
}

/// Every word of one line, joining a word the page split across spans.
///
/// Each span is cut into whitespace-separated pieces first; then the last
/// piece of a span and the first piece of the next join when the spans
/// themselves carry no space between them and the shared rule
/// ([`crate::words_join::pieces_join`]) joins them. The joined word's text is
/// the concatenation and its box the union; `bold`, `italic`, `font_size`
/// and `dir` come from the piece with more characters, the left one on a tie.
fn words_of_line(
    block: &Block,
    block_index: usize,
    line_index: usize,
    line: &[usize],
) -> Vec<Word> {
    let mut out: Vec<Word> = Vec::new();
    let mut tail: Option<Tail> = None;
    let mut prev_ends_with_space = false;
    for (pos, span_index) in line.iter().enumerate() {
        let span = &block.spans[*span_index];
        let (span_dir, _) = span_angle(span);
        let starts_with_space = starts_with_whitespace(&span.text);
        for (piece_index, (text, bbox)) in words_of_span(span).iter().enumerate() {
            let join = piece_index == 0
                && tail.as_ref().is_some_and(|t| {
                    let Some(last) = out.last() else {
                        return false;
                    };
                    if t.span_pos + 1 != pos || prev_ends_with_space || starts_with_space {
                        return false;
                    }
                    let em = t.font_size.max(span.font_size);
                    let gap = bbox[0] - last.bbox[2];
                    crate::words_join::pieces_join(gap, (t.y - span.y).abs(), em)
                });
            let len = text.chars().count();
            if join {
                let last = out.last_mut().expect("join implies a word");
                last.text.push_str(text);
                last.bbox = union(last.bbox, *bbox);
                if let Some(t) = tail.as_mut() {
                    if len > t.style_len {
                        last.bold = span.is_bold;
                        last.italic = span.is_italic;
                        last.font_size = span.font_size;
                        last.dir = span_dir;
                        t.style_len = len;
                    }
                    t.span_pos = pos;
                    t.y = span.y;
                    t.font_size = span.font_size;
                }
            } else {
                out.push(Word {
                    page: block.page,
                    text: text.clone(),
                    bbox: *bbox,
                    block: block_index,
                    line: line_index,
                    word: 0,
                    dir: span_dir,
                    bold: span.is_bold,
                    italic: span.is_italic,
                    font_size: span.font_size,
                });
                tail = Some(Tail {
                    span_pos: pos,
                    y: span.y,
                    font_size: span.font_size,
                    style_len: len,
                });
            }
        }
        prev_ends_with_space = ends_with_whitespace(&span.text);
    }
    for (index, word) in out.iter_mut().enumerate() {
        word.word = index;
    }
    out
}

/// Every word of `blocks`, block by block and line by line.
///
/// `block` on each word is the position in `blocks` -- the window's
/// numbering -- and `line` the line within its block. The lines are the same
/// grouping the block's own readers use, top first, each line left to right.
pub(crate) fn words_from_blocks(blocks: &[Block]) -> Vec<Word> {
    let mut out = Vec::new();
    for (block_index, block) in blocks.iter().enumerate() {
        for (line_index, line) in crate::text::lines_of_block(block).iter().enumerate() {
            out.extend(words_of_line(block, block_index, line_index, line));
        }
    }
    out
}

impl ExtractOptions {
    /// Carry the word boxes beside the blocks. Default off: [`crate::to_json`]
    /// writes exactly what it wrote before this option existed.
    #[must_use]
    pub fn with_word_boxes(mut self, word_boxes: bool) -> Self {
        self.word_boxes = word_boxes;
        self
    }
}

/// The blocks as JSON, each block carrying the lines it groups.
///
/// Beside every field [`crate::to_json`] writes, each block carries
/// `"lines": [{"spans": [...], "words": [{"text", "bbox", "word", "dir"}]}]`.
/// Strip the added key and the document is [`crate::to_json`]'s, byte for byte.
pub fn to_json_with_words(blocks: &[Block]) -> String {
    use crate::json::INDENT;
    if blocks.is_empty() {
        return format!("{{\n{INDENT}\"blocks\": []\n}}\n");
    }
    let pad = INDENT.repeat(2);
    let inner = INDENT.repeat(3);
    let written: Vec<String> = blocks
        .iter()
        .enumerate()
        .map(|(index, block)| {
            let plain = crate::json::block(block, 2);
            let suffix = format!("\n{pad}}}");
            let stem = plain.strip_suffix(&suffix).expect(
                "to_json_with_words: the block object does not close as json::block writes it",
            );
            format!(
                "{stem},\n{inner}\"lines\": {}{suffix}",
                lines_json(block, index)
            )
        })
        .collect();
    format!(
        "{{\n{INDENT}\"blocks\": [\n{}\n{INDENT}]\n}}\n",
        written.join(",\n")
    )
}

/// One block's lines with their words, as the `"lines"` value of
/// [`to_json_with_words`]: `[{"spans": [...], "words": [...]}]`.
///
/// `spans` names the block spans the line groups, by position; each word
/// carries `text`, `bbox`, `word` and `dir`. One line object per line, like
/// the one-line span records of [`crate::to_json`].
fn lines_json(block: &Block, block_index: usize) -> String {
    use crate::json::{number, string, INDENT};
    let inner = INDENT.repeat(3);
    let pad = INDENT.repeat(4);
    let mut lines = Vec::new();
    for (line_index, line) in crate::text::lines_of_block(block).iter().enumerate() {
        let words = words_of_line(block, block_index, line_index, line);
        let spans = line
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        let mut entries = Vec::with_capacity(words.len());
        for word in &words {
            let bbox = word
                .bbox
                .iter()
                .map(|v| number(*v))
                .collect::<Vec<_>>()
                .join(", ");
            entries.push(format!(
                "{{\"text\": {}, \"bbox\": [{bbox}], \"word\": {}, \"dir\": {}}}",
                string(&word.text),
                word.word,
                number(word.dir)
            ));
        }
        lines.push(format!(
            "{pad}{{\"spans\": [{spans}], \"words\": [{}]}}",
            entries.join(", ")
        ));
    }
    if lines.is_empty() {
        "[]".to_string()
    } else {
        format!("[\n{}\n{inner}]", lines.join(",\n"))
    }
}

/// The words as JSON lines, one object per word.
///
/// Each line is `{"page", "block", "line", "word", "text", "bbox", "dir"}` in
/// the order [`crate::PdfDocument::extract_words`] hands the words back.
pub fn to_words_json_lines(words: &[Word]) -> String {
    use crate::json::{number, string};
    let mut out = String::new();
    for word in words {
        let bbox = word
            .bbox
            .iter()
            .map(|v| number(*v))
            .collect::<Vec<_>>()
            .join(", ");
        out.push_str(&format!(
            "{{\"page\": {}, \"block\": {}, \"line\": {}, \"word\": {}, \"text\": {}, \"bbox\": [{bbox}], \"dir\": {}}}\n",
            word.page,
            word.block,
            word.line,
            word.word,
            string(&word.text),
            number(word.dir)
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blocks::BlockKind;

    fn block_of(spans: Vec<TextSpan>) -> Block {
        let mut block = Block::new(
            BlockKind::Paragraph,
            0,
            [0.0, 0.0, 100.0, 100.0],
            String::new(),
            0,
        );
        block.spans = spans;
        block
    }

    fn span_with(text: &str, x: f64, y: f64, glyph_width: f64, size: f64) -> TextSpan {
        let mut span = TextSpan {
            text: text.to_string(),
            x,
            y,
            width: glyph_width * text.chars().count() as f64,
            height: size,
            font_size: size,
            ..Default::default()
        };
        let mut cursor = x;
        for _ in text.chars() {
            span.char_bounds
                .push([cursor, y, cursor + glyph_width, y + size]);
            cursor += glyph_width;
        }
        span
    }

    /// A span drawn at an angle, baseline `degrees` counter-clockwise from +x.
    ///
    /// The boxes stay in the device's own form -- a page-space origin with an
    /// advance and a size hung off it -- so the word pass turns them exactly
    /// as it turns a span the interpreter drew.
    fn turned_span(
        text: &str,
        x: f64,
        y: f64,
        glyph_width: f64,
        size: f64,
        degrees: f64,
    ) -> TextSpan {
        let mut span = span_with(text, x, y, glyph_width, size);
        let (sin, cos) = degrees.to_radians().sin_cos();
        span.transform = Some([cos, sin, -sin, cos, x, y]);
        span
    }

    #[test]
    fn a_word_takes_the_union_of_its_glyphs() {
        let span = span_with("hello world", 10.0, 20.0, 6.0, 12.0);
        let words = words_of_span(&span);
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].0, "hello");
        assert_eq!(words[1].0, "world");
        assert_eq!(words[0].1, [10.0, 20.0, 40.0, 32.0]);
        assert_eq!(words[1].1, [46.0, 20.0, 76.0, 32.0]);
    }

    #[test]
    fn a_span_without_boxes_splits_proportionally() {
        let span = TextSpan {
            text: "ab cd".to_string(),
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 10.0,
            font_size: 10.0,
            ..Default::default()
        };
        assert!(span.char_bounds.is_empty());
        let words = words_of_span(&span);
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].1, [0.0, 0.0, 40.0, 10.0]);
        assert_eq!(words[1].1, [60.0, 0.0, 100.0, 10.0]);
    }

    #[test]
    fn an_inserted_space_owns_no_glyph() {
        // Two glyphs, three characters: the space was inserted by the
        // consensus and the letters keep their own boxes.
        let mut span = span_with("ab", 0.0, 0.0, 10.0, 10.0);
        span.text = "a b".to_string();
        span.width = 30.0;
        let words = words_of_span(&span);
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].1, [0.0, 0.0, 10.0, 10.0]);
        assert_eq!(words[1].1, [10.0, 0.0, 20.0, 10.0]);
    }

    #[test]
    fn two_touching_spans_join_into_one_word() {
        // "Hel" in bold 12pt, "lo" in roman 10pt, boxes touching: one word
        // "Hello" with the union box and the longer piece's style. Take the
        // join out and this comes back as two words.
        let mut hel = span_with("Hel", 0.0, 20.0, 6.0, 12.0);
        hel.is_bold = true;
        let lo = span_with("lo", 18.0, 20.0, 6.0, 10.0);
        let words = words_from_blocks(&[block_of(vec![hel, lo])]);
        assert_eq!(words.len(), 1, "the split word did not join: {words:?}");
        assert_eq!(words[0].text, "Hello");
        assert_eq!(words[0].bbox, [0.0, 20.0, 30.0, 32.0]);
        assert!(words[0].bold, "the longer piece lends the style");
        assert_eq!(words[0].font_size, 12.0);
    }

    #[test]
    fn two_spans_a_word_space_apart_stay_two_words() {
        // 0.4 em at 12pt: a real inter-word space, past the 0.15 em a join
        // allows. Both pieces sit on one line, so only the gap refuses.
        let hel = span_with("Hel", 0.0, 20.0, 6.0, 12.0);
        let lo = span_with("lo", 18.0 + 0.4 * 12.0, 20.0, 6.0, 12.0);
        let words = words_from_blocks(&[block_of(vec![hel, lo])]);
        assert_eq!(words.len(), 2, "the space did not hold: {words:?}");
        assert_eq!(words[0].text, "Hel");
        assert_eq!(words[1].text, "lo");
        assert_eq!(words[0].line, words[1].line, "not on one line");
    }

    #[test]
    fn a_superscript_right_after_its_base_stays_its_own_word() {
        // The "2" touches "x" but rides 0.4 em higher: past the 0.3 em a
        // join allows, yet inside the half em that keeps one line one line.
        let x = span_with("x", 0.0, 20.0, 6.0, 10.0);
        let two = span_with("2", 6.0, 24.0, 6.0, 10.0);
        let words = words_from_blocks(&[block_of(vec![x, two])]);
        assert_eq!(words.len(), 2, "the superscript joined: {words:?}");
        assert_eq!(words[0].text, "x");
        assert_eq!(words[1].text, "2");
        assert_eq!(words[0].line, words[1].line, "not on one line");
    }

    #[test]
    fn a_tied_join_keeps_the_left_pieces_style() {
        // Two characters each, boxes touching: one word with the left
        // piece's bold, size and angle. Take the angle with the style and
        // this comes back 0.
        let mut ab = turned_span("ab", 0.0, 20.0, 6.0, 10.0, 10.0);
        ab.is_bold = true;
        let cd = span_with("cd", 12.0, 20.0, 6.0, 14.0);
        let words = words_from_blocks(&[block_of(vec![ab, cd])]);
        assert_eq!(words.len(), 1, "the split word did not join: {words:?}");
        assert_eq!(words[0].text, "abcd");
        assert!(words[0].bold, "the tie went right");
        assert_eq!(words[0].font_size, 10.0);
        assert!(
            (words[0].dir - 10.0).abs() < 1e-9,
            "the tie went right: {}",
            words[0].dir
        );
    }

    #[test]
    fn a_join_takes_the_longer_pieces_angle() {
        // Three characters at ten degrees, two upright: the joined word reads
        // at the longer piece's angle. Take the angle with the style and this
        // comes back 0.
        let hel = turned_span("Hel", 0.0, 20.0, 6.0, 12.0, 10.0);
        let lo = span_with("lo", 18.0, 20.0, 6.0, 12.0);
        let words = words_from_blocks(&[block_of(vec![hel, lo])]);
        assert_eq!(words.len(), 1, "the split word did not join: {words:?}");
        assert_eq!(words[0].text, "Hello");
        assert!(
            (words[0].dir - 10.0).abs() < 1e-9,
            "the angle went right: {}",
            words[0].dir
        );
    }

    #[test]
    fn a_join_takes_the_longer_pieces_angle_from_the_right() {
        // Two characters at ten degrees, four upright: length beats position
        // and the joined word reads upright. Take the left piece's angle and
        // this comes back 10.
        let ab = turned_span("ab", 0.0, 20.0, 6.0, 12.0, 10.0);
        let cdef = span_with("cdef", 12.0, 20.0, 6.0, 12.0);
        let words = words_from_blocks(&[block_of(vec![ab, cdef])]);
        assert_eq!(words.len(), 1, "the split word did not join: {words:?}");
        assert_eq!(words[0].text, "abcdef");
        assert_eq!(words[0].dir, 0.0, "length lost to position");
    }

    #[test]
    fn a_space_at_the_span_boundary_blocks_the_join() {
        // The pieces touch, but the left span ends in a space: two words.
        let hel = span_with("Hel ", 0.0, 20.0, 6.0, 12.0);
        let lo = span_with("lo", 18.0, 20.0, 6.0, 12.0);
        let words = words_from_blocks(&[block_of(vec![hel, lo])]);
        assert_eq!(words.len(), 2, "the boundary space did not hold: {words:?}");
        assert_eq!(words[0].text, "Hel");
        assert_eq!(words[1].text, "lo");
    }
}
