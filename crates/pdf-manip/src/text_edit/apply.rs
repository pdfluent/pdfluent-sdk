//! Edit planning and container rebuilding (design §2, §5, §10.4).
//!
//! All rebuilding happens on cloned operator lists: nothing here mutates the
//! document. The session's commit swaps the prepared stream bytes in only
//! after every staged edit validated (prepare-then-swap, AllOrNothing).

use lopdf::content::Operation;
use lopdf::{Object, ObjectId, StringFormat};

use crate::content_editor::ContentEditor;
use crate::text_replace::{encode_latin1, encode_text_for_font};
use crate::text_run::{FontMap, TextRun};

use super::scan::PageScan;
use super::{Diagnostic, FontFallback, TextEditError};

/// Resource name used for the injected standard fallback font.
pub(crate) const FALLBACK_FONT_NAME: &str = "F__Helv";

/// One edit to apply against a page scan, already resolved and validated for
/// staleness/overlap.
pub(crate) struct EditRequest {
    /// Index into the session's staged-edit list (for result attribution).
    pub staged_index: usize,
    /// Byte range in the page container's combined visual text.
    pub chr: (usize, usize),
    /// Replacement text.
    pub replacement: String,
    /// Font fallback policy for this edit.
    pub fallback: FontFallback,
}

/// Per-edit outcome of preparing a page.
pub(crate) struct EditOutcome {
    pub staged_index: usize,
    pub result: Result<AppliedInfo, TextEditError>,
}

/// Details of a successfully prepared edit.
#[derive(Clone)]
pub(crate) struct AppliedInfo {
    pub font_used: String,
    pub font_substituted: bool,
    pub diagnostics: Vec<Diagnostic>,
}

/// Prepared (not yet applied) rewrite of a page.
pub(crate) struct PreparedPage {
    /// Streams to rewrite: (stream object, new uncompressed bytes).
    pub touched_streams: Vec<(ObjectId, Vec<u8>)>,
    /// Whether the standard fallback font must be injected into the page
    /// resources before the streams are swapped in.
    pub inject_fallback: bool,
    /// Per-edit outcomes, in `staged_index` order of the input.
    pub outcomes: Vec<EditOutcome>,
    /// Stream indices (into the page's `/Contents` order) that were touched.
    pub touched_stream_indices: Vec<usize>,
}

// ---------------------------------------------------------------------------
// Segments
// ---------------------------------------------------------------------------

/// A piece of a run's new text.
enum Segment {
    /// Original text retained, byte range within the run's decoded text.
    Kept { start: usize, end: usize },
    /// Replacement text from one edit.
    Repl { text: String, staged_index: usize },
}

