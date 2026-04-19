#!/usr/bin/env python3
"""
score_impact.py — Compute impact scores for failure clusters.

Impact Score = Frequency × Severity × Enterprise Impact

CLI:
    python3 scripts/score_impact.py \\
        --clusters benchmarks/failure_clusters.json \\
        --total-docs 155 \\
        --output benchmarks/impact_scores.json \\
        --report benchmarks/impact_scores_report.md

Importable as module:
    from score_impact import compute_scores, SEVERITY_MAP, ENTERPRISE_MAP
"""

from __future__ import annotations

import argparse
import json
import sys
from datetime import date
from pathlib import Path

# ---------------------------------------------------------------------------
# Lookup tables (from IMPACT_SCORING.md)
# ---------------------------------------------------------------------------

SEVERITY_MAP: dict[str, int] = {
    "CRASH-001": 5,
    "CRASH-002": 5,
    "CRASH-003": 5,
    "FLAT-001": 4,
    "BIND-001": 4,
    "LAYOUT-001": 4,
    "BIND-002": 3,
    "LAYOUT-002": 3,
    "RENDER-001": 5,
    "RENDER-002": 2,
    "RENDER-003": 2,
    "BIND-003": 3,
    "PERF-001": 2,
    "ORACLE-001": 1,
    "ORACLE-002": 1,
    "LAYOUT-003": 2,
    "UNKNOWN": 3,
}

ENTERPRISE_MAP: dict[str, int] = {
    "CRASH-001": 3,
    "CRASH-002": 3,
    "CRASH-003": 3,
    "FLAT-001": 3,
    "BIND-001": 3,
    "LAYOUT-001": 3,
    "BIND-002": 2,
    "LAYOUT-002": 2,
    "RENDER-001": 3,
    "RENDER-002": 1,
    "RENDER-003": 1,
    "BIND-003": 2,
    "PERF-001": 2,
    "ORACLE-001": 1,
    "ORACLE-002": 1,
    "LAYOUT-003": 2,
    "UNKNOWN": 2,
}

# Codes excluded from impact statistics
EXCLUDED_CODES: frozenset[str] = frozenset({"ORACLE-001", "ORACLE-002"})


def frequency_score(count: int, total: int) -> int:
    """Map a document count to a frequency score 1-5."""
    if total == 0:
        return 1
    pct = count / total * 100
    if pct > 30:
        return 5
    if pct > 15:
        return 4
    if pct > 5:
        return 3
    if pct >= 2:
        return 2
    return 1


def compute_scores(clusters: dict, total_docs: int | None = None) -> dict:
    """
    Compute impact scores from a failure_clusters.json dict.

    Parameters
    ----------
    clusters : dict
        Output of cluster_failures.py (failure_clusters.json).
    total_docs : int | None
        Override total document count; uses clusters['total_documents'] if None.

    Returns
    -------
    dict
        Impact scores JSON payload.
    """
    total = total_docs if total_docs is not None else clusters.get("total_documents", 0)
    total_failed = clusters.get("total_failed", 0)

    by_primary = clusters.get("by_primary_code", {})

    scores: list[dict] = []
    for code, stats in by_primary.items():
        if code in EXCLUDED_CODES:
            continue

        count = stats.get("count", 0)
        pct = round(count / total * 100, 1) if total else 0.0
        freq = frequency_score(count, total)
        sev = SEVERITY_MAP.get(code, SEVERITY_MAP["UNKNOWN"])
        ent = ENTERPRISE_MAP.get(code, ENTERPRISE_MAP["UNKNOWN"])
        total_score = freq * sev * ent

        avg_ssim = stats.get("avg_ssim")

        scores.append({
            "code": code,
            "count": count,
            "pct": pct,
            "frequency_score": freq,
            "severity_score": sev,
            "enterprise_score": ent,
            "total_score": total_score,
            "avg_ssim": avg_ssim,
        })

    # Sort by total_score descending, then count descending as tiebreaker
    scores.sort(key=lambda x: (-x["total_score"], -x["count"]))

    return {
        "total_documents": total,
        "total_failed": total_failed,
        "scores": scores,
    }


def render_report(impact: dict) -> str:
    """Render a Markdown impact score report."""
    today = date.today().isoformat()
    lines: list[str] = []

    total = impact["total_documents"]
    total_failed = impact["total_failed"]
    pct_failed = round(total_failed / total * 100, 1) if total else 0.0

    lines.append(f"# Impact Score Report — {today}")
    lines.append("")
    lines.append("## Summary")
    lines.append("")
    lines.append(f"- **Total documents:** {total}")
    lines.append(f"- **Total failed:** {total_failed} ({pct_failed}%)")
    lines.append("")
    lines.append("## Impact Scores by Failure Code")
    lines.append("")

    scores = impact.get("scores", [])
    if not scores:
        lines.append("_No failure data available._")
        lines.append("")
        return "\n".join(lines)

    lines.append("| Code | Count | % of Docs | Freq | Sev | Ent | **Total** | Avg SSIM |")
    lines.append("|------|-------|-----------|------|-----|-----|-----------|---------|")
    for s in scores:
        avg_ssim = f"{s['avg_ssim']:.4f}" if s["avg_ssim"] is not None else "n/a"
        lines.append(
            f"| {s['code']} | {s['count']} | {s['pct']}% "
            f"| {s['frequency_score']} | {s['severity_score']} | {s['enterprise_score']} "
            f"| **{s['total_score']}** | {avg_ssim} |"
        )
    lines.append("")
    lines.append("_Impact Score = Frequency × Severity × Enterprise Impact (max 75)_")
    lines.append("")

    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Compute impact scores for failure clusters."
    )
    parser.add_argument(
        "--clusters",
        required=True,
        type=Path,
        help="Path to failure_clusters.json",
    )
    parser.add_argument(
        "--total-docs",
        type=int,
        default=None,
        help="Override total document count (default: read from clusters JSON)",
    )
    parser.add_argument(
        "--output",
        required=True,
        type=Path,
        help="Output path for impact_scores.json",
    )
    parser.add_argument(
        "--report",
        type=Path,
        default=None,
        help="Output path for Markdown report (optional)",
    )
    args = parser.parse_args(argv)

    if not args.clusters.exists():
        print(f"ERROR: clusters file not found: {args.clusters}", file=sys.stderr)
        return 1

    clusters = json.loads(args.clusters.read_text())
    impact = compute_scores(clusters, total_docs=args.total_docs)

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(impact, indent=2))
    print(f"Impact scores written to {args.output}")

    if args.report:
        report_md = render_report(impact)
        args.report.parent.mkdir(parents=True, exist_ok=True)
        args.report.write_text(report_md)
        print(f"Report written to {args.report}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
