//! TATR table structure: the ONNX table model behind `ort-tables`.
//!
//! WHERE AND WHY HERE. The learned table pass rebuilds a table region's grid
//! from its words ([`super::tables::rebuild_region`]), and this module is the
//! model that rebuilds it better: Table Transformer structure recognition
//! (`microsoft/table-transformer-structure-recognition-v1.1-all`, MIT),
//! exported to ONNX by P1 and run through the shared ONNX Runtime layer
//! ([`pdf_ocr::runtime`]). [`tatr_cells`] renders the region, runs the model,
//! and returns the same [`Block`] cells `rebuild_region` produces, so the P3
//! hook can swap them in; it returns `None` where TATR does not fire and the
//! existing chain runs unchanged.
//!
//! THE FEATURE. `ort-tables` is opt-in and off by default. It depends on a
//! release candidate of the `ort` crate, pinned exactly (`ort =2.0.0-rc.13`,
//! `api-23`, against ONNX Runtime 1.23.2, loaded dynamically, never linked),
//! and the default build stays pure Rust: without the feature no `ort` is
//! resolved at all (held by `the_default_tree_carries_no_ort.py`).
//!
//! THE MODEL PACKAGE. The weights are never downloaded: they come from an
//! explicitly installed model package, `<model dir>/tatr-v1.1-all/
//! tatr-v1.1-all.onnx`, verified against [`TATR_SHA256`] before it loads. A
//! missing or mismatched file is a [`pdf_ocr::runtime::ModelError`] carried
//! in the returned [`std::io::Error`], found back through
//! [`pdf_ocr::runtime::model_error`]; no new [`crate::EngineError`] variant.
//!
//! THE REFERENCE. Pre-processing mirrors P1's `TatrAdapter.preprocess`
//! exactly: the region at 150 dpi with the reference 10 px pad, MaxResize so
//! the longest side is 1000 (halves round to even, as theirs), bilinear, the
//! hub's ImageNet normalisation. Decoding mirrors the hub processor's
//! `post_process_object_detection` (softmax over all seven classes, the best
//! of the six foreground classes wins each query), and the structure is a
//! Rust port of the vendored `tatr_postprocess.objects_to_cells` path the
//! eval runs -- same thresholds, same refinement, same cells. The parity
//! test (`tatr_postprocess_matches_the_vendored_python`) feeds raw ONNX
//! outputs and requires the Python-decoded cells, value for value.
//!
//! [`Block`]: crate::blocks::Block

// Copyright (c) 2026 Innovation Trigger B.V.
//
// PDFluent is available under two licences, at your option: the GNU AGPLv3, or
// the PDFluent Commercial Licence. See the LICENSE file in this repository --
// that file travels with the copy you received, which a URL does not.

use crate::blocks::Block;
use crate::learned::features::PageWord;
use crate::learned::{grid, judge};
use crate::{PdfDocument, RenderOptions};

/// The TATR model name: the package directory inside the model dir.
pub const TATR_MODEL_NAME: &str = "tatr-v1.1-all";
/// The P1 export's file name inside the package dir.
pub const TATR_MODEL_FILE: &str = "tatr-v1.1-all.onnx";
/// The ONNX file's SHA-256, lowercase hex, from P1's `export_meta`.
///
/// Pinned to the P1 export (`tatr-v1.1-all.onnx`, weights revision
/// `7587a7ef111d9dcbf8ac695f1376ab7014340a0c`): the pin test below fails
/// while this is zeroes, so a placeholder can never ship quietly.
pub const TATR_SHA256: &str = "8c28418690b896a845a53633736a62c22ef8c9a515a23c06e9976e61360f1fcf";
/// Shown verbatim in `MODEL_NOT_AVAILABLE`: where to get the package.
///
/// The per-ecosystem model packages (crate / wheel / npm) are T1/T2's
/// packaging work; until they land this names the route -- an explicit
/// install into the model dir, never a download -- and the path the file
/// must sit at.
pub const TATR_INSTALL_COMMAND: &str = "install the tatr-v1.1-all model package for your \
    platform into the model dir (PDFLUENT_MODEL_DIR or <data dir>/pdfluent/models/), or place \
    the verified tatr-v1.1-all.onnx at <model dir>/tatr-v1.1-all/tatr-v1.1-all.onnx -- see the \
    pdf-engine ort-tables feature documentation";

/// What the TATR model package must provide, for the shared runtime layer.
pub const TATR_SPEC: pdf_ocr::runtime::ModelSpec = pdf_ocr::runtime::ModelSpec::new(
    TATR_MODEL_NAME,
    TATR_MODEL_FILE,
    TATR_SHA256,
    TATR_INSTALL_COMMAND,
);

/// The resolution TATR regions render at, in dots per inch.
pub const TATR_DPI: u32 = 150;
/// The reference pad around the region, in pixels at [`TATR_DPI`].
pub const TATR_PAD_PX: f64 = 10.0;
/// The MaxResize longest side, in pixels.
pub const TATR_MAX_SIZE: f64 = 1000.0;
/// The hub preprocessor_config's channel means (ImageNet).
pub const TATR_MEAN: [f32; 3] = [0.485, 0.456, 0.406];
/// The hub preprocessor_config's channel standard deviations (ImageNet).
pub const TATR_STD: [f32; 3] = [0.229, 0.224, 0.225];
/// DETR queries in the exported head.
pub const TATR_QUERIES: usize = 125;
/// Classes in the exported head: six foreground plus no-object.
pub const TATR_CLASSES: usize = 7;

/// The foreground labels, in head order: what query argmax 0..5 names.
pub const TATR_LABELS: [&str; 6] = [
    "table",
    "table column",
    "table row",
    "table column header",
    "table projected row header",
    "table spanning cell",
];
/// The reference class thresholds: every structure class at 0.5.
pub const TATR_CLASS_THRESHOLD: f64 = 0.5;
/// The no-object threshold: 10, so no-object never survives it.
pub const TATR_NO_OBJECT_THRESHOLD: f64 = 10.0;

/// TATR fires only with at least this many refined rows and columns.
pub const TATR_FIRE_MIN_ROWS: usize = 2;
/// TATR fires only with at least this many refined rows and columns.
pub const TATR_FIRE_MIN_COLUMNS: usize = 2;
/// TATR fires only when the kept objects' mean score reaches this.
///
/// The reference value, pending tuning: every kept object already scores at
/// least the class threshold, so this leg is vacuous today and exists so P4
/// can raise it on the tuning halves without reshaping the rule.
pub const TATR_FIRE_MEAN_SCORE: f64 = 0.5;

/// One word where the decode reads it: text plus its box in crop pixels.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct TatrWord {
    /// The word's text.
    pub text: String,
    /// `[x0, y0, x1, y1]` in crop pixels, y down from the crop's top-left.
    pub bbox: [f64; 4],
}

impl TatrWord {
    /// A word in crop pixels, as the decode reads it.
    pub fn new(text: String, bbox: [f64; 4]) -> Self {
        Self { text, bbox }
    }
}

/// One decoded grid cell: its grid position and its box in crop pixels.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct TatrCell {
    /// The top row the cell occupies, 0 at the top.
    pub row: usize,
    /// The left column the cell occupies, 0 at the left.
    pub col: usize,
    /// How many rows the cell spans.
    pub row_span: usize,
    /// How many columns the cell spans.
    pub col_span: usize,
    /// Whether the cell's row meets the column header box.
    pub header: bool,
    /// `[x0, y0, x1, y1]` in crop pixels, y down from the crop's top-left.
    pub bbox: [f64; 4],
}

/// Raw TATR outputs decoded: the grid cells plus what the fires rule reads.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub struct TatrDecode {
    /// The grid cells in row-major order, boxes in crop pixels.
    pub cells: Vec<TatrCell>,
    /// The reference confidence: mean and minimum cell-match score, halved.
    pub confidence: f64,
    /// Refined rows the structures hold.
    pub rows: usize,
    /// Refined columns the structures hold.
    pub columns: usize,
    /// Detections that survived the class thresholds.
    pub kept_objects: usize,
    /// Their mean score, 0.0 where none survived.
    pub mean_kept_score: f64,
}

