//! Which blocks are headings, and at what level, from the document's own
//! statistics (#568).
//!
//! [`crate::blocks`] hands back paragraphs. A paragraph and a heading are the
//! same thing to it, and to a retrieval system they are not: the heading is
//! what a chunk is filed under, what a citation names, and what a reader scans
//! for. So the blocks are read a second time here, and the ones the document
//! sets apart become [`BlockKind::Heading`].
//!
//! NO FIXED POINT SIZES. There is no size at which type becomes a heading.
//! Fourteen point is a heading in a report set in ten and body text in a large
//! print edition; a legal document sets its headings in the body size and turns
//! them bold; a form sets everything in one face and shouts its sections in
//! capitals. Every one of those is a statement about the DOCUMENT, so the
//! statistics are taken per document: the size and weight every block is set
//! in, clustered, and the cluster that carries most of the characters is the
//! body. What stands above the body is a heading, and the order the standing
//! clusters are in is the order of the levels.
//!
//! THE THREE AXES, and why each one is there rather than the obvious one
//!
//! * SIZE, clustered rather than compared. A size arrives through a text matrix
//!   and lands on 11.9997; a document's "twelve point" is a small band and not
//!   a number, so the sizes are merged into bands within
//!   [`SIZE_TOLERANCE`] of each other before anything is ranked.
//! * WEIGHT. A document with one size still has headings, and bold is how it
//!   says so. Weight is read from the spans ([`crate::TextSpan::is_bold`]),
//!   which is the font's own answer -- the descriptor's weight or the
//!   PostScript name -- and not from a stroke width we measured.
//! * CAPITALS, as a promoter and nothing else. A document that sets a heading
//!   in the body size and the body weight has one signal left, and refusing to
//!   read it would lose every heading in a form. It cannot RANK, because a
//!   capitalised line is not by that fact more important than a bold one, so it
//!   decides candidacy and then stands aside for position.
//!
//! POSITION BREAKS THE TIE. Two clusters set in the same size and the same
//! weight are two levels of the same hierarchy, and the type does not say which
//! is the outer one. The page does: the outer heading is the one that starts
//! further left of the text column and carries more white above it. That is the
//! whole of what position decides here -- it never makes a heading out of a
//! block the type did not already set apart.
//!
//! WHAT IT REFUSES
//!
//! * A LONG BLOCK. A heading is a line, sometimes two. A block of prose set in
//!   a size that stands above the body is a pull quote, a block quotation or the
//!   opening paragraph of a magazine feature, and calling it a heading files
//!   a page of text under a heading level.
//! * A DEEPER LEVEL THAN THREE. The clusters below the third fold into level 3
//!   rather than growing an `h7`, which is what markdown consumers and every
//!   chunker downstream actually read.
//!
//! WHAT IT DOES NOT DO. Running heads and page numbers are repetition across
//! pages, not typography, and they are #571: a bold folio at the top of every
//! page is a heading to this file until that ticket removes it from the flow.
//! Table cells (#570) are not candidates at all. List items
//! arrive already marked (#569) and are never promoted: an item is set in the
//! body of its column, which is what this refuses to call a heading.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::cmp::Ordering;

use crate::blocks::{dominant_font_size, Block, BlockKind};
#[cfg(test)]
use crate::table_trace::TableOrigin;

/// How far two type sizes may differ, as a fraction of the smaller, and still
/// be the same size to this file.
///
/// A size that arrives through a text matrix is not the size in the `Tf`, and
/// the same nominal twelve point turns up as 11.9997 and 12.0004 on one page.
/// Five per cent holds that spread together and still keeps ten point apart
/// from eleven, which is the tightest step a document actually sets a heading
/// at.
const SIZE_TOLERANCE: f64 = 0.05;

/// The most lines a heading may have.
///
/// Two is the common maximum -- a title that wraps -- and three admits a long
/// one set in a narrow column. Above that the block is prose, whatever it is
/// set in: a pull quote in fourteen point is not a heading, and a classifier
/// that says it is files the paragraphs after it under it.
const MAX_HEADING_LINES: usize = 3;

/// How far under the body's own size capitals may still speak for a block
/// (#583). Nine tenths; see [`capitals_hold`].
const CAPITALS_FLOOR: f64 = 0.9;

