// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! Running headers, footers and page numbers (#571).
//!
//! A page's furniture is the matter the document repeats at the top and the
//! bottom of every page: the chapter it is in, the form's revision, `Page 4 of
//! 12`. It is set in the text frame like everything else, so extraction reads
//! it as body -- and it lands in the middle of the flow, between the last
//! paragraph of one page and the first of the next, where it breaks a sentence
//! in two for anything that chunks, cites or embeds the document.
//!
//! THE EVIDENCE IS REPETITION, AND AT THE FOOT OF A PAGE A FIGURE IS ENOUGH. A
//! line at the top of a page is a title as often as it is a running head, and
//! neither its type nor its position separates them: what separates them is
//! that the document says the running head again on the next page, and the
//! page after that. So this reads the whole document, groups the lines at the
//! two ends of each page by what they say and where they stand, and calls a
//! group furniture once THREE pages carry it. Two is a coincidence a two-page
//! document cannot rule out; three is the smallest run that is a decision by
//! the typesetter.
//!
//! That rule needs a document, and a document is what a corpus of loose pages
//! does not have (#579). So there is a second reading, for ONE page: a band at
//! the FOOT of a page, set off from the body by white, whose line carries a
//! figure is a page number, and a page number is furniture on the evidence of
//! the page it stands on. The repetition rule is left exactly as it is and
//! still decides everything else, which is what makes this an addition rather
//! than a replacement: on a document the two agree, and on a single page the
//! foot is the only end that can be read alone.
//!
//! THE HEAD CANNOT BE READ THAT WAY, and it was measured rather than assumed.
//! Over the 200 annotated pages of run `structured-doclaynet-20260921T000905Z`
//! the same signal at the top of the page marks 26 blocks the ground truth
//! calls body -- section headings standing over their own white -- and takes
//! the heading axis from 0.465 to 0.404. Two further guards on the foot were
//! tried and refused: a band no taller than the page's median block loses four
//! running feet and saves no body block, and a forty-character limit loses
//! twenty-seven and saves one.
//!
//! * THE SAME TEXT, OR THE SAME PATTERN. A page number changes on every page,
//!   which is the whole of its job, so the comparison is made on a normalised
//!   reading in which every run of digits is one `#` and a lone roman numeral
//!   is the same `#`: `Page 4 of 12` and `Page 5 of 12` are one group, and so
//!   are `ii` and `iii` on the front matter of a report.
//! * AT THE SAME PLACE. Position is measured as a FRACTION of the page's own
//!   text extent rather than in points, so a landscape page in a portrait
//!   document is judged against its own measure instead of against the
//!   portrait pages' -- the failure this would otherwise have on every report
//!   that turns a wide table sideways.
//! * SET OFF FROM THE BODY. A band at the end of a page is furniture only when
//!   white stands between it and the rest of the page. Without that rail the
//!   first paragraph of a page whose text happens to repeat -- a form's
//!   standing clause, a table's caption -- is read as a running head. The
//!   white is measured against what stands OVER the band and not against
//!   everything else on the page: a page number in the outer corner has a
//!   whole margin around it and a caption in the middle of the same strip
//!   beside it, and measuring against the caption reads that margin as a
//!   NEGATIVE gap. It cost 66 of the 128 running heads this rule missed on the
//!   annotated sample of #579, most of them the folio of an annual report set
//!   alternately left and right.
//!
//! WHAT IT LEAVES ALONE. The first page of a report carries no running head,
//! and nothing here asks it to: a group is decided over the pages that have
//! one. A footnote is at the bottom of its page and is not furniture, because
//! its text is its own and no other page repeats it. A list item and a table
//! cell are refused before anything else is measured: both are a statement
//! about a region of one page (#569, #570), and a cell that reads `1` in the
//! same corner of three pages is not a page number.
//!
//! What this does NOT do is drop the blocks. They keep their text, their box
//! and their page, and take the kind [`BlockKind::Header`] or
//! [`BlockKind::Footer`]; `crate::reading_order` then keeps them out of the
//! flow by putting them after the body. A caller reading the JSON still has
//! them, which is the point of marking rather than deleting.

