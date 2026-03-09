#!/usr/bin/env python3
"""Compare two Criterion benchmark JSON outputs and detect regressions.

Usage:
    bench-compare.py baseline.json current.json [--threshold 10]

Exit codes:
    0 — no regressions above threshold
    1 — at least one regression above threshold
    2 — error (missing files, parse error)

The JSON files are Criterion's message-format output, produced by:
    cargo bench --package pdf-bench -- --output-format=json 2>/dev/null > bench.json

Alternatively, this script can parse Criterion's target/criterion/ directory
structure (estimates.json files).
"""

import json
import os
import sys
from pathlib import Path


def load_criterion_dir(criterion_dir: Path) -> dict[str, float]:
    """Load benchmark results from Criterion's target/criterion/ directory."""
    results = {}
    for estimates_path in criterion_dir.rglob("new/estimates.json"):
        try:
            with open(estimates_path) as f:
                data = json.load(f)
            # Extract median time in nanoseconds.
            median_ns = data.get("median", {}).get("point_estimate", 0)
            # Build benchmark name from path: group/benchmark/new/estimates.json
            parts = estimates_path.relative_to(criterion_dir).parts
            if len(parts) >= 3:
                bench_name = "/".join(parts[:-2])
            else:
                bench_name = str(estimates_path)
            results[bench_name] = median_ns
        except (json.JSONDecodeError, KeyError):
            continue
    return results


def load_json_output(path: Path) -> dict[str, float]:
    """Load benchmark results from cargo bench JSON output.

    Each line is a JSON object. Benchmark results have:
    {"reason": "benchmark-complete", "id": "...", "median": {"estimate": ns, ...}}
    """
    results = {}
    with open(path) as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            try:
                obj = json.loads(line)
            except json.JSONDecodeError:
                continue
            if obj.get("reason") == "benchmark-complete":
                bench_id = obj.get("id", "unknown")
                median = obj.get("median", {}).get("estimate", 0)
                results[bench_id] = median
    return results


def load_results(path: str) -> dict[str, float]:
    """Load benchmark results from either a JSON file or a Criterion directory."""
    p = Path(path)
    if p.is_dir():
        return load_criterion_dir(p)
    elif p.is_file():
        return load_json_output(p)
    else:
        print(f"Error: {path} not found", file=sys.stderr)
        sys.exit(2)


def compare(
    baseline: dict[str, float],
    current: dict[str, float],
    threshold_pct: float,
) -> list[dict]:
    """Compare benchmark results and return regressions."""
    comparisons = []
    for name in sorted(set(baseline.keys()) | set(current.keys())):
        base_val = baseline.get(name)
        curr_val = current.get(name)

        if base_val is None or curr_val is None:
            comparisons.append({
                "name": name,
                "baseline_ns": base_val,
                "current_ns": curr_val,
                "delta_pct": None,
                "status": "new" if base_val is None else "removed",
            })
            continue

        if base_val == 0:
            delta_pct = 0.0
        else:
            delta_pct = ((curr_val - base_val) / base_val) * 100

        if delta_pct > threshold_pct:
            status = "REGRESSION"
        elif delta_pct < -threshold_pct:
            status = "improvement"
        else:
            status = "ok"

        comparisons.append({
            "name": name,
            "baseline_ns": base_val,
            "current_ns": curr_val,
            "delta_pct": delta_pct,
            "status": status,
        })

    return comparisons


def format_ns(ns: float | None) -> str:
    """Format nanoseconds in human-readable form."""
    if ns is None:
        return "—"
    if ns < 1_000:
        return f"{ns:.0f}ns"
    elif ns < 1_000_000:
        return f"{ns/1_000:.1f}µs"
    elif ns < 1_000_000_000:
        return f"{ns/1_000_000:.1f}ms"
    else:
        return f"{ns/1_000_000_000:.2f}s"


def main():
    import argparse

    parser = argparse.ArgumentParser(description="Compare Criterion benchmark results")
    parser.add_argument("baseline", help="Baseline JSON file or Criterion directory")
    parser.add_argument("current", help="Current JSON file or Criterion directory")
    parser.add_argument(
        "--threshold",
        type=float,
        default=10.0,
        help="Regression threshold in percent (default: 10)",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Output comparison as JSON",
    )
    args = parser.parse_args()

    baseline = load_results(args.baseline)
    current = load_results(args.current)

    if not baseline:
        print("Warning: no baseline benchmarks found", file=sys.stderr)
    if not current:
        print("Warning: no current benchmarks found", file=sys.stderr)

    comparisons = compare(baseline, current, args.threshold)

    if args.json:
        print(json.dumps(comparisons, indent=2))
        sys.exit(0)

    # Print table.
    regressions = []
    print(f"\n{'Benchmark':<60} {'Baseline':>10} {'Current':>10} {'Delta':>8} {'Status'}")
    print("─" * 100)

    for c in comparisons:
        delta_str = f"{c['delta_pct']:+.1f}%" if c["delta_pct"] is not None else "—"
        status_marker = ""
        if c["status"] == "REGRESSION":
            status_marker = " ❌"
            regressions.append(c)
        elif c["status"] == "improvement":
            status_marker = " ✅"

        print(
            f"{c['name']:<60} "
            f"{format_ns(c['baseline_ns']):>10} "
            f"{format_ns(c['current_ns']):>10} "
            f"{delta_str:>8}"
            f"{status_marker}"
        )

    print("─" * 100)
    print(
        f"\nTotal: {len(comparisons)} benchmarks, "
        f"{len(regressions)} regressions (>{args.threshold}% slower)"
    )

    if regressions:
        print(f"\n⚠ {len(regressions)} REGRESSION(S) DETECTED:")
        for r in regressions:
            print(f"  - {r['name']}: {format_ns(r['baseline_ns'])} → {format_ns(r['current_ns'])} ({r['delta_pct']:+.1f}%)")
        sys.exit(1)
    else:
        print("\n✓ No regressions detected.")
        sys.exit(0)


if __name__ == "__main__":
    main()
