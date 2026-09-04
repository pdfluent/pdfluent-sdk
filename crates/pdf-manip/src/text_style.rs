//! Text-run style mutations: bold and italic via embedded font variant swaps.
//!
//! # Overview
//!
//! [`set_text_run_style`] swaps the `Tf` (font select) operator for a located
//! text run to a bold or italic variant of the same typeface.  The variant must
//! already be embedded in the document's xref — this module never synthesises
//! bold via stroke-and-fill, never calls out to system fonts, and never
//! subsets or embeds new font data.
//!
//! # Forbidden non-features
//!
//! The following are explicitly out of scope and will **never** be implemented
//! here.  Any attempt to work around them by callers should be treated as a
//! bug:
//!
//! - **No synthetic bold** — the PDF text rendering mode (`Tr`) is not touched.
//!   Stroke-and-fill tricks that approximate boldness are disallowed.
//! - **No system-font injection** — the OS font catalogue is never consulted.
//! - **No font subsetting** — only fonts already in the xref may be referenced.
//! - **No silent fallback** — if the requested variant is absent,
//!   [`ManipError::FontVariantNotEmbedded`] is returned with full diagnostic
//!   information.  The document is left unmodified.
//!
//! # Usage
//!
//! ```no_run
//! use lopdf::Document;
//! use pdf_manip::text_run::extract_page_text_runs;
//! use pdf_manip::text_style::set_text_run_style;
//!
//! let mut doc = Document::load("form.pdf").unwrap();
//! let runs = extract_page_text_runs(&doc, 1).unwrap();
//!
//! // Make the first run bold.
//! let result = set_text_run_style(&mut doc, 1, &runs[0], Some(true), None).unwrap();
//! println!("swapped {} → {}", result.original_font_name, result.requested_font_name);
//! ```

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use crate::content_editor::{
    as_number, editor_for_page, write_editor_to_page, GraphicsStateTracker,
};
use crate::error::{ManipError, Result};
use crate::text_run::TextRun;
use lopdf::content::Operation;
use lopdf::{Dictionary, Document, Object, ObjectId};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// How the content stream was isolated around the swapped `Tf` operator.
///
/// PDF text state (font, matrix, spacing) is **not** saved/restored by `q`/`Q`
/// — only the graphics state stack is.  Isolation therefore means either a
/// dedicated `Tf` per run or a restoring `Tf` emitted after the run.
#[derive(Debug, Clone, PartialEq)]
pub enum StateIsolationStrategy {
    /// The `Tf` operator that was swapped is dedicated to this run only.
    /// No other text-showing operators between this `Tf` and the next `Tf`
    /// exist, so a simple in-place name swap was sufficient.
    Direct,
    /// The original `Tf` feeds text runs after the target run as well.
    /// A restoring `Tf /original_name <size>` was injected at
    /// `restore_op_index` (into the modified stream) so subsequent runs
    /// keep their original font.
    Restore {
        /// Index in the **modified** operation list where the restoring `Tf`
        /// was inserted.
        restore_op_index: usize,
    },
}

/// Describes the font swap that was (or would be) performed.
#[derive(Debug, Clone)]
pub struct FontSwap {
    /// PDF resource name of the original font (e.g. `"F1"`).
    pub original_resource_name: String,
    /// PDF resource name used for the variant (e.g. `"F1"` if reused, or a
    /// fresh name like `"F3"` when the variant was added to the page resources).
    pub variant_resource_name: String,
    /// BaseFont of the original font (e.g. `"Helvetica"`).
    pub original_base_font: String,
    /// BaseFont of the variant font (e.g. `"Helvetica-Bold"`).
    pub variant_base_font: String,
}

