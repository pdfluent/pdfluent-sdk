#!/usr/bin/env python3
"""
gate_analyze.py — Structured GATE analysis with optional delta comparison.

Usage:
    python3 scripts/gate_analyze.py \\
        --gate benchmarks/gate-history/gate-5k-02.json \\
        [--baseline benchmarks/gate-history/gate-5k-01.json] \\
        [--out /tmp/gate_analysis.md]
"""

import argparse
import json
import os
import sys
from datetime import date


# ── helpers ───────────────────────────────────────────────────────────────────

def load_gate(path: str) -> dict:
    with open(path) as f:
        return json.load(f)


def build_index(data: dict) -> dict:
    """Return dict keyed by filename."""
    return {r["file"]: r for r in data.get("results", [])}


def stem(path: str) -> str:
    """Return filename without directory or extension, used as report name."""
    return os.path.splitext(os.path.basename(path))[0]


def pct(n: int, total: int) -> str:
    if total == 0:
        return "—"
    return f"{100 * n / total:.1f}%"


def fmt_delta(delta: int, show_pct: bool = False, base: int = 0) -> str:
    """Format a numeric delta with sign, optional pp suffix."""
    if delta == 0:
        sign = "±0"
    elif delta > 0:
        sign = f"+{delta}"
    else:
        sign = str(delta)

    if show_pct and base > 0:
        pp = 100 * delta / base
        pp_str = f"{pp:+.1f}pp"
        return f"{sign} ({pp_str})"
    return sign


def fmt_ssim_delta(delta: float) -> str:
    if delta == 0:
        return "±0.0000"
    return f"{delta:+.4f}"


def file_prefix(filename: str) -> str:
    """
    Detect corpus source prefix from filename.

    Strategy:
    1. Names starting with a known double-underscore corpus (stressful__)
       are collapsed to their first __ segment.
    2. Names with a leading dash-separated token use that token.
    3. Otherwise 'plain'.
    """
    # Double-underscore prefixed corpora (e.g. stressful__, stress__)
    if "__" in filename:
        return filename.split("__")[0]
    # Dash-separated prefix (c4k-, xfa-, gen-, PDFIUM-, cs-, curated-, ...)
    if "-" in filename:
        return filename.split("-")[0]
    return "plain"


SSIM_BANDS = [
    ("0.90–0.94", 0.90, 0.94),
    ("0.80–0.90", 0.80, 0.90),
    ("0.70–0.80", 0.70, 0.80),
    ("0.50–0.70", 0.50, 0.70),
    ("< 0.50",    0.00, 0.50),
]


def band_distribution(results: list) -> dict:
    """
    Count fail-status entries per SSIM band.
    Returns {label: count}.
    """
    counts = {label: 0 for label, _, _ in SSIM_BANDS}
    for r in results:
        render = r.get("render", {})
        if render.get("status") != "fail":
            continue
        ssim = render.get("ssim")
        if ssim is None:
            continue
        for label, lo, hi in SSIM_BANDS:
            if lo <= ssim < hi or (label.startswith("<") and ssim < hi):
                counts[label] += 1
                break
    return counts


def count_statuses(results: list) -> dict:
    """Count each render status value."""
    counts = {}
    for r in results:
        s = r.get("render", {}).get("status", "unknown")
        counts[s] = counts.get(s, 0) + 1
    return counts


def fail_prefix_distribution(results: list) -> dict:
    """
    For results whose render status is fail/crash, count by corpus prefix.
    """
    counts = {}
    for r in results:
        status = r.get("render", {}).get("status", "")
        if status not in ("fail", "crash", "xfa_error", "timeout",
                          "encrypted", "degenerate"):
            continue
        p = file_prefix(r["file"])
        counts[p] = counts.get(p, 0) + 1
    return counts


# ── report builder ────────────────────────────────────────────────────────────

