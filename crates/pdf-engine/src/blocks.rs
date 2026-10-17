//! Paragraphs out of the lines a page draws (#567).
//!
//! [`crate::text`] turns a page's glyphs into lines: one [`TextBlock`] per band,
//! in the order an XY-cut says the page is read in. That is where extraction
//! stopped. A line is not a unit anybody chunks, cites or embeds -- the unit is
//! the paragraph, and the line breaks inside it are an accident of the measure
//! the typesetter chose.
//!
//! So this module merges lines into [`Block`]s. It decides on geometry the page
//! already carries, never on the words: leading, the left edge, the right edge,
//! the type size, and the rules the document itself draws. Two lines join when
//! nothing on that list says they are apart.
//!
//! WHAT IT REFUSES TO CROSS, and why each one is a case rather than a rule
//!
//! * A COLUMN GAP. Lines from two columns are never adjacent in the list the
//!   XY-cut produces, but the last line of the left column and the first of the
//!   right one are -- and the second runs UP the page. A pair that does not
//!   descend, or whose horizontal extents barely meet, is two columns meeting.
//! * A TABLE. The page's own ruling lines say "rows" where no geometry of the
//!   text can (#545), and an unruled table says it a second way: two lines with
//!   a wide gap at the same x is a gutter between cells, which is the one thing
//!   justified prose -- the failure case this has to survive -- cannot produce
//!   twice at the same place.
//! * A CHANGE OF TYPE. A heading, a caption and a footnote are set in something
//!   other than the body, and a paragraph that swallows one loses both. Size
//!   and WEIGHT both count: a document set in one size says "heading" with its
//!   bold face and nothing else, and a merge deaf to weight hands #568 a
//!   paragraph with the heading inside it.
//! * A LIST ITEM. The commonest list of all -- one line per item, set solid at
//!   one left edge -- differs from a paragraph in nothing the rails above
//!   measure, so a merge deaf to markers hands back one block with every bullet
//!   inside it. `crate::lists` says which lines a column has confirmed as
//!   items, and this refuses to merge into one (#569).
//!
//! WHAT IT JOINS THAT LOOKS LIKE A BREAK
//!
//! * A HANGING INDENT: the continuation lines of a bibliography entry are
//!   indented and belong to the entry above them. A FIRST-LINE INDENT is the
//!   same measurement with the opposite meaning, so the two are told apart once
//!   per column, from what sits ABOVE each indented line: a first-line indent
//!   follows the short last line of the paragraph before it, a hanging
//!   continuation follows a line that ran the full measure.
//! * A DROP CAP: an initial set three lines deep is drawn in the band of the
//!   third line, so the naive reading puts it in the middle of its own
//!   paragraph. It is taken out of that line's geometry and put back at the
//!   front of the text.
//!
//! The kind is [`BlockKind::Paragraph`] or, where the merge stopped at a
//! confirmed marker, [`BlockKind::ListItem`]. A heading is decided afterwards
//! instead: `crate::headings` reads the blocks a second time (#568), because
//! that evidence is the whole DOCUMENT's typography where a list's is one
//! column's. A TABLE is taken out before the merge runs: `crate::tables` reads
//! each region's lines for the grid they stand in, those lines never reach the
//! merge, and they come out one block per cell (#570). Running heads (#571)
//! add their own variant, which is what `#[non_exhaustive]` is for.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use std::cmp::Ordering;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use crate::error::{EngineError, OptionsError, Result};
use crate::lists::{self, Marker};
use crate::table_trace::{LearnedTrace, TableOrigin};
use crate::tables;
use crate::text::{is_hyphen_wrap_candidate, HorizontalRule, TextBlock, TextSpan};

/// How far apart two baselines may be, as a multiple of the leading in force,
/// and still belong to one paragraph.
///
/// Extra space between paragraphs is conventionally half a line or more, so
/// 1.5 is where the two populations sit; the threshold is under it because a
/// leading estimated from a page's tightest quartile runs a little low, and a
/// paragraph wrongly split is a worse answer than two wrongly joined -- the
/// first loses a sentence's context, the second only its boundary.
const PARAGRAPH_LEADING_TOLERANCE: f64 = 1.35;

/// The largest baseline step that can be running text, as a multiple of the
/// type size, whatever the column's own leading says.
///
/// [`run_leading`] measures what the column IS set on, which is what tells a
/// paragraph break from a line break -- and on a page set entirely at a
/// paragraph's distance it measures the paragraph distance and merges the page.
/// `Overlayed-with-rot0` is that page: eleven-point type on a twenty-five-point
/// pitch, which mutool and poppler both read as one block per line and this read
/// as one block. So there is a second rail, and it is typographic rather than
/// measured: running text is set between about 1.1 and 1.5 times its own size,
/// and white the depth of most of another line is a break however regularly it
/// recurs.
const MAX_LEADING_EM: f64 = 1.8;

/// The largest baseline step at which a line under an item's hang is still
/// the item's, as a multiple of the type size (#599).
///
/// [`MAX_LEADING_EM`] is right about a line of prose and wrong about a
/// bibliography set double-spaced: `[2] A. Mizel, ... Phys. Rev. A.` on one
/// line and `Rapid Comm., 63:40302, 2001` two ems below it, starting exactly
/// where the item's text starts. The hang is the list's own statement that
/// the line continues the item -- the next item would start at the marker --
/// so under it a double-spaced step is allowed, and nothing wider.
///
/// What it cannot tell: a list set double-spaced throughout, followed two ems
/// down by a paragraph set at the list's indent -- an EU supply contract in
/// DocLayNet's validation split. Its geometry is the bibliography's to the
/// point (the item-to-item pitch is the same two ems), and only the words
/// differ. On the 34 validation documents that is one item lost against 43
/// found (#599).
const HANG_MAX_LEADING_EM: f64 = 2.2;

/// The smallest baseline step that counts as "the next line down", as a
/// multiple of the type size. Below it the two lines are the same line: a
/// superscript, a sub-baseline rule of a formula, or a band the cut split.
const MIN_BASELINE_STEP_EM: f64 = 0.25;

/// How much of the narrower line's width has to lie under the wider one before
/// the two count as being in the same text column.
const COLUMN_OVERLAP_FRACTION: f64 = 0.35;

/// How far the type size may differ inside one paragraph, as a fraction.
/// A tenth admits the rounding of a size that arrives through a text matrix and
/// refuses the step from body to heading, caption or footnote.
const SIZE_TOLERANCE: f64 = 0.10;

/// How far short of the measure a line has to fall, in ems, before its full
/// stop is read as the end of the paragraph rather than the end of a sentence.
const SHORT_LINE_EM: f64 = 2.5;

/// How far apart two baselines may be and still be one: the rounding a text
/// matrix arrives through, and nothing else.
const SAME_BASELINE: f64 = 1e-6;

/// How much of a run's row pitch a line may sit below the row above it and
/// still belong to that row. Three quarters: a cell's wrapped line sits at the
/// leading, and a table is set on a pitch of a line plus its cells' padding.
const ROW_PITCH_FRACTION: f64 = 0.75;

/// How much wider than the step under it a step has to be before the run is
/// read as having two kinds of step in it: the leading a cell wraps at, and
/// the pitch the rows are set on.
///
/// Two fifths more. A table of descriptions sets its lines at the leading and
/// parts its rows by half a line again, which is what a reader sees the rows
/// by; a table whose cells are one line each has one step and no jump, and
/// asking this of it answers no rather than guessing a rail. Lower and the
/// rounding between two nominally equal steps becomes a row boundary.
const ROW_PITCH_JUMP: f64 = 1.4;

/// How many steps above the jump there have to be before the wider side is a
/// row pitch rather than a single gap -- the space under a header, or the end
/// of the table. Two.
const ROW_PITCH_CLUSTER: usize = 2;

/// How far from a cell's left edge, in ems, a line may open and still be read
/// as continuing that cell. Half: a cell's second line is set at the cell's
/// own edge to the point, and the tolerance is for the rounding a text matrix
/// arrives through and for a leading space.
const CELL_LEFT_EM: f64 = 0.5;

/// The smallest left-edge step that counts as an indent, in ems.
const INDENT_MIN_EM: f64 = 0.8;

/// How close two left edges have to be, in ems, to count as the same edge.
const INDENT_MATCH_EM: f64 = 0.35;

/// The smallest gap inside a line, in ems, that can be a gutter between cells
/// rather than a word space. Justified setting stretches a word space to
/// perhaps an em and a half on a bad line; three ems is past anything a
/// justifier produces, and it still has to happen at the same x on a second
/// line before it is believed.
const TABULAR_GAP_EM: f64 = 3.0;

/// The longest text an oversized initial may hold before it stops being an
/// initial. Two, because a drop cap is sometimes a letter and its quotation
/// mark.
const INITIAL_MAX_CHARS: usize = 2;

/// How much larger than the line it sits in an initial has to be set.
const INITIAL_SIZE_RATIO: f64 = 1.8;

/// How much smaller than the line a figure has to be set before it is raised
/// out of it rather than part of it (#578).
///
/// Nine tenths: a reference number is set at six or seven point over ten-point
/// type, and the floor is high enough to admit the small end of that while
/// keeping a line whose first span is merely a hair smaller out.
const RAISED_SIZE: f64 = 0.9;

/// How far above the line's own baseline a raised figure sits, in ems of the
/// line (#578). A tenth: a superscript is set a third of an em up and a
/// baseline that wandered by rounding is nowhere near it.
const RAISED_RISE: f64 = 0.1;

/// The most characters a raised figure may hold and still be a marker (#578).
///
/// Three, which is what `lists::MAX_ORDINAL_DIGITS` reads: a bibliography
/// passes a hundred entries often enough, and a longer run of small type at
/// the head of a line is a formula rather than a number.
const MAX_ORDINAL_CHARS: usize = 3;

/// The gap after a glyph before the page is setting it apart as a marker, in
/// ems of the line (#578).
///
/// Two fifths, which is wider than any word space -- a quarter of an em is the
/// usual one -- and narrower than the hanging indent a list of any kind sets
/// its text at.
const MARKER_GAP_EM: f64 = 0.4;

/// The longest a run of picture-font glyphs may be and still be a list marker
/// rather than a line of the document (#569). Two, because a marker is a glyph
/// and sometimes a glyph and its space; a formula set in Symbol opens a line in
/// a picture font too, and it is not an item.
const MARKER_MAX_CHARS: usize = 2;

/// How far apart the left edges of two one-line blocks may stand, in ems, and
/// still be read as one narrow column by the line join below (#574).
///
/// Half an em: the rounding a text matrix arrives through, and a ragged edge's
/// own jitter. A first-line indent (0.8 em and up) never clears it, which is
/// what keeps two one-line paragraphs of an indented page apart.
const NARROW_X0_EM: f64 = 0.5;

/// How far apart the baselines of a narrow-column join may stand, as a multiple
/// of the pitch of the block above (#574).
///
/// Just over one: the join is for lines the cut sliced at their own leading,
/// not for lines the typesetter spaced apart. The absolute ceiling stays
/// [`MAX_LEADING_EM`], whatever the pitch measures.
const NARROW_PITCH_MULT: f64 = 1.2;

/// How far apart the type sizes of a narrow-column join may stand, in points
/// (#574). Half a point: the rounding of a size that arrives through a text
/// matrix, and nothing a reader would call a step.
const NARROW_SIZE_PT: f64 = 0.5;

/// The pitch a one-line block is credited with where its run measures none
/// (#574). Solid setting, the same 1.16 [`run_leading`] falls back on: a run
/// of one line has no step to measure, and the page's own quartile is the
/// offset between two interleaved columns rather than any leading at all.
///
/// Shared with the v2 release rejoin, which credits a refused table's column
/// the same pitch where the column measures none.
pub(crate) const SOLID_PITCH_EM: f64 = 1.16;

/// What the heuristic table builder had to go on (#574).
///
/// The builder knows when it is guessing, and until now it threw that away: the
/// fill ratio was computed and reduced to a pass, and the rest was never recorded.
/// These are the facts it has at the moment it decides, for two consumers -- a
/// caller deciding whether to send a page to a vision model (#624), and the
/// learned structure model of #600 deciding whether it may overrule the grid.
///
/// NOT COLLAPSED INTO A SCORE, which is why there are five numbers and not one. A
/// weighted worst-of-five would be a hand-made value that reads as a measurement,
/// and nobody could test which part of it was wrong. Each of these is a fact with
/// a unit, and each consumer writes its own rule over them where the rule can be
/// read and argued with. If a scalar is ever wanted, it can be fitted against an
/// outcome and say what it was fitted on.
///
/// A PROPERTY OF THE TABLE, not of the cell that carries it. Every cell of one
/// table reports the same value, because a cell is where a model looks and a
/// consumer holding the cells has no other handle on the table they came from.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct GridDoubt {
    /// How many rows were admitted with too few runs of ink to be a row on their
    /// own, through the rule that lets a section head stand inside a table
    /// (#576). Each is a row the builder kept on the strength of its neighbours
    /// rather than of its own shape.
    pub thin_rows: usize,
    /// The share of the grid's cells the page drew ink in, between 0 and 1.
    /// `MIN_FILL` is the floor a table has to clear, so a grid barely above it is
    /// one the builder nearly refused.
    pub fill: f64,
    /// Which attempt produced the grid: 0 for the run's own first row, 1 or 2
    /// where the first rows had to be trimmed. Above 0 means the first candidate
    /// was not a grid.
    pub retried_from: usize,
    /// How many rows have ink in every column, which is what the column
    /// boundaries are derived from. A table whose bounds rest on one full row has
    /// almost no evidence for where its columns are.
    pub full_rows: usize,
    /// How many columns the grid has. Every fact above means something different
    /// in a two-column table than in a nine-column one, so the scale travels with
    /// them rather than being guessed at by the consumer.
    pub columns: usize,
}

impl Block {
    /// A block with the fields every kind has, and the rest left at their
    /// defaults: no confidence, no grid evidence, not a continuation.
    ///
    /// The way to build one from outside this crate, now that [`Block`] is
    /// `#[non_exhaustive]`. The five arguments are the ones no block can do
    /// without -- a kind, a page, a box, its text and its place in the reading
    /// order -- and everything else is set afterwards on the returned value.
    pub fn new(kind: BlockKind, page: usize, bbox: [f64; 4], text: String, index: usize) -> Self {
        Self {
            kind,
            page,
            bbox,
            text,
            spans: Vec::new(),
            index,
            continues_previous: false,
            confidence: None,
            grid: None,
            table_origin: TableOrigin::Other,
            trace_learned: None,
        }
    }
}

/// The kind of a block. Paragraphs (#567), headings (#568) and list items
/// (#569); tables and running heads arrive with theirs (#570, #571).
///
/// `#[non_exhaustive]` because those variants land in a published crate: a
/// caller that matches on this must keep a fallback arm, and adding a kind must
/// not be a breaking change.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BlockKind {
    /// Running text: one or more lines the page sets as one paragraph.
    Paragraph,
    /// A heading, at a level from 1 to 3 (#568).
    ///
    /// The level is the block's rank among the type the DOCUMENT sets above its
    /// own body, never a point size: see `crate::headings`. Levels below the
    /// third fold into it, so `level` is always 1, 2 or 3 and a consumer
    /// mapping it to `#`..`######` never runs off the end.
    Heading {
        /// 1, 2 or 3.
        level: u8,
    },
    /// One item of a list (#569).
    ///
    /// ONE BLOCK PER ITEM, not one per list. The item is what a caller chunks,
    /// cites and embeds -- the same unit every other kind here is -- and a
    /// container kind would need a second shape of [`Block`], with its own text
    /// and its own box, for a grouping that consecutive items already say. A
    /// list is therefore a run of adjacent blocks of this kind, and the
    /// markdown serializer reads it as one.
    ///
    /// The marker itself is NOT in [`Block::text`]: a bullet and a number are
    /// what the page used to say "item", and a consumer that wanted them back
    /// would have to strip them again before indexing the words. It is still in
    /// [`Block::spans`], which carry what the page drew rather than what it
    /// said. The one exception is an outline's number -- `II.4.1`, `10.2` --
    /// which stays at the head of the text, because it is the clause's name as
    /// much as its marker: a cross-reference cites it, and no one ordinal can
    /// say it (#599).
    ListItem {
        /// Nesting depth, 0 for the outermost list. From the indent steps the
        /// column itself uses, never a measurement in points, and never more
        /// than four: deeper steps fold into the fourth, the way heading
        /// clusters below the third fold into level 3.
        level: u8,
        /// The number the document wrote -- `3.` is 3, `c)` is 3, `iv.` is 4 --
        /// or `None` where the item was marked with a bullet, or numbered by
        /// its place in an outline (`II.4.1`), which no one number says and
        /// which the item's text therefore keeps.
        ///
        /// The document's own number and not a count of this list's items, so a
        /// list that opens at 5, or one the extraction lost an item out of,
        /// still says what the page says.
        ordinal: Option<u32>,
    },
    /// One cell of a table (#570).
    ///
    /// ONE BLOCK PER CELL, for the reason a list is one block per item: the
    /// cell is what a caller chunks, cites and embeds, and a container kind
    /// would need a second shape of [`Block`] -- its own text, its own box --
    /// for a grouping that `row`, `column` and a run of adjacent blocks already
    /// say. A table is therefore a run of these in ROW-MAJOR order, opening at
    /// `row: 0, column: 0`, and `crate::markdown` reads such a run back as one
    /// table.
    ///
    /// The grid is the whole table's, not the row's: `column` is a column of
    /// the table, so the third cell of a row whose second cell is missing is at
    /// column 2 and not at column 1.
    TableCell {
        /// 0-based row inside the table.
        row: u16,
        /// 0-based column inside the table.
        column: u16,
        /// How many rows the cell covers, 1 unless the document ruled two rows
        /// together by breaking the rule between them over this column.
        row_span: u16,
        /// How many columns the cell covers, 1 unless its words cross a column
        /// boundary the rest of the table keeps.
        column_span: u16,
        /// Whether the cell is in the table's header row.
        header: bool,
    },
    /// A running header: the line a document repeats at the TOP of its pages
    /// (#571).
    ///
    /// Not in the reading order. `crate::reading_order` keeps these blocks
    /// after the body rather than dropping them, because what a page repeats
    /// is still something a caller may want to read -- the chapter it is in,
    /// the revision of the form -- and it is the body it must not interrupt.
    Header,
    /// A running footer, including a page number (#571).
    ///
    /// The same block as [`BlockKind::Header`] at the other end of the page:
    /// see there for why it is kept rather than dropped.
    Footer,
    /// A caption: the line or lines that say what a picture or a table beside
    /// them shows (#636). Read by the learned pass from where the page draws
    /// its images; the heuristics never produce one.
    Caption,
    /// A figure: a placed image or a drawn vector cluster on the page (#658).
    ///
    /// Listed late, after the learned pass and the text joins, from the
    /// figure boxes the text pass recorded: placed images first, then the
    /// path clusters [`crate::text::TextExtractionDevice::take_graphics`]
    /// keeps. A page with no text block still carries its placed images --
    /// a scan reads as its figure markers -- but no drawn cluster, and
    /// figures merge into their page's reading order by vertical position.
    ///
    /// [`Block::text`] is empty: a figure says nothing; the Markdown
    /// serializer writes it as an image reference from its page and its
    /// number among that page's figures.
    Figure,
}

/// One block of a document's structured text: what a caller chunks, cites or
/// embeds.
///
/// The fields are the ones the serializers consume (#572): the kind, where it
/// is, what it says, what it was made of, and where it falls in the reading
/// order of the whole document.
///
/// `#[non_exhaustive]`, DECIDED AFTER TWO FIELDS HAD ALREADY BROKEN IT (codex on
/// #1977 and #1989). This is a type callers READ: `extract_blocks` returns it and
/// nothing outside this crate has a reason to build one. But it was an exhaustive
/// public struct in a 1.0.0 crate, so `confidence` and then `grid` each made every
/// downstream literal fail to compile -- and the learned work has more of both
/// coming, one per model that has something to say about a block. Closing it now
/// means the next field costs nobody anything.
///
/// Construct one with [`Block::new`] and set the rest, which is what this crate's
/// own examples do.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct Block {
    /// What this block is.
    pub kind: BlockKind,
    /// 0-based page index the block was drawn on.
    pub page: usize,
    /// Bounding box in PDF user space, `[x0, y0, x1, y1]` with y up and
    /// `y0 < y1`. The union of the glyph boxes of every span in the block.
    pub bbox: [f64; 4],
    /// The block's text, with the line breaks of the measure removed: lines are
    /// joined by a space, and a word the typesetter broke across a line is put
    /// back together.
    pub text: String,
    /// Every span the block is made of, in the order the page DRAWS them: left
    /// to right, line by line.
    ///
    /// Visual and not logical, which matters on a line of Arabic or Hebrew: the
    /// spans carry the geometry, and the geometry is visual, so extraction
    /// leaves them where the page put them and reorders only the text
    /// ([`TextBlock::text`], #436). `text` above is therefore the logical
    /// reading and this is not, and a caller rebuilding the text from the spans
    /// of a right-to-left block has to reorder them itself (codex, round 1).
    pub spans: Vec<TextSpan>,
    /// Position of this block in the document's reading order, counted from the
    /// first block of the first page.
    ///
    /// The running heads a document repeats are not in that order: they carry
    /// the numbers after the last block of the body, which is where
    /// [`crate::reading_order`] puts them (#571).
    pub index: usize,
    /// Whether the block continues the one before it in the reading order,
    /// because the page broke a paragraph in two (#571).
    ///
    /// A RELATION AND NOT A MERGE. The two halves are set on two pages, so one
    /// paragraph there has two boxes and two page numbers, and a merged block
    /// could state neither: [`Self::bbox`] is one rectangle and [`Self::page`]
    /// is one page. The serializers join what this marks -- `crate::markdown`
    /// writes the two as one paragraph -- and a caller chunking the document
    /// joins it the same way, with the geometry of both halves still in front
    /// of it.
    pub continues_previous: bool,
    /// How sure the learned model is of [`Self::kind`], between 0 and 1, or
    /// `None` where no model spoke about this block (#624).
    ///
    /// WHAT `None` MEANS, because it is not "we are unsure". A block that no
    /// model has an opinion about carries the heuristics' verdict, and the
    /// heuristics do not produce a probability -- they produce a decision. So
    /// `None` is "nobody measured this", and a caller that wants to route the
    /// doubtful cases somewhere must treat it as unknown rather than as low.
    ///
    /// Today the only model on master that answers is the list-item one of
    /// #599, so a paragraph that model considered carries its probability and a
    /// heading carries `None` until #599's heading model lands. The value is the
    /// model's own probability for the decision it made, not a calibrated
    /// confidence over all kinds: 0.9 on a list item means the item model put
    /// 0.9 on "this is an item", and says nothing about the alternative kinds.
    ///
    /// AND IT IS THE PROBABILITY OF THE DECISION THAT PRODUCED THIS BLOCK'S KIND,
    /// which is not always the one the block began with (codex on #1977). A block
    /// the opens model cuts into three items has three pieces whose kinds come
    /// from three different lines' probabilities, and each piece reports its own.
    /// A block whose kind the heuristics decided carries `None` even where a model
    /// ran on it and agreed, because a number about a decision nobody acted on
    /// would read as a number about this kind.
    pub confidence: Option<f64>,
    /// What the heuristic builder had to go on when it found this cell's table,
    /// or `None` for a block that is not a table cell (#574).
    ///
    /// See [`GridDoubt`]: five facts rather than a score, and a property of the
    /// table repeated on each of its cells.
    pub grid: Option<GridDoubt>,
    /// Which stage produced this cell's table, for the env-gated table trace
    /// (#636). Stamped where the cells are built and read back where the
    /// trace prints; it never decides what is a table.
    ///
    /// Crate-internal: [`Block`] is `#[non_exhaustive]` and this travels
    /// with the block through every transform, so a rebuilt table reports
    /// its builder and a kept one keeps its stamp.
    pub(crate) table_origin: TableOrigin,
    /// Trace-only record of the learned pass's judge probability and forced
    /// reason for this cell, for the env-gated table trace (#636). Stamped
    /// where the origin is stamped and read back where the trace prints;
    /// it never decides what is a table.
    ///
    /// Crate-internal like [`Block::table_origin`](Self::table_origin).
    pub(crate) trace_learned: Option<LearnedTrace>,
}

#[non_exhaustive]
/// How a change of page reaches the Markdown output of
/// [`PdfDocument::extract_markdown`](crate::PdfDocument::extract_markdown).
///
/// [`ExtractOptions::page_separators`]. Default [`PageSeparators::Comment`]:
/// today's `<!-- page N -->` marker. [`PageSeparators::None`] writes no
/// marker at all; the paragraphs on either side stay separated by a blank
/// line, the way two paragraphs of one page are.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PageSeparators {
    /// A change of page writes `<!-- page N -->` as its own paragraph, `N`
    /// the 0-based [`Block::page`] of the page that opens -- the default, and
    /// what the output has always been.
    #[default]
    Comment,
    /// A change of page writes nothing.
    None,
}

/// Which tier reads the document: the shipping engine, or the layout model
/// beside it (#641).
///
/// Read by [`crate::PdfDocument::extract_markdown`] only: a call that returns
/// blocks ([`crate::PdfDocument::extract_blocks`],
/// [`crate::PdfDocument::extract_docling`]) answers
/// [`OptionsError::BalancedIsMarkdownOnly`] for [`Quality::Balanced`], because
/// the balanced tier assembles markdown and there are no balanced blocks to
/// return.
///
/// `#[non_exhaustive]`: `best` stays a scripts tier for now and arrives here
/// as a new variant later, which must not break a downstream match.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Quality {
    /// The shipping engine: geometric extraction, no model, every build.
    /// The default.
    #[default]
    Fast,
    /// The balanced tier: the layout model (Heron-ft ep03) regions the page
    /// and the engine's own text fills them, through `balanced_markdown` on
    /// a build with the `ort-layout` cargo feature.
    ///
    /// Needs the model and the runtime beside the feature: ONNX Runtime
    /// 1.23.2 under `ort` =2.0.0-rc.13, and the weights installed by an
    /// explicit `pdfluent models install layout` -- nothing is downloaded by
    /// default, ever. Without the feature the call fails with
    /// `MODEL_NOT_AVAILABLE` naming the feature and the install command;
    /// with it but without the files, with `MODEL_NOT_AVAILABLE` naming the
    /// path looked at and the install command. [`ExtractOptions::model_dir`]
    /// points the tier at a scratch directory, and
    /// [`ExtractOptions::keep_furniture`] keeps running heads as paragraphs.
    Balanced,
}

/// TATR table structure, opted into per call (track B, P3).
///
/// Set on [`ExtractOptions`] with [`ExtractOptions::with_tatr`]. When set,
/// every table region is offered to the Table Transformer first: where it
/// fires its cells are the region's table, and where it does not the current
/// grid runs unchanged. Unset, extraction never touches the model and reads
/// byte for byte as without the option.
///
/// The knobs mirror the shared layer's `RuntimeConfig` without naming it:
/// that type only exists behind the `ort` feature, and
/// this one must exist on every build so the option, its refusal, and the
/// test seam below compile everywhere. Behind `ort-tables` the engine builds
/// the runtime config from these fields; without it, asking for TATR is an
/// [`OptionsError::TatrNeedsOrtTables`], never a silent fast run.
///
/// `#[non_exhaustive]`, like every option type here: start from [`Default`]
/// and assign the fields. All fields are `Eq`, so [`ExtractOptions`] keeps
/// its `Eq`.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct TatrOptions {
    /// Explicit model dir; wins over the env var and the default, as the
    /// shared layer's `model_dir` does. Default `None`: resolve it there.
    pub model_dir: Option<PathBuf>,
    /// Explicit ONNX Runtime library; wins over the env var and the
    /// default. Default `None`: resolve it there.
    pub ort_dylib: Option<PathBuf>,
    /// ONNX Runtime intra-op threads per session. Default 4.
    pub intra_threads: usize,
    /// Sessions per model: a pool so parallel pages do not serialise on one
    /// session. Default 1.
    pub sessions_per_model: usize,
    /// TATR acts only on regions from a detector or teacher box. Default
    /// false: every region [`crate::learned::tables::rebuild_region`]
    /// rebuilds is offered to TATR, whether the learned pass or a teacher
    /// box drew it. The switch P4 restricts with once the detector lands.
    pub detector_regions_only: bool,
    /// The P3 test seam: a canned structurer that answers without a model.
    /// Hidden: tests name it, callers never do. When set, the hook answers
    /// from the fake and never loads the model -- on any build.
    #[doc(hidden)]
    pub test_hook: Option<TatrTestHook>,
}

impl Default for TatrOptions {
    fn default() -> Self {
        Self {
            model_dir: None,
            ort_dylib: None,
            intra_threads: 4,
            sessions_per_model: 1,
            detector_regions_only: false,
            test_hook: None,
        }
    }
}

impl TatrOptions {
    /// The shared layer's config these options name.
    #[cfg(feature = "ort-tables")]
    pub(crate) fn runtime_config(&self) -> pdf_ocr::runtime::RuntimeConfig {
        let mut config = pdf_ocr::runtime::RuntimeConfig::new()
            .with_intra_threads(self.intra_threads)
            .with_sessions_per_model(self.sessions_per_model);
        if let Some(dir) = &self.model_dir {
            config = config.with_model_dir(dir.clone());
        }
        if let Some(lib) = &self.ort_dylib {
            config = config.with_ort_dylib(lib.clone());
        }
        config
    }
}

/// The P3 test seam: a canned structurer behind [`TatrOptions::test_hook`].
///
/// Hidden rather than published: integration tests name it to prove the hook
/// wiring without a model or a dylib, and no caller does. `Eq`, so
/// [`TatrOptions`] -- and through it [`ExtractOptions`] -- keeps its `Eq`.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TatrTestHook {
    /// Fire with these cells: they land in the region's table as TATR's own
    /// would, text filled from the region's words.
    Cells(Vec<TatrTestCell>),
    /// Decline: the base output, exactly.
    Declined,
    /// Fail like a per-region inference error: traced, and the region falls
    /// back to the base grid.
    Failed,
}

/// One canned cell of [`TatrTestHook::Cells`]: its grid position and the
/// region words it claims, by index into the region's words.
#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct TatrTestCell {
    /// The rows this cell occupies, ascending, 0 at the top.
    pub rows: Vec<usize>,
    /// The columns this cell occupies, ascending, 0 at the left.
    pub columns: Vec<usize>,
    /// Whether the cell is in the table's header row.
    pub header: bool,
    /// Indices into the region's words, in reading order.
    pub words: Vec<usize>,
}

#[doc(hidden)]
impl TatrTestCell {
    /// A canned cell: grid position, header flag, and claimed word indices.
    pub fn new(rows: Vec<usize>, columns: Vec<usize>, header: bool, words: Vec<usize>) -> Self {
        Self {
            rows,
            columns,
            header,
            words,
        }
    }
}

/// One caller-supplied steering of what counts as a heading (lane L1).
///
/// The document's own statistics decide headings by default; an override
/// relevels the block whose text matches [`Self::text`] exactly, after
/// every other heading decision, so the caller always wins. `level` `None`
/// demotes the block to prose, `Some(n)` promotes it to a heading at that
/// level, clamped to 1..=3 (the levels [`BlockKind::Heading`] carries).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HeadingOverride {
    /// The block text to steer, matched exactly.
    pub text: String,
    /// The level to force, or `None` to force prose.
    pub level: Option<u8>,
}

