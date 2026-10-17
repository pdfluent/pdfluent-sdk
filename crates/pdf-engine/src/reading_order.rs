//! The order a document is read in, across columns and across pages (#571).
//!
//! WITHIN A PAGE the order is already decided, and this does not decide it
//! again. `crate::text` cuts the page on its LINES and `crate::blocks` keeps
//! the groups in the order that cut produced, so the blocks of a two-column
//! page come back one column at a time -- the column gaps of #567, which is
//! what this ticket asks the order to be built on. A second cut over the
//! blocks was written and measured against that one; what it changed is in
//! the ticket, and it is not shipped, because a coarser instrument re-deciding
//! what a finer one already decided is a way of losing an order, not of
//! gaining one.
//!
//! What IS decided here is the order of the document, and there are two
//! statements the pages cannot make about themselves.
//!
//! * THE FURNITURE IS NOT IN THE FLOW. A running head is the first block of
//!   its page and a footer the last (#571, `crate::furniture`), so between the
//!   last paragraph of one page and the first of the next stand two lines that
//!   belong to neither. They are kept -- with their text, their box and their
//!   page -- after the last block of the body, so a caller reading the blocks
//!   in order reads the document and then the lines it repeats.
//! * A PARAGRAPH THE PAGE BROKE IN TWO is two blocks on two pages, and
//!   nothing on either page can join them. The evidence is the pair: the last
//!   block of one page ends without a full stop and the first of the next
//!   opens in lower case. Set in the same type, which is what keeps a footnote
//!   out -- a footnote is the last thing on its page, often breaks off
//!   mid-sentence, and is set smaller than the body it hangs from.
//!
//! The join is a RELATION and not a merge: [`Block::continues_previous`] says
//! it, `crate::markdown` writes the two halves as one paragraph, and both
//! halves keep the box and the page number they were drawn with -- which one
//! merged block could not state, having one of each.
//!
//! Numbering is last, so [`Block::index`] is the reading order of the body
//! with no gap where the furniture came out of it.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use crate::blocks::{dominant_font_size, Block, BlockKind};

/// The characters that end a sentence, and therefore a paragraph the next page
/// does not continue.
///
/// The closing quotes and brackets are in the list because a paragraph that
/// ends `... the Act."` ends on the quote and not on the stop.
const TERMINAL: &[char] = &[
    '.', '!', '?', ':', ';', '"', '\'', '\u{201d}', '\u{2019}', ')', ']', '\u{00bb}', '\u{3002}',
    '\u{ff01}', '\u{ff1f}',
];

/// How far two blocks' type may differ and still be one paragraph, as a
/// fraction. The same tenth `crate::blocks` merges lines under, and the rail
/// that keeps a footnote from swallowing the next page's first paragraph.
const SIZE_TOLERANCE: f64 = 0.10;

/// Put `blocks` in the document's reading order and number them from
/// `base_index` (#571).
///
/// Consumes and returns the vector because the order is the whole of what it
/// does: the furniture moves to the end and the body keeps the order the page
/// cut gave it.
pub(crate) fn arrange(blocks: Vec<Block>, base_index: usize) -> Vec<Block> {
    let (mut body, furniture): (Vec<Block>, Vec<Block>) = blocks
        .into_iter()
        .partition(|block| !matches!(block.kind, BlockKind::Header | BlockKind::Footer));

    mark_continuations(&mut body);
    body.extend(furniture);
    for (n, block) in body.iter_mut().enumerate() {
        block.index = base_index + n;
    }
    body
}

/// Mark the block at each page break that continues the one before it.
///
/// Over the BODY, after the furniture has gone: the pair the evidence is about
/// is the last paragraph of one page and the first of the next, and before
/// this a running head stood between them.
fn mark_continuations(blocks: &mut [Block]) {
    for i in 1..blocks.len() {
        // THE NEXT PAGE, not the next page that has a body on it. A page whose
        // blocks are all furniture, or which carries no text at all, is a
        // blank verso -- and a blank page in a book is a break rather than a
        // hyphenless continuation, so a pair with a page between them is two
        // paragraphs (codex, round 1).
        if blocks[i].page != blocks[i - 1].page + 1 {
            continue;
        }
        if continues(&blocks[i - 1], &blocks[i]) {
            blocks[i].continues_previous = true;
        }
    }
}

/// Whether `next`, the first block of its page, continues `previous`, the last
/// block of the page before it.
///
/// LOWER CASE IS THE SIGNAL, so a script that has no case -- Chinese, Arabic,
/// Hebrew -- never joins here. That is the conservative direction: the cost of
/// refusing is a paragraph in two pieces, which is what the reading was
/// before this, and the cost of accepting wrongly is two paragraphs run
/// together. A signal for those scripts is a question of its own.
fn continues(previous: &Block, next: &Block) -> bool {
    if !matches!(previous.kind, BlockKind::Paragraph) || !matches!(next.kind, BlockKind::Paragraph)
    {
        return false;
    }
    let Some(last) = previous.text.trim_end().chars().next_back() else {
        return false;
    };
    if TERMINAL.contains(&last) {
        return false;
    }
    let Some(first) = next.text.trim_start().chars().next() else {
        return false;
    };
    if !first.is_lowercase() {
        return false;
    }
    // Set in the same type, or the second is something other than the body of
    // the first: a footnote, a caption, a running head the repetition rule did
    // not reach because the document said it only twice.
    let (a, b) = (
        dominant_font_size(previous.spans.iter()),
        dominant_font_size(next.spans.iter()),
    );
    a > 0.0 && b > 0.0 && (a - b).abs() <= SIZE_TOLERANCE * a.max(b)
}
