#![warn(missing_docs)]
//! HTML, Markdown and plain-text extraction onto the engine's blocks.
//!
//! Reads markup and produces the same structured output the engine produces
//! for PDFs: [`Block`]s with [`BlockKind`]s such as heading/paragraph/list
//! item/table cell, serialized by the engine's own
//! [`pdf_engine::to_markdown`]. Downstream users get one format.
//!
//! | Module | Input | Backend |
//! |---|---|---|
//! | [`html`] | HTML documents and fragments | `html5ever` tokenizer |
//! | [`markdown`] | CommonMark | `pulldown-cmark` |
//! | [`text`] | plain text | blank-line splitting |
//!
//! Markup has no pages, so every block carries `page: 0` and a zero bounding
//! box, and blocks are numbered in document order.
//!
//! # Where this differs from PDF blocks, on purpose
//!
//! PDF extraction puts bold/italic into span flags and lets the Markdown
//! serializer mark them. Markup formatting is explicit, so this crate writes
//! the CommonMark inline spans directly into [`Block::text`] and leaves the
//! span flags plain: doing both would mark every run twice. Hyperlinks and
//! images have no engine-block shape either, so they are inline Markdown too.
//!
//! Block-level syntax is never written into the text: the engine's serializer
//! escapes a paragraph that OPENS like a quote or a fence, because PDF text
//! that does so is prose that would otherwise turn into a quote or a code
//! block, so such markers would come out backslashed. HTML `blockquote`
//! therefore becomes plain paragraphs, and `pre` one paragraph holding the
//! code verbatim (whitespace and line breaks kept): the engine's [`BlockKind`]
//! has no code variant. (Markdown fenced code still comes out fenced, as it
//! always has; its fences read as literal backticks.)
//!
//! # Example
//!
//! ```no_run
//! let html = std::fs::read_to_string("report.html").unwrap();
//! let blocks = markup_extract::html::extract_blocks(&html);
//! let markdown = markup_extract::html::extract_markdown(&html);
//! ```

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

pub mod html;
pub mod markdown;
pub mod text;

mod inline;

pub use pdf_engine::{Block, BlockKind};
