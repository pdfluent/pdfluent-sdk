#!/usr/bin/env python3
"""
classify_failure.py — Assign taxonomy codes to benchmark results.

Importable as module:
    from classify_failure import classify_failure, assign_all_failures

CLI:
    python3 scripts/classify_failure.py \
        --results benchmarks/enterprise-baseline-2026-04-19.json \
        --output benchmarks/enterprise-baseline-2026-04-19-classified.json
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

# Priority ordering for primary_code selection
PRIORITY = [
    "CRASH-001", "CRASH-002", "CRASH-003",
    "ORACLE-001", "FLAT-001",
    "BIND-001", "BIND-002",
    "LAYOUT-001", "LAYOUT-002",
    "RENDER-001", "RENDER-002", "RENDER-003",
    "BIND-003", "PERF-001",
    "ORACLE-002", "LAYOUT-003",
    "UNKNOWN",
]


def classify_failure(result: dict) -> tuple[list[str], str, str]:
    """
    Classify a single benchmark result according to the failure taxonomy.

    Parameters
    ----------
    result : dict
        A single document result dict from benchmark JSON.

    Returns
    -------
    codes : list[str]
        All applicable taxonomy codes.
    primary : str
        The highest-priority code, used as the single representative label.
    confidence : str
        One of "high", "medium", or "low".
    """
    codes: list[str] = []
    exit_code = result.get("exit_code", 1)
    status = result.get("status", "crash")

    # CRASH codes
    if exit_code == 1:
        codes.append("CRASH-001")
    elif exit_code == 4:
        codes.append("CRASH-002")

    # Oracle codes
    if result.get("oracle_fault") is True:
        codes.append("ORACLE-001")

    # FLAT codes
    if result.get("flatten_clean") is False:
        codes.append("FLAT-001")

    # BIND codes
    dims = result.get("dimensions", {})
    if dims.get("data_complete") == "fail":
        codes.append("BIND-001")
        if result.get("metadata", {}).get("has_formcalc"):
            codes.append("BIND-003")
    elif dims.get("data_complete") == "partial":
        codes.append("BIND-002")

    # LAYOUT codes
    if dims.get("structurally_equivalent") == "fail":
        codes.append("LAYOUT-001")
    elif dims.get("structurally_equivalent") == "partial":
        codes.append("LAYOUT-002")

    # RENDER codes
    if dims.get("visual_acceptable") == "fail":
        codes.append("RENDER-001")
    elif dims.get("visual_acceptable") == "partial":
        codes.append("RENDER-002")

    # PERF codes
    if result.get("timing_ms", 0) > 5000:
        codes.append("PERF-001")

    # Filter codes based on status
    if status == "fully_correct":
        codes = []  # fully_correct: no failure codes

    if not codes and status != "fully_correct":
        codes = ["UNKNOWN"]

    # For fully_correct documents there are no failure codes — return early
    if not codes:
        return [], "", "high"

    # Priority ordering for primary_code
    primary = next((c for c in PRIORITY if c in codes), codes[0])

    # Confidence
    if len(codes) >= 2 and primary not in ("UNKNOWN", "ORACLE-002"):
        confidence = "high"
    elif primary in ("UNKNOWN",):
        confidence = "low"
    else:
        confidence = "medium"

    return codes, primary, confidence


def needs_review(codes: list[str], primary: str, confidence: str) -> bool:
    """Return True if this result should be flagged for manual review."""
    if not codes:
        return False  # fully_correct — no review needed
    if confidence == "low":
        return True
    if primary == "UNKNOWN":
        return True
    if "ORACLE-002" in codes:
        return True
    return False


def assign_all_failures(data: dict) -> dict:
    """
    Process all results in a benchmark JSON and add classification fields.

    Parameters
    ----------
    data : dict
        Full benchmark JSON (with "results" list).

    Returns
    -------
    dict
        A copy of the input with classification fields added to each result.
    """
    import copy
    out = copy.deepcopy(data)
    for result in out.get("results", []):
        codes, primary, confidence = classify_failure(result)
        result["failure_codes"] = codes
        result["primary_code"] = primary
        result["classification_confidence"] = confidence
        result["needs_manual_review"] = needs_review(codes, primary, confidence)
    return out


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Assign taxonomy codes to XFA benchmark results.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    parser.add_argument(
        "--results",
        required=True,
        help="Path to benchmark results JSON (from run_xfa_benchmark.py).",
    )
    parser.add_argument(
        "--output",
        required=True,
        help="Path for output JSON with classification fields added.",
    )
    args = parser.parse_args()

    results_path = Path(args.results)
    output_path = Path(args.output)

    if not results_path.exists():
        print(f"ERROR: results file not found: {results_path}", file=sys.stderr)
        return 1

    try:
        data = json.loads(results_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        print(f"ERROR: failed to parse JSON from {results_path}: {exc}", file=sys.stderr)
        return 1

    classified = assign_all_failures(data)

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(
        json.dumps(classified, indent=2, ensure_ascii=False),
        encoding="utf-8",
    )

    total = len(classified.get("results", []))
    flagged = sum(1 for r in classified.get("results", []) if r.get("needs_manual_review"))
    print(f"Classified {total} documents. {flagged} flagged for manual review.")
    print(f"Output written to: {output_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