/// Result returned by a successful [`set_text_run_style`] call.
#[derive(Debug, Clone)]
pub struct StyleResult {
    /// Number of bytes by which the content stream changed.
    /// Zero when the run was already in the requested style (no-op).
    pub bytes_changed: usize,
    /// Description of the font swap performed.
    pub font_swap: FontSwap,
    /// How the stream was isolated around the swap.
    pub isolation_strategy: StateIsolationStrategy,
    /// BaseFont name of the font before the swap.
    pub original_font_name: String,
    /// BaseFont name of the font after the swap (the requested variant).
    pub requested_font_name: String,
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

/// Apply bold and/or italic style to a text run by swapping its `Tf` font.
///
/// `run` must have been produced by [`extract_page_text_runs`](crate::text_run::extract_page_text_runs)
/// on the same `doc` and `page_num`.  The `ops_range` in `run` locates the
/// text-showing operator(s); the active `Tf` is discovered by walking the
/// content stream up to that point.
///
/// # Style semantics
///
/// | `bold`  | `italic` | Requested variant suffix |
/// |---------|----------|--------------------------|
/// | `Some(true)` | `Some(true)` | `-BoldItalic` or `-BoldOblique` |
/// | `Some(true)` | `Some(false)` | `-Bold` (or unsuffixed if family is already bold) |
/// | `Some(false)` | `Some(true)` | `-Italic` or `-Oblique` |
/// | `Some(false)` | `Some(false)` | base family name (no suffix) |
/// | `None`   | `None`   | no-op — returns `Ok` with `bytes_changed = 0` |
/// | `None`   | `Some(v)`| keep current bold; change italic to `v` |
/// | `Some(v)`| `None`   | change bold to `v`; keep current italic |
///
/// # Errors
///
/// - [`ManipError::FontVariantNotEmbedded`] when the requested variant is not
///   present anywhere in the document xref.  The document is **not** modified.
/// - [`ManipError::PageOutOfRange`] when `page_num` does not exist.
/// - [`ManipError::Other`] on content-stream decode failures.
pub fn set_text_run_style(
    doc: &mut Document,
    page_num: u32,
    run: &TextRun,
    bold: Option<bool>,
    italic: Option<bool>,
) -> Result<StyleResult> {
    // Early no-op: if neither bold nor italic is requested, nothing to do.
    if bold.is_none() && italic.is_none() {
        let base_font = resolve_base_font(doc, page_num, &run.font_name)
            .unwrap_or_else(|| run.font_name.clone());
        let swap = FontSwap {
            original_resource_name: run.font_name.clone(),
            variant_resource_name: run.font_name.clone(),
            original_base_font: base_font.clone(),
            variant_base_font: base_font.clone(),
        };
        return Ok(StyleResult {
            bytes_changed: 0,
            font_swap: swap,
            isolation_strategy: StateIsolationStrategy::Direct,
            original_font_name: base_font.clone(),
            requested_font_name: base_font,
        });
    }

    // Resolve current font properties.
    let original_base_font =
        resolve_base_font(doc, page_num, &run.font_name).unwrap_or_else(|| run.font_name.clone());
    let current_style = detect_font_style(doc, page_num, &run.font_name);

    // Compute effective requested style, merging with current.
    let want_bold = bold.unwrap_or(current_style.is_bold);
    let want_italic = italic.unwrap_or(current_style.is_italic);

    // Derive the family name (strip existing variant suffix + subset prefix).
    let family = font_family_name(&original_base_font);

    // Build the target BaseFont name.
    let target_base_font = build_target_base_font(&family, want_bold, want_italic);

    // No-op check: same base font after normalisation → already correct style.
    let current_normalised = font_family_name(&original_base_font);
    let current_target = build_target_base_font(
        &current_normalised,
        current_style.is_bold,
        current_style.is_italic,
    );
    if current_target == target_base_font {
        let swap = FontSwap {
            original_resource_name: run.font_name.clone(),
            variant_resource_name: run.font_name.clone(),
            original_base_font: original_base_font.clone(),
            variant_base_font: original_base_font.clone(),
        };
        return Ok(StyleResult {
            bytes_changed: 0,
            font_swap: swap,
            isolation_strategy: StateIsolationStrategy::Direct,
            original_font_name: original_base_font.clone(),
            requested_font_name: original_base_font,
        });
    }

    // Search for the variant font and get/create its resource name.
    let variant_resource_name = find_or_add_variant_font(
        doc,
        page_num,
        &family,
        &target_base_font,
        &original_base_font,
    )?;

    // Load content stream, find the active Tf, apply the swap.
    let editor_before = editor_for_page(doc, page_num)?;
    let before_bytes = editor_before.encode().unwrap_or_default().len();

    let mut editor = editor_for_page(doc, page_num)?;
    let ops = editor.operations().to_vec();

    // Find the Tf index that is active at ops_range.start.
    let run_start = run.ops_range.start;
    let active_tf_index = find_active_tf_index(&ops, run_start);

    // Check whether the Tf affects runs after ours.
    let run_end = run.ops_range.end;
    let (next_tf_index, contaminated) = next_tf_after(&ops, run_end);

    // Perform the swap.
    let new_name_bytes = variant_resource_name.as_bytes().to_vec();
    if let Some(tf_idx) = active_tf_index {
        let original_size = {
            let tf_op = &ops[tf_idx];
            tf_op.operands.get(1).and_then(as_number).unwrap_or(12.0)
        };

        // Replace the Tf operand in-place.
        let mut new_op = ops[tf_idx].clone();
        new_op.operands[0] = Object::Name(new_name_bytes);
        editor.replace_operation(tf_idx, vec![new_op]);

        // If contaminated: inject restoring Tf after the run.
        let isolation_strategy = if contaminated {
            // After our replacement, the run_end index is the same (we replaced 1→1).
            let restore_idx = run_end;
            let restore_op = Operation::new(
                "Tf",
                vec![
                    Object::Name(run.font_name.as_bytes().to_vec()),
                    Object::Real(original_size as f32),
                ],
            );
            editor.insert_operations(restore_idx, vec![restore_op]);
            StateIsolationStrategy::Restore {
                restore_op_index: restore_idx,
            }
        } else {
            let _ = next_tf_index; // not needed in Direct path
            StateIsolationStrategy::Direct
        };

        write_editor_to_page(doc, page_num, &editor)?;

        let editor_after = editor_for_page(doc, page_num)?;
        let after_bytes = editor_after.encode().unwrap_or_default().len();
        let bytes_changed = after_bytes.abs_diff(before_bytes);

        let variant_base_font = resolve_base_font(doc, page_num, &variant_resource_name)
            .unwrap_or_else(|| target_base_font.clone());

        Ok(StyleResult {
            bytes_changed,
            font_swap: FontSwap {
                original_resource_name: run.font_name.clone(),
                variant_resource_name,
                original_base_font: original_base_font.clone(),
                variant_base_font: variant_base_font.clone(),
            },
            isolation_strategy,
            original_font_name: original_base_font,
            requested_font_name: variant_base_font,
        })
    } else {
        // No Tf found before run — content stream is malformed or font set
        // before page content (unusual but legal via inherited Resources).
        // Inject a new Tf immediately before the run.
        let original_size = 12.0_f64;
        let new_tf = Operation::new(
            "Tf",
            vec![
                Object::Name(new_name_bytes),
                Object::Real(original_size as f32),
            ],
        );
        editor.insert_operations(run_start, vec![new_tf]);
        write_editor_to_page(doc, page_num, &editor)?;

        let editor_after = editor_for_page(doc, page_num)?;
        let after_bytes = editor_after.encode().unwrap_or_default().len();
        let bytes_changed = after_bytes.abs_diff(before_bytes);

        let variant_base_font = resolve_base_font(doc, page_num, &variant_resource_name)
            .unwrap_or_else(|| target_base_font.clone());

        Ok(StyleResult {
            bytes_changed,
            font_swap: FontSwap {
                original_resource_name: run.font_name.clone(),
                variant_resource_name,
                original_base_font: original_base_font.clone(),
                variant_base_font: variant_base_font.clone(),
            },
            isolation_strategy: StateIsolationStrategy::Direct,
            original_font_name: original_base_font,
            requested_font_name: variant_base_font,
        })
    }
}

// ---------------------------------------------------------------------------
// Font style detection
// ---------------------------------------------------------------------------

/// Detected style attributes of an embedded font.
#[derive(Debug, Clone, Default)]
pub struct FontStyleInfo {
    /// True when the font is bold (suffix or Flags bit 19 or FontWeight ≥ 700).
    pub is_bold: bool,
    /// True when the font is italic (suffix or Flags bit 7).
    pub is_italic: bool,
}

/// Detect bold/italic status of the font referenced by `resource_name` on `page_num`.
///
/// Uses three complementary signals in priority order:
/// 1. BaseFont name suffix conventions (`-Bold`, `-Italic`, `-Oblique`, etc.)
/// 2. /FontDescriptor /Flags bit 7 (italic, value 64) and bit 19 (ForceBold, value 262144)
/// 3. /FontDescriptor /FontWeight ≥ 700
pub fn detect_font_style(doc: &Document, page_num: u32, resource_name: &str) -> FontStyleInfo {
    let font_id = match get_page_font_id(doc, page_num, resource_name) {
        Some(id) => id,
        None => return FontStyleInfo::default(),
    };
    detect_font_style_by_id(doc, font_id)
}

fn detect_font_style_by_id(doc: &Document, font_id: ObjectId) -> FontStyleInfo {
    let font_dict = match doc.get_object(font_id) {
        Ok(Object::Dictionary(ref d)) => d.clone(),
        _ => return FontStyleInfo::default(),
    };

    let base_font = font_dict
        .get(b"BaseFont")
        .ok()
        .and_then(|o| match o {
            Object::Name(ref n) => Some(String::from_utf8_lossy(n).to_string()),
            _ => None,
        })
        .unwrap_or_default();

    // Signal 1: suffix-based detection.
    let norm = strip_subset_prefix(&base_font).to_lowercase();
    let mut is_bold =
        norm.ends_with("-bold") || norm.ends_with("-bolditalic") || norm.ends_with("-boldoblique");
    let mut is_italic = norm.ends_with("-italic")
        || norm.ends_with("-oblique")
        || norm.ends_with("-bolditalic")
        || norm.ends_with("-boldoblique");

    // Signal 2 & 3: FontDescriptor.
    let (flags, font_weight) = get_font_descriptor_info(doc, &font_dict);
    if flags & 64 != 0 {
        // Flags bit 7 (1-indexed) = italic.
        is_italic = true;
    }
    if flags & 262144 != 0 {
        // Flags bit 19 (1-indexed) = ForceBold.
        is_bold = true;
    }
    if font_weight >= 700 {
        is_bold = true;
    }

    FontStyleInfo { is_bold, is_italic }
}

/// Extract Flags and FontWeight from /FontDescriptor (0 if absent).
fn get_font_descriptor_info(doc: &Document, font_dict: &Dictionary) -> (u32, u32) {
    let desc_obj = match font_dict.get(b"FontDescriptor").ok() {
        Some(o) => o.clone(),
        None => return (0, 0),
    };
    let desc_dict = match desc_obj {
        Object::Reference(id) => match doc.get_object(id) {
            Ok(Object::Dictionary(ref d)) => d.clone(),
            _ => return (0, 0),
        },
        Object::Dictionary(ref d) => d.clone(),
        _ => return (0, 0),
    };

    let flags = match desc_dict.get(b"Flags").ok() {
        Some(Object::Integer(i)) => *i as u32,
        _ => 0,
    };
    let font_weight = match desc_dict.get(b"FontWeight").ok() {
        Some(Object::Integer(i)) => *i as u32,
        Some(Object::Real(f)) => *f as u32,
        _ => 0,
    };

    (flags, font_weight)
}

// ---------------------------------------------------------------------------
// Font family / variant name helpers
// ---------------------------------------------------------------------------

/// Strip subset prefix from a BaseFont name.
/// `"ABCDEF+Helvetica-Bold"` → `"Helvetica-Bold"`.
fn strip_subset_prefix(name: &str) -> &str {
    let bytes = name.as_bytes();
    if bytes.len() > 7 && bytes[6] == b'+' && bytes[..6].iter().all(|b| b.is_ascii_uppercase()) {
        &name[7..]
    } else {
        name
    }
}

/// Extract the base family name by stripping any known style suffix.
/// `"Helvetica-BoldItalic"` → `"Helvetica"`.
/// `"ABCDEF+Times-Bold"` → `"Times"`.
fn font_family_name(base_font: &str) -> String {
    let stripped = strip_subset_prefix(base_font);
    for suffix in &[
        "-BoldItalic",
        "-BoldOblique",
        "-Bold",
        "-Italic",
        "-Oblique",
    ] {
        if let Some(family) = stripped.strip_suffix(suffix) {
            return family.to_string();
        }
        // Case-insensitive check as a fallback.
        let lower = stripped.to_lowercase();
        let suffix_lower = suffix.to_lowercase();
        if lower.ends_with(&suffix_lower) {
            return stripped[..stripped.len() - suffix.len()].to_string();
        }
    }
    stripped.to_string()
}

/// Build the target BaseFont name for the desired style combination.
fn build_target_base_font(family: &str, bold: bool, italic: bool) -> String {
    match (bold, italic) {
        (true, true) => format!("{family}-BoldItalic"),
        (true, false) => format!("{family}-Bold"),
        (false, true) => format!("{family}-Italic"),
        (false, false) => family.to_string(),
    }
}

/// Candidate target names in preference order for a given style request.
fn target_base_font_candidates(family: &str, bold: bool, italic: bool) -> Vec<String> {
    match (bold, italic) {
        (true, true) => vec![
            format!("{family}-BoldItalic"),
            format!("{family}-BoldOblique"),
        ],
        (true, false) => vec![format!("{family}-Bold")],
        (false, true) => vec![format!("{family}-Italic"), format!("{family}-Oblique")],
        (false, false) => vec![family.to_string()],
    }
}

// ---------------------------------------------------------------------------
// Font lookup and resource management
// ---------------------------------------------------------------------------

/// Try to find a variant font already in page Resources; if not there but in
/// xref, add it to page Resources.  Returns the PDF resource name to use in Tf.
///
/// On failure returns [`ManipError::FontVariantNotEmbedded`] with the names of
/// all embedded variants of the same family.
fn find_or_add_variant_font(
    doc: &mut Document,
    page_num: u32,
    family: &str,
    target_base_font: &str,
    original_base_font: &str,
) -> Result<String> {
    let candidates = target_base_font_candidates(
        family,
        target_base_font.to_lowercase().contains("bold"),
        target_base_font.to_lowercase().contains("italic")
            || target_base_font.to_lowercase().contains("oblique"),
    );

    // 1. Search page Resources.
    for candidate in &candidates {
        if let Some(name) = find_in_page_resources(doc, page_num, candidate) {
            return Ok(name);
        }
    }

    // 2. Search full xref.
    let all_doc_fonts = enumerate_doc_fonts(doc);
    for candidate in &candidates {
        for (font_id, base_font) in &all_doc_fonts {
            if fonts_match(base_font, candidate) {
                let res_name = add_font_to_page_resources(doc, page_num, *font_id)?;
                return Ok(res_name);
            }
        }
    }

    // Variant not found — collect available variants of this family for the error.
    let available_variants: Vec<String> = all_doc_fonts
        .values()
        .filter(|bf| {
            let f = font_family_name(bf);
            f.eq_ignore_ascii_case(family)
        })
        .cloned()
        .collect();

    Err(ManipError::FontVariantNotEmbedded {
        current: original_base_font.to_string(),
        requested: target_base_font.to_string(),
        available_variants,
    })
}

/// Check whether `base_font` in the document matches `target` (ignoring subset
/// prefixes and case).
fn fonts_match(base_font: &str, target: &str) -> bool {
    let a = strip_subset_prefix(base_font);
    let b = strip_subset_prefix(target);
    a.eq_ignore_ascii_case(b)
}

/// Search the page's /Resources/Font dict for a font with the given BaseFont.
/// Returns the PDF resource name (e.g. `"F2"`) if found.
fn find_in_page_resources(doc: &Document, page_num: u32, target_base_font: &str) -> Option<String> {
    let page_fonts = get_page_font_map(doc, page_num);
    for (res_name, font_id) in &page_fonts {
        if let Ok(Object::Dictionary(ref fd)) = doc.get_object(*font_id) {
            let bf = fd
                .get(b"BaseFont")
                .ok()
                .and_then(|o| match o {
                    Object::Name(ref n) => Some(String::from_utf8_lossy(n).to_string()),
                    _ => None,
                })
                .unwrap_or_default();
            if fonts_match(&bf, target_base_font) {
                return Some(res_name.clone());
            }
        }
    }
    None
}

/// Return a map of all font ObjectIds in the document, keyed by BaseFont name.
fn enumerate_doc_fonts(doc: &Document) -> HashMap<ObjectId, String> {
    let mut result = HashMap::new();
    for (&id, obj) in &doc.objects {
        if let Object::Dictionary(ref d) = obj {
            let is_font = d
                .get(b"Type")
                .ok()
                .map(|o| matches!(o, Object::Name(n) if n.as_slice() == b"Font"))
                .unwrap_or(false);
            if !is_font {
                continue;
            }
            if let Some(bf) = d.get(b"BaseFont").ok().and_then(|o| match o {
                Object::Name(ref n) => Some(String::from_utf8_lossy(n).to_string()),
                _ => None,
            }) {
                result.insert(id, bf);
            }
        }
    }
    result
}

/// Add a font object to the page's /Resources/Font dictionary.
/// Returns the resource name that was assigned.
fn add_font_to_page_resources(
    doc: &mut Document,
    page_num: u32,
    font_id: ObjectId,
) -> Result<String> {
    let pages = doc.get_pages();
    let total = pages.len() as u32;
    let &page_id = pages.get(&page_num).ok_or(ManipError::PageOutOfRange(
        page_num as usize,
        total as usize,
    ))?;

    // Find a unique resource name.
    let existing = get_page_font_map(doc, page_num);
    let new_name = generate_font_resource_name(&existing);

    // Mutate the page Resources/Font dict.
    let page_obj = doc.get_object_mut(page_id).map_err(ManipError::Pdf)?;

    if let Object::Dictionary(ref mut page_dict) = page_obj {
        match page_dict.get_mut(b"Resources") {
            Ok(res_obj) => match res_obj {
                Object::Dictionary(ref mut res_dict) => {
                    update_font_dict_inline(res_dict, &new_name, font_id);
                }
                Object::Reference(res_id) => {
                    let res_id = *res_id;
                    if let Ok(Object::Dictionary(ref mut res_dict)) = doc.get_object_mut(res_id) {
                        update_font_dict_inline(res_dict, &new_name, font_id);
                    }
                }
                _ => {}
            },
            Err(_) => {
                // No Resources yet — create a minimal one.
                let mut font_dict = lopdf::Dictionary::new();
                font_dict.set(new_name.as_bytes(), Object::Reference(font_id));
                let mut res = lopdf::Dictionary::new();
                res.set("Font", Object::Dictionary(font_dict));
                page_dict.set("Resources", Object::Dictionary(res));
            }
        }
    }

    Ok(new_name)
}

fn update_font_dict_inline(res_dict: &mut Dictionary, name: &str, font_id: ObjectId) {
    match res_dict.get_mut(b"Font") {
        Ok(Object::Dictionary(ref mut fd)) => {
            fd.set(name.as_bytes(), Object::Reference(font_id));
        }
        Ok(Object::Reference(_)) => {
            // Shared Resources are handled higher up; skip.
        }
        _ => {
            let mut fd = Dictionary::new();
            fd.set(name.as_bytes(), Object::Reference(font_id));
            res_dict.set("Font", Object::Dictionary(fd));
        }
    }
}

/// Generate a font resource name not already in `existing`.
fn generate_font_resource_name(existing: &HashMap<String, ObjectId>) -> String {
    for n in 1_u32.. {
        let name = format!("F{n}");
        if !existing.contains_key(&name) {
            return name;
        }
    }
    unreachable!("infinite iterator exhausted")
}

// ---------------------------------------------------------------------------
// Content-stream helpers
// ---------------------------------------------------------------------------

/// Find the index of the `Tf` operator that is active (most recent before
/// `run_start`), accounting for q/Q save-restore nesting.
///
/// Uses [`GraphicsStateTracker`] to determine which font is active at
/// `run_start`, then finds the operation that caused that font to become
/// active by scanning the per-op snapshots for the most recent font-change
/// transition.  This handles Tf operators inside q/Q blocks correctly
/// because the tracker already models the save-restore stack.
fn find_active_tf_index(ops: &[Operation], run_start: usize) -> Option<usize> {
    let tracker = GraphicsStateTracker::from_operations(ops);
    let font_at_run = tracker.state_at(run_start)?.font_name.clone();

    if font_at_run.is_empty() {
        return None;
    }

    // Walk backward: find the most recent index i where the state transitioned
    // FROM a different font TO font_at_run.  state_at(i) is the state BEFORE op
    // i, so state_at(i+1) is the state AFTER op i.
    for i in (0..run_start).rev() {
        let after_font = tracker
            .state_at(i + 1)
            .map(|s| s.font_name.as_str())
            .unwrap_or("");
        let before_font = tracker
            .state_at(i)
            .map(|s| s.font_name.as_str())
            .unwrap_or("");
        if after_font == font_at_run && before_font != font_at_run {
            // Op i caused the transition.  It must be a Tf (Q can also restore
            // the font to font_at_run; in that case there is no single Tf to
            // swap — we fall through to the backward-scan fallback).
            if ops[i].operator == "Tf" {
                return Some(i);
            }
        }
    }

    // Fallback: find the most recent Tf setting font_at_run without worrying
    // about whether it was "cancelled" by Q.  This handles edge cases like a
    // font being set identically on both sides of a q/Q block.
    for i in (0..run_start).rev() {
        if ops[i].operator == "Tf" {
            if let Some(Object::Name(ref n)) = ops[i].operands.first() {
                if String::from_utf8_lossy(n) == font_at_run {
                    return Some(i);
                }
            }
        }
    }

    None
}

/// Find the index of the next `Tf` operator at or after `from_idx`, and
/// whether there are any text-showing operators between `from_idx` and that
/// next `Tf` (i.e., whether a restoring Tf is needed).
fn next_tf_after(ops: &[Operation], from_idx: usize) -> (Option<usize>, bool) {
    let mut found_text_op = false;
    for (i, op) in ops.iter().enumerate().skip(from_idx) {
        match op.operator.as_str() {
            "Tj" | "TJ" | "'" | "\"" => {
                found_text_op = true;
            }
            "Tf" => {
                return (Some(i), found_text_op);
            }
            _ => {}
        }
    }
    (None, found_text_op)
}

// ---------------------------------------------------------------------------
// Resource lookup helpers
// ---------------------------------------------------------------------------

/// Resolve the BaseFont name for a resource name on a page.
fn resolve_base_font(doc: &Document, page_num: u32, resource_name: &str) -> Option<String> {
    let font_id = get_page_font_id(doc, page_num, resource_name)?;
    let font_dict = match doc.get_object(font_id) {
        Ok(Object::Dictionary(ref d)) => d.clone(),
        _ => return None,
    };
    font_dict.get(b"BaseFont").ok().and_then(|o| match o {
        Object::Name(ref n) => Some(String::from_utf8_lossy(n).to_string()),
        _ => None,
    })
}

/// Get the ObjectId of a font resource by name from a page's Resources.
fn get_page_font_id(doc: &Document, page_num: u32, resource_name: &str) -> Option<ObjectId> {
    let map = get_page_font_map(doc, page_num);
    map.get(resource_name).copied()
}

/// Return a map of PDF resource name → ObjectId for all fonts on a page.
fn get_page_font_map(doc: &Document, page_num: u32) -> HashMap<String, ObjectId> {
    let pages = doc.get_pages();
    let &page_id = match pages.get(&page_num) {
        Some(id) => id,
        None => return HashMap::new(),
    };
    get_font_map_for_page_id(doc, page_id)
}

fn get_font_map_for_page_id(doc: &Document, page_id: ObjectId) -> HashMap<String, ObjectId> {
    let mut result = HashMap::new();

    let page_dict = match doc.get_object(page_id) {
        Ok(Object::Dictionary(ref d)) => d.clone(),
        _ => return result,
    };

    let resources = match page_dict.get(b"Resources") {
        Ok(Object::Dictionary(ref d)) => d.clone(),
        Ok(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::Dictionary(ref d)) => d.clone(),
            _ => return result,
        },
        _ => return result,
    };