/// Raw TATR outputs as grid cells, through the reference path.
///
/// `logits` is 125 rows of 7 and `pred_boxes` 125 normalised (cx, cy, w, h)
/// rows, both row-major; anything else is an `InvalidInput`. Decoding is the
/// hub processor's reading (softmax over all seven classes, the best of the
/// six foreground classes wins each query, boxes to corners scaled to the
/// crop), and the structure is the port of the vendored
/// `tatr_postprocess.objects_to_cells` below: same thresholds, same
/// refinement, same cells. `words` carry the crop's words in crop pixels;
/// the refinement and the text-adjusted boxes read them, exactly as the
/// reference reads its tokens.
pub fn cells_from_raw_outputs(
    logits: &[f32],
    pred_boxes: &[f32],
    crop_width_px: f64,
    crop_height_px: f64,
    words: &[TatrWord],
) -> Result<TatrDecode, std::io::Error> {
    if logits.len() != TATR_QUERIES * TATR_CLASSES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "tatr logits hold {} values, want {}",
                logits.len(),
                TATR_QUERIES * TATR_CLASSES
            ),
        ));
    }
    if pred_boxes.len() != TATR_QUERIES * 4 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "tatr pred_boxes hold {} values, want {}",
                pred_boxes.len(),
                TATR_QUERIES * 4
            ),
        ));
    }
    let mut objects = Vec::with_capacity(TATR_QUERIES);
    for query in 0..TATR_QUERIES {
        let row = &logits[query * TATR_CLASSES..(query + 1) * TATR_CLASSES];
        // Softmax in f64, as the reference computes it; f32 would land an
        // ulp off and could flip a score sitting exactly on 0.5.
        let top = row
            .iter()
            .map(|&v| f64::from(v))
            .fold(f64::NEG_INFINITY, f64::max);
        let mut total = 0.0;
        let mut best_label = 0;
        let mut best_prob = f64::NEG_INFINITY;
        let mut probs = [0.0; TATR_CLASSES];
        for (i, &v) in row.iter().enumerate() {
            let e = (f64::from(v) - top).exp();
            probs[i] = e;
            total += e;
        }
        // The best of the six FOREGROUND classes wins, whatever the seventh
        // says; ties keep the first, as the reference's argmax does.
        for (i, &e) in probs.iter().enumerate().take(TATR_CLASSES - 1) {
            let p = e / total;
            if p > best_prob {
                best_prob = p;
                best_label = i;
            }
        }
        // Kept strictly above zero, as the reference's threshold=0.0 keeps
        // every query; the class thresholds fall later.
        if best_prob > 0.0 {
            let b = &pred_boxes[query * 4..(query + 1) * 4];
            let (cx, cy, w, h) = (
                f64::from(b[0]),
                f64::from(b[1]),
                f64::from(b[2]),
                f64::from(b[3]),
            );
            objects.push(Obj {
                bbox: Rect {
                    x0: (cx - w / 2.0) * crop_width_px,
                    y0: (cy - h / 2.0) * crop_height_px,
                    x1: (cx + w / 2.0) * crop_width_px,
                    y1: (cy + h / 2.0) * crop_height_px,
                },
                label: best_label,
                score: best_prob,
                header: false,
                subheader: false,
                row_numbers: Vec::new(),
                column_numbers: Vec::new(),
            });
        }
    }
    let kept = apply_class_thresholds(&objects);
    let mean_kept_score = if kept.is_empty() {
        0.0
    } else {
        kept.iter().map(|o| o.score).sum::<f64>() / kept.len() as f64
    };
    let tokens: Vec<Tok> = words
        .iter()
        .map(|w| Tok {
            bbox: Rect::of(w.bbox),
            blank: w.text.trim().is_empty(),
        })
        .collect();
    let (structures, cells, confidence) = objects_to_cells(&kept, &tokens);
    Ok(TatrDecode {
        cells,
        confidence,
        rows: structures.rows.len(),
        columns: structures.columns.len(),
        kept_objects: kept.len(),
        mean_kept_score,
    })
}

/// Why TATR declines a decode it ran: too few rows or columns, kept
/// objects under the confidence bar, or cells claiming too few of the words
/// inside TATR's own table extent.
///
/// TWO PREDICATES, one per half. [`decline_reason`] is the decode-level
/// rule -- rows, columns, confidence -- and the decode's firing decision
/// its negation, so the trace's reason and the decision cannot drift. The
/// coverage half is claim-level: it reads the fill past the decode, in
/// `tatr_attempt` for the real structurer and in the hook for the fake seam,
/// because only there do the region words exist to count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TatrDecline {
    /// Fewer than [`TATR_FIRE_MIN_ROWS`] rows or [`TATR_FIRE_MIN_COLUMNS`]
    /// columns.
    FewRowsOrColumns { rows: usize, columns: usize },
    /// Enough rows and columns, but the kept objects' mean score under
    /// [`TATR_FIRE_MEAN_SCORE`].
    LowConfidence { mean_kept_score_bits: u64 },
    /// The decode fires, but its cells claim fewer than the hook's
    /// coverage threshold (`super::tables::TATR_MIN_COVERAGE`) of the region
    /// words inside its own table extent: `claimed` of `inside`, the direct
    /// word-centre claims before the gap fill. `inside == 0` where the
    /// extent covers no region word at all -- a table where the region holds
    /// nothing is not a fire.
    LowCoverage { claimed: usize, inside: usize },
}

/// Which decode-level refusal a decode earns, or `None` where the decode
/// fires: the one decode rule, read both by [`fires`] and by the P3 hook's
/// trace. The coverage half is not here -- it reads the fill, past this.
pub(crate) fn decline_reason(
    rows: usize,
    columns: usize,
    mean_kept_score: f64,
) -> Option<TatrDecline> {
    if rows < TATR_FIRE_MIN_ROWS || columns < TATR_FIRE_MIN_COLUMNS {
        return Some(TatrDecline::FewRowsOrColumns { rows, columns });
    }
    if mean_kept_score < TATR_FIRE_MEAN_SCORE {
        return Some(TatrDecline::LowConfidence {
            mean_kept_score_bits: mean_kept_score.to_bits(),
        });
    }
    None
}

/// Whether a decode fires its level: enough rows and columns, and confident
/// enough kept objects. The coverage half is past this, on the fill.
///
/// Test-only: the serving path reads [`decline_reason`] and matches on the
/// refusal, so this negation survives for the parity tests alone.
#[cfg(test)]
pub(crate) fn fires(rows: usize, columns: usize, mean_kept_score: f64) -> bool {
    decline_reason(rows, columns, mean_kept_score).is_none()
}

/// A rectangle with the shipped `Rect`'s semantics (`rect_standin.Rect`):
/// the empty rect is the infinite inverse, `intersect` returns new, and the
/// area of an empty or inverted rect is 0.
#[derive(Debug, Clone, PartialEq)]
struct Rect {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

impl Rect {
    fn of(bbox: [f64; 4]) -> Self {
        Self {
            x0: bbox[0],
            y0: bbox[1],
            x1: bbox[2],
            y1: bbox[3],
        }
    }

    fn empty() -> Self {
        Self {
            x0: f64::INFINITY,
            y0: f64::INFINITY,
            x1: f64::NEG_INFINITY,
            y1: f64::NEG_INFINITY,
        }
    }

    fn is_empty(&self) -> bool {
        self.x0 >= self.x1 || self.y0 >= self.y1
    }

    fn area(&self) -> f64 {
        if self.is_empty() {
            0.0
        } else {
            (self.x1 - self.x0) * (self.y1 - self.y0)
        }
    }

    fn intersect(&self, other: &Rect) -> Rect {
        Rect {
            x0: self.x0.max(other.x0),
            y0: self.y0.max(other.y0),
            x1: self.x1.min(other.x1),
            y1: self.y1.min(other.y1),
        }
    }

    /// Grow to cover `other`; an empty rect BECOMES its argument.
    fn include_rect(&mut self, other: &Rect) {
        // Exactly the shipped test: x0 AND y0 infinite, not the area.
        if self.x0 == f64::INFINITY && self.y0 == f64::INFINITY {
            *self = other.clone();
        } else {
            self.x0 = self.x0.min(other.x0);
            self.y0 = self.y0.min(other.y0);
            self.x1 = self.x1.max(other.x1);
            self.y1 = self.y1.max(other.y1);
        }
    }
}

/// One detection through the pipeline. `PartialEq` is the header-tree
/// removal's equality, as the reference's dict equality is.
#[derive(Debug, Clone, PartialEq)]
struct Obj {
    bbox: Rect,
    label: usize,
    score: f64,
    header: bool,
    /// A projected row header travelling with the spanning cells.
    subheader: bool,
    row_numbers: Vec<usize>,
    column_numbers: Vec<usize>,
}

/// One word where the refinement reads it: only its box and whether it is
/// blank matter. The reference joins overlapping spans' texts to test for
/// content, which is non-blank exactly when some overlapping span is.
#[derive(Debug, Clone)]
struct Tok {
    bbox: Rect,
    blank: bool,
}

/// The reconciled structures: rows, columns, headers and spanning cells.
///
/// Headers travel as objects through the refinement (it sorts by score);
/// what lands here are shells around the aligned boxes, read by nothing
/// after -- the live effect of the header pass is the rows' flags.
#[derive(Debug, Clone, Default)]
struct Structures {
    rows: Vec<Obj>,
    columns: Vec<Obj>,
    headers: Vec<Obj>,
    supercells: Vec<Obj>,
}

/// Detections scoring at least their class's threshold, in query order.
fn apply_class_thresholds(objects: &[Obj]) -> Vec<Obj> {
    objects
        .iter()
        .filter(|o| {
            let threshold = if o.label < TATR_LABELS.len() {
                TATR_CLASS_THRESHOLD
            } else {
                TATR_NO_OBJECT_THRESHOLD
            };
            o.score >= threshold
        })
        .cloned()
        .collect()
}

/// The intersection over `first`'s area, 0 where it has none.
fn iob(first: &Rect, second: &Rect) -> f64 {
    let area = first.area();
    if area > 0.0 {
        first.intersect(second).area() / area
    } else {
        0.0
    }
}

/// Whether at least `threshold` of `first` overlaps `second`.
fn overlaps(first: &Rect, second: &Rect, threshold: f64) -> bool {
    let area = first.area();
    if area > 0.0 {
        first.intersect(second).area() / area >= threshold
    } else {
        false
    }
}

/// High score to low, stable, as the reference sorts.
fn sort_by_score_desc(objects: &mut [Obj]) {
    objects.sort_by(|a, b| b.score.total_cmp(&a.score));
}

/// Left to right on the horizontal midpoint sum, stable: ties keep their
/// incoming order, as the reference's `sorted` does -- no tiebreak.
fn sort_left_to_right(objects: &mut [Obj]) {
    objects.sort_by(|a, b| (a.bbox.x0 + a.bbox.x1).total_cmp(&(b.bbox.x0 + b.bbox.x1)));
}

/// Top to bottom on the vertical midpoint sum, stable, likewise.
fn sort_top_to_bottom(objects: &mut [Obj]) {
    objects.sort_by(|a, b| (a.bbox.y0 + a.bbox.y1).total_cmp(&(b.bbox.y0 + b.bbox.y1)));
}

/// Slot packages into the container holding their largest fraction.
///
/// Returns per-container package indices, per-package container indices, and
/// per-package best fractions. A package with no area holds no fraction of
/// anything: the reference divides by it and crashes, this reads 0.
fn slot_into_containers(
    containers: &[Rect],
    packages: &[Rect],
    overlap_threshold: f64,
    unique_assignment: bool,
    forced_assignment: bool,
) -> (Vec<Vec<usize>>, Vec<Vec<usize>>, Vec<f64>) {
    let mut container_assignments: Vec<Vec<usize>> = vec![Vec::new(); containers.len()];
    let mut package_assignments: Vec<Vec<usize>> = vec![Vec::new(); packages.len()];
    let mut best_match_scores = Vec::new();
    if containers.is_empty() || packages.is_empty() {
        return (
            container_assignments,
            package_assignments,
            best_match_scores,
        );
    }
    for (package_num, package) in packages.iter().enumerate() {
        let package_area = package.area();
        let mut match_scores: Vec<(usize, f64)> = containers
            .iter()
            .enumerate()
            .map(|(container_num, container)| {
                let intersect_area = container.intersect(package).area();
                let overlap = if package_area > 0.0 {
                    intersect_area / package_area
                } else {
                    0.0
                };
                (container_num, overlap)
            })
            .collect();
        // Best fraction first, stable: ties keep the lowest container.
        match_scores.sort_by(|a, b| b.1.total_cmp(&a.1));
        let &(best_container, best_score) = match_scores.first().expect("containers is not empty");
        best_match_scores.push(best_score);
        if forced_assignment || best_score >= overlap_threshold {
            container_assignments[best_container].push(package_num);
            package_assignments[package_num].push(best_container);
        }
        if !unique_assignment {
            for &(container_num, score) in match_scores.iter().skip(1) {
                if score >= overlap_threshold {
                    container_assignments[container_num].push(package_num);
                    package_assignments[package_num].push(container_num);
                } else {
                    break;
                }
            }
        }
    }
    (
        container_assignments,
        package_assignments,
        best_match_scores,
    )
}

/// Non-maxima suppression by shared containment of the tokens.
fn nms_by_containment(objects: &[Obj], tokens: &[Tok]) -> Vec<Obj> {
    let mut sorted = objects.to_vec();
    sort_by_score_desc(&mut sorted);
    let containers: Vec<Rect> = sorted.iter().map(|o| o.bbox.clone()).collect();
    let packages: Vec<Rect> = tokens.iter().map(|t| t.bbox.clone()).collect();
    let (packages_by_container, _, _) =
        slot_into_containers(&containers, &packages, 0.5, true, false);
    let mut suppression = vec![false; sorted.len()];
    for object2 in 1..sorted.len() {
        // Set semantics, as the reference's `set` reads them.
        let packages2: std::collections::BTreeSet<usize> =
            packages_by_container[object2].iter().copied().collect();
        if packages2.is_empty() {
            suppression[object2] = true;
        }
        for object1 in 0..object2 {
            if suppression[object1] {
                continue;
            }
            let shares = packages_by_container[object1]
                .iter()
                .any(|p| packages2.contains(p));
            if shares {
                suppression[object2] = true;
            }
        }
    }
    sorted
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !suppression[*i])
        .map(|(_, o)| o)
        .collect()
}

