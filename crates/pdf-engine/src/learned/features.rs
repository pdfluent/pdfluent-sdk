//! What the learned decisions read about a page (#598, #599).
//!
//! ONE COMPUTATION FOR THE DUMP AND FOR THE ENGINE. The models are trained on
//! the records `pdf-learned`'s `feature_dump` writes, and they run inside
//! [`crate::PdfDocument::extract_blocks`]. If the two computed their features
//! apart, every difference between them -- a rounding, an order, a missing
//! neighbour -- would be a model reading input it was never trained on, and
//! it would look exactly like a weak model. So the features are computed here,
//! once, from the blocks `extract_blocks` has made, and `feature_dump`
//! serialises this struct instead of computing its own.
//!
//! Nothing here decides anything. It measures the blocks our heuristics made
//! and the words inside them; [`super::refine`] asks the models.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use crate::blocks::{Block, BlockKind};
use crate::text::TextSpan;

/// A word of a span: its text and the union of its glyph boxes.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub struct SpanWord {
    /// The word, without surrounding whitespace.
    pub text: String,
    /// `[x0, y0, x1, y1]` in PDF user space, y up, the union of its glyph boxes.
    pub bbox: [f64; 4],
}

/// A word of a page: a [`SpanWord`] with the block and span it came from and
/// the type it is set in.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub struct PageWord {
    /// The word.
    pub text: String,
    /// `[x0, y0, x1, y1]` in PDF user space, y up.
    pub bbox: [f64; 4],
    /// Index of the block on the page, in the order the page's blocks came.
    pub block: usize,
    /// Index of the span inside that block's `spans`.
    pub span: usize,
    /// The span's font size.
    pub size: f64,
    /// Whether the span is set bold.
    pub bold: bool,
    /// Whether the span is set italic.
    pub italic: bool,
    /// The span's font, its PostScript name without a subset prefix.
    pub font: Option<String>,
    /// The span's fill colour, RGB; `None` for a pattern fill.
    pub color: Option<[u8; 3]>,
}

/// What one block measures, in the fields of schema `pdfluent.features/1`.
///
/// `None` is a neighbour the block does not have (the first block has none
/// above it) or a value only the whole document knows; it reaches a model as
/// NaN, which LightGBM and the exported code route the same way.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub struct BlockFeatures {
    pub kind: &'static str,
    pub level: Option<u32>,
    pub n_chars: usize,
    pub n_words: usize,
    pub n_lines: usize,
    pub size: f64,
    pub size_ratio: f64,
    pub bold: f64,
    pub italic: f64,
    pub all_caps: bool,
    pub font: Option<String>,
    pub x_frac: f64,
    pub width_frac: f64,
    pub y_frac: f64,
    pub gap_above_em: f64,
    pub gap_below_em: f64,
    pub ends_terminal: bool,
    pub ends_colon: bool,
    pub marker: Option<&'static str>,
    pub caption: Option<&'static str>,
    pub section_number: bool,
    pub prev_kind: Option<&'static str>,
    pub next_kind: Option<&'static str>,
    pub prev_ends_terminal: Option<bool>,
    pub prev_size_ratio: Option<f64>,
    pub next_x_frac: Option<f64>,
    pub next_size_ratio: Option<f64>,
    /// On how many pages of the document this block's normalised text occurs.
    /// Filled by [`fill_repeats`], which sees the whole document.
    pub repeats_in_document: Option<u32>,
    pub first_on_page: bool,
    pub last_on_page: bool,
    /// Whether the words are capitalised the way a title is, and not shouted:
    /// `Significant Accounting Policies` yes, `THE LOUD ONE` no.
    pub title_case: bool,
    /// How the block is numbered, if it is: 1 arabic (`1.`, `1.2.3`), 2 roman
    /// (`IV.`), 3 a letter (`A)`), 0 not at all.
    pub number_kind: u8,
    /// How deep that number goes: `1.2.3` is three, `IV.` and `A)` are one.
    pub number_depth: u8,
    /// Whether the text ends in a comma, which a heading does not.
    pub ends_comma: bool,
    /// The block's width against the COLUMN's measure rather than the page's:
    /// the median width of the page's paragraphs. A heading stops short of the
    /// column, and the column is not the page (#584). Measured the strongest
    /// of the six on the whole training split.
    pub width_vs_column: f64,
    /// Whether the block's face is the document's body face. Filled from
    /// [`DocumentFacts`], like `repeats_in_document`, because a document's
    /// body face is a fact about the document.
    pub font_is_the_body_face: Option<bool>,
}

