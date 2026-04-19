#!/usr/bin/env python3
"""
generate_gap_backlog.py — Generate a structured gap backlog from impact scores.

CLI:
    python3 scripts/generate_gap_backlog.py \\
        --impact-scores benchmarks/impact_scores.json \\
        --clusters benchmarks/failure_clusters.json \\
        --output benchmarks/GAP_BACKLOG.md \\
        --top-n 20

Importable as module:
    from generate_gap_backlog import generate_backlog
"""

from __future__ import annotations

import argparse
import json
import sys
from datetime import date
from pathlib import Path

# ---------------------------------------------------------------------------
# Heuristic label tables
# ---------------------------------------------------------------------------

PIPELINE_STAGE_HINT: dict[str, str] = {
    "CRASH-001": "general",
    "CRASH-002": "general",
    "CRASH-003": "general",
    "FLAT-001": "flatten",
    "BIND-001": "binding",
    "BIND-002": "binding",
    "BIND-003": "binding",
    "LAYOUT-001": "layout",
    "LAYOUT-002": "layout",
    "LAYOUT-003": "layout",
    "RENDER-001": "rendering",
    "RENDER-002": "rendering",
    "RENDER-003": "rendering",
    "PERF-001": "general",
    "ORACLE-001": "oracle",
    "ORACLE-002": "oracle",
    "UNKNOWN": "general",
}

# Codes treated as architectural or cross-cutting
ARCHITECTURAL_CODES: frozenset[str] = frozenset({
    "CRASH-001", "CRASH-002", "CRASH-003",
    "RENDER-001",
})
CROSS_CUTTING_CODES: frozenset[str] = frozenset({
    "BIND-003",
    "LAYOUT-001",
    "PERF-001",
    "UNKNOWN",
})


def fix_complexity(code: str) -> str:
    """Heuristic fix complexity label."""
    if code in ARCHITECTURAL_CODES:
        return "High"
    if code in CROSS_CUTTING_CODES:
        return "Medium"
    return "Low"


def generate_backlog(impact: dict, clusters: dict, top_n: int = 20) -> str:
    """
    Generate a Markdown gap backlog document.

    Parameters
    ----------
    impact : dict
        Output of score_impact.py (impact_scores.json).
    clusters : dict
        Output of cluster_failures.py (failure_clusters.json).
    top_n : int
        Maximum number of gap entries to include.

    Returns
    -------
    str
        Markdown content for GAP_BACKLOG.md.
    """
    today = date.today().isoformat()
    total = impact.get("total_documents", 0)
    total_failed = impact.get("total_failed", 0)
    scores = impact.get("scores", [])
    by_primary = clusters.get("by_primary_code", {})

    lines: list[str] = []
    lines.append("# Gap Backlog")
    lines.append("")
    lines.append(f"> Generated: {today}  ")
    lines.append(
        f"> Total documents: {total} | Failed: {total_failed} "
        f"({round(total_failed / total * 100, 1) if total else 0}%)"
    )
    lines.append("")
    lines.append(
        "Gaps are ranked by impact score (Frequency × Severity × Enterprise Impact). "
        "Pipeline stage hints and fix complexity are heuristic labels — actual root cause "
        "and fix approach must be determined from baseline data analysis."
    )
    lines.append("")
    lines.append("---")
    lines.append("")

    top_scores = scores[:top_n]
    if not top_scores:
        lines.append("_No failure data available. Run the benchmark pipeline first._")
        lines.append("")
        return "\n".join(lines)

    for gap_num, s in enumerate(top_scores, start=1):
        code = s["code"]
        gap_id = f"GAP-{gap_num:03d}"
        # Friendly code name from prefix
        prefix = code.split("-")[0]
        name_map = {
            "CRASH": "Crash / unprocessable document",
            "FLAT": "Flatten artifact",
            "BIND": "Data binding failure",
            "LAYOUT": "Layout / pagination failure",
            "RENDER": "Visual rendering failure",
            "PERF": "Performance issue",
            "ORACLE": "Oracle reliability issue",
        }
        friendly = name_map.get(prefix, "Unknown failure")

        count = s["count"]
        pct = s["pct"]
        avg_ssim = s["avg_ssim"]
        total_score = s["total_score"]
        freq = s["frequency_score"]
        sev = s["severity_score"]
        ent = s["enterprise_score"]

        stage = PIPELINE_STAGE_HINT.get(code, "general")
        complexity = fix_complexity(code)

        # Sample files from cluster data
        cluster_entry = by_primary.get(code, {})
        sample_files = cluster_entry.get("sample_files", [])
        sample_str = ", ".join(sample_files[:3]) if sample_files else "n/a"

        # Expected improvement: documents that currently fail with this as primary code
        expected_improvement = count

        ssim_str = f"{avg_ssim:.4f}" if avg_ssim is not None else "n/a"

        lines.append(f"### {gap_id}: {friendly} ({code})")
        lines.append(
            f"**Impact score**: {total_score} "
            f"(Frequency: {freq}, Severity: {sev}, Enterprise: {ent})"
        )
        lines.append(f"**Documents affected**: {count}/{total} ({pct}%)")
        lines.append(f"**Average SSIM when failing**: {ssim_str}")
        lines.append(f"**Pipeline stage hint**: {stage}")
        lines.append(f"**Estimated fix complexity**: {complexity}")
        lines.append(
            f"**Expected improvement**: ~{expected_improvement} documents would improve "
            f"to correct/minor_deviation"
        )
        lines.append(f"**Sample files**: {sample_str}")
        lines.append("")

    return "\n".join(lines)


PLACEHOLDER = """\
# Gap Backlog

> **Note**: This file is a placeholder. Run `scripts/generate_gap_backlog.py` after
> the first baseline run (EVH-BASELINE-02) to populate this backlog.

Run command:
```bash
python3 scripts/generate_gap_backlog.py \\
  --impact-scores benchmarks/impact_scores.json \\
  --clusters benchmarks/failure_clusters.json \\
  --output benchmarks/GAP_BACKLOG.md
```
"""


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Generate a structured gap backlog from impact scores."
    )
    parser.add_argument(
        "--impact-scores",
        type=Path,
        default=None,
        help="Path to impact_scores.json",
    )
    parser.add_argument(
        "--clusters",
        type=Path,
        default=None,
        help="Path to failure_clusters.json",
    )
    parser.add_argument(
        "--output",
        required=True,
        type=Path,
        help="Output path for GAP_BACKLOG.md",
    )
    parser.add_argument(
        "--top-n",
        type=int,
        default=20,
        help="Maximum number of gap entries to include (default: 20)",
    )
    args = parser.parse_args(argv)

    # If input files are missing, write the placeholder
    if args.impact_scores is None or not args.impact_scores.exists():
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(PLACEHOLDER)
        print(f"Placeholder written to {args.output}")
        return 0

    if args.clusters is None or not args.clusters.exists():
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(PLACEHOLDER)
        print(f"Placeholder written to {args.output} (clusters file not found)")
        return 0

    impact = json.loads(args.impact_scores.read_text())
    clusters = json.loads(args.clusters.read_text())

    content = generate_backlog(impact, clusters, top_n=args.top_n)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(content)
    print(f"Gap backlog written to {args.output}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
