#!/usr/bin/env python3
"""EVH-C2-07: Generate a Markdown benchmark report from a results JSON file.

Usage:
    python3 scripts/generate_benchmark_report.py \
        --results benchmarks/xfa-benchmark-2026-04-19.json \
        --output  benchmarks/xfa-benchmark-2026-04-19-report.md \
        --previous benchmarks/xfa-benchmark-2026-04-18.json

    # Custom KPI targets:
    python3 scripts/generate_benchmark_report.py \
        --results benchmarks/xfa-benchmark-2026-04-19.json \
        --output  benchmarks/report.md \
        --config  benchmarks/kpi_targets.json
"""
import argparse
import json
import sys
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Optional

# ---------------------------------------------------------------------------
# Default KPI targets
# ---------------------------------------------------------------------------

DEFAULT_KPI_TARGETS: dict[str, dict] = {
    "fully_correct_pct":   {"op": ">=", "target": 75.0,  "label": "Correct (fully_correct)"},
    "minor_deviation_pct": {"op": "<=", "target": 15.0,  "label": "Minor deviation"},
    "major_deviation_pct": {"op": "<=", "target":  5.0,  "label": "Major deviation"},
    "partial_render_pct":  {"op": "<=", "target":  3.0,  "label": "Partial render"},
    "crash_pct":           {"op": "<=", "target":  2.0,  "label": "Crash"},
    "mean_ssim":           {"op": ">=", "target":  0.970, "label": "Mean SSIM"},
}

VALID_STATUSES = [
    "fully_correct", "minor_deviation", "major_deviation",
    "partial_render", "render_fail", "flatten_issue",
    "crash", "encrypted", "degenerate", "xfa_error", "oracle_fault",
]

# ---------------------------------------------------------------------------
# Load helpers
# ---------------------------------------------------------------------------

def _load_json(path: str) -> Optional[dict]:
    try:
        return json.loads(Path(path).read_text())
    except Exception as exc:
        print(f"WARNING: could not load {path}: {exc}", file=sys.stderr)
        return None


def _unpack(data: Any) -> tuple[list[dict], dict, dict]:
    """Unpack a benchmark JSON into (results, config, summary)."""
    if data is None:
        return [], {}, {}
    if isinstance(data, list):
        return data, {}, {}
    results = data.get("results", [])
    config = data.get("config", {})
    summary = data.get("summary", {})
    return results, config, summary


# ---------------------------------------------------------------------------
# Compute summary statistics
# ---------------------------------------------------------------------------

def _compute_summary(results: list[dict]) -> dict:
    total = len(results)
    if total == 0:
        return {"total": 0}

    counts: dict[str, int] = {s: 0 for s in VALID_STATUSES}
    ssim_vals: list[float] = []
    completeness_vals: list[float] = []

    for r in results:
        status = r.get("status", "crash")
        if status in counts:
            counts[status] += 1
        ssim = r.get("ssim")
        if isinstance(ssim, (int, float)):
            ssim_vals.append(float(ssim))
        comp = r.get("completeness")
        if isinstance(comp, (int, float)):
            completeness_vals.append(float(comp))

    summary: dict = {"total": total}
    for s in VALID_STATUSES:
        summary[s] = counts[s]
        summary[f"{s}_pct"] = round(100.0 * counts[s] / total, 2) if total else 0.0

    summary["mean_ssim"] = (
        round(sum(ssim_vals) / len(ssim_vals), 4) if ssim_vals else None
    )
    summary["mean_completeness"] = (
        round(sum(completeness_vals) / len(completeness_vals), 4)
        if completeness_vals else None
    )
    return summary


# ---------------------------------------------------------------------------
# Report sections
# ---------------------------------------------------------------------------

def _badge(ok: bool) -> str:
    return "✅" if ok else "❌"


def _kpi_ok(key: str, actual: Any, targets: dict) -> bool:
    if actual is None:
        return False
    cfg = targets.get(key, {})
    op = cfg.get("op", ">=")
    threshold = cfg.get("target", 0)
    try:
        if op == ">=":
            return float(actual) >= float(threshold)
        if op == "<=":
            return float(actual) <= float(threshold)
        if op == "==":
            return float(actual) == float(threshold)
    except (TypeError, ValueError):
        pass
    return False


