#!/usr/bin/env python3
"""xfa_oracle_fault_classifier.py — Classify XFA low-SSIM cases by fault source.

Consumes the OV2-01 SSIM JSON output (from ``scripts/xfa_visual_evidence.py``)
and assigns each case to one of:

  - ``engine_bug``                  — our rendering deviates; oracle is correct.
  - ``oracle_mismatch_acroform_xfa``— oracle was rendered through the AcroForm
                                      flattening path while ours is XFA-native.
  - ``oracle_mismatch_rendering``   — oracle render artefact (subpixel/font diff).
  - ``ambiguous``                   — heuristics inconclusive.
  - ``both_fail``                   — both sides appear blank/errored.
  - ``high_fidelity_no_action``     — SSIM >= 0.94, no investigation needed.

For ``oracle_mismatch_rendering`` cases the classifier also assigns a
``fine_category`` subcategory (Wave 1 Agent D extension; QF2-E + QF4-F additions):

  - ``antialias_variant``        — pure sub-pixel AA difference; text strokes only.
  - ``font_metric``              — systematic font size / leading / tracking difference
                                   that distributes diff uniformly across all text.
  - ``element_position``         — one or more elements are geometrically shifted
                                   (localized dense diff block, not global).
  - ``clipping``                 — diff concentrated at page/element boundary edges.
  - ``border_rendering_delta``   — oracle renders vertical/horizontal border lines or
                                   table rules that are absent in our output; identified
                                   by very high column-mean variance, specific hot columns,
                                   and oracle being darker than ours (negative blank_gap).
                                   (QF2-E — added from gen-854_854654 corpus pattern.)
  - ``oracle_artifact``          — generalised oracle-side rendering artifact: oracle
                                   adds visible content that our output lacks (blank_gap
                                   negative), but the spatial signature does not match the
                                   strict border_rendering_delta hot-column pattern.
                                   Borders, AA rasterisation widths, anti-aliased fills.
                                   (QF4-F — formal generalisation of border_rendering_delta;
                                   see QF3_A_BORDER_RENDERING_REPORT.md and
                                   QF4_F_CLASSIFIER_HARDENING_REPORT.md.)
  - ``candidate_engine_bug``     — bulk geometric shift NOT explained by font-metric,
                                   element-position, clipping or oracle-side artifact.
                                   Conservative flag: marks the case for follow-up engine
                                   review; never a confirmed engine bug on its own.
                                   (QF4-F — placeholder for future engine-suspect signals.)
  - ``mixed_rendering``          — combination of the above; no dominant subcategory.

Heuristics are best-effort signals over the per-page panel PNGs already written
by OV2-01 (``*_ours.png``, ``*_oracle.png``). The classifier does NOT call any
paid API and does NOT generate new oracles.

Inputs:
  - One or more OV2-01 SSIM JSON result files (``*_ssim.json``).
  - Optionally the original PDF (probed for ``/AcroForm`` / ``/XFA`` entries).

Outputs:
  - Per-doc JSON conforming to ``scripts/xfa_oracle_fault_classifier_schema.json``.
  - Aggregate per-category JSON lists in the supplied output directory.

Usage:
  python3 scripts/xfa_oracle_fault_classifier.py \
      --ssim-json benchmarks/runs/.../sprint2/heatmaps_demo/*_ssim.json \
      --out-dir   benchmarks/runs/.../sprint2/fault_classification/ \
      --apply
"""

from __future__ import annotations

import argparse
import glob
import hashlib
import json
import os
import sys
from pathlib import Path
from typing import Any

try:
    import numpy as np
    from PIL import Image
except ImportError as e:  # pragma: no cover - depends on env
    print(f"ERROR: Missing Python dependency: {e}")
    print("Install with: pip install pillow numpy")
    sys.exit(1)


# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

HIGH_FIDELITY_THRESHOLD = 0.94   # OV2-02 threshold from Wave 2 plan.
ENGINE_BUG_SSIM_FLOOR = 0.80     # below this with no oracle-mismatch signal -> engine_bug.
BLANK_THRESHOLD_FRACTION = 0.985 # fraction of near-white pixels -> blank panel.
HIGH_DIFF_PIXEL_THRESHOLD = 32   # abs grayscale delta considered "significant" pixel.
TEXT_ANTIALIAS_DIFF_CEILING = 6.0  # mean_abs_diff <= this with high stroke density -> AA.
GEOMETRIC_SHIFT_FRACTION = 0.18  # fraction of high-diff pixels indicating bulk shift.

# Wave 1 Agent D — fine-category thresholds for oracle_mismatch_rendering subcategory.
#
# Font-metric signature: diff is distributed nearly uniformly across all text
# columns/rows (low column-mean variance); blank fraction of ours is LOWER than
# oracle (ours renders denser); mean_abs_diff is moderate (10–18).
FONT_METRIC_BLANK_GAP = 0.02     # oracle_blank - ours_blank >= this -> font-metric gap.
FONT_METRIC_MEAN_ABS_LOW = 8.0   # mean_abs_diff range low end.
FONT_METRIC_MEAN_ABS_HIGH = 22.0 # mean_abs_diff range high end.
FONT_METRIC_COL_VAR_CEILING = 0.35  # normalised column-mean std <= this -> uniform diff.
# Clipping signature: high-diff pixels concentrated at row/column extremes (edges).
CLIPPING_EDGE_FRACTION = 0.30    # fraction of high-diff pixels in outer 10% of image.
# Element-position signature: localised dense diff block (not global); high column
# variance + isolated peak.
ELEMENT_POSITION_COL_VAR_FLOOR = 0.50
# Antialias-variant signature: only at stroke edges (handled by existing text_antialiasing
# band); mean_abs_diff very low.
ANTIALIAS_MEAN_ABS_CEILING = 8.0

