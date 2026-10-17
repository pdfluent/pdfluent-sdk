//! A document set wholly in monospace reads as prose (lane L1).
//!
//! The whole-document typewriter switch is gone, so every monospace
//! paragraph is marked as code even where monospace is the document's own
//! body face: a typewritten letter reads as one fenced block after another.
//! The rebuilt engine treats a document set wholly in monospace as prose,
//! and fences only code standing out against a proportional face.
//!
//! Through the Markdown output directly: the interpreter only marks
//! monospace from embedded font data, which no operator-built page carries,
//! so whole-document monospace is built here as spans, the way the
//! monospace tests build theirs. Only the shape is pinned: no fenced
//! blocks. Unspecified; our behaviour: a monospaced span inside prose is
//! inline code by default (G10), but a document of nothing else is not.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_engine::{to_markdown, Block, BlockKind, TextSpan};

fn mono(text: &str, x: f64, y: f64) -> TextSpan {
    TextSpan {
        text: text.to_string(),
        x,
        y,
        width: text.len().max(1) as f64 * 6.0,
        font_size: 10.0,
        is_monospace: Some(true),
        ..Default::default()
    }
}

/// A paragraph of two typewritten lines.
fn typed_paragraph(first: &str, second: &str, top: f64) -> Block {
    let mut block = Block::new(
        BlockKind::Paragraph,
        0,
        [0.0; 4],
        format!("{first} {second}"),
        0,
    );
    block.spans = vec![mono(first, 72.0, top), mono(second, 72.0, top - 12.0)];
    block
}

#[test]
fn a_typewritten_letter_has_no_fenced_blocks() {
    let blocks = vec![
        typed_paragraph("Dear Sir,", "I write about the pump.", 700.0),
        typed_paragraph("It runs all night.", "Yours faithfully,", 660.0),
    ];
    let markdown = to_markdown(&blocks);
    assert!(
        !markdown.contains("```"),
        "a document set wholly in monospace reads as code: {markdown:?}"
    );
}