/// Which part of the document [`crate::PdfDocument::extract_blocks`] and
/// [`crate::PdfDocument::extract_markdown`] hand back (#573).
///
/// A struct rather than two arguments, because this is the parameter both
/// methods take across six languages: a field added here is a field added to
/// every binding at once, and a caller that built it from
/// `ExtractOptions::default()` keeps compiling. `#[non_exhaustive]`, so that
/// is the only way to build one from outside this crate: struct literals are
/// refused, and a field added here cannot break a caller that never named it.
/// Start from [`Default`], [`Self::all`] or [`Self::window`] and assign the
/// fields you want -- the fields stay public and assignable, only the literal
/// is gone. (Unlike [`crate::RenderOptions`], which still takes literals.)
///
/// THE WINDOW IS A WINDOW ON THE ANSWER, NOT ON THE WORK. Whatever it asks for,
/// the document is read whole, because everything the block model says about a
/// block is said against the rest of the document: a size is a heading against
/// what the body is set in (#568), a line is a running head because THREE pages
/// repeat it (#571), and a window that re-derived those from its own pages
/// would call a different thing a heading than the same document does. A caller
/// that wants one page read on its own, and the statistics of that page alone,
/// asks [`crate::PdfDocument::extract_page_blocks`] instead -- which is the
/// cheap path, and says so.
///
/// # Example
///
/// ```no_run
/// use pdf_engine::{ExtractOptions, PdfDocument};
///
/// let document = PdfDocument::open(std::fs::read("report.pdf").unwrap()).unwrap();
///
/// // The whole document.
/// let all = document.extract_blocks(&ExtractOptions::default()).unwrap();
///
/// // Pages 10 to 19, for a caller indexing a long document in windows.
/// let window = document.extract_blocks(&ExtractOptions::window(10, 10)).unwrap();
/// # let _ = (all, window);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ExtractOptions {
    /// The first page of the window, counted from zero. Default 0.
    pub first_page: usize,
    /// How many pages from [`Self::first_page`], or `None` for the rest of the
    /// document. Default `None`.
    pub page_count: Option<usize>,
    /// An explicit list of pages, counted from zero, instead of the
    /// [`Self::first_page`]/[`Self::page_count`] window. Default `None`.
    ///
    /// A SELECTION, NOT AN ORDER: `[2, 0]` hands back the blocks of pages 0
    /// and 2 in the document's reading order, the same order the window
    /// would. Anything else would renumber [`Block::index`] and break
    /// [`Block::continues_previous`], which both speak document order.
    ///
    /// A page past the last one is an error, where a window past the last
    /// page is empty: a window walks a document until the answer runs out,
    /// but a list names its pages and a name that is not there is a
    /// mistake. Setting this beside a window -- `first_page` other than 0
    /// or `page_count` set -- is an error too.
    pub pages: Option<Vec<usize>>,
    /// Whether the learned pass runs over the finished blocks. Default true,
    /// which is what a caller gets.
    ///
    /// FOR A TEST OF A HEURISTIC, AND NOT FOR A CALLER TO TUNE. The rules in
    /// `headings`, `lists` and `blocks` each have operator tests that assert
    /// what THAT rule does; the learned pass is a stage after them with a
    /// switch of its own (`learned::SWITCHES`), and a test of the first that
    /// silently measures the second tells you nothing about either. Every such
    /// test says so where it sets this (#599).
    pub learned: bool,
    /// Whether a tagged PDF's own structure tree may decide kinds and order.
    /// Default false: geometry decides, exactly as without the option.
    ///
    /// When true and the document carries a structure tree that passes the
    /// validity check, heading levels come from `H1`-`H6`, list items from
    /// `LI`, table cells and spans from `TD`/`TH`, the reading order from the
    /// tree, and `/Artifact` content becomes furniture. A tree that fails the
    /// check -- or a document without one -- falls back to the geometric
    /// answer, so the option can only replace output the tree earns (#636).
    pub use_tags: bool,
    /// Whether spans carry the document's own font names and the
    /// descriptor's metrics. Default false: embedded Type1/CFF spans report
    /// the substitute face and its AFM pair, exactly as without the option.
    ///
    /// When true, [`TextSpan::font_name`](crate::TextSpan::font_name) is the
    /// font dictionary's /BaseFont with any subset prefix stripped -- the
    /// name the page set its text in -- and
    /// [`TextSpan::font_metrics`](crate::TextSpan::font_metrics) is the
    /// FontDescriptor /Ascent /Descent pair (/CapHeight /XHeight), in /1000
    /// em, falling back to the substitute face's AFM values where the
    /// descriptor carries none. Span merging still reads the face either
    /// way (#643).
    pub document_font_names: bool,
    /// Whether the running heads reach the Markdown. Default false: they are
    /// marked [`BlockKind::Header`] and left out of
    /// [`PdfDocument::extract_markdown`](crate::PdfDocument::extract_markdown),
    /// exactly as without the option.
    ///
    /// When true they are written as plain paragraphs at their reading-order
    /// position -- never as headings, which they are not -- for callers that
    /// need running titles in the text (citations, legal documents). Read
    /// only by `extract_markdown`: `extract_blocks` always carries the
    /// furniture, whatever this says.
    pub include_headers: bool,
    /// Whether the running footers and page numbers reach the Markdown.
    /// Default false: they are marked [`BlockKind::Footer`] and left out of
    /// [`PdfDocument::extract_markdown`](crate::PdfDocument::extract_markdown),
    /// exactly as without the option.
    ///
    /// When true they are written as plain paragraphs at their reading-order
    /// position -- never as headings, which they are not -- for callers that
    /// need page numbers in the text. Read only by `extract_markdown`:
    /// `extract_blocks` always carries the furniture, whatever this says.
    pub include_footers: bool,
    /// How a change of page is marked in the Markdown. Default
    /// [`PageSeparators::Comment`]: today's `<!-- page N -->` marker.
    /// [`PageSeparators::None`] writes no marker at all, and the paragraphs
    /// on either side stand separated by a blank line. Read only by
    /// [`PdfDocument::extract_markdown`](crate::PdfDocument::extract_markdown).
    pub page_separators: PageSeparators,
    /// Whether monospace runs reach the Markdown as code. Default true.
    ///
    /// A run of spans set in a monospace face (`TextSpan::is_monospace`) inside
    /// a paragraph or a list item is written `` `like this` ``, and a paragraph
    /// output from before code existed, byte for byte -- the off switch
    /// pymupdf4llm spells `ignore_code` (#636).
    pub code: bool,
    /// Caller-supplied heading steers, applied after every other heading
    /// decision. Default empty: the document's own statistics decide.
    ///
    /// Each override relevels the block whose text matches exactly: `None`
    /// demotes it to prose, `Some(n)` promotes it to a heading. The front
    /// door steers its own headings the same way, with a detector object
    /// the caller hands in (SPEC G24).
    pub heading_overrides: Vec<HeadingOverride>,
    /// Whether text the page hides reaches the blocks. Default false: text
    /// drawn with render mode 3 or painted with zero alpha is dropped, the
    /// way the front door's own default engine drops hidden lines from its
    /// output (SPEC G165).
    ///
    /// Invisible text that is a page's ONLY text is kept either way: without
    /// it the page would read empty, and that is how a scanned page reads
    /// through its own layer. Pass true to keep every hidden line, for
    /// callers that filter themselves.
    pub include_hidden: bool,
    /// Whether blocks follow the content stream instead of the reading order.
    /// Default false: the reading order decides, exactly as without the
    /// option.
    ///
    /// When true, `extract_blocks` hands back the runs the page draws in the
    /// order it draws them, grouped into lines and blocks by content-order
    /// proximity -- and the reading-order sort, the furniture, the headings
    /// and the learned pass never run (#643). The compat layer reads through
    /// this where the recorded shapes need draw order rather than reading
    /// order.
    pub raw_order: bool,
    /// Whether pages that need it are OCR'd before extraction. Default
    /// [`OcrMode::Never`]: the SDK and the rig read what the document draws,
    /// and OCR is an explicit request.
    ///
    /// [`OcrMode::Auto`] recognizes the pages under
    /// `pdf-ocr`'s text threshold and leaves the rest untouched;
    /// [`OcrMode::Force`] recognizes every page in the window, replacing a
    /// born-digital page's own text with the OCR layer rather than emitting
    /// both. Either way the recognized words go through the same
    /// segmentation as born-digital text, because extraction reads a copy of
    /// the document carrying them as an invisible text layer.
    ///
    pub ocr: OcrMode,
    /// The Tesseract language tag OCR runs in (`-l`). Default `"eng"`.
    /// Read only when [`Self::ocr`] is not [`OcrMode::Never`]; several tags
    /// join with `+` as Tesseract takes them.
    pub ocr_language: String,
    /// The resolution OCR renders pages at, in dots per inch. Default 150.
    /// Read only when [`Self::ocr`] is not [`OcrMode::Never`]; must be
    /// positive when it is read.
    pub ocr_dpi: u32,
    /// Clip margins in points, or `None` for the whole page. Default `None`.
    pub clip_margins: Option<ClipMargins>,
    /// Which tier reads the document. Default [`Quality::Fast`]: the shipping
    /// engine, byte for byte what a caller got before this field existed.
    ///
    /// [`Quality::Balanced`] is read by
    /// [`crate::PdfDocument::extract_markdown`] only; a call that returns
    /// blocks answers [`OptionsError::BalancedIsMarkdownOnly`] for it.
    pub quality: Quality,
    /// Where the model tier looks for its weights. Default `None`:
    /// `PDFLUENT_MODEL_DIR`, else the per-user `<data dir>/pdfluent/models/`.
    ///
    /// Read only when [`Self::quality`] is [`Quality::Balanced`]: an explicit
    /// dir for a caller -- and a test -- that keeps models outside the
    /// default. Wins over the environment and the default, exactly as the
    /// shared runtime resolves it.
    pub model_dir: Option<PathBuf>,
    /// Whether the balanced tier keeps running heads and feet as paragraphs.
    /// Default false: furniture lines are dropped, exactly as without the
    /// option.
    ///
    /// Read only when [`Self::quality`] is [`Quality::Balanced`], where it
    /// maps to the balanced assembly's keep-furniture switch.
    pub keep_furniture: bool,
    /// Whether the word boxes travel with the answer. Default false: the
    /// blocks, the markdown and [`crate::to_json`] read exactly as without
    /// the option. Set with [`Self::with_word_boxes`].
    pub word_boxes: bool,
    /// TATR table structure, opted into per call. Default `None`: the
    /// current grid, byte for byte. Set with [`Self::with_tatr`] only --
    /// a private field, so the opt-in stays a builder call and a caller
    /// matching on options never names it.
    tatr: Option<TatrOptions>,
    /// Whether figures reach the Markdown as image references. Default true.
    ///
    /// Read only by
    /// [`PdfDocument::extract_markdown`](crate::PdfDocument::extract_markdown),
    /// the way [`Self::include_headers`] is: `extract_blocks` always carries
    /// the figures, whatever this says. False writes no reference at all.
    pub include_figures: bool,
    /// Whether figure references embed the figure's pixels. Default false:
    /// references are markers naming the figure.
    ///
    /// True writes `![](data:image/png;base64,...)` with the figure's box
    /// rendered at 150 dots per inch. Read only by
    /// [`PdfDocument::extract_markdown`](crate::PdfDocument::extract_markdown);
    /// [`Self::figure_image_dir`] wins where both are set.
    pub embed_figures: bool,
    /// Where figure image files are written, or `None` for no files.
    /// Default `None`.
    ///
    /// Read only by
    /// [`PdfDocument::extract_markdown`](crate::PdfDocument::extract_markdown):
    /// each figure is rendered at 150 dots per inch, written under the
    /// directory, and referenced from the Markdown under that path. The
    /// directory -- and any parents -- is created; a write failure fails the
    /// extraction. The one writer of files on this path, so the references
    /// and the files cannot disagree.
    pub figure_image_dir: Option<PathBuf>,
    /// Whether text drawn over a placed image is dropped. Default false:
    /// every line is kept, wherever it stands.
    ///
    /// True drops a body block whose centre falls inside a placed image's
    /// box on the same page -- the over-picture line of a caption pair --
    /// before figures merge into the blocks. Running heads and feet are
    /// never dropped: page chrome is positioned independently of figures.
    pub drop_text_over_images: bool,
    /// Which evidence may find a table. Default [`TableStrategy::Both`]:
    /// today's combined detection, byte for byte.
    ///
    /// [`TableStrategy::Lines`] keeps only the tables a drawn rule
    /// crosses -- the rest go back to the paragraph merge -- and
    /// [`TableStrategy::Text`] drops the drawn grid from the evidence.
    /// Read by [`crate::PdfDocument::extract_blocks`] and, through it,
    /// [`crate::PdfDocument::extract_markdown`]; the balanced tier and
    /// [`crate::PdfDocument::extract_page_blocks`] read every table.
    pub table_strategy: TableStrategy,
    /// How tables reach the Markdown. Default [`TableRenderMode::Auto`]:
    /// pipes where Markdown holds the table's shape, HTML elsewhere --
    /// today's output, byte for byte.
    ///
    /// [`TableRenderMode::Markdown`] writes every table as pipes and
    /// [`TableRenderMode::Html`] every table as HTML. Read only by
    /// [`PdfDocument::extract_markdown`](crate::PdfDocument::extract_markdown):
    /// [`PdfDocument::extract_blocks`](crate::PdfDocument::extract_blocks)
    /// always carries the cells, whatever this says.
    pub table_render_mode: TableRenderMode,
    /// Whether table-cell words join on the cell path. Default false: the
    /// leader fold, the cell-word join, the TATR fill and the quarter-em
    /// word-space boundary read exactly as without the option.
    ///
    /// When true, the four cell-path word rules run: leader dots abutting
    /// their label join it as text and box, keeping the dots' glyph boxes;
    /// cell words join by the shared gap rule with only a drawn space
    /// vetoing; the TATR fill aligns fragments onto text tokens in both
    /// directions; and a word space set at exactly a quarter em reads as a
    /// space (#659). Read by the `extract_blocks` family; the direct learned
    /// entries keep the shipped words either way. Independent of
    /// [`Self::actual_text`]: this option never applies `/ActualText`.
    pub cell_path_words: bool,
    /// Whether `/ActualText` replacements read instead of their marked runs.
    /// Default false: replacements are ignored, exactly as without the
    /// option.
    ///
    /// When true, an `/ActualText` replacement reads instead of the glyphs of
    /// its marked run, once, on the run's first glyph; a run that closes
    /// without a glyph to carry it reads once at the box it drew instead.
    /// Without the option the run reads what the page drew (#663). Opt-in
    /// pending a variant measurement: the default output is what it was
    /// before the replacements were read. A glyphless replacement in a
    /// widget appearance reads either way: the appearance is the only place
    /// that value is painted (#549). Read by the `extract_blocks` family;
    /// independent of [`Self::cell_path_words`], which moves only the four
    /// word rules.
    pub actual_text: bool,
}

/// Clip margins in points: left, top, right, bottom.
///
/// A validated newtype rather than a bare array, for two reasons. `f64` is
/// not `Eq` -- NaN is not equal to itself -- so a bare `[f64; 4]` on
/// [`ExtractOptions`] cost the options their `Eq`, which is public API a
/// caller may compare or key on. And a margin is a distance into the page:
/// negative or non-finite is a mistake in the call, caught by
/// [`ClipMargins::new`] rather than read as a rectangle.
#[derive(Debug, Clone, Copy)]
pub struct ClipMargins([f64; 4]);

impl ClipMargins {
    /// `[left, top, right, bottom]` in points. Each side must be finite and
    /// at least zero; anything else is an
    /// [`OptionsError::InvalidClipMargins`] naming the side.
    ///
    /// `-0.0` is normalised to `0.0`, so margins that compare equal as floats
    /// compare equal here too.
    pub fn new(
        left: f64,
        top: f64,
        right: f64,
        bottom: f64,
    ) -> std::result::Result<Self, OptionsError> {
        for (side, value) in [
            ("left", left),
            ("top", top),
            ("right", right),
            ("bottom", bottom),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(OptionsError::InvalidClipMargins { side, value });
            }
        }
        Ok(Self([left + 0.0, top + 0.0, right + 0.0, bottom + 0.0]))
    }

    /// The margins as `[left, top, right, bottom]` in points.
    pub fn as_array(&self) -> [f64; 4] {
        self.0
    }
}

impl PartialEq for ClipMargins {
    fn eq(&self, other: &Self) -> bool {
        self.0.map(f64::to_bits) == other.0.map(f64::to_bits)
    }
}

impl Eq for ClipMargins {}

impl Hash for ClipMargins {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.map(f64::to_bits).hash(state);
    }
}

/// Whether extraction recognizes pages that need it before reading them.
///
/// The engine default is [`OcrMode::Never`], so default output never changes
/// under an OCR install: a caller opts in per call. The pymupdf4llm front
/// door maps upstream's `use_ocr` onto [`OcrMode::Auto`] and `force_ocr`
/// onto [`OcrMode::Force`].
///
/// Non-exhaustive: re-exported from the facade, so new modes must not break a
/// downstream match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum OcrMode {
    /// No OCR: read what the document draws. The default.
    #[default]
    Never,
    /// OCR the pages that need it -- the ones under `pdf-ocr`'s text
    /// threshold -- and leave the rest untouched. With no `tesseract`
    Auto,
    /// OCR every page in the window. On a page that already has a text
    /// layer the OCR layer REPLACES the native text for that page; without
    /// a `tesseract` binary this raises naming it, where upstream stays
    /// silent.
    Force,
}

/// Which evidence may find a table: the lines the page drew, the alignment
/// of its words, or both (#659).
///
/// Read by [`ExtractOptions::table_strategy`]. The builder finds its grids
/// from gutters and the learned proposers from words, while the lattice
/// reads the drawn grid and the rules refuse the cuts a table stands
/// across: [`TableStrategy::Lines`] keeps only the tables a drawn rule
/// crosses, [`TableStrategy::Text`] runs without the drawn grid, and the
/// default [`TableStrategy::Both`] is today's combined detection.
///
/// Non-exhaustive: re-exported from the facade, so new strategies must not
/// break a downstream match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum TableStrategy {
    /// Only tables the page ruled: a grid no drawn rule crosses goes back
    /// to the paragraph merge, and the word-reading learned proposers sit
    /// out, so nothing puts it back.
    Lines,
    /// Only tables the words align into: the lattice proposer, which
    /// reads the drawn grid alone, sits out. The builder keeps its rules
    /// -- they only refine grids its gutters already found, and a ruled
    /// table whose words align is still found.
    Text,
    /// Today's combined detection: every proposer runs and every grid the
    /// builder reads stands. The default.
    #[default]
    Both,
}

/// How a table reaches the Markdown: pipes, HTML, or the automatic choice
/// between them (#659).
///
/// Read by [`ExtractOptions::table_render_mode`]. Pipes hold a rectangular
/// table with at most one header row; anything else -- a spanned grid, a
/// headless one -- needs HTML to say what it is. The default
/// [`TableRenderMode::Auto`] writes each table the way it always was, and
/// the other two ask for one form for every table.
///
/// Non-exhaustive: re-exported from the facade, so new modes must not break
/// a downstream match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum TableRenderMode {
    /// Each table the way it always was: pipes where Markdown holds its
    /// shape, HTML elsewhere. The default.
    #[default]
    Auto,
    /// Every table as a pipe table, its first row above the divider.
    Markdown,
    /// Every table as an HTML table, header cells marked `th`.
    Html,
}

impl Default for ExtractOptions {
    fn default() -> Self {
        Self {
            first_page: 0,
            page_count: None,
            pages: None,
            // A caller gets the learned pass; only a test of a heuristic turns
            // it off, and says why where it does.
            learned: true,
            // Tags never decide unless asked: the geometric answer is the
            // default, and the tree is evidence for an option, not a second
            // default (#636).
            use_tags: false,
            // And the document's font names stay opt-in: the default
            // output is what it was before the option existed (#643).
            document_font_names: false,
            // The markdown a caller of the default gets is what the rig
            // measures, so the markdown switches default to today's output:
            // no furniture, and every page change a comment.
            include_headers: false,
            include_footers: false,
            page_separators: PageSeparators::Comment,
            // Monospace reads as code unless the gate below says it costs the
            // rig more than 0.002 on any axis, in which case this is false and
            // the option is opt-in (#636).
            code: true,
            // No caller steers headings, and hidden text stays out: the
            // document's own statistics decide, and what the page hides is
            // not content (lane L1).
            heading_overrides: Vec::new(),
            include_hidden: false,
            // And the reading order decides unless asked: the raw path is a
            // compat instrument, never a second default (#643).
            raw_order: false,
            // And OCR never runs unless asked: the default output is what
            // the document draws, with or without a tesseract installed.
            ocr: OcrMode::Never,
            ocr_language: "eng".to_string(),
            ocr_dpi: 150,
            clip_margins: None,
            // The shipping engine, the default model root, furniture dropped:
            // a caller that never heard of the balanced tier reads exactly
            // what it read before these fields existed.
            quality: Quality::Fast,
            model_dir: None,
            keep_furniture: false,
            // Word boxes travel only when asked: the default answer is what
            // it was before they existed.
            word_boxes: false,
            // And TATR stays off unless asked: the default output is the
            // current grid, with or without the model installed.
            tatr: None,
            // Figures are listed and marked unless asked otherwise; embeds,
            // files and the over-image drop are all opt-in.
            include_figures: true,
            embed_figures: false,
            figure_image_dir: None,
            drop_text_over_images: false,
            // And tables are found the way they always were: every
            // proposer runs, whatever the evidence.
            table_strategy: TableStrategy::Both,
            // And rendered the way they always were: each table in the
            // form that holds its shape.
            table_render_mode: TableRenderMode::Auto,
            // And cell words join the way they always did: the cell path
            // is opt-in, never a second default (#659).
            cell_path_words: false,
            // And marked runs read what the page drew: the replacement is
            // opt-in, never a second default (#663).
            actual_text: false,
        }
    }
}

impl ExtractOptions {
    /// The whole document, which is what [`Default`] gives too.
    pub fn all() -> Self {
        Self::default()
    }

    /// The same window with the learned pass off, for a test that asserts what
    /// a heuristic does (#599).
    pub fn without_the_learned_pass(self) -> Self {
        Self {
            learned: false,
            ..self
        }
    }

    /// Rebuild table regions with TATR first, where it fires.
    ///
    /// The only way to set the TATR opt-in: there is no public field. With
    /// it set, every table region is offered to the Table Transformer
    /// before the current grid runs; where TATR fires its cells are the
    /// region's table, and where it declines, or a region's inference
    /// fails, the current grid runs unchanged. A missing model is an
    /// error, never a silent fast run -- as is asking on a build without
    /// the `ort-tables` feature.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use pdf_engine::{ExtractOptions, TatrOptions};
    ///
    /// let options = ExtractOptions::default().with_tatr(TatrOptions::default());
    /// # let _ = options;
    /// ```
    pub fn with_tatr(self, tatr: TatrOptions) -> Self {
        Self {
            tatr: Some(tatr),
            ..self
        }
    }

    /// The TATR opt-in, if set.
    pub(crate) fn tatr(&self) -> Option<&TatrOptions> {
        self.tatr.as_ref()
    }

    /// `count` pages from `first`, counted from zero.
    ///
    /// A window past the last page is EMPTY rather than an error, and so is a
    /// `count` of zero: a caller stepping through a long document a window at a
    /// time stops when the answer is empty, and an error at the end would make
    /// the ordinary end of that loop indistinguishable from a broken file.
    pub fn window(first: usize, count: usize) -> Self {
        Self {
            first_page: first,
            page_count: Some(count),
            ..Self::default()
        }
    }

    /// Which tier reads the document: [`Quality::Fast`] (the default) or
    /// [`Quality::Balanced`], read by
    /// [`crate::PdfDocument::extract_markdown`] only.
    #[must_use]
    pub fn with_quality(mut self, quality: Quality) -> Self {
        self.quality = quality;
        self
    }

    /// Where the model tier looks for its weights, winning over
    /// `PDFLUENT_MODEL_DIR` and the per-user default. Read only when
    /// [`Self::quality`] is [`Quality::Balanced`].
    #[must_use]
    pub fn with_model_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.model_dir = Some(dir.into());
        self
    }

    /// Keep running heads and feet as paragraphs in balanced markdown
    /// (`true`) or drop them (the default). Read only when [`Self::quality`]
    /// is [`Quality::Balanced`], where it maps to the balanced assembly's
    /// keep-furniture switch.
    #[must_use]
    pub fn with_keep_furniture(mut self, keep: bool) -> Self {
        self.keep_furniture = keep;
        self
    }

    /// Whether `page` falls inside the window, or is named by [`Self::pages`].
    pub(crate) fn holds(&self, page: usize) -> bool {
        if let Some(pages) = &self.pages {
            return pages.contains(&page);
        }
        if page < self.first_page {
            return false;
        }
        match self.page_count {
            None => true,
            Some(count) => page - self.first_page < count,
        }
    }

    /// Whether the window is the whole document, which is the case that needs
    /// no filtering at all.
    pub(crate) fn is_whole_document(&self) -> bool {
        self.first_page == 0 && self.page_count.is_none() && self.pages.is_none()
    }

    ///
    /// The window needs no validation -- past the end is empty -- but a
    /// list names its pages: one past the end is
    /// [`EngineError::PageOutOfRange`], and a list beside a window is an
    /// [`OptionsError::ConflictingPageSelection`] carried in an
    pub(crate) fn validate(&self, page_count: usize) -> Result<()> {
        // The balanced tier assembles markdown, so it is read by
        // `extract_markdown` before it ever reaches here; a call that returns
        // blocks has no balanced answer and says so, rather than handing back
        // the fast tier under a balanced name.
        if matches!(self.quality, Quality::Balanced) {
            return Err(EngineError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                OptionsError::BalancedIsMarkdownOnly,
            )));
        }
        // Asked for TATR on a build without it: a refusal, never a silent
        // fast run. The test seam answers without a model, so it is not
        // asked for anything this build lacks.
        #[cfg(not(feature = "ort-tables"))]
        if self.tatr.as_ref().is_some_and(|t| t.test_hook.is_none()) {
            return Err(EngineError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                OptionsError::TatrNeedsOrtTables,
            )));
        }
        let Some(pages) = &self.pages else {
            return Ok(());
        };
        if self.first_page != 0 || self.page_count.is_some() {
            return Err(EngineError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                OptionsError::ConflictingPageSelection,
            )));
        }
        if let Some(&index) = pages.iter().find(|page| **page >= page_count) {
            return Err(EngineError::PageOutOfRange {
                index,
                count: page_count,
            });
        }
        Ok(())
    }
}

/// The blocks of `options`' window, in the order they were read in (#573).
///
/// The document was classified whole, so what survives here is what the WHOLE
/// document said each block was; the window only decides which of them the
/// caller asked for.
///
/// [`Block::index`] keeps the document's own numbering rather than being
/// counted again from zero. It is a place in the document's reading order --
/// two windows of one document have to agree about the same block -- and a
/// caller that wants the position within the window it asked for has the
/// position within the vector it is holding.
///
/// [`Block::continues_previous`] is cleared wherever the block it continues
/// fell outside the window: the flag says "join me onto the one above", and
/// there is no longer one above to join onto. Left standing, the markdown
/// serializer would glue the opening half-sentence of the window onto whatever
/// happened to precede it.
/// Relevel the blocks the caller steers (see [`HeadingOverride`]).
fn apply_heading_overrides(blocks: &mut [Block], overrides: &[HeadingOverride]) {
    if overrides.is_empty() {
        return;
    }
    for block in blocks.iter_mut() {
        if let Some(hit) = overrides.iter().find(|o| o.text == block.text) {
            match hit.level {
                None => block.kind = BlockKind::Paragraph,
                Some(level) => {
                    block.kind = BlockKind::Heading {
                        level: level.clamp(1, 3),
                    }
                }
            }
        }
    }
}

/// Rebuild a block's text and box from its spans after filtering.
fn rebuild_block_text_and_box(block: &mut Block) {
    // The shared join rule: two spans with no whitespace between them read
    // as one word where `spans_join` says so, and are spaced otherwise --
    // the same boundary the word assembly draws, so a block's words joined
    // with single spaces are always its own text (#663). The emphasis marks
    // find their runs across the same boundary (#668).
    let mut text = String::new();
    for (index, span) in block.spans.iter().enumerate() {
        if index > 0 && !crate::words_join::spans_join(&block.spans[index - 1], span) {
            text.push(' ');
        }
        text.push_str(&span.text);
    }
    block.text = text;
    let mut bbox = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for span in &block.spans {
        bbox[0] = bbox[0].min(span.x);
        bbox[1] = bbox[1].min(span.y);
        bbox[2] = bbox[2].max(span.x + span.width);
        bbox[3] = bbox[3].max(span.y + span.height);
    }
    block.bbox = bbox;
}

/// Drop text the page hides, page by page (see [`ExtractOptions::include_hidden`]).
///
/// Pages that draw visible text lose their hidden spans; a page whose text
/// is all hidden keeps it, so a scan still reads through its own layer.
/// Spans the engine's own OCR layer wrote stay either way: the layer is the
/// engine's own voice, not ink the author hid.
/// Blocks emptied by the drop go, and a block that loses spans gets its text
/// and box rebuilt from the spans that stay. A block that loses nothing is
/// left byte for byte as it arrived: the rebuild joins spans by its own
/// rule, which is not the rule the text the block carries was joined by, so
/// rebuilding it would respell visible text the drop was never asked about.
/// Blocks without spans are kept: with nothing to judge, there is nothing
/// to prove hidden.
fn drop_hidden_text(blocks: &mut Vec<Block>) {
    use std::collections::HashSet;
    // Only spans vote: a block with none (a figure marker) carries no text
    // either way, and counting it as visible wiped a scanned page clean --
    // its whole text layer hides in render mode 3 over its own image, and
    // the figure beside it marked the page visible.
    let visible_pages: HashSet<usize> = blocks
        .iter()
        .filter(|b| b.spans.iter().any(|s| !s.is_hidden()))
        .map(|b| b.page)
        .collect();
    let mut kept = Vec::with_capacity(blocks.len());
    for mut block in blocks.drain(..) {
        if visible_pages.contains(&block.page) && !block.spans.is_empty() {
            let span_count = block.spans.len();
            block.spans.retain(|s| !s.is_hidden() || s.is_ocr_layer);
            if block.spans.is_empty() {
                continue;
            }
            if block.spans.len() != span_count {
                rebuild_block_text_and_box(&mut block);
            }
        }
        kept.push(block);
    }
    *blocks = kept;
}

/// Drop text the page hides from raw blocks, page by page (#643).
///
/// The same page rule as [`drop_hidden_text`]: pages that draw visible text
/// lose their hidden spans, and a page whose text is all hidden keeps it.
/// The survivors keep RAW text -- the runs concatenated as drawn -- where the
/// default path joins its spans with spaces. Like that path, a block that
/// loses no spans is left as it arrived.
fn drop_hidden_text_raw(blocks: &mut Vec<Block>) {
    use std::collections::HashSet;
    // Only spans vote, as above: a spanless block must not mark its page
    // visible, or a scanned page reads empty on the raw path too.
    let visible_pages: HashSet<usize> = blocks
        .iter()
        .filter(|b| b.spans.iter().any(|s| !s.is_hidden()))
        .map(|b| b.page)
        .collect();
    let mut kept = Vec::with_capacity(blocks.len());
    for mut block in blocks.drain(..) {
        if visible_pages.contains(&block.page) && !block.spans.is_empty() {
            let span_count = block.spans.len();
            block.spans.retain(|s| !s.is_hidden() || s.is_ocr_layer);
            if block.spans.is_empty() {
                continue;
            }
            if block.spans.len() != span_count {
                let (text, bbox) = crate::text::raw_text_and_box(&block.spans);
                block.text = text;
                block.bbox = bbox;
            }
        }
        kept.push(block);
    }
    *blocks = kept;
}

pub(crate) fn window(blocks: Vec<Block>, options: &ExtractOptions) -> Vec<Block> {
    // Lane L1: the caller steers and the hidden text drops here, before the
    // window cuts pages. This is the only post-decision point in this module
    // that sees the options; both passes read the whole document, which is
    // what the page rule for hidden text needs.
    let mut blocks = blocks;
    apply_heading_overrides(&mut blocks, &options.heading_overrides);
    if !options.include_hidden {
        if options.raw_order {
            drop_hidden_text_raw(&mut blocks);
        } else {
            drop_hidden_text(&mut blocks);
        }
    }
    if options.is_whole_document() {
        return blocks;
    }
    let mut out: Vec<Block> = Vec::new();
    let mut previous_kept = false;
    for mut block in blocks {
        if !options.holds(block.page) {
            previous_kept = false;
            continue;
        }
        if !previous_kept {
            block.continues_previous = false;
        }
        previous_kept = true;
        out.push(block);
    }
    out
}

/// Takes a table's caption, and the note or sentence under it, back out of the
/// table (#574).
///
/// A grid built from a region's rows takes whatever the region held: the
/// `Table 3 Corticobulbar projections…` over it, the `AA, arachidonic acid;
/// PFA-100, …` under it, a line of the running text the region reached into.
/// Each is ONE filled cell spanning a row of a table of several columns, and
/// each says so in its words: a caption opens with `Table n`, a note with a
/// mark or `Note`, and a sentence runs to a full stop or opens in lower case.
/// A spanning header -- `Male Female`, `Diplotype No. …` -- is short and says
/// none of it, and stays. On 700 PubTables train pages the top row rule was
/// right 24 times and wrong once, the bottom row 16 and once.
///
/// Only the first and the last row, once each, and only in a table of three
/// rows or more: what is left must still be a table.
///
/// `runs_on_figures` is the `table_runs_on_figures` switch: a complete-figure
/// line never runs on from another, so stacked figure rows stay split here
/// as where the learned pass split them.
pub(crate) fn peel_table_edges(blocks: &mut Vec<Block>, runs_on_figures: bool) {
    let mut i = 0;
    let mut moved = false;
    while i < blocks.len() {
        let Some(end) = crate::markdown::table_run(blocks, i) else {
            i += 1;
            continue;
        };
        // The half of a table a page broke, as `tables_across_pages` will
        // read it: first on its page with a cell last on the page before, or
        // last on its page with a cell first on the page after, furniture
        // aside.
        let body = |b: &Block| !matches!(b.kind, BlockKind::Header | BlockKind::Footer);
        let cell = |b: &Block| matches!(b.kind, BlockKind::TableCell { .. });
        let page = blocks[i].page;
        let first_on_page = !blocks[..i]
            .iter()
            .rev()
            .take_while(|b| b.page == page)
            .any(body);
        let last_on_page = !blocks[end..]
            .iter()
            .take_while(|b| b.page == page)
            .any(body);
        // AND WITH THE SAME COLUMNS as the table it would join, which is what
        // `tables_across_pages` asks too: a one-row grid under a page's last
        // table with columns of its own is no half of it (codex, #2058).
        let table_ending_at = |last: usize| -> &[Block] {
            let mut start = last;
            while start > 0
                && cell(&blocks[start - 1])
                && blocks[start - 1].page == blocks[last].page
            {
                start -= 1;
            }
            &blocks[start..=last]
        };
        let table_starting_at = |first: usize| -> &[Block] {
            let mut stop = first;
            while stop < blocks.len()
                && cell(&blocks[stop])
                && blocks[stop].page == blocks[first].page
            {
                stop += 1;
            }
            &blocks[first..stop]
        };
        let continues_before = blocks[..i]
            .iter()
            .rposition(body)
            .filter(|&k| blocks[k].page + 1 == page && cell(&blocks[k]))
            .is_some_and(|k| same_columns(table_ending_at(k), &blocks[i..end]));
        // The table after it as it will stand, not as it stands now: a table
        // read as one column gets its columns back only when this loop
        // reaches it, and the columns compared are those (codex, #2065).
        let continues_after = (end..blocks.len())
            .find(|&k| body(&blocks[k]))
            .filter(|&k| blocks[k].page == page + 1 && cell(&blocks[k]))
            .is_some_and(|k| {
                let (_, regridded, _) = regrid_one_column(table_starting_at(k).to_vec());
                same_columns(&blocks[i..end], &regridded)
            });
        let at_page_edge = (first_on_page && continues_before) || (last_on_page && continues_after);
        let run: Vec<Block> = blocks.drain(i..end).collect();
        let before = run.len();
        let (head, run) = caption_row_or_single_row(run, at_page_edge);
        let (over, run, under) = regrid_one_column(run);
        let over: Vec<Block> = head.into_iter().chain(over).collect();
        let (mut table, after) = split_prose_column(run);
        moved |= !over.is_empty()
            || !under.is_empty()
            || !after.is_empty()
            || !table
                .iter()
                .any(|b| matches!(b.kind, BlockKind::TableCell { .. }));
        peel_run(&mut table);
        // A label set on two lines is one cell, in the builder's grid as in
        // the decoder's (#574).
        crate::learned::cell_merge::join_run_on_rows(&mut table, runs_on_figures);
        let mut placed: Vec<Block> = over;
        placed.extend(table);
        placed.extend(after);
        placed.extend(under);
        let n = placed.len();
        // A table cut into more cells, or fewer, moves every block after it
        // (codex, #2051): the indices are the reading order.
        moved |= n != before;
        blocks.splice(i..i, placed);
        i += n.max(1);
    }
    if moved {
        for (n, block) in blocks.iter_mut().enumerate() {
            block.index = n;
        }
    }
}

/// A caption spread over a table's first row, and a table of one row (#574).
///
/// Read with the weight its font names give it, a caption set in bold over a
/// table is a row of the grid like any other, and the grid cuts it at the
/// table's own column gutters: `Table 1: Comparison | of the | logP values
/// calculated | by several`. Joined in column order it opens as a numbered
/// caption, and comes out whole. A running head with its page number --
/// `New records of the endemic Sicilian land snail species … | 137` -- is a
/// table of one row, which no table is.
///
/// Answers the paragraph to set over the table, and the table.
fn caption_row_or_single_row(run: Vec<Block>, at_page_edge: bool) -> (Option<Block>, Vec<Block>) {
    let place = |b: &Block| match b.kind {
        BlockKind::TableCell { row, column, .. } => Some((row, column)),
        _ => None,
    };
    let rows: std::collections::BTreeSet<u16> =
        run.iter().filter_map(|b| place(b).map(|p| p.0)).collect();
    let Some(&top) = rows.iter().next() else {
        return (None, run);
    };
    let join = |cells: Vec<Block>| -> Option<Block> {
        let mut cells: Vec<Block> = cells
            .into_iter()
            .filter(|b| !b.text.trim().is_empty())
            .collect();
        cells.sort_by_key(|b| place(b).map(|p| p.1));
        let mut cells = cells.into_iter();
        let mut out = cells.next()?;
        out.kind = BlockKind::Paragraph;
        out.grid = None;
        out.text = out.text.trim().to_string();
        for cell in cells {
            out.text.push(' ');
            out.text.push_str(cell.text.trim());
            out.bbox = [
                out.bbox[0].min(cell.bbox[0]),
                out.bbox[1].min(cell.bbox[1]),
                out.bbox[2].max(cell.bbox[2]),
                out.bbox[3].max(cell.bbox[3]),
            ];
            out.spans.extend(cell.spans);
        }
        Some(out)
    };
    // A contents line run out to its page numbers on dot leaders is a table
    // the annotation keeps, one row or not (DocLayNet validation).
    let leaders = run.iter().any(|b| {
        b.text
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
            .contains("....")
    });
    // NOR AT A PAGE'S EDGE: a table of many rows can leave one of them on a
    // page, and that row is the table's, not a line (codex, #2055).
    if rows.len() == 1 && !leaders && !at_page_edge {
        return (join(run), Vec::new());
    }
    let mut on_top: Vec<&Block> = run
        .iter()
        .filter(|b| place(b).is_some_and(|p| p.0 == top) && !b.text.trim().is_empty())
        .collect();
    if on_top.len() < 2 {
        return (None, run);
    }
    on_top.sort_by_key(|b| place(b).map(|p| p.1));
    // The caption opens the row, and the row is the caption's only where
    // every other cell carries its sentence on (`of the`, `logP values …`).
    // Beside column heads -- `Statistics`, `PR P1` -- only the first cell is
    // the caption, and the heads stay (PubTables train).
    if !is_numbered_caption(on_top[0].text.trim()) {
        return (None, run);
    }
    let carried = on_top[1..]
        .iter()
        .all(|b| b.text.trim().chars().next().is_some_and(char::is_lowercase));
    let caption_at: Vec<(u16, u16)> = if carried {
        on_top.iter().filter_map(|b| place(b)).collect()
    } else {
        place(on_top[0]).into_iter().collect()
    };
    let (caption, mut rest): (Vec<Block>, Vec<Block>) = run
        .into_iter()
        .partition(|b| place(b).is_some_and(|p| caption_at.contains(&p)));
    let row_left = rest.iter().any(|b| place(b).is_some_and(|p| p.0 == top));
    if !row_left {
        // The table now starts a row lower; its rows are counted from there.
        for b in rest.iter_mut() {
            if let BlockKind::TableCell { row, .. } = &mut b.kind {
                *row = row.saturating_sub(top + 1);
            }
        }
    }
    (join(caption), rest)
}

/// A table read as one column whose rows are two or more runs of ink at the
/// same places, cut back into its columns (#574).
///
/// The grid finds a table's columns from the white that runs down between
/// them, and one line that crosses it -- the note under a table,
/// `Note that the AUC score for Biosignature-15 …` -- closes that white for
/// the whole region. What is left is a table of one column whose every row
/// still says `CSF t-tau` at one x and `0.616` at another. The runs are read
/// again: each row's spans split where more white than `REGRID_GAP_EM` stands
/// between them, and the left edges the rows agree on are the columns. A row
/// at the table's top or bottom that runs across a column edge is the line
/// that closed the white, and comes out as a paragraph.
///
/// Answers the paragraphs to set over the table, the table, and those under.
fn regrid_one_column(run: Vec<Block>) -> (Vec<Block>, Vec<Block>, Vec<Block>) {
    let one_column = run.iter().all(|b| {
        matches!(
            b.kind,
            BlockKind::TableCell {
                column: 0,
                column_span: 1,
                ..
            }
        )
    });
    if !one_column || run.len() < REGRID_MIN_ROWS {
        return (Vec::new(), run, Vec::new());
    }
    // Each cell's runs of ink: its spans, left to right, split at wide white.
    let groups: Vec<Vec<Vec<usize>>> = run
        .iter()
        .map(|cell| {
            let mut order: Vec<usize> = (0..cell.spans.len())
                .filter(|&k| !cell.spans[k].text.trim().is_empty())
                .collect();
            order.sort_by(|&a, &b| cell.spans[a].x.total_cmp(&cell.spans[b].x));
            let mut out: Vec<Vec<usize>> = Vec::new();
            let mut right = f64::NEG_INFINITY;
            for k in order {
                let span = &cell.spans[k];
                let gap = REGRID_GAP_EM * span.font_size.max(1.0);
                match out.last_mut() {
                    Some(group) if span.x - right < gap => group.push(k),
                    _ => out.push(vec![k]),
                }
                right = right.max(span.x + span.width.max(0.0));
            }
            out
        })
        .collect();
    // The left edges of the runs, over the rows that have two or more.
    let multi = groups.iter().filter(|g| g.len() >= 2).count();
    if multi < REGRID_MIN_ROWS || (multi as f64) < REGRID_SHARE * run.len() as f64 {
        return (Vec::new(), run, Vec::new());
    }
    let mut lefts: Vec<f64> = groups
        .iter()
        .zip(&run)
        .filter(|(g, _)| g.len() >= 2)
        .flat_map(|(g, cell)| {
            g.iter()
                .map(|group| cell.spans[group[0]].x)
                .collect::<Vec<_>>()
        })
        .collect();
    lefts.sort_by(f64::total_cmp);
    let mut edges: Vec<(f64, usize)> = Vec::new(); // (left, rows)
    for x in lefts {
        match edges.last_mut() {
            Some((left, n)) if x - *left <= REGRID_EDGE_TOLERANCE => *n += 1,
            _ => edges.push((x, 1)),
        }
    }
    let needed = ((REGRID_SHARE * multi as f64).ceil() as usize).max(REGRID_MIN_ROWS - 1);
    let edges: Vec<f64> = edges
        .into_iter()
        .filter(|(_, n)| *n >= needed)
        .map(|(x, _)| x)
        .collect();
    if edges.len() < 2 {
        return (Vec::new(), run, Vec::new());
    }
    let column_at = |x: f64| -> usize {
        edges
            .iter()
            .rposition(|&e| x >= e - REGRID_EDGE_TOLERANCE)
            .unwrap_or(0)
    };
    // A single run across a column edge: the line that closed the white.
    let crosses = |cell: &Block, g: &[Vec<usize>]| -> bool {
        if g.len() != 1 {
            return false;
        }
        let left = g[0]
            .iter()
            .map(|&k| cell.spans[k].x)
            .fold(f64::INFINITY, f64::min);
        let right = g[0]
            .iter()
            .map(|&k| cell.spans[k].x + cell.spans[k].width.max(0.0))
            .fold(f64::NEG_INFINITY, f64::max);
        edges[1..]
            .iter()
            .any(|&e| left < e - REGRID_EDGE_TOLERANCE && right > e)
    };
    let rows = run.len();
    let mut top = 0;
    while top < rows && crosses(&run[top], &groups[top]) {
        top += 1;
    }
    let mut bottom = rows;
    while bottom > top && crosses(&run[bottom - 1], &groups[bottom - 1]) {
        bottom -= 1;
    }
    let first_row = run
        .get(top)
        .and_then(|b| match b.kind {
            BlockKind::TableCell { row, .. } => Some(row),
            _ => None,
        })
        .unwrap_or(0);
    let mut over = Vec::new();
    let mut table = Vec::new();
    let mut under = Vec::new();
    for (k, (cell, g)) in run.into_iter().zip(groups).enumerate() {
        if k < top {
            over.push(cell);
            continue;
        }
        if k >= bottom {
            under.push(cell);
            continue;
        }
        let row = match cell.kind {
            BlockKind::TableCell { row, .. } => row.saturating_sub(first_row),
            _ => 0,
        };
        for group in g {
            let spans: Vec<TextSpan> = group.iter().map(|&i| cell.spans[i].clone()).collect();
            let column = column_at(spans[0].x) as u16;
            // A space only where the page left white: `p-` and `tau` drawn
            // in two faces are one word (codex, #2051).
            let mut text = String::new();
            let mut right = f64::NEG_INFINITY;
            for s in &spans {
                if !text.is_empty() && s.x - right > REGRID_WORD_GAP_EM * s.font_size.max(1.0) {
                    text.push(' ');
                }
                text.push_str(&s.text);
                right = right.max(s.x + s.width.max(0.0));
            }
            let text = text.trim().to_string();
            let left = spans.iter().map(|s| s.x).fold(f64::INFINITY, f64::min);
            let right = spans
                .iter()
                .map(|s| s.x + s.width.max(0.0))
                .fold(f64::NEG_INFINITY, f64::max);
            let mut piece = cell.clone();
            piece.kind = BlockKind::TableCell {
                row,
                column,
                row_span: 1,
                column_span: 1,
                header: row == 0,
            };
            piece.bbox = [left, cell.bbox[1], right, cell.bbox[3]];
            piece.text = text;
            piece.spans = spans;
            table.push(piece);
        }
    }
    // Row-major, as a table is read, and ONE CELL A PLACE: two runs of a row
    // that fall to the same column are one cell's words, and two blocks at
    // one place would read as a second table starting.
    table.sort_by_key(|b| match b.kind {
        BlockKind::TableCell { row, column, .. } => (row, column),
        _ => (u16::MAX, u16::MAX),
    });
    let place = |b: &Block| match b.kind {
        BlockKind::TableCell { row, column, .. } => Some((row, column)),
        _ => None,
    };
    let mut cells: Vec<Block> = Vec::new();
    for piece in table {
        match cells.last_mut() {
            Some(last) if place(last) == place(&piece) => {
                last.text.push(' ');
                last.text.push_str(&piece.text);
                last.bbox[0] = last.bbox[0].min(piece.bbox[0]);
                last.bbox[2] = last.bbox[2].max(piece.bbox[2]);
                last.spans.extend(piece.spans);
            }
            _ => cells.push(piece),
        }
    }
    (join_as_paragraphs(over), cells, join_as_paragraphs(under))
}