/// One page as the learned decisions see it.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq)]
pub struct PageFeatures {
    /// The page's median glyph size.
    pub body_size: f64,
    /// One entry per block, in the order the page's blocks were given.
    pub blocks: Vec<BlockFeatures>,
    /// Every word of every block, block by block, span by span.
    pub words: Vec<PageWord>,
}

/// The block feature vector, in the order the block models take it.
///
/// The same list and order as `scripts/extraction/learned/features.py`
/// (`BLOCK_FEATURES`); the parity test holds the two together.
#[doc(hidden)]
pub const BLOCK_FEATURES: [&str; 30] = [
    "size_ratio",
    "bold",
    "italic",
    "all_caps",
    "n_chars",
    "n_words",
    "n_lines",
    "x_frac",
    "width_frac",
    "y_frac",
    "gap_above_em",
    "gap_below_em",
    "ends_terminal",
    "ends_colon",
    "section_number",
    "has_marker",
    "has_caption",
    "first_on_page",
    "last_on_page",
    "prev_ends_terminal",
    "prev_size_ratio",
    "next_x_frac",
    "next_size_ratio",
    "repeats_in_document",
    // The six the heading lane added (#599), appended so that a model trained
    // on the first twenty-four keeps its indices. Measured on the whole
    // training split: these six take the heading axis from 0.783 to 0.797 and
    // every class with it, while the five DOCUMENT-WIDE ones tried beside them
    // (how many sizes stand above the block, how many blocks share its type)
    // cost 0.139 -- a split by document lets a tree use them to recognise the
    // document rather than the block.
    "font_is_the_body_face",
    "title_case",
    "number_kind",
    "number_depth",
    "width_vs_column",
    "ends_comma",
];

fn flag(value: bool) -> f64 {
    if value {
        1.0
    } else {
        0.0
    }
}

fn opt(value: Option<f64>) -> f64 {
    value.unwrap_or(f64::NAN)
}

/// One block's vector, in [`BLOCK_FEATURES`] order.
#[doc(hidden)]
pub fn block_vector(b: &BlockFeatures) -> [f64; 30] {
    [
        b.size_ratio,
        b.bold,
        b.italic,
        flag(b.all_caps),
        b.n_chars as f64,
        b.n_words as f64,
        b.n_lines as f64,
        b.x_frac,
        b.width_frac,
        b.y_frac,
        b.gap_above_em,
        b.gap_below_em,
        flag(b.ends_terminal),
        flag(b.ends_colon),
        flag(b.section_number),
        flag(b.marker.is_some()),
        flag(b.caption.is_some()),
        flag(b.first_on_page),
        flag(b.last_on_page),
        opt(b.prev_ends_terminal.map(flag)),
        opt(b.prev_size_ratio),
        opt(b.next_x_frac),
        opt(b.next_size_ratio),
        opt(b.repeats_in_document.map(f64::from)),
        opt(b.font_is_the_body_face.map(flag)),
        flag(b.title_case),
        f64::from(b.number_kind),
        f64::from(b.number_depth),
        b.width_vs_column,
        flag(b.ends_comma),
    ]
}

/// Whether a block's words are capitalised the way a title is (#599).
///
/// Six in ten words opening with a capital, at least two words, and not the
/// whole line shouted: `all_caps` already says that, and a rule that counted
/// capitals would call every running head a title.
pub(crate) fn title_case(text: &str) -> bool {
    let words: Vec<&str> = text
        .split_whitespace()
        .filter(|word| word.chars().any(char::is_alphabetic))
        .collect();
    if words.len() < 2
        || text
            .chars()
            .filter(|c| c.is_alphabetic())
            .all(char::is_uppercase)
    {
        return false;
    }
    let capitalised = words
        .iter()
        .filter(|word| word.chars().next().is_some_and(char::is_uppercase))
        .count();
    capitalised * 10 >= words.len() * 6
}