/// Prepare all edits for one page. Returns per-edit outcomes plus the
/// re-encoded bytes for every touched stream; the document is not mutated.
pub(crate) fn prepare_page(
    scan: &PageScan,
    edits: &[EditRequest],
) -> Result<PreparedPage, TextEditError> {
    let content = &scan.content;
    let runs = &content.runs;
    let bounds = &content.run_bounds;

    let mut outcomes: Vec<EditOutcome> = Vec::with_capacity(edits.len());
    let mut inject_fallback = false;

    // Per-run collected segments-to-replace: run index -> (range in run text,
    // Option<(replacement, staged_index, fallback)>).
    struct RunEditPart<'a> {
        range: (usize, usize),
        replacement: Option<&'a str>,
        staged_index: usize,
        fallback: &'a FontFallback,
    }
    let mut per_run: std::collections::BTreeMap<usize, Vec<RunEditPart>> =
        std::collections::BTreeMap::new();

    for edit in edits {
        let (s, e) = edit.chr;
        let ri0 = run_containing(bounds, s);
        let ri1 = run_containing(bounds, e.saturating_sub(1).max(s));
        for ri in ri0..=ri1 {
            let run_start = bounds[ri];
            let run_end = bounds[ri + 1];
            let local_s = s.max(run_start) - run_start;
            let local_e = e.min(run_end) - run_start;
            per_run.entry(ri).or_default().push(RunEditPart {
                range: (local_s, local_e),
                // The full replacement lands in the run where the match
                // starts; overlapped tails in later runs are deleted.
                replacement: (ri == ri0).then_some(edit.replacement.as_str()),
                staged_index: edit.staged_index,
                fallback: &edit.fallback,
            });
        }
    }

    // Rebuild each touched run into new operator sequences.
    // (local op splices, grouped per stream index)
    let mut splices: std::collections::BTreeMap<usize, Vec<(usize, Vec<Operation>)>> =
        std::collections::BTreeMap::new();
    // Track which staged edits failed (an edit may touch several runs).
    let mut failed: std::collections::BTreeMap<usize, TextEditError> =
        std::collections::BTreeMap::new();
    let mut applied: std::collections::BTreeMap<usize, AppliedInfo> =
        std::collections::BTreeMap::new();

    for (&ri, parts) in &per_run {
        let run = &runs[ri];
        let mut sorted: Vec<&RunEditPart> = parts.iter().collect();
        sorted.sort_by_key(|p| p.range.0);

        // Build the segment list over this run's text.
        let mut segments: Vec<Segment> = Vec::new();
        let mut pos = 0usize;
        for part in &sorted {
            if part.range.0 > pos {
                segments.push(Segment::Kept {
                    start: pos,
                    end: part.range.0,
                });
            }
            segments.push(Segment::Repl {
                text: part.replacement.unwrap_or("").to_string(),
                staged_index: part.staged_index,
            });
            pos = part.range.1;
        }
        if pos < run.text.len() {
            segments.push(Segment::Kept {
                start: pos,
                end: run.text.len(),
            });
        }

        let fallback_policy = sorted
            .first()
            .map(|p| p.fallback.clone())
            .unwrap_or(FontFallback::Deny);

        match rebuild_run(scan, run, &segments, &fallback_policy) {
            Ok(rebuilt) => {
                if rebuilt.used_fallback {
                    inject_fallback = true;
                }
                for part in &sorted {
                    let info = applied.entry(part.staged_index).or_insert(AppliedInfo {
                        font_used: run.font_name.clone(),
                        font_substituted: false,
                        diagnostics: Vec::new(),
                    });
                    if rebuilt.used_fallback && part.replacement.is_some() {
                        info.font_used = rebuilt.fallback_font.clone().unwrap_or_default();
                        info.font_substituted = true;
                    }
                    info.diagnostics.extend(rebuilt.diagnostics.iter().cloned());
                }
                let global_op = run.ops_range.start;
                let (stream_idx, local_idx) =
                    scan.source_of(global_op)
                        .ok_or_else(|| TextEditError::Internal {
                            detail: format!("no provenance for op {global_op}"),
                        })?;
                splices
                    .entry(stream_idx)
                    .or_default()
                    .push((local_idx, rebuilt.ops));
            }
            Err((staged_index, err)) => {
                failed.entry(staged_index).or_insert(err);
            }
        }
    }

    for edit in edits {
        let result = if let Some(err) = failed.remove(&edit.staged_index) {
            Err(err)
        } else if let Some(info) = applied.remove(&edit.staged_index) {
            Ok(info)
        } else {
            Err(TextEditError::Internal {
                detail: "edit produced no outcome".to_string(),
            })
        };
        outcomes.push(EditOutcome {
            staged_index: edit.staged_index,
            result,
        });
    }

    // Any failure: report outcomes without building stream bytes (AllOrNothing
    // aborts anyway; BestEffort in Phase 1C will re-plan with the subset).
    if outcomes.iter().any(|o| o.result.is_err()) {
        return Ok(PreparedPage {
            touched_streams: Vec::new(),
            inject_fallback: false,
            outcomes,
            touched_stream_indices: Vec::new(),
        });
    }

    // Rebuild only the touched streams.
    let mut touched_streams = Vec::new();
    let mut touched_stream_indices = Vec::new();
    for (stream_idx, mut ops_splices) in splices {
        let stream_id =
            *scan
                .stream_ids
                .get(stream_idx)
                .ok_or_else(|| TextEditError::Internal {
                    detail: format!("stream index {stream_idx} out of range"),
                })?;
        // Collect this stream's operator list from the logical sequence.
        let mut local_ops: Vec<Operation> = content
            .ops
            .iter()
            .zip(content.op_src.iter())
            .filter(|(_, (si, _))| *si == stream_idx)
            .map(|(op, _)| op.clone())
            .collect();
        // Apply splices in descending local index so indices stay valid.
        ops_splices.sort_by(|a, b| b.0.cmp(&a.0));
        for (local_idx, new_ops) in ops_splices {
            if local_idx < local_ops.len() {
                local_ops.splice(local_idx..=local_idx, new_ops);
            }
        }
        let editor = ContentEditor::from_operations(local_ops);
        let bytes = editor.encode().map_err(TextEditError::Document)?;
        touched_streams.push((stream_id, bytes));
        touched_stream_indices.push(stream_idx);
    }

    Ok(PreparedPage {
        touched_streams,
        inject_fallback,
        outcomes,
        touched_stream_indices,
    })
}