/// The white, in ems, between two spans of one cell that is a word space.
const REGRID_WORD_GAP_EM: f64 = 0.15;

/// The fewest rows a one-column table needs before its runs are read.
const REGRID_MIN_ROWS: usize = 4;
/// The white, in ems, that parts two runs of a row.
const REGRID_GAP_EM: f64 = 2.0;
/// The share of rows that must agree before a left edge is a column.
const REGRID_SHARE: f64 = 0.5;
/// How far apart two runs' left edges may stand, in points, and be one edge.
const REGRID_EDGE_TOLERANCE: f64 = 6.0;

/// A table's first or last column that is a column of the page's running
/// text, taken back out as the paragraph it is (#574).
///
/// On a page set in two columns a table in one reaches into the other, and
/// every line of the neighbouring text becomes a cell of an extra column:
/// `group) indicated that 69% of the total genetic var…`, one line a row. What
/// gives it away is that it FLOWS: a column of descriptions is cells that each
/// start afresh, and a column of running text is lines that carry on from the
/// one above, opening in lower case or closing on a hyphen. On 700 PubTables
/// train pages, 32 edge columns of long lines flowed at 0.4 or more; 31 of
/// them were not in the table.
///
/// Answers the table and the paragraphs to set after it; a table that is
/// left with one column comes back as paragraphs, whole.
fn split_prose_column(run: Vec<Block>) -> (Vec<Block>, Vec<Block>) {
    let column_of = |b: &Block| match b.kind {
        BlockKind::TableCell { column, .. } => Some(column),
        _ => None,
    };
    let row_of = |b: &Block| match b.kind {
        BlockKind::TableCell { row, .. } => Some(row),
        _ => None,
    };
    let columns: Vec<u16> = run.iter().filter_map(column_of).collect();
    let (Some(&first), Some(&last)) = (columns.iter().min(), columns.iter().max()) else {
        return (run, Vec::new());
    };
    // THE LAST COLUMN ONLY. A first column that flows is as often a table's
    // own row labels wrapping (`Temporary differences relating to investm-
    // ents … / w hich deferred tax …`) as the text column to its left, and on
    // the DocLayNet validation proxy taking it cost a statement its labels.
    let col = last;
    if first == last {
        return (run, Vec::new());
    }
    let texts: Vec<&str> = run
        .iter()
        .filter(|b| column_of(b) == Some(col))
        .map(|b| b.text.trim())
        .filter(|t| !t.is_empty())
        .collect();
    if texts.len() < PROSE_COLUMN_MIN_CELLS {
        return (run, Vec::new());
    }
    let lines = texts.iter().filter(|t| is_a_line_of_prose(t)).count();
    // A line carried on from the one above counts only BESIDE A ROW OF THE
    // TABLE: a description that wraps inside its own cell carries on on a row
    // the other columns leave empty (Sasol's resolutions).
    let others_on = |row: u16| {
        run.iter().any(|b| {
            column_of(b).is_some_and(|c| c != col)
                && row_of(b) == Some(row)
                && !b.text.trim().is_empty()
        })
    };
    let carried = run
        .iter()
        .filter(|b| column_of(b) == Some(col))
        .filter(|b| {
            let t = b.text.trim();
            (t.chars().next().is_some_and(char::is_lowercase) || t.ends_with('-'))
                && row_of(b).is_some_and(others_on)
        })
        .count();
    if (lines as f64) < PROSE_COLUMN_SHARE * texts.len() as f64
        || (carried as f64) < PROSE_COLUMN_FLOW * texts.len() as f64
    {
        return (run, Vec::new());
    }
    let (prose, table): (Vec<Block>, Vec<Block>) =
        run.into_iter().partition(|b| column_of(b) == Some(col));
    let paragraphs = join_as_paragraphs(prose);
    // WHAT IS LEFT OF ONE COLUMN IS NO TABLE: a page of two columns of prose
    // that the grid took whole comes apart into the two, the first read
    // first (an annual report's accounting policies, DocLayNet validation).
    let left: std::collections::BTreeSet<u16> = table.iter().filter_map(column_of).collect();
    if left.len() < 2 {
        let mut all = join_as_paragraphs(table);
        all.extend(paragraphs);
        return (all, Vec::new());
    }
    (table, paragraphs)
}

/// The fewest filled cells a column needs before its flow is read.
const PROSE_COLUMN_MIN_CELLS: usize = 6;
/// The share of a column's cells that must be lines of prose.
const PROSE_COLUMN_SHARE: f64 = 0.7;
/// The share of its cells that must carry on from the line above.
const PROSE_COLUMN_FLOW: f64 = 0.4;

/// A line of running text: 30 characters or more, five words of three letters.
fn is_a_line_of_prose(text: &str) -> bool {
    text.chars().count() >= 30
        && text
            .split_whitespace()
            .filter(|w| w.chars().filter(|c| c.is_alphabetic()).count() >= 3)
            .count()
            >= 5
}

/// Lines joined back into the paragraphs they were: a line's closing hyphen
/// joined to the word it broke, and a paragraph ended where a line ends a
/// sentence and more white than the column's leading stands under it. One
/// paragraph for the whole column would be one block where the page has four,
/// and each of the four is its own item for a reader and for a chunker.
fn join_as_paragraphs(lines: Vec<Block>) -> Vec<Block> {
    let lines: Vec<Block> = lines
        .into_iter()
        .filter(|b| !b.text.trim().is_empty())
        .collect();
    // The white between each line and the next, top of the page first.
    let gaps: Vec<f64> = lines
        .windows(2)
        .map(|pair| pair[0].bbox[1] - pair[1].bbox[3])
        .collect();
    let mut sorted = gaps.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let leading = sorted.get(sorted.len() / 2).copied().unwrap_or(0.0);
    let mut out: Vec<Block> = Vec::new();
    let mut open = false;
    for (k, line) in lines.into_iter().enumerate() {
        let next = line.text.trim().to_string();
        let white_under = gaps.get(k).copied().unwrap_or(f64::INFINITY);
        let ends = next.ends_with(['.', '!', '?', ':'])
            && white_under > PARAGRAPH_GAP_FACTOR * leading.max(0.5);
        match out.last_mut() {
            Some(current) if open => {
                if current.text.ends_with('-')
                    && next.chars().next().is_some_and(char::is_lowercase)
                {
                    current.text.pop();
                } else {
                    current.text.push(' ');
                }
                current.text.push_str(&next);
                current.bbox = [
                    current.bbox[0].min(line.bbox[0]),
                    current.bbox[1].min(line.bbox[1]),
                    current.bbox[2].max(line.bbox[2]),
                    current.bbox[3].max(line.bbox[3]),
                ];
                current.spans.extend(line.spans);
            }
            _ => {
                let mut block = line;
                block.kind = BlockKind::Paragraph;
                block.grid = None;
                block.continues_previous = false;
                block.text = next;
                out.push(block);
            }
        }
        open = !ends;
    }
    out
}

/// How many times the column's own leading the white under a line must be for
/// the line to end its paragraph.
const PARAGRAPH_GAP_FACTOR: f64 = 1.5;

fn peel_run(cells: &mut [Block]) {
    let at = |b: &Block| match b.kind {
        BlockKind::TableCell {
            row,
            column,
            column_span,
            ..
        } => Some((row, column, column_span)),
        _ => None,
    };
    let rows: Vec<u16> = cells
        .iter()
        .filter_map(|b| at(b).map(|(r, _, _)| r))
        .collect();
    let (Some(&top), Some(&bottom)) = (rows.iter().min(), rows.iter().max()) else {
        return;
    };
    let columns = cells
        .iter()
        .filter_map(|b| at(b).map(|(_, c, s)| c + s.max(1)))
        .max()
        .unwrap_or(0);
    if bottom - top < 2 || columns < 2 {
        return;
    }
    let lone = |row: u16| -> Option<usize> {
        let filled: Vec<usize> = (0..cells.len())
            .filter(|&k| at(&cells[k]).is_some_and(|(r, _, _)| r == row))
            .filter(|&k| !cells[k].text.trim().is_empty())
            .collect();
        match filled[..] {
            [k] if reads_as_caption_or_note(&cells[k].text, row == bottom) => Some(k),
            _ => None,
        }
    };
    let top_cell = lone(top);
    let bottom_cell = lone(bottom);
    for k in [top_cell, bottom_cell].into_iter().flatten() {
        cells[k].kind = BlockKind::Paragraph;
        // A paragraph carries no table evidence (codex, #2048).
        cells[k].grid = None;
    }
    if top_cell.is_some() {
        // The table now starts a row lower; its rows are counted from there.
        for b in cells.iter_mut() {
            if let BlockKind::TableCell { row, .. } = &mut b.kind {
                *row = row.saturating_sub(top + 1);
            }
        }
    }
}

/// Whether a text opens as a numbered caption: `Table 3`, `TABLE 2`,
/// `aTable 2.`, `*Table 1`, `Tab. 1`.
fn is_numbered_caption(t: &str) -> bool {
    // Read on the page's own case: a mark glued to the caption is a small
    // letter against a capital (`aTable 2.`), and `Stable isotope …` is a
    // word that happens to contain `table` (codex, #2048).
    let marked = t.trim_start_matches(['*', '\u{2020}']);
    let mut chars = marked.chars();
    let glued = matches!(
        (chars.next(), chars.next()),
        (Some(a), Some('T')) if a.is_ascii_lowercase()
    );
    let lowered = marked.to_lowercase();
    let s = if glued {
        &lowered[1..]
    } else {
        lowered.as_str()
    };
    let rest = s
        .strip_prefix("table")
        .or_else(|| s.strip_prefix("tab."))
        .map(str::trim_start);
    // A whole identifier: a figure, a Roman numeral that ends at a mark or
    // white (`II.`, `IV:`), or `S` and a figure (`S1`). `Table size` and
    // `Table value` are heads (codex, #2055).
    let Some(rest) = rest else {
        return false;
    };
    if rest.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return true;
    }
    let word: String = rest.chars().take_while(|c| c.is_alphanumeric()).collect();
    let roman = !word.is_empty() && word.len() <= 5 && word.chars().all(|c| "ivxlc".contains(c));
    let supplementary =
        word.len() >= 2 && word.starts_with('s') && word[1..].chars().all(|c| c.is_ascii_digit());
    roman || supplementary
}

/// Whether a lone cell's text is a caption, a note or a sentence rather than
/// a table's own words.
fn reads_as_caption_or_note(text: &str, below: bool) -> bool {
    let t = text.trim();
    let words = t.split_whitespace().count();
    let lower = t.to_lowercase();
    let first = t.chars().next();
    let caption = is_numbered_caption(t);
    // A sentence: long, and it ends like one or opens mid-way through one.
    // Of words, not of symbols: `x y z U iso*/ U eq` is seven tokens and a
    // crystal table's head.
    let real_words = t
        .split_whitespace()
        .filter(|w| w.chars().filter(|c| c.is_alphabetic()).count() >= 3)
        .count();
    let sentence = words >= 7
        && real_words >= 5
        && (t.ends_with('.') || first.is_some_and(char::is_lowercase));
    // A note: its opening word, or a mark, says what it is.
    let note = words >= 4
        && (["note", "abbreviation", "source", "data are", "values are"]
            .iter()
            .any(|w| lower.starts_with(w))
            // `Symmetry codes: (i) x −1, …`: a label of a word or three and a
            // colon, then what it explains.
            || t.split_once(':').is_some_and(|(label, rest)| {
                (1..=3).contains(&label.split_whitespace().count())
                    && label.chars().all(|c| c.is_alphabetic() || c.is_whitespace())
                    && rest.split_whitespace().count() >= 3
            })
            || first.is_some_and(|c| "*\u{2020}\u{2021}\u{a7}\u{b6}#".contains(c))
            // `a Values are…`, `b P for trend`: a letter mark, then a word.
            // `x y z U iso` is a crystal table's head, and its letters are
            // coordinates (PubTables train).
            || (first.is_some_and(|c| c.is_ascii_lowercase())
                && t.chars().nth(1).is_some_and(char::is_whitespace)
                && t.split_whitespace()
                    .nth(1)
                    .is_some_and(|w| w.chars().filter(|c| c.is_alphabetic()).count() >= 3)));
    // A SENTENCE ONLY UNDER THE TABLE. Over it, a lone cell that reads like
    // one is a cell the page before began: a table set over a page break
    // opens with the tail of its last row (ParseBench, a work programme).
    caption || note || (below && sentence)
}

/// Marks a table that runs on over a page break as the continuation of the
/// table before it (#574): the first cell of the next page's table gets
/// [`Block::continues_previous`], and the serializer reads the two as one.
///
/// A RELATION, as for a paragraph the page broke (#571): the two halves keep
/// their own pages, boxes and row numbers.
///
/// The page must say so three ways, because a page of one table followed by a
/// page of another is the common case and the halves of one table are not:
/// the table is the LAST thing on its page but for furniture, the next page's
/// table is the FIRST thing on it but for furniture, and the two have the same
/// number of columns standing at the same places -- each column's left edge
/// within `CONTINUED_COLUMN_TOLERANCE`. A head the second half repeats is left
/// in the blocks and left out of the table by the serializer.
pub(crate) fn tables_across_pages(blocks: &mut [Block]) {
    let body = |b: &Block| !matches!(b.kind, BlockKind::Header | BlockKind::Footer);
    let cell = |b: &Block| matches!(b.kind, BlockKind::TableCell { .. });
    let mut k = 1;
    while k < blocks.len() {
        if blocks[k].page == blocks[k - 1].page || !cell(&blocks[k]) {
            k += 1;
            continue;
        }
        // The last body block before the break must be a cell, and nothing but
        // furniture may stand between it and the break; the same after it.
        let before = (0..k).rev().find(|&i| body(&blocks[i]));
        let first_after = (k..blocks.len())
            .take_while(|&i| blocks[i].page == blocks[k].page)
            .find(|&i| body(&blocks[i]));
        let (Some(last), Some(first)) = (before, first_after) else {
            k += 1;
            continue;
        };
        if first != k || !cell(&blocks[last]) || blocks[last].page + 1 != blocks[k].page {
            k += 1;
            continue;
        }
        // The two tables' cells.
        let mut start = last;
        while start > 0 && cell(&blocks[start - 1]) && blocks[start - 1].page == blocks[last].page {
            start -= 1;
        }
        let mut end = k;
        while end < blocks.len() && cell(&blocks[end]) && blocks[end].page == blocks[k].page {
            end += 1;
        }
        if same_columns(&blocks[start..=last], &blocks[k..end]) {
            blocks[k].continues_previous = true;
        }
        k = end.max(k + 1);
    }
}

/// A heading keeps the tight subtitle line under it (#636).
///
/// A title with its subtitle directly under it -- `Statements of cash flows`
/// over `for the year ended 30 June 2003`, flush with it and nearly touching,
/// often in a smaller size -- is annotated as ONE heading, and the extraction
/// hands back a heading plus a paragraph (or two headings). So a heading joins
/// the block under it when that block sits tight, aligned and is no larger:
/// the same left edge or the same centre within a point, a gap of at most
/// 0.25 of the heading's size, no larger than the heading, no longer than ten
/// words, and neither side ending in `. ! ? :` nor the lower one opening with
/// a list marker. The join is greedy: a heading that took its subtitle tests
/// the line under that against the joined box, for the titles set on three
/// lines.
///
/// Runs after the learned pass and the table passes, where the kinds are
/// final, because that is what it reads: a `Heading` over a `Heading` or a
/// `Paragraph`. The kind stays `Heading`, the level stays the heading's, the
/// text is joined with a space, the spans are concatenated and the box is the
/// union.
pub(crate) fn join_heading_subtitles(blocks: &mut Vec<Block>) {
    let mut out: Vec<Block> = Vec::with_capacity(blocks.len());
    // The size the accumulated heading is set in: the median of the ORIGINAL
    // heading's spans, which the lines it collects do not move -- a subtitle
    // in a smaller size must not shrink the leading its own successor is
    // judged by.
    let mut head_size = 0.0;
    let mut joined = false;
    for block in blocks.drain(..) {
        let joins = out
            .last()
            .filter(|head| matches!(head.kind, BlockKind::Heading { .. }))
            .is_some_and(|head| {
                matches!(block.kind, BlockKind::Heading { .. } | BlockKind::Paragraph)
                    && block.page == head.page
                    && subtitle_joins(head, &block, head_size)
            });
        if joins {
            if let Some(head) = out.last_mut() {
                head.text.push(' ');
                head.text.push_str(&block.text);
                head.spans.extend(block.spans);
                head.bbox = [
                    head.bbox[0].min(block.bbox[0]),
                    head.bbox[1].min(block.bbox[1]),
                    head.bbox[2].max(block.bbox[2]),
                    head.bbox[3].max(block.bbox[3]),
                ];
            }
            joined = true;
        } else {
            head_size = if matches!(block.kind, BlockKind::Heading { .. }) {
                median_span_size(&block)
            } else {
                0.0
            };
            out.push(block);
        }
    }
    *blocks = out;
    if joined {
        for (n, block) in blocks.iter_mut().enumerate() {
            block.index = n;
        }
    }
}

/// How far the lower block's left edge, or the two blocks' centres, may stand
/// apart and still be the same title.
const SUBTITLE_ALIGN_TOLERANCE: f64 = 1.0;

/// The leading under a heading that still says "subtitle", as a multiple of
/// the heading's size. 0.25, chosen on the proxy tuning half plus the
/// independent tuning half: 0.3 joins the same proxy lines, so the smaller
/// value stays, and 0.35 and above add wrong joins. The held-out halves are
/// descriptive only.
const SUBTITLE_LEADING_EM: f64 = 0.25;

/// How much larger than the heading the line under it may be set.
const SUBTITLE_SIZE_TOLERANCE: f64 = 0.5;

/// The longest a subtitle is, in words. The middle of 8-12, which the tuning
/// half does not separate.
const SUBTITLE_MAX_WORDS: usize = 10;

/// The size a heading without spans is judged by. A block always carries the
/// spans it was made of, so this is the fallback for a block built without
/// them rather than a measurement.
const SUBTITLE_FALLBACK_EM: f64 = 10.0;

/// Whether the block under a heading is its subtitle: tight, aligned, no
/// larger, short, and neither prose nor a label.
fn subtitle_joins(head: &Block, under: &Block, head_size: f64) -> bool {
    let left_aligned = (under.bbox[0] - head.bbox[0]).abs() <= SUBTITLE_ALIGN_TOLERANCE;
    let head_centre = (head.bbox[0] + head.bbox[2]) / 2.0;
    let under_centre = (under.bbox[0] + under.bbox[2]) / 2.0;
    if !left_aligned && (under_centre - head_centre).abs() > SUBTITLE_ALIGN_TOLERANCE {
        return false;
    }
    let em = if head_size > 0.0 {
        head_size
    } else {
        SUBTITLE_FALLBACK_EM
    };
    // The boxes are `[x0, y0, x1, y1]` with y up: the heading's bottom is its
    // `y0`, the subtitle's top its `y1`.
    let gap = head.bbox[1] - under.bbox[3];
    if gap < 0.0 || gap > SUBTITLE_LEADING_EM * em {
        return false;
    }
    if median_span_size(under) > head_size + SUBTITLE_SIZE_TOLERANCE {
        return false;
    }
    if under.text.split_whitespace().count() > SUBTITLE_MAX_WORDS {
        return false;
    }
    // A heading joined with the SENTENCE under it (`January 1989` over
    // `Emperor Hirohito of Japan dead at 87.`), or a label with its value
    // (`Vollzitat:`): prose ends in `. ! ?` and a label in `:`.
    if ends_like_prose_or_a_label(&head.text) || ends_like_prose_or_a_label(&under.text) {
        return false;
    }
    !opens_with_a_list_marker(&under.text)
}

/// The median of the sizes the block's spans are set in, or 0 where the block
/// carries no spans.
fn median_span_size(block: &Block) -> f64 {
    let mut sizes: Vec<f64> = block.spans.iter().map(|span| span.font_size).collect();
    if sizes.is_empty() {
        return 0.0;
    }
    sizes.sort_by(f64::total_cmp);
    let mid = sizes.len() / 2;
    if sizes.len() % 2 == 1 {
        sizes[mid]
    } else {
        (sizes[mid - 1] + sizes[mid]) / 2.0
    }
}

/// Whether the text ends like a sentence or a label: a full stop, an
/// exclamation or question mark, or a colon.
fn ends_like_prose_or_a_label(text: &str) -> bool {
    matches!(text.trim_end().chars().last(), Some('.' | '!' | '?' | ':'))
}

/// Whether the text opens with a list marker: `1.`, `(a)`, a bullet.
///
/// An optional opening bracket, one to three figures or one lowercase letter
/// with its delimiter, or one bullet character with the space after it. What
/// the guard refuses is the line the list pass did not confirm as an item but
/// still opens as one.
fn opens_with_a_list_marker(text: &str) -> bool {
    let rest = text.trim_start_matches(char::is_whitespace);
    let rest = rest.strip_prefix('(').unwrap_or(rest);
    let mut chars = rest.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if first.is_ascii_digit() {
        let mut digits = 1;
        for c in chars {
            if c.is_ascii_digit() {
                digits += 1;
                if digits > 3 {
                    return false;
                }
            } else {
                return c == '.' || c == ')';
            }
        }
        return false;
    }
    if first.is_ascii_lowercase() {
        return match (chars.next(), chars.next()) {
            (Some(delimiter), Some(after)) if delimiter == '.' || delimiter == ')' => {
                after.is_whitespace()
            }
            _ => false,
        };
    }
    matches!(first, '•' | '●' | '▪' | '■' | '–' | '—' | '-' | '*')
        && chars.next().is_some_and(|c| c.is_whitespace())
}

/// A multi-line heading set in one style stays one heading (#574).
///
/// A heading the page sets on two or three lines in ONE style -- `ELECTRONIC
/// DELIVERY OF` over `ANNUAL MEETING MATERIALS`, a name over its title in the
/// same bold -- arrives as blocks of its own: the paragraph merge parts lines
/// the heading classifier then reads separately, and the extraction hands back
/// a heading plus a paragraph (or two headings) where the annotation says one.
/// So a heading joins the block next to it when the two are set alike -- the
/// same size after rounding to half a point, bold in both or in neither,
/// italic likewise -- stand on the same left edge or centre within a point at
/// most half an em apart, and together hold at most twenty words: the same
/// shape as [`join_heading_subtitles`], with the style rail added and the
/// leading rail widened, because these lines are set as one heading and not as
/// a title with its subtitle.
///
/// EITHER side may be the heading: a paragraph line over its heading joins
/// upward, and the result keeps the level of whichever piece was a heading --
/// the first, where both were. What it refuses is prose: the upper side ends
/// in `. ! ? ;`, or a PARAGRAPH under it ends in `. ! ? ; :`. A lower piece
/// the classifier already read as a heading joins whatever punctuation it
/// closes with: the guard exists to stop a heading swallowing a paragraph's
/// first sentence, and that risk only arises where the lower line is a
/// paragraph. A lower line that opens with a list marker always refuses. The
/// join chains to at most three pieces.
///
/// Runs beside [`join_heading_subtitles`], right after it, where the kinds are
/// final: a `Heading` over or under a `Heading` or a `Paragraph`, never a
/// table cell, a list item, a caption or furniture. The kind is `Heading`, the
/// text is joined with a space, the spans are concatenated and the box is the
/// union.
pub(crate) fn join_heading_lines(blocks: &mut Vec<Block>) {
    let mut out: Vec<Block> = Vec::with_capacity(blocks.len());
    // How many pieces the accumulated heading holds: at most three join.
    let mut pieces = 0usize;
    let mut joined = false;
    for block in blocks.drain(..) {
        let joins = match out.last() {
            Some(head)
                if matches!(head.kind, BlockKind::Heading { .. } | BlockKind::Paragraph)
                    && matches!(block.kind, BlockKind::Heading { .. } | BlockKind::Paragraph)
                    && (matches!(head.kind, BlockKind::Heading { .. })
                        || matches!(block.kind, BlockKind::Heading { .. }))
                    && block.page == head.page
                    && pieces < HEADING_LINE_MAX_PIECES =>
            {
                heading_line_joins(head, &block)
            }
            _ => false,
        };
        if joins {
            if let Some(head) = out.last_mut() {
                let level = match (head.kind, block.kind) {
                    (BlockKind::Heading { level }, _) => level,
                    (_, BlockKind::Heading { level }) => level,
                    _ => unreachable!("a heading join with no heading in it"),
                };
                head.kind = BlockKind::Heading { level };
                head.text.push(' ');
                head.text.push_str(&block.text);
                head.spans.extend(block.spans);
                head.bbox = [
                    head.bbox[0].min(block.bbox[0]),
                    head.bbox[1].min(block.bbox[1]),
                    head.bbox[2].max(block.bbox[2]),
                    head.bbox[3].max(block.bbox[3]),
                ];
            }
            pieces += 1;
            joined = true;
        } else {
            pieces = 1;
            out.push(block);
        }
    }
    *blocks = out;
    if joined {
        for (n, block) in blocks.iter_mut().enumerate() {
            block.index = n;
        }
    }
}

/// How far the lower block's left edge, or the two blocks' centres, may stand
/// apart and still be one heading.
const HEADING_LINE_ALIGN_TOLERANCE: f64 = 1.0;

/// The leading under a heading line that still says "one heading", as a
/// multiple of the size both lines are set in.
const HEADING_LINE_LEADING_EM: f64 = 0.5;

/// How far the lower block may overlap the upper one and still be its
/// continuation, in points.
const HEADING_LINE_MIN_GAP: f64 = -0.5;

/// The longest a multi-line heading is, in words over all its pieces.
const HEADING_LINE_MAX_WORDS: usize = 20;

/// How many pieces a heading joins from, at most.
const HEADING_LINE_MAX_PIECES: usize = 3;

/// The size a heading line without a measurable size is judged by. A block
/// always carries the spans it was made of, so this is the fallback for a
/// block built without a size rather than a measurement.
const HEADING_LINE_FALLBACK_EM: f64 = 10.0;

/// Whether the block under a heading line is its continuation: set alike,
/// aligned, standing close, short, and neither prose nor a label.
fn heading_line_joins(head: &Block, under: &Block) -> bool {
    let (Some(over), Some(below)) = (heading_line_style(head), heading_line_style(under)) else {
        return false;
    };
    if over != below {
        return false;
    }
    let em = if over.0 > 0.0 {
        over.0
    } else {
        HEADING_LINE_FALLBACK_EM
    };
    // The boxes are `[x0, y0, x1, y1]` with y up: the upper block's bottom is
    // its `y0`, the lower block's top its `y1`.
    let gap = head.bbox[1] - under.bbox[3];
    if gap < HEADING_LINE_MIN_GAP || gap > HEADING_LINE_LEADING_EM * em {
        return false;
    }
    let left_aligned = (under.bbox[0] - head.bbox[0]).abs() <= HEADING_LINE_ALIGN_TOLERANCE;
    let head_centre = (head.bbox[0] + head.bbox[2]) / 2.0;
    let under_centre = (under.bbox[0] + under.bbox[2]) / 2.0;
    if !left_aligned && (under_centre - head_centre).abs() > HEADING_LINE_ALIGN_TOLERANCE {
        return false;
    }
    // A heading joined with the SENTENCE under it (`Remuneration` over its
    // paragraph's first line), or a label with its value (`Vollzitat:`): prose
    // ends in `. ! ? ;` and a label in `:`. The lower guard only binds a
    // PARAGRAPH: a piece the classifier already read as a heading is reunited
    // with its first line whatever punctuation it closes with.
    if ends_like_a_sentence(&head.text)
        || (matches!(under.kind, BlockKind::Paragraph)
            && ends_like_a_sentence_or_a_label(&under.text))
    {
        return false;
    }
    if opens_with_a_list_marker(&under.text) {
        return false;
    }
    head.text.split_whitespace().count() + under.text.split_whitespace().count()
        <= HEADING_LINE_MAX_WORDS
}

/// The style a block is set in: its median size rounded to half a point, and
/// whether at least half its spans are bold or italic. `None` where the block
/// carries no spans, which no style can be read off.
fn heading_line_style(block: &Block) -> Option<(f64, bool, bool)> {
    if block.spans.is_empty() {
        return None;
    }
    let size = (median_span_size(block) * 2.0).round() / 2.0;
    let bold = block.spans.iter().filter(|span| span.is_bold).count() * 2 >= block.spans.len();
    let italic = block.spans.iter().filter(|span| span.is_italic).count() * 2 >= block.spans.len();
    Some((size, bold, italic))
}

/// Whether the text ends like a sentence: a full stop, an exclamation or
/// question mark, or a semicolon.
fn ends_like_a_sentence(text: &str) -> bool {
    matches!(text.trim_end().chars().last(), Some('.' | '!' | '?' | ';'))
}

/// Whether the text ends like a sentence or a label: as above, and a colon.
fn ends_like_a_sentence_or_a_label(text: &str) -> bool {
    matches!(
        text.trim_end().chars().last(),
        Some('.' | '!' | '?' | ';' | ':')
    )
}

/// How much wider than a paragraph's own median step a step inside it has to
/// be before it is read as a paragraph boundary (#574).
///
/// One and a half, chosen on the proxy's tuning half and the independent
/// tuning half: 1.5 beats 1.8 on both (proxy tuning coverage +0.0025 against
/// +0.0017, independent tuning +0.0083 against +0.0071), and the held-out
/// halves only confirm.
const PARAGRAPH_STEP_JUMP: f64 = 1.5;

/// How far the left edges of the two lines either side of a step jump may
/// stand apart, in ems, before the jump is a column change and not this
/// rule's business (#574). One em: a first-line indent never clears it, and
/// the cut owns whatever does.
const PARAGRAPH_STEP_LEFT_EM: f64 = 1.0;

/// How many non-space characters the median line of a paragraph may hold
/// before its lines stop reading as stacked glyphs (#574). Two, chosen on
/// the proxy's tuning half and the independent tuning half: the stacked
/// cuts there sit at one or two to the line and the shortest intended prose
/// cut at five, so the value refuses the stacked cuts and no prose. Two
/// word-changing cuts sit higher still, and the reassembly guard below
/// catches those. The held-out halves only confirm.
const PARAGRAPH_STEP_STACKED_LINE: usize = 2;

/// How far off the horizontal a span's transform may lean before the span
/// counts as rotated (#574): |b| or |c| past a tenth of |a|. An upright
/// span sits at zero against its own size; a 90-degree stamp sits at all
/// of it. The page set the block vertically, whatever the line grouping
/// says.
const PARAGRAPH_STEP_ROTATION: f64 = 0.1;

/// How much of a paragraph's non-space characters may be figures and their
/// punctuation before the block is a number block and not prose (#574).
/// Half: ORI p64's released table sits at practically all of them, while
/// the most numeric intended prose cut on either tuning half sits under
/// it. Chosen on the proxy's tuning half and the independent tuning half,
/// and the held-out halves only confirm.
const PARAGRAPH_STEP_DIGIT_SHARE: f64 = 0.5;

/// A paragraph the builder merged across a paragraph-sized step, cut where its
/// own white jumps (#574).
///
/// The builder's own break, `PARAGRAPH_LEADING_TOLERANCE` (1.35) times the
/// leading in force, misses these because that leading is measured on the RUN
/// while the paragraph still has one line to its name -- and on a run set at
/// paragraph distance the run leading IS the paragraph gap. The learned pass's
/// merges and the list continuation then join more blocks without asking the
/// geometry at all. So this runs on the FINAL blocks, after the learned pass
/// and the table passes, beside [`join_heading_subtitles`]: for each paragraph
/// of three lines or more, cut between two lines where the baseline step is at
/// least [`PARAGRAPH_STEP_JUMP`] times the block's own median step. The pieces
/// keep the parent's kind, page and verdicts, and carry their own spans, text
/// and boxes, in order.
///
/// NOT APPLIED to headings, list items, captions or table cells: their kinds
/// are statements about the whole block, and a step inside one is the shape of
/// the thing rather than a boundary. Not applied where the two lines' left
/// edges differ by more than an em, which is the cut's business.
///
/// Three guards keep the cut from doing damage, all refused in full -- a
/// block the rule may not cut is returned whole, never in part:
///
/// 1. The pieces must read the parent's characters in the same order
///    ([`step_split_reassembles`]). A cut between stacked glyphs reassembles
///    with spaces inserted or, where the page set the word bottom-to-top,
///    backwards (`Thickness` into `ss e` plus `kn ic h T`); every intended
///    prose cut reassembles identically, so the guard refuses exactly the
///    salad.
/// 2. Stacked glyphs are not lines ([`step_split_stacked`],
///    [`step_split_vertical`]): a block whose median line holds
///    [`PARAGRAPH_STEP_STACKED_LINE`] non-space characters or fewer, or
///    whose spans mostly carry a rotation, is vertical text the rule never
///    looks at.
/// 3. A number block is not prose ([`step_split_number_block`]): a block at
///    or past [`PARAGRAPH_STEP_DIGIT_SHARE`] figures and their punctuation
///    is the table pass's business.
///
/// Every refusal goes out under `PDFLUENT_BLOCK_TRACE`, naming the guard
/// and the parent, the way the builder's own rails do.
pub(crate) fn split_paragraphs_on_steps(blocks: &mut Vec<Block>) {
    let mut out: Vec<Block> = Vec::with_capacity(blocks.len());
    let mut split = false;
    for block in blocks.drain(..) {
        if !matches!(block.kind, BlockKind::Paragraph) {
            out.push(block);
            continue;
        }
        let pieces = split_paragraph_on_steps(&block);
        if pieces.len() <= 1 {
            out.push(block);
        } else {
            split = true;
            out.extend(pieces);
        }
    }
    *blocks = out;
    if split {
        for (n, block) in blocks.iter_mut().enumerate() {
            block.index = n;
        }
    }
}

/// Whether the block's characters are mostly figures and their punctuation
/// -- a table released as one paragraph, like ORI p64's, and the table
/// pass's business rather than this rule's (#574).
fn step_split_number_block(block: &Block) -> bool {
    let mut total = 0usize;
    let mut figures = 0usize;
    for c in block.text.chars().filter(|c| !c.is_whitespace()) {
        total += 1;
        if matches!(
            c,
            '0'..='9' | '.' | ',' | '(' | ')' | '%' | '$' | '€' | '£' | '–' | '-'
        ) {
            figures += 1;
        }
    }
    total > 0 && figures as f64 >= PARAGRAPH_STEP_DIGIT_SHARE * total as f64
}

/// Whether the span leans off the horizontal: |b| or |c| past
/// [`PARAGRAPH_STEP_ROTATION`] of |a| (#574). A span with no transform
/// carries no lean.
fn step_split_rotated(span: &TextSpan) -> bool {
    match span.transform {
        Some([a, b, c, ..]) => {
            b.abs() > PARAGRAPH_STEP_ROTATION * a.abs()
                || c.abs() > PARAGRAPH_STEP_ROTATION * a.abs()
        }
        None => false,
    }
}