/// Drop the objects no non-blank token overlaps by half.
fn remove_objects_without_content(tokens: &[Tok], objects: &mut Vec<Obj>) {
    objects.retain(|o| {
        tokens
            .iter()
            .any(|t| !t.blank && overlaps(&t.bbox, &o.bbox, 0.5))
    });
}

/// Non-maxima suppression: a lower-confidence object overlapping a kept one
/// past the threshold goes.
///
/// Overlap is the lower-confidence object's own fraction (`object2_overlap`)
/// and the higher score wins: every call on the structure path uses exactly
/// this, so the reference's other criteria are not ported. A zero area
/// measures nothing: the reference divides by it and recovers from the
/// crash, this skips the pair.
fn nms(objects: &[Obj], match_threshold: f64) -> Vec<Obj> {
    if objects.is_empty() {
        return Vec::new();
    }
    let mut sorted = objects.to_vec();
    sort_by_score_desc(&mut sorted);
    let mut suppression = vec![false; sorted.len()];
    for object2 in 1..sorted.len() {
        let area2 = sorted[object2].bbox.area();
        for object1 in 0..object2 {
            if suppression[object1] {
                continue;
            }
            let intersect = sorted[object1].bbox.intersect(&sorted[object2].bbox).area();
            let metric = if area2 > 0.0 {
                Some(intersect / area2)
            } else {
                None
            };
            if metric.is_some_and(|m| m >= match_threshold) {
                suppression[object2] = true;
                break;
            }
        }
    }
    sorted
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !suppression[*i])
        .map(|(_, o)| o)
        .collect()
}

/// Rows through containment NMS (or plain NMS without tokens), contentless
/// rows dropped, top to bottom.
fn refine_rows(rows: &[Obj], tokens: &[Tok]) -> Vec<Obj> {
    let mut out = if tokens.is_empty() {
        nms(rows, 0.5)
    } else {
        let mut kept = nms_by_containment(rows, tokens);
        remove_objects_without_content(tokens, &mut kept);
        kept
    };
    if out.len() > 1 {
        sort_top_to_bottom(&mut out);
    }
    out
}

/// Columns, likewise, left to right (plain NMS at 0.25 without tokens).
fn refine_columns(columns: &[Obj], tokens: &[Tok]) -> Vec<Obj> {
    let mut out = if tokens.is_empty() {
        nms(columns, 0.25)
    } else {
        let mut kept = nms_by_containment(columns, tokens);
        remove_objects_without_content(tokens, &mut kept);
        kept
    };
    if out.len() > 1 {
        sort_left_to_right(&mut out);
    }
    out
}

/// Snap columns' top and bottom to the table box.
fn align_columns(columns: &mut [Obj], bbox: &Rect) {
    for column in columns.iter_mut() {
        column.bbox.y0 = bbox.y0;
        column.bbox.y1 = bbox.y1;
    }
}

/// Snap rows' left and right to the table box.
fn align_rows(rows: &mut [Obj], bbox: &Rect) {
    for row in rows.iter_mut() {
        row.bbox.x0 = bbox.x0;
        row.bbox.x1 = bbox.x1;
    }
}

/// Kept detections as a consistent set of structures: rows, columns,
/// headers and spanning cells, conflicts resolved, boxes aligned.
fn objects_to_table_structures(objects: &[Obj], tokens: &[Tok]) -> Structures {
    let columns: Vec<Obj> = objects
        .iter()
        .filter(|o| o.label < TATR_LABELS.len() && TATR_LABELS[o.label] == "table column")
        .cloned()
        .collect();
    let mut rows: Vec<Obj> = objects
        .iter()
        .filter(|o| o.label < TATR_LABELS.len() && TATR_LABELS[o.label] == "table row")
        .cloned()
        .collect();
    let headers: Vec<Obj> = objects
        .iter()
        .filter(|o| o.label < TATR_LABELS.len() && TATR_LABELS[o.label] == "table column header")
        .cloned()
        .collect();
    let mut supercells: Vec<Obj> = objects
        .iter()
        .filter(|o| o.label < TATR_LABELS.len() && TATR_LABELS[o.label] == "table spanning cell")
        .cloned()
        .collect();
    for obj in supercells.iter_mut() {
        obj.subheader = false;
    }
    let mut subheaders: Vec<Obj> = objects
        .iter()
        .filter(|o| {
            o.label < TATR_LABELS.len() && TATR_LABELS[o.label] == "table projected row header"
        })
        .cloned()
        .collect();
    for obj in subheaders.iter_mut() {
        obj.subheader = true;
    }
    supercells.append(&mut subheaders);
    // The first header marking; `align_headers` recomputes it below, but
    // only where the refinement runs, so this stands where it does not.
    for row in rows.iter_mut() {
        row.header = false;
        for header in headers.iter() {
            if iob(&row.bbox, &header.bbox) >= 0.5 {
                row.header = true;
            }
        }
    }
    // (`page` is write-only in the reference and travels no further.)

    let mut rows = refine_rows(&rows, tokens);
    let mut columns = refine_columns(&columns, tokens);

    // Shrink the table box to the rows' height and the columns' width.
    let mut row_rect = Rect::empty();
    for row in rows.iter() {
        row_rect.include_rect(&row.bbox);
    }
    let mut column_rect = Rect::empty();
    for column in columns.iter() {
        column_rect.include_rect(&column.bbox);
    }
    let table_bbox = Rect {
        x0: column_rect.x0,
        y0: row_rect.y0,
        x1: column_rect.x1,
        y1: row_rect.y1,
    };
    align_columns(&mut columns, &table_bbox);
    align_rows(&mut rows, &table_bbox);

    let mut structures = Structures {
        rows,
        columns,
        headers,
        supercells,
    };
    if !structures.rows.is_empty() && structures.columns.len() > 1 {
        // (`page_spans` is dead in the reference -- the gap fills using it
        // are commented out -- and travels no further.)
        refine_table_structures(&mut structures);
    }
    structures
}

/// Headers and spanning cells reconciled against the rows and columns.
fn refine_table_structures(structures: &mut Structures) {
    let headers = apply_threshold(
        std::mem::take(&mut structures.headers),
        TATR_CLASS_THRESHOLD,
    );
    let headers = nms(&headers, 0.05);
    let aligned = align_headers(&headers, &mut structures.rows);
    structures.headers = aligned
        .into_iter()
        .map(|bbox| Obj {
            bbox,
            label: 3,
            score: 0.0,
            header: false,
            subheader: false,
            row_numbers: Vec::new(),
            column_numbers: Vec::new(),
        })
        .collect();

    let mut supercells = std::mem::take(&mut structures.supercells);
    let mut subheaders = Vec::new();
    supercells.retain(|s| {
        if s.subheader {
            subheaders.push(s.clone());
            false
        } else {
            true
        }
    });
    let mut supercells = apply_threshold(supercells, TATR_CLASS_THRESHOLD);
    let subheaders = apply_threshold(subheaders, TATR_CLASS_THRESHOLD);
    supercells.extend(subheaders);
    let mut supercells = align_supercells(supercells, &structures.rows, &structures.columns);
    supercells = nms_supercells(supercells);
    header_supercell_tree(&mut supercells);
    structures.supercells = supercells;
}

/// Objects scoring at least `threshold`, in order.
fn apply_threshold(objects: Vec<Obj>, threshold: f64) -> Vec<Obj> {
    objects
        .into_iter()
        .filter(|o| o.score >= threshold)
        .collect()
}