/// The most white a wrapped heading may have between its lines, in ems of the
/// body (#581). Three: a title set over two lines is set at its own leading,
/// and a heading with a second heading under it is set apart from it by more.
const MAX_SPLIT_WHITE_EM: f64 = 3.0;

/// The deepest level this produces. `#`..`###` in markdown; deeper clusters
/// fold into it.
const MAX_HEADING_LEVEL: u8 = 3;

/// How many cased letters a block needs before "it is all capitals" is a
/// statement about it rather than about one word.
///
/// A four-letter floor keeps `A.`, `II` and a stray initial out, and admits
/// `SCOPE` -- which is a real heading in a real form, and the shortest one.
const MIN_CAPS_LETTERS: usize = 4;

/// How far apart two baselines have to be before they are two lines, as a
/// fraction of the type size. The same question [`crate::blocks`] asks of a
/// pair of lines, asked here of a block that has already been merged.
const LINE_STEP_EM: f64 = 0.25;

/// How far two blocks' left edges have to differ, in ems of the body, before
/// the page is saying they are set at different places.
///
/// One em, because an indent step IS an em or more: nobody indents a heading by
/// a third of a point, and a floor any lower would hand the tie to the rounding
/// of a text matrix rather than to the typesetter.
const INDENT_STEP_EM: f64 = 1.0;

/// What a block is set in and where it stands: the key two heading blocks have
/// to agree on before they are the same level.
///
/// The style is the type, and `indent` is how far left of the body column the
/// block starts, in whole ems. Position is in the key rather than beside it
/// because two headings the type CANNOT separate -- same size, same weight --
/// are still two levels when one starts at the margin and the other is indented
/// into the text, and a key without it has one cluster where the document has
/// two.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Cluster {
    style: Style,
    indent: i32,
}

/// What a block is set in, as the classifier compares one block with another.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Style {
    /// The size band, as the char-weighted middle of its cluster.
    size: f64,
    /// Most of the block's characters are bold.
    bold: bool,
    /// Every cased letter in the block is a capital, and there are enough of
    /// them to mean it.
    caps: bool,
}

impl Style {
    /// Size and weight: the two axes that RANK. Capitals and position do not
    /// appear, because neither of them makes one heading outrank another.
    fn rank(&self) -> (f64, bool) {
        (self.size, self.bold)
    }

    /// The same size band and the same weight.
    fn ranks_with(&self, other: &Style) -> bool {
        (self.size - other.size).abs()
            <= SIZE_TOLERANCE * self.size.min(other.size).max(f64::MIN_POSITIVE)
            && self.bold == other.bold
    }

    fn outranks(&self, other: &Style) -> bool {
        if self.ranks_with(other) {
            return false;
        }
        match self.rank().0.partial_cmp(&other.rank().0) {
            Some(Ordering::Greater) => true,
            Some(Ordering::Less) => false,
            _ => self.bold && !other.bold,
        }
    }
}

/// Everything the classifier reads off one block.
struct Metrics {
    style: Style,
    /// The page the block was drawn on. A column is a fact about ONE page.
    page: usize,
    /// Non-whitespace characters, which is what the body cluster is weighed in.
    chars: usize,
    /// How many lines the block was merged from.
    lines: usize,
    /// Left edge of the block, in PDF user space.
    left: f64,
    /// Right edge of the block, in PDF user space. Read only to tell one
    /// column from another: two blocks whose horizontal extents overlap are
    /// set in the same column, and a left edge is a statement about the column
    /// it is measured in.
    right: f64,
    /// White above the block, in points, or `None` where nothing is above it --
    /// the top of a page says nothing about a hierarchy.
    space_above: Option<f64>,
}

/// Whether a block's cased letters are all capitals.
fn is_all_caps(text: &str) -> bool {
    let mut cased = 0usize;
    for c in text.chars() {
        if c.is_lowercase() {
            return false;
        }
        if c.is_uppercase() {
            cased += 1;
        }
    }
    cased >= MIN_CAPS_LETTERS
}

/// How many lines a block holds, counted from the baselines its spans sit on.
///
/// The text has had its line breaks taken out by then -- that is what a block
/// is -- so the only surviving record of how many lines there were is the
/// geometry the spans still carry.
fn line_count(block: &Block) -> usize {
    let size = dominant_font_size(block.spans.iter()).max(f64::MIN_POSITIVE);
    let mut baselines: Vec<f64> = block.spans.iter().map(|span| span.y).collect();
    baselines.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let mut lines = 0usize;
    let mut last: Option<f64> = None;
    for y in baselines {
        if last.is_none_or(|previous: f64| (y - previous).abs() > LINE_STEP_EM * size) {
            lines += 1;
            last = Some(y);
        }
    }
    lines.max(1)
}