use std::collections::BTreeMap;

use std::cmp::Ordering;

use crate::blocks::{Block, BlockKind};

/// How many pages have to carry the same line in the same place before it is
/// furniture.
///
/// Three. Two lines that agree are a pair, and a pair happens: two pages of a
/// contract that both open `Schedule 2`, two slides with the same footer under
/// a title that is not repeated. Three is the shortest run in which the
/// typesetter rather than the text is speaking, and it is what the ticket asks
/// for.
const MIN_PAGES: usize = 3;

/// How far the next line may stand from the band and still belong to it, in
/// lines of its own type (#584).
///
/// A running head set over two lines -- the company on one, what the
/// statements are on the next -- is set at its own leading, and a band of one
/// line left the second half in the body: on `NASDAQ_SHEN_2003.pdf` the
/// document repeats such a head on twelve pages and we called both halves a
/// heading on every one of them.
const BAND_LINE_REACH: f64 = 1.25;

/// How far short of the page's own measure a line must stop to be taken into
/// the band beneath the outermost one (#584).
///
/// This is what tells the second line of a running head from the first line of
/// the body: a running head does not run to the measure and a body line does.
/// Without it the band grows into the text, which is the case
/// `a_line_too_near_the_body_is_not_furniture` has held since #571.
const BAND_SHORT_FRACTION: f64 = 0.9;

/// The most lines a band may hold (#584). A head of two lines happens, three
/// is a title block, and beyond that a page is being read as its own margin.
const BAND_MAX_LINES: usize = 4;

/// The longest a running head may be, in characters.
///
/// A running head is a line; a paragraph that happens to sit at the top of
/// three pages is not one. Generous, because a legal footer runs long.
const MAX_CHARS: usize = 160;

/// How much white has to stand between the band and the body, as a fraction of
/// the band's own height.
///
/// Three quarters of a line. A running head is set off by at least a line of
/// white in every manual of style; the body's own paragraphs are set at their
/// leading, which is less than a line of white between the boxes.
const BAND_GAP_FRACTION: f64 = 0.75;

/// How far the band may move between pages, as a fraction of the page's own
/// text height.
const POSITION_TOLERANCE: f64 = 0.06;

/// How far the band may move across the page between pages, as a fraction of
/// the page's own text width.
///
/// Wider than the vertical tolerance on purpose: a running head is often set
/// on alternating sides, and a centred page number moves with the width of the
/// number it holds.
const SIDE_TOLERANCE: f64 = 0.12;

/// How near the body's own left edge a band has to start before it is read as
/// being set in the body's measure, as a fraction of the page's text width.
///
/// A hundredth: the rounding a text matrix arrives through, and nothing else.
/// A footnote is set to the body's edge and a folio is not, and the two are
/// either at the same edge or points apart.
const BODY_MEASURE_TOLERANCE: f64 = 0.01;

/// How many of a group's candidates are tried as the centre of a cluster.
///
/// The search is quadratic in the group, and a group is as long as the
/// document. Sixty-four is far past what a running head needs: it stands in
/// one place on every page that carries it, so the first candidates of the
/// group find it.
const MAX_SEEDS: usize = 64;

/// One line at one end of one page, reduced to what the grouping decides on.
struct Candidate {
    /// Index into the document's blocks.
    block: usize,
    /// 0-based page.
    page: usize,
    /// Whether it stands at the top of the page.
    top: bool,
    /// Its vertical centre, as a fraction of the page's text height measured
    /// down from the top of that text.
    y: f64,
    /// Its horizontal centre, as a fraction of the page's text width.
    x: f64,
    /// What it says, with the numbers taken out.
    key: String,
    /// Whether the band is set at the body's own left edge.
    ///
    /// What tells a FOOTNOTE from a folio, and the one thing the page-number
    /// reading below needs that position alone does not give: a footnote opens
    /// with its reference figure and is set in the measure the body is set in,
    /// and a page number stands clear of it -- centred, or out in the margin.
    in_the_body_measure: bool,
}