/// The header boundary as the convex hull of the rows it covers by half.
///
/// One header only: rows above the first covered row join it, and the walk
/// stops at the first gap. Rows of no height cover nothing: the reference
/// divides by the height and crashes, this skips them.
fn align_headers(headers: &[Obj], rows: &mut [Obj]) -> Vec<Rect> {
    for row in rows.iter_mut() {
        row.header = false;
    }
    let mut header_row_nums = Vec::new();
    for header in headers.iter() {
        for (row_num, row) in rows.iter().enumerate() {
            let row_height = row.bbox.y1 - row.bbox.y0;
            if row_height <= 0.0 {
                continue;
            }
            let overlap_top = row.bbox.y0.max(header.bbox.y0);
            let overlap_bottom = row.bbox.y1.min(header.bbox.y1);
            if (overlap_bottom - overlap_top) / row_height >= 0.5 {
                header_row_nums.push(row_num);
            }
        }
    }
    if header_row_nums.is_empty() {
        return Vec::new();
    }
    if header_row_nums[0] > 0 {
        let mut grown: Vec<usize> = (0..=header_row_nums[0]).collect();
        grown.extend_from_slice(&header_row_nums);
        header_row_nums = grown;
    }
    let mut header_rect = Rect::empty();
    let mut last_row_num: isize = -1;
    for row_num in header_row_nums {
        // A repeated row number breaks the walk, as the reference's does:
        // only the exact next row continues it.
        if row_num as isize == last_row_num + 1 {
            rows[row_num].header = true;
            header_rect.include_rect(&rows[row_num].bbox.clone());
            last_row_num = row_num as isize;
        } else {
            break;
        }
    }
    vec![header_rect]
}

/// Each spanning cell snapped to the rows and columns it covers by half.
///
/// A cell spanning the header boundary keeps its larger side; one covering
/// no row or no column, or a single grid cell, goes. (`span` supercells do
/// not occur: nothing in the reference ever sets the key, so its branches
/// are not ported.)
fn align_supercells(supercells: Vec<Obj>, rows: &[Obj], columns: &[Obj]) -> Vec<Obj> {
    let mut aligned = Vec::new();
    for mut supercell in supercells {
        supercell.header = false;
        let mut row_rect: Option<Rect> = None;
        let mut col_rect: Option<Rect> = None;
        let mut header_rows = std::collections::BTreeSet::new();
        let mut data_rows = std::collections::BTreeSet::new();
        for (row_num, row) in rows.iter().enumerate() {
            let row_height = row.bbox.y1 - row.bbox.y0;
            if row_height <= 0.0 {
                continue;
            }
            let overlap = row.bbox.y1.min(supercell.bbox.y1) - row.bbox.y0.max(supercell.bbox.y0);
            if overlap / row_height >= 0.5 {
                if row.header {
                    header_rows.insert(row_num);
                } else {
                    data_rows.insert(row_num);
                }
            }
        }
        if !data_rows.is_empty() && !header_rows.is_empty() {
            if data_rows.len() > header_rows.len() {
                header_rows.clear();
            } else {
                data_rows.clear();
            }
        }
        if !header_rows.is_empty() {
            supercell.header = true;
        }
        let intersecting_rows: std::collections::BTreeSet<usize> =
            data_rows.union(&header_rows).copied().collect();
        for row_num in intersecting_rows.iter() {
            match row_rect.as_mut() {
                Some(rect) => rect.include_rect(&rows[*row_num].bbox),
                None => row_rect = Some(rows[*row_num].bbox.clone()),
            }
        }
        let Some(row_rect) = row_rect else {
            continue;
        };

        let mut intersecting_cols = Vec::new();
        for (col_num, col) in columns.iter().enumerate() {
            let col_width = col.bbox.x1 - col.bbox.x0;
            if col_width <= 0.0 {
                continue;
            }
            let overlap = col.bbox.x1.min(supercell.bbox.x1) - col.bbox.x0.max(supercell.bbox.x0);
            if overlap / col_width >= 0.5 {
                intersecting_cols.push(col_num);
                match col_rect.as_mut() {
                    Some(rect) => rect.include_rect(&col.bbox),
                    None => col_rect = Some(col.bbox.clone()),
                }
            }
        }
        let Some(col_rect) = col_rect else {
            continue;
        };

        supercell.bbox = row_rect.intersect(&col_rect);
        // Only a true spanning cell survives: across several rows or
        // columns. Row numbers sort ascending; the reference lists a set,
        // whose order nothing downstream reads.
        if !intersecting_rows.is_empty()
            && !intersecting_cols.is_empty()
            && (intersecting_rows.len() > 1 || intersecting_cols.len() > 1)
        {
            supercell.row_numbers = intersecting_rows.into_iter().collect();
            supercell.column_numbers = intersecting_cols;
            aligned.push(supercell);
        }
    }
    aligned
}

/// Overlapping spanning cells, shrunk apart: the lower-confidence one loses
/// grid cells until the overlap is gone, then goes if nothing is left.
fn nms_supercells(supercells: Vec<Obj>) -> Vec<Obj> {
    let mut sorted = supercells;
    sort_by_score_desc(&mut sorted);
    let mut suppression = vec![false; sorted.len()];
    for second in 1..sorted.len() {
        for first in 0..second {
            let (head, tail) = sorted.split_at_mut(second);
            remove_supercell_overlap(&head[first], &mut tail[0]);
        }
        let cell = &sorted[second];
        if (cell.row_numbers.len() < 2 && cell.column_numbers.len() < 2)
            || cell.row_numbers.is_empty()
            || cell.column_numbers.is_empty()
        {
            suppression[second] = true;
        }
    }
    sorted
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !suppression[*i])
        .map(|(_, o)| o)
        .collect()
}

/// Shrink `second` off `first`'s grid cells, one row or column at a time:
/// the shorter axis goes, which removes fewer cells.
fn remove_supercell_overlap(first: &Obj, second: &mut Obj) {
    let mut common_rows: std::collections::BTreeSet<usize> = first
        .row_numbers
        .iter()
        .copied()
        .filter(|r| second.row_numbers.contains(r))
        .collect();
    let mut common_columns: std::collections::BTreeSet<usize> = first
        .column_numbers
        .iter()
        .copied()
        .filter(|c| second.column_numbers.contains(c))
        .collect();
    while !common_rows.is_empty() && !common_columns.is_empty() {
        if second.row_numbers.len() < second.column_numbers.len() {
            let min_column = second.column_numbers.iter().copied().min();
            let max_column = second.column_numbers.iter().copied().max();
            if max_column.is_some_and(|m| common_columns.contains(&m)) {
                let m = max_column.expect("a maximum was found");
                common_columns.remove(&m);
                second.column_numbers.retain(|&c| c != m);
            } else if min_column.is_some_and(|m| common_columns.contains(&m)) {
                let m = min_column.expect("a minimum was found");
                common_columns.remove(&m);
                second.column_numbers.retain(|&c| c != m);
            } else {
                second.column_numbers.clear();
                common_columns.clear();
            }
        } else if !(max_row_in_common(second, &mut common_rows)
            || min_row_in_common(second, &mut common_rows))
        {
            second.row_numbers.clear();
            common_rows.clear();
        }
    }
}

/// Remove `second`'s highest row where it is a common one.
fn max_row_in_common(
    second: &mut Obj,
    common_rows: &mut std::collections::BTreeSet<usize>,
) -> bool {
    if let Some(max_row) = second.row_numbers.iter().copied().max() {
        if common_rows.contains(&max_row) {
            common_rows.remove(&max_row);
            second.row_numbers.retain(|&r| r != max_row);
            return true;
        }
    }
    false
}

/// Remove `second`'s lowest row where it is a common one.
fn min_row_in_common(
    second: &mut Obj,
    common_rows: &mut std::collections::BTreeSet<usize>,
) -> bool {
    if let Some(min_row) = second.row_numbers.iter().copied().min() {
        if common_rows.contains(&min_row) {
            common_rows.remove(&min_row);
            second.row_numbers.retain(|&r| r != min_row);
            return true;
        }
    }
    false
}

/// No header cell below two parents: a header spanning cell with anything
/// but exactly one ancestor per row above it goes.
fn header_supercell_tree(supercells: &mut Vec<Obj>) {
    let mut ordered: Vec<Obj> = supercells.iter().filter(|s| s.header).cloned().collect();
    sort_by_score_desc(&mut ordered);
    // Over a snapshot, as the reference iterates its copy: removals land in
    // `supercells`, never in the list being walked.
    for index in 0..ordered.len() {
        let cell = ordered[index].clone();
        let min_row = cell.row_numbers.iter().copied().min().unwrap_or(0);
        let mut ancestors_by_row: std::collections::HashMap<usize, usize> =
            std::collections::HashMap::new();
        for other in ordered.iter() {
            let max_other = other.row_numbers.iter().copied().max().unwrap_or(0);
            if max_other < min_row
                && cell
                    .column_numbers
                    .iter()
                    .all(|c| other.column_numbers.contains(c))
            {
                for row in other.row_numbers.iter() {
                    *ancestors_by_row.entry(*row).or_insert(0) += 1;
                }
            }
        }
        let mut bad = false;
        for row in 0..min_row {
            if ancestors_by_row.get(&row).copied().unwrap_or(0) != 1 {
                bad = true;
                break;
            }
        }
        if bad {
            // The first equal cell, as the reference's `remove` takes it.
            if let Some(pos) = supercells.iter().position(|s| *s == cell) {
                supercells.remove(pos);
            }
        }
    }
}

/// One grid cell under construction: rows, columns and header flag set, the
/// spans it holds beside it for the text-adjusted boxes.
#[derive(Debug, Clone)]
struct GridCell {
    bbox: Rect,
    row_nums: Vec<usize>,
    column_nums: Vec<usize>,
    header: bool,
    spans: Vec<usize>,
}