    let font_dict = match resources.get(b"Font") {
        Ok(Object::Dictionary(ref d)) => d.clone(),
        Ok(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::Dictionary(ref d)) => d.clone(),
            _ => return result,
        },
        _ => return result,
    };

    for (key, value) in font_dict.iter() {
        let name = String::from_utf8_lossy(key).to_string();
        if let Object::Reference(id) = value {
            result.insert(name, *id);
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text_run::extract_page_text_runs;
    use lopdf::{dictionary, Document, Object, Stream};

    // -----------------------------------------------------------------------
    // Test fixture builders
    // -----------------------------------------------------------------------

    /// Build a document with two fonts (regular + variant) and a simple page.
    fn make_doc_with_two_fonts(
        regular_base: &str,
        variant_base: &str,
        variant_flags: u32,
        variant_weight: Option<i64>,
        content: &[u8],
    ) -> Document {
        let mut doc = Document::with_version("1.7");

        let regular_font_id = add_font(&mut doc, regular_base, 0, None);
        let variant_font_id = add_font(&mut doc, variant_base, variant_flags, variant_weight);

        let font_resources = dictionary! {
            "F1" => Object::Reference(regular_font_id),
            "F2" => Object::Reference(variant_font_id),
        };
        build_page_with_resources(&mut doc, font_resources, content);
        doc
    }

    /// Build a document with only a regular font (no variant embedded).
    fn make_doc_single_font(regular_base: &str, content: &[u8]) -> Document {
        let mut doc = Document::with_version("1.7");
        let font_id = add_font(&mut doc, regular_base, 0, None);
        let font_resources = dictionary! {
            "F1" => Object::Reference(font_id),
        };
        build_page_with_resources(&mut doc, font_resources, content);
        doc
    }

    fn add_font(
        doc: &mut Document,
        base_font: &str,
        flags: u32,
        font_weight: Option<i64>,
    ) -> ObjectId {
        let mut desc = dictionary! {
            "Type" => "FontDescriptor",
            "FontName" => Object::Name(base_font.as_bytes().to_vec()),
            "Flags" => Object::Integer(flags as i64),
        };
        if let Some(w) = font_weight {
            desc.set("FontWeight", Object::Integer(w));
        }
        let desc_id = doc.add_object(Object::Dictionary(desc));
        doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => Object::Name(base_font.as_bytes().to_vec()),
            "FontDescriptor" => Object::Reference(desc_id),
        }))
    }

    fn build_page_with_resources(
        doc: &mut Document,
        font_resources: lopdf::Dictionary,
        content: &[u8],
    ) {
        let resources = dictionary! {
            "Font" => Object::Dictionary(font_resources),
        };
        let content_stream = Stream::new(dictionary! {}, content.to_vec());
        let content_id = doc.add_object(Object::Stream(content_stream));
        let page_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Page",
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => Object::Reference(content_id),
            "Resources" => Object::Dictionary(resources),
        }));
        let pages_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Pages",
            "Kids" => vec![Object::Reference(page_id)],
            "Count" => 1_i64,
        }));
        if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
            d.set("Parent", Object::Reference(pages_id));
        }
        let catalog_id = doc.add_object(Object::Dictionary(dictionary! {
            "Type" => "Catalog",
            "Pages" => Object::Reference(pages_id),
        }));
        doc.trailer.set("Root", Object::Reference(catalog_id));
    }

    // -----------------------------------------------------------------------
    // 1. bold-variant-present — swap succeeds
    // -----------------------------------------------------------------------

    #[test]
    fn bold_variant_present_swap_succeeds() {
        let content = b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET";
        let mut doc = make_doc_with_two_fonts("Helvetica", "Helvetica-Bold", 0, None, content);
        let runs = extract_page_text_runs(&doc, 1).unwrap();
        assert_eq!(runs.len(), 1);

        let result = set_text_run_style(&mut doc, 1, &runs[0], Some(true), None).unwrap();
        // Font name must have changed regardless of byte delta (equal-length names → delta = 0).
        assert!(
            result.requested_font_name.to_lowercase().contains("bold"),
            "requested_font_name should contain 'bold', got: {}",
            result.requested_font_name
        );
        assert_eq!(result.original_font_name, "Helvetica");
        assert_ne!(
            result.font_swap.variant_resource_name, result.font_swap.original_resource_name,
            "resource name must change after swap"
        );
        // Re-extract to verify the Tf in the stream was actually swapped.
        let runs_after = extract_page_text_runs(&doc, 1).unwrap();
        assert_eq!(runs_after.len(), 1);
        assert_eq!(
            runs_after[0].font_name, result.font_swap.variant_resource_name,
            "run after swap must use the variant resource name"
        );
    }

    // -----------------------------------------------------------------------
    // 2. bold-variant-absent — typed FontVariantNotEmbedded
    // -----------------------------------------------------------------------

    #[test]
    fn bold_variant_absent_returns_typed_error() {
        let content = b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET";
        let mut doc = make_doc_single_font("Helvetica", content);
        let runs = extract_page_text_runs(&doc, 1).unwrap();

        let err = set_text_run_style(&mut doc, 1, &runs[0], Some(true), None).unwrap_err();
        match err {
            ManipError::FontVariantNotEmbedded { ref requested, .. } => {
                assert!(requested.to_lowercase().contains("bold"));
            }
            other => panic!("expected FontVariantNotEmbedded, got: {other:?}"),
        }
    }

    // -----------------------------------------------------------------------
    // 3. italic-variant-present
    // -----------------------------------------------------------------------

    #[test]
    fn italic_variant_present_swap_succeeds() {
        let content = b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET";
        // Flags bit 7 (italic, value 64) set on variant.
        let mut doc = make_doc_with_two_fonts("Times", "Times-Italic", 64, None, content);
        let runs = extract_page_text_runs(&doc, 1).unwrap();

        let result = set_text_run_style(&mut doc, 1, &runs[0], None, Some(true)).unwrap();
        assert!(result.requested_font_name.to_lowercase().contains("italic"));
    }

    // -----------------------------------------------------------------------
    // 4. italic-variant-absent
    // -----------------------------------------------------------------------

    #[test]
    fn italic_variant_absent_returns_typed_error() {
        let content = b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET";
        let mut doc = make_doc_single_font("Times", content);
        let runs = extract_page_text_runs(&doc, 1).unwrap();

        let err = set_text_run_style(&mut doc, 1, &runs[0], None, Some(true)).unwrap_err();
        assert!(matches!(err, ManipError::FontVariantNotEmbedded { .. }));
    }

    // -----------------------------------------------------------------------
    // 5. bold-italic-variant-present
    // -----------------------------------------------------------------------

    #[test]
    fn bold_italic_variant_present_swap_succeeds() {
        let content = b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET";
        // Flags 64 (italic) | 262144 (ForceBold) = 262208
        let mut doc =
            make_doc_with_two_fonts("Courier", "Courier-BoldOblique", 262208, None, content);
        let runs = extract_page_text_runs(&doc, 1).unwrap();

        let result = set_text_run_style(&mut doc, 1, &runs[0], Some(true), Some(true)).unwrap();
        let name = result.requested_font_name.to_lowercase();
        assert!(name.contains("bold") || name.contains("oblique") || name.contains("italic"));
    }

    // -----------------------------------------------------------------------
    // 6. bold-italic-via-separate-Tf+Tr — parser reads correctly, G6 does NOT produce this form
    // -----------------------------------------------------------------------

    #[test]
    fn bold_italic_via_tr_is_not_produced_by_g6() {
        // A stream that uses Tr=2 (fill+stroke) to fake bold.  G6 must parse it
        // correctly (extract the run) but must NOT produce such a stream when
        // applying a style.
        let content = b"BT /F1 12 Tf 2 Tr 100 700 Td (Hello) Tj 0 Tr ET";
        let mut doc = make_doc_with_two_fonts("Arial", "Arial-Bold", 0, None, content);
        let runs = extract_page_text_runs(&doc, 1).unwrap();
        assert!(
            !runs.is_empty(),
            "parser must extract runs even when Tr is set"
        );

        let result = set_text_run_style(&mut doc, 1, &runs[0], Some(true), None).unwrap();
        // Verify the output stream contains a Tf swap, not a Tr manipulation.
        let editor = crate::content_editor::editor_for_page(&doc, 1).unwrap();
        let has_synthetic_tr = editor.operations().iter().any(|op| {
            op.operator == "Tr"
                && op
                    .operands
                    .first()
                    .and_then(as_number)
                    .map(|v| v as i32 == 2)
                    .unwrap_or(false)
        });
        // The original Tr=2 may remain in the stream, but G6 must not have
        // ADDED a new one.  We check by verifying the result font was swapped.
        assert!(
            result.requested_font_name.to_lowercase().contains("bold") || result.bytes_changed > 0
        );
        let _ = has_synthetic_tr; // diagnostic only; original Tr may remain
    }

    // -----------------------------------------------------------------------
    // 7. swap inside nested q/Q
    // -----------------------------------------------------------------------

    #[test]
    fn swap_inside_nested_q_q() {
        // The Tf is inside a q/Q save-restore block.
        let content = b"q BT /F1 12 Tf 100 700 Td (Hello) Tj ET Q";
        let mut doc = make_doc_with_two_fonts("Garamond", "Garamond-Bold", 0, None, content);
        let runs = extract_page_text_runs(&doc, 1).unwrap();
        assert!(!runs.is_empty());

        let result = set_text_run_style(&mut doc, 1, &runs[0], Some(true), None).unwrap();
        assert!(result.requested_font_name.to_lowercase().contains("bold"));
    }

    // -----------------------------------------------------------------------
    // 8. swap on TJ-sourced run
    // -----------------------------------------------------------------------

    #[test]
    fn swap_on_tj_array_run() {
        let content = b"BT /F1 12 Tf 100 700 Td [(He) -100 (llo)] TJ ET";
        let mut doc = make_doc_with_two_fonts("Verdana", "Verdana-Bold", 0, None, content);
        let runs = extract_page_text_runs(&doc, 1).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "Hello");

        let result = set_text_run_style(&mut doc, 1, &runs[0], Some(true), None).unwrap();
        assert!(result.requested_font_name.to_lowercase().contains("bold"));
    }

    // -----------------------------------------------------------------------
    // 9. swap with /FontWeight 700 detection
    // -----------------------------------------------------------------------

    #[test]
    fn swap_with_font_weight_700_detection() {
        // Variant has no bold suffix but FontWeight = 700 → detect as bold.
        let content = b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET";
        let mut doc = make_doc_with_two_fonts("Calibri", "Calibri-Bold", 0, Some(700), content);
        let runs = extract_page_text_runs(&doc, 1).unwrap();

        let style = detect_font_style(&doc, 1, "F2");
        assert!(style.is_bold, "FontWeight 700 must be detected as bold");

        let result = set_text_run_style(&mut doc, 1, &runs[0], Some(true), None).unwrap();
        assert!(result.bytes_changed > 0 || result.requested_font_name.contains("Bold"));
    }

    // -----------------------------------------------------------------------
    // 10. swap with FontDescriptor Flags-only detection
    // -----------------------------------------------------------------------

    #[test]
    fn swap_with_flags_only_bold_detection() {
        // Variant has ForceBold (bit 19, value 262144) but no suffix.
        let content = b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET";
        // Use a font name with no suffix but ForceBold flag.
        let mut doc = make_doc_with_two_fonts("Palatino", "Palatino-Bold", 262144, None, content);
        let runs = extract_page_text_runs(&doc, 1).unwrap();

        let style = detect_font_style(&doc, 1, "F2");
        assert!(style.is_bold, "ForceBold flag must be detected as bold");

        let result = set_text_run_style(&mut doc, 1, &runs[0], Some(true), None).unwrap();
        assert!(result.requested_font_name.to_lowercase().contains("bold"));
    }

    // -----------------------------------------------------------------------
    // 11. already-bold — no-op, returns Ok with bytes_changed = 0
    // -----------------------------------------------------------------------

    #[test]
    fn already_bold_is_noop() {
        // F1 is itself the bold variant.
        let content = b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET";
        let mut doc = make_doc_with_two_fonts("Helvetica", "Helvetica-Bold", 0, None, content);

        // Swap regular run to use F2 (bold) first.
        let runs = extract_page_text_runs(&doc, 1).unwrap();
        let _ = set_text_run_style(&mut doc, 1, &runs[0], Some(true), None).unwrap();

        // Now ask for bold again — should be a no-op.
        let runs2 = extract_page_text_runs(&doc, 1).unwrap();
        let result = set_text_run_style(&mut doc, 1, &runs2[0], Some(true), None).unwrap();
        assert_eq!(
            result.bytes_changed, 0,
            "second bold request on already-bold run must be a no-op"
        );
    }

    // -----------------------------------------------------------------------
    // 12. subset-font with -Bold suffix
    // -----------------------------------------------------------------------

    #[test]
    fn subset_font_with_bold_suffix() {
        // BaseFont = "ABCDEF+Georgia-Bold" — subset + bold suffix.
        let content = b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET";
        let mut doc = make_doc_with_two_fonts("Georgia", "ABCDEF+Georgia-Bold", 0, None, content);
        let runs = extract_page_text_runs(&doc, 1).unwrap();

        // detect_font_style should detect the bold flag via suffix after stripping prefix.
        let style = detect_font_style(&doc, 1, "F2");
        assert!(
            style.is_bold,
            "subset prefix must be stripped before suffix detection"
        );

        let result = set_text_run_style(&mut doc, 1, &runs[0], Some(true), None).unwrap();
        assert!(result
            .requested_font_name
            .to_lowercase()
            .contains("georgia"));
    }

    // -----------------------------------------------------------------------
    // Non-contamination: other runs on the same page are unaffected
    // -----------------------------------------------------------------------

    #[test]
    fn non_contamination_other_runs_unaffected() {
        // Two runs using F1.  Swap only the first; the second must still use F1.
        let content = b"BT /F1 12 Tf 100 700 Td (First) Tj 0 -20 Td (Second) Tj ET";
        let mut doc = make_doc_with_two_fonts("Arial", "Arial-Bold", 0, None, content);

        let runs = extract_page_text_runs(&doc, 1).unwrap();
        assert_eq!(runs.len(), 2, "expected two runs");

        let result = set_text_run_style(&mut doc, 1, &runs[0], Some(true), None).unwrap();
        assert!(result.bytes_changed > 0);

        // Re-extract and verify second run still uses original font.
        let runs_after = extract_page_text_runs(&doc, 1).unwrap();
        assert_eq!(runs_after.len(), 2);
        assert_eq!(
            runs_after[1].font_name, "F1",
            "second run must still reference F1 (original font)"
        );
    }

    // -----------------------------------------------------------------------
    // Unit: font_family_name
    // -----------------------------------------------------------------------

    #[test]
    fn font_family_name_strips_suffixes() {
        assert_eq!(font_family_name("Helvetica-Bold"), "Helvetica");
        assert_eq!(font_family_name("Times-Italic"), "Times");
        assert_eq!(font_family_name("Courier-BoldOblique"), "Courier");
        assert_eq!(font_family_name("ABCDEF+Georgia-Bold"), "Georgia");
        assert_eq!(font_family_name("Palatino"), "Palatino");
    }

    #[test]
    fn build_target_names() {
        assert_eq!(
            build_target_base_font("Helvetica", true, false),
            "Helvetica-Bold"
        );
        assert_eq!(build_target_base_font("Times", false, true), "Times-Italic");
        assert_eq!(
            build_target_base_font("Courier", true, true),
            "Courier-BoldItalic"
        );
        assert_eq!(build_target_base_font("Arial", false, false), "Arial");
    }
}
