#!/usr/bin/env python3
"""
regression_detector.py — Detect regressions between two benchmark runs.

Exit codes:
  0 — no regression triggered
  1 — regression triggered (one or more triggers fired)

Regression TRIGGERS (exit code 1):
  - crash_rate increased by > 1 percentage point
  - fully_correct_pct decreased by > 3 percentage points
  - A failure code that had 0 occurrences now has > 5 occurrences

Regression WARNINGS (exit code stays 0, but included in report):
  - mean_ssim decreased by > 0.005
  - Any failure code's count increased by > 20%
  - 5+ documents that were fully_correct in previous run are now worse

CLI:
    python3 scripts/regression_detector.py \\
        --current benchmarks/enterprise-baseline-2026-04-19.json \\
        --previous benchmarks/enterprise-baseline-2026-04-18.json \\
        --output benchmarks/regression_report.md

Machine-readable JSON summary is always written to stdout:
    {"regression_detected": true, "triggers": [...], "warnings": [...]}

Importable as module:
    from regression_detector import detect_regression, RegressionResult
"""

from __future__ import annotations

import argparse
import json
import sys
from collections import defaultdict
from dataclasses import dataclass, field
from datetime import date
from pathlib import Path
from statistics import mean


@dataclass
class RegressionResult:
    regression_detected: bool
    triggers: list[str]
    warnings: list[str]

    def to_dict(self) -> dict:
        return {
            "regression_detected": self.regression_detected,
            "triggers": self.triggers,
            "warnings": self.warnings,
        }


# ---------------------------------------------------------------------------
# Metric extraction helpers
# ---------------------------------------------------------------------------

def _extract_metrics(data: dict) -> dict:
    """
    Extract summary metrics from a benchmark JSON payload.

    Supports both raw benchmark JSON (with a 'results' list) and
    pre-classified JSON produced by classify_failure.py.

    Returns a dict with keys:
        total, fully_correct, crash_count, ssim_values,
        code_counts, fully_correct_files
    """
    results = data.get("results", [])
    total = len(results)

    fully_correct = 0
    crash_count = 0
    ssim_values: list[float] = []
    code_counts: dict[str, int] = defaultdict(int)
    fully_correct_files: set[str] = set()

    for r in results:
        status = r.get("status", "")
        fname = r.get("file", "")

        if status == "fully_correct":
            fully_correct += 1
            if fname:
                fully_correct_files.add(fname)

        # Crash detection: status == "crash" OR exit_code indicates crash
        exit_code = r.get("exit_code")
        if status == "crash" or exit_code in (1, 4):
            crash_count += 1

        ssim = r.get("ssim")
        if ssim is not None:
            ssim_values.append(float(ssim))

        # Count failure codes (from classified results)
        for code in r.get("failure_codes", []):
            code_counts[code] += 1
        # Also count primary code if no failure_codes list
        primary = r.get("primary_code")
        if primary and not r.get("failure_codes"):
            code_counts[primary] += 1

    return {
        "total": total,
        "fully_correct": fully_correct,
        "crash_count": crash_count,
        "ssim_values": ssim_values,
        "code_counts": dict(code_counts),
        "fully_correct_files": fully_correct_files,
    }