# QF2-E — border_rendering_delta fine-category thresholds.
#
# Border-line signature: oracle renders table separator lines / vertical rules that
# are absent in our output.  Identified from gen-854_854654 corpus pattern (3 docs).
# Key signals (require panels): column-mean variance very high (specific hot columns)
# AND oracle is DARKER than ours (blank_gap <= 0) AND mean_abs_diff is low (< 10).
# Stats-only fallback: blank_gap <= BORDER_BLANK_GAP_STATS_FLOOR AND mean_abs_diff
# below BORDER_MEAN_ABS_STATS_CEILING (when panels are unavailable).
BORDER_COL_VAR_FLOOR = 1.5         # normalised column-mean std >= this -> hot columns.
BORDER_COL_MAX_FLOOR = 50.0        # max column mean >= this -> extreme hotspot column.
BORDER_BLANK_GAP_CEILING = 0.01    # blank_gap <= this (oracle darker / adds content).
BORDER_MEAN_ABS_CEILING = 10.0     # mean_abs_diff < this (globally low but spiked cols).
# Stats-only border fallback thresholds (less precise, applied when no panels):
BORDER_BLANK_GAP_STATS_FLOOR = -0.005   # blank_gap <= this in stats-only mode.
BORDER_MEAN_ABS_STATS_CEILING = 8.0     # mean_abs_diff < this in stats-only mode.

# QF4-F — oracle_artifact thresholds (generalises border_rendering_delta).
#
# Oracle-side rendering artifact: oracle adds visible content the engine does not
# (negative blank_gap), but the spatial signature is broader than the strict
# border_rendering_delta hot-column pattern. Examples: anti-aliased fills, wider
# rasterised stroke widths, oracle-specific decoration. Applied AFTER border check
# so border cases keep their existing label.
ORACLE_ARTIFACT_BLANK_GAP_CEILING = -0.01   # oracle clearly darker (more strict than border stats floor).
ORACLE_ARTIFACT_MEAN_ABS_LOW = 3.0          # mean_abs window low end.
ORACLE_ARTIFACT_MEAN_ABS_HIGH = 30.0        # mean_abs window high end (broad).
ORACLE_ARTIFACT_COL_VAR_CEILING = 1.5       # col_var_norm < BORDER_COL_VAR_FLOOR to avoid border overlap.

# QF4-F — candidate_engine_bug thresholds (conservative engine-suspect signal).
#
# Marks geometric-shift cases that are NOT explained by any oracle-side signal
# (font_metric, element_position, clipping, oracle_artifact, border) for follow-up
# engine review. Never a confirmed engine bug on its own; the top-level
# ``engine_bug`` category remains the strong verdict path. This fine_category is a
# triage placeholder for cases that warrant manual engine-side investigation.
CANDIDATE_ENGINE_BUG_MEAN_ABS_FLOOR = 10.0  # mean_abs >= this to consider engine-suspect.

FINE_CATEGORIES = (
    "antialias_variant",
    "font_metric",
    "element_position",
    "clipping",
    "border_rendering_delta",
    "oracle_artifact",
    "candidate_engine_bug",
    "mixed_rendering",
)

CATEGORIES = (
    "engine_bug",
    "oracle_mismatch_acroform_xfa",
    "oracle_mismatch_rendering",
    "ambiguous",
    "both_fail",
    "high_fidelity_no_action",
)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def probe_pdf_form_entries(pdf_path: Path) -> tuple[bool, bool]:
    """Return (has_acroform_entry, has_xfa_entry) by scanning the PDF head."""
    if not pdf_path.exists():
        return (False, False)
    try:
        # Scan first ~2 MB — sufficient for catalog and AcroForm dict.
        with open(pdf_path, "rb") as f:
            data = f.read(2_000_000)
    except OSError:
        return (False, False)
    return (b"/AcroForm" in data, b"/XFA" in data)


def detect_oracle_source_kind(oracle_path: str) -> str:
    """Classify oracle path provenance based on filename/path conventions.

    Handles both local filesystem paths and VPS-prefixed logical paths
    (``oracle:pdfrest/...``, ``vps:t2-visual-140/...``).
    """
    if not oracle_path:
        return "missing"
    p = oracle_path.lower()
    # Logical VPS paths used in T2 SSIM JSONs (oracle:pdfrest/...).
    # These are always pdfRest-rendered XFA pages (not AcroForm flattened).
    if p.startswith("oracle:pdfrest/") or p.startswith("oracle:pdfrest_xfa"):
        if "pdfrest_flat" in p or "xfa-golden" in p:
            return "vps_xfa_golden"
        return "pdfrest_xfa"
    if p.startswith("oracle:"):
        # Generic oracle: prefix without pdfrest sub-path.
        return "unknown_external"
    # Local filesystem paths.
    if not Path(oracle_path).exists():
        return "missing"
    if "pdfrest_flat" in p or "xfa-golden" in p:
        return "vps_xfa_golden"
    if "pdfrest" in p and "xfa-forms" in p:
        return "vps_xfa_forms"
    if "pdfrest" in p:
        return "pdfrest_xfa"
    if "xfa_ov2_oracles" in p:
        # Fetched-from-VPS staging area; treat as vps_xfa_forms by default.
        return "vps_xfa_forms"
    return "unknown_external"


