#!/usr/bin/env python3
"""EVH-GOLD-03: Calibrate SSIM and completeness thresholds against a labeled gold set.

Finds F1-optimal thresholds for the boundary between quality classes, using only
documents that have been human-labeled (human_label != null).

Requires at least 10 labeled documents; outputs a warning and placeholder
when fewer are available.

Usage:
    python3 scripts/calibrate_thresholds.py \
        --gold-set benchmarks/gold_set/gold_set.json \
        --output benchmarks/gold_set/CALIBRATED_THRESHOLDS.md \
        --json-output benchmarks/gold_set/calibrated_thresholds.json
"""

from __future__ import annotations

import argparse
import datetime
import json
import sys
from pathlib import Path
from typing import Optional

# ---------------------------------------------------------------------------
# Default (uncalibrated) thresholds — mirrors classify_document.py
# ---------------------------------------------------------------------------

DEFAULT_THRESHOLDS = {
    "visual_acceptable_pass": 0.94,
    "visual_acceptable_partial_lower": 0.85,
    "data_complete_pass": 0.90,
    "data_complete_partial_lower": 0.50,
}

MIN_LABELED_DOCS = 10

# ---------------------------------------------------------------------------
# F1 helper (stdlib only)
# ---------------------------------------------------------------------------

def f1(tp: int, fp: int, fn: int) -> float:
    precision = tp / (tp + fp) if (tp + fp) > 0 else 0.0
    recall = tp / (tp + fn) if (tp + fn) > 0 else 0.0
    return (
        2 * precision * recall / (precision + recall)
        if (precision + recall) > 0
        else 0.0
    )


# ---------------------------------------------------------------------------
# Range generator (avoids floating-point accumulation issues)
# ---------------------------------------------------------------------------

def frange(start: float, stop: float, step: float) -> list[float]:
    result = []
    n = round((stop - start) / step)
    for i in range(n + 1):
        result.append(round(start + i * step, 6))
    return result


# ---------------------------------------------------------------------------
# Loaders
# ---------------------------------------------------------------------------

def load_json(path: Path) -> dict | list:
    with open(path, "r", encoding="utf-8") as fh:
        return json.load(fh)


def save_json(data: dict, path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "w", encoding="utf-8") as fh:
        json.dump(data, fh, indent=2, ensure_ascii=False)
    print(f"Wrote: {path}")