/// Consistent structures as grid cells: row-by-column intersections plus
/// the spanning cells, boxes dilated to their rows and columns and then
/// adjusted to the text they hold, with the reference confidence.
fn table_structure_to_cells(structures: &Structures, tokens: &[Tok]) -> (Vec<GridCell>, f64) {
    let rows = &structures.rows;
    let columns = &structures.columns;
    let supercells = &structures.supercells;
    let mut cells = Vec::new();
    let mut subcells = Vec::new();

    // Column-major emission, as the reference emits; row-major comes last.
    for (column_num, column) in columns.iter().enumerate() {
        for (row_num, row) in rows.iter().enumerate() {
            let cell_rect = row.bbox.intersect(&column.bbox);
            let header = row.header;
            // A cell of no area is no subcell of anything: the reference
            // divides by the area and crashes, this keeps the cell.
            let subcell = cell_rect.area() > 0.0
                && supercells
                    .iter()
                    .any(|s| s.bbox.intersect(&cell_rect).area() / cell_rect.area() > 0.5);
            let cell = GridCell {
                bbox: cell_rect,
                row_nums: vec![row_num],
                column_nums: vec![column_num],
                header,
                spans: Vec::new(),
            };
            if subcell {
                subcells.push(cell);
            } else {
                cells.push(cell);
            }
        }
    }

    for supercell in supercells.iter() {
        let mut cell_rows = std::collections::BTreeSet::new();
        let mut cell_columns = std::collections::BTreeSet::new();
        let mut cell_rect: Option<Rect> = None;
        let mut header = true;
        for subcell in subcells.iter() {
            let sub_area = subcell.bbox.area();
            if sub_area > 0.0 && subcell.bbox.intersect(&supercell.bbox).area() / sub_area > 0.5 {
                match cell_rect.as_mut() {
                    Some(rect) => rect.include_rect(&subcell.bbox),
                    None => cell_rect = Some(subcell.bbox.clone()),
                }
                cell_rows.extend(subcell.row_nums.iter().copied());
                cell_columns.extend(subcell.column_nums.iter().copied());
                // All subcells header, or the supercell is not: a single
                // data subcell would un-rectangle the header region.
                header = header && subcell.header;
            }
        }
        if !cell_rows.is_empty() && !cell_columns.is_empty() {
            cells.push(GridCell {
                // (`subheader` is write-only from here and travels no
                // further.)
                bbox: cell_rect.expect("a claimed subcell unions a rect"),
                row_nums: cell_rows.into_iter().collect(),
                column_nums: cell_columns.into_iter().collect(),
                header,
                spans: Vec::new(),
            });
        }
    }

    let cell_rects: Vec<Rect> = cells.iter().map(|c| c.bbox.clone()).collect();
    let token_rects: Vec<Rect> = tokens.iter().map(|t| t.bbox.clone()).collect();
    let (_, _, cell_match_scores) =
        slot_into_containers(&cell_rects, &token_rects, 0.5, true, false);
    let confidence = if cell_match_scores.is_empty() {
        0.0
    } else {
        let mean = cell_match_scores.iter().sum::<f64>() / cell_match_scores.len() as f64;
        let min = cell_match_scores
            .iter()
            .copied()
            .fold(f64::INFINITY, f64::min);
        (mean + min) / 2.0
    };

    // Dilate every cell to its rows and columns (the gap fills are commented
    // out in the reference, so the dilated boxes are the boxes).
    for cell in cells.iter_mut() {
        let mut column_rect = Rect::empty();
        for &column_num in cell.column_nums.iter() {
            column_rect.include_rect(&columns[column_num].bbox);
        }
        let mut row_rect = Rect::empty();
        for &row_num in cell.row_nums.iter() {
            row_rect.include_rect(&rows[row_num].bbox);
        }
        cell.bbox = column_rect.intersect(&row_rect);
    }

    let cell_rects: Vec<Rect> = cells.iter().map(|c| c.bbox.clone()).collect();
    let (span_nums_by_cell, _, _) =
        slot_into_containers(&cell_rects, &token_rects, 0.001, true, false);
    for (cell, span_nums) in cells.iter_mut().zip(span_nums_by_cell) {
        cell.spans = span_nums;
    }

    // Adjust rows, columns and cells to the text they hold.
    let num_rows = rows.len();
    let num_columns = columns.len();
    let mut ordered_rows = rows.clone();
    sort_top_to_bottom(&mut ordered_rows);
    let mut ordered_columns = columns.clone();
    sort_left_to_right(&mut ordered_columns);
    let mut min_x_by_column: Vec<Vec<f64>> = vec![Vec::new(); num_columns];
    let mut max_x_by_column: Vec<Vec<f64>> = vec![Vec::new(); num_columns];
    let mut min_y_by_row: Vec<Vec<f64>> = vec![Vec::new(); num_rows];
    let mut max_y_by_row: Vec<Vec<f64>> = vec![Vec::new(); num_rows];
    for cell in cells.iter() {
        let min_row = cell.row_nums.iter().copied().min().unwrap_or(0);
        let max_row = cell.row_nums.iter().copied().max().unwrap_or(0);
        let min_column = cell.column_nums.iter().copied().min().unwrap_or(0);
        let max_column = cell.column_nums.iter().copied().max().unwrap_or(0);
        for &span in cell.spans.iter() {
            let bbox = &tokens[span].bbox;
            min_x_by_column[min_column].push(bbox.x0);
            min_y_by_row[min_row].push(bbox.y0);
            max_x_by_column[max_column].push(bbox.x1);
            max_y_by_row[max_row].push(bbox.y1);
        }
    }
    for (row_num, row) in ordered_rows.iter_mut().enumerate() {
        if !min_x_by_column[0].is_empty() {
            row.bbox.x0 = min_x_by_column[0]
                .iter()
                .copied()
                .fold(f64::INFINITY, f64::min);
        }
        if !min_y_by_row[row_num].is_empty() {
            row.bbox.y0 = min_y_by_row[row_num]
                .iter()
                .copied()
                .fold(f64::INFINITY, f64::min);
        }
        if !max_x_by_column[num_columns - 1].is_empty() {
            row.bbox.x1 = max_x_by_column[num_columns - 1]
                .iter()
                .copied()
                .fold(f64::NEG_INFINITY, f64::max);
        }
        if !max_y_by_row[row_num].is_empty() {
            row.bbox.y1 = max_y_by_row[row_num]
                .iter()
                .copied()
                .fold(f64::NEG_INFINITY, f64::max);
        }
    }
    for (column_num, column) in ordered_columns.iter_mut().enumerate() {
        if !min_x_by_column[column_num].is_empty() {
            column.bbox.x0 = min_x_by_column[column_num]
                .iter()
                .copied()
                .fold(f64::INFINITY, f64::min);
        }
        if !min_y_by_row[0].is_empty() {
            column.bbox.y0 = min_y_by_row[0]
                .iter()
                .copied()
                .fold(f64::INFINITY, f64::min);
        }
        if !max_x_by_column[column_num].is_empty() {
            column.bbox.x1 = max_x_by_column[column_num]
                .iter()
                .copied()
                .fold(f64::NEG_INFINITY, f64::max);
        }
        if !max_y_by_row[num_rows - 1].is_empty() {
            column.bbox.y1 = max_y_by_row[num_rows - 1]
                .iter()
                .copied()
                .fold(f64::NEG_INFINITY, f64::max);
        }
    }
    for cell in cells.iter_mut() {
        let mut row_rect = Rect::empty();
        for &row_num in cell.row_nums.iter() {
            row_rect.include_rect(&ordered_rows[row_num].bbox);
        }
        let mut column_rect = Rect::empty();
        for &column_num in cell.column_nums.iter() {
            column_rect.include_rect(&ordered_columns[column_num].bbox);
        }
        let adjusted = row_rect.intersect(&column_rect);
        if adjusted.area() > 0.0 {
            cell.bbox = adjusted;
        }
    }

    (cells, confidence)
}

/// Kept detections and tokens as structures, grid cells and confidence.
///
/// No rows or no columns is an empty table at zero confidence, as the
/// reference reads it.
fn objects_to_cells(objects: &[Obj], tokens: &[Tok]) -> (Structures, Vec<TatrCell>, f64) {
    let structures = objects_to_table_structures(objects, tokens);
    if structures.columns.is_empty() || structures.rows.is_empty() {
        return (structures, Vec::new(), 0.0);
    }
    let (cells, confidence) = table_structure_to_cells(&structures, tokens);
    let mut out: Vec<TatrCell> = cells
        .into_iter()
        .filter_map(|cell| {
            let mut row_nums = cell.row_nums;
            let mut column_nums = cell.column_nums;
            row_nums.sort_unstable();
            column_nums.sort_unstable();
            let (row, col) = match (row_nums.first(), column_nums.first()) {
                (Some(&row), Some(&col)) => (row, col),
                _ => return None,
            };
            Some(TatrCell {
                row,
                col,
                row_span: row_nums.len(),
                col_span: column_nums.len(),
                header: cell.header,
                bbox: [cell.bbox.x0, cell.bbox.y0, cell.bbox.x1, cell.bbox.y1],
            })
        })
        .collect();
    // Row-major, as the reference's cells read.
    out.sort_by(|a, b| (a.row, a.col).cmp(&(b.row, b.col)));
    (structures, out, confidence)
}

/// Crop pixel bounds: left, top, width, height.
type CropBounds = (u32, u32, u32, u32);

/// A region in PDF points (y up) as crop pixels, and the crop box in points
/// read back off them.
///
/// The region grows by the reference 10 px at 150 dpi on every side,
/// clamped to the page; then, pixels first, the integer bounds ARE the
/// geometry, exactly as the eval crops. `None` where nothing is left.
fn crop_geometry(
    region: [f64; 4],
    page_width_pt: f64,
    page_height_pt: f64,
) -> Option<(CropBounds, [f64; 4])> {
    let scale = f64::from(TATR_DPI) / 72.0;
    let pad = TATR_PAD_PX * 72.0 / f64::from(TATR_DPI);
    let x0 = (region[0].min(region[2]) - pad).clamp(0.0, page_width_pt);
    let x1 = (region[0].max(region[2]) + pad).clamp(0.0, page_width_pt);
    let y0 = (region[1].min(region[3]) - pad).clamp(0.0, page_height_pt);
    let y1 = (region[1].max(region[3]) + pad).clamp(0.0, page_height_pt);
    if !(x1 > x0 && y1 > y0) {
        return None;
    }
    let left = (x0 * scale) as u32;
    let top = ((page_height_pt - y1) * scale) as u32;
    let right = (x1 * scale + 0.999) as u32;
    let bottom = ((page_height_pt - y0) * scale + 0.999) as u32;
    if right <= left || bottom <= top {
        return None;
    }
    let crop_box = [
        f64::from(left) / scale,
        page_height_pt - f64::from(bottom) / scale,
        f64::from(right) / scale,
        page_height_pt - f64::from(top) / scale,
    ];
    Some(((left, top, right - left, bottom - top), crop_box))
}