/// Whether most of a block's characters are bold.
fn is_bold(block: &Block) -> bool {
    let mut bold = 0usize;
    let mut total = 0usize;
    for span in &block.spans {
        let chars = span.text.chars().filter(|c| !c.is_whitespace()).count();
        total += chars;
        if span.is_bold {
            bold += chars;
        }
    }
    total > 0 && bold * 2 > total
}

/// The size bands of a document: every block's size, merged while the next one
/// is within [`SIZE_TOLERANCE`], each band reported as the char-weighted middle
/// of what went into it.
///
/// Single-link merging, deliberately. A document sets a handful of sizes and
/// the spread inside each one is rounding, so a chain of near-equal sizes is
/// one size; a document that really does set eleven, twelve and thirteen point
/// keeps three bands, because eleven and thirteen are not within a tolerance of
/// each other and nothing between them joins them up.
fn size_bands(sizes: &[(f64, usize)]) -> Vec<f64> {
    let mut sorted: Vec<(f64, usize)> = sizes.iter().copied().filter(|(s, _)| *s > 0.0).collect();
    sorted.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));

    let mut bands: Vec<Vec<(f64, usize)>> = Vec::new();
    for entry in sorted {
        match bands.last_mut() {
            Some(band)
                if entry.0 - band.last().expect("a band is never empty").0
                    <= SIZE_TOLERANCE * band.last().expect("a band is never empty").0 =>
            {
                band.push(entry)
            }
            _ => bands.push(vec![entry]),
        }
    }

    bands
        .into_iter()
        .map(|band| {
            let weight: usize = band.iter().map(|(_, chars)| *chars).sum();
            if weight == 0 {
                return band[band.len() / 2].0;
            }
            band.iter()
                .map(|(size, chars)| size * *chars as f64)
                .sum::<f64>()
                / weight as f64
        })
        .collect()
}

/// The band a size falls in.
fn band_of(bands: &[f64], size: f64) -> f64 {
    bands
        .iter()
        .copied()
        .min_by(|a, b| {
            (a - size)
                .abs()
                .partial_cmp(&(b - size).abs())
                .unwrap_or(Ordering::Equal)
        })
        .unwrap_or(size)
}

/// Read every block, with its size put in the document's own bands.
fn metrics_of(blocks: &[Block]) -> Vec<Metrics> {
    let raw: Vec<(f64, usize)> = blocks
        .iter()
        .map(|block| {
            (
                dominant_font_size(block.spans.iter()),
                block.text.chars().filter(|c| !c.is_whitespace()).count(),
            )
        })
        .collect();
    let bands = size_bands(&raw);

    let mut previous: Option<(usize, f64)> = None;
    let mut out = Vec::with_capacity(blocks.len());
    for (block, (size, chars)) in blocks.iter().zip(raw.iter().copied()) {
        // The white above a block, on the page it is on: the drop from the
        // bottom of whatever came before it in reading order to its own top.
        // Across a page break there is no such distance, and a negative one is
        // two columns meeting rather than white.
        let space_above = match previous {
            Some((page, bottom)) if page == block.page => Some(bottom - block.bbox[3]),
            _ => None,
        }
        .filter(|gap| *gap >= 0.0);
        previous = Some((block.page, block.bbox[1]));

        out.push(Metrics {
            page: block.page,
            style: Style {
                size: band_of(&bands, size),
                bold: is_bold(block),
                caps: is_all_caps(&block.text),
            },
            chars,
            lines: line_count(block),
            left: block.bbox[0],
            right: block.bbox[2],
            space_above,
        });
    }
    out
}

/// The style most of the document's characters are set in.
fn body_style(metrics: &[Metrics]) -> Option<Style> {
    let mut groups: Vec<(Style, usize)> = Vec::new();
    for entry in metrics {
        match groups.iter_mut().find(|(style, _)| *style == entry.style) {
            Some((_, weight)) => *weight += entry.chars,
            None => groups.push((entry.style, entry.chars)),
        }
    }
    // The heaviest style wins; the largest of them where two carry the same
    // weight, so that a two-block document with a title and a line of text
    // reads as a title over a body rather than the other way round.
    groups
        .into_iter()
        .max_by(|a, b| {
            a.1.cmp(&b.1).then_with(|| {
                a.0.size
                    .partial_cmp(&b.0.size)
                    .unwrap_or(Ordering::Equal)
                    .reverse()
            })
        })
        .map(|(style, _)| style)
}

