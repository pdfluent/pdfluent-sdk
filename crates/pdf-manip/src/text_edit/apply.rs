//! Edit planning and container rebuilding (design §2, §5, §10.4).
//!
//! All rebuilding happens on cloned operator lists: nothing here mutates the
//! document. The session's commit swaps the prepared stream bytes in only
//! after every staged edit validated (prepare-then-swap, AllOrNothing).

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use lopdf::content::Operation;
use lopdf::{Object, ObjectId, StringFormat};

use crate::content_editor::ContentEditor;
use crate::text_replace::{encode_latin1, encode_text_for_font};
use crate::text_run::{FontMap, TextRun};

use super::scan::PageScan;
use super::{Diagnostic, FitPolicy, FontFallback, TextEditError};

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
    /// Fit policy for this edit.
    pub fit: FitPolicy,
}

/// Smallest fraction of the original font size ShrinkToFit will go to.
///
/// Below this, shrinking stops being a fix and starts being a different
/// defect: text that technically fits but nobody can read. The edit is
/// applied at the floor and the shortfall is reported, so the caller can see
/// that the block still overruns rather than discovering it in print.
const MIN_SHRINK: f64 = 0.5;

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
    /// Glyphs accumulated for an embedded Unicode font, when any edit on this
    /// page used [`FontFallback::EmbedUnicode`].
    ///
    /// Carried rather than applied here because `prepare_page` must not touch
    /// the document: the character codes already written into the rebuilt
    /// streams are post-subset glyph indices, so this encoder and those
    /// streams are only valid together and must land in the same swap.
    #[cfg(feature = "font-subset")]
    pub unicode_encoder: Option<crate::unicode_font::UnicodeEncoder>,
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
    #[cfg(feature = "font-subset")]
    let mut unicode_encoder: Option<crate::unicode_font::UnicodeEncoder> = None;

    // Per-run collected segments-to-replace: run index -> (range in run text,
    // Option<(replacement, staged_index, fallback)>).
    struct RunEditPart<'a> {
        range: (usize, usize),
        replacement: Option<&'a str>,
        staged_index: usize,
        fallback: &'a FontFallback,
        fit: FitPolicy,
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
                fit: edit.fit,
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
        let fit_policy = sorted.first().map_or(FitPolicy::Exact, |p| p.fit);

        match rebuild_run(
            scan,
            run,
            &segments,
            &fallback_policy,
            fit_policy,
            #[cfg(feature = "font-subset")]
            &mut unicode_encoder,
        ) {
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
            // Deliberately dropped: no streams are being swapped, so an
            // embedded font would be an orphan referenced by nothing.
            #[cfg(feature = "font-subset")]
            unicode_encoder: None,
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
        #[cfg(feature = "font-subset")]
        unicode_encoder,
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
    fit_policy: FitPolicy,
    #[cfg(feature = "font-subset")] unicode_encoder: &mut Option<
        crate::unicode_font::UnicodeEncoder,
    >,
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
                        #[cfg(feature = "font-subset")]
                        FontFallback::EmbedUnicode(font) => {
                            // One encoder per page: repeated characters across
                            // edits then share a glyph id and the subset stays
                            // as small as the page's real alphabet.
                            let enc = unicode_encoder.get_or_insert_with(|| {
                                crate::unicode_font::UnicodeEncoder::new(font.clone())
                            });
                            let bytes = enc.encode(text).map_err(|e| {
                                (
                                    *staged_index,
                                    TextEditError::EncodingFailed {
                                        match_id: None,
                                        font: font.name().to_string(),
                                        detail: e.to_string(),
                                    },
                                )
                            })?;
                            used_fallback = true;
                            fallback_font =
                                Some(crate::unicode_font::UNICODE_FONT_RESOURCE.to_string());
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

    // ShrinkToFit: measure what the replacement needs against the space the
    // original occupied, and scale the font down if it overruns. Measured, not
    // estimated — a substitute font is never exactly as wide as the one it
    // replaces, and a translation is usually longer on top of that.
    let mut fit_scale = 1.0f64;
    if fit_policy == FitPolicy::ShrinkToFit {
        let (orig_em, new_em) = measure_segments(
            run,
            segments,
            fonts,
            font,
            #[cfg(feature = "font-subset")]
            unicode_encoder.as_ref(),
        );
        if new_em > orig_em && new_em > 0.0 {
            let needed = orig_em / new_em;
            fit_scale = needed.max(MIN_SHRINK);
            if needed < MIN_SHRINK {
                diagnostics.push(Diagnostic {
                    code: "shrink-floor-reached".to_string(),
                    message: format!(
                        "replacement needs {:.0}% of the original size to fit; \
                         stopped at the {:.0}% floor, so it still overruns",
                        needed * 100.0,
                        MIN_SHRINK * 100.0
                    ),
                });
            } else {
                diagnostics.push(Diagnostic {
                    code: "shrunk-to-fit".to_string(),
                    message: format!("font size scaled to {:.0}% to fit", fit_scale * 100.0),
                });
            }
        }
    }

    // ReflowInBounds: Acrobat's behaviour — keep the font size and let the
    // text rewrap onto more lines inside the width the original occupied.
    // For a translation this reads naturally where shrinking does not: one
    // line more looks like typesetting, smaller type looks like a defect.
    if fit_policy == FitPolicy::ReflowInBounds {
        if let Some(reflowed) = reflow_ops(
            op,
            run,
            block_width_for(&scan.content.runs, run),
            segments,
            fonts,
            font,
            fallback_font.as_deref(),
            &mut diagnostics,
            #[cfg(feature = "font-subset")]
            unicode_encoder,
        ) {
            return Ok(RebuiltRun {
                ops: reflowed,
                used_fallback,
                fallback_font,
                diagnostics,
            });
        }
    }

    let ops = if encoded.iter().all(|s| !s.fallback) && fit_scale >= 1.0 {
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
            run.font_size * fit_scale,
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
    replaced_size: f64,
    fallback_name: &str,
) -> Vec<Operation> {
    let tf = |name: &str, size: f64| {
        Operation::new(
            "Tf",
            vec![
                Object::Name(name.as_bytes().to_vec()),
                Object::Real(size as f32),
            ],
        )
    };

    let mut ops = Vec::new();
    // `None` = no Tf emitted yet, so the first segment always states its font.
    let mut current: Option<(bool, u64)> = None;
    let mut first = true;

    for seg in encoded {
        if seg.bytes.is_empty() {
            continue;
        }
        // Kept text keeps the run's own size; only replaced text is scaled,
        // so shrinking one phrase never resizes the sentence around it.
        let size = if seg.fallback {
            replaced_size
        } else {
            font_size
        };
        let key = (seg.fallback, size.to_bits());
        if current != Some(key) {
            ops.push(tf(if seg.fallback { fallback_name } else { font }, size));
            current = Some(key);
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
    // Restore the run's own font and size if the last piece changed either,
    // so the text state after this run is what the rest of the stream expects.
    if current.is_some_and(|(fb, size)| fb || size != font_size.to_bits()) {
        ops.push(tf(font, font_size));
    }
    if first {
        // Every segment was empty: keep an empty operator so the positioning
        // state around the run is undisturbed.
        ops.push(rebuild_simple_op(op, Vec::new()));
    }
    ops
}

/// Width of the original text versus the replacement, both in em units
/// (1.0 = one font size), for the segments of one run.
///
/// Kept text is measured once and counted on both sides — it is unchanged, so
/// it cancels out of the comparison but still occupies space the replacement
/// has to share.
fn measure_segments(
    run: &TextRun,
    segments: &[Segment],
    fonts: &FontMap,
    font: &str,
    #[cfg(feature = "font-subset")] unicode_encoder: Option<&crate::unicode_font::UnicodeEncoder>,
) -> (f64, f64) {
    let original_em = fonts.text_width_em(font, &run.text);
    let mut new_em = 0.0;

    for segment in segments {
        match segment {
            Segment::Kept { start, end } => {
                new_em += fonts.text_width_em(font, &run.text[*start..*end]);
            }
            Segment::Repl { text, .. } => {
                // Measure with the font the text will actually be drawn in.
                // Measuring a Cyrillic replacement against the Latin font it
                // replaces would compare against widths that do not exist.
                #[cfg(feature = "font-subset")]
                let width = match unicode_encoder {
                    Some(enc) => enc.width_em(text),
                    None => fonts.text_width_em(font, text),
                };
                #[cfg(not(feature = "font-subset"))]
                let width = fonts.text_width_em(font, text);
                new_em += width;
            }
        }
    }

    (original_em, new_em)
}

/// Line advance as a multiple of the font size, used when reflow adds lines.
///
/// The run itself carries no leading (`TL` is document state, not run state),
/// so a conventional single-spacing factor is the honest default. It matches
/// what most producers emit for body text.
const REFLOW_LEADING: f64 = 1.2;

/// The width available for reflow: the enclosing text block, not just the run.
///
/// WHY THIS EXISTS
///
/// Reflow used the width of the run being replaced. When a whole line is
/// replaced — the translation case — that is the right answer and this changes
/// nothing. When a short phrase inside a wide column is replaced, the run is a
/// fraction of the column, so the text wrapped far earlier than the page had
/// room for. Acrobat reflows inside a text frame it infers from the page; this
/// is the same idea, kept deliberately timid.
///
/// PDF has no paragraphs. Everything here is inference from geometry, so the
/// rules are conservative and the failure mode is chosen: when the evidence is
/// weak we return the run's own width and behave exactly as before.
///
///   * same font size (within 5%) — a size change usually marks a different
///     role, e.g. a heading above body text;
///   * consecutive baselines separated by 0.8–2.0 × font size — a plausible
///     leading, not an unrelated element that happens to sit nearby;
///   * horizontally overlapping by at least half the narrower run — same
///     column, not the next column across;
///   * at least two runs. One run is not evidence of a block.
///
/// The result is never narrower than the run itself, so this can only ever give
/// reflow *more* room. That bound is what makes the heuristic safe to ship: the
/// worst case is the behaviour we already had.
fn block_width_for(runs: &[TextRun], target: &TextRun) -> f64 {
    const SIZE_TOLERANCE: f64 = 0.05;
    const MIN_LEADING: f64 = 0.8;
    const MAX_LEADING: f64 = 2.0;
    const MIN_OVERLAP: f64 = 0.5;

    if target.font_size <= 0.0 {
        return target.width;
    }

    let overlaps = |a: &TextRun, b: &TextRun| {
        let (a0, a1) = (a.x, a.x + a.width);
        let (b0, b1) = (b.x, b.x + b.width);
        let shared = a1.min(b1) - a0.max(b0);
        let narrower = a.width.min(b.width);
        narrower > 0.0 && shared / narrower >= MIN_OVERLAP
    };
    let same_size = |a: &TextRun, b: &TextRun| {
        (a.font_size - b.font_size).abs() <= b.font_size * SIZE_TOLERANCE
    };
    let plausible_leading = |a: &TextRun, b: &TextRun| {
        let gap = (a.y - b.y).abs();
        gap >= b.font_size * MIN_LEADING && gap <= b.font_size * MAX_LEADING
    };

    // Walk outward from the target line by line. Stopping at the first row that
    // does not qualify keeps an unrelated block further down the page from
    // being pulled in through a chain of coincidences.
    let mut block: Vec<&TextRun> = vec![target];
    let mut frontier_up = target;
    let mut frontier_down = target;
    let mut grew = true;
    while grew {
        grew = false;
        for r in runs {
            if std::ptr::eq(r, target) || block.iter().any(|b| std::ptr::eq(*b, r)) {
                continue;
            }
            if !same_size(r, target) {
                continue;
            }
            if r.y > frontier_up.y && plausible_leading(r, frontier_up) && overlaps(r, frontier_up)
            {
                block.push(r);
                frontier_up = r;
                grew = true;
            } else if r.y < frontier_down.y
                && plausible_leading(r, frontier_down)
                && overlaps(r, frontier_down)
            {
                block.push(r);
                frontier_down = r;
                grew = true;
            }
        }
    }

    if block.len() < 2 {
        return target.width;
    }

    let left = block.iter().map(|r| r.x).fold(f64::INFINITY, f64::min);
    let right = block
        .iter()
        .map(|r| r.x + r.width)
        .fold(f64::NEG_INFINITY, f64::max);
    let width = right - left;

    // Never narrower than the run: this may only add room.
    if width.is_finite() && width > target.width {
        width
    } else {
        target.width
    }
}

/// Rebuild a run as several stacked lines that fit the original width, keeping
/// the font size — the behaviour Acrobat has when you edit inside a text box.
///
/// Returns `None` when reflow does not apply (no replacement, or it already
/// fits), so the caller falls through to the ordinary single-line path.
///
/// Like Acrobat, this does **not** move anything else on the page: added lines
/// can fall over content below. That is reported rather than hidden, because
/// the alternative — repositioning unrelated content — is a far more invasive
/// change than the caller asked for.
#[allow(clippy::too_many_arguments)]
fn reflow_ops(
    op: &Operation,
    run: &TextRun,
    // Width to wrap inside, in user-space units: the enclosing block when we
    // could infer one, otherwise the run's own width.
    available_width: f64,
    segments: &[Segment],
    fonts: &FontMap,
    font: &str,
    fallback_font: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
    #[cfg(feature = "font-subset")] unicode_encoder: &mut Option<
        crate::unicode_font::UnicodeEncoder,
    >,
) -> Option<Vec<Operation>> {
    // Reflow only makes sense for a run that is one replacement, which is the
    // translation case. A run with kept text on either side would need the
    // surrounding words re-laid out too, and that is a different problem.
    let replacement = match segments {
        [Segment::Repl { text, .. }] if !text.is_empty() => text.as_str(),
        _ => return None,
    };

    // Space available for the rewrapped text.
    //
    // Measured in em so it can be compared against measure() below, which works
    // in em too. The run's text gives us the em-per-user-unit scale; the width
    // itself comes from the enclosing block when one could be inferred, which
    // is what stops a short phrase in a wide column from wrapping at the
    // phrase's own width. See block_width_for.
    let run_em = fonts.text_width_em(font, &run.text);
    if run_em <= 0.0 || run.width <= 0.0 {
        return None;
    }
    let available_em = run_em * (available_width / run.width);
    if available_em <= 0.0 {
        return None;
    }

    #[cfg(feature = "font-subset")]
    let measure = |t: &str| match unicode_encoder.as_ref() {
        Some(enc) => enc.width_em(t),
        None => fonts.text_width_em(font, t),
    };
    #[cfg(not(feature = "font-subset"))]
    let measure = |t: &str| fonts.text_width_em(font, t);

    if measure(replacement) <= available_em {
        return None; // fits as one line; nothing to reflow
    }

    // Greedy wrap on whitespace. A word longer than the line gets its own line
    // and overruns: breaking inside a word would need hyphenation rules per
    // language, and a wrong hyphen is worse than a long line.
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    for word in replacement.split_whitespace() {
        let candidate = if current.is_empty() {
            word.to_string()
        } else {
            format!("{current} {word}")
        };
        if measure(&candidate) <= available_em || current.is_empty() {
            current = candidate;
        } else {
            lines.push(std::mem::take(&mut current));
            current = word.to_string();
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.len() <= 1 {
        return None;
    }

    let target_font = fallback_font.unwrap_or(font);
    let advance = -(run.font_size * REFLOW_LEADING);
    let mut ops = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let bytes = {
            #[cfg(feature = "font-subset")]
            {
                match unicode_encoder.as_mut() {
                    Some(enc) => enc.encode(line).ok()?,
                    None => encode_text_for_font(font, line, fonts).ok()?,
                }
            }
            #[cfg(not(feature = "font-subset"))]
            {
                encode_text_for_font(font, line, fonts).ok()?
            }
        };

        if i == 0 {
            if fallback_font.is_some() {
                ops.push(Operation::new(
                    "Tf",
                    vec![
                        Object::Name(target_font.as_bytes().to_vec()),
                        Object::Real(run.font_size as f32),
                    ],
                ));
            }
            ops.push(rebuild_simple_op(op, bytes));
        } else {
            // Move down one line, then draw. Td is relative to the current
            // line start, so each step is one advance rather than cumulative.
            ops.push(Operation::new(
                "Td",
                vec![Object::Real(0.0), Object::Real(advance as f32)],
            ));
            ops.push(Operation::new(
                "Tj",
                vec![Object::String(bytes, StringFormat::Literal)],
            ));
        }
    }

    // Put the text position back where the caller's stream expects it, or
    // everything after this run drifts down the page.
    let restore = -advance * (lines.len() - 1) as f64;
    ops.push(Operation::new(
        "Td",
        vec![Object::Real(0.0), Object::Real(restore as f32)],
    ));
    if fallback_font.is_some() {
        ops.push(Operation::new(
            "Tf",
            vec![
                Object::Name(font.as_bytes().to_vec()),
                Object::Real(run.font_size as f32),
            ],
        ));
    }

    diagnostics.push(Diagnostic {
        code: "reflowed".to_string(),
        message: format!(
            "replacement rewrapped onto {} lines at the original size; \
             added lines may overlap content below, which is not moved",
            lines.len()
        ),
    });

    Some(ops)
}