def build_report(gate_path: str, baseline_path: str | None, out_path: str | None):
    gate_data = load_gate(gate_path)
    gate_idx = build_index(gate_data)
    cur_results = gate_data["results"]
    cur_summary = gate_data["summary"]
    cur_name = stem(gate_path)

    has_baseline = baseline_path is not None
    if has_baseline:
        base_data = load_gate(baseline_path)
        base_idx = build_index(base_data)
        base_results = base_data["results"]
        base_summary = base_data["summary"]
        base_name = stem(baseline_path)
        title = f"# GATE Analysis: {cur_name} vs {base_name}"
    else:
        title = f"# GATE Analysis: {cur_name}"

    lines = []
    lines.append(title)
    lines.append("")
    lines.append(f"**Generated**: {date.today().isoformat()}")
    lines.append("")

    # ── Summary table ──────────────────────────────────────────────────────────
    lines.append("## Summary")
    lines.append("")

    cur_statuses = count_statuses(cur_results)
    cur_total = cur_summary.get("total", len(cur_results))
    cur_pass = cur_statuses.get("pass", cur_summary.get("pass", 0))
    cur_fail = cur_statuses.get("fail", cur_summary.get("fail", 0))
    cur_crash = cur_statuses.get("crash", cur_summary.get("crash", 0))
    cur_encrypted = cur_statuses.get("encrypted", 0)
    cur_degen = cur_statuses.get("degenerate", 0)
    cur_xfa_err = cur_statuses.get("xfa_error", 0)
    cur_timeout = cur_statuses.get("timeout", 0)
    cur_oracle = cur_statuses.get("oracle_fault", cur_summary.get("oracle_fault", 0))
    cur_mean_ssim = cur_summary.get("mean_ssim", 0.0)

    if has_baseline:
        base_statuses = count_statuses(base_results)
        base_total = base_summary.get("total", len(base_results))
        base_pass = base_statuses.get("pass", base_summary.get("pass", 0))
        base_fail = base_statuses.get("fail", base_summary.get("fail", 0))
        base_crash = base_statuses.get("crash", base_summary.get("crash", 0))
        base_encrypted = base_statuses.get("encrypted", 0)
        base_degen = base_statuses.get("degenerate", 0)
        base_xfa_err = base_statuses.get("xfa_error", 0)
        base_timeout = base_statuses.get("timeout", 0)
        base_oracle = base_statuses.get("oracle_fault",
                                        base_summary.get("oracle_fault", 0))
        base_mean_ssim = base_summary.get("mean_ssim", 0.0)

        lines.append("| Metric | Baseline | Current | Delta |")
        lines.append("|--------|----------|---------|-------|")

        def row(label, bv, cv, show_pct=False, base_for_pct=0, is_ssim=False):
            if is_ssim:
                delta_str = fmt_ssim_delta(cv - bv)
                return f"| {label} | {bv:.4f} | {cv:.4f} | {delta_str} |"
            bpct = f" ({pct(bv, base_for_pct)})" if show_pct and base_for_pct else ""
            cpct = f" ({pct(cv, cur_total)})" if show_pct else ""
            delta = cv - bv
            delta_str = fmt_delta(delta, show_pct=show_pct, base=base_for_pct)
            return f"| {label} | {bv}{bpct} | {cv}{cpct} | {delta_str} |"

        lines.append(f"| Total | {base_total} | {cur_total} | "
                     f"{fmt_delta(cur_total - base_total)} |")
        lines.append(row(f"Pass (≥{cur_summary.get('threshold', 0.95)})",
                         base_pass, cur_pass,
                         show_pct=True, base_for_pct=base_total))
        lines.append(row("Fail", base_fail, cur_fail,
                         show_pct=True, base_for_pct=base_total))
        lines.append(row("Crash", base_crash, cur_crash,
                         show_pct=True, base_for_pct=base_total))

        def opt_row(label, bv, cv):
            if bv == 0 and cv == 0:
                return None
            delta_str = fmt_delta(cv - bv)
            return f"| {label} | {bv} | {cv} | {delta_str} |"

        for lbl, bv, cv in [
            ("Encrypted", base_encrypted, cur_encrypted),
            ("Degenerate", base_degen, cur_degen),
            ("XFA Error", base_xfa_err, cur_xfa_err),
            ("Timeout", base_timeout, cur_timeout),
        ]:
            r = opt_row(lbl, bv, cv)
            if r:
                lines.append(r)

        lines.append(row("Oracle fault", base_oracle, cur_oracle))
        lines.append(row("Mean SSIM", base_mean_ssim, cur_mean_ssim,
                         is_ssim=True))

    else:
        # No baseline — single-column table
        lines.append("| Metric | Value |")
        lines.append("|--------|-------|")
        lines.append(f"| Total | {cur_total} |")
        threshold = cur_summary.get("threshold", 0.95)
        lines.append(f"| Pass (≥{threshold}) | {cur_pass} ({pct(cur_pass, cur_total)}) |")
        lines.append(f"| Fail | {cur_fail} ({pct(cur_fail, cur_total)}) |")
        lines.append(f"| Crash | {cur_crash} ({pct(cur_crash, cur_total)}) |")
        for lbl, val in [
            ("Encrypted", cur_encrypted),
            ("Degenerate", cur_degen),
            ("XFA Error", cur_xfa_err),
            ("Timeout", cur_timeout),
        ]:
            if val:
                lines.append(f"| {lbl} | {val} |")
        lines.append(f"| Oracle fault | {cur_oracle} |")
        lines.append(f"| Mean SSIM | {cur_mean_ssim:.4f} |")

    lines.append("")

    # ── SSIM Band Distribution ─────────────────────────────────────────────────
    lines.append("## SSIM Band Distribution (render status=fail)")
    lines.append("")

    cur_bands = band_distribution(cur_results)
    if has_baseline:
        base_bands = band_distribution(base_results)
        lines.append("| Band | Baseline | Current | Delta |")
        lines.append("|------|----------|---------|-------|")
        for label, _, _ in SSIM_BANDS:
            bv = base_bands.get(label, 0)
            cv = cur_bands.get(label, 0)
            lines.append(f"| {label} | {bv} | {cv} | {fmt_delta(cv - bv)} |")
    else:
        lines.append("| Band | Count |")
        lines.append("|------|-------|")
        for label, _, _ in SSIM_BANDS:
            lines.append(f"| {label} | {cur_bands.get(label, 0)} |")

    lines.append("")

    # ── Corpus Source Distribution ─────────────────────────────────────────────
    lines.append("## Corpus Source Distribution (fails)")
    lines.append("")

    cur_pfx = fail_prefix_distribution(cur_results)
    total_fails = sum(cur_pfx.values())

    lines.append("| Prefix | Count | % of fails |")
    lines.append("|--------|-------|-----------|")
    for prefix, count in sorted(cur_pfx.items(), key=lambda x: -x[1]):
        lines.append(f"| {prefix} | {count} | {pct(count, total_fails)} |")

    lines.append("")

    # ── Regressions & Improvements (only when baseline provided) ──────────────
    if has_baseline:
        regressions = []
        improvements = []
        reclassified = []

        FAIL_LIKE = {"fail", "crash", "xfa_error", "timeout"}
        PASS_LIKE = {"pass"}
        CLASSIFIED = {"encrypted", "degenerate", "xfa_error", "timeout"}

        for fname, cur_r in gate_idx.items():
            if fname not in base_idx:
                continue
            base_r = base_idx[fname]

            base_status = base_r.get("render", {}).get("status", "")
            cur_status = cur_r.get("render", {}).get("status", "")
            base_ssim = base_r.get("render", {}).get("ssim")
            cur_ssim = cur_r.get("render", {}).get("ssim")

            # Regression: was pass, now fail/crash
            if base_status in PASS_LIKE and cur_status in FAIL_LIKE:
                regressions.append((
                    fname,
                    base_ssim if base_ssim is not None else "—",
                    cur_ssim if cur_ssim is not None else "—",
                    cur_status,
                ))

            # Improvement: was fail/crash, now pass
            elif base_status in FAIL_LIKE and cur_status in PASS_LIKE:
                base_desc = (f"fail ({base_ssim:.4f})"
                             if base_status == "fail" and base_ssim is not None
                             else base_status)
                improvements.append((
                    fname,
                    base_desc,
                    cur_ssim if cur_ssim is not None else "—",
                ))

            # Reclassified: was crash, now properly classified
            elif base_status == "crash" and cur_status in CLASSIFIED:
                reclassified.append((fname, cur_status))

        lines.append("## Regressions (pass → fail or crash)")
        lines.append("")
        if regressions:
            lines.append("| File | Baseline SSIM | Current SSIM | Change |")
            lines.append("|------|--------------|--------------|--------|")
            for fname, bssim, cssim, cstatus in sorted(regressions):
                if isinstance(bssim, float) and isinstance(cssim, float):
                    delta = f"{cssim - bssim:+.4f}"
                else:
                    delta = "—"
                bssim_s = f"{bssim:.4f}" if isinstance(bssim, float) else str(bssim)
                cssim_s = (f"{cssim:.4f} ({cstatus})"
                           if isinstance(cssim, float) else cstatus)
                lines.append(f"| {fname} | {bssim_s} | {cssim_s} | {delta} |")
        else:
            lines.append("_No regressions detected._")
        lines.append("")

        lines.append("## Improvements (fail/crash → pass)")
        lines.append("")
        if improvements:
            lines.append("| File | Baseline Status | Current SSIM |")
            lines.append("|------|----------------|--------------|")
            for fname, base_desc, cssim in sorted(improvements):
                cssim_s = f"{cssim:.4f}" if isinstance(cssim, float) else str(cssim)
                lines.append(f"| {fname} | {base_desc} | {cssim_s} |")
        else:
            lines.append("_No improvements detected._")
        lines.append("")

        lines.append("## New crash types resolved")
        lines.append("")
        lines.append(
            "_Files that were `crash` in baseline but are now properly "
            "classified (`encrypted`, `degenerate`, `xfa_error`, `timeout`)._"
        )
        lines.append("")
        if reclassified:
            lines.append("| File | New Status |")
            lines.append("|------|-----------|")
            for fname, new_status in sorted(reclassified):
                lines.append(f"| {fname} | {new_status} |")
        else:
            lines.append("_No reclassifications detected._")
        lines.append("")

    report = "\n".join(lines) + "\n"

    if out_path:
        with open(out_path, "w") as f:
            f.write(report)
        print(f"Report written to {out_path}", file=sys.stderr)
    else:
        sys.stdout.write(report)


# ── CLI ───────────────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(
        description="Analyze a GATE JSON result file, optionally comparing to a baseline."
    )
    parser.add_argument(
        "--gate", required=True,
        help="Path to current GATE JSON file"
    )
    parser.add_argument(
        "--baseline", default=None,
        help="Path to baseline GATE JSON file for delta comparison"
    )
    parser.add_argument(
        "--out", default=None,
        help="Output path for the Markdown report (default: stdout)"
    )
    args = parser.parse_args()

    if not os.path.isfile(args.gate):
        print(f"ERROR: gate file not found: {args.gate}", file=sys.stderr)
        sys.exit(1)
    if args.baseline and not os.path.isfile(args.baseline):
        print(f"ERROR: baseline file not found: {args.baseline}", file=sys.stderr)
        sys.exit(1)

    build_report(args.gate, args.baseline, args.out)


if __name__ == "__main__":
    main()