/// How a block is numbered, and how deep: `1.2.3` is (1, 3), `IV.` is (2, 1),
/// `A)` is (3, 1), anything else (0, 0) (#599).
///
/// The kind is kept apart from the depth because `A` and `1` mean different
/// things at one depth: a letter is usually a subsection and a roman numeral a
/// part of the front matter.
pub(crate) fn numbering(text: &str) -> (u8, u8) {
    // THE SAME THREE SHAPES PYTHON MATCHES, and the same refusals, because the
    // model was trained on Python's answer: an arabic number with optional
    // `.`-parts and an optional `.` or `)`, a roman numeral with one of those
    // delimiters, or a single letter with one. In every case SOMETHING has to
    // follow: `18.90` on its own is a page number or a table cell, not a
    // numbered heading, and the parity test caught this one exactly there.
    let head = text.trim_start();
    let (first, rest) = match head.find(char::is_whitespace) {
        Some(at) => (&head[..at], &head[at..]),
        None => return (0, 0),
    };
    if first.is_empty() || rest.is_empty() {
        return (0, 0);
    }
    let body = first
        .strip_suffix('.')
        .or_else(|| first.strip_suffix(')'))
        .unwrap_or(first);
    let delimited = body.len() < first.len();
    if !body.is_empty()
        && body.chars().all(|c| c.is_ascii_digit() || c == '.')
        && body.chars().next().is_some_and(|c| c.is_ascii_digit())
        && !body.ends_with('.')
    {
        let depth = body.split('.').filter(|part| !part.is_empty()).count();
        return (1, depth.min(u8::MAX as usize) as u8);
    }
    if delimited
        && !body.is_empty()
        && body
            .chars()
            .all(|c| matches!(c, 'I' | 'V' | 'X' | 'L' | 'C' | 'D' | 'M'))
    {
        return (2, 1);
    }
    // ASCII, because the training side is `[A-Za-z]`. `is_alphabetic` also
    // takes 'É', so `É. Résumé` had number_kind 3 in the engine and 0 in its
    // training vector -- a feature the heading model splits on, reading one
    // thing here and another there (codex, #1983).
    if delimited && body.chars().count() == 1 && body.chars().all(|c| c.is_ascii_alphabetic()) {
        return (3, 1);
    }
    (0, 0)
}

fn union(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}

/// The words of one span, boxed from `char_bounds`.
///
/// `char_bounds` holds one box per SOURCE glyph, not per char: a ligature or a
/// `bfrange` mapped to a string is one glyph and several chars. When the two
/// counts agree the mapping is one to one. When they do not, char `c` of `n`
/// maps to glyph `c * g / n`, and every glyph is then given to the word of the
/// char it maps from -- so each glyph lands in exactly one word (or in none,
/// when it only carries whitespace) and the union of a span's word boxes is
/// the union of its non-whitespace glyph boxes. A span with no glyph boxes
/// gives its words the span's own box.
#[doc(hidden)]
pub fn words_of(span: &TextSpan) -> Vec<SpanWord> {
    let chars: Vec<char> = span.text.chars().collect();
    let n = chars.len();
    let glyphs = &span.char_bounds;
    let g = glyphs.len();
    let span_box = [span.x, span.y, span.x + span.width, span.y + span.height];

    // Which word each char belongs to, whitespace being in none.
    let mut word_of_char: Vec<Option<usize>> = Vec::with_capacity(n);
    let mut texts: Vec<String> = Vec::new();
    let mut in_word = false;
    for &c in &chars {
        if c.is_whitespace() {
            in_word = false;
            word_of_char.push(None);
        } else {
            if !in_word {
                texts.push(String::new());
                in_word = true;
            }
            let w = texts.len() - 1;
            texts[w].push(c);
            word_of_char.push(Some(w));
        }
    }
    if texts.is_empty() {
        return Vec::new();
    }

    let mut boxes: Vec<Option<[f64; 4]>> = vec![None; texts.len()];
    if g == 0 || n == 0 {
        boxes.fill(Some(span_box));
    } else {
        // The char a glyph comes from: the first char mapping to it.
        for (glyph_index, glyph_box) in glyphs.iter().enumerate() {
            let owner = (0..n)
                .find(|&c| c * g / n == glyph_index)
                .or_else(|| (0..n).rev().find(|&c| c * g / n <= glyph_index));
            let Some(c) = owner else { continue };
            // A glyph shared by a whitespace char and a word char belongs to
            // the word: look forward inside the glyph's chars for one.
            let word = (c..n)
                .take_while(|&k| k * g / n == glyph_index || k == c)
                .find_map(|k| word_of_char[k]);
            if let Some(w) = word {
                boxes[w] = Some(match boxes[w] {
                    Some(b) => union(b, *glyph_box),
                    None => *glyph_box,
                });
            }
        }
    }

    texts
        .into_iter()
        .zip(boxes)
        .map(|(text, bbox)| SpanWord {
            text,
            bbox: bbox.unwrap_or(span_box),
        })
        .collect()
}