def load_gray_panel(panel_path: str) -> np.ndarray | None:
    """Load a panel PNG as grayscale uint8 array."""
    if not panel_path or not Path(panel_path).exists():
        return None
    try:
        img = Image.open(panel_path).convert("L")
        return np.array(img, dtype=np.uint8)
    except (OSError, ValueError):
        return None


def blank_fraction(gray: np.ndarray) -> float:
    """Fraction of pixels that are near-white (>= 250)."""
    if gray.size == 0:
        return 1.0
    return float((gray >= 250).sum()) / float(gray.size)


def diff_band_correlation(
    diff: np.ndarray, ours_gray: np.ndarray, oracle_gray: np.ndarray
) -> str:
    """Classify the spatial pattern of pixel difference.

    Strategy (heuristic):
      * Compute fraction of pixels with abs diff >= ``HIGH_DIFF_PIXEL_THRESHOLD``.
      * Identify thin-stroke (text-like) pixels using a local-contrast proxy on
        the oracle panel: pixels much darker than their row median.
      * If high-diff pixels are dominated by thin-stroke positions -> text_aa.
      * If high-diff pixels form large contiguous blobs (column means with broad
        elevated bands) -> geometric_shift.
      * Otherwise mixed / low_signal.
    """
    h, w = diff.shape
    if h == 0 or w == 0:
        return "low_signal"

    high_mask = diff >= HIGH_DIFF_PIXEL_THRESHOLD
    high_frac = float(high_mask.sum()) / float(diff.size)

    # Text-stroke proxy on the oracle: pixels >= 60 darker than row median.
    row_median = np.median(oracle_gray, axis=1, keepdims=True).astype(np.int32)
    stroke_mask = (row_median - oracle_gray.astype(np.int32)) >= 60
    stroke_frac = float(stroke_mask.sum()) / float(stroke_mask.size)

    if high_frac < 0.01:
        return "low_signal"

    if stroke_frac > 0.01:
        overlap = float(np.logical_and(high_mask, stroke_mask).sum())
        text_share = overlap / max(1.0, float(high_mask.sum()))
    else:
        text_share = 0.0

    # Detect large-band horizontal/vertical offsets via column-mean spikes.
    col_diff_mean = diff.mean(axis=0)
    band_threshold = max(8.0, col_diff_mean.mean() * 2.0)
    band_pixels = float((col_diff_mean >= band_threshold).sum()) / float(w)

    if high_frac >= GEOMETRIC_SHIFT_FRACTION and band_pixels >= 0.25:
        return "geometric_shift"
    if text_share >= 0.55 and high_frac < GEOMETRIC_SHIFT_FRACTION:
        return "text_antialiasing"
    if text_share >= 0.35 and high_frac < 0.30:
        return "mixed"
    if high_frac >= GEOMETRIC_SHIFT_FRACTION:
        return "geometric_shift"
    return "mixed"


def compute_diff_stats(
    ours_path: str, oracle_path: str
) -> dict[str, Any] | None:
    """Compute heuristic diff statistics from rendered PNG panels.

    Returns None when panels are not available (e.g. dry-run input).
    """
    ours_gray = load_gray_panel(ours_path)
    oracle_gray = load_gray_panel(oracle_path)
    if ours_gray is None or oracle_gray is None:
        return None

    # Resize oracle to ours dimensions if needed.
    if ours_gray.shape != oracle_gray.shape:
        target_h, target_w = ours_gray.shape
        oracle_img = Image.fromarray(oracle_gray).resize(
            (target_w, target_h), Image.LANCZOS
        )
        oracle_gray = np.array(oracle_img, dtype=np.uint8)

    diff = np.abs(ours_gray.astype(np.int32) - oracle_gray.astype(np.int32)).astype(
        np.uint16
    )

    mean_abs_diff = float(diff.mean())
    high_diff_pixel_fraction = float(
        (diff >= HIGH_DIFF_PIXEL_THRESHOLD).sum()
    ) / float(diff.size)
    band = diff_band_correlation(diff, ours_gray, oracle_gray)

    return {
        "mean_abs_diff": round(mean_abs_diff, 4),
        "high_diff_pixel_fraction": round(high_diff_pixel_fraction, 6),
        "diff_band_correlation": band,
        "image_blank_fraction_ours": round(blank_fraction(ours_gray), 6),
        "image_blank_fraction_oracle": round(blank_fraction(oracle_gray), 6),
    }


