#!/usr/bin/env python3
"""
track_failure_trends.py — Track failure trends across multiple benchmark runs.

CLI:
    python3 scripts/track_failure_trends.py \
        --results benchmarks/run1.json benchmarks/run2.json \
        --output benchmarks/failure_trends.md \
        --json-output benchmarks/failure_trends.json

Exit code 1 if a regression alert is triggered (any code's count increases > 20%
vs the most recent previous run).
"""

from __future__ import annotations

import argparse
import json
import sys
from collections import Counter
from pathlib import Path


# All known taxonomy codes in priority order
ALL_CODES = [
    "CRASH-001", "CRASH-002", "CRASH-003",
    "ORACLE-001", "FLAT-001",
    "BIND-001", "BIND-002",
    "LAYOUT-001", "LAYOUT-002",
    "RENDER-001", "RENDER-002", "RENDER-003",
    "BIND-003", "PERF-001",
    "ORACLE-002", "LAYOUT-003",
    "UNKNOWN",
]


def _extract_run_info(data: dict) -> dict:
    """Extract run_id, timestamp, and per-code counts from a classified results file."""
    config = data.get("config", {})
    run_id = config.get("run_id", "unknown")
    timestamp = config.get("timestamp", "unknown")
    results = data.get("results", [])
    total = len(results)

    primary_counter: Counter = Counter()
    for r in results:
        primary = r.get("primary_code")
        if primary:
            primary_counter[primary] += 1

    return {
        "run_id": run_id,
        "timestamp": timestamp,
        "total": total,
        "counts": dict(primary_counter),
    }


def _trend_indicator(prev_count: int, cur_count: int, is_new: bool, is_fixed: bool) -> str:
    if is_fixed:
        return "FIXED"
    if is_new:
        return "NEW"
    if prev_count == 0:
        return "NEW"
    pct_change = (cur_count - prev_count) / prev_count * 100
    if pct_change > 10:
        return "UP"
    if pct_change < -10:
        return "DOWN"
    return "STABLE"


def analyze_trends(runs: list[dict]) -> dict:
    """
    Compute trend data across multiple runs.

    Parameters
    ----------
    runs : list[dict]
        List of dicts from _extract_run_info, in chronological order.

    Returns
    -------
    dict
        Trend statistics JSON structure.
    """
    all_codes_seen: set[str] = set()
    for run in runs:
        all_codes_seen.update(run["counts"].keys())

    # Codes that appeared in any run
    ordered_codes = [c for c in ALL_CODES if c in all_codes_seen]
    # Plus any codes not in the canonical list (safety net)
    for c in sorted(all_codes_seen):
        if c not in ordered_codes:
            ordered_codes.append(c)

    # Build per-code time series
    code_series: dict[str, list[dict]] = {code: [] for code in ordered_codes}
    for run in runs:
        for code in ordered_codes:
            code_series[code].append({
                "run_id": run["run_id"],
                "timestamp": run["timestamp"],
                "count": run["counts"].get(code, 0),
                "total": run["total"],
            })

    # Regression alerts: count increased > 20% vs most recent previous run
    regression_alerts: list[dict] = []
    if len(runs) >= 2:
        prev_run = runs[-2]
        cur_run = runs[-1]
        for code in ordered_codes:
            prev_count = prev_run["counts"].get(code, 0)
            cur_count = cur_run["counts"].get(code, 0)
            if prev_count > 0:
                pct_change = (cur_count - prev_count) / prev_count * 100
                if pct_change > 20:
                    regression_alerts.append({
                        "code": code,
                        "prev_count": prev_count,
                        "cur_count": cur_count,
                        "pct_change": round(pct_change, 1),
                    })

    return {
        "runs": [
            {"run_id": r["run_id"], "timestamp": r["timestamp"], "total": r["total"]}
            for r in runs
        ],
        "codes_tracked": ordered_codes,
        "code_series": code_series,
        "regression_alerts": regression_alerts,
    }