/// Mark every running header and footer in `blocks` (#571).
///
/// Runs over the WHOLE document, before the headings are classified: a running
/// head is often set in a face of its own, and a classifier that sees it as
/// body first would spend a heading level on it.
pub(crate) fn mark(blocks: &mut [Block]) {
    let mut pages: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for (i, block) in blocks.iter().enumerate() {
        pages.entry(block.page).or_default().push(i);
    }

    let mut candidates: Vec<Candidate> = Vec::new();
    for (page, on_page) in &pages {
        for top in [true, false] {
            candidates.extend(band(blocks, *page, on_page, top));
        }
    }

    // The page's own evidence first, so that a document of one or two pages --
    // and a corpus of loose pages -- still loses its folio (#579).
    let mut marked: Vec<usize> = candidates
        .iter()
        .enumerate()
        .filter(|(_, candidate)| is_a_page_number(candidate))
        .map(|(i, _)| i)
        .collect();
    if pages.len() >= MIN_PAGES {
        marked.extend(clusters(&candidates).into_iter().flatten());
    }

    for candidate in marked {
        let candidate = &candidates[candidate];
        blocks[candidate.block].kind = if candidate.top {
            BlockKind::Header
        } else {
            BlockKind::Footer
        };
    }
}

/// Whether one page on its own says this band is a page number.
///
/// The foot of the page, and a figure in what it says. [`normalise`] has
/// already written every run of digits -- and a line that is nothing but a
/// roman numeral -- as `#`, so the question is whether the key carries one:
/// `Page 4 of 12`, `iv`, `SHENANDOAH TELECOMMUNICATIONS COMPANY 22`, and the
/// bare `41` in the corner of an annual report.
///
/// AND NOT A FOOTNOTE, which is the one thing at the foot of a page that opens
/// with a figure of its own. What separates them is where the line starts: a
/// footnote is set in the body's measure and a folio stands clear of it. The
/// discriminator was chosen against the annotated sample and against the
/// footnote case this suite already had; on the sample it costs two running
/// feet and saves one body block, and without it the footnote goes.
///
/// EVERYTHING ELSE IS LEFT TO THE REPETITION RULE, and the head above all. A
/// heading standing over its own white at the top of a page is what the same
/// signal marks there, and the sample of #579 says how often: 26 body blocks
/// on 200 pages, and the heading axis from 0.465 to 0.404. A rule that takes a
/// section heading out of the body to save a running head has made the
/// document worse.
fn is_a_page_number(candidate: &Candidate) -> bool {
    !candidate.top && candidate.key.contains('#') && !candidate.in_the_body_measure
}