def _section_executive_summary(summary: dict, targets: dict) -> str:
    lines = ["## Executive Summary", ""]
    lines.append("| KPI | Target | Actual | Status |")
    lines.append("|-----|--------|--------|--------|")

    kpi_rows = [
        ("fully_correct_pct",   "≥ 75%"),
        ("minor_deviation_pct", "≤ 15%"),
        ("major_deviation_pct", "≤  5%"),
        ("partial_render_pct",  "≤  3%"),
        ("crash_pct",           "≤  2%"),
        ("mean_ssim",           "≥ 0.970"),
    ]

    for key, target_str in kpi_rows:
        label = targets.get(key, {}).get("label", key)
        actual = summary.get(key)
        ok = _kpi_ok(key, actual, targets)

        if actual is None:
            actual_str = "n/a"
        elif isinstance(actual, float) and actual < 2.0 and "pct" not in key:
            actual_str = f"{actual:.4f}"
        elif isinstance(actual, float):
            actual_str = f"{actual:.2f}%"
        else:
            actual_str = str(actual)

        lines.append(
            f"| {label} | {target_str} | {actual_str} | {_badge(ok)} |"
        )

    total = summary.get("total", 0)
    lines.append("")
    lines.append(f"**Total documents evaluated:** {total}")
    lines.append("")
    return "\n".join(lines)


def _section_status_distribution(summary: dict) -> str:
    total = summary.get("total", 0)
    lines = ["## Status Distribution", ""]
    lines.append("| Status | Count | % |")
    lines.append("|--------|-------|---|")
    for s in VALID_STATUSES:
        count = summary.get(s, 0)
        pct = summary.get(f"{s}_pct", 0.0)
        lines.append(f"| {s} | {count} | {pct:.1f}% |")
    lines.append("")
    return "\n".join(lines)


def _section_worst_documents(results: list[dict], n: int = 20) -> str:
    lines = [f"## Top {n} Worst Documents", ""]

    # Score each document: lower is worse
    # Ordering: render_fail < partial_render < major_deviation < minor_deviation < fully_correct
    status_rank = {
        "render_fail": 0, "crash": 1, "xfa_error": 2, "degenerate": 3,
        "partial_render": 4, "flatten_issue": 5, "major_deviation": 6,
        "minor_deviation": 7, "oracle_fault": 8, "encrypted": 9, "fully_correct": 10,
    }

    def _worst_key(r: dict) -> tuple:
        rank = status_rank.get(r.get("status", "crash"), 99)
        ssim = r.get("ssim")
        ssim_sort = float(ssim) if isinstance(ssim, (int, float)) else 0.0
        return (rank, ssim_sort)

    worst = sorted(results, key=_worst_key)[:n]

    if not worst:
        lines.append("_(no results)_")
        lines.append("")
        return "\n".join(lines)

    lines.append("| File | Status | SSIM | Completeness | Primary Reason |")
    lines.append("|------|--------|------|--------------|----------------|")

    for r in worst:
        fname = r.get("file", "?")
        status = r.get("status", "?")
        ssim = r.get("ssim")
        ssim_str = f"{ssim:.4f}" if isinstance(ssim, (int, float)) else "-"
        comp = r.get("completeness")
        comp_str = f"{comp:.2f}" if isinstance(comp, (int, float)) else "-"
        reasons = r.get("failure_reasons", [])
        reason_str = "; ".join(reasons[:2]) if reasons else "-"
        # Truncate long filenames
        short_name = fname[-50:] if len(fname) > 50 else fname
        lines.append(
            f"| `{short_name}` | {status} | {ssim_str} | {comp_str} | {reason_str} |"
        )

    lines.append("")
    return "\n".join(lines)