/// Crop pixels out of a row-major RGB page raster, top-left origin.
fn crop_rgb(
    rgb: &[u8],
    page_width: u32,
    page_height: u32,
    bounds: (u32, u32, u32, u32),
) -> Option<(Vec<u8>, u32, u32)> {
    let (left, top, width, height) = bounds;
    if width == 0 || height == 0 {
        return None;
    }
    let right = left.checked_add(width)?;
    let bottom = top.checked_add(height)?;
    if right > page_width || bottom > page_height {
        return None;
    }
    let mut out = Vec::with_capacity(width as usize * height as usize * 3);
    for row in top..bottom {
        let start = (row as usize * page_width as usize + left as usize) * 3;
        out.extend_from_slice(&rgb[start..start + width as usize * 3]);
    }
    Some((out, width, height))
}

/// MaxResize dimensions: the longest side to 1000, the aspect kept, halves
/// to even as the reference's `round` does.
pub fn max_resize_dims(width: u32, height: u32) -> (u32, u32) {
    let longest = width.max(height).max(1) as f64;
    let scale = TATR_MAX_SIZE / longest;
    (
        (f64::from(width) * scale).round_ties_even() as u32,
        (f64::from(height) * scale).round_ties_even() as u32,
    )
}

/// Bilinear resize of row-major RGB, to the reference's geometry.
///
/// Centre-mapped sampling with clamped edges, the mapping PIL's bilinear
/// uses; values may land an lsb off PIL's fixed-point passes, which the
/// end-to-end test holds to an approximate bar rather than an exact one.
pub fn resize_bilinear_rgb(
    rgb: &[u8],
    width: u32,
    height: u32,
    new_width: u32,
    new_height: u32,
) -> Vec<u8> {
    if width == 0 || height == 0 || new_width == 0 || new_height == 0 {
        return Vec::new();
    }
    let sample = |x: f64, y: f64, channel: usize| -> f64 {
        let x0 = x.floor().clamp(0.0, f64::from(width - 1));
        let y0 = y.floor().clamp(0.0, f64::from(height - 1));
        let x1 = (x0 + 1.0).min(f64::from(width - 1));
        let y1 = (y0 + 1.0).min(f64::from(height - 1));
        let fx = (x - x.floor()).clamp(0.0, 1.0);
        let fy = (y - y.floor()).clamp(0.0, 1.0);
        let at = |x: f64, y: f64| {
            f64::from(rgb[(y as usize * width as usize + x as usize) * 3 + channel])
        };
        at(x0, y0) * (1.0 - fx) * (1.0 - fy)
            + at(x1, y0) * fx * (1.0 - fy)
            + at(x0, y1) * (1.0 - fx) * fy
            + at(x1, y1) * fx * fy
    };
    let mut out = Vec::with_capacity(new_width as usize * new_height as usize * 3);
    for out_y in 0..new_height {
        let y = (f64::from(out_y) + 0.5) * f64::from(height) / f64::from(new_height) - 0.5;
        for out_x in 0..new_width {
            let x = (f64::from(out_x) + 0.5) * f64::from(width) / f64::from(new_width) - 0.5;
            for channel in 0..3 {
                out.push(sample(x, y, channel).round().clamp(0.0, 255.0) as u8);
            }
        }
    }
    out
}

/// Resized RGB as the model's NCHW input: bytes over 255, normalised.
pub fn normalize_nchw(rgb: &[u8], width: u32, height: u32) -> Vec<f32> {
    let pixels = width as usize * height as usize;
    let mut out = vec![0.0f32; pixels * 3];
    for (i, px) in rgb.chunks_exact(3).enumerate() {
        for (c, &byte) in px.iter().enumerate() {
            out[c * pixels + i] = (f32::from(byte) / 255.0 - TATR_MEAN[c]) / TATR_STD[c];
        }
    }
    out
}

/// A PDF point box (y up) in crop pixels (y down), through the crop box.
pub fn pdf_to_crop_px(bbox: [f64; 4], crop_box: [f64; 4]) -> [f64; 4] {
    let scale = f64::from(TATR_DPI) / 72.0;
    [
        (bbox[0] - crop_box[0]) * scale,
        (crop_box[3] - bbox[3]) * scale,
        (bbox[2] - crop_box[0]) * scale,
        (crop_box[3] - bbox[1]) * scale,
    ]
}

/// A crop-pixel box (y down) in PDF points (y up), through the crop box.
pub fn crop_px_to_pdf(bbox: [f64; 4], crop_box: [f64; 4]) -> [f64; 4] {
    let scale = f64::from(TATR_DPI) / 72.0;
    [
        crop_box[0] + bbox[0] / scale,
        crop_box[3] - bbox[3] / scale,
        crop_box[0] + bbox[2] / scale,
        crop_box[3] - bbox[1] / scale,
    ]
}

/// Each cell's word indices, by word-centre-in-cell.
///
/// A word is claimed by the FIRST cell, in row-major order, that holds its
/// centre -- predicted boxes overlap, so without an order a word on a grid
/// line would join two cells. A word no cell holds is claimed by none.
/// Words keep their given order inside a cell. Both box lists share one
/// frame; the caller maps them there.
pub fn fill_cells(word_boxes: &[[f64; 4]], cell_boxes: &[[f64; 4]]) -> Vec<Vec<usize>> {
    let mut claims: Vec<Vec<usize>> = vec![Vec::new(); cell_boxes.len()];
    for (word, bbox) in word_boxes.iter().enumerate() {
        let cx = (bbox[0] + bbox[2]) / 2.0;
        let cy = (bbox[1] + bbox[3]) / 2.0;
        for (cell, cbox) in cell_boxes.iter().enumerate() {
            if cbox[0] <= cx && cx <= cbox[2] && cbox[1] <= cy && cy <= cbox[3] {
                claims[cell].push(word);
                break;
            }
        }
    }
    claims
}

/// One crop's NCHW input through the loaded TATR session: logits and
/// boxes, row-major. The outputs must be the head's own shapes.
pub fn infer_nchw(
    shared: &pdf_ocr::runtime::SharedSession,
    nchw: &[f32],
    width: u32,
    height: u32,
) -> Result<(Vec<f32>, Vec<f32>), std::io::Error> {
    let input =
        ndarray::Array4::from_shape_vec((1, 3, height as usize, width as usize), nchw.to_vec())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string()))?;
    let (logits, boxes) = shared
        .run(|session| {
            let tensor = ort::value::Tensor::from_array(input.into_dyn())?;
            let outputs = session.run(ort::inputs!["pixel_values" => tensor])?;
            // By name, not by position: a model whose outputs renamed is an
            // error, and indexing would panic instead of answering one.
            // The names are P1's export's own (`export_meta.output_names`):
            // `linear_96` holds the 125x7 logits, `sigmoid` the 125x4
            // boxes, told apart again by the shape checks below.
            let named = |name: &str| {
                outputs.get(name).ok_or_else(|| {
                    ort::Error::new(format!("tatr model has no output named {name}"))
                })
            };
            let logits = named("linear_96")?.try_extract_array::<f32>()?.to_owned();
            let boxes = named("sigmoid")?.try_extract_array::<f32>()?.to_owned();
            Ok((logits, boxes))
        })
        .map_err(std::io::Error::from)?;
    for (name, output, want) in [
        ("logits", &logits, [1, TATR_QUERIES, TATR_CLASSES]),
        ("pred_boxes", &boxes, [1, TATR_QUERIES, 4]),
    ] {
        if output.shape() != want {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("tatr {name} has shape {:?}, want {want:?}", output.shape()),
            ));
        }
    }
    Ok((
        logits.into_raw_vec_and_offset().0,
        boxes.into_raw_vec_and_offset().0,
    ))
}

/// What one TATR attempt over a region concluded: the hook's answer.
#[derive(Debug)]
pub(crate) enum TatrAttempt {
    /// Fired: the region's cells as blocks.
    Cells(Vec<Block>),
    /// Ran, but declined: the trace's reason.
    Declined(TatrDecline),
}

/// Where a TATR attempt failed, for the hook's trace and its fallback rule.
///
/// A failed SETUP -- [`TatrFailure::ModelMissing`] and
/// [`TatrFailure::ModelInvalid`] -- refuses the extraction up front, before
/// any region runs; a region only sees those past a file deleted mid-run. A
/// failed REGION -- [`TatrFailure::Render`] and [`TatrFailure::Inference`] --
/// falls back to the current grid for that region, traced, never fatal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TatrFailure {
    /// The page or region would not render: geometry, the render, the crop.
    Render,
    /// The model ran, or the decode read it, and failed on this region.
    Inference,
    /// The model file is absent.
    ModelMissing,
    /// The model or library is present but unusable: checksum, version, a
    /// conflicting init, a session that would not build.
    ModelInvalid,
}

/// A failed TATR attempt: the unchanged [`std::io::Error`] plus where it
/// failed. [`tatr_cells`] drops the stage, so its errors read exactly as
/// before this type existed.
#[derive(Debug)]
pub(crate) struct TatrAttemptError {
    /// Where the attempt failed.
    pub stage: TatrFailure,
    /// The failure itself, value for value what [`tatr_cells`] returned.
    pub error: std::io::Error,
}

impl TatrAttemptError {
    /// A render-path failure: geometry, the render, the crop.
    fn render(error: std::io::Error) -> Self {
        Self {
            stage: TatrFailure::Render,
            error,
        }
    }

