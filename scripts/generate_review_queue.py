#!/usr/bin/env python3
"""
generate_review_queue.py — Generate a prioritized manual review queue.

CLI:
    python3 scripts/generate_review_queue.py \
        --results benchmarks/enterprise-baseline-2026-04-19-classified.json \
        --output benchmarks/review_queue.md \
        --previous benchmarks/enterprise-baseline-prev-classified.json
"""

from __future__ import annotations

import argparse
import json
import sys
from datetime import date
from pathlib import Path


def _fmt_row(file: str, ssim, exit_code, status: str, extra: str) -> str:
    ssim_str = f"{ssim:.4f}" if ssim is not None else "n/a"
    return f"| {file} | {ssim_str} | {exit_code} | {status} | {extra} |"


def build_review_queue(data: dict, previous_data: dict | None = None) -> str:
    """
    Build a prioritised Markdown manual review queue.

    Parameters
    ----------
    data : dict
        Classified benchmark JSON.
    previous_data : dict or None
        Optional previous run for regression detection.

    Returns
    -------
    str
        Markdown review queue.
    """
    today = date.today().isoformat()
    results = data.get("results", [])

    # --- Split into priority buckets ---

    # P1: UNKNOWN primary code
    p1 = [r for r in results if r.get("primary_code") == "UNKNOWN"]

    # P2: low confidence (exclude UNKNOWN which is already in P1)
    p2 = [
        r for r in results
        if r.get("classification_confidence") == "low"
        and r.get("primary_code") != "UNKNOWN"
    ]

    # P3: suspected oracle faults (ORACLE-002 in codes)
    p3 = [
        r for r in results
        if "ORACLE-002" in r.get("failure_codes", [])
        and r.get("primary_code") != "UNKNOWN"
        and r.get("classification_confidence") != "low"
    ]

    # P4: regressions vs previous run
    p4: list[dict] = []
    if previous_data is not None:
        prev_by_file = {
            r.get("file"): r.get("status") for r in previous_data.get("results", [])
        }
        for r in results:
            fname = r.get("file")
            prev_status = prev_by_file.get(fname)
            cur_status = r.get("status")
            # Regression: was fully_correct or not failing before, now failing
            if (
                prev_status == "fully_correct"
                and cur_status not in (None, "fully_correct")
            ):
                # Only include if not already in higher priority buckets
                if r not in p1 and r not in p2 and r not in p3:
                    p4.append(r)

    total_review = len(p1) + len(p2) + len(p3) + len(p4)

    lines: list[str] = []
    lines.append(f"# Manual Review Queue — {today}")
    lines.append("")
    lines.append(f"**Total documents needing review: {total_review}**")
    lines.append("")

    # --- Priority 1 ---
    lines.append(f"## Priority 1: Unknown failures ({len(p1)} documents)")
    lines.append("")
    lines.append(
        "These documents failed but no failure code was assigned. "
        "Each needs manual diagnosis."
    )
    lines.append("")
    if p1:
        lines.append("| File | SSIM | Exit Code | Status | Notes |")
        lines.append("|------|------|-----------|--------|-------|")
        for r in p1:
            lines.append(_fmt_row(
                r.get("file", ""),
                r.get("ssim"),
                r.get("exit_code", ""),
                r.get("status", ""),
                "UNKNOWN",
            ))
    else:
        lines.append("_No unknown failures._")
    lines.append("")

    # --- Priority 2 ---
    lines.append(f"## Priority 2: Low confidence classifications ({len(p2)} documents)")
    lines.append("")
    lines.append(
        "These were classified but with low confidence. "
        "Manual review may improve accuracy."
    )
    lines.append("")
    if p2:
        lines.append("| File | SSIM | Primary Code | Confidence | Codes |")
        lines.append("|------|------|-------------|------------|-------|")
        for r in p2:
            codes_str = ", ".join(r.get("failure_codes", []))
            ssim_str = f"{r.get('ssim'):.4f}" if r.get("ssim") is not None else "n/a"
            lines.append(
                f"| {r.get('file', '')} | {ssim_str} | {r.get('primary_code', '')} "
                f"| {r.get('classification_confidence', '')} | {codes_str} |"
            )
    else:
        lines.append("_No low-confidence classifications._")
    lines.append("")

    # --- Priority 3 ---
    lines.append(f"## Priority 3: Suspected oracle faults ({len(p3)} documents)")
    lines.append("")
    lines.append(
        "These documents may have failed due to oracle (pdfRest) output anomalies "
        "rather than genuine XFA processing issues."
    )
    lines.append("")
    if p3:
        lines.append("| File | SSIM | Primary Code | Confidence | Codes |")
        lines.append("|------|------|-------------|------------|-------|")
        for r in p3:
            codes_str = ", ".join(r.get("failure_codes", []))
            ssim_str = f"{r.get('ssim'):.4f}" if r.get("ssim") is not None else "n/a"
            lines.append(
                f"| {r.get('file', '')} | {ssim_str} | {r.get('primary_code', '')} "
                f"| {r.get('classification_confidence', '')} | {codes_str} |"
            )
    else:
        lines.append("_No suspected oracle faults._")
    lines.append("")

    # --- Priority 4 ---
    if previous_data is not None:
        lines.append(f"## Priority 4: New regressions vs previous run ({len(p4)} documents)")
        lines.append("")
        lines.append(
            "These documents were passing in the previous run but are now failing."
        )
        lines.append("")
        if p4:
            lines.append("| File | SSIM | Exit Code | Status | Primary Code |")
            lines.append("|------|------|-----------|--------|--------------|")
            for r in p4:
                ssim_str = f"{r.get('ssim'):.4f}" if r.get("ssim") is not None else "n/a"
                lines.append(
                    f"| {r.get('file', '')} | {ssim_str} | {r.get('exit_code', '')} "
                    f"| {r.get('status', '')} | {r.get('primary_code', '')} |"
                )
        else:
            lines.append("_No new regressions vs previous run._")
        lines.append("")

    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate a prioritized manual review queue from classified benchmark results.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    parser.add_argument(
        "--results",
        required=True,
        help="Path to classified benchmark results JSON.",
    )
    parser.add_argument(
        "--output",
        required=True,
        help="Path for output Markdown review queue.",
    )
    parser.add_argument(
        "--previous",
        default=None,
        help="Optional path to a previous classified benchmark JSON for regression detection.",
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

    previous_data: dict | None = None
    if args.previous:
        prev_path = Path(args.previous)
        if not prev_path.exists():
            print(f"WARNING: previous results file not found: {prev_path}", file=sys.stderr)
        else:
            try:
                previous_data = json.loads(prev_path.read_text(encoding="utf-8"))
            except json.JSONDecodeError as exc:
                print(
                    f"WARNING: failed to parse previous results JSON from {prev_path}: {exc}",
                    file=sys.stderr,
                )

    queue_md = build_review_queue(data, previous_data)

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(queue_md, encoding="utf-8")

    print(f"Review queue written to: {output_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