def render_trends_markdown(trend_data: dict) -> str:
    """Render Markdown trend report."""
    runs = trend_data["runs"]
    codes = trend_data["codes_tracked"]
    code_series = trend_data["code_series"]
    regression_alerts = trend_data["regression_alerts"]

    lines: list[str] = []
    lines.append("# Failure Trend Report")
    lines.append("")
    lines.append("## Runs analyzed")
    lines.append("")
    for r in runs:
        lines.append(f"- **{r['run_id']}** ({r['timestamp']}) — {r['total']} documents")
    lines.append("")

    if regression_alerts:
        lines.append("## Regression Alerts")
        lines.append("")
        lines.append(
            "> WARNING: The following failure codes increased by more than 20% "
            "vs the previous run."
        )
        lines.append("")
        lines.append("| Code | Previous | Current | Change |")
        lines.append("|------|----------|---------|--------|")
        for alert in regression_alerts:
            lines.append(
                f"| {alert['code']} | {alert['prev_count']} | {alert['cur_count']} "
                f"| +{alert['pct_change']}% |"
            )
        lines.append("")

    # Trend table header
    run_ids = [r["run_id"] for r in runs]
    header_cols = " | ".join(run_ids)
    sep_cols = " | ".join(["------"] * len(runs))
    lines.append("## Failure Code Trends")
    lines.append("")
    lines.append(f"| Code | {header_cols} | Trend |")
    lines.append(f"|------|{sep_cols}|-------|")

    for code in codes:
        series = code_series.get(code, [])
        counts = [s["count"] for s in series]
        count_cols = " | ".join(str(c) for c in counts)

        # Compute trend indicator for most recent transition
        if len(counts) >= 2:
            prev_c = counts[-2]
            cur_c = counts[-1]
            is_fixed = cur_c == 0 and prev_c > 0
            is_new = prev_c == 0 and cur_c > 0
            indicator = _trend_indicator(prev_c, cur_c, is_new, is_fixed)
        elif len(counts) == 1:
            indicator = "NEW" if counts[0] > 0 else "STABLE"
        else:
            indicator = "STABLE"

        SYMBOLS = {
            "DOWN": "↓ Improving",
            "UP": "↑ Regressing",
            "STABLE": "→ Stable",
            "NEW": "🆕 New",
            "FIXED": "✅ Fixed",
        }
        trend_label = SYMBOLS.get(indicator, indicator)
        lines.append(f"| {code} | {count_cols} | {trend_label} |")

    lines.append("")
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Track failure trends across multiple benchmark runs.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    parser.add_argument(
        "--results",
        nargs="+",
        required=True,
        metavar="FILE",
        help="Two or more classified benchmark result JSON files in chronological order.",
    )
    parser.add_argument(
        "--output",
        required=True,
        help="Path for output Markdown trend report.",
    )
    parser.add_argument(
        "--json-output",
        required=True,
        dest="json_output",
        help="Path for output trend statistics JSON.",
    )
    args = parser.parse_args()

    if len(args.results) < 2:
        print("ERROR: at least 2 result files required for trend analysis.", file=sys.stderr)
        return 1

    runs: list[dict] = []
    for fpath in args.results:
        p = Path(fpath)
        if not p.exists():
            print(f"ERROR: results file not found: {p}", file=sys.stderr)
            return 1
        try:
            data = json.loads(p.read_text(encoding="utf-8"))
        except json.JSONDecodeError as exc:
            print(f"ERROR: failed to parse JSON from {p}: {exc}", file=sys.stderr)
            return 1
        runs.append(_extract_run_info(data))

    trend_data = analyze_trends(runs)

    output_path = Path(args.output)
    json_output_path = Path(args.json_output)

    output_path.parent.mkdir(parents=True, exist_ok=True)
    json_output_path.parent.mkdir(parents=True, exist_ok=True)

    output_path.write_text(render_trends_markdown(trend_data), encoding="utf-8")
    json_output_path.write_text(
        json.dumps(trend_data, indent=2, ensure_ascii=False),
        encoding="utf-8",
    )

    print(f"Trend report written to: {output_path}")
    print(f"Trend JSON written to: {json_output_path}")

    alerts = trend_data.get("regression_alerts", [])
    if alerts:
        print(
            f"\nREGRESSION ALERT: {len(alerts)} failure code(s) increased >20% vs previous run:",
            file=sys.stderr,
        )
        for a in alerts:
            print(
                f"  {a['code']}: {a['prev_count']} -> {a['cur_count']} (+{a['pct_change']}%)",
                file=sys.stderr,
            )
        return 1

    return 0


if __name__ == "__main__":
    sys.exit(main())