/// Whether a block names anything: a heading is what a section is called, so it
/// has a word in it.
///
/// Measured on `tracemonkey`, where the tick labels of two charts are set in a
/// symbol font, larger than the body and two characters long -- `%#"`, `01"`,
/// `$!"` -- and every one of them came back as a heading. A block with no
/// letter in it is a number, a rule drawn as text or a symbol, and none of
/// those is what a chunk gets filed under.
fn names_something(text: &str) -> bool {
    text.chars().any(char::is_alphabetic)
}

/// Whether a style stands above the body.
///
/// Above in rank -- larger, or bold where the body is not -- or level with it
/// in rank and set in capitals where the body is not. The second clause is the
/// whole of what a single-face document has to say, and it is a promoter and
/// never a rank: see the module header.
fn stands_above(style: &Style, body: &Style) -> bool {
    style.outranks(body) || (capitals_hold(style, body) && style.caps && !body.caps)
}

/// Whether capitals are allowed to speak for this style: it is not set LARGER
/// than the body -- that is already a rank -- and not so much smaller that the
/// capitals are a footnote's or a caption's (#583).
///
/// The band on its own was too tight. A journal that sets `I. INTRODUCTION`
/// nine per cent under its body -- which is what a preprint's section head
/// measures against a page whose running text is set one size up -- fell
/// outside it, and the capitals could say nothing. Measured on run
/// `structured-doclaynet-20260922T113831Z`: from nine tenths of the body the
/// axis reads 0.546 against 0.500, and at four fifths it stops paying (0.541
/// for six more of the same). So nine tenths, and nothing below it.
fn capitals_hold(style: &Style, body: &Style) -> bool {
    style.ranks_with(body)
        || (body.size > 0.0
            && style.size >= CAPITALS_FLOOR * body.size
            && style.size <= body.size * (1.0 + SIZE_TOLERANCE)
            && style.bold == body.bold)
}

/// The median left edge of the body blocks set in the same column as `block`,
/// ON THE SAME PAGE.
///
/// PER COLUMN, and not one figure for the document. "Left of the text column"
/// is the whole of what position says here, and on a two-column page a single
/// left edge for the page puts every heading in the right-hand column a
/// column's width to the right of it -- which reads as an indent, and would
/// make the same heading style two levels depending on which column it fell
/// in. Two blocks whose horizontal extents overlap are in one column.
///
/// AND PER PAGE, because a column is a fact about one page: a document that
/// changes its margins or its column count partway through would otherwise
/// measure a heading on page two against a paragraph on page one, and the
/// change of margin would read as an indent (codex, round 1). A candidate that
/// overlaps no body block on its own page is measured against every body block
/// on that page, and against the whole document only where its page holds no
/// body at all -- a heading standing alone on a page has no column of its own.
fn column_left(block: &Metrics, metrics: &[Metrics], body: &Style) -> f64 {
    let body_on_page: Vec<&Metrics> = metrics
        .iter()
        .filter(|m| m.style == *body && m.page == block.page)
        .collect();
    let overlapping: Vec<f64> = body_on_page
        .iter()
        .filter(|m| m.right.min(block.right) > m.left.max(block.left))
        .map(|m| m.left)
        .collect();
    let mut lefts = if !overlapping.is_empty() {
        overlapping
    } else if !body_on_page.is_empty() {
        body_on_page.iter().map(|m| m.left).collect()
    } else {
        metrics
            .iter()
            .filter(|m| m.style == *body)
            .map(|m| m.left)
            .collect()
    };
    if lefts.is_empty() {
        return block.left;
    }
    lefts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    lefts[lefts.len() / 2]
}

/// The mean white above the blocks of a cluster, in points; zero where none of
/// them has anything above it on its page.
fn space_above(metrics: &[&Metrics]) -> f64 {
    let spaced: Vec<f64> = metrics.iter().filter_map(|m| m.space_above).collect();
    if spaced.is_empty() {
        return 0.0;
    }
    spaced.iter().sum::<f64>() / spaced.len() as f64
}