fn run_containing(bounds: &[usize], offset: usize) -> usize {
    match bounds.binary_search(&offset) {
        Ok(i) => i.min(bounds.len().saturating_sub(2)),
        Err(i) => i - 1,
    }
}

// ---------------------------------------------------------------------------
// Run rebuilding
// ---------------------------------------------------------------------------

struct RebuiltRun {
    ops: Vec<Operation>,
    used_fallback: bool,
    fallback_font: Option<String>,
    diagnostics: Vec<Diagnostic>,
}

/// Encoded piece of the new run: bytes plus the font they are encoded for
/// (None = the run's original font).
struct EncodedSegment {
    bytes: Vec<u8>,
    fallback: bool,
}

/// Rebuild one text-showing operator from its segment list.
///
/// Errors are attributed to the staged edit responsible: kept-text failures
/// blame the first edit in the run (it forced the rewrite), replacement
/// failures blame their own edit.
fn rebuild_run(
    scan: &PageScan,
    run: &TextRun,
    segments: &[Segment],
    fallback_policy: &FontFallback,
) -> Result<RebuiltRun, (usize, TextEditError)> {
    let content = &scan.content;
    let op = &content.ops[run.ops_range.start];
    let fonts = &scan.fonts;
    let font = &run.font_name;
    let mut diagnostics = Vec::new();

    let first_edit_index = segments
        .iter()
        .find_map(|s| match s {
            Segment::Repl { staged_index, .. } => Some(*staged_index),
            _ => None,
        })
        .unwrap_or(0);

    // Original operand bytes of the whole run (for verbatim kept slices).
    let orig_bytes = run_string_bytes(op);

    let mut encoded: Vec<EncodedSegment> = Vec::new();
    let mut used_fallback = false;
    let mut fallback_font: Option<String> = None;

    for segment in segments {
        match segment {
            Segment::Kept { start, end } => {
                let kept_text = &run.text[*start..*end];
                if kept_text.is_empty() {
                    continue;
                }
                let bytes = slice_run_bytes(&orig_bytes, &run.text, (*start, *end), fonts, font)
                    .map(Ok)
                    .unwrap_or_else(|| encode_text_for_font(font, kept_text, fonts))
                    .map_err(|e| {
                        (
                            first_edit_index,
                            TextEditError::EncodingFailed {
                                match_id: None,
                                font: font.clone(),
                                detail: format!("kept text not re-encodable: {e}"),
                            },
                        )
                    })?;
                encoded.push(EncodedSegment {
                    bytes,
                    fallback: false,
                });
            }
            Segment::Repl { text, staged_index } => {
                if text.is_empty() {
                    continue;
                }
                match encode_text_for_font(font, text, fonts) {
                    Ok(bytes) => encoded.push(EncodedSegment {
                        bytes,
                        fallback: false,
                    }),
                    Err(primary) => match fallback_policy {
                        FontFallback::Deny => {
                            return Err((
                                *staged_index,
                                TextEditError::FontFallbackDenied {
                                    match_id: None,
                                    font: font.clone(),
                                    detail: primary.to_string(),
                                },
                            ));
                        }
                        FontFallback::InjectStandard => {
                            let bytes = encode_latin1(text).map_err(|e| {
                                (
                                    *staged_index,
                                    TextEditError::EncodingFailed {
                                        match_id: None,
                                        font: FALLBACK_FONT_NAME.to_string(),
                                        detail: e.to_string(),
                                    },
                                )
                            })?;
                            used_fallback = true;
                            fallback_font = Some(FALLBACK_FONT_NAME.to_string());
                            encoded.push(EncodedSegment {
                                bytes,
                                fallback: true,
                            });
                        }
                        FontFallback::Explicit(name) => {
                            let bytes = encode_text_for_font(name, text, fonts).map_err(|e| {
                                (
                                    *staged_index,
                                    TextEditError::EncodingFailed {
                                        match_id: None,
                                        font: name.clone(),
                                        detail: e.to_string(),
                                    },
                                )
                            })?;
                            used_fallback = true;
                            fallback_font = Some(name.clone());
                            encoded.push(EncodedSegment {
                                bytes,
                                fallback: true,
                            });
                        }
                    },
                }
            }
        }
    }

    let ops = if encoded.iter().all(|s| !s.fallback) {
        let all_bytes: Vec<u8> = encoded.iter().flat_map(|s| s.bytes.clone()).collect();
        if op.operator == "TJ" {
            match rebuild_tj_preserving(op, run, segments, fonts, font) {
                Some((new_op, dropped_kerning)) => {
                    if dropped_kerning {
                        diagnostics.push(Diagnostic {
                            code: "kerning-dropped-in-match-region".to_string(),
                            message: "TJ spacing adjustments inside the edited region were dropped"
                                .to_string(),
                        });
                    }
                    vec![new_op]
                }
                None => {
                    diagnostics.push(Diagnostic {
                        code: "tj-flattened".to_string(),
                        message: "TJ array flattened to a single string".to_string(),
                    });
                    vec![rebuild_simple_op(op, all_bytes)]
                }
            }
        } else {
            vec![rebuild_simple_op(op, all_bytes)]
        }
    } else {
        // Mixed fonts: emit an operator sequence with Tf switches.
        if op.operator == "TJ" {
            diagnostics.push(Diagnostic {
                code: "tj-flattened".to_string(),
                message: "TJ array flattened for font-fallback replacement".to_string(),
            });
        }
        build_mixed_font_ops(
            op,
            &encoded,
            font,
            run.font_size,
            fallback_font.as_deref().unwrap_or(FALLBACK_FONT_NAME),
        )
    };

    Ok(RebuiltRun {
        ops,
        used_fallback,
        fallback_font,
        diagnostics,
    })
}