def save_text(text: str, path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")
    print(f"Wrote: {path}")


# ---------------------------------------------------------------------------
# Extract values from gold set entries
# ---------------------------------------------------------------------------

def get_ssim(entry: dict) -> Optional[float]:
    val = entry.get("ssim")
    if val is None:
        return None
    try:
        return float(val)
    except (TypeError, ValueError):
        return None


def get_completeness(entry: dict) -> Optional[float]:
    val = entry.get("completeness")
    if val is None:
        return None
    try:
        return float(val)
    except (TypeError, ValueError):
        return None


# ---------------------------------------------------------------------------
# SSIM threshold calibration
# ---------------------------------------------------------------------------

def calibrate_ssim_boundary(
    labeled: list[dict],
    positive_labels: set[str],
    ssim_range: tuple[float, float, float],
) -> tuple[float, float]:
    """
    Find the SSIM threshold that maximizes F1 for the positive class.

    A document is predicted positive if ssim >= threshold.
    A document is ground-truth positive if human_label in positive_labels.

    Returns (best_threshold, best_f1).
    """
    # Only use docs where ssim is available
    usable = [e for e in labeled if get_ssim(e) is not None]
    if not usable:
        return ssim_range[0], 0.0

    best_threshold = ssim_range[0]
    best_f1_score = -1.0

    for threshold in frange(*ssim_range):
        tp = fp = fn = 0
        for entry in usable:
            ssim = get_ssim(entry)
            label = entry.get("human_label", "")
            predicted_positive = ssim >= threshold
            ground_truth_positive = label in positive_labels

            if predicted_positive and ground_truth_positive:
                tp += 1
            elif predicted_positive and not ground_truth_positive:
                fp += 1
            elif not predicted_positive and ground_truth_positive:
                fn += 1

        score = f1(tp, fp, fn)
        if score > best_f1_score:
            best_f1_score = score
            best_threshold = threshold

    return best_threshold, best_f1_score


# ---------------------------------------------------------------------------
# Completeness threshold calibration
# ---------------------------------------------------------------------------

def calibrate_completeness_boundary(
    labeled: list[dict],
    complete_labels: set[str],
) -> tuple[float, float]:
    """
    Find the completeness threshold that maximizes agreement with complete labels.

    A document is predicted complete if completeness >= threshold.
    Ground truth: human_label in complete_labels.

    Returns (best_threshold, best_f1).
    """
    usable = [e for e in labeled if get_completeness(e) is not None]
    if not usable:
        return DEFAULT_THRESHOLDS["data_complete_pass"], 0.0

    best_threshold = DEFAULT_THRESHOLDS["data_complete_pass"]
    best_f1_score = -1.0

    for threshold in frange(0.70, 0.99, 0.01):
        tp = fp = fn = 0
        for entry in usable:
            comp = get_completeness(entry)
            label = entry.get("human_label", "")
            predicted_complete = comp >= threshold
            ground_truth_complete = label in complete_labels

            if predicted_complete and ground_truth_complete:
                tp += 1
            elif predicted_complete and not ground_truth_complete:
                fp += 1
            elif not predicted_complete and ground_truth_complete:
                fn += 1

        score = f1(tp, fp, fn)
        if score > best_f1_score:
            best_f1_score = score
            best_threshold = threshold

    return best_threshold, best_f1_score


# ---------------------------------------------------------------------------
# Build calibration outputs
# ---------------------------------------------------------------------------

def build_not_calibrated(gold_set_size: int) -> tuple[dict, str]:
    thresholds_json = {
        "calibrated_on": datetime.date.today().isoformat(),
        "gold_set_size": gold_set_size,
        "status": "not_calibrated",
        "note": f"Calibration requires at least {MIN_LABELED_DOCS} labeled gold set documents.",
        "thresholds": DEFAULT_THRESHOLDS,
        "calibration_metrics": {},
    }

    md = f"""# Calibrated Thresholds

**Status**: Not calibrated

**Reason**: Calibration requires at least {MIN_LABELED_DOCS} labeled documents in the gold set.
Currently labeled: {gold_set_size}.

## Default Thresholds (Uncalibrated)

| Threshold | Value |
|-----------|-------|
| `visual_acceptable_pass` (SSIM fully_correct boundary) | {DEFAULT_THRESHOLDS['visual_acceptable_pass']} |
| `visual_acceptable_partial_lower` (SSIM minor/major boundary) | {DEFAULT_THRESHOLDS['visual_acceptable_partial_lower']} |
| `data_complete_pass` (completeness pass boundary) | {DEFAULT_THRESHOLDS['data_complete_pass']} |
| `data_complete_partial_lower` (completeness partial lower) | {DEFAULT_THRESHOLDS['data_complete_partial_lower']} |

## Next Steps

1. Label at least {MIN_LABELED_DOCS} documents in `benchmarks/gold_set/gold_set.json`
   following the protocol in `LABELING_GUIDELINES.md`.
2. Re-run this script to compute F1-optimal thresholds.
"""
    return thresholds_json, md


def build_calibrated(
    labeled: list[dict],
    vis_pass_threshold: float,
    vis_pass_f1: float,
    vis_partial_threshold: float,
    vis_partial_f1: float,
    comp_pass_threshold: float,
    comp_pass_f1: float,
) -> tuple[dict, str]:
    n = len(labeled)
    today = datetime.date.today().isoformat()

    thresholds_json = {
        "calibrated_on": today,
        "gold_set_size": n,
        "status": "calibrated",
        "thresholds": {
            "visual_acceptable_pass": vis_pass_threshold,
            "visual_acceptable_partial_lower": vis_partial_threshold,
            "data_complete_pass": comp_pass_threshold,
            "data_complete_partial_lower": DEFAULT_THRESHOLDS["data_complete_partial_lower"],
        },
        "calibration_metrics": {
            "visual_acceptable_pass_f1": round(vis_pass_f1, 4),
            "visual_acceptable_partial_f1": round(vis_partial_f1, 4),
            "data_complete_pass_f1": round(comp_pass_f1, 4),
        },
    }

    label_dist: dict[str, int] = {}
    for entry in labeled:
        lbl = entry.get("human_label") or "unknown"
        label_dist[lbl] = label_dist.get(lbl, 0) + 1

    label_rows = "\n".join(
        f"| `{lbl}` | {cnt} |" for lbl, cnt in sorted(label_dist.items())
    )

    md = f"""# Calibrated Thresholds

**Status**: Calibrated
**Calibrated on**: {today}
**Gold set size**: {n} labeled documents

## Calibrated Thresholds

| Threshold | Value | F1 |
|-----------|-------|----|
| `visual_acceptable_pass` (SSIM: fully_correct boundary) | {vis_pass_threshold} | {vis_pass_f1:.4f} |
| `visual_acceptable_partial_lower` (SSIM: minor/major boundary) | {vis_partial_threshold} | {vis_partial_f1:.4f} |
| `data_complete_pass` (completeness pass boundary) | {comp_pass_threshold} | {comp_pass_f1:.4f} |
| `data_complete_partial_lower` (completeness partial lower) | {DEFAULT_THRESHOLDS['data_complete_partial_lower']} | (fixed) |

## Label Distribution

| Label | Count |
|-------|-------|
{label_rows}

## Methodology

- **visual_acceptable_pass**: SSIM threshold maximizing F1 for `fully_correct` class
  (threshold range: 0.90–0.99, step 0.005)
- **visual_acceptable_partial_lower**: SSIM threshold maximizing F1 for
  `minor_deviation` class (range: 0.80–0.94, step 0.005)
- **data_complete_pass**: completeness threshold maximizing F1 for
  `fully_correct` + `minor_deviation` class (range: 0.70–0.99, step 0.01)
- `data_complete_partial_lower` is kept at the default (0.50) as it represents
  a structural boundary not amenable to F1 calibration without a dedicated
  completeness dimension label.

## Next Steps

Update `scripts/classify_document.py` to use the calibrated threshold values
once they have been reviewed and accepted.
"""
    return thresholds_json, md


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def calibrate(args: argparse.Namespace) -> None:
    gold_path = Path(args.gold_set)
    output_md = Path(args.output)
    output_json = Path(args.json_output)

    if not gold_path.exists():
        print(f"ERROR: gold set not found: {gold_path}", file=sys.stderr)
        sys.exit(1)

    raw = load_json(gold_path)
    if not isinstance(raw, dict):
        print("ERROR: gold set JSON must be an object.", file=sys.stderr)
        sys.exit(1)

    all_docs: list[dict] = raw.get("documents", [])
    labeled = [d for d in all_docs if d.get("human_label") is not None]

    print(f"Gold set: {len(all_docs)} total, {len(labeled)} labeled.")

    if len(labeled) < MIN_LABELED_DOCS:
        print(
            f"WARNING: only {len(labeled)} labeled documents "
            f"(minimum {MIN_LABELED_DOCS} required). "
            "Writing placeholder outputs."
        )
        thresholds_json, md_text = build_not_calibrated(len(labeled))
    else:
        print(f"Calibrating on {len(labeled)} labeled documents...")

        # --- Visual pass threshold: fully_correct boundary ---
        # Positive = human label is fully_correct
        vis_pass_threshold, vis_pass_f1 = calibrate_ssim_boundary(
            labeled,
            positive_labels={"fully_correct"},
            ssim_range=(0.90, 0.99, 0.005),
        )
        print(
            f"  visual_acceptable_pass: {vis_pass_threshold:.3f} "
            f"(F1={vis_pass_f1:.4f})"
        )

        # --- Visual partial-lower threshold: minor_deviation boundary ---
        # Positive = human label is minor_deviation (not fully_correct, not major)
        vis_partial_threshold, vis_partial_f1 = calibrate_ssim_boundary(
            labeled,
            positive_labels={"minor_deviation"},
            ssim_range=(0.80, 0.94, 0.005),
        )
        print(
            f"  visual_acceptable_partial_lower: {vis_partial_threshold:.3f} "
            f"(F1={vis_partial_f1:.4f})"
        )

        # --- Completeness pass threshold ---
        # "data complete" = fully_correct or minor_deviation (both have complete data)
        comp_pass_threshold, comp_pass_f1 = calibrate_completeness_boundary(
            labeled,
            complete_labels={"fully_correct", "minor_deviation"},
        )
        print(
            f"  data_complete_pass: {comp_pass_threshold:.3f} "
            f"(F1={comp_pass_f1:.4f})"
        )

        thresholds_json, md_text = build_calibrated(
            labeled,
            vis_pass_threshold,
            vis_pass_f1,
            vis_partial_threshold,
            vis_partial_f1,
            comp_pass_threshold,
            comp_pass_f1,
        )

    save_json(thresholds_json, output_json)
    save_text(md_text, output_md)


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Calibrate SSIM and completeness thresholds against a labeled gold set."
    )
    parser.add_argument(
        "--gold-set",
        metavar="FILE",
        default="benchmarks/gold_set/gold_set.json",
        help="Path to gold_set.json (default: benchmarks/gold_set/gold_set.json).",
    )
    parser.add_argument(
        "--output",
        metavar="FILE",
        default="benchmarks/gold_set/CALIBRATED_THRESHOLDS.md",
        help="Output path for human-readable calibration report "
             "(default: benchmarks/gold_set/CALIBRATED_THRESHOLDS.md).",
    )
    parser.add_argument(
        "--json-output",
        metavar="FILE",
        default="benchmarks/gold_set/calibrated_thresholds.json",
        help="Output path for machine-readable calibrated thresholds JSON "
             "(default: benchmarks/gold_set/calibrated_thresholds.json).",
    )
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    calibrate(args)


if __name__ == "__main__":
    main()