/// The name a block kind has in the dump.
#[doc(hidden)]
pub fn kind_name(kind: &BlockKind) -> (&'static str, Option<u32>) {
    match kind {
        BlockKind::Paragraph => ("paragraph", None),
        BlockKind::Heading { level } => ("heading", Some(u32::from(*level))),
        BlockKind::ListItem { level, .. } => ("list_item", Some(u32::from(*level))),
        BlockKind::TableCell { .. } => ("table_cell", None),
        BlockKind::Header => ("header", None),
        BlockKind::Footer => ("footer", None),
        BlockKind::Caption => ("caption", None),
        // Unreachable today, and there so that a kind added to `BlockKind`
        // lands in the dump as `other` instead of breaking the build of the
        // one place that names every kind.
        #[allow(unreachable_patterns)]
        _ => ("other", None),
    }
}

fn median(mut v: Vec<f64>) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

/// One entry per glyph (or per span when it has no glyph boxes), as the
/// engine's own dominant-size rule counts.
fn glyph_sizes<'a>(spans: impl Iterator<Item = &'a TextSpan>) -> Vec<f64> {
    spans
        .flat_map(|s| {
            let k = s.char_bounds.len().max(1);
            std::iter::repeat_n(s.font_size, k)
        })
        .collect()
}

fn glyph_fraction(spans: &[TextSpan], pick: impl Fn(&TextSpan) -> bool) -> f64 {
    let (mut yes, mut all) = (0usize, 0usize);
    for s in spans {
        let k = s.char_bounds.len().max(1);
        all += k;
        if pick(s) {
            yes += k;
        }
    }
    if all == 0 {
        0.0
    } else {
        yes as f64 / all as f64
    }
}

fn marker(text: &str) -> Option<&'static str> {
    let t = text.trim_start();
    let first = t.split_whitespace().next()?;
    if matches!(
        first,
        "\u{2022}" | "-" | "\u{2013}" | "\u{2014}" | "*" | "\u{b7}" | "\u{25aa}" | "\u{25e6}"
    ) {
        return Some("bullet");
    }
    let body = first.trim_end_matches(['.', ')', ':']);
    if body != first && !body.is_empty() {
        if body.chars().all(|c| c.is_ascii_digit()) {
            return Some("number");
        }
        if body.len() == 1 && body.chars().all(|c| c.is_ascii_alphabetic()) {
            return Some("letter");
        }
    }
    None
}

fn caption(text: &str) -> Option<&'static str> {
    let mut words = text.split_whitespace();
    let first = words.next()?;
    let second = words.next().unwrap_or("");
    let numbered = second.chars().next().is_some_and(|c| c.is_ascii_digit());
    match first {
        "Table" if numbered => Some("table"),
        "Figure" | "Fig." if numbered => Some("figure"),
        _ => None,
    }
}