/// Whether the block's lines are stacked glyphs rather than lines of prose
/// -- vertical text, a rotated stamp, a letter-stacked axis label (#574):
/// the median line holds [`PARAGRAPH_STEP_STACKED_LINE`] non-space
/// characters or fewer.
fn step_split_stacked(block: &Block, lines: &[Vec<usize>]) -> bool {
    let mut counts: Vec<usize> = lines
        .iter()
        .map(|line| line.iter().map(|i| char_count(&block.spans[*i])).sum())
        .collect();
    counts.sort_unstable();
    !counts.is_empty() && counts[counts.len() / 2] <= PARAGRAPH_STEP_STACKED_LINE
}

/// Whether the page set the block vertically, whatever the line grouping
/// says: most spans carry a rotation (#574).
fn step_split_vertical(block: &Block) -> bool {
    let rotated = block.spans.iter().filter(|s| step_split_rotated(s)).count();
    rotated * 2 > block.spans.len()
}

/// Whether the pieces read the parent's characters in the same order (#574).
/// Both sides drop the hyphen-plus-space a merge boundary leaves where
/// [`join_lines`] would have closed the word, and all white space; what is
/// left must match exactly. Script-neutral, and exact for every intended
/// prose cut.
fn step_split_reassembles(parent: &Block, pieces: &[Block]) -> bool {
    let plain = |text: &str| {
        text.replace("- ", "")
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<Vec<_>>()
    };
    let want = plain(&parent.text);
    let mut got: Vec<char> = Vec::with_capacity(want.len());
    for piece in pieces {
        got.extend(plain(&piece.text));
    }
    want == got
}

/// A refused split, out under `PDFLUENT_BLOCK_TRACE` the way the builder's
/// own rails go: the guard and the head of the parent.
fn step_split_trace(guard: &str, block: &Block) {
    if tracing() {
        let head: String = block.text.chars().take(80).collect();
        eprintln!("STEP-SPLIT {guard} | {head}");
    }
}

/// The one paragraph cut into its pieces, or itself alone where no step jumps.
fn split_paragraph_on_steps(block: &Block) -> Vec<Block> {
    if step_split_number_block(block) {
        step_split_trace("number-block", block);
        return vec![block.clone()];
    }
    let mut order: Vec<usize> = (0..block.spans.len()).collect();
    order.sort_by(|a, b| {
        block.spans[*b]
            .y
            .partial_cmp(&block.spans[*a].y)
            .unwrap_or(Ordering::Equal)
    });
    // Back into lines by baseline, the way `assemble_cell` groups a cell's
    // spans: half an em holds a superscript on its line and never two lines.
    let mut lines: Vec<Vec<usize>> = Vec::new();
    for i in order {
        let same_line = lines.last().is_some_and(|line: &Vec<usize>| {
            let first = &block.spans[line[0]];
            let span = &block.spans[i];
            (first.y - span.y).abs() <= first.font_size.max(span.font_size) * 0.5
        });
        match lines.last_mut() {
            Some(line) if same_line => line.push(i),
            _ => lines.push(vec![i]),
        }
    }
    if lines.len() < 3 {
        return vec![block.clone()];
    }
    if step_split_stacked(block, &lines) {
        step_split_trace("stacked-glyphs", block);
        return vec![block.clone()];
    }
    if step_split_vertical(block) {
        step_split_trace("rotated-spans", block);
        return vec![block.clone()];
    }
    // The baseline each line sits on: its longest span's, the way the builder
    // anchors a line, so a raised figure does not move it.
    let baseline_of = |line: &[usize]| -> f64 {
        line.iter()
            .map(|i| &block.spans[*i])
            .max_by_key(|span| char_count(span))
            .map(|span| span.y)
            .unwrap_or(0.0)
    };
    let left_of = |line: &[usize]| -> f64 {
        line.iter()
            .map(|i| block.spans[*i].x)
            .fold(f64::INFINITY, f64::min)
    };
    let mut steps: Vec<f64> = lines
        .windows(2)
        .map(|pair| baseline_of(&pair[0]) - baseline_of(&pair[1]))
        .filter(|step| *step > 0.0)
        .collect();
    if steps.is_empty() {
        return vec![block.clone()];
    }
    steps.sort_by(f64::total_cmp);
    let median = steps[steps.len() / 2];
    if !median.is_finite() || median <= 0.0 {
        return vec![block.clone()];
    }
    let em = median_span_size(block);
    let em = if em > 0.0 { em } else { SUBTITLE_FALLBACK_EM };
    let mut cuts: Vec<bool> = vec![false; lines.len() - 1];
    for (i, pair) in lines.windows(2).enumerate() {
        let step = baseline_of(&pair[0]) - baseline_of(&pair[1]);
        cuts[i] = step >= PARAGRAPH_STEP_JUMP * median
            && (left_of(&pair[0]) - left_of(&pair[1])).abs() <= PARAGRAPH_STEP_LEFT_EM * em;
    }
    if !cuts.iter().any(|cut| *cut) {
        return vec![block.clone()];
    }
    let mut pieces: Vec<Block> = Vec::new();
    let mut start = 0usize;
    for (i, cut) in cuts.iter().enumerate() {
        if *cut {
            pieces.push(piece_of(block, &lines, start, i + 1));
            start = i + 1;
        }
    }
    pieces.push(piece_of(block, &lines, start, lines.len()));
    if !step_split_reassembles(block, &pieces) {
        step_split_trace("words-changed", block);
        return vec![block.clone()];
    }
    pieces
}

/// Lines `start..end` of the paragraph as a block of their own: the parent's
/// kind, page and verdicts, their own spans, text and box. The text goes
/// through the same assembly as any block's -- the word spacing and the
/// right-to-left reordering per line, [`join_lines`] across them -- so a piece
/// reads the way the builder would have written it.
fn piece_of(block: &Block, lines: &[Vec<usize>], start: usize, end: usize) -> Block {
    let mut indices: Vec<usize> = lines[start..end].iter().flatten().copied().collect();
    indices.sort_unstable();
    let spans: Vec<TextSpan> = indices.iter().map(|i| block.spans[*i].clone()).collect();
    let mut texts: Vec<String> = Vec::with_capacity(end - start);
    for line in &lines[start..end] {
        let mut order: Vec<usize> = line.clone();
        order.sort_unstable();
        let row: Vec<TextSpan> = order.iter().map(|i| block.spans[*i].clone()).collect();
        texts.push(TextBlock { spans: row }.text());
    }
    let mut piece = Block {
        kind: block.kind,
        page: block.page,
        bbox: [0.0; 4],
        text: join_lines(&texts),
        spans,
        index: 0,
        continues_previous: start == 0 && block.continues_previous,
        confidence: block.confidence,
        grid: block.grid,
        table_origin: block.table_origin,
        trace_learned: block.trace_learned,
    };
    recompute_bbox(&mut piece);
    piece
}

/// The left edge of every column of a run of cells, from its unspanned cells.
fn column_lefts(cells: &[Block]) -> Vec<Option<f64>> {
    let mut lefts: Vec<Option<f64>> = Vec::new();
    for b in cells {
        if let BlockKind::TableCell {
            column,
            column_span: 1,
            ..
        } = b.kind
        {
            let c = column as usize;
            if lefts.len() <= c {
                lefts.resize(c + 1, None);
            }
            let x = b.bbox[0];
            if x.is_finite() && b.bbox != [0.0; 4] {
                lefts[c] = Some(lefts[c].map_or(x, |l: f64| l.min(x)));
            }
        }
    }
    lefts
}

fn same_columns(a: &[Block], b: &[Block]) -> bool {
    let (la, lb) = (column_lefts(a), column_lefts(b));
    la.len() >= 2
        && la.len() == lb.len()
        && la.iter().zip(&lb).all(|(x, y)| match (x, y) {
            (Some(x), Some(y)) => (x - y).abs() <= CONTINUED_COLUMN_TOLERANCE,
            _ => true,
        })
        && la
            .iter()
            .zip(&lb)
            .filter(|(x, y)| x.is_some() && y.is_some())
            .count()
            >= 2
}

/// How far apart, in points, a column's left edge may stand on two pages and
/// still be the same column.
const CONTINUED_COLUMN_TOLERANCE: f64 = 6.0;

/// Which way a column's indents point, decided once for the whole column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IndentStyle {
    /// No line in the column is indented against the others.
    Flat,
    /// An indented line STARTS a paragraph.
    FirstLine,
    /// An indented line CONTINUES the one above it.
    Hanging,
}

/// One line, reduced to the numbers the merge decides on.
struct LineMetrics {
    /// Leftmost edge of the line's ordinary text.
    left: f64,
    /// The left edge the indent is measured from. Equal to `left`, except
    /// beside a drop cap, where it is the cap's own left: the lines set around
    /// an initial are indented by the initial and not by the typesetter.
    indent_left: f64,
    /// Rightmost edge of the line.
    right: f64,
    /// The baseline the line sits on.
    baseline: f64,
    /// Top of the line's glyphs.
    top: f64,
    /// Bottom of the line's glyphs.
    bottom: f64,
    /// The type size most of the line's characters are set in.
    font_size: f64,
    /// Whether most of the line's characters are set in a bold face.
    bold: bool,
    /// The line as it reads.
    text: String,
    /// Index into `spans` of an oversized initial, when the line carries one.
    initial: Option<usize>,
    /// Gaps inside the line wide enough to be a gutter between table cells.
    wide_gaps: Vec<(f64, f64)>,
    /// Whether a second line confirmed one of those gaps.
    tabular: bool,
    /// The list marker this line opens with, if any -- a CANDIDATE, which one
    /// line's own text is all the evidence there is for (#569).
    marker: Option<Marker>,
    /// Where the text after that marker begins, when the page drew the marker
    /// in a span of its own. `None` where it did not: a marker glued into the
    /// line's first span has no column of its own to measure.
    marker_text_left: Option<f64>,
    /// The candidate once a run of agreeing markers in the same column has
    /// confirmed it. Set by [`mark_list_items`] and by nothing else.
    item: Option<ItemMark>,
}

/// A confirmed list item: what to take off the front of the line, how deep it
/// sits, and the number the document gave it.
#[derive(Debug, Clone, Copy)]
struct ItemMark {
    /// Bytes of the marker and the white space after it, from the start of the
    /// line's text.
    len: usize,
    /// Nesting depth, 0 for the outermost list in the column.
    level: u8,
    /// The number the document wrote, or `None` for a bullet or an outline's
    /// `II.4.1`, which the item's text keeps.
    ordinal: Option<u32>,
    /// The item's own content column: where its text starts, and therefore
    /// where its continuation lines hang from.
    text_left: Option<f64>,
}

/// The right edge of a span, measured where the font gave a width and estimated
/// at half an em per character where it did not -- the same fallback
/// [`crate::text_info::TextSpanInfo`] states as the wire contract.
pub(crate) fn span_right(span: &TextSpan) -> f64 {
    if span.width > 0.0 {
        span.x + span.width
    } else {
        span.x + span.font_size * 0.5 * span.text.chars().count() as f64
    }
}

/// The vertical extent of a span, from its glyph boxes where they exist and
/// from the type size where they do not.
fn span_extent(span: &TextSpan) -> (f64, f64) {
    let mut bottom = f64::INFINITY;
    let mut top = f64::NEG_INFINITY;
    for bounds in &span.char_bounds {
        bottom = bottom.min(bounds[1]);
        top = top.max(bounds[3]);
    }
    if bottom.is_finite() && top.is_finite() && top > bottom {
        return (bottom, top);
    }
    // A conventional ascender and descender. Used only to ask whether a ruling
    // line falls between two lines and to bound a block, so it does not have to
    // be the font's own.
    (span.y - span.font_size * 0.2, span.y + span.font_size * 0.8)
}

fn char_count(span: &TextSpan) -> usize {
    span.text.chars().filter(|c| !c.is_whitespace()).count()
}

/// Where the text after a list marker begins: the left edge of the first span
/// the marker does not reach into (#569).
///
/// A typeset list draws its marker in a run of its own, so the answer is the
/// next span's `x` and it is exact. A marker the page drew inside the line's
/// first span -- one text run holding `1. The committee` -- has no edge of its
/// own to give, and this answers `None` rather than estimating one: the caller
/// has a geometric fallback for that case and an invented column would silence
/// it.
///
/// Counted in non-whitespace characters, because the line's text is the spans
/// joined with a space wherever the page left a gap: the byte offsets of the
/// marker inside that text are not offsets into any span, and the characters
/// are the one thing both sides agree on.
///
/// EXACTLY, and `None` the moment the count overshoots. A span that runs past
/// the end of the marker is a span the marker is INSIDE, and the next span
/// after it is wherever the line happens to change face -- on `cweb` that is a
/// word in the middle of the item, forty points in, and an item whose content
/// column was read from there loses every line that wraps under it.
fn text_left_after(line: &TextBlock, order: &[usize], marker_chars: usize) -> Option<f64> {
    if marker_chars == 0 {
        return None;
    }
    let mut seen = 0usize;
    for i in order {
        match seen.cmp(&marker_chars) {
            // A run of white space after the marker is the spacing the page
            // set it apart with, not where its text begins (#574).
            Ordering::Equal if char_count(&line.spans[*i]) == 0 => {}
            Ordering::Equal => return Some(line.spans[*i].x),
            Ordering::Greater => return None,
            Ordering::Less => seen += char_count(&line.spans[*i]),
        }
    }
    None
}

impl LineMetrics {
    /// Everything the merge asks about one line, an oversized initial taken out
    /// of its geometry where the line has one.
    fn of(line: &TextBlock) -> Self {
        Self::of_with(line, true, false)
    }

    /// The same, with `allow_initial` false: every span counts, including a
    /// leading oversized one. [`confirm_initials`] asks for this again on a
    /// line whose initial turned out to be a marker -- the geometry taken with
    /// the marker held out is not the geometry of a line that keeps it, and
    /// leaving it behind moved that line's left edge a marker's width to the
    /// right and made the page read as first-line indented.
    ///
    /// `cell_path_words` is
    /// [`ExtractOptions::cell_path_words`](ExtractOptions::cell_path_words):
    /// the line's text reads with the cell-path word rules where true.
    fn of_with(line: &TextBlock, allow_initial: bool, cell_path_words: bool) -> Self {
        let mut order: Vec<usize> = (0..line.spans.len()).collect();
        order.sort_by(|a, b| {
            line.spans[*a]
                .x
                .partial_cmp(&line.spans[*b].x)
                .unwrap_or(Ordering::Equal)
        });

        // The initial, if there is one: the leftmost span, no more than two
        // characters, set far larger than the rest of the line. It is taken out
        // of the geometry below -- a three-line initial sits on the baseline of
        // the THIRD line, so left unhandled it makes that line start a page's
        // width to the left of its own paragraph and tower over it in size.
        let dominant_size = dominant_font_size(line.spans.iter());
        let initial = order.first().copied().filter(|&first| {
            allow_initial
                && line.spans.len() > 1
                && char_count(&line.spans[first]) <= INITIAL_MAX_CHARS
                && char_count(&line.spans[first]) > 0
                && dominant_size > 0.0
                && line.spans[first].font_size >= INITIAL_SIZE_RATIO * dominant_size
        });

        let body: Vec<usize> = order
            .iter()
            .copied()
            .filter(|i| Some(*i) != initial)
            .collect();
        let measured: Vec<&TextSpan> = if body.is_empty() {
            order.iter().map(|i| &line.spans[*i]).collect()
        } else {
            body.iter().map(|i| &line.spans[*i]).collect()
        };

        let left = measured.iter().map(|s| s.x).fold(f64::INFINITY, f64::min);
        let right = measured
            .iter()
            .map(|s| span_right(s))
            .fold(f64::NEG_INFINITY, f64::max);
        let mut bottom = f64::INFINITY;
        let mut top = f64::NEG_INFINITY;
        for span in &measured {
            let (b, t) = span_extent(span);
            bottom = bottom.min(b);
            top = top.max(t);
        }

        // The baseline and the size of the span that carries most of the line's
        // characters, so a superscript marker or a bullet does not speak for it.
        let anchor = anchor_span(measured.iter().copied()).or_else(|| line.spans.first());
        let baseline = anchor.map(|s| s.y).unwrap_or(0.0);
        let font_size = match dominant_font_size(measured.iter().copied()) {
            size if size > 0.0 => size,
            _ => anchor.map(|s| s.font_size).unwrap_or(0.0),
        };
        let bold = spans_read_bold(measured.iter().copied());

        // Gaps inside the line wide enough to be a gutter rather than a word
        // space. Recorded here and believed only in `mark_tabular`, where a
        // second line has to show one at the same x.
        let mut wide_gaps = Vec::new();
        if font_size > 0.0 {
            let threshold = TABULAR_GAP_EM * font_size;
            for pair in body.windows(2) {
                let end = span_right(&line.spans[pair[0]]);
                let start = line.spans[pair[1]].x;
                if start - end >= threshold {
                    wide_gaps.push((end, start));
                }
            }
        }

        // WHAT THE PAGE PUT AT THE HEAD OF THE LINE, in the three ways the
        // characters alone cannot say it. All three are read off the span the
        // line's TEXT opens with, and only where that span really is the front
        // of it: the spans are in draw order and the text is assembled from
        // them, so a line whose leftmost span is not its first, or whose text
        // was reordered for a right-to-left reading, has no prefix to measure.
        // How LONG the prefix may be is each rule's own business, so the cap
        // is on each of them and not here.
        let text = line.text_with_cell_path_words(cell_path_words);
        let head = line
            .spans
            .first()
            .filter(|span| char_count(span) > 0)
            .filter(|span| line.spans.len() > 1 && text.starts_with(span.text.as_str()));
        // A bullet drawn in a picture font decodes to whatever code the font
        // maps it to -- `l` in ZapfDingbats -- so the character says nothing
        // and only the font does.
        let symbol_prefix = head
            .filter(|span| char_count(span) <= MARKER_MAX_CHARS)
            .filter(|span| lists::is_symbol_font(span.font_name.as_deref()))
            .map(|span| span.text.len());
        // A REFERENCE NUMBERED WITH A RAISED FIGURE (#578). Smaller than the
        // line it opens and above its baseline is what a journal's reference
        // number is and what a sentence opening with a year is not. Three
        // characters, because a bibliography passes a hundred entries and the
        // grammar next door reads three digits.
        let raised_prefix = head
            .filter(|span| char_count(span) <= MAX_ORDINAL_CHARS)
            .filter(|span| font_size > 0.0 && span.font_size < font_size * RAISED_SIZE)
            .filter(|span| span.y > baseline + RAISED_RISE * font_size)
            .map(|span| span.text.len());
        // A GLYPH THE PAGE SET APART (#578): one character, in a run of its
        // own, and then a gap no word space could be. The gap is measured to
        // the next span of the line in draw order that draws a character,
        // which is where its text goes on: a page that sets the gap with a
        // space run of its own (`t`, ` `, `Gross margins`, NYSE_MSA on the
        // rig, #574) has put the space in the gap, not closed it.
        let apart_prefix = head
            .filter(|span| char_count(span) == 1 && font_size > 0.0)
            .filter(|span| {
                line.spans
                    .iter()
                    .skip(1)
                    .find(|next| char_count(next) > 0)
                    .is_some_and(|next| next.x - span_right(span) >= MARKER_GAP_EM * font_size)
            })
            .map(|span| span.text.len());

        let marker = lists::marker_at(
            &text,
            lists::Lead {
                symbol: symbol_prefix,
                raised: raised_prefix,
                apart: apart_prefix,
            },
        );
        let marker_text_left = marker.and_then(|marker| {
            let marker_chars = text[..marker.len]
                .chars()
                .filter(|c| !c.is_whitespace())
                .count();
            text_left_after(line, &order, marker_chars)
        });

        Self {
            left,
            indent_left: left,
            right,
            baseline,
            top,
            bottom,
            font_size,
            bold,
            marker,
            marker_text_left,
            text,
            initial,
            wide_gaps,
            tabular: false,
            item: None,
        }
    }

    /// Whether the line reads as the end of a sentence.
    fn ends_sentence(&self) -> bool {
        let trimmed = self
            .text
            .trim_end()
            .trim_end_matches(['"', '\'', '”', '’', ')', ']', '»']);
        matches!(
            trimmed.chars().next_back(),
            Some('.') | Some('!') | Some('?') | Some(':') | Some(';')
        )
    }
}

/// The type size most of a set of spans' GLYPHS are set in.
///
/// Per glyph, because a span's own `font_size` is the size of its FIRST glyph
/// and a span is a run of glyphs the space consensus merged: a bullet set in a
/// symbol font two points smaller than the item beside it makes the whole line
/// read as two points smaller, and every item of every bulleted list on
/// `tracemonkey` then failed the type-size test against its own continuation.
/// `draw_glyph` writes each glyph's bound as `[x, y, x + advance, y + size]`,
/// so the size of every glyph drawn is there to be read; a span with no bounds
/// -- every hand-built one -- answers with its own.
pub(crate) fn dominant_font_size<'a>(spans: impl Iterator<Item = &'a TextSpan>) -> f64 {
    let mut sizes: Vec<f64> = Vec::new();
    for span in spans {
        if span.char_bounds.is_empty() {
            sizes.extend(std::iter::repeat_n(span.font_size, char_count(span).max(1)));
        } else {
            sizes.extend(span.char_bounds.iter().map(|bound| bound[3] - bound[1]));
        }
    }
    sizes.retain(|size| *size > 0.0);
    if sizes.is_empty() {
        return 0.0;
    }
    sizes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    sizes[sizes.len() / 2]
}

/// Whether most of a set of spans' characters are set in a bold face.
///
/// Per character, so a bold phrase inside a line of prose does not speak for
/// it. Shared with the v2 release rejoin, which reads a released cell's
/// weight the way the builder reads a line's.
pub(crate) fn spans_read_bold<'a>(spans: impl Iterator<Item = &'a TextSpan>) -> bool {
    let mut heavy = 0usize;
    let mut total = 0usize;
    for span in spans {
        let chars = char_count(span);
        total += chars;
        if span.is_bold {
            heavy += chars;
        }
    }
    total > 0 && heavy * 2 > total
}

/// The span that carries most of a line's characters, so a superscript marker
/// or a bullet does not speak for its baseline. Shared with the v2 release
/// rejoin, which reads a released cell's baseline the way the builder reads
/// a line's.
pub(crate) fn anchor_span<'a>(spans: impl Iterator<Item = &'a TextSpan>) -> Option<&'a TextSpan> {
    spans.max_by_key(|s| char_count(s))
}

/// How much of the narrower of two lines lies under the wider one.
fn overlap_fraction(a: &LineMetrics, b: &LineMetrics) -> f64 {
    overlap_fraction_of(a.left, a.right, b.left, b.right)
}

/// The same, on bare edges: the narrow-column pair join reads lines the
/// builder never measured ([`JoinLine`]), and the fraction is one definition.
pub(crate) fn overlap_fraction_of(a_left: f64, a_right: f64, b_left: f64, b_right: f64) -> f64 {
    let overlap = a_right.min(b_right) - a_left.max(b_left);
    let narrower = (a_right - a_left).max(0.0).min((b_right - b_left).max(0.0));
    if narrower <= 0.0 {
        return if overlap >= 0.0 { 1.0 } else { 0.0 };
    }
    (overlap / narrower).max(0.0)
}

/// A wide gap inside a line is a word space until a neighbouring line shows one
/// at the same place. Two lines with white running down between them at the
/// same x is a gutter, and a gutter inside a region the XY-cut refused to split
/// is a table nobody ruled.
fn mark_tabular(metrics: &mut [LineMetrics]) {
    let confirmed: Vec<bool> = (0..metrics.len())
        .map(|i| {
            if metrics[i].wide_gaps.is_empty() {
                return false;
            }
            let neighbours = [i.checked_sub(1), (i + 1 < metrics.len()).then_some(i + 1)];
            neighbours.into_iter().flatten().any(|j| {
                metrics[i].wide_gaps.iter().any(|mine| {
                    metrics[j]
                        .wide_gaps
                        .iter()
                        .any(|theirs| mine.0 < theirs.1 && theirs.0 < mine.1)
                })
            })
        })
        .collect();
    for (line, is_tabular) in metrics.iter_mut().zip(confirmed) {
        line.tabular = is_tabular;
    }
}

/// Which oversized initials are drop caps, and what the lines beside them are
/// indented by.
///
/// WHAT MAKES IT A DROP CAP is that the page set lines BESIDE it. A large
/// bullet, a section marker, a pull-quote's opening mark: each is a short span
/// set larger than the line it opens, and [`LineMetrics::of`] cannot tell them
/// from an initial, because the difference is not in the line -- it is in
/// whether anything is set into the space the letter takes up (codex, round 2).
/// An initial with nothing beside it is left in its line, where it belongs, and
/// the line keeps its own geometry.
///
/// The lines that ARE beside one are indented by the initial and not by the
/// typesetter, so they are measured from the initial's own left edge.
fn confirm_initials(metrics: &mut [LineMetrics], lines: &[&TextBlock], cell_path_words: bool) {
    let boxes: Vec<Option<(f64, f64, f64, f64)>> = metrics
        .iter()
        .zip(lines)
        .map(|(m, line)| {
            let span = &line.spans[m.initial?];
            let (bottom, top) = span_extent(span);
            Some((span.x, span_right(span), bottom, top))
        })
        .collect();

    // At the cap's top, not below it: the first line beside a three-line
    // initial is set with its own cap height against the initial's, so its
    // baseline sits AT the top of the initial's box and a strict comparison
    // leaves that line -- the one the initial belongs to -- out of the very set
    // this exists to build.
    let beside = |line: &LineMetrics, (left, right, bottom, top): (f64, f64, f64, f64)| {
        let _ = left;
        line.baseline > bottom && line.baseline <= top && line.left >= right - 1.0
    };

    let mut confirmed: Vec<(f64, f64, f64, f64)> = Vec::new();
    let mut rejected: Vec<usize> = Vec::new();
    for (i, cap) in boxes.iter().enumerate() {
        let Some(cap) = *cap else { continue };
        // Beside, and ABOVE: a line on the initial's own baseline is the line
        // it was drawn in.
        let has_neighbour = metrics
            .iter()
            .enumerate()
            .any(|(j, line)| j != i && line.baseline > metrics[i].baseline && beside(line, cap));
        if has_neighbour {
            confirmed.push(cap);
        } else {
            rejected.push(i);
        }
    }
    for i in rejected {
        metrics[i] = LineMetrics::of_with(lines[i], false, cell_path_words);
    }
    if confirmed.is_empty() {
        return;
    }
    for line in metrics.iter_mut() {
        for cap in &confirmed {
            if beside(line, *cap) {
                line.indent_left = line.indent_left.min(cap.0);
            }
        }
    }
}

/// Which candidate markers a region confirms as list items, and how deep each
/// one sits (#569).
///
/// A marker on one line is a coincidence; a region that puts the same KIND of
/// marker at the same left edge twice, climbing if it is numbered, is a list.
/// `crate::lists` owns that rule and the grammar under it -- this is where the
/// page's own geometry is handed to it.
///
/// THE REGION AND NOT THE COLUMN RUN, which is the same frame a left edge means
/// anything in -- the XY-cut's leaf -- and deliberately not the finer split
/// [`column_runs`] makes inside it. That split fires when two consecutive lines
/// barely overlap horizontally, and a SHORT nested item is exactly that: on
///
/// ```text
/// - Fruit
///   - Apple
///   - Pear
/// - Vegetables
/// ```
///
/// `- Fruit` ends before `- Apple` begins, so the run seam falls between every
/// level change. Confirmed inside those slices, each outer marker is a
/// singleton and never an item at all, and the inner pair -- the only run left
/// -- is the leftmost edge of its slice and comes out as level 0. A list is one
/// of the few things on a page whose lines are MEANT not to line up, so the
/// rule has to see the whole region the cut handed over (codex, round 1).
///
/// AND NOT LESS THAN THE COLUMN THE REGIONS STACK INTO (#589). The XY-cut
/// splits a column at every band of white a line tall, and a list whose items
/// are paragraphs is set with exactly that band between them: `1.` in one
/// region, `2.` in the next, each a singleton that no run can confirm. Read on
/// the document, that shape was 12 of the 16 list items of one annual report
/// and none of them were found. So consecutive regions that stand one under
/// the other in the same column are read as one: the regions are consecutive
/// slices of `metrics`, which makes the stack a slice too.
fn mark_list_items(metrics: &mut [LineMetrics], regions: &[(usize, usize)]) {
    let foot = in_the_foot(metrics);
    for (start, end) in column_stacks(metrics, regions) {
        mark_column_items(&mut metrics[start..end], &foot[start..end]);
    }
}

/// Letters a line needs before it counts as running text rather than a page
/// number, a folio or a reference code.
const RUNNING_TEXT_LETTERS: usize = 20;

/// The share of a page's text height, from the bottom, that a foot may take and
/// that the page's body is therefore not measured in.
const FOOT_SHARE: f64 = 0.4;

/// The size most of these lines' characters are set in.
fn body_size_of<'a>(lines: impl Iterator<Item = &'a LineMetrics>) -> f64 {
    let mut sizes: Vec<f64> = Vec::new();
    for line in lines {
        let chars = line.text.chars().filter(|c| !c.is_whitespace()).count();
        if line.font_size > 0.0 {
            sizes.extend(std::iter::repeat_n(line.font_size, chars.max(1)));
        }
    }
    sizes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    sizes.get(sizes.len() / 2).copied().unwrap_or(0.0)
}

/// The lines of a page's foot: set smaller than the page's body and below the
/// last line that is set in it.
///
/// A FOOTNOTE IS NOT AN ITEM (#589). Legislation marks its footnotes `(a)`,
/// `(b)`, `(c)` -- a run that climbs like any list's, in a region of its own,
/// where the small type IS the body the region is measured against. Only the
/// page knows the type is small: measured against the region, a block of
/// footnotes is a list of body-size items. A list of references is not caught
/// here: where a page is all small type, that small type is its body.
fn in_the_foot(metrics: &[LineMetrics]) -> Vec<bool> {
    // THE BODY IS MEASURED ABOVE THE FOOT. Near the end of a section the
    // footnotes can hold more characters than the text above them, and a
    // median over the whole page then calls the footnote size the body --
    // after which nothing is smaller than it (codex, #1950). The upper part
    // of the page is where the foot is not.
    let low = metrics
        .iter()
        .map(|l| l.bottom)
        .fold(f64::INFINITY, f64::min);
    let high = metrics
        .iter()
        .map(|l| l.top)
        .fold(f64::NEG_INFINITY, f64::max);
    let upper: Vec<usize> = (0..metrics.len())
        .filter(|i| metrics[*i].bottom >= low + FOOT_SHARE * (high - low))
        .collect();
    let page_body = if upper.is_empty() {
        column_body(metrics).0
    } else {
        body_size_of(upper.iter().map(|i| &metrics[*i]))
    };
    if page_body <= 0.0 {
        return vec![false; metrics.len()];
    }
    // RUNNING TEXT, not any line in body size: legislation prints its page
    // number in body size under the footnotes, and measured from that line the
    // foot was empty on every page of the order (#589).
    let last_body_line = metrics
        .iter()
        .filter(|l| {
            l.font_size >= page_body * (1.0 - SIZE_TOLERANCE)
                && l.text.chars().filter(|c| c.is_alphabetic()).count() >= RUNNING_TEXT_LETTERS
        })
        .map(|l| l.bottom)
        .fold(f64::INFINITY, f64::min);
    // A HEADING BETWEEN THE BODY AND THE SMALL TYPE opens a section, and a
    // section is not a foot: a bibliography that starts low on a page is set
    // smaller than the body and under it, exactly like footnotes, and what
    // tells them apart is the `References` above it. Nothing opens a block of
    // footnotes with a heading (codex, #1950).
    let heads: Vec<(f64, f64)> = metrics
        .iter()
        // A head says something. Legislation draws the rule over its
        // footnotes as a line of blank glyphs in a larger size -- it reads
        // as larger type and nothing in it -- and counted as a head it opened
        // a section over every footnote under it (uksi on the rig, #574). A
        // head of figures, `2024`, says something too (codex, #2041).
        .filter(|l| {
            let letters = l.text.chars().filter(|c| c.is_alphabetic()).count();
            l.top <= last_body_line
                && l.marker.is_none()
                && (l.bold || l.font_size > page_body * (1.0 + SIZE_TOLERANCE))
                && l.text.chars().any(char::is_alphanumeric)
                && letters < RUNNING_TEXT_LETTERS
        })
        .map(|l| (l.top, l.bottom))
        .collect();
    metrics
        .iter()
        .map(|l| {
            l.font_size < page_body * (1.0 - SIZE_TOLERANCE)
                && l.top <= last_body_line
                && !heads.iter().any(|(_, bottom)| *bottom >= l.top)
        })
        .collect()
}

/// How much white, in lines of the type, may stand between two regions of one
/// stack. A blank line between items is one; a list further down the column,
/// past a paragraph and its spacing, is a list of its own and must not be
/// nested under this one by its indent (codex, #1950).
const STACK_GAP_LINES: f64 = 3.0;

/// Consecutive regions that stand one under the other in the same column,
/// merged into one span of lines.
///
/// Same column means the horizontal extents overlap by at least half the
/// narrower of the two, so a side-by-side pair -- two columns, a sidebar
/// beside its text -- never joins. Under means the next region starts below
/// where the stack ends, so a region the cut ordered back up the page (the
/// top of the next column) starts a stack of its own.
fn column_stacks(metrics: &[LineMetrics], regions: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let extent = |start: usize, end: usize| {
        let lines = &metrics[start..end];
        let left = lines.iter().map(|l| l.left).fold(f64::INFINITY, f64::min);
        let right = lines
            .iter()
            .map(|l| l.right)
            .fold(f64::NEG_INFINITY, f64::max);
        let top = lines
            .iter()
            .map(|l| l.top)
            .fold(f64::NEG_INFINITY, f64::max);
        let bottom = lines.iter().map(|l| l.bottom).fold(f64::INFINITY, f64::min);
        (left, right, top, bottom)
    };
    let mut stacks: Vec<(usize, usize)> = Vec::new();
    let mut last: Option<(f64, f64, f64, f64)> = None;
    for &(start, end) in regions {
        if start >= end {
            continue;
        }
        let here = extent(start, end);
        let joins = match (stacks.last(), last) {
            (Some(&(_, stack_end)), Some((left, right, _, bottom))) if stack_end == start => {
                let overlap = right.min(here.1) - left.max(here.0);
                let narrower = (right - left).min(here.1 - here.0);
                let size = metrics[start..end]
                    .iter()
                    .map(|l| l.font_size)
                    .fold(0.0_f64, f64::max);
                narrower > 0.0
                    && overlap >= 0.5 * narrower
                    && here.2 <= bottom + 1.0
                    && bottom - here.2 <= STACK_GAP_LINES * size.max(1.0)
            }
            _ => false,
        };
        if joins {
            if let (Some(stack), Some(prev)) = (stacks.last_mut(), last) {
                stack.1 = end;
                last = Some((
                    prev.0.min(here.0),
                    prev.1.max(here.1),
                    prev.2,
                    here.3.min(prev.3),
                ));
            }
        } else {
            stacks.push((start, end));
            last = Some(here);
        }
    }
    stacks
}

