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
    """Classify oracle path provenance based on filename/path conventions."""
    p = oracle_path.lower()
    if not oracle_path or not Path(oracle_path).exists():
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
    """Return (category, confidence, rationale, manual_review_recommended)."""

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

    signals: dict[str, Any] = {
        "pdf_has_acroform_entry": has_acroform,
        "pdf_has_xfa_entry": has_xfa,
        "oracle_source_kind": oracle_kind,
    }
    if diff_stats is not None:
        signals["diff_stats"] = diff_stats

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
        "classification": {
            "category": category,
            "confidence": confidence,
            "rationale": rationale,
        },
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