fn section_number(text: &str) -> bool {
    let Some(first) = text.split_whitespace().next() else {
        return false;
    };
    let body = first.trim_end_matches('.');
    body.contains('.')
        && body
            .split('.')
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

// How many times `page_features` ran on this thread: the speed round's
// counter seam (T1). `#[cfg(test)]`-only and thread-local, like the table
// model's threshold override, so serving has no counter and one test's count
// never leaks into another test on another thread. The learned pass must
// measure a page once and pass the measurement along; the tests hold it to
// one call per page per block snapshot.
#[cfg(test)]
thread_local! {
    static PAGE_FEATURES_CALLS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// How many times `page_features` ran on this thread since the last reset.
#[cfg(test)]
pub fn page_features_calls() -> u64 {
    PAGE_FEATURES_CALLS.with(|calls| calls.get())
}

/// Forget how many times `page_features` ran on this thread.
#[cfg(test)]
pub fn reset_page_features_calls() {
    PAGE_FEATURES_CALLS.with(|calls| calls.set(0));
}

/// One page's blocks, measured.
///
/// `blocks` are the page's blocks in the order the page's reading order gives
/// them, every kind included -- a `paragraph` is our own verdict, not the
/// absence of one. `width` and `height` are the page's size in points.
#[doc(hidden)]
pub fn page_features(blocks: &[&Block], width: f64, height: f64) -> PageFeatures {
    #[cfg(test)]
    PAGE_FEATURES_CALLS.with(|calls| calls.set(calls.get() + 1));
    let body = median(glyph_sizes(blocks.iter().flat_map(|b| b.spans.iter())));
    let n = blocks.len();

    // What a neighbour IS, not what we decided it is (T3, #599): prev_kind and
    // next_kind are our own classifier's verdicts, and a model that reads them
    // learns to copy the heuristic it replaces. These are the facts the
    // hand-set rules of #584 read -- "the block above is finished", "the one
    // below starts at this left edge".
    let sizes: Vec<f64> = blocks
        .iter()
        .map(|b| median(glyph_sizes(b.spans.iter())))
        .collect();
    let ratio = |i: usize| if body > 0.0 { sizes[i] / body } else { 0.0 };
    let x_frac = |i: usize| {
        if width > 0.0 {
            blocks[i].bbox[0] / width
        } else {
            0.0
        }
    };
    let terminal = |i: usize| blocks[i].text.trim().ends_with(['.', '!', '?']);

    // The column's measure: the median width of the page's PARAGRAPHS. A
    // heading stops short of the column it is set in, and on a page of two
    // columns the page's own width says nothing about that (#584). The median
    // rather than the widest, so that one table or one caption cannot move it.
    let mut paragraph_widths: Vec<f64> = blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::Paragraph))
        .map(|b| b.bbox[2] - b.bbox[0])
        .filter(|w| *w > 0.0)
        .collect();
    paragraph_widths.sort_by(f64::total_cmp);
    let column = paragraph_widths
        .get(paragraph_widths.len() / 2)
        .copied()
        .unwrap_or(f64::NAN);

    let mut out_blocks = Vec::with_capacity(n);
    let mut out_words = Vec::new();
    for (i, b) in blocks.iter().enumerate() {
        let (kind, level) = kind_name(&b.kind);
        let size = sizes[i];
        let em = if size > 0.0 { size } else { body.max(1.0) };
        let gap_above = if i > 0 && blocks[i - 1].page == b.page {
            (blocks[i - 1].bbox[1] - b.bbox[3]) / em
        } else {
            (height - b.bbox[3]) / em
        };
        let gap_below = if i + 1 < n && blocks[i + 1].page == b.page {
            (b.bbox[1] - blocks[i + 1].bbox[3]) / em
        } else {
            b.bbox[1] / em
        };
        let text = b.text.trim();
        let letters: Vec<char> = text.chars().filter(|c| c.is_alphabetic()).collect();
        // THE COLUMN'S MEASURE, which is the page's paragraphs and not the
        // page: a heading stops short of the column it is set in, and on a
        // page of two columns the page's width says nothing about that (#584).
        // The median, so that one wide table or one caption cannot move it.
        let mut lines: Vec<f64> = b.spans.iter().map(|s| (s.y * 2.0).round() / 2.0).collect();
        lines.sort_by(|a, c| a.total_cmp(c));
        lines.dedup();
        out_blocks.push(BlockFeatures {
            kind,
            level,
            n_chars: text.chars().count(),
            n_words: text.split_whitespace().count(),
            n_lines: lines.len(),
            size,
            size_ratio: ratio(i),
            bold: glyph_fraction(&b.spans, |s| s.is_bold),
            italic: glyph_fraction(&b.spans, |s| s.is_italic),
            all_caps: !letters.is_empty() && letters.iter().all(|c| c.is_uppercase()),
            font: b.spans.iter().find_map(|s| s.font_name.clone()),
            x_frac: x_frac(i),
            width_frac: if width > 0.0 {
                (b.bbox[2] - b.bbox[0]) / width
            } else {
                0.0
            },
            y_frac: if height > 0.0 {
                b.bbox[1] / height
            } else {
                0.0
            },
            gap_above_em: gap_above,
            gap_below_em: gap_below,
            ends_terminal: text.ends_with(['.', '!', '?']),
            ends_colon: text.ends_with(':'),
            marker: marker(text),
            caption: caption(text),
            section_number: section_number(text),
            prev_kind: (i > 0).then(|| kind_name(&blocks[i - 1].kind).0),
            next_kind: (i + 1 < n).then(|| kind_name(&blocks[i + 1].kind).0),
            prev_ends_terminal: (i > 0).then(|| terminal(i - 1)),
            prev_size_ratio: (i > 0).then(|| ratio(i - 1)),
            next_x_frac: (i + 1 < n).then(|| x_frac(i + 1)),
            next_size_ratio: (i + 1 < n).then(|| ratio(i + 1)),
            repeats_in_document: None,
            first_on_page: i == 0,
            last_on_page: i + 1 == n,
            title_case: title_case(text),
            number_kind: numbering(text).0,
            number_depth: numbering(text).1,
            ends_comma: text.ends_with(','),
            width_vs_column: if column > 0.0 {
                (b.bbox[2] - b.bbox[0]) / column
            } else {
                f64::NAN
            },
            font_is_the_body_face: None,
        });
        // THE PAGE'S WORDS, GLYPH FRAGMENTS REJOINED. Spans arrive one per
        // glyph run, down to a character, and every consumer of these words
        // -- the judge's lines and proposals, the stitch's rules, TATR's
        // claims -- was written and tested against whole words. Fragments
        // join in span order where they share a visual line and the shared
        // word rule joins them, across neither a style change nor a
        // substantial overlap; a run keeps its first span's index, the union
        // of its boxes, and the largest size it was set in. Order is
        // otherwise untouched: runs merge in place, and a word's box is the
        // union of what tiled it, so extents read the same joined as split.
        // Never across whitespace the span text carries: training split the
        // dump's words on whitespace and never rejoined them, so a tight
        // leader dot reads as its own word there and must read so here.
        let mut run: Option<PageWord> = None;
        let mut prev_ends_ws = false;
        for (s, span) in b.spans.iter().enumerate() {
            let span_starts_ws = span.text.chars().next().is_some_and(char::is_whitespace);
            for (j, w) in words_of(span).into_iter().enumerate() {
                let frag = PageWord {
                    text: w.text,
                    bbox: w.bbox,
                    block: i,
                    span: s,
                    size: span.font_size,
                    bold: span.is_bold,
                    italic: span.is_italic,
                    font: span.font_name.clone(),
                    color: span.color.map(|[r, g, b, _]| [r, g, b]),
                };
                // Consecutive fragments of one span always have whitespace
                // between them: that is what split them.
                let boundary_ws = j > 0 || span_starts_ws || prev_ends_ws;
                let join = match &run {
                    None => false,
                    Some(last) => {
                        let em = last.size.max(frag.size).max(f64::MIN_POSITIVE);
                        let gap = frag.bbox[0] - last.bbox[2];
                        let drift = (frag.bbox[1] - last.bbox[1]).abs();
                        let same_line = last.bbox[1] < frag.bbox[3] && last.bbox[3] > frag.bbox[1];
                        !boundary_ws
                            && same_line
                            && last.bold == frag.bold
                            && last.italic == frag.italic
                            && last.font == frag.font
                            && last.color == frag.color
                            && (last.size - frag.size).abs() <= 0.1 * em
                            && gap > -0.1 * em
                            && crate::words_join::pieces_join(gap, drift, em)
                    }
                };
                if join {
                    let last = run.as_mut().expect("a join has a run");
                    last.text.push_str(&frag.text);
                    last.bbox[0] = last.bbox[0].min(frag.bbox[0]);
                    last.bbox[1] = last.bbox[1].min(frag.bbox[1]);
                    last.bbox[2] = last.bbox[2].max(frag.bbox[2]);
                    last.bbox[3] = last.bbox[3].max(frag.bbox[3]);
                    last.size = last.size.max(frag.size);
                } else {
                    if let Some(word) = run.replace(frag) {
                        out_words.push(word);
                    }
                }
            }
            prev_ends_ws = span
                .text
                .chars()
                .next_back()
                .is_some_and(char::is_whitespace);
        }
        if let Some(word) = run.take() {
            out_words.push(word);
        }
    }

    PageFeatures {
        body_size: body,
        blocks: out_blocks,
        words: out_words,
    }
}

