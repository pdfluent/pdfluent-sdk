#![warn(missing_docs)]
//! Text run formatting for PDF content streams.
//!
//! Provides [`format_text_run`]: change the font size and/or fill color of a
//! single identified text run (Tj/TJ) without contaminating any other run on
//! the page.  State isolation is chosen automatically and reported in
//! [`FormatResult`].
//!
//! # Design notes
//!
//! Separate crate (not a module of pdf-manip) per coordinator preference —
//! clean dep boundary between read-side (text extraction) and write-side
//! (formatting writes).
//!
//! ## State isolation
//!
//! PDF `q`/`Q` saves and restores the graphics state (fill color, stroke
//! color, CTM, etc.) but does **not** restore text state (Tf, Tc, Tw, …).
//! Therefore font-size changes are always restored by an explicit `Tf`
//! operator injected immediately after the target `Tj`/`TJ`.  Color changes
//! are isolated via `q … Q` (new group or pre-existing group).
//!
//! ## Color model
//!
//! Injected color uses the `rg` operator (DeviceRGB).  The original color
//! reported in [`FormatResult::original_color`] is the RGB snapshot from
//! [`pdf_manip::content_editor::GraphicsStateTracker`] — which tracks `rg`
//! and `g` operators.  CMYK (`k`) is not yet tracked by the state machine;
//! if the original color was set only by `k`, the snapshot will be `[0,0,0]`.

use lopdf::content::Operation;
use lopdf::{Document, Object};
use pdf_manip::content_editor::{editor_for_page, write_editor_to_page, ContentEditor};
use pdf_manip::error::ManipError;
use thiserror::Error;

// Re-export the TextRun type so callers can work with a single import.
pub use pdf_manip::text_run::TextRun;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Identifies a text-showing operator in a page's parsed content stream.
///
/// Obtain `op_index` from [`TextRun::ops_range`]`.start` after calling
/// [`pdf_manip::text_run::extract_page_text_runs`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextRunLocator {
    /// Zero-based index of the operator in the parsed content stream.
    pub op_index: usize,
}

impl TextRunLocator {
    /// Create a locator from a raw operator index.
    pub fn new(op_index: usize) -> Self {
        Self { op_index }
    }

    /// Create a locator from the start of a [`TextRun`]'s op range.
    pub fn from_run(run: &TextRun) -> Self {
        Self {
            op_index: run.ops_range.start,
        }
    }
}

/// How state changes around the formatted run are isolated from other runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateIsolationStrategy {
    /// No `q`/`Q` injection.  Either no color change was requested (font-size
    /// only), or color state is already confined by a pre-existing group that
    /// contains only this run.  Font is always restored by an explicit `Tf`.
    NoIsolation,

    /// A new `q … Q` group was injected around the target `Tj`/`TJ` to
    /// localize fill-color changes.  Font is additionally restored by an
    /// explicit `Tf` after the run (PDF spec: `Q` does not restore text state).
    AddQGroup,

    /// The target run already sits alone inside an existing `q … Q` block.
    /// The enclosing `Q` restores fill color; no new group is needed.
    /// Font is still restored by an explicit `Tf`.
    ReuseExistingQGroup,
}

/// Result returned by [`format_text_run`].
#[derive(Debug, Clone)]
pub struct FormatResult {
    /// Total bytes added to the content stream by injected operators.
    pub bytes_changed: usize,
    /// Isolation strategy chosen for this run.
    pub state_isolation_strategy: StateIsolationStrategy,
    /// Font size (points) recorded *before* the format operation.
    /// `None` when the tracker found no preceding `Tf` (empty font state).
    pub original_size: Option<f32>,
    /// Fill color `[r, g, b]` in `0.0..=1.0` recorded *before* the operation.
    pub original_color: Option<[f32; 3]>,
}

/// Errors from [`format_text_run`].
#[derive(Debug, Error)]
pub enum FormatError {
    /// The locator's `op_index` is past the end of the content stream.
    #[error("op_index {0} out of range (stream has {1} ops)")]
    LocatorOutOfRange(usize, usize),

    /// The operator at `op_index` is not a text-showing operator.
    #[error("op at index {0} is '{1}', expected Tj/TJ/'/\"")]
    NotTextShowingOperator(usize, String),