/// Give every block that stands above the body its heading level (#568).
///
/// The statistics are the ones of the slice it is handed, which is the whole
/// document in [`crate::PdfDocument::extract_blocks`] and one page in
/// [`crate::PdfDocument::extract_page_blocks`] -- a page asked for on its own
/// is the only document there is to take statistics from, and a caller that
/// wants the document's has the document's method.
pub(crate) fn classify(blocks: &mut Vec<Block>) {
    if blocks.len() < 2 {
        // One block is its own body. Nothing stands above it, and calling it a
        // heading would file a one-paragraph document under a heading and leave
        // the text of it empty.
        return;
    }

    let metrics = metrics_of(blocks);
    let Some(body) = body_style(&metrics) else {
        return;
    };
    // The candidates: short blocks whose style stands above the body's. A long
    // block is refused HERE, before its style is collected, so that a page of
    // large-set prose does not spend a heading level on itself.
    let em = body.size.max(f64::MIN_POSITIVE);
    //
    // A BLOCK THAT IS ALREADY SOMETHING ELSE is left alone. `crate::lists` has
    // run inside the merge and marked the items it confirmed (#569), and the
    // two never disagree about a block: a list item is set in the body of its
    // column, which is the one thing `stands_above` refuses -- but a marker the
    // list rule threw out for a reason of its own is a block this must not then
    // promote out from under it.
    let set_apart = set_apart_by_the_page(blocks, &metrics, em);
    let candidates: Vec<usize> = (0..blocks.len())
        .filter(|i| {
            matches!(blocks[*i].kind, BlockKind::Paragraph)
                && metrics[*i].lines <= MAX_HEADING_LINES
                && names_something(&blocks[*i].text)
                && (stands_above(&metrics[*i].style, &body) || set_apart.contains(i))
        })
        .collect();
    if candidates.is_empty() {
        return;
    }

    let key_of = |i: usize| Cluster {
        style: metrics[i].style,
        indent: ((column_left(&metrics[i], &metrics, &body) - metrics[i].left)
            / (INDENT_STEP_EM * em))
            .round() as i32,
    };

    let mut clusters: Vec<(Cluster, Vec<&Metrics>)> = Vec::new();
    for i in candidates.iter().copied() {
        let key = key_of(i);
        match clusters.iter_mut().find(|(cluster, _)| *cluster == key) {
            Some((_, members)) => members.push(&metrics[i]),
            None => clusters.push((key, vec![&metrics[i]])),
        }
    }

    // Size, then weight -- and then, for the clusters the type cannot separate,
    // the page: further left first, more white above next. Capitals come last
    // and only as a tie breaker of the tie breaker: a capitalised heading is not
    // by that fact outside a bold one.
    clusters.sort_by(|a, b| {
        b.0.style
            .size
            .partial_cmp(&a.0.style.size)
            .unwrap_or(Ordering::Equal)
            .then_with(|| b.0.style.bold.cmp(&a.0.style.bold))
            .then_with(|| b.0.indent.cmp(&a.0.indent))
            .then_with(|| {
                space_above(&b.1)
                    .partial_cmp(&space_above(&a.1))
                    .unwrap_or(Ordering::Equal)
            })
            .then_with(|| b.0.style.caps.cmp(&a.0.style.caps))
    });

    let levels: Vec<(Cluster, u8)> = clusters
        .iter()
        .enumerate()
        .map(|(rank, (cluster, _))| {
            // Clamped BEFORE the cast. A document with more than 255 heading
            // clusters would otherwise wrap the rank round to zero and hand the
            // 257th cluster level 1 -- the outermost level in the document,
            // given to the least prominent type in it.
            let level = rank.saturating_add(1).min(MAX_HEADING_LEVEL as usize);
            (*cluster, level as u8)
        })
        .collect();

    for i in candidates {
        let key = key_of(i);
        if let Some((_, level)) = levels.iter().find(|(cluster, _)| *cluster == key) {
            blocks[i].kind = BlockKind::Heading { level: *level };
        }
    }
    put_split_headings_together(blocks, &metrics, em);
}

