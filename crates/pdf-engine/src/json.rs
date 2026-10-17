//! The block model as JSON, for callers that chunk it themselves (#572).
//!
//! `crate::markdown` next door writes the document a RAG pipeline READS. This
//! writes the document a pipeline TAKES APART: every block with the kind it
//! was classified as, the page and the box it was drawn in, its text, the
//! spans it is made of and its place in the reading order. A caller that
//! chunks by heading, cites a paragraph by page, or draws a highlight over
//! what it retrieved needs the geometry, and markdown has none.
//!
//! WRITTEN BY HAND AND NOT BY A DERIVE. `serde` is an optional dependency of
//! this crate and off by default, deliberately -- the core engine stays
//! serialization-agnostic and the bindings that emit JSON turn it on. A
//! serializer that only existed under a feature flag would be a serializer
//! most callers of the library do not have, so this one writes its own bytes
//! and is always there.
//!
//! WHAT MAKES IT DETERMINISTIC, which is the whole of what a golden test over
//! it rests on: the keys are written in a fixed order rather than iterated out
//! of a map, and every number goes through [`number`], which has one rule for
//! every value a `f64` can hold.
//!
//! A NON-FINITE NUMBER IS `null`. JSON has no NaN and no infinity, and the two
//! ways out of that are both worse than saying nothing: printing `NaN` makes a
//! file no parser accepts, and printing `0` states a coordinate the document
//! does not have. `Block::bbox` is already finite or `[0, 0, 0, 0]` by the
//! time it leaves `crate::blocks`, so this is about a span from a pathological
//! transform and about a block a caller built by hand.
//!
//! THE KIND'S OWN FIELDS ARE FLAT, next to `kind` rather than nested under it:
//! `level` on a heading, `row` and `column` on a cell. Each object has exactly
//! one kind, so reading `level` after reading `kind` is unambiguous -- and it
//! is one step instead of two for every filter a caller writes.
//!
//! AND THERE IS NO FALLBACK ARM FOR A KIND THIS DOES NOT KNOW. `BlockKind` is
//! `#[non_exhaustive]` for the callers outside this crate, and inside it the
//! compiler still counts the variants -- so a kind added later stops the build
//! here instead of leaving the block to a `_` arm that would write it out as
//! something it is not. The name of a kind is a wire contract, and the day one
//! is added is the day somebody has to choose its name.

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

//! WHY THE SPANS ARE THE SIX NUMBERS AND NOT THE WHOLE `TextSpan`. The per-
//! glyph boxes on a span outweigh everything else in this file by an order of
//! magnitude -- one box of four numbers per character -- and nothing that
//! chunks a document reads them. The caller that does want them asks
//! `extract_text_blocks`, whose `TextSpanInfo` is the canonical wire form of a
//! span and carries all of it. This carries what a chunk needs to be placed:
//! where the run is, how big it is, and what it was set in.

use crate::blocks::{Block, BlockKind};
#[cfg(test)]
use crate::table_trace::TableOrigin;
use crate::text::TextSpan;

/// Two spaces per level, the indentation of the whole file.
pub(crate) const INDENT: &str = "  ";

/// One number, or `null` where a `f64` holds something JSON cannot.
///
/// The finite case is Rust's shortest representation that reads back as the
/// same `f64`, which is both exact and stable across platforms -- an integral
/// value is written without a fractional part, as JSON allows.
pub(crate) fn number(value: f64) -> String {
    if value.is_finite() {
        format!("{value}")
    } else {
        "null".to_string()
    }
}

/// One string as a JSON string, quotes included.
///
/// The two escapes JSON requires, the five it names, and `\u00xx` for every
/// other control character. Nothing else is touched: a Rust `String` is valid
/// UTF-8 and JSON is a UTF-8 format, so escaping the rest would only make the
/// file bigger and the text harder to read.
pub(crate) fn string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The name of a block's kind.
fn kind_name(kind: &BlockKind) -> &'static str {
    match kind {
        BlockKind::Paragraph => "paragraph",
        BlockKind::Heading { .. } => "heading",
        BlockKind::ListItem { .. } => "listItem",
        BlockKind::TableCell { .. } => "tableCell",
        BlockKind::Header => "header",
        BlockKind::Footer => "footer",
        BlockKind::Caption => "caption",
        BlockKind::Figure => "figure",
    }
}