def _section_category_breakdown(results: list[dict], inventory_path: Optional[str]) -> str:
    lines = ["## Category Breakdown", ""]

    if not inventory_path:
        lines.append("_No corpus inventory provided — category breakdown not available._")
        lines.append("")
        return "\n".join(lines)

    inv_data = _load_json(inventory_path)
    if not inv_data:
        lines.append("_Could not load corpus inventory._")
        lines.append("")
        return "\n".join(lines)

    # inventory is expected to map filename → {category: str, ...}
    inv: dict[str, dict] = {}
    if isinstance(inv_data, list):
        for item in inv_data:
            if isinstance(item, dict) and "file" in item:
                inv[item["file"]] = item
    elif isinstance(inv_data, dict):
        inv = inv_data

    cat_stats: dict[str, dict] = {}
    for r in results:
        fname = r.get("file", "")
        meta = inv.get(fname, inv.get(Path(fname).name, {}))
        category = meta.get("category", "unknown") if isinstance(meta, dict) else "unknown"
        if category not in cat_stats:
            cat_stats[category] = {"total": 0, "fully_correct": 0, "ssim_sum": 0.0, "ssim_n": 0}
        cat_stats[category]["total"] += 1
        if r.get("status") == "fully_correct":
            cat_stats[category]["fully_correct"] += 1
        ssim = r.get("ssim")
        if isinstance(ssim, (int, float)):
            cat_stats[category]["ssim_sum"] += float(ssim)
            cat_stats[category]["ssim_n"] += 1

    if not cat_stats:
        lines.append("_No category data found._")
        lines.append("")
        return "\n".join(lines)

    lines.append("| Category | Total | Correct | Correct% | Mean SSIM |")
    lines.append("|----------|-------|---------|----------|-----------|")

    for cat in sorted(cat_stats):
        s = cat_stats[cat]
        total = s["total"]
        correct = s["fully_correct"]
        pct = 100.0 * correct / total if total else 0.0
        mean_ssim = (s["ssim_sum"] / s["ssim_n"]) if s["ssim_n"] > 0 else None
        ssim_str = f"{mean_ssim:.4f}" if mean_ssim is not None else "-"
        lines.append(f"| {cat} | {total} | {correct} | {pct:.1f}% | {ssim_str} |")

    lines.append("")
    return "\n".join(lines)


def _section_delta(current_summary: dict, previous_path: Optional[str]) -> str:
    lines = ["## Delta vs Previous Run", ""]

    if not previous_path:
        lines.append("_No previous run provided — delta not available._")
        lines.append("")
        return "\n".join(lines)

    prev_data = _load_json(previous_path)
    if not prev_data:
        lines.append("_Could not load previous run._")
        lines.append("")
        return "\n".join(lines)

    prev_results, _, prev_stored_summary = _unpack(prev_data)
    prev_summary = prev_stored_summary if prev_stored_summary else _compute_summary(prev_results)

    metrics = [
        ("fully_correct_pct",   "Correct %"),
        ("minor_deviation_pct", "Minor deviation %"),
        ("major_deviation_pct", "Major deviation %"),
        ("crash_pct",           "Crash %"),
        ("mean_ssim",           "Mean SSIM"),
        ("mean_completeness",   "Mean completeness"),
    ]

    lines.append("| Metric | Previous | Current | Delta |")
    lines.append("|--------|----------|---------|-------|")

    for key, label in metrics:
        prev_val = prev_summary.get(key)
        curr_val = current_summary.get(key)

        def _fmt(v: Any) -> str:
            if v is None:
                return "n/a"
            if isinstance(v, float):
                return f"{v:.4f}"
            return str(v)

        if prev_val is not None and curr_val is not None:
            try:
                delta = float(curr_val) - float(prev_val)
                delta_str = f"{delta:+.4f}"
            except (TypeError, ValueError):
                delta_str = "n/a"
        else:
            delta_str = "n/a"

        lines.append(f"| {label} | {_fmt(prev_val)} | {_fmt(curr_val)} | {delta_str} |")

    lines.append("")
    return "\n".join(lines)


