//! The book as engine blocks: EPUB text through `pdf-engine`'s serializers.
//!
//! [`extract_engine_blocks`] maps the chapters [`crate::extract_epub`] reads
//! onto [`pdf_engine::Block`], so EPUBs serialize through the same
//! [`pdf_engine::to_markdown`] and [`pdf_engine::to_json`] as Office files
//! and PDFs, and the scorer reads them unchanged:
//!
//! | EPUB block | Engine block |
//! |---|
//! | `Heading { level }` (`h1`–`h6`) | `BlockKind::Heading { level }` |
//! | `Paragraph` (`p`) | `BlockKind::Paragraph` |
//! | `ListItem { level, ordinal }` (`li`) | `BlockKind::ListItem` with both |
//! | `TableCell { row, column, spans, header }` (`td`/`th`) | `BlockKind::TableCell` runs |
//! | emphasis and links inside the text | kept: they already are inline Markdown |
//! | standalone `Link { href }` / `Image { src, alt }` | a paragraph spelling the reference out |
//!
//! The engine has no link or image kind, so a standalone link becomes a
//! paragraph holding `[text](href)` and a standalone image one holding
//! `![alt](src)`: the same inline shapes the parser writes inside text, with
//! nothing dropped. [`pdf_engine::Block::page`] carries the spine index of
//! the chapter, the bounding box is zero (an EPUB chapter has no pages), and
//! [`pdf_engine::Block::index`] counts reading order across the whole book.
//!
//! [`extract_markdown`] is this mapping followed by the engine's Markdown.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use crate::blocks::{Block as EpubBlock, BlockKind as EpubKind};

/// Extract the text of an EPUB archive as engine blocks.
///
/// Reads the book exactly as [`crate::extract_epub`] does, then maps every
/// block as the [module documentation](self) says: headings keep their
/// levels, list items their nesting and ordinals, table cells their grid, and
/// inline Markdown stays untouched.
pub fn extract_engine_blocks(epub: &[u8]) -> crate::Result<Vec<pdf_engine::Block>> {
    Ok(crate::extract_epub(epub)?
        .into_iter()
        .map(engine_block)
        .collect())
}

/// Extract the text of an EPUB archive directly as Markdown.
///
/// This is [`extract_engine_blocks`] followed by the engine's
/// [`pdf_engine::to_markdown`], so EPUBs and PDFs serialize through the same
/// code.
pub fn extract_markdown(epub: &[u8]) -> crate::Result<String> {
    Ok(pdf_engine::to_markdown(&extract_engine_blocks(epub)?))
}

/// One EPUB block as an engine block: the kind mapped, the chapter as the
/// page, a zero box, the reading index kept.
fn engine_block(block: EpubBlock) -> pdf_engine::Block {
    use pdf_engine::BlockKind as EngineKind;

    let (kind, text) = match block.kind {
        EpubKind::Paragraph => (EngineKind::Paragraph, block.text),
        EpubKind::Heading { level } => (EngineKind::Heading { level }, block.text),
        EpubKind::ListItem { level, ordinal } => {
            (EngineKind::ListItem { level, ordinal }, block.text)
        }
        EpubKind::TableCell {
            row,
            column,
            row_span,
            column_span,
            header,
        } => (
            EngineKind::TableCell {
                row,
                column,
                row_span,
                column_span,
                header,
            },
            block.text,
        ),
        // No engine kind for these; spell the reference out in the inline
        // shape the parser uses inside text, so the target survives.
        EpubKind::Link { href } => (EngineKind::Paragraph, format!("[{}]({href})", block.text)),
        EpubKind::Image { src, alt } => (EngineKind::Paragraph, format!("![{alt}]({src})")),
    };
    pdf_engine::Block::new(kind, block.chapter, [0.0; 4], text, block.index)
}