/// The same, for one region.
fn mark_column_items(column: &mut [LineMetrics], foot: &[bool]) {
    let (body_size, body_bold) = column_body(column);

    // A CANDIDATE IS SET IN THE BODY -- WHEN IT IS NUMBERED. A numbered
    // heading is a run of agreeing markers at one left edge and is not a list;
    // what separates it is that the page sets it above its body, which is the
    // same evidence `crate::headings` reads and the reason the two never claim
    // the same block. Smaller type is admitted -- a list of footnotes is a
    // list -- and larger is not. A bullet is neither a heading nor a footnote
    // mark, so the weight and the foot are not asked of it: a bold bullet run
    // in regular prose, and a small-type bullet list under the body, are lists
    // the page set apart the way lists are set apart.
    let candidates: Vec<usize> = (0..column.len())
        .filter(|i| {
            let line = &column[*i];
            let numbered = line.marker.is_some_and(|marker| marker.is_ordered());
            line.marker.is_some()
                && (!numbered || !foot[*i])
                && (!line.tabular || hangs_from_its_marker(line))
                && line.font_size > 0.0
                && line.font_size <= body_size * (1.0 + SIZE_TOLERANCE)
                && (!numbered || line.bold == body_bold)
        })
        .collect();
    if candidates.len() < 2 {
        return;
    }

    // Grouped by left edge, because two levels of one list are two sequences
    // and their markers interleave on the page.
    let mut by_edge: Vec<Vec<usize>> = Vec::new();
    let mut sorted = candidates.clone();
    sorted.sort_by(|a, b| {
        column[*a]
            .indent_left
            .partial_cmp(&column[*b].indent_left)
            .unwrap_or(Ordering::Equal)
    });
    for i in sorted {
        let edge = column[i].indent_left;
        match by_edge.last_mut() {
            Some(group)
                if edge - column[group[0]].indent_left
                    <= INDENT_MATCH_EM * column[i].font_size.max(1.0) =>
            {
                group.push(i)
            }
            _ => by_edge.push(vec![i]),
        }
    }

    // Lifted out of `column` before anything is written back to it: every read
    // below is of a marker, and every write is of an item.
    let markers: Vec<Option<Marker>> = column.iter().map(|line| line.marker).collect();
    let marker_of = |i: usize| markers[i].expect("a candidate carries its marker");

    // Each edge splits again into maximal SEQUENCES, before anything is
    // confirmed: a page with two numbered lists on it restarts at 1, and one
    // sequence over the whole column would see the fall and throw both lists
    // away. The sequence is also what reads a bare letter -- `i. ii. iii.` is
    // roman and `g. h. i.` is not, and the marker `i.` is 1 in the first and 9
    // in the second.
    // A LABEL IS NOT AN ITEM (#589). Notes to a set of accounts number their
    // sub-heads -- `(a) Accounts Payable`, then a paragraph, `(b) Acquisition
    // of Assets`, then a paragraph -- in body type, and the markers climb like
    // any list's. What the page says is that each of them is one short line
    // with a paragraph under it: the text after the marker stops well short of
    // the measure, so it did not run on, and what follows is not the next
    // marker. Where that paragraph starts -- back at the marker's edge or
    // hanging under the label's text -- the notes of one annual report do both.
    // A list has at most its LAST item in that position; a sequence in which
    // EVERY member is a label is a run of numbered sub-heads, and is left for
    // the heading pass. On that annual report the shape was 40 of the 96 items
    // we produced and none of the 82 the annotation drew.
    //
    // Short means a title's length, not a line that fell a word shy of the
    // measure: legislation sets `(i) the height of the highest part of the
    // roof of the original building,` and hangs the rest of the clause under
    // it, and that item is also one line with a non-marker line under it. What
    // tells the two apart is that the clause runs on -- it ends in a comma, a
    // dash, `and`, `or` -- and fills most of the measure, while a label is a
    // few words that end where they end.
    let left = column.iter().map(|l| l.left).fold(f64::INFINITY, f64::min);
    let measure = column
        .iter()
        .map(|l| l.right)
        .fold(f64::NEG_INFINITY, f64::max);
    let labels: Vec<bool> = (0..column.len())
        .map(|i| {
            let line = &column[i];
            let short = line.right - left < LABEL_SHARE * (measure - left);
            // Numbered only. A bulleted item with a short first line and its
            // description under it is the commonest list there is, and a
            // bullet carries no number for a sub-head to be counted by
            // (codex, #1950).
            let next = column.get(i + 1);
            // AN OUTLINE'S SUB-HEAD MAY BE LONG (#599). `II.4.11 Conformity of
            // the supplies delivered with the contract` fills most of the
            // measure and is still a title: it ends no sentence, and the line
            // under it did not wrap out of it. A clause does one or the other.
            let title = line.marker.is_some_and(|m| m.is_outline())
                && !line.ends_sentence()
                && next.is_some_and(|next| !wrapped_before(line, next, measure));
            line.marker.is_some_and(|m| m.is_ordered())
                && (short || title)
                && !runs_on(&line.text)
                && next.is_none_or(|next| next.marker.is_none())
        })
        .collect();

    let mut confirmed: Vec<(usize, Option<u32>)> = Vec::new();
    let take = |sequence: &[usize], confirmed: &mut Vec<(usize, Option<u32>)>| {
        if sequence.len() < 2 || sequence.iter().all(|i| labels[*i]) {
            return;
        }
        let run: Vec<Marker> = sequence.iter().copied().map(marker_of).collect();
        let roman = lists::is_roman(&run);
        confirmed.extend(
            sequence
                .iter()
                .zip(&run)
                // An outline's `II.4.1` is no one number, and stays in the
                // item's text rather than being written as one.
                .map(|(i, marker)| (*i, marker.value(roman).filter(|_| !marker.keeps_its_text()))),
        );
    };
    for group in &mut by_edge {
        group.sort_unstable();
        let mut sequence: Vec<usize> = Vec::new();
        for i in group.iter().copied() {
            // A NUMBERED SUB-HEAD IS NOT A CLAUSE (#599). `2.1 Materials` over
            // its paragraph is the label shape, and an outline's numbers are
            // what both a contract's clauses and a paper's sections are
            // numbered with: the clauses run on, the sub-heads stay short, and
            // a sub-head among clauses is left where it was rather than
            // counted into their run.
            if labels[i] && marker_of(i).is_outline() {
                continue;
            }
            let mut extended: Vec<Marker> = sequence.iter().copied().map(marker_of).collect();
            extended.push(marker_of(i));
            if sequence.is_empty() || lists::is_a_run(&extended) {
                sequence.push(i);
            } else {
                take(&sequence, &mut confirmed);
                sequence = vec![i];
            }
        }
        take(&sequence, &mut confirmed);
    }
    if confirmed.is_empty() {
        return;
    }

    // The levels: the region's own indent steps, ranked. Level 0 is the
    // leftmost edge that carries an item, whatever that edge measures.
    let tolerance = INDENT_MATCH_EM * body_size.max(1.0);
    let mut edges: Vec<f64> = confirmed
        .iter()
        .map(|(i, _)| column[*i].indent_left)
        .collect();
    edges.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let mut steps: Vec<f64> = Vec::new();
    for edge in edges {
        if steps.last().is_none_or(|last| edge - last > tolerance) {
            steps.push(edge);
        }
    }

    for (i, ordinal) in confirmed {
        // The NEAREST step, not the last one this edge clears: an edge that
        // lands between two steps belongs to the one it is closer to, and a
        // walk that takes the last it clears hands it the deeper level every
        // time.
        let edge = column[i].indent_left;
        let level = steps
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                (edge - *a)
                    .abs()
                    .partial_cmp(&(edge - *b).abs())
                    .unwrap_or(Ordering::Equal)
            })
            .map_or(0, |(rank, _)| rank)
            .min(lists::MAX_LIST_LEVEL as usize) as u8;
        let marker = marker_of(i);
        column[i].item = Some(ItemMark {
            len: if marker.keeps_its_text() {
                0
            } else {
                marker.len
            },
            level,
            ordinal,
            text_left: column[i].marker_text_left,
        });
    }
}

/// Whether the one gutter a tabular line has is the white between its marker
/// and its text (#1973).
///
/// A list hung from its markers at one x -- a statute's contents, `§ 70` and
/// then the title a hand's width to the right -- has white running down the
/// page between the markers and the text, which is what `mark_tabular` calls a
/// gutter. That white is the list's hanging indent. A line whose ONE wide gap
/// ends where the text after its marker begins is a candidate like any other;
/// a second gap is a table's next column, and the line stays out.
fn hangs_from_its_marker(line: &LineMetrics) -> bool {
    let (Some(text_left), [(_, gap_end)]) = (line.marker_text_left, line.wide_gaps.as_slice())
    else {
        return false;
    };
    (gap_end - text_left).abs() <= INDENT_MATCH_EM * line.font_size.max(1.0)
}

/// How much of the column's measure a numbered sub-head may fill and still be
/// read as a label rather than the first line of an item (#589).
const LABEL_SHARE: f64 = 0.6;

/// Whether a line's text stops mid-sentence: a clause that continues on the
/// next line, which a label never does.
/// Whether `previous` was wrapped: the room it left before the column's measure
/// is too small for the first word of `line` and the space before it, so the
/// typesetter had to carry that word down (#599). The measure is the COLUMN's,
/// the longest line standing over it: taken from the item and the candidate alone,
/// a short paragraph after `2. Pears` would set it and pass for the room that
/// was left (codex, #1981). `(i) Implementation of
/// training so that report and medical accident` stops three ems short
/// because `investigation` is six wide; `2. Pears` stops with room for a
/// sentence and is finished (codex, #1950). The word's width is taken at the
/// line's own average advance, which is what a count of characters can say.
fn wrapped_before(previous: &LineMetrics, line: &LineMetrics, measure: f64) -> bool {
    let chars = previous.text.chars().filter(|c| !c.is_whitespace()).count();
    let width = previous.right - previous.indent_left;
    if chars == 0 || width <= 0.0 {
        return false;
    }
    let advance = width / previous.text.trim().chars().count().max(1) as f64;
    let Some(word) = line.text.split_whitespace().next() else {
        return false;
    };
    let needed = (word.chars().count() + 1) as f64 * advance;
    let room = measure - previous.right;
    room >= 0.0 && room < needed
}

fn runs_on(text: &str) -> bool {
    let text = text.trim_end();
    text.ends_with([',', ';', ':', '-', '\u{2013}', '\u{2014}'])
        || text
            .rsplit(char::is_whitespace)
            .next()
            .is_some_and(|last| matches!(last, "and" | "or" | "of" | "the" | "to"))
}

/// What a region sets its body in: the size most of its characters are at, and
/// whether most of them are bold.
fn column_body(column: &[LineMetrics]) -> (f64, bool) {
    let mut sizes: Vec<f64> = Vec::new();
    let mut heavy = 0usize;
    let mut total = 0usize;
    for line in column {
        let chars = line.text.chars().filter(|c| !c.is_whitespace()).count();
        total += chars;
        if line.bold {
            heavy += chars;
        }
        if line.font_size > 0.0 {
            sizes.extend(std::iter::repeat_n(line.font_size, chars.max(1)));
        }
    }
    sizes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let size = sizes.get(sizes.len() / 2).copied().unwrap_or(0.0);
    (size, total > 0 && heavy * 2 > total)
}

/// Where one column of lines ends and the next begins.
///
/// The XY-cut hands its regions over in reading order, so lines of one column
/// are consecutive. What marks the seam is that the next line does not descend
/// -- it is back at the top of the page -- or that it barely lies under the one
/// before it.
fn column_runs(metrics: &[LineMetrics]) -> Vec<(usize, usize)> {
    let mut runs = Vec::new();
    let mut start = 0usize;
    for i in 1..metrics.len() {
        let previous = &metrics[i - 1];
        let current = &metrics[i];
        let descends = previous.baseline - current.baseline
            > MIN_BASELINE_STEP_EM * current.font_size.max(previous.font_size);
        if !descends || overlap_fraction(previous, current) < COLUMN_OVERLAP_FRACTION {
            runs.push((start, i));
            start = i;
        }
    }
    if start < metrics.len() {
        runs.push((start, metrics.len()));
    }
    runs
}

/// The left edge most of a column's lines are set to.
///
/// The tolerance comes from the column's MIDDLE type size and not from its
/// first line's: a column that opens with a heading would otherwise measure its
/// margin with the heading's em, and on a 24-point heading over 9-point body a
/// first-line indent of one em falls inside the tolerance and stops being an
/// indent at all.
fn body_left(metrics: &[LineMetrics]) -> f64 {
    let mut sizes: Vec<f64> = metrics.iter().map(|m| m.font_size).collect();
    sizes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let tolerance = INDENT_MATCH_EM * sizes[sizes.len() / 2].max(1.0);

    let mut lefts: Vec<f64> = metrics.iter().map(|m| m.indent_left).collect();
    lefts.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let mut best = lefts.first().copied().unwrap_or(0.0);
    let mut best_count = 0usize;
    for (i, left) in lefts.iter().enumerate() {
        let count = lefts[i..]
            .iter()
            .take_while(|l| **l - left <= tolerance)
            .count();
        if count > best_count {
            best_count = count;
            best = *left;
        }
    }
    best
}

/// Which way the page's indents point.
///
/// Told from what sits above each indented line, which is the only place the
/// two styles differ: the line above a first-line indent is the last line of a
/// paragraph and falls short of the measure; the line above a hanging
/// continuation is the entry's own first line and runs the full measure.
///
/// ONE VOTE FOR THE PAGE, counted over every column on it, because the style is
/// a decision the typesetter made once for the document and the evidence for it
/// is thin in any one column. A column of six lines holding one indent decides
/// on that indent alone, and on `tracemonkey` -- which is set first-line
/// indented throughout -- the short columns beside its figures came out
/// `Hanging` and read every paragraph opening as the end of the one above.
/// The MARGIN stays each column's own: two columns of a page sit at two left
/// edges, and an indent is a step away from the edge the column is set to.
fn indent_style(metrics: &[LineMetrics], runs: &[(usize, usize, f64)]) -> IndentStyle {
    let mut first_line = 0usize;
    let mut hanging = 0usize;
    for (start, end, margin) in runs {
        let measure = metrics[*start..*end]
            .iter()
            .map(|m| m.right)
            .fold(f64::NEG_INFINITY, f64::max);
        for i in *start + 1..*end {
            let line = &metrics[i];
            if line.font_size <= 0.0 {
                continue;
            }
            if line.indent_left - margin <= INDENT_MIN_EM * line.font_size {
                continue;
            }
            let above = &metrics[i - 1];
            if measure - above.right > SHORT_LINE_EM * above.font_size.max(line.font_size) {
                first_line += 1;
            } else {
                hanging += 1;
            }
        }
    }
    if first_line == 0 && hanging == 0 {
        IndentStyle::Flat
    } else if first_line >= hanging {
        IndentStyle::FirstLine
    } else {
        IndentStyle::Hanging
    }
}

/// The entries of a list that marks them with nothing but its hang (#574).
///
/// A REFERENCE LIST SET AUTHOR-YEAR -- `Diehl, M., 2003, Phys. Rept. 388, 41.`
/// -- has no bullet and no number: each entry opens at the column's margin and
/// its continuation lines hang an indent in from it, which is how the hanging
/// indent above already keeps an entry together. What it did not do is call
/// the entries items, and DocLayNet marks every one of them a list item: on
/// the validation proxy of #632 they were 175 of the 284 unmarked items we
/// read as paragraphs.
///
/// AN ENTRY is a group of at least two lines whose first line stands at the
/// margin and whose other lines all hang at one indent from it. ONE such group
/// is a paragraph set with a hanging indent, and two can be a pair of
/// definitions; THREE in a row is the list, and a one-line entry between them
/// -- a reference short enough to fit its line -- belongs to it as long as the
/// hung entries are at least half of the run, which opens and closes on a
/// hung entry.
fn entries_of_a_list(metrics: &[LineMetrics], groups: &[Vec<usize>], margin: f64) -> Vec<bool> {
    let hung = |group: &Vec<usize>| -> bool {
        let first = &metrics[group[0]];
        let em = first.font_size;
        if group.len() < 2 || em <= 0.0 || first.indent_left - margin > INDENT_MIN_EM * em {
            return false;
        }
        let hang = metrics[group[1]].indent_left;
        hang - first.indent_left > INDENT_MIN_EM * em
            && group[1..]
                .iter()
                .all(|i| (metrics[*i].indent_left - hang).abs() <= INDENT_MATCH_EM * em)
    };
    let one_line_at_the_margin = |group: &Vec<usize>| -> bool {
        let first = &metrics[group[0]];
        group.len() == 1 && first.indent_left - margin <= INDENT_MIN_EM * first.font_size
    };
    let mut out = vec![false; groups.len()];
    let mut start = 0;
    while start < groups.len() {
        let mut end = start;
        let mut hung_count = 0;
        while end < groups.len() && (hung(&groups[end]) || one_line_at_the_margin(&groups[end])) {
            hung_count += usize::from(hung(&groups[end]));
            end += 1;
        }
        // Trimmed to its first and last hung entry: the one line at the margin
        // above a reference list is its heading, and the one below it the
        // next paragraph, not an entry.
        let (mut first, mut last) = (start, end);
        while first < last && !hung(&groups[first]) {
            first += 1;
        }
        while last > first && !hung(&groups[last - 1]) {
            last -= 1;
        }
        if hung_count >= 3 && 2 * hung_count >= last - first {
            out[first..last].fill(true);
        }
        start = end.max(start + 1);
    }
    out
}

/// A ruling line the document drew between two lines of text.
fn rule_between(above: &LineMetrics, below: &LineMetrics, rules: &[HorizontalRule]) -> bool {
    rule_between_boxes(
        [above.left, above.bottom, above.right, above.top],
        [below.left, below.bottom, below.right, below.top],
        rules,
    )
}

/// The same, on bare boxes in `[left, bottom, right, top]` order: the
/// narrow-column pair join reads lines the builder never measured
/// ([`JoinLine`]), and "between" is one definition.
pub(crate) fn rule_between_boxes(
    above: [f64; 4],
    below: [f64; 4],
    rules: &[HorizontalRule],
) -> bool {
    let left = above[0].min(below[0]);
    let right = above[2].max(below[2]);
    rules.iter().any(|rule| {
        rule.y >= below[3] && rule.y <= above[1] && rule.left < right && rule.right > left
    })
}

/// The leading a column is set on: the middle of its own baseline steps.
///
/// Measured per column and not per page. `PageStats` takes the tightest
/// quartile of the gaps between the page's BANDS, and a band is a stripe across
/// the whole page: two columns whose baselines sit a few points apart produce a
/// band for each of them, so the page's figure on a two-column paper is the
/// offset between the columns rather than the leading of either. Every pair on
/// `tracemonkey` then read as a paragraph break, and the page came back one
/// block per line.
///
/// Steps that descend by more than four lines are left out of the middle: a
/// column ends with the gap above a footnote or a figure, and the median is
/// taken of what the type is set on, not of the page's furniture.
fn run_leading(metrics: &[LineMetrics], page_leading: f64) -> f64 {
    let em = metrics
        .iter()
        .map(|m| m.font_size)
        .fold(0.0_f64, f64::max)
        .max(1.0);
    let mut steps: Vec<f64> = metrics
        .windows(2)
        .map(|pair| pair[0].baseline - pair[1].baseline)
        .filter(|step| *step > 0.0 && *step < 4.0 * em)
        .collect();
    if steps.is_empty() {
        return if page_leading > 0.0 {
            page_leading
        } else {
            // The leading of solid setting, for a column that gave no second
            // line to measure one from.
            SOLID_PITCH_EM * em
        };
    }
    steps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    steps[steps.len() / 2]
}

/// The leading in force inside a paragraph: its own, once it has two lines to
/// measure, and the column's until then.
fn leading_of(metrics: &[LineMetrics], current: &[usize], run_leading: f64) -> f64 {
    if current.len() >= 2 {
        let mut steps: Vec<f64> = current
            .windows(2)
            .map(|pair| metrics[pair[0]].baseline - metrics[pair[1]].baseline)
            .collect();
        steps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        return steps[steps.len() / 2];
    }
    run_leading
}

/// Whether the merge says out loud which rail stopped each pair it refused.
///
/// Read once: this is asked on every rejected pair of every page, and an
/// environment lookup per pair is a cost paid by every caller for a switch
/// almost nobody turns on.
fn tracing() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("PDFLUENT_BLOCK_TRACE").is_some())
}

/// Whether the line at `next` continues the paragraph `current` is building.
#[allow(clippy::too_many_arguments)]
/// Whether `line` starts under the text of the item `current` opens -- its
/// hang. Where the marker has a column of its own the hang is that column,
/// the edge the continuation rail in [`continues`] measures too; where the page drew it inside the first span there is no such
/// column, and the hang is any clear indent past the marker's edge, as the
/// width rail in [`continues`] reads it. Either way the next item starts at
/// the marker and the prose after the list at the marker's edge, so neither
/// starts here.
fn under_the_hang(metrics: &[LineMetrics], current: &[usize], line: &LineMetrics, em: f64) -> bool {
    let first = &metrics[current[0]];
    match first.item.map(|item| item.text_left) {
        Some(Some(text_left)) => {
            text_left - first.indent_left > INDENT_MIN_EM * em
                && line.indent_left >= text_left - INDENT_MATCH_EM * em
        }
        Some(None) => line.indent_left - first.indent_left > INDENT_MIN_EM * em,
        None => false,
    }
}

/// Whether `step` is no wider than the list's own pitch: from this item down
/// to the next one, or -- for the last item, which has none -- from the line
/// above it down to it. Double-spaced references step two lines from one to
/// the next and one inside each, so a continuation is never further below its
/// item than the next item is. After a list set solid the next paragraph sits
/// a paragraph's white below the last item -- wider than the list's one-line
/// pitch -- and stays a paragraph even where it is set at the list's indent.
/// The next item is looked for within [`HANG_LOOKAHEAD_LINES`] lines, which is
/// as far as an item that runs to three lines and its hang reach.
fn within_the_lists_pitch(metrics: &[LineMetrics], current: &[usize], step: f64, em: f64) -> bool {
    let first = current[0];
    let top = metrics[first].baseline;
    let next_item = metrics
        .iter()
        .skip(first + 1)
        .take(HANG_LOOKAHEAD_LINES)
        .find(|m| m.item.is_some() && m.baseline < top);
    let pitch = match next_item {
        Some(next) => top - next.baseline,
        None => match first.checked_sub(1) {
            Some(before) => metrics[before].baseline - top,
            None => return true,
        },
    };
    pitch <= 0.0 || step <= pitch + SIZE_TOLERANCE * em
}

/// How many lines below an item [`within_the_lists_pitch`] looks for the next.
const HANG_LOOKAHEAD_LINES: usize = 6;

/// The column run a merge is asked within: the left margin it is set to, and
/// the right edge the longest line standing over it reaches.
#[derive(Debug, Clone, Copy)]
struct Column {
    margin: f64,
    right: f64,
}

fn continues(
    metrics: &[LineMetrics],
    current: &[usize],
    next: usize,
    style: IndentStyle,
    column: Column,
    rules: &[HorizontalRule],
    leading: f64,
) -> bool {
    let margin = column.margin;
    let previous = &metrics[*current.last().expect("a paragraph holds a line")];
    let line = &metrics[next];

    // Every refusal below goes out under `PDFLUENT_BLOCK_TRACE`, naming the pair
    // and the rail that stopped it. A merge that declines is invisible in its
    // output -- what comes back is a page of short blocks, and which of eight
    // rails produced each one is exactly what a reader of that page wants and
    // cannot get. Three of the corpus findings here were found with it on.
    macro_rules! no {
        ($reason:expr) => {{
            if tracing() {
                eprintln!("BREAK {} | {:?} -> {:?}", $reason, previous.text, line.text);
            }
            return false;
        }};
    }
    // A row of cells is not a line of prose, whichever side of the pair it is.
    if previous.tabular || line.tabular {
        no!("tabular");
    }

    // A CONFIRMED LIST ITEM STARTS ITS OWN BLOCK (#569). Without this the
    // commonest list of all -- items of one line each, set solid at one left
    // edge -- differs from a paragraph in nothing the rails below measure, and
    // comes back as a single block with every bullet inside it. The marker is
    // not believed on its own: `mark_list_items` has already required a second
    // item in the same column to agree with this one, which is what keeps a
    // line of dialogue opening with an em dash from cutting a paragraph in two.
    if line.item.is_some() {
        no!("list item");
    }

    let em = previous.font_size.max(line.font_size);
    if em <= 0.0 {
        no!("no type size");
    }

    let step = previous.baseline - line.baseline;
    if step <= MIN_BASELINE_STEP_EM * em {
        no!("same line");
    }
    // UNDER AN ITEM'S HANG THE LIST'S OWN PITCH SPEAKS FOR IT, not the page's,
    // on both of the rails below: the column's measured leading (double-spaced
    // references in a column otherwise set solid, codex #1967) and the white
    // a line of prose never has (references in a paper set double-spaced).
    let hung = step <= HANG_MAX_LEADING_EM * em
        && under_the_hang(metrics, current, line, em)
        && within_the_lists_pitch(metrics, current, step, em);
    if step > leading_of(metrics, current, leading) * PARAGRAPH_LEADING_TOLERANCE && !hung {
        no!(format!(
            "leading {step:.2} over {:.2}",
            leading_of(metrics, current, leading)
        ));
    }
    if step > MAX_LEADING_EM * em && !hung {
        no!(format!("a line of white: {step:.2} on {em:.2} of type"));
    }

    if (line.font_size - previous.font_size).abs() > SIZE_TOLERANCE * em {
        no!(format!(
            "type size {:.3} vs {:.3}",
            previous.font_size, line.font_size
        ));
    }

    // A CHANGE OF WEIGHT IS A CHANGE OF TYPE, and it is the only one a document
    // set in a single size has left: a bold heading over a paragraph set solid
    // with it differs from the paragraph in nothing a measurement of geometry
    // can see, and merged into it there is no block for the classifier to call
    // a heading (#568). Line against line and by the characters, so a bold
    // phrase inside a line of prose does not end its paragraph and a document
    // set entirely in a heavy face never splits at all.
    if line.bold != previous.bold {
        no!(format!(
            "weight {} vs {}",
            if previous.bold { "bold" } else { "regular" },
            if line.bold { "bold" } else { "regular" }
        ));
    }

    if overlap_fraction(previous, line) < COLUMN_OVERLAP_FRACTION {
        no!("column");
    }

    if rule_between(previous, line, rules) {
        no!("rule");
    }

    // A line that ends a sentence well short of the measure ends the paragraph.
    // The measure is taken from the paragraph itself rather than from the
    // column, so a paragraph set narrow inside a wider column is judged against
    // its own right edge.
    let measure = current
        .iter()
        .map(|i| metrics[*i].right)
        .chain(std::iter::once(line.right))
        .fold(f64::NEG_INFINITY, f64::max);
    if previous.ends_sentence() && measure - previous.right > SHORT_LINE_EM * em {
        no!("short line");
    }

    // AN ITEM CARRIES ITS OWN HANG, and the page's paragraph shape does not
    // reach inside it. The item's continuation lines start at its TEXT, and a
    // line that starts at its MARKER is under nothing -- so it is the prose
    // after the list, whatever the item's first line measured.
    //
    // A line under the hang returns HERE rather than falling through to the
    // style below, because on a first-line-indented page that rail reads the
    // hang as an indent and starts a paragraph on it -- which splits every
    // conventionally hanging continuation out of its item (codex, round 2).
    // The hang is the list's own statement and it outranks the page's.
    if let Some(item) = metrics[current[0]].item {
        let marker_left = metrics[current[0]].indent_left;
        match item.text_left {
            Some(text_left) => {
                if line.indent_left >= text_left - INDENT_MATCH_EM * em {
                    return true;
                }
                // A NUMBER MAY SIT IN A FIRST-LINE INDENT. On a page set that
                // way the marker is inside the indent and the item's own body
                // returns to the margin, so its second line is further LEFT
                // than its first. The prose after such a list opens with the
                // same indent the marker sat in, which is not this.
                if !(style == IndentStyle::FirstLine
                    && line.indent_left < marker_left - INDENT_MIN_EM * em)
                {
                    no!("the line is back at the item's marker edge");
                }
            }
            // The page drew the marker inside the line's own first span, so
            // there is no column to measure. Fall back on the width: an item
            // that ran the full measure may be wrapping, one that stopped short
            // is finished. Asked only while the item is still one line, because
            // from the second line on the outdent rail below measures the same
            // hang.
            None if current.len() == 1 => {
                if line.indent_left - marker_left <= INDENT_MIN_EM * em
                    && measure - previous.right > SHORT_LINE_EM * em
                {
                    no!("the item ended short of the measure");
                }
                // A line that hangs under the item is the item's, as it is
                // where the marker has a column of its own. Left to the page's
                // rail below, a first-line-indented page reads the hang as an
                // indent and cuts the clause after its first line (#589).
                //
                // Only when the item has more to say: its first line ran to
                // the measure or stops mid-clause. `2. Pears` is finished, and
                // an indented line under it is the page's next paragraph
                // (codex, #1950).
                if line.indent_left - marker_left > INDENT_MIN_EM * em
                    && (measure - previous.right <= SHORT_LINE_EM * em
                        || runs_on(&previous.text)
                        || wrapped_before(previous, line, column.right))
                {
                    return true;
                }
            }
            // AND ONCE THE SECOND LINE HAS SET THE HANG, a line at that hang
            // is the item's too (#599). The width rail above is asked only of
            // the first pair; from there a first-line-indented page's own rail
            // read every further line at the hang as an indent and cut the
            // clause into one paragraph per line (`(i) Implementation of
            // training ...`, a Japanese ordinance in DocLayNet's validation
            // split). A finished line still ends the item: the short-line rail
            // is asked before this.
            None => {
                let hang = metrics[current[1]].indent_left;
                if hang - marker_left > INDENT_MIN_EM * em
                    && (line.indent_left - hang).abs() <= INDENT_MATCH_EM * em
                {
                    return true;
                }
            }
        }
    }

    match style {
        // An indented line starts a paragraph, wherever the paragraph so far
        // happens to sit.
        IndentStyle::FirstLine => {
            if line.indent_left - margin > INDENT_MIN_EM * em {
                no!("first-line indent");
            }
            true
        }
        // A line that goes back to the margin after a hang is the next entry.
        IndentStyle::Hanging | IndentStyle::Flat => {
            let body = if current.len() >= 2 {
                current[1..]
                    .iter()
                    .map(|i| metrics[*i].indent_left)
                    .fold(f64::INFINITY, f64::min)
            } else {
                previous.indent_left
            };
            if line.indent_left - body < -INDENT_MIN_EM * em {
                no!("outdent");
            }
            true
        }
    }
}

/// Join a paragraph's lines into the text it reads as: a space where the
/// typesetter put a line break, and nothing at all where it broke a word.
pub(crate) fn join_lines(lines: &[String]) -> String {
    let mut out = String::new();
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if out.is_empty() {
            out.push_str(trimmed);
            continue;
        }
        if is_hyphen_wrap_candidate(&out, trimmed) {
            out.pop();
            out.push_str(trimmed);
        } else {
            out.push(' ');
            out.push_str(trimmed);
        }
    }
    out
}

/// One block out of the lines that were merged into it.
///
/// `item` is the confirmed list marker the block's FIRST line opens with, when
/// it opens with one (#569): it decides the kind, and it comes off the front of
/// the text. The span that drew it stays in `spans`, which say what the page
/// drew and not what it reads as.
fn assemble(
    page: usize,
    index: usize,
    lines: &[&TextBlock],
    metrics: &[&LineMetrics],
    item: Option<ItemMark>,
) -> Block {
    let mut spans: Vec<TextSpan> = Vec::new();
    let mut texts: Vec<String> = Vec::new();

    // An initial that belongs to a LATER line of the paragraph was set at the
    // front of it and drawn three lines down; the text says so even though the
    // geometry does not.
    //
    // Only where the line reads with the initial in front of it, which is the
    // one case the move can be undone in the text as well as in the spans: the
    // line's own text is assembled and reordered by `TextBlock::text`, and a
    // prefix that is not there is an initial that would be written twice --
    // once at the front and once where it already stood.
    let displaced = metrics
        .iter()
        .enumerate()
        .skip(1)
        .find_map(|(i, m)| m.initial.map(|s| (i, s)))
        .filter(|(i, s)| {
            metrics[*i]
                .text
                .starts_with(lines[*i].spans[*s].text.as_str())
        });
    if let Some((line, span)) = displaced {
        spans.push(lines[line].spans[span].clone());
        texts.push(lines[line].spans[span].text.clone());
    }

    for (i, line) in lines.iter().enumerate() {
        for (s, span) in line.spans.iter().enumerate() {
            if displaced == Some((i, s)) {
                continue;
            }
            spans.push(span.clone());
        }
        let text = match displaced {
            Some((displaced_line, span)) if displaced_line == i => metrics[i]
                .text
                .strip_prefix(lines[i].spans[span].text.as_str())
                .expect("the prefix `displaced` was chosen for")
                .to_string(),
            _ => metrics[i].text.clone(),
        };
        texts.push(text);
    }

    // Taken off the joined text rather than off the first line's, because
    // `join_lines` trims each line as it goes: the marker's own trailing space
    // is already gone by then, and the prefix that survives is the marker as it
    // reads.
    let mut text = join_lines(&texts);
    if let Some(item) = item {
        let marker = metrics
            .first()
            .and_then(|m| m.text.get(..item.len))
            .unwrap_or_default()
            .trim();
        if let Some(rest) = text.strip_prefix(marker) {
            text = rest.trim_start().to_string();
        }
    }

    let mut block = Block {
        kind: match item {
            Some(item) => BlockKind::ListItem {
                level: item.level,
                ordinal: item.ordinal,
            },
            None => BlockKind::Paragraph,
        },
        page,
        bbox: [0.0; 4],
        text,
        spans,
        index,
        continues_previous: false,
        confidence: None,
        grid: None,
        table_origin: TableOrigin::Other,
        trace_learned: None,
    };
    recompute_bbox(&mut block);
    block
}

/// What one stretch of a column turned out to be: lines the merge still has to
/// group, or a table `crate::tables` already read the grid of.
enum Group {
    /// Line indices the merge joined into one block.
    Lines(Vec<usize>),
    /// A table, its cells already placed in a grid.
    Table(tables::Table),
}

/// The rows of one column run: which of its lines each row is drawn on.
///
/// A ROW IS NOT ALWAYS ONE LINE. `crate::text` hands back a row whose cell
/// wrapped as one block per cell, the continuation lines among them (#434), so
/// the lines of one row arrive consecutively and neither in one block nor in
/// one baseline. What holds them together is the run's own ROW PITCH: a
/// continuation sits at the leading, which is inside the pitch, and the next
/// row sits at the pitch itself.
///
/// The pitch is the middle of the run's own baseline steps, counted over the
/// DISTINCT baselines, because the cells of one row share theirs and would
/// otherwise vote for a pitch of nothing.
///
/// AND COUNTED OVER THE TABULAR LINES ONLY. A region holds a table and the
/// prose around it, and prose is the majority of almost every page that has
/// both: the median of every step is then the prose leading, which on a table
/// set on a wider pitch is smaller than the wrapped line it has to admit
/// (codex, round 1). A line with a gap wide enough to be a gutter is the one
/// thing here that says "table" before the grid is known, so the steps are
/// taken between those lines, and the region's own steps are the fallback for
/// a run that has none.
fn table_rows(metrics: &[LineMetrics], start: usize, end: usize) -> Vec<tables::Row> {
    let baselines_of = |tabular_only: bool| -> Vec<f64> {
        let mut baselines: Vec<f64> = (start..end)
            .filter(|i| !tabular_only || !metrics[*i].wide_gaps.is_empty())
            .map(|i| metrics[i].baseline)
            .collect();
        baselines.sort_by(|a, b| b.partial_cmp(a).unwrap_or(Ordering::Equal));
        baselines.dedup_by(|a, b| (*a - *b).abs() < SAME_BASELINE);
        baselines
    };
    let steps_of = |baselines: Vec<f64>| -> Vec<f64> {
        let mut steps: Vec<f64> = baselines.windows(2).map(|pair| pair[0] - pair[1]).collect();
        steps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
        steps
    };
    let steps = match steps_of(baselines_of(true)) {
        tabular if !tabular.is_empty() => tabular,
        _ => steps_of(baselines_of(false)),
    };
    let pitch = match steps.len() {
        0 => f64::INFINITY,
        len => steps[len / 2] * ROW_PITCH_FRACTION,
    };
    // AND WHERE THE RUN HAS TWO KINDS OF STEP, THE SMALLER ONE IS THE RAIL.
    // A table whose cells hold sentences sets those sentences at the leading
    // and parts its rows by more, so its steps fall into two clusters and the
    // widest jump between them is the boundary a reader sees the rows by. The
    // rules below cannot find those rows on their own: every line of such a
    // cell has a gutter of its own (the cell beside it wrapped too) and sits
    // as far from the line above as from the one below, so each came out as a
    // row of one line and the table was read as a grid of fragments in the
    // wrong order (#576). A run whose steps are all alike has no wrapped cell
    // in it and answers `None`, which leaves the rules below exactly as they
    // were.
    //
    // OVER EVERY BASELINE, not over the tabular ones `pitch` is measured on.
    // Leaving the untabular lines out takes the wrapped lines themselves out
    // of the measure -- a table with a head set across each bank then shows
    // steps of one pitch and of two, and two of one thing is a jump this
    // would believe.
    let leading = wrapped_leading(&steps_of(baselines_of(false)));

    // AND THE PITCH IS NOT THE ONLY RAIL. A pitch measured off a table set wide
    // swallows whatever the page put between its rows -- on `AlignmentTests` it
    // glued eleven separate field labels into one cell -- so a line joins the
    // row above only when it also sits CLOSER to it than to what follows, and
    // only when it has no gutter of its own. A line with a gutter in it is a
    // whole row; a cell of a row that arrived cell by cell (#434) has none, and
    // shares its row's baseline anyway.
    let mut rows: Vec<(usize, usize)> = Vec::new();
    let mut top = f64::NEG_INFINITY;
    // The line before this one, which is what the leading rail is measured
    // against: a cell four lines deep is four steps of the leading, and
    // measuring every one of them from the row's own first baseline turns the
    // fourth into a step of four leadings and ends the row there.
    let mut previous = f64::NEG_INFINITY;
    // Where the row puts its cells: every cell edge the lines of it have used
    // so far. A wrapped cell continues one of them and opens none of its own,
    // which is what tells a wrapped line from a line of a page that merely
    // sits close under another one. It grows with the row because a row
    // arrives cell by cell where one of its cells wrapped (#434): the first
    // line of it is one cell, not the whole row.
    let mut opener: Vec<f64> = Vec::new();
    for (i, line) in metrics.iter().enumerate().take(end).skip(start) {
        let gap_up = top - line.baseline;
        let step = previous - line.baseline;
        // Only where there is a rail to hold them against: every other run
        // pays nothing for this.
        let starts = if leading.is_some() {
            cell_starts(line)
        } else {
            Vec::new()
        };
        let in_the_same_cells = starts.iter().all(|at| {
            opener
                .iter()
                .any(|was| (at - was).abs() <= line.font_size * CELL_LEFT_EM)
        });
        let gap_down = (i + 1..end)
            .map(|j| line.baseline - metrics[j].baseline)
            .find(|step| *step > SAME_BASELINE)
            .unwrap_or(f64::INFINITY);
        let continues_the_row = gap_up <= SAME_BASELINE
            || match leading {
                Some(rail) => step <= rail + SAME_BASELINE && in_the_same_cells,
                None => gap_up < pitch && line.wide_gaps.is_empty() && gap_up < gap_down,
            };
        match rows.last_mut() {
            Some(row) if continues_the_row => {
                row.1 = i + 1;
                top = top.max(line.baseline);
                opener.extend(starts);
            }
            _ => {
                rows.push((i, i + 1));
                top = line.baseline;
                opener = starts;
            }
        }
        previous = line.baseline;
    }

    // AND A ROW THE RAIL FOLDED HAS TO HOLD A LINE THAT IS NOT A WHOLE ROW.
    //
    // The rail is the smallest step the REGION has, and a compact table set at
    // the region's own line spacing, with wider gaps somewhere else in the
    // region, therefore has every one of its rows at the rail -- and its rows
    // do share cell edges, so the test above would fold the whole table into
    // one row and `find` would lose it for having fewer than two (codex, #576,
    // round one). What tells the two apart is what the folded lines hold: a
    // cell that wrapped puts only SOME of the row's cells on its second line,
    // and a row of a compact table puts all of them on every line. So a fold
    // whose every line opens every cell the row has is undone.
    let rows = match leading {
        None => rows,
        Some(_) => rows
            .into_iter()
            .flat_map(|(first, end)| unfold_whole_lines(metrics, first, end))
            .collect(),
    };

    rows.into_iter()
        .map(|(first, end)| {
            let lines = &metrics[first..end];
            tables::Row {
                first,
                end,
                left: lines.iter().map(|m| m.left).fold(f64::INFINITY, f64::min),
                right: lines
                    .iter()
                    .map(|m| m.right)
                    .fold(f64::NEG_INFINITY, f64::max),
                top: lines
                    .iter()
                    .map(|m| m.top)
                    .fold(f64::NEG_INFINITY, f64::max),
                bottom: lines.iter().map(|m| m.bottom).fold(f64::INFINITY, f64::min),
                font_size: dominant_size(lines),
                bold: lines.iter().filter(|m| m.bold).count() * 2 > lines.len(),
            }
        })
        .collect()
}