def _section_known_issues(results: list[dict], inventory_path: Optional[str]) -> str:
    lines = ["## Known Issues", ""]

    if not inventory_path:
        lines.append("_No corpus inventory — skipping known issues section._")
        lines.append("")
        return "\n".join(lines)

    inv_data = _load_json(inventory_path)
    if not inv_data:
        lines.append("_Could not load corpus inventory._")
        lines.append("")
        return "\n".join(lines)

    inv: dict[str, dict] = {}
    if isinstance(inv_data, list):
        for item in inv_data:
            if isinstance(item, dict) and "file" in item:
                inv[item["file"]] = item
    elif isinstance(inv_data, dict):
        inv = inv_data

    # Find results with known_issues tag
    known_issue_rows: list[tuple[str, str, str]] = []
    for r in results:
        fname = r.get("file", "")
        meta = inv.get(fname, inv.get(Path(fname).name, {}))
        if not isinstance(meta, dict):
            continue
        ki = meta.get("known_issues")
        if ki:
            status = r.get("status", "?")
            if isinstance(ki, list):
                ki_str = "; ".join(str(x) for x in ki)
            else:
                ki_str = str(ki)
            known_issue_rows.append((fname, status, ki_str))

    if not known_issue_rows:
        lines.append("_No documents with known_issues tags found in inventory._")
        lines.append("")
        return "\n".join(lines)

    lines.append("| File | Status | Known Issues |")
    lines.append("|------|--------|--------------|")
    for fname, status, ki_str in sorted(known_issue_rows):
        short_name = fname[-50:] if len(fname) > 50 else fname
        lines.append(f"| `{short_name}` | {status} | {ki_str} |")

    lines.append("")
    return "\n".join(lines)


# ---------------------------------------------------------------------------
# Report assembly
# ---------------------------------------------------------------------------

def generate_report(
    results: list[dict],
    config: dict,
    date_str: str,
    previous_path: Optional[str] = None,
    inventory_path: Optional[str] = None,
    kpi_targets: Optional[dict] = None,
) -> str:
    targets = {**DEFAULT_KPI_TARGETS}
    if kpi_targets:
        # Allow overriding individual target values
        for key, val in kpi_targets.items():
            if key in targets and isinstance(val, dict):
                targets[key].update(val)
            elif key in targets:
                targets[key]["target"] = val

    summary = _compute_summary(results)
    run_id = config.get("run_id", "")
    oracle = config.get("oracle", "")

    meta_parts = []
    if run_id:
        meta_parts.append(f"run_id: `{run_id}`")
    if oracle:
        meta_parts.append(f"oracle: {oracle}")
    meta_line = "  |  ".join(meta_parts) if meta_parts else ""

    lines: list[str] = []
    lines.append(f"# XFA Benchmark Report — {date_str}")
    lines.append("")
    if meta_line:
        lines.append(f"_{meta_line}_")
        lines.append("")

    lines.append(_section_executive_summary(summary, targets))
    lines.append(_section_status_distribution(summary))
    lines.append(_section_worst_documents(results))
    lines.append(_section_category_breakdown(results, inventory_path))
    lines.append(_section_delta(summary, previous_path))
    lines.append(_section_known_issues(results, inventory_path))

    # Footer
    generated_at = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    lines.append(f"---")
    lines.append(f"_Report generated: {generated_at}_")
    lines.append("")

    return "\n".join(lines)


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------

def main() -> None:
    p = argparse.ArgumentParser(
        description="Generate a Markdown benchmark report from a results JSON file"
    )
    p.add_argument("--results", required=True,
                   help="Path to benchmark result JSON file")
    p.add_argument("--output", required=True,
                   help="Path to write Markdown report")
    p.add_argument("--previous", default=None,
                   help="Path to previous run result JSON (for delta section)")
    p.add_argument("--inventory", default=None,
                   help="Path to corpus inventory JSON (for category breakdown and known issues)")
    p.add_argument("--config", default=None,
                   help="Path to JSON file with custom KPI targets (overrides defaults)")
    p.add_argument("--date", default=None,
                   help="Date string for report title (default: today, ISO format)")
    args = p.parse_args()

    if not Path(args.results).exists():
        sys.exit(f"ERROR: results file not found: {args.results}")

    data = _load_json(args.results)
    results, config, _ = _unpack(data)

    kpi_targets = None
    if args.config:
        kpi_targets = _load_json(args.config)

    date_str = args.date or datetime.now(timezone.utc).strftime("%Y-%m-%d")

    report = generate_report(
        results=results,
        config=config,
        date_str=date_str,
        previous_path=args.previous,
        inventory_path=args.inventory,
        kpi_targets=kpi_targets,
    )

    out_path = Path(args.output)
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(report, encoding="utf-8")
    print(f"Report written: {out_path}  ({len(report)} chars, {len(results)} results)")


if __name__ == "__main__":
    main()