/// A block's text as `repeats_in_document` compares it: lower case, runs of
/// white space made one space. The same rule as the dump driver's
/// `normalise` (`scripts/extraction/learned/dump_doclaynet.py`).
fn normalise(text: &str) -> String {
    text.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// What a model on one page may know about the rest of its document, computed
/// once and read by every page.
///
/// PAGES ARE DECIDED ON THEIR OWN. Everything the learned pass reads is the
/// page's own -- its blocks, its words, its type -- except one feature,
/// `repeats_in_document`: on how many of the document's pages a block's text
/// occurs, which is what tells a running head from a section head (#583). That
/// is a fact about the whole document, so it is gathered here before any page
/// is looked at and handed to each page behind a shared reference. The struct
/// is immutable after [`DocumentFacts::of`] and holds no interior mutability,
/// so pages can be decided in any order or at the same time and come out the
/// same (`crates/pdf-engine/tests/learned_pages_run_in_parallel.rs`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentFacts {
    pages_with: std::collections::HashMap<String, u32>,
    /// The face most of the document's characters are set in, where the caller
    /// built the facts from the pages' own features. A block set in another
    /// face is one the document has singled out, which is what a heading is
    /// (#599) -- and like the repetition, it is a fact about the document and
    /// not about a page, so it is gathered here and handed over immutably.
    body_face: Option<String>,
}

