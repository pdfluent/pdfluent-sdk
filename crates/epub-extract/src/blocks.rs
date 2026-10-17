//! One block of a book's structured text: what a caller chunks, cites or embeds.
//!
//! The shape follows `pdf-engine`'s [`Block`](https://pdfluent.com/docs),
//! minus what an EPUB does not have. There are no pages, so there is no page
//! number and no bounding box; what orders the blocks is the spine, so every
//! block carries its chapter instead. Heading levels run 1 to 6 rather than
//! folding at 3, because the chapter states them outright (`h1` to `h6`) and
//! there is nothing to infer.
//!
//! `#[non_exhaustive]` for the reason `pdf-engine` learned the expensive way:
//! extraction keeps gaining kinds and fields, and each one must not break the
//! callers that only read. Build one with [`Block::new`], which takes the
//! fields no block can do without.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

/// One block of a book's structured text, in spine order.
///
/// See the [module documentation](self) for how this relates to
/// `pdf-engine`'s block.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct Block {
    /// What this block is.
    pub kind: BlockKind,
    /// 0-based position of the block's chapter in the spine.
    pub chapter: usize,
    /// Archive path of the block's chapter file, as resolved from the spine.
    pub chapter_href: String,
    /// The block's text, with inline Markdown for emphasis (`**bold**`,
    /// `*italic*`), links (`[text](href)`) and images (`![alt](src)`).
    pub text: String,
    /// Position of this block in the reading order of the whole book, counted
    /// from the first block of the first spine item.
    pub index: usize,
}

impl Block {
    /// A block with the fields every kind has.
    ///
    /// The way to build one from outside this crate, now that [`Block`] is
    /// `#[non_exhaustive]`.
    pub fn new(
        kind: BlockKind,
        chapter: usize,
        chapter_href: String,
        text: String,
        index: usize,
    ) -> Self {
        Self {
            kind,
            chapter,
            chapter_href,
            text,
            index,
        }
    }
}

/// The kind of a block: headings (`h1`–`h6`), paragraphs (`p`), list items
/// (`li`), table cells (`td`/`th`), links (`a`) and images (`img`).
///
/// `#[non_exhaustive]` because new kinds land in a published crate: a caller
/// that matches on this must keep a fallback arm, and adding a kind must not
/// be a breaking change.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BlockKind {
    /// Running text: a `p`, or stray text that belongs to no element.
    Paragraph,
    /// A heading, at the level the chapter states.
    Heading {
        /// 1 to 6, from `h1` to `h6`.
        level: u8,
    },
    /// One item of a list.
    ///
    /// One block per item, not one per list, for the reason a table is one
    /// block per cell: the item is what a caller chunks, cites and embeds.
    /// A list is therefore a run of adjacent blocks of this kind. The marker
    /// itself is not in [`Block::text`].
    ListItem {
        /// Nesting depth, 0 for the outermost list.
        level: u8,
        /// The item's number within its parent `ol` (1-based), or `None`
        /// where the item sits in a `ul`.
        ordinal: Option<u32>,
    },
    /// One cell of a table.
    ///
    /// One block per cell: a table is a run of these in row-major order,
    /// opening at `row: 0, column: 0`. The grid is the whole table's, so a
    /// cell after a `colspan` sits at the column it is drawn in, not at the
    /// count of cells before it in its row.
    TableCell {
        /// 0-based row inside the table.
        row: u16,
        /// 0-based column inside the table.
        column: u16,
        /// How many rows the cell covers, 1 unless `rowspan` says more.
        row_span: u16,
        /// How many columns the cell covers, 1 unless `colspan` says more.
        column_span: u16,
        /// Whether the cell is a header cell: a `th`, or any cell in `thead`.
        header: bool,
    },
    /// A link standing on its own. A link inside a paragraph, heading, item
    /// or cell stays inline instead, as `[text](href)` in that block's text.
    Link {
        /// The link target, as written in `href`.
        href: String,
    },
    /// An image standing on its own, with [`Block::text`] holding the alt
    /// text. An image inside a paragraph, heading, item or cell stays inline
    /// instead, as `![alt](src)` in that block's text.
    Image {
        /// The image location, as written in `src`.
        src: String,
        /// The alt text, as written in `alt` (empty when absent).
        alt: String,
    },
}