/// The blocks a page sets apart where the TYPE says nothing at all (#583).
///
/// A document that sets its headings in the body's own size, the body's own
/// weight and no capitals has said nothing this file can read -- and it has
/// still said it, with the page: `Financial Reporting Framework` over its
/// paragraph is a line of its own, short of the measure, with something
/// finished above it, and the section it names starts flush under it.
///
/// FOUR CLAUSES AND NOT ONE. A single positional promoter -- short, one line,
/// no full stop, a line of white -- was measured two rounds ago and REFUSED:
/// it recovered 22 headings and invented 58 to 68, and the axis fell from
/// 0.465 to 0.453. What the section clause adds is ownership: a heading owns
/// the text under it, and the text under it begins at the column's own left
/// edge rather than indented like a new paragraph of a running argument.
///
/// AND WHITE IS NOT THE CLAUSE THAT PAYS FOR IT (#584). The round this rule
/// first shipped in asked for more white above the line than the page puts
/// between two of its blocks, and a document whose sections are set at the
/// paragraph gap -- which is what the annual reports in the set do, 13.9
/// point over a heading and 13.9 between two paragraphs -- says nothing by
/// that measure. Counted over every annotated page, the white clause reads 22
/// of the headings and 6 other blocks; asking instead that what stands ABOVE
/// the line be finished -- a block that ends its sentence, or a block that is
/// not a paragraph at all -- reads 66 and 8. Both together read 67 and 11.
/// The clause that pays is the one that refuses a paragraph's own short last
/// line, and a short last line has an unfinished sentence above it whatever
/// the page's white does.
///
/// AND NEVER A RANK. Like the capitals, this decides candidacy and then stands
/// aside: a block the page set apart is not by that fact above or below a
/// block the type set apart.
fn set_apart_by_the_page(blocks: &[Block], metrics: &[Metrics], em: f64) -> Vec<usize> {
    let mut out = Vec::new();
    for i in 0..blocks.len() {
        if !matches!(blocks[i].kind, BlockKind::Paragraph) || metrics[i].lines != 1 {
            continue;
        }
        // Short of the column's own measure, which is the widest the page sets
        // its body to on the page this block is on.
        let measure = metrics
            .iter()
            .filter(|m| m.page == metrics[i].page)
            .map(|m| m.right - m.left)
            .fold(f64::NEG_INFINITY, f64::max);
        if !(measure > 0.0 && (metrics[i].right - metrics[i].left) < TITLE_MEASURE * measure) {
            continue;
        }
        if ends_sentence(&blocks[i].text) || blocks[i].text.trim_end().ends_with(':') {
            continue;
        }
        // And what stands above it is FINISHED. A page's first block is
        // refused by the same clause: it has nothing above it that this file
        // can read, because what runs on to it was set on the page before.
        if metrics[i].space_above.is_none() {
            continue;
        }
        let Some(previous) = i.checked_sub(1) else {
            continue;
        };
        if metrics[previous].page != metrics[i].page {
            continue;
        }
        let finished = match &blocks[previous].kind {
            // A paragraph that has finished its sentence. A paragraph's own
            // short last line never has, and that is the line this refuses.
            BlockKind::Paragraph => ends_sentence(&blocks[previous].text),
            // A list, a table or another heading is finished by being over.
            BlockKind::ListItem { .. } | BlockKind::TableCell { .. } => true,
            BlockKind::Heading { .. } | BlockKind::Caption => true,
            // A figure likewise, by being over. Figures merge after this
            // pass runs, so the arm is exhaustiveness only.
            BlockKind::Figure => true,
            // Furniture says nothing: under a running head stands the first
            // line of the body, and what runs on to it was set on the page
            // before.
            BlockKind::Header | BlockKind::Footer => false,
        };
        if !finished {
            continue;
        }
        // And the section it names starts under it, at the heading's OWN left
        // edge. Not at the column's median edge: a page set with its
        // paragraphs' first lines in has half its lines at the indent, and the
        // median cannot tell the start of a section from the next paragraph of
        // a running argument. What this refuses is everything set IN from the
        // line -- a quotation, a note, a table's stub -- none of which is the
        // section a heading owns.
        let Some(next) = metrics.get(i + 1) else {
            continue;
        };
        if next.page != metrics[i].page
            || !matches!(
                blocks[i + 1].kind,
                BlockKind::Paragraph | BlockKind::ListItem { .. } | BlockKind::TableCell { .. }
            )
            || (next.left - metrics[i].left).abs() > 0.5 * em
        {
            continue;
        }
        out.push(i);
    }
    out
}

