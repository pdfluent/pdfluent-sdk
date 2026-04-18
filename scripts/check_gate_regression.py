#!/usr/bin/env python3
"""Check GATE result against baseline; fail if pass rate regressed.

Usage:
    python3 scripts/check_gate_regression.py \
        --result /tmp/gate_ci_result.json \
        --baseline corpus/CI_BASELINE.json \
        --tolerance 0.005 \
        --output /tmp/gate_report.md
"""
import argparse
import json
import sys
from pathlib import Path


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--result", required=True)
    p.add_argument("--baseline", required=True)
    p.add_argument("--tolerance", type=float, default=0.005)
    p.add_argument("--output", default="-")
    args = p.parse_args()

    result_data = json.loads(Path(args.result).read_text())
    baseline_path = Path(args.baseline)

    current_summary = result_data.get("summary", {})
    current_total = current_summary.get("total", 0)
    current_pass = current_summary.get("pass", 0)
    current_oracle_fault = current_summary.get("oracle_fault", 0)
    current_crash = current_summary.get("crash", 0)
    current_fail = current_summary.get("fail", 0)
    current_mean_ssim = current_summary.get("mean_ssim", 0.0)

    # Effective pass rate: exclude oracle faults from denominator
    effective_total = current_total - current_oracle_fault
    current_rate = current_pass / effective_total if effective_total > 0 else 0.0

    # Load baseline
    if not baseline_path.exists():
        print(f"WARNING: No baseline found at {baseline_path}. Creating initial baseline.")
        baseline = {"pass_rate": current_rate, "pass": current_pass,
                    "total": current_total, "mean_ssim": current_mean_ssim}
        baseline_path.write_text(json.dumps(baseline, indent=2))
        baseline_rate = current_rate
        is_new_baseline = True
    else:
        baseline = json.loads(baseline_path.read_text())
        baseline_rate = baseline.get("pass_rate", 0.0)
        is_new_baseline = False

    delta = current_rate - baseline_rate
    regressed = delta < -args.tolerance
    improved = delta > args.tolerance

    # Build report
    lines = ["## GATE Rendering Quality Report\n"]

    if regressed:
        lines.append(f"### 🔴 REGRESSION DETECTED\n")
        lines.append(f"Pass rate dropped by **{abs(delta)*100:.2f}%** (tolerance: {args.tolerance*100:.1f}%).\n")
    elif improved:
        lines.append(f"### 🟢 Rendering Quality Improved\n")
        lines.append(f"Pass rate improved by **{delta*100:.2f}%**.\n")
    elif is_new_baseline:
        lines.append(f"### 🟡 Initial Baseline Created\n")
    else:
        lines.append(f"### ✅ No Regression\n")

    lines.append("| Metric | Baseline | Current | Delta |")
    lines.append("|--------|----------|---------|-------|")
    lines.append(f"| Pass rate | {baseline_rate*100:.2f}% | {current_rate*100:.2f}% | {delta*100:+.2f}% |")
    lines.append(f"| Pass | {baseline.get('pass', '-')} | {current_pass} | {current_pass - baseline.get('pass', current_pass):+d} |")
    lines.append(f"| Fail | {baseline.get('fail', '-')} | {current_fail} | - |")
    lines.append(f"| Crash | {baseline.get('crash', '-')} | {current_crash} | - |")
    lines.append(f"| Mean SSIM | {baseline.get('mean_ssim', '-')} | {current_mean_ssim} | - |")
    lines.append("")

    # List newly failing entries if regressed
    if regressed:
        baseline_pass_set = set(baseline.get("pass_files", []))
        current_results = result_data.get("results", [])
        newly_failing = [
            r["file"] for r in current_results
            if r.get("render", {}).get("status") != "pass"
            and r["file"] in baseline_pass_set
        ]
        if newly_failing:
            lines.append("**Newly failing entries:**")
            for f in sorted(newly_failing)[:20]:
                lines.append(f"- `{f}`")
            if len(newly_failing) > 20:
                lines.append(f"- ...and {len(newly_failing) - 20} more")
            lines.append("")

    if current_crash > 0:
        crashed = [r["file"] for r in result_data.get("results", [])
                   if r.get("render", {}).get("status") == "crash"]
        lines.append(f"**⚠️ {current_crash} crash(es) detected:**")
        for f in crashed:
            lines.append(f"- `{f}`")
        lines.append("")

    report = "\n".join(lines)

    if args.output == "-":
        print(report)
    else:
        Path(args.output).write_text(report)
        print(f"Report written to {args.output}")

    if regressed:
        print(f"\nFAIL: Pass rate regressed {abs(delta)*100:.2f}% (baseline {baseline_rate*100:.2f}% → current {current_rate*100:.2f}%)", file=sys.stderr)
        sys.exit(1)

    if current_crash > 0:
        print(f"\nFAIL: {current_crash} crash(es) detected", file=sys.stderr)
        sys.exit(1)

    print(f"PASS: {current_pass}/{effective_total} ({current_rate*100:.2f}%) — delta {delta*100:+.2f}%")


if __name__ == "__main__":
    main()