def compute_fine_category(
    ours_path: str,
    oracle_path: str,
    diff_stats: dict[str, Any] | None,
    band: str,
) -> tuple[str, str]:
    """Return (fine_category, fine_rationale) for an oracle_mismatch_rendering case.

    Wave 1 Agent D extension: produces a finer subcategory classification
    (font_metric / element_position / clipping / antialias_variant / mixed_rendering).

    QF2-E extension: adds border_rendering_delta subcategory.

    This function performs additional image analysis beyond what diff_band_correlation
    captures: column-mean variance (uniformity proxy), edge-concentration analysis,
    and hot-column detection for border lines.
    When panel images are unavailable it falls back to diff_stats signals alone.
    """
    mean_abs = diff_stats.get("mean_abs_diff", 0.0) if diff_stats else 0.0
    blank_ours = diff_stats.get("image_blank_fraction_ours", 0.0) if diff_stats else 0.0
    blank_oracle = diff_stats.get("image_blank_fraction_oracle", 0.0) if diff_stats else 0.0
    blank_gap = blank_oracle - blank_ours  # positive -> oracle renders more white (sparser)

    # Fast path: existing band already resolved to text_antialiasing.
    if band == "text_antialiasing":
        if mean_abs <= ANTIALIAS_MEAN_ABS_CEILING:
            return (
                "antialias_variant",
                (
                    f"Band is text_antialiasing and mean_abs_diff={mean_abs:.2f} is very low; "
                    "pure sub-pixel AA difference at stroke edges."
                ),
            )
        return (
            "mixed_rendering",
            (
                f"Band is text_antialiasing but mean_abs_diff={mean_abs:.2f} is elevated; "
                "AA + secondary signal present."
            ),
        )

    # Stats-only fast path: border_rendering_delta when panels are unavailable.
    # Applied before loading panels to short-circuit on unambiguous cases.
    # (Full panel-based detection below is more precise.)
    ours_gray = load_gray_panel(ours_path)
    oracle_gray = load_gray_panel(oracle_path)

    if ours_gray is None or oracle_gray is None:
        # Stats-only fallback: detect border_rendering_delta from diff_stats.
        if (
            diff_stats is not None
            and blank_gap <= BORDER_BLANK_GAP_STATS_FLOOR
            and mean_abs < BORDER_MEAN_ABS_STATS_CEILING
        ):
            return (
                "border_rendering_delta",
                (
                    f"Stats-only: blank_gap={blank_gap:.4f} <= {BORDER_BLANK_GAP_STATS_FLOOR} "
                    f"(oracle darker than ours) and mean_abs_diff={mean_abs:.2f} < "
                    f"{BORDER_MEAN_ABS_STATS_CEILING}; pattern matches border/rule lines "
                    "present in oracle but absent in our output. "
                    "(QF2-E — gen-854_854654 pattern; see QF2_E_VISUAL_DEEPENING_REPORT.md)"
                ),
            )

    col_var_norm: float | None = None
    edge_high_frac: float | None = None
    col_max: float | None = None

    if ours_gray is not None and oracle_gray is not None:
        if ours_gray.shape != oracle_gray.shape:
            target_h, target_w = ours_gray.shape
            oracle_img = Image.fromarray(oracle_gray).resize(
                (target_w, target_h), Image.LANCZOS
            )
            oracle_gray = np.array(oracle_img, dtype=np.uint8)

        diff_arr = np.abs(
            ours_gray.astype(np.int32) - oracle_gray.astype(np.int32)
        ).astype(np.float32)

        h, w = diff_arr.shape

        # Column-mean uniformity: low variance -> diff spread uniformly (font-metric).
        col_means = diff_arr.mean(axis=0)
        col_mean_global = float(col_means.mean()) if col_means.size > 0 else 1.0
        col_max = float(col_means.max()) if col_means.size > 0 else 0.0
        if col_mean_global > 0:
            col_var_norm = float(col_means.std()) / col_mean_global
        else:
            col_var_norm = 0.0

        # Edge concentration: fraction of high-diff pixels in outer 10% rows/cols.
        if h > 10 and w > 10:
            edge_rows = max(1, h // 10)
            edge_cols = max(1, w // 10)
            high_mask = (diff_arr >= HIGH_DIFF_PIXEL_THRESHOLD)
            total_high = float(high_mask.sum())
            if total_high > 0:
                edge_mask = np.zeros_like(high_mask)
                edge_mask[:edge_rows, :] = True
                edge_mask[-edge_rows:, :] = True
                edge_mask[:, :edge_cols] = True
                edge_mask[:, -edge_cols:] = True
                edge_high_frac = float(np.logical_and(high_mask, edge_mask).sum()) / total_high
            else:
                edge_high_frac = 0.0

    # --- Decision tree ---

    # QF2-E: border_rendering_delta — oracle renders border/rule lines absent in ours.
    # Requires panels; signals: very high col_var_norm (hot columns), oracle darker,
    # mean_abs globally low.  Takes priority over clipping to avoid misclassification.
    if (
        col_var_norm is not None
        and col_var_norm >= BORDER_COL_VAR_FLOOR
        and col_max is not None
        and col_max >= BORDER_COL_MAX_FLOOR
        and blank_gap <= BORDER_BLANK_GAP_CEILING
        and mean_abs < BORDER_MEAN_ABS_CEILING
    ):
        return (
            "border_rendering_delta",
            (
                f"col_var_norm={col_var_norm:.3f} >= {BORDER_COL_VAR_FLOOR} and "
                f"col_max={col_max:.1f} >= {BORDER_COL_MAX_FLOOR}; "
                f"blank_gap={blank_gap:.4f} <= {BORDER_BLANK_GAP_CEILING} (oracle darker); "
                f"mean_abs_diff={mean_abs:.2f} < {BORDER_MEAN_ABS_CEILING}. "
                "Oracle renders table separator / border lines absent in our output. "
                "(QF2-E — gen-854_854654 pattern; see QF2_E_VISUAL_DEEPENING_REPORT.md)"
            ),
        )

    # Clipping: diff concentrated at image edges.
    if edge_high_frac is not None and edge_high_frac >= CLIPPING_EDGE_FRACTION:
        return (
            "clipping",
            (
                f"Edge-concentration of high-diff pixels: {edge_high_frac:.2%} of high-diff "
                f"pixels are in outer 10% margin (threshold {CLIPPING_EDGE_FRACTION:.0%}). "
                "Diff pattern consistent with a clipping boundary or page-margin offset."
            ),
        )

    # Element-position: high column-mean variance -> localised block.
    if (
        col_var_norm is not None
        and col_var_norm >= ELEMENT_POSITION_COL_VAR_FLOOR
        and band == "geometric_shift"
    ):
        return (
            "element_position",
            (
                f"Column-mean normalised std={col_var_norm:.3f} >= "
                f"{ELEMENT_POSITION_COL_VAR_FLOOR} and band=geometric_shift; "
                "diff is localised to a specific element or region (position offset)."
            ),
        )

    # Font-metric: uniform diff (low col_var_norm) + moderate mean_abs + oracle renders
    # more white (blank_gap positive, meaning oracle font is sparser/larger).
    font_metric_mean_ok = FONT_METRIC_MEAN_ABS_LOW <= mean_abs <= FONT_METRIC_MEAN_ABS_HIGH
    font_metric_uniform = col_var_norm is not None and col_var_norm <= FONT_METRIC_COL_VAR_CEILING
    font_metric_blank_gap_ok = blank_gap >= FONT_METRIC_BLANK_GAP

    if font_metric_mean_ok and font_metric_blank_gap_ok:
        confidence_detail = ""
        if font_metric_uniform:
            confidence_detail = f"; col_var_norm={col_var_norm:.3f} confirms uniform spread"
        return (
            "font_metric",
            (
                f"Systematic font-metric difference: mean_abs_diff={mean_abs:.2f} (range "
                f"{FONT_METRIC_MEAN_ABS_LOW}–{FONT_METRIC_MEAN_ABS_HIGH}), "
                f"blank_gap={blank_gap:.4f} >= {FONT_METRIC_BLANK_GAP} (oracle renders "
                f"sparser/larger than ours){confidence_detail}. "
                "Root cause: font size, line-height, or tracking difference between "
                "PDFluent and pdfRest renderer. Not an engine layout bug."
            ),
        )

    # Font-metric without blank-gap signal (same magnitude, no density difference).
    if font_metric_mean_ok and font_metric_uniform:
        return (
            "font_metric",
            (
                f"Uniform diff spread (col_var_norm={col_var_norm:.3f} <= "
                f"{FONT_METRIC_COL_VAR_CEILING}) with mean_abs_diff={mean_abs:.2f}; "
                "likely font-metric difference (size/tracking) without significant "
                "density gap. Oracle-gap rather than engine bug."
            ),
        )

    # QF4-F: oracle_artifact — generalised oracle-side artifact.
    # Oracle is clearly darker (blank_gap very negative) but signature does NOT match
    # the strict border_rendering_delta hot-column pattern. Examples: anti-aliased
    # fills, wider rasterised strokes, oracle-side decoration. Placed AFTER border to
    # preserve existing border classifications.
    oracle_artifact_blank_gap_ok = blank_gap <= ORACLE_ARTIFACT_BLANK_GAP_CEILING
    oracle_artifact_mean_ok = (
        ORACLE_ARTIFACT_MEAN_ABS_LOW <= mean_abs <= ORACLE_ARTIFACT_MEAN_ABS_HIGH
    )
    oracle_artifact_not_border = (
        col_var_norm is None or col_var_norm < ORACLE_ARTIFACT_COL_VAR_CEILING
    )
    if (
        oracle_artifact_blank_gap_ok
        and oracle_artifact_mean_ok
        and oracle_artifact_not_border
    ):
        col_var_detail = (
            f"col_var_norm={col_var_norm:.3f}" if col_var_norm is not None else "col_var_norm=n/a"
        )
        return (
            "oracle_artifact",
            (
                f"Oracle adds visible content not present in our output: "
                f"blank_gap={blank_gap:.4f} <= {ORACLE_ARTIFACT_BLANK_GAP_CEILING} "
                f"(oracle darker); mean_abs_diff={mean_abs:.2f} in "
                f"[{ORACLE_ARTIFACT_MEAN_ABS_LOW}, {ORACLE_ARTIFACT_MEAN_ABS_HIGH}]; "
                f"{col_var_detail} below border-hot-column threshold "
                f"{ORACLE_ARTIFACT_COL_VAR_CEILING}. "
                "Generalised oracle-side rendering artifact (not border-specific). "
                "(QF4-F — formal generalisation of border_rendering_delta.)"
            ),
        )

    # QF4-F: candidate_engine_bug — conservative engine-suspect flag.
    # Bulk geometric shift NOT explained by oracle-side signals (font_metric, element_
    # position, clipping, oracle_artifact, border). Mean_abs is substantial. This is
    # a triage placeholder; the top-level ``engine_bug`` category remains the strong
    # verdict path for confirmed engine bugs.
    if (
        band == "geometric_shift"
        and mean_abs >= CANDIDATE_ENGINE_BUG_MEAN_ABS_FLOOR
        and not font_metric_blank_gap_ok
    ):
        col_var_detail = (
            f"col_var_norm={col_var_norm:.3f}" if col_var_norm is not None else "col_var_norm=n/a"
        )
        edge_detail = (
            f"edge_high_frac={edge_high_frac:.3f}" if edge_high_frac is not None else "edge_high_frac=n/a"
        )
        return (
            "candidate_engine_bug",
            (
                f"Bulk geometric shift with mean_abs_diff={mean_abs:.2f} >= "
                f"{CANDIDATE_ENGINE_BUG_MEAN_ABS_FLOOR}; no oracle-side signal matched "
                f"(blank_gap={blank_gap:.4f}, {col_var_detail}, {edge_detail}). "
                "Conservative engine-suspect flag for manual follow-up; NOT a confirmed "
                "engine bug. (QF4-F — placeholder for future engine-suspect signals.)"
            ),
        )

    # Fallback: mixed rendering.
    detail_parts = []
    if col_var_norm is not None:
        detail_parts.append(f"col_var_norm={col_var_norm:.3f}")
    if edge_high_frac is not None:
        detail_parts.append(f"edge_high_frac={edge_high_frac:.3f}")
    detail_parts.append(f"blank_gap={blank_gap:.4f}")
    detail_parts.append(f"mean_abs={mean_abs:.2f}")
    return (
        "mixed_rendering",
        (
            "No dominant fine-category signal; signals: "
            + ", ".join(detail_parts)
            + ". Classified as mixed_rendering oracle-gap."
        ),
    )


# ---------------------------------------------------------------------------
# Classifier
# ---------------------------------------------------------------------------


def classify(
    ssim_mean: float,
    pdf_has_acroform: bool,
    pdf_has_xfa: bool,
    oracle_source_kind: str,
    diff_stats: dict[str, Any] | None,
) -> tuple[str, str, str, bool]:
    """Return (category, confidence, rationale, manual_review_recommended).

    Note: fine_category subcategory for oracle_mismatch_rendering is computed
    separately via compute_fine_category() and merged in classify_one().
    """

    # Both-fail check: near-blank on BOTH sides AND SSIM also low (rendering
    # truly failed). High SSIM means the two blank pages still agree — that
    # is high-fidelity, not both-fail.
    if (
        diff_stats
        and diff_stats["image_blank_fraction_ours"] >= BLANK_THRESHOLD_FRACTION
        and diff_stats["image_blank_fraction_oracle"] >= BLANK_THRESHOLD_FRACTION
        and ssim_mean < HIGH_FIDELITY_THRESHOLD
    ):
        return (
            "both_fail",
            "high",
            "Both ours and oracle panels are near-blank with low SSIM; rendering failed on both sides.",
            True,
        )

    # High-fidelity short-circuit.
    if ssim_mean >= HIGH_FIDELITY_THRESHOLD:
        return (
            "high_fidelity_no_action",
            "high",
            f"SSIM {ssim_mean:.4f} >= {HIGH_FIDELITY_THRESHOLD} (Wave 2 threshold); no investigation needed.",
            False,
        )

    # If panel data unavailable, decide on SSIM + oracle-source heuristics alone.
    if diff_stats is None:
        if oracle_source_kind == "vps_xfa_golden" and pdf_has_acroform and pdf_has_xfa:
            return (
                "oracle_mismatch_acroform_xfa",
                "medium",
                (
                    "Oracle path indicates pdfrest_flat (AcroForm flattening) and PDF "
                    "carries both /AcroForm and /XFA entries; without panel images this "
                    "is a likely oracle-source mismatch."
                ),
                True,
            )
        return (
            "ambiguous",
            "low",
            "No panel PNGs available for diff analysis; SSIM alone is insufficient.",
            True,
        )

    band = diff_stats["diff_band_correlation"]
    mean_abs = diff_stats["mean_abs_diff"]
    high_frac = diff_stats["high_diff_pixel_fraction"]

    # Primary signal: AcroForm/XFA oracle-source mismatch.
    # 60df78fe-like case: oracle from xfa-golden (flattened) AND PDF has both
    # /AcroForm and /XFA entries AND there is a substantial bulk diff.
    if (
        oracle_source_kind == "vps_xfa_golden"
        and pdf_has_acroform
        and pdf_has_xfa
        and (band == "geometric_shift" or high_frac >= 0.10)
    ):
        return (
            "oracle_mismatch_acroform_xfa",
            "high",
            (
                "Oracle is pdfrest_flat (AcroForm flattening path), PDF has both "
                "/AcroForm and /XFA entries, diff is "
                f"'{band}' with high_diff_pixel_fraction={high_frac:.3f}, "
                f"mean_abs_diff={mean_abs:.2f}. This is the canonical AcroForm-vs-XFA "
                "oracle-source mismatch (60df78fe pattern)."
            ),
            False,
        )

    if (
        oracle_source_kind == "vps_xfa_golden"
        and pdf_has_acroform
        and pdf_has_xfa
    ):
        return (
            "oracle_mismatch_acroform_xfa",
            "medium",
            (
                "Oracle path indicates pdfrest_flat (AcroForm flattening) and PDF "
                "carries both /AcroForm and /XFA; diff pattern is "
                f"'{band}' with smaller magnitude, so confidence is medium."
            ),
            True,
        )

    # Text antialiasing pattern + moderate SSIM -> rendering artefact.
    if band == "text_antialiasing" and ssim_mean >= 0.85:
        return (
            "oracle_mismatch_rendering",
            "high",
            (
                f"Diff pattern is text-antialiasing (mean_abs={mean_abs:.2f}, "
                f"high_diff_pixel_fraction={high_frac:.3f}); SSIM {ssim_mean:.4f} "
                "is consistent with sub-pixel font rasterization differences "
                "between renderers."
            ),
            False,
        )

    if band == "text_antialiasing" and ssim_mean < 0.85:
        return (
            "oracle_mismatch_rendering",
            "medium",
            (
                f"Diff pattern is text-antialiasing but SSIM {ssim_mean:.4f} is "
                "relatively low; rendering artefact most likely but engine bug "
                "in layout cannot be excluded without manual review."
            ),
            True,
        )

    # Mixed pattern -> recommend manual review.
    if band == "mixed" and ssim_mean >= 0.85:
        return (
            "oracle_mismatch_rendering",
            "medium",
            (
                f"Mixed text + minor geometric diff (mean_abs={mean_abs:.2f}); "
                f"SSIM {ssim_mean:.4f}. Most likely renderer artefact; flag for "
                "sample inspection."
            ),
            True,
        )

    # Mixed pattern below 0.85 threshold with moderate uniform diff:
    # Wave 1 Agent D extended rule — font-metric signal takes precedence over
    # generic ambiguous when blank_gap and mean_abs_diff are in the font-metric
    # range. These cases are NOT engine bugs; they are systematic font-rendering
    # differences between PDFluent and the pdfRest oracle renderer.
    if (
        band == "mixed"
        and FONT_METRIC_MEAN_ABS_LOW <= mean_abs <= FONT_METRIC_MEAN_ABS_HIGH
        and diff_stats is not None
        and (diff_stats.get("image_blank_fraction_oracle", 0.0)
             - diff_stats.get("image_blank_fraction_ours", 0.0)) >= FONT_METRIC_BLANK_GAP
    ):
        return (
            "oracle_mismatch_rendering",
            "medium",
            (
                f"Mixed diff with font-metric signature: mean_abs_diff={mean_abs:.2f}, "
                f"blank_gap={diff_stats['image_blank_fraction_oracle'] - diff_stats['image_blank_fraction_ours']:.4f} "
                f">= {FONT_METRIC_BLANK_GAP}, SSIM {ssim_mean:.4f}. "
                "Oracle renders sparser/larger than ours; root cause is font size / "
                "line-height / tracking difference. Fine-category: font_metric."
            ),
            True,
        )

    # Geometric shift on a non-flattened oracle -> engine bug.
    if band == "geometric_shift":
        return (
            "engine_bug",
            "medium",
            (
                f"Diff is a bulk geometric shift (high_diff_pixel_fraction={high_frac:.3f}) "
                "without an AcroForm-vs-XFA oracle-source indicator; likely engine "
                "layout/positioning bug."
            ),
            True,
        )

    # Low SSIM with low-signal diff is suspicious.
    if ssim_mean < ENGINE_BUG_SSIM_FLOOR:
        return (
            "engine_bug",
            "low",
            (
                f"SSIM {ssim_mean:.4f} below engine_bug floor {ENGINE_BUG_SSIM_FLOOR} "
                f"but diff pattern is '{band}'; manual review required."
            ),
            True,
        )

    return (
        "ambiguous",
        "low",
        (
            f"SSIM {ssim_mean:.4f}, band='{band}', high_diff_frac={high_frac:.3f}: "
            "heuristics inconclusive."
        ),
        True,
    )


# ---------------------------------------------------------------------------
# Pipeline
# ---------------------------------------------------------------------------


def classify_one(
    ssim_json_path: Path, doc_path_override: Path | None = None
) -> dict[str, Any]:
    """Classify a single OV2-01 SSIM JSON result."""
    with open(ssim_json_path) as f:
        data = json.load(f)

    doc_sha = data["doc_sha256"]
    doc_filename = data["doc_filename"]
    oracle_path = data.get("oracle_path", "")
    aggregate = data.get("aggregate", {})
    per_page = data.get("per_page", [])

    mean_ssim = aggregate.get("mean_ssim")
    min_ssim = aggregate.get("min_ssim", mean_ssim)
    max_ssim = aggregate.get("max_ssim", mean_ssim)
    page_count = aggregate.get("page_count", 0)

    # Resolve original PDF: prefer override, else best-effort search.
    pdf_path = doc_path_override
    if pdf_path is None:
        # Try common candidate locations.
        candidates = [
            Path("/tmp/xfa_ov2_oracles") / doc_filename,
            Path("crates/xfa-golden-tests/golden") / doc_filename,
        ]
        for c in candidates:
            if c.exists():
                pdf_path = c
                break

    has_acroform = has_xfa = False
    if pdf_path is not None and pdf_path.exists():
        has_acroform, has_xfa = probe_pdf_form_entries(pdf_path)

    oracle_kind = detect_oracle_source_kind(oracle_path)

    # Override: 60df78fe pdf_0012 oracle was sourced from VPS xfa-golden in OV2-01
    # demo (per SSIM_PIPELINE_SPRINT2_DEMO.md). The staged path /tmp/xfa_ov2_oracles
    # erases the original provenance, so reapply provenance for the demo doc.
    if oracle_kind == "vps_xfa_forms" and doc_filename.startswith("60df78fe"):
        oracle_kind = "vps_xfa_golden"

    # Pull first-page panel images (OV2-01 writes per-page images).
    diff_stats: dict[str, Any] | None = None
    if per_page:
        paths = per_page[0].get("image_paths", {})
        diff_stats = compute_diff_stats(paths.get("ours", ""), paths.get("oracle", ""))

    category, confidence, rationale, manual_review = classify(
        ssim_mean=float(mean_ssim) if mean_ssim is not None else 0.0,
        pdf_has_acroform=has_acroform,
        pdf_has_xfa=has_xfa,
        oracle_source_kind=oracle_kind,
        diff_stats=diff_stats,
    )

    # Wave 1 Agent D: compute fine_category for rendering-mismatch cases.
    fine_category: str | None = None
    fine_rationale: str | None = None
    if category == "oracle_mismatch_rendering" and per_page:
        paths = per_page[0].get("image_paths", {})
        band = diff_stats.get("diff_band_correlation", "mixed") if diff_stats else "mixed"
        fine_category, fine_rationale = compute_fine_category(
            ours_path=paths.get("ours", ""),
            oracle_path=paths.get("oracle", ""),
            diff_stats=diff_stats,
            band=band,
        )

    signals: dict[str, Any] = {
        "pdf_has_acroform_entry": has_acroform,
        "pdf_has_xfa_entry": has_xfa,
        "oracle_source_kind": oracle_kind,
    }
    if diff_stats is not None:
        signals["diff_stats"] = diff_stats

    classification_block: dict[str, Any] = {
        "category": category,
        "confidence": confidence,
        "rationale": rationale,
    }
    if fine_category is not None:
        classification_block["fine_category"] = fine_category
        classification_block["fine_rationale"] = fine_rationale

    return {
        "doc_sha256": doc_sha,
        "doc_filename": doc_filename,
        "oracle_path": oracle_path,
        "ssim_input": {
            "mean_ssim": mean_ssim if mean_ssim is not None else 0.0,
            "min_ssim": min_ssim if min_ssim is not None else 0.0,
            "max_ssim": max_ssim if max_ssim is not None else 0.0,
            "page_count": int(page_count),
        },
        "classification": classification_block,
        "heuristic_signals": signals,
        "manual_review_recommended": manual_review,
    }


def write_category_aggregates(
    results: list[dict[str, Any]], out_dir: Path
) -> dict[str, int]:
    """Write per-category JSON lists; return count distribution."""
    counts = {cat: 0 for cat in CATEGORIES}
    grouped: dict[str, list[dict[str, Any]]] = {cat: [] for cat in CATEGORIES}
    for r in results:
        cat = r["classification"]["category"]
        counts[cat] = counts.get(cat, 0) + 1
        grouped.setdefault(cat, []).append(
            {
                "doc_sha256": r["doc_sha256"],
                "doc_filename": r["doc_filename"],
                "ssim_mean": r["ssim_input"]["mean_ssim"],
                "confidence": r["classification"]["confidence"],
                "manual_review_recommended": r["manual_review_recommended"],
            }
        )

    out_dir.mkdir(parents=True, exist_ok=True)
    for cat in CATEGORIES:
        with open(out_dir / f"{cat}.json", "w") as f:
            json.dump(
                {
                    "category": cat,
                    "count": counts[cat],
                    "docs": grouped[cat],
                },
                f,
                indent=2,
            )
    return counts


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------


def main() -> None:
    parser = argparse.ArgumentParser(
        description=(
            "Classify XFA visual-fidelity cases by likely fault source. "
            "Consumes OV2-01 SSIM JSON results; zero paid API calls."
        )
    )
    parser.add_argument(
        "--ssim-json",
        nargs="+",
        required=True,
        metavar="PATH_OR_GLOB",
        help="One or more OV2-01 SSIM JSON files. Globs are supported.",
    )
    parser.add_argument(
        "--out-dir",
        required=True,
        metavar="DIR",
        help="Output directory for per-doc and per-category classification JSON.",
    )
    parser.add_argument(
        "--pdf-search-root",
        default=None,
        metavar="DIR",
        help=(
            "Optional directory to search for original PDFs by filename when "
            "probing /AcroForm / /XFA entries."
        ),
    )
    parser.add_argument(
        "--apply",
        action="store_true",
        default=False,
        help=(
            "Write per-doc and per-category JSON files. Without --apply, the "
            "script prints a dry-run summary to stdout and exits."
        ),
    )
    args = parser.parse_args()

    # Expand globs.
    inputs: list[Path] = []
    for pattern in args.ssim_json:
        matches = sorted(glob.glob(pattern))
        if not matches and Path(pattern).exists():
            matches = [pattern]
        for m in matches:
            inputs.append(Path(m))

    if not inputs:
        print("ERROR: No SSIM JSON inputs matched.", file=sys.stderr)
        sys.exit(2)

    pdf_search_root = Path(args.pdf_search_root) if args.pdf_search_root else None

    results: list[dict[str, Any]] = []
    for ssim_json in inputs:
        # Determine PDF override.
        pdf_override = None
        if pdf_search_root is not None:
            with open(ssim_json) as f:
                meta = json.load(f)
            candidate = pdf_search_root / meta["doc_filename"]
            if candidate.exists():
                pdf_override = candidate

        result = classify_one(ssim_json, doc_path_override=pdf_override)
        results.append(result)
        print(
            f"{result['doc_filename']:40s}  ssim={result['ssim_input']['mean_ssim']:.4f}  "
            f"-> {result['classification']['category']:32s}  "
            f"({result['classification']['confidence']})"
        )

    out_dir = Path(args.out_dir)
    if not args.apply:
        print()
        print(f"[dry-run] Would write {len(results)} per-doc JSONs + 6 category aggregates to {out_dir}/")
        return

    out_dir.mkdir(parents=True, exist_ok=True)
    for r in results:
        per_doc_path = out_dir / f"{Path(r['doc_filename']).stem}_classification.json"
        with open(per_doc_path, "w") as f:
            json.dump(r, f, indent=2)

    counts = write_category_aggregates(results, out_dir)
    print()
    print("Aggregate distribution:")
    total = sum(counts.values())
    for cat in CATEGORIES:
        pct = (counts[cat] / total * 100.0) if total else 0.0
        print(f"  {cat:34s}  {counts[cat]:4d}  ({pct:5.1f}%)")
    print(f"  {'TOTAL':34s}  {total:4d}")


if __name__ == "__main__":
    main()