/// One row per baseline, where the baselines under the first are rows of
/// their own; the row unchanged where they are the wrapped lines of it.
///
/// The cells of one row share a baseline, so a baseline is the unit here and
/// not a line: a row that arrived cell by cell (#434) is several lines at one
/// baseline, and it is the baseline the test is about. A row of a compact
/// table holds every cell the table has; the second line of a table of
/// descriptions, where the shortest cell has already ended, does not.
fn unfold_whole_lines(metrics: &[LineMetrics], first: usize, end: usize) -> Vec<(usize, usize)> {
    let mut baselines: Vec<(usize, usize)> = Vec::new();
    for i in first..end {
        match baselines.last_mut() {
            Some(group)
                if (metrics[group.0].baseline - metrics[i].baseline).abs() <= SAME_BASELINE =>
            {
                group.1 = i + 1
            }
            _ => baselines.push((i, i + 1)),
        }
    }
    if baselines.len() < 2 {
        return vec![(first, end)];
    }
    let starts_of = |group: &(usize, usize)| -> Vec<f64> {
        (group.0..group.1)
            .flat_map(|i| cell_starts(&metrics[i]))
            .collect()
    };
    let em = metrics[first].font_size.max(f64::MIN_POSITIVE) * CELL_LEFT_EM;
    let whole: Vec<f64> = baselines.iter().flat_map(starts_of).collect();
    let is_whole = |group: &(usize, usize)| -> bool {
        let mine = starts_of(group);
        whole
            .iter()
            .all(|was| mine.iter().any(|at| (at - was).abs() <= em))
    };
    // WHAT A FOLD NEEDS IS EVIDENCE THAT A CELL WRAPPED, and a baseline
    // holding less than the row holds is that evidence -- with one exception,
    // which is the whole of the difference between the two cases this has to
    // tell apart. A baseline with one run of ink in the row's FIRST column
    // and nothing else is a sparse row of data as readily as it is a cell
    // carrying on, and no measurement here decides which; every other short
    // baseline leaves the first column empty, or holds cells side by side,
    // and a row does neither.
    //
    // Without the exception one sparse row kept a whole compact table folded
    // into a single row, which `find` then dropped for having fewer than two
    // (codex, #576). With it, a table of descriptions keeps its rows, because
    // the lines its cells wrap onto sit in the columns they belong to.
    let column_one = starts_of(&baselines[0])
        .into_iter()
        .fold(f64::INFINITY, f64::min);
    let a_sparse_row = |group: &(usize, usize)| -> bool {
        let mine = starts_of(group);
        mine.len() == 1 && (mine[0] - column_one).abs() <= em
    };
    let wrapped = baselines[1..]
        .iter()
        .any(|group| !is_whole(group) && !a_sparse_row(group));
    if wrapped {
        vec![(first, end)]
    } else {
        baselines
    }
}

/// Where a line puts its cells: its own left edge, and the right edge of every
/// gap in it wide enough to be a gutter.
fn cell_starts(line: &LineMetrics) -> Vec<f64> {
    std::iter::once(line.left)
        .chain(line.wide_gaps.iter().map(|(_, to)| *to))
        .collect()
}

/// The leading a wrapped cell sits at, where the run has two kinds of step.
///
/// `steps` are the run's baseline steps, ascending. The FIRST jump of
/// `ROW_PITCH_JUMP` or more is where the leading stops: the leading is the
/// smallest kind of step a run has, and everything above the first jump is
/// something else -- a row pitch, the space under a header, the gap to the
/// next bank. Taking the WIDEST jump instead reads the largest of those gaps
/// as the boundary and folds a header into the first row of its own table.
/// A run with no such jump has one kind of step, so it is one line per row --
/// the commoner table, and the one the pitch rail already reads correctly.
fn wrapped_leading(steps: &[f64]) -> Option<f64> {
    if steps.len() <= ROW_PITCH_CLUSTER {
        return None;
    }
    // AND THE PITCH SIDE HAS TO BE A CLUSTER. One step above the jump is a
    // gap -- the space under a header, the end of the table -- and reading it
    // as the row pitch makes every other step a leading and the whole run one
    // row, which is what `a_table_inside_a_cell_is_that_cell_s_text` showed
    // when the jump was believed on its own. The leading side needs no such
    // floor: one wrapped line in a table is a table with a wrapped line in
    // it, and that same test is the case.
    let last = steps.len() - ROW_PITCH_CLUSTER;
    steps
        .windows(2)
        .enumerate()
        .take_while(|(at, _)| *at < last)
        .find(|(_, pair)| pair[0] > 0.0 && pair[1] / pair[0] >= ROW_PITCH_JUMP)
        .map(|(_, pair)| pair[0])
}

/// The type size most of a row's lines are set in.
fn dominant_size(lines: &[LineMetrics]) -> f64 {
    let mut sizes: Vec<f64> = lines
        .iter()
        .map(|m| m.font_size)
        .filter(|s| *s > 0.0)
        .collect();
    sizes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    sizes.get(sizes.len() / 2).copied().unwrap_or(0.0)
}

/// One cell of a table as a block.
///
/// The cell's spans are taken in reading order -- line by line down, left to
/// right across -- rather than in the order they were drawn, because a cell
/// whose text wrapped arrives interleaved with the rest of its row (#434): its
/// two lines are two runs at two baselines inside one [`TextBlock`], and read
/// in draw order the second would come out before half of the first.
fn assemble_cell(
    page: usize,
    lines: &[&TextBlock],
    cell: &tables::Cell,
    doubt: GridDoubt,
    origin: TableOrigin,
    cell_path_words: bool,
) -> Block {
    let mut spans: Vec<TextSpan> = cell
        .spans
        .iter()
        .map(|(line, span)| lines[*line].spans[*span].clone())
        .collect();
    spans.sort_by(|a, b| {
        b.y.partial_cmp(&a.y)
            .unwrap_or(Ordering::Equal)
            .then(a.x.partial_cmp(&b.x).unwrap_or(Ordering::Equal))
    });

    // Back into lines, so that each one is assembled by the same code that
    // assembles a line of prose -- the word spacing and the right-to-left
    // reordering included -- and the lines are then joined the way the lines of
    // a paragraph are.
    let mut rows: Vec<Vec<TextSpan>> = Vec::new();
    for span in &spans {
        let same_line = rows.last().is_some_and(|row: &Vec<TextSpan>| {
            let previous: &TextSpan = &row[0];
            (previous.y - span.y).abs() <= previous.font_size.max(span.font_size) * 0.5
        });
        match rows.last_mut() {
            Some(row) if same_line => row.push(span.clone()),
            _ => rows.push(vec![span.clone()]),
        }
    }
    let texts: Vec<String> = rows
        .into_iter()
        .map(|spans| TextBlock { spans }.text_with_cell_path_words(cell_path_words))
        .collect();

    let mut block = Block {
        kind: BlockKind::TableCell {
            row: cell.row,
            column: cell.column,
            row_span: cell.row_span,
            column_span: cell.column_span,
            header: cell.header,
        },
        page,
        bbox: [0.0; 4],
        text: join_lines(&texts),
        spans,
        index: 0,
        continues_previous: false,
        confidence: None,
        grid: Some(doubt),
        table_origin: origin,
        trace_learned: None,
    };
    recompute_bbox(&mut block);
    block
}

/// The block's box, from the spans it holds now.
///
/// Called again after a turned page's spans are put back in page space: the box
/// is the caller's frame, and the frame the paragraph was decided in is not it.
pub(crate) fn recompute_bbox(block: &mut Block) {
    let mut bbox = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for span in &block.spans {
        let (bottom, top) = span_extent(span);
        bbox[0] = bbox[0].min(span.x);
        bbox[1] = bbox[1].min(bottom);
        bbox[2] = bbox[2].max(span_right(span));
        bbox[3] = bbox[3].max(top);
    }
    block.bbox = if bbox.iter().all(|v| v.is_finite()) {
        bbox
    } else {
        [0.0; 4]
    };
}

/// The face most of a line's characters are set in: the PostScript name the
/// page gave the font, or `None` where the line keeps no name (standard-14 and
/// Type3 faces). Counted per character, so a bold phrase inside a line of
/// prose does not speak for it.
fn dominant_face(line: &TextBlock) -> Option<&str> {
    dominant_face_of_spans(&line.spans)
}

/// The same, on bare spans: the v2 release rejoin reads a released cell's
/// face the way the builder reads a line's, and the count is one definition.
pub(crate) fn dominant_face_of_spans(spans: &[TextSpan]) -> Option<&str> {
    let mut faces: Vec<(Option<&str>, usize)> = Vec::new();
    for span in spans {
        let face = span.font_name.as_deref();
        let chars = char_count(span);
        match faces.iter_mut().find(|(held, _)| *held == face) {
            Some(entry) => entry.1 += chars,
            None => faces.push((face, chars)),
        }
    }
    faces
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .and_then(|(face, _)| face)
}

/// Whether the line ends in terminal punctuation: `.`, `!` or `?`, trailing
/// closers aside. A colon or a semicolon is not terminal: it introduces what
/// follows or continues a clause, and a join that stopped at one would cut
/// every enumerated lead-in off its item.
fn ends_terminally(text: &str) -> bool {
    let trimmed = text
        .trim_end()
        .trim_end_matches(['"', '\'', '”', '’', ')', ']', '»']);
    matches!(
        trimmed.chars().next_back(),
        Some('.') | Some('!') | Some('?')
    )
}

/// Whether the line opens in lowercase: its first letter, leading punctuation
/// and figures aside, is a lowercase one. A line that opens with a capital --
/// a new sentence -- joins the line above it; so does a line with no letter
/// at all, which has no case to refuse on.
fn opens_lowercase(text: &str) -> bool {
    text.chars()
        .find(|c| c.is_alphabetic())
        .is_some_and(|c| c.is_lowercase())
}

/// The middle of a set of baseline steps, upper middle: the pitch of a block
/// that measured its own. Empty reads as none, for a column that gave no
/// second line to measure one from.
pub(crate) fn median_step(steps: &mut [f64]) -> f64 {
    if steps.is_empty() {
        return 0.0;
    }
    steps.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    steps[steps.len() / 2]
}

/// One line as the narrow-column pair join reads it: the geometry the page
/// drew, the type it is set in, and the text as it reads.
///
/// [`LineMetrics`] is the builder's richer reading of the same line; this is
/// the pair join's whole input, so the builder ([`join_narrow_columns`]) and
/// the v2 release rejoin decide by one function
/// ([`narrow_column_pair_joins`]).
pub(crate) struct JoinLine<'a> {
    /// Leftmost edge of the line's ordinary text.
    pub left: f64,
    /// The left edge the indent is measured from: [`LineMetrics`]' own, which
    /// beside a drop cap is the cap's left rather than the text's.
    pub indent_left: f64,
    /// Rightmost edge of the line.
    pub right: f64,
    /// The baseline the line sits on.
    pub baseline: f64,
    /// Top of the line's glyphs.
    pub top: f64,
    /// Bottom of the line's glyphs.
    pub bottom: f64,
    /// The type size most of the line's characters are set in.
    pub font_size: f64,
    /// Whether most of the line's characters are set in a bold face.
    pub bold: bool,
    /// The face most of the line's characters are set in.
    pub face: Option<&'a str>,
    /// The line as it reads.
    pub text: &'a str,
    /// A list marker opens the line, or the line is a list item: either way
    /// it starts its own block. Read on the line below only.
    pub marker_or_item: bool,
    /// A second line confirmed a gutter inside it: a row of cells, not prose.
    pub tabular: bool,
}

impl<'a> JoinLine<'a> {
    /// The builder's reading of one of its own lines: `face` from the line's
    /// spans, `marker_or_item` from its marker, its item, and the unmarked
    /// entries.
    fn of(metrics: &'a LineMetrics, face: Option<&'a str>, marker_or_item: bool) -> Self {
        Self {
            left: metrics.left,
            indent_left: metrics.indent_left,
            right: metrics.right,
            baseline: metrics.baseline,
            top: metrics.top,
            bottom: metrics.bottom,
            font_size: metrics.font_size,
            bold: metrics.bold,
            face,
            text: &metrics.text,
            marker_or_item,
            tabular: metrics.tabular,
        }
    }
}

/// Whether the line `below` joins the block `above` ends: the narrow-column
/// pair join, one function for the builder and the v2 release rejoin.
///
/// `above_item` is whether the open block's FIRST line is a list item -- a
/// marker, a confirmed item, or an unmarked entry -- because nothing joins
/// onto an item either. `pitch` is the pitch of the block above: its own,
/// once the join has given it two lines to measure, and its column's until
/// then. `measure` is the column's own measure, over both runs the pair
/// stands in. `rules` are the horizontal lines the page drew, a boundary the
/// join never crosses.
///
/// The pair joins where the left edges agree to half an em (or both lines
/// are ragged inside the column they overlap), the step is the block's own
/// pitch, the size holds to half a point and the face and the weight do not
/// change. A sentence boundary joins too. What refuses is a marker on the
/// lower line, a table under either, a ruling line, a second line of white,
/// a lead-in ending in a colon, or a full-measure line ending in terminal
/// punctuation over a lowercase opening.
pub(crate) fn narrow_column_pair_joins(
    above: &JoinLine,
    above_item: bool,
    below: &JoinLine,
    pitch: f64,
    measure: f64,
    rules: &[HorizontalRule],
) -> bool {
    // A list item starts its own block, here as in the merge -- and on
    // the marker as written, without asking a second item to agree,
    // because agreeing markers in adjacent one-line groups are exactly
    // what this join would wrongly fuse.
    if below.marker_or_item {
        return false;
    }
    // And nothing joins onto an item either: the line after one at the
    // same edge is the prose after the list, not its second line. The
    // unmarked entries count as items too.
    if above_item {
        return false;
    }
    // A row of cells is not a line of prose, and a ruling line is a
    // boundary the builder never crosses.
    if above.tabular || below.tabular {
        return false;
    }
    if rule_between_boxes(
        [above.left, above.bottom, above.right, above.top],
        [below.left, below.bottom, below.right, below.top],
        rules,
    ) {
        return false;
    }
    let em = above.font_size.max(below.font_size);
    if em <= 0.0 {
        return false;
    }
    let step = above.baseline - below.baseline;
    if step <= MIN_BASELINE_STEP_EM * em
        || step > NARROW_PITCH_MULT * pitch
        || step > MAX_LEADING_EM * em
    {
        return false;
    }
    if (above.font_size - below.font_size).abs() > NARROW_SIZE_PT {
        return false;
    }
    if above.bold != below.bold || above.face != below.face {
        return false;
    }
    let ragged = |m: &JoinLine| measure - m.right > SHORT_LINE_EM * em;
    if (above.indent_left - below.indent_left).abs() > NARROW_X0_EM * em
        && !(ragged(above)
            && ragged(below)
            && overlap_fraction_of(above.left, above.right, below.left, below.right)
                >= COLUMN_OVERLAP_FRACTION)
    {
        return false;
    }
    // A sentence boundary joins: the annotation keeps one paragraph whole
    // across it. What refuses is a full-measure line ending in terminal
    // punctuation over a lowercase opening.
    if ends_terminally(above.text) && opens_lowercase(below.text) && !ragged(above) {
        return false;
    }
    // A line ending in a colon is a label or a lead-in, and its
    // disposition belongs to the heading logic: a `NOTE 12:` over its
    // title joins there, at that logic's level, and an early join here
    // would hand the classifier one block where it reads two.
    if above.text.trim_end().ends_with(':') {
        return false;
    }
    true
}

/// One-line blocks back into the narrow column they were sliced out of (#574).
///
/// A paragraph set in a narrow column -- a sidebar, a caption under a photo,
/// an address block -- comes out of the merge as one block per line, and the
/// merge is not what cut it: the XY-cut put each line in a region of its own.
/// Two columns with offset baselines drag the cut's quartile down until every
/// line gap qualifies, and the leading floor only arms on a repeated step, so
/// a column the cut meets one line at a time is sliced one line at a time --
/// and regions never merge back. This joins them again, pair by pair, where
/// the pair reads as two lines of one column: the left edges agree to half an
/// em (or both lines are ragged inside the column they overlap), the step is
/// the block's own pitch, the size holds to half a point and the face and the
/// weight do not change. A sentence boundary joins too -- the annotation keeps
/// one paragraph whole across it. What refuses is a marker on the lower line,
/// a table between or under either, a ruling line, a second line of white, a
/// lead-in ending in a colon (the heading logic's to decide), or a
/// full-measure line ending in terminal punctuation over a lowercase opening:
/// the next paragraph starting where the last ended, not a wrap.
///
/// Only ever one line onto the block above it, and only onto a block of one
/// line or one these joins built: a paragraph the merge assembled was refused
/// its next line for a reason this repair does not overrule.
fn join_narrow_columns(
    groups: &mut Vec<(usize, Group)>,
    metrics: &[LineMetrics],
    lines: &[&TextBlock],
    runs: &[(usize, usize, f64)],
    shapes: &[(f64, f64)],
    rules: &[HorizontalRule],
    entries: &std::collections::HashSet<usize>,
) {
    // Whether the one-line group `next` joins the group `open` above it.
    // `open` is one line, or ends a run of these joins. The pair itself is
    // [`narrow_column_pair_joins`]' to decide, shared with the v2 release
    // rejoin; what stays here is the group shape it never sees, and the
    // pitch and the measure, which only the builder's runs can give it.
    let joinable = |open: &[usize], open_joined: bool, next: &[usize]| -> bool {
        if next.len() != 1 || (open.len() != 1 && !open_joined) {
            return false;
        }
        let (Some(&last), Some(&first), Some(&at)) = (open.last(), open.first(), next.first())
        else {
            return false;
        };
        let above = &metrics[last];
        let below = &metrics[at];
        let em = above.font_size.max(below.font_size);
        // The pitch of the block above: its own, once the join has given it
        // two lines to measure, and its run's until then -- floored at solid
        // setting, because a run of one line measures the page's quartile
        // instead, which on an interleaved page is the offset between two
        // columns. Whatever it measures, white the depth of most of another
        // line is a break.
        let pitch = if open.len() >= 2 {
            let mut steps: Vec<f64> = open
                .windows(2)
                .map(|pair| metrics[pair[0]].baseline - metrics[pair[1]].baseline)
                .collect();
            median_step(&mut steps)
        } else {
            runs.iter()
                .zip(shapes.iter())
                .find(|((start, end, _), _)| *start <= last && last < *end)
                .map(|(_, shape)| shape.0)
                .unwrap_or(0.0)
                .max(SOLID_PITCH_EM * em)
        };
        // The column's own measure, over both runs the pair stands in: a cut
        // that sliced every line left each run one line wide, which is no
        // measure at all.
        let measure = runs
            .iter()
            .zip(shapes.iter())
            .filter(|((start, end, _), _)| {
                (*start <= last && last < *end) || (*start <= at && at < *end)
            })
            .map(|(_, shape)| shape.1)
            .fold(f64::NEG_INFINITY, f64::max);
        let upper = JoinLine::of(
            above,
            dominant_face(lines[last]),
            // Read on the line below only; the block above stands already.
            false,
        );
        let lower = JoinLine::of(
            below,
            dominant_face(lines[at]),
            below.marker.is_some() || below.item.is_some() || entries.contains(&at),
        );
        let above_item = metrics[first].item.is_some() || entries.contains(&first);
        narrow_column_pair_joins(&upper, above_item, &lower, pitch, measure, rules)
    };

    let mut out: Vec<(usize, Group)> = Vec::with_capacity(groups.len());
    // The group still open to a join: its key, its lines, and whether these
    // joins built it.
    let mut open: Option<(usize, Vec<usize>, bool)> = None;
    for (first, group) in std::mem::take(groups) {
        match group {
            Group::Table(table) => {
                if let Some((key, lines, _)) = open.take() {
                    out.push((key, Group::Lines(lines)));
                }
                out.push((first, Group::Table(table)));
            }
            Group::Lines(next) => {
                let join = match &open {
                    Some((_, open_lines, open_joined)) => joinable(open_lines, *open_joined, &next),
                    None => false,
                };
                if join {
                    if tracing() {
                        if let Some((_, open_lines, _)) = &open {
                            let a = open_lines
                                .last()
                                .map(|i| metrics[*i].text.as_str())
                                .unwrap_or("");
                            let b = next
                                .first()
                                .map(|i| metrics[*i].text.as_str())
                                .unwrap_or("");
                            eprintln!("JOIN narrow-column | {a:?} -> {b:?}");
                        }
                    }
                    let (_, open_lines, open_joined) =
                        open.as_mut().expect("a join has a block above it");
                    open_lines.extend(next);
                    *open_joined = true;
                } else {
                    if let Some((key, lines, _)) = open.take() {
                        out.push((key, Group::Lines(lines)));
                    }
                    open = Some((first, next, false));
                }
            }
        }
    }
    if let Some((key, lines, _)) = open.take() {
        out.push((key, Group::Lines(lines)));
    }
    *groups = out;
}

/// The most white, in ems of the marker, between a lone bullet and the line
/// it opens. A marker column stands about an em from its text -- 0.2 to 1.3
/// em on the lists tuning pages -- and a table's gutter is three
/// (`TABULAR_GAP_EM`), so two keeps the fusion out of tables.
const FUSED_MARKER_GAP_EM: f64 = 2.0;

/// How far apart, in points, a lone bullet and its line may be across the
/// baseline. The two are drawn on one baseline -- exactly 0.0 apart on the
/// tuning pages -- so a point is dust from the transforms, and the next line
/// down is a leading away.
const FUSED_BASELINE_PT: f64 = 1.0;

/// The bullet a line holds and nothing else, if that is all it holds.
///
/// One glyph from [`lists::BULLET_CHARS`], and white space besides: the tab
/// run a producer draws after its bullets counts as white space, and a second
/// glyph of any kind -- a second bullet, a word -- says the line is complete
/// as it stands.
fn lone_marker_glyph(line: &TextBlock) -> Option<char> {
    let mut found = None;
    for span in &line.spans {
        for c in span.text.chars() {
            if c.is_whitespace() {
                continue;
            }
            if found.is_some() || !lists::BULLET_CHARS.contains(&c) {
                return None;
            }
            found = Some(c);
        }
    }
    found
}

/// Each lone marker back onto the line it opens.
///
/// The cut parts a marker column from its text where the white between them
/// runs the region's height: the bullets come back as lines of their own,
/// ordered away from their text, and a line with no text after its marker is
/// no marker at all -- so a list set marker-out loses every item whose bullet
/// the cut lifted. The marker and its text are still drawn on ONE baseline,
/// which no two lines of a column share: a line that holds nothing but a
/// bullet joins the nearest line to its right on its own baseline.
///
/// A page's lines with each lone marker back on the line it opens: the fused
/// lines in the text's place, the markers gone, and the region bounds rebuilt
/// round what is left.
struct FusedPage {
    lines: Vec<TextBlock>,
    bounds: Vec<(usize, usize)>,
    table_first: Vec<bool>,
}

/// Each lone marker back onto the line it opens.
///
/// The cut parts a marker column from its text where the white between them
/// runs the region's height: the bullets come back as lines of their own,
/// ordered away from their text, and a line with no text after its marker is
/// no marker at all -- so a list set marker-out loses every item whose bullet
/// the cut lifted. The marker and its text are still drawn on ONE baseline,
/// which no two lines of a column share: a line that holds nothing but a
/// bullet joins the nearest line to its right on its own baseline.
///
/// Page-wide and before everything else, because the cut may have ordered the
/// marker's sliver anywhere -- only the baseline says which line is its own.
/// A line that already opens with a marker takes no donation, and a fused
/// line takes no second one. Returns `None` when no line took one and the
/// page is untouched.
fn fuse_lone_markers(
    lines: &[&TextBlock],
    bounds: &[(usize, usize)],
    table_first: &[bool],
    cell_path_words: bool,
) -> Option<FusedPage> {
    if !lines.iter().any(|line| lone_marker_glyph(line).is_some()) {
        return None;
    }
    // The anchor span's baseline is the line's, the way `LineMetrics::of`
    // reads it; the edges are the spans'.
    let baseline = |line: &TextBlock| anchor_span(line.spans.iter()).map(|s| s.y).unwrap_or(0.0);
    let left = |line: &TextBlock| line.spans.iter().map(|s| s.x).fold(f64::INFINITY, f64::min);
    let right = |line: &TextBlock| {
        line.spans
            .iter()
            .map(span_right)
            .fold(f64::NEG_INFINITY, f64::max)
    };
    let mut donated: Vec<Option<Vec<TextSpan>>> = vec![None; lines.len()];
    let mut killed = vec![false; lines.len()];
    for (i, marker) in lines.iter().enumerate() {
        let Some(_) = lone_marker_glyph(marker) else {
            continue;
        };
        let em = anchor_span(marker.spans.iter())
            .map(|s| s.font_size.max(1.0))
            .unwrap_or(1.0);
        let base = baseline(marker);
        let edge = right(marker);
        let mut best: Option<(usize, f64)> = None;
        for (j, line) in lines.iter().enumerate() {
            if i == j || killed[j] || donated[j].is_some() || lone_marker_glyph(line).is_some() {
                continue;
            }
            if line
                .text_with_cell_path_words(cell_path_words)
                .trim()
                .is_empty()
            {
                continue;
            }
            // Complete as it stands: whatever sits left of it at its
            // baseline is not its marker. Read without a lead, which is
            // what a marker in running text is; a picture-font bullet or a
            // raised figure needs the spans `LineMetrics::of` still holds.
            if lists::marker_at(
                &line.text_with_cell_path_words(cell_path_words),
                lists::Lead::default(),
            )
            .is_some()
            {
                continue;
            }
            if (baseline(line) - base).abs() > FUSED_BASELINE_PT {
                continue;
            }
            let start = left(line);
            if start <= edge {
                continue;
            }
            let gap = start - edge;
            if gap > FUSED_MARKER_GAP_EM * em {
                continue;
            }
            if best.is_none_or(|(_, known): (usize, f64)| gap < known) {
                best = Some((j, gap));
            }
        }
        if let Some((j, _)) = best {
            let mut spans = marker.spans.clone();
            spans.extend(lines[j].spans.iter().cloned());
            spans.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(Ordering::Equal));
            donated[j] = Some(spans);
            killed[i] = true;
        }
    }
    if donated.iter().all(Option::is_none) {
        return None;
    }
    let mut region_of = vec![0usize; lines.len()];
    for (region, &(start, end)) in bounds.iter().enumerate() {
        for slot in region_of.iter_mut().take(end.min(lines.len())).skip(start) {
            *slot = region;
        }
    }
    let mut owned: Vec<TextBlock> = Vec::with_capacity(lines.len());
    let mut regions: Vec<usize> = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        if killed[i] {
            continue;
        }
        match donated[i].take() {
            Some(spans) => owned.push(TextBlock { spans }),
            None => owned.push((*line).clone()),
        }
        regions.push(region_of[i]);
    }
    let mut fused_bounds: Vec<(usize, usize)> = Vec::new();
    let mut fused_first: Vec<bool> = Vec::new();
    for (i, region) in regions.iter().enumerate() {
        if i > 0 && regions[i - 1] == *region {
            fused_bounds.last_mut().expect("a bound is open").1 = i + 1;
        } else {
            fused_bounds.push((i, i + 1));
            fused_first.push(table_first.get(*region).copied().unwrap_or(false));
        }
    }
    Some(FusedPage {
        lines: owned,
        bounds: fused_bounds,
        table_first: fused_first,
    })
}

/// How many lines must show white at the same x before it counts as the
/// gutter between two columns of text rather than a wide word space.
///
/// Three: the narrowest fused list on the tuning pages shows five, and a
/// single-column page never aligns three table-wide gaps at one x -- with the
/// list-bearing rule below, nothing single-column ever splits at all.
const GUTTER_SUPPORT_LINES: usize = 3;

/// How many characters each side of a gutter split must hold, not counting
/// white space. A contents page sets its page numbers apart from its entries
/// with white as wide as any gutter, and the number is a figure or two where
/// a column's line runs on.
const GUTTER_HALF_MIN_CHARS: usize = 8;

/// Lines the page fused across its own column gutter, split back into their
/// columns: the lines, the region bounds round them, and which of them a
/// table holds. The table groups are renumbered in place, through `groups`.
struct SplitPage {
    lines: Vec<TextBlock>,
    bounds: Vec<(usize, usize)>,
    in_table: Vec<bool>,
}

/// Each line the cut fused across a column gutter back into its two columns.
///
/// The cut parts columns where white runs the region's height, and a marker
/// hung into the gutter -- a bullet column standing between the text it marks
/// and the column beside it -- closes it: the page comes back read across both
/// columns, one fused line per row. The gutter is still there inside every
/// line, a wide gap at the same x row after row, which is what this splits on.
/// What comes out are the page's own columns: same-column items group under
/// the list rule, and no list ever continues across the gutter again.
///
/// AFTER the tables are found, and never through one: a table's rows are
/// fused lines too, and splitting them would unmake the grid. Only a line no
/// table holds is split, and only in a stack where two of the halves open
/// with a marker -- a list is the reason to cut, and prose with an aligned
/// gap of its own is left as the page wrote it. The tables keep their lines;
/// only their line numbers move to the longer lines.
/// One gap of one line: the line number, and the white from the end of the
/// span on its left to the start of the span on its right.
type LineGap = (usize, f64, f64);

fn split_fused_lines(
    lines: &[&TextBlock],
    metrics: &[LineMetrics],
    bounds: &[(usize, usize)],
    in_table: &[bool],
    groups: &mut [(usize, Group)],
) -> Option<SplitPage> {
    // The wide gaps of the lines no table holds, clustered by overlap: gaps
    // that share an x are one gutter, and one gutter with few lines behind it
    // is a wide word space.
    let mut gaps: Vec<LineGap> = Vec::new();
    for (i, line) in metrics.iter().enumerate() {
        if in_table[i] {
            continue;
        }
        for &(end, start) in &line.wide_gaps {
            gaps.push((i, end, start));
        }
    }
    if gaps.len() < GUTTER_SUPPORT_LINES {
        return None;
    }
    gaps.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));
    // Each cluster holds its members' line numbers, so the support counts
    // lines and not gaps: two gaps of one line in one cluster are one voice.
    let mut clusters: Vec<(Vec<usize>, Vec<LineGap>)> = Vec::new();
    for (line, end, start) in gaps {
        match clusters.last_mut() {
            Some((member_lines, members))
                if members.iter().any(|&(_, e, s)| end <= s && e <= start) =>
            {
                if !member_lines.contains(&line) {
                    member_lines.push(line);
                }
                members.push((line, end, start));
            }
            _ => clusters.push((vec![line], vec![(line, end, start)])),
        }
    }
    let gutters: Vec<&Vec<LineGap>> = clusters
        .iter()
        .filter(|(member_lines, _)| member_lines.len() >= GUTTER_SUPPORT_LINES)
        .map(|(_, members)| members)
        .collect();
    if gutters.is_empty() {
        return None;
    }
    let in_a_gutter = |line: usize, end: f64, start: f64| {
        gutters.iter().any(|members| {
            members
                .iter()
                .any(|&(l, e, s)| l == line && e == end && s == start)
        })
    };

    // The halves of every line its gutter gaps cut, measured the way any line
    // is: the marker each half opens with is read off its own spans, so a
    // picture-font bullet counts like an inline one.
    let x_order = |line: &TextBlock| {
        let mut order: Vec<usize> = (0..line.spans.len()).collect();
        order.sort_by(|a, b| {
            line.spans[*a]
                .x
                .partial_cmp(&line.spans[*b].x)
                .unwrap_or(Ordering::Equal)
        });
        order
    };
    // Line by line, the halves it splits into: `None` where the line stays
    // whole -- no gutter gap, or a half too short to be a column's line.
    let mut halves: Vec<Option<Vec<TextBlock>>> = vec![None; lines.len()];
    let mut marks: Vec<Vec<bool>> = vec![Vec::new(); lines.len()];
    for (i, line) in lines.iter().enumerate() {
        if in_table[i] {
            continue;
        }
        let order = x_order(line);
        let mut cuts = vec![false; order.len().saturating_sub(1)];
        for (k, pair) in order.windows(2).enumerate() {
            let end = span_right(&line.spans[pair[0]]);
            let start = line.spans[pair[1]].x;
            cuts[k] = end < start && in_a_gutter(i, end, start);
        }
        if !cuts.iter().any(|cut| *cut) {
            continue;
        }
        let mut parts: Vec<TextBlock> = Vec::new();
        let mut at = 0usize;
        for (k, cut) in cuts.iter().enumerate() {
            if *cut {
                parts.push(TextBlock {
                    spans: order[at..=k]
                        .iter()
                        .map(|j| line.spans[*j].clone())
                        .collect(),
                });
                at = k + 1;
            }
        }
        parts.push(TextBlock {
            spans: order[at..].iter().map(|j| line.spans[*j].clone()).collect(),
        });
        let measured: Vec<LineMetrics> = parts.iter().map(LineMetrics::of).collect();
        let substantial = measured.iter().all(|half| {
            half.text.chars().filter(|c| !c.is_whitespace()).count() >= GUTTER_HALF_MIN_CHARS
        });
        if !substantial {
            continue;
        }
        marks[i] = measured.iter().map(|half| half.marker.is_some()).collect();
        halves[i] = Some(parts);
    }

    // A stack is list-bearing when two of its halves open with a marker: the
    // run rule still confirms them afterwards, and anything less is prose.
    let mut bearing = vec![false; lines.len()];
    for (start, end) in column_stacks(metrics, bounds) {
        let count: usize = (start..end)
            .map(|i| marks[i].iter().filter(|marked| **marked).count())
            .sum();
        if count >= 2 {
            bearing[start..end].fill(true);
        }
    }
    if !halves
        .iter()
        .enumerate()
        .any(|(i, parts)| parts.is_some() && bearing[i])
    {
        return None;
    }

    // The new lines, halves in the place of their line, and every line number
    // moved to them: the tables keep their lines, which never split.
    let mut region_of = vec![0usize; lines.len()];
    for (region, &(start, end)) in bounds.iter().enumerate() {
        for slot in region_of.iter_mut().take(end.min(lines.len())).skip(start) {
            *slot = region;
        }
    }
    let split: Vec<bool> = halves
        .iter()
        .enumerate()
        .map(|(i, parts)| parts.is_some() && bearing[i])
        .collect();
    let mut owned: Vec<TextBlock> = Vec::with_capacity(lines.len());
    let mut regions: Vec<usize> = Vec::with_capacity(lines.len());
    let mut tabled: Vec<bool> = Vec::with_capacity(lines.len());
    let mut moved: Vec<usize> = vec![0; lines.len()];
    for (i, line) in lines.iter().enumerate() {
        moved[i] = owned.len();
        match halves[i].take() {
            Some(parts) if split[i] => {
                for part in parts {
                    owned.push(part);
                    regions.push(region_of[i]);
                    tabled.push(false);
                }
            }
            _ => {
                owned.push((*line).clone());
                regions.push(region_of[i]);
                tabled.push(in_table[i]);
            }
        }
    }
    let mut split_bounds: Vec<(usize, usize)> = Vec::new();
    for (i, region) in regions.iter().enumerate() {
        if i > 0 && regions[i - 1] == *region {
            split_bounds.last_mut().expect("a bound is open").1 = i + 1;
        } else {
            split_bounds.push((i, i + 1));
        }
    }
    debug_assert_eq!(
        split_bounds.len(),
        bounds.len(),
        "a split adds lines to regions, never regions"
    );
    for (first, group) in groups.iter_mut() {
        *first = moved[*first];
        if let Group::Table(table) = group {
            for cell in table.cells.iter_mut() {
                for (line, _) in cell.spans.iter_mut() {
                    debug_assert!(!split[*line], "a table holds no split line");
                    *line = moved[*line];
                }
            }
        }
    }
    Some(SplitPage {
        lines: owned,
        bounds: split_bounds,
        in_table: tabled,
    })
}

/// Every paragraph on one page, in reading order.
///
/// `regions` are the page's lines as [`crate::text`] produced them, grouped by
/// the leaf region of the XY-cut they came from and in the order the page is
/// read in. THE REGION IS THE COLUMN GAP: a page's two columns are two regions,
/// and a full-width title above them is a third -- so a paragraph that must
/// never cross a column gap is a paragraph that never crosses a region, and
/// nothing here has to rediscover from the text's own geometry what the cut
/// already decided. (A title left-aligned over a column, set in the body size,
/// is the same picture as the first line of a paragraph; no measurement of the
/// two lines tells them apart, and the cut is the only thing that knows.)
///
/// One repair crosses regions afterwards: [`join_narrow_columns`] joins
/// one-line groups back into the narrow column the cut sliced, pair by pair
/// and only where the pair reads as two lines of one column.
///
/// `rules` are the horizontal lines the page drew, in the same frame;
/// `page_leading` is the page's tight line-to-line spacing. `first_index` is
/// where this page's blocks start in the document's reading order.
/// `table_strategy` is which evidence may find a table: under
/// [`TableStrategy::Lines`] a grid no drawn rule crosses is left for the
/// paragraph merge below, exactly as a run the builder refused.
#[cfg(test)]
pub(crate) fn paragraphs(
    page: usize,
    first_index: usize,
    regions: &[Vec<TextBlock>],
    whole: &[bool],
    rules: &[HorizontalRule],
    page_leading: f64,
    table_strategy: TableStrategy,
) -> Vec<Block> {
    paragraphs_with_cell_path_words(
        page,
        first_index,
        regions,
        whole,
        rules,
        page_leading,
        table_strategy,
        true,
    )
}