/// How much of its column's measure a block the page set apart stops short of
/// (#583, widened in #584).
///
/// Six tenths reads 66 of the headings the set's annotation draws and 8 other
/// blocks; eight tenths reads 68 and the same 8. What the clause is for is the
/// full line of a paragraph, and a full line is the measure.
const TITLE_MEASURE: f64 = 0.8;

/// A heading set over two lines is ONE heading (#581).
///
/// The merge in [`crate::blocks`] decides on the body's own measure: a line
/// that stops short of it ends a paragraph, and a heading stops short of it on
/// purpose -- so a title that wraps arrives here as two blocks, and the run
/// that measured this found 25 half-headings against 11 whole ones it could
/// not match because of them.
///
/// WHAT MAY BE JOINED, and every clause of it was measured before it was
/// written. Two blocks are one heading when the page says so four times over:
/// they are next to each other in the flow, they are on one page, the
/// classifier put them in the SAME cluster -- the same size band, the same
/// weight, the same capitals, the same place in the column -- and the first
/// does not end a sentence. Joining adjacent headings without the cluster is
/// the same rule with one clause missing, and on the round this was sized
/// against it takes the axis DOWN by more than this takes it up (0.408 against
/// 0.508 from 0.465): two headings of different type next to each other are two
/// headings, and a document with a title over a subtitle says so in the type.
///
/// AND THE WHITE BETWEEN THEM. Two lines of one title are set at the leading;
/// a heading with another heading under it is set apart from it. Three lines
/// of white is the widest a wrapped title is ever set at and narrower than any
/// section break.
fn put_split_headings_together(blocks: &mut Vec<Block>, metrics: &[Metrics], em: f64) {
    // WHERE EACH BLOCK'S METRICS ARE. Joining removes a block, and `metrics`
    // is the measurement of the blocks as they arrived -- so after the first
    // join every later block sits one place to the left of its own row, and a
    // classifier reading `metrics[i + 1]` would be comparing a heading with
    // the style of the block before it. (codex, #1944)
    let mut source: Vec<usize> = (0..blocks.len()).collect();
    let mut i = 0;
    while i + 1 < blocks.len() {
        let (first, next) = (&blocks[i], &blocks[i + 1]);
        // The LEVEL is not part of the test, only the type is. Two lines of one
        // title land in two clusters wherever the second is set at another
        // place in the column -- centred under a line that runs to the margin
        // is exactly that -- and those two clusters are two levels. What says
        // they are one heading is that the page set them in the same type, put
        // them at its own leading, and left the first without a full stop.
        let joins = matches!(first.kind, BlockKind::Heading { .. })
            && matches!(next.kind, BlockKind::Heading { .. })
            && first.page == next.page
            && metrics[source[i]].style == metrics[source[i + 1]].style
            && !ends_sentence(&first.text)
            // The drop from the bottom of the first line to the top of the
            // second, in ems of the body.
            && (first.bbox[1] - next.bbox[3]) <= MAX_SPLIT_WHITE_EM * em
            && next.bbox[3] <= first.bbox[1];
        if !joins {
            i += 1;
            continue;
        }
        let tail = blocks.remove(i + 1);
        source.remove(i + 1);
        let head = &mut blocks[i];
        head.text = format!("{} {}", head.text.trim_end(), tail.text.trim_start());
        head.bbox = [
            head.bbox[0].min(tail.bbox[0]),
            head.bbox[1].min(tail.bbox[1]),
            head.bbox[2].max(tail.bbox[2]),
            head.bbox[3].max(tail.bbox[3]),
        ];
        head.spans.extend(tail.spans);
    }
}