    /// An inference- or decode-path failure on this region.
    fn inference(error: std::io::Error) -> Self {
        Self {
            stage: TatrFailure::Inference,
            error,
        }
    }

    /// A shared-layer failure: absent means missing, anything else means the
    /// setup is unusable. A session that would not BUILD is setup too -- it
    /// fails the same on every region -- so only the region's own run reads
    /// as inference.
    fn session(error: pdf_ocr::runtime::ModelError) -> Self {
        let stage = match error.kind {
            pdf_ocr::runtime::ModelErrorKind::ModelNotAvailable => TatrFailure::ModelMissing,
            _ => TatrFailure::ModelInvalid,
        };
        Self {
            stage,
            error: std::io::Error::from(error),
        }
    }
}

/// The table region rebuilt by TATR, or `None` where it does not fire.
///
/// Renders `page` at 150 dpi through the engine's renderer, crops `region`
/// (PDF points, y up) with the reference 10 px pad, runs the model through
/// the shared session, and builds the same [`Block`] cells
/// [`rebuild_region`](super::tables::rebuild_region) produces -- row,
/// column, spans, header flag, text filled from [`words_of_region`](super::tables::words_of_region)
/// words by word centre -- plus the region words no cell claimed, re-emitted
/// as the blocks they came from, so the P3 hook can swap them in. Words
/// inside the table extent but in no cell join the nearest cell first (the
/// gap fill), so only words past the extent come back as prose.
/// `page_blocks`, `lines`, `words` and `region_lines` are the rebuild's own
/// inputs.
///
/// `None` means TATR does not fire: fewer than two refined rows or columns,
/// the kept objects' mean score below the threshold, or the cells claiming
/// under the hook's coverage threshold of the words inside their own extent
/// ([`TatrDecline`], which [`tatr_attempt`] hands back). A missing model or
/// library, a checksum or version mismatch, and an inference failure are the
/// [`std::io::Error`] carrying the shared layer's [`pdf_ocr::runtime::ModelError`];
/// a page that will not render rides the same error.
///
/// [`Block`]: crate::blocks::Block
#[allow(clippy::too_many_arguments)]
pub fn tatr_cells(
    document: &PdfDocument,
    page: usize,
    region: [f64; 4],
    page_blocks: &[&Block],
    lines: &[judge::Line],
    words: &[PageWord],
    region_lines: &[usize],
    config: &pdf_ocr::runtime::RuntimeConfig,
) -> Result<Option<Vec<Block>>, std::io::Error> {
    match tatr_attempt(
        document,
        page,
        region,
        page_blocks,
        lines,
        words,
        region_lines,
        config,
        true,
    ) {
        Ok(TatrAttempt::Cells(cells)) => Ok(Some(cells)),
        Ok(TatrAttempt::Declined(_)) => Ok(None),
        Err(failed) => Err(failed.error),
    }
}