/// The fields that belong to the kind alone, in the order they are written.
fn kind_fields(kind: &BlockKind) -> Vec<(&'static str, String)> {
    match *kind {
        BlockKind::Heading { level } => vec![("level", level.to_string())],
        BlockKind::ListItem { level, ordinal } => vec![
            ("level", level.to_string()),
            (
                "ordinal",
                ordinal.map_or_else(|| "null".to_string(), |n| n.to_string()),
            ),
        ],
        BlockKind::TableCell {
            row,
            column,
            row_span,
            column_span,
            header,
        } => vec![
            ("row", row.to_string()),
            ("column", column.to_string()),
            ("rowSpan", row_span.to_string()),
            ("columnSpan", column_span.to_string()),
            ("header", header.to_string()),
        ],
        _ => Vec::new(),
    }
}

/// One span, on one line: it is read as a record and not as a shape, and a
/// block of forty spans over nine lines each is a file nobody scrolls through.
fn span(span: &TextSpan, depth: usize) -> String {
    let pad = INDENT.repeat(depth);
    let mut out = format!(
        "{pad}{{ \"text\": {}, \"x\": {}, \"y\": {}, \"width\": {}, \"height\": {}, \"fontSize\": {}",
        string(&span.text),
        number(span.x),
        number(span.y),
        number(span.width),
        number(span.height),
        number(span.font_size),
    );
    // The face, where the document said what it was. Omitted rather than
    // written as `null`: a standard-14 font has no name to report, and a key
    // that is absent says that more plainly than a key that is empty -- the
    // same rule `TextSpanInfo` follows on the same field.
    if let Some(name) = &span.font_name {
        out.push_str(&format!(", \"fontName\": {}", string(name)));
    }
    out.push_str(&format!(
        ", \"isBold\": {}, \"isItalic\": {} }}",
        span.is_bold, span.is_italic
    ));
    out
}

/// One block as a JSON object.
pub(crate) fn block(block: &Block, depth: usize) -> String {
    let pad = INDENT.repeat(depth);
    let inner = INDENT.repeat(depth + 1);
    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("{inner}\"index\": {}", block.index));
    lines.push(format!(
        "{inner}\"kind\": {}",
        string(kind_name(&block.kind))
    ));
    for (key, value) in kind_fields(&block.kind) {
        lines.push(format!("{inner}\"{key}\": {value}"));
    }
    lines.push(format!("{inner}\"page\": {}", block.page));
    let bbox: Vec<String> = block.bbox.iter().map(|v| number(*v)).collect();
    lines.push(format!("{inner}\"bbox\": [{}]", bbox.join(", ")));
    lines.push(format!("{inner}\"text\": {}", string(&block.text)));
    lines.push(format!(
        "{inner}\"continuesPrevious\": {}",
        block.continues_previous
    ));
    // OMITTED RATHER THAN NULL WHERE NO MODEL SPOKE (#624, codex on #1977). The C
    // ABI and the .NET binding read structured blocks through this serializer and
    // nothing else, so a field missing here is a field those callers cannot have.
    // Absent and `null` would both parse, and absent is the one that cannot be
    // mistaken for a confidence of zero by a reader that coerces.
    if let Some(confidence) = block.confidence {
        lines.push(format!("{inner}\"confidence\": {}", number(confidence)));
    }
    // The table builder's own evidence, on every cell of the table it describes
    // (#574). Absent on a block that is not a cell, for the reason `confidence` is
    // absent where no model spoke: a reader that coerces must not be handed zeros.
    if let Some(grid) = block.grid {
        let facts = [
            format!("\"thinRows\": {}", grid.thin_rows),
            format!("\"fill\": {}", number(grid.fill)),
            format!("\"retriedFrom\": {}", grid.retried_from),
            format!("\"fullRows\": {}", grid.full_rows),
            format!("\"columns\": {}", grid.columns),
        ];
        lines.push(format!("{inner}\"grid\": {{{}}}", facts.join(", ")));
    }
    let spans: Vec<String> = block.spans.iter().map(|s| span(s, depth + 2)).collect();
    if spans.is_empty() {
        lines.push(format!("{inner}\"spans\": []"));
    } else {
        lines.push(format!(
            "{inner}\"spans\": [\n{}\n{inner}]",
            spans.join(",\n")
        ));
    }
    format!("{pad}{{\n{}\n{pad}}}", lines.join(",\n"))
}

