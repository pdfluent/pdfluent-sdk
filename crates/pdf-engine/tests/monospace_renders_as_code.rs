//! Monospace reads as code in the markdown, through the public surface (#636).
//!
//! The unit tests beside the serializer pin the detail; what is pinned here is
//! the contract a caller sees: [`pdf_engine::to_markdown`] writes the spans,
//! [`pdf_engine::to_markdown_with_code`] switches them off, and
//! [`pdf_engine::ExtractOptions::code`] -- settable from outside the crate --
//! is what `extract_markdown` reads.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use pdf_engine::{Block, BlockKind, ExtractOptions, TextSpan};

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

fn plain(text: &str, x: f64, y: f64) -> TextSpan {
    TextSpan {
        text: text.to_string(),
        x,
        y,
        width: text.len().max(1) as f64 * 6.0,
        font_size: 10.0,
        ..Default::default()
    }
}

fn paragraph(text: &str, spans: Vec<TextSpan>) -> Block {
    let mut block = Block::new(BlockKind::Paragraph, 0, [0.0; 4], text.to_string(), 0);
    block.spans = spans;
    block
}

/// A monospace word inside a sentence becomes an inline span by default.
#[test]
fn a_monospace_word_is_inline_code() {
    let block = paragraph(
        "Use foo here.",
        vec![
            plain("Use ", 100.0, 700.0),
            mono("foo", 130.0, 700.0),
            plain(" here.", 150.0, 700.0),
        ],
    );
    assert_eq!(pdf_engine::to_markdown(&[block]), "Use `foo` here.\n");
}

/// An all-monospace paragraph over two lines is a fenced block with its lines
/// kept -- where it stands out against a proportional face. A document set
/// wholly in monospace reads as prose instead (lane L1 typewriter switch),
/// which the typewriter suite pins; what is pinned here is that the fence
/// survives where the switch does not reach.
#[test]
fn an_all_monospace_block_is_fenced_with_its_lines() {
    let prose = paragraph(
        "A note about the pump.",
        vec![plain("A note about the pump.", 100.0, 724.0)],
    );
    let block = paragraph(
        "let x = 1; let y = 2;",
        vec![
            mono("let x = 1;", 100.0, 700.0),
            mono("let y = 2;", 100.0, 688.0),
        ],
    );
    assert_eq!(
        pdf_engine::to_markdown(&[prose, block]),
        "A note about the pump.\n\n```\nlet x = 1;\nlet y = 2;\n```\n"
    );
}

/// A monospace column of figures is not code: tabular figures are often set in
/// monospace faces in financial reports.
#[test]
fn a_monospace_numeric_column_is_not_wrapped() {
    let block = paragraph(
        "1,234 5,678 9,012",
        vec![mono("1,234 5,678 9,012", 100.0, 700.0)],
    );
    assert_eq!(pdf_engine::to_markdown(&[block]), "1,234 5,678 9,012\n");
}

/// `code = false` writes what the serializer wrote before code existed, byte
/// for byte -- and the option is settable from outside the crate, which is
/// what the bindings assign.
#[test]
fn code_false_gives_todays_output_byte_for_byte() {
    let inline = paragraph(
        "Use foo here.",
        vec![
            plain("Use ", 100.0, 700.0),
            mono("foo", 130.0, 700.0),
            plain(" here.", 150.0, 700.0),
        ],
    );
    let fenced = paragraph(
        "let x = 1; let y = 2;",
        vec![
            mono("let x = 1;", 100.0, 700.0),
            mono("let y = 2;", 100.0, 688.0),
        ],
    );
    assert!(ExtractOptions::default().code);
    let mut off = ExtractOptions::default();
    off.code = false;
    assert!(!off.code);
    assert_eq!(
        pdf_engine::to_markdown_with_code(&[inline], off.code),
        "Use foo here.\n"
    );
    assert_eq!(
        pdf_engine::to_markdown_with_code(&[fenced], off.code),
        "let x = 1; let y = 2;\n"
    );
}