/// [`tatr_cells`], with the decline reason and the failure stage the P3 hook
/// traces: which refusal a decode earned, and where a failed attempt failed.
/// The one implementation -- [`tatr_cells`] maps it onto `Option`, dropping
/// nothing but the stage.
#[allow(clippy::too_many_arguments)]
pub(crate) fn tatr_attempt(
    document: &PdfDocument,
    page: usize,
    region: [f64; 4],
    page_blocks: &[&Block],
    lines: &[judge::Line],
    words: &[PageWord],
    region_lines: &[usize],
    config: &pdf_ocr::runtime::RuntimeConfig,
    cell_path_words: bool,
) -> Result<TatrAttempt, TatrAttemptError> {
    let (page_width, page_height) = document
        .page_geometry(page)
        .map(|g| g.effective_dimensions())
        .map_err(std::io::Error::other)
        .map_err(TatrAttemptError::render)?;
    let Some((bounds, crop_box)) = crop_geometry(region, page_width, page_height) else {
        return Err(TatrAttemptError::render(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!(
                "tatr region {region:?} covers no pixels at {} dpi",
                TATR_DPI
            ),
        )));
    };
    let rendered = document
        .render_page(
            page,
            &RenderOptions {
                dpi: f64::from(TATR_DPI),
                ..Default::default()
            },
        )
        .map_err(std::io::Error::other)
        .map_err(TatrAttemptError::render)?;
    // RGBA composited over white, as `vision` reads the same render.
    let mut rgb = Vec::with_capacity(rendered.width as usize * rendered.height as usize * 3);
    for px in rendered.pixels.chunks_exact(4) {
        let alpha = f32::from(px[3]) / 255.0;
        for &c in &px[0..3] {
            rgb.push((f32::from(c) * alpha + 255.0 * (1.0 - alpha)).round() as u8);
        }
    }
    let Some((crop, crop_width, crop_height)) =
        crop_rgb(&rgb, rendered.width, rendered.height, bounds)
    else {
        return Err(TatrAttemptError::render(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!(
                "tatr crop {bounds:?} falls outside the {}x{} render",
                rendered.width, rendered.height
            ),
        )));
    };
    let (resized_width, resized_height) = max_resize_dims(crop_width, crop_height);
    let resized = resize_bilinear_rgb(
        &crop,
        crop_width,
        crop_height,
        resized_width,
        resized_height,
    );
    let nchw = normalize_nchw(&resized, resized_width, resized_height);
    let shared =
        pdf_ocr::runtime::session(&TATR_SPEC, config).map_err(TatrAttemptError::session)?;
    let (logits, boxes) = infer_nchw(&shared, &nchw, resized_width, resized_height)
        .map_err(TatrAttemptError::inference)?;

    let tokens: Vec<TatrWord> = words
        .iter()
        .map(|w| TatrWord::new(w.text.clone(), pdf_to_crop_px(w.bbox, crop_box)))
        .collect();
    let decode = cells_from_raw_outputs(
        &logits,
        &boxes,
        f64::from(crop_width),
        f64::from(crop_height),
        &tokens,
    )
    .map_err(TatrAttemptError::inference)?;
    if let Some(declined) = decline_reason(decode.rows, decode.columns, decode.mean_kept_score) {
        return Ok(TatrAttempt::Declined(declined));
    }

    // THE ENGINE'S WORDS for the fill: each block's fragments aligned
    // onto its own text, so a character-spaced PDF reads the same in both
    // paths. The decode above keeps the raw page words as tokens.
    let fill_words =
        super::tables::tatr_fill_words_with_cell_path_words(words, page_blocks, cell_path_words);
    let (pair_words, from) =
        super::tables::words_of_region(page_blocks, lines, &fill_words, region_lines);
    if pair_words.is_empty() {
        return Ok(TatrAttempt::Cells(Vec::new()));
    }
    let page_number = region_lines
        .iter()
        .filter_map(|&i| lines.get(i))
        .filter_map(|line| page_blocks.get(line.block))
        .map(|block| block.page)
        .next()
        .unwrap_or(page);
    let cell_boxes: Vec<[f64; 4]> = decode
        .cells
        .iter()
        .map(|cell| crop_px_to_pdf(cell.bbox, crop_box))
        .collect();
    let word_boxes: Vec<[f64; 4]> = pair_words.iter().map(|w| w.bbox).collect();
    let mut claims = fill_cells(&word_boxes, &cell_boxes);
    // THE COVERAGE GUARD, on the direct claims: the cells must earn the
    // extent they draw, or the grid runs. Past it the gap fill lands every
    // inside word in a cell, and the hook re-emits the rest as prose, so a
    // fire never loses a word.
    let Some(extent) = super::tables::tatr_table_extent(&cell_boxes) else {
        return Ok(TatrAttempt::Declined(TatrDecline::LowCoverage {
            claimed: 0,
            inside: 0,
        }));
    };
    let (claimed_inside, inside) =
        super::tables::tatr_coverage_counts(&word_boxes, extent, &claims);
    if inside == 0 || claimed_inside as f64 / (inside as f64) < super::tables::TATR_MIN_COVERAGE {
        return Ok(TatrAttempt::Declined(TatrDecline::LowCoverage {
            claimed: claimed_inside,
            inside,
        }));
    }
    super::tables::tatr_fill_gaps(&word_boxes, &cell_boxes, extent, &mut claims);
    let placed: Vec<(grid::Cell, bool)> = decode
        .cells
        .iter()
        .zip(claims)
        .zip(cell_boxes)
        .map(|((cell, claimed), bbox)| {
            (
                grid::Cell {
                    words: claimed,
                    bbox,
                    rows: (cell.row..cell.row + cell.row_span).collect(),
                    columns: (cell.col..cell.col + cell.col_span).collect(),
                },
                cell.header,
            )
        })
        .collect();
    let out = super::tables::conserved_table_blocks(
        &placed,
        &pair_words,
        &from,
        page_blocks,
        page_number,
        cell_path_words,
    );
    Ok(TatrAttempt::Cells(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_hex64(s: &str) -> bool {
        s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
    }

    #[test]
    fn tatr_sha256_is_pinned_to_the_p1_export() {
        // RED until P1's terminal run lands the export's digest: a zero
        // placeholder verifies every file against nothing, and must never
        // ship. Set TATR_SHA256 from P1's export_meta and this greens.
        assert!(
            is_hex64(TATR_SHA256),
            "TATR_SHA256 is not 64 hex characters: {TATR_SHA256:?}"
        );
        assert_ne!(
            TATR_SHA256, "0000000000000000000000000000000000000000000000000000000000000000",
            "TATR_SHA256 is still the placeholder: pin P1's export digest"
        );
    }

    #[test]
    fn tatr_spec_names_the_package_and_its_route() {
        assert_eq!(TATR_SPEC.name, "tatr-v1.1-all");
        assert_eq!(TATR_SPEC.file, "tatr-v1.1-all.onnx");
        assert!(
            TATR_SPEC.install_command.contains("tatr-v1.1-all"),
            "the install line names the package: {}",
            TATR_SPEC.install_command
        );
        assert!(
            TATR_SPEC.install_command.contains("model dir"),
            "the install line names the route, never a download: {}",
            TATR_SPEC.install_command
        );
    }

    #[test]
    fn max_resize_dims_keeps_the_aspect_halves_to_even() {
        assert_eq!(max_resize_dims(400, 300), (1000, 750));
        assert_eq!(max_resize_dims(300, 400), (750, 1000));
        assert_eq!(max_resize_dims(1000, 1000), (1000, 1000));
        // Halves to even, as the reference's `round` does: 1.5 and 2.5
        // both land on 2, where round-half-away would split them.
        assert_eq!(max_resize_dims(3, 2000), (2, 1000));
        assert_eq!(max_resize_dims(5, 2000), (2, 1000));
    }

    #[test]
    fn bilinear_resize_matches_hand_values() {
        // Four corners, distinct reds: the 1x1 is their mean.
        let rgb = vec![0, 0, 0, 100, 100, 100, 200, 200, 200, 255, 255, 255];
        let out = resize_bilinear_rgb(&rgb, 2, 2, 1, 1);
        assert_eq!(out, vec![139, 139, 139]);
        // Same size: centre-mapped sampling lands on the pixels.
        let out = resize_bilinear_rgb(&rgb, 2, 2, 2, 2);
        assert_eq!(out, rgb);
        assert!(resize_bilinear_rgb(&rgb, 2, 2, 0, 1).is_empty());
        assert!(resize_bilinear_rgb(&[], 0, 0, 1, 1).is_empty());
    }

    #[test]
    fn normalize_nchw_matches_hand_values() {
        let out = normalize_nchw(&[255, 255, 255, 0, 0, 0], 2, 1);
        assert_eq!(out.len(), 6);
        for (c, &mean) in TATR_MEAN.iter().enumerate() {
            let white = (1.0 - mean) / TATR_STD[c];
            let black = (0.0 - mean) / TATR_STD[c];
            assert!((out[c * 2] - white).abs() < 1e-6, "white channel {c}");
            assert!((out[c * 2 + 1] - black).abs() < 1e-6, "black channel {c}");
        }
    }

    #[test]
    fn crop_geometry_pads_ten_px_and_reads_back() {
        let (bounds, crop_box) = crop_geometry([100.0, 100.0, 300.0, 200.0], 612.0, 792.0)
            .expect("a region on the page");
        // The 10 px pad at 150 dpi is 4.8 pt a side; pixels first.
        assert_eq!(bounds, (198, 1223, 437, 229));
        for (got, want) in crop_box.iter().zip([95.04, 95.04, 304.8, 204.96]) {
            assert!((got - want).abs() < 1e-9, "{crop_box:?}");
        }
        // Clamped to the page, never past it.
        let (bounds, _) = crop_geometry([0.0, 0.0, 10.0, 10.0], 612.0, 792.0).expect("clamped");
        assert_eq!((bounds.0, bounds.1), (0, 1619));
        assert!(crop_geometry([10.0, 10.0, 10.0, 10.0], 0.0, 0.0).is_none());
    }

    #[test]
    fn frames_round_trip() {
        let crop_box = [95.04, 95.04, 304.8, 204.96];
        let bbox = [100.0, 100.0, 200.0, 150.0];
        let back = crop_px_to_pdf(pdf_to_crop_px(bbox, crop_box), crop_box);
        for (got, want) in back.iter().zip(bbox) {
            assert!((got - want).abs() < 1e-9, "{back:?}");
        }
    }

    #[test]
    fn decode_rejects_misshapen_outputs() {
        let words = vec![];
        let err = cells_from_raw_outputs(&[0.0; 10], &[0.5; 500], 100.0, 100.0, &words)
            .expect_err("short logits refuse");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        let err = cells_from_raw_outputs(&[0.0; 875], &[0.5; 10], 100.0, 100.0, &words)
            .expect_err("short boxes refuse");
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    }

    /// 125 queries of background plus the given foreground rows.
    fn queries(foreground: &[(usize, [f32; 4])]) -> (Vec<f32>, Vec<f32>) {
        let mut logits = Vec::with_capacity(TATR_QUERIES * TATR_CLASSES);
        let mut boxes = Vec::with_capacity(TATR_QUERIES * 4);
        for _ in 0..TATR_QUERIES {
            let mut row = [0.0f32; TATR_CLASSES];
            row[TATR_CLASSES - 1] = 8.0;
            logits.extend_from_slice(&row);
            boxes.extend_from_slice(&[0.5, 0.5, 0.01, 0.01]);
        }
        for (i, (label, bbox)) in foreground.iter().enumerate() {
            let mut row = [0.0f32; TATR_CLASSES];
            row[*label] = 8.0;
            logits[i * TATR_CLASSES..(i + 1) * TATR_CLASSES].copy_from_slice(&row);
            boxes[i * 4..(i + 1) * 4].copy_from_slice(bbox);
        }
        (logits, boxes)
    }

    #[test]
    fn decode_keeps_the_foreground_argmax() {
        // One strong "table" query over background: kept, but no row or
        // column, so structures without cells.
        let (logits, boxes) = queries(&[(0, [0.5, 0.5, 1.0, 1.0])]);
        let decode =
            cells_from_raw_outputs(&logits, &boxes, 400.0, 300.0, &[]).expect("shapes hold");
        assert_eq!(decode.kept_objects, 1);
        assert!(decode.mean_kept_score > 0.99, "{}", decode.mean_kept_score);
        assert_eq!(decode.rows, 0);
        assert_eq!(decode.columns, 0);
        assert!(decode.cells.is_empty());
        assert_eq!(decode.confidence, 0.0);
    }

    #[test]
    fn decode_scales_boxes_to_the_crop() {
        // One row and one column, no words: the cell is their intersection
        // snapped to the table box, boxes in crop pixels.
        let (logits, boxes) = queries(&[(2, [0.5, 0.25, 1.0, 0.5]), (1, [0.25, 0.5, 0.5, 1.0])]);
        let decode =
            cells_from_raw_outputs(&logits, &boxes, 400.0, 300.0, &[]).expect("shapes hold");
        assert_eq!(decode.kept_objects, 2);
        assert_eq!(decode.rows, 1);
        assert_eq!(decode.columns, 1);
        assert_eq!(decode.cells.len(), 1);
        let cell = &decode.cells[0];
        assert_eq!(
            (cell.row, cell.col, cell.row_span, cell.col_span),
            (0, 0, 1, 1)
        );
        assert!(!cell.header);
        assert_eq!(cell.bbox, [0.0, 0.0, 200.0, 150.0]);
    }

    #[test]
    fn fires_needs_two_rows_two_columns_and_confidence() {
        assert!(fires(2, 2, 0.9));
        assert!(fires(5, 3, 0.5));
        assert!(!fires(1, 2, 0.9));
        assert!(!fires(2, 1, 0.9));
        assert!(!fires(0, 0, 0.0));
        assert!(!fires(2, 2, 0.49));
    }

    #[test]
    fn decline_reason_is_the_fires_rule_negated() {
        // The trace's reason and the firing decision are one predicate:
        // every refusal names its leg, and firing refuses nothing.
        assert_eq!(decline_reason(2, 2, 0.9), None);
        assert_eq!(decline_reason(5, 3, 0.5), None);
        assert_eq!(
            decline_reason(1, 2, 0.9),
            Some(TatrDecline::FewRowsOrColumns {
                rows: 1,
                columns: 2
            })
        );
        assert_eq!(
            decline_reason(2, 1, 0.9),
            Some(TatrDecline::FewRowsOrColumns {
                rows: 2,
                columns: 1
            })
        );
        // Too few rows or columns wins over low confidence: the count is
        // checked first, as `fires` conjoins it first.
        assert!(matches!(
            decline_reason(1, 1, 0.1),
            Some(TatrDecline::FewRowsOrColumns { .. })
        ));
        assert!(matches!(
            decline_reason(2, 2, 0.49),
            Some(TatrDecline::LowConfidence { .. })
        ));
        for (rows, columns, mean) in [
            (2, 2, 0.9),
            (5, 3, 0.5),
            (1, 2, 0.9),
            (2, 1, 0.9),
            (0, 0, 0.0),
            (2, 2, 0.49),
        ] {
            assert_eq!(
                fires(rows, columns, mean),
                decline_reason(rows, columns, mean).is_none(),
                "fires and decline_reason disagree at {rows}x{columns} {mean}"
            );
        }
    }

    #[test]
    fn fill_claims_the_first_cell_and_skips_strays() {
        // Overlapping cells: a word on the shared edge joins the first.
        let cells = [[0.0, 0.0, 10.0, 10.0], [5.0, 0.0, 15.0, 10.0]];
        let words = [
            [1.0, 1.0, 2.0, 2.0],
            [7.0, 1.0, 8.0, 2.0],
            [100.0, 100.0, 101.0, 101.0],
            [6.0, 1.0, 6.5, 2.0],
        ];
        assert_eq!(fill_cells(&words, &cells), vec![vec![0, 1, 3], vec![]]);
    }

    #[test]
    fn rect_matches_the_shipped_semantics() {
        assert_eq!(Rect::empty().area(), 0.0);
        // The first include becomes its argument.
        let mut grown = Rect::empty();
        grown.include_rect(&Rect::of([10.0, 20.0, 30.0, 40.0]));
        assert_eq!(grown, Rect::of([10.0, 20.0, 30.0, 40.0]));
        grown.include_rect(&Rect::of([0.0, 0.0, 12.0, 22.0]));
        assert_eq!(grown, Rect::of([0.0, 0.0, 30.0, 40.0]));
        // Intersect returns new and never touches its receiver.
        let first = Rect::of([0.0, 0.0, 10.0, 10.0]);
        let overlap = first.intersect(&Rect::of([5.0, 5.0, 15.0, 15.0]));
        assert_eq!(overlap, Rect::of([5.0, 5.0, 10.0, 10.0]));
        assert_eq!(first, Rect::of([0.0, 0.0, 10.0, 10.0]));
        assert_eq!(
            first.intersect(&Rect::of([20.0, 20.0, 30.0, 30.0])).area(),
            0.0
        );
    }
}