impl DocumentFacts {
    /// The facts of a document given as its pages, each the texts of its
    /// blocks.
    pub fn of<'a, P, T>(pages: P) -> Self
    where
        P: IntoIterator<Item = T>,
        T: IntoIterator<Item = &'a str>,
    {
        let mut pages_with = std::collections::HashMap::new();
        for texts in pages {
            let seen: std::collections::HashSet<String> = texts
                .into_iter()
                .map(normalise)
                .filter(|t| !t.is_empty())
                .collect();
            for key in seen {
                *pages_with.entry(key).or_insert(0) += 1;
            }
        }
        DocumentFacts {
            pages_with,
            body_face: None,
        }
    }

    /// The facts of a document given its pages' texts AND every block's face
    /// with its length: this knows the body face as well.
    pub fn of_texts_and_faces<'a, P, T, F>(pages: P, faces: F) -> Self
    where
        P: IntoIterator<Item = T>,
        T: IntoIterator<Item = &'a str>,
        F: IntoIterator<Item = (Option<String>, usize)>,
    {
        let mut facts = DocumentFacts::of(pages);
        // IN THE ORDER THE FACES FIRST APPEAR, because two faces can carry the
        // same number of characters and something has to break the tie. A
        // `HashMap` broke it by iteration order, which is randomised per
        // process: the same PDF could come out with a different
        // `font_is_the_body_face` on every run, and a feature the heading model
        // splits on cannot be a coin (codex, #1983). The training side breaks
        // the tie by first occurrence (`Counter.most_common`), so this does too.
        let mut order: Vec<String> = Vec::new();
        let mut by_face: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for (face, chars) in faces {
            if let Some(face) = face {
                // BY CHARACTERS AND NOT BY BLOCKS: a document with a hundred
                // short captions in one face and twenty long paragraphs in
                // another is set in the second, and a reader would say so.
                let seen = by_face.entry(face.clone()).or_insert_with(|| {
                    order.push(face);
                    0
                });
                *seen += chars.max(1);
            }
        }
        facts.body_face = order
            .into_iter()
            .fold(None::<(String, usize)>, |best, face| {
                let chars = by_face.get(&face).copied().unwrap_or(0);
                match &best {
                    Some((_, most)) if *most >= chars => best,
                    _ => Some((face, chars)),
                }
            })
            .map(|(face, _)| face)
            // NO FACE ANYWHERE IS A FACE OF ITS OWN, the empty one: training
            // takes `body_face = ""` when no block names a font, and every
            // block without one then IS the body's (`"" == ""`). Left `None`
            // the engine read NaN where the models learned 1 -- on every block
            // of a document whose fonts carry no name, `nyse_ssl_2003_p59` among
            // them (#574, parity audit).
            .or_else(|| Some(String::new()));
        facts
    }

    /// The face most of the document is set in, if the facts were built from
    /// features rather than from texts alone.
    pub fn body_face(&self) -> Option<&str> {
        self.body_face.as_deref()
    }

    /// `repeats_in_document` for a block of this text: on how many pages its
    /// normalised text occurs. A block with no text gets 0.
    pub fn repeats(&self, text: &str) -> u32 {
        let key = normalise(text);
        if key.is_empty() {
            0
        } else {
            self.pages_with.get(&key).copied().unwrap_or(0)
        }
    }

