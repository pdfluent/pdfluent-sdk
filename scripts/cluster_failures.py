#!/usr/bin/env python3
"""
cluster_failures.py — Cluster and aggregate failure codes from benchmark results.

CLI:
    python3 scripts/cluster_failures.py \
        --results benchmarks/enterprise-baseline-2026-04-19-classified.json \
        --output benchmarks/failure_clusters.json \
        --report benchmarks/failure_clusters_report.md
"""

from __future__ import annotations

import argparse
import json
import sys
from collections import Counter, defaultdict
from datetime import date
from itertools import combinations
from pathlib import Path
from statistics import mean


def compute_clusters(data: dict) -> dict:
    """
    Aggregate failure code statistics from a classified benchmark JSON.

    Parameters
    ----------
    data : dict
        Classified benchmark JSON (output of classify_failure.py).

    Returns
    -------
    dict
        Cluster statistics JSON.
    """
    results = data.get("results", [])
    run_id = data.get("config", {}).get("run_id", "unknown")

    total = len(results)
    failed_results = [
        r for r in results
        if r.get("status", "") != "fully_correct"
        and r.get("failure_codes", ["UNKNOWN"]) != []
    ]
    total_failed = len(failed_results)
    pct_failed = round(total_failed / total * 100, 1) if total else 0.0

    # Per primary_code aggregation
    by_primary: dict[str, dict] = defaultdict(lambda: {
        "count": 0, "ssim_values": [], "sample_files": []
    })

    for r in failed_results:
        primary = r.get("primary_code", "UNKNOWN")
        by_primary[primary]["count"] += 1
        ssim = r.get("ssim")
        if ssim is not None:
            by_primary[primary]["ssim_values"].append(ssim)
        fname = r.get("file", "")
        if fname and len(by_primary[primary]["sample_files"]) < 3:
            by_primary[primary]["sample_files"].append(fname)

    by_primary_out: dict[str, dict] = {}
    for code, stats in sorted(by_primary.items(), key=lambda x: -x[1]["count"]):
        ssim_vals = stats["ssim_values"]
        by_primary_out[code] = {
            "count": stats["count"],
            "pct_of_failed": round(stats["count"] / total_failed * 100, 1) if total_failed else 0.0,
            "pct_of_total": round(stats["count"] / total * 100, 1) if total else 0.0,
            "avg_ssim": round(mean(ssim_vals), 4) if ssim_vals else None,
            "sample_files": stats["sample_files"],
        }

    # Multi-failure documents
    multi_failure = sum(
        1 for r in failed_results
        if len(r.get("failure_codes", [])) >= 2
    )

    # Unknown documents
    unknown_docs = sum(
        1 for r in failed_results
        if r.get("primary_code", "UNKNOWN") == "UNKNOWN"
    )

    # Top co-occurring pairs
    pair_counter: Counter = Counter()
    for r in failed_results:
        codes = r.get("failure_codes", [])
        if len(codes) >= 2:
            for pair in combinations(sorted(codes), 2):
                pair_counter[pair] += 1

    top_pairs = [list(pair) for pair, _ in pair_counter.most_common(5)]

    # Review needed
    review_needed = sum(
        1 for r in failed_results if r.get("needs_manual_review", False)
    )

    return {
        "run_id": run_id,
        "total_documents": total,
        "total_failed": total_failed,
        "pct_failed": pct_failed,
        "by_primary_code": by_primary_out,
        "multi_failure_documents": multi_failure,
        "unknown_documents": unknown_docs,
        "top_co_occurring_pairs": top_pairs,
        "review_needed": review_needed,
    }


def render_report(clusters: dict) -> str:
    """Render a Markdown cluster report from cluster statistics."""
    today = date.today().isoformat()
    lines: list[str] = []

    lines.append(f"# Failure Cluster Report — {today}")
    lines.append("")
    lines.append("## Summary")
    lines.append("")
    lines.append(f"- **Total documents:** {clusters['total_documents']}")
    lines.append(f"- **Total failed:** {clusters['total_failed']} ({clusters['pct_failed']}%)")
    lines.append(f"- **Multi-failure documents:** {clusters['multi_failure_documents']}")
    lines.append(f"- **Unknown failures:** {clusters['unknown_documents']}")
    lines.append(f"- **Needing manual review:** {clusters['review_needed']}")
    lines.append("")

    by_primary = clusters.get("by_primary_code", {})
    if by_primary:
        lines.append("| Category | Count | % of failures | Avg SSIM |")
        lines.append("|----------|-------|--------------|---------|")
        for code, stats in by_primary.items():
            avg_ssim = f"{stats['avg_ssim']:.4f}" if stats["avg_ssim"] is not None else "n/a"
            lines.append(
                f"| {code} | {stats['count']} | {stats['pct_of_failed']}% | {avg_ssim} |"
            )
        lines.append("")

    lines.append("## Top co-occurring failure combinations")
    lines.append("")
    pairs = clusters.get("top_co_occurring_pairs", [])
    if pairs:
        for pair in pairs:
            lines.append(f"- {' + '.join(pair)}")
    else:
        lines.append("_No co-occurring failure pairs found._")
    lines.append("")

    lines.append("## Sample documents per category")
    lines.append("")
    for code, stats in by_primary.items():
        samples = stats.get("sample_files", [])
        if samples:
            lines.append(f"### {code}")
            for f in samples:
                lines.append(f"- `{f}`")
            lines.append("")

    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Cluster and aggregate failure codes from classified benchmark results.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__,
    )
    parser.add_argument(
        "--results",
        required=True,
        help="Path to classified benchmark results JSON (output of classify_failure.py).",
    )
    parser.add_argument(
        "--output",
        required=True,
        help="Path for output cluster statistics JSON.",
    )
    parser.add_argument(
        "--report",
        required=True,
        help="Path for output Markdown cluster report.",
    )
    args = parser.parse_args()

    results_path = Path(args.results)
    output_path = Path(args.output)
    report_path = Path(args.report)

    if not results_path.exists():
        print(f"ERROR: results file not found: {results_path}", file=sys.stderr)
        return 1

    try:
        data = json.loads(results_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        print(f"ERROR: failed to parse JSON from {results_path}: {exc}", file=sys.stderr)
        return 1

    clusters = compute_clusters(data)

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(
        json.dumps(clusters, indent=2, ensure_ascii=False),
        encoding="utf-8",
    )

    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(render_report(clusters), encoding="utf-8")

    print(
        f"Cluster analysis complete: {clusters['total_failed']}/{clusters['total_documents']} "
        f"documents failed ({clusters['pct_failed']}%)."
    )
    print(f"JSON output: {output_path}")
    print(f"Markdown report: {report_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