/// The candidates at one end of one page, or nothing where that end holds no
/// band that is set off from the body.
fn band(blocks: &[Block], page: usize, on_page: &[usize], top: bool) -> Vec<Candidate> {
    if on_page.len() < 2 {
        // A page of one block has no body to stand apart from.
        return Vec::new();
    }
    let extent = extent(blocks, on_page);
    let (width, height) = (extent[2] - extent[0], extent[3] - extent[1]);
    if width <= 0.0 || height <= 0.0 || !width.is_finite() || !height.is_finite() {
        return Vec::new();
    }

    // The outermost line, and everything that shares its band: a running head
    // and the page number beside it are one band and have to be found together.
    let edge = if top {
        on_page
            .iter()
            .map(|i| blocks[*i].bbox[3])
            .fold(f64::NEG_INFINITY, f64::max)
    } else {
        on_page
            .iter()
            .map(|i| blocks[*i].bbox[1])
            .fold(f64::INFINITY, f64::min)
    };
    // A BAND MAY BE MORE THAN ONE LINE (#584). It grows inward from the
    // outermost line while the next line stands within a line and a quarter of
    // it AND stops short of the measure; the second clause is what tells the
    // second line of a running head from the first line of the body. The white
    // below is then measured from the whole band, which is the white a reader
    // sees.
    let measure = on_page
        .iter()
        .map(|i| blocks[*i].bbox[2] - blocks[*i].bbox[0])
        .fold(0.0_f64, f64::max);
    let mut inward: Vec<usize> = on_page.to_vec();
    inward.sort_by(|a, b| {
        let (first, second) = if top {
            (blocks[*b].bbox[3], blocks[*a].bbox[3])
        } else {
            (blocks[*a].bbox[1], blocks[*b].bbox[1])
        };
        first.partial_cmp(&second).unwrap_or(Ordering::Equal)
    });
    let mut band: Vec<usize> = Vec::new();
    let mut frontier = edge;
    for i in inward.into_iter().take(BAND_MAX_LINES) {
        let bbox = blocks[i].bbox;
        let line = (bbox[3] - bbox[1]).max(f64::MIN_POSITIVE);
        let reaches = if top {
            bbox[3] >= frontier - BAND_LINE_REACH * line
        } else {
            bbox[1] <= frontier + BAND_LINE_REACH * line
        };
        let short = measure <= 0.0 || (bbox[2] - bbox[0]) < BAND_SHORT_FRACTION * measure;
        // AND IT STANDS OVER WHAT IS ALREADY IN THE BAND. The second line of a
        // running head is set above the first; a caption that happens to sit
        // by the folio is BESIDE it, across the page, and belongs to the body.
        // That is the same axis #579's rail measures the white on, and without
        // it this rule takes the caption of
        // `a_folio_beside_a_caption_is_not_measured_against_the_caption` out
        // of the body.
        let over = band.iter().any(|j| {
            let other = blocks[*j].bbox;
            bbox[2].min(other[2]) > bbox[0].max(other[0])
        });
        if !reaches || (!band.is_empty() && (!short || !over)) {
            break;
        }
        band.push(i);
        frontier = if top { bbox[1] } else { bbox[3] };
    }
    let body: Vec<usize> = on_page
        .iter()
        .copied()
        .filter(|i| !band.contains(i))
        .collect();
    if band.is_empty() || body.is_empty() {
        return Vec::new();
    }

    // The white between the two. Measured band edge to body edge, because that
    // is the white a reader sees, and required to be most of a line of the
    // band's own type.
    //
    // AGAINST WHAT STANDS OVER THE BAND, and not against every other block on
    // the page. A folio in the outer corner is a whole margin away from the
    // text, and a caption set in the middle of the same horizontal strip is
    // beside it rather than above it: measured against that caption the margin
    // reads as a NEGATIVE gap and the folio stays in the body. On the
    // annotated sample of #579 that is 66 of the 128 running heads the rule
    // missed (codex would call it an off-by-one; it is an off-by-axis).
    let band_height = band
        .iter()
        .map(|i| blocks[*i].bbox[3] - blocks[*i].bbox[1])
        .fold(0.0_f64, f64::max);
    let (band_left, band_right) = (
        band.iter()
            .map(|i| blocks[*i].bbox[0])
            .fold(f64::INFINITY, f64::min),
        band.iter()
            .map(|i| blocks[*i].bbox[2])
            .fold(f64::NEG_INFINITY, f64::max),
    );
    let over: Vec<usize> = body
        .iter()
        .copied()
        .filter(|i| {
            let bbox = blocks[*i].bbox;
            bbox[2].min(band_right) > bbox[0].max(band_left)
        })
        .collect();
    // NOTHING OVER IT IS NOT NO ANSWER. A band with no block above or below it
    // anywhere across its own width is set off by the whole of the page, which
    // is the most white a band can have rather than the least.
    let gap = if over.is_empty() {
        f64::INFINITY
    } else if top {
        band.iter()
            .map(|i| blocks[*i].bbox[1])
            .fold(f64::INFINITY, f64::min)
            - over
                .iter()
                .map(|i| blocks[*i].bbox[3])
                .fold(f64::NEG_INFINITY, f64::max)
    } else {
        over.iter()
            .map(|i| blocks[*i].bbox[1])
            .fold(f64::INFINITY, f64::min)
            - band
                .iter()
                .map(|i| blocks[*i].bbox[3])
                .fold(f64::NEG_INFINITY, f64::max)
    };
    if gap.is_nan() || gap < BAND_GAP_FRACTION * band_height {
        return Vec::new();
    }

    let body_left = body
        .iter()
        .map(|i| blocks[*i].bbox[0])
        .fold(f64::INFINITY, f64::min);
    band.into_iter()
        .filter(|i| eligible(&blocks[*i]))
        .filter_map(|i| {
            let bbox = blocks[i].bbox;
            let key = normalise(&blocks[i].text);
            if key.is_empty() {
                return None;
            }
            Some(Candidate {
                block: i,
                page,
                top,
                y: (extent[3] - (bbox[1] + bbox[3]) / 2.0) / height,
                x: ((bbox[0] + bbox[2]) / 2.0 - extent[0]) / width,
                key,
                in_the_body_measure: (bbox[0] - body_left).abs() <= BODY_MEASURE_TOLERANCE * width,
            })
        })
        .collect()
}