    /// Set `repeats_in_document` on every block of one page's features.
    pub fn fill(&self, page: &mut PageFeatures, texts: &[String]) {
        for (block, text) in page.blocks.iter_mut().zip(texts) {
            block.repeats_in_document = Some(self.repeats(text));
            block.font_is_the_body_face = match (&self.body_face, &block.font) {
                // A BLOCK WITH NO FACE HAS THE EMPTY ONE, as the training
                // vector reads it (`(font or "") == body_face`): not the body's
                // where the body has a face -- the parity test found that on
                // `corpus/fw7.pdf`, whose first block has no face at all -- and
                // the body's where nothing in the document names one.
                (Some(body), face) => Some(face.as_deref().unwrap_or("") == body),
                // Facts built from texts alone know no body face; nothing in
                // the engine's own path does that, and the model then reads
                // NaN, which is what it was trained to route.
                (None, _) => None,
            };
        }
    }
}

/// Fill `repeats_in_document` on every block of one document: on how many of
/// its pages the block's normalised text occurs. A block with no text gets 0.
#[doc(hidden)]
pub fn fill_repeats(pages: &mut [(PageFeatures, Vec<String>)]) {
    let facts = DocumentFacts::of_texts_and_faces(
        pages
            .iter()
            .map(|(_, texts)| texts.iter().map(String::as_str)),
        pages.iter().flat_map(|(page, _)| {
            page.blocks
                .iter()
                .map(|block| (block.font.clone(), block.n_chars))
        }),
    );
    for (page, texts) in pages.iter_mut() {
        facts.fill(page, texts);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A feature the models split on has to read the same here as it did in
    /// training, and two of them did not (codex, #1983).
    #[test]
    fn a_letter_marker_is_ascii_like_the_training_regex() {
        // What the training side's `[A-Za-z]` matches.
        assert_eq!(numbering("A. The first part"), (3, 1));
        assert_eq!(numbering("b) and the second"), (3, 1));
        // And what it does not. `is_alphabetic` took these, so the engine said
        // 3 where the training vector said 0 -- on a document in any language
        // that accents its initials.
        assert_eq!(numbering("É. Résumé"), (0, 0));
        assert_eq!(numbering("Ω. Omega"), (0, 0));
        // The other kinds are untouched by that change.
        assert_eq!(numbering("1.2.3 Deep"), (1, 3));
        assert_eq!(numbering("IV. Fourth"), (2, 1));
        assert_eq!(numbering("Plain text"), (0, 0));
    }

    /// The body face is a decision between faces, and a tie has to fall the
    /// same way twice: a `HashMap`'s iteration order is randomised per process,
    /// so the same PDF could take either face on either run (codex, #1983).
    #[test]
    fn a_tie_between_two_faces_falls_to_the_first_one_seen() {
        let faces = || {
            [
                (Some("Helvetica".to_string()), 100usize),
                (Some("Times".to_string()), 100usize),
            ]
        };
        // Enough runs that a coin would have shown itself: a randomised
        // `HashMap` gives each face about half the runs.
        for _ in 0..64 {
            let facts = DocumentFacts::of_texts_and_faces(std::iter::empty::<Vec<&str>>(), faces());
            assert_eq!(
                facts.body_face(),
                Some("Helvetica"),
                "the tie fell the other way, so this feature is a coin"
            );
        }
        // And the count still decides where there is no tie.
        let facts = DocumentFacts::of_texts_and_faces(
            std::iter::empty::<Vec<&str>>(),
            [
                (Some("Helvetica".to_string()), 100usize),
                (Some("Times".to_string()), 101usize),
            ],
        );
        assert_eq!(facts.body_face(), Some("Times"));
    }

    /// A document whose blocks name no face has the EMPTY one as its body face,
    /// as training does (`body_face = ""`), so every block of it is the body's
    /// -- not unknown (#574, parity audit: the engine read NaN where the models
    /// learned 1). Facts built from texts alone still know no face at all.
    #[test]
    fn a_document_that_names_no_face_has_the_empty_one() {
        let facts = DocumentFacts::of_texts_and_faces(
            std::iter::empty::<Vec<&str>>(),
            [(None, 40usize), (None, 12usize)],
        );
        assert_eq!(facts.body_face(), Some(""));
        let texts_only = DocumentFacts::of(std::iter::empty::<Vec<&str>>());
        assert_eq!(texts_only.body_face(), None);
    }
}