/// Concatenated string operand bytes of a text-showing operator.
fn run_string_bytes(op: &Operation) -> Vec<u8> {
    match op.operator.as_str() {
        "Tj" | "'" => match op.operands.first() {
            Some(Object::String(b, _)) => b.clone(),
            _ => Vec::new(),
        },
        "\"" => match op.operands.get(2) {
            Some(Object::String(b, _)) => b.clone(),
            _ => Vec::new(),
        },
        "TJ" => match op.operands.first() {
            Some(Object::Array(arr)) => arr
                .iter()
                .filter_map(|o| match o {
                    Object::String(b, _) => Some(b.clone()),
                    _ => None,
                })
                .flatten()
                .collect(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// Verbatim byte slice of a run's original operand bytes for the given text
/// byte range, when a reliable char↔byte mapping exists (design §8).
fn slice_run_bytes(
    orig_bytes: &[u8],
    decoded: &str,
    range: (usize, usize),
    fonts: &FontMap,
    font: &str,
) -> Option<Vec<u8>> {
    let prefix_chars = decoded[..range.0].chars().count();
    let slice_chars = decoded[range.0..range.1].chars().count();
    if fonts.is_cid_font(font) {
        if !orig_bytes.len().is_multiple_of(2) {
            return None;
        }
        let start = fonts.cid_byte_offset_for_chars(font, orig_bytes, prefix_chars);
        let end = fonts.cid_byte_offset_for_chars(font, orig_bytes, prefix_chars + slice_chars);
        if end <= orig_bytes.len() && start <= end {
            return Some(orig_bytes[start..end].to_vec());
        }
        None
    } else {
        // Single-byte fonts: only safe when bytes map 1:1 to chars.
        if orig_bytes.len() != decoded.chars().count() {
            return None;
        }
        let start = prefix_chars;
        let end = prefix_chars + slice_chars;
        (end <= orig_bytes.len()).then(|| orig_bytes[start..end].to_vec())
    }
}

/// Rebuild Tj / ' / " with new string bytes, preserving operator semantics.
fn rebuild_simple_op(op: &Operation, bytes: Vec<u8>) -> Operation {
    match op.operator.as_str() {
        "\"" => {
            let mut operands = op.operands.clone();
            if operands.len() >= 3 {
                operands[2] = Object::String(bytes, StringFormat::Literal);
            }
            Operation::new("\"", operands)
        }
        "TJ" => Operation::new(
            "TJ",
            vec![Object::Array(vec![Object::String(
                bytes,
                StringFormat::Literal,
            )])],
        ),
        other => Operation::new(other, vec![Object::String(bytes, StringFormat::Literal)]),
    }
}

/// Element-aware TJ rebuild: elements (and spacing) entirely before the first
/// edited character and entirely after the last edited character are kept
/// verbatim; the middle is re-encoded as one string element.
///
/// Returns `None` when the structure cannot be mapped (caller flattens).
/// The boolean is true when spacing elements inside the edited region were
/// dropped.
fn rebuild_tj_preserving(
    op: &Operation,
    run: &TextRun,
    segments: &[Segment],
    fonts: &FontMap,
    font: &str,
) -> Option<(Operation, bool)> {
    let arr = match op.operands.first() {
        Some(Object::Array(a)) => a,
        _ => return None,
    };

    // The edited original span is the complement of the leading/trailing
    // kept ranges.
    let mut kept_head_end = 0usize; // end of the leading kept range
    let mut kept_tail_start = run.text.len(); // start of the trailing kept range
    if let Some(Segment::Kept { start: 0, end }) = segments.first() {
        kept_head_end = *end;
    }
    if let Some(Segment::Kept { start, end }) = segments.last() {
        if *end == run.text.len() {
            kept_tail_start = *start;
        }
    }

    // Decode elements and find which are fully inside head/tail kept ranges.
    let mut elements: Vec<(usize, usize, usize)> = Vec::new(); // (arr idx, text start, text end)
    let mut pos = 0usize;
    for (i, item) in arr.iter().enumerate() {
        if let Object::String(bytes, _) = item {
            let text = fonts.decode_string(font, bytes);
            elements.push((i, pos, pos + text.len()));
            pos += text.len();
        }
    }
    if pos != run.text.len() {
        return None; // decode mismatch — flatten
    }

    let mut head_elems: Vec<usize> = Vec::new(); // arr indices kept verbatim (front)
    let mut tail_elems: Vec<usize> = Vec::new(); // arr indices kept verbatim (back)
    for &(ai, s, e) in &elements {
        if e <= kept_head_end {
            head_elems.push(ai);
        } else if s >= kept_tail_start {
            tail_elems.push(ai);
        }
    }
    let head_cut = head_elems.last().map(|&i| i + 1).unwrap_or(0);
    let tail_cut = tail_elems.first().copied().unwrap_or(arr.len());

    // Middle text: original head-partial + edits applied + tail-partial.
    // Rebuild from segments, restricted to [middle_text_start, middle_text_end).
    let middle_text_start = elements
        .iter()
        .find(|&&(ai, _, _)| ai >= head_cut)
        .map(|&(_, s, _)| s)
        .unwrap_or(run.text.len());
    let middle_text_end = elements
        .iter()
        .rev()
        .find(|&&(ai, _, _)| ai < tail_cut)
        .map(|&(_, _, e)| e)
        .unwrap_or(middle_text_start);

    let mut middle = String::new();
    for seg in segments {
        match seg {
            Segment::Kept { start, end } => {
                let s = (*start).max(middle_text_start);
                let e = (*end).min(middle_text_end);
                if s < e {
                    middle.push_str(&run.text[s..e]);
                }
            }
            Segment::Repl { text, .. } => middle.push_str(text),
        }
    }

    let mut new_arr: Vec<Object> = Vec::new();
    let mut dropped_kerning = false;
    // Head: keep everything (strings AND spacing) before head_cut verbatim.
    for item in &arr[..head_cut] {
        new_arr.push(item.clone());
    }
    // Middle: one re-encoded string; spacing items in the middle are dropped.
    if arr[head_cut..tail_cut]
        .iter()
        .any(|o| !matches!(o, Object::String(_, _)))
    {
        dropped_kerning = true;
    }
    if !middle.is_empty() {
        let bytes = encode_text_for_font(font, &middle, fonts).ok()?;
        new_arr.push(Object::String(bytes, StringFormat::Literal));
    }
    // Tail: verbatim.
    for item in &arr[tail_cut..] {
        new_arr.push(item.clone());
    }

    Some((
        Operation::new("TJ", vec![Object::Array(new_arr)]),
        dropped_kerning,
    ))
}

/// Emit an operator sequence for a run whose segments use mixed fonts.
///
/// The first emitted text op preserves the original operator (so `'` keeps
/// its line advance and `"` its spacing operands); subsequent pieces are
/// plain `Tj` with `Tf` font switches, and the original font is restored at
/// the end.
fn build_mixed_font_ops(
    op: &Operation,
    encoded: &[EncodedSegment],
    font: &str,
    font_size: f64,
    fallback_name: &str,
) -> Vec<Operation> {
    let tf = |name: &str| {
        Operation::new(
            "Tf",
            vec![
                Object::Name(name.as_bytes().to_vec()),
                Object::Real(font_size as f32),
            ],
        )
    };

    let mut ops = Vec::new();
    let mut current_fallback = false;
    let mut first = true;

    for seg in encoded {
        if seg.bytes.is_empty() {
            continue;
        }
        if seg.fallback != current_fallback {
            ops.push(tf(if seg.fallback { fallback_name } else { font }));
            current_fallback = seg.fallback;
        }
        if first {
            // Preserve the original operator semantics on the first piece.
            match op.operator.as_str() {
                "'" => ops.push(Operation::new(
                    "'",
                    vec![Object::String(seg.bytes.clone(), StringFormat::Literal)],
                )),
                "\"" => {
                    let mut operands = op.operands.clone();
                    if operands.len() >= 3 {
                        operands[2] = Object::String(seg.bytes.clone(), StringFormat::Literal);
                    }
                    ops.push(Operation::new("\"", operands));
                }
                _ => ops.push(Operation::new(
                    "Tj",
                    vec![Object::String(seg.bytes.clone(), StringFormat::Literal)],
                )),
            }
            first = false;
        } else {
            ops.push(Operation::new(
                "Tj",
                vec![Object::String(seg.bytes.clone(), StringFormat::Literal)],
            ));
        }
    }
    if current_fallback {
        ops.push(tf(font));
    }
    if first {
        // Every segment was empty: keep an empty operator so the positioning
        // state around the run is undisturbed.
        ops.push(rebuild_simple_op(op, Vec::new()));
    }
    ops
}