    /// An error from the underlying pdf-manip layer.
    #[error(transparent)]
    ManipError(#[from] ManipError),
}

// ---------------------------------------------------------------------------
// Core API
// ---------------------------------------------------------------------------

/// Change the font size and/or fill color of a single text run.
///
/// `page_num` is **1-based**.  `locator` is obtained from
/// `TextRunLocator::from_run(&run)` or `TextRunLocator::new(op_index)`.
///
/// Both `font_size` and `color` are optional; passing `None` for both returns
/// `Ok` with `bytes_changed == 0` and the pre-existing state.
///
/// ## Isolation guarantee
///
/// Every other text run on the page is unaffected: the injected operators are
/// scoped so that state changes revert before the next run executes.
pub fn format_text_run(
    doc: &mut Document,
    page_num: u32,
    locator: TextRunLocator,
    font_size: Option<f32>,
    color: Option<[f32; 3]>,
) -> Result<FormatResult, FormatError> {
    let mut editor = editor_for_page(doc, page_num)?;
    let op_index = locator.op_index;
    let n_ops = editor.len();

    if op_index >= n_ops {
        return Err(FormatError::LocatorOutOfRange(op_index, n_ops));
    }

    validate_text_op(editor.operations(), op_index)?;

    // Capture state snapshot before the target op.
    let tracker = editor.track_state();
    let snap = tracker
        .state_at(op_index)
        .expect("snapshot index in bounds");
    let original_size = if snap.font_size > 0.0 {
        Some(snap.font_size as f32)
    } else {
        None
    };
    let original_color = Some([
        snap.fill_color[0] as f32,
        snap.fill_color[1] as f32,
        snap.fill_color[2] as f32,
    ]);
    let font_name = snap.font_name.clone();

    if font_size.is_none() && color.is_none() {
        return Ok(FormatResult {
            bytes_changed: 0,
            state_isolation_strategy: StateIsolationStrategy::NoIsolation,
            original_size,
            original_color,
        });
    }

    let q_depth = count_q_depth_at(editor.operations(), op_index);
    let strategy = determine_strategy(editor.operations(), op_index, q_depth, color.is_some());

    let pre_ops = build_pre_ops(strategy, &font_name, font_size, color);
    let post_ops = build_post_ops(strategy, &font_name, font_size, original_size);

    let bytes_changed = encoded_len(&pre_ops) + encoded_len(&post_ops);

    // Insert post first (preserves op_index), then pre.
    let post_index = op_index + 1;
    editor.insert_operations(post_index, post_ops);
    editor.insert_operations(op_index, pre_ops);

    write_editor_to_page(doc, page_num, &editor)?;

    Ok(FormatResult {
        bytes_changed,
        state_isolation_strategy: strategy,
        original_size,
        original_color,
    })
}

// ---------------------------------------------------------------------------
// Strategy determination
// ---------------------------------------------------------------------------

fn validate_text_op(ops: &[Operation], op_index: usize) -> Result<(), FormatError> {
    let op = &ops[op_index];
    if !matches!(op.operator.as_str(), "Tj" | "TJ" | "'" | "\"") {
        return Err(FormatError::NotTextShowingOperator(
            op_index,
            op.operator.clone(),
        ));
    }
    Ok(())
}

/// Count the `q`/`Q` nesting depth immediately before `target_index`.
fn count_q_depth_at(ops: &[Operation], target_index: usize) -> usize {
    let mut depth: usize = 0;
    for op in ops.iter().take(target_index) {
        match op.operator.as_str() {
            "q" => depth += 1,
            "Q" => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    depth
}

fn determine_strategy(
    ops: &[Operation],
    target_index: usize,
    q_depth: usize,
    has_color: bool,
) -> StateIsolationStrategy {
    if !has_color {
        // Font-only change: explicit Tf restore is sufficient — no q/Q needed.
        return StateIsolationStrategy::NoIsolation;
    }
    if q_depth > 0 && run_is_isolated_in_enclosing_q(ops, target_index) {
        return StateIsolationStrategy::ReuseExistingQGroup;
    }
    StateIsolationStrategy::AddQGroup
}

/// Returns `true` when the target run is the **only** text-showing operator
/// inside its innermost enclosing `q … Q` block.
fn run_is_isolated_in_enclosing_q(ops: &[Operation], target_index: usize) -> bool {
    let q_start = match find_innermost_q(ops, target_index) {
        Some(i) => i,
        None => return false,
    };
    let q_end = match find_matching_q_close(ops, q_start) {
        Some(i) => i,
        None => return false,
    };

    // Any other text-showing op between q_start..q_end (excluding target)?
    !ops[(q_start + 1)..q_end]
        .iter()
        .enumerate()
        .any(|(offset, op)| {
            q_start + 1 + offset != target_index
                && matches!(op.operator.as_str(), "Tj" | "TJ" | "'" | "\"")
        })
}

/// Find the index of the innermost `q` that encloses `target_index`.
fn find_innermost_q(ops: &[Operation], target_index: usize) -> Option<usize> {
    let mut depth: usize = 0;
    let mut last_q: Option<usize> = None;
    for (i, op) in ops.iter().take(target_index).enumerate() {
        match op.operator.as_str() {
            "q" => {
                depth += 1;
                last_q = Some(i);
            }
            "Q" => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    last_q = None;
                }
            }
            _ => {}
        }
    }
    if depth > 0 {
        last_q
    } else {
        None
    }
}

/// Find the index of the `Q` that closes the `q` at `q_start`.
fn find_matching_q_close(ops: &[Operation], q_start: usize) -> Option<usize> {
    let mut depth: usize = 1;
    for (i, op) in ops[(q_start + 1)..].iter().enumerate() {
        match op.operator.as_str() {
            "q" => depth += 1,
            "Q" => {
                depth -= 1;
                if depth == 0 {
                    return Some(q_start + 1 + i);
                }
            }
            _ => {}
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Operator injection
// ---------------------------------------------------------------------------

fn build_pre_ops(
    strategy: StateIsolationStrategy,
    font_name: &str,
    font_size: Option<f32>,
    color: Option<[f32; 3]>,
) -> Vec<Operation> {
    let mut ops = Vec::new();
    match strategy {
        StateIsolationStrategy::AddQGroup => {
            ops.push(Operation::new("q", vec![]));
            if let Some([r, g, b]) = color {
                ops.push(make_rg(r, g, b));
            }
            if let Some(sz) = font_size {
                ops.push(make_tf(font_name, sz));
            }
        }
        StateIsolationStrategy::ReuseExistingQGroup | StateIsolationStrategy::NoIsolation => {
            if let Some([r, g, b]) = color {
                ops.push(make_rg(r, g, b));
            }
            if let Some(sz) = font_size {
                ops.push(make_tf(font_name, sz));
            }
        }
    }
    ops
}

fn build_post_ops(
    strategy: StateIsolationStrategy,
    font_name: &str,
    font_size: Option<f32>,
    original_size: Option<f32>,
) -> Vec<Operation> {
    let mut ops = Vec::new();
    match strategy {
        StateIsolationStrategy::AddQGroup => {
            // Q restores fill color (graphics state).
            ops.push(Operation::new("Q", vec![]));
            // Q does NOT restore text state per PDF spec — restore Tf explicitly.
            if font_size.is_some() {
                ops.push(make_tf(font_name, original_size.unwrap_or(12.0)));
            }
        }
        StateIsolationStrategy::ReuseExistingQGroup | StateIsolationStrategy::NoIsolation => {
            // Enclosing Q (ReuseExistingQGroup) or no color change (NoIsolation).
            // Restore font explicitly regardless.
            if font_size.is_some() {
                ops.push(make_tf(font_name, original_size.unwrap_or(12.0)));
            }
        }
    }
    ops
}

fn make_rg(r: f32, g: f32, b: f32) -> Operation {
    Operation::new(
        "rg",
        vec![Object::Real(r), Object::Real(g), Object::Real(b)],
    )
}

fn make_tf(font_name: &str, size: f32) -> Operation {
    Operation::new(
        "Tf",
        vec![
            Object::Name(font_name.as_bytes().to_vec()),
            Object::Real(size),
        ],
    )
}

/// Approximate byte length of a list of operations when serialized.
fn encoded_len(ops: &[Operation]) -> usize {
    if ops.is_empty() {
        return 0;
    }
    ContentEditor::from_operations(ops.to_vec())
        .encode()
        .map(|b| b.len())
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::content::{Content, Operation};
    use lopdf::{dictionary, Document, Object, Stream};
    use pdf_manip::text_run::extract_page_text_runs;

    // -----------------------------------------------------------------------
    // Fixture helpers
    // -----------------------------------------------------------------------

    fn make_doc(content: &[u8]) -> Document {
        make_doc_with_font(content, "Helvetica")
    }

    fn make_doc_with_font(content: &[u8], base_font: &str) -> Document {
        let mut doc = Document::with_version("1.7");
        let font = dictionary! {
            "Type" => Object::Name(b"Font".to_vec()),
            "Subtype" => Object::Name(b"Type1".to_vec()),
            "BaseFont" => Object::Name(base_font.as_bytes().to_vec()),
        };
        let font_id = doc.add_object(Object::Dictionary(font));
        let resources = dictionary! {
            "Font" => Object::Dictionary(dictionary! {
                "F1" => Object::Reference(font_id),
            }),
        };
        let stream = Stream::new(dictionary! {}, content.to_vec());
        let stream_id = doc.add_object(Object::Stream(stream));
        let page = dictionary! {
            "Type" => Object::Name(b"Page".to_vec()),
            "MediaBox" => Object::Array(vec![
                Object::Integer(0), Object::Integer(0),
                Object::Integer(612), Object::Integer(792),
            ]),
            "Contents" => Object::Reference(stream_id),
            "Resources" => Object::Dictionary(resources),
        };
        let page_id = doc.add_object(Object::Dictionary(page));
        let pages = dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => Object::Array(vec![Object::Reference(page_id)]),
            "Count" => Object::Integer(1),
        };
        let pages_id = doc.add_object(Object::Dictionary(pages));
        if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(page_id) {
            d.set("Parent", Object::Reference(pages_id));
        }
        let catalog = dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));
        doc
    }

    /// Extract text runs from page 1 of a document.
    fn page_runs(doc: &Document) -> Vec<TextRun> {
        extract_page_text_runs(doc, 1).expect("extract_page_text_runs")
    }

    /// Encode operations to a byte stream.
    fn encode_content(ops: Vec<Operation>) -> Vec<u8> {
        Content { operations: ops }.encode().unwrap()
    }

    /// Assert that every run on the page *except* the one at `skip_op_index`
    /// has the same text as in `original_runs`.
    fn assert_other_runs_unchanged(
        doc: &Document,
        original_runs: &[TextRun],
        skip_op_index: usize,
    ) {
        let after = page_runs(doc);
        for orig in original_runs {
            if orig.ops_range.start == skip_op_index {
                continue;
            }
            let found = after
                .iter()
                .find(|r| r.ops_range.start != skip_op_index && r.text == orig.text);
            assert!(
                found.is_some(),
                "run '{}' (op {}) was changed or lost after formatting op {}",
                orig.text,
                orig.ops_range.start,
                skip_op_index
            );
        }
    }

    // -----------------------------------------------------------------------
    // Fixture 1: font size change — Tj operator
    // -----------------------------------------------------------------------
    #[test]
    fn f01_font_size_tj_no_color() {
        let mut doc = make_doc(b"BT /F1 12 Tf 100 700 Td (Hello) Tj ET");
        let runs = page_runs(&doc);
        assert_eq!(runs.len(), 1);
        let loc = TextRunLocator::from_run(&runs[0]);

        let result = format_text_run(&mut doc, 1, loc, Some(24.0), None).unwrap();
        assert_eq!(
            result.state_isolation_strategy,
            StateIsolationStrategy::NoIsolation
        );
        assert!(result.original_size.is_some());
        assert!((result.original_size.unwrap() - 12.0).abs() < 0.01);
        assert_eq!(result.original_color, Some([0.0, 0.0, 0.0]));
        // Other runs untouched (only 1 run, so just check it still exists).
        assert_eq!(page_runs(&doc).len(), 1);
    }

    // -----------------------------------------------------------------------
    // Fixture 2: color change — Tj operator, top-level (AddQGroup)
    // -----------------------------------------------------------------------
    #[test]
    fn f02_color_tj_addqgroup() {
        let mut doc = make_doc(b"BT /F1 12 Tf 100 700 Td (World) Tj ET");
        let runs = page_runs(&doc);
        let loc = TextRunLocator::from_run(&runs[0]);
        let original = runs.clone();

        let result = format_text_run(&mut doc, 1, loc, None, Some([1.0, 0.0, 0.0])).unwrap();
        assert_eq!(
            result.state_isolation_strategy,
            StateIsolationStrategy::AddQGroup
        );
        assert!(result.bytes_changed > 0);
        assert_other_runs_unchanged(&doc, &original, loc.op_index);
    }

    // -----------------------------------------------------------------------
    // Fixture 3: font size + color — Tj operator
    // -----------------------------------------------------------------------
    #[test]
    fn f03_size_and_color_tj() {
        let mut doc = make_doc(b"BT /F1 10 Tf 50 600 Td (Test) Tj ET");
        let runs = page_runs(&doc);
        let loc = TextRunLocator::from_run(&runs[0]);

        let result = format_text_run(&mut doc, 1, loc, Some(18.0), Some([0.0, 1.0, 0.0])).unwrap();
        assert_eq!(
            result.state_isolation_strategy,
            StateIsolationStrategy::AddQGroup
        );
        assert!((result.original_size.unwrap() - 10.0).abs() < 0.01);
        assert!(result.bytes_changed > 0);
    }

    // -----------------------------------------------------------------------
    // Fixture 4: font size change — TJ operator
    // -----------------------------------------------------------------------
    #[test]
    fn f04_font_size_tj_array() {
        let mut doc = make_doc(b"BT /F1 14 Tf 100 700 Td [(He) -100 (llo)] TJ ET");
        let runs = page_runs(&doc);
        assert_eq!(runs.len(), 1);
        let loc = TextRunLocator::from_run(&runs[0]);
        let original = runs.clone();

        let result = format_text_run(&mut doc, 1, loc, Some(20.0), None).unwrap();
        assert_eq!(
            result.state_isolation_strategy,
            StateIsolationStrategy::NoIsolation
        );
        assert!((result.original_size.unwrap() - 14.0).abs() < 0.01);
        assert_other_runs_unchanged(&doc, &original, loc.op_index);
    }

    // -----------------------------------------------------------------------
    // Fixture 5: color change — TJ operator
    // -----------------------------------------------------------------------
    #[test]
    fn f05_color_tj_array() {
        let mut doc = make_doc(b"BT /F1 12 Tf 100 700 Td [(Foo) -50 (Bar)] TJ ET");
        let runs = page_runs(&doc);
        let loc = TextRunLocator::from_run(&runs[0]);
        let original = runs.clone();

        let result = format_text_run(&mut doc, 1, loc, None, Some([0.0, 0.0, 1.0])).unwrap();
        assert_eq!(
            result.state_isolation_strategy,
            StateIsolationStrategy::AddQGroup
        );
        assert_other_runs_unchanged(&doc, &original, loc.op_index);
    }

    // -----------------------------------------------------------------------
    // Fixture 6: font size + color — TJ operator
    // -----------------------------------------------------------------------
    #[test]
    fn f06_size_and_color_tj_array() {
        let mut doc = make_doc(b"BT /F1 8 Tf 100 700 Td [(AB) -20 (CD)] TJ ET");
        let runs = page_runs(&doc);
        let loc = TextRunLocator::from_run(&runs[0]);

        let result = format_text_run(&mut doc, 1, loc, Some(16.0), Some([0.5, 0.5, 0.0])).unwrap();
        assert_eq!(
            result.state_isolation_strategy,
            StateIsolationStrategy::AddQGroup
        );
        assert!(result.bytes_changed > 0);
    }

    // -----------------------------------------------------------------------
    // Fixture 7: ReuseExistingQGroup — run already alone in q/Q
    // -----------------------------------------------------------------------
    #[test]
    fn f07_reuse_existing_q_group_color() {
        // One run inside a q/Q with no other text ops in that block.
        let content = b"BT /F1 12 Tf 100 700 Td (Outside) Tj ET \
                        q BT /F1 12 Tf 100 600 Td (Isolated) Tj ET Q";
        let mut doc = make_doc(content);
        let runs = page_runs(&doc);
        // Find the "Isolated" run.
        let isolated = runs.iter().find(|r| r.text == "Isolated").unwrap();
        let loc = TextRunLocator::from_run(isolated);
        let original = runs.clone();

        let result = format_text_run(&mut doc, 1, loc, None, Some([0.0, 0.5, 1.0])).unwrap();
        assert_eq!(
            result.state_isolation_strategy,
            StateIsolationStrategy::ReuseExistingQGroup
        );
        assert_other_runs_unchanged(&doc, &original, loc.op_index);
    }

    // -----------------------------------------------------------------------
    // Fixture 8: AddQGroup — q/Q present but multiple runs inside
    // -----------------------------------------------------------------------
    #[test]
    fn f08_add_q_group_multiple_runs_in_existing_q() {
        let content = b"q BT /F1 12 Tf 100 700 Td (Run1) Tj 0 -20 Td (Run2) Tj ET Q";
        let mut doc = make_doc(content);
        let runs = page_runs(&doc);
        assert!(runs.len() >= 2);
        let loc = TextRunLocator::from_run(&runs[0]);
        let original = runs.clone();

        // Even though inside a q/Q, there are two runs — must AddQGroup.
        let result = format_text_run(&mut doc, 1, loc, None, Some([1.0, 0.0, 0.0])).unwrap();
        assert_eq!(
            result.state_isolation_strategy,
            StateIsolationStrategy::AddQGroup
        );
        assert_other_runs_unchanged(&doc, &original, loc.op_index);
    }

    // -----------------------------------------------------------------------
    // Fixture 9: top-level stream (q_depth=0) with two runs
    // -----------------------------------------------------------------------
    #[test]
    fn f09_two_runs_format_first_second_unchanged() {
        let content = b"BT /F1 12 Tf 100 700 Td (Alpha) Tj 0 -20 Td (Beta) Tj ET";
        let mut doc = make_doc(content);
        let runs = page_runs(&doc);
        assert_eq!(runs.len(), 2);

        let loc0 = TextRunLocator::from_run(&runs[0]);
        let original = runs.clone();

        let result = format_text_run(&mut doc, 1, loc0, Some(18.0), Some([1.0, 0.0, 0.0])).unwrap();
        assert_eq!(
            result.state_isolation_strategy,
            StateIsolationStrategy::AddQGroup
        );
        assert_other_runs_unchanged(&doc, &original, loc0.op_index);
    }

    // -----------------------------------------------------------------------
    // Fixture 10: two runs — format second, first unchanged
    // -----------------------------------------------------------------------
    #[test]
    fn f10_two_runs_format_second_first_unchanged() {
        let content = b"BT /F1 12 Tf 100 700 Td (First) Tj 0 -20 Td (Second) Tj ET";
        let mut doc = make_doc(content);
        let runs = page_runs(&doc);
        assert_eq!(runs.len(), 2);

        let loc1 = TextRunLocator::from_run(&runs[1]);
        let original = runs.clone();

        let result = format_text_run(&mut doc, 1, loc1, Some(8.0), Some([0.0, 1.0, 0.0])).unwrap();
        assert_eq!(
            result.state_isolation_strategy,
            StateIsolationStrategy::AddQGroup
        );
        assert_other_runs_unchanged(&doc, &original, loc1.op_index);
    }

    // -----------------------------------------------------------------------
    // Fixture 11: deep nesting (q q ... Q Q) — still isolates correctly
    // -----------------------------------------------------------------------
    #[test]
    fn f11_deep_nesting_color_change() {
        let content = b"q q BT /F1 12 Tf 100 700 Td (Deep) Tj ET Q Q";
        let mut doc = make_doc(content);
        let runs = page_runs(&doc);
        assert_eq!(runs.len(), 1);
        let loc = TextRunLocator::from_run(&runs[0]);

        // The run is alone in the innermost q/Q.
        let result = format_text_run(&mut doc, 1, loc, None, Some([0.5, 0.0, 0.5])).unwrap();
        assert_eq!(
            result.state_isolation_strategy,
            StateIsolationStrategy::ReuseExistingQGroup
        );
    }

    // -----------------------------------------------------------------------
    // Fixture 12: grayscale initial color tracked via `g` operator
    // -----------------------------------------------------------------------
    #[test]
    fn f12_grayscale_initial_color_tracked() {
        let content = b"0.5 g BT /F1 12 Tf 100 700 Td (Gray) Tj ET";
        let mut doc = make_doc(content);
        let runs = page_runs(&doc);
        let loc = TextRunLocator::from_run(&runs[0]);

        let result = format_text_run(&mut doc, 1, loc, None, Some([1.0, 0.0, 0.0])).unwrap();
        // Original gray [0.5, 0.5, 0.5] was tracked.
        let orig = result.original_color.unwrap();
        assert!((orig[0] - 0.5).abs() < 0.01);
        assert!((orig[1] - 0.5).abs() < 0.01);
    }

    // -----------------------------------------------------------------------
    // Fixture 13: rg initial color tracked
    // -----------------------------------------------------------------------
    #[test]
    fn f13_rg_initial_color_tracked() {
        let content = b"0.2 0.4 0.6 rg BT /F1 12 Tf 100 700 Td (Color) Tj ET";
        let mut doc = make_doc(content);
        let runs = page_runs(&doc);
        let loc = TextRunLocator::from_run(&runs[0]);

        let result = format_text_run(&mut doc, 1, loc, None, Some([0.0, 0.0, 0.0])).unwrap();
        let orig = result.original_color.unwrap();
        assert!((orig[0] - 0.2).abs() < 0.01, "r: {}", orig[0]);
        assert!((orig[1] - 0.4).abs() < 0.01, "g: {}", orig[1]);
        assert!((orig[2] - 0.6).abs() < 0.01, "b: {}", orig[2]);
    }

    // -----------------------------------------------------------------------
    // Fixture 14: no-op (both None) returns correct original state
    // -----------------------------------------------------------------------
    #[test]
    fn f14_noop_returns_original_state() {
        let content = b"1 0 0 rg BT /F1 9 Tf 100 700 Td (NoOp) Tj ET";
        let mut doc = make_doc(content);
        let runs = page_runs(&doc);
        let loc = TextRunLocator::from_run(&runs[0]);

        let result = format_text_run(&mut doc, 1, loc, None, None).unwrap();
        assert_eq!(result.bytes_changed, 0);
        assert_eq!(
            result.state_isolation_strategy,
            StateIsolationStrategy::NoIsolation
        );
        assert!((result.original_size.unwrap() - 9.0).abs() < 0.01);
        let orig = result.original_color.unwrap();
        assert!((orig[0] - 1.0).abs() < 0.01);
    }

    // -----------------------------------------------------------------------
    // Fixture 15: out-of-range op_index returns error
    // -----------------------------------------------------------------------
    #[test]
    fn f15_out_of_range_returns_error() {
        let mut doc = make_doc(b"BT /F1 12 Tf (Hello) Tj ET");
        let err = format_text_run(&mut doc, 1, TextRunLocator::new(999), Some(14.0), None);
        assert!(matches!(err, Err(FormatError::LocatorOutOfRange(999, _))));
    }

    // -----------------------------------------------------------------------
    // Fixture 16: non-text op at locator returns error
    // -----------------------------------------------------------------------
    #[test]
    fn f16_non_text_op_returns_error() {
        let mut doc = make_doc(b"BT /F1 12 Tf (Hello) Tj ET");
        // Op 0 = BT, which is not a text-showing op.
        let err = format_text_run(&mut doc, 1, TextRunLocator::new(0), Some(14.0), None);
        assert!(matches!(
            err,
            Err(FormatError::NotTextShowingOperator(0, _))
        ));
    }

    // -----------------------------------------------------------------------
    // Fixture 17: three runs — format middle, first and third unchanged
    // -----------------------------------------------------------------------
    #[test]
    fn f17_three_runs_format_middle() {
        let content = b"BT /F1 12 Tf 100 700 Td (AAA) Tj 0 -20 Td (BBB) Tj 0 -20 Td (CCC) Tj ET";
        let mut doc = make_doc(content);
        let runs = page_runs(&doc);
        assert_eq!(runs.len(), 3);
        let loc = TextRunLocator::from_run(&runs[1]);
        let original = runs.clone();

        let result = format_text_run(&mut doc, 1, loc, Some(20.0), Some([0.0, 0.0, 1.0])).unwrap();
        assert_eq!(
            result.state_isolation_strategy,
            StateIsolationStrategy::AddQGroup
        );
        assert_other_runs_unchanged(&doc, &original, loc.op_index);
    }

    // -----------------------------------------------------------------------
    // Fixture 18: font-size-only, three runs — other two font sizes unchanged
    // -----------------------------------------------------------------------
    #[test]
    fn f18_font_size_only_no_contamination_three_runs() {
        let content =
            b"BT /F1 12 Tf 100 700 Td (X) Tj /F1 12 Tf 0 -20 Td (Y) Tj /F1 12 Tf 0 -20 Td (Z) Tj ET";
        let mut doc = make_doc(content);
        let before_runs = page_runs(&doc);
        assert_eq!(before_runs.len(), 3);
        let loc = TextRunLocator::from_run(&before_runs[0]);

        format_text_run(&mut doc, 1, loc, Some(30.0), None).unwrap();

        // Re-parse and check that runs 1 and 2 still have font_size 12.
        let after = page_runs(&doc);
        let run_y = after.iter().find(|r| r.text == "Y").unwrap();
        let run_z = after.iter().find(|r| r.text == "Z").unwrap();
        assert!(
            (run_y.font_size - 12.0).abs() < 0.01,
            "Y font_size drifted: {}",
            run_y.font_size
        );
        assert!(
            (run_z.font_size - 12.0).abs() < 0.01,
            "Z font_size drifted: {}",
            run_z.font_size
        );
    }

    // -----------------------------------------------------------------------
    // Fixture 19: font-size zero original state (no preceding Tf)
    // -----------------------------------------------------------------------
    #[test]
    fn f19_zero_original_font_size_handled() {
        // Content stream with no Tf before Tj — font_size tracked as 0.
        let content = b"BT 100 700 Td (NoFont) Tj ET";
        let mut doc = make_doc(content);
        let runs = page_runs(&doc);
        let loc = TextRunLocator::from_run(&runs[0]);

        let result = format_text_run(&mut doc, 1, loc, Some(16.0), None);
        // Should succeed (no crash on zero original size).
        assert!(result.is_ok());
        let r = result.unwrap();
        assert_eq!(r.original_size, None); // 0.0 maps to None
    }

    // -----------------------------------------------------------------------
    // Fixture 20: ' operator (move-and-show) is accepted
    // -----------------------------------------------------------------------
    #[test]
    fn f20_apostrophe_op_accepted() {
        let content = encode_content(vec![
            Operation::new("BT", vec![]),
            Operation::new("Tf", vec![Object::Name(b"F1".to_vec()), Object::Real(12.0)]),
            Operation::new(
                "'",
                vec![Object::String(
                    b"Hello".to_vec(),
                    lopdf::StringFormat::Literal,
                )],
            ),
            Operation::new("ET", vec![]),
        ]);
        let mut doc = make_doc(&content);
        let runs = page_runs(&doc);
        // ' op is decoded as a text run.
        let loc = TextRunLocator::from_run(&runs[0]);

        let result = format_text_run(&mut doc, 1, loc, Some(14.0), Some([0.2, 0.4, 0.6]));
        assert!(result.is_ok());
        let r = result.unwrap();
        assert_eq!(
            r.state_isolation_strategy,
            StateIsolationStrategy::AddQGroup
        );
    }

    // -----------------------------------------------------------------------
    // Performance gate: < 50 ms native on a synthetic 10-page document.
    // Run with: cargo test -p pdf-text-format perf_gate -- --ignored
    // -----------------------------------------------------------------------
    #[test]
    #[ignore]
    fn perf_gate_50ms_native_10_pages() {
        use std::time::Instant;

        // Build a 10-page document, each page with 5 runs.
        let mut doc = Document::with_version("1.7");
        let font = dictionary! {
            "Type" => Object::Name(b"Font".to_vec()),
            "Subtype" => Object::Name(b"Type1".to_vec()),
            "BaseFont" => Object::Name(b"Helvetica".to_vec()),
        };
        let font_id = doc.add_object(Object::Dictionary(font));
        let resources = dictionary! {
            "Font" => Object::Dictionary(dictionary! {
                "F1" => Object::Reference(font_id),
            }),
        };
        let content = b"BT /F1 12 Tf \
            100 700 Td (Run1) Tj \
            0 -20 Td (Run2) Tj \
            0 -20 Td (Run3) Tj \
            0 -20 Td (Run4) Tj \
            0 -20 Td (Run5) Tj ET";

        let mut page_ids = Vec::new();
        for _ in 0..10 {
            let stream = Stream::new(dictionary! {}, content.to_vec());
            let stream_id = doc.add_object(Object::Stream(stream));
            let page = dictionary! {
                "Type" => Object::Name(b"Page".to_vec()),
                "MediaBox" => Object::Array(vec![
                    Object::Integer(0), Object::Integer(0),
                    Object::Integer(612), Object::Integer(792),
                ]),
                "Contents" => Object::Reference(stream_id),
                "Resources" => Object::Dictionary(resources.clone()),
            };
            let pid = doc.add_object(Object::Dictionary(page));
            page_ids.push(pid);
        }
        let pages = dictionary! {
            "Type" => Object::Name(b"Pages".to_vec()),
            "Kids" => Object::Array(page_ids.iter().map(|id| Object::Reference(*id)).collect()),
            "Count" => Object::Integer(10),
        };
        let pages_id = doc.add_object(Object::Dictionary(pages));
        for &pid in &page_ids {
            if let Ok(Object::Dictionary(ref mut d)) = doc.get_object_mut(pid) {
                d.set("Parent", Object::Reference(pages_id));
            }
        }
        let catalog = dictionary! {
            "Type" => Object::Name(b"Catalog".to_vec()),
            "Pages" => Object::Reference(pages_id),
        };
        let catalog_id = doc.add_object(Object::Dictionary(catalog));
        doc.trailer.set("Root", Object::Reference(catalog_id));

        // Time: format one run per page for all 10 pages.
        let t0 = Instant::now();
        for page_num in 1u32..=10 {
            let runs = extract_page_text_runs(&doc, page_num).unwrap();
            let loc = TextRunLocator::from_run(&runs[0]);
            format_text_run(&mut doc, page_num, loc, Some(16.0), Some([1.0, 0.0, 0.0])).unwrap();
        }
        let elapsed_ms = t0.elapsed().as_millis();
        assert!(
            elapsed_ms < 50,
            "performance gate failed: {elapsed_ms} ms >= 50 ms"
        );
    }
}