/// Whether a block may be furniture at all.
fn eligible(block: &Block) -> bool {
    matches!(block.kind, BlockKind::Paragraph)
        && block.text.chars().count() <= MAX_CHARS
        && !block.text.trim().is_empty()
}

/// The box every block on a page stands in.
fn extent(blocks: &[Block], on_page: &[usize]) -> [f64; 4] {
    let mut extent = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for i in on_page {
        let bbox = blocks[*i].bbox;
        extent[0] = extent[0].min(bbox[0]);
        extent[1] = extent[1].min(bbox[1]);
        extent[2] = extent[2].max(bbox[2]);
        extent[3] = extent[3].max(bbox[3]);
    }
    extent
}

/// What a line says, with everything that changes from page to page taken out.
///
/// Lower case, one space between words, every run of digits one `#`, and a
/// line that is nothing but a roman numeral the same `#` -- which is what
/// makes `ii`, `iii` and `iv` at the foot of three pages one group. The roman
/// rule is deliberately the WHOLE line and not a token inside it: `Chapter V`
/// is the name of a chapter and stays what it says.
fn normalise(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_digits = false;
    let mut spaced = true;
    for c in text.chars() {
        if c.is_ascii_digit() {
            if !in_digits {
                out.push('#');
                in_digits = true;
                spaced = false;
            }
            continue;
        }
        in_digits = false;
        if c.is_whitespace() {
            if !spaced {
                out.push(' ');
                spaced = true;
            }
            continue;
        }
        spaced = false;
        out.extend(c.to_lowercase());
    }
    let out = out.trim().to_string();
    if is_roman_numeral(&out) {
        return "#".to_string();
    }
    out
}

/// Whether the whole line is a roman numeral as a typesetter writes one.
///
/// THE SYNTAX AND NOT THE ALPHABET. `Civil`, `Mild` and `Livid` are spelt out
/// of nothing but the seven letters, and a membership test called each of them
/// a page number -- so three pages that open with three such words at the same
/// place lost all three out of the body (codex, round 2). This reads the
/// numeral instead: thousands, then hundreds, tens and units, each of them the
/// subtractive pair, or the five, or up to three of the one, and the line has
/// to end where the numeral does.
fn is_roman_numeral(line: &str) -> bool {
    let digits: Vec<char> = line.chars().collect();
    if digits.is_empty() {
        return false;
    }
    let mut at = 0usize;
    let mut thousands = 0;
    while digits.get(at) == Some(&'m') && thousands < 3 {
        at += 1;
        thousands += 1;
    }
    at = power(&digits, at, 'c', 'd', 'm');
    at = power(&digits, at, 'x', 'l', 'c');
    at = power(&digits, at, 'i', 'v', 'x');
    at == digits.len()
}

