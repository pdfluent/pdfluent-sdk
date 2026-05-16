//! Text-run locator: maps a logical text span to its operator position(s) + state.

use crate::ops::{ContentOp, TjItem};
use crate::parser::ParsedOp;
use crate::state::{ContentStateMachine, GraphicsState};

/// A located text run in the content stream.
#[derive(Debug, Clone)]
pub struct TextRunLocation {
    /// Index of the text-showing operator in the `ParsedOp` slice.
    pub op_index: usize,
    /// Byte offset of the start of this instruction (including leading whitespace).
    pub byte_start: usize,
    /// Byte offset right after the operator keyword.
    pub byte_end: usize,
    /// Graphics and text state operative at this operator (snapshot before applying the op).
    pub state: GraphicsState,
    /// The decoded text bytes (from Tj/TJ/'/").
    pub text: Vec<u8>,
}

/// Extracted text run with decoded content, position, and associated state.
#[derive(Debug, Clone)]
pub struct ExtractedRun {
    /// Index in the `ParsedOp` slice.
    pub op_index: usize,
    pub byte_start: usize,
    pub byte_end: usize,
    /// Graphics + text state at the point of the operator.
    pub state: GraphicsState,
    /// Raw text bytes (PDF encoding).
    pub text: Vec<u8>,
}

/// Walk all parsed operators and return every text-showing operator with its
/// associated state snapshot and decoded text bytes.
///
/// State is captured **before** applying the text-showing op (i.e. the state
/// under which the text is rendered).
pub fn extract_text_runs(ops: &[ParsedOp]) -> Vec<ExtractedRun> {
    let mut machine = ContentStateMachine::new();
    let mut runs = Vec::new();

    for (i, parsed) in ops.iter().enumerate() {
        let state_snapshot = machine.state().clone();

        match &parsed.op {
            ContentOp::ShowText(bytes) => {
                runs.push(ExtractedRun {
                    op_index: i,
                    byte_start: parsed.byte_start,
                    byte_end: parsed.byte_end,
                    state: state_snapshot,
                    text: bytes.clone(),
                });
            }
            ContentOp::ShowTexts(items) => {
                let text: Vec<u8> = items
                    .iter()
                    .filter_map(|it| {
                        if let TjItem::Text(t) = it {
                            Some(t.as_slice())
                        } else {
                            None
                        }
                    })
                    .flatten()
                    .copied()
                    .collect();
                runs.push(ExtractedRun {
                    op_index: i,
                    byte_start: parsed.byte_start,
                    byte_end: parsed.byte_end,
                    state: state_snapshot,
                    text,
                });
            }
            ContentOp::NextLineAndShowText(bytes) => {
                machine.apply(&ContentOp::NextLine);
                runs.push(ExtractedRun {
                    op_index: i,
                    byte_start: parsed.byte_start,
                    byte_end: parsed.byte_end,
                    state: machine.state().clone(),
                    text: bytes.clone(),
                });
                machine.apply(&parsed.op);
                continue;
            }
            ContentOp::ShowTextWithParams { text, .. } => {
                runs.push(ExtractedRun {
                    op_index: i,
                    byte_start: parsed.byte_start,
                    byte_end: parsed.byte_end,
                    state: state_snapshot,
                    text: text.clone(),
                });
            }
            _ => {}
        }

        machine.apply(&parsed.op);
    }

    runs
}

/// Find a text span by its logical identity: raw text bytes + index among spans
/// with the same text in the stream.
///
/// Returns the location of the matching operator, or `None` if not found.
///
/// `span_index` is 0-based: 0 = first occurrence, 1 = second, etc.
pub fn find_span(
    ops: &[ParsedOp],
    text_bytes: &[u8],
    span_index: usize,
) -> Option<TextRunLocation> {
    let runs = extract_text_runs(ops);
    let mut match_count = 0usize;

    for run in runs {
        if run.text == text_bytes {
            if match_count == span_index {
                return Some(TextRunLocation {
                    op_index: run.op_index,
                    byte_start: run.byte_start,
                    byte_end: run.byte_end,
                    state: run.state,
                    text: run.text,
                });
            }
            match_count += 1;
        }
    }
    None
}

/// Find all spans whose raw text bytes contain `needle` as a sub-sequence of bytes.
pub fn find_spans_containing(ops: &[ParsedOp], needle: &[u8]) -> Vec<TextRunLocation> {
    extract_text_runs(ops)
        .into_iter()
        .filter(|r| contains_subslice(&r.text, needle))
        .map(|r| TextRunLocation {
            op_index: r.op_index,
            byte_start: r.byte_start,
            byte_end: r.byte_end,
            state: r.state,
            text: r.text,
        })
        .collect()
}

fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}