/// [`paragraphs`] with the cell-path word rules where `cell_path_words` is
/// true: line and cell texts read the quarter-em boundary as a space. The
/// bare entry keeps the shipped words for its unit tests; the extraction
/// path reads this one with the option.
#[allow(clippy::too_many_arguments)]
pub(crate) fn paragraphs_with_cell_path_words(
    page: usize,
    first_index: usize,
    regions: &[Vec<TextBlock>],
    whole: &[bool],
    rules: &[HorizontalRule],
    page_leading: f64,
    table_strategy: TableStrategy,
    cell_path_words: bool,
) -> Vec<Block> {
    let mut lines: Vec<&TextBlock> = Vec::new();
    let mut bounds: Vec<(usize, usize)> = Vec::new();
    let mut table_first: Vec<bool> = Vec::new();
    for (k, region) in regions.iter().enumerate() {
        let start = lines.len();
        lines.extend(region.iter().filter(|line| !line.spans.is_empty()));
        if lines.len() > start {
            bounds.push((start, lines.len()));
            table_first.push(whole.get(k).copied().unwrap_or(false));
        }
    }
    if lines.is_empty() {
        return Vec::new();
    }
    // Each lone marker back onto the line it opens, before anything reads
    // the lines: the cut may have ordered the marker's sliver anywhere, and
    // only its own baseline survives. Untouched pages keep their lines and
    // their bounds -- the fusion hands back `None` and nothing is rebuilt.
    let owned;
    if let Some(fused) = fuse_lone_markers(&lines, &bounds, &table_first, cell_path_words) {
        owned = fused.lines;
        lines = owned.iter().collect();
        bounds = fused.bounds;
        table_first = fused.table_first;
    }

    let mut metrics: Vec<LineMetrics> = lines
        .iter()
        .map(|line| LineMetrics::of_with(line, true, cell_path_words))
        .collect();
    mark_tabular(&mut metrics);
    confirm_initials(&mut metrics, &lines, cell_path_words);

    // The tables, before anything is cut into columns: a table is a statement
    // about a REGION, the way a list's levels are (#569, #570), and its rows
    // arrive one block per cell wherever a cell wrapped -- which is exactly the
    // seam `column_runs` cuts at. What is left of each region after its tables
    // are taken out is what the paragraph merge sees.
    //
    // Keyed by the line each group opens at, because the tables are taken out
    // of the regions and the runs are then built round them: sorting the two
    // back together by line is the reading order, since the regions and the
    // lines inside them are already in it.
    let mut groups: Vec<(usize, Group)> = Vec::new();
    let mut in_table: Vec<bool> = vec![false; lines.len()];
    for ((region_start, region_end), whole) in bounds.iter().zip(&table_first) {
        let rows = table_rows(&metrics, *region_start, *region_end);
        // The trace's page for vetoed candidates: `find` never learns it.
        let _trace_page = crate::table_trace::trace_page_guard(page);
        let found = if *whole {
            tables::find_whole(&lines, &rows, rules)
        } else {
            tables::find(&lines, &rows, rules)
        };
        // Under `Lines` the unruled grids never become groups: their lines
        // stay out of `in_table`, so the stretches below read them as the
        // paragraphs they would have been without the builder.
        let found = match table_strategy {
            TableStrategy::Lines => found
                .into_iter()
                .filter(|table| tables::table_has_ruling(&rows, table, rules))
                .collect(),
            TableStrategy::Text | TableStrategy::Both => found,
        };
        for table in found {
            let (first, end) = (rows[table.first].first, rows[table.end - 1].end);
            in_table[first..end].fill(true);
            groups.push((first, Group::Table(table)));
        }
    }

    // Each line the cut fused across a column gutter back into its columns,
    // before anything is cut into columns itself: the merge and the list rule
    // below then see the page's own columns, and a list never continues
    // across the gutter again. Untouched pages keep their lines -- the split
    // hands back `None` where no gutter runs or no stack is list-bearing, and
    // nothing is rebuilt.
    let split_owned;
    if let Some(split) = split_fused_lines(&lines, &metrics, &bounds, &in_table, &mut groups) {
        split_owned = split.lines;
        lines = split_owned.iter().collect();
        bounds = split.bounds;
        in_table = split.in_table;
        metrics = lines
            .iter()
            .map(|line| LineMetrics::of_with(line, true, cell_path_words))
            .collect();
        mark_tabular(&mut metrics);
        confirm_initials(&mut metrics, &lines, cell_path_words);
    }

    let mut stretches: Vec<(usize, usize)> = Vec::new();
    for (region_start, region_end) in &bounds {
        let mut start = *region_start;
        for (i, held) in in_table
            .iter()
            .enumerate()
            .take(*region_end)
            .skip(*region_start)
        {
            if *held {
                if start < i {
                    stretches.push((start, i));
                }
                start = i + 1;
            }
        }
        if start < *region_end {
            stretches.push((start, *region_end));
        }
    }

    // Every column of the page, with the left edge it is set to.
    let runs: Vec<(usize, usize, f64)> = stretches
        .iter()
        .flat_map(|(stretch_start, stretch_end)| {
            column_runs(&metrics[*stretch_start..*stretch_end])
                .into_iter()
                .map(|(first, last)| (stretch_start + first, stretch_start + last))
                .collect::<Vec<_>>()
        })
        .map(|(start, end)| (start, end, body_left(&metrics[start..end])))
        .collect();
    // Before the merge, because a confirmed item is a boundary the merge has to
    // refuse to cross, and over the REGIONS rather than the column runs: a
    // list's own levels do not line up, which is what a run seam is made of
    // (#569).
    mark_list_items(&mut metrics, &bounds);
    let style = indent_style(&metrics, &runs);
    // Each run's leading and its column measure, for the merge below and the
    // narrow-column join after it. The measure is the COLUMN's -- every line
    // on the page that stands over this run -- and neither the run's nor its
    // region's: a list is a run seam and the cut can make a region of two
    // short items and the line after them, whose own edge is no measure at
    // all (codex, #1981). Too wide errs toward keeping lines apart, which is
    // the safe side.
    let run_shapes: Vec<(f64, f64)> = runs
        .iter()
        .map(|(start, end, _)| {
            let leading = run_leading(&metrics[*start..*end], page_leading);
            let run_left = metrics[*start..*end]
                .iter()
                .map(|m| m.indent_left)
                .fold(f64::INFINITY, f64::min);
            let run_right = metrics[*start..*end]
                .iter()
                .map(|m| m.right)
                .fold(f64::NEG_INFINITY, f64::max);
            let measure = metrics
                .iter()
                .filter(|m| m.indent_left < run_right && m.right > run_left)
                .map(|m| m.right)
                .fold(run_right, f64::max);
            (leading, measure)
        })
        .collect();
    let mut entries: std::collections::HashSet<usize> = std::collections::HashSet::new();

    for (index, (start, end, margin)) in runs.iter().enumerate() {
        let (start, end, margin) = (*start, *end, *margin);
        let (leading, measure) = run_shapes[index];
        let column = Column {
            margin,
            right: measure,
        };
        let mut current = vec![start];
        let mut run_groups: Vec<Vec<usize>> = Vec::new();
        for i in start + 1..end {
            if continues(&metrics, &current, i, style, column, rules, leading) {
                current.push(i);
            } else {
                run_groups.push(std::mem::take(&mut current));
                current = vec![i];
            }
        }
        run_groups.push(current);
        for (group, entry) in
            run_groups
                .iter()
                .zip(entries_of_a_list(&metrics, &run_groups, margin))
        {
            if entry {
                entries.insert(group[0]);
            }
        }
        groups.extend(
            run_groups
                .into_iter()
                .map(|group| (group[0], Group::Lines(group))),
        );
    }
    groups.sort_by_key(|(first, _)| *first);
    join_narrow_columns(
        &mut groups,
        &metrics,
        &lines,
        &runs,
        &run_shapes,
        rules,
        &entries,
    );

    // Numbered AFTER the blocks that hold nothing are gone. Numbering first left
    // a gap in the reading order wherever a group turned out to be whitespace,
    // and `PdfDocument::extract_blocks` starts each page at the count it has --
    // so the gap on one page became a REPEATED index on the next (codex,
    // round 2).
    let mut blocks: Vec<Block> = Vec::new();
    for (_, group) in groups {
        match group {
            Group::Lines(group) if !group.is_empty() => {
                let block_lines: Vec<&TextBlock> = group.iter().map(|i| lines[*i]).collect();
                let block_metrics: Vec<&LineMetrics> = group.iter().map(|i| &metrics[*i]).collect();
                let item = block_metrics.first().and_then(|m| m.item);
                let mut block = assemble(page, 0, &block_lines, &block_metrics, item);
                if item.is_none() && entries.contains(&group[0]) {
                    block.kind = BlockKind::ListItem {
                        level: 0,
                        ordinal: None,
                    };
                }
                blocks.push(block);
            }
            Group::Lines(_) => {}
            Group::Table(table) => {
                for cell in &table.cells {
                    // The same record on every cell of the table: see `GridDoubt`.
                    blocks.push(assemble_cell(
                        page,
                        &lines,
                        cell,
                        table.doubt,
                        table.origin,
                        cell_path_words,
                    ));
                }
            }
        }
    }
    blocks.retain(|block| !block.text.trim().is_empty());
    for (n, block) in blocks.iter_mut().enumerate() {
        block.index = first_index + n;
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_options_defaults_pin_every_new_field() {
        let options = ExtractOptions::default();
        assert_eq!(options.ocr, OcrMode::Never);
        assert_eq!(options.ocr_language, "eng");
        assert_eq!(options.ocr_dpi, 150);
        assert!(matches!(options.quality, Quality::Fast));
        assert_eq!(options.model_dir, None);
        assert!(!options.keep_furniture);
        assert!(options.heading_overrides.is_empty());
        assert!(!options.include_hidden);
        assert!(!options.raw_order);
        assert!(matches!(options.table_strategy, TableStrategy::Both));
        assert!(matches!(options.table_render_mode, TableRenderMode::Auto));
    }

    #[test]
    fn extract_options_still_implements_eq() {
        fn assert_is_eq<T: Eq>() {}
        assert_is_eq::<ExtractOptions>();
        assert_is_eq::<HeadingOverride>();
    }

    #[test]
    fn extract_options_stays_eq() {
        // A caller may compare or key on the options: the derive above is
        // public API, and this fails to compile the day a field breaks it.
        fn assert_impl_eq<T: Eq>() {}
        assert_impl_eq::<ExtractOptions>();
    }

    fn table_cell(row: u16, column: u16, text: &str) -> Block {
        Block {
            kind: BlockKind::TableCell {
                row,
                column,
                row_span: 1,
                column_span: if text.len() > 30 { 3 } else { 1 },
                header: row == 0,
            },
            page: 0,
            bbox: [
                72.0 + 100.0 * f64::from(column),
                700.0 - 12.0 * f64::from(row),
                160.0,
                709.0,
            ],
            text: text.into(),
            spans: Vec::new(),
            index: 0,
            continues_previous: false,
            confidence: None,
            grid: Some(GridDoubt::default()),
            table_origin: TableOrigin::Other,
            trace_learned: None,
        }
    }

    /// A body of three rows and three columns, with `over` as a lone cell on
    /// the row above it and `under` on the row below.
    fn table_with(over: Option<&str>, under: Option<&str>) -> Vec<Block> {
        let mut blocks = Vec::new();
        let mut row = 0;
        if let Some(text) = over {
            blocks.push(table_cell(row, 0, text));
            row += 1;
        }
        for (label, a, b) in [
            ("Macrophages", "49.5", "50.5"),
            ("Granulocytes", "17.7", "15.5"),
            ("T lymphocytes", "5.0", "6.7"),
        ] {
            blocks.push(table_cell(row, 0, label));
            blocks.push(table_cell(row, 1, a));
            blocks.push(table_cell(row, 2, b));
            row += 1;
        }
        if let Some(text) = under {
            blocks.push(table_cell(row, 0, text));
        }
        blocks
    }

    fn cells(blocks: &[Block]) -> usize {
        blocks
            .iter()
            .filter(|b| matches!(b.kind, BlockKind::TableCell { .. }))
            .count()
    }

    #[test]
    fn extract_options_keeps_its_eq() {
        // `ExtractOptions` is public API a caller may compare or key on; the
        // bare float array cost it `Eq`, and the newtype below gives it back.
        // Compile-time: removing `Eq` fails this test's build, not a run.
        fn assert_eq<T: Eq>() {}
        assert_eq::<ExtractOptions>();
    }

    #[test]
    fn tatr_options_default_to_four_threads() {
        // P4a: 8 threads is slower and fails the p95 table budget, 4 passes;
        // a caller that does not set the knob gets the passing value.
        assert_eq!(TatrOptions::default().intra_threads, 4);
    }

    #[test]
    fn clip_margins_rejects_a_negative_side() {
        assert!(ClipMargins::new(0.0, 0.0, 0.0, 0.0).is_ok());
        assert!(ClipMargins::new(10.0, 20.0, 30.0, 40.0).is_ok());
        for (side, margins) in [
            ("left", (-1.0, 0.0, 0.0, 0.0)),
            ("top", (0.0, -1.0, 0.0, 0.0)),
            ("right", (0.0, 0.0, -1.0, 0.0)),
            ("bottom", (0.0, 0.0, 0.0, -0.5)),
        ] {
            let (left, top, right, bottom) = margins;
            let value = [left, top, right, bottom]
                .into_iter()
                .find(|v| *v < 0.0)
                .expect("one negative side");
            assert_eq!(
                ClipMargins::new(left, top, right, bottom),
                Err(OptionsError::InvalidClipMargins { side, value }),
                "{side} rejects its negative side",
            );
        }
    }

    #[test]
    fn clip_margins_rejects_what_is_not_finite() {
        for (side, index) in [("left", 0), ("top", 1), ("right", 2), ("bottom", 3)] {
            for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                let mut margins = [0.0; 4];
                margins[index] = value;
                let [left, top, right, bottom] = margins;
                assert_eq!(
                    ClipMargins::new(left, top, right, bottom),
                    Err(OptionsError::InvalidClipMargins { side, value }),
                    "{side} rejects {value:?}",
                );
            }
        }
    }

    #[test]
    fn clip_margins_round_trips_through_its_accessor() {
        let margins = ClipMargins::new(10.0, 20.0, 30.0, 40.0).expect("valid");
        assert_eq!(margins.as_array(), [10.0, 20.0, 30.0, 40.0]);
    }

    #[test]
    fn clip_margins_compares_float_equal_margins_equal() {
        let one = ClipMargins::new(1.0, 2.0, 3.0, 4.0).expect("valid");
        let same = ClipMargins::new(1.0, 2.0, 3.0, 4.0).expect("valid");
        assert_eq!(one, same);
        // `-0.0` is normalised to `0.0` at construction, so margins that
        // compare equal as floats are `Eq`-equal. Non-finite values never get
        // this far -- the constructor refuses them -- so every value here is
        // reflexive and `Eq` holds.
        let neg_zero = ClipMargins::new(-0.0, 0.0, 0.0, 0.0).expect("valid");
        let pos_zero = ClipMargins::new(0.0, 0.0, 0.0, 0.0).expect("valid");
        assert_eq!(neg_zero, pos_zero);
        // Bit-exact: `-0.0 == 0.0` as floats, so only the bits say the
        // normalisation happened.
        assert_eq!(neg_zero.as_array().map(f64::to_bits), [0.0f64.to_bits(); 4]);
        let mut seen = std::collections::HashSet::new();
        seen.insert(one);
        assert!(seen.contains(&same));
        seen.insert(neg_zero);
        assert!(seen.contains(&pos_zero));
    }

    #[test]
    fn clip_margins_error_names_the_side() {
        let err = ClipMargins::new(-1.0, 0.0, 0.0, 0.0).expect_err("negative");
        assert_eq!(
            err.to_string(),
            "invalid clip margins: left must be finite and at least 0, got -1"
        );
        // `Display` plus `std::error::Error`, so bindings can map it.
        let _: &dyn std::error::Error = &err;
    }

    fn hidden_span(text: &str) -> TextSpan {
        TextSpan {
            text: text.to_string(),
            render_mode: Some(3),
            ..Default::default()
        }
    }

    fn visible_span(text: &str) -> TextSpan {
        TextSpan {
            text: text.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn a_spanless_figure_does_not_mark_its_page_visible() {
        // A scanned page: its whole text layer hides in render mode 3 over
        // its own image, and the figure marker beside it carries no spans.
        // The marker must not vote the page visible, or the drop below takes
        // every word on the page.
        let figure = Block::new(
            BlockKind::Figure,
            0,
            [0.0, 0.0, 10.0, 10.0],
            String::new(),
            0,
        );
        let mut words = Block::new(
            BlockKind::Paragraph,
            0,
            [0.0, 0.0, 10.0, 10.0],
            "words".to_string(),
            1,
        );
        words.spans = vec![hidden_span("words")];
        let mut blocks = vec![figure, words];
        drop_hidden_text(&mut blocks);
        assert_eq!(blocks.len(), 2, "the hidden words went with the figure");
        assert_eq!(blocks[1].text, "words");
    }

    #[test]
    fn visible_text_still_drops_hidden_spans() {
        // The other direction: where the page draws visible text, hidden
        // spans go -- the test above is not a drop that never fires.
        let mut block = Block::new(
            BlockKind::Paragraph,
            0,
            [0.0, 0.0, 10.0, 10.0],
            "seen gone".to_string(),
            0,
        );
        block.spans = vec![visible_span("seen"), hidden_span("gone")];
        let mut blocks = vec![block];
        drop_hidden_text(&mut blocks);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].spans.len(), 1);
        assert_eq!(blocks[0].text, "seen");
    }

    #[test]
    fn a_caption_and_a_note_are_taken_out_of_the_table() {
        let mut blocks = table_with(
            Some("Table 1 Cellular composition of cells infiltrating synovial tissue"),
            Some("Values are percentages (mean \u{b1} standard deviation) of positive cells."),
        );
        peel_table_edges(&mut blocks, false);
        assert_eq!(
            blocks[0].kind,
            BlockKind::Paragraph,
            "the caption stayed a cell"
        );
        assert!(
            blocks[0].grid.is_none(),
            "a peeled caption kept its table's evidence"
        );
        assert_eq!(
            blocks.last().map(|b| b.kind),
            Some(BlockKind::Paragraph),
            "the note stayed a cell"
        );
        assert_eq!(cells(&blocks), 9);
        assert!(
            matches!(
                blocks[1].kind,
                BlockKind::TableCell {
                    row: 0,
                    column: 0,
                    ..
                }
            ),
            "the table's rows were not counted from its new first row: {:?}",
            blocks[1].kind
        );
    }

    #[test]
    fn a_spanning_head_and_a_crystal_head_stay_in_the_table() {
        for head in [
            "Male Female",
            "x y z U iso*/ U eq",
            "Diplotype No. rs12979860 rs738409",
            // `table` inside a word is no caption (codex, #2048).
            "Stable isotope composition",
        ] {
            let mut blocks = table_with(Some(head), None);
            peel_table_edges(&mut blocks, false);
            assert_eq!(cells(&blocks), 10, "`{head}` was taken out of the table");
            // And under it: a row of symbols is not a sentence, however many
            // of them there are.
            // It may join the cell over it, where it reads as running on
            // from it; it stays in the table either way.
            let mut blocks = table_with(None, Some(head));
            peel_table_edges(&mut blocks, false);
            assert!(
                blocks
                    .iter()
                    .all(|b| { matches!(b.kind, BlockKind::TableCell { .. }) })
                    && blocks.iter().any(|b| b.text.contains(head)),
                "`{head}` under the table was taken out"
            );
        }
    }

    /// A table set over a page break opens with the tail of the cell the page
    /// before began; read as a sentence it would be taken out.
    #[test]
    fn a_sentence_over_the_table_stays_in_it() {
        let mut blocks = table_with(
            Some("suitable for clinical trials for regulatory purposes and for the review."),
            None,
        );
        peel_table_edges(&mut blocks, false);
        assert_eq!(cells(&blocks), 10);
    }

    /// A table's rows beside a column of the page's running text: `(label,
    /// figure, line of prose)` on every row.
    fn table_beside_prose(prose: &[&str]) -> Vec<Block> {
        let mut blocks = Vec::new();
        for (row, line) in prose.iter().enumerate() {
            let row = row as u16;
            let mut label = table_cell(row, 0, "Campbelltown");
            label.bbox = [
                72.0,
                700.0 - 12.0 * f64::from(row),
                150.0,
                709.0 - 12.0 * f64::from(row),
            ];
            let mut figure = table_cell(row, 1, "1,000");
            figure.bbox = [
                200.0,
                700.0 - 12.0 * f64::from(row),
                230.0,
                709.0 - 12.0 * f64::from(row),
            ];
            let mut text = table_cell(row, 2, line);
            text.kind = BlockKind::TableCell {
                row,
                column: 2,
                row_span: 1,
                column_span: 1,
                header: row == 0,
            };
            // A paragraph break where the page left a blank line.
            let drop = if row >= 3 { 12.0 } else { 0.0 };
            text.bbox = [
                320.0,
                700.0 - 12.0 * f64::from(row) - drop,
                460.0,
                709.0 - 12.0 * f64::from(row) - drop,
            ];
            blocks.extend([label, figure, text]);
        }
        blocks
    }

    #[test]
    fn a_column_of_running_text_leaves_the_table() {
        let mut blocks = table_beside_prose(&[
            "During the year considerable effort was given to restocking",
            "our Residential Estates land portfolio across the country to",
            "meet our future growth requirements and our objectives.",
            "We successfully acquired future lots and we now control more",
            "residential allotments for development in the coming years,",
            "most of them in strategic growth areas within the states that",
            "are expected to lead demand for housing over the next decade.",
        ]);
        peel_table_edges(&mut blocks, false);
        let paragraphs: Vec<&str> = blocks
            .iter()
            .filter(|b| b.kind == BlockKind::Paragraph)
            .map(|b| b.text.as_str())
            .collect();
        assert_eq!(cells(&blocks), 14, "the running text stayed in the table");
        assert_eq!(paragraphs.len(), 2, "not two paragraphs: {paragraphs:?}");
        assert!(
            paragraphs[0].starts_with("During the year") && paragraphs[0].ends_with("objectives.")
        );
        assert!(
            blocks.iter().position(|b| b.kind == BlockKind::Paragraph)
                > blocks
                    .iter()
                    .rposition(|b| matches!(b.kind, BlockKind::TableCell { .. })),
            "the text was not set after the table"
        );
    }

    /// A description that wraps inside its own cell carries on on a row the
    /// other columns leave empty, and stays in the table.
    #[test]
    fn a_wrapping_description_column_stays_in_the_table() {
        let mut blocks = Vec::new();
        for (row, (date, text)) in [
            (
                "18 December 2002",
                "Authorised the directors to approve the purchase of shares",
            ),
            (
                "",
                "in issue on that date by the company or any of its subsidiaries",
            ),
            (
                "18 December 2002",
                "Amended the articles of association in terms of which the maximum",
            ),
            (
                "",
                "number of directors allowed was increased from thirteen to fifteen.",
            ),
            (
                "18 December 2002",
                "Amended the articles of association to ensure that one-third",
            ),
            (
                "",
                "of the directors retire from office at every annual general meeting.",
            ),
            ("20 March 2003", "Approved the annual financial statements"),
        ]
        .iter()
        .enumerate()
        {
            if !date.is_empty() {
                blocks.push(table_cell(row as u16, 0, date));
            }
            let mut cell = table_cell(row as u16, 1, text);
            cell.kind = BlockKind::TableCell {
                row: row as u16,
                column: 1,
                row_span: 1,
                column_span: 1,
                header: row == 0,
            };
            blocks.push(cell);
        }
        peel_table_edges(&mut blocks, false);
        assert!(
            blocks
                .iter()
                .all(|b| matches!(b.kind, BlockKind::TableCell { .. })),
            "a wrapping description column left the table"
        );
        // And each description is one cell again, its line run on joined.
        assert!(
            blocks.iter().any(|b| b.text
                == "Amended the articles of association in terms of which the maximum \
                    number of directors allowed was increased from thirteen to fifteen."),
            "{:?}",
            blocks.iter().map(|b| &b.text).collect::<Vec<_>>()
        );
    }

    fn span_at(text: &str, x: f64, width: f64) -> TextSpan {
        TextSpan {
            text: text.into(),
            x,
            width,
            font_size: 9.0,
            height: 9.0,
            ..TextSpan::default()
        }
    }

    /// A one-column table whose cells hold `runs` (text, x, width), one row a
    /// cell.
    fn one_column(rows: &[&[(&str, f64, f64)]]) -> Vec<Block> {
        rows.iter()
            .enumerate()
            .map(|(row, runs)| {
                let mut cell = table_cell(row as u16, 0, "");
                cell.text = runs.iter().map(|r| r.0).collect::<Vec<_>>().join(" ");
                cell.spans = runs.iter().map(|&(t, x, w)| span_at(t, x, w)).collect();
                cell
            })
            .collect()
    }

    #[test]
    fn a_table_read_as_one_column_gets_its_columns_back() {
        let mut blocks = one_column(&[
            &[("AUC score", 57.0, 40.0)],
            &[("CSF t-tau", 57.0, 29.0), ("0.616", 264.0, 17.0)],
            &[("CSF A\u{3b2}42", 57.0, 29.0), ("0.612", 264.0, 17.0)],
            &[("CSF p-tau", 57.0, 31.0), ("0.628", 264.0, 17.0)],
            &[("Biomarkers-15", 57.0, 45.0), ("0.830", 264.0, 17.0)],
            &[(
                "Note that the AUC score for Biosignature-15 reported here",
                57.0,
                260.0,
            )],
        ]);
        peel_table_edges(&mut blocks, false);
        let at = |text: &str| {
            blocks
                .iter()
                .find(|b| b.text == text)
                .map(|b| b.kind)
                .unwrap_or(BlockKind::Paragraph)
        };
        assert!(
            matches!(at("0.616"), BlockKind::TableCell { column: 1, .. }),
            "the figures did not get a column of their own: {:?}",
            blocks.iter().map(|b| (&b.text, b.kind)).collect::<Vec<_>>()
        );
        assert_eq!(
            blocks.last().map(|b| b.kind),
            Some(BlockKind::Paragraph),
            "the note that closed the white stayed in the table"
        );
    }

    /// The cells a re-grid makes take their places in the reading order, and
    /// every block after them moves with them (codex, #2051).
    #[test]
    fn a_regridded_table_keeps_the_indices_in_reading_order() {
        let mut blocks = one_column(&[
            &[("CSF t-tau", 57.0, 29.0), ("0.616", 264.0, 17.0)],
            &[("CSF A\u{3b2}42", 57.0, 29.0), ("0.612", 264.0, 17.0)],
            &[("CSF p-tau", 57.0, 31.0), ("0.628", 264.0, 17.0)],
            &[("Biomarkers-15", 57.0, 45.0), ("0.830", 264.0, 17.0)],
        ]);
        let mut after = table_cell(0, 0, "The paragraph after the table.");
        after.kind = BlockKind::Paragraph;
        blocks.push(after);
        for (n, b) in blocks.iter_mut().enumerate() {
            b.index = n;
        }
        peel_table_edges(&mut blocks, false);
        let indices: Vec<usize> = blocks.iter().map(|b| b.index).collect();
        assert_eq!(indices, (0..blocks.len()).collect::<Vec<_>>());
    }

    /// Spans with no white between them are one word in the rebuilt cell.
    #[test]
    fn a_regridded_cell_keeps_a_word_drawn_in_two_spans() {
        let mut blocks = one_column(&[
            &[("CSF t-tau", 57.0, 29.0), ("0.616", 264.0, 17.0)],
            &[
                ("CSF p-", 57.0, 24.0),
                ("tau", 81.0, 10.0),
                ("0.628", 264.0, 17.0),
            ],
            &[("CSF A\u{3b2}42", 57.0, 29.0), ("0.612", 264.0, 17.0)],
            &[("Biomarkers-15", 57.0, 45.0), ("0.830", 264.0, 17.0)],
        ]);
        peel_table_edges(&mut blocks, false);
        assert!(
            blocks.iter().any(|b| b.text == "CSF p-tau"),
            "{:?}",
            blocks.iter().map(|b| &b.text).collect::<Vec<_>>()
        );
    }

    /// Two runs of a row that fall to one column are one cell, not a second
    /// table starting.
    #[test]
    fn two_runs_in_one_column_are_one_cell() {
        let mut blocks = one_column(&[
            &[("Label one", 57.0, 30.0), ("1.0", 200.0, 12.0)],
            &[("Label two", 57.0, 30.0), ("2.0", 200.0, 12.0)],
            &[("Label three", 57.0, 34.0), ("3.0", 200.0, 12.0)],
            &[
                ("Label", 57.0, 20.0),
                ("four", 110.0, 16.0),
                ("4.0", 200.0, 12.0),
            ],
        ]);
        peel_table_edges(&mut blocks, false);
        let end = crate::markdown::table_run(&blocks, 0);
        assert_eq!(end, Some(blocks.len()), "the table came apart into two");
    }

    /// Prose read as a table of one column has no runs at shared places, and
    /// is left as it was.
    #[test]
    fn one_column_of_ragged_runs_is_left_alone() {
        let mut blocks = one_column(&[
            &[
                ("The committee met", 57.0, 70.0),
                ("on Monday", 180.0, 40.0),
            ],
            &[("and resolved", 57.0, 50.0), ("that the", 240.0, 30.0)],
            &[("officers report", 57.0, 60.0), ("before the", 300.0, 40.0)],
            &[("next meeting", 57.0, 50.0), ("of the board", 150.0, 40.0)],
        ]);
        peel_table_edges(&mut blocks, false);
        assert!(blocks
            .iter()
            .all(|b| matches!(b.kind, BlockKind::TableCell { column: 0, .. })));
    }

    /// Cells at `(row, column, text)`, one column wide each.
    fn grid(cells: &[(u16, u16, &str)]) -> Vec<Block> {
        cells
            .iter()
            .map(|&(row, column, text)| {
                let mut cell = table_cell(row, column, text);
                cell.kind = BlockKind::TableCell {
                    row,
                    column,
                    row_span: 1,
                    column_span: 1,
                    header: row == 0,
                };
                cell
            })
            .collect()
    }

    /// A bold caption the grid cut at the table's own gutters comes out whole
    /// (PMC1557522 on PubTables train, after #623 read its weight).
    #[test]
    fn a_caption_across_the_head_row_comes_out_whole() {
        let mut blocks = grid(&[
            (0, 0, "Table 1: Comparison methods"),
            (0, 1, "of the"),
            (0, 2, "logP values calculated"),
            (0, 3, "by several"),
            (1, 0, "Program"),
            (1, 1, "colchicine"),
            (1, 2, "paclitaxel"),
            (1, 3, "colchitaxel"),
            (2, 0, "ALOGP"),
            (2, 1, "1.59"),
            (2, 2, "3.20"),
            (2, 3, "4.64"),
            (3, 0, "CLOGP"),
            (3, 1, "1.20"),
            (3, 2, "4.95"),
            (3, 3, "6.50"),
        ]);
        peel_table_edges(&mut blocks, false);
        assert_eq!(blocks[0].kind, BlockKind::Paragraph);
        assert_eq!(
            blocks[0].text,
            "Table 1: Comparison methods of the logP values calculated by several"
        );
        assert!(
            matches!(
                blocks[1].kind,
                BlockKind::TableCell {
                    row: 0,
                    column: 0,
                    ..
                }
            ),
            "the table does not start at its own head: {:?}",
            blocks[1].kind
        );
        assert_eq!(cells(&blocks), 12);
    }

    /// Beside column heads only the caption cell is the caption.
    #[test]
    fn a_caption_beside_column_heads_leaves_the_heads() {
        let mut blocks = grid(&[
            (0, 0, "Table 1 Descriptive statistics for publication rate"),
            (0, 1, "Statistics"),
            (0, 2, "PR P1"),
            (0, 3, "PR P2"),
            (1, 1, "Mean"),
            (1, 2, "0.41"),
            (1, 3, "0.52"),
            (2, 1, "SD"),
            (2, 2, "0.12"),
            (2, 3, "0.18"),
        ]);
        peel_table_edges(&mut blocks, false);
        assert_eq!(blocks[0].kind, BlockKind::Paragraph);
        assert!(blocks.iter().any(
            |b| b.text == "Statistics" && matches!(b.kind, BlockKind::TableCell { row: 0, .. })
        ));
    }

    /// A running head with its page number is no table of one row; a contents
    /// line on its dot leaders is.
    #[test]
    fn a_table_of_one_row_is_a_line_unless_it_runs_on_leaders() {
        let mut blocks = grid(&[
            (
                0,
                0,
                "New records of the endemic Sicilian land snail species",
            ),
            (0, 1, "137"),
        ]);
        peel_table_edges(&mut blocks, false);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].kind, BlockKind::Paragraph);

        let mut blocks = grid(&[
            (0, 0, "Letter from the Chairman . . . . . . . . 2"),
            (0, 1, "Feature Client Stories . . . . . . . . 6"),
            (0, 2, "Client Innovation Highlights . . . . 14"),
        ]);
        peel_table_edges(&mut blocks, false);
        assert_eq!(cells(&blocks), 3, "a contents line lost its table");
    }

    /// `Table size` over lower-case heads is a head row, not a caption
    /// (codex, #2055).
    #[test]
    fn a_head_that_opens_with_the_word_table_is_no_caption() {
        let mut blocks = grid(&[
            (0, 0, "Table size"),
            (0, 1, "rows"),
            (0, 2, "columns"),
            (1, 0, "Small"),
            (1, 1, "12"),
            (1, 2, "4"),
            (2, 0, "Large"),
            (2, 1, "480"),
            (2, 2, "16"),
        ]);
        peel_table_edges(&mut blocks, false);
        assert_eq!(cells(&blocks), 9, "the head row was taken for a caption");
        assert!(is_numbered_caption("Table IV. Results"));
        assert!(is_numbered_caption("Table S2 Primers"));
        assert!(!is_numbered_caption("Table value"));
    }

    /// A table the page broke may leave one row on the next page; that row is
    /// the table's, for `tables_across_pages` to join (codex, #2055).
    #[test]
    fn one_row_left_by_a_page_break_stays_a_table() {
        let mut blocks = grid(&[
            (0, 0, "Revenue"),
            (0, 1, "13,110"),
            (1, 0, "Cost of sales"),
            (1, 1, "(8,406)"),
            (2, 0, "Gross profit"),
            (2, 1, "4,704"),
        ]);
        let mut rest = grid(&[(0, 0, "Operating profit"), (0, 1, "1,559")]);
        for b in rest.iter_mut() {
            b.page = 1;
        }
        blocks.extend(rest);
        peel_table_edges(&mut blocks, false);
        assert_eq!(cells(&blocks), 8, "the page's one row became a line");
    }

    /// A one-row grid after a page break with columns of its own is not the
    /// half of the table before it, and reads as the line it is (codex,
    /// #2058).
    #[test]
    fn one_row_with_columns_of_its_own_after_a_break_is_a_line() {
        let mut blocks = grid(&[
            (0, 0, "Revenue"),
            (0, 1, "13,110"),
            (1, 0, "Cost of sales"),
            (1, 1, "(8,406)"),
            (2, 0, "Gross profit"),
            (2, 1, "4,704"),
        ]);
        let mut head = grid(&[
            (0, 0, "New records of the endemic snail species"),
            (0, 1, "137"),
        ]);
        for (k, b) in head.iter_mut().enumerate() {
            b.page = 1;
            // Two columns, but standing where the table above has none.
            b.bbox[0] = [300.0, 540.0][k];
        }
        blocks.extend(head);
        peel_table_edges(&mut blocks, false);
        assert_eq!(
            cells(&blocks),
            6,
            "an unrelated one-row grid stayed a table"
        );
    }

    /// The row a page break left at the foot of a page stays a table when
    /// the rest, on the next page, was read as one column: its columns are
    /// the ones it gets back (codex, #2065).
    #[test]
    fn one_row_before_a_table_read_as_one_column_stays_a_table() {
        let mut lead = table_cell(0, 0, "Notes to the statements.");
        lead.kind = BlockKind::Paragraph;
        let mut blocks = vec![lead];
        let mut row = grid(&[(0, 0, "Revenue"), (0, 1, "13,110")]);
        row[0].bbox[0] = 57.0;
        row[1].bbox[0] = 264.0;
        blocks.extend(row);
        let mut rest = one_column(&[
            &[("Cost of sales", 57.0, 50.0), ("(8,406)", 264.0, 30.0)],
            &[("Gross profit", 57.0, 45.0), ("4,704", 264.0, 20.0)],
            &[("Selling expenses", 57.0, 60.0), ("(3,145)", 264.0, 30.0)],
            &[("Operating profit", 57.0, 60.0), ("1,559", 264.0, 20.0)],
        ]);
        for b in rest.iter_mut() {
            b.page = 1;
        }
        blocks.extend(rest);
        peel_table_edges(&mut blocks, false);
        assert!(
            blocks
                .iter()
                .any(|b| b.text == "Revenue" && matches!(b.kind, BlockKind::TableCell { .. })),
            "the page's last row became a line: {:?}",
            blocks.iter().map(|b| (&b.text, b.kind)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_financial_row_label_under_the_table_stays_in_it() {
        let mut blocks = table_with(
            None,
            Some("Prima facie income tax (benefit)/expense calculated at 30%"),
        );
        peel_table_edges(&mut blocks, false);
        assert_eq!(cells(&blocks), 10);
    }

    /// A line of text at a position, in a single span, as the extractor would
    /// have produced it.
    fn line(text: &str, x: f64, y: f64, size: f64) -> TextBlock {
        TextBlock {
            spans: vec![span(text, x, y, size)],
        }
    }

    fn span(text: &str, x: f64, y: f64, size: f64) -> TextSpan {
        TextSpan {
            text: text.to_string(),
            x,
            y,
            width: size * 0.5 * text.chars().count() as f64,
            height: size,
            font_size: size,
            ..Default::default()
        }
    }

    fn texts(blocks: &[Block]) -> Vec<String> {
        blocks.iter().map(|b| b.text.clone()).collect()
    }

    /// A paragraph block of one-span lines, as the builder would have
    /// assembled it: `(text, x, y)` per line at `size`.
    fn para_block(lines: &[(&str, f64, f64)], size: f64) -> Block {
        let spans: Vec<TextSpan> = lines
            .iter()
            .map(|(text, x, y)| span(text, *x, *y, size))
            .collect();
        let mut block = Block {
            kind: BlockKind::Paragraph,
            page: 0,
            bbox: [0.0; 4],
            text: join_lines(
                &lines
                    .iter()
                    .map(|(text, _, _)| text.to_string())
                    .collect::<Vec<_>>(),
            ),
            spans,
            index: 0,
            continues_previous: false,
            confidence: None,
            grid: None,
            table_origin: TableOrigin::Other,
            trace_learned: None,
        };
        recompute_bbox(&mut block);
        block
    }

    /// Six lines at a twelve-point pitch with a twenty-four-point step in the
    /// middle: the control every step-split test below carries, so each goes
    /// red with one block where it asks for two when the rule is out.
    fn merged_pair() -> Block {
        para_block(
            &[
                ("the first paragraph opens here and runs", 72.0, 700.0),
                ("the whole measure to its second line,", 72.0, 688.0),
                ("and closes on its third.", 72.0, 676.0),
                ("the second paragraph opens lower down", 72.0, 652.0),
                ("and runs the measure to its own end,", 72.0, 640.0),
                ("closing here.", 72.0, 628.0),
            ],
            10.0,
        )
    }

    #[test]
    fn a_paragraph_sized_gap_inside_a_block_cuts_it() {
        let mut blocks = vec![merged_pair()];
        split_paragraphs_on_steps(&mut blocks);
        assert_eq!(
            texts(&blocks),
            vec![
                "the first paragraph opens here and runs the whole measure to its second line, and closes on its third.",
                "the second paragraph opens lower down and runs the measure to its own end, closing here.",
            ]
        );
        assert!(blocks
            .iter()
            .all(|b| matches!(b.kind, BlockKind::Paragraph)));
        assert_eq!(blocks[0].spans.len(), 3);
        assert_eq!(blocks[1].spans.len(), 3);
        assert!(blocks[0].bbox[1] > blocks[1].bbox[3]);
    }

    #[test]
    fn an_evenly_double_spaced_paragraph_is_not_cut() {
        let even = para_block(
            &[
                ("a double-spaced line one", 72.0, 700.0),
                ("a double-spaced line two", 72.0, 676.0),
                ("a double-spaced line three", 72.0, 652.0),
                ("a double-spaced line four", 72.0, 628.0),
                ("a double-spaced line five", 72.0, 604.0),
                ("a double-spaced line six", 72.0, 580.0),
            ],
            10.0,
        );
        let mut blocks = vec![merged_pair(), even];
        split_paragraphs_on_steps(&mut blocks);
        assert_eq!(blocks.len(), 3, "{:?}", texts(&blocks));
        assert!(blocks[2].text.starts_with("a double-spaced line one"));
        assert!(blocks[2].text.ends_with("line six"));
        assert_eq!(blocks[2].spans.len(), 6);
    }

    #[test]
    fn a_column_change_is_not_this_rules_business() {
        let shifted = para_block(
            &[
                ("the left column ends here and runs", 72.0, 700.0),
                ("the whole measure to its own end.", 72.0, 688.0),
                ("it closes on this third line.", 72.0, 676.0),
                ("the right column opens lower down", 300.0, 652.0),
                ("and runs to its own second line.", 300.0, 640.0),
                ("closing here at last.", 300.0, 628.0),
            ],
            10.0,
        );
        let mut blocks = vec![merged_pair(), shifted];
        split_paragraphs_on_steps(&mut blocks);
        assert_eq!(blocks.len(), 3, "{:?}", texts(&blocks));
        assert_eq!(blocks[2].spans.len(), 6);
    }

    #[test]
    fn headings_and_list_items_are_untouched() {
        let mut heading = merged_pair();
        heading.kind = BlockKind::Heading { level: 1 };
        let mut item = merged_pair();
        item.kind = BlockKind::ListItem {
            level: 0,
            ordinal: None,
        };
        let heading_text = heading.text.clone();
        let mut blocks = vec![merged_pair(), heading, item];
        split_paragraphs_on_steps(&mut blocks);
        assert_eq!(blocks.len(), 4, "{:?}", texts(&blocks));
        assert_eq!(blocks[2].text, heading_text);
        assert!(matches!(blocks[2].kind, BlockKind::Heading { level: 1 }));
        assert!(matches!(
            blocks[3].kind,
            BlockKind::ListItem { level: 0, .. }
        ));
        assert_eq!(blocks[3].spans.len(), 6);
    }

    #[test]
    fn a_vertical_word_is_not_cut_into_salad() {
        let word = para_block(
            &[
                ("T", 72.0, 700.0),
                ("h", 72.0, 688.0),
                ("i", 72.0, 676.0),
                ("c", 72.0, 664.0),
                ("k", 72.0, 652.0),
                ("n", 72.0, 628.0),
                ("e", 72.0, 616.0),
                ("s", 72.0, 604.0),
                ("s", 72.0, 592.0),
            ],
            10.0,
        );
        let text = word.text.clone();
        let mut blocks = vec![merged_pair(), word];
        split_paragraphs_on_steps(&mut blocks);
        assert_eq!(blocks.len(), 3, "{:?}", texts(&blocks));
        assert_eq!(blocks[2].text, text);
        assert_eq!(blocks[2].spans.len(), 9);
    }

    #[test]
    fn a_rotated_stamp_is_not_cut() {
        let mut stamp = merged_pair();
        for span in &mut stamp.spans {
            let size = span.font_size;
            span.transform = Some([0.0, size, -size, 0.0, span.x, span.y]);
        }
        let text = stamp.text.clone();
        let mut blocks = vec![merged_pair(), stamp];
        split_paragraphs_on_steps(&mut blocks);
        assert_eq!(blocks.len(), 3, "{:?}", texts(&blocks));
        assert_eq!(blocks[2].text, text);
        assert_eq!(blocks[2].spans.len(), 6);
    }

    #[test]
    fn a_number_block_is_not_split() {
        let figure = "1,234.5 (678) 12.3%";
        let mut rows: Vec<(&str, f64, f64)> = Vec::new();
        let mut y = 700.0;
        for row in 0..20 {
            rows.push((figure, 72.0, y));
            y -= if row % 5 == 4 { 24.0 } else { 12.0 };
        }
        let figures = para_block(&rows, 10.0);
        let text = figures.text.clone();
        let mut blocks = vec![merged_pair(), figures];
        split_paragraphs_on_steps(&mut blocks);
        assert_eq!(blocks.len(), 3, "{:?}", texts(&blocks));
        assert_eq!(blocks[2].text, text);
        assert_eq!(blocks[2].spans.len(), 20);
    }

    #[test]
    fn prose_cuts_still_happen() {
        let parent = merged_pair();
        let mut blocks = vec![parent.clone()];
        split_paragraphs_on_steps(&mut blocks);
        assert_eq!(blocks.len(), 2, "{:?}", texts(&blocks));
        let plain = |text: &str| {
            text.replace("- ", "")
                .chars()
                .filter(|c| !c.is_whitespace())
                .collect::<Vec<_>>()
        };
        let mut got: Vec<char> = Vec::new();
        for piece in &blocks {
            got.extend(plain(&piece.text));
        }
        assert_eq!(got, plain(&parent.text));
    }

    #[test]
    fn a_reversed_reassembly_refuses_the_whole_split() {
        let label = para_block(
            &[
                ("aaa", 72.0, 100.0),
                ("bbb", 72.0, 112.0),
                ("ccc", 72.0, 124.0),
                ("ddd", 72.0, 148.0),
            ],
            10.0,
        );
        let text = label.text.clone();
        let mut blocks = vec![merged_pair(), label];
        split_paragraphs_on_steps(&mut blocks);
        assert_eq!(blocks.len(), 3, "{:?}", texts(&blocks));
        assert_eq!(blocks[2].text, text);
        assert_eq!(blocks[2].spans.len(), 4);
    }

    #[test]
    fn consecutive_lines_of_one_measure_become_one_paragraph() {
        let lines = vec![
            line("the first line of the paragraph runs", 72.0, 700.0, 10.0),
            line("the whole measure and the second one", 72.0, 688.0, 10.0),
            line("stops short of it.", 72.0, 676.0, 10.0),
        ];
        let blocks = paragraphs(
            0,
            0,
            std::slice::from_ref(&lines),
            &[],
            &[],
            12.0,
            TableStrategy::Both,
        );
        assert_eq!(
            texts(&blocks),
            vec![
                "the first line of the paragraph runs the whole measure and the second one stops short of it."
            ]
        );
    }

    #[test]
    fn extra_leading_starts_a_new_paragraph() {
        let lines = vec![
            line("the first paragraph ends here and", 72.0, 700.0, 10.0),
            line("carries on to a second line.", 72.0, 688.0, 10.0),
            line("the second paragraph begins after a", 72.0, 664.0, 10.0),
            line("line of extra space.", 72.0, 652.0, 10.0),
        ];
        let blocks = paragraphs(
            0,
            0,
            std::slice::from_ref(&lines),
            &[],
            &[],
            12.0,
            TableStrategy::Both,
        );
        assert_eq!(blocks.len(), 2, "{:?}", texts(&blocks));
        assert!(blocks[0].text.ends_with("second line."));
        assert!(blocks[1].text.starts_with("the second paragraph"));
    }

    #[test]
    fn a_word_broken_across_a_line_is_put_back_together() {
        let lines = vec![
            line("the typesetter broke this para-", 72.0, 700.0, 10.0),
            line("graph across two lines.", 72.0, 688.0, 10.0),
        ];
        let blocks = paragraphs(
            0,
            0,
            std::slice::from_ref(&lines),
            &[],
            &[],
            12.0,
            TableStrategy::Both,
        );
        assert_eq!(
            texts(&blocks),
            vec!["the typesetter broke this paragraph across two lines."]
        );
    }

    /// A reference list set author-year, as `1002.4666` in DocLayNet sets it:
    /// each entry at the margin, its continuation hung an indent in.
    fn references(entries: usize) -> Vec<TextBlock> {
        let mut lines = vec![line("References", 72.0, 716.0, 12.0)];
        let mut y = 700.0;
        for n in 0..entries {
            lines.push(line(
                &format!("Author{n}, A. B., and C. D. Author, 2003, Phys. Rev. Lett. 88,"),
                72.0,
                y,
                10.0,
            ));
            lines.push(line(
                "014517, and the pages after it.",
                84.0,
                y - 12.0,
                10.0,
            ));
            y -= 24.0;
        }
        lines
    }

    #[test]
    fn a_run_of_hung_entries_is_a_list_of_items() {
        // #574: DocLayNet marks every entry of a reference list a list item,
        // and an author-year list has no marker to say so -- only its hang.
        let blocks = paragraphs(
            0,
            0,
            std::slice::from_ref(&references(4)),
            &[],
            &[],
            12.0,
            TableStrategy::Both,
        );
        let kinds: Vec<bool> = blocks
            .iter()
            .map(|b| {
                matches!(
                    b.kind,
                    BlockKind::ListItem {
                        level: 0,
                        ordinal: None
                    }
                )
            })
            .collect();
        assert_eq!(
            kinds,
            vec![false, true, true, true, true],
            "{:?}",
            texts(&blocks)
        );
        assert!(
            blocks[1].text.starts_with("Author0, A. B."),
            "{:?}",
            texts(&blocks)
        );
        assert!(
            blocks[1].text.ends_with("the pages after it."),
            "{:?}",
            texts(&blocks)
        );
    }

    #[test]
    fn two_hung_entries_are_not_yet_a_list() {
        // One paragraph set with a hanging indent is a paragraph, and two can
        // be a pair of definitions: the list starts at three.
        let blocks = paragraphs(
            0,
            0,
            std::slice::from_ref(&references(2)),
            &[],
            &[],
            12.0,
            TableStrategy::Both,
        );
        assert_eq!(blocks.len(), 3, "{:?}", texts(&blocks));
        assert!(
            blocks
                .iter()
                .all(|b| matches!(b.kind, BlockKind::Paragraph)),
            "{:?}",
            texts(&blocks)
        );
    }

    #[test]
    fn a_first_line_indent_starts_a_paragraph_and_a_hanging_indent_does_not() {
        // First-line indent: every indented line follows a short line.
        let indented = vec![
            line("    the first paragraph opens with an", 90.0, 700.0, 10.0),
            line("indent and runs the full measure of", 72.0, 688.0, 10.0),
            line("the column.", 72.0, 676.0, 10.0),
            line("    the second one opens the same way", 90.0, 664.0, 10.0),
            line("and also runs the full measure here.", 72.0, 652.0, 10.0),
        ];
        let blocks = paragraphs(
            0,
            0,
            std::slice::from_ref(&indented),
            &[],
            &[],
            12.0,
            TableStrategy::Both,
        );
        assert_eq!(blocks.len(), 2, "{:?}", texts(&blocks));

        // Hanging indent: every indented line follows a full line.
        let hanging = vec![
            line("Aho, A. and Ullman, J. Principles of", 72.0, 700.0, 10.0),
            line("    compiler design. Addison-Wesley,", 90.0, 688.0, 10.0),
            line("    Reading, 1977.", 90.0, 676.0, 10.0),
            line("Knuth, D. The art of computer program", 72.0, 664.0, 10.0),
            line("    ming. Addison-Wesley, Reading, 1968", 90.0, 652.0, 10.0),
        ];
        let blocks = paragraphs(
            0,
            0,
            std::slice::from_ref(&hanging),
            &[],
            &[],
            12.0,
            TableStrategy::Both,
        );
        assert_eq!(blocks.len(), 2, "{:?}", texts(&blocks));
        assert!(blocks[0].text.starts_with("Aho"));
        assert!(blocks[1].text.starts_with("Knuth"));
    }

    #[test]
    fn a_heading_is_not_swallowed_by_the_paragraph_under_it() {
        let lines = vec![
            line("2 Scope", 72.0, 700.0, 16.0),
            line("the body of the section starts here", 72.0, 684.0, 10.0),
            line("and runs on for a second line.", 72.0, 672.0, 10.0),
        ];
        let blocks = paragraphs(
            0,
            0,
            std::slice::from_ref(&lines),
            &[],
            &[],
            12.0,
            TableStrategy::Both,
        );
        assert_eq!(blocks.len(), 2, "{:?}", texts(&blocks));
        assert_eq!(blocks[0].text, "2 Scope");
    }

    #[test]
    fn a_ruled_row_never_joins_the_row_under_it() {
        // The Isartor appendix's pitch: two rows 18 pt apart with a hairline
        // between them, and the same two rows with the hairline withheld.
        let lines = vec![
            line("id-6-1-2-t01 the first message", 60.0, 700.0, 10.0),
            line("id-6-1-3-t01 the second message", 60.0, 682.0, 10.0),
        ];
        let rules = vec![HorizontalRule {
            left: 55.0,
            right: 540.0,
            y: 693.0,
        }];
        assert_eq!(
            paragraphs(
                0,
                0,
                std::slice::from_ref(&lines),
                &[],
                &[],
                18.0,
                TableStrategy::Both
            )
            .len(),
            1,
            "the unruled fixture no longer merges, so the pair below proves nothing"
        );
        assert_eq!(
            paragraphs(
                0,
                0,
                std::slice::from_ref(&lines),
                &[],
                &rules,
                18.0,
                TableStrategy::Both
            )
            .len(),
            2,
            "a rule between two rows did not keep them apart"
        );
    }

    #[test]
    fn a_wide_word_gap_on_one_line_is_not_a_table() {
        // Justified setting stretches the spaces of a single line; nothing
        // lines up underneath it.
        let justified = vec![
            TextBlock {
                spans: vec![
                    span("the quick brown", 72.0, 700.0, 10.0),
                    span("fox jumps over it", 200.0, 700.0, 10.0),
                ],
            },
            line("and the second line is set solid.", 72.0, 688.0, 10.0),
        ];
        assert_eq!(
            paragraphs(
                0,
                0,
                std::slice::from_ref(&justified),
                &[],
                &[],
                12.0,
                TableStrategy::Both
            )
            .len(),
            1
        );

        // Two lines with white running down the same x is a table nobody ruled.
        let tabular = vec![
            TextBlock {
                spans: vec![
                    span("Widget", 72.0, 700.0, 10.0),
                    span("12.00", 260.0, 700.0, 10.0),
                ],
            },
            TextBlock {
                spans: vec![
                    span("Gadget", 72.0, 688.0, 10.0),
                    span("8.50", 260.0, 688.0, 10.0),
                ],
            },
        ];
        // ... and a table is read as its cells, one block each, in row-major
        // order (#570): the two rows never join, and neither do the two
        // columns.
        let cells = paragraphs(
            0,
            0,
            std::slice::from_ref(&tabular),
            &[],
            &[],
            12.0,
            TableStrategy::Both,
        );
        assert_eq!(texts(&cells), vec!["Widget", "12.00", "Gadget", "8.50"]);
        assert!(
            cells
                .iter()
                .all(|block| matches!(block.kind, BlockKind::TableCell { .. })),
            "{:?}",
            cells.iter().map(|b| b.kind).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_full_width_title_does_not_join_the_column_under_it() {
        // The cut gives a two-column page with a full-width title three
        // regions: the title, then each column. Set in the body size and
        // left-aligned on the left column, the title is the same picture as
        // the first line of a paragraph -- which is the whole reason the
        // region is what decides here and no measurement of the lines is.
        let title = vec![line(
            "A title that runs the whole width of the page and then some more",
            72.0,
            720.0,
            10.0,
        )];
        let left = vec![
            line("the left column", 72.0, 708.0, 10.0),
            line("starts here and", 72.0, 696.0, 10.0),
        ];
        let right = vec![
            line("the right column", 320.0, 708.0, 10.0),
            line("starts up here", 320.0, 696.0, 10.0),
        ];
        let blocks = paragraphs(
            0,
            0,
            &[title, left, right],
            &[],
            &[],
            12.0,
            TableStrategy::Both,
        );
        assert_eq!(blocks.len(), 3, "{:?}", texts(&blocks));
        assert!(blocks[0].text.starts_with("A title"));
        assert_eq!(blocks[1].text, "the left column starts here and");
        assert_eq!(blocks[2].text, "the right column starts up here");
    }

    #[test]
    fn a_drop_cap_is_read_at_the_front_of_its_paragraph() {
        // Three-line initial: drawn on the baseline of the third line, at the
        // margin, with the first two lines set beside it.
        let lines = vec![
            line("he first line clears the", 100.0, 700.0, 10.0),
            line("initial, and so does the", 100.0, 688.0, 10.0),
            TextBlock {
                spans: vec![
                    span("T", 72.0, 676.0, 30.0),
                    span("second one; the third runs", 100.0, 676.0, 10.0),
                ],
            },
        ];
        let blocks = paragraphs(
            0,
            0,
            std::slice::from_ref(&lines),
            &[],
            &[],
            12.0,
            TableStrategy::Both,
        );
        assert_eq!(blocks.len(), 1, "{:?}", texts(&blocks));
        assert!(
            blocks[0].text.starts_with("T he first line"),
            "the initial is not at the front: {:?}",
            blocks[0].text
        );
    }

    #[test]
    fn a_footnote_under_a_rule_stays_its_own_block() {
        let lines = vec![
            line("the last line of the body text sits", 72.0, 200.0, 10.0),
            line("above the footnote rule.", 72.0, 188.0, 10.0),
            line("1 The footnote itself is set smaller.", 72.0, 170.0, 8.0),
        ];
        let blocks = paragraphs(
            0,
            0,
            std::slice::from_ref(&lines),
            &[],
            &[],
            12.0,
            TableStrategy::Both,
        );
        assert_eq!(blocks.len(), 2, "{:?}", texts(&blocks));
        assert!(blocks[1].text.starts_with("1 The footnote"));
    }

    #[test]
    fn a_block_carries_its_page_bbox_and_reading_order() {
        let lines = vec![
            line("one", 72.0, 700.0, 10.0),
            line("two", 72.0, 600.0, 10.0),
        ];
        let blocks = paragraphs(
            3,
            17,
            std::slice::from_ref(&lines),
            &[],
            &[],
            12.0,
            TableStrategy::Both,
        );
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].page, 3);
        assert_eq!(blocks[0].index, 17);
        assert_eq!(blocks[1].index, 18);
        assert_eq!(blocks[0].kind, BlockKind::Paragraph);
        assert!(blocks[0].bbox[0] <= 72.0 && blocks[0].bbox[3] >= 700.0);
        assert!(blocks[0].bbox[1] < blocks[0].bbox[3]);
        assert_eq!(blocks[0].spans.len(), 1);
    }

    #[test]
    fn an_oversized_marker_with_nothing_beside_it_is_not_a_drop_cap() {
        // A large bullet opening a line is the same shape as an initial and is
        // not one: nothing is set into the space it takes up.
        let lines = vec![
            line("the first line of the paragraph", 72.0, 700.0, 10.0),
            TextBlock {
                spans: vec![
                    span("\u{2022}", 72.0, 688.0, 20.0),
                    span("and the second one after a marker", 95.0, 688.0, 10.0),
                ],
            },
        ];
        let blocks = paragraphs(
            0,
            0,
            std::slice::from_ref(&lines),
            &[],
            &[],
            12.0,
            TableStrategy::Both,
        );
        assert_eq!(blocks.len(), 1, "{:?}", texts(&blocks));
        assert!(
            blocks[0].text.starts_with("the first line"),
            "the marker was hoisted to the front as if it were an initial: {:?}",
            blocks[0].text
        );
    }

    #[test]
    fn the_reading_order_has_no_gap_where_a_block_held_only_space() {
        let lines = vec![
            line("the first paragraph", 72.0, 700.0, 10.0),
            line("   ", 72.0, 600.0, 10.0),
            line("the second paragraph", 72.0, 500.0, 10.0),
        ];
        let blocks = paragraphs(
            0,
            7,
            std::slice::from_ref(&lines),
            &[],
            &[],
            12.0,
            TableStrategy::Both,
        );
        assert_eq!(
            texts(&blocks),
            vec!["the first paragraph", "the second paragraph"]
        );
        assert_eq!(
            blocks.iter().map(|b| b.index).collect::<Vec<_>>(),
            vec![7, 8],
            "a block that held only space left a hole in the reading order, and \
             the next page starts at the count rather than at the last index"
        );
    }

    /// A line set in a bold face at a size of its own.
    fn heavy(text: &str, x: f64, y: f64, size: f64) -> TextBlock {
        let mut span = span(text, x, y, size);
        span.is_bold = true;
        TextBlock { spans: vec![span] }
    }

    #[test]
    fn a_numbered_heading_inside_a_column_is_not_a_list_item() {
        // The XY-cut usually puts a heading in a region of its own, and then
        // the run rule never sees a second marker to agree with. This is the
        // page where it does: two numbered headings and the prose between
        // them, all one region and all one column. What refuses them is the
        // type -- both are set above the body -- and nothing else here can.
        let mut lines = vec![heavy("3. Method", 72.0, 700.0, 14.0)];
        for (i, text) in [
            "The committee considered the proposals put before it in",
            "the order they had been received, and resolved that each",
        ]
        .iter()
        .enumerate()
        {
            lines.push(line(text, 72.0, 686.0 - i as f64 * 12.0, 10.0));
        }
        lines.push(heavy("4. Results", 72.0, 650.0, 14.0));
        lines.push(line(
            "of them be referred to the officers for a report.",
            72.0,
            636.0,
            10.0,
        ));

        let blocks = paragraphs(
            0,
            0,
            std::slice::from_ref(&lines),
            &[],
            &[],
            12.0,
            TableStrategy::Both,
        );
        assert!(
            blocks
                .iter()
                .all(|block| !matches!(block.kind, BlockKind::ListItem { .. })),
            "a numbered heading was read as a list item: {:?}",
            blocks.iter().map(|b| (b.kind, &b.text)).collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_numbered_list_inside_the_same_column_is_one() {
        // The control for the case above: the same page, the same markers, the
        // same left edge -- set in the body instead of above it.
        let mut lines = vec![line(
            "The committee resolved that each of the proposals be referred",
            72.0,
            700.0,
            10.0,
        )];
        for (i, text) in ["3. to the officers", "4. to the auditors"]
            .iter()
            .enumerate()
        {
            lines.push(line(text, 72.0, 686.0 - i as f64 * 12.0, 10.0));
        }

        let blocks = paragraphs(
            0,
            0,
            std::slice::from_ref(&lines),
            &[],
            &[],
            12.0,
            TableStrategy::Both,
        );
        let kinds: Vec<BlockKind> = blocks.iter().map(|block| block.kind).collect();
        assert_eq!(
            kinds,
            vec![
                BlockKind::Paragraph,
                BlockKind::ListItem {
                    level: 0,
                    ordinal: Some(3)
                },
                BlockKind::ListItem {
                    level: 0,
                    ordinal: Some(4)
                },
            ],
            "{:?}",
            texts(&blocks)
        );
        assert_eq!(blocks[1].text, "to the officers");
    }

    #[test]
    fn a_page_of_nothing_produces_no_blocks() {
        assert!(paragraphs(0, 0, &[], &[], &[], 0.0, TableStrategy::Both).is_empty());
        assert!(paragraphs(
            0,
            0,
            &[vec![TextBlock { spans: Vec::new() }]],
            &[],
            &[],
            0.0,
            TableStrategy::Both
        )
        .is_empty());
    }

    fn subtitle_span(size: f64) -> TextSpan {
        TextSpan {
            font_size: size,
            ..Default::default()
        }
    }

    /// A block as the table passes hand it on: classified, boxed in `[x0, y0,
    /// x1, y1]` with y up, set in one size.
    fn subtitle_block(kind: BlockKind, text: &str, size: f64, bbox: [f64; 4]) -> Block {
        Block {
            kind,
            page: 0,
            bbox,
            text: text.into(),
            spans: vec![subtitle_span(size)],
            trace_learned: None,
            index: 0,
            continues_previous: false,
            confidence: None,
            grid: None,
            table_origin: TableOrigin::Other,
        }
    }

    fn subtitle_heading(text: &str, size: f64, bbox: [f64; 4]) -> Block {
        subtitle_block(BlockKind::Heading { level: 1 }, text, size, bbox)
    }

    /// The joined heading the first test joins, asserted whole: one block, the
    /// heading's level, both texts, both spans, the union box.
    #[test]
    fn a_title_keeps_its_subtitle() {
        let mut blocks = vec![
            subtitle_heading(
                "Statements of cash flows",
                18.0,
                [72.0, 700.0, 300.0, 718.0],
            ),
            subtitle_block(
                BlockKind::Paragraph,
                "for the year ended 30 June 2003",
                9.5,
                [72.0, 680.0, 280.0, 698.5],
            ),
        ];
        join_heading_subtitles(&mut blocks);
        assert_eq!(blocks.len(), 1, "the subtitle stayed apart: {blocks:?}");
        assert_eq!(blocks[0].kind, BlockKind::Heading { level: 1 });
        assert_eq!(
            blocks[0].text,
            "Statements of cash flows for the year ended 30 June 2003"
        );
        assert_eq!(blocks[0].spans.len(), 2);
        assert_eq!(blocks[0].bbox, [72.0, 680.0, 300.0, 718.0]);
        assert_eq!(blocks[0].index, 0);
    }

    #[test]
    fn a_two_line_title_is_one_heading() {
        let mut blocks = vec![
            subtitle_heading(
                "WE DON'T PLAY GAMES WITH",
                32.0,
                [72.0, 700.0, 400.0, 732.0],
            ),
            subtitle_heading(
                "ELECTRICITY, OUR CUSTOMERS",
                32.0,
                [72.0, 660.0, 400.0, 693.6],
            ),
        ];
        join_heading_subtitles(&mut blocks);
        assert_eq!(blocks.len(), 1, "the title stayed in pieces: {blocks:?}");
        assert_eq!(blocks[0].kind, BlockKind::Heading { level: 1 });
        assert_eq!(
            blocks[0].text,
            "WE DON'T PLAY GAMES WITH ELECTRICITY, OUR CUSTOMERS"
        );
    }

    #[test]
    fn a_heading_collects_two_tight_lines() {
        let mut blocks = vec![
            subtitle_heading("Annual report", 18.0, [72.0, 700.0, 200.0, 718.0]),
            subtitle_block(
                BlockKind::Paragraph,
                "for the year ended",
                9.5,
                [72.0, 690.0, 180.0, 698.5],
            ),
            subtitle_block(
                BlockKind::Paragraph,
                "30 June 2003",
                9.5,
                [72.0, 680.0, 150.0, 688.5],
            ),
            subtitle_block(
                BlockKind::Paragraph,
                "The year in review.",
                10.0,
                [72.0, 600.0, 300.0, 610.0],
            ),
        ];
        join_heading_subtitles(&mut blocks);
        assert_eq!(blocks.len(), 2, "the three lines did not join: {blocks:?}");
        assert_eq!(
            blocks[0].text,
            "Annual report for the year ended 30 June 2003"
        );
        assert_eq!(blocks[0].spans.len(), 3);
        assert_eq!((blocks[0].index, blocks[1].index), (0, 1));
    }

    #[test]
    fn a_heading_over_a_sentence_stays_apart() {
        let mut blocks = vec![
            subtitle_heading("January 1989", 14.0, [72.0, 700.0, 200.0, 714.0]),
            subtitle_block(
                BlockKind::Paragraph,
                "Emperor Hirohito of Japan dead at 87.",
                10.0,
                [72.0, 680.0, 300.0, 698.0],
            ),
        ];
        join_heading_subtitles(&mut blocks);
        assert_eq!(
            blocks.len(),
            2,
            "the heading swallowed the sentence under it: {blocks:?}"
        );
    }

    #[test]
    fn a_larger_line_under_a_heading_stays_apart() {
        let mut blocks = vec![
            subtitle_heading("Notes", 10.0, [72.0, 700.0, 200.0, 710.0]),
            subtitle_block(
                BlockKind::Paragraph,
                "A line set larger than the heading above it",
                14.0,
                [72.0, 680.0, 300.0, 698.5],
            ),
        ];
        join_heading_subtitles(&mut blocks);
        assert_eq!(
            blocks.len(),
            2,
            "the heading swallowed a larger line: {blocks:?}"
        );
    }

    #[test]
    fn a_loose_line_under_a_heading_stays_apart() {
        let mut blocks = vec![
            subtitle_heading(
                "Statements of cash flows",
                18.0,
                [72.0, 700.0, 300.0, 718.0],
            ),
            subtitle_block(
                BlockKind::Paragraph,
                "for the year ended 30 June 2003",
                9.5,
                [72.0, 671.0, 280.0, 691.0],
            ),
        ];
        join_heading_subtitles(&mut blocks);
        assert_eq!(
            blocks.len(),
            2,
            "the heading reached half an em down: {blocks:?}"
        );
    }

    #[test]
    fn an_indented_line_under_a_heading_stays_apart() {
        let mut blocks = vec![
            subtitle_heading(
                "Statements of cash flows",
                18.0,
                [72.0, 700.0, 300.0, 718.0],
            ),
            subtitle_block(
                BlockKind::Paragraph,
                "for the year ended 30 June 2003",
                9.5,
                [92.0, 680.0, 300.0, 698.5],
            ),
        ];
        join_heading_subtitles(&mut blocks);
        assert_eq!(
            blocks.len(),
            2,
            "the heading swallowed an indented line: {blocks:?}"
        );
    }

    #[test]
    fn a_heading_over_a_list_like_line_stays_apart() {
        let mut blocks = vec![
            subtitle_heading("Contents", 14.0, [72.0, 700.0, 200.0, 714.0]),
            subtitle_block(
                BlockKind::Paragraph,
                "1. The first item of a list",
                10.0,
                [72.0, 680.0, 300.0, 698.0],
            ),
        ];
        join_heading_subtitles(&mut blocks);
        assert_eq!(
            blocks.len(),
            2,
            "the heading swallowed a list item: {blocks:?}"
        );
    }

    #[test]
    fn a_subtitle_two_points_under_a_nine_point_head_joins() {
        // Two points under a nine-point head is 0.22 em: inside the quarter
        // em the join allows, past the two tenths it allowed before. The
        // SRB section head over its second line, on the same left edge.
        let mut blocks = vec![
            subtitle_heading(
                "Directors and Their Interests",
                9.0,
                [72.0, 700.0, 250.0, 709.0],
            ),
            subtitle_block(
                BlockKind::Paragraph,
                "Ordinary Shares and Options",
                9.0,
                [72.0, 689.0, 280.0, 698.0],
            ),
        ];
        join_heading_subtitles(&mut blocks);
        assert_eq!(blocks.len(), 1, "the subtitle stayed apart: {blocks:?}");
        assert_eq!(
            blocks[0].text,
            "Directors and Their Interests Ordinary Shares and Options"
        );
    }

    #[test]
    fn a_line_three_points_under_a_nine_point_head_stays_apart() {
        // Three points under a nine-point head is a third of an em: past the
        // quarter em the join allows, so the line stays its own block.
        let mut blocks = vec![
            subtitle_heading(
                "Directors and Their Interests",
                9.0,
                [72.0, 700.0, 250.0, 709.0],
            ),
            subtitle_block(
                BlockKind::Paragraph,
                "Ordinary Shares and Options",
                9.0,
                [72.0, 688.0, 280.0, 697.0],
            ),
        ];
        join_heading_subtitles(&mut blocks);
        assert_eq!(
            blocks.len(),
            2,
            "the heading reached a third of an em down: {blocks:?}"
        );
    }

    fn heading_line_span(size: f64, bold: bool) -> TextSpan {
        TextSpan {
            font_size: size,
            is_bold: bold,
            ..Default::default()
        }
    }

    /// A block as the table passes hand it on: classified, boxed, set in one
    /// size and one weight.
    fn heading_line_block(
        kind: BlockKind,
        text: &str,
        size: f64,
        bold: bool,
        bbox: [f64; 4],
    ) -> Block {
        Block {
            kind,
            page: 0,
            bbox,
            text: text.into(),
            spans: vec![heading_line_span(size, bold)],
            trace_learned: None,
            index: 0,
            continues_previous: false,
            confidence: None,
            grid: None,
            table_origin: TableOrigin::Other,
        }
    }

    #[test]
    fn two_heading_lines_in_one_style_become_one_heading() {
        let mut blocks = vec![
            heading_line_block(
                BlockKind::Heading { level: 1 },
                "ELECTRONIC DELIVERY OF",
                8.0,
                true,
                [72.0, 700.0, 220.0, 708.0],
            ),
            heading_line_block(
                BlockKind::Heading { level: 2 },
                "ANNUAL MEETING MATERIALS",
                8.0,
                true,
                [72.0, 689.0, 230.0, 697.0],
            ),
        ];
        join_heading_lines(&mut blocks);
        assert_eq!(blocks.len(), 1, "the title stayed in pieces: {blocks:?}");
        assert_eq!(blocks[0].kind, BlockKind::Heading { level: 1 });
        assert_eq!(
            blocks[0].text,
            "ELECTRONIC DELIVERY OF ANNUAL MEETING MATERIALS"
        );
        assert_eq!(blocks[0].spans.len(), 2);
        assert_eq!(blocks[0].bbox, [72.0, 689.0, 230.0, 708.0]);
        assert_eq!(blocks[0].index, 0);
    }

    #[test]
    fn a_heading_and_its_same_style_paragraph_line_join_when_short() {
        // Without the abbreviation's dot: a b-side full stop is a sentence
        // the join refuses.
        let mut blocks = vec![
            heading_line_block(
                BlockKind::Paragraph,
                "Powell Power Electronics",
                10.0,
                true,
                [72.0, 700.0, 240.0, 710.0],
            ),
            heading_line_block(
                BlockKind::Heading { level: 2 },
                "Company, Inc",
                10.0,
                true,
                [72.0, 688.0, 180.0, 698.0],
            ),
        ];
        join_heading_lines(&mut blocks);
        assert_eq!(blocks.len(), 1, "the name stayed in pieces: {blocks:?}");
        assert_eq!(blocks[0].kind, BlockKind::Heading { level: 2 });
        assert_eq!(blocks[0].text, "Powell Power Electronics Company, Inc");
    }

    #[test]
    fn a_heading_never_swallows_a_sentence() {
        let mut blocks = vec![
            heading_line_block(
                BlockKind::Heading { level: 2 },
                "Remuneration",
                10.0,
                true,
                [72.0, 700.0, 180.0, 710.0],
            ),
            heading_line_block(
                BlockKind::Paragraph,
                "The managing director's remuneration is made up of the following components.",
                10.0,
                true,
                [72.0, 685.0, 420.0, 698.0],
            ),
        ];
        join_heading_lines(&mut blocks);
        assert_eq!(
            blocks.len(),
            2,
            "the heading swallowed the sentence under it: {blocks:?}"
        );
    }

    #[test]
    fn a_heading_piece_ending_in_a_colon_still_joins() {
        let mut blocks = vec![
            heading_line_block(
                BlockKind::Paragraph,
                "To the Board of Directors and Stockholders of",
                9.0,
                true,
                [72.0, 700.0, 300.0, 709.0],
            ),
            heading_line_block(
                BlockKind::Heading { level: 2 },
                "Powell Industries, Inc.:",
                9.0,
                true,
                [72.0, 689.2, 250.0, 698.2],
            ),
        ];
        join_heading_lines(&mut blocks);
        assert_eq!(blocks.len(), 1, "the name stayed in pieces: {blocks:?}");
        assert_eq!(blocks[0].kind, BlockKind::Heading { level: 2 });
        assert_eq!(
            blocks[0].text,
            "To the Board of Directors and Stockholders of Powell Industries, Inc.:"
        );
    }

    #[test]
    fn different_sizes_do_not_join() {
        let mut blocks = vec![
            heading_line_block(
                BlockKind::Heading { level: 1 },
                "Notes",
                11.0,
                false,
                [72.0, 700.0, 150.0, 711.0],
            ),
            heading_line_block(
                BlockKind::Paragraph,
                "A line set smaller than the heading",
                9.0,
                false,
                [72.0, 690.0, 300.0, 699.0],
            ),
        ];
        join_heading_lines(&mut blocks);
        assert_eq!(
            blocks.len(),
            2,
            "the heading swallowed a line set in another size: {blocks:?}"
        );
    }

    #[test]
    fn a_marker_line_is_not_joined() {
        let mut blocks = vec![
            heading_line_block(
                BlockKind::Heading { level: 2 },
                "Employee share plans",
                10.0,
                true,
                [72.0, 700.0, 260.0, 710.0],
            ),
            heading_line_block(
                BlockKind::Paragraph,
                "(a) The consolidated entity has implemented two plans",
                10.0,
                true,
                [72.0, 688.0, 420.0, 698.0],
            ),
        ];
        join_heading_lines(&mut blocks);
        assert_eq!(
            blocks.len(),
            2,
            "the heading swallowed a list item: {blocks:?}"
        );
    }
}