/// One power of ten of a roman numeral: `one`, the `five` above it and the
/// `ten` above that. Returns where the numeral has got to.
fn power(digits: &[char], at: usize, one: char, five: char, ten: char) -> usize {
    let pair = |from: usize, second: char| {
        digits.get(from) == Some(&one) && digits.get(from + 1) == Some(&second)
    };
    if pair(at, ten) || pair(at, five) {
        return at + 2;
    }
    let mut at = at;
    if digits.get(at) == Some(&five) {
        at += 1;
    }
    let mut ones = 0;
    while digits.get(at) == Some(&one) && ones < 3 {
        at += 1;
        ones += 1;
    }
    at
}

/// The groups of candidates that say the same thing in the same place on at
/// least [`MIN_PAGES`] pages.
///
/// Grouped by what they say first, and then clustered by position inside that
/// group -- EVERY cluster that reaches the count, not the biggest one. A book
/// sets its running head at the OUTER margin, so one line of text stands on
/// the left of every verso and the right of every recto: two clusters of one
/// text, and keeping the larger would leave half the pages' heads in the body
/// (codex, round 1). The clusters are taken one at a time, biggest first, and
/// each one's members are out of the running for the next -- so a candidate is
/// in one cluster and a document that repeats one line at the top of every
/// page AND again in a box halfway down still gets two answers rather than one
/// smeared over both places.
fn clusters(candidates: &[Candidate]) -> Vec<Vec<usize>> {
    let mut by_text: BTreeMap<(bool, &str), Vec<usize>> = BTreeMap::new();
    for (i, candidate) in candidates.iter().enumerate() {
        by_text
            .entry((candidate.top, candidate.key.as_str()))
            .or_default()
            .push(i);
    }

    let mut out = Vec::new();
    for (_, mut members) in by_text {
        while members.len() >= MIN_PAGES {
            let Some((best, pages)) = best_cluster(candidates, &members) else {
                break;
            };
            if pages < MIN_PAGES {
                break;
            }
            members.retain(|i| !best.contains(i));
            out.push(best);
        }
    }
    out
}

/// The largest cluster of `members` that sit within tolerance of one of them,
/// with the number of PAGES it covers -- which is what the count is about, a
/// head and a page number on one page being one page and not two.
///
/// Only the first [`MAX_SEEDS`] members are tried as the centre. The work is
/// quadratic in the size of the group, and the group is as long as the
/// document: a thousand pages of one running head is a million comparisons,
/// which is nothing, and a hundred thousand is not. A running head stands in
/// the same place on every page that carries it, so a cluster that the first
/// few dozen candidates cannot find is not a running head.
fn best_cluster(candidates: &[Candidate], members: &[usize]) -> Option<(Vec<usize>, usize)> {
    let mut best: Vec<usize> = Vec::new();
    let mut best_pages = 0usize;
    for seed in members.iter().take(MAX_SEEDS) {
        let near: Vec<usize> = members
            .iter()
            .copied()
            .filter(|i| {
                (candidates[*i].y - candidates[*seed].y).abs() <= POSITION_TOLERANCE
                    && (candidates[*i].x - candidates[*seed].x).abs() <= SIDE_TOLERANCE
            })
            .collect();
        let mut on: Vec<usize> = near.iter().map(|i| candidates[*i].page).collect();
        on.sort_unstable();
        on.dedup();
        if on.len() > best_pages {
            best_pages = on.len();
            best = near;
        }
    }
    if best.is_empty() {
        return None;
    }
    Some((best, best_pages))
}