def detect_regression(current_data: dict, previous_data: dict) -> RegressionResult:
    """
    Compare two benchmark runs and detect regressions.

    Parameters
    ----------
    current_data : dict
        Benchmark JSON for the current run.
    previous_data : dict
        Benchmark JSON for the previous run.

    Returns
    -------
    RegressionResult
        Dataclass with regression_detected, triggers, and warnings.
    """
    cur = _extract_metrics(current_data)
    prev = _extract_metrics(previous_data)

    triggers: list[str] = []
    warnings: list[str] = []

    cur_total = cur["total"] or 1
    prev_total = prev["total"] or 1

    # --- TRIGGER: crash rate increased by > 1 pp ---
    cur_crash_pct = cur["crash_count"] / cur_total * 100
    prev_crash_pct = prev["crash_count"] / prev_total * 100
    if cur_crash_pct - prev_crash_pct > 1.0:
        triggers.append(
            f"crash_rate increased: {prev_crash_pct:.1f}% -> {cur_crash_pct:.1f}%"
        )

    # --- TRIGGER: fully_correct_pct decreased by > 3 pp ---
    cur_correct_pct = cur["fully_correct"] / cur_total * 100
    prev_correct_pct = prev["fully_correct"] / prev_total * 100
    if prev_correct_pct - cur_correct_pct > 3.0:
        triggers.append(
            f"fully_correct_pct decreased: {prev_correct_pct:.1f}% -> {cur_correct_pct:.1f}%"
        )

    # --- TRIGGER: failure code that had 0 now has > 5 ---
    for code, cur_count in cur["code_counts"].items():
        prev_count = prev["code_counts"].get(code, 0)
        if prev_count == 0 and cur_count > 5:
            triggers.append(
                f"new failure code {code}: 0 -> {cur_count} occurrences"
            )

    # --- WARNING: mean SSIM decreased by > 0.005 ---
    if cur["ssim_values"] and prev["ssim_values"]:
        cur_ssim = mean(cur["ssim_values"])
        prev_ssim = mean(prev["ssim_values"])
        if prev_ssim - cur_ssim > 0.005:
            warnings.append(
                f"mean_ssim decreased: {prev_ssim:.4f} -> {cur_ssim:.4f}"
            )

    # --- WARNING: any failure code count increased by > 20% ---
    for code, prev_count in prev["code_counts"].items():
        cur_count = cur["code_counts"].get(code, 0)
        if prev_count > 0 and (cur_count - prev_count) / prev_count > 0.20:
            pct_inc = round((cur_count - prev_count) / prev_count * 100, 1)
            warnings.append(
                f"{code} count increased by {pct_inc}%: {prev_count} -> {cur_count}"
            )

    # --- WARNING: 5+ documents that were fully_correct are now worse ---
    prev_correct_files = prev["fully_correct_files"]
    cur_correct_files = cur["fully_correct_files"]
    regressed_files = prev_correct_files - cur_correct_files
    if len(regressed_files) >= 5:
        sample = sorted(regressed_files)[:5]
        warnings.append(
            f"{len(regressed_files)} previously-correct documents now worse "
            f"(sample: {', '.join(sample)})"
        )

    regression_detected = len(triggers) > 0

    return RegressionResult(
        regression_detected=regression_detected,
        triggers=triggers,
        warnings=warnings,
    )


def render_report(result: RegressionResult, current_path: str, previous_path: str) -> str:
    """Render a Markdown regression report."""
    today = date.today().isoformat()
    lines: list[str] = []

    status_icon = "REGRESSION DETECTED" if result.regression_detected else "OK — no regression"

    lines.append(f"# Regression Report — {today}")
    lines.append("")
    lines.append(f"**Status**: {status_icon}")
    lines.append("")
    lines.append(f"- **Current run**: `{current_path}`")
    lines.append(f"- **Previous run**: `{previous_path}`")
    lines.append("")

    if result.triggers:
        lines.append("## Triggers (exit code 1)")
        lines.append("")
        for t in result.triggers:
            lines.append(f"- {t}")
        lines.append("")
    else:
        lines.append("## Triggers")
        lines.append("")
        lines.append("_None._")
        lines.append("")

    if result.warnings:
        lines.append("## Warnings")
        lines.append("")
        for w in result.warnings:
            lines.append(f"- {w}")
        lines.append("")
    else:
        lines.append("## Warnings")
        lines.append("")
        lines.append("_None._")
        lines.append("")

    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Detect regressions between two benchmark runs."
    )
    parser.add_argument(
        "--current",
        required=True,
        type=Path,
        help="Path to current benchmark JSON",
    )
    parser.add_argument(
        "--previous",
        required=True,
        type=Path,
        help="Path to previous benchmark JSON",
    )
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help="Output path for Markdown regression report (optional)",
    )
    args = parser.parse_args(argv)

    if not args.current.exists():
        print(f"ERROR: current file not found: {args.current}", file=sys.stderr)
        return 1
    if not args.previous.exists():
        print(f"ERROR: previous file not found: {args.previous}", file=sys.stderr)
        return 1

    current_data = json.loads(args.current.read_text())
    previous_data = json.loads(args.previous.read_text())

    result = detect_regression(current_data, previous_data)

    # Always write machine-readable summary to stdout
    print(json.dumps(result.to_dict()))

    if args.output:
        report_md = render_report(result, str(args.current), str(args.previous))
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(report_md)
        # Use stderr so stdout stays clean for machine-readable output
        print(f"Regression report written to {args.output}", file=sys.stderr)

    return 1 if result.regression_detected else 0


if __name__ == "__main__":
    sys.exit(main())
