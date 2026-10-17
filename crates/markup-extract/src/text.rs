#![warn(missing_docs)]
//! Plain text to engine blocks.
//!
//! Blank lines split paragraphs; lines that open with `- `, `* ` or a
//! number and a dot (`1. `) become list items. Lines that belong together --
//! an item and its continuation line -- join with a space.
//!
//! # Mapping
//!
//! | Text | Engine output |
//! |---|---|
//! | blank-line paragraphs | `BlockKind::Paragraph` |
//! | `- `, `* ` lines | `BlockKind::ListItem { level: 0, ordinal: None }` |
//! | `1. `, `2. `, ... lines | `BlockKind::ListItem { level: 0, ordinal }` |
//!
//! # Limitations (MVP)
//!
//! * Lists are flat: indentation is not read as nesting.
//! * Only `.` numbers items; `1)` stays a paragraph.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_engine::{Block, BlockKind, TextSpan};

/// Extract the structured blocks of a plain-text document.
///
/// Blocks carry `page: 0` and are numbered in document order: text has no
/// pages.
///
/// # Example
///
/// ```
/// let blocks = markup_extract::text::extract_blocks("Hello.\n\n- one\n- two\n");
/// assert_eq!(blocks.len(), 3);
/// ```
pub fn extract_blocks(text: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    // The open block: its kind, its lines, and whether it is an item (whose
    // marker line started it).
    let mut open: Option<(BlockKind, Vec<String>)> = None;
    let push = |kind: BlockKind, lines: Vec<String>, blocks: &mut Vec<Block>| {
        let text = lines.join(" ");
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        let index = blocks.len();
        let mut block = Block::new(kind, 0, [0.0; 4], text.clone(), index);
        block.spans = vec![TextSpan {
            text,
            ..Default::default()
        }];
        blocks.push(block);
    };

    for line in text.lines() {
        if line.trim().is_empty() {
            if let Some((kind, lines)) = open.take() {
                push(kind, lines, &mut blocks);
            }
            continue;
        }
        match item_marker(line) {
            Some(kind) => {
                if let Some((open_kind, lines)) = open.take() {
                    push(open_kind, lines, &mut blocks);
                }
                open = Some((kind, vec![marker_text(line)]));
            }
            None => match open.as_mut() {
                Some((_, lines)) => lines.push(line.trim().to_string()),
                None => {
                    open = Some((BlockKind::Paragraph, vec![line.trim().to_string()]));
                }
            },
        }
    }
    if let Some((kind, lines)) = open.take() {
        push(kind, lines, &mut blocks);
    }
    blocks
}

/// Extract a plain-text document directly as Markdown.
///
/// This is [`extract_blocks`] followed by the engine's
/// [`pdf_engine::to_markdown`], so markup and PDFs serialize through the
/// same code.
pub fn extract_markdown(text: &str) -> String {
    pdf_engine::to_markdown(&extract_blocks(text))
}

/// The list kind a line opens, or `None` where it opens none.
fn item_marker(line: &str) -> Option<BlockKind> {
    if line.starts_with("- ") || line.starts_with("* ") {
        return Some(BlockKind::ListItem {
            level: 0,
            ordinal: None,
        });
    }
    // A number and a dot: `1. `. Nine digits is where CommonMark stops
    // reading a marker, so longer runs stay paragraphs.
    let digits = line.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 || digits > 9 {
        return None;
    }
    let rest = &line[digits..];
    if !rest.starts_with(". ") {
        return None;
    }
    line[..digits]
        .parse::<u32>()
        .ok()
        .map(|ordinal| BlockKind::ListItem {
            level: 0,
            ordinal: Some(ordinal),
        })
}

/// A marker line without its marker.
fn marker_text(line: &str) -> String {
    if line.starts_with("- ") || line.starts_with("* ") {
        return line[2..].trim().to_string();
    }
    let digits = line.chars().take_while(|c| c.is_ascii_digit()).count();
    line[digits + 2..].trim().to_string()
}