/// Every block as JSON: kind, page, box, text, spans and reading-order index.
///
/// Preview: output quality is still improving and the API may change in a minor release.
///
/// An OBJECT with a `blocks` array in it, not a bare array. A document is a
/// thing with blocks in it and other things later -- the page sizes, the
/// document's own metadata -- and a format that opened as an array could never
/// say one without breaking every reader of it.
///
/// THE FURNITURE IS IN IT, which is the difference from `crate::to_markdown`.
/// A running head standing between two paragraphs is what makes it a defect
/// for anything that chunks the text, so the markdown does not write it; a
/// caller reading the blocks is not reading a flow, and the line the document
/// repeats is something it may well want. It carries `"header"` or `"footer"`
/// and stands after the body, where the reading order puts it (#571).
///
/// Deterministic: the same blocks produce the same bytes. Indented with two
/// spaces per level, because the golden test over this is a diff somebody has
/// to read, and any consumer that wants it smaller minifies it.
///
/// ```
/// use pdf_engine::{to_json, Block, BlockKind};
///
/// let blocks = vec![Block::new(
///     BlockKind::Heading { level: 1 },
///     0,
///     [72.0, 700.0, 200.0, 712.0],
///     "Scope".to_string(),
///     0,
/// )];
/// assert!(to_json(&blocks).contains("\"kind\": \"heading\""));
/// ```
pub fn to_json(blocks: &[Block]) -> String {
    if blocks.is_empty() {
        return "{\n  \"blocks\": []\n}\n".to_string();
    }
    let written: Vec<String> = blocks.iter().map(|b| block(b, 2)).collect();
    format!(
        "{{\n{INDENT}\"blocks\": [\n{}\n{INDENT}]\n}}\n",
        written.join(",\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block_of(kind: BlockKind, text: &str) -> Block {
        Block {
            kind,
            page: 0,
            bbox: [1.0, 2.0, 3.0, 4.0],
            text: text.to_string(),
            spans: Vec::new(),
            index: 0,
            continues_previous: false,
            confidence: None,
            grid: None,
            table_origin: TableOrigin::Other,
            trace_learned: None,
        }
    }

    #[test]
    fn nothing_in_is_an_empty_document_out() {
        assert_eq!(to_json(&[]), "{\n  \"blocks\": []\n}\n");
    }

    /// A caption is its own kind in the JSON, not a paragraph (#636).
    #[test]
    fn a_caption_is_named_a_caption() {
        let json = to_json(&[block_of(BlockKind::Caption, "Figure 1. The plant.")]);
        assert!(json.contains("\"kind\": \"caption\""), "{json}");
    }

    #[test]
    fn a_heading_carries_its_level_beside_its_kind() {
        let json = to_json(&[block_of(BlockKind::Heading { level: 2 }, "Terms")]);
        assert!(json.contains("\"kind\": \"heading\""), "{json}");
        assert!(json.contains("\"level\": 2"), "{json}");
        assert!(json.contains("\"text\": \"Terms\""), "{json}");
        assert!(json.contains("\"bbox\": [1, 2, 3, 4]"), "{json}");
    }

    #[test]
    fn a_bulleted_item_says_it_has_no_number() {
        let json = to_json(&[block_of(
            BlockKind::ListItem {
                level: 0,
                ordinal: None,
            },
            "first",
        )]);
        assert!(json.contains("\"ordinal\": null"), "{json}");
    }

    #[test]
    fn a_non_finite_coordinate_is_null_and_not_a_number_no_parser_reads() {
        let mut block = block_of(BlockKind::Paragraph, "x");
        block.bbox = [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 1.5];
        let json = to_json(&[block]);
        assert!(json.contains("\"bbox\": [null, null, null, 1.5]"), "{json}");
        assert!(!json.contains("NaN") && !json.contains("inf"), "{json}");
    }

    #[test]
    fn a_quote_and_a_control_character_leave_as_escapes() {
        let json = to_json(&[block_of(
            BlockKind::Paragraph,
            "he said \"go\"\tand\u{1}stopped",
        )]);
        let want = concat!(
            "\"text\": \"he said ",
            "\\\"go\\\"",
            "\\tand",
            "\\u0001",
            "stopped\""
        );
        assert!(json.contains(want), "{json}");
    }

    #[test]
    fn the_same_blocks_write_the_same_bytes() {
        let blocks = vec![
            block_of(BlockKind::Heading { level: 1 }, "Scope"),
            block_of(BlockKind::Paragraph, "Body."),
        ];
        assert_eq!(to_json(&blocks), to_json(&blocks));
    }
}