/// Whether a block reads as the end of a sentence, which no heading does and
/// the first line of a wrapped one never does either.
fn ends_sentence(text: &str) -> bool {
    matches!(
        text.trim_end()
            .trim_end_matches(['"', '\'', '\u{201d}', '\u{2019}', ')', ']'])
            .chars()
            .next_back(),
        Some('.') | Some('!') | Some('?') | Some(';')
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::TextSpan;

    fn span(text: &str, x: f64, y: f64, size: f64, bold: bool) -> TextSpan {
        TextSpan {
            text: text.to_string(),
            x,
            y,
            width: size * 0.5 * text.chars().count() as f64,
            height: size,
            font_size: size,
            font_name: None,
            is_bold: bold,
            is_italic: false,
            color: None,
            width_source: crate::text::WidthSource::Estimate,
            char_bounds: Vec::new(),
            ..TextSpan::default()
        }
    }

    fn block(text: &str, y: f64, size: f64, bold: bool) -> Block {
        let mut block = Block {
            kind: BlockKind::Paragraph,
            page: 0,
            bbox: [0.0; 4],
            text: text.to_string(),
            spans: vec![span(text, 72.0, y, size, bold)],
            index: 0,
            continues_previous: false,
            confidence: None,
            grid: None,
            table_origin: TableOrigin::Other,
            trace_learned: None,
        };
        crate::blocks::recompute_bbox(&mut block);
        block
    }

    #[test]
    fn near_equal_sizes_are_one_band_and_a_real_step_is_two() {
        // A twelve point that arrives through three text matrices, and a
        // fourteen point that does not belong with it.
        let bands = size_bands(&[(11.9997, 400), (12.0004, 300), (12.0, 200), (14.0, 40)]);
        assert_eq!(bands.len(), 2, "{bands:?}");
        assert!((bands[0] - 12.0).abs() < 0.01, "{bands:?}");
        assert!((bands[1] - 14.0).abs() < 0.01, "{bands:?}");
    }

    #[test]
    fn a_chain_of_near_equal_sizes_stays_one_band() {
        // 10, 10.4, 10.8: no neighbour is a step, so nothing between them is.
        let bands = size_bands(&[(10.0, 100), (10.4, 100), (10.8, 100)]);
        assert_eq!(bands.len(), 1, "{bands:?}");
    }

    #[test]
    fn capitals_need_enough_letters_to_mean_it() {
        assert!(is_all_caps("SCOPE"));
        assert!(is_all_caps("SECTION A: THE APPLICANT"));
        assert!(!is_all_caps("Scope"), "one capital is a sentence opening");
        assert!(!is_all_caps("II"), "two letters say nothing");
        assert!(!is_all_caps("2.1"), "digits are not capitals");
    }

    #[test]
    fn lines_are_counted_from_the_baselines_the_spans_sit_on() {
        let mut one = block("a heading", 700.0, 12.0, false);
        assert_eq!(line_count(&one), 1);
        one.spans
            .push(span("a second line", 72.0, 688.0, 12.0, false));
        assert_eq!(line_count(&one), 2);
        // A superscript on the same line is not a second line.
        one.spans.push(span("1", 200.0, 689.0, 7.0, false));
        assert_eq!(line_count(&one), 2);
    }

    #[test]
    fn the_body_is_the_style_that_carries_the_characters_not_the_blocks() {
        // Six short bold headings against one long paragraph: the paragraph is
        // the body, because a body is measured in characters.
        let mut blocks: Vec<Block> = (0..6)
            .map(|i| block("Heading", 700.0 - i as f64 * 40.0, 10.0, true))
            .collect();
        blocks.push(block(
            &"A sentence of running text that is far longer than any of them. ".repeat(4),
            400.0,
            10.0,
            false,
        ));
        let metrics = metrics_of(&blocks);
        let body = body_style(&metrics).expect("a body");
        assert!(!body.bold, "the headings outvoted the body");
    }

    #[test]
    fn a_block_with_no_letter_in_it_is_not_a_heading() {
        // A chart's tick labels, set in a symbol font four points over the body.
        let mut blocks = vec![
            block("%#\"", 700.0, 14.0, false),
            block("01\"", 680.0, 14.0, false),
            block("Section one", 660.0, 14.0, false),
            block(
                &"A sentence of running text that is far longer than any of them. ".repeat(4),
                400.0,
                10.0,
                false,
            ),
        ];
        classify(&mut blocks);
        assert_eq!(
            blocks[0].kind,
            BlockKind::Paragraph,
            "a tick label became a heading"
        );
        assert_eq!(
            blocks[1].kind,
            BlockKind::Paragraph,
            "a tick label became a heading"
        );
        assert_eq!(blocks[2].kind, BlockKind::Heading { level: 1 });
    }

    #[test]
    fn a_one_block_document_has_no_heading() {
        let mut blocks = vec![block("A note", 700.0, 24.0, true)];
        classify(&mut blocks);
        assert_eq!(blocks[0].kind, BlockKind::Paragraph);
    }
}
